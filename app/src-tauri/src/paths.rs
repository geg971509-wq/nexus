//! Per-OS data / log directories for Nexus shell + Core cwd/cache.

use std::path::PathBuf;

/// User data root: store.json, cache.db, privileged Core (macOS).
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join("Library/Application Support/Nexus");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".local/share/Nexus");
        }
    }
    std::env::temp_dir().join("Nexus")
}

/// Core process log file directory.
pub fn log_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join("Library/Logs/Nexus");
        }
        return std::env::temp_dir().join("Nexus-logs");
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".local/share/Nexus/logs");
        }
        return std::env::temp_dir().join("Nexus-logs");
    }
}

/// Restrict a directory we own to its user.
///
/// create_dir_all leaves 0755, and what lands inside is a record of where the
/// user's traffic went: Core's log names every outbound destination, and
/// sing-box's cache.db holds resolved domains. store.json already saved itself
/// 0600 while its neighbours stayed world-readable — doing it on the directory
/// covers those and anything added later, instead of chasing each file.
///
/// No-op off Unix.
fn restrict_to_owner(dir: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

pub fn ensure_data_dir() -> PathBuf {
    let d = data_dir();
    let _ = std::fs::create_dir_all(&d);
    restrict_to_owner(&d);
    d
}

pub fn ensure_log_dir() -> PathBuf {
    let d = log_dir();
    let _ = std::fs::create_dir_all(&d);
    restrict_to_owner(&d);
    d
}
