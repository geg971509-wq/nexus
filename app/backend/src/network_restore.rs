//! Transactional preservation of macOS Proxy/PAC/DNS state.
//!
//! This module owns lifecycle semantics only. `macos` translates state to and
//! from `networksetup`; `state` owns the private crash-recovery file.

mod macos;
mod state;

use crate::sys;
use macos::{
    capture_dns, capture_proxy, disable_automatic_proxy, restore_dns_snapshot,
    restore_proxy_snapshot,
};
use state::{load_state, recovery_path, save_state};
use std::sync::Mutex;

static RECOVERY_LOCK: Mutex<()> = Mutex::new(());

fn apply_proxy_locked(network: &sys::SystemNetworkChange, port: u16) -> Result<String, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    if state.proxy.is_none() {
        state.proxy = Some(capture_proxy()?);
        // Persist before the first OS mutation. If the process dies on the next
        // instruction, startup recovery still has the original values.
        save_state(&path, &state)?;
    }
    let snapshot = state.proxy.clone().unwrap_or_default();
    let applied =
        disable_automatic_proxy(&snapshot).and_then(|_| network.set_system_proxy(true, port));
    match applied {
        Ok(note) => Ok(note),
        Err(e) => match restore_proxy_snapshot(&snapshot) {
            Ok(_) => {
                state.proxy = None;
                if let Err(save_err) = save_state(&path, &state) {
                    return Err(format!(
                        "apply system proxy failed: {e}; rollback succeeded but recovery cleanup failed: {save_err}"
                    ));
                }
                Err(format!(
                    "apply system proxy failed: {e}; original proxy/PAC restored"
                ))
            }
            Err(restore_err) => Err(format!(
                "apply system proxy failed: {e}; rollback also failed: {restore_err}"
            )),
        },
    }
}

fn apply_dns_locked(
    network: &sys::SystemNetworkChange,
    servers: &[String],
) -> Result<String, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    if state.dns.is_none() {
        state.dns = Some(capture_dns()?);
        save_state(&path, &state)?;
    }
    let snapshot = state.dns.clone().unwrap_or_default();
    match network.set_system_dns_bootstrap(true, servers) {
        Ok(note) => Ok(note),
        Err(e) => match restore_dns_snapshot(&snapshot) {
            Ok(_) => {
                state.dns = None;
                if let Err(save_err) = save_state(&path, &state) {
                    return Err(format!(
                        "apply system DNS failed: {e}; rollback succeeded but recovery cleanup failed: {save_err}"
                    ));
                }
                Err(format!(
                    "apply system DNS failed: {e}; original DNS restored"
                ))
            }
            Err(restore_err) => Err(format!(
                "apply system DNS failed: {e}; rollback also failed: {restore_err}"
            )),
        },
    }
}

fn restore_proxy_locked() -> Result<Option<String>, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    let Some(snapshot) = state.proxy.clone() else {
        return Ok(None);
    };
    let note = restore_proxy_snapshot(&snapshot)?;
    state.proxy = None;
    save_state(&path, &state)?;
    Ok(Some(note))
}

fn restore_dns_locked() -> Result<Option<String>, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    let Some(snapshot) = state.dns.clone() else {
        return Ok(None);
    };
    let note = restore_dns_snapshot(&snapshot)?;
    state.dns = None;
    save_state(&path, &state)?;
    Ok(Some(note))
}

fn restore_all_locked() -> Result<Vec<String>, String> {
    let _guard = RECOVERY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = recovery_path();
    let mut state = load_state(&path)?;
    let mut notes = Vec::new();
    let mut failures = Vec::new();

    if let Some(snapshot) = state.proxy.clone() {
        match restore_proxy_snapshot(&snapshot) {
            Ok(note) => {
                state.proxy = None;
                notes.push(note);
            }
            Err(e) => failures.push(e),
        }
    }
    if let Some(snapshot) = state.dns.clone() {
        match restore_dns_snapshot(&snapshot) {
            Ok(note) => {
                state.dns = None;
                notes.push(note);
            }
            Err(e) => failures.push(e),
        }
    }
    // Successful categories are cleared even if another category failed. The
    // file retains only the exact work still required on the next retry.
    save_state(&path, &state)?;
    if failures.is_empty() {
        Ok(notes)
    } else {
        Err(failures.join(" · "))
    }
}

/// Reconcile post-Start OS state in one serialized network transaction. A false
/// feature flag means “restore Nexus ownership if present”, never “force the
/// user's system setting off”.
pub(crate) fn reconcile_if(
    is_current: impl FnOnce() -> bool,
    use_system_proxy: bool,
    use_tun: bool,
    port: u16,
    dns_servers: &[String],
) -> Option<Result<Vec<String>, String>> {
    sys::with_system_network_change_if(is_current, |network| {
        let mut notes = Vec::new();
        if use_system_proxy {
            notes.push(apply_proxy_locked(network, port)?);
        } else if let Some(note) = restore_proxy_locked()? {
            notes.push(note);
        }
        if use_tun {
            notes.push(apply_dns_locked(network, dns_servers)?);
        } else if let Some(note) = restore_dns_locked()? {
            notes.push(note);
        }
        Ok(notes)
    })
}

pub(crate) fn apply_proxy(port: u16) -> Result<String, String> {
    sys::with_system_network_change(|| apply_proxy_locked(&sys::SystemNetworkChange))
}

pub(crate) fn restore_proxy() -> Result<Option<String>, String> {
    sys::with_system_network_change(restore_proxy_locked)
}

pub(crate) fn restore_all_if(
    is_current: impl FnOnce() -> bool,
) -> Option<Result<Vec<String>, String>> {
    sys::with_system_network_change_if(is_current, |_| restore_all_locked())
}

pub(crate) fn restore_all() -> Result<Vec<String>, String> {
    sys::with_system_network_change(restore_all_locked)
}

pub(crate) fn has_pending() -> bool {
    recovery_path().is_file()
}
