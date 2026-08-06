//! Minimal app store (JSON file). Catalog blob is the node source of truth.
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    /// User blocklist: hostnames and IPs (no ports). Rejected at generate time.
    #[serde(default)]
    pub blocklist: Vec<String>,
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
        if let Ok(s) = fs::read_to_string(&p) {
            // Unknown legacy fields (profiles/groups/…) are ignored by serde.
            if let Ok(st) = serde_json::from_str(&s) {
                return st;
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path();
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&p, s).map_err(|e| e.to_string())?;
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
