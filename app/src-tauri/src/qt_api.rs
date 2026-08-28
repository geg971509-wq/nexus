//! JSON C ABI for the Qt host. Command names match the handlers in lib.rs.

use serde::Serialize;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicPtr, Ordering};

type EventCb = unsafe extern "C" fn(*const c_char, *const c_char);
type BoolCb = unsafe extern "C" fn(bool);

static EVENT_CB: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
static TRAY_VISIBLE_CB: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
static SPINNING_CB: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

fn load_event_cb() -> Option<EventCb> {
    let p = EVENT_CB.load(Ordering::SeqCst);
    if p.is_null() {
        None
    } else {
        Some(unsafe { std::mem::transmute::<*mut (), EventCb>(p) })
    }
}

fn load_bool_cb(slot: &AtomicPtr<()>) -> Option<BoolCb> {
    let p = slot.load(Ordering::SeqCst);
    if p.is_null() {
        None
    } else {
        Some(unsafe { std::mem::transmute::<*mut (), BoolCb>(p) })
    }
}

// ponytail: AtomicPtr + transmute for C callbacks; OnceLock<Option<fn>> if we grow more slots.

pub(crate) fn notify_spinning(on: bool) {
    if let Some(cb) = load_bool_cb(&SPINNING_CB) {
        unsafe { cb(on) };
    }
}

fn notify_tray_visible(visible: bool) {
    if let Some(cb) = load_bool_cb(&TRAY_VISIBLE_CB) {
        unsafe { cb(visible) };
    }
}

#[allow(dead_code)]
pub(crate) fn notify_event(name: &str, payload: &Value) {
    let Some(cb) = load_event_cb() else {
        return;
    };
    let Ok(n) = CString::new(name) else {
        return;
    };
    let body = payload.to_string();
    let Ok(j) = CString::new(body) else {
        return;
    };
    unsafe { cb(n.as_ptr(), j.as_ptr()) };
}

fn to_c_string(s: String) -> *mut c_char {
    let bytes: Vec<u8> = s.into_bytes().into_iter().filter(|&b| b != 0).collect();
    CString::new(bytes)
        .unwrap_or_else(|_| CString::from_vec_with_nul(b"{}\0".to_vec()).unwrap())
        .into_raw()
}

fn result_json<T: Serialize>(r: Result<T, String>) -> String {
    match r {
        Ok(v) => serde_json::to_string(&v)
            .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string()),
        Err(e) => json!({"error": e}).to_string(),
    }
}

fn await_json<F, T>(f: F) -> String
where
    F: std::future::Future<Output = Result<T, String>>,
    T: Serialize,
{
    result_json(crate::runtime::block_on(f))
}

fn obj(json: &str) -> Value {
    let t = json.trim();
    if t.is_empty() {
        return json!({});
    }
    serde_json::from_str(t).unwrap_or_else(|_| json!({}))
}

fn get_bool(v: &Value, snake: &str, camel: &str) -> Option<bool> {
    v.get(snake)
        .or_else(|| v.get(camel))
        .and_then(|x| x.as_bool())
}

fn get_str(v: &Value, snake: &str, camel: &str) -> Option<String> {
    v.get(snake)
        .or_else(|| v.get(camel))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
}

fn get_i32(v: &Value, snake: &str, camel: &str) -> Option<i32> {
    v.get(snake)
        .or_else(|| v.get(camel))
        .and_then(|x| x.as_i64())
        .map(|n| n as i32)
}

fn get_val(v: &Value, snake: &str, camel: &str) -> Option<Value> {
    v.get(snake).or_else(|| v.get(camel)).cloned()
}

fn qt_net_tcp_probe(v: &Value) -> String {
    if let Err(e) = crate::require_tunnel_idle("Direct TCP probe") {
        return json!({"error": e}).to_string();
    }
    let Some(targets) = v.get("targets").and_then(|x| x.as_array()).cloned() else {
        return json!({"error": "no targets"}).to_string();
    };
    if targets.is_empty() {
        return json!({"error": "no targets"}).to_string();
    }
    if targets.len() > 500 {
        return json!({"error": "too many targets"}).to_string();
    }
    let timeout_ms = get_i32(v, "timeout_ms", "timeoutMs")
        .map(|n| n.max(0) as u64)
        .unwrap_or(3000);
    let concurrency = get_i32(v, "concurrency", "concurrency")
        .map(|n| n.max(1) as usize)
        .unwrap_or(32);
    // ponytail: return immediately; progressive paint is nexus.event("net-probe-result").
    std::thread::spawn(move || {
        let _ = crate::net::probe_batch_progressive(&targets, timeout_ms, concurrency, |r| {
            if let Ok(payload) = serde_json::to_value(r) {
                notify_event("net-probe-result", &payload);
            }
        });
    });
    json!({"started": true}).to_string()
}

fn qt_net_resolve_hosts(v: &Value) -> String {
    let Some(targets) = v.get("targets").and_then(|x| x.as_array()).cloned() else {
        return json!({"error": "no targets"}).to_string();
    };
    if targets.is_empty() {
        return json!({"error": "no targets"}).to_string();
    }
    if targets.len() > 2000 {
        return json!({"error": "too many targets"}).to_string();
    }
    let concurrency = get_i32(v, "concurrency", "concurrency")
        .map(|n| n.max(1) as usize)
        .unwrap_or(16);
    std::thread::spawn(move || {
        let _ = crate::net::resolve_batch_progressive(&targets, concurrency, |r| {
            if let Ok(payload) = serde_json::to_value(r) {
                notify_event("net-resolve-result", &payload);
            }
        });
    });
    json!({"started": true}).to_string()
}

fn qt_connect_selected(v: &Value) -> String {
    let link = get_str(v, "link", "link");
    let outbound = get_val(v, "outbound", "outbound");
    let profile_id = get_i32(v, "profile_id", "profileId");
    let tun = get_bool(v, "tun", "tun");
    let system_proxy = get_bool(v, "system_proxy", "systemProxy");
    // ponytail: same kick-off as tcp probe — GUI must not block_on osascript/Start.
    std::thread::spawn(move || {
        let payload = match crate::connect_selected_sync(link, outbound, profile_id, tun, system_proxy)
        {
            Ok(v) => v,
            Err(e) => json!({"error": e}),
        };
        notify_event("connect-result", &payload);
    });
    json!({"started": true}).to_string()
}

fn qt_disconnect_selected() -> String {
    std::thread::spawn(|| {
        let payload = match crate::disconnect_selected_sync() {
            Ok(v) => v,
            Err(e) => json!({"error": e}),
        };
        notify_event("disconnect-result", &payload);
    });
    json!({"started": true}).to_string()
}

fn qt_set_tun(enabled: bool) -> String {
    // ponytail: same kick-off as connect — GUI must not block_on osascript.
    std::thread::spawn(move || {
        let payload = match crate::set_tun_cmd_sync(enabled) {
            Ok(v) => v,
            Err(e) => json!({"error": e}),
        };
        notify_event("tun-result", &payload);
    });
    json!({"started": true}).to_string()
}

fn qt_set_sys(enabled: bool) -> String {
    // ponytail: same kick-off as tun — GUI must not block_on networksetup.
    std::thread::spawn(move || {
        let payload = match crate::set_system_proxy_cmd_sync(enabled) {
            Ok(s) => json!({"note": s}),
            Err(e) => json!({"error": e}),
        };
        notify_event("proxy-result", &payload);
    });
    json!({"started": true}).to_string()
}

fn qt_sub_fetch(url: String) -> String {
    // ponytail: curl --max-time 30; GUI must not block_on.
    if url.trim().is_empty() {
        return json!({"error": "empty url"}).to_string();
    }
    std::thread::spawn(move || {
        let payload = match crate::sub_fetch_sync(url) {
            Ok(v) => v,
            Err(e) => json!({"error": e}),
        };
        notify_event("sub-fetch-result", &payload);
    });
    json!({"started": true}).to_string()
}

pub(crate) fn dispatch(cmd: &str, json: &str) -> String {
    let v = obj(json);
    match cmd {
        "app_identity" => crate::app_identity().to_string(),
        "store_snapshot" => await_json(crate::store_snapshot()),
        "session_status" => await_json(crate::session_status()),
        "catalog_get" => await_json(crate::catalog_get()),
        "catalog_put" => {
            let blob = get_val(&v, "blob", "blob").unwrap_or(Value::Null);
            await_json(crate::catalog_put(blob))
        }
        "set_tun_cmd" => {
            let Some(enabled) = get_bool(&v, "enabled", "enabled") else {
                return json!({"error": "missing enabled"}).to_string();
            };
            qt_set_tun(enabled)
        }
        "set_system_proxy_cmd" => {
            let Some(enabled) = get_bool(&v, "enabled", "enabled") else {
                return json!({"error": "missing enabled"}).to_string();
            };
            qt_set_sys(enabled)
        }
        "set_hide_tray" => {
            let Some(hide) = get_bool(&v, "hide", "hide") else {
                return json!({"error": "missing hide"}).to_string();
            };
            match crate::persist_hide_tray(hide) {
                Ok(msg) => {
                    notify_tray_visible(!hide);
                    serde_json::to_string(&msg).unwrap_or_else(|_| json!({"error": "json"}).to_string())
                }
                Err(e) => json!({"error": e}).to_string(),
            }
        }
        "connect_selected" => qt_connect_selected(&v),
        "disconnect_selected" => qt_disconnect_selected(),
        "query_connections" => await_json(crate::query_connections()),
        "query_stats" => await_json(crate::query_stats()),
        "firewall_status" => await_json(crate::firewall_status()),
        "firewall_helper_install" => await_json(crate::firewall_helper_install()),
        "firewall_helper_uninstall" => await_json(crate::firewall_helper_uninstall()),
        "sub_fetch" => {
            let url = get_str(&v, "url", "url").unwrap_or_default();
            qt_sub_fetch(url)
        }
        "sub_parse_clash" => {
            let body = get_str(&v, "body", "body").unwrap_or_default();
            await_json(crate::sub_parse_clash(body))
        }
        "sub_parse_share" => {
            let body = get_str(&v, "body", "body").unwrap_or_default();
            await_json(crate::sub_parse_share(body))
        }
        "generate_preview" => await_json(crate::generate_preview(
            get_str(&v, "link", "link"),
            get_val(&v, "outbound", "outbound"),
        )),
        "qr_svg" => {
            let text = get_str(&v, "text", "text").unwrap_or_default();
            result_json(crate::qr_svg(text))
        }
        "exit_ip_probe" => await_json(crate::exit_ip_probe()),
        "net_tcp_probe" => qt_net_tcp_probe(&v),
        "net_tcp_probe_stop" => result_json(crate::net_tcp_probe_stop()),
        "core_url_test_current" => await_json(crate::core_url_test_current(
            get_str(&v, "url", "url"),
            get_i32(&v, "timeout_ms", "timeoutMs"),
        )),
        "core_url_test_stop" => await_json(crate::core_url_test_stop()),
        "net_resolve_hosts" => qt_net_resolve_hosts(&v),
        "app_quit" => {
            let force = get_bool(&v, "force", "force").unwrap_or(false);
            if crate::prepare_quit(force) {
                json!({"quit": true}).to_string()
            } else {
                json!({"quit": false}).to_string()
            }
        }
        _ => json!({"error": "nyi"}).to_string(),
    }
}

fn cstr<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(p) }.to_str().unwrap_or("")
}

#[no_mangle]
pub extern "C" fn nexus_invoke(cmd: *const c_char, json: *const c_char) -> *mut c_char {
    to_c_string(dispatch(cstr(cmd), cstr(json)))
}

#[no_mangle]
pub extern "C" fn nexus_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

#[no_mangle]
pub extern "C" fn nexus_teardown() {
    crate::teardown_session();
}

#[no_mangle]
pub extern "C" fn nexus_init() {
    crate::core::session::CoreSession::warm_binary_cache();
    std::thread::spawn(|| {
        if !crate::tunnel_is_live() {
            crate::firewall::reset_best_effort();
        }
    });
}

#[no_mangle]
pub extern "C" fn nexus_set_event_cb(cb: Option<EventCb>) {
    EVENT_CB.store(
        cb.map(|f| f as *mut ()).unwrap_or(std::ptr::null_mut()),
        Ordering::SeqCst,
    );
}

#[no_mangle]
pub extern "C" fn nexus_set_tray_visible_cb(cb: Option<BoolCb>) {
    TRAY_VISIBLE_CB.store(
        cb.map(|f| f as *mut ()).unwrap_or(std::ptr::null_mut()),
        Ordering::SeqCst,
    );
}

#[no_mangle]
pub extern "C" fn nexus_set_spinning_cb(cb: Option<BoolCb>) {
    SPINNING_CB.store(
        cb.map(|f| f as *mut ()).unwrap_or(std::ptr::null_mut()),
        Ordering::SeqCst,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_has_mixed_port() {
        let s = dispatch("app_identity", "{}");
        assert!(s.contains("mixed_port"), "{s}");
        assert!(s.contains("Nexus"), "{s}");
    }

    #[test]
    fn unknown_is_nyi() {
        let s = dispatch("not_a_command", "{}");
        assert!(s.contains("nyi"), "{s}");
    }

    #[test]
    fn hide_tray_missing_arg() {
        let s = dispatch("set_hide_tray", "{}");
        assert!(s.contains("missing hide"), "{s}");
    }

    #[test]
    fn probe_stop_is_not_nyi() {
        let s = dispatch("net_tcp_probe_stop", "{}");
        assert!(!s.contains("nyi"), "{s}");
    }

    #[test]
    fn tcp_probe_rejects_empty() {
        let s = dispatch("net_tcp_probe", r#"{"targets":[]}"#);
        assert!(s.contains("no targets"), "{s}");
    }

    #[test]
    fn connect_kicks_off() {
        let s = dispatch("connect_selected", "{}");
        assert!(s.contains(r#""started":true"#) || s.contains(r#""started": true"#), "{s}");
    }

    #[test]
    fn core_stop_is_nyi() {
        let s = dispatch("core_stop", "{}");
        assert!(s.contains("nyi"), "{s}");
    }

    #[test]
    fn tun_missing_enabled() {
        let s = dispatch("set_tun_cmd", "{}");
        assert!(s.contains("missing enabled"), "{s}");
    }

    #[test]
    fn proxy_missing_enabled() {
        let s = dispatch("set_system_proxy_cmd", "{}");
        assert!(s.contains("missing enabled"), "{s}");
    }

    #[test]
    fn sub_fetch_empty_url() {
        let s = dispatch("sub_fetch", "{}");
        assert!(s.contains("empty url"), "{s}");
        assert!(!s.contains(r#""started""#), "{s}");
    }

    #[test]
    fn sub_fetch_kicks_off() {
        // scheme check runs in the worker — kick-off must not wait on curl.
        let s = dispatch("sub_fetch", r#"{"url":"ftp://example.invalid"}"#);
        assert!(
            s.contains(r#""started":true"#) || s.contains(r#""started": true"#),
            "{s}"
        );
    }
}
