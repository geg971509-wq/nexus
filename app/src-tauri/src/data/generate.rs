//! Pure generate: outbound + flags → sing-box JSON.
use serde_json::{json, Map, Value};

/// Generate-time node handle (not persisted; catalog is store truth).
pub struct GenNode {
    pub outbound: Value,
    /// "unknown" | "yes" | "no" — default-on mux injects only when "yes".
    pub mux_capability: String,
}

pub struct GenerateInput<'a> {
    pub node: &'a GenNode,
    pub system_proxy_port: u16,
    pub tun: bool,
    /// Inject mux when outbound has no multiplex key and capability is yes.
    pub mux_default_on: bool,
    pub mux_protocol: &'a str,
    pub mux_concurrency: i64,
    /// Bootstrap resolvers; empty = product default. The firewall passes exactly
    /// this list, so config and PF must be generated from the same source.
    pub dns_bootstrap: &'a [String],
}

/// Decision table: unspecified outbound + mux_default_on → enable only if capability == "yes".
/// Explicit multiplex object on outbound is never stripped.
pub fn apply_mux_gate(
    outbound: &mut Value,
    capability: &str,
    mux_default_on: bool,
    protocol: &str,
    concurrency: i64,
) {
    let Some(obj) = outbound.as_object_mut() else {
        return;
    };
    if obj.contains_key("multiplex") {
        return; // subscription / explicit
    }
    if !mux_default_on || capability != "yes" {
        return;
    }
    let mut mux = Map::new();
    mux.insert("enabled".into(), json!(true));
    if !protocol.is_empty() {
        mux.insert("protocol".into(), json!(protocol));
    }
    if concurrency > 0 {
        mux.insert("max_streams".into(), json!(concurrency));
    }
    obj.insert("multiplex".into(), Value::Object(mux));
}

/// engine-aligned DNS (MVP): DoH to IP so system DNS (often TUN-hijacked) is not required.
/// - dns-remote detours via proxy only when outbound is not `direct` (sing-box rejects detour→direct).
/// - Proxy server hostname (if any) resolves via dns-direct so chain bootstrap cannot deadlock.
/// - Tun on Darwin: avoid type=local for bootstrap (upstream uses underlying/udp).
fn build_dns_section(outbound: &Value, tun: bool, dns_bootstrap: &[String]) -> Value {
    let is_direct = outbound.get("type").and_then(|t| t.as_str()) == Some("direct");
    // Same sanitize as PF/OS: a hostname here would be the config resolver
    // while PF falls back to 8.8.8.8 and system DNS dies.
    let cleaned = crate::defaults::sanitize_dns_bootstrap(dns_bootstrap);
    let resolver = cleaned[0].as_str();

    // Bootstrap resolver: local is fine without Tun; under Tun/macOS use UDP IP (no system getaddrinfo).
    let dns_local = if tun {
        json!({"type": "udp", "tag": "dns-local", "server": resolver})
    } else {
        json!({"type": "local", "tag": "dns-local"})
    };

    // No-detour DoH — works when default path is a foreign TUN (e.g. upstream already up).
    let dns_direct = json!({
        "type": "https",
        "tag": "dns-direct",
        "server": resolver,
        "path": "/dns-query",
        "domain_resolver": "dns-local"
    });

    let mut dns_remote = json!({
        "type": "https",
        "tag": "dns-remote",
        "server": resolver,
        "path": "/dns-query",
        "domain_resolver": "dns-local"
    });
    if !is_direct {
        // remote DNS rides the proxy so plain UDP DNS is not required on the WAN.
        dns_remote
            .as_object_mut()
            .unwrap()
            .insert("detour".into(), json!("proxy"));
    }

    // Hostnames that must bootstrap off-proxy (server + TLS SNI / reality).
    let mut bootstrap = Vec::new();
    for key in ["server", "server_name"] {
        if let Some(h) = outbound.get(key).and_then(|s| s.as_str()) {
            if !h.is_empty() && h.chars().any(|c| c.is_ascii_alphabetic()) {
                bootstrap.push(h.to_string());
            }
        }
    }
    if let Some(tls) = outbound.get("tls") {
        if let Some(h) = tls.get("server_name").and_then(|s| s.as_str()) {
            if !h.is_empty() && h.chars().any(|c| c.is_ascii_alphabetic()) {
                bootstrap.push(h.to_string());
            }
        }
    }
    bootstrap.sort();
    bootstrap.dedup();

    let mut rules = Vec::new();
    if !bootstrap.is_empty() {
        rules.push(json!({
            "domain": bootstrap,
            "action": "route",
            "server": "dns-direct"
        }));
    }

    json!({
        "servers": [dns_remote, dns_direct, dns_local],
        "rules": rules,
        "final": if is_direct { "dns-direct" } else { "dns-remote" },
        "independent_cache": true
    })
}

/// Pure function — no UI/socket.
pub fn generate_config(input: &GenerateInput<'_>) -> Value {
    let mut outbound = input.node.outbound.clone();
    // UI/catalog-only hints (PF peer allow). Never pass to Core/sing-box.
    if let Some(obj) = outbound.as_object_mut() {
        obj.remove("server_ip");
        obj.remove("ip");
    }
    if outbound.get("tag").is_none() {
        if let Some(obj) = outbound.as_object_mut() {
            obj.insert("tag".into(), json!("proxy"));
        }
    }
    apply_mux_gate(
        &mut outbound,
        &input.node.mux_capability,
        input.mux_default_on,
        input.mux_protocol,
        input.mux_concurrency,
    );
    let dns = build_dns_section(&outbound, input.tun, input.dns_bootstrap);

    // sing-box 1.12+: dial fields need domain_resolver. Without it, proxy server
    // hostname resolves via dns-remote→detour proxy → "DNS query loopback".
    // route.default_domain_resolver = dns-direct (and pin on outbound).
    if outbound.get("domain_resolver").is_none() {
        if let Some(obj) = outbound.as_object_mut() {
            let server = obj.get("server").and_then(|s| s.as_str()).unwrap_or("");
            if !server.is_empty() && server.chars().any(|c| c.is_ascii_alphabetic()) {
                obj.insert("domain_resolver".into(), json!("dns-direct"));
            }
        }
    }
    let outbounds = vec![outbound, json!({"type":"direct","tag":"direct"})];

    // mixed 127.0.0.1:inbound_socks_port (default 2080)
    let mut inbounds = vec![json!({
        "type": "mixed",
        "tag": "mixed-in",
        "listen": "127.0.0.1",
        "listen_port": input.system_proxy_port
    })];

    if input.tun {
        // generate.cpp genTunName(): macOS empty → sing-box CalculateInterfaceName → utunN.
        // Non-empty non-utun name → "bad tun name" on darwin (sing-tun tun_darwin.go).
        // address array (sing-box ≥1.10); inet4_address removed in 1.12.
        // SettingsRepo: vpn_tun_ipv4_cidr default 172.19.0.1/24; private_range_bypass true.
        const TUN_V4: &str = "172.19.0.1/24";
        let mut tun_obj = serde_json::Map::new();
        tun_obj.insert("type".into(), json!("tun"));
        tun_obj.insert("tag".into(), json!("tun-in"));
        tun_obj.insert("address".into(), json!([TUN_V4]));
        tun_obj.insert("auto_route".into(), json!(true));
        tun_obj.insert("strict_route".into(), json!(false));
        // SettingsRepo macOS default: gvisor (Windows may use system).
        tun_obj.insert("stack".into(), json!("gvisor"));
        // SettingsRepo macOS default vpn_mtu = 1500 (not jumbo 9000).
        tun_obj.insert("mtu".into(), json!(1500));
        // Do NOT set route_address to Tun CIDR (sing-tun allowlist replaces full auto_route).
        // Carve Tun out of private excludes so Tun+1 DNS stays on-iface while LAN bypasses.
        tun_obj.insert(
            "route_exclude_address".into(),
            Value::Array(
                private_range_bypass_excluding_tun(TUN_V4)
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
        // macOS: omit interface_name (auto utunN). Elsewhere: named device ok.
        #[cfg(not(target_os = "macos"))]
        {
            tun_obj.insert("interface_name".into(), json!("nexus-tun"));
        }
        inbounds.push(Value::Object(tun_obj));
    }

    // RouteProfile defaults: sniff → hijack DNS → private/LAN direct → final proxy.
    // auto_detect_interface: true (upstream when vpn; also correct for mixed+sysproxy).
    // default_domain_resolver: upstream always sets dns-direct so egress dial never
    // uses dns-remote (would detour proxy while resolving proxy itself).
    let route_rules = vec![
        json!({"action": "sniff"}),
        json!({"protocol": "dns", "action": "hijack-dns"}),
        json!({"ip_is_private": true, "outbound": "direct"}),
    ];
    // Always resolve process at route time so conn table + enricher see ProcessInfo
    // (path/pid). Independent of blocklist; throng darwin ProcessID still via build patch.
    let route = json!({
        "rules": route_rules,
        "final": "proxy",
        "auto_detect_interface": true,
        "find_process": true,
        "default_domain_resolver": {
            "server": "dns-direct"
        }
    });

    // experimental.clash_api required for TrafficManager / QueryConnections
    // (even without external_controller — empty object still creates clash server).
    // cache_file.path MUST be absolute: GUI Core cwd is `/`, relative → /cache.db
    // (read-only FS) → Start fails → power button looks dead.
    let cache_path = cache_file_path();
    json!({
        "log": {"level": "info"},
        "dns": dns,
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": route,
        "experimental": {
            "clash_api": { "default_mode": "" },
            "cache_file": {
                "enabled": true,
                "path": cache_path,
                "store_fakeip": true,
                "store_rdrc": true
            }
        }
    })
}

fn cache_file_path() -> String {
    crate::paths::ensure_data_dir()
        .join("cache.db")
        .to_string_lossy()
        .into_owned()
}

/// TunPrivateBypass::privateRangeBypassExcludingTun — private LAN bypass at OS route
/// level, with Tun subnet carved out so Core system DNS (Tun+1) is not swallowed by 172.16/12.
fn private_range_bypass_excluding_tun(tun_cidr: &str) -> Vec<String> {
    const PRIVATE: &[&str] = &[
        "127.0.0.0/8",
        "10.0.0.0/8",
        "172.16.0.0/12",
        "192.168.0.0/16",
        "169.254.0.0/16",
        "224.0.0.0/4",
        "255.255.255.255/32",
    ];
    let tun = parse_v4_cidr(tun_cidr);
    let mut out = Vec::new();
    for range_text in PRIVATE {
        let Some(range) = parse_v4_cidr(range_text) else {
            out.push((*range_text).to_string());
            continue;
        };
        match tun {
            Some(t) if cidr_contains_v4(range, t) => {
                out.extend(subtract_v4_cidr(range, t));
            }
            _ => out.push((*range_text).to_string()),
        }
    }
    out
}

fn parse_v4_cidr(text: &str) -> Option<(u32, u8)> {
    let (ip, bits_s) = text.split_once('/')?;
    let bits: u8 = bits_s.parse().ok()?;
    if bits > 32 {
        return None;
    }
    let parts: Vec<_> = ip.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut host = 0u32;
    for p in parts {
        let o: u8 = p.parse().ok()?;
        host = (host << 8) | u32::from(o);
    }
    let mask = if bits == 0 {
        0
    } else {
        u32::MAX << (32 - bits)
    };
    Some((host & mask, bits))
}

fn format_v4_cidr(network: u32, bits: u8) -> String {
    format!(
        "{}.{}.{}.{}/{}",
        (network >> 24) & 0xff,
        (network >> 16) & 0xff,
        (network >> 8) & 0xff,
        network & 0xff,
        bits
    )
}

fn cidr_contains_v4(outer: (u32, u8), inner: (u32, u8)) -> bool {
    let (outer_net, outer_bits) = outer;
    let (inner_net, inner_bits) = inner;
    if outer_bits > inner_bits {
        return false;
    }
    let mask = if outer_bits == 0 {
        0
    } else {
        u32::MAX << (32 - outer_bits)
    };
    (inner_net & mask) == (outer_net & mask)
}

/// base \ hole as CIDR list; hole must sit inside base (upstream subtractV4Cidr).
fn subtract_v4_cidr(base: (u32, u8), hole: (u32, u8)) -> Vec<String> {
    if !cidr_contains_v4(base, hole) {
        return vec![format_v4_cidr(base.0, base.1)];
    }
    if base.1 == hole.1 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cur_net = base.0;
    let mut cur_bits = base.1;
    while cur_bits < hole.1 {
        cur_bits += 1;
        let bit = 1u32 << (32 - cur_bits);
        let left = cur_net;
        let right = cur_net | bit;
        if (hole.0 & bit) == 0 {
            out.push(format_v4_cidr(right, cur_bits));
            cur_net = left;
        } else {
            out.push(format_v4_cidr(left, cur_bits));
            cur_net = right;
        }
    }
    out
}

/// Build config from an explicit outbound (UI-selected share link / JSON).
pub fn generate_with_outbound(
    outbound: Value,
    port: u16,
    tun: bool,
    dns_bootstrap: &[String],
) -> Value {
    // ensure tag
    let mut outbound = outbound;
    if outbound.get("tag").is_none() {
        if let Some(obj) = outbound.as_object_mut() {
            obj.insert("tag".into(), json!("proxy"));
        }
    }
    generate_config(&GenerateInput {
        node: &GenNode {
            outbound,
            mux_capability: "unknown".into(),
        },
        system_proxy_port: port,
        tun,
        mux_default_on: false,
        mux_protocol: "h2mux",
        mux_concurrency: 8,
        dns_bootstrap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_node(outbound: Value) -> GenNode {
        GenNode {
            outbound,
            mux_capability: "unknown".into(),
        }
    }

    fn gen(n: &GenNode, port: u16, tun: bool) -> Value {
        generate_config(&GenerateInput {
            node: n,
            system_proxy_port: port,
            tun,
            mux_default_on: false,
            mux_protocol: "smux",
            mux_concurrency: 8,
            dns_bootstrap: &[],
        })
    }

    #[test]
    fn generate_has_mixed_and_proxy() {
        let n = sample_node(json!({"type":"socks","tag":"proxy","server":"127.0.0.1","server_port":1080}));
        let v = gen(&n, 2080, false);
        assert_eq!(v["inbounds"][0]["listen_port"], 2080);
        assert_eq!(v["outbounds"][0]["type"], "socks");
    }

    #[test]
    fn strips_catalog_peer_hints_from_outbound() {
        let n = sample_node(json!({
            "type":"vmess",
            "tag":"proxy",
            "server":"us.example.com",
            "server_port":443,
            "server_ip":"1.2.3.4",
            "ip":"5.6.7.8",
            "uuid":"00000000-0000-0000-0000-000000000001"
        }));
        let v = gen(&n, 2080, false);
        let o = &v["outbounds"][0];
        assert!(o.get("server_ip").is_none(), "server_ip must not reach Core: {o}");
        assert!(o.get("ip").is_none(), "ip must not reach Core: {o}");
        assert_eq!(o["server"], "us.example.com");
    }

    #[test]
    fn tun_mac_omits_named_interface() {
        let n = sample_node(json!({"type":"socks","tag":"proxy","server":"127.0.0.1","server_port":1080}));
        let v = gen(&n, 2080, true);
        let tun = v["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["type"] == "tun")
            .expect("tun inbound");
        assert_eq!(tun["address"][0], "172.19.0.1/24");
        assert_eq!(tun["stack"], "gvisor");
        assert_eq!(tun["mtu"], 1500);
        let excl = tun["route_exclude_address"]
            .as_array()
            .expect("route_exclude_address");
        let excl_s: Vec<_> = excl.iter().filter_map(|v| v.as_str()).collect();
        assert!(excl_s.contains(&"10.0.0.0/8"));
        assert!(excl_s.contains(&"192.168.0.0/16"));
        // Tun /24 carved out of 172.16.0.0/12 — full /12 must not remain.
        assert!(!excl_s.iter().any(|s| *s == "172.16.0.0/12"));
        assert!(!excl_s.iter().any(|s| *s == "172.19.0.0/24"));
        #[cfg(target_os = "macos")]
        {
            assert!(
                tun.get("interface_name").is_none(),
                "macOS must omit interface_name so sing-box picks utunN; got {tun}"
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(tun["interface_name"], "nexus-tun");
        }
    }

    #[test]
    fn private_bypass_carves_tun_from_172_16() {
        let list = private_range_bypass_excluding_tun("172.19.0.1/24");
        assert!(list.contains(&"10.0.0.0/8".into()));
        assert!(!list.iter().any(|s| s == "172.16.0.0/12"));
        // hole 172.19.0.0/24 → siblings cover the rest of 172.16/12
        assert!(list.iter().any(|s| s.starts_with("172.")));
        assert!(!list.iter().any(|s| s == "172.19.0.0/24"));
    }
    #[test]
    fn route_defaults_proxy_final() {
        let p = sample_node(json!({"type":"socks","tag":"proxy","server":"1.1.1.1","server_port":1080}));
        let v = gen(&p, 2080, false);
        assert_eq!(v["route"]["final"], "proxy");
        assert_eq!(v["route"]["auto_detect_interface"], true);
        assert_eq!(
            v["route"]["find_process"],
            true,
            "conn table / process enricher need route.find_process"
        );
        let rules = v["route"]["rules"].as_array().expect("rules");
        assert!(rules.iter().any(|r| r.get("action") == Some(&json!("sniff"))));
        assert!(rules.iter().any(|r| r.get("ip_is_private") == Some(&json!(true))
            && r.get("outbound") == Some(&json!("direct"))));
        assert_eq!(v["inbounds"][0]["type"], "mixed");
        assert_eq!(v["inbounds"].as_array().unwrap().len(), 1); // no tun
        assert!(v["experimental"]["clash_api"].is_object(), "clash_api required for QueryConnections");
        let path = v["experimental"]["cache_file"]["path"]
            .as_str()
            .expect("cache_file.path required (Core cwd is /)");
        assert!(
            path.starts_with('/') || path.contains(':'),
            "cache_file.path must be absolute, got {path}"
        );
    }

    /// Config resolver, PF :53 set, and OS argv must be the same list.
    /// Hardcoding 8.8.8.8 in generate.rs used to pass every other test.
    #[test]
    fn tun_resolver_is_in_pf_and_os_set() {
        use crate::firewall::rules::rules_fail_closed;
        use crate::sys::dns_servers_for_os;
        use crate::tunnel_sm::PeerEndpoint;
        use std::net::{IpAddr, Ipv4Addr};

        let cases: &[&[&str]] = &[
            &[],
            &["9.9.9.9"],
            &["dns.google"],
            &["dns.google", "9.9.9.9"],
            &["2001:4860:4860::8888"],
        ];
        let peer = PeerEndpoint {
            ip: IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)),
            port: 443,
            ips: vec![],
        };
        for raw in cases {
            let list: Vec<String> = raw.iter().map(|s| s.to_string()).collect();
            let cfg = generate_with_outbound(
                json!({"type":"socks","tag":"proxy","server":"1.2.3.4","server_port":1080}),
                2080,
                true,
                &list,
            );
            let servers = cfg["dns"]["servers"].as_array().expect("dns.servers");
            let resolver = servers
                .iter()
                .find(|s| s["tag"] == "dns-local")
                .and_then(|s| s["server"].as_str())
                .expect("tun dns-local.server")
                .to_string();
            let pf = rules_fail_closed(&peer, 2080, Some("utun9"), &list);
            let dns53 = pf
                .lines()
                .find(|l| l.contains("port 53") && l.starts_with("pass out"))
                .unwrap_or("");
            assert!(
                dns53.contains(&resolver),
                "PF must pass config resolver {resolver} for {raw:?}: {dns53}"
            );
            let os = dns_servers_for_os(&list);
            assert!(
                os.contains(&resolver),
                "OS argv must include config resolver {resolver} for {raw:?}: {os:?}"
            );
            if list.iter().any(|s| s == "9.9.9.9") {
                assert_eq!(resolver, "9.9.9.9", "{raw:?}");
                assert!(!os.contains(&"8.8.8.8".to_string()), "{raw:?} {os:?}");
            }
        }
    }

    #[test]
    fn mux_gate_unknown_no_inject() {
        let mut o = json!({"type":"vmess","uuid":"x"});
        apply_mux_gate(&mut o, "unknown", true, "smux", 8);
        assert!(o.get("multiplex").is_none());
    }

    #[test]
    fn mux_gate_yes_injects() {
        let mut o = json!({"type":"vmess","uuid":"x"});
        apply_mux_gate(&mut o, "yes", true, "smux", 8);
        assert_eq!(o["multiplex"]["enabled"], true);
        assert_eq!(o["multiplex"]["protocol"], "smux");
        assert_eq!(o["multiplex"]["max_streams"], 8);
    }

    #[test]
    fn mux_gate_explicit_preserved() {
        let mut o = json!({"type":"vmess","multiplex":{"enabled":false}});
        apply_mux_gate(&mut o, "yes", true, "smux", 8);
        assert_eq!(o["multiplex"]["enabled"], false);
    }

    #[test]
    fn dns_present_and_hijack_route() {
        let p = sample_node(json!({"type":"socks","tag":"proxy","server":"1.1.1.1","server_port":1080}));
        let v = gen(&p, 2080, false);
        assert!(v.get("dns").is_some(), "dns section required");
        let servers = v["dns"]["servers"].as_array().expect("dns.servers");
        assert!(servers.iter().any(|s| s["tag"] == "dns-remote"));
        assert!(servers.iter().any(|s| s["tag"] == "dns-local"));
        // non-direct → remote detours proxy
        let remote = servers.iter().find(|s| s["tag"] == "dns-remote").unwrap();
        assert_eq!(remote["detour"], "proxy");
        assert_eq!(v["dns"]["final"], "dns-remote");
        let rules = v["route"]["rules"].as_array().unwrap();
        assert!(rules.iter().any(|r| r.get("action") == Some(&json!("hijack-dns"))
            || (r.get("protocol") == Some(&json!("dns")) && r.get("action") == Some(&json!("hijack-dns")))));
    }

    #[test]
    fn dns_direct_outbound_no_detour() {
        let p = sample_node(json!({"type":"direct","tag":"proxy"}));
        let v = gen(&p, 2080, false);
        let remote = v["dns"]["servers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["tag"] == "dns-remote")
            .unwrap();
        assert!(remote.get("detour").is_none(), "detour→direct rejected by sing-box");
        assert_eq!(v["dns"]["final"], "dns-direct");
    }

    #[test]
    fn dns_proxy_hostname_uses_direct_resolver() {
        let p = sample_node(json!({
            "type":"vmess","tag":"proxy",
            "server":"us6.example.com","server_port":443,"uuid":"x"
        }));
        let v = gen(&p, 2080, false);
        let rules = v["dns"]["rules"].as_array().expect("dns.rules");
        assert!(
            rules.iter().any(|r| {
                r.get("server") == Some(&json!("dns-direct"))
                    && r.get("domain")
                        .and_then(|d| d.as_array())
                        .map(|a| a.iter().any(|x| x == "us6.example.com"))
                        .unwrap_or(false)
            }),
            "proxy server hostname must bootstrap via dns-direct; got {rules:?}"
        );
        assert_eq!(
            v["route"]["default_domain_resolver"]["server"],
            "dns-direct",
            "missing upstream default_domain_resolver → DNS loopback on proxy dial"
        );
        assert_eq!(
            v["outbounds"][0]["domain_resolver"],
            "dns-direct",
            "proxy outbound must pin domain_resolver for hostname server"
        );
    }

    #[test]
    fn dns_tls_sni_also_bootstraps() {
        let p = sample_node(json!({
            "type":"http","tag":"proxy",
            "server":"edge.example.net","server_port":443,
            "tls":{"enabled":true,"server_name":"sni.example.net"}
        }));
        let v = gen(&p, 2080, false);
        let rules = v["dns"]["rules"].as_array().expect("dns.rules");
        let domains = rules
            .iter()
            .find(|r| r.get("server") == Some(&json!("dns-direct")))
            .and_then(|r| r.get("domain"))
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(domains.iter().any(|x| x == "edge.example.net"));
        assert!(domains.iter().any(|x| x == "sni.example.net"));
    }

}
