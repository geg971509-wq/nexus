//! macOS firewall client: talk to NexusFwD; install/uninstall LaunchDaemon (H2).

use super::wire::{PolicyDto, Request, Response};
use super::Policy;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const SOCK: &str = "/var/run/nexusfwd.sock";
const PLIST_LABEL: &str = "app.nexus.firewall";
const PLIST_PATH: &str = "/Library/LaunchDaemons/app.nexus.firewall.plist";
const HELPER_PATH: &str = "/Library/PrivilegedHelperTools/app.nexus.NexusFwD";

pub fn apply_policy(policy: &Policy) -> Result<(), String> {
    ensure_helper_ready()?;
    match policy {
        Policy::Reset => rpc(&Request::Reset),
        other => rpc(&Request::Apply {
            policy: PolicyDto::from_policy(other),
        }),
    }
}

pub fn helper_status() -> (bool, bool, Option<String>) {
    // (installed, running, detail)
    let installed = Path::new(HELPER_PATH).is_file() && Path::new(PLIST_PATH).is_file();
    match rpc_raw(&Request::Ping) {
        Ok(r) if r.ok => (installed, true, r.helper),
        Ok(r) => (installed, false, r.err),
        Err(e) => (installed, false, Some(e)),
    }
}

fn wait_helper_ping(budget: Duration) -> bool {
    let t0 = std::time::Instant::now();
    loop {
        if rpc_raw(&Request::Ping).map(|r| r.ok).unwrap_or(false) {
            return true;
        }
        if t0.elapsed() >= budget {
            return false;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

pub fn ensure_helper_ready() -> Result<(), String> {
    if wait_helper_ping(Duration::from_millis(0)) {
        return Ok(());
    }
    // Try kickstart if installed — poll ready instead of fixed 200ms sleep.
    if Path::new(PLIST_PATH).is_file() {
        let _ = Command::new("/bin/launchctl")
            .args(["kickstart", "-k", &format!("system/{PLIST_LABEL}")])
            .output();
        if wait_helper_ping(Duration::from_millis(800)) {
            return Ok(());
        }
    }
    Err("firewall helper not running — install via the Firewall tab (NexusFwD)".into())
}

/// One-shot admin install: copy binary + plist + bootstrap.
pub fn install_helper(src_bin: &Path) -> Result<(), String> {
    if !src_bin.is_file() {
        return Err(format!("NexusFwD source missing: {}", src_bin.display()));
    }
    let plist = plist_body(HELPER_PATH);
    let tmp_plist = crate::paths::data_dir().join("app.nexus.firewall.plist");
    if let Some(p) = tmp_plist.parent() {
        let _ = fs::create_dir_all(p);
    }
    fs::write(&tmp_plist, plist).map_err(|e| format!("write plist: {e}"))?;

    // L1: installer uid → /var/run/nexusfwd.allow for socket peer allowlist
    let my_uid = unsafe { libc::getuid() };
    let allow_tmp = crate::paths::data_dir().join("nexusfwd.allow");
    fs::write(&allow_tmp, format!("{my_uid}\n")).map_err(|e| format!("write allow: {e}"))?;

    let q_src = shq(&src_bin.to_string_lossy());
    let q_dest = shq(HELPER_PATH);
    let q_plist_src = shq(&tmp_plist.to_string_lossy());
    let q_plist = shq(PLIST_PATH);
    let q_allow_src = shq(&allow_tmp.to_string_lossy());
    let shell = format!(
        "/bin/mkdir -p /Library/PrivilegedHelperTools /Library/LaunchDaemons && \
         /bin/cp -f {q_src} {q_dest} && /usr/sbin/chown root:wheel {q_dest} && /bin/chmod 755 {q_dest} && \
         /bin/cp -f {q_plist_src} {q_plist} && /usr/sbin/chown root:wheel {q_plist} && /bin/chmod 644 {q_plist} && \
         /bin/cp -f {q_allow_src} /var/run/nexusfwd.allow && /usr/sbin/chown root:wheel /var/run/nexusfwd.allow && /bin/chmod 644 /var/run/nexusfwd.allow && \
         /bin/launchctl bootout system/{PLIST_LABEL} >/dev/null 2>&1; \
         /bin/launchctl bootstrap system {q_plist} && \
         /bin/launchctl enable system/{PLIST_LABEL} && \
         /bin/launchctl kickstart -k system/{PLIST_LABEL}"
    );
    run_admin(&shell)?;
    // Poll sock ready (was fixed 300ms) — first install often answers in <100ms.
    if wait_helper_ping(Duration::from_millis(1500)) {
        return Ok(());
    }
    ensure_helper_ready()
}

pub fn uninstall_helper() -> Result<(), String> {
    let shell = format!(
        "/bin/launchctl bootout system/{PLIST_LABEL} >/dev/null 2>&1; \
         /bin/rm -f {PLIST_PATH} {HELPER_PATH} /var/run/nexusfwd.allow /var/run/nexusfwd.sock /var/run/nexus-pf.conf; \
         /sbin/pfctl -a nexus -F all >/dev/null 2>&1; \
         /sbin/pfctl -a nexus -f /dev/null >/dev/null 2>&1; true"
    );
    run_admin(&shell)
}

fn plist_body(helper: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{PLIST_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{helper}</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/var/log/nexusfwd.log</string>
  <key>StandardErrorPath</key><string>/var/log/nexusfwd.log</string>
</dict>
</plist>
"#
    )
}

fn rpc(req: &Request) -> Result<(), String> {
    let r = rpc_raw(req)?;
    if r.ok {
        Ok(())
    } else {
        Err(r.err.unwrap_or_else(|| "helper error".into()))
    }
}

fn rpc_raw(req: &Request) -> Result<Response, String> {
    let mut stream = UnixStream::connect(SOCK).map_err(|e| format!("connect helper: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(15)))
        .ok();
    let line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("write helper: {e}"))?;
    stream.write_all(b"\n").map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    reader
        .read_line(&mut resp_line)
        .map_err(|e| format!("read helper: {e}"))?;
    serde_json::from_str(resp_line.trim()).map_err(|e| format!("helper resp: {e}"))
}

fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn run_admin(shell: &str) -> Result<(), String> {
    let mut script_shell = shell.replace('\\', "\\\\");
    script_shell = script_shell.replace('\"', "\\\"");
    let script = format!("do shell script \"{script_shell}\" with administrator privileges");
    let out = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        Err(if msg.trim().is_empty() {
            "administrator authentication failed or cancelled".into()
        } else {
            format!("install helper: {}", msg.trim())
        })
    }
}

/// Resolve staged NexusFwD next to app / target.
pub fn resolve_fwd_binary() -> PathBuf {
    // Dev: target/debug|release/nexusfwd
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("nexusfwd");
            if cand.is_file() {
                return cand;
            }
            // Contents/MacOS → Resources/nexusfwd
            if let Some(contents) = dir.parent() {
                let r = contents.join("Resources").join("nexusfwd");
                if r.is_file() {
                    return r;
                }
            }
        }
    }
    PathBuf::from("nexusfwd")
}

/// 7A: true when installed helper is missing or differs from staged (size or mtime).
pub fn helper_binary_stale(src: &Path) -> bool {
    let dest = Path::new(HELPER_PATH);
    if !dest.is_file() {
        return true;
    }
    let Ok(smeta) = fs::metadata(src) else {
        return false;
    };
    let Ok(dmeta) = fs::metadata(dest) else {
        return true;
    };
    if smeta.len() != dmeta.len() {
        return true;
    }
    match (smeta.modified(), dmeta.modified()) {
        (Ok(s), Ok(d)) => s > d,
        _ => false,
    }
}
