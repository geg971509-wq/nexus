//! Crash-safe preservation/restoration of macOS network settings Nexus owns.
//!
//! The recovery journal is intentionally separate from the long-lived product
//! Store. Proxy/PAC and DNS are recorded independently so disconnect only restores
//! settings Nexus actually changed. A stale journal survives an abnormal exit and
//! is recovered before a new process may take ownership.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const NETWORKSETUP: &str = "/usr/sbin/networksetup";
const TIMEOUT: Duration = Duration::from_secs(5);
const SNAPSHOT_VERSION: u32 = 2;

// False at process start means any on-disk journal belongs to an abnormal prior
// process. True means the journal belongs to this process's current ownership.
static CURRENT_OWNERSHIP: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProxyState {
    enabled: bool,
    server: String,
    port: u16,
    // networksetup exposes only whether authentication is enabled, not the
    // credentials required to reconstruct it.
    #[serde(default)]
    authenticated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AutoProxyState {
    enabled: bool,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProxyServiceState {
    service: String,
    web: ProxyState,
    secure_web: ProxyState,
    socks: ProxyState,
    auto_proxy: AutoProxyState,
    // Old version-2 journals did not record WPAD. None means “unknown, do not
    // mutate during restore”; new captures always store Some(...).
    #[serde(default)]
    auto_discovery_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DnsServiceState {
    service: String,
    /// None means DHCP / no explicitly configured DNS servers.
    dns: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Snapshot {
    version: u32,
    proxy: Option<Vec<ProxyServiceState>>,
    dns: Option<Vec<DnsServiceState>>,
}

impl Snapshot {
    fn empty() -> Self {
        Self {
            version: SNAPSHOT_VERSION,
            proxy: None,
            dns: None,
        }
    }

    fn is_empty(&self) -> bool {
        self.proxy.is_none() && self.dns.is_none()
    }
}

fn path() -> PathBuf {
    crate::paths::ensure_data_dir().join("network-recovery.json")
}

fn run_capture(args: &[&str]) -> Result<String, String> {
    let mut child = Command::new(NETWORKSETUP)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("networksetup start {args:?}: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = Vec::new();
                let mut err = Vec::new();
                if let Some(stdout) = child.stdout.as_mut() {
                    let _ = stdout.read_to_end(&mut out);
                }
                if let Some(stderr) = child.stderr.as_mut() {
                    let _ = stderr.read_to_end(&mut err);
                }
                if status.success() {
                    return Ok(String::from_utf8_lossy(&out).trim().to_string());
                }
                return Err(format!(
                    "networksetup {args:?} exit={status}: {}",
                    String::from_utf8_lossy(&err).trim()
                ));
            }
            Ok(None) if started.elapsed() <= TIMEOUT => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("networksetup timed out: {args:?}"));
            }
            Err(e) => return Err(format!("networksetup wait {args:?}: {e}")),
        }
    }
}

fn run(args: &[&str]) -> Result<(), String> {
    run_capture(args).map(|_| ())
}

fn value<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.split_once(':').filter(|(k, _)| k.trim() == key))
        .map(|(_, v)| v.trim())
        .unwrap_or("")
}

fn bool_value(text: &str, key: &str) -> bool {
    matches!(value(text, key), "Yes" | "yes" | "On" | "on" | "1")
}

fn parse_proxy(text: &str) -> ProxyState {
    ProxyState {
        enabled: bool_value(text, "Enabled"),
        server: value(text, "Server").to_string(),
        port: value(text, "Port").parse::<u16>().unwrap_or(0),
        authenticated: bool_value(text, "Authenticated Proxy Enabled"),
    }
}

fn parse_auto_proxy(text: &str) -> AutoProxyState {
    let raw = value(text, "URL");
    AutoProxyState {
        enabled: bool_value(text, "Enabled"),
        url: if raw.eq_ignore_ascii_case("(null)") {
            String::new()
        } else {
            raw.to_string()
        },
    }
}

fn parse_auto_discovery(text: &str) -> bool {
    bool_value(text, "Auto Proxy Discovery") || bool_value(text, "Enabled")
}

fn parse_dns(text: &str) -> Option<Vec<String>> {
    if text.is_empty() || text.starts_with("There aren't any DNS Servers set on") {
        return None;
    }
    let servers: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    (!servers.is_empty()).then_some(servers)
}

fn capture_proxy_service(service: &str) -> Result<ProxyServiceState, String> {
    let state = ProxyServiceState {
        service: service.to_string(),
        web: parse_proxy(&run_capture(&["-getwebproxy", service])?),
        secure_web: parse_proxy(&run_capture(&["-getsecurewebproxy", service])?),
        socks: parse_proxy(&run_capture(&["-getsocksfirewallproxy", service])?),
        auto_proxy: parse_auto_proxy(&run_capture(&["-getautoproxyurl", service])?),
        auto_discovery_enabled: Some(parse_auto_discovery(&run_capture(&[
            "-getproxyautodiscovery",
            service,
        ])?)),
    };
    for (kind, proxy) in [
        ("Web", &state.web),
        ("Secure Web", &state.secure_web),
        ("SOCKS", &state.socks),
    ] {
        if proxy.enabled && (proxy.server.is_empty() || proxy.port == 0) {
            return Err(format!(
                "enabled {kind} proxy on `{service}` has no restorable server/port"
            ));
        }
    }
    if state.auto_proxy.enabled && state.auto_proxy.url.is_empty() {
        return Err(format!(
            "enabled PAC proxy on `{service}` has no restorable URL"
        ));
    }
    Ok(state)
}

fn capture_dns_service(service: &str) -> Result<DnsServiceState, String> {
    Ok(DnsServiceState {
        service: service.to_string(),
        dns: parse_dns(&run_capture(&["-getdnsservers", service])?),
    })
}

/// `networksetup` prefixes disabled-but-existing services with `*`. Keep them in
/// a recovery transaction; only a genuinely deleted service may be discarded.
fn current_service_names() -> Option<HashSet<String>> {
    let out = run_capture(&["-listallnetworkservices"]).ok()?;
    Some(
        out.lines()
            .skip(1)
            .filter_map(|line| {
                let raw = line.trim();
                if raw.is_empty() {
                    return None;
                }
                let name = raw.strip_prefix('*').unwrap_or(raw).trim();
                (!name.is_empty()).then(|| name.to_string())
            })
            .collect(),
    )
}

fn prune_deleted_services(snapshot: &mut Snapshot) {
    let Some(existing) = current_service_names() else {
        // Discovery failure is not evidence of deletion. Preserve the user's
        // only original snapshot and retry later.
        return;
    };
    if let Some(proxy) = snapshot.proxy.as_mut() {
        proxy.retain(|item| existing.contains(&item.service));
    }
    if let Some(dns) = snapshot.dns.as_mut() {
        dns.retain(|item| existing.contains(&item.service));
    }
}

fn merge_proxy_states(
    stored: &mut Vec<ProxyServiceState>,
    current: Vec<ProxyServiceState>,
) -> bool {
    let mut known: HashSet<String> = stored.iter().map(|item| item.service.clone()).collect();
    let mut changed = false;
    for item in current {
        if known.insert(item.service.clone()) {
            stored.push(item);
            changed = true;
        }
    }
    changed
}

fn merge_dns_states(stored: &mut Vec<DnsServiceState>, current: Vec<DnsServiceState>) -> bool {
    let mut known: HashSet<String> = stored.iter().map(|item| item.service.clone()).collect();
    let mut changed = false;
    for item in current {
        if known.insert(item.service.clone()) {
            stored.push(item);
            changed = true;
        }
    }
    changed
}

fn load_snapshot(p: &Path) -> Result<Option<Snapshot>, String> {
    if !p.exists() {
        return Ok(None);
    }
    let bytes = fs::read(p).map_err(|e| format!("read network recovery snapshot: {e}"))?;
    let snapshot: Snapshot = serde_json::from_slice(&bytes)
        .map_err(|e| format!("parse network recovery snapshot: {e}"))?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(format!(
            "unsupported network recovery snapshot version {}",
            snapshot.version
        ));
    }
    Ok(Some(snapshot))
}

fn sync_parent(p: &Path) -> Result<(), String> {
    let Some(parent) = p.parent() else {
        return Ok(());
    };
    fs::File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| format!("sync network recovery directory: {e}"))
}

fn save_snapshot(p: &Path, snapshot: &Snapshot) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.tmp");
    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("create network recovery snapshot: {e}"))?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("chmod network recovery snapshot: {e}"))?;
        file.write_all(&bytes)
            .map_err(|e| format!("write network recovery snapshot: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("sync network recovery snapshot: {e}"))?;
    }
    fs::rename(&tmp, p).map_err(|e| format!("commit network recovery snapshot: {e}"))?;
    fs::set_permissions(p, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod committed network recovery snapshot: {e}"))?;
    sync_parent(p)
}

fn persist_or_remove(p: &Path, snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.is_empty() {
        if p.exists() {
            fs::remove_file(p).map_err(|e| format!("remove network recovery snapshot: {e}"))?;
            sync_parent(p)?;
        }
        CURRENT_OWNERSHIP.store(false, Ordering::SeqCst);
        Ok(())
    } else {
        save_snapshot(p, snapshot)?;
        CURRENT_OWNERSHIP.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn mark_stale<T>(result: Result<T, String>) -> Result<T, String> {
    if result.is_err() {
        CURRENT_OWNERSHIP.store(false, Ordering::SeqCst);
    }
    result
}

fn restore_proxy_state(service: &str, kind: &str, state: &ProxyState) -> Result<(), String> {
    let (set_value, set_state) = match kind {
        "web" => ("-setwebproxy", "-setwebproxystate"),
        "secure" => ("-setsecurewebproxy", "-setsecurewebproxystate"),
        "socks" => ("-setsocksfirewallproxy", "-setsocksfirewallproxystate"),
        _ => return Err(format!("unknown proxy kind: {kind}")),
    };
    if !state.server.is_empty() && state.port != 0 {
        let port = state.port.to_string();
        run(&[set_value, service, &state.server, &port])?;
    } else if state.enabled {
        return Err(format!("enabled {kind} proxy snapshot has no server/port"));
    }
    run(&[set_state, service, if state.enabled { "on" } else { "off" }])
}

fn restore_proxy(services: &[ProxyServiceState]) -> Result<(), String> {
    let mut failures = Vec::new();
    for service in services {
        let result = (|| {
            restore_proxy_state(&service.service, "web", &service.web)?;
            restore_proxy_state(&service.service, "secure", &service.secure_web)?;
            restore_proxy_state(&service.service, "socks", &service.socks)?;
            if !service.auto_proxy.url.is_empty() {
                run(&[
                    "-setautoproxyurl",
                    &service.service,
                    &service.auto_proxy.url,
                ])?;
            } else if service.auto_proxy.enabled {
                return Err("enabled PAC snapshot has no URL".into());
            }
            run(&[
                "-setautoproxystate",
                &service.service,
                if service.auto_proxy.enabled {
                    "on"
                } else {
                    "off"
                },
            ])?;
            if let Some(enabled) = service.auto_discovery_enabled {
                run(&[
                    "-setproxyautodiscovery",
                    &service.service,
                    if enabled { "on" } else { "off" },
                ])?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            failures.push(format!("{}: {e}", service.service));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "restore system proxy/PAC failed: {}",
            failures.join(" · ")
        ))
    }
}

fn restore_dns(services: &[DnsServiceState]) -> Result<(), String> {
    let mut failures = Vec::new();
    for service in services {
        let result = if let Some(servers) = &service.dns {
            let mut args = vec!["-setdnsservers", service.service.as_str()];
            args.extend(servers.iter().map(String::as_str));
            run(&args)
        } else {
            run(&["-setdnsservers", &service.service, "Empty"])
        };
        if let Err(e) = result {
            failures.push(format!("{}: {e}", service.service));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "restore system DNS failed: {}",
            failures.join(" · ")
        ))
    }
}

/// Restore a journal left by a prior process. Failed categories remain in the
/// journal so a later startup/network mutation can retry them.
pub(crate) fn recover_stale_and_clear() -> Result<bool, String> {
    if CURRENT_OWNERSHIP.load(Ordering::SeqCst) {
        return Ok(false);
    }
    let p = path();
    let Some(mut snapshot) = load_snapshot(&p)? else {
        return Ok(false);
    };
    prune_deleted_services(&mut snapshot);
    let mut failures = Vec::new();
    if let Some(proxy) = snapshot.proxy.as_ref() {
        match restore_proxy(proxy) {
            Ok(()) => snapshot.proxy = None,
            Err(e) => failures.push(e),
        }
    }
    if let Some(dns) = snapshot.dns.as_ref() {
        match restore_dns(dns) {
            Ok(()) => snapshot.dns = None,
            Err(e) => failures.push(e),
        }
    }
    mark_stale(persist_or_remove(&p, &snapshot))?;
    if failures.is_empty() {
        Ok(true)
    } else {
        CURRENT_OWNERSHIP.store(false, Ordering::SeqCst);
        Err(failures.join(" · "))
    }
}

fn prepare_current_snapshot() -> Result<(PathBuf, Snapshot), String> {
    if !CURRENT_OWNERSHIP.load(Ordering::SeqCst) {
        recover_stale_and_clear()?;
    }
    let p = path();
    let snapshot = load_snapshot(&p)?.unwrap_or_else(Snapshot::empty);
    Ok((p, snapshot))
}

fn authenticated_proxy_description(states: &[ProxyServiceState]) -> Option<String> {
    for service in states {
        for (kind, proxy) in [
            ("Web", &service.web),
            ("Secure Web", &service.secure_web),
            ("SOCKS", &service.socks),
        ] {
            if proxy.authenticated {
                return Some(format!("{kind} proxy on `{}`", service.service));
            }
        }
    }
    None
}

pub(crate) fn ensure_proxy_snapshot(services: &[String]) -> Result<(), String> {
    let (p, mut snapshot) = prepare_current_snapshot()?;
    let was_none = snapshot.proxy.is_none();
    let mut stored = snapshot.proxy.take().unwrap_or_default();
    let known: HashSet<String> = stored.iter().map(|item| item.service.clone()).collect();
    let mut current = Vec::new();
    for service in services.iter().filter(|service| !known.contains(*service)) {
        current.push(capture_proxy_service(service)?);
    }
    if let Some(found) = authenticated_proxy_description(&current) {
        return Err(format!(
            "authenticated {found} is configured; Nexus will not overwrite authenticated system proxies because networksetup cannot read the credentials needed for exact restoration"
        ));
    }
    let changed = merge_proxy_states(&mut stored, current);
    snapshot.proxy = Some(stored);
    if was_none || changed {
        mark_stale(save_snapshot(&p, &snapshot))?;
    }
    CURRENT_OWNERSHIP.store(true, Ordering::SeqCst);
    Ok(())
}

pub(crate) fn ensure_dns_snapshot(services: &[String]) -> Result<(), String> {
    let (p, mut snapshot) = prepare_current_snapshot()?;
    let was_none = snapshot.dns.is_none();
    let mut stored = snapshot.dns.take().unwrap_or_default();
    let known: HashSet<String> = stored.iter().map(|item| item.service.clone()).collect();
    let mut current = Vec::new();
    for service in services.iter().filter(|service| !known.contains(*service)) {
        current.push(capture_dns_service(service)?);
    }
    let changed = merge_dns_states(&mut stored, current);
    snapshot.dns = Some(stored);
    if was_none || changed {
        mark_stale(save_snapshot(&p, &snapshot))?;
    }
    CURRENT_OWNERSHIP.store(true, Ordering::SeqCst);
    Ok(())
}

pub(crate) fn restore_proxy_only() -> Result<bool, String> {
    let p = path();
    let Some(mut snapshot) = load_snapshot(&p)? else {
        return Ok(false);
    };
    prune_deleted_services(&mut snapshot);
    let Some(proxy) = snapshot.proxy.as_ref() else {
        return Ok(false);
    };
    mark_stale(restore_proxy(proxy))?;
    snapshot.proxy = None;
    mark_stale(persist_or_remove(&p, &snapshot))?;
    Ok(true)
}

pub(crate) fn restore_dns_only() -> Result<bool, String> {
    let p = path();
    let Some(mut snapshot) = load_snapshot(&p)? else {
        return Ok(false);
    };
    prune_deleted_services(&mut snapshot);
    let Some(dns) = snapshot.dns.as_ref() else {
        return Ok(false);
    };
    mark_stale(restore_dns(dns))?;
    snapshot.dns = None;
    mark_stale(persist_or_remove(&p, &snapshot))?;
    Ok(true)
}

pub(crate) fn restore_all_and_clear() -> Result<bool, String> {
    let p = path();
    let Some(mut snapshot) = load_snapshot(&p)? else {
        CURRENT_OWNERSHIP.store(false, Ordering::SeqCst);
        return Ok(false);
    };
    prune_deleted_services(&mut snapshot);
    let mut restored_any = false;
    let mut failures = Vec::new();
    if let Some(proxy) = snapshot.proxy.as_ref() {
        match restore_proxy(proxy) {
            Ok(()) => {
                snapshot.proxy = None;
                restored_any = true;
            }
            Err(e) => failures.push(e),
        }
    }
    if let Some(dns) = snapshot.dns.as_ref() {
        match restore_dns(dns) {
            Ok(()) => {
                snapshot.dns = None;
                restored_any = true;
            }
            Err(e) => failures.push(e),
        }
    }
    mark_stale(persist_or_remove(&p, &snapshot))?;
    if failures.is_empty() {
        Ok(restored_any)
    } else {
        // A partial teardown keeps only unfinished categories and marks them stale
        // so a future mutation cannot silently take ownership over them.
        CURRENT_OWNERSHIP.store(false, Ordering::SeqCst);
        Err(failures.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy_state(service: &str, server: &str) -> ProxyServiceState {
        ProxyServiceState {
            service: service.into(),
            web: ProxyState {
                enabled: !server.is_empty(),
                server: server.into(),
                port: if server.is_empty() { 0 } else { 8080 },
                authenticated: false,
            },
            secure_web: ProxyState {
                enabled: false,
                server: String::new(),
                port: 0,
                authenticated: false,
            },
            socks: ProxyState {
                enabled: false,
                server: String::new(),
                port: 0,
                authenticated: false,
            },
            auto_proxy: AutoProxyState {
                enabled: false,
                url: String::new(),
            },
            auto_discovery_enabled: Some(false),
        }
    }

    #[test]
    fn parsers_preserve_proxy_and_dns_shapes() {
        assert_eq!(
            parse_proxy(
                "Enabled: Yes\nServer: proxy.example\nPort: 8080\nAuthenticated Proxy Enabled: 0\n"
            ),
            ProxyState {
                enabled: true,
                server: "proxy.example".into(),
                port: 8080,
                authenticated: false,
            }
        );
        assert!(
            parse_proxy(
                "Enabled: Yes\nServer: proxy.example\nPort: 8080\nAuthenticated Proxy Enabled: 1\n"
            )
            .authenticated
        );
        assert_eq!(
            parse_dns("1.1.1.1\n9.9.9.9\n"),
            Some(vec!["1.1.1.1".into(), "9.9.9.9".into()])
        );
    }

    #[test]
    fn auto_proxy_null_is_unset() {
        assert_eq!(
            parse_auto_proxy("URL: (null)\nEnabled: No\n"),
            AutoProxyState {
                enabled: false,
                url: String::new(),
            }
        );
    }

    #[test]
    fn first_seen_proxy_state_is_never_overwritten() {
        let mut stored = vec![proxy_state("Wi-Fi", "original.example")];
        assert!(merge_proxy_states(
            &mut stored,
            vec![
                proxy_state("Wi-Fi", "nexus.example"),
                proxy_state("USB LAN", "user.example"),
            ]
        ));
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].web.server, "original.example");
        assert_eq!(stored[1].web.server, "user.example");
    }

    #[test]
    fn authenticated_proxy_is_not_safe_to_take_over() {
        let mut state = proxy_state("Wi-Fi", "proxy.example");
        state.web.authenticated = true;
        assert!(authenticated_proxy_description(&[state.clone()]).is_some());
        state.web.authenticated = false;
        assert!(authenticated_proxy_description(&[state]).is_none());
    }

    #[test]
    fn old_snapshot_without_wpad_does_not_claim_wpad() {
        let body = r#"{
            "service":"Wi-Fi",
            "web":{"enabled":false,"server":"","port":0},
            "secure_web":{"enabled":false,"server":"","port":0},
            "socks":{"enabled":false,"server":"","port":0},
            "auto_proxy":{"enabled":false,"url":""}
        }"#;
        let state: ProxyServiceState = serde_json::from_str(body).unwrap();
        assert_eq!(state.auto_discovery_enabled, None);
    }

    #[test]
    fn empty_snapshot_has_no_owned_components() {
        assert!(Snapshot::empty().is_empty());
    }
}
