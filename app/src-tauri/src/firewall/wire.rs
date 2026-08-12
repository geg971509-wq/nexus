//! JSON wire format for NexusFwD ↔ shell (no I/O).
//! peer_ips carries all resolved relay addresses (Mullvad peer_endpoints[]).

use super::Policy;
use crate::tunnel_sm::PeerEndpoint;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Status,
    Reset,
    Apply { policy: PolicyDto },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicyDto {
    Reset,
    Connecting {
        peer_ip: String,
        peer_port: u16,
        #[serde(default)]
        peer_ips: Vec<String>,
        tun: bool,
        mixed_port: u16,
    },
    Connected {
        peer_ip: String,
        peer_port: u16,
        #[serde(default)]
        peer_ips: Vec<String>,
        tun: bool,
        mixed_port: u16,
        tun_if: Option<String>,
    },
    Blocked {
        peer_ip: Option<String>,
        peer_port: Option<u16>,
        #[serde(default)]
        peer_ips: Vec<String>,
        mixed_port: u16,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default)]
    pub err: Option<String>,
    #[serde(default)]
    pub helper: Option<String>,
}

fn peer_ip_list(peer: &PeerEndpoint) -> Vec<String> {
    let mut v: Vec<String> = peer.ips.iter().map(|ip| ip.to_string()).collect();
    if v.is_empty() {
        v.push(peer.ip.to_string());
    } else if !v.iter().any(|s| s == &peer.ip.to_string()) {
        v.insert(0, peer.ip.to_string());
    }
    v
}

impl PolicyDto {
    pub fn from_policy(p: &Policy) -> Self {
        match p {
            Policy::Reset => PolicyDto::Reset,
            Policy::Connecting {
                peer,
                tun,
                mixed_port,
            } => PolicyDto::Connecting {
                peer_ip: peer.ip.to_string(),
                peer_port: peer.port,
                peer_ips: peer_ip_list(peer),
                tun: *tun,
                mixed_port: *mixed_port,
            },
            Policy::Connected {
                peer,
                tun,
                mixed_port,
                tun_if,
            } => PolicyDto::Connected {
                peer_ip: peer.ip.to_string(),
                peer_port: peer.port,
                peer_ips: peer_ip_list(peer),
                tun: *tun,
                mixed_port: *mixed_port,
                tun_if: tun_if.clone(),
            },
            Policy::Blocked { peer, mixed_port } => PolicyDto::Blocked {
                peer_ip: peer.as_ref().map(|p| p.ip.to_string()),
                peer_port: peer.as_ref().map(|p| p.port),
                peer_ips: peer.as_ref().map(peer_ip_list).unwrap_or_default(),
                mixed_port: *mixed_port,
            },
        }
    }

    pub fn into_policy(self) -> Result<Policy, String> {
        match self {
            PolicyDto::Reset => Ok(Policy::Reset),
            PolicyDto::Connecting {
                peer_ip,
                peer_port,
                peer_ips,
                tun,
                mixed_port,
            } => Ok(Policy::Connecting {
                peer: parse_peer(&peer_ip, peer_port, &peer_ips)?,
                tun,
                mixed_port,
            }),
            PolicyDto::Connected {
                peer_ip,
                peer_port,
                peer_ips,
                tun,
                mixed_port,
                tun_if,
            } => Ok(Policy::Connected {
                peer: parse_peer(&peer_ip, peer_port, &peer_ips)?,
                tun,
                mixed_port,
                tun_if,
            }),
            PolicyDto::Blocked {
                peer_ip,
                peer_port,
                peer_ips,
                mixed_port,
            } => {
                let peer = match (peer_ip, peer_port) {
                    (Some(ip), Some(port)) => Some(parse_peer(&ip, port, &peer_ips)?),
                    _ => None,
                };
                Ok(Policy::Blocked { peer, mixed_port })
            }
        }
    }
}

fn parse_peer(ip: &str, port: u16, more: &[String]) -> Result<PeerEndpoint, String> {
    let primary: IpAddr = ip
        .parse()
        .map_err(|e| format!("bad peer ip {ip}: {e}"))?;
    let mut ips = Vec::new();
    for s in more {
        if let Ok(a) = s.parse::<IpAddr>() {
            if !ips.contains(&a) {
                ips.push(a);
            }
        }
    }
    if !ips.contains(&primary) {
        ips.insert(0, primary);
    }
    if ips.is_empty() {
        ips.push(primary);
    }
    Ok(PeerEndpoint {
        ip: primary,
        port,
        ips,
    })
}
