use crate::{
    core::session::SESSION,
    firewall, network_restore,
    session_access::{
        action_is_current, bump_connect_gen, commit_if_action_current, commit_if_current,
        current_connect_gen,
    },
    tray_spin, tun_if, tunnel_sm,
};

pub(super) fn fail_connecting(
    action_gen: u64,
    gen: u64,
    p: &tunnel_sm::ConnectParams,
    msg: String,
) -> String {
    let mut reported = msg.clone();
    let Some((true, mut session)) = commit_if_current(gen, || {
        if tunnel_sm::state() != tunnel_sm::State::Connecting {
            return (false, None);
        }
        let session = SESSION.lock().ok().and_then(|mut g| g.take());
        let _ = bump_connect_gen();
        let tr = tunnel_sm::apply(tunnel_sm::Event::Fail(msg));
        let _ = firewall::apply(firewall::policy_from_sm(tr.to, Some(p)));
        tray_spin::set_spinning(false);
        (true, session)
    }) else {
        return "connect superseded".into();
    };
    if let Some(s) = session.as_mut() {
        let _ = s.stop_rpc();
        let _ = s.stop_core_process();
    }
    if let Some(Err(e)) = network_restore::restore_all_if(|| action_is_current(action_gen)) {
        reported.push_str(&format!("; restore system network: {e}"));
    }
    reported
}

pub(super) fn fail_connected(
    action_gen: u64,
    gen: u64,
    p: &tunnel_sm::ConnectParams,
    msg: String,
) -> bool {
    let Some((true, mut session)) = commit_if_action_current(action_gen, || {
        if gen != current_connect_gen() || tunnel_sm::state() != tunnel_sm::State::Connected {
            return (false, None);
        }
        let session = SESSION.lock().ok().and_then(|mut g| g.take());
        let _ = bump_connect_gen();
        let tr = tunnel_sm::apply(tunnel_sm::Event::Fail(msg));
        let _ = firewall::apply(firewall::policy_from_sm(tr.to, Some(p)));
        tray_spin::set_spinning(false);
        (true, session)
    }) else {
        return false;
    };
    if let Some(s) = session.as_mut() {
        let _ = s.stop_rpc();
        let _ = s.stop_core_process();
    }
    let _ = network_restore::restore_all_if(|| action_is_current(action_gen));
    true
}

pub(super) fn spawn_tun_if_rebind(
    action_gen: u64,
    gen: u64,
    params: tunnel_sm::ConnectParams,
    before: Vec<String>,
) {
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        {
            for _ in 0..100 {
                if gen != current_connect_gen() || tunnel_sm::state() != tunnel_sm::State::Connected
                {
                    return;
                }
                if let Some(name) = tun_if::detect_nexus_tun_ifname(1, &before) {
                    if params.tun_if.as_deref() == Some(name.as_str()) && tun_if::if_exists(&name) {
                        return;
                    }
                    let mut p = params.clone();
                    p.tun_if = Some(name);
                    let applied = commit_if_current(gen, || {
                        if tunnel_sm::state() != tunnel_sm::State::Connected {
                            return Ok::<bool, String>(false);
                        }
                        tunnel_sm::update_tun_if(p.tun_if.clone());
                        firewall::apply(firewall::policy_from_sm(
                            tunnel_sm::State::Connected,
                            Some(&p),
                        ))?;
                        Ok::<bool, String>(true)
                    });
                    if let Some(Err(e)) = applied {
                        let _ = fail_connected(
                            action_gen,
                            gen,
                            &p,
                            format!("firewall tun rebind: {e}"),
                        );
                    }
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if gen != current_connect_gen() || tunnel_sm::state() != tunnel_sm::State::Connected {
                return;
            }
            let planned_live = params
                .tun_if
                .as_deref()
                .map(tun_if::if_exists)
                .unwrap_or(false);
            if !planned_live {
                let _ = fail_connected(
                    action_gen,
                    gen,
                    &params,
                    "firewall tun rebind: ifname timeout".into(),
                );
            }
        }
    });
}
