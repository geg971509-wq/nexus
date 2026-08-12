pub mod core;
mod data;
mod defaults;
mod paths;
mod sys;
mod sub;
mod net;
mod tray_spin;
mod winhide;
pub mod tunnel_sm;
pub mod firewall;

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
        let _ = sys::set_system_dns_bootstrap(false);
    }
}

#[tauri::command]
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
    if session.child_exited() {
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
        match sys::set_system_dns_bootstrap(true) {
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

/// Share-link → SVG QR (offline; for 显示二维码 dialog).
#[tauri::command]
fn qr_svg(text: String) -> Result<serde_json::Value, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("empty qr payload".into());
    }
    if t.len() > 2000 {
        return Err("内容过长，无法生成二维码".into());
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
    if gen == 0 || gen != current_connect_gen() {
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

/// Kept for debug/tools only — product UI must use connect_selected / session_status.
/// Spawns Core process without profile Start (tunnel not loaded).
#[tauri::command]
async fn core_start() -> Result<String, String> {
    Err("core_start is disabled; use connect_selected".into())
}

/// Alias of session_status for older UI callers (single status truth after 4A/8A).
#[tauri::command]
async fn core_query_state() -> Result<serde_json::Value, String> {
    session_status().await
}

/// Boot / power sync: store chips + live Core (SESSION QueryState, or orphan process).
/// 4A: never treat mixed-port alone as live (unrelated listener on 2080).
/// 2A: Connected/Connecting + Core dead → CoreLost → firewall Blocked (keep peer).
#[tauri::command]
async fn session_status() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
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
        if !live
            && matches!(
                sm,
                tunnel_sm::State::Connected | tunnel_sm::State::Connecting
            )
        {
            let tr = tunnel_sm::apply(tunnel_sm::Event::CoreLost);
            let params = tr.params.or_else(tunnel_sm::last_params);
            let _ = firewall::apply(firewall::policy_from_sm(tr.to, params.as_ref()));
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
        }))
    })
    .await
    .map_err(|e| format!("session_status join: {e}"))?
}

#[tauri::command]
async fn core_check_config(json: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        let s = g.as_mut().ok_or("core not started")?;
        let err = s.check_config(&json)?;
        Ok(serde_json::json!({ "error": err }))
    })
    .await
    .map_err(|e| format!("core_check_config join: {e}"))?
}

/// Full disconnect (same body as disconnect_selected). Kept for UI fallback callers.
#[tauri::command]
async fn core_stop() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let _ = disconnect_selected_sync()?;
        Ok("stopped".into())
    })
    .await
    .map_err(|e| format!("core_stop join: {e}"))?
}

pub fn core_smoke_run() -> Result<(), String> {
    use core::session::CoreSession;
    let bin = CoreSession::resolve_core_binary();
    if !bin.is_file() {
        return Err(format!("missing core bin {}", bin.display()));
    }
    println!("core bin: {}", bin.display());
    let mut session = CoreSession::start(&bin).map_err(|e| e.to_string())?;
    let (running, pid) = session.query_state()?;
    println!("QueryState running={running} profile_id={pid}");
    // minimal sing-box-ish JSON — CheckConfig should return structured error or ok
    let minimal = r#"{"log":{"level":"info"},"inbounds":[],"outbounds":[{"type":"direct","tag":"direct"}]}"#;
    match session.check_config(minimal) {
        Ok(err) => println!("CheckConfig error_field={err:?}"),
        Err(e) => println!("CheckConfig call err (may be ok if invalid): {e}"),
    }
    let _ = session.stop_rpc();
    session.stop_core_process().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn store_snapshot() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use data::store::Store;
        let st = Store::load();
        // read-only — never upsert Direct demo (UI catalog is truth)
        serde_json::to_value(&st).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("store_snapshot join: {e}"))?
}

/// Live tray hide: persist + menu-bar icon show/hide immediately.
#[tauri::command]
async fn set_hide_tray(app: tauri::AppHandle, hide: bool) -> Result<String, String> {
    {
        use data::store::Store;
        Store::update(|st| {
            st.hide_tray = hide;
            Ok(())
        })?;
    }
    tray_spin::set_visible(&app, !hide);
    Ok(if hide {
        "tray hidden".into()
    } else {
        "tray shown".into()
    })
}

/// Export config for selected node — same input shape as connect_selected.
#[tauri::command]
async fn generate_preview(
    link: Option<String>,
    outbound: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
        Ok(generate_with_outbound(ob, MIXED_PORT, st.tun))
    })
    .await
    .map_err(|e| format!("generate_preview join: {e}"))?
}

/// UI catalog blob (groups + nodes). Replaces localStorage nexus.catalog.v1 as source of truth.
#[tauri::command]
async fn catalog_get() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use data::store::Store;
        let st = Store::load();
        Ok(st.catalog.unwrap_or(serde_json::Value::Null))
    })
    .await
    .map_err(|e| format!("catalog_get join: {e}"))?
}

#[tauri::command]
async fn catalog_put(blob: serde_json::Value) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
/// Runs off the async runtime so the webview keeps painting while networksetup works.
#[tauri::command]
async fn set_system_proxy_cmd(enabled: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| format!("system proxy join: {e}"))?
}

/// set_spmode_vpn: persist + elevate Core (osascript password sheet).
/// Live tunnel re-Start is UI-side (needs node payload); here only privilege + flag.
#[tauri::command]
async fn set_tun_cmd(enabled: bool) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
                            match s.recycle_privileged() {
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
    })
    .await
    .map_err(|e| format!("set_tun join: {e}"))?
}

/// engine-aligned connect: BuildSingBoxConfig → Start(LoadConfigReq).
/// UI passes selected node share `link` or raw `outbound` JSON; no invent credentials.
/// Tun/system-proxy follow upstream `spmode_vpn` / `spmode_system_proxy` (UI chips → optional args → store).
/// Async + spawn_blocking: sync Start holds SESSION + Core RPC and freezes the power button.
#[tauri::command]
async fn connect_selected(
    link: Option<String>,
    outbound: Option<serde_json::Value>,
    profile_id: Option<i32>,
    tun: Option<bool>,
    system_proxy: Option<bool>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        connect_selected_sync(link, outbound, profile_id, tun, system_proxy)
    })
    .await
    .map_err(|e| format!("connect join: {e}"))?
}

fn connect_selected_sync(
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
    let (use_tun, use_sys_proxy) = Store::update(|st| {
        if let Some(v) = tun {
            st.tun = v;
        }
        if let Some(v) = system_proxy {
            st.system_proxy = v;
        }
        Ok((st.tun, st.system_proxy))
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
    let mut cfg = generate_with_outbound(ob.clone(), port, use_tun);

    // mac Tun: pin next free utunN into Core config + PF Connected.
    // Detection-only (172.19 / new-utun / stale core.log) left pure-Tun dead under
    // fail-closed: no reliable `pass quick on utun…` while sysproxy still worked.
    #[cfg(target_os = "macos")]
    let planned_tun_if: Option<String> = if use_tun {
        let name = next_free_utun().ok_or_else(|| "no free utun for Tun".to_string())?;
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
    #[cfg(not(target_os = "macos"))]
    let planned_tun_if: Option<String> = if use_tun {
        Some("nexus-tun".into())
    } else {
        None
    };

    let json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;

    // Tun: setuid Core before LoadConfig (upstream profile_start elevation).
    // osascript password sheet runs here if setuid copy missing — outside SESSION.
    if use_tun {
        CoreSession::ensure_privileged_core()?;
    }

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
        list_utun_names()
    } else {
        Vec::new()
    };
    #[cfg(not(target_os = "macos"))]
    let utun_before: Vec<String> = Vec::new();

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
        if use_tun {
            let priv_now = s.is_privileged().unwrap_or(false);
            if !priv_now {
                if let Err(e) = s.recycle_privileged() {
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
            .or_else(|| detect_tun_ifname(&utun_before))
    } else {
        None
    };
    let mut params_connected = connect_params.clone();
    params_connected.tun_if = tun_if.clone();
    // Connected only if fail-closed policy applies (or platform Unsupported).
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
    let proxy_note = apply_post_start_side_effects(connect_gen, use_sys_proxy, use_tun, port);
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

fn detect_tun_ifname(before: &[String]) -> Option<String> {
    // Fallback only: planned interface_name should already be set. Order:
    // (1) 172.19.0.0/24 on utun (2) new utun vs pre-Start (3) live core.log utun.
    #[cfg(target_os = "macos")]
    {
        detect_nexus_tun_ifname(20, before) // ~1s; late via spawn_tun_if_rebind
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = before;
        Some("nexus-tun".into())
    }
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
                if let Some(name) = detect_nexus_tun_ifname(1, &before) {
                    // Skip rebind if already matching a live planned/current ifname.
                    if params.tun_if.as_deref() == Some(name.as_str()) && if_exists(&name) {
                        return;
                    }
                    let mut p = params.clone();
                    p.tun_if = Some(name);
                    if gen != current_connect_gen() || tunnel_sm::state() != tunnel_sm::State::Connected
                    {
                        return;
                    }
                    tunnel_sm::update_tun_if(p.tun_if.clone());
                    let _ = firewall::apply(firewall::policy_from_sm(
                        tunnel_sm::State::Connected,
                        Some(&p),
                    ));
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (gen, params, before);
        }
    });
}

/// Poll for Core's tun after Start (iface appears slightly after LoadConfig).
#[cfg(target_os = "macos")]
fn detect_nexus_tun_ifname(attempts: u32, before: &[String]) -> Option<String> {
    for i in 0..attempts {
        if let Some(name) = ifname_nexus_tun_by_addr() {
            return Some(name);
        }
        if let Some(name) = new_utun_since(before) {
            return Some(name);
        }
        if let Some(name) = tun_if_from_core_log() {
            return Some(name);
        }
        if i + 1 < attempts {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    tun_if_from_core_log().or_else(|| new_utun_since(before))
}

/// Next free utunN (same algorithm as sing-tun CalculateInterfaceName on darwin).
#[cfg(target_os = "macos")]
fn next_free_utun() -> Option<String> {
    let mut max_idx: i32 = -1;
    for name in list_utun_names() {
        if let Some(rest) = name.strip_prefix("utun") {
            if let Ok(n) = rest.parse::<i32>() {
                if n > max_idx {
                    max_idx = n;
                }
            }
        }
    }
    let candidate = format!("utun{}", max_idx + 1);
    if firewall::is_safe_ifname(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn if_exists(name: &str) -> bool {
    unsafe {
        let c = match std::ffi::CString::new(name) {
            Ok(c) => c,
            Err(_) => return false,
        };
        libc::if_nametoindex(c.as_ptr()) != 0
    }
}

#[cfg(target_os = "macos")]
fn list_utun_names() -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 || ifap.is_null() {
            return names;
        }
        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_name.is_null() {
                continue;
            }
            if let Ok(name) = std::ffi::CStr::from_ptr(ifa.ifa_name).to_str() {
                if name.starts_with("utun")
                    && firewall::is_safe_ifname(name)
                    && !names.iter().any(|n| n == name)
                {
                    names.push(name.to_string());
                }
            }
        }
        libc::freeifaddrs(ifap);
    }
    names.sort();
    names
}

#[cfg(target_os = "macos")]
fn new_utun_since(before: &[String]) -> Option<String> {
    let after = list_utun_names();
    let mut added: Vec<String> = after
        .into_iter()
        .filter(|n| !before.iter().any(|b| b == n))
        .collect();
    // Prefer highest utunN (sing-box tends to pick next free)
    added.sort_by(|a, b| {
        let na = a.trim_start_matches("utun").parse::<u32>().unwrap_or(0);
        let nb = b.trim_start_matches("utun").parse::<u32>().unwrap_or(0);
        na.cmp(&nb)
    });
    added.pop()
}

#[cfg(target_os = "macos")]
fn ifname_nexus_tun_by_addr() -> Option<String> {
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 || ifap.is_null() {
            return None;
        }
        let mut found = None;
        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() || ifa.ifa_name.is_null() {
                continue;
            }
            if (*ifa.ifa_addr).sa_family as i32 != libc::AF_INET {
                continue;
            }
            let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
            let ip = u32::from_be(sin.sin_addr.s_addr).to_be_bytes();
            // 172.19.0.0/24 (generate TUN_V4)
            if ip[0] != 172 || ip[1] != 19 || ip[2] != 0 {
                continue;
            }
            if let Ok(name) = std::ffi::CStr::from_ptr(ifa.ifa_name).to_str() {
                if firewall::is_safe_ifname(name) && name.starts_with("utun") {
                    found = Some(name.to_string());
                    break;
                }
            }
        }
        libc::freeifaddrs(ifap);
        found
    }
}

/// Last live "started at utunN" from core.log. Ignores stale names (iface gone).
#[cfg(target_os = "macos")]
fn tun_if_from_core_log() -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let path = crate::paths::log_dir().join("core.log");
    let mut f = std::fs::File::open(&path).ok()?;
    let len = f.seek(SeekFrom::End(0)).ok()?;
    let start = len.saturating_sub(65536);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = String::new();
    f.read_to_string(&mut buf).ok()?;
    let mut last: Option<String> = None;
    for line in buf.lines() {
        // inbound/tun[tun-in]: started at utun5
        if let Some(idx) = line.find("started at utun") {
            let rest = &line[idx + "started at ".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if name.starts_with("utun") && firewall::is_safe_ifname(&name) && if_exists(&name) {
                last = Some(name);
            }
        }
    }
    last
}

fn firewall_status_json() -> serde_json::Value {
    let st = firewall::status();
    let support = match st.support {
        firewall::PlatformSupport::Active => "active",
        firewall::PlatformSupport::Unsupported => "unsupported",
    };
    // 6A: desired (SM) vs applied (last successful apply) + mismatch.
    let desired = firewall::desired_policy_name();
    let applied = st.last_policy.clone();
    let mismatch = !desired.is_empty() && !applied.is_empty() && desired != applied;
    serde_json::json!({
        "support": support,
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

#[tauri::command]
async fn firewall_status() -> Result<serde_json::Value, String> {
    Ok(firewall_status_json())
}

#[tauri::command]
async fn firewall_helper_install() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        firewall::install_helper()?;
        Ok(firewall_status_json())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
async fn firewall_helper_uninstall() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        firewall::uninstall_helper()?;
        Ok(firewall_status_json())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Live connections from Core TrafficManager (needs experimental.clash_api).
/// 6A: take/put + gen so poll does not hold SESSION across Core RPC.
#[tauri::command]
async fn query_connections() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
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
#[tauri::command]
async fn query_stats() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
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
#[tauri::command]
async fn disconnect_selected() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(disconnect_selected_sync)
        .await
        .map_err(|e| format!("disconnect join: {e}"))?
}

fn disconnect_selected_sync() -> Result<serde_json::Value, String> {
    let _ = tunnel_sm::apply(tunnel_sm::Event::BeginDisconnect);
    // Invalidate any in-flight Start before killing (1A residual).
    let _ = bump_connect_gen();
    // L9: Blocked first so traffic cannot leak while Core/proxy tear down.
    // eng 2A: keep last peer on intermediate Blocked when present.
    let (peer, mixed_port) = match tunnel_sm::last_params() {
        Some(p) => (Some(p.peer), p.mixed_port),
        None => (None, MIXED_PORT),
    };
    let _ = firewall::apply(firewall::Policy::Blocked {
        peer,
        mixed_port,
    });
    let (stop_err, running, pid) = {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
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
        match sys::set_system_dns_bootstrap(false) {
            Ok(m) => notes.push(m),
            Err(e) => notes.push(format!("clear system dns: {e}")),
        }
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
        "tunnel_state": tunnel_sm::state().as_str(),
        "firewall": firewall_status_json(),
    }))
}

/// GroupUpdater::HttpGet — download subscription body (no parse).
#[tauri::command]
async fn sub_fetch(url: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || sub::fetch(&url))
        .await
        .map_err(|e| format!("sub_fetch join: {e}"))?
}

/// Throne RawUpdater::updateClash — YAML proxies → catalog nodes with outbound JSON.
#[tauri::command]
async fn sub_parse_clash(body: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let nodes = data::clash::parse_clash_yaml(&body)?;
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
        Ok(serde_json::json!({ "ok": true, "nodes": arr, "count": arr.len() }))
    })
    .await
    .map_err(|e| format!("sub_parse_clash join: {e}"))?
}

/// Free-list / share URI body → catalog nodes with full outbound (vless/vmess/trojan/ss/…).
#[tauri::command]
async fn sub_parse_share(body: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let nodes = data::share_link::parse_share_body(&body);
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
        Ok(serde_json::json!({ "ok": true, "nodes": arr, "count": arr.len() }))
    })
    .await
    .map_err(|e| format!("sub_parse_share join: {e}"))?
}

/// TCP connect RTT probe. Emits `net-probe-result` per finished target (upstream progressive).
/// Runs off the async runtime so the webview keeps painting while probes are in flight.
#[tauri::command]
async fn net_tcp_probe(
    app: tauri::AppHandle,
    targets: Vec<serde_json::Value>,
    timeout_ms: Option<u64>,
    concurrency: Option<usize>,
) -> Result<serde_json::Value, String> {
    if targets.is_empty() {
        return Err("no targets".into());
    }
    if targets.len() > 500 {
        return Err("too many targets".into());
    }
    let timeout_ms = timeout_ms.unwrap_or(3000);
    // Default closer to Throne test_concurrent (was 8 → free-list ~300 feels frozen)
    let concurrency = concurrency.unwrap_or(32);
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let results = net::probe_batch_progressive(
            &targets,
            timeout_ms,
            concurrency,
            |r| {
                let _ = app.emit("net-probe-result", r);
            },
        );
        // aborted if stop was called for this batch (canonical error "aborted")
        let aborted = results.iter().any(|r| r.error.as_deref() == Some("aborted"));
        serde_json::json!({
            "results": results,
            "aborted": aborted,
        })
    })
    .await
    .map_err(|e| format!("probe join: {e}"))
}

/// Abort in-flight TCP probes (upstream stopSpeedtest).
#[tauri::command]
fn net_tcp_probe_stop() -> Result<(), String> {
    net::abort_probes();
    Ok(())
}

/// Core TestCurrent: URL test via live box proxy/default outbound only.
/// take/reinstall session so poll/disconnect is not blocked for the whole Test.
#[tauri::command]
async fn core_url_test_current(
    url: Option<String>,
    timeout_ms: Option<i32>,
) -> Result<serde_json::Value, String> {
    let url = url.unwrap_or_default();
    let timeout_ms = timeout_ms.unwrap_or(3000);
    tauri::async_runtime::spawn_blocking(move || {
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
#[tauri::command]
async fn core_url_test_stop() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
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

/// DNS resolve host → IPs (single; kept for misc use).
#[tauri::command]
async fn net_resolve_host(host: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ips = net::resolve_host(&host)?;
        Ok(serde_json::json!({ "host": host, "ips": ips }))
    })
    .await
    .map_err(|e| format!("resolve join: {e}"))?
}

/// Batch DNS like net_tcp_probe: one spawn_blocking + progressive `net-resolve-result` events.
/// Avoids N IPC round-trips that freeze the webview when "select all" resolves IPs.
#[tauri::command]
async fn net_resolve_hosts(
    app: tauri::AppHandle,
    targets: Vec<serde_json::Value>,
    concurrency: Option<usize>,
) -> Result<serde_json::Value, String> {
    if targets.is_empty() {
        return Err("no targets".into());
    }
    if targets.len() > 2000 {
        return Err("too many targets".into());
    }
    let concurrency = concurrency.unwrap_or(16);
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let results = net::resolve_batch_progressive(&targets, concurrency, |r| {
            let _ = app.emit("net-resolve-result", r);
        });
        let aborted = results.iter().any(|r| r.error.as_deref() == Some("aborted"));
        serde_json::json!({
            "results": results,
            "count": results.len(),
            "aborted": aborted,
        })
    })
    .await
    .map_err(|e| format!("resolve join: {e}"))
}

/// Policy A: explicit quit fully tears down tunnel (Core + OS proxy + spin).
/// Set only after user confirms (or tunnel already dead). ExitRequested honors this.
static ALLOW_EXIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

/// Native warning before full teardown (tray / Cmd+Q when webview dialog unavailable).
fn confirm_disconnect_quit() -> bool {
    #[cfg(target_os = "macos")]
    {
        let script = r#"display dialog "当前隧道仍在运行（含 Tun / 系统代理）。退出将停止 Core、关闭系统代理并拆除隧道。" with title "Nexus" buttons {"取消", "断开并退出"} default button "断开并退出" cancel button "取消" with icon caution"#;
        return std::process::Command::new("osascript")
            .args(["-e", script])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    #[cfg(target_os = "windows")]
    {
        // VBScript MsgBox: Yes=6. CREATE_NO_WINDOW so cscript itself doesn't flash a console.
        let script = r#"
WScript.Quit CreateObject("WScript.Shell").Popup("Tunnel still running (Tun / system proxy). Exit will stop Core and clear system proxy.", 0, "Nexus", 49)
"#;
        let dir = std::env::temp_dir();
        let path = dir.join("nexus-quit-confirm.vbs");
        if std::fs::write(&path, script).is_err() {
            return true;
        }
        let mut cmd = std::process::Command::new("cscript");
        crate::winhide::apply(&mut cmd);
        let ok = cmd
            .args(["//Nologo", &path.to_string_lossy()])
            .status()
            .map(|s| s.code() == Some(6))
            .unwrap_or(true);
        let _ = std::fs::remove_file(&path);
        return ok;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        true
    }
}

/// force=true: UI already warned → teardown + exit. force=false: warn if live.
fn request_quit(app: tauri::AppHandle, force: bool) {
    use std::sync::atomic::Ordering;
    if !force && tunnel_is_live() && !confirm_disconnect_quit() {
        return;
    }
    ALLOW_EXIT.store(true, Ordering::SeqCst);
    teardown_session();
    app.exit(0);
}

#[tauri::command]
fn app_quit(app: tauri::AppHandle, force: Option<bool>) {
    request_quit(app, force.unwrap_or(false));
}

fn show_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            app_identity,
            app_quit,
            qr_svg,
            core_start,
            core_query_state,
            session_status,
            core_check_config,
            core_stop,
            store_snapshot,
            set_hide_tray,
            generate_preview,
            catalog_get,
            catalog_put,
            firewall_status,
            firewall_helper_install,
            firewall_helper_uninstall,
            set_system_proxy_cmd,
            set_tun_cmd,
            connect_selected,
            disconnect_selected,
            query_connections,
            query_stats,
            sub_fetch,
            sub_parse_clash,
            sub_parse_share,
            net_tcp_probe,
            net_tcp_probe_stop,
            core_url_test_current,
            core_url_test_stop,
            net_resolve_host,
            net_resolve_hosts
        ])
        // traffic-light close → tray (not quit)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            use tauri::Manager;

            let show_i = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("missing default window icon")?;

            let _tray = TrayIconBuilder::with_id("main")
                .icon(icon)
                .menu(&menu)
                .tooltip("Nexus")
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    // Policy A: warn if tunnel live, then full teardown
                    "quit" => request_quit(app.clone(), false),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            // tray registered with app on build; retain handle for process life
            app.manage(_tray);
            tray_spin::init(app.handle());
            // apply hide_tray before first paint of menu bar
            {
                use data::store::Store;
                let hide = Store::load().hide_tray;
                tray_spin::set_visible(app.handle(), !hide);
            }
            // 5A: cold boot residual PF — no active tunnel → best-effort Reset.
            // Helper down → status only (reset_best_effort is soft).
            std::thread::spawn(|| {
                if !tunnel_is_live() {
                    firewall::reset_best_effort();
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                // dock icon click while hidden → show again (macOS only)
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen {
                    has_visible_windows,
                    ..
                } => {
                    if !has_visible_windows {
                        show_main_window(app);
                    }
                }
                // Cmd+Q / dock Quit: warn if live, then teardown on Exit
                tauri::RunEvent::ExitRequested { api, .. } => {
                    use std::sync::atomic::Ordering;
                    if ALLOW_EXIT.load(Ordering::SeqCst) {
                        // confirmed (or not live via request_quit) — proceed to Exit
                    } else if tunnel_is_live() {
                        api.prevent_exit();
                        let app = app.clone();
                        std::thread::Builder::new()
                            .name("nexus-quit-confirm".into())
                            .spawn(move || {
                                if confirm_disconnect_quit() {
                                    ALLOW_EXIT.store(true, Ordering::SeqCst);
                                    teardown_session();
                                    app.exit(0);
                                }
                            })
                            .ok();
                    }
                    // not live → allow exit; Exit handler teardowns (idempotent)
                }
                tauri::RunEvent::Exit => {
                    teardown_session();
                }
                _ => {}
            }
        });
}
