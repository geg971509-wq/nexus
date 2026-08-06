//! macOS system proxy / DNS — upstream QvProxyConfigurator subset.
//! networksetup over services; proxy only meaningful when mixed is up.
//!
//! Latency: full scan of every service is ~1s on multi-NIC Macs. Apply primary
//! first (return), rest in background so the chip feels instant.

use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const NETWORKSETUP: &str = "/usr/sbin/networksetup";
const NS_TIMEOUT: Duration = Duration::from_secs(5);

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

/// macOSgetNetworkServices: skip header + disabled (*).
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
            continue; // "An asterisk (*) denotes..."
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

/// VPN / bridge / phone-tether services: set-on is wasted; still clear on off.
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

/// Wi-Fi / Ethernet first; secondaries last. Cached per process — services rarely change mid-session.
fn ordered_services() -> &'static Vec<String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE.get_or_init(|| {
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
    })
}

fn apply_one(service: &str, enabled: bool, host: &str, port_s: &str) -> Result<(), String> {
    if !enabled {
        // ClearSystemProxy
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
    // SetSystemProxy: set + explicit state on
    run_ns(&["-setwebproxy", service, host, port_s])?;
    run_ns(&["-setsecurewebproxy", service, host, port_s])?;
    run_ns(&["-setwebproxystate", service, "on"])?;
    run_ns(&["-setsecurewebproxystate", service, "on"])?;
    run_ns(&["-setsocksfirewallproxy", service, host, port_s])?;
    run_ns(&["-setsocksfirewallproxystate", service, "on"])?;
    Ok(())
}

/// SetSystemProxy / ClearSystemProxy (macOS).
/// Primary service applied synchronously (~0.2s); remaining services in a background thread.
pub fn set_system_proxy(enabled: bool, port: u16) -> Result<String, String> {
    let services = ordered_services().clone();
    let host = "127.0.0.1";
    let port_s = port.to_string();

    // ON: skip pure secondaries in the hot path (Shadowrocket/Bridge/…)
    // OFF: clear every service we may have touched earlier
    let hot: Vec<String> = if enabled {
        let real: Vec<String> = services
            .iter()
            .filter(|s| !is_secondary_service(s))
            .cloned()
            .collect();
        if real.is_empty() {
            services.clone()
        } else {
            real
        }
    } else {
        services.clone()
    };

    let primary = hot.first().cloned().unwrap_or_else(|| "Wi-Fi".into());
    apply_one(&primary, enabled, host, &port_s).map_err(|e| {
        format!("system proxy on primary `{primary}` failed: {e}")
    })?;

    let rest: Vec<String> = hot.into_iter().skip(1).collect();
    let rest_n = rest.len();
    if rest_n > 0 {
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

