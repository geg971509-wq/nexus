//! Subscription HTTP fetch (upstream GroupUpdater::HttpGet equivalent).
//! Uses system `curl` on macOS so we get TLS + redirects without extra crates.

use serde_json::json;
use std::process::Command;

/// Ceiling for a fetched subscription body. Real ones are KBs; this is only here
/// to keep a hostile URL from streaming until --max-time.
const MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;

fn is_http_url(url: &str) -> bool {
    let u = url.trim();
    let Some((scheme, rest)) = u.split_once("://") else {
        return false;
    };
    !rest.is_empty()
        && !u.chars().any(char::is_whitespace)
        && (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
}

/// Fetch subscription body. Returns `{ ok, body, status, error, bytes }`.
pub fn fetch(url: &str) -> Result<serde_json::Value, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("empty subscription url".into());
    }
    if !is_http_url(url) {
        return Err(format!("url must be http(s): {url}"));
    }
    if url.len() > 4096 {
        return Err("url too long".into());
    }

    // System curl supplies the platform TLS roots and follows redirects.
    let mut cmd = Command::new("/usr/bin/curl");
    let out = cmd
        .args([
            "-fsSL",
            "--max-time",
            "30",
            "--connect-timeout",
            "12",
            // Without a ceiling a hostile URL just streams for --max-time: curl's
            // stdout, the lossy-UTF8 copy, the IPC JSON and the UI parser all
            // grow unbounded. Aborts mid-transfer, so Content-Length lies don't help.
            // Real subscriptions are KBs; the Go core bounds its own input at 16 MiB.
            "--max-filesize",
            &MAX_BODY_BYTES.to_string(),
            // Explicit, not curl's default: a redirect must not leave http/https.
            "--proto",
            "=https,http",
            "--proto-redir",
            "=https,http",
            "-A",
            "Nexus/0.2 (subscription)",
            "-H",
            "Accept: text/plain,application/json,*/*",
            "--compressed",
            url,
        ])
        .output()
        .map_err(|e| format!("spawn curl: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let msg = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout.chars().take(200).collect()
        } else {
            format!("curl exit {}", out.status)
        };
        return Ok(json!({
            "ok": false,
            "body": "",
            "error": msg,
            "bytes": 0,
            "url": url,
        }));
    }

    // Lossy UTF-8 is fine for share links / base64 / yaml text.
    let body = String::from_utf8_lossy(&out.stdout).to_string();
    let bytes = out.stdout.len();
    Ok(json!({
        "ok": true,
        "body": body,
        "error": null,
        "bytes": bytes,
        "url": url,
    }))
}

#[cfg(test)]
mod tests {
    use super::is_http_url;

    #[test]
    fn subscription_url_is_http_only_and_has_no_whitespace() {
        assert!(is_http_url("https://example.com/sub"));
        assert!(is_http_url("HTTP://example.com/sub"));
        assert!(!is_http_url("ftp://example.com/sub"));
        assert!(!is_http_url("https://"));
        assert!(!is_http_url("https://example.com/a b"));
    }
}
