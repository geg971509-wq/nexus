//! Windows named-pipe server (GUI side) for Core go-winio DialPipe clients.
#![cfg(windows)]

use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
    PIPE_WAIT,
};

// PIPE_ACCESS_DUPLEX not exported on all windows-sys versions; duplex = 3.
const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;

/// Connected duplex byte pipe, Read/Write via ReadFile/WriteFile.
pub struct PipeStream {
    handle: OwnedHandle,
}

impl PipeStream {
    unsafe fn from_raw(h: HANDLE) -> Self {
        Self {
            handle: OwnedHandle::from_raw_handle(h as RawHandle),
        }
    }

    fn as_raw(&self) -> HANDLE {
        self.handle.as_raw_handle() as HANDLE
    }
}

impl Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut done: u32 = 0;
        let ok = unsafe {
            ReadFile(
                self.as_raw(),
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut done,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(done as usize)
    }
}

impl Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut done: u32 = 0;
        let ok = unsafe {
            WriteFile(
                self.as_raw(),
                buf.as_ptr() as *const _,
                buf.len() as u32,
                &mut done,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(done as usize)
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

    // One CreateNamedPipe + ConnectNamedPipe (blocks until client or we abort via timeout thread).
    // For timeout: create pipe, ConnectNamedPipe on worker, wait with deadline.
    let h: HANDLE = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    // ConnectNamedPipe blocks; run with deadline via channel.
    // HANDLE is a pointer — move as isize so the worker is Send.
    let h_bits = h as isize;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let handle = h_bits as HANDLE;
        let connected = unsafe { ConnectNamedPipe(handle, ptr::null_mut()) };
        let err = unsafe { GetLastError() };
        let ok = connected != 0 || err == ERROR_PIPE_CONNECTED;
        let _ = tx.send((ok, err, h_bits));
    });

    loop {
        match rx.try_recv() {
            Ok((true, _, handle_bits)) => {
                return Ok(unsafe { PipeStream::from_raw(handle_bits as HANDLE) });
            }
            Ok((false, err, handle_bits)) => {
                unsafe {
                    let _ = CloseHandle(handle_bits as HANDLE);
                }
                return Err(io::Error::from_raw_os_error(err as i32));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if Instant::now() > deadline {
                    // Closing handle unblocks ConnectNamedPipe
                    unsafe {
                        let _ = CloseHandle(h);
                    }
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "named pipe accept timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "pipe accept thread died",
                ));
            }
        }
    }
}
