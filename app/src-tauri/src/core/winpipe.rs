//! Windows named-pipe server (GUI side) for Core go-winio DialPipe clients.
//! Overlapped Read/Write so call_timeout can actually expire (PIPE_WAIT alone blocks forever).
#![cfg(windows)]

use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING,
    ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile, FILE_FLAG_OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
    PIPE_WAIT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

// PIPE_ACCESS_DUPLEX not exported on all windows-sys versions; duplex = 3.
const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;

/// Owner + SYSTEM get full access; nobody else is on the DACL. A NULL security
/// descriptor would inherit the default pipe DACL, which grants Everyone
/// generic read/write — any local user could then drive the privileged core.
const PIPE_SDDL: &str = "D:P(A;;GA;;;OW)(A;;GA;;;SY)";

/// Owns the LocalAlloc'd descriptor behind SECURITY_ATTRIBUTES.
struct PipeSecurity {
    sd: PSECURITY_DESCRIPTOR,
    attrs: SECURITY_ATTRIBUTES,
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        if !self.sd.is_null() {
            unsafe { LocalFree(self.sd as _) };
        }
    }
}

fn pipe_security() -> io::Result<PipeSecurity> {
    let sddl = to_wide(PIPE_SDDL);
    let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let attrs = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    Ok(PipeSecurity { sd, attrs })
}

/// Connected duplex byte pipe with optional R/W deadlines (mirrors UnixStream timeouts).
pub struct PipeStream {
    handle: OwnedHandle,
    /// Interior-mutable timeouts so API matches UnixStream (&self setters).
    timeouts: Mutex<(Option<Duration>, Option<Duration>)>,
}

impl PipeStream {
    unsafe fn from_raw(h: HANDLE) -> Self {
        Self {
            handle: OwnedHandle::from_raw_handle(h as RawHandle),
            timeouts: Mutex::new((
                Some(Duration::from_secs(15)),
                Some(Duration::from_secs(15)),
            )),
        }
    }

    fn as_raw(&self) -> HANDLE {
        self.handle.as_raw_handle() as HANDLE
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        if let Ok(mut g) = self.timeouts.lock() {
            g.0 = timeout;
        }
        Ok(())
    }

    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        if let Ok(mut g) = self.timeouts.lock() {
            g.1 = timeout;
        }
        Ok(())
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        Ok(self.timeouts.lock().map(|g| g.0).unwrap_or(None))
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        Ok(self.timeouts.lock().map(|g| g.1).unwrap_or(None))
    }

    fn io_timeout_ms(&self, write: bool) -> u32 {
        let d = self
            .timeouts
            .lock()
            .ok()
            .and_then(|g| if write { g.1 } else { g.0 });
        match d {
            None => INFINITE,
            Some(t) if t.is_zero() => 0,
            Some(t) => t.as_millis().min(u32::MAX as u128) as u32,
        }
    }

    fn read_xfer(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.xfer(buf.as_mut_ptr(), buf.len(), false)
    }

    fn write_xfer(&self, buf: &[u8]) -> io::Result<usize> {
        self.xfer(buf.as_ptr() as *mut u8, buf.len(), true)
    }

    /// Overlapped ReadFile/WriteFile + WaitForSingleObject(deadline).
    fn xfer(&self, ptr: *mut u8, len: usize, write: bool) -> io::Result<usize> {
        if len == 0 {
            return Ok(0);
        }
        unsafe {
            let event = CreateEventW(ptr::null_mut(), 1, 0, ptr::null());
            if event.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut ov: OVERLAPPED = std::mem::zeroed();
            ov.hEvent = event;

            let mut done: u32 = 0;
            let ok = if write {
                WriteFile(
                    self.as_raw(),
                    ptr as *const _,
                    len as u32,
                    &mut done,
                    &mut ov,
                )
            } else {
                ReadFile(
                    self.as_raw(),
                    ptr as *mut _,
                    len as u32,
                    &mut done,
                    &mut ov,
                )
            };

            if ok != 0 {
                let _ = CloseHandle(event);
                return Ok(done as usize);
            }
            let err = GetLastError();
            if err != ERROR_IO_PENDING {
                let _ = CloseHandle(event);
                return Err(io::Error::from_raw_os_error(err as i32));
            }

            let wait = WaitForSingleObject(event, self.io_timeout_ms(write));
            if wait == WAIT_TIMEOUT {
                // CancelIoEx races the operation it cancels, so bytes may already
                // have moved. Reporting only TimedOut threw them away: on a read
                // they were gone from the pipe, on a write they were already on
                // the wire — either way the stream lost its position. A short
                // count is a legal Read/Write result, and read_exact/write_all
                // loop on it, so hand it back instead.
                let _ = CancelIoEx(self.as_raw(), &mut ov);
                let mut partial: u32 = 0;
                let _ = GetOverlappedResult(self.as_raw(), &mut ov, &mut partial, 1);
                let _ = CloseHandle(event);
                if partial > 0 {
                    return Ok(partial as usize);
                }
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    if write {
                        "named pipe write timed out"
                    } else {
                        "named pipe read timed out"
                    },
                ));
            }
            if wait != WAIT_OBJECT_0 {
                let _ = CloseHandle(event);
                return Err(io::Error::last_os_error());
            }

            let mut transferred: u32 = 0;
            let got = GetOverlappedResult(self.as_raw(), &mut ov, &mut transferred, 0);
            let _ = CloseHandle(event);
            if got == 0 {
                let e = GetLastError();
                if e == ERROR_IO_INCOMPLETE {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "pipe io incomplete"));
                }
                return Err(io::Error::from_raw_os_error(e as i32));
            }
            Ok(transferred as usize)
        }
    }
}

impl Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read_xfer(buf)
    }
}

impl Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_xfer(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Create `\\.\pipe\<name>` and accept one client connection (with timeout).
pub fn accept_one(pipe_name: &str, timeout: Duration) -> io::Result<PipeStream> {
    let full = if pipe_name.starts_with(r"\\.\pipe\") {
        pipe_name.to_string()
    } else {
        format!(r"\\.\pipe\{pipe_name}")
    };
    let wide = to_wide(&full);
    let deadline = Instant::now() + timeout;

    let mut sec = pipe_security()?;
    let h: HANDLE = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            &mut sec.attrs,
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    // Overlapped ConnectNamedPipe with deadline (no worker thread).
    unsafe {
        let event = CreateEventW(ptr::null_mut(), 1, 0, ptr::null());
        if event.is_null() {
            let _ = CloseHandle(h);
            return Err(io::Error::last_os_error());
        }
        let mut ov: OVERLAPPED = std::mem::zeroed();
        ov.hEvent = event;

        let connected = ConnectNamedPipe(h, &mut ov);
        if connected != 0 {
            let _ = CloseHandle(event);
            return Ok(PipeStream::from_raw(h));
        }
        let err = GetLastError();
        if err == ERROR_PIPE_CONNECTED {
            let _ = CloseHandle(event);
            return Ok(PipeStream::from_raw(h));
        }
        if err != ERROR_IO_PENDING {
            let _ = CloseHandle(event);
            let _ = CloseHandle(h);
            return Err(io::Error::from_raw_os_error(err as i32));
        }

        loop {
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                let _ = CancelIoEx(h, &mut ov);
                let mut ignored: u32 = 0;
                let _ = GetOverlappedResult(h, &mut ov, &mut ignored, 1);
                let _ = CloseHandle(event);
                let _ = CloseHandle(h);
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "named pipe accept timed out",
                ));
            }
            let ms = remain.as_millis().min(u32::MAX as u128) as u32;
            let wait = WaitForSingleObject(event, ms.min(50).max(1));
            if wait == WAIT_OBJECT_0 {
                let mut transferred: u32 = 0;
                let got = GetOverlappedResult(h, &mut ov, &mut transferred, 0);
                let _ = CloseHandle(event);
                if got == 0 {
                    let e = GetLastError();
                    let _ = CloseHandle(h);
                    return Err(io::Error::from_raw_os_error(e as i32));
                }
                return Ok(PipeStream::from_raw(h));
            }
            if wait == WAIT_TIMEOUT {
                continue;
            }
            let _ = CloseHandle(event);
            let _ = CloseHandle(h);
            return Err(io::Error::last_os_error());
        }
    }
}
