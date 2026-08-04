pub mod core;
mod data;
mod sys;
mod warp;

use core::session::{CoreSession, SESSION};

#[tauri::command]
fn app_identity() -> serde_json::Value {
    serde_json::json!({
        "name": "Nexus",
        "identifier": "app.nexus.desktop",
        "phase": "CDE-mvp-bridge",
        "warp": "bundled-warp-cli",
    })
}

#[tauri::command]
fn core_start() -> Result<String, String> {
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
}

#[tauri::command]
fn core_query_state() -> Result<serde_json::Value, String> {
    let mut g = SESSION.lock().map_err(|e| e.to_string())?;
    let s = g.as_mut().ok_or("core not started")?;
    let (running, profile_id) = s.query_state()?;
    Ok(serde_json::json!({ "running": running, "profile_id": profile_id }))
}

#[tauri::command]
fn core_check_config(json: String) -> Result<serde_json::Value, String> {
    let mut g = SESSION.lock().map_err(|e| e.to_string())?;
    let s = g.as_mut().ok_or("core not started")?;
    let err = s.check_config(&json)?;
    Ok(serde_json::json!({ "error": err }))
}

#[tauri::command]
fn core_stop() -> Result<String, String> {
    let mut g = SESSION.lock().map_err(|e| e.to_string())?;
    if let Some(mut s) = g.take() {
        let _ = s.stop_rpc();
        s.stop_core_process().map_err(|e| e.to_string())?;
    }
    Ok("stopped".into())
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
fn store_snapshot() -> Result<serde_json::Value, String> {
    use data::store::Store;
    let mut st = Store::load();
    // seed demo only once when empty — snapshot otherwise read-only
    if st.profiles.is_empty() {
        st.upsert_direct_demo();
        st.save()?;
    }
    serde_json::to_value(&st).map_err(|e| e.to_string())
}

#[tauri::command]
fn generate_preview() -> Result<serde_json::Value, String> {
    use data::generate::generate_for_store;
    use data::store::Store;
    let mut st = Store::load();
    st.upsert_direct_demo();
    generate_for_store(&st, 2080)
}


#[tauri::command]
fn set_system_proxy_cmd(enabled: bool) -> Result<String, String> {
    use data::store::Store;
    let mut st = Store::load();
    st.system_proxy = enabled;
    st.save()?;
    // Only flip OS proxy when enabling; port matches generate default
    sys::set_system_proxy(enabled, 2080)
}

#[tauri::command]
fn set_system_dns_cmd(enabled: bool) -> Result<String, String> {
    use data::store::Store;
    let mut st = Store::load();
    st.system_dns = enabled;
    st.save()?;
    sys::set_system_dns(enabled)
}

#[tauri::command]
fn set_tun_cmd(enabled: bool) -> Result<String, String> {
    use data::store::Store;
    let mut st = Store::load();
    if enabled && st.system_proxy {
        // soft note: tun+proxy both ok in many setups; WARP mutex later
    }
    st.tun = enabled;
    st.save()?;
    Ok(format!("tun={enabled} (applied on next generate/start)"))
}

#[tauri::command]
fn connect_selected() -> Result<serde_json::Value, String> {
    use data::generate::generate_for_store;
    use data::store::Store;
    use core::session::{CoreSession, SESSION};
    let mut st = Store::load();
    st.upsert_direct_demo();
    let cfg = generate_for_store(&st, 2080)?;
    let json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
    let mut g = SESSION.lock().map_err(|e| e.to_string())?;
    if g.is_none() {
        let bin = CoreSession::resolve_core_binary();
        *g = Some(CoreSession::start(&bin).map_err(|e| e.to_string())?);
    }
    let s = g.as_mut().unwrap();
    // Prefer CheckConfig for safety in Phase D; Start when config trusted
    let check_err = s.check_config(&json)?;
    Ok(serde_json::json!({
        "check_error": check_err,
        "config": cfg,
        "started": false, "note": "CheckConfig only — Start() deferred until import path is trusted"
    }))
}


#[tauri::command]
fn warp_status() -> serde_json::Value {
    warp::status_json()
}

/// Enable/disable WARP via bundled or resolved `warp-cli` (connect/disconnect).
#[tauri::command]
fn warp_set(enabled: bool) -> Result<String, String> {
    warp::set_enabled(enabled)
}

/// Official GUI Mode: udp|https|tls → warp|warp+doh|warp+dot
#[tauri::command]
fn warp_set_mode(mode: String) -> Result<String, String> {
    warp::set_mode(&mode)
}

/// Optional: open full Cloudflare WARP.app GUI if installed.
#[tauri::command]
fn warp_open() -> Result<String, String> {
    warp::open_warp_app()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            app_identity,
            core_start,
            core_query_state,
            core_check_config,
            core_stop,
            store_snapshot,
            generate_preview,
            set_system_proxy_cmd,
            set_system_dns_cmd,
            set_tun_cmd,
            connect_selected,
            warp_status,
            warp_set,
            warp_set_mode,
            warp_open
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
