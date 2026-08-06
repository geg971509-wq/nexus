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

/// Privileged Core path (macOS setuid copy). On Windows, unused — returns data_dir bin path.
pub fn privileged_core_path() -> PathBuf {
    crate::paths::data_dir().join("bin").join(core_bin_name())
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
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }

        let q_src = shell_single_quote(&src.to_string_lossy());
        let q_dest = shell_single_quote(&dest.to_string_lossy());
        let shell = format!(
            "/bin/cp -f {q_src} {q_dest} && /usr/sbin/chown root:wheel {q_dest} && /bin/chmod 4755 {q_dest}"
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

    #[test]
    fn privileged_path_under_data_dir() {
        let p = privileged_core_path();
        let s = p.to_string_lossy();
        assert!(s.contains("Nexus"), "{s}");
        assert!(s.contains("bin"), "{s}");
    }
}
