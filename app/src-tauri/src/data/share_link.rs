//! Share-link → sing-box outbound JSON (upstream configs/outbounds/* ParseFromLink + ExportToJson subset).
//! Supported: vless:// vmess:// trojan:// ss:// socks:// http(s):// anytls:// tuic:// (+ optional JSON outbound object).
//! Full transport/reality parity can grow field-by-field from upstream when needed.

use serde_json::{json, Map, Value};
use std::collections::HashMap;

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
    let tls_on = j
        .get("tls")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("tls"))
        .unwrap_or(false);
    // SNI only from sni field (host is often WS Host header, not SNI)
    let sni = j.get("sni").and_then(|v| v.as_str()).unwrap_or("");
    if tls_on || !sni.is_empty() {
        let mut tls = Map::new();
        tls.insert("enabled".into(), json!(true));
        if !sni.is_empty() {
            tls.insert("server_name".into(), json!(sni));
        }
        o.insert("tls".into(), Value::Object(tls));
    }
    let net = j
        .get("net")
        .or_else(|| j.get("network"))
        .and_then(|v| v.as_str())
        .unwrap_or("tcp");
    let net = if net == "h2" { "http" } else { net };
    if net != "tcp" && net != "raw" {
        let mut t = Map::new();
        let ty = match net {
            "ws" | "websocket" => "ws",
            "grpc" => "grpc",
            "h2" | "http" => "http",
            other => other,
        };
        t.insert("type".into(), json!(ty));
        if let Some(path) = j.get("path").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            t.insert("path".into(), json!(path));
        }
        if let Some(h) = j.get("host").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            t.insert("headers".into(), json!({"Host": h}));
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
    if let Some(host) = q.get("host").filter(|s| !s.is_empty()) {
        t.insert("host".into(), json!(host));
    }
    if let Some(path) = q.get("path").filter(|s| !s.is_empty()) {
        t.insert("path".into(), json!(path));
    }
    if let Some(sn) = q.get("serviceName").filter(|s| !s.is_empty()) {
        t.insert("service_name".into(), json!(sn));
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
    fn vmess_uri_security_tls() {
        // standard VMess URL (not v2rayN b64)
        let o = parse_to_outbound(
            "vmess://11111111-1111-1111-1111-111111111111@node.example.com:49362?security=tls#US",
        )
        .unwrap();
        assert_eq!(o["type"], "vmess");
        assert_eq!(o["uuid"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(o["server"], "node.example.com");
        assert_eq!(o["server_port"], 49362);
        assert_eq!(o["tls"]["enabled"], true);
    }

    #[test]
    fn vmess_fake_name_user_rejected() {
        // Old Nexus QR/clipboard fabricated: btoa(name)@host — not a UUID
        let e = parse_to_outbound(
            "vmess://VVMgLSDnvo7lm73psqjpsbwy@node.example.com:49362?security=tls#US",
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
            "tuic://33D9EFC7-8BCD-F4A1-AAFF-D95A2F06970D:e4e6e20b-8713-4b3e-a12a-772b7e530c04@sing1.71edgeiqiyi.com:8443?congestion_control=bbr&alpn=h3#sg",
        )
        .unwrap();
        assert_eq!(o["type"], "tuic");
        assert_eq!(o["uuid"], "33D9EFC7-8BCD-F4A1-AAFF-D95A2F06970D");
        assert_eq!(o["password"], "e4e6e20b-8713-4b3e-a12a-772b7e530c04");
        assert_eq!(o["server"], "sing1.71edgeiqiyi.com");
        assert_eq!(o["server_port"], 8443);
        assert_eq!(o["congestion_control"], "bbr");
        assert_eq!(o["tls"]["enabled"], true);
        assert_eq!(o["tls"]["alpn"][0], "h3");
    }
}
