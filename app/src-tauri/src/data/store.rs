//! Minimal app store (JSON file). Catalog blob is the node source of truth.
use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::path::PathBuf;

/// One reject entry: host for all processes, or host+process_path for that app only.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BlockEntry {
    pub host: String,
    /// Full executable path when scoping to one process; omit = any process.
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
