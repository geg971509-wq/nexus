//! Startup-only repair for network settings left by an abnormal Nexus exit.
//!
//! The Qt host acquires its QLockFile before calling `nexus_init`, so this module
//! can treat a pending recovery file as abandoned by a previous process without
//! duplicating process-discovery heuristics here.

use crate::{core::session::CoreSession, network_restore};

/// Repair any Proxy/PAC/DNS transaction left by an abnormal exit before QML is
/// loaded. Failure is fatal to startup: accepting new connection actions while
/// an older recovery transaction is unresolved could destroy the only exact
/// snapshot of the user's original system settings.
pub(crate) fn recover_pending_network_state() -> Result<Vec<String>, String> {
    if !network_restore::has_pending() {
        return Ok(Vec::new());
    }
    let session_live = crate::core::session::SESSION
        .lock()
        .map_err(|e| e.to_string())?
        .is_some();
    if session_live {
        return Err("cannot recover network state while a Core session is owned".into());
    }

    // The previous GUI no longer owns the single-instance lock, so any remaining
    // NexusCore is orphaned. Stop it before restoring Proxy/DNS to prevent traffic
    // from racing the recovery transaction.
    CoreSession::kill_stray_cores(None);
    let notes = network_restore::restore_all()?;
    crate::firewall::reset_best_effort();
    Ok(notes)
}
