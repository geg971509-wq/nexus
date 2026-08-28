//! Exit IP / country, measured through the tunnel.
//!
//! The whole point of the field is "where does my traffic actually come out", so
//! the request must go through the local mixed inbound. A direct request would
//! report this machine's own address — the wrong answer, stated confidently.
//!
//! Needs no firewall change: the fail-closed ruleset already passes loopback →
//! mixed port and Core → peer, and proxying by CONNECT leaves the hostname for
//! Core to resolve, so no local DNS is involved either.
//!
//! Cloudflare's trace endpoint returns `ip=` and `loc=` as plain `key=value`
//! text, with no API key and no quota to run out of.

use std::process::Command;

const TRACE_URL: &str = "https://www.cloudflare.com/cdn-cgi/trace";

/// `{ ip, country }` as the far end sees us.
///
/// Not gated on tunnel state on purpose: routing through the proxy *is* the gate.
/// If Core is not listening, curl fails and the caller shows nothing — there is
/// no path here that can return this machine's own address.
pub fn probe(mixed_port: u16) -> Result<serde_json::Value, String> {
    let proxy = format!("http://127.0.0.1:{mixed_port}");
    let mut cmd = Command::new("/usr/bin/curl");
    let out = cmd
        .args([
            "-fsS",
            "--max-time",
            "8",
            "--connect-timeout",
            "5",
            // The trace body is a few hundred bytes; anything larger is not it.
            "--max-filesize",
            "8192",
            "--proto",
            "=https",
            "-x",
            &proxy,
            TRACE_URL,
        ])
        .output()
        .map_err(|e| format!("spawn curl: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("curl exit {}", out.status)
        } else {
            err
        });
    }

    let body = String::from_utf8_lossy(&out.stdout);
    let ip = trace_field(&body, "ip")
        .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
        .ok_or("trace response carried no usable ip")?;
    let country = trace_field(&body, "loc")
        .filter(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_alphabetic()))
        .unwrap_or_default();
    Ok(serde_json::json!({ "ip": ip, "country": country }))
}

/// Value for `key` in a `key=value` line. Remote input, so an unexpected body
/// yields None rather than a partial parse.
fn trace_field(body: &str, key: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.strip_prefix('=')?;
        Some(rest.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "fl=123abc\nh=www.cloudflare.com\nip=203.0.113.7\nts=1.0\nvisit_scheme=https\ncolo=NRT\nloc=JP\ntls=TLSv1.3\n";

    #[test]
    fn reads_ip_and_loc_and_ignores_prefix_collisions() {
        assert_eq!(trace_field(SAMPLE, "ip").as_deref(), Some("203.0.113.7"));
        assert_eq!(trace_field(SAMPLE, "loc").as_deref(), Some("JP"));
        // `ts=` must not answer a lookup for `t`, and a missing key is None.
        assert_eq!(trace_field(SAMPLE, "t"), None);
        assert_eq!(trace_field(SAMPLE, "warp"), None);
    }

    /// A v6 exit address must pass the guard, so it has to be IpAddr and not
    /// Ipv4Addr. Address from the RFC 3849 documentation range.
    #[test]
    fn accepts_ipv6_exit_address() {
        let body = "h=www.cloudflare.com\nip=2001:db8:1::a2b:c3d\ncolo=XYZ\nloc=NL\nwarp=off\n";
        let ip = trace_field(body, "ip").filter(|s| s.parse::<std::net::IpAddr>().is_ok());
        assert_eq!(ip.as_deref(), Some("2001:db8:1::a2b:c3d"));
        assert_eq!(trace_field(body, "loc").as_deref(), Some("NL"));
    }

    /// A body that is not the trace endpoint (captive portal, proxy error page)
    /// must not become a displayed "exit IP".
    #[test]
    fn rejects_non_ip_and_bad_country() {
        let junk = "ip=not-an-address\nloc=NOTACODE\n";
        assert!(trace_field(junk, "ip")
            .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
            .is_none());
        assert!(trace_field(junk, "loc")
            .filter(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_alphabetic()))
            .is_none());
    }
}
