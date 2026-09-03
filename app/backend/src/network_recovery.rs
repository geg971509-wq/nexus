//! Crash-safe preservation/restoration of the user's macOS proxy and DNS state.
//!
//! This is deliberately separate from the long-lived product Store. The file is
//! a one-shot recovery journal: create it before Nexus mutates system networking,
//! keep it across abnormal exits, and remove it only after a complete restore.

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
const SNAPSHOT_VERSION: u32 = 1;

// False at process start means any on-disk journal is from an abnormal previous
// run. True means the journal belongs to this process's current ownership period.
static CURRENT_OWNERSHIP: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProxyState {
    enabled: bool,
    server: String,
    port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AutoProxyState {
    enabled: bool,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ServiceState {
    service: String,
    web: ProxyState,
    secure_web: ProxyState,
    socks: ProxyState,
    auto_proxy: AutoProxyState,
    /// None means DHCP / no explicit DNS servers.
    dns: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Snapshot {
    version: u32,
    services: Vec<ServiceState>,
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

fn parse_proxy(text: &str) -> ProxyState {
    let enabled = matches!(value(text, "Enabled"), "Yes" | "yes" | "1");
    ProxyState {
        enabled,
        server: value(text, "Server").to_string(),
        port: value(text, "Port").parse::<u16>().unwrap_or(0),
    }
}

fn parse_auto_proxy(text: &str) -> AutoProxyState {
    AutoProxyState {
        enabled: matches!(value(text, "Enabled"), "Yes" | "yes" | "1"),
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

fn capture_service(service: &str) -> Result<ServiceState, String> {
    Ok(ServiceState {
        service: service.to_string(),
        web: parse_proxy(&run_capture(&["-getwebproxy", service])?),
        secure_web: parse_proxy(&run_capture(&["-getsecurewebproxy", service])?),
        socks: parse_proxy(&run_capture(&["-getsocksfirewallproxy", service])?),
        auto_proxy: parse_auto_proxy(&run_capture(&["-getautoproxyurl", service])?),
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

fn restore_proxy(service: &ServiceState) -> Result<(), String> {
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
}

fn restore_dns(service: &ServiceState) -> Result<(), String> {
    if let Some(servers) = &service.dns {
        let mut args = vec!["-setdnsservers", service.service.as_str()];
        args.extend(servers.iter().map(String::as_str));
        run(&args)
    } else {
        run(&["-setdnsservers", &service.service, "Empty"])
    }
}

fn restore(snapshot: &Snapshot, proxy: bool, dns: bool) -> Result<(), String> {
    let mut failures = Vec::new();
    for service in &snapshot.services {
        if proxy {
            if let Err(e) = restore_proxy(service) {
                failures.push(format!("{} proxy: {e}", service.service));
            }
        }
        if dns {
            if let Err(e) = restore_dns(service) {
                failures.push(format!("{} dns: {e}", service.service));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "restore system network state failed: {}",
            failures.join(" · ")
        ))
    }
}

/// Restore a journal left by a previous process. Safe to call repeatedly before
/// this process starts owning system network state.
pub(crate) fn recover_stale_and_clear() -> Result<bool, String> {
    if CURRENT_OWNERSHIP.load(Ordering::SeqCst) {
        return Ok(false);
    }
    let p = path();
    let Some(snapshot) = load_snapshot(&p)? else {
        return Ok(false);
    };
    restore(&snapshot, true, true)?;
    fs::remove_file(&p).map_err(|e| format!("remove stale network recovery snapshot: {e}"))?;
    Ok(true)
}

/// Idempotent within one Nexus ownership period. If startup recovery has not run
/// yet, a pre-existing journal is stale and must be restored before capturing a
/// new baseline. Once ownership is current, later Proxy/DNS writes reuse it.
pub(crate) fn ensure_snapshot(services: &[String]) -> Result<(), String> {
    if !CURRENT_OWNERSHIP.load(Ordering::SeqCst) {
        recover_stale_and_clear()?;
    }
    let p = path();
    if load_snapshot(&p)?.is_some() {
        CURRENT_OWNERSHIP.store(true, Ordering::SeqCst);
        return Ok(());
    }
    let mut states = Vec::with_capacity(services.len());
    for service in services {
        states.push(capture_service(service)?);
    }
    save_snapshot(
        &p,
        &Snapshot {
            version: SNAPSHOT_VERSION,
            services: states,
        },
    )?;
    CURRENT_OWNERSHIP.store(true, Ordering::SeqCst);
    Ok(())
}

pub(crate) fn restore_proxy_only() -> Result<bool, String> {
    let Some(snapshot) = load_snapshot(&path())? else {
        return Ok(false);
    };
    restore(&snapshot, true, false)?;
    Ok(true)
}

pub(crate) fn restore_dns_only() -> Result<bool, String> {
    let Some(snapshot) = load_snapshot(&path())? else {
        return Ok(false);
    };
    restore(&snapshot, false, true)?;
    Ok(true)
}

pub(crate) fn restore_all_and_clear() -> Result<bool, String> {
    let p = path();
    let Some(snapshot) = load_snapshot(&p)? else {
        CURRENT_OWNERSHIP.store(false, Ordering::SeqCst);
        return Ok(false);
    };
    restore(&snapshot, true, true)?;
    fs::remove_file(&p).map_err(|e| format!("remove network recovery snapshot: {e}"))?;
    CURRENT_OWNERSHIP.store(false, Ordering::SeqCst);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers_preserve_proxy_and_dns_shapes() {
        assert_eq!(
            parse_proxy("Enabled: Yes\nServer: proxy.example\nPort: 8080\n"),
            ProxyState {
                enabled: true,
                server: "proxy.example".into(),
                port: 8080
            }
        );
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
}
