//! macOS Tun privilege: setuid-root NexusCore (Throne Mac_Set_Core_Permissions subset).
//! Bundle may live on nosuid volumes (`a nosuid volume`); copy into
//! `~/Library/Application Support/Nexus/bin/` (Data volume) before chown/chmod u+s.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Privileged Core lives here so setuid works even when the app is on nosuid media.
pub fn privileged_core_path() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(home)
        .join("Library/Application Support/Nexus/bin/NexusCore")
}

pub fn path_has_setuid(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    meta.permissions().mode() & 0o4000 != 0
}

fn shell_single_quote(s: &str) -> String {
    // 'foo'\''bar' for embedded apostrophes
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Copy `src` → Application Support and `chown root` + `chmod u+s` via osascript.
/// Shows the system admin password sheet. No-op (no prompt) when dest already setuid.
pub fn ensure_setuid_core(src: &Path) -> Result<PathBuf, String> {
    if !src.is_file() {
        return Err(format!("NexusCore source missing: {}", src.display()));
    }
    let dest = privileged_core_path();
    if dest.is_file() && path_has_setuid(&dest) {
        // Skip sheet only when dest still matches this bundle Core.
        // Size alone is not enough (rebuild can keep same length); also compare mtime.
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
    // Single elevated shell: copy + setuid (user cannot overwrite root-owned dest).
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
    // Brief settle; then verify.
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
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_spaces_and_apos() {
        assert_eq!(shell_single_quote("a b"), "'a b'");
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn privileged_path_under_app_support() {
        let p = privileged_core_path();
        let s = p.to_string_lossy();
        assert!(s.contains("Application Support/Nexus/bin/NexusCore"), "{s}");
    }
}
