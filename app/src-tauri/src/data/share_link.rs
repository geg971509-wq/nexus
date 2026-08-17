//! Share-link → sing-box outbound JSON (upstream configs/outbounds/* ParseFromLink + ExportToJson subset).
//! Supported: vless:// vmess:// trojan:// ss:// socks:// http(s):// anytls:// tuic:// (+ optional JSON outbound object).
//! Full transport/reality parity can grow field-by-field from upstream when needed.

use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// One catalog node from a share-line body (vless/vmess/trojan/ss/…).
#[derive(Debug, Clone)]
pub struct ShareNode {
    pub name: String,
    pub type_label: String,
    pub addr: String,
    pub link: String,
    pub outbound: Value,
}

/// Parse a free-list / share-URI body into catalog nodes with full outbound JSON.
/// Per-line isolation: bad lines are skipped. Cap 5000 like Clash path.
/// Returns `(nodes, skipped_schemes)`. The second half exists because "imported 0
/// nodes" alone cannot tell an empty subscription apart from one full of a
/// protocol we do not parse — the user has no way to report the difference.
pub fn parse_share_body(body: &str) -> (Vec<ShareNode>, Vec<String>) {
    let mut out = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut note = |label: String| {
        if !skipped.contains(&label) {
            skipped.push(label);
        }
    };
    for line in body.lines() {
        if out.len() >= 5000 {
            break;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // subscription URL, not a proxy share
        if (line.starts_with("http://") || line.starts_with("https://")) && !line.contains('@') {
            continue;
        }
        // Xray-only VLESS would parse fine and then fail at connect, because
        // sing-box cannot carry it and the shell has no Xray config generator.
        // Turn it away by name instead.
        if line.to_ascii_lowercase().starts_with("vless://")
            && crate::data::xray::needs_xray_link(line)
        {
            note(crate::data::xray::XRAY_VLESS_LABEL.to_string());
            continue;
        }
        match parse_to_outbound(line) {
            Ok(outbound) => match share_node_from_outbound(line, &outbound, out.len()) {
                Some(n) => out.push(n),
                // Parsed, but no usable server/port — still a dropped entry.
                None => note(crate::data::scheme_of(line)),
            },
            Err(_) => note(crate::data::scheme_of(line)),
        }
    }
    (out, skipped)
}

fn share_node_from_outbound(link: &str, outbound: &Value, idx: usize) -> Option<ShareNode> {
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
        return None;
    }
    let ty = outbound
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("vless");
    let type_label = match ty {
        "vless" => "VLESS",
        "vmess" => "VMess",
        "trojan" => "Trojan",
        "shadowsocks" => "SS",
        "socks" => "SOCKS",
        "http" => {
            if outbound
                .get("tls")
                .and_then(|t| t.get("enabled"))
                .and_then(|e| e.as_bool())
                .unwrap_or(false)
            {
                "HTTPS"
            } else {
                "HTTP"
            }
        }
        "anytls" => "AnyTLS",
        "tuic" => "TUIC",
        "hysteria2" => "Hysteria2",
        "hysteria" => "Hysteria",
        other => other,
    }
    .to_string();
    let name = fragment_name(link).unwrap_or_else(|| {
        // vmess v2rayN: ps/name inside JSON
        if ty == "vmess" {
            if let Some(ps) = vmess_remark_from_link(link) {
                return ps;
            }
        }
        format!("{type_label}-{}", idx + 1)
    });
    let addr = format!("{server}:{port}");
    Some(ShareNode {
        name,
        type_label,
        addr,
        link: link.to_string(),
        outbound: outbound.clone(),
    })
}

fn fragment_name(link: &str) -> Option<String> {
    let hash = link.find('#')?;
    let frag = &link[hash + 1..];
    if frag.is_empty() {
        return None;
    }
    let name = pct_decode(frag).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn vmess_remark_from_link(link: &str) -> Option<String> {
    let rest = link.split_once("://").map(|(_, r)| r.trim())?;
    let b64 = rest.split('#').next().unwrap_or(rest).trim();
    if b64.contains('@') {
        return None;
    }
    let raw = b64_decode_std(b64).ok()?;
    let txt = String::from_utf8_lossy(&raw);
    let j: Value = serde_json::from_str(txt.trim()).ok()?;
    let ps = j
        .get("ps")
        .or_else(|| j.get("name"))
        .or_else(|| j.get("remark"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    Some(ps.to_string())
}

/// Parse one share URL or a JSON outbound object string into a sing-box outbound.
pub fn parse_to_outbound(input: &str) -> Result<Value, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty share link".into());
    }
    if s.starts_with('{') {
        let v: Value = serde_json::from_str(s).map_err(|e| format!("json outbound: {e}"))?;
        if v.get("type").and_then(|t| t.as_str()).is_some() {
            return Ok(v);
        }
        return Err("json missing type".into());
    }
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("vless://") {
        parse_vless(s)
    } else if lower.starts_with("vmess://") {
        parse_vmess(s)
    } else if lower.starts_with("trojan://") {
        parse_trojan(s)
    } else if lower.starts_with("ss://") {
        parse_ss(s)
    } else if lower.starts_with("socks://") || lower.starts_with("socks5://") {
        parse_socks(s)
    } else if lower.starts_with("anytls://") {
        parse_anytls(s)
    } else if lower.starts_with("tuic://") {
        parse_tuic(s)
    } else if lower.starts_with("hysteria2://") || lower.starts_with("hy2://") {
        parse_hysteria(s, 2)
    } else if lower.starts_with("hysteria://") {
        parse_hysteria(s, 1)
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        // only treat as http proxy if looks like user@host:port (not a subscription URL)
        if s.contains('@') {
            parse_http_proxy(s)
        } else {
            Err("http(s) without user@ is not a proxy share link".into())
        }
    } else {
        Err(format!("unsupported share scheme: {}", s.chars().take(16).collect::<String>()))
    }
}

fn parse_vless(link: &str) -> Result<Value, String> {
    // vless::ParseFromLink
    let u = UrlParts::parse(link)?;
    if u.user.is_empty() || u.host.is_empty() {
        return Err("vless: need uuid@host".into());
    }
    let port = if u.port == 0 { 443 } else { u.port };
    let q = &u.query;
    let mut o = Map::new();
    o.insert("type".into(), json!("vless"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(port));
    o.insert("uuid".into(), json!(u.user));
    if let Some(flow) = q.get("flow").filter(|s| !s.is_empty()) {
        o.insert("flow".into(), json!(flow));
    }
    let pe = q
        .get("packetEncoding")
        .or_else(|| q.get("packet_encoding"))
        .map(|s| s.as_str())
        .unwrap_or("xudp");
    if pe != "none" && !pe.is_empty() {
        o.insert("packet_encoding".into(), json!(pe));
    }
    if let Some(tls) = tls_from_query(q, true) {
        o.insert("tls".into(), tls);
    }
    if let Some(tr) = transport_from_query(q) {
        o.insert("transport".into(), tr);
    }
    Ok(Value::Object(o))
}


fn parse_vmess(link: &str) -> Result<Value, String> {
    // vmess::ParseFromLink:
    // 1) v2rayN: base64 JSON after vmess://
    // 2) standard URI: vmess://uuid@host:port?security=tls&…
    let rest = link
        .split_once("://")
        .map(|(_, r)| r.trim())
        .ok_or("vmess: bad scheme")?;
    let b64 = rest.split('#').next().unwrap_or(rest).trim();
    // Prefer JSON body when whole body (no @) is valid b64→json
    if !b64.contains('@') {
        if let Ok(raw) = b64_decode_std(b64) {
            let txt = String::from_utf8_lossy(&raw);
            if let Ok(j) = serde_json::from_str::<Value>(txt.trim()) {
                return vmess_from_v2rayn_json(&j);
            }
        }
    }
    parse_vmess_uri(link)
}

fn vmess_from_v2rayn_json(j: &Value) -> Result<Value, String> {
    let uuid = j
        .get("id")
        .or_else(|| j.get("uuid"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let host = j
        .get("add")
        .or_else(|| j.get("host"))
        .or_else(|| j.get("server"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if uuid.is_empty() || host.is_empty() {
        return Err("vmess: need id+add".into());
    }
    if !looks_like_uuid(&uuid) {
        return Err("vmess: id is not a UUID (refusing fake/share-name body)".into());
    }
    let port = j
        .get("port")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(443) as u16;
    // scy = cipher (auto/aes-128-gcm/…); not TLS
    let security = j
        .get("scy")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");
    let alter_id = j
        .get("aid")
        .or_else(|| j.get("alterId"))
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(0);
    let mut o = Map::new();
    o.insert("type".into(), json!("vmess"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(host));
    o.insert("server_port".into(), json!(port));
    o.insert("uuid".into(), json!(uuid));
    if security != "auto" {
        o.insert("security".into(), json!(security));
    }
    if alter_id > 0 {
        o.insert("alter_id".into(), json!(alter_id));
    }
    let tls_raw = j
        .get("tls")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // SNI only from sni field (host is often WS Host header, not SNI)
    let sni = j.get("sni").and_then(|v| v.as_str()).unwrap_or("");
    // "tls"/"reality" enable; bare sni also enables (common free-list style)
    let tls_on = tls_raw == "tls" || tls_raw == "reality" || !sni.is_empty();
    if tls_on {
        let mut tls = Map::new();
        tls.insert("enabled".into(), json!(true));
        if !sni.is_empty() {
            tls.insert("server_name".into(), json!(sni));
        }
        if let Some(fp) = j.get("fp").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            tls.insert(
                "utls".into(),
                json!({"enabled": true, "fingerprint": fp}),
            );
        }
        if tls_raw == "reality" {
            let mut reality = Map::new();
            reality.insert("enabled".into(), json!(true));
            if let Some(pbk) = j.get("pbk").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                reality.insert("public_key".into(), json!(pbk));
            }
            if let Some(sid) = j.get("sid").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                reality.insert("short_id".into(), json!(sid));
            }
            tls.insert("reality".into(), Value::Object(reality));
        }
        o.insert("tls".into(), Value::Object(tls));
    }
    let net = j
        .get("net")
        .or_else(|| j.get("network"))
        .and_then(|v| v.as_str())
        .unwrap_or("tcp");
    let net = if net == "h2" { "http" } else { net };
    let path = j
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let host_hdr = j
        .get("host")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    // Non-tcp nets always get transport; tcp/raw with host/path → http camouflage (v2rayN style)
    if (net != "tcp" && net != "raw") || !path.is_empty() || !host_hdr.is_empty() {
        let mut t = Map::new();
        let ty = match net {
            "ws" | "websocket" => "ws",
            "grpc" => "grpc",
            "h2" | "http" => "http",
            "tcp" | "raw" if !path.is_empty() || !host_hdr.is_empty() => "http",
            other => other,
        };
        t.insert("type".into(), json!(ty));
        if !path.is_empty() {
            t.insert("path".into(), json!(path));
        }
        if !host_hdr.is_empty() {
            if ty == "ws" || ty == "http" {
                t.insert("headers".into(), json!({"Host": host_hdr}));
            } else {
                t.insert("host".into(), json!(host_hdr));
            }
        }
        o.insert("transport".into(), Value::Object(t));
    }
    Ok(Value::Object(o))
}

fn looks_like_uuid(s: &str) -> bool {
    // standard 8-4-4-4-12 or 32 hex
    let t = s.trim();
    if t.len() == 36 {
        let b = t.as_bytes();
        return b[8] == b'-'
            && b[13] == b'-'
            && b[18] == b'-'
            && b[23] == b'-'
            && t.chars().filter(|c| *c != '-').all(|c| c.is_ascii_hexdigit());
    }
    t.len() == 32 && t.chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_vmess_uri(link: &str) -> Result<Value, String> {
    // standard VMess URL: uuid@host:port?encryption=&security=tls&…
    let u = UrlParts::parse(link)?;
    if u.user.is_empty() || u.host.is_empty() {
        return Err("vmess: need uuid@host".into());
    }
    if !looks_like_uuid(&u.user) {
        // Nexus used to fabricate vmess://btoa(name)@host — reject clearly
        return Err(
            "vmess: user is not a UUID (clipboard/QR fake link — re-update subscription for real outbound)"
                .into(),
        );
    }
    let port = if u.port == 0 { 443 } else { u.port };
    let mut o = Map::new();
    o.insert("type".into(), json!("vmess"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(port));
    o.insert("uuid".into(), json!(u.user));
    // encryption = cipher (upstream GetQueryValue encryption default auto)
    let enc = u
        .query
        .get("encryption")
        .or_else(|| u.query.get("scy"))
        .map(|s| s.as_str())
        .unwrap_or("auto");
    if enc != "auto" && !enc.is_empty() {
        o.insert("security".into(), json!(enc));
    }
    if let Some(aid) = u
        .query
        .get("alterId")
        .or_else(|| u.query.get("aid"))
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0)
    {
        o.insert("alter_id".into(), json!(aid));
    }
    // TLS: security=tls|reality (upstream TLS::ParseFromLink) — NOT the cipher field
    if let Some(tls) = tls_from_query(&u.query, true) {
        o.insert("tls".into(), tls);
    } else if u
        .query
        .get("security")
        .map(|s| s.eq_ignore_ascii_case("tls") || s.eq_ignore_ascii_case("reality"))
        .unwrap_or(false)
    {
        o.insert("tls".into(), json!({"enabled": true}));
    }
    if let Some(tr) = transport_from_query(&u.query) {
        o.insert("transport".into(), tr);
    }
    Ok(Value::Object(o))
}

fn b64_decode_std(s: &str) -> Result<Vec<u8>, String> {
    // standard + url-safe, pad
    let mut t = s.replace('-', "+").replace('_', "/");
    while t.len() % 4 != 0 {
        t.push('=');
    }
    // minimal decoder without extra crate — reuse existing if any
    fn dec(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let bytes = t.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 3 < bytes.len() {
        let (a, b, c, d) = (
            dec(bytes[i]).ok_or("b64")?,
            dec(bytes[i + 1]).ok_or("b64")?,
            dec(bytes[i + 2]).ok_or("b64")?,
            dec(bytes[i + 3]).ok_or("b64")?,
        );
        out.push((a << 2) | (b >> 4));
        if bytes[i + 2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if bytes[i + 3] != b'=' {
            out.push((c << 6) | d);
        }
        i += 4;
    }
    Ok(out)
}

fn parse_trojan(link: &str) -> Result<Value, String> {
    let u = UrlParts::parse(link)?;
    if u.user.is_empty() || u.host.is_empty() {
        return Err("trojan: need password@host".into());
    }
    let port = if u.port == 0 { 443 } else { u.port };
    let mut o = Map::new();
    o.insert("type".into(), json!("trojan"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(port));
    o.insert("password".into(), json!(u.user));
    // Trojan clash path enables TLS; link path uses tls->ParseFromLink
    let tls = tls_from_query(&u.query, true).unwrap_or_else(|| json!({"enabled": true}));
    o.insert("tls".into(), tls);
    if let Some(tr) = transport_from_query(&u.query) {
        o.insert("transport".into(), tr);
    }
    Ok(Value::Object(o))
}

fn parse_ss(link: &str) -> Result<Value, String> {
    // shadowsocks::ParseFromLink (SIP002 + legacy base64)
    let rest = link.trim_start_matches(|c| c != ':').trim_start_matches("://");
    let (main, _frag) = match rest.split_once('#') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let u = if main.contains('@') {
        UrlParts::parse(&format!("ss://{main}"))?
    } else {
        // v2rayN: base64 body
        let decoded = b64_decode(main).ok_or("ss: invalid base64 body")?;
        // may already include user@host or method:password@host:port
        UrlParts::parse(&format!("ss://{decoded}"))?
    };
    let (method, password) = if u.password.is_empty() {
        let mp = b64_decode(&u.user).unwrap_or_else(|| u.user.clone());
        let (m, p) = mp
            .split_once(':')
            .ok_or("ss: method:password missing")?;
        (m.to_string(), p.to_string())
    } else {
        (u.user.clone(), u.password.clone())
    };
    if u.host.is_empty() || method.is_empty() || password.is_empty() {
        return Err("ss: incomplete".into());
    }
    let port = if u.port == 0 { 8388 } else { u.port };
    let mut o = Map::new();
    o.insert("type".into(), json!("shadowsocks"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(port));
    o.insert("method".into(), json!(method));
    o.insert("password".into(), json!(password));
    Ok(Value::Object(o))
}

fn parse_socks(link: &str) -> Result<Value, String> {
    let u = UrlParts::parse(link)?;
    if u.host.is_empty() {
        return Err("socks: need host".into());
    }
    let port = if u.port == 0 { 1080 } else { u.port };
    let mut o = Map::new();
    o.insert("type".into(), json!("socks"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(port));
    if !u.user.is_empty() {
        o.insert("username".into(), json!(u.user));
    }
    if !u.password.is_empty() {
        o.insert("password".into(), json!(u.password));
    }
    Ok(Value::Object(o))
}

fn parse_http_proxy(link: &str) -> Result<Value, String> {
    // http.cpp ParseFromLink: type always "http"; https scheme → tls.enabled
    let u = UrlParts::parse(link)?;
    if u.host.is_empty() {
        return Err("http: need host".into());
    }
    let is_https = link.to_ascii_lowercase().starts_with("https://");
    let port = if u.port == 0 {
        if is_https {
            443
        } else {
            80
        }
    } else {
        u.port
    };
    let mut o = Map::new();
    o.insert("type".into(), json!("http"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(port));
    let pass = u.password.clone();
    // password-only → username = password
    let user = if u.user.is_empty() && !pass.is_empty() {
        pass.clone()
    } else {
        u.user.clone()
    };
    if !user.is_empty() {
        o.insert("username".into(), json!(user));
    }
    if !pass.is_empty() {
        o.insert("password".into(), json!(pass));
    }
    if is_https {
        o.insert("tls".into(), json!({"enabled": true}));
    }
    Ok(Value::Object(o))
}

fn parse_anytls(link: &str) -> Result<Value, String> {
    // anyTLS::ParseFromLink — password in username field; tls always on
    let u = UrlParts::parse(link)?;
    if u.host.is_empty() {
        return Err("anytls: need host".into());
    }
    let pass = if !u.user.is_empty() {
        u.user.clone()
    } else {
        u.password.clone()
    };
    if pass.is_empty() {
        return Err("anytls: need password@host".into());
    }
    let port = if u.port == 0 { 443 } else { u.port };
    let mut o = Map::new();
    o.insert("type".into(), json!("anytls"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(port));
    o.insert("password".into(), json!(pass));
    let mut tls = Map::new();
    tls.insert("enabled".into(), json!(true));
    if let Some(sni) = u
        .query
        .get("sni")
        .or_else(|| u.query.get("peer"))
        .filter(|s| !s.is_empty())
    {
        tls.insert("server_name".into(), json!(sni));
    }
    o.insert("tls".into(), Value::Object(tls));
    Ok(Value::Object(o))
}

fn parse_tuic(link: &str) -> Result<Value, String> {
    // tuic::ParseFromLink — uuid:password@host; congestion_control; tls always on
    let u = UrlParts::parse(link)?;
    if u.user.is_empty() || u.host.is_empty() {
        return Err("tuic: need uuid@host".into());
    }
    let port = if u.port == 0 { 443 } else { u.port };
    let mut o = Map::new();
    o.insert("type".into(), json!("tuic"));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(port));
    o.insert("uuid".into(), json!(u.user));
    if !u.password.is_empty() {
        o.insert("password".into(), json!(u.password));
    }
    if let Some(cc) = u
        .query
        .get("congestion_control")
        .or_else(|| u.query.get("congestion"))
        .filter(|s| !s.is_empty())
    {
        o.insert("congestion_control".into(), json!(cc));
    }
    if let Some(mode) = u
        .query
        .get("udp_relay_mode")
        .filter(|s| !s.is_empty())
    {
        o.insert("udp_relay_mode".into(), json!(mode));
    }
    let mut tls = Map::new();
    tls.insert("enabled".into(), json!(true));
    if let Some(sni) = u
        .query
        .get("sni")
        .or_else(|| u.query.get("peer"))
        .filter(|s| !s.is_empty())
    {
        tls.insert("server_name".into(), json!(sni));
    }
    if let Some(alpn) = u.query.get("alpn").filter(|s| !s.is_empty()) {
        let list: Vec<Value> = alpn
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| json!(s))
            .collect();
        if !list.is_empty() {
            tls.insert("alpn".into(), Value::Array(list));
        }
    }
    if u.query.get("insecure").map(|s| s == "1" || s == "true").unwrap_or(false)
        || u.query.get("allowInsecure").map(|s| s == "1" || s == "true").unwrap_or(false)
    {
        tls.insert("insecure".into(), json!(true));
    }
    o.insert("tls".into(), Value::Object(tls));
    Ok(Value::Object(o))
}

/// `hysteria::ParseFromLink`. `ver` 1 = `hysteria://`, 2 = `hysteria2://` / `hy2://`.
/// The Clash path already produced these outbounds; only the URI form was missing,
/// so a hysteria2 node imported from a YAML sub but vanished from a share list.
fn parse_hysteria(link: &str, ver: u8) -> Result<Value, String> {
    let u = UrlParts::parse(link)?;
    if u.host.is_empty() {
        return Err("hysteria: need host".into());
    }
    let ty = if ver == 1 { "hysteria" } else { "hysteria2" };
    let mut o = Map::new();
    o.insert("type".into(), json!(ty));
    o.insert("tag".into(), json!("proxy"));
    o.insert("server".into(), json!(u.host));
    // Upstream falls back to the TLS port; hysteria is always TLS.
    o.insert(
        "server_port".into(),
        json!(if u.port == 0 { 443 } else { u.port }),
    );

    let q = &u.query;
    if ver == 1 {
        if let Some(auth) = q.get("auth").filter(|s| !s.is_empty()) {
            o.insert("auth_str".into(), json!(auth));
        }
        if let Some(obfs) = q.get("obfsParam").filter(|s| !s.is_empty()) {
            o.insert("obfs".into(), json!(obfs));
        }
        for (key, out) in [
            ("recv_window_conn", "recv_window_conn"),
            ("recv_window", "recv_window"),
        ] {
            if let Some(n) = q.get(key).and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0) {
                o.insert(out.into(), json!(n));
            }
        }
        if q.get("disable_mtu_discovery").map(|v| v == "1" || v == "true") == Some(true) {
            o.insert("disable_mtu_discovery".into(), json!(true));
        }
    } else {
        // userinfo is the password; `user:pass` keeps both halves (upstream).
        let password = if u.password.is_empty() {
            u.user.clone()
        } else {
            format!("{}:{}", u.user, u.password)
        };
        if !password.is_empty() {
            o.insert("password".into(), json!(password));
        }
        if let Some(pw) = q.get("obfs-password").filter(|s| !s.is_empty()) {
            o.insert(
                "obfs".into(),
                json!({ "type": "salamander", "password": pw }),
            );
        }
    }

    for (key, out) in [("upmbps", "up_mbps"), ("downmbps", "down_mbps")] {
        if let Some(n) = q.get(key).and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0) {
            o.insert(out.into(), json!(n));
        }
    }
    // Port hopping: `mport=1000,2000-3000` becomes sing-box `server_ports`.
    if let Some(mport) = q.get("mport").filter(|s| !s.is_empty()) {
        let ports: Vec<Value> = mport
            .split(|c: char| c == ',' || c == '/')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| json!(s))
            .collect();
        if !ports.is_empty() {
            o.insert("server_ports".into(), Value::Array(ports));
        }
    }
    if let Some(hop) = q.get("hop_interval").filter(|s| !s.is_empty()) {
        o.insert("hop_interval".into(), json!(hop));
    }

    // Always TLS, whether or not the link says security=tls.
    let mut tls = match tls_from_query(q, true) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    tls.insert("enabled".into(), json!(true));
    o.insert("tls".into(), Value::Object(tls));
    Ok(Value::Object(o))
}

fn tls_from_query(q: &HashMap<String, String>, default_enable_if_security: bool) -> Option<Value> {
    // TLS::ParseFromLink keys: security=tls|reality, sni, alpn, fp, pbk, sid, spx, insecure
    let security = q
        .get("security")
        .map(|s| s.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let sni = q
        .get("sni")
        .or_else(|| q.get("peer"))
        .cloned()
        .unwrap_or_default();
    let mut enabled = security == "tls" || security == "reality" || !sni.is_empty();
    if default_enable_if_security && (security == "tls" || security == "reality") {
        enabled = true;
    }
    if !enabled && security.is_empty() && sni.is_empty() {
        // trojan default TLS enabled by caller
        return None;
    }
    let mut tls = Map::new();
    tls.insert("enabled".into(), json!(enabled || !sni.is_empty() || security == "tls" || security == "reality"));
    if !sni.is_empty() {
        tls.insert("server_name".into(), json!(sni));
    }
    if let Some(alpn) = q.get("alpn").filter(|s| !s.is_empty()) {
        let arr: Vec<Value> = alpn.split(',').map(|a| json!(a.trim())).collect();
        tls.insert("alpn".into(), Value::Array(arr));
    }
    if let Some(fp) = q.get("fp").filter(|s| !s.is_empty()) {
        tls.insert(
            "utls".into(),
            json!({"enabled": true, "fingerprint": fp}),
        );
    }
    if q.get("insecure").map(|s| s == "1" || s == "true").unwrap_or(false)
        || q.get("allowInsecure").map(|s| s == "1" || s == "true").unwrap_or(false)
    {
        tls.insert("insecure".into(), json!(true));
    }
    if security == "reality" {
        let mut reality = Map::new();
        reality.insert("enabled".into(), json!(true));
        if let Some(pbk) = q.get("pbk").filter(|s| !s.is_empty()) {
            reality.insert("public_key".into(), json!(pbk));
        }
        if let Some(sid) = q.get("sid").filter(|s| !s.is_empty()) {
            reality.insert("short_id".into(), json!(sid));
        }
        tls.insert("reality".into(), Value::Object(reality));
    }
    Some(Value::Object(tls))
}

fn transport_from_query(q: &HashMap<String, String>) -> Option<Value> {
    // Transport::ParseFromLink
    let mut ty = q.get("type").cloned().unwrap_or_default();
    if ty.is_empty() || ty == "tcp" || ty == "raw" {
        if q.get("headerType").map(|s| s.as_str()) == Some("http") {
            ty = "http".into();
        } else {
            return None;
        }
    }
    if ty == "h2" {
        ty = "http".into();
    }
    let mut t = Map::new();
    t.insert("type".into(), json!(ty));
    // WS/HTTP Host belongs in headers (sing-box + UI fieldsFromOutbound); grpc uses service_name / host
    if let Some(host) = q.get("host").filter(|s| !s.is_empty()) {
        if ty == "ws" || ty == "http" {
            t.insert("headers".into(), json!({"Host": host}));
        } else {
            t.insert("host".into(), json!(host));
        }
    }
    if let Some(path) = q.get("path").filter(|s| !s.is_empty()) {
        t.insert("path".into(), json!(path));
    }
    if let Some(sn) = q
        .get("serviceName")
        .or_else(|| q.get("service_name"))
        .filter(|s| !s.is_empty())
    {
        t.insert("service_name".into(), json!(sn));
    }
    if let Some(mode) = q.get("mode").filter(|s| !s.is_empty()) {
        // grpc mode (gun/multi) — pass through when present
        if ty == "grpc" {
            t.insert("mode".into(), json!(mode));
        }
    }
    Some(Value::Object(t))
}

struct UrlParts {
    user: String,
    password: String,
    host: String,
    port: u16,
    query: HashMap<String, String>,
}

impl UrlParts {
    fn parse(link: &str) -> Result<Self, String> {
        // scheme://[user[:pass]@]host[:port][/path][?query][#frag]
        let without_scheme = link
            .split_once("://")
            .map(|(_, r)| r)
            .ok_or("missing ://")?;
        let without_frag = without_scheme.split('#').next().unwrap_or(without_scheme);
        let (authority_path, query_s) = match without_frag.split_once('?') {
            Some((a, q)) => (a, q),
            None => (without_frag, ""),
        };
        let authority = authority_path.split('/').next().unwrap_or(authority_path);
        let (userinfo, hostport) = if let Some((ui, hp)) = authority.rsplit_once('@') {
            (ui, hp)
        } else {
            ("", authority)
        };
        let (user, password) = if userinfo.is_empty() {
            (String::new(), String::new())
        } else if let Some((u, p)) = userinfo.split_once(':') {
            (pct_decode(u), pct_decode(p))
        } else {
            (pct_decode(userinfo), String::new())
        };
        let (host, port) = parse_host_port(hostport)?;
        let mut query = HashMap::new();
        if !query_s.is_empty() {
            for pair in query_s.split('&') {
                if pair.is_empty() {
                    continue;
                }
                let (k, v) = match pair.split_once('=') {
                    Some((k, v)) => (pct_decode(k), pct_decode(v)),
                    None => (pct_decode(pair), String::new()),
                };
                query.insert(k, v);
            }
        }
        Ok(Self {
            user,
            password,
            host,
            port,
            query,
        })
    }
}

fn parse_host_port(hp: &str) -> Result<(String, u16), String> {
    if hp.is_empty() {
        return Err("empty host".into());
    }
    if hp.starts_with('[') {
        let end = hp.find(']').ok_or("bad ipv6")?;
        let host = hp[1..end].to_string();
        let rest = &hp[end + 1..];
        let port = if let Some(p) = rest.strip_prefix(':') {
            p.parse().unwrap_or(0)
        } else {
            0
        };
        return Ok((host, port));
    }
    if let Some((h, p)) = hp.rsplit_once(':') {
        if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) {
            return Ok((h.to_string(), p.parse().unwrap_or(0)));
        }
    }
    Ok((hp.to_string(), 0))
}

fn pct_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn b64_decode(s: &str) -> Option<String> {
    let s = s.trim();
    // std-only: try via base64 alphabet manually is heavy — use a tiny decoder
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut buf = cleaned.replace('-', "+").replace('_', "/");
    while buf.len() % 4 != 0 {
        buf.push('=');
    }
    decode_standard_b64(&buf).map(|v| String::from_utf8_lossy(&v).into_owned())
}

fn decode_standard_b64(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let (a, b, c, d) = (
            val(chunk[0])?,
            val(chunk[1])?,
            val(chunk[2])?,
            val(chunk[3])?,
        );
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            out.push(((b & 0xf) << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            out.push(((c & 0x3) << 6) | d);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vless_basic() {
        let o = parse_to_outbound(
            "vless://11111111-1111-1111-1111-111111111111@example.com:443?security=tls&sni=example.com&type=ws&path=%2Fws#n",
        )
        .unwrap();
        assert_eq!(o["type"], "vless");
        assert_eq!(o["server"], "example.com");
        assert_eq!(o["server_port"], 443);
        assert_eq!(o["uuid"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(o["tls"]["enabled"], true);
        assert_eq!(o["tls"]["server_name"], "example.com");
        assert_eq!(o["transport"]["type"], "ws");
        assert_eq!(o["transport"]["path"], "/ws");
    }

    #[test]
    fn parse_share_body_smoke() {
        let body = concat!(
            "vless://11111111-1111-1111-1111-111111111111@1.2.3.4:443",
            "?encryption=none&security=reality&sni=www.example.com&fp=chrome",
            "&pbk=PUBLICKEY&sid=abcd&type=tcp&flow=xtls-rprx-vision#RealityNode\n",
            "vless://22222222-2222-2222-2222-222222222222@5.6.7.8:80",
            "?encryption=none&security=none&type=ws&host=cdn.example.com&path=%2Fws#WsNode\n",
            "trojan://secretpass@9.9.9.9:443",
            "?security=tls&sni=trojan.example.com&type=ws&host=trojan.example.com&path=%2Ftrojan#TrojanWs\n",
        );
        let (nodes, _skipped) = parse_share_body(body);
        assert_eq!(nodes.len(), 3);
        let r = &nodes[0];
        assert_eq!(r.name, "RealityNode");
        assert_eq!(r.outbound["uuid"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(r.outbound["flow"], "xtls-rprx-vision");
        assert_eq!(r.outbound["tls"]["enabled"], true);
        assert_eq!(r.outbound["tls"]["server_name"], "www.example.com");
        assert_eq!(r.outbound["tls"]["reality"]["public_key"], "PUBLICKEY");
        let w = &nodes[1];
        assert_eq!(w.name, "WsNode");
        assert_eq!(w.outbound["transport"]["type"], "ws");
        assert_eq!(w.outbound["transport"]["headers"]["Host"], "cdn.example.com");
        assert_eq!(w.outbound["transport"]["path"], "/ws");
        let t = &nodes[2];
        assert_eq!(t.name, "TrojanWs");
        assert_eq!(t.outbound["password"], "secretpass");
        assert_eq!(t.outbound["tls"]["server_name"], "trojan.example.com");
        assert_eq!(t.outbound["transport"]["headers"]["Host"], "trojan.example.com");
    }

    #[test]
    fn vmess_uri_security_tls() {
        // standard VMess URL (not v2rayN b64)
        let o = parse_to_outbound(
            "vmess://11111111-1111-1111-1111-111111111111@node.example.com:443?security=tls#US",
        )
        .unwrap();
        assert_eq!(o["type"], "vmess");
        assert_eq!(o["uuid"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(o["server"], "node.example.com");
        assert_eq!(o["server_port"], 443);
        assert_eq!(o["tls"]["enabled"], true);
    }

    #[test]
    fn vmess_fake_name_user_rejected() {
        // Old Nexus QR/clipboard fabricated: btoa(name)@host — not a UUID
        let e = parse_to_outbound(
            "vmess://VVMgLSDnvo7lm73psqjpsbwy@node.example.com:443?security=tls#US",
        )
        .unwrap_err();
        assert!(
            e.contains("UUID") || e.contains("uuid"),
            "expected uuid rejection, got {e}"
        );
    }

    #[test]
    fn vmess_b64() {
        // {"v":"2","ps":"n","add":"1.2.3.4","port":"443","id":"11111111-1111-1111-1111-111111111111","aid":"0","net":"tcp","type":"none","host":"","path":"","tls":"tls"}
        let raw = r#"{"v":"2","ps":"n","add":"1.2.3.4","port":"443","id":"11111111-1111-1111-1111-111111111111","aid":"0","net":"tcp","type":"none","host":"","path":"","tls":"tls"}"#;
        let b64 = {
            const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let data = raw.as_bytes();
            let mut out = String::new();
            let mut i = 0;
            while i < data.len() {
                let b0 = data[i] as u32;
                let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
                let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
                let triple = (b0 << 16) | (b1 << 8) | b2;
                out.push(TABLE[((triple >> 18) & 63) as usize] as char);
                out.push(TABLE[((triple >> 12) & 63) as usize] as char);
                if i + 1 < data.len() {
                    out.push(TABLE[((triple >> 6) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
                if i + 2 < data.len() {
                    out.push(TABLE[(triple & 63) as usize] as char);
                } else {
                    out.push('=');
                }
                i += 3;
            }
            out
        };
        let o = parse_to_outbound(&format!("vmess://{b64}")).unwrap();
        assert_eq!(o["type"], "vmess");
        assert_eq!(o["server"], "1.2.3.4");
        assert_eq!(o["uuid"], "11111111-1111-1111-1111-111111111111");
    }

    #[test]
    fn trojan_basic() {
        let o = parse_to_outbound("trojan://secret@1.2.3.4:443?sni=a.com#t").unwrap();
        assert_eq!(o["type"], "trojan");
        assert_eq!(o["password"], "secret");
        assert_eq!(o["tls"]["enabled"], true);
    }

    #[test]
    fn socks_basic() {
        let o = parse_to_outbound("socks5://u:p@127.0.0.1:1080").unwrap();
        assert_eq!(o["type"], "socks");
        assert_eq!(o["server_port"], 1080);
        assert_eq!(o["username"], "u");
    }

    #[test]
    fn https_proxy_enables_tls() {
        // https://user:pass@host:port → type http + tls.enabled
        let o = parse_to_outbound(
            "https://e4e6e20b-8713-4b3e-a12a-772b7e530c04:e4e6e20b-8713-4b3e-a12a-772b7e530c04@edge.example.com:443#igg",
        )
        .unwrap();
        assert_eq!(o["type"], "http");
        assert_eq!(o["server"], "edge.example.com");
        assert_eq!(o["server_port"], 443);
        assert_eq!(o["username"], "e4e6e20b-8713-4b3e-a12a-772b7e530c04");
        assert_eq!(o["password"], "e4e6e20b-8713-4b3e-a12a-772b7e530c04");
        assert_eq!(o["tls"]["enabled"], true);
    }

    #[test]
    fn http_proxy_no_tls() {
        let o = parse_to_outbound("http://u:p@1.2.3.4:8080").unwrap();
        assert_eq!(o["type"], "http");
        assert!(o.get("tls").is_none());
        assert_eq!(o["server_port"], 8080);
    }

    #[test]
    fn anytls_basic() {
        let o = parse_to_outbound(
            "anytls://e4e6e20b-8713-4b3e-a12a-772b7e530c04@vU.example.com:443?sni=cdn.example#n",
        )
        .unwrap();
        assert_eq!(o["type"], "anytls");
        assert_eq!(o["password"], "e4e6e20b-8713-4b3e-a12a-772b7e530c04");
        assert_eq!(o["server"], "vU.example.com");
        assert_eq!(o["tls"]["enabled"], true);
        assert_eq!(o["tls"]["server_name"], "cdn.example");
    }

    #[test]
    fn tuic_igg_style() {
        // tuic://uuid:pass@host:port?congestion_control=bbr&alpn=h3
        let o = parse_to_outbound(
            "tuic://11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222@tuic.example.com:8443?congestion_control=bbr&alpn=h3#sg",
        )
        .unwrap();
        assert_eq!(o["type"], "tuic");
        assert_eq!(o["uuid"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(o["password"], "22222222-2222-2222-2222-222222222222");
        assert_eq!(o["server"], "tuic.example.com");
        assert_eq!(o["server_port"], 8443);
        assert_eq!(o["congestion_control"], "bbr");
        assert_eq!(o["tls"]["enabled"], true);
        assert_eq!(o["tls"]["alpn"][0], "h3");
    }

    #[test]
    fn hysteria2_and_hy2_alias() {
        for link in [
            "hysteria2://pw123@hy.example.com:8443?obfs-password=sala&upmbps=50&downmbps=200&sni=cdn.example#hk",
            "hy2://pw123@hy.example.com:8443?obfs-password=sala&upmbps=50&downmbps=200&sni=cdn.example#hk",
        ] {
            let o = parse_to_outbound(link).unwrap();
            assert_eq!(o["type"], "hysteria2", "{link}");
            assert_eq!(o["password"], "pw123");
            assert_eq!(o["server"], "hy.example.com");
            assert_eq!(o["server_port"], 8443);
            assert_eq!(o["obfs"]["type"], "salamander");
            assert_eq!(o["obfs"]["password"], "sala");
            assert_eq!(o["up_mbps"], 50);
            assert_eq!(o["down_mbps"], 200);
            // hysteria is always TLS even when the link never says security=tls.
            assert_eq!(o["tls"]["enabled"], true);
            assert_eq!(o["tls"]["server_name"], "cdn.example");
        }
    }

    /// v1 is a different outbound type with different keys — not an alias of v2.
    #[test]
    fn hysteria_v1_keys_and_port_hopping() {
        let o = parse_to_outbound(
            "hysteria://hy1.example.com:443?auth=secret&obfsParam=xyz&mport=1000,2000-3000&upmbps=10#v1",
        )
        .unwrap();
        assert_eq!(o["type"], "hysteria");
        assert_eq!(o["auth_str"], "secret");
        assert_eq!(o["obfs"], "xyz");
        assert_eq!(o["server_ports"][0], "1000");
        assert_eq!(o["server_ports"][1], "2000-3000");
        assert_eq!(o["up_mbps"], 10);
        assert!(o.get("password").is_none(), "v1 has no password field");
    }

    /// The whole point of the gap: these lines used to be dropped on the floor.
    #[test]
    fn share_body_keeps_hysteria_nodes() {
        let body = "hysteria2://pw@a.example.com:443#one\nvless://11111111-1111-1111-1111-111111111111@b.example.com:443?encryption=none#two\n";
        let (nodes, skipped) = parse_share_body(body);
        assert_eq!(nodes.len(), 2, "{nodes:?}");
        assert_eq!(nodes[0].type_label, "Hysteria2");
        assert_eq!(nodes[0].name, "one");
        assert!(skipped.is_empty(), "{skipped:?}");
    }

    /// Dropped entries must be named. An Xray-only VLESS parses fine on this path,
    /// so without the explicit check it would import and then fail at connect.
    #[test]
    fn skipped_entries_are_named_including_xray_vless() {
        let body = concat!(
            "vless://11111111-1111-1111-1111-111111111111@a.example.com:443?type=xhttp#x\n",
            "juicity://pw@b.example.com:443#j\n",
            "vless://11111111-1111-1111-1111-111111111111@c.example.com:443?encryption=none#ok\n",
        );
        let (nodes, skipped) = parse_share_body(body);
        assert_eq!(nodes.len(), 1, "only the plain vless survives: {nodes:?}");
        assert!(skipped.contains(&"vless-xray".to_string()), "{skipped:?}");
        assert!(skipped.contains(&"juicity".to_string()), "{skipped:?}");
    }
}

