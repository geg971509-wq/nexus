//! Connection lifecycle, fail-closed side effects, and Core start/stop runtime.

use crate::{
    core::session::{CoreSession, SESSION},
    defaults::MIXED_PORT,
    diagnostics::firewall_status_json,
    firewall, runtime,
    session_access::{
        bump_connect_gen, current_connect_gen, put_session_back, reinstall_poll_session,
    },
    sys, tray_spin, tun_if, tunnel_sm,
};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

/// 1A: true only after this session set system DNS bootstrap (Tun path).
static DNS_BOOTSTRAP_SET: AtomicBool = AtomicBool::new(false);

/// Clear bootstrap DNS only if this session set it (1A: never wipe custom DNS).
pub(crate) fn clear_dns_bootstrap_if_set() {
    if DNS_BOOTSTRAP_SET.swap(false, AtomicOrdering::SeqCst) {
        let _ = sys::set_system_dns_bootstrap(false, &[]);
    }
}

fn apply_post_start_side_effects(
    gen: u64,
    use_sys_proxy: bool,
    use_tun: bool,
    port: u16,
    dns_bootstrap: &[String],
) -> Option<String> {
    if gen == 0 || gen != current_connect_gen() {
        return Some("skipped proxy: connect superseded".into());
    }
    let still_ours = SESSION
        .lock()
        .ok()
        .map(|g| g.is_some())
        .unwrap_or(false);
    if !still_ours || gen != current_connect_gen() {
        return Some("skipped proxy: session gone".into());
    }
    let mut notes: Vec<String> = Vec::new();
    let proxy_note = if use_sys_proxy {
        match sys::set_system_proxy(true, port) {
            Ok(m) => m,
            Err(e) => format!("system proxy failed: {e}"),
        }
    } else {
        match sys::set_system_proxy(false, port) {
            Ok(m) => m,
            Err(e) => format!("clear system proxy: {e}"),
        }
    };
    notes.push(proxy_note);
    // 1A: Tun only — set bootstrap DNS; non-Tun never touch system DNS.
    if use_tun {
        match sys::set_system_dns_bootstrap(true, dns_bootstrap) {
            Ok(m) => {
                DNS_BOOTSTRAP_SET.store(true, AtomicOrdering::SeqCst);
                notes.push(m);
            }
            Err(e) => notes.push(format!("system dns failed: {e}")),
        }
    }
    // TOCTOU close: if disconnect bumped gen during set_system_proxy, undo enable.
    if gen != current_connect_gen() {
        if use_sys_proxy {
            let _ = sys::set_system_proxy(false, port);
        }
        clear_dns_bootstrap_if_set();
        tray_spin::set_spinning(false);
        return Some("rolled back proxy: connect superseded".into());
    }
    let still_ours = SESSION
        .lock()
        .ok()
        .map(|g| g.is_some())
        .unwrap_or(false);
    if !still_ours {
        if use_sys_proxy {
            let _ = sys::set_system_proxy(false, port);
        }
        clear_dns_bootstrap_if_set();
        tray_spin::set_spinning(false);
        return Some("rolled back proxy: session gone".into());
    }
    tray_spin::set_spinning(true);
    Some(notes.join(" · "))
}

/// Boot / power sync: store chips + live Core (SESSION QueryState, or orphan process).
/// 4A: never treat mixed-port alone as live (unrelated listener on 2080).
/// 2A: Connected/Connecting + Core dead → CoreLost → firewall Blocked (keep peer).
pub(crate) async fn session_status() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use crate::data::store::Store;
        let st = Store::load();
        let mut rpc_running = false;
        let mut profile_id = -1i32;
        let mut has_session = false;
        // 6A: take session for short QueryState so poll/disconnect is not blocked.
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
        // Prefer RPC truth; else owned/orphan process + mixed; never mixed alone.
        let live = rpc_running || (process_alive && mixed_open);
        // 2A: SM still Connected/Connecting but Core gone → Error + Blocked (peer kept).
        let sm = tunnel_sm::state();
        let mut firewall_err: Option<String> = None;
        if !live
            && matches!(
                sm,
                tunnel_sm::State::Connected | tunnel_sm::State::Connecting
            )
        {
            let tr = tunnel_sm::apply(tunnel_sm::Event::CoreLost);
            let params = tr.params.or_else(tunnel_sm::last_params);
            // Core died with the tunnel up: if fail-closed does not land, the box is
            // wide open. Surfaced below so the UI can say so instead of showing Idle.
            firewall_err = firewall::apply(firewall::policy_from_sm(tr.to, params.as_ref())).err();
            clear_dns_bootstrap_if_set();
            tray_spin::set_spinning(false);
        } else {
            tray_spin::set_spinning(live);
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
        }))
    })
    .await
    .map_err(|e| format!("session_status join: {e}"))?
}

/// engine-aligned connect: BuildSingBoxConfig → Start(LoadConfigReq).
/// UI passes selected node share `link` or raw `outbound` JSON; no invent credentials.
/// Tun/system-proxy follow upstream `spmode_vpn` / `spmode_system_proxy` (UI chips → optional args → store).
pub(crate) fn connect_selected_sync(
    link: Option<String>,
    outbound: Option<serde_json::Value>,
    profile_id: Option<i32>,
    tun: Option<bool>,
    system_proxy: Option<bool>,
) -> Result<serde_json::Value, String> {
    use crate::core::session::{CoreSession, SESSION};
    use crate::data::generate::generate_with_outbound;
    use crate::data::share_link::parse_to_outbound;
    use crate::data::store::Store;

    // Start uses current checkbox state, not a stale disk flag.
    // Prefer explicit UI args; persist so next cold Start matches chips.
    // One read: config and PF must agree on the resolver list or PF blocks the
    // server the config just chose.
    let (use_tun, use_sys_proxy, dns_bootstrap) = Store::update(|st| {
        if let Some(v) = tun {
            st.tun = v;
        }
        if let Some(v) = system_proxy {
            st.system_proxy = v;
        }
        Ok((st.tun, st.system_proxy, st.dns_bootstrap()))
    })?;
    let port = MIXED_PORT;
    let pid = profile_id.unwrap_or(1);

    let ob = if let Some(v) = outbound {
        if v.get("type").and_then(|t| t.as_str()).is_none() {
            return Err("outbound missing type".into());
        }
        v
    } else if let Some(lk) = link.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        parse_to_outbound(lk)?
    } else {
        return Err(
            "selected node has no share link — import a subscription or paste a vless/trojan/ss link"
                .into(),
        );
    };

    // generate.cpp: tun inbound only if spmode_vpn
    let mut cfg = generate_with_outbound(ob.clone(), port, use_tun, &dns_bootstrap);

    // mac Tun: pin next free utunN into Core config + PF Connected.
    // Detection-only (172.19 / new-utun / stale core.log) left pure-Tun dead under
    // fail-closed: no reliable `pass quick on utun…` while sysproxy still worked.
    #[cfg(target_os = "macos")]
    let planned_tun_if: Option<String> = if use_tun {
        let name = tun_if::next_free_utun().ok_or_else(|| "no free utun for Tun".to_string())?;
        if let Some(arr) = cfg.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
            for ib in arr.iter_mut() {
                if ib.get("type").and_then(|t| t.as_str()) == Some("tun") {
                    if let Some(obj) = ib.as_object_mut() {
                        obj.insert(
                            "interface_name".into(),
                            serde_json::Value::String(name.clone()),
                        );
                    }
                }
            }
        }
        Some(name)
    } else {
        None
    };
    let json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;

    // Tun: setuid Core before LoadConfig (upstream profile_start elevation).
    // osascript password sheet runs here if setuid copy missing — outside SESSION.
    // The path is kept so the recycle below cannot re-enter elevation under the lock.
    let privileged_core = if use_tun {
        Some(CoreSession::ensure_privileged_core()?)
    } else {
        None
    };

    // Firewall: helper ready → peer → Connecting before Core Start (C2 L3/L5).
    firewall::require_ready_for_connect()?;
    // Residual Blocked (esp. peer-less) can leave getaddrinfo dead if DNS was closed.
    // Soft-open once so hostname peers (VMess CDN etc.) can resolve, then Connecting.
    let peer = match firewall::peer_from_outbound(&ob) {
        Ok(p) => p,
        Err(e) => {
            let _ = firewall::apply(firewall::Policy::Reset);
            firewall::peer_from_outbound(&ob).map_err(|e2| format!("{e}; after reset: {e2}"))?
        }
    };
    let connect_params = tunnel_sm::ConnectParams {
        peer: peer.clone(),
        tun: use_tun,
        mixed_port: port,
        // Planned ifname is known before Start; Connected uses it so pass-on-utun
        // is not gated on post-Start detection races.
        tun_if: planned_tun_if.clone(),
        dns: dns_bootstrap.clone(),
    };
    let tr = tunnel_sm::apply(tunnel_sm::Event::BeginConnect(connect_params.clone()));
    if let Err(e) = firewall::apply(firewall::policy_from_sm(tr.to, Some(&connect_params))) {
        // Connecting never applied — safe to Reset (network open).
        let _ = tunnel_sm::apply(tunnel_sm::Event::Fail(e.clone()));
        let _ = firewall::apply(firewall::Policy::Reset);
        return Err(format!("firewall connecting: {e}"));
    }
    // After Connecting is live: failures stay fail-closed (Blocked), not Reset.
    // Mullvad ErrorState keeps lockdown until user disconnects.
    let fail_closed = |msg: String, params: &tunnel_sm::ConnectParams| {
        let tr = tunnel_sm::apply(tunnel_sm::Event::Fail(msg.clone()));
        let _ = firewall::apply(firewall::policy_from_sm(tr.to, Some(params)));
        msg
    };

    // Snapshot utun names before Start — gvisor often never assigns 172.19.0.1
    // on the kernel iface; we detect by new utun + core.log "started at utunN".
    #[cfg(target_os = "macos")]
    let utun_before = if use_tun {
        tun_if::list_utun_names()
    } else {
        Vec::new()
    };
    // 1A: take session out of SESSION so Start (≤60s) does not block poll/disconnect.
    // Setup under lock; mint connect gen so mid-Start disconnect invalidates put_session_back.
    let (mut session, connect_gen) = {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        if let Some(s) = g.as_mut() {
            if s.child_exited() {
                let _ = s.stop_core_process();
                *g = None;
            }
        }
        if g.is_none() {
            let bin = CoreSession::resolve_core_binary();
            if !bin.is_file() {
                let msg = format!("NexusCore not found at {}", bin.display());
                return Err(fail_closed(msg, &connect_params));
            }
            match CoreSession::start(&bin) {
                Ok(s) => *g = Some(s),
                Err(e) => {
                    return Err(fail_closed(e.to_string(), &connect_params));
                }
            }
        }
        let s = g.as_mut().unwrap();
        if let Some(bin) = privileged_core.as_deref() {
            let priv_now = s.is_privileged().unwrap_or(false);
            if !priv_now {
                if let Err(e) = s.recycle_privileged(bin) {
                    return Err(fail_closed(e, &connect_params));
                }
            }
        }
        if let Ok((running, _)) = s.query_state() {
            if running {
                let _ = s.stop_rpc();
            }
        }
        let gen = bump_connect_gen();
        let session = match g.take() {
            Some(s) => s,
            None => {
                return Err(fail_closed(
                    "session vanished before start".into(),
                    &connect_params,
                ));
            }
        };
        (session, gen)
    };

    let mut start_err = match session.start_rpc(&json, pid) {
        Ok(e) => e,
        Err(e) => {
            let _ = put_session_back(session, connect_gen);
            return Err(fail_closed(e, &connect_params));
        }
    };
    // Orphan Core / stale bbolt → initialize cache-file: timeout. One recovery:
    // kill strays, drop cache.db, Stop, Start again.
    if let Some(ref e) = start_err {
        let el = e.to_ascii_lowercase();
        if el.contains("cache-file") || el.contains("cache.db") || el.contains("timeout") {
            let keep = session.child_pid();
            CoreSession::kill_stray_cores(keep);
            let _ = session.stop_rpc();
            let _ = std::fs::remove_file(CoreSession::cache_db_path());
            start_err = match session.start_rpc(&json, pid) {
                Ok(e) => e,
                Err(e) => {
                    let _ = put_session_back(session, connect_gen);
                    return Err(fail_closed(e, &connect_params));
                }
            };
        }
    }
    if start_err.is_some() {
        let _ = put_session_back(session, connect_gen);
        let msg = start_err.clone().unwrap_or_else(|| "start failed".into());
        let _ = fail_closed(msg.clone(), &connect_params);
        return Ok(serde_json::json!({
            "started": false,
            "start_error": start_err,
            "config": cfg,
            "profile_id": pid,
            "tun": use_tun,
            "system_proxy": use_sys_proxy,
            "tunnel_state": tunnel_sm::state().as_str(),
        }));
    }
    let (running, qpid) = session.query_state().unwrap_or((false, -1));
    // 2A arch: started only if this gen still owns a reinstalled session.
    let owned = put_session_back(session, connect_gen);
    if !owned {
        // Disconnect bumps gen and owns FW teardown. Other races stay fail-closed.
        if current_connect_gen() == connect_gen {
            let _ = fail_closed("session not owned after start".into(), &connect_params);
        }
        return Ok(serde_json::json!({
            "started": false,
            "start_error": "connect superseded",
            "running": false,
            "profile_id": qpid,
            "listen_port": port,
            "proxy_note": "session discarded: connect superseded",
            "tun": use_tun,
            "system_proxy": use_sys_proxy,
            "config": cfg,
        }));
    }

    // Prefer planned ifname (already in Core config). Fall back to detect only
    // if pin failed or Core picked a different utun.
    let tun_if = if use_tun {
        planned_tun_if
            .clone()
            .or_else(|| tun_if::detect_tun_ifname(&utun_before))
    } else {
        None
    };
    let mut params_connected = connect_params.clone();
    params_connected.tun_if = tun_if.clone();
    // Connected only if the fail-closed policy applies.
    // 1A/6A: SM Connected only via MarkConnected after firewall OK (no set_state).
    if let Err(e) = firewall::apply(firewall::policy_from_sm(
        tunnel_sm::State::Connected,
        Some(&params_connected),
    )) {
        // Tear down Core; stay Blocked rather than fake Connected without PF.
        let _ = SESSION.lock().ok().and_then(|mut g| {
            if let Some(mut s) = g.take() {
                let _ = s.stop_rpc();
                let _ = s.stop_core_process();
            }
            Some(())
        });
        CoreSession::kill_stray_cores(None);
        let tr = tunnel_sm::apply(tunnel_sm::Event::Fail(format!("firewall connected: {e}")));
        let _ = firewall::apply(firewall::policy_from_sm(tr.to, Some(&params_connected)));
        let _ = sys::set_system_proxy(false, port);
        clear_dns_bootstrap_if_set();
        tray_spin::set_spinning(false);
        return Ok(serde_json::json!({
            "started": false,
            "start_error": format!("firewall connected: {e}"),
            "running": false,
            "profile_id": qpid,
            "listen_port": port,
            "proxy_note": "firewall connected policy failed",
            "tun": use_tun,
            "tun_if": params_connected.tun_if,
            "system_proxy": use_sys_proxy,
            "config": cfg,
            "tunnel_state": tr.to.as_str(),
            "firewall": firewall_status_json(),
        }));
    }
    let _ = tunnel_sm::apply(tunnel_sm::Event::MarkConnected {
        tun_if: params_connected.tun_if.clone(),
    });
    // Verify live utun matches planned name; rebind if Core/log differ.
    if use_tun {
        spawn_tun_if_rebind(connect_gen, params_connected.clone(), utun_before.clone());
    }

    // cycle2 2A: proxy/spin only if this connect gen still owns the live session.
    let proxy_note = apply_post_start_side_effects(
        connect_gen,
        use_sys_proxy,
        use_tun,
        port,
        &dns_bootstrap,
    );
    // If proxy path rolled back because gen died mid-call, still report not started.
    let still = current_connect_gen() == connect_gen
        && SESSION.lock().ok().map(|g| g.is_some()).unwrap_or(false);
    if !still {
        if current_connect_gen() == connect_gen {
            let _ = fail_closed("session lost after connect".into(), &params_connected);
        }
        return Ok(serde_json::json!({
            "started": false,
            "start_error": "connect superseded",
            "running": false,
            "profile_id": qpid,
            "listen_port": port,
            "proxy_note": proxy_note,
            "tun": use_tun,
            "system_proxy": use_sys_proxy,
            "config": cfg,
        }));
    }

    Ok(serde_json::json!({
        "started": true,
        "start_error": null,
        "running": running,
        "profile_id": qpid,
        "listen_port": port,
        "proxy_note": proxy_note,
        "tun": use_tun,
        "tun_if": params_connected.tun_if,
        "system_proxy": use_sys_proxy,
        "config": cfg,
        "tunnel_state": tunnel_sm::state().as_str(),
        "firewall": firewall_status_json(),
    }))
}

/// Tear down like connect-time Connected apply fail (Blocked, not Reset).
fn fail_closed_from_rebind(p: &tunnel_sm::ConnectParams, msg: String) {
    let _ = SESSION.lock().ok().and_then(|mut g| {
        if let Some(mut s) = g.take() {
            let _ = s.stop_rpc();
            let _ = s.stop_core_process();
        }
        Some(())
    });
    CoreSession::kill_stray_cores(None);
    let _ = bump_connect_gen();
    let tr = tunnel_sm::apply(tunnel_sm::Event::Fail(msg));
    let _ = firewall::apply(firewall::policy_from_sm(tr.to, Some(p)));
    let _ = sys::set_system_proxy(false, p.mixed_port);
    clear_dns_bootstrap_if_set();
    tray_spin::set_spinning(false);
}

/// Background: if planned ifname missing or Core used another utun, rebind pass-on-utun.
fn spawn_tun_if_rebind(gen: u64, params: tunnel_sm::ConnectParams, before: Vec<String>) {
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        {
            for _ in 0..100 {
                if gen != current_connect_gen() || tunnel_sm::state() != tunnel_sm::State::Connected {
                    return;
                }
                if let Some(name) = tun_if::detect_nexus_tun_ifname(1, &before) {
                    // Skip rebind if already matching a live planned/current ifname.
                    if params.tun_if.as_deref() == Some(name.as_str()) && tun_if::if_exists(&name) {
                        return;
                    }
                    let mut p = params.clone();
                    p.tun_if = Some(name);
                    if gen != current_connect_gen() || tunnel_sm::state() != tunnel_sm::State::Connected
                    {
                        return;
                    }
                    tunnel_sm::update_tun_if(p.tun_if.clone());
                    if let Err(e) = firewall::apply(firewall::policy_from_sm(
                        tunnel_sm::State::Connected,
                        Some(&p),
                    )) {
                        fail_closed_from_rebind(&p, format!("firewall tun rebind: {e}"));
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
                fail_closed_from_rebind(&params, "firewall tun rebind: ifname timeout".into());
            }
        }
    });
}


/// Full disconnect: stop RPC + kill Core + clear OS proxy + stop tray spin.
/// Invalidates in-flight connect gen so put_session_back will not reinstall.
pub(crate) fn disconnect_selected_sync() -> Result<serde_json::Value, String> {
    let _ = tunnel_sm::apply(tunnel_sm::Event::BeginDisconnect);
    // Invalidate any in-flight Start before killing (1A residual).
    let _ = bump_connect_gen();
    // L9: Blocked first so traffic cannot leak while Core/proxy tear down.
    // eng 2A: keep last peer on intermediate Blocked when present.
    let (peer, mixed_port, dns) = match tunnel_sm::last_params() {
        Some(p) => (Some(p.peer), p.mixed_port, p.dns),
        None => (None, MIXED_PORT, Vec::new()),
    };
    // If this fails traffic can leak while Core tears down, and the ResetIdle below
    // overwrites last_error — so carry it out rather than letting it vanish.
    let blocked_err = firewall::apply(firewall::Policy::Blocked {
        peer,
        mixed_port,
        dns,
    })
    .err();
    let (stop_err, running, pid) = {
        // A poisoned SESSION must not skip the teardown below (proxy/DNS/tray/ResetIdle),
        // or disconnect leaves the app pinned in Blocked with the spinner running.
        let mut g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_none() {
            CoreSession::kill_stray_cores(None);
            (None, false, -1i32)
        } else {
            let s = g.as_mut().unwrap();
            let stop_err = s.stop_rpc().unwrap_or_else(|e| Some(e));
            let (running, pid) = s.query_state().unwrap_or((false, -1));
            let _ = s.stop_core_process();
            *g = None;
            CoreSession::kill_stray_cores(None);
            (stop_err, running, pid)
        }
    };

    // Always best-effort clear OS proxy; DNS only if this session set bootstrap (1A).
    let mut notes = Vec::new();
    match sys::set_system_proxy(false, MIXED_PORT) {
        Ok(m) => notes.push(m),
        Err(e) => notes.push(format!("clear system proxy: {e}")),
    }
    if DNS_BOOTSTRAP_SET.swap(false, AtomicOrdering::SeqCst) {
        match sys::set_system_dns_bootstrap(false, &[]) {
            Ok(m) => notes.push(m),
            Err(e) => notes.push(format!("clear system dns: {e}")),
        }
    }
    if let Some(e) = &blocked_err {
        notes.push(format!("firewall blocked: {e}"));
    }
    let proxy_note = Some(notes.join(" · "));
    tray_spin::set_spinning(false);
    // After Core + proxy down: leave Idle and flush PF anchor (3A).
    let _ = tunnel_sm::apply(tunnel_sm::Event::ResetIdle);
    firewall::reset_best_effort();
    Ok(serde_json::json!({
        "stopped": stop_err.is_none(),
        "stop_error": stop_err,
        "running": running,
        "profile_id": pid,
        "proxy_note": proxy_note,
        "firewall_error": blocked_err,
        "tunnel_state": tunnel_sm::state().as_str(),
        "firewall": firewall_status_json(),
    }))
}
