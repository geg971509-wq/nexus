//! macOS system integration stubs (Phase D partial).
//! Real networksetup / SCDynamicStore can replace these without UI changes.

/// Apply system HTTP/HTTPS/SOCKS proxy to 127.0.0.1:port (best-effort).
/// Returns human-readable status; never panics.
pub fn set_system_proxy(enabled: bool, port: u16) -> Result<String, String> {
    // Prefer networksetup if present; dry-run log if not privileged.
    let service = detect_network_service().unwrap_or_else(|| "Wi-Fi".into());
    if !enabled {
        let _ = std::process::Command::new("networksetup")
            .args(["-setwebproxystate", &service, "off"])
            .output();
        let _ = std::process::Command::new("networksetup")
            .args(["-setsecurewebproxystate", &service, "off"])
            .output();
        let _ = std::process::Command::new("networksetup")
            .args(["-setsocksfirewallproxystate", &service, "off"])
            .output();
        return Ok(format!("system proxy off ({service})"));
    }
    let host = "127.0.0.1";
    let port_s = port.to_string();
    let o1 = std::process::Command::new("networksetup")
        .args(["-setwebproxy", &service, host, &port_s])
        .output()
        .map_err(|e| e.to_string())?;
    let o2 = std::process::Command::new("networksetup")
        .args(["-setsecurewebproxy", &service, host, &port_s])
        .output()
        .map_err(|e| e.to_string())?;
    let o3 = std::process::Command::new("networksetup")
        .args(["-setsocksfirewallproxy", &service, host, &port_s])
        .output()
        .map_err(|e| e.to_string())?;
    if !(o1.status.success() && o2.status.success() && o3.status.success()) {
        return Err(format!(
            "networksetup may need privileges; web={} secure={} socks={}",
            o1.status, o2.status, o3.status
        ));
    }
    Ok(format!("system proxy on {host}:{port} ({service})"))
}

/// System DNS toggle: record intent; full resolver rewrite is privileged & deferred.
pub fn set_system_dns(enabled: bool) -> Result<String, String> {
    // Do not silently break user DNS without a clear restore path.
    // Phase D: persist intent + log; actual DNS write can use networksetup -setdnsservers.
    Ok(if enabled {
        "system_dns intent=on (apply deferred without admin UX)".into()
    } else {
        "system_dns intent=off".into()
    })
}

fn detect_network_service() -> Option<String> {
    let out = std::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('*') || t.contains("An asterisk") {
            continue;
        }
        if t == "Wi-Fi" || t == "Ethernet" {
            return Some(t.to_string());
        }
    }
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('*') && !l.contains("An asterisk"))
        .map(str::to_string)
}
