//! Minimal app store (JSON file). Catalog blob is the node source of truth.
//! Save uses exclusive advisory lock + atomic replace so concurrent writers cannot interleave.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    /// System proxy intent — product default ON.
    #[serde(default = "default_true")]
    pub system_proxy: bool,
    /// Tun intent — default off (needs privilege).
    #[serde(default)]
    pub tun: bool,
    /// Hide menu-bar / tray icon (main window + dock remain).
    #[serde(default)]
    pub hide_tray: bool,
    /// UI node catalog blob (`nexus.catalog.v1` shape).
    #[serde(default)]
    pub catalog: Option<serde_json::Value>,
    /// Bootstrap resolver IPs; empty means DEFAULT_DNS_BOOTSTRAP. Read via
    /// `dns_bootstrap()` — never use this field directly, it is unvalidated
    /// user JSON and the values reach PF rule text.
    #[serde(default)]
    pub dns_bootstrap: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Store {
    /// Validated bootstrap resolvers. Non-IP entries are dropped: these are
    /// interpolated into PF rule text, where a hostname is both a syntax error
    /// and an injection vector. Falls back to the product default when empty.
    pub fn dns_bootstrap(&self) -> Vec<String> {
        crate::defaults::sanitize_dns_bootstrap(&self.dns_bootstrap)
    }
}

impl Default for Store {
    fn default() -> Self {
        Self {
            system_proxy: true,
            tun: false,
            hide_tray: false,
            catalog: None,
            dns_bootstrap: Vec::new(),
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
        load_unlocked(&p)
    }

    #[allow(dead_code)] // kept for callers that already hold a Store value
    pub fn save(&self) -> Result<(), String> {
        let p = Self::path();
        let _guard = lock_store_file(&p).map_err(|e| e.to_string())?;
        save_unlocked(&p, self)
    }

    /// 3A: one exclusive lock for load → mutate → save (no lost field updates).
    pub fn update<F, T>(f: F) -> Result<T, String>
    where
        F: FnOnce(&mut Store) -> Result<T, String>,
    {
        let p = Self::path();
        let _guard = lock_store_file(&p).map_err(|e| e.to_string())?;
        let mut st = load_unlocked(&p);
        let out = f(&mut st)?;
        save_unlocked(&p, &st)?;
        Ok(out)
    }
}

fn load_unlocked(p: &std::path::Path) -> Store {
    if let Ok(mut f) = fs::File::open(p) {
        let mut s = String::new();
        if f.read_to_string(&mut s).is_ok() {
            if let Ok(st) = serde_json::from_str(&s) {
                return st;
            }
        }
    }
    Store::default()
}

fn save_unlocked(p: &std::path::Path, st: &Store) -> Result<(), String> {
    let s = serde_json::to_string_pretty(st).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, p).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o600));
    }
    Ok(())
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
