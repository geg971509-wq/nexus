//! Shell tunnel state machine (1A) + connect gen ownership.
//! Sole transition surface for firewall + lifecycle side-effects hooks.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static CONNECT_GEN: AtomicU64 = AtomicU64::new(1);
static STATE: Mutex<State> = Mutex::new(State::Idle);
static LAST_PARAMS: Mutex<Option<ConnectParams>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Connecting,
    Connected,
    Disconnecting,
    Error,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Connecting => "connecting",
            State::Connected => "connected",
            State::Disconnecting => "disconnecting",
            State::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeerEndpoint {
    /// Primary (first resolved) address — used for display / legacy single-IP paths.
    pub ip: std::net::IpAddr,
    pub port: u16,
    /// All resolved addresses for this peer (CDN / dual-stack). PF must allow every one.
    pub ips: Vec<std::net::IpAddr>,
}

#[derive(Debug, Clone)]
pub struct ConnectParams {
    pub peer: PeerEndpoint,
    pub tun: bool,
    pub mixed_port: u16,
    pub tun_if: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    BeginConnect(ConnectParams),
    MarkConnected { tun_if: Option<String> },
    Fail(String),
    BeginDisconnect,
    ResetIdle,
    /// Core died while user still wants tunnel — stay non-Idle for 2A.
    CoreLost,
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub from: State,
    pub to: State,
    pub gen: u64,
    pub params: Option<ConnectParams>,
    pub error: Option<String>,
}

pub fn bump_gen() -> u64 {
    CONNECT_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
}

pub fn current_gen() -> u64 {
    CONNECT_GEN.load(Ordering::SeqCst)
}

pub fn state() -> State {
    *STATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn last_params() -> Option<ConnectParams> {
    LAST_PARAMS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Apply event; returns transition for firewall/proxy wiring.
pub fn apply(event: Event) -> Transition {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let mut lp = LAST_PARAMS.lock().unwrap_or_else(|e| e.into_inner());
    let from = *g;
    let (to, params, error, bump) = match (&event, from) {
        (Event::BeginConnect(p), _) => {
            *lp = Some(p.clone());
            (State::Connecting, Some(p.clone()), None, true)
        }
        (Event::MarkConnected { tun_if }, State::Connecting) => {
            if let Some(ref mut p) = *lp {
                p.tun_if = tun_if.clone();
            }
            (State::Connected, lp.clone(), None, false)
        }
        (Event::MarkConnected { .. }, other) => (other, lp.clone(), None, false),
        (Event::Fail(msg), State::Connecting | State::Connected | State::Disconnecting) => {
            (State::Error, lp.clone(), Some(msg.clone()), false)
        }
        (Event::Fail(msg), State::Error) => (State::Error, lp.clone(), Some(msg.clone()), false),
        (Event::Fail(_), State::Idle) => (State::Idle, None, None, false),
        (Event::BeginDisconnect, State::Idle) => (State::Idle, None, None, true),
        (Event::BeginDisconnect, _) => (State::Disconnecting, lp.clone(), None, true),
        (Event::ResetIdle, _) => {
            *lp = None;
            (State::Idle, None, None, false)
        }
        // 2A: do not Idle on core loss — keep last peer for Blocked.
        (Event::CoreLost, State::Connected | State::Connecting) => {
            (State::Error, lp.clone(), Some("core lost".into()), false)
        }
        (Event::CoreLost, other) => (other, lp.clone(), None, false),
    };
    if bump {
        let _ = CONNECT_GEN.fetch_add(1, Ordering::SeqCst);
    }
    *g = to;
    let gen = CONNECT_GEN.load(Ordering::SeqCst);
    Transition {
        from,
        to,
        gen,
        params,
        error,
    }
}

/// Test/helper only — production Connected must use `Event::MarkConnected` (eng 1A/6A).
#[cfg(test)]
pub fn set_state(s: State) {
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = s;
}

/// Update last_params tun_if without SM transition (Connected rebind after ifname detect).
pub fn update_tun_if(tun_if: Option<String>) {
    if let Some(ref mut p) = *LAST_PARAMS.lock().unwrap_or_else(|e| e.into_inner()) {
        p.tun_if = tun_if;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn peer() -> PeerEndpoint {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        PeerEndpoint {
            ip,
            port: 443,
            ips: vec![ip],
        }
    }

    use std::net::IpAddr;

    #[test]
    fn connect_fail_error_then_idle() {
        let _ = apply(Event::ResetIdle);
        let t = apply(Event::BeginConnect(ConnectParams {
            peer: peer(),
            tun: true,
            mixed_port: 2080,
            tun_if: None,
        }));
        assert_eq!(t.to, State::Connecting);
        let t = apply(Event::Fail("boom".into()));
        assert_eq!(t.to, State::Error);
        assert!(t.params.is_some());
        let t = apply(Event::BeginDisconnect);
        assert_eq!(t.to, State::Disconnecting);
        let t = apply(Event::ResetIdle);
        assert_eq!(t.to, State::Idle);
        assert!(last_params().is_none());
    }

    #[test]
    fn core_lost_stays_error_not_idle() {
        let _ = apply(Event::ResetIdle);
        let _ = apply(Event::BeginConnect(ConnectParams {
            peer: peer(),
            tun: false,
            mixed_port: 2080,
            tun_if: None,
        }));
        set_state(State::Connected);
        let t = apply(Event::CoreLost);
        assert_eq!(t.to, State::Error);
        assert_ne!(t.to, State::Idle);
        assert!(t.params.is_some());
        assert!(last_params().is_some());
    }
}
