use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) const RECOVERY_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ManualProxyState {
    pub(super) enabled: bool,
    pub(super) server: String,
    pub(super) port: u16,
    pub(super) authenticated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ProxyServiceState {
    pub(super) service: String,
    pub(super) web: ManualProxyState,
    pub(super) secure_web: ManualProxyState,
    pub(super) socks: ManualProxyState,
    pub(super) auto_proxy_enabled: bool,
    pub(super) auto_discovery_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct DnsServiceState {
    pub(super) service: String,
    /// None means the service inherited DNS (networksetup reports no explicit
    /// DNS servers). Some(vec) must be restored byte-for-byte as argv values.
    pub(super) servers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RecoveryState {
    pub(super) version: u8,
    #[serde(default)]
    pub(super) proxy: Option<Vec<ProxyServiceState>>,
    #[serde(default)]
    pub(super) dns: Option<Vec<DnsServiceState>>,
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            version: RECOVERY_VERSION,
            proxy: None,
            dns: None,
        }
    }
}

pub(super) fn recovery_path() -> PathBuf {
    crate::paths::ensure_data_dir().join("network-recovery.json")
}

pub(super) fn load_state(path: &Path) -> Result<RecoveryState, String> {
    if !path.exists() {
        return Ok(RecoveryState::default());
    }
    let body = fs::read_to_string(path).map_err(|e| format!("read network recovery: {e}"))?;
    let state: RecoveryState =
        serde_json::from_str(&body).map_err(|e| format!("parse network recovery: {e}"))?;
    if state.version != RECOVERY_VERSION {
        return Err(format!(
            "unsupported network recovery version {}",
            state.version
        ));
    }
    Ok(state)
}

pub(super) fn save_state(path: &Path, state: &RecoveryState) -> Result<(), String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    if state.proxy.is_none() && state.dns.is_none() {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("remove network recovery: {e}")),
        }
    }
    let body = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("open network recovery temp: {e}"))?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod network recovery temp: {e}"))?;
    file.write_all(body.as_bytes())
        .map_err(|e| format!("write network recovery: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("sync network recovery: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("install network recovery: {e}"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod network recovery: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_state_round_trips_without_network_calls() {
        let state = RecoveryState {
            version: RECOVERY_VERSION,
            proxy: Some(vec![ProxyServiceState {
                service: "Wi-Fi".into(),
                web: ManualProxyState {
                    enabled: false,
                    server: String::new(),
                    port: 0,
                    authenticated: false,
                },
                secure_web: ManualProxyState {
                    enabled: true,
                    server: "secure.example".into(),
                    port: 8443,
                    authenticated: false,
                },
                socks: ManualProxyState {
                    enabled: false,
                    server: String::new(),
                    port: 0,
                    authenticated: false,
                },
                auto_proxy_enabled: true,
                auto_discovery_enabled: false,
            }]),
            dns: Some(vec![DnsServiceState {
                service: "Wi-Fi".into(),
                servers: Some(vec!["9.9.9.9".into(), "2620:fe::fe".into()]),
            }]),
        };
        let body = serde_json::to_string(&state).unwrap();
        let decoded: RecoveryState = serde_json::from_str(&body).unwrap();
        assert_eq!(decoded.proxy.unwrap()[0].service, "Wi-Fi");
        assert_eq!(decoded.dns.unwrap()[0].servers.as_ref().unwrap().len(), 2);
    }
}
