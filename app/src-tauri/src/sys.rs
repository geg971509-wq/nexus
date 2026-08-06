//! System proxy: macOS networksetup; Windows WinINet Internet Settings (HKCU).

use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
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

#[cfg(target_os = "macos")]
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
pub fn set_system_proxy(enabled: bool, port: u16) -> Result<String, String> {
    let services = ordered_services().clone();
    let host = "127.0.0.1";
    let port_s = port.to_string();

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

/// Windows: HKCU Internet Settings (WinINet) + notify. Covers Edge/Chrome user proxy.
#[cfg(target_os = "windows")]
pub fn set_system_proxy(enabled: bool, port: u16) -> Result<String, String> {
    let host = "127.0.0.1";
    let proxy = format!("{host}:{port}");
    let enable = if enabled { "1" } else { "0" };
    // Single PowerShell: set registry + InternetSetOption refresh via winhttp/netsh not enough for browsers.
    let ps = format!(
        r#"$p='HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'; Set-ItemProperty -Path $p -Name ProxyEnable -Value {enable} -Type DWord; if ({enable} -eq 1) {{ Set-ItemProperty -Path $p -Name ProxyServer -Value '{proxy}' -Type String; Set-ItemProperty -Path $p -Name ProxyOverride -Value 'localhost;127.*;<local>' -Type String }} else {{ Remove-ItemProperty -Path $p -Name ProxyServer -ErrorAction SilentlyContinue }}; Add-Type -Namespace N -Name W -MemberDefinition '[DllImport(\"wininet.dll\")] public static extern bool InternetSetOption(IntPtr h, int o, IntPtr b, int l);'; [void][N.W]::InternetSetOption([IntPtr]::Zero, 39, [IntPtr]::Zero, 0); [void][N.W]::InternetSetOption([IntPtr]::Zero, 37, [IntPtr]::Zero, 0)"#
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("powershell proxy: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        return Err(format!(
            "windows system proxy failed: {} {}",
            err.trim(),
            stdout.trim()
        ));
    }
    if enabled {
        Ok(format!("system proxy on {proxy} (WinINet HKCU)"))
    } else {
        Ok("system proxy off (WinINet HKCU)".into())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn set_system_proxy(enabled: bool, port: u16) -> Result<String, String> {
    Err(format!(
        "system proxy not implemented on this OS (enabled={enabled} port={port})"
    ))
}
