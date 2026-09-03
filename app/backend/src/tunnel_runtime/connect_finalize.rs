use super::{
    connect::PreparedConnect,
    recovery::{fail_connected, fail_connecting, spawn_tun_if_rebind},
    side_effects::apply_post_start_side_effects,
};
use crate::{
    core::session::SESSION,
    diagnostics::firewall_status_json,
    firewall,
    session_access::{commit_if_current, current_connect_gen},
    tun_if, tunnel_sm,
};

pub(super) fn finish_connect(
    prepared: PreparedConnect,
    running: bool,
    profile_id: i32,
) -> Result<serde_json::Value, String> {
    let tun_if = if prepared.use_tun {
        prepared
            .planned_tun_if
            .clone()
            .or_else(|| tun_if::detect_tun_ifname(&prepared.utun_before))
    } else {
        None
    };
    let mut params_connected = prepared.params.clone();
    params_connected.tun_if = tun_if;
    let connected_commit = commit_if_current(prepared.connect_gen, || {
        if tunnel_sm::state() != tunnel_sm::State::Connecting {
            return Ok::<bool, String>(false);
        }
        firewall::apply(firewall::policy_from_sm(
            tunnel_sm::State::Connected,
            Some(&params_connected),
        ))?;
        let tr = tunnel_sm::apply(tunnel_sm::Event::MarkConnected {
            tun_if: params_connected.tun_if.clone(),
        });
        Ok::<bool, String>(tr.to == tunnel_sm::State::Connected)
    });
    match connected_commit {
        None | Some(Ok(false)) => {
            return Ok(serde_json::json!({
                "started": false,
                "start_error": "connect superseded",
                "running": false,
                "profile_id": profile_id,
                "listen_port": prepared.port,
                "proxy_note": "connected policy skipped: connect superseded",
                "tun": prepared.use_tun,
                "system_proxy": prepared.use_sys_proxy,
                "config": prepared.cfg,
            }));
        }
        Some(Err(e)) => {
            let start_error = fail_connecting(
                prepared.connect_gen,
                &params_connected,
                format!("firewall connected: {e}"),
            );
            return Ok(serde_json::json!({
                "started": false,
                "start_error": start_error,
                "running": false,
                "profile_id": profile_id,
                "listen_port": prepared.port,
                "proxy_note": "firewall connected policy failed",
                "tun": prepared.use_tun,
                "tun_if": params_connected.tun_if,
                "system_proxy": prepared.use_sys_proxy,
                "config": prepared.cfg,
                "tunnel_state": tunnel_sm::state().as_str(),
                "firewall": firewall_status_json(),
            }));
        }
        Some(Ok(true)) => {}
    }
    if prepared.use_tun {
        spawn_tun_if_rebind(
            prepared.action_gen,
            prepared.connect_gen,
            params_connected.clone(),
            prepared.utun_before.clone(),
        );
    }

    let proxy_note = match apply_post_start_side_effects(
        prepared.connect_gen,
        prepared.use_sys_proxy,
        prepared.use_tun,
        prepared.port,
        &prepared.dns_bootstrap,
    ) {
        Ok(note) => Some(note),
        Err(e) => {
            if current_connect_gen() != prepared.connect_gen {
                return Ok(serde_json::json!({
                    "started": false,
                    "start_error": "connect superseded",
                    "running": false,
                    "profile_id": profile_id,
                    "listen_port": prepared.port,
                    "proxy_note": e,
                    "tun": prepared.use_tun,
                    "system_proxy": prepared.use_sys_proxy,
                    "config": prepared.cfg,
                }));
            }
            let _ = fail_connected(
                prepared.action_gen,
                prepared.connect_gen,
                &params_connected,
                format!("post-start side effect: {e}"),
            );
            return Ok(serde_json::json!({
                "started": false,
                "start_error": e,
                "running": false,
                "profile_id": profile_id,
                "listen_port": prepared.port,
                "proxy_note": "post-start system side effect failed; connection rolled back",
                "tun": prepared.use_tun,
                "tun_if": params_connected.tun_if,
                "system_proxy": prepared.use_sys_proxy,
                "config": prepared.cfg,
                "tunnel_state": tunnel_sm::state().as_str(),
                "firewall": firewall_status_json(),
            }));
        }
    };
    let still = current_connect_gen() == prepared.connect_gen
        && SESSION.lock().ok().map(|g| g.is_some()).unwrap_or(false);
    if !still {
        if current_connect_gen() == prepared.connect_gen {
            let _ = fail_connected(
                prepared.action_gen,
                prepared.connect_gen,
                &params_connected,
                "session lost after connect".into(),
            );
        }
        return Ok(serde_json::json!({
            "started": false,
            "start_error": "connect superseded",
            "running": false,
            "profile_id": profile_id,
            "listen_port": prepared.port,
            "proxy_note": proxy_note,
            "tun": prepared.use_tun,
            "system_proxy": prepared.use_sys_proxy,
            "config": prepared.cfg,
        }));
    }

    Ok(serde_json::json!({
        "started": true,
        "start_error": null,
        "running": running,
        "profile_id": profile_id,
        "listen_port": prepared.port,
        "proxy_note": proxy_note,
        "tun": prepared.use_tun,
        "tun_if": params_connected.tun_if,
        "system_proxy": prepared.use_sys_proxy,
        "config": prepared.cfg,
        "tunnel_state": tunnel_sm::state().as_str(),
        "firewall": firewall_status_json(),
    }))
}
