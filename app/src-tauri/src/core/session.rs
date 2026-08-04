//! GUI-side session: listen unix socket, spawn NexusCore, accept, libcore calls.
use super::frame::LibcoreClient;
use super::proto_min::{decode_core_state, decode_error_resp, encode_load_config_core_json};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::os::unix::net::UnixListener;

pub struct CoreSession {
    listener_path: PathBuf,
    child: Option<Child>,
    client: Option<LibcoreClient>,
}

impl Drop for CoreSession {
    fn drop(&mut self) {
        let _ = self.stop_core_process();
        let _ = std::fs::remove_file(&self.listener_path);
    }
}

impl CoreSession {
    pub fn socket_path() -> PathBuf {
        let dir = std::env::temp_dir().join("nexus");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("nexus-core-{}.sock", std::process::id()))
    }

    pub fn resolve_core_binary() -> PathBuf {
        if let Ok(p) = std::env::var("NEXUS_CORE_BIN") {
            let pb = PathBuf::from(&p);
            // only absolute existing file — refuse PATH hijack via bare name
            if pb.is_absolute() && pb.is_file() {
                return pb;
            }
        }
        // 1) same directory as GUI binary (Nexus.app/Contents/MacOS/NexusCore)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let beside = dir.join("NexusCore");
                if beside.is_file() {
                    return beside;
                }
                // tauri externalBin may suffix target triple
                if let Ok(rd) = std::fs::read_dir(dir) {
                    for e in rd.flatten() {
                        let n = e.file_name();
                        let s = n.to_string_lossy();
                        if s == "NexusCore" || s.starts_with("NexusCore-") {
                            let p = e.path();
                            if p.is_file() {
                                return p;
                            }
                        }
                    }
                }
            }
        }
        // 2) repo / dev paths
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bin/NexusCore"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../bin/NexusCore"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries/NexusCore"),
        ];
        for c in candidates {
            if c.is_file() {
                return c;
            }
        }
        // missing — caller must error; do not spawn from PATH
        PathBuf::from("")
    }

    pub fn start(core_bin: &Path) -> io::Result<Self> {
        let path = Self::socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        listener.set_nonblocking(true)?;

        let mut child = Command::new(core_bin)
            .env("THRONE_CORE_SOCKET", &path)
            .env("THRONE_CORE_DEBUG", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let deadline = Instant::now() + Duration::from_secs(10);
        let stream = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() > deadline {
                        let _ = child.kill();
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "core did not connect to socket",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = child.kill();
                    return Err(e);
                }
            }
        };
        stream.set_nonblocking(false)?;
        let client = LibcoreClient::from_stream(stream)?;
        Ok(Self {
            listener_path: path,
            child: Some(child),
            client: Some(client),
        })
    }

    pub fn query_state(&mut self) -> Result<(bool, i32), String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("QueryState", &[]).map_err(|e| e.to_string())?;
        Ok(decode_core_state(&data))
    }

    pub fn check_config(&mut self, json: &str) -> Result<Option<String>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let payload = encode_load_config_core_json(json);
        let data = c.call("CheckConfig", &payload).map_err(|e| e.to_string())?;
        Ok(decode_error_resp(&data))
    }

    pub fn stop_rpc(&mut self) -> Result<Option<String>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("Stop", &[]).map_err(|e| e.to_string())?;
        Ok(decode_error_resp(&data))
    }

    pub fn stop_core_process(&mut self) -> io::Result<()> {
        if let Some(c) = self.client.take() {
            c.shutdown();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(&self.listener_path);
        Ok(())
    }
}

/// Process-wide optional session for Tauri commands (Phase B smoke).
pub static SESSION: Mutex<Option<CoreSession>> = Mutex::new(None);
