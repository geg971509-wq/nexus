//! Detect VLESS inputs unsupported by the bundled sing-box-only engine.
//!
//! Port of Throne `Configs::useXrayVless` (`src/configs/common/utils.cpp`) and the
//! `vlessFromClash` arm of its protocol dispatch (`src/configs/sub/GroupUpdater.cpp`).
//! Throne also has an `xray_vless_preference` setting that can force *all* VLESS
//! (or all reality) onto Xray; Nexus has no such setting, so only the clauses that
//! hold under upstream's default are ported — a node is Xray-only when sing-box
//! genuinely cannot carry it.
//!
//! Nexus intentionally does not bundle Xray Core, so these nodes are refused at
//! import with a named reason rather than left to fail at connect.

use serde_json::Value;

/// Label used when such a node is turned away, so the import log can name it.
pub const UNSUPPORTED_VLESS_LABEL: &str = "vless-xray";

/// `useXrayVless` for a share URI. Only meaningful for `vless://` links.
pub fn is_unsupported_link(link: &str) -> bool {
    let Some((_, rest)) = link.split_once("://") else {
        return false;
    };
    let query = rest
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(rest)
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("");
    let get = |key: &str| -> Option<String> {
        query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k.eq_ignore_ascii_case(key)).then(|| v.to_string())
        })
    };
    is_unsupported(
        get("type").as_deref(),
        get("encryption").as_deref(),
        get("extra").as_deref(),
    )
}

/// `vlessFromClash`: xhttp network, or an encryption other than `none`.
pub fn is_unsupported_clash(proxy: &Value) -> bool {
    let f = |k: &str| proxy.get(k).and_then(|v| v.as_str());
    is_unsupported(f("network"), f("encryption"), None)
}

/// The shared rule, so the link and Clash paths cannot drift apart.
fn is_unsupported(network: Option<&str>, encryption: Option<&str>, extra: Option<&str>) -> bool {
    let xhttp = network.is_some_and(|n| n.eq_ignore_ascii_case("xhttp"));
    // Upstream treats absent and "none" alike; anything else is Xray-only.
    let encrypted = encryption.is_some_and(|e| !e.is_empty() && !e.eq_ignore_ascii_case("none"));
    let has_extra = extra.is_some_and(|e| !e.is_empty());
    xhttp || encrypted || has_extra
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const UUID: &str = "11111111-1111-1111-1111-111111111111";

    #[test]
    fn plain_vless_stays_on_sing_box() {
        // The ordinary cases: no query, encryption=none, ws, and reality —
        // reality only moves to Xray under a preference Nexus does not have.
        for link in [
            &format!("vless://{UUID}@a.example.com:443"),
            &format!("vless://{UUID}@a.example.com:443?encryption=none&type=ws&host=cdn.example#n"),
            &format!("vless://{UUID}@a.example.com:443?encryption=none&security=reality&pbk=k#n"),
        ] {
            assert!(!is_unsupported_link(link), "{link}");
        }
        assert!(!is_unsupported_clash(&json!({"type":"vless","network":"ws"})));
        assert!(!is_unsupported_clash(&json!({"type":"vless","encryption":"none"})));
    }

    #[test]
    fn xhttp_encryption_and_extra_are_unsupported() {
        assert!(is_unsupported_link(&format!("vless://{UUID}@a.example.com:443?type=xhttp&path=%2Fx")));
        assert!(is_unsupported_link(&format!("vless://{UUID}@a.example.com:443?encryption=mlkem768x25519plus")));
        assert!(is_unsupported_link(&format!("vless://{UUID}@a.example.com:443?extra=%7B%22a%22%3A1%7D")));
        assert!(is_unsupported_clash(&json!({"type":"vless","network":"xhttp"})));
        assert!(is_unsupported_clash(&json!({"type":"vless","encryption":"mlkem768x25519plus"})));
    }

    /// A `#fragment` may contain anything; it must not be mistaken for the query.
    #[test]
    fn fragment_is_not_parsed_as_query() {
        assert!(!is_unsupported_link(&format!("vless://{UUID}@a.example.com:443#type=xhttp")));
        assert!(!is_unsupported_link(&format!("vless://{UUID}@a.example.com:443#&encryption=aes")));
    }
}
