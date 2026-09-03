//! Tun privilege: macOS setuid-root NexusCore.

use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use std::time::Duration;

/// Root-owned dir for the setuid copy. A setuid-root binary must never sit in a
/// user-writable dir: same-uid code could swap the file between `cp` and `chmod`,
/// or plant a symlink for root's `cp` to follow. Same location the firewall
/// helper uses.
#[cfg(target_os = "macos")]
const PRIV_CORE_DIR: &str = "/Library/PrivilegedHelperTools";

/// Privileged Core path for the macOS setuid copy.
pub fn privileged_core_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from(PRIV_CORE_DIR).join("app.nexus.NexusCore")
    }
}

fn core_bin_name() -> &'static str {
    "NexusCore"
}

#[cfg(target_os = "macos")]
fn legacy_core_path() -> PathBuf {
    crate::paths::data_dir().join("bin").join(core_bin_name())
}

#[cfg(target_os = "macos")]
pub fn path_has_setuid(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    meta.permissions().mode() & 0o4000 != 0
}

#[cfg(target_os = "macos")]
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn setuid_install_shell(src: &Path, expected_sha256: &str) -> String {
    let dest = privileged_core_path();
    let q_src = shell_single_quote(&src.to_string_lossy());
    let q_dest = shell_single_quote(&dest.to_string_lossy());
    let q_dir = shell_single_quote(PRIV_CORE_DIR);
    format!(
        "/bin/mkdir -p {q_dir} && /usr/sbin/chown root:wheel {q_dir} && /bin/chmod 755 {q_dir} && \
         /bin/cp -f {q_src} {q_dest}.new && \
         actual=$(/usr/bin/shasum -a 256 {q_dest}.new | /usr/bin/cut -d ' ' -f 1) && \
         {{ /bin/test \"$actual\" = {expected_sha256} || {{ /bin/rm -f {q_dest}.new; exit 1; }}; }} && \
         /usr/sbin/chown root:wheel {q_dest}.new && \
         /bin/chmod 4755 {q_dest}.new && /bin/mv -f {q_dest}.new {q_dest}"
    )
}

/// macOS: copy → root-owned helper directory + chown root + chmod u+s via osascript.
pub fn ensure_setuid_core(src: &Path) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        if !src.is_file() {
            return Err(format!("NexusCore source missing: {}", src.display()));
        }
        let legacy = legacy_core_path();
        match fs::remove_file(&legacy) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "remove legacy setuid Core {}: {e}",
                    legacy.display()
                ))
            }
        }
        let dest = privileged_core_path();
        if dest.is_file() && path_has_setuid(&dest) {
            if let (Ok(s), Ok(d)) = (fs::metadata(src), fs::metadata(&dest)) {
                let src_newer = match (s.modified(), d.modified()) {
                    (Ok(sm), Ok(dm)) => sm > dm,
                    _ => true,
                };
                if s.len() == d.len() && !src_newer {
                    return Ok(dest);
                }
            }
        }
        // chmod 4755 before moving into place: the setuid bit must never exist on a
        // path an attacker can still swap. Staged under the root-owned dir (not /tmp)
        // so the rename is atomic and the staging path is never user-writable.
        // Pin the exact source bytes before the authentication dialog. Root reopens
        // the source pathname only to copy it into the protected directory, then
        // verifies that copy against this digest before granting setuid.
        let expected_sha256 = sha256_file(src)?;
        let shell = setuid_install_shell(src, &expected_sha256);
        let mut script_shell = shell.replace('\\', "\\\\");
        script_shell = script_shell.replace('\"', "\\\"");
        let script = format!("do shell script \"{script_shell}\" with administrator privileges");

        let out = Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("osascript start: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let err2 = String::from_utf8_lossy(&out.stdout);
            let msg = format!("{err}{err2}").trim().to_string();
            if msg.is_empty() {
                return Err("administrator authentication failed or was cancelled".into());
            }
            return Err(format!("setuid failed: {msg}"));
        }
        std::thread::sleep(Duration::from_millis(50));
        if !dest.is_file() {
            return Err(format!(
                "setuid copy missing after elevate: {}",
                dest.display()
            ));
        }
        if !path_has_setuid(&dest) {
            return Err(format!(
                "NexusCore not setuid after elevate (volume may be nosuid): {}",
                dest.display()
            ));
        }
        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn shell_quote_spaces_and_apos() {
        assert_eq!(shell_single_quote("a b"), "'a b'");
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
    }

    /// The setuid target must live in a root-owned dir. Under the user's home it
    /// would be swappable between `cp` and `chmod 4755` — local root for free.
    #[test]
    #[cfg(target_os = "macos")]
    fn privileged_path_is_root_owned_dir() {
        let p = privileged_core_path();
        let s = p.to_string_lossy();
        assert!(s.starts_with("/Library/PrivilegedHelperTools/"), "{s}");
        let home = std::env::var("HOME").unwrap_or_default();
        assert!(!home.is_empty() && !s.starts_with(&home), "{s}");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn setuid_install_pins_source_digest_before_granting_setuid() {
        let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let shell = setuid_install_shell(Path::new("/tmp/NexusCore"), digest);
        let verify_at = shell
            .find("/usr/bin/shasum -a 256")
            .expect("root-owned staging copy must be hashed");
        let chmod_at = shell
            .find("/bin/chmod 4755")
            .expect("setuid chmod must remain explicit");
        assert!(
            shell.contains(digest),
            "expected digest must be pinned: {shell}"
        );
        assert!(
            shell.contains("/bin/test") && !shell.contains("/usr/bin/test"),
            "digest compare must use /bin/test (no /usr/bin/test on current macOS): {shell}"
        );
        assert!(
            verify_at < chmod_at,
            "hash must be verified before setuid: {shell}"
        );
        assert!(
            !shell.contains(&crate::paths::data_dir().to_string_lossy().into_owned()),
            "elevated shell must not touch user-data paths: {shell}"
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
