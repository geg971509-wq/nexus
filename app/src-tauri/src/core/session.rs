//! GUI-side session: listen unix socket, spawn NexusCore, accept, libcore calls.
use super::elevate::{ensure_setuid_core, path_has_setuid, privileged_core_path};
use super::frame::LibcoreClient;
use super::proto_min::{
        decode_core_state, decode_error_resp, decode_has_privilege, decode_query_connections,
        encode_load_config_core_json, encode_load_config_req, ConnRow,
    };
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

    /// Bundle / env Core (may be on nosuid volume — not for Tun).
    pub fn resolve_bundle_core() -> PathBuf {
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

    /// Prefer setuid Application Support copy when present (Tun); else bundle.
    pub fn resolve_core_binary() -> PathBuf {
        let priv_p = privileged_core_path();
        if priv_p.is_file() && path_has_setuid(&priv_p) {
            return priv_p;
        }
        Self::resolve_bundle_core()
    }

    /// Throne get_elevated_permissions: setuid copy + password sheet if needed.
    pub fn ensure_privileged_core() -> Result<PathBuf, String> {
        let src = Self::resolve_bundle_core();
        if src.as_os_str().is_empty() || !src.is_file() {
            return Err(format!("NexusCore not found at {}", src.display()));
        }
        ensure_setuid_core(&src)
    }

    /// Core child stdout/stderr. Piped+unread → kernel pipe fills → Core blocks
    /// on write while mixed still LISTENs (sysproxy "stuck").
    fn core_stdio_sinks() -> (Stdio, Stdio) {
        let log_path = Self::dirs_core_log();
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(f) => {
                // second handle for stderr (same file)
                match f.try_clone() {
                    Ok(f2) => (Stdio::from(f), Stdio::from(f2)),
                    Err(_) => (Stdio::from(f), Stdio::null()),
                }
            }
            Err(_) => (Stdio::null(), Stdio::null()),
        }
    }

    fn dirs_core_log() -> PathBuf {
        // ~/Library/Logs/Nexus/core.log (macOS); fallback temp
        if let Some(home) = std::env::var_os("HOME") {
            let dir = PathBuf::from(home).join("Library/Logs/Nexus");
            let _ = std::fs::create_dir_all(&dir);
            return dir.join("core.log");
        }
        std::env::temp_dir().join("nexus-core.log")
    }

    fn core_workdir() -> PathBuf {
        if let Some(home) = std::env::var_os("HOME") {
            let dir = PathBuf::from(home).join("Library/Application Support/Nexus");
            let _ = std::fs::create_dir_all(&dir);
            return dir;
        }
        std::env::temp_dir()
    }

    /// GUI crash / force-quit leaves NexusCore as ppid=1 with exclusive
    /// bbolt on cache.db + :2080. Next Start → `initialize cache-file: timeout`.
    /// Kill every other NexusCore before we spawn (keep `except` = our child).
    pub fn kill_stray_cores(except: Option<u32>) {
        let Ok(out) = Command::new("pgrep").args(["-f", "NexusCore"]).output() else {
            return;
        };
        if !out.status.success() && out.stdout.is_empty() {
            return;
        }
        let me = std::process::id();
        for tok in String::from_utf8_lossy(&out.stdout).split_whitespace() {
            let Ok(pid) = tok.parse::<u32>() else { continue };
            if pid == me {
                continue;
            }
            if except == Some(pid) {
                continue;
            }
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }
        // brief wait so bbolt / :2080 release
        std::thread::sleep(Duration::from_millis(250));
        // stubborn leftovers
        let Ok(out2) = Command::new("pgrep").args(["-f", "NexusCore"]).output() else {
            return;
        };
        for tok in String::from_utf8_lossy(&out2.stdout).split_whitespace() {
            let Ok(pid) = tok.parse::<u32>() else { continue };
            if pid == me || except == Some(pid) {
                continue;
            }
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status();
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// Absolute cache.db path (must match generate.rs).
    pub fn cache_db_path() -> PathBuf {
        Self::core_workdir().join("cache.db")
    }

    pub fn start(core_bin: &Path) -> io::Result<Self> {
        // Drop orphans first — they hold cache.db exclusively (bbolt).
        Self::kill_stray_cores(None);

        let path = Self::socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        listener.set_nonblocking(true)?;

        // File sink (not piped): keeps THRONE_CORE_DEBUG without freezing Core.
        // cwd under Application Support so relative paths never hit `/` (read-only).
        let (stdout, stderr) = Self::core_stdio_sinks();
        let core_cwd = Self::core_workdir();
        let mut child = Command::new(core_bin)
            .current_dir(&core_cwd)
            .env("THRONE_CORE_SOCKET", &path)
            .env("THRONE_CORE_DEBUG", "1")
            .stdout(stdout)
            .stderr(stderr)
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

    /// Core euid==0 (setuid child). Throne IsPrivileged RPC.
    pub fn is_privileged(&mut self) -> Result<bool, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("IsPrivileged", &[]).map_err(|e| e.to_string())?;
        Ok(decode_has_privilege(&data))
    }

    /// Kill live unprivileged Core and spawn setuid copy (Tun path).
    pub fn recycle_privileged(&mut self) -> Result<(), String> {
        let bin = Self::ensure_privileged_core()?;
        let _ = self.stop_core_process();
        *self = Self::start(&bin).map_err(|e| format!("restart privileged core: {e}"))?;
        Ok(())
    }

    pub fn check_config(&mut self, json: &str) -> Result<Option<String>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let payload = encode_load_config_core_json(json);
        let data = c.call("CheckConfig", &payload).map_err(|e| e.to_string())?;
        Ok(decode_error_resp(&data))
    }

    /// Throne Client::Start(LoadConfigReq) — load sing-box config into running Core process.
    pub fn start_rpc(&mut self, core_json: &str, profile_id: i32) -> Result<Option<String>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let payload = encode_load_config_req(core_json, Some(profile_id));
        // Start can take longer than smoke CheckConfig (box create + bind)
        let data = c
            .call_timeout("Start", &payload, std::time::Duration::from_secs(60))
            .map_err(|e| e.to_string())?;
        Ok(decode_error_resp(&data))
    }

    pub fn stop_rpc(&mut self) -> Result<Option<String>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("Stop", &[]).map_err(|e| e.to_string())?;
        Ok(decode_error_resp(&data))
    }

    /// Throne QueryConnections — live traffic rows for the connection table.
    pub fn query_connections(&mut self) -> Result<Vec<ConnRow>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("QueryConnections", &[]).map_err(|e| e.to_string())?;
        Ok(decode_query_connections(&data))
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

    /// True if any NexusCore process is still alive (incl. orphan after GUI quit).
    pub fn core_process_alive() -> bool {
        let Ok(out) = Command::new("pgrep").args(["-f", "NexusCore"]).output() else {
            return false;
        };
        if out.stdout.is_empty() {
            return false;
        }
        let me = std::process::id();
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .filter_map(|t| t.parse::<u32>().ok())
            .any(|pid| pid != me)
    }

    /// Mixed inbound still accepting (tunnel Start left it up even if GUI SESSION died).
    pub fn mixed_port_open(port: u16) -> bool {
        use std::net::{SocketAddr, TcpStream};
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
    }
}

/// Process-wide optional session for Tauri commands (Phase B smoke).
pub static SESSION: Mutex<Option<CoreSession>> = Mutex::new(None);
