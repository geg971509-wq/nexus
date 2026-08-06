//! libcore framed IPC (LE), matches NexusCore dispatch.go / Qt RPC.cpp
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

pub struct LibcoreClient {
    stream: UnixStream,
}

#[derive(Debug)]
pub enum IpcError {
    Io(std::io::Error),
    Status(String),
    Protocol(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::Io(e) => write!(f, "io: {e}"),
            IpcError::Status(s) => write!(f, "core status: {s}"),
            IpcError::Protocol(s) => write!(f, "protocol: {s}"),
        }
    }
}

impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        IpcError::Io(e)
    }
}

impl LibcoreClient {
    pub fn from_stream(stream: UnixStream) -> std::io::Result<Self> {
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        Ok(Self { stream })
    }

    pub fn call(&mut self, method: &str, payload: &[u8]) -> Result<Vec<u8>, IpcError> {
        self.call_timeout(method, payload, Duration::from_secs(15))
    }

    /// Same framed IPC as upstream Qt Client::Call; optional per-call read timeout.
    pub fn call_timeout(
        &mut self,
        method: &str,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, IpcError> {
        let prev_r = self.stream.read_timeout().ok().flatten();
        let prev_w = self.stream.write_timeout().ok().flatten();
        let _ = self.stream.set_read_timeout(Some(timeout));
        let _ = self.stream.set_write_timeout(Some(timeout));

        let result = (|| {
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
            self.stream.write_all(&frame)?;

            // Response: [u32 id][u8 status][u32 len][data]
            let mut hdr = [0u8; 9];
            self.stream.read_exact(&mut hdr)?;
            let rid = u32::from_le_bytes(hdr[0..4].try_into().unwrap());
            let status = hdr[4];
            let dlen = u32::from_le_bytes(hdr[5..9].try_into().unwrap()) as usize;
            // ponytail: 16 MiB ceiling; raise if core legitimately returns larger blobs
            const MAX_IPC_PAYLOAD: usize = 16 * 1024 * 1024;
            if dlen > MAX_IPC_PAYLOAD {
                return Err(IpcError::Protocol(format!(
                    "payload too large: {dlen} > {MAX_IPC_PAYLOAD}"
                )));
            }
            if rid != req_id {
                return Err(IpcError::Protocol(format!(
                    "req id mismatch: sent {req_id} got {rid}"
                )));
            }
            let mut data = vec![0u8; dlen];
            if dlen > 0 {
                self.stream.read_exact(&mut data)?;
            }
            if status != 0 {
                return Err(IpcError::Status(String::from_utf8_lossy(&data).into_owned()));
            }
            Ok(data)
        })();

        let _ = self.stream.set_read_timeout(prev_r);
        let _ = self.stream.set_write_timeout(prev_w);
        result
    }

    pub fn shutdown(&self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}
