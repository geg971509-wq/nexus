use crate::{core::session::SESSION, session_access::current_connect_gen, sys, tray_spin};
use std::sync::atomic::{AtomicBool, Ordering};

static DNS_BOOTSTRAP_SET: AtomicBool = AtomicBool::new(false);

pub(super) fn clear_dns_bootstrap_with(
    network: &sys::SystemNetworkChange,
) -> Result<Option<String>, String> {
    if DNS_BOOTSTRAP_SET.swap(false, Ordering::SeqCst) {
        match network.set_system_dns_bootstrap(false, &[]) {
            Ok(message) => Ok(Some(message)),
            Err(e) => {
                DNS_BOOTSTRAP_SET.store(true, Ordering::SeqCst);
                Err(e)
            }
        }
    } else {
        Ok(None)
    }
}

pub(super) fn apply_post_start_side_effects(
    gen: u64,
    use_sys_proxy: bool,
    use_tun: bool,
    port: u16,
    dns_bootstrap: &[String],
) -> Result<String, String> {
    if gen == 0 || gen != current_connect_gen() {
        return Err("connect superseded before system side effects".into());
    }
    let still_ours = SESSION.lock().ok().map(|g| g.is_some()).unwrap_or(false);
    if !still_ours || gen != current_connect_gen() {
        return Err("session gone before system side effects".into());
    }
    let notes = sys::with_system_network_change_if(
        || gen != 0 && gen == current_connect_gen(),
        |network| -> Result<Vec<String>, String> {
            let mut notes = Vec::new();
            let proxy_note = if use_sys_proxy {
                network
                    .set_system_proxy(true, port)
                    .map_err(|e| format!("system proxy failed: {e}"))?
            } else {
                match network.set_system_proxy(false, port) {
                    Ok(m) => m,
                    Err(e) => format!("clear system proxy: {e}"),
                }
            };
            notes.push(proxy_note);
            if use_tun {
                DNS_BOOTSTRAP_SET.store(true, Ordering::SeqCst);
                let dns_note = network
                    .set_system_dns_bootstrap(true, dns_bootstrap)
                    .map_err(|e| format!("system dns failed: {e}"))?;
                notes.push(dns_note);
            } else if let Some(dns_note) =
                clear_dns_bootstrap_with(network).map_err(|e| format!("clear system dns: {e}"))?
            {
                notes.push(dns_note);
            }
            Ok(notes)
        },
    )
    .ok_or_else(|| "connect superseded before system side effects".to_string())??;
    if gen != current_connect_gen() {
        return Err("connect superseded during system side effects".into());
    }
    let still_ours = SESSION.lock().ok().map(|g| g.is_some()).unwrap_or(false);
    if !still_ours {
        return Err("session gone during system side effects".into());
    }
    tray_spin::set_spinning(true);
    Ok(notes.join(" · "))
}
