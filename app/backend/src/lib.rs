#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
compile_error!("Nexus backend supports Apple Silicon macOS only");

pub mod core;
mod data;
mod defaults;
mod exit_ip;
mod paths;
mod runtime;
mod sys;
mod sub;
mod net;
mod tray_spin;
mod tun_if;
pub mod tunnel_sm;
pub mod firewall;
pub mod qt_api;

use core::session::{CoreSession, SESSION};
use defaults::{APP_IDENTIFIER, APP_NAME, APP_VERSION, MIXED_PORT};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

/// 1A: true only after this session set system DNS bootstrap (Tun path).
static DNS_BOOTSTRAP_SET: AtomicBool = AtomicBool::new(false);

fn bump_connect_gen() -> u64 {
    tunnel_sm::bump_gen()
}

fn current_connect_gen() -> u64 {
    tunnel_sm::current_gen()
}

/// Clear bootstrap DNS only if this session set it (1A: never wipe custom DNS).
fn clear_dns_bootstrap_if_set() {
    if DNS_BOOTSTRAP_SET.swap(false, AtomicOrdering::SeqCst) {
        let _ = sys::set_system_dns_bootstrap(false, &[]);
    }
}

fn app_identity() -> serde_json::Value {
    serde_json::json!({
        "name": APP_NAME,
        "identifier": APP_IDENTIFIER,
        "version": APP_VERSION,
        "mixed_port": MIXED_PORT,
    })
}

/// Put a session back after long RPC outside the lock.
/// Only reinstalls when `gen` is still current and the slot is empty.
/// Returns true when the session was reinstalled under this gen.
fn put_session_back(mut session: CoreSession, gen: u64) -> bool {
    // A desynced IPC stream is as dead as an exited child: reinstalling it means
    // every later call misparses. Drop it so the next connect spawns a fresh Core.
    if session.child_exited() || session.client_broken() {
        let _ = session.stop_core_process();
        return false;
    }
    if gen == 0 || gen != current_connect_gen() {
        let _ = session.stop_core_process();
        return false;
    }
    match SESSION.lock() {
        Ok(mut g) if g.is_none() => {
            if gen == current_connect_gen() {
                *g = Some(session);
                true
            } else {
                let _ = session.stop_core_process();
                false
            }
        }
        Ok(_) => {
            let _ = session.stop_core_process();
            false
        }
        Err(_) => {
            let _ = session.stop_core_process();
            false
        }
    }
}

/// 2A: apply proxy/spin only if this connect gen still owns the live session.
/// After OS proxy write, re-check gen — disconnect can win during the call; undo enable.
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

/// Share-link → SVG QR (offline; for the share-QR dialog).
fn qr_svg(text: String) -> Result<serde_json::Value, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("empty qr payload".into());
    }
    if t.len() > 2000 {
        return Err("payload too long for QR".into());
    }
    use qrcode::render::svg;
    use qrcode::QrCode;
    let code = QrCode::new(t.as_bytes()).map_err(|e| format!("qr encode: {e}"))?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(200, 200)
        .dark_color(svg::Color("#111111"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(serde_json::json!({ "svg": svg, "len": t.len() }))
}

#[cfg(test)]
mod qr_tests {
    use super::*;
    #[test]
    fn qr_svg_vless_sample() {
        let v = qr_svg("vless://11111111-1111-1111-1111-111111111111@1.1.1.1:443?encryption=none&type=ws#n".into()).unwrap();
        let s = v["svg"].as_str().unwrap();
        assert!(s.contains("<svg"), "{s}");
        assert!(s.len() > 200);
    }
}

/// Reinstall a short-poll session only if gen still current and slot empty.
fn reinstall_poll_session(mut session: CoreSession, gen: u64) {
    if gen == 0 || gen != current_connect_gen() || session.client_broken() {
        let _ = session.stop_core_process();
        return;
    }
    match SESSION.lock() {
        Ok(mut g) if g.is_none() && gen == current_connect_gen() => {
            *g = Some(session);
        }
        Ok(_) => {
            let _ = session.stop_core_process();
        }
        Err(_) => {
            let _ = session.stop_core_process();
        }
    }
}

/// Boot / power sync: store chips + live Core (SESSION QueryState, or orphan process).
/// 4A: never treat mixed-port alone as live (unrelated listener on 2080).
/// 2A: Connected/Connecting + Core dead → CoreLost → firewall Blocked (keep peer).
async fn session_status() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use data::store::Store;
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

async fn store_snapshot() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use data::store::Store;
        let st = Store::load();
        // read-only — never upsert Direct demo (UI catalog is truth)
        serde_json::to_value(&st).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("store_snapshot join: {e}"))?
}

/// Persist hide_tray. Qt paints the menu-bar icon; this only writes store.json.
fn persist_hide_tray(hide: bool) -> Result<String, String> {
    use data::store::Store;
    Store::update(|st| {
        st.hide_tray = hide;
        Ok(())
    })?;
    Ok(if hide {
        "tray hidden".into()
    } else {
        "tray shown".into()
    })
}

/// Export config for selected node — same input shape as connect_selected.
async fn generate_preview(
    link: Option<String>,
    outbound: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(move || {
        use data::generate::generate_with_outbound;
        use data::share_link::parse_to_outbound;
        use data::store::Store;
        let st = Store::load();
        let ob = if let Some(v) = outbound {
            if v.get("type").and_then(|t| t.as_str()).is_none() {
                return Err("outbound missing type".into());
            }
            v
        } else if let Some(lk) = link.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            parse_to_outbound(lk)?
        } else {
            return Err("no selected node link/outbound for preview".into());
        };
        Ok(generate_with_outbound(
            ob,
            MIXED_PORT,
            st.tun,
            &st.dns_bootstrap(),
        ))
    })
    .await
    .map_err(|e| format!("generate_preview join: {e}"))?
}

/// UI catalog blob (groups + nodes). Replaces localStorage nexus.catalog.v1 as source of truth.
async fn catalog_get() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use data::store::Store;
        let st = Store::load();
        Ok(st.catalog.unwrap_or(serde_json::Value::Null))
    })
    .await
    .map_err(|e| format!("catalog_get join: {e}"))?
}

async fn catalog_put(blob: serde_json::Value) -> Result<String, String> {
    runtime::spawn_blocking(move || {
        use data::store::Store;
        if !blob.is_object() {
            return Err("catalog blob must be object".into());
        }
        Store::update(|st| {
            st.catalog = Some(blob);
            Ok("ok".into())
        })
    })
    .await
    .map_err(|e| format!("catalog_put join: {e}"))?
}


/// Persist chip intent; OS apply only when Core is running (or always on disable).
pub(crate) fn set_system_proxy_cmd_sync(enabled: bool) -> Result<String, String> {
    use core::session::SESSION;
    use data::store::Store;
    // set_spmode_system_proxy: always persist intent; OS write only if profile running.
    Store::update(|st| {
        st.system_proxy = enabled;
        Ok(())
    })?;
    let port = MIXED_PORT;
    // Short lock: query only — never hold across networksetup.
    let core_running = {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        g.as_mut()
            .and_then(|s| s.query_state().ok().map(|(r, _)| r))
            .unwrap_or(false)
    };
    if enabled && !core_running {
        return Ok(format!(
            "system_proxy intent=on (OS apply on Start · mixed 127.0.0.1:{port})"
        ));
    }
    // enable+running → point OS at mixed; disable → clear OS always (upstream ClearSystemProxy)
    // primary service sync (~0.2s); other NICs background — chip must not wait ~1s for all.
    sys::set_system_proxy(enabled, port)
}

/// Persist Tun chip + elevate Core (osascript password sheet).
/// Live tunnel re-Start is UI-side (needs node payload); here only privilege + flag.
pub(crate) fn set_tun_cmd_sync(enabled: bool) -> Result<serde_json::Value, String> {
    use data::store::Store;
    let prev = Store::update(|st| {
        let prev = st.tun;
        st.tun = enabled;
        Ok(prev)
    })?;
    if !enabled {
        return Ok(serde_json::json!({
            "tun": false,
            "elevated": false,
            "note": "tun=off (applied on next generate/start)",
        }));
    }
    // Tun needs root Core. Bundle may be on nosuid → Application Support setuid copy.
    match CoreSession::ensure_privileged_core() {
        Ok(path) => {
            // If Core already running unprivileged, recycle so next Start is root.
            let mut recycled = false;
            if let Ok(mut g) = SESSION.lock() {
                if let Some(s) = g.as_mut() {
                    let priv_now = s.is_privileged().unwrap_or(false);
                    if !priv_now {
                        match s.recycle_privileged(&path) {
                            Ok(()) => recycled = true,
                            Err(e) => {
                                let _ = Store::update(|st| {
                                    st.tun = prev;
                                    Ok(())
                                });
                                return Err(format!("Tun elevate recycle failed: {e}"));
                            }
                        }
                    }
                }
            }
            Ok(serde_json::json!({
                "tun": true,
                "elevated": true,
                "recycled": recycled,
                "core": path.display().to_string(),
                "note": if recycled {
                    "tun=on · Core elevated (re-Start to apply Tun inbound)"
                } else {
                    "tun=on · Core setuid ready (re-Start to apply Tun inbound)"
                },
            }))
        }
        Err(e) => {
            let _ = Store::update(|st| {
                st.tun = prev;
                Ok(())
            });
            Err(format!("Tun needs admin: {e}"))
        }
    }
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
    use core::session::{CoreSession, SESSION};
    use data::generate::generate_with_outbound;
    use data::share_link::parse_to_outbound;
    use data::store::Store;

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


fn firewall_status_json() -> serde_json::Value {
    let st = firewall::status();
    // 6A: desired (SM) vs applied (last successful apply) + mismatch.
    let desired = firewall::desired_policy_name();
    let applied = st.last_policy.clone();
    let mismatch = !desired.is_empty() && !applied.is_empty() && desired != applied;
    serde_json::json!({
        "support": "active",
        "last_policy": st.last_policy,
        "desired_policy": desired,
        "applied_policy": applied,
        "policy_mismatch": mismatch,
        "last_error": st.last_error,
        "peer": st.peer,
        "tun_if": st.tun_if,
        "tunnel_state": tunnel_sm::state().as_str(),
        "helper_installed": st.helper_installed,
        "helper_running": st.helper_running,
        "helper_detail": st.helper_detail,
    })
}

async fn firewall_status() -> Result<serde_json::Value, String> {
    Ok(firewall_status_json())
}

async fn firewall_helper_install() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        firewall::install_helper()?;
        Ok(firewall_status_json())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

async fn firewall_helper_uninstall() -> Result<serde_json::Value, String> {
    // Uninstall boots the daemon out and flushes the PF anchor, so doing it mid
    // tunnel silently removes the kill switch while traffic keeps flowing: if Core
    // then dies there is nothing left to fail closed. Refuse rather than degrade —
    // disconnecting first is one click and leaves the user in a defined state.
    require_tunnel_idle("Uninstalling the firewall helper")?;
    runtime::spawn_blocking(|| {
        firewall::uninstall_helper()?;
        Ok(firewall_status_json())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Live connections from Core TrafficManager (needs experimental.clash_api).
/// 6A: take/put + gen so poll does not hold SESSION across Core RPC.
async fn query_connections() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use core::session::SESSION;
        let (mut session, gen) = {
            let mut g = SESSION.lock().map_err(|e| e.to_string())?;
            let gen = current_connect_gen();
            let s = g.take().ok_or_else(|| "core not started".to_string())?;
            (s, gen)
        };
        let rows = match session.query_connections() {
            Ok(r) => r,
            Err(e) => {
                reinstall_poll_session(session, gen);
                return Err(e);
            }
        };
        reinstall_poll_session(session, gen);
        let list: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "created_at": r.created_at,
                    "process": r.process,
                    "process_path": r.process_path,
                    "process_id": r.process_id,
                    "dest": r.dest,
                    "domain": r.domain,
                    "network": r.network,
                    "protocol": r.protocol,
                    "outbound": r.outbound,
                    "upload": r.upload,
                    "download": r.download,
                })
            })
            .collect();
        Ok(serde_json::json!({ "active": list, "count": list.len() }))
    })
    .await
    .map_err(|e| format!("query_connections join: {e}"))?
}

/// Cumulative proxy outbound traffic (Core QueryStats / TrafficManager).
/// 6A: take/put + gen so poll does not hold SESSION across Core RPC.
async fn query_stats() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use core::session::SESSION;
        let (mut session, gen) = {
            let mut g = SESSION.lock().map_err(|e| e.to_string())?;
            let gen = current_connect_gen();
            let s = g.take().ok_or_else(|| "core not started".to_string())?;
            (s, gen)
        };
        let stats = match session.query_stats_proxy() {
            Ok(r) => r,
            Err(e) => {
                reinstall_poll_session(session, gen);
                return Err(e);
            }
        };
        reinstall_poll_session(session, gen);
        let (upload, download) = stats;
        Ok(serde_json::json!({
            "upload": upload,
            "download": download,
        }))
    })
    .await
    .map_err(|e| format!("query_stats join: {e}"))?
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

/// Exit IP + country as seen from the far end, fetched through the mixed inbound.
/// Errors when the tunnel cannot carry it — the UI then shows nothing rather than
/// this machine's own address.
async fn exit_ip_probe() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| exit_ip::probe(MIXED_PORT))
        .await
        .map_err(|e| format!("exit_ip join: {e}"))?
}

/// GroupUpdater::HttpGet — download subscription body (no parse).
pub(crate) fn sub_fetch_sync(url: String) -> Result<serde_json::Value, String> {
    sub::fetch(&url)
}

/// Throne RawUpdater::updateClash — YAML proxies → catalog nodes with outbound JSON.
async fn sub_parse_clash(body: String) -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(move || {
        let (nodes, skipped) = data::clash::parse_clash_yaml(&body)?;
        let arr: Vec<serde_json::Value> = nodes
            .into_iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "type": n.type_label,
                    "addr": n.addr,
                    "lat": null,
                    "flow": null,
                    "outbound": n.outbound,
                })
            })
            .collect();
        Ok(serde_json::json!({ "ok": true, "nodes": arr, "count": arr.len(), "skipped": skipped }))
    })
    .await
    .map_err(|e| format!("sub_parse_clash join: {e}"))?
}

/// Free-list / share URI body → catalog nodes with full outbound (vless/vmess/trojan/ss/…).
async fn sub_parse_share(body: String) -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(move || {
        let (nodes, skipped) = data::share_link::parse_share_body(&body);
        let arr: Vec<serde_json::Value> = nodes
            .into_iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "type": n.type_label,
                    "addr": n.addr,
                    "lat": null,
                    "flow": null,
                    "link": n.link,
                    "outbound": n.outbound,
                })
            })
            .collect();
        Ok(serde_json::json!({ "ok": true, "nodes": arr, "count": arr.len(), "skipped": skipped }))
    })
    .await
    .map_err(|e| format!("sub_parse_share join: {e}"))?
}

/// Err unless the tunnel is fully down, for actions that are only safe then.
///
/// Direct NIC probes must not run beside a live tunnel. Enforced here, not in QML.
fn require_tunnel_idle(action: &str) -> Result<(), String> {
    let st = tunnel_sm::state();
    if st == tunnel_sm::State::Idle {
        return Ok(());
    }
    Err(format!(
        "{action} requires the tunnel to be fully disconnected (currently {})",
        st.as_str()
    ))
}

/// Abort in-flight TCP probes (upstream stopSpeedtest).
fn net_tcp_probe_stop() -> Result<(), String> {
    net::abort_probes();
    Ok(())
}

/// Core TestCurrent: URL test via live box proxy/default outbound only.
/// take/reinstall session so poll/disconnect is not blocked for the whole Test.
async fn core_url_test_current(
    url: Option<String>,
    timeout_ms: Option<i32>,
) -> Result<serde_json::Value, String> {
    let url = url.unwrap_or_default();
    let timeout_ms = timeout_ms.unwrap_or(3000);
    runtime::spawn_blocking(move || {
        let (taken, gen) = match SESSION.lock() {
            Ok(mut g) => {
                let gen = current_connect_gen();
                (g.take(), gen)
            }
            Err(_) => (None, 0),
        };
        let Some(mut s) = taken else {
            return Err("no core session".into());
        };
        let result = s.test_current_url(&url, timeout_ms);
        reinstall_poll_session(s, gen);
        let rows = result?;
        let results: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "tag": r.tag,
                    "ms": r.ms,
                    "error": r.error,
                })
            })
            .collect();
        Ok(serde_json::json!({ "results": results }))
    })
    .await
    .map_err(|e| format!("url test join: {e}"))?
}

/// Cancel in-flight Core URL test (StopTest).
async fn core_url_test_stop() -> Result<(), String> {
    runtime::spawn_blocking(|| {
        let (taken, gen) = match SESSION.lock() {
            Ok(mut g) => {
                let gen = current_connect_gen();
                (g.take(), gen)
            }
            Err(_) => (None, 0),
        };
        if let Some(mut s) = taken {
            let _ = s.stop_test();
            reinstall_poll_session(s, gen);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("url test stop join: {e}"))?
}

/// Single quit/teardown path: stop Core + always best-effort clear OS proxy at MIXED_PORT.
/// Used by app_quit, tray quit, and Exit (after confirm). Idempotent.
/// 3A: quit → Reset (session kill-switch ends with app; not post-quit lockdown).
fn teardown_session() {
    let _ = bump_connect_gen();
    tray_spin::set_spinning(false);
    let _ = tunnel_sm::apply(tunnel_sm::Event::ResetIdle);
    firewall::reset_best_effort();
    // Always clear proxy; DNS only if this session set bootstrap (1A).
    let _ = sys::set_system_proxy(false, MIXED_PORT);
    clear_dns_bootstrap_if_set();
    let _ = (|| -> Result<(), String> {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        if let Some(mut s) = g.take() {
            let _ = s.stop_rpc();
            let _ = s.stop_core_process();
        }
        CoreSession::kill_stray_cores(None);
        Ok(())
    })();
}

/// Same live rule as session_status (4A/5A): never treat mixed-port alone as live.
/// RPC running, or (Core process ∧ mixed open). Unrelated :2080 listener is not a tunnel.
fn tunnel_is_live() -> bool {
    if let Ok(mut g) = SESSION.lock() {
        if let Some(s) = g.as_mut() {
            if let Ok((running, _)) = s.query_state() {
                if running {
                    return true;
                }
            }
        }
    }
    CoreSession::core_process_alive() && CoreSession::mixed_port_open(MIXED_PORT)
}

/// Native warning before full teardown (tray / Cmd+Q when the Qt dialog is unavailable).
fn confirm_disconnect_quit() -> bool {
    #[cfg(target_os = "macos")]
    {
        let script = r#"display dialog "Tunnel still running (Tun / system proxy). Exit will stop Core, clear system proxy, and tear down the tunnel." with title "Nexus" buttons {"Cancel", "Disconnect and Quit"} default button "Disconnect and Quit" cancel button "Cancel" with icon caution"#;
        return std::process::Command::new("/usr/bin/osascript")
            .args(["-e", script])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
}

/// force=true: UI already warned → teardown. force=false: warn if live.
/// Returns true when the host should exit. Qt path uses this without AppHandle.
fn prepare_quit(force: bool) -> bool {
    if !force && tunnel_is_live() && !confirm_disconnect_quit() {
        return false;
    }
    teardown_session();
    true
}

#[cfg(test)]
mod idle_guard_tests {
    use super::*;

    /// The direct TCP probe binds the physical NIC and the uninstall flushes PF,
    /// so both are only safe with the tunnel fully down.
    #[test]
    fn only_idle_passes() {
        let _g = tunnel_sm::test_lock();
        let _ = tunnel_sm::apply(tunnel_sm::Event::ResetIdle);
        assert!(require_tunnel_idle("probe").is_ok());

        for state in [
            tunnel_sm::State::Connecting,
            tunnel_sm::State::Connected,
            tunnel_sm::State::Disconnecting,
            tunnel_sm::State::Error,
        ] {
            tunnel_sm::set_state(state);
            let err = require_tunnel_idle("probe").unwrap_err();
            // The caller has to be able to tell the user which state blocked it.
            assert!(err.contains(state.as_str()), "{state:?}: {err}");
        }

        let _ = tunnel_sm::apply(tunnel_sm::Event::ResetIdle);
        assert!(require_tunnel_idle("probe").is_ok(), "recovers after reset");
    }
}
