pub mod core;
mod data;
mod paths;
mod sys;
mod sub;
mod net;
mod tray_spin;
mod winhide;

use core::session::{CoreSession, SESSION};

#[tauri::command]
fn app_identity() -> serde_json::Value {
    serde_json::json!({
        "name": "Nexus",
        "identifier": "app.nexus.desktop",
        "version": "0.2.0",
    })
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

/// Off async runtime: spawn + socket accept can take seconds and freezes UI if sync.
#[tauri::command]
async fn core_start() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        if g.is_some() {
            return Ok("already running".into());
        }
        let bin = CoreSession::resolve_core_binary();
        if !bin.is_file() {
            return Err(format!("NexusCore not found at {}", bin.display()));
        }
        let session = CoreSession::start(&bin).map_err(|e| e.to_string())?;
        *g = Some(session);
        Ok(format!("started {}", bin.display()))
    })
    .await
    .map_err(|e| format!("core_start join: {e}"))?
}

#[tauri::command]
async fn core_query_state() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        let s = g.as_mut().ok_or("core not started")?;
        let (running, profile_id) = s.query_state()?;
        Ok(serde_json::json!({ "running": running, "profile_id": profile_id }))
    })
    .await
    .map_err(|e| format!("core_query_state join: {e}"))?
}

/// Boot / power sync: store chips + live Core (SESSION QueryState, or orphan process/port).
/// GUI quit without Stop leaves NexusCore + utun/mixed; power must show 已连接.
#[tauri::command]
async fn session_status() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use data::store::Store;
        let st = Store::load();
        let mut rpc_running = false;
        let mut profile_id = -1i32;
        let mut has_session = false;
        if let Ok(mut g) = SESSION.lock() {
            if let Some(s) = g.as_mut() {
                has_session = true;
                if let Ok((r, pid)) = s.query_state() {
                    rpc_running = r;
                    profile_id = pid;
                }
            }
        }
        let process_alive = CoreSession::core_process_alive();
        let mixed_open = CoreSession::mixed_port_open(2080);
        // Live if RPC Start still loaded, OR Core/mixed residual (GUI relaunch / SESSION lag).
        // Do not require !has_session — dead SESSION + live process was painting 未连接.
        let live = rpc_running || process_alive || mixed_open;
        // Keep menu-bar Earth spin in sync (boot / orphan Core / reconnect).
        tray_spin::set_spinning(live);
        Ok(serde_json::json!({
            "running": live,
            "rpc_running": rpc_running,
            "has_session": has_session,
            "process_alive": process_alive,
            "mixed_open": mixed_open,
            "profile_id": profile_id,
            "tun": st.tun,
            "system_proxy": st.system_proxy,
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

#[tauri::command]
async fn core_stop() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        if let Some(mut s) = g.take() {
            let _ = s.stop_rpc();
            s.stop_core_process().map_err(|e| e.to_string())?;
        }
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
        Ok(generate_with_outbound(ob, 2080, st.tun, &st.blocklist))
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
        let mut st = Store::load();
        st.catalog = Some(blob);
        st.save()?;
        Ok("ok".into())
    })
    .await
    .map_err(|e| format!("catalog_put join: {e}"))?
}

#[tauri::command]
async fn blocklist_get() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use data::store::Store;
        let st = Store::load();
        Ok(serde_json::json!({ "items": st.blocklist }))
    })
    .await
    .map_err(|e| format!("blocklist_get join: {e}"))?
}

/// Full-replace blocklist. Items: `{host, process_path?}` (legacy bare host string still deserializes).
#[tauri::command]
async fn blocklist_put(items: Vec<data::store::BlockEntry>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use data::generate::normalize_blocklist;
        use data::store::Store;
        let normalized = normalize_blocklist(&items)?;
        let mut st = Store::load();
        st.blocklist = normalized.clone();
        st.save()?;
        Ok(serde_json::json!({ "items": normalized }))
    })
    .await
    .map_err(|e| format!("blocklist_put join: {e}"))?
}

/// Persist chip intent; OS apply only when Core is running (or always on disable).
/// Runs off the async runtime so the webview keeps painting while networksetup works.
#[tauri::command]
async fn set_system_proxy_cmd(enabled: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use core::session::SESSION;
        use data::store::Store;
        // set_spmode_system_proxy: always persist intent; OS write only if profile running.
        let mut st = Store::load();
        st.system_proxy = enabled;
        st.save()?;
        let port = 2080u16;
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
        let mut st = Store::load();
        let prev = st.tun;
        st.tun = enabled;
        st.save()?;
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
                                    st.tun = prev;
                                    let _ = st.save();
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
                st.tun = prev;
                let _ = st.save();
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

    let mut st = Store::load();
    // Start uses current checkbox state, not a stale disk flag.
    // Prefer explicit UI args; persist so next cold Start matches chips.
    let mut dirty = false;
    if let Some(v) = tun {
        if st.tun != v {
            st.tun = v;
            dirty = true;
        }
    }
    if let Some(v) = system_proxy {
        if st.system_proxy != v {
            st.system_proxy = v;
            dirty = true;
        }
    }
    if dirty {
        let _ = st.save();
    }
    let use_tun = st.tun;
    let use_sys_proxy = st.system_proxy;
    let port = 2080u16;
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
    let cfg = generate_with_outbound(ob, port, use_tun, &st.blocklist);
    let json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;

    // Tun: setuid Core before LoadConfig (upstream profile_start elevation).
    // osascript password sheet runs here if setuid copy missing — outside SESSION.
    if use_tun {
        CoreSession::ensure_privileged_core()?;
    }

    // Hold SESSION only for Core IPC. networksetup must not sit under the lock
    // (blocks query_connections poll + other commands for ~0.2–1s+).
    let (start_err, running, qpid) = {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        if g.is_none() {
            let bin = CoreSession::resolve_core_binary();
            if !bin.is_file() {
                return Err(format!("NexusCore not found at {}", bin.display()));
            }
            *g = Some(CoreSession::start(&bin).map_err(|e| e.to_string())?);
        }
        let s = g.as_mut().unwrap();

        // Tun requires euid=0. Recycle unprivileged live Core onto setuid copy.
        if use_tun {
            let priv_now = s.is_privileged().unwrap_or(false);
            if !priv_now {
                s.recycle_privileged()?;
            }
        }

        // Start may replace running instance; Stop first if already running
        if let Ok((running, _)) = s.query_state() {
            if running {
                let _ = s.stop_rpc();
            }
        }

        let mut start_err = s.start_rpc(&json, pid)?;
        // Orphan Core / stale bbolt → initialize cache-file: timeout. One recovery:
        // kill strays, drop cache.db, Stop, Start again.
        if let Some(ref e) = start_err {
            let el = e.to_ascii_lowercase();
            if el.contains("cache-file") || el.contains("cache.db") || el.contains("timeout") {
                let keep = s.child_pid();
                CoreSession::kill_stray_cores(keep);
                let _ = s.stop_rpc();
                let _ = std::fs::remove_file(CoreSession::cache_db_path());
                start_err = s.start_rpc(&json, pid)?;
            }
        }
        if start_err.is_some() {
            return Ok(serde_json::json!({
                "started": false,
                "start_error": start_err,
                "config": cfg,
                "profile_id": pid,
                "tun": use_tun,
                "system_proxy": use_sys_proxy,
            }));
        }
        let (running, qpid) = s.query_state().unwrap_or((false, -1));
        (start_err, running, qpid)
    };

    // system proxy applied when spmode_system_proxy (not when Tun-only).
    // Only after Start success — OS must point at a live mixed port.
    let mut proxy_note = None;
    if start_err.is_none() {
        if use_sys_proxy {
            match sys::set_system_proxy(true, port) {
                Ok(m) => proxy_note = Some(m),
                Err(e) => proxy_note = Some(format!("system proxy failed: {e}")),
            }
        } else {
            // Chip off: clear any leftover OS proxy from a previous session
            match sys::set_system_proxy(false, port) {
                Ok(m) => proxy_note = Some(m),
                Err(e) => proxy_note = Some(format!("clear system proxy: {e}")),
            }
        }
    }

    // Menu-bar Earth spins while tunnel is up (proxy and/or Tun).
    if start_err.is_none() {
        tray_spin::set_spinning(true);
    }

    Ok(serde_json::json!({
        "started": true,
        "start_error": null,
        "running": running,
        "profile_id": qpid,
        "listen_port": port,
        "proxy_note": proxy_note,
        "tun": use_tun,
        "system_proxy": use_sys_proxy,
        "config": cfg,
    }))
}

/// Live connections from Core TrafficManager (needs experimental.clash_api).
/// Includes recently closed rows (short HTTP often leaves active empty).
#[tauri::command]
async fn query_connections() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use core::session::SESSION;
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        let s = g.as_mut().ok_or("core not started")?;
        let rows = s.query_connections()?;
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
/// Use this for the node 流量 column — connection-window sums freeze when conns close.
#[tauri::command]
async fn query_stats() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use core::session::SESSION;
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        let s = g.as_mut().ok_or("core not started")?;
        let (upload, download) = s.query_stats_proxy()?;
        Ok(serde_json::json!({
            "upload": upload,
            "download": download,
        }))
    })
    .await
    .map_err(|e| format!("query_stats join: {e}"))?
}

/// Stop RPC only — keep Core process for next Start.
#[tauri::command]
async fn disconnect_selected() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(disconnect_selected_sync)
        .await
        .map_err(|e| format!("disconnect join: {e}"))?
}

fn disconnect_selected_sync() -> Result<serde_json::Value, String> {
    use data::store::Store;

    // SESSION for Stop; orphan Core (GUI relaunched) → kill process so power can go off.
    let (stop_err, running, pid, clear_proxy) = {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        if g.is_none() {
            CoreSession::kill_stray_cores(None);
            let clear_proxy = Store::load().system_proxy;
            (None, false, -1i32, clear_proxy)
        } else {
            let s = g.as_mut().unwrap();
            let stop_err = s.stop_rpc()?;
            let (running, pid) = s.query_state().unwrap_or((false, -1));
            let clear_proxy = Store::load().system_proxy;
            let _ = s.stop_core_process();
            *g = None;
            (stop_err, running, pid, clear_proxy)
        }
    };

    let mut proxy_note = None;
    if clear_proxy {
        match sys::set_system_proxy(false, 2080) {
            Ok(m) => proxy_note = Some(m),
            Err(e) => proxy_note = Some(format!("clear system proxy: {e}")),
        }
    }
    tray_spin::set_spinning(false);
    Ok(serde_json::json!({
        "stopped": stop_err.is_none(),
        "stop_error": stop_err,
        "running": running,
        "profile_id": pid,
        "proxy_note": proxy_note,
    }))
}

/// GroupUpdater::HttpGet — download subscription body (no parse).
#[tauri::command]
async fn sub_fetch(url: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || sub::fetch(&url))
        .await
        .map_err(|e| format!("sub_fetch join: {e}"))?
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
    let concurrency = concurrency.unwrap_or(8);
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
        serde_json::json!({
            "results": results,
            "aborted": net::is_aborted(),
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
        serde_json::json!({
            "results": results,
            "count": results.len(),
        })
    })
    .await
    .map_err(|e| format!("resolve join: {e}"))
}

/// Policy A: explicit quit fully tears down tunnel (Core + OS proxy + spin).
/// Set only after user confirms (or tunnel already dead). ExitRequested honors this.
static ALLOW_EXIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Single quit/teardown path: stop Core + always best-effort clear OS proxy at :2080.
/// Used by app_quit, tray quit, and Exit (after confirm). Idempotent.
fn teardown_session() {
    tray_spin::set_spinning(false);
    // Always clear — store flag can lag OS; exit must not leave browsers on dead :2080.
    let _ = sys::set_system_proxy(false, 2080);
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

/// Best-effort: RPC running, NexusCore process, or mixed :2080 still accepting.
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
    CoreSession::core_process_alive() || CoreSession::mixed_port_open(2080)
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
            generate_preview,
            catalog_get,
            catalog_put,
            blocklist_get,
            blocklist_put,
            set_system_proxy_cmd,
            set_tun_cmd,
            connect_selected,
            disconnect_selected,
            query_connections,
            query_stats,
            sub_fetch,
            net_tcp_probe,
            net_tcp_probe_stop,
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
