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
/// launchd stdout/stderr sink for the daemon. Pre-created 0600 at install because
/// launchd would otherwise make it 0644, and it only ever appends to an existing
/// file — so creating it ourselves is the one chance to set the mode.
const LOG_PATH: &str = "/var/log/nexusfwd.log";

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

fn sha256_file(path: &Path) -> Result<String, String> {
    let out = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(|e| format!("hash {}: {e}", path.display()))?;
    if !out.status.success() {
        return Err(format!(
            "hash {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let digest = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("invalid sha256 for {}", path.display()));
    }
    Ok(digest)
}

fn helper_install_shell(src_bin: &Path, expected_sha256: &str, my_uid: u32) -> String {
    let q_src = shq(&src_bin.to_string_lossy());
    let q_dest = shq(HELPER_PATH);
    let q_plist = shq(PLIST_PATH);
    let q_plist_body = shq(&plist_body(HELPER_PATH));
    format!(
        "/bin/mkdir -p /Library/PrivilegedHelperTools /Library/LaunchDaemons && \
         /bin/cp -f {q_src} {q_dest}.new && \
         actual=$(/usr/bin/shasum -a 256 {q_dest}.new | /usr/bin/cut -d ' ' -f 1) && \
         {{ /bin/test \"$actual\" = {expected_sha256} || {{ /bin/rm -f {q_dest}.new; exit 1; }}; }} && \
         /usr/sbin/chown root:wheel {q_dest}.new && /bin/chmod 755 {q_dest}.new && \
         /usr/bin/printf '%s' {q_plist_body} > {q_plist}.new && /usr/sbin/chown root:wheel {q_plist}.new && /bin/chmod 644 {q_plist}.new && \
         /usr/bin/printf '%s\\n' {my_uid} > /var/run/nexusfwd.allow.new && /usr/sbin/chown root:wheel /var/run/nexusfwd.allow.new && /bin/chmod 644 /var/run/nexusfwd.allow.new && \
         /usr/bin/touch {LOG_PATH} && /usr/sbin/chown root:wheel {LOG_PATH} && /bin/chmod 600 {LOG_PATH} && \
         (/bin/launchctl bootout system/{PLIST_LABEL} >/dev/null 2>&1 || true) && \
         /bin/mv -f {q_dest}.new {q_dest} && /bin/mv -f {q_plist}.new {q_plist} && /bin/mv -f /var/run/nexusfwd.allow.new /var/run/nexusfwd.allow && \
         /bin/launchctl bootstrap system {q_plist} && \
         /bin/launchctl enable system/{PLIST_LABEL} && \
         /bin/launchctl kickstart -k system/{PLIST_LABEL}"
    )
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
    let my_uid = unsafe { libc::getuid() };
    let expected_sha256 = sha256_file(src_bin)?;
    let shell = helper_install_shell(src_bin, &expected_sha256, my_uid);
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
         /bin/rm -f {PLIST_PATH} {HELPER_PATH} {LOG_PATH} /var/run/nexusfwd.allow /var/run/nexusfwd.sock /var/run/nexus-pf.conf; \
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
  <key>StandardOutPath</key><string>{LOG_PATH}</string>
  <key>StandardErrorPath</key><string>{LOG_PATH}</string>
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
    stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(15))).ok();
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

/// Resolve staged NexusFwD next to app / cargo target.
pub fn resolve_fwd_binary() -> PathBuf {
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
    // Dev: cargo target next to this crate when the .app is not yet bundled.
    let crate_target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    for profile in ["release", "debug"] {
        let cand = crate_target.join(profile).join("nexusfwd");
        if cand.is_file() {
            return cand;
        }
    }
    PathBuf::from("nexusfwd")
}

/// True when the installed helper is absent or not byte-for-byte the staged
/// helper. Size/mtime alone misses same-size rebuilds and older staged binaries.
pub fn helper_binary_stale(src: &Path) -> bool {
    let dest = Path::new(HELPER_PATH);
    if !src.is_file() || !dest.is_file() {
        return true;
    }

    let size_matches = match (fs::metadata(src), fs::metadata(dest)) {
        (Ok(s), Ok(d)) => s.len() == d.len(),
        _ => false,
    };
    if !size_matches {
        return true;
    }

    match (sha256_file(src), sha256_file(dest)) {
        (Ok(src_hash), Ok(dest_hash)) => src_hash != dest_hash,
        // Hashing is part of the install integrity contract. If it cannot be
        // established, force the install path to return a concrete error rather
        // than treating an unverifiable privileged helper as current.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_install_pins_binary_and_generates_root_metadata() {
        let digest = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
        let shell = helper_install_shell(Path::new("/tmp/nexusfwd"), digest, 501);
        let data_dir = crate::paths::data_dir().to_string_lossy().into_owned();

        assert!(
            shell.contains("/usr/bin/shasum -a 256") && shell.contains(digest),
            "helper staging copy must be pinned to the pre-auth digest: {shell}"
        );
        assert!(
            shell.contains("/bin/test") && !shell.contains("/usr/bin/test"),
            "digest compare must use /bin/test (no /usr/bin/test on current macOS): {shell}"
        );
        assert!(
            !shell.contains(&data_dir),
            "root install must not reopen user-writable staging paths: {shell}"
        );
        assert!(
            shell.contains("<key>ProgramArguments</key>") && shell.contains("501"),
            "plist and allow UID must be generated by the elevated script: {shell}"
        );
        assert!(
            Command::new("/bin/sh")
                .args(["-n", "-c", &shell])
                .status()
                .unwrap()
                .success(),
            "invalid elevated shell: {shell}"
        );
    }
}
