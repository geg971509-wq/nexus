//! Startup-only repair for network settings left by an abnormal Nexus exit.
//!
//! The Qt host acquires its QLockFile before calling this module, so a pending
//! recovery file belongs to an abandoned previous process.

use crate::{core::session::CoreSession, network_restore};
use std::ffi::{c_char, CString};

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

fn into_c_string(body: String) -> *mut c_char {
    let bytes: Vec<u8> = body.into_bytes().into_iter().filter(|&b| b != 0).collect();
    CString::new(bytes)
        .unwrap_or_else(|_| CString::from_vec_with_nul(b"{\"error\":\"json\"}\0".to_vec()).unwrap())
        .into_raw()
}

/// The host calls this after acquiring the single-instance lock and before
/// `nexus_init`/QML. The returned pointer follows the same ownership convention
/// as `nexus_invoke` and must be released with `nexus_free`.
#[no_mangle]
pub extern "C" fn nexus_recover_startup() -> *mut c_char {
    let body = match recover_pending_network_state() {
        Ok(notes) => serde_json::json!({"ok": true, "notes": notes}).to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    };
    into_c_string(body)
}
