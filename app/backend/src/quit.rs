//! One idempotent application quit and teardown path.

use crate::{
    core::session::{CoreSession, SESSION},
    defaults::MIXED_PORT,
    firewall,
    session_access::bump_connect_gen,
    sys, tray_spin,
    tunnel_runtime::clear_dns_bootstrap_if_set,
    tunnel_sm,
};

/// Single quit/teardown path: stop Core + always best-effort clear OS proxy at MIXED_PORT.
/// Used by app_quit, tray quit, and Exit (after confirm). Idempotent.
/// 3A: quit → Reset (session kill-switch ends with app; not post-quit lockdown).
pub(crate) fn teardown_session() {
    let _ = bump_connect_gen();
    tray_spin::set_spinning(false);
    let _ = tunnel_sm::apply(tunnel_sm::Event::ResetIdle);
    firewall::reset_best_effort();
    // Always clear proxy; DNS only if this session set bootstrap (1A).
    let _ = sys::set_system_proxy(false, MIXED_PORT);
    clear_dns_bootstrap_if_set();
    let _ = (|| -> Result<(), String> {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        if let Some(mut s) = g.take() {
            let _ = s.stop_rpc();
            let _ = s.stop_core_process();
        }
        CoreSession::kill_stray_cores(None);
        Ok(())
    })();
}

/// Same live rule as session_status (4A/5A): never treat mixed-port alone as live.
/// RPC running, or (Core process ∧ mixed open). Unrelated :2080 listener is not a tunnel.
pub(crate) fn tunnel_is_live() -> bool {
    if let Ok(mut g) = SESSION.lock() {
        if let Some(s) = g.as_mut() {
            if let Ok((running, _)) = s.query_state() {
                if running {
                    return true;
                }
            }
        }
    }
    CoreSession::core_process_alive() && CoreSession::mixed_port_open(MIXED_PORT)
}

/// force=true: UI already warned → teardown. force=false: refuse while live so
/// the Qt host can route every entry point through the localized QML dialog.
/// Returns true when the host should exit. Qt path uses this without AppHandle.
pub(crate) fn prepare_quit(force: bool) -> bool {
    if !force && tunnel_is_live() {
        return false;
    }
    teardown_session();
    true
}
