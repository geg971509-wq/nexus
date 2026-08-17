//! Tun privilege: macOS setuid-root NexusCore; Windows runs Core as-is (elevate app for Tun).

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

/// Privileged Core path (macOS setuid copy). On Windows, unused — returns data_dir bin path.
pub fn privileged_core_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from(PRIV_CORE_DIR).join("app.nexus.NexusCore")
    }
    #[cfg(not(target_os = "macos"))]
    {
        crate::paths::data_dir().join("bin").join(core_bin_name())
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

#[cfg(target_os = "macos")]
pub fn path_has_setuid(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    meta.permissions().mode() & 0o4000 != 0
}

#[cfg(not(target_os = "macos"))]
pub fn path_has_setuid(_path: &Path) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// macOS: copy → Application Support + chown root + chmod u+s via osascript.
/// Windows / other: return `src` (run app elevated for Tun).
pub fn ensure_setuid_core(src: &Path) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        if !src.is_file() {
            return Err(format!("NexusCore source missing: {}", src.display()));
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
        // The dest dir is root-owned, so mkdir happens inside the elevated script.
        let q_src = shell_single_quote(&src.to_string_lossy());
        let q_dest = shell_single_quote(&dest.to_string_lossy());
        let q_dir = shell_single_quote(PRIV_CORE_DIR);
        // Versions before this shipped the setuid copy into the user-writable data
        // dir; that file is still setuid-root and still exploitable, so drop it here
        // while we already hold root.
        let q_legacy = shell_single_quote(
            &crate::paths::data_dir()
                .join("bin")
                .join(core_bin_name())
                .to_string_lossy(),
        );
        // chmod 4755 before moving into place: the setuid bit must never exist on a
        // path an attacker can still swap. Staged under the root-owned dir (not /tmp)
        // so the rename is atomic and the staging path is never user-writable.
        let shell = format!(
            "/bin/mkdir -p {q_dir} && /usr/sbin/chown root:wheel {q_dir} && /bin/chmod 755 {q_dir} && \
             /bin/cp -f {q_src} {q_dest}.new && /usr/sbin/chown root:wheel {q_dest}.new && \
             /bin/chmod 4755 {q_dest}.new && /bin/mv -f {q_dest}.new {q_dest} && \
             /bin/rm -f {q_legacy}"
        );
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
            return Err(format!("setuid copy missing after elevate: {}", dest.display()));
        }
        if !path_has_setuid(&dest) {
            return Err(format!(
                "NexusCore not setuid after elevate (volume may be nosuid): {}",
                dest.display()
            ));
        }
        return Ok(dest);
    }
    #[cfg(not(target_os = "macos"))]
    {
        if !src.is_file() {
            return Err(format!("NexusCore source missing: {}", src.display()));
        }
        // ponytail: Tun on Windows needs an elevated process; no separate setuid copy.
        Ok(src.to_path_buf())
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
    #[cfg(not(target_os = "macos"))]
    fn privileged_path_under_data_dir() {
        let p = privileged_core_path();
        let s = p.to_string_lossy();
        assert!(s.contains("Nexus"), "{s}");
        assert!(s.contains("bin"), "{s}");
    }
}
