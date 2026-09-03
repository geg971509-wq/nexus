use crate::{
    core::session::{CoreSession, SESSION},
    defaults::MIXED_PORT,
    diagnostics::firewall_status_json,
    firewall,
    network_restore,
    session_access::{
        action_is_current, commit_if_action_current, current_connect_gen, lifecycle_commit,
    },
    tray_spin, tunnel_sm,
};

pub(crate) fn disconnect_selected_sync(action_gen: u64) -> Result<serde_json::Value, String> {
    let (disconnect_gen, disconnect_state_revision, blocked_err, mut session) =
        commit_if_action_current(action_gen, || {
            let tr = tunnel_sm::apply(tunnel_sm::Event::BeginDisconnect);
            let (peer, mixed_port, dns) = match tr.params {
                Some(p) => (Some(p.peer), p.mixed_port, p.dns),
                None => (None, MIXED_PORT, Vec::new()),
            };
            let blocked_err = firewall::apply(firewall::Policy::Blocked {
                peer,
                mixed_port,
                dns,
            })
            .err();
            let session = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take();
            CoreSession::kill_stray_cores(session.as_ref().and_then(CoreSession::child_pid));
            (tr.gen, tr.state_revision, blocked_err, session)
        })
        .ok_or_else(|| "disconnect superseded".to_string())?;

    let (stop_err, pid) = if let Some(s) = session.as_mut() {
        let stop_err = s.stop_rpc().unwrap_or_else(Some);
        let (_, pid) = s.query_state().unwrap_or((false, -1));
        let _ = s.stop_core_process();
        (stop_err, pid)
    } else {
        (None, -1i32)
    };

    let mut notes = match network_restore::restore_all_if(|| {
        tunnel_sm::state() == tunnel_sm::State::Disconnecting
            && tunnel_sm::state_revision_is(disconnect_state_revision)
    }) {
        Some(Ok(notes)) => notes,
        Some(Err(e)) => vec![format!("restore system network: {e}")],
        None => Vec::new(),
    };
    if let Some(e) = &blocked_err {
        notes.push(format!("firewall blocked: {e}"));
    }

    lifecycle_commit(|| {
        if tunnel_sm::state() == tunnel_sm::State::Disconnecting
            && tunnel_sm::state_revision_is(disconnect_state_revision)
        {
            tray_spin::set_spinning(false);
            let _ = tunnel_sm::apply(tunnel_sm::Event::ResetIdle);
            firewall::reset_best_effort();
        }

        if !action_is_current(action_gen) || disconnect_gen != current_connect_gen() {
            return Err("disconnect superseded".to_string());
        }

        let running = CoreSession::core_process_alive();
        let proxy_note = Some(notes.join(" · "));
        Ok(serde_json::json!({
            "stopped": !running,
            "stop_error": stop_err,
            "running": running,
            "profile_id": pid,
            "proxy_note": proxy_note,
            "firewall_error": blocked_err,
            "tunnel_state": tunnel_sm::state().as_str(),
            "firewall": firewall_status_json(),
        }))
    })
}
