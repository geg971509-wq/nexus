//! Pure PF rule text (no I/O). Shared by daemon + tests.
//!
//! Semantics aligned with Mullvad security.md + talpid-core/src/firewall/macos.rs:
//! always: lo0, DHCPv4/v6 client, NDP subset, peer endpoint(s), mixed local port
//! Connected+tun: allow all on tun_if; block bare DNS (port 53) outside that path
//! (proxy mode keeps DNS out so Core can resolve / DoH bootstrap)
//! always tail: block return out + block drop in

use crate::tunnel_sm::PeerEndpoint;
use std::net::IpAddr;

pub const ANCHOR: &str = "nexus";

pub fn is_safe_ifname(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn peer_ips(peer: &PeerEndpoint) -> Vec<IpAddr> {
    let mut v = peer.ips.clone();
    if v.is_empty() {
        v.push(peer.ip);
    } else if !v.contains(&peer.ip) {
        v.insert(0, peer.ip);
    }
    v
}

/// Mullvad: allow relay IP+port for tunnel establishment (all peers).
fn peer_pass(peer: &PeerEndpoint) -> String {
    let mut s = String::new();
    for ip in peer_ips(peer) {
        s.push_str(&format!(
            "pass out quick proto {{ tcp, udp }} from any to {} port {}\n",
            ip, peer.port
        ));
    }
    s
}

/// L2 essentials independent of LAN (Mullvad security.md always-on subset).
fn common_l2() -> String {
    let mut s = String::new();
    s.push_str("pass quick on lo0 all\n");
    s.push_str("pass out quick proto udp from any port 68 to any port 67\n");
    s.push_str("pass in quick proto udp from any port 67 to any port 68\n");
    s.push_str(
        "pass out quick inet6 proto udp from fe80::/10 port 546 to { ff02::1:2, ff05::1:3 } port 547\n",
    );
    s.push_str(
        "pass in quick inet6 proto udp from fe80::/10 port 547 to fe80::/10 port 546\n",
    );
    // NDP / IPv6 L2 (Mullvad typed NDP; we keep ipv6-icmp for stability)
    s.push_str("pass quick proto ipv6-icmp all\n");
    s
}

/// LAN + ULA + link-local + multicast. Emitted **after** DNS block in tun mode
/// (Mullvad: block DNS before allow LAN — first `quick` match wins).
fn lan_pass() -> String {
    let mut s = String::new();
    s.push_str(
        "pass out quick from any to { 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16, fe80::/10, fc00::/7 }\n",
    );
    s.push_str(
        "pass in quick from { 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16, fe80::/10, fc00::/7 } to any\n",
    );
    s.push_str(
        "pass out quick from any to { 224.0.0.0/24, 239.0.0.0/8, 255.255.255.255/32, ff01::/16, ff02::/16, ff03::/16, ff04::/16, ff05::/16 }\n",
    );
    s
}

fn mixed_pass(port: u16) -> String {
    // Local mixed: apps → Core and Core loopback
    format!(
        "pass in quick proto tcp from any to 127.0.0.1 port {port}\n\
         pass out quick proto tcp from 127.0.0.1 port {port} to any\n\
         pass quick proto tcp from 127.0.0.1 to 127.0.0.1 port {port}\n"
    )
}

/// PF list literal `{ a, b }` from validated resolver IPs.
/// Non-IP entries are dropped here too: this string is PF syntax, and rules.rs
/// is also reachable from the root daemon over the JSON wire.
fn dns_set(dns: &[String]) -> String {
    let ips = crate::defaults::sanitize_dns_bootstrap(dns);
    format!("{{ {} }}", ips.join(", "))
}

/// generate.rs dns-direct / dns-remote use DoH to the bootstrap IPs on :443.
/// Without this, fail-closed blocks Core bootstrap while still allowing peer.
fn doh_bootstrap_pass(dns: &[String]) -> String {
    format!(
        "pass out quick proto {{ tcp, udp }} from any to {} port 443\n",
        dns_set(dns)
    )
}

fn tail_block() -> &'static str {
    "block return out quick all\nblock drop in quick all\n"
}

/// Connecting / Connected(proxy) / early Connected without tun_if.
/// Proxy mode: allow DNS out so Core/system can resolve (not a WireGuard-only stack).
pub fn rules_fail_closed(
    peer: &PeerEndpoint,
    mixed_port: u16,
    tun_if: Option<&str>,
    dns: &[String],
) -> String {
    let mut s = String::from("# nexus fail-closed\n");
    s.push_str(&common_l2());
    s.push_str(&peer_pass(peer));
    s.push_str(&mixed_pass(mixed_port));
    s.push_str(&doh_bootstrap_pass(dns));
    if let Some(iface) = tun_if {
        if is_safe_ifname(iface) {
            // Mullvad Connected: pass tunnel first, block bare DNS, **then** LAN
            // (LAN before DNS-block lets 192.168.x.1:53 leak).
            s.push_str(&format!("pass quick on {iface} all\n"));
            // generate.rs tun dns-local is UDP <bootstrap>:53 (physical path via auto_detect).
            // Allow only bootstrap resolvers; blanket :53 stays blocked.
            s.push_str(&format!(
                "pass out quick proto {{ tcp, udp }} from any to {} port 53\n",
                dns_set(dns)
            ));
            s.push_str("block return out quick proto { tcp, udp } to any port 53\n");
            s.push_str(&lan_pass());
        } else {
            s.push_str(&lan_pass());
            s.push_str("pass out quick proto { tcp, udp } from any to any port 53\n");
        }
    } else {
        // No tunnel yet / system-proxy: LAN + DNS53 for Core bootstrap.
        s.push_str(&lan_pass());
        s.push_str("pass out quick proto { tcp, udp } from any to any port 53\n");
    }
    s.push_str(tail_block());
    s
}

pub fn rules_blocked(peer: Option<&PeerEndpoint>, mixed_port: u16, dns: &[String]) -> String {
    let mut s = String::from("# nexus blocked\n");
    s.push_str(&common_l2());
    s.push_str(&lan_pass());
    if let Some(p) = peer {
        s.push_str(&peer_pass(p));
    }
    s.push_str(&mixed_pass(mixed_port));
    // DoH + DNS53: reconnect must resolve proxy hostnames (getaddrinfo / dns-local).
    // Without :53, residual Blocked makes peer_from_outbound fail forever for non-IP servers.
    s.push_str(&doh_bootstrap_pass(dns));
    s.push_str("pass out quick proto { tcp, udp } from any to any port 53\n");
    s.push_str(tail_block());
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn peer() -> PeerEndpoint {
        PeerEndpoint {
            ip: IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)),
            port: 443,
            ips: vec![IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))],
        }
    }

    /// Product default resolvers (what the store yields when unset).
    fn dns() -> Vec<String> {
        crate::defaults::DEFAULT_DNS_BOOTSTRAP
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn connecting_has_peer_dns_and_block() {
        let r = rules_fail_closed(&peer(), 2080, None, &dns());
        assert!(r.contains("1.2.3.4"));
        assert!(r.contains("port 443"));
        assert!(r.contains("block return out"));
        assert!(r.contains("127.0.0.1 port 2080"));
        assert!(r.contains("port 53"), "proxy/connecting allows DNS: {r}");
        assert!(r.contains("8.8.8.8"), "DoH bootstrap must pass: {r}");
        assert!(r.contains("10.0.0.0/8"));
        assert!(r.contains("fe80::/10"));
    }

    #[test]
    fn blocked_keeps_doh_bootstrap() {
        let r = rules_blocked(Some(&peer()), 2080, &dns());
        assert!(r.contains("8.8.8.8"), "blocked still allows DoH bootstrap: {r}");
        // reconnect resolve needs DNS53 (hostname peers)
        assert!(r.contains("to any port 53"), "blocked must allow DNS53 for reconnect: {r}");
    }

    #[test]
    fn multi_ip_peer_emits_all() {
        let p = PeerEndpoint {
            ip: IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            port: 443,
            ips: vec![
                IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
                IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)),
            ],
        };
        let r = rules_fail_closed(&p, 2080, None, &dns());
        assert!(r.contains("1.1.1.1"));
        assert!(r.contains("1.0.0.1"));
    }

    #[test]
    fn tun_connected_blocks_dns_outside_tunnel() {
        let r = rules_fail_closed(&peer(), 2080, Some("utun9"), &dns());
        assert!(r.contains("pass quick on utun9 all"));
        assert!(
            r.contains("block return out quick proto { tcp, udp } to any port 53"),
            "tun mode must block bare DNS: {r}"
        );
        assert!(
            r.contains("to { 8.8.8.8, 8.8.4.4, 1.1.1.1, 1.0.0.1 } port 53"),
            "tun dns-local bootstrap resolvers must pass: {r}"
        );
        // DNS block must precede LAN pass (quick match order)
        let dns_i = r
            .find("block return out quick proto { tcp, udp } to any port 53")
            .expect("dns block");
        let lan_i = r.find("10.0.0.0/8").expect("lan");
        assert!(dns_i < lan_i, "LAN before DNS block leaks LAN DNS: {r}");
        // bootstrap :53 allow before blanket block
        let boot_i = r
            .find("to { 8.8.8.8, 8.8.4.4, 1.1.1.1, 1.0.0.1 } port 53")
            .expect("bootstrap dns");
        assert!(boot_i < dns_i, "bootstrap DNS after blanket block: {r}");
    }

    #[test]
    fn tun_if_injected_when_safe() {
        let r = rules_fail_closed(&peer(), 2080, Some("utun9"), &dns());
        assert!(r.contains("pass quick on utun9 all"));
        let bad = rules_fail_closed(&peer(), 2080, Some("utun0;rm"), &dns());
        assert!(!bad.contains("rm"));
    }

    /// Custom resolvers must reach PF, and the Google default must be gone —
    /// otherwise the config resolves via one server while PF passes another.
    #[test]
    fn custom_dns_replaces_default() {
        let custom = vec!["9.9.9.9".to_string(), "149.112.112.112".to_string()];
        let r = rules_fail_closed(&peer(), 2080, Some("utun9"), &custom);
        assert!(r.contains("{ 9.9.9.9, 149.112.112.112 } port 443"), "{r}");
        assert!(r.contains("{ 9.9.9.9, 149.112.112.112 } port 53"), "{r}");
        assert!(!r.contains("8.8.8.8"), "default must not leak in: {r}");
    }

    /// A hostname or shell fragment in the list is both invalid PF syntax and an
    /// injection vector; drop it and fall back rather than emitting it.
    #[test]
    fn non_ip_dns_entries_are_dropped() {
        let bad = vec!["dns.google".to_string(), "} \n pass out all \n #".to_string()];
        let r = rules_fail_closed(&peer(), 2080, Some("utun9"), &bad);
        assert!(!r.contains("dns.google"), "{r}");
        assert!(!r.contains("pass out all"), "{r}");
        assert!(r.contains("8.8.8.8"), "fell back to default: {r}");

        let mixed = vec!["9.9.9.9".to_string(), "dns.google".to_string()];
        let r = rules_fail_closed(&peer(), 2080, Some("utun9"), &mixed);
        assert!(r.contains("{ 9.9.9.9 } port 53"), "{r}");
        assert!(!r.contains("dns.google"), "{r}");
    }

    #[test]
    fn blocked_without_peer() {
        let r = rules_blocked(None, 2080, &dns());
        assert!(r.contains("block return out"));
        // DoH bootstrap uses 8.8.8.8 port 443; no peer pass
        assert!(!r.contains("1.2.3.4"));
        assert!(r.contains("8.8.8.8"));
        // DNS53 kept so next connect can resolve hostnames
        assert!(r.contains("to any port 53"));
    }
}
