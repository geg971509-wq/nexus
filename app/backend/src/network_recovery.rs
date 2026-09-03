//! Crash-safe preservation/restoration of macOS network settings Nexus owns.
//!
//! The recovery journal is intentionally separate from the long-lived product
//! Store. Proxy/PAC and DNS are recorded independently so disconnect only restores
//! settings Nexus actually changed. A stale journal survives an abnormal exit and
//! is recovered before a new process may take ownership.

use serde::{Deserialize, Serialize};
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
    // networksetup's getter exposes only whether authentication is enabled, not
    // credentials. Keep a default for journals written by earlier audit builds.
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
    let start = Instant::now();
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
            Ok(None) if start.elapsed() <= TIMEOUT => {
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
    AutoProxyState {
        enabled: bool_value(text, "Enabled"),
        url: value(text, "URL").to_string(),
    }
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
    Ok(ProxyServiceState {
        service: service.to_string(),
        web: parse_proxy(&run_capture(&["-getwebproxy", service])?),
        secure_web: parse_proxy(&run_capture(&["-getsecurewebproxy", service])?),
        socks: parse_proxy(&run_capture(&["-getsocksfirewallproxy", service])?),
        auto_proxy: parse_auto_proxy(&run_capture(&["-getautoproxyurl", service])?),
    })
}

fn capture_dns_service(service: &str) -> Result<DnsServiceState, String> {
    Ok(DnsServiceState {
        service: service.to_string(),
        dns: parse_dns(&run_capture(&["-getdnsservers", service])?),
    })
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
    Ok(())
}

fn persist_or_remove(p: &Path, snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.is_empty() {
        if p.exists() {
            fs::remove_file(p).map_err(|e| format!("remove network recovery snapshot: {e}"))?;
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
            }
            run(&[
                "-setautoproxystate",
                &service.service,
                if service.auto_proxy.enabled { "on" } else { "off" },
            ])
        })();
        if let Err(e) = result {
            failures.push(format!("{}: {e}", service.service));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("restore system proxy/PAC failed: {}", failures.join(" · ")))
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
        Err(format!("restore system DNS failed: {}", failures.join(" · ")))
    }
}

/// Restore a journal left by a prior process. A failed restoration deliberately
/// leaves the journal intact so a later startup/network mutation can retry it.
pub(crate) fn recover_stale_and_clear() -> Result<bool, String> {
    if CURRENT_OWNERSHIP.load(Ordering::SeqCst) {
        return Ok(false);
    }
    let p = path();
    let Some(mut snapshot) = load_snapshot(&p)? else {
        return Ok(false);
    };
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
    if snapshot.proxy.is_none() {
        let mut states = Vec::with_capacity(services.len());
        for service in services {
            states.push(capture_proxy_service(service)?);
        }
        if let Some(found) = authenticated_proxy_description(&states) {
            return Err(format!(
                "authenticated {found} is configured; Nexus will not overwrite authenticated system proxies because networksetup cannot read the credentials needed for exact restoration"
            ));
        }
        snapshot.proxy = Some(states);
        save_snapshot(&p, &snapshot)?;
    }
    CURRENT_OWNERSHIP.store(true, Ordering::SeqCst);
    Ok(())
}

pub(crate) fn ensure_dns_snapshot(services: &[String]) -> Result<(), String> {
    let (p, mut snapshot) = prepare_current_snapshot()?;
    if snapshot.dns.is_none() {
        let mut states = Vec::with_capacity(services.len());
        for service in services {
            states.push(capture_dns_service(service)?);
        }
        snapshot.dns = Some(states);
        save_snapshot(&p, &snapshot)?;
    }
    CURRENT_OWNERSHIP.store(true, Ordering::SeqCst);
    Ok(())
}

pub(crate) fn restore_proxy_only() -> Result<bool, String> {
    let p = path();
    let Some(mut snapshot) = load_snapshot(&p)? else {
        return Ok(false);
    };
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
        // A full teardown that could not restore every owned component leaves a
        // recovery journal, but that journal must be treated as stale before any
        // future mutation—even inside this process.
        CURRENT_OWNERSHIP.store(false, Ordering::SeqCst);
        Err(failures.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(parse_proxy(
            "Enabled: Yes\nServer: proxy.example\nPort: 8080\nAuthenticated Proxy Enabled: 1\n"
        )
        .authenticated);
        assert_eq!(
            parse_auto_proxy("URL: https://example/pac\nEnabled: Yes\n"),
            AutoProxyState {
                enabled: true,
                url: "https://example/pac".into()
            }
        );
        assert_eq!(
            parse_dns("1.1.1.1\n9.9.9.9\n"),
            Some(vec!["1.1.1.1".into(), "9.9.9.9".into()])
        );
        assert_eq!(
            parse_dns("There aren't any DNS Servers set on Wi-Fi."),
            None
        );
    }

    #[test]
    fn authenticated_proxy_is_not_safe_to_take_over() {
        let mut state = ProxyServiceState {
            service: "Wi-Fi".into(),
            web: ProxyState {
                enabled: true,
                server: "proxy.example".into(),
                port: 8080,
                authenticated: true,
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
        };
        assert!(authenticated_proxy_description(&[state.clone()]).is_some());
        state.web.authenticated = false;
        assert!(authenticated_proxy_description(&[state]).is_none());
    }

    #[test]
    fn empty_snapshot_has_no_owned_components() {
        assert!(Snapshot::empty().is_empty());
    }
}
