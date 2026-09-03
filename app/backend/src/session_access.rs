//! Generation-safe access to the single live Core IPC session.

use crate::{
    core::session::{CoreSession, SESSION},
    tunnel_sm,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

// Final network-policy commits must be ordered with lifecycle invalidation. Keep
// this separate from SESSION so firewall/networksetup work never holds Core IPC.
static LIFECYCLE_COMMIT: Mutex<()> = Mutex::new(());
static LIFECYCLE_ACTION_GEN: AtomicU64 = AtomicU64::new(0);

pub(crate) fn bump_connect_gen() -> u64 {
    tunnel_sm::bump_gen()
}

pub(crate) fn current_connect_gen() -> u64 {
    tunnel_sm::current_gen()
}

pub(crate) fn lifecycle_commit<T>(f: impl FnOnce() -> T) -> T {
    let _guard = LIFECYCLE_COMMIT.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

pub(crate) fn admit_lifecycle_action() -> u64 {
    lifecycle_commit(|| {
        let action_gen = LIFECYCLE_ACTION_GEN
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        let _ = bump_connect_gen();
        action_gen
    })
}

pub(crate) fn action_is_current(gen: u64) -> bool {
    gen != 0 && gen == LIFECYCLE_ACTION_GEN.load(Ordering::SeqCst)
}

pub(crate) fn commit_if_action_current<T>(gen: u64, f: impl FnOnce() -> T) -> Option<T> {
    lifecycle_commit(|| action_is_current(gen).then(f))
}

pub(crate) fn commit_if_current<T>(gen: u64, f: impl FnOnce() -> T) -> Option<T> {
    lifecycle_commit(|| {
        if gen == 0 || gen != current_connect_gen() {
            None
        } else {
            Some(f())
        }
    })
}

/// Put a session back after long RPC outside the lock.
/// Only reinstalls when `gen` is still current and the slot is empty.
/// Returns true when the session was reinstalled under this gen.
pub(crate) fn put_session_back(mut session: CoreSession, gen: u64) -> bool {
    // A desynced IPC stream is as dead as an exited child: reinstalling it means
    // every later call misparses. Drop it so the next connect spawns a fresh Core.
    if session.child_exited() || session.client_broken() {
        let _ = session.stop_core_process();
        return false;
    }
    let mut pending = Some(session);
    let installed = lifecycle_commit(|| {
        if gen == 0 || gen != current_connect_gen() {
            return false;
        }
        match SESSION.lock() {
            Ok(mut g) if g.is_none() => {
                *g = pending.take();
                true
            }
            Ok(_) | Err(_) => false,
        }
    });
    if let Some(mut session) = pending {
        let _ = session.stop_core_process();
    }
    installed
}

/// 2A: apply proxy/spin only if this connect gen still owns the live session.
/// After OS proxy write, re-check gen — disconnect can win during the call; undo enable.
/// Reinstall a short-poll session only if gen still current and slot empty.
pub(crate) fn reinstall_poll_session(mut session: CoreSession, gen: u64) {
    // Poll RPCs use the same reuse contract as every other long call. A Core can
    // exit cleanly between the RPC and this point without the client latching a
    // protocol error; putting that child back would make the next poll treat a
    // dead process as a live session.
    if session.child_exited() || session.client_broken() {
        let _ = session.stop_core_process();
        return;
    }
    let mut pending = Some(session);
    lifecycle_commit(|| {
        if gen == 0 || gen != current_connect_gen() {
            return;
        }
        if let Ok(mut g) = SESSION.lock() {
            if g.is_none() {
                *g = pending.take();
            }
        }
    });
    if let Some(mut session) = pending {
        let _ = session.stop_core_process();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunnel_sm::{ConnectParams, Event, PeerEndpoint};
    use std::{
        net::{IpAddr, Ipv4Addr},
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc, Barrier,
        },
        time::Duration,
    };

    #[test]
    fn stale_generation_cannot_enter_lifecycle_commit() {
        let _g = tunnel_sm::test_lock();
        let _ = tunnel_sm::apply(Event::ResetIdle);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let connected = tunnel_sm::apply(Event::BeginConnect(ConnectParams {
            peer: PeerEndpoint {
                ip,
                port: 443,
                ips: vec![ip],
            },
            tun: false,
            mixed_port: 2080,
            tun_if: None,
            dns: Vec::new(),
        }));
        let _ = tunnel_sm::apply(Event::BeginDisconnect);
        let ran = AtomicBool::new(false);

        let result = commit_if_current(connected.gen, || ran.store(true, Ordering::SeqCst));

        assert!(result.is_none());
        assert!(!ran.load(Ordering::SeqCst));
        let _ = tunnel_sm::apply(Event::ResetIdle);
    }

    #[test]
    fn newer_admitted_action_invalidates_older_dispatch() {
        let _g = tunnel_sm::test_lock();
        let before = current_connect_gen();
        let older = admit_lifecycle_action();
        let newer = admit_lifecycle_action();
        let older_ran = AtomicBool::new(false);
        let newer_ran = AtomicBool::new(false);

        let old = commit_if_action_current(older, || older_ran.store(true, Ordering::SeqCst));
        let new = commit_if_action_current(newer, || newer_ran.store(true, Ordering::SeqCst));

        assert!(old.is_none());
        assert!(new.is_some());
        assert!(!older_ran.load(Ordering::SeqCst));
        assert!(newer_ran.load(Ordering::SeqCst));
        assert!(current_connect_gen().wrapping_sub(before) >= 2);
    }

    #[test]
    fn admitted_action_does_not_steal_unchanged_disconnect_state() {
        let _g = tunnel_sm::test_lock();
        let _ = tunnel_sm::apply(Event::ResetIdle);
        tunnel_sm::set_state(tunnel_sm::State::Connected);
        let older = admit_lifecycle_action();
        let disconnect =
            commit_if_action_current(older, || tunnel_sm::apply(Event::BeginDisconnect))
                .expect("older disconnect should still be current");

        let _newer = admit_lifecycle_action();

        assert_eq!(tunnel_sm::state(), tunnel_sm::State::Disconnecting);
        assert!(tunnel_sm::state_revision_is(disconnect.state_revision));
        let _ = tunnel_sm::apply(Event::ResetIdle);
    }

    #[test]
    fn committed_newer_connect_steals_disconnect_state() {
        let _g = tunnel_sm::test_lock();
        let _ = tunnel_sm::apply(Event::ResetIdle);
        tunnel_sm::set_state(tunnel_sm::State::Connected);
        let older = admit_lifecycle_action();
        let disconnect =
            commit_if_action_current(older, || tunnel_sm::apply(Event::BeginDisconnect))
                .expect("older disconnect should still be current");
        let newer = admit_lifecycle_action();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

        let connect = commit_if_action_current(newer, || {
            tunnel_sm::apply(Event::BeginConnect(ConnectParams {
                peer: PeerEndpoint {
                    ip,
                    port: 443,
                    ips: vec![ip],
                },
                tun: false,
                mixed_port: 2080,
                tun_if: None,
                dns: Vec::new(),
            }))
        })
        .expect("newer connect should be current");

        assert_eq!(connect.to, tunnel_sm::State::Connecting);
        assert!(!tunnel_sm::state_revision_is(disconnect.state_revision));
        let _ = tunnel_sm::apply(Event::ResetIdle);
    }

    #[test]
    fn lifecycle_commits_are_serialized() {
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
                lifecycle_commit(|| {
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
}
