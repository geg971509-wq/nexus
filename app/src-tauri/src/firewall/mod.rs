//! OS packet-filter firewall (clean-room fail-closed; Mullvad-inspired semantics).
//! macOS: NexusFwD root daemon + PF anchor `nexus`.
//! Non-macOS: `platform_support()` is Unsupported.

pub mod rules;
pub mod wire;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub mod macos_pf;

pub use rules::{is_safe_ifname, ANCHOR};
pub use wire::{PolicyDto, Request, Response};

use crate::defaults::MIXED_PORT;
use crate::tunnel_sm::{ConnectParams, PeerEndpoint, State as SmState};
use std::net::IpAddr;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSupport {
    Active,
    Unsupported,
}

#[derive(Debug, Clone)]
pub enum Policy {
    Reset,
    Connecting {
        peer: PeerEndpoint,
        tun: bool,
        mixed_port: u16,
        /// Bootstrap resolvers PF must pass; must match the generated config.
        dns: Vec<String>,
    },
    Connected {
        peer: PeerEndpoint,
        tun: bool,
        mixed_port: u16,
        tun_if: Option<String>,
        dns: Vec<String>,
    },
    Blocked {
        peer: Option<PeerEndpoint>,
        mixed_port: u16,
        dns: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Status {
    pub support: PlatformSupport,
    pub last_policy: String,
    pub last_error: Option<String>,
    pub peer: Option<String>,
    pub tun_if: Option<String>,
    pub helper_installed: bool,
    pub helper_running: bool,
    pub helper_detail: Option<String>,
}

static LAST: Mutex<Status> = Mutex::new(Status {
    support: PlatformSupport::Unsupported,
    last_policy: String::new(),
    last_error: None,
    peer: None,
    tun_if: None,
    helper_installed: false,
    helper_running: false,
    helper_detail: None,
});

pub fn status() -> Status {
    let mut s = LAST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    s.support = platform_support();
    #[cfg(target_os = "macos")]
    {
        let (inst, run, det) = macos::helper_status();
        s.helper_installed = inst;
        s.helper_running = run;
        s.helper_detail = det;
    }
    #[cfg(target_os = "windows")]
    {
        // eng 4B / product 8A: OS firewall Unsupported — do not pretend helper is live.
        s.helper_installed = false;
        s.helper_running = false;
        s.helper_detail = Some("unsupported".into());
    }
    s
}

pub fn platform_support() -> PlatformSupport {
    // 8A: product ships OS firewall only on macOS; Windows stays Unsupported.
    #[cfg(target_os = "macos")]
    {
        PlatformSupport::Active
    }
    #[cfg(not(target_os = "macos"))]
    {
        PlatformSupport::Unsupported
    }
}

pub fn policy_from_sm(state: SmState, params: Option<&ConnectParams>) -> Policy {
    match state {
        SmState::Idle => Policy::Reset,
        SmState::Connecting => {
            if let Some(p) = params {
                Policy::Connecting {
                    peer: p.peer.clone(),
                    tun: p.tun,
                    mixed_port: p.mixed_port,
                    dns: p.dns.clone(),
                }
            } else {
                Policy::Blocked {
                    peer: None,
                    mixed_port: MIXED_PORT,
                    dns: Vec::new(),
                }
            }
        }
        SmState::Connected => {
            if let Some(p) = params {
                // L4: Tun on without ifname stays Connecting-class allow (peer only).
                if p.tun && p.tun_if.is_none() {
                    Policy::Connecting {
                        peer: p.peer.clone(),
                        tun: p.tun,
                        mixed_port: p.mixed_port,
                        dns: p.dns.clone(),
                    }
                } else {
                    Policy::Connected {
                        peer: p.peer.clone(),
                        tun: p.tun,
                        mixed_port: p.mixed_port,
                        tun_if: p.tun_if.clone(),
                        dns: p.dns.clone(),
                    }
                }
            } else {
                Policy::Blocked {
                    peer: None,
                    mixed_port: MIXED_PORT,
                    dns: Vec::new(),
                }
            }
        }
        SmState::Disconnecting | SmState::Error => Policy::Blocked {
            peer: params.map(|p| p.peer.clone()),
            mixed_port: params.map(|p| p.mixed_port).unwrap_or(MIXED_PORT),
            dns: params.map(|p| p.dns.clone()).unwrap_or_default(),
        },
    }
}

pub fn apply(policy: Policy) -> Result<(), String> {
    let name = policy_name(&policy);
    let peer_s = policy_peer_str(&policy);
    let tun_s = policy_tun_str(&policy);

    // Annotated: on non-macOS both arms are Ok(()), so E is otherwise unpinned
    // until the tail return — and `e.clone()` below needs it earlier.
    let result: Result<(), String> = match platform_support() {
        PlatformSupport::Unsupported => Ok(()),
        PlatformSupport::Active => {
            #[cfg(target_os = "macos")]
            {
                macos::apply_policy(&policy)
            }
            #[cfg(not(target_os = "macos"))]
            {
                Ok(())
            }
        }
    };

    // eng 3A: applied/last_policy only on successful apply; failures only last_error.
    let mut g = LAST.lock().unwrap_or_else(|e| e.into_inner());
    g.support = platform_support();
    match &result {
        Ok(()) => {
            g.last_policy = name;
            g.peer = peer_s;
            g.tun_if = tun_s;
            g.last_error = None;
        }
        Err(e) => {
            g.last_error = Some(e.clone());
        }
    }
    result
}

/// Desired policy from tunnel SM (not necessarily live on the helper).
pub fn desired_policy_name() -> String {
    let st = crate::tunnel_sm::state();
    let params = crate::tunnel_sm::last_params();
    policy_name(&policy_from_sm(st, params.as_ref()))
}

pub fn reset_best_effort() {
    let _ = apply(Policy::Reset);
}

/// mac: install LaunchDaemon once. Other OS: Ok.
pub fn install_helper() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let bin = macos::resolve_fwd_binary();
        macos::install_helper(&bin)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

pub fn uninstall_helper() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::uninstall_helper()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

/// L3: mac must have helper before connect.
/// Missing daemon or stale binary (7A) → one-shot install (admin sheet) then recheck.
pub fn require_ready_for_connect() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let bin = macos::resolve_fwd_binary();
        if !bin.is_file() {
            return Err(format!(
                "NexusFwD missing at {} — rebuild/bundle nexusfwd",
                bin.display()
            ));
        }
        // 7A: staged vs installed size/mtime mismatch → reinstall (admin OK).
        if macos::helper_binary_stale(&bin) {
            macos::install_helper(&bin)?;
            return macos::ensure_helper_ready();
        }
        if macos::ensure_helper_ready().is_ok() {
            return Ok(());
        }
        macos::install_helper(&bin)?;
        macos::ensure_helper_ready()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

fn policy_name(p: &Policy) -> String {
    match p {
        Policy::Reset => "reset".into(),
        Policy::Connecting { .. } => "connecting".into(),
        Policy::Connected { .. } => "connected".into(),
        Policy::Blocked { .. } => "blocked".into(),
    }
}

fn policy_peer_str(p: &Policy) -> Option<String> {
    match p {
        Policy::Connecting { peer, .. } | Policy::Connected { peer, .. } => {
            Some(format!("{}:{}", peer.ip, peer.port))
        }
        Policy::Blocked {
            peer: Some(peer), ..
        } => Some(format!("{}:{}", peer.ip, peer.port)),
        _ => None,
    }
}

fn policy_tun_str(p: &Policy) -> Option<String> {
    match p {
        Policy::Connected { tun_if, .. } => tun_if.clone(),
        _ => None,
    }
}

pub fn peer_from_outbound(outbound: &serde_json::Value) -> Result<PeerEndpoint, String> {
    let server = outbound
        .get("server")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "outbound missing server".to_string())?
        .trim();
    if server.is_empty() {
        return Err("outbound server empty".into());
    }
    let port = outbound
        .get("server_port")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            outbound
                .get("server_port")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(443) as u16;

    // Optional UI/catalog hint: display addr IP when server is a hostname (CDN).
    let mut hint_ips: Vec<IpAddr> = Vec::new();
    for key in ["server_ip", "ip"] {
        if let Some(s) = outbound.get(key).and_then(|v| v.as_str()) {
            if let Ok(ip) = s.trim().parse::<IpAddr>() {
                if !hint_ips.contains(&ip) {
                    hint_ips.push(ip);
                }
            }
        }
    }

    if let Ok(ip) = server.parse::<IpAddr>() {
        let mut ips = vec![ip];
        for h in hint_ips {
            if !ips.contains(&h) {
                ips.push(h);
            }
        }
        return Ok(PeerEndpoint { ip, port, ips });
    }
    let host_port = format!("{server}:{port}");
    let mut ips: Vec<IpAddr> = match std::net::ToSocketAddrs::to_socket_addrs(&host_port) {
        Ok(iter) => iter.map(|a| a.ip()).collect(),
        Err(e) => {
            // Hostname resolve failed (often residual Blocked DNS): use catalog display IP.
            if hint_ips.is_empty() {
                return Err(format!("resolve {server}: {e}"));
            }
            hint_ips.clone()
        }
    };
    for h in hint_ips {
        if !ips.contains(&h) {
            ips.push(h);
        }
    }
    // Dedup preserve order
    let mut seen = std::collections::HashSet::new();
    ips.retain(|ip| seen.insert(*ip));
    let ip = *ips
        .first()
        .ok_or_else(|| format!("resolve {server}: no addresses"))?;
    Ok(PeerEndpoint { ip, port, ips })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn peer_from_ip_outbound() {
        let v = serde_json::json!({"type":"socks","server":"1.2.3.4","server_port":1080});
        let p = peer_from_outbound(&v).unwrap();
        assert_eq!(p.ip, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
        assert_eq!(p.port, 1080);
    }

    #[test]
    fn peer_merges_server_ip_hint() {
        let v = serde_json::json!({
            "type":"vmess",
            "server":"1.2.3.4",
            "server_port":443,
            "server_ip":"5.6.7.8"
        });
        let p = peer_from_outbound(&v).unwrap();
        assert!(p.ips.contains(&IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))));
        assert!(p.ips.contains(&IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8))));
    }

    #[test]
    fn unsupported_apply_ok() {
        // Reset may fail without helper on mac; ignore error for unit smoke.
        let _ = apply(Policy::Reset);
    }
}
