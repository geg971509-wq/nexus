//! Minimal profile store (JSON file). SQLite schema can replace without changing command shapes.
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub group_id: String,
    /// Raw outbound object for sing-box (import-first)
    pub outbound: serde_json::Value,
    /// Mux A/B probe: "unknown" | "yes" | "no". Default-on injects only when "yes".
    #[serde(default = "default_mux_unknown")]
    pub mux_capability: String,
    #[serde(default)]
    pub mux_capability_at: i64,
}

fn default_mux_unknown() -> String {
    "unknown".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auto_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    pub groups: Vec<Group>,
    pub profiles: Vec<Profile>,
    pub selected_profile_id: Option<String>,
    /// Throne spmode_system_proxy — product default ON (user: 默认打开系统代理).
    #[serde(default = "default_true")]
    pub system_proxy: bool,
    /// Throne spmode_vpn — default off (needs privilege).
    #[serde(default)]
    pub tun: bool,
    /// UI node catalog blob (`nexus.catalog.v1` shape). Single source after migration from localStorage.
    #[serde(default)]
    pub catalog: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

impl Default for Store {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            profiles: Vec::new(),
            selected_profile_id: None,
            system_proxy: true,
            tun: false,
            catalog: None,
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

    pub fn upsert_direct_demo(&mut self) {
        if self.groups.is_empty() {
            self.groups.push(Group {
                id: "default".into(),
                name: "Default".into(),
                url: String::new(),
                auto_update: false,
            });
        }
        if self.profiles.is_empty() {
            self.profiles.push(Profile {
                id: "direct-1".into(),
                name: "Direct".into(),
                group_id: "default".into(),
                outbound: serde_json::json!({"type":"direct","tag":"proxy"}),
                mux_capability: "unknown".into(),
                mux_capability_at: 0,
            });
            self.selected_profile_id = Some("direct-1".into());
        }
    }
}

fn dirs_next_path() -> PathBuf {
    // ~/Library/Application Support/Nexus on macOS
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/Nexus");
    }
    std::env::temp_dir().join("Nexus")
}
