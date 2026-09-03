use super::side_effects::clear_dns_bootstrap_with;
use crate::{
    core::session::{CoreSession, SESSION},
    defaults::MIXED_PORT,
    firewall, runtime,
    session_access::{commit_if_current, current_connect_gen, reinstall_poll_session},
    sys, tray_spin, tunnel_sm,
};

pub(crate) async fn session_status() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use crate::data::store::Store;
        let st = Store::load();
        let mut rpc_running = false;
        let mut profile_id = -1i32;
        let mut has_session = false;
        let (taken, gen) = match SESSION.lock() {
            Ok(mut g) => {
                let gen = current_connect_gen();
                (g.take(), gen)
            }
            Err(_) => (None, 0),
        };
        if let Some(mut s) = taken {
            has_session = true;
            if let Ok((r, pid)) = s.query_state() {
                rpc_running = r;
                profile_id = pid;
            }
            reinstall_poll_session(s, gen);
        }
        let process_alive = CoreSession::core_process_alive();
        let mixed_open = CoreSession::mixed_port_open(MIXED_PORT);
        let live = rpc_running || (process_alive && mixed_open);
        let mut firewall_err: Option<String> = None;
        let mut system_proxy_err: Option<String> = None;
        let mut system_dns_err: Option<String> = None;
        let core_lost = commit_if_current(gen, || {
            let sm = tunnel_sm::state();
            if !live
                && matches!(
                    sm,
                    tunnel_sm::State::Connected | tunnel_sm::State::Connecting
                )
            {
                let tr = tunnel_sm::apply(tunnel_sm::Event::CoreLost);
                let params = tr.params.or_else(tunnel_sm::last_params);
                firewall_err =
                    firewall::apply(firewall::policy_from_sm(tr.to, params.as_ref())).err();
                tray_spin::set_spinning(false);
                true
            } else {
                tray_spin::set_spinning(live);
                false
            }
        })
        .unwrap_or(false);
        if core_lost {
            if let Some((proxy_error, dns_error)) = sys::with_system_network_change_if(
                || gen != 0 && gen == current_connect_gen(),
                |network| {
                    let proxy_error = network.set_system_proxy(false, MIXED_PORT).err();
                    let dns_error = clear_dns_bootstrap_with(network).err();
                    (proxy_error, dns_error)
                },
            ) {
                system_proxy_err = proxy_error;
                system_dns_err = dns_error;
            }
        }
        Ok(serde_json::json!({
            "running": live,
            "rpc_running": rpc_running,
            "has_session": has_session,
            "process_alive": process_alive,
            "mixed_open": mixed_open,
            "profile_id": profile_id,
            "tun": st.tun,
            "system_proxy": st.system_proxy,
            "tunnel_state": tunnel_sm::state().as_str(),
            "firewall_error": firewall_err.or_else(tunnel_sm::last_error),
            "system_proxy_error": system_proxy_err,
            "system_dns_error": system_dns_err,
        }))
    })
    .await
    .map_err(|e| format!("session_status join: {e}"))?
}
