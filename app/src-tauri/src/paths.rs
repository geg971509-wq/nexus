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
    #[cfg(target_os = "windows")]
    {
        if let Some(ad) = std::env::var_os("APPDATA") {
            return PathBuf::from(ad).join("Nexus");
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(home).join("AppData/Roaming/Nexus");
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
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
    #[cfg(target_os = "windows")]
    {
        return data_dir().join("logs");
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".local/share/Nexus/logs");
        }
        return std::env::temp_dir().join("Nexus-logs");
    }
}

pub fn ensure_data_dir() -> PathBuf {
    let d = data_dir();
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn ensure_log_dir() -> PathBuf {
    let d = log_dir();
    let _ = std::fs::create_dir_all(&d);
    d
}
