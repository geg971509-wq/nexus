use crate::{
    core::session::SESSION, network_restore, session_access::current_connect_gen, tray_spin,
};

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

    let notes = network_restore::reconcile_if(
        || gen != 0 && gen == current_connect_gen(),
        use_sys_proxy,
        use_tun,
        port,
        dns_bootstrap,
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
