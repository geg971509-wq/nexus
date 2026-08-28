//! System proxy: macOS networksetup.

#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
const NETWORKSETUP: &str = "/usr/sbin/networksetup";
#[cfg(target_os = "macos")]
const NS_TIMEOUT: Duration = Duration::from_secs(5);

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
                    .and_then(|s| {
                        let mut b = Vec::new();
                        let _ = std::io::Read::read_to_end(s, &mut b);
                        Some(String::from_utf8_lossy(&b).trim().to_string())
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
fn hot_services(enabled: bool) -> Vec<String> {
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
    let host = "127.0.0.1";
    let port_s = port.to_string();
    let hot = hot_services(enabled);

    let primary = hot.first().cloned().unwrap_or_else(|| "Wi-Fi".into());
    apply_one(&primary, enabled, host, &port_s).map_err(|e| {
        format!("system proxy on primary `{primary}` failed: {e}")
    })?;

    let rest: Vec<String> = hot.into_iter().skip(1).collect();
    let rest_n = rest.len();
    if rest_n > 0 {
        if enabled {
            // Enabling late only costs latency, so the secondary services can
            // finish behind the connect.
            let host = host.to_string();
            let port_s = port_s.clone();
            std::thread::Builder::new()
                .name("nexus-sysproxy".into())
                .spawn(move || {
                    for s in rest {
                        let _ = apply_one(&s, enabled, &host, &port_s);
                    }
                })
                .ok();
        } else {
            // Disabling must not be: quit calls this and then exits, killing a
            // detached thread mid-clear and leaving those services pointed at a
            // dead 127.0.0.1:port — that interface simply stops working until the
            // user fixes it by hand.
            for s in &rest {
                let _ = apply_one(s, enabled, host, &port_s);
            }
        }
    }

    if enabled {
        Ok(format!(
            "system proxy on {host}:{port} · primary {primary}{}",
            if rest_n > 0 {
                format!(" · +{rest_n} bg")
            } else {
                String::new()
            }
        ))
    } else {
        Ok(format!(
            "system proxy off · primary {primary}{}",
            if rest_n > 0 {
                format!(" · +{rest_n} bg")
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
pub fn set_system_dns_bootstrap(enabled: bool, servers: &[String]) -> Result<String, String> {
    let ips = dns_servers_for_os(servers);
    let hot = hot_services(true);
    let primary = hot.first().cloned().unwrap_or_else(|| "Wi-Fi".into());
    let apply_one_dns = |service: &str, on: bool| -> Result<(), String> {
        if on {
            let mut argv: Vec<&str> = vec!["-setdnsservers", service];
            for ip in &ips {
                argv.push(ip.as_str());
            }
            run_ns(&argv)
        } else {
            run_ns(&["-setdnsservers", service, "Empty"])
        }
    };
    apply_one_dns(&primary, enabled)
        .map_err(|e| format!("system dns on primary `{primary}` failed: {e}"))?;
    let rest: Vec<String> = hot.into_iter().skip(1).collect();
    let rest_n = rest.len();
    if rest_n > 0 {
        let ips_bg = ips.clone();
        std::thread::Builder::new()
            .name("nexus-sysdns".into())
            .spawn(move || {
                for s in rest {
                    if enabled {
                        let mut argv: Vec<&str> = vec!["-setdnsservers", &s];
                        for ip in &ips_bg {
                            argv.push(ip.as_str());
                        }
                        let _ = run_ns(&argv);
                    } else {
                        let _ = run_ns(&["-setdnsservers", &s, "Empty"]);
                    }
                }
            })
            .ok();
    }
    if enabled {
        Ok(format!(
            "system dns {} · primary {primary}{}",
            ips.join(","),
            if rest_n > 0 {
                format!(" · +{rest_n} bg")
            } else {
                String::new()
            }
        ))
    } else {
        Ok(format!(
            "system dns dhcp · primary {primary}{}",
            if rest_n > 0 {
                format!(" · +{rest_n} bg")
            } else {
                String::new()
            }
        ))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_system_dns_bootstrap(enabled: bool, servers: &[String]) -> Result<String, String> {
    let _ = (enabled, servers);
    Ok("system dns: no-op".into())
}

#[cfg(test)]
mod tests {
    use super::dns_servers_for_os;

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
}

#[cfg(not(target_os = "macos"))]
pub fn set_system_proxy(enabled: bool, port: u16) -> Result<String, String> {
    Err(format!(
        "system proxy not implemented on this OS (enabled={enabled} port={port})"
    ))
}
