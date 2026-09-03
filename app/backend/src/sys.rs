//! System proxy: macOS networksetup.

#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
const NETWORKSETUP: &str = "/usr/sbin/networksetup";
#[cfg(target_os = "macos")]
const NS_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(target_os = "macos")]
static SYSTEM_NETWORK_CHANGE: Mutex<()> = Mutex::new(());

#[cfg(target_os = "macos")]
pub(crate) fn with_system_network_change<T>(f: impl FnOnce() -> T) -> T {
    let _guard = SYSTEM_NETWORK_CHANGE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    f()
}

#[cfg(target_os = "macos")]
pub(crate) struct SystemNetworkChange;

#[cfg(target_os = "macos")]
impl SystemNetworkChange {
    pub(crate) fn set_system_proxy(&self, enabled: bool, port: u16) -> Result<String, String> {
        set_system_proxy_inner(enabled, port)
    }

    pub(crate) fn set_system_dns_bootstrap(
        &self,
        enabled: bool,
        servers: &[String],
    ) -> Result<String, String> {
        set_system_dns_bootstrap_inner(enabled, servers)
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn with_system_network_change_if<T>(
    is_current: impl FnOnce() -> bool,
    f: impl FnOnce(&SystemNetworkChange) -> T,
) -> Option<T> {
    let _guard = SYSTEM_NETWORK_CHANGE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !is_current() {
        return None;
    }
    Some(f(&SystemNetworkChange))
}

#[cfg(target_os = "macos")]
fn run_ns(args: &[&str]) -> Result<(), String> {
    let mut child = Command::new(NETWORKSETUP)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("networksetup start: {e}"))?;
    let t0 = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let err = child
                    .stderr
                    .as_mut()
                    .map(|s| {
                        let mut b = Vec::new();
                        let _ = std::io::Read::read_to_end(s, &mut b);
                        String::from_utf8_lossy(&b).trim().to_string()
                    })
                    .unwrap_or_default();
                return Err(format!(
                    "networksetup {:?} exit={status}{e}",
                    args,
                    e = if err.is_empty() {
                        String::new()
                    } else {
                        format!(" err={err}")
                    }
                ));
            }
            Ok(None) => {
                if t0.elapsed() > NS_TIMEOUT {
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

#[cfg(target_os = "macos")]
pub fn list_network_services() -> Vec<String> {
    let out = Command::new(NETWORKSETUP)
        .args(["-listallnetworkservices"])
        .output();
    let Ok(out) = out else {
        return vec!["Wi-Fi".into()];
    };
    let s = String::from_utf8_lossy(&out.stdout);
    let mut services = Vec::new();
    for (i, line) in s.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let t = line.trim();
        if t.is_empty() || t.contains('*') {
            continue;
        }
        services.push(t.to_string());
    }
    if services.is_empty() {
        services.push("Wi-Fi".into());
    }
    services
}

#[cfg(target_os = "macos")]
fn is_secondary_service(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("thunderbolt")
        || n.contains("bridge")
        || n.contains("iphone")
        || n.contains("ipad")
        || n.contains("shadowrocket")
        || n.contains("clash")
        || n.contains("tailscale")
        || n.contains("wireguard")
        || n.contains("utun")
        || n.contains("vpn")
        || n.contains("parallels")
        || n.contains("vmware")
        || n.contains("virtual")
}

/// Listed fresh on every call, not cached for the process lifetime: a dock or
/// USB NIC attached after launch would otherwise never receive the proxy, and
/// traffic over it would leave unproxied. This runs only on connect/disconnect,
/// so one `networksetup` listing is not on any hot path.
#[cfg(target_os = "macos")]
fn ordered_services() -> Vec<String> {
    let mut svcs = list_network_services();
    svcs.sort_by_key(|s| {
        let l = s.to_ascii_lowercase();
        if l == "wi-fi" || l == "wifi" {
            0u8
        } else if l.contains("ethernet") || l.contains("usb") || l.starts_with("ax") {
            1
        } else if is_secondary_service(s) {
            3
        } else {
            2
        }
    });
    svcs
}

#[cfg(target_os = "macos")]
fn apply_one(service: &str, enabled: bool, host: &str, port_s: &str) -> Result<(), String> {
    if !enabled {
        for args in [
            ["-setautoproxystate", service, "off"],
            ["-setwebproxystate", service, "off"],
            ["-setsecurewebproxystate", service, "off"],
            ["-setsocksfirewallproxystate", service, "off"],
        ] {
            run_ns(&args)?;
        }
        return Ok(());
    }
    run_ns(&["-setwebproxy", service, host, port_s])?;
    run_ns(&["-setsecurewebproxy", service, host, port_s])?;
    run_ns(&["-setwebproxystate", service, "on"])?;
    run_ns(&["-setsecurewebproxystate", service, "on"])?;
    run_ns(&["-setsocksfirewallproxy", service, host, port_s])?;
    run_ns(&["-setsocksfirewallproxystate", service, "on"])?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn hot_services(enabled: bool) -> Vec<String> {
    let services = ordered_services();
    if enabled {
        let real: Vec<String> = services
            .iter()
            .filter(|s| !is_secondary_service(s))
            .cloned()
            .collect();
        if real.is_empty() {
            services
        } else {
            real
        }
    } else {
        services
    }
}

#[cfg(target_os = "macos")]
pub fn set_system_proxy(enabled: bool, port: u16) -> Result<String, String> {
    with_system_network_change(|| set_system_proxy_inner(enabled, port))
}

#[cfg(target_os = "macos")]
fn set_system_proxy_inner(enabled: bool, port: u16) -> Result<String, String> {
    let host = "127.0.0.1";
    let port_s = port.to_string();
    let services = hot_services(enabled);
    let primary = services.first().cloned().unwrap_or_else(|| "Wi-Fi".into());
    let mut failures = Vec::new();
    for service in &services {
        if let Err(e) = apply_one(service, enabled, host, &port_s) {
            failures.push(format!("`{service}`: {e}"));
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "system proxy {} failed: {}",
            if enabled { "on" } else { "off" },
            failures.join(" · ")
        ));
    }
    let rest_n = services.len().saturating_sub(1);
    if enabled {
        Ok(format!(
            "system proxy on {host}:{port} · primary {primary}{}",
            if rest_n > 0 {
                format!(" · +{rest_n}")
            } else {
                String::new()
            }
        ))
    } else {
        Ok(format!(
            "system proxy off · primary {primary}{}",
            if rest_n > 0 {
                format!(" · +{rest_n}")
            } else {
                String::new()
            }
        ))
    }
}

/// IPs that `networksetup -setdnsservers` will receive.
/// Same validation as store/PF: a hostname is neither a resolver nor safe argv.
/// Empty / all-invalid falls back to the product default so OS, config, and PF
/// stay on one list.
pub fn dns_servers_for_os(servers: &[String]) -> Vec<String> {
    crate::defaults::sanitize_dns_bootstrap(servers)
}

/// Tun + fail-closed blocks bare DNS to LAN resolvers (router :53).
/// Point primary services at the same bootstrap list PF already passes.
/// `enabled=false` restores DHCP (`Empty`); `servers` is ignored then.
#[cfg(target_os = "macos")]
fn set_system_dns_bootstrap_inner(enabled: bool, servers: &[String]) -> Result<String, String> {
    let ips = dns_servers_for_os(servers);
    let services = hot_services(true);
    let primary = services.first().cloned().unwrap_or_else(|| "Wi-Fi".into());
    let mut failures = Vec::new();
    for service in &services {
        let result = if enabled {
            let mut argv: Vec<&str> = vec!["-setdnsservers", service];
            for ip in &ips {
                argv.push(ip.as_str());
            }
            run_ns(&argv)
        } else {
            run_ns(&["-setdnsservers", service, "Empty"])
        };
        if let Err(e) = result {
            failures.push(format!("`{service}`: {e}"));
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "system dns {} failed: {}",
            if enabled { "on" } else { "off" },
            failures.join(" · ")
        ));
    }
    let rest_n = services.len().saturating_sub(1);
    if enabled {
        Ok(format!(
            "system dns {} · primary {primary}{}",
            ips.join(","),
            if rest_n > 0 {
                format!(" · +{rest_n}")
            } else {
                String::new()
            }
        ))
    } else {
        Ok(format!(
            "system dns dhcp · primary {primary}{}",
            if rest_n > 0 {
                format!(" · +{rest_n}")
            } else {
                String::new()
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dns_servers_for_os, with_system_network_change, with_system_network_change_if,
        SYSTEM_NETWORK_CHANGE,
    };
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Barrier,
    };
    use std::time::Duration;

    #[test]
    fn os_dns_uses_custom_not_google() {
        let v = dns_servers_for_os(&["9.9.9.9".into()]);
        assert_eq!(v, vec!["9.9.9.9".to_string()]);
        assert!(!v.iter().any(|s| s == "8.8.8.8"));
    }

    #[test]
    fn os_dns_drops_hostnames_and_falls_back() {
        let v = dns_servers_for_os(&["dns.google".into(), "".into()]);
        assert!(v.contains(&"8.8.8.8".to_string()), "{v:?}");
        assert!(!v.iter().any(|s| s == "dns.google"));
    }

    #[test]
    fn os_dns_keeps_valid_from_mixed() {
        let v = dns_servers_for_os(&["dns.google".into(), "9.9.9.9".into()]);
        assert_eq!(v, vec!["9.9.9.9".to_string()]);
    }

    #[test]
    fn system_network_changes_are_serialized() {
        let start = Arc::new(Barrier::new(3));
        let active = Arc::new(AtomicUsize::new(0));
        let overlaps = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();

        for _ in 0..2 {
            let start = Arc::clone(&start);
            let active = Arc::clone(&active);
            let overlaps = Arc::clone(&overlaps);
            workers.push(std::thread::spawn(move || {
                start.wait();
                with_system_network_change(|| {
                    if active.fetch_add(1, Ordering::SeqCst) != 0 {
                        overlaps.fetch_add(1, Ordering::SeqCst);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }));
        }

        start.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(overlaps.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn stale_system_network_change_is_skipped_after_waiting_for_lock() {
        let guard = SYSTEM_NETWORK_CHANGE
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation = Arc::new(AtomicUsize::new(1));
        let started = Arc::new(Barrier::new(2));
        let ran = Arc::new(AtomicBool::new(false));
        let worker = {
            let generation = Arc::clone(&generation);
            let started = Arc::clone(&started);
            let ran = Arc::clone(&ran);
            std::thread::spawn(move || {
                started.wait();
                with_system_network_change_if(
                    || generation.load(Ordering::SeqCst) == 1,
                    |_| ran.store(true, Ordering::SeqCst),
                )
            })
        };

        started.wait();
        generation.store(2, Ordering::SeqCst);
        drop(guard);

        assert!(worker.join().unwrap().is_none());
        assert!(!ran.load(Ordering::SeqCst));
    }
}
