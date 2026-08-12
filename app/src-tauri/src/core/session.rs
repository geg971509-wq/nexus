//! GUI-side session: listen IPC, spawn NexusCore, accept, libcore calls.
use super::elevate::{ensure_setuid_core, path_has_setuid, privileged_core_path};
use super::frame::LibcoreClient;
use super::proto_min::{
    decode_core_state, decode_error_resp, decode_has_privilege, decode_query_connections,
    decode_query_stats_proxy, encode_load_config_core_json, encode_load_config_req, ConnRow,
};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct CoreSession {
    /// Path or named-pipe id passed to Core via NEXUS_CORE_SOCKET.
    /// Unix Drop unlinks the sock; Windows only needs it at spawn (pipe is not a file).
    #[cfg_attr(windows, allow(dead_code))]
    listener_path: PathBuf,
    child: Option<Child>,
    client: Option<LibcoreClient>,
}

impl Drop for CoreSession {
    fn drop(&mut self) {
        let _ = self.stop_core_process();
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.listener_path);
        }
    }
}

impl CoreSession {
    /// IPC endpoint name for Core env. Unix = filesystem path; Windows = `\\.\pipe\…`.
    pub fn socket_path() -> PathBuf {
        #[cfg(unix)]
        {
            let dir = std::env::temp_dir().join("nexus");
            let _ = std::fs::create_dir_all(&dir);
            return dir.join(format!("nexus-core-{}.sock", std::process::id()));
        }
        #[cfg(windows)]
        {
            PathBuf::from(format!(r"\\.\pipe\nexus-core-{}", std::process::id()))
        }
    }

    fn core_bin_name() -> &'static str {
        #[cfg(windows)]
        {
            "NexusCore.exe"
        }
        #[cfg(not(windows))]
        {
            "NexusCore"
        }
    }

    /// Bundle / env Core (may be on nosuid volume — not for Tun on macOS).
    pub fn resolve_bundle_core() -> PathBuf {
        if let Ok(p) = std::env::var("NEXUS_CORE_BIN") {
            let pb = PathBuf::from(&p);
            if pb.is_absolute() && pb.is_file() {
                return pb;
            }
        }
        let bin = Self::core_bin_name();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let beside = dir.join(bin);
                if beside.is_file() {
                    return beside;
                }
                if let Ok(rd) = std::fs::read_dir(dir) {
                    for e in rd.flatten() {
                        let n = e.file_name();
                        let s = n.to_string_lossy();
                        if s == bin || s.starts_with("NexusCore-") || s.starts_with("NexusCore.") {
                            let p = e.path();
                            if p.is_file() {
                                return p;
                            }
                        }
                    }
                }
            }
        }
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../bin")
                .join(bin),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../bin")
                .join(bin),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join(bin),
        ];
        for c in candidates {
            if c.is_file() {
                return c;
            }
        }
        PathBuf::from("")
    }

    /// Prefer setuid Application Support copy when present (Tun macOS); else bundle.
    pub fn resolve_core_binary() -> PathBuf {
        let priv_p = privileged_core_path();
        if priv_p.is_file() && path_has_setuid(&priv_p) {
            return priv_p;
        }
        Self::resolve_bundle_core()
    }

    pub fn ensure_privileged_core() -> Result<PathBuf, String> {
        let src = Self::resolve_bundle_core();
        if src.as_os_str().is_empty() || !src.is_file() {
            return Err(format!("NexusCore not found at {}", src.display()));
        }
        ensure_setuid_core(&src)
    }

    fn core_stdio_sinks() -> (Stdio, Stdio) {
        let log_path = Self::dirs_core_log();
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(f) => match f.try_clone() {
                Ok(f2) => (Stdio::from(f), Stdio::from(f2)),
                Err(_) => (Stdio::from(f), Stdio::null()),
            },
            Err(_) => (Stdio::null(), Stdio::null()),
        }
    }

    fn dirs_core_log() -> PathBuf {
        crate::paths::ensure_log_dir().join("core.log")
    }

    fn core_workdir() -> PathBuf {
        crate::paths::ensure_data_dir()
    }

    /// Kill every other NexusCore before we spawn (keep `except` = our child).
    pub fn kill_stray_cores(except: Option<u32>) {
        #[cfg(unix)]
        {
            let Ok(out) = Command::new("pgrep").args(["-f", "NexusCore"]).output() else {
                return;
            };
            if !out.status.success() && out.stdout.is_empty() {
                return;
            }
            let me = std::process::id();
            for tok in String::from_utf8_lossy(&out.stdout).split_whitespace() {
                let Ok(pid) = tok.parse::<u32>() else {
                    continue;
                };
                if pid == me || except == Some(pid) {
                    continue;
                }
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
            // Poll exit instead of fixed 250ms; only KILL survivors.
            let deadline = Instant::now() + Duration::from_millis(400);
            loop {
                let Ok(out2) = Command::new("pgrep").args(["-f", "NexusCore"]).output() else {
                    return;
                };
                let mut survivors = false;
                for tok in String::from_utf8_lossy(&out2.stdout).split_whitespace() {
                    let Ok(pid) = tok.parse::<u32>() else {
                        continue;
                    };
                    if pid == me || except == Some(pid) {
                        continue;
                    }
                    survivors = true;
                    if Instant::now() >= deadline {
                        let _ = Command::new("kill")
                            .args(["-KILL", &pid.to_string()])
                            .status();
                    }
                }
                if !survivors {
                    return;
                }
                if Instant::now() >= deadline {
                    std::thread::sleep(Duration::from_millis(40));
                    return;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
        }
        #[cfg(windows)]
        {
            // Honor `except`: never blanket /IM (would kill the live child).
            let me = std::process::id();
            let mut list = Command::new("tasklist");
            crate::winhide::apply(&mut list);
            let Ok(out) = list
                .args(["/FI", "IMAGENAME eq NexusCore.exe", "/FO", "CSV", "/NH"])
                .output()
            else {
                return;
            };
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                // "NexusCore.exe","1234","Session Name","Session#","Mem Usage"
                let cols: Vec<&str> = line.split(',').collect();
                if cols.len() < 2 {
                    continue;
                }
                let pid_s = cols[1].trim().trim_matches('"');
                let Ok(pid) = pid_s.parse::<u32>() else {
                    continue;
                };
                if pid == me || except == Some(pid) {
                    continue;
                }
                let mut kill = Command::new("taskkill");
                crate::winhide::apply(&mut kill);
                let _ = kill
                    .args(["/F", "/PID", &pid.to_string(), "/T"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    pub fn cache_db_path() -> PathBuf {
        Self::core_workdir().join("cache.db")
    }

    #[cfg(unix)]
    pub fn start(core_bin: &Path) -> io::Result<Self> {
        use std::os::unix::net::UnixListener;

        Self::kill_stray_cores(None);

        let path = Self::socket_path();
        let env_socket = path.to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        listener.set_nonblocking(true)?;

        let (stdout, stderr) = Self::core_stdio_sinks();
        let core_cwd = Self::core_workdir();
        let mut child = Command::new(core_bin)
            .current_dir(&core_cwd)
            .env("NEXUS_CORE_SOCKET", &env_socket)
            .env("NEXUS_CORE_DEBUG", "1")
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
                        let _ = child.wait();
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "core did not connect to socket",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
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

    #[cfg(windows)]
    pub fn start(core_bin: &Path) -> io::Result<Self> {
        use super::winpipe;

        Self::kill_stray_cores(None);

        let short = format!("nexus-core-{}", std::process::id());
        let full_pipe = format!(r"\\.\pipe\{short}");

        // Accept thread first so Core's DialPipe can connect immediately after spawn.
        let (tx, rx) = std::sync::mpsc::channel();
        let pipe_for_accept = full_pipe.clone();
        std::thread::spawn(move || {
            let _ = tx.send(winpipe::accept_one(&pipe_for_accept, Duration::from_secs(20)));
        });
        // brief settle so CreateNamedPipe is listening before child starts
        std::thread::sleep(Duration::from_millis(80));

        let (stdout, stderr) = Self::core_stdio_sinks();
        let core_cwd = Self::core_workdir();
        let mut cmd = Command::new(core_bin);
        crate::winhide::apply(&mut cmd);
        let mut child = cmd
            .current_dir(&core_cwd)
            .env("NEXUS_CORE_SOCKET", &full_pipe)
            .env("NEXUS_CORE_DEBUG", "1")
            .stdout(stdout)
            .stderr(stderr)
            .spawn()?;

        let deadline = Instant::now() + Duration::from_secs(20);
        let stream = loop {
            match rx.try_recv() {
                Ok(Ok(s)) => break s,
                Ok(Err(e)) => {
                    let _ = child.kill();
                    return Err(e);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    if Instant::now() > deadline {
                        let _ = child.kill();
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "core did not connect to named pipe",
                        ));
                    }
                    if let Ok(Some(status)) = child.try_wait() {
                        return Err(io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            format!("core exited before IPC connect: {status}"),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let _ = child.kill();
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "accept thread died",
                    ));
                }
            }
        };

        let client = LibcoreClient::from_stream(stream)?;
        Ok(Self {
            listener_path: PathBuf::from(full_pipe),
            child: Some(child),
            client: Some(client),
        })
    }

    pub fn query_state(&mut self) -> Result<(bool, i32), String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("QueryState", &[]).map_err(|e| e.to_string())?;
        Ok(decode_core_state(&data))
    }

    pub fn is_privileged(&mut self) -> Result<bool, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("IsPrivileged", &[]).map_err(|e| e.to_string())?;
        Ok(decode_has_privilege(&data))
    }

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

    pub fn start_rpc(
        &mut self,
        core_json: &str,
        profile_id: i32,
    ) -> Result<Option<String>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let payload = encode_load_config_req(core_json, Some(profile_id));
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

    pub fn query_connections(&mut self) -> Result<Vec<ConnRow>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("QueryConnections", &[]).map_err(|e| e.to_string())?;
        Ok(decode_query_connections(&data))
    }

    /// Cumulative outbound traffic for tag `proxy` (TrafficManager.TotalOutbound).
    pub fn query_stats_proxy(&mut self) -> Result<(i64, i64), String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("QueryStats", &[]).map_err(|e| e.to_string())?;
        Ok(decode_query_stats_proxy(&data))
    }

    pub fn stop_core_process(&mut self) -> io::Result<()> {
        if let Some(c) = self.client.take() {
            c.shutdown();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.listener_path);
        }
        Ok(())
    }

    /// True when owned child has exited (SESSION may still be Some).
    pub fn child_exited(&mut self) -> bool {
        match self.child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(Some(_))),
            None => true,
        }
    }

    pub fn core_process_alive() -> bool {
        #[cfg(unix)]
        {
            let Ok(out) = Command::new("pgrep").args(["-f", "NexusCore"]).output() else {
                return false;
            };
            if out.stdout.is_empty() {
                return false;
            }
            let me = std::process::id();
            return String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .filter_map(|t| t.parse::<u32>().ok())
                .any(|pid| pid != me);
        }
        #[cfg(windows)]
        {
            let mut cmd = Command::new("tasklist");
            crate::winhide::apply(&mut cmd);
            let Ok(out) = cmd
                .args(["/FI", "IMAGENAME eq NexusCore.exe", "/NH"])
                .output()
            else {
                return false;
            };
            let s = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
            return s.contains("nexuscore.exe");
        }
        #[allow(unreachable_code)]
        false
    }

    pub fn mixed_port_open(port: u16) -> bool {
        use std::net::{SocketAddr, TcpStream};
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
    }
}

/// Process-wide optional session for Tauri commands.
pub static SESSION: Mutex<Option<CoreSession>> = Mutex::new(None);
