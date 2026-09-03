//! GUI-side session: listen IPC, spawn NexusCore, accept, libcore calls.
use super::elevate::{ensure_setuid_core, path_has_setuid, privileged_core_path};
use super::frame::{LibcoreClient, LibcoreControl};
use super::proto_min::{
    decode_core_state, decode_error_resp, decode_has_privilege, decode_query_connections,
    decode_query_stats_proxy, decode_test_resp, encode_load_config_core_json,
    encode_load_config_req, encode_test_req_current, ConnRow, UrlTestRow, DEFAULT_URL_TEST,
};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};

static CORE_SOCKET_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct CoreSession {
    /// Unix socket path passed to Core via NEXUS_CORE_SOCKET.
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
            // Core's corelock never unlinks its own file, so these accumulated one
            // per run. Safe here: stop_core_process already ran, so nothing holds
            // the flock. An abnormal exit still leaves one behind — that is what
            // the lock file's wider mode covers.
            let mut lock = self.listener_path.clone().into_os_string();
            lock.push(".lock");
            let _ = std::fs::remove_file(std::path::PathBuf::from(lock));
        }
    }
}

impl CoreSession {
    /// IPC filesystem path passed to Core.
    pub fn socket_path() -> PathBuf {
        #[cfg(unix)]
        {
            let dir = std::env::temp_dir().join("nexus");
            let seq = CORE_SOCKET_SEQ
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1);
            dir.join(format!("nexus-core-{}-{seq}.sock", std::process::id()))
        }
    }

    /// Create and verify the private parent directory before a socket is bound.
    /// Core's parent-PID check is defense in depth; this 0700 directory is the
    /// first boundary and must not silently degrade to a shared path.
    #[cfg(unix)]
    fn prepare_socket_dir(path: &Path) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let dir = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "core socket has no parent directory",
            )
        })?;
        std::fs::create_dir_all(dir)?;
        let meta = std::fs::symlink_metadata(dir)?;
        if !meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("core socket parent is not a directory: {}", dir.display()),
            ));
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        let mode = std::fs::metadata(dir)?.permissions().mode() & 0o777;
        if mode != 0o700 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("core socket parent mode is {mode:o}, expected 700"),
            ));
        }
        Ok(())
    }

    fn core_bin_name() -> &'static str {
        "NexusCore"
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

    /// Pay the kernel's first-exec cost for Core off the connect path.
    ///
    /// macOS validates a binary's signature the first time a given file is
    /// executed and caches the verdict per file. Core is ~68 MB with about 17k
    /// hash pages, so that first run measures ~0.9s against ~0.01s once cached —
    /// and a rebuild produces a new file, which is why the delay reappears only
    /// after compiling. Running it once at startup moves that off the power
    /// button and into the seconds before the user reaches for it.
    ///
    /// `version` exits immediately and touches nothing: no socket, no config.
    pub fn warm_binary_cache() {
        std::thread::Builder::new()
            .name("nexus-core-warm".into())
            .spawn(|| {
                let bin = Self::resolve_core_binary();
                if bin.as_os_str().is_empty() || !bin.is_file() {
                    return;
                }
                let _ = Command::new(bin)
                    .arg("version")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            })
            .ok();
    }

    /// Roll core.log over once it gets large, keeping one generation.
    ///
    /// Core logs every outbound destination at info level and nothing ever
    /// trimmed it — 36 MB over five days here, unbounded. Rotating rather than
    /// truncating because the previous session's log is exactly what a crash
    /// report needs, and diagnosis is the only reason this file exists.
    /// Runs at spawn, when no Core holds the file.
    fn rotate_core_log(path: &Path) {
        const MAX_LOG_BYTES: u64 = 16 * 1024 * 1024;
        let too_big = std::fs::metadata(path)
            .map(|m| m.len() > MAX_LOG_BYTES)
            .unwrap_or(false);
        if !too_big {
            return;
        }
        let mut prev = path.to_path_buf().into_os_string();
        prev.push(".1");
        let _ = std::fs::rename(path, std::path::PathBuf::from(prev));
    }

    fn core_stdio_sinks() -> (Stdio, Stdio) {
        let log_path = Self::dirs_core_log();
        Self::rotate_core_log(&log_path);
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

    /// Executable names a real Core can have: the bundled sidecar and the setuid
    /// copy installed under /Library/PrivilegedHelperTools.
    #[cfg(unix)]
    const CORE_EXEC_NAMES: [&'static str; 2] = ["NexusCore", "app.nexus.NexusCore"];

    /// PIDs whose executable *is* a Core.
    ///
    /// Not `pgrep -f NexusCore`: that matches the whole command line, so
    /// `tail -f bin/NexusCore.log`, an editor holding a Core source file, or the
    /// `go build -o bin/NexusCore` inside build.sh all matched — and this runs on
    /// every connect and disconnect, so they got SIGTERM and then SIGKILL.
    /// `ps -o comm=` gives the executable path; compare its basename exactly.
    #[cfg(unix)]
    fn core_pids() -> Vec<u32> {
        let Ok(out) = Command::new("/bin/ps")
            .args(["-axo", "pid=,comm="])
            .output()
        else {
            return Vec::new();
        };
        parse_core_pids(&String::from_utf8_lossy(&out.stdout))
    }

    /// Kill every other NexusCore before we spawn (keep `except` = our child).
    pub fn kill_stray_cores(except: Option<u32>) {
        #[cfg(unix)]
        {
            let me = std::process::id();
            for pid in Self::core_pids() {
                if pid == me || except == Some(pid) {
                    continue;
                }
                let _ = Command::new("/bin/kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
            // Poll exit instead of fixed 250ms; only KILL survivors.
            let deadline = Instant::now() + Duration::from_millis(400);
            loop {
                let mut survivors = false;
                for pid in Self::core_pids() {
                    if pid == me || except == Some(pid) {
                        continue;
                    }
                    survivors = true;
                    if Instant::now() >= deadline {
                        let _ = Command::new("/bin/kill")
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
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    pub fn cache_db_path() -> PathBuf {
        Self::core_workdir().join("cache.db")
    }

    #[cfg(unix)]
    pub fn start(core_bin: &Path) -> io::Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::net::UnixListener;

        Self::kill_stray_cores(None);

        let path = Self::socket_path();
        Self::prepare_socket_dir(&path)?;
        let env_socket = path.to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
            drop(listener);
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
        listener.set_nonblocking(true)?;

        let (stdout, stderr) = Self::core_stdio_sinks();
        let core_cwd = Self::core_workdir();
        let mut child = Command::new(core_bin)
            .current_dir(&core_cwd)
            .env("NEXUS_CORE_SOCKET", &env_socket)
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

    pub fn query_state(&mut self) -> Result<(bool, i32), String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c
            .call_timeout("QueryState", &[], Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        decode_core_state(&data)
    }

    pub fn is_privileged(&mut self) -> Result<bool, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("IsPrivileged", &[]).map_err(|e| e.to_string())?;
        decode_has_privilege(&data)
    }

    /// Takes the already-elevated binary rather than elevating itself: callers hold
    /// the SESSION lock here, and `ensure_privileged_core` can raise an osascript
    /// password sheet that blocks on user input for as long as the user ignores it.
    pub fn recycle_privileged(&mut self, bin: &Path) -> Result<(), String> {
        let _ = self.stop_core_process();
        *self = Self::start(bin).map_err(|e| format!("restart privileged core: {e}"))?;
        Ok(())
    }

    pub fn check_config(&mut self, json: &str) -> Result<Option<String>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let payload = encode_load_config_core_json(json);
        let data = c.call("CheckConfig", &payload).map_err(|e| e.to_string())?;
        decode_error_resp(&data)
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
        decode_error_resp(&data)
    }

    pub fn stop_rpc(&mut self) -> Result<Option<String>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c.call("Stop", &[]).map_err(|e| e.to_string())?;
        decode_error_resp(&data)
    }

    pub fn query_connections(&mut self) -> Result<Vec<ConnRow>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c
            .call_timeout("QueryConnections", &[], Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        decode_query_connections(&data)
    }

    /// Cumulative outbound traffic for tag `proxy` (TrafficManager.TotalOutbound).
    pub fn query_stats_proxy(&mut self) -> Result<(i64, i64), String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let data = c
            .call_timeout("QueryStats", &[], Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        decode_query_stats_proxy(&data)
    }

    /// One-shot Core `Test(test_current=true)` via live box proxy/default outbound.
    /// `call_timeout` ≥ TestTimeoutMs + 5s slack so slow peers do not trip IPC.
    pub fn test_current_url(
        &mut self,
        url: &str,
        timeout_ms: i32,
        sent: impl FnOnce(LibcoreControl),
    ) -> Result<Vec<UrlTestRow>, String> {
        let c = self.client.as_mut().ok_or("no client")?;
        let control = c.control_handle().map_err(|e| e.to_string())?;
        let timeout_ms = if timeout_ms > 0 { timeout_ms } else { 3000 };
        let url = if url.is_empty() {
            DEFAULT_URL_TEST
        } else {
            url
        };
        let payload = encode_test_req_current(url, timeout_ms, 1);
        let call_to = Duration::from_millis((timeout_ms as u64).saturating_add(5_000).max(10_000));
        let data = c
            .call_timeout_with_sent("Test", &payload, call_to, || sent(control))
            .map_err(|e| e.to_string())?;
        decode_test_resp(&data)
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

    /// The IPC stream lost its frame boundary. Core may well still be healthy,
    /// but this connection can only misparse now, so the session has to go.
    pub fn client_broken(&self) -> bool {
        self.client.as_ref().is_some_and(|c| c.is_broken())
    }

    pub fn core_process_alive() -> bool {
        // Same executable-name match as kill_stray_cores. A command line that
        // merely mentions NexusCore must not look like a live Core.
        let me = std::process::id();
        Self::core_pids().into_iter().any(|pid| pid != me)
    }

    pub fn mixed_port_open(port: u16) -> bool {
        use std::net::{SocketAddr, TcpStream};
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
    }
}

/// Process-wide optional Core session shared by the C ABI commands.
pub static SESSION: Mutex<Option<CoreSession>> = Mutex::new(None);

/// Pick out PIDs whose executable basename is a Core, from `ps -axo pid=,comm=`.
///
/// Split from the spawn so the matching can be exercised against real `ps` text:
/// this is the guard that stops a command line merely *mentioning* NexusCore from
/// being SIGKILLed.
#[cfg(unix)]
fn parse_core_pids(ps_output: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    for line in ps_output.lines() {
        let line = line.trim_start();
        let Some((pid_s, comm)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(pid) = pid_s.trim().parse::<u32>() else {
            continue;
        };
        let name = comm.trim().rsplit('/').next().unwrap_or("");
        if CoreSession::CORE_EXEC_NAMES.contains(&name) {
            pids.push(pid);
        }
    }
    pids
}

#[cfg(all(test, unix))]
mod stray_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Verbatim shape of `ps -axo pid=,comm=` on macOS, including the setuid copy
    /// under its own name and the decoys that `pgrep -f NexusCore` used to hit.
    const PS: &str = "\
  1 /sbin/launchd
 5948 /Library/PrivilegedHelperTools/app.nexus.NexusCore
 6001 /Volumes/work/bin/NexusCore
 5103 /usr/bin/tail
 7000 /usr/local/go/bin/go
 7100 /Applications/Nexus.app/Contents/MacOS/nexus
 7200 /usr/bin/NexusCoreHelper
";

    #[test]
    fn matches_both_core_names_only() {
        let pids = parse_core_pids(PS);
        assert_eq!(pids, vec![5948, 6001], "{pids:?}");
    }

    /// The whole point: `tail -f bin/NexusCore.log` and `go build -o bin/NexusCore`
    /// carry the string on their command line but are not Cores. comm holds the
    /// executable, so they never appear — and neither does a longer name that
    /// merely starts with ours.
    #[test]
    fn decoys_and_prefix_names_are_not_cores() {
        let pids = parse_core_pids(PS);
        for not_core in [5103u32, 7000, 7100, 7200] {
            assert!(!pids.contains(&not_core), "{not_core} matched: {pids:?}");
        }
    }

    #[test]
    fn junk_lines_are_skipped() {
        assert!(parse_core_pids("").is_empty());
        assert!(parse_core_pids("garbage\nno-pid /bin/NexusCore\n").is_empty());
    }

    #[test]
    fn socket_paths_are_unique_per_session() {
        let first = CoreSession::socket_path();
        let second = CoreSession::socket_path();
        assert_ne!(first, second);
    }

    #[test]
    fn socket_parent_is_restricted_before_bind() {
        let dir = std::env::temp_dir().join(format!(
            "nexus-socket-perm-test-{}-{}",
            std::process::id(),
            CORE_SOCKET_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let path = dir.join("core.sock");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        CoreSession::prepare_socket_dir(&path).unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(all(test, unix))]
mod log_rotate_tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nexus-log-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Under the ceiling the log is left alone — rotating every launch would
    /// throw away the history a bug report needs.
    #[test]
    fn small_log_is_untouched() {
        let d = tmp("small");
        let p = d.join("core.log");
        std::fs::write(&p, b"recent lines").unwrap();
        CoreSession::rotate_core_log(&p);
        assert_eq!(std::fs::read(&p).unwrap(), b"recent lines");
        assert!(!d.join("core.log.1").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Over the ceiling it moves aside instead of being truncated, so the
    /// previous session survives one more launch.
    #[test]
    fn large_log_is_rotated_not_dropped() {
        let d = tmp("large");
        let p = d.join("core.log");
        std::fs::write(&p, vec![b'x'; 17 * 1024 * 1024]).unwrap();
        CoreSession::rotate_core_log(&p);
        assert!(!p.exists(), "caller reopens it fresh");
        let rolled = d.join("core.log.1");
        assert_eq!(std::fs::metadata(&rolled).unwrap().len(), 17 * 1024 * 1024);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_log_is_not_an_error() {
        let d = tmp("missing");
        CoreSession::rotate_core_log(&d.join("core.log"));
        assert!(!d.join("core.log.1").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
