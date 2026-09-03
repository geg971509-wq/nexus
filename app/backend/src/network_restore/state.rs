use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    #[serde(default)]
    pub(super) auto_proxy_url: String,
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

/// Add only services that Nexus has never owned in this transaction. Existing
/// entries are the original pre-Nexus values and must never be overwritten by a
/// later capture after Nexus has already changed that service.
pub(super) fn merge_proxy_snapshot(
    stored: &mut Vec<ProxyServiceState>,
    current: &[ProxyServiceState],
) -> bool {
    let mut known: HashSet<String> = stored.iter().map(|item| item.service.clone()).collect();
    let mut changed = false;
    for item in current {
        if known.insert(item.service.clone()) {
            stored.push(item.clone());
            changed = true;
        }
    }
    changed
}

pub(super) fn merge_dns_snapshot(
    stored: &mut Vec<DnsServiceState>,
    current: &[DnsServiceState],
) -> bool {
    let mut known: HashSet<String> = stored.iter().map(|item| item.service.clone()).collect();
    let mut changed = false;
    for item in current {
        if known.insert(item.service.clone()) {
            stored.push(item.clone());
            changed = true;
        }
    }
    changed
}

pub(super) fn retain_existing_proxy_services(
    snapshot: &mut Vec<ProxyServiceState>,
    existing: &HashSet<String>,
) -> bool {
    let before = snapshot.len();
    snapshot.retain(|item| existing.contains(&item.service));
    snapshot.len() != before
}

pub(super) fn retain_existing_dns_services(
    snapshot: &mut Vec<DnsServiceState>,
    existing: &HashSet<String>,
) -> bool {
    let before = snapshot.len();
    snapshot.retain(|item| existing.contains(&item.service));
    snapshot.len() != before
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

fn sync_parent(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = fs::File::open(parent).map_err(|e| format!("open recovery directory: {e}"))?;
    directory
        .sync_all()
        .map_err(|e| format!("sync recovery directory: {e}"))
}

pub(super) fn save_state(path: &Path, state: &RecoveryState) -> Result<(), String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    if state.proxy.is_none() && state.dns.is_none() {
        match fs::remove_file(path) {
            Ok(()) => {
                sync_parent(path)?;
                return Ok(());
            }
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
    sync_parent(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn manual(enabled: bool, server: &str, port: u16) -> ManualProxyState {
        ManualProxyState {
            enabled,
            server: server.into(),
            port,
            authenticated: false,
        }
    }

    fn proxy(service: &str, server: &str) -> ProxyServiceState {
        ProxyServiceState {
            service: service.into(),
            web: manual(!server.is_empty(), server, if server.is_empty() { 0 } else { 8080 }),
            secure_web: manual(false, "", 0),
            socks: manual(false, "", 0),
            auto_proxy_url: String::new(),
            auto_proxy_enabled: false,
            auto_discovery_enabled: false,
        }
    }

    fn sample_state() -> RecoveryState {
        RecoveryState {
            version: RECOVERY_VERSION,
            proxy: Some(vec![ProxyServiceState {
                service: "Wi-Fi".into(),
                web: manual(false, "", 0),
                secure_web: manual(true, "secure.example", 8443),
                socks: manual(false, "", 0),
                auto_proxy_url: "https://pac.example/proxy.pac".into(),
                auto_proxy_enabled: true,
                auto_discovery_enabled: false,
            }]),
            dns: Some(vec![DnsServiceState {
                service: "Wi-Fi".into(),
                servers: Some(vec!["9.9.9.9".into(), "2620:fe::fe".into()]),
            }]),
        }
    }

    #[test]
    fn recovery_state_round_trips_without_network_calls() {
        let state = sample_state();
        let body = serde_json::to_string(&state).unwrap();
        let decoded: RecoveryState = serde_json::from_str(&body).unwrap();
        let proxy = decoded.proxy.unwrap();
        assert_eq!(proxy[0].service, "Wi-Fi");
        assert_eq!(proxy[0].auto_proxy_url, "https://pac.example/proxy.pac");
        assert_eq!(decoded.dns.unwrap()[0].servers.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn merge_keeps_first_seen_original_and_adds_new_service() {
        let mut stored = vec![proxy("Wi-Fi", "original.example")];
        let current = vec![
            proxy("Wi-Fi", "nexus-overwrite.example"),
            proxy("USB 10/100/1000 LAN", "user-usb.example"),
        ];
        assert!(merge_proxy_snapshot(&mut stored, &current));
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].web.server, "original.example");
        assert_eq!(stored[1].web.server, "user-usb.example");
    }

    #[test]
    fn restore_filter_drops_only_services_that_no_longer_exist() {
        let mut stored = vec![proxy("Wi-Fi", "one"), proxy("Old USB", "two")];
        let existing = HashSet::from(["Wi-Fi".to_string()]);
        assert!(retain_existing_proxy_services(&mut stored, &existing));
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].service, "Wi-Fi");
    }

    #[test]
    fn recovery_file_is_private_and_removed_when_empty() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "nexus-network-recovery-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("network-recovery.json");

        let mut state = sample_state();
        save_state(&path, &state).unwrap();
        assert!(path.is_file());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let loaded = load_state(&path).unwrap();
        assert!(loaded.proxy.is_some() && loaded.dns.is_some());

        state.proxy = None;
        state.dns = None;
        save_state(&path, &state).unwrap();
        assert!(
            !path.exists(),
            "empty recovery transaction must remove its file"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
