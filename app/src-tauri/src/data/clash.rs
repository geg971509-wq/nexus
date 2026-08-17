//! Clash YAML → sing-box outbound nodes.
//! Port of Throne `RawUpdater::updateClash` + per-protocol `ParseFromClash` / `ExportToJson`
//! (`Throne/src/configs/sub/GroupUpdater.cpp`, `include/configs/sub/clash.hpp`,
//! `src/configs/outbounds/*`, `src/configs/common/{TLS,transport,multiplex}.cpp`).
//! Real YAML via serde_yaml — not a custom regex parser.

use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// One catalog node produced from a Clash proxy entry.
#[derive(Debug, Clone)]
pub struct ClashNode {
    pub name: String,
    pub type_label: String,
    pub addr: String,
    pub outbound: Value,
}

/// Throne `RawUpdater::updateClash`: deserialize YAML, take `proxies`, map each.
/// Returns Err only on whole-document YAML failure; per-proxy errors are skipped.
/// Returns `(nodes, skipped_types)` — see [`crate::data::share_link::parse_share_body`]
/// for why the skipped half is reported rather than folded into a smaller count.
pub fn parse_clash_yaml(body: &str) -> Result<(Vec<ClashNode>, Vec<String>), String> {
    let root: Value = serde_yaml::from_str(body).map_err(|e| format!("Clash YAML parse error: {e}"))?;
    let Some(proxies) = root.get("proxies").and_then(|v| v.as_array()) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut out = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut note = |label: String| {
        if !skipped.contains(&label) {
            skipped.push(label);
        }
    };
    for p in proxies {
        if out.len() >= 5000 {
            break;
        }
        // See share_link: Xray-only VLESS parses but cannot run on sing-box.
        if s(p, "type") == "vless" && crate::data::xray::needs_xray_clash(p) {
            note(crate::data::xray::XRAY_VLESS_LABEL.to_string());
            continue;
        }
        match proxy_to_node(p) {
            Ok(Some(n)) => out.push(n),
            // Throne: per-proxy isolation — skip malformed, continue
            Ok(None) | Err(_) => note(crate::data::skipped_label(&s(p, "type"))),
        }
    }
    Ok((out, skipped))
}

fn proxy_to_node(p: &Value) -> Result<Option<ClashNode>, String> {
    let ty = s(p, "type");
    if ty.is_empty() {
        return Ok(None);
    }
    // Throne findProtocolByClashType
    let outbound = match ty.as_str() {
        "vless" => parse_vless(p)?,
        "vmess" => parse_vmess(p)?,
        "trojan" => parse_trojan(p)?,
        "ss" => parse_ss(p)?,
        "http" => parse_http(p)?,
        "socks5" => parse_socks(p)?,
        "hysteria" | "hysteria2" => parse_hysteria(p, &ty)?,
        "tuic" => parse_tuic(p)?,
        "anytls" => parse_anytls(p)?,
        // ssh supported in Throne table but rarely needed for sub feeds
        "ssh" => return Ok(None),
        _ => return Ok(None),
    };
    let name = s(p, "name");
    let server = outbound
        .get("server")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let port = outbound
        .get("server_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if server.is_empty() || port == 0 {
        return Ok(None);
    }
    let type_label = type_label_for(&ty, &outbound);
    let display_name = if name.is_empty() {
        format!("{type_label}-?")
    } else {
        name
    };
    Ok(Some(ClashNode {
        name: display_name,
        type_label,
        addr: format!("{server}:{port}"),
        outbound,
    }))
}

fn type_label_for(clash_ty: &str, outbound: &Value) -> String {
    match clash_ty {
        "vless" => "VLESS".into(),
        "vmess" => "VMess".into(),
        "trojan" => "Trojan".into(),
        "ss" => "SS".into(),
        "socks5" => "SOCKS".into(),
        "hysteria" => "Hysteria".into(),
        "hysteria2" => "Hysteria2".into(),
        "tuic" => "TUIC".into(),
        "anytls" => "AnyTLS".into(),
        "http" => {
            if outbound
                .get("tls")
                .and_then(|t| t.get("enabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                "HTTPS".into()
            } else {
                "HTTP".into()
            }
        }
        other => other.to_string(),
    }
}

// ── field helpers (Throne load_opt / MyBool / MyInt tolerant) ──

fn s(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        _ => String::new(),
    }
}

fn s_any(v: &Value, keys: &[&str]) -> String {
    for k in keys {
        let x = s(v, k);
        if !x.is_empty() {
            return x;
        }
    }
    String::new()
}

fn b(v: &Value, key: &str) -> bool {
    match v.get(key) {
        Some(Value::Bool(x)) => *x,
        Some(Value::Number(n)) => n.as_i64() == Some(1),
        Some(Value::String(s)) => s == "true" || s == "1",
        _ => false,
    }
}

fn i(v: &Value, key: &str) -> i64 {
    match v.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn port_of(p: &Value) -> u16 {
    let n = i(p, "port");
    if n > 0 && n <= 65535 {
        n as u16
    } else {
        0
    }
}

fn str_list(v: &Value, key: &str) -> Vec<String> {
    match v.get(key) {
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

fn map_str(v: &Value, key: &str) -> HashMap<String, String> {
    match v.get(key) {
        Some(Value::Object(m)) => m
            .iter()
            .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        _ => HashMap::new(),
    }
}

// ── outbound base (Throne outbound::ParseFromClash / ExportToJson) ──

fn base_outbound(p: &Value, ty: &str) -> Map<String, Value> {
    let mut o = Map::new();
    o.insert("type".into(), json!(ty));
    o.insert("tag".into(), json!("proxy"));
    let server = s(p, "server");
    if !server.is_empty() {
        o.insert("server".into(), json!(server));
    }
    let port = port_of(p);
    if port > 0 {
        o.insert("server_port".into(), json!(port));
    }
    o
}

// ── TLS (Throne TLS::ParseFromClash + ExportToJson subset) ──

fn tls_from_clash(p: &Value, force_enabled: bool) -> Option<Value> {
    let mut enabled = b(p, "tls") || force_enabled;
    let servername = s(p, "servername");
    let sni = s(p, "sni");
    let server = s(p, "server");
    let server_name = if !servername.is_empty() {
        servername
    } else if !sni.is_empty() {
        sni
    } else {
        server
    };
    let insecure = b(p, "skip-cert-verify");
    let alpn = str_list(p, "alpn");
    let fp = s_any(p, &["client-fingerprint", "client_fingerprint"]);
    let reality = p.get("reality-opts").or_else(|| p.get("reality_opts"));
    let pbk = reality.map(|r| s(r, "public-key")).unwrap_or_default();
    let sid = reality.map(|r| s(r, "short-id")).unwrap_or_default();
    if !pbk.is_empty() {
        enabled = true;
    }
    if !enabled && pbk.is_empty() && fp.is_empty() && alpn.is_empty() && !insecure {
        // still export when force? force_enabled already set enabled
        if !force_enabled {
            return None;
        }
    }
    if !enabled {
        return None;
    }
    let mut tls = Map::new();
    tls.insert("enabled".into(), json!(true));
    if !server_name.is_empty() {
        tls.insert("server_name".into(), json!(server_name));
    }
    if insecure {
        tls.insert("insecure".into(), json!(true));
    }
    if !alpn.is_empty() {
        tls.insert("alpn".into(), json!(alpn));
    }
    if b(p, "disable-sni") {
        tls.insert("disable_sni".into(), json!(true));
    }
    if !fp.is_empty() {
        tls.insert(
            "utls".into(),
            json!({ "enabled": true, "fingerprint": fp }),
        );
    }
    if !pbk.is_empty() {
        let mut r = Map::new();
        r.insert("enabled".into(), json!(true));
        r.insert("public_key".into(), json!(pbk));
        if !sid.is_empty() {
            r.insert("short_id".into(), json!(sid));
        }
        tls.insert("reality".into(), Value::Object(r));
    }
    Some(Value::Object(tls))
}

// ── Transport (Throne Transport::ParseFromClash + ExportToJson) ──

fn transport_from_clash(p: &Value) -> Option<Value> {
    let network = s(p, "network");
    let ws = p.get("ws-opts").or_else(|| p.get("ws_opts"));
    let grpc = p.get("grpc-opts").or_else(|| p.get("grpc_opts"));
    let h2 = p.get("h2-opts").or_else(|| p.get("h2_opts"));
    let http = p.get("http-opts").or_else(|| p.get("http_opts"));

    // ws path / network == ws
    if ws.is_some() || network == "ws" {
        let ws = ws.cloned().unwrap_or(Value::Object(Map::new()));
        let v2ray_upgrade = b(&ws, "v2ray-http-upgrade") || b(&ws, "v2ray_http_upgrade");
        let ty = if v2ray_upgrade { "httpupgrade" } else { "ws" };
        let mut t = Map::new();
        t.insert("type".into(), json!(ty));
        let path = s(&ws, "path");
        if !path.is_empty() {
            // Throne ExportToJson: path?ed= → max_early_data
            if let Some((base, ed)) = path.split_once("?ed=") {
                t.insert("path".into(), json!(base));
                if let Ok(n) = ed.parse::<i64>() {
                    t.insert("max_early_data".into(), json!(n));
                    t.insert(
                        "early_data_header_name".into(),
                        json!("Sec-WebSocket-Protocol"),
                    );
                }
            } else {
                t.insert("path".into(), json!(path));
            }
        }
        let med = i(&ws, "max-early-data");
        if med > 0 && !t.contains_key("max_early_data") {
            t.insert("max_early_data".into(), json!(med));
        }
        let edhn = s_any(&ws, &["early-data-header-name", "early_data_header_name"]);
        if !edhn.is_empty() && !t.contains_key("early_data_header_name") {
            t.insert("early_data_header_name".into(), json!(edhn));
        }
        let servername = s(p, "servername");
        let headers = map_str(&ws, "headers");
        let host = if !servername.is_empty() {
            servername
        } else {
            headers.get("Host").cloned().unwrap_or_default()
        };
        if ty == "ws" {
            let mut hobj = Map::new();
            for (k, v) in &headers {
                hobj.insert(k.clone(), json!(v));
            }
            if !host.is_empty() {
                hobj.insert("Host".into(), json!(host));
            }
            if !hobj.is_empty() {
                t.insert("headers".into(), Value::Object(hobj));
            }
        } else if !host.is_empty() {
            // httpupgrade: host field
            t.insert("host".into(), json!(host));
            if !headers.is_empty() {
                let mut hobj = Map::new();
                for (k, v) in &headers {
                    hobj.insert(k.clone(), json!(v));
                }
                t.insert("headers".into(), Value::Object(hobj));
            }
        }
        return Some(Value::Object(t));
    }

    // grpc
    if let Some(g) = grpc {
        let svc = s_any(g, &["grpc-service-name", "grpc_service_name"]);
        if !svc.is_empty() {
            let mut t = Map::new();
            t.insert("type".into(), json!("grpc"));
            t.insert("service_name".into(), json!(svc));
            return Some(Value::Object(t));
        }
    }

    // h2
    if h2.is_some() || network == "h2" {
        let h2 = h2.cloned().unwrap_or(Value::Object(Map::new()));
        let mut t = Map::new();
        t.insert("type".into(), json!("http"));
        let hosts = str_list(&h2, "host");
        if let Some(h) = hosts.first() {
            t.insert("host".into(), json!(h));
        }
        let path = s(&h2, "path");
        if !path.is_empty() {
            t.insert("path".into(), json!(path));
        }
        return Some(Value::Object(t));
    }

    // http-opts
    if let Some(h) = http {
        if !s(h, "method").is_empty() || h.get("path").is_some() {
            let mut t = Map::new();
            t.insert("type".into(), json!("http"));
            // headers Host first element
            if let Some(hdrs) = h.get("headers").and_then(|x| x.as_object()) {
                if let Some(host_v) = hdrs.get("Host") {
                    let host = match host_v {
                        Value::String(s) => s.clone(),
                        Value::Array(a) => a
                            .first()
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        _ => String::new(),
                    };
                    if !host.is_empty() {
                        t.insert("host".into(), json!(host));
                    }
                }
            }
            let paths = str_list(h, "path");
            if let Some(path) = paths.first() {
                t.insert("path".into(), json!(path));
            }
            let method = s(h, "method");
            if !method.is_empty() {
                t.insert("method".into(), json!(method));
            }
            return Some(Value::Object(t));
        }
    }

    None
}

// ── Multiplex (Throne Multiplex::ParseFromClash + ExportToJson) ──

fn multiplex_from_clash(p: &Value) -> Option<Value> {
    let smux = p.get("smux")?;
    let enabled = b(smux, "enabled");
    // Throne: always sets unspecified=false when ParseFromClash runs;
    // ExportToJson omits only when unspecified. We only emit when smux present.
    let mut m = Map::new();
    m.insert("enabled".into(), json!(enabled));
    if !enabled {
        return Some(Value::Object(m));
    }
    let protocol = s(smux, "protocol");
    if !protocol.is_empty() {
        m.insert("protocol".into(), json!(protocol));
    }
    let max_streams = i(smux, "max-streams");
    if max_streams > 0 {
        m.insert("max_streams".into(), json!(max_streams));
    } else {
        let max_conn = i(smux, "max-connections");
        let min_streams = i(smux, "min-streams");
        if max_conn > 0 {
            m.insert("max_connections".into(), json!(max_conn));
        }
        if min_streams > 0 {
            m.insert("min_streams".into(), json!(min_streams));
        }
    }
    if b(smux, "padding") {
        m.insert("padding".into(), json!(true));
    }
    Some(Value::Object(m))
}

// ── protocols ──

fn parse_vless(p: &Value) -> Result<Value, String> {
    // vless::ParseFromClash + ExportToJson
    let mut o = base_outbound(p, "vless");
    let uuid = s(p, "uuid");
    if uuid.is_empty() {
        return Err("vless: empty uuid".into());
    }
    o.insert("uuid".into(), json!(uuid));
    let flow = s(p, "flow");
    if !flow.is_empty() {
        o.insert("flow".into(), json!(flow));
    }
    let pe = s_any(p, &["packet-encoding", "packet_encoding"]);
    o.insert("packet_encoding".into(), json!(pe));
    // Throne TLS::ParseFromClash: enabled=object.tls; reality opts also enable
    let reality = p.get("reality-opts").or_else(|| p.get("reality_opts"));
    let has_reality = reality
        .map(|r| !s(r, "public-key").is_empty())
        .unwrap_or(false);
    if let Some(tls) = tls_from_clash(p, b(p, "tls") || has_reality) {
        o.insert("tls".into(), tls);
    }
    if let Some(tr) = transport_from_clash(p) {
        o.insert("transport".into(), tr);
    }
    if let Some(mx) = multiplex_from_clash(p) {
        o.insert("multiplex".into(), mx);
    }
    Ok(Value::Object(o))
}

fn parse_vmess(p: &Value) -> Result<Value, String> {
    let mut o = base_outbound(p, "vmess");
    let uuid = s(p, "uuid");
    if uuid.is_empty() {
        return Err("vmess: empty uuid".into());
    }
    o.insert("uuid".into(), json!(uuid));
    let cipher = s(p, "cipher");
    if !cipher.is_empty() && cipher != "auto" {
        o.insert("security".into(), json!(cipher));
    }
    let alter = i(p, "alterId");
    if alter > 0 {
        o.insert("alter_id".into(), json!(alter));
    }
    let pe = s_any(p, &["packet-encoding", "packet_encoding"]);
    if !pe.is_empty() {
        o.insert("packet_encoding".into(), json!(pe));
    }
    if let Some(tls) = tls_from_clash(p, false) {
        o.insert("tls".into(), tls);
    }
    if let Some(tr) = transport_from_clash(p) {
        o.insert("transport".into(), tr);
    }
    if let Some(mx) = multiplex_from_clash(p) {
        o.insert("multiplex".into(), mx);
    }
    Ok(Value::Object(o))
}

fn parse_trojan(p: &Value) -> Result<Value, String> {
    let mut o = base_outbound(p, "trojan");
    let password = s(p, "password");
    if password.is_empty() {
        return Err("trojan: empty password".into());
    }
    o.insert("password".into(), json!(password));
    // Throne: tls->ParseFromClash then tls->enabled = true
    if let Some(tls) = tls_from_clash(p, true) {
        o.insert("tls".into(), tls);
    }
    if let Some(tr) = transport_from_clash(p) {
        o.insert("transport".into(), tr);
    }
    if let Some(mx) = multiplex_from_clash(p) {
        o.insert("multiplex".into(), mx);
    }
    Ok(Value::Object(o))
}

fn parse_ss(p: &Value) -> Result<Value, String> {
    let mut o = base_outbound(p, "shadowsocks");
    let method = s(p, "cipher");
    let password = s(p, "password");
    if method.is_empty() || password.is_empty() {
        return Err("ss: need cipher+password".into());
    }
    o.insert("method".into(), json!(method));
    o.insert("password".into(), json!(password));
    if b(p, "udp-over-tcp") {
        o.insert("udp_over_tcp".into(), json!(true));
    }
    // plugin (Throne shadowsocks::ParseFromClash)
    let plugin = s(p, "plugin");
    if !plugin.is_empty() {
        if let Some(opts) = p.get("plugin-opts").or_else(|| p.get("plugin_opts")) {
            if plugin == "v2ray-plugin" {
                o.insert("plugin".into(), json!("v2ray-plugin"));
                let mut parts = Vec::new();
                if b(opts, "tls") {
                    parts.push("tls".to_string());
                }
                let host = s(opts, "host");
                if !host.is_empty() {
                    parts.push(format!("host={host}"));
                }
                let path = s(opts, "path");
                if !path.is_empty() {
                    parts.push(format!("path={path}"));
                }
                let mode = s(opts, "mode");
                if !mode.is_empty() {
                    parts.push(format!("mode={mode}"));
                }
                if b(opts, "mux") {
                    parts.push("mux".to_string());
                }
                o.insert("plugin_opts".into(), json!(parts.join(";")));
            } else if plugin == "obfs" {
                o.insert("plugin".into(), json!("obfs-local"));
                let mut parts = Vec::new();
                let mode = s(opts, "mode");
                if !mode.is_empty() {
                    parts.push(format!("obfs={mode}"));
                }
                let host = s(opts, "host");
                if !host.is_empty() {
                    parts.push(format!("obfs-host={host}"));
                }
                o.insert("plugin_opts".into(), json!(parts.join(";")));
            }
        }
    }
    if let Some(mx) = multiplex_from_clash(p) {
        o.insert("multiplex".into(), mx);
    }
    Ok(Value::Object(o))
}

fn parse_http(p: &Value) -> Result<Value, String> {
    let mut o = base_outbound(p, "http");
    let username = s(p, "username");
    let password = s(p, "password");
    if !username.is_empty() {
        o.insert("username".into(), json!(username));
    }
    if !password.is_empty() {
        o.insert("password".into(), json!(password));
    }
    if let Some(tls) = tls_from_clash(p, false) {
        o.insert("tls".into(), tls);
    }
    Ok(Value::Object(o))
}

fn parse_socks(p: &Value) -> Result<Value, String> {
    let mut o = base_outbound(p, "socks");
    let username = s(p, "username");
    let password = s(p, "password");
    if !username.is_empty() {
        o.insert("username".into(), json!(username));
    }
    if !password.is_empty() {
        o.insert("password".into(), json!(password));
    }
    Ok(Value::Object(o))
}

fn any_to_mbps(raw: &str) -> i64 {
    // Throne hysteria anyToMbps
    let s = raw.trim();
    if s.is_empty() {
        return 0;
    }
    if let Ok(n) = s.parse::<i64>() {
        return n;
    }
    let re = regex_lite_num_unit(s);
    re
}

/// Minimal unit parse without new crate (Throne regex ^(\d+)([KMGT]?)([Bb]?)).
fn regex_lite_num_unit(s: &str) -> i64 {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return 0;
    }
    let v: f64 = s[..i].parse().unwrap_or(0.0);
    let rest = s[i..].to_ascii_uppercase();
    let (unit, typ) = if rest.is_empty() {
        ("", "")
    } else {
        let u = rest.chars().next().unwrap_or(' ');
        let unit = match u {
            'K' | 'M' | 'G' | 'T' => {
                let t = rest.chars().nth(1).unwrap_or(' ');
                (u, t)
            }
            'B' => (' ', 'B'),
            _ => (' ', ' '),
        };
        (
            match unit.0 {
                'K' => "K",
                'M' => "M",
                'G' => "G",
                'T' => "T",
                _ => "",
            },
            if unit.1 == 'B' { "B" } else { "" },
        )
    };
    let mut n = 1.0_f64;
    match unit {
        "K" => n = 0.001,
        "M" => n = 1.0,
        "G" => n = 1000.0,
        "T" => n = 1_000_000.0,
        _ => {}
    }
    if typ == "B" {
        n *= 8.0;
    }
    (v * n) as i64
}

fn parse_hysteria(p: &Value, clash_ty: &str) -> Result<Value, String> {
    let ver = if clash_ty == "hysteria" { "1" } else { "2" };
    let ty = if ver == "1" { "hysteria" } else { "hysteria2" };
    let mut o = base_outbound(p, ty);
    let ports = s(p, "ports");
    if !ports.is_empty() {
        let arr: Vec<String> = ports
            .split(|c| c == ',' || c == '/')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect();
        if !arr.is_empty() {
            o.insert("server_ports".into(), json!(arr));
        }
    }
    let up = s(p, "up");
    let down = s(p, "down");
    let up_mbps = any_to_mbps(&up);
    let down_mbps = any_to_mbps(&down);
    if up_mbps > 0 {
        o.insert("up_mbps".into(), json!(up_mbps));
    }
    if down_mbps > 0 {
        o.insert("down_mbps".into(), json!(down_mbps));
    }
    if ver == "1" {
        let auth = s_any(p, &["auth-str", "auth_str"]);
        if !auth.is_empty() {
            o.insert("auth_str".into(), json!(auth));
        }
        let obfs = s(p, "obfs");
        if !obfs.is_empty() {
            o.insert("obfs".into(), json!(obfs));
        }
        let rw = {
            let a = i(p, "recv-window");
            if a > 0 {
                a
            } else {
                i(p, "recv_window")
            }
        };
        if rw > 0 {
            o.insert("recv_window".into(), json!(rw));
        }
        let rwc = {
            let a = i(p, "recv-window-conn");
            if a > 0 {
                a
            } else {
                i(p, "recv_window_conn")
            }
        };
        if rwc > 0 {
            o.insert("recv_window_conn".into(), json!(rwc));
        }
        if b(p, "disable_mtu_discovery") {
            o.insert("disable_mtu_discovery".into(), json!(true));
        }
    } else {
        let password = s(p, "password");
        if !password.is_empty() {
            o.insert("password".into(), json!(password));
        }
        let obfs_pw = s_any(p, &["obfs-password", "obfs_password"]);
        if !obfs_pw.is_empty() {
            o.insert(
                "obfs".into(),
                json!({ "type": "salamander", "password": obfs_pw }),
            );
        }
    }
    if let Some(tls) = tls_from_clash(p, true) {
        o.insert("tls".into(), tls);
    }
    Ok(Value::Object(o))
}

fn parse_tuic(p: &Value) -> Result<Value, String> {
    let mut o = base_outbound(p, "tuic");
    // Throne: if !object.ip.empty() server = ip
    let ip = s(p, "ip");
    if !ip.is_empty() {
        o.insert("server".into(), json!(ip));
    }
    let uuid = s(p, "uuid");
    if uuid.is_empty() {
        return Err("tuic: empty uuid".into());
    }
    o.insert("uuid".into(), json!(uuid));
    let password = s(p, "password");
    if !password.is_empty() {
        o.insert("password".into(), json!(password));
    }
    let cc = s_any(p, &["congestion-controller", "congestion_controller"]);
    if !cc.is_empty() {
        o.insert("congestion_control".into(), json!(cc));
    }
    let urm = s_any(p, &["udp-relay-mode", "udp_relay_mode"]);
    if !urm.is_empty() {
        o.insert("udp_relay_mode".into(), json!(urm));
    }
    if b(p, "reduce-rtt") {
        o.insert("zero_rtt_handshake".into(), json!(true));
    }
    let hb = i(p, "heartbeat-interval");
    if hb > 0 {
        o.insert("heartbeat".into(), json!(format!("{hb}ms")));
    }
    if let Some(tls) = tls_from_clash(p, true) {
        o.insert("tls".into(), tls);
    }
    Ok(Value::Object(o))
}

fn parse_anytls(p: &Value) -> Result<Value, String> {
    let mut o = base_outbound(p, "anytls");
    let password = s(p, "password");
    if !password.is_empty() {
        o.insert("password".into(), json!(password));
    }
    let isci = i(p, "idle-session-check-interval");
    if isci > 0 {
        o.insert(
            "idle_session_check_interval".into(),
            json!(format!("{isci}s")),
        );
    }
    let ist = i(p, "idle-session-timeout");
    if ist > 0 {
        o.insert("idle_session_timeout".into(), json!(format!("{ist}s")));
    }
    let mis = i(p, "min-idle-session");
    if mis > 0 {
        o.insert("min_idle_session".into(), json!(mis));
    }
    if let Some(tls) = tls_from_clash(p, true) {
        o.insert("tls".into(), tls);
    }
    Ok(Value::Object(o))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
proxies:
  - name: "vless-ws"
    type: vless
    server: 1.2.3.4
    port: 443
    uuid: 12345678-1234-1234-1234-123456789abc
    tls: true
    servername: example.com
    network: ws
    client-fingerprint: chrome
    ws-opts:
      path: /ray
      headers:
        Host: example.com
    reality-opts:
      public-key: abcd
      short-id: "01"
  - name: "ss-basic"
    type: ss
    server: 5.6.7.8
    port: 8388
    cipher: aes-256-gcm
    password: secret
  - name: "trojan-a"
    type: trojan
    server: 9.9.9.9
    port: 443
    password: pass
    sni: t.example.com
  - name: "hy2"
    type: hysteria2
    server: 8.8.8.8
    port: 443
    password: hy2pass
    sni: hy.example.com
    skip-cert-verify: true
  - { name: "inline-vmess", type: vmess, server: 7.7.7.7, port: 80, uuid: 12345678-1234-1234-1234-123456789abc, alterId: 0, cipher: auto, network: ws, ws-opts: { path: /v, headers: { Host: h.com } } }
proxy-groups:
  - name: PROXY
    type: select
    proxies: [vless-ws]
rules:
  - MATCH,PROXY
"#;

    #[test]
    fn parse_nested_opts_and_skip_groups() {
        let (nodes, _skipped) = parse_clash_yaml(SAMPLE).expect("yaml");
        assert_eq!(nodes.len(), 5, "proxy-groups/rules must not become nodes");

        let v = &nodes[0];
        assert_eq!(v.type_label, "VLESS");
        assert_eq!(v.addr, "1.2.3.4:443");
        assert_eq!(v.outbound["type"], "vless");
        assert_eq!(v.outbound["uuid"], "12345678-1234-1234-1234-123456789abc");
        assert_eq!(v.outbound["tls"]["enabled"], true);
        assert_eq!(v.outbound["tls"]["server_name"], "example.com");
        assert_eq!(v.outbound["tls"]["reality"]["public_key"], "abcd");
        assert_eq!(v.outbound["tls"]["utls"]["fingerprint"], "chrome");
        assert_eq!(v.outbound["transport"]["type"], "ws");
        assert_eq!(v.outbound["transport"]["path"], "/ray");
        assert_eq!(v.outbound["transport"]["headers"]["Host"], "example.com");

        let ss = &nodes[1];
        assert_eq!(ss.outbound["type"], "shadowsocks");
        assert_eq!(ss.outbound["method"], "aes-256-gcm");
        assert_eq!(ss.outbound["password"], "secret");

        let tr = &nodes[2];
        assert_eq!(tr.outbound["type"], "trojan");
        assert_eq!(tr.outbound["tls"]["enabled"], true);
        assert_eq!(tr.outbound["tls"]["server_name"], "t.example.com");

        let hy = &nodes[3];
        assert_eq!(hy.outbound["type"], "hysteria2");
        assert_eq!(hy.outbound["password"], "hy2pass");
        assert_eq!(hy.outbound["tls"]["insecure"], true);

        let vm = &nodes[4];
        assert_eq!(vm.outbound["type"], "vmess");
        assert_eq!(vm.outbound["transport"]["type"], "ws");
        assert_eq!(vm.outbound["transport"]["path"], "/v");
    }

    #[test]
    fn empty_proxies_ok() {
        let (nodes, _skipped) = parse_clash_yaml("port: 7890\nproxies: []\n").unwrap();
        assert!(nodes.is_empty());
    }
}
