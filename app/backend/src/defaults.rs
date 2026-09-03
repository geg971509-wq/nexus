//! Product defaults — one place to change, used by IPC / store / UI identity.
//! Bump app version only in Cargo.toml (`version`); this re-exports it.

/// Mixed inbound + system-proxy port (sing-box generate + OS proxy).
pub const MIXED_PORT: u16 = 2080;

/// Same as package version in `app/backend/Cargo.toml`.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Desktop identifier (store / bundle id surface).
pub const APP_IDENTIFIER: &str = "app.nexus.desktop";

pub const APP_NAME: &str = "Nexus";

/// Bootstrap resolvers. The generator points DoH/UDP DNS at the first entry, and
/// the fail-closed firewall passes exactly this list — they must agree or PF
/// blocks the resolver the config just chose. Override via store `dns_bootstrap`.
pub const DEFAULT_DNS_BOOTSTRAP: [&str; 4] = ["8.8.8.8", "8.8.4.4", "1.1.1.1", "1.0.0.1"];

/// Drop hostnames / junk, fall back to the product default.
/// Store, generate, PF, and OS argv all go through here so a hostname cannot
/// become the config resolver while PF/OS silently use 8.8.8.8.
pub fn sanitize_dns_bootstrap(raw: &[String]) -> Vec<String> {
    let v: Vec<String> = raw
        .iter()
        .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
        .cloned()
        .collect();
    if v.is_empty() {
        DEFAULT_DNS_BOOTSTRAP
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        v
    }
}
