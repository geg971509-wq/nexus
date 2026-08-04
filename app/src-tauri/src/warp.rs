//! Cloudflare WARP via bundled `warp-cli` (not full GUI.app).
//! Daemon may still be the system LaunchDaemon; CLI is resolved from Nexus bundle first.

use std::path::{Path, PathBuf};
use std::process::Command;

const SYSTEM_APP_CLI: &str = "/Applications/Cloudflare WARP.app/Contents/Resources/warp-cli";
const SYSTEM_CLI_LINK: &str = "/usr/local/bin/warp-cli";

/// Prefer Nexus-bundled binary, then common install locations.
pub fn resolve_warp_cli() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("NEXUS_WARP_CLI") {
        let pb = PathBuf::from(&p);
        if pb.is_absolute() && pb.is_file() {
            return Some(pb);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in [
                "warp-cli",
                "warp-cli-aarch64-apple-darwin",
                "warp-cli-x86_64-apple-darwin",
            ] {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
            if let Some(contents) = dir.parent() {
                let p = contents.join("Resources/warp/warp-cli");
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for c in [
        manifest.join("../../third_party/cloudflare-warp/warp-cli"),
        manifest.join("../../bin/warp-cli"),
        manifest.join("binaries/warp-cli"),
    ] {
        if c.is_file() {
            return Some(c);
        }
    }
    for c in [SYSTEM_CLI_LINK, SYSTEM_APP_CLI] {
        let p = PathBuf::from(c);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn run_cli(args: &[&str]) -> Result<String, String> {
    let bin = resolve_warp_cli().ok_or_else(|| {
        "warp-cli not found (bundle third_party/cloudflare-warp or install Cloudflare WARP)"
            .to_string()
    })?;
    let out = Command::new(&bin)
        .args(args)
        .output()
        .map_err(|e| format!("spawn {}: {e}", bin.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        if !stdout.is_empty() {
            Ok(stdout)
        } else if !stderr.is_empty() {
            Ok(stderr)
        } else {
            Ok(String::new())
        }
    } else {
        let msg = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("exit {}", out.status)
        };
        Err(msg)
    }
}

/// Official GUI Mode ↔ warp-cli `mode`:
/// Traffic and DNS (UDP) → warp
/// Traffic and DNS (HTTPS) → warp+doh
/// Traffic and DNS (TLS) → warp+dot
pub fn normalize_mode(input: &str) -> Result<&'static str, String> {
    let s = input.trim().to_ascii_lowercase().replace(' ', "");
    match s.as_str() {
        "warp" | "udp" | "trafficanddns(udp)" | "trafficanddnsudp" => Ok("warp"),
        "warp+doh" | "warp_doh" | "warpdoh" | "https" | "doh" | "trafficanddns(https)"
        | "trafficanddnshttps" => Ok("warp+doh"),
        "warp+dot" | "warp_dot" | "warpdot" | "tls" | "dot" | "trafficanddns(tls)"
        | "trafficanddnstls" => Ok("warp+dot"),
        other => Err(format!(
            "unknown WARP mode '{other}' (use udp|https|tls → warp|warp+doh|warp+dot)"
        )),
    }
}

pub fn mode_label(cli_mode: &str) -> &'static str {
    match cli_mode {
        "warp+doh" => "https",
        "warp+dot" => "tls",
        _ => "udp",
    }
}

pub fn status_connected() -> Result<bool, String> {
    match run_cli(&["-j", "status"]) {
        Ok(s) => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                let st = v
                    .get("status")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                return Ok(st == "connected" || st.contains("connected"));
            }
            Ok(s.to_ascii_lowercase().contains("connected"))
        }
        Err(_) => {
            let s = run_cli(&["status"]).unwrap_or_default();
            Ok(s.to_ascii_lowercase().contains("connected"))
        }
    }
}

fn read_operation_mode() -> Option<String> {
    let s = run_cli(&["-j", "settings", "list"]).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    v.get("settings")
        .and_then(|x| x.get("operation_mode"))
        .and_then(|x| x.as_str())
        .map(|x| x.to_string())
}

pub fn set_mode(mode: &str) -> Result<String, String> {
    let m = normalize_mode(mode)?;
    let _ = run_cli(&["mode", m])?;
    Ok(format!("warp-cli mode {m}"))
}

pub fn connect() -> Result<String, String> {
    let _ = run_cli(&["connect"])?;
    Ok("warp-cli connect".into())
}

pub fn disconnect() -> Result<String, String> {
    let _ = run_cli(&["disconnect"])?;
    Ok("warp-cli disconnect".into())
}

pub fn set_enabled(on: bool) -> Result<String, String> {
    if on {
        connect()
    } else {
        disconnect()
    }
}

pub fn open_warp_app() -> Result<String, String> {
    let app = Path::new("/Applications/Cloudflare WARP.app");
    if !app.is_dir() {
        return Err("Cloudflare WARP.app not in /Applications (optional GUI)".into());
    }
    Command::new("open")
        .arg(app)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok("opened Cloudflare WARP.app".into())
}

pub fn daemon_running_heuristic() -> bool {
    let out = Command::new("pgrep")
        .args(["-f", "CloudflareWARP"])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

pub fn status_json() -> serde_json::Value {
    let path = resolve_warp_cli().map(|p| p.display().to_string());
    let connected = status_connected().ok();
    let op = read_operation_mode();
    let ui_mode = op.as_deref().map(mode_label).unwrap_or("udp");
    serde_json::json!({
        "installed": path.is_some(),
        "cli_path": path,
        "connected": connected,
        "daemon_heuristic": daemon_running_heuristic(),
        "operation_mode": op,
        "ui_mode": ui_mode,
        "mode": "bundled-warp-cli",
        "vendored_client": true,
        "note": "GUI Mode UDP/HTTPS/TLS maps to warp-cli mode warp|warp+doh|warp+dot"
    })
}
