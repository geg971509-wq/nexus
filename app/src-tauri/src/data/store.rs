//! Minimal app store (JSON file). Catalog blob is the node source of truth.
//! Save uses exclusive advisory lock + atomic replace so concurrent writers cannot interleave.
use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

/// One reject entry:
/// - host only → any process hitting that host
/// - host + process_path → that process hitting that host
/// - process_path only (host empty) → that process, all destinations
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BlockEntry {
    #[serde(default)]
    pub host: String,
    /// Full executable path; omit = any process (host required then).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_path: Option<String>,
}

impl<'de> Deserialize<'de> for BlockEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Host(String),
            Obj {
                #[serde(default)]
                host: String,
                #[serde(default)]
                process_path: Option<String>,
            },
        }
        match Raw::deserialize(deserializer)? {
            Raw::Host(host) => Ok(BlockEntry {
                host,
                process_path: None,
            }),
            Raw::Obj {
                host,
                process_path,
            } => Ok(BlockEntry {
                host,
                process_path: process_path.filter(|p| !p.trim().is_empty()),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    /// System proxy intent — product default ON.
    #[serde(default = "default_true")]
    pub system_proxy: bool,
    /// Tun intent — default off (needs privilege).
    #[serde(default)]
    pub tun: bool,
    /// UI node catalog blob (`nexus.catalog.v1` shape).
    #[serde(default)]
    pub catalog: Option<serde_json::Value>,
    /// User blocklist: host (any process) and optional process_path scope.
    #[serde(default)]
    pub blocklist: Vec<BlockEntry>,
}

fn default_true() -> bool {
    true
}

impl Default for Store {
    fn default() -> Self {
        Self {
            system_proxy: true,
            tun: false,
            catalog: None,
            blocklist: Vec::new(),
        }
    }
}

impl Store {
    pub fn path() -> PathBuf {
        let base = dirs_next_path();
        let _ = fs::create_dir_all(&base);
        base.join("store.json")
    }

    pub fn load() -> Self {
        let p = Self::path();
        let _guard = lock_store_file(&p);
        if let Ok(mut f) = fs::File::open(&p) {
            let mut s = String::new();
            if f.read_to_string(&mut s).is_ok() {
                if let Ok(st) = serde_json::from_str(&s) {
                    return st;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path();
        let _guard = lock_store_file(&p).map_err(|e| e.to_string())?;
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = p.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
            f.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

fn dirs_next_path() -> PathBuf {
    crate::paths::ensure_data_dir()
}

/// Holds exclusive advisory lock for the duration of load/save.
struct StoreLock {
    _file: fs::File,
}

fn lock_store_file(store_path: &std::path::Path) -> Result<StoreLock, std::io::Error> {
    let lock_path = store_path.with_extension("json.lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    apply_exclusive_lock(&file)?;
    Ok(StoreLock { _file: file })
}

#[cfg(unix)]
fn apply_exclusive_lock(file: &fs::File) -> Result<(), std::io::Error> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    let rc = unsafe { libc::flock(fd, libc::LOCK_EX) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn apply_exclusive_lock(file: &fs::File) -> Result<(), std::io::Error> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{LockFileEx, LOCKFILE_EXCLUSIVE_LOCK};
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let handle = file.as_raw_handle() as HANDLE;
    let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
    // Lock one byte of the lock file (whole-file exclusive via exclusive flag).
    let ok = unsafe {
        LockFileEx(
            handle,
            LOCKFILE_EXCLUSIVE_LOCK,
            0,
            1,
            0,
            &mut ov,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn apply_exclusive_lock(_file: &fs::File) -> Result<(), std::io::Error> {
    Ok(())
}

// Unlock on drop: flock unlock / handle close releases LockFileEx.
#[cfg(unix)]
impl Drop for StoreLock {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        let fd = self._file.as_raw_fd();
        unsafe {
            let _ = libc::flock(fd, libc::LOCK_UN);
        }
    }
}
