//! Generation-safe access to the single live Core IPC session.

use crate::{
    core::session::{CoreSession, SESSION},
    tunnel_sm,
};

pub(crate) fn bump_connect_gen() -> u64 {
    tunnel_sm::bump_gen()
}

pub(crate) fn current_connect_gen() -> u64 {
    tunnel_sm::current_gen()
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
    if gen == 0 || gen != current_connect_gen() {
        let _ = session.stop_core_process();
        return false;
    }
    match SESSION.lock() {
        Ok(mut g) if g.is_none() => {
            if gen == current_connect_gen() {
                *g = Some(session);
                true
            } else {
                let _ = session.stop_core_process();
                false
            }
        }
        Ok(_) => {
            let _ = session.stop_core_process();
            false
        }
        Err(_) => {
            let _ = session.stop_core_process();
            false
        }
    }
}

/// 2A: apply proxy/spin only if this connect gen still owns the live session.
/// After OS proxy write, re-check gen — disconnect can win during the call; undo enable.
/// Reinstall a short-poll session only if gen still current and slot empty.
pub(crate) fn reinstall_poll_session(mut session: CoreSession, gen: u64) {
    if gen == 0 || gen != current_connect_gen() || session.client_broken() {
        let _ = session.stop_core_process();
        return;
    }
    match SESSION.lock() {
        Ok(mut g) if g.is_none() && gen == current_connect_gen() => {
            *g = Some(session);
        }
        Ok(_) => {
            let _ = session.stop_core_process();
        }
        Err(_) => {
            let _ = session.stop_core_process();
        }
    }
}

