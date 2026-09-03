//! Transactional preservation of macOS Proxy/PAC/DNS state.
//!
//! Nexus changes system network settings only while it owns a tunnel-side effect.
//! Before the first write in a category, capture the user's current settings and
//! persist them privately. Successful restore removes that category from the
//! recovery file; an abnormal exit leaves enough state for the next launch to
//! repair the machine before new tunnel activity starts.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::sys;

const NETWORKSETUP: &str = "/usr/sbin/networksetup";
const NS_TIMEOUT: Duration = Duration::from_secs(5);
const RECOVERY_VERSION: u8 = 1;
static RECOVERY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ManualProxyState {
    enabled: bool,
    server: String,
    port: u16,
    authenticated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProxyServiceState {
    service: String,
    web: ManualProxyState,
    secure_web: ManualProxyState,
    socks: ManualProxyState,
    auto_proxy_enabled: bool,
    auto_discovery_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DnsServiceState {
    service: String,
    /// None means the service inherited DNS (networksetup reports no explicit
    /// DNS servers). Some(vec) must be restored byte-for-byte as argv values.
    servers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveryState {
    version: u8,
    #[serde(default)]
    proxy: Option<Vec<ProxyServiceState>>,
    #[serde(default)]
    dns: Option<Vec<DnsServiceState>>,
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

fn recovery_path() -> PathBuf {
    crate::paths::ensure_data_dir().join("network-recovery.json")
}

fn run_ns_capture(args: &[&str]) -> Result<String, String> {
    let mut child = Command::new(NETWORKSETUP)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("networksetup start: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = Vec::new();
                let mut err = Vec::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_end(&mut out);
                }
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_end(&mut err);
                }
                let out = String::from_utf8_lossy(&out).trim().to_string();
                let err = String::from_utf8_lossy(&err).trim().to_string();
                if status.success() {
                    return Ok(out);
                }
                return Err(format!(
                    "networksetup {:?} exit={status}{}",
                    args,
                    if err.is_empty() {
                        String::new()
                    } else {
                        format!(" err={err}")
                    }
                ));
            }
            Ok(None) => {
                if started.elapsed() > NS_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("networksetup timed out: {args:?}"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(format!("networksetup wait: {e}")),
        }
    }
}

fn run_ns(args: &[&str]) -> Result<(), String> {
    run_ns_capture(args).map(|_| ())
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        if left.trim().eq_ignore_ascii_case(key) {
            Some(right.trim())
        } else {
            None
        }
    })
}

fn parse_switch(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "yes" | "on" | "1" | "true" => Ok(true),
        "no" | "off" | "0" | "false" => Ok(false),
        other => Err(format!("unexpected switch value: {other}")),
    }
}

fn parse_manual_proxy(text: &str) -> Result<ManualProxyState, String> {
    let enabled = parse_switch(
        field(text, "Enabled").ok_or_else(|| "proxy output missing Enabled".to_string())?,
    )?;
    let server = field(text, "Server").unwrap_or_default().to_string();
    let port = field(text, "Port")
        .ok_or_else(|| "proxy output missing Port".to_string())?
        .parse::<u16>()
        .map_err(|e| format!("invalid proxy port: {e}"))?;
    let authenticated = parse_switch(
        field(text, "Authenticated Proxy Enabled")
            .ok_or_else(|| "proxy output missing authentication state".to_string())?,
    )?;
    Ok(ManualProxyState {
        enabled,
        server,
        port,
        authenticated,
    })
}

fn capture_proxy_service(service: &str) -> Result<ProxyServiceState, String> {
    let web = parse_manual_proxy(&run_ns_capture(&["-getwebproxy", service])?)?;
    let secure_web = parse_manual_proxy(&run_ns_capture(&["-getsecurewebproxy", service])?)?;
    let socks = parse_manual_proxy(&run_ns_capture(&["-getsocksfirewallproxy", service])?)?;
    if web.authenticated || secure_web.authenticated || socks.authenticated {
        return Err(format!(
            "cannot safely replace authenticated proxy settings on `{service}` without access to the user's credentials"
        ));
    }
    let auto_proxy_enabled = parse_switch(
        field(
            &run_ns_capture(&["-getautoproxyurl", service])?,
            "Enabled",
        )
        .ok_or_else(|| format!("auto proxy output missing Enabled for `{service}`"))?,
    )?;
    let discovery = run_ns_capture(&["-getproxyautodiscovery", service])?;
    let auto_discovery_enabled = parse_switch(
        field(&discovery, "Auto Proxy Discovery")
            .or_else(|| field(&discovery, "Enabled"))
            .ok_or_else(|| format!("proxy autodiscovery output missing state for `{service}`"))?,
    )?;
    Ok(ProxyServiceState {
        service: service.to_string(),
        web,
        secure_web,
        socks,
        auto_proxy_enabled,
        auto_discovery_enabled,
    })
}

fn capture_proxy() -> Result<Vec<ProxyServiceState>, String> {
    sys::hot_services(true)
        .into_iter()
        .map(|service| capture_proxy_service(&service))
        .collect()
}

fn capture_dns_service(service: &str) -> Result<DnsServiceState, String> {
    let out = run_ns_capture(&["-getdnsservers", service])?;
    let servers = if out
        .to_ascii_lowercase()
        .contains("there aren't any dns servers set")
    {
        None
    } else {
        let values: Vec<String> = out
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        if values.is_empty() {
            return Err(format!("empty DNS query response for `{service}`"));
        }
        Some(values)
    };
    Ok(DnsServiceState {
        service: service.to_string(),
        servers,
    })
}

fn capture_dns() -> Result<Vec<DnsServiceState>, String> {
    sys::hot_services(true)
        .into_iter()
        .map(|service| capture_dns_service(&service))
        .collect()
}

fn restore_manual_proxy(service: &str, kind: &str, state: &ManualProxyState) -> Result<(), String> {
    let port = state.port.to_string();
    let (set_cmd, state_cmd) = match kind {
        "web" => ("-setwebproxy", "-setwebproxystate"),
        "secure" => ("-setsecurewebproxy", "-setsecurewebproxystate"),
        "socks" => ("-setsocksfirewallproxy", "-setsocksfirewallproxystate"),
        _ => return Err(format!("unknown proxy kind: {kind}")),
    };
    run_ns(&[set_cmd, service, state.server.as_str(), port.as_str()])?;
    run_ns(&[
        state_cmd,
        service,
        if state.enabled { "on" } else { "off" },
    ])
}

fn restore_proxy_snapshot(snapshot: &[ProxyServiceState]) -> Result<String, String> {
    let mut failures = Vec::new();
    for state in snapshot {
        let r = (|| -> Result<(), String> {
            restore_manual_proxy(&state.service, "web", &state.web)?;
            restore_manual_proxy(&state.service, "secure", &state.secure_web)?;
            restore_manual_proxy(&state.service, "socks", &state.socks)?;
            run_ns(&[
                "-setautoproxystate",
                state.service.as_str(),
                if state.auto_proxy_enabled { "on" } else { "off" },
            ])?;
            run_ns(&[
                "-setproxyautodiscovery",
                state.service.as_str(),
                if state.auto_discovery_enabled {
                    "on"
                } else {
                    "off"
                },
            ])?;
            Ok(())
        })();
        if let Err(e) = r {
            failures.push(format!("`{}`: {e}", state.service));
        }
    }
    if failures.is_empty() {
        Ok(format!("restored system proxy/PAC · {} service(s)", snapshot.len()))
    } else {
        Err(format!("restore system proxy/PAC failed: {}", failures.join(" · ")))
    }
}

fn restore_dns_snapshot(snapshot: &[DnsServiceState]) -> Result<String, String> {
    let mut failures = Vec::new();
    for state in snapshot {
        let result = match &state.servers {
            Some(servers) => {
                let mut args = vec!["-setdnsservers", state.service.as_str()];
                args.extend(servers.iter().map(String::as_str));
                run_ns(&args)
            }
            None => run_ns(&["-setdnsservers", state.service.as_str(), "Empty"]),
        };
        if let Err(e) = result {
            failures.push(format!("`{}`: {e}", state.service));
        }
    }
    if failures.is_empty() {
        Ok(format!("restored system DNS · {} service(s)", snapshot.len()))
    } else {
        Err(format!("restore system DNS failed: {}", failures.join(" · ")))
    }
}

fn disable_automatic_proxy(snapshot: &[ProxyServiceState]) -> Result<(), String> {
    let mut failures = Vec::new();
    for state in snapshot {
        for args in [
            ["-setautoproxystate", state.service.as_str(), "off"],
            ["-setproxyautodiscovery", state.service.as_str(), "off"],
        ] {
            if let Err(e) = run_ns(&args) {
                failures.push(format!("`{}`: {e}", state.service));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("disable automatic proxy failed: {}", failures.join(" · ")))
    }
}

fn load_state(path: &Path) -> Result<RecoveryState, String> {
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

fn save_state(path: &Path, state: &RecoveryState) -> Result<(), String> {
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

fn apply_proxy_locked(network: &sys::SystemNetworkChange, port: u16) -> Result<String, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    if state.proxy.is_none() {
        state.proxy = Some(capture_proxy()?);
        save_state(&path, &state)?;
    }
    let snapshot = state.proxy.clone().unwrap_or_default();
    let applied = disable_automatic_proxy(&snapshot)
        .and_then(|_| network.set_system_proxy(true, port));
    match applied {
        Ok(note) => Ok(note),
        Err(e) => {
            match restore_proxy_snapshot(&snapshot) {
                Ok(_) => {
                    state.proxy = None;
                    if let Err(save_err) = save_state(&path, &state) {
                        return Err(format!(
                            "apply system proxy failed: {e}; rollback succeeded but recovery cleanup failed: {save_err}"
                        ));
                    }
                    Err(format!("apply system proxy failed: {e}; original proxy/PAC restored"))
                }
                Err(restore_err) => Err(format!(
                    "apply system proxy failed: {e}; rollback also failed: {restore_err}"
                )),
            }
        }
    }
}

fn apply_dns_locked(
    network: &sys::SystemNetworkChange,
    servers: &[String],
) -> Result<String, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    if state.dns.is_none() {
        state.dns = Some(capture_dns()?);
        save_state(&path, &state)?;
    }
    let snapshot = state.dns.clone().unwrap_or_default();
    match network.set_system_dns_bootstrap(true, servers) {
        Ok(note) => Ok(note),
        Err(e) => match restore_dns_snapshot(&snapshot) {
            Ok(_) => {
                state.dns = None;
                if let Err(save_err) = save_state(&path, &state) {
                    return Err(format!(
                        "apply system DNS failed: {e}; rollback succeeded but recovery cleanup failed: {save_err}"
                    ));
                }
                Err(format!("apply system DNS failed: {e}; original DNS restored"))
            }
            Err(restore_err) => Err(format!(
                "apply system DNS failed: {e}; rollback also failed: {restore_err}"
            )),
        },
    }
}

fn restore_proxy_locked() -> Result<Option<String>, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    let Some(snapshot) = state.proxy.clone() else {
        return Ok(None);
    };
    let note = restore_proxy_snapshot(&snapshot)?;
    state.proxy = None;
    save_state(&path, &state)?;
    Ok(Some(note))
}

fn restore_dns_locked() -> Result<Option<String>, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    let Some(snapshot) = state.dns.clone() else {
        return Ok(None);
    };
    let note = restore_dns_snapshot(&snapshot)?;
    state.dns = None;
    save_state(&path, &state)?;
    Ok(Some(note))
}

fn restore_all_locked() -> Result<Vec<String>, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    let mut notes = Vec::new();
    let mut failures = Vec::new();

    if let Some(snapshot) = state.proxy.clone() {
        match restore_proxy_snapshot(&snapshot) {
            Ok(note) => {
                state.proxy = None;
                notes.push(note);
            }
            Err(e) => failures.push(e),
        }
    }
    if let Some(snapshot) = state.dns.clone() {
        match restore_dns_snapshot(&snapshot) {
            Ok(note) => {
                state.dns = None;
                notes.push(note);
            }
            Err(e) => failures.push(e),
        }
    }
    save_state(&path, &state)?;
    if failures.is_empty() {
        Ok(notes)
    } else {
        Err(failures.join(" · "))
    }
}

pub(crate) fn reconcile_if(
    is_current: impl FnOnce() -> bool,
    use_system_proxy: bool,
    use_tun: bool,
    port: u16,
    dns_servers: &[String],
) -> Option<Result<Vec<String>, String>> {
    sys::with_system_network_change_if(is_current, |network| {
        let mut notes = Vec::new();
        if use_system_proxy {
            notes.push(apply_proxy_locked(network, port)?);
        } else if let Some(note) = restore_proxy_locked()? {
            notes.push(note);
        }
        if use_tun {
            notes.push(apply_dns_locked(network, dns_servers)?);
        } else if let Some(note) = restore_dns_locked()? {
            notes.push(note);
        }
        Ok(notes)
    })
}

pub(crate) fn apply_proxy(port: u16) -> Result<String, String> {
    sys::with_system_network_change(|| apply_proxy_locked(&sys::SystemNetworkChange))
}

pub(crate) fn restore_proxy() -> Result<Option<String>, String> {
    sys::with_system_network_change(restore_proxy_locked)
}

pub(crate) fn restore_all_if(
    is_current: impl FnOnce() -> bool,
) -> Option<Result<Vec<String>, String>> {
    sys::with_system_network_change_if(is_current, |_| restore_all_locked())
}

pub(crate) fn restore_all() -> Result<Vec<String>, String> {
    sys::with_system_network_change(restore_all_locked)
}

pub(crate) fn has_pending() -> bool {
    recovery_path().is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_parser_reads_disabled_empty_state() {
        let p = parse_manual_proxy(
            "Enabled: No\nServer: \nPort: 0\nAuthenticated Proxy Enabled: 0\n",
        )
        .unwrap();
        assert!(!p.enabled);
        assert_eq!(p.server, "");
        assert_eq!(p.port, 0);
        assert!(!p.authenticated);
    }

    #[test]
    fn proxy_parser_reads_enabled_state() {
        let p = parse_manual_proxy(
            "Enabled: Yes\nServer: proxy.example\nPort: 8080\nAuthenticated Proxy Enabled: 0\n",
        )
        .unwrap();
        assert!(p.enabled);
        assert_eq!(p.server, "proxy.example");
        assert_eq!(p.port, 8080);
    }

    #[test]
    fn switches_accept_networksetup_spellings() {
        for value in ["Yes", "On", "1", "true"] {
            assert!(parse_switch(value).unwrap(), "{value}");
        }
        for value in ["No", "Off", "0", "false"] {
            assert!(!parse_switch(value).unwrap(), "{value}");
        }
    }

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
