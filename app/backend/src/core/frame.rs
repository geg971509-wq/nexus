//! libcore framed IPC (LE), matches NexusCore dispatch.go / Qt RPC.cpp
use std::io::{Read, Write};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

type IpcStream = std::os::unix::net::UnixStream;

/// Stale replies to skip before declaring the stream unreadable. A call that
/// timed out leaves exactly one reply behind, so a small number covers the real
/// case; more than that means we are no longer on a frame boundary.
const MAX_STALE_FRAMES: u32 = 8;

pub struct LibcoreClient {
    stream: IpcStream,
    write_lock: Arc<Mutex<()>>,
    /// Latched on [`IpcError::Desync`]. Callers must drop the session rather than
    /// keep issuing calls that can only misparse from here on.
    broken: bool,
}

pub struct LibcoreControl {
    stream: IpcStream,
    write_lock: Arc<Mutex<()>>,
}

#[derive(Debug)]
pub enum IpcError {
    Io(std::io::Error),
    Status(String),
    Protocol(String),
    /// The stream no longer carries frames we can locate. Unlike the others this
    /// is not recoverable on this connection — the session must be rebuilt.
    Desync(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::Io(e) => write!(f, "io: {e}"),
            IpcError::Status(s) => write!(f, "core status: {s}"),
            IpcError::Protocol(s) => write!(f, "protocol: {s}"),
            IpcError::Desync(s) => write!(f, "ipc desync: {s}"),
        }
    }
}

impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        IpcError::Io(e)
    }
}

fn write_request(
    stream: &mut IpcStream,
    write_lock: &Mutex<()>,
    method: &str,
    payload: &[u8],
) -> Result<u32, IpcError> {
    let req_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let method_bytes = method.as_bytes();
    if method_bytes.len() > u16::MAX as usize {
        return Err(IpcError::Protocol("method name too long".into()));
    }
    let mut frame = Vec::with_capacity(4 + 2 + method_bytes.len() + 4 + payload.len());
    frame.extend_from_slice(&req_id.to_le_bytes());
    frame.extend_from_slice(&(method_bytes.len() as u16).to_le_bytes());
    frame.extend_from_slice(method_bytes);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    let _guard = write_lock.lock().unwrap_or_else(|e| e.into_inner());
    stream.write_all(&frame)?;
    Ok(req_id)
}

impl LibcoreControl {
    pub fn send(&mut self, method: &str, payload: &[u8]) -> Result<(), IpcError> {
        let _ = write_request(&mut self.stream, &self.write_lock, method, payload)?;
        Ok(())
    }
}

impl LibcoreClient {
    pub fn from_stream(stream: IpcStream) -> std::io::Result<Self> {
        let default = Duration::from_secs(15);
        stream.set_read_timeout(Some(default))?;
        stream.set_write_timeout(Some(default))?;
        Ok(Self {
            stream,
            write_lock: Arc::new(Mutex::new(())),
            broken: false,
        })
    }

    pub fn control_handle(&self) -> std::io::Result<LibcoreControl> {
        Ok(LibcoreControl {
            stream: self.stream.try_clone()?,
            write_lock: Arc::clone(&self.write_lock),
        })
    }

    /// True once a desync was seen. The connection cannot be resynchronised.
    pub fn is_broken(&self) -> bool {
        self.broken
    }

    pub fn call(&mut self, method: &str, payload: &[u8]) -> Result<Vec<u8>, IpcError> {
        self.call_timeout(method, payload, Duration::from_secs(15))
    }

    pub fn call_timeout(
        &mut self,
        method: &str,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, IpcError> {
        self.call_timeout_with_sent(method, payload, timeout, || {})
    }

    pub fn call_timeout_with_sent(
        &mut self,
        method: &str,
        payload: &[u8],
        timeout: Duration,
        sent: impl FnOnce(),
    ) -> Result<Vec<u8>, IpcError> {
        // Apply a deadline for this call, then restore the previous values.
        let prev_r = self.stream.read_timeout().ok().flatten();
        let prev_w = self.stream.write_timeout().ok().flatten();
        let _ = self.stream.set_read_timeout(Some(timeout));
        let _ = self.stream.set_write_timeout(Some(timeout));

        let result = (|| {
            let req_id = write_request(&mut self.stream, &self.write_lock, method, payload)?;
            sent();

            const MAX_IPC_PAYLOAD: usize = 16 * 1024 * 1024;

            // A frame is always consumed whole before its id is judged. Returning
            // between the header and the body used to leave the stream mid-frame,
            // so one timed-out call desynced every later call on the connection.
            // dispatch.go answers each request from its own goroutine, so a reply
            // that is not ours is legal protocol, not corruption — skip it.
            let mut skipped = 0u32;
            let (status, data) = loop {
                let mut hdr = [0u8; 9];
                self.stream.read_exact(&mut hdr)?;
                let rid = u32::from_le_bytes(hdr[0..4].try_into().unwrap());
                let status = hdr[4];
                let dlen = u32::from_le_bytes(hdr[5..9].try_into().unwrap()) as usize;
                if dlen > MAX_IPC_PAYLOAD {
                    // Not drainable: we cannot tell where this frame ends, so the
                    // boundary is lost for good.
                    self.broken = true;
                    return Err(IpcError::Desync(format!(
                        "payload too large: {dlen} > {MAX_IPC_PAYLOAD}"
                    )));
                }
                let mut data = vec![0u8; dlen];
                if dlen > 0 {
                    self.stream.read_exact(&mut data)?;
                }
                if rid == req_id {
                    break (status, data);
                }
                skipped += 1;
                if skipped > MAX_STALE_FRAMES {
                    self.broken = true;
                    return Err(IpcError::Desync(format!(
                        "no reply for {req_id} after {skipped} stale frames"
                    )));
                }
            };
            if status != 0 {
                return Err(IpcError::Status(
                    String::from_utf8_lossy(&data).into_owned(),
                ));
            }
            Ok(data)
        })();

        let _ = self.stream.set_read_timeout(prev_r);
        let _ = self.stream.set_write_timeout(prev_w);
        result
    }

    pub fn shutdown(&self) {
        #[cfg(unix)]
        {
            use std::net::Shutdown;
            let _ = self.stream.shutdown(Shutdown::Both);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    /// Read one request frame and return its id, so the fake Core answers the id
    /// the client actually sent rather than a guess.
    fn read_req_id(s: &mut UnixStream) -> u32 {
        let mut hdr = [0u8; 6];
        s.read_exact(&mut hdr).unwrap();
        let id = u32::from_le_bytes(hdr[0..4].try_into().unwrap());
        let mlen = u16::from_le_bytes(hdr[4..6].try_into().unwrap()) as usize;
        let mut method = vec![0u8; mlen];
        s.read_exact(&mut method).unwrap();
        let mut plen = [0u8; 4];
        s.read_exact(&mut plen).unwrap();
        let mut payload = vec![0u8; u32::from_le_bytes(plen) as usize];
        s.read_exact(&mut payload).unwrap();
        id
    }

    fn reply(s: &mut UnixStream, rid: u32, status: u8, body: &[u8]) {
        let mut f = Vec::new();
        f.extend_from_slice(&rid.to_le_bytes());
        f.push(status);
        f.extend_from_slice(&(body.len() as u32).to_le_bytes());
        f.extend_from_slice(body);
        s.write_all(&f).unwrap();
    }

    /// The reachable case: a call timed out earlier, so its reply is still in the
    /// pipe. It must be drained and the real reply still found.
    #[test]
    fn stale_reply_is_skipped_not_desynced() {
        let (client_s, mut server_s) = UnixStream::pair().unwrap();
        let mut c = LibcoreClient::from_stream(client_s).unwrap();
        let t = std::thread::spawn(move || {
            let id = read_req_id(&mut server_s);
            // Leftover from a previous call, then ours.
            reply(&mut server_s, id.wrapping_sub(1), 0, b"stale-body");
            reply(&mut server_s, id, 0, b"mine");
        });
        let got = c.call("QueryState", &[]).unwrap();
        t.join().unwrap();
        assert_eq!(got, b"mine");
        assert!(!c.is_broken(), "one stale frame must not brick the channel");
    }

    /// Past the skip budget the stream is declared unusable, and the flag latches
    /// so the session gets dropped instead of reused.
    #[test]
    fn too_many_stale_frames_latch_broken() {
        let (client_s, mut server_s) = UnixStream::pair().unwrap();
        let mut c = LibcoreClient::from_stream(client_s).unwrap();
        let t = std::thread::spawn(move || {
            let id = read_req_id(&mut server_s);
            for i in 0..(MAX_STALE_FRAMES + 2) {
                reply(&mut server_s, id.wrapping_add(1000 + i), 0, b"x");
            }
        });
        let err = c.call("QueryState", &[]).unwrap_err();
        t.join().unwrap();
        assert!(matches!(err, IpcError::Desync(_)), "{err}");
        assert!(c.is_broken());
    }

    /// A status frame is an application error, not a desync — the connection
    /// stays usable and the body comes back as the message.
    #[test]
    fn status_error_keeps_channel_usable() {
        let (client_s, mut server_s) = UnixStream::pair().unwrap();
        let mut c = LibcoreClient::from_stream(client_s).unwrap();
        let t = std::thread::spawn(move || {
            let id = read_req_id(&mut server_s);
            reply(&mut server_s, id, 1, b"boom");
        });
        let err = c.call("Start", &[]).unwrap_err();
        t.join().unwrap();
        assert!(matches!(&err, IpcError::Status(s) if s == "boom"), "{err}");
        assert!(!c.is_broken());
    }

    #[test]
    fn control_frame_can_cancel_while_primary_call_waits() {
        let (client_s, mut server_s) = UnixStream::pair().unwrap();
        let mut c = LibcoreClient::from_stream(client_s).unwrap();
        let mut control = c.control_handle().unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let test_id = read_req_id(&mut server_s);
            ready_tx.send(()).unwrap();
            let stop_id = read_req_id(&mut server_s);
            reply(&mut server_s, stop_id, 0, b"");
            reply(&mut server_s, test_id, 0, b"test-result");
        });
        let primary = std::thread::spawn(move || c.call("Test", &[]));

        ready_rx.recv().unwrap();
        control.send("StopTest", &[]).unwrap();

        let got = primary.join().unwrap().unwrap();
        server.join().unwrap();
        assert_eq!(got, b"test-result");
    }
}
