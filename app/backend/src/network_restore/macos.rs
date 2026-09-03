use super::state::{DnsServiceState, ManualProxyState, ProxyServiceState};
use crate::sys;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const NETWORKSETUP: &str = "/usr/sbin/networksetup";
const NS_TIMEOUT: Duration = Duration::from_secs(5);

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
        field(&run_ns_capture(&["-getautoproxyurl", service])?, "Enabled")
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

pub(super) fn capture_proxy() -> Result<Vec<ProxyServiceState>, String> {
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

pub(super) fn capture_dns() -> Result<Vec<DnsServiceState>, String> {
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
    run_ns(&[state_cmd, service, if state.enabled { "on" } else { "off" }])
}

pub(super) fn restore_proxy_snapshot(snapshot: &[ProxyServiceState]) -> Result<String, String> {
    let mut failures = Vec::new();
    for state in snapshot {
        let result = (|| -> Result<(), String> {
            restore_manual_proxy(&state.service, "web", &state.web)?;
            restore_manual_proxy(&state.service, "secure", &state.secure_web)?;
            restore_manual_proxy(&state.service, "socks", &state.socks)?;
            run_ns(&[
                "-setautoproxystate",
                state.service.as_str(),
                if state.auto_proxy_enabled {
                    "on"
                } else {
                    "off"
                },
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
        if let Err(e) = result {
            failures.push(format!("`{}`: {e}", state.service));
        }
    }
    if failures.is_empty() {
        Ok(format!(
            "restored system proxy/PAC · {} service(s)",
            snapshot.len()
        ))
    } else {
        Err(format!(
            "restore system proxy/PAC failed: {}",
            failures.join(" · ")
        ))
    }
}

pub(super) fn restore_dns_snapshot(snapshot: &[DnsServiceState]) -> Result<String, String> {
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
        Ok(format!(
            "restored system DNS · {} service(s)",
            snapshot.len()
        ))
    } else {
        Err(format!(
            "restore system DNS failed: {}",
            failures.join(" · ")
        ))
    }
}

pub(super) fn disable_automatic_proxy(snapshot: &[ProxyServiceState]) -> Result<(), String> {
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
        Err(format!(
            "disable automatic proxy failed: {}",
            failures.join(" · ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_parser_reads_disabled_empty_state() {
        let p =
            parse_manual_proxy("Enabled: No\nServer: \nPort: 0\nAuthenticated Proxy Enabled: 0\n")
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
}
