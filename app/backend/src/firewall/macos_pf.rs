//! Root-side PF apply for NexusFwD. Uses `pfctl` crate (Apache-2.0/MIT) for enable +
//! main-ruleset anchor registration; rule text via `pfctl -a nexus -f`.

use super::rules::{self, ANCHOR};
use super::Policy;
use std::fs;
use std::io::Write;
use std::process::Command;

pub fn apply_as_root(policy: &Policy) -> Result<(), String> {
    match policy {
        Policy::Reset => reset_as_root(),
        Policy::Connecting {
            peer,
            mixed_port,
            dns,
            ..
        } => load_as_root(&rules::rules_fail_closed(peer, *mixed_port, None, dns)),
        Policy::Connected {
            peer,
            tun,
            mixed_port,
            tun_if,
            dns,
        } => {
            let iface = if *tun {
                tun_if.as_deref()
            } else {
                None
            };
            load_as_root(&rules::rules_fail_closed(peer, *mixed_port, iface, dns))
        }
        Policy::Blocked {
            peer,
            mixed_port,
            dns,
        } => load_as_root(&rules::rules_blocked(peer.as_ref(), *mixed_port, dns)),
    }
}

fn load_as_root(body: &str) -> Result<(), String> {
    ensure_anchor()?;
    let path = std::path::PathBuf::from("/var/run/nexus-pf.conf");
    {
        let mut f = fs::File::create(&path).map_err(|e| format!("write pf conf: {e}"))?;
        f.write_all(body.as_bytes())
            .map_err(|e| format!("write pf conf: {e}"))?;
    }
    let path_s = path.to_string_lossy();
    // No `-F all` first: pfctl loads an anchor's ruleset in a transaction, so `-f`
    // already replaces it atomically. Flushing beforehand empties the anchor, and
    // an empty anchor passes traffic until the load lands — a leak window on every
    // Connecting → Connected → Blocked transition of a fail-closed firewall.
    // State teardown is the `pfctl -k` below, not the flush.
    let load = format!("/sbin/pfctl -a {ANCHOR} -f '{path_s}'");
    sh_ok(&load)?;
    let _ = Command::new("/sbin/pfctl")
        .args(["-k", "0.0.0.0/0"])
        .output();
    Ok(())
}

fn reset_as_root() -> Result<(), String> {
    // Flush must succeed; empty load is the real clear.
    let shell = format!(
        "/sbin/pfctl -a {ANCHOR} -F all >/dev/null 2>&1; \
         /sbin/pfctl -a {ANCHOR} -f /dev/null"
    );
    sh_ok(&shell)?;
    // Drop stale conf so status/debug do not show last Blocked body after open.
    let _ = fs::remove_file("/var/run/nexus-pf.conf");
    // Leave PF enable ref as-is (other clients may hold -E).
    Ok(())
}

fn ensure_anchor() -> Result<(), String> {
    let mut pf = pfctl::PfCtl::new().map_err(|e| format!("pf open: {e}"))?;
    pf.try_enable().map_err(|e| format!("pf enable: {e}"))?;
    pf.try_add_anchor(ANCHOR, pfctl::AnchorKind::Filter)
        .map_err(|e| format!("add filter anchor: {e}"))?;
    Ok(())
}

fn sh_ok(shell: &str) -> Result<(), String> {
    let out = Command::new("/bin/sh")
        .args(["-c", shell])
        .output()
        .map_err(|e| format!("sh: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "pf apply: {} {}",
            String::from_utf8_lossy(&out.stderr).trim(),
            String::from_utf8_lossy(&out.stdout).trim()
        ))
    }
}
