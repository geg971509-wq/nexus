//! Startup-only repair for network settings left by an abnormal Nexus exit.

use crate::{core::session::CoreSession, network_restore};
use std::path::Path;
use std::process::Command;

fn other_nexus_gui_alive_from_ps(ps: &str, me: u32) -> bool {
    ps.lines().any(|line| {
        let line = line.trim();
        let Some((pid_s, command)) = line.split_once(char::is_whitespace) else {
            return false;
        };
        let Ok(pid) = pid_s.trim().parse::<u32>() else {
            return false;
        };
        if pid == me {
            return false;
        }
        Path::new(command.trim())
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "nexus")
            .unwrap_or(false)
    })
}

fn other_nexus_gui_alive() -> bool {
    let Ok(out) = Command::new("/bin/ps")
        .args(["-axo", "pid=,comm="])
        .output()
    else {
        // If process ownership cannot be established, do not risk restoring a
        // live instance's transaction from a second GUI process.
        return true;
    };
    other_nexus_gui_alive_from_ps(&String::from_utf8_lossy(&out.stdout), std::process::id())
}

/// Called from the pre-event-loop store snapshot path. Normal launches have no
/// recovery file and return immediately. A recovery file means Nexus changed
/// Proxy/PAC and/or DNS and did not complete its matching restore.
pub(crate) fn recover_pending_network_state() -> Result<(), String> {
    if !network_restore::has_pending() {
        return Ok(());
    }
    if crate::core::session::SESSION
        .lock()
        .ok()
        .map(|session| session.is_some())
        .unwrap_or(true)
    {
        return Ok(());
    }
    if other_nexus_gui_alive() {
        return Err(
            "pending network recovery deferred because another Nexus GUI process is running".into(),
        );
    }

    // With no live GUI owner, any NexusCore belongs to the crashed process. Stop
    // it before restoring Proxy/DNS so traffic cannot race the recovery write.
    CoreSession::kill_stray_cores(None);
    let notes = network_restore::restore_all()?;
    crate::firewall::reset_best_effort();
    if !notes.is_empty() {
        eprintln!("Nexus startup recovery: {}", notes.join(" · "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_parser_ignores_self_and_matches_other_nexus() {
        let ps = "  10 /Applications/Nexus.app/Contents/MacOS/nexus\n  11 /bin/bash\n";
        assert!(!other_nexus_gui_alive_from_ps(ps, 10));
        assert!(other_nexus_gui_alive_from_ps(ps, 99));
    }

    #[test]
    fn ps_parser_does_not_match_similar_names() {
        let ps = "  10 /tmp/NexusCore\n  11 /tmp/nexus-helper\n";
        assert!(!other_nexus_gui_alive_from_ps(ps, 99));
    }
}
