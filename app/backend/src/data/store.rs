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

/// Real empty catalog for first launch. The UI must never invent placeholder
/// groups that are not present in this source of truth.
pub fn default_catalog() -> serde_json::Value {
    serde_json::json!({
        "v": 1,
        "active": "default",
        "groups": [{ "id": "default", "name": "Default", "url": "", "count": 0 }],
        "profiles": {
            "default": { "label": "Default", "nodes": [] }
        }
    })
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
            match serde_json::from_str(&s) {
                Ok(st) => return st,
                // Falling back to Store::default() here is not enough: the very next
                // Store::update touches one field and saves the whole struct, which
                // writes catalog=None over every node the user had. Move the bad file
                // aside first so the data is recoverable by hand.
                Err(_) => quarantine_unreadable(p),
            }
        }
    }
    Store::default()
}

/// Rename an unparseable store aside. Best effort: a failure here only means we
/// fall through to defaults exactly as before.
fn quarantine_unreadable(p: &std::path::Path) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bad = p.with_extension(format!("json.corrupt-{stamp}"));
    let _ = fs::rename(p, &bad);
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
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o600));
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

fn apply_exclusive_lock(file: &fs::File) -> Result<(), std::io::Error> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    let rc = unsafe { libc::flock(fd, libc::LOCK_EX) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A corrupt store must be moved aside, not silently replaced in place: the
    /// next Store::update writes the whole struct back, so defaults-in-place
    /// destroys the user's entire catalog. A valid store must be left alone.
    #[test]
    fn corrupt_store_is_quarantined_valid_one_is_not() {
        let dir = std::env::temp_dir().join(format!("nexus-store-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("store.json");

        fs::write(&p, b"{ not json").unwrap();
        let st = load_unlocked(&p);
        assert!(st.catalog.is_none(), "corrupt store falls back to defaults");
        assert!(!p.exists(), "corrupt store must be renamed away");
        let saved: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt-"))
            .collect();
        assert_eq!(saved.len(), 1, "exactly one quarantined copy");
        assert_eq!(fs::read(saved[0].path()).unwrap(), b"{ not json");

        let mut good = Store::default();
        good.catalog = Some(serde_json::json!({"groups": []}));
        save_unlocked(&p, &good).unwrap();
        assert!(load_unlocked(&p).catalog.is_some(), "valid store round-trips");
        assert!(p.exists(), "valid store must not be quarantined");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_launch_catalog_is_one_real_empty_default_group() {
        let c = default_catalog();
        assert_eq!(c["v"], 1);
        assert_eq!(c["active"], "default");
        assert_eq!(c["groups"].as_array().map(Vec::len), Some(1));
        assert_eq!(c["groups"][0]["id"], "default");
        assert_eq!(c["profiles"]["default"]["nodes"].as_array().map(Vec::len), Some(0));
    }
}

// Unlock on drop: flock unlock and file close release the lock.
impl Drop for StoreLock {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        let fd = self._file.as_raw_fd();
        unsafe {
            let _ = libc::flock(fd, libc::LOCK_UN);
        }
    }
}
