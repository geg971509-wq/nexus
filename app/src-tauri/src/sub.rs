//! Subscription HTTP fetch (upstream GroupUpdater::HttpGet equivalent).
//! Uses system `curl` on macOS so we get TLS + redirects without extra crates.

use serde_json::json;
use std::process::Command;

fn is_http_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("http://") || u.starts_with("https://")
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

    // Prefer curl: reliable TLS roots on macOS, follows redirects.
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "30",
            "--connect-timeout",
            "12",
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
            "status": 0,
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
        "status": 200,
        "error": null,
        "bytes": bytes,
        "url": url,
    }))
}

