//! NexusFwD — root LaunchDaemon: PF apply over unix socket.
//! Socket: /var/run/nexusfwd.sock mode 0666 + mandatory getpeereid allowlist (L1).

#[cfg(target_os = "macos")]
fn main() {
    if let Err(e) = run() {
        eprintln!("nexusfwd fatal: {e}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
fn run() -> Result<(), String> {
    use std::os::unix::net::UnixListener;

    const SOCK: &str = "/var/run/nexusfwd.sock";
    const ALLOW_PATH: &str = "/var/run/nexusfwd.allow";

    let uid = unsafe { libc::getuid() };
    if uid != 0 {
        return Err(format!("must run as root (uid={uid})"));
    }

    // Seed allowlist with console user if file missing (install writes this too).
    if !std::path::Path::new(ALLOW_PATH).is_file() {
        if let Some(cu) = console_uid() {
            let _ = std::fs::write(ALLOW_PATH, format!("{cu}\n"));
            unsafe {
                let c = std::ffi::CString::new(ALLOW_PATH).unwrap();
                libc::chmod(c.as_ptr(), 0o644);
            }
        }
    }

    let _ = std::fs::remove_file(SOCK);
    let listener = UnixListener::bind(SOCK).map_err(|e| format!("bind {SOCK}: {e}"))?;
    // L1: 0666 + peer UID allowlist (not bare open).
    unsafe {
        let c = std::ffi::CString::new(SOCK).unwrap();
        libc::chmod(c.as_ptr(), 0o666);
    }
    eprintln!("nexusfwd listening on {SOCK}");

    for conn in listener.incoming() {
        match conn {
            // One thread per connection: handling inline means a peer that
            // connects and never writes wedges the accept loop, and with it every
            // later policy apply. The socket is 0666, so any local process can.
            Ok(stream) => {
                std::thread::spawn(move || {
                    if let Err(e) = handle_client(stream) {
                        eprintln!("client: {}", clamp_log(&e));
                    }
                });
            }
            Err(e) => eprintln!("accept: {}", clamp_log(&e.to_string())),
        }
    }
    Ok(())
}

/// Cap on any one line reaching this process's stderr.
///
/// launchd points that at /var/log/nexusfwd.log and never rotates it, so every
/// byte written here is unbounded growth on the root filesystem, driven by
/// whoever is connecting. Bounding at the sink means no future log line can
/// reopen that hole by accident.
#[cfg(target_os = "macos")]
fn clamp_log(msg: &str) -> String {
    const MAX: usize = 200;
    let one_line: String = msg
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX)
        .collect();
    if msg.chars().count() > MAX {
        format!("{one_line}… (+{} chars)", msg.chars().count() - MAX)
    } else {
        one_line
    }
}

#[cfg(target_os = "macos")]
fn console_uid() -> Option<u32> {
    // scutil-style: owner of /dev/console
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/dev/console")
        .ok()
        .map(|m| m.uid())
        .filter(|&u| u != 0)
}

/// Peers permitted to drive PF.
///
/// The console user is appended unconditionally, so the allow file does not gate
/// them — that is deliberate, not an oversight: a desktop VPN must stay
/// controllable after a reboot or a user switch, when the uid recorded at install
/// time may no longer be the one at the keyboard. The file only widens the set,
/// for the case where whoever installed the daemon is not the console user (an
/// SSH or MDM install). Treat this as "local interactive user", not access control.
#[cfg(target_os = "macos")]
fn allowed_uids() -> Vec<u32> {
    let mut v = vec![0u32]; // root always
    if let Ok(s) = std::fs::read_to_string("/var/run/nexusfwd.allow") {
        for line in s.lines() {
            if let Ok(u) = line.trim().parse::<u32>() {
                if !v.contains(&u) {
                    v.push(u);
                }
            }
        }
    }
    if let Some(cu) = console_uid() {
        if !v.contains(&cu) {
            v.push(cu);
        }
    }
    v
}

/// A policy request is a few hundred bytes. Without a ceiling, a peer that never
/// sends a newline grows this root process until the machine suffers.
#[cfg(target_os = "macos")]
const MAX_REQUEST_BYTES: u64 = 64 * 1024;

/// Read one request line, refusing anything that reaches the ceiling.
///
/// Split out from `handle_client` so the boundary is testable without a socket:
/// a peer that sends `MAX_REQUEST_BYTES` and no newline must be rejected, not
/// silently truncated into a shorter request that still parses.
#[cfg(target_os = "macos")]
fn read_request_line<R: std::io::BufRead>(reader: R) -> Result<String, String> {
    use std::io::BufRead;
    let mut limited = reader.take(MAX_REQUEST_BYTES);
    let mut line = String::new();
    let n = limited
        .read_line(&mut line)
        .map_err(|e| format!("read: {e}"))?;
    if n as u64 >= MAX_REQUEST_BYTES {
        return Err("request exceeds size limit".into());
    }
    Ok(line)
}

#[cfg(target_os = "macos")]
fn handle_client(stream: std::os::unix::net::UnixStream) -> Result<(), String> {
    use nexus_lib::firewall::{macos_pf, wire, Policy};
    use std::io::{BufReader, Write};
    use std::os::unix::io::AsRawFd;
    use std::time::Duration;

    /// The shell already bounds its side at 15s (firewall/macos.rs); the
    /// privileged side had no bound at all.
    const IO_TIMEOUT: Duration = Duration::from_secs(5);

    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

    let fd = stream.as_raw_fd();
    let mut euid: libc::uid_t = 0;
    let mut egid: libc::gid_t = 0;
    let rc = unsafe { libc::getpeereid(fd, &mut euid, &mut egid) };
    if rc != 0 {
        return Err("getpeereid failed".into());
    }
    let allow = allowed_uids();
    if !allow.contains(&euid) {
        // Refuse without applying PF.
        let mut stream = stream;
        let body = serde_json::to_string(&wire::Response {
            ok: false,
            err: Some(format!("peer uid {euid} not allowed")),
            helper: None,
        })
        .unwrap_or_else(|_| r#"{"ok":false,"err":"denied"}"#.into());
        let _ = stream.write_all(body.as_bytes());
        let _ = stream.write_all(b"\n");
        return Err(format!("denied uid {euid}"));
    }
    let _ = egid;

    let line = read_request_line(BufReader::new(
        stream.try_clone().map_err(|e| e.to_string())?,
    ))?;
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }

    // Never echo the request. It carries peer IPs, the resolver list and the tun
    // ifname, and the sink is a world-readable root-owned file that nothing
    // rotates — so echoing let any allowed caller write 64 KiB per connection
    // into it. The parse error and a length are what a bug report actually needs.
    let req: wire::Request = serde_json::from_str(line)
        .map_err(|e| format!("bad json ({e}) in {} byte request", line.len()))?;
    let resp = match req {
        wire::Request::Ping | wire::Request::Status => wire::Response {
            ok: true,
            err: None,
            helper: Some("nexusfwd".into()),
        },
        wire::Request::Reset => match macos_pf::apply_as_root(&Policy::Reset) {
            Ok(()) => wire::Response {
                ok: true,
                err: None,
                helper: None,
            },
            Err(e) => wire::Response {
                ok: false,
                err: Some(e),
                helper: None,
            },
        },
        wire::Request::Apply { policy } => match policy.into_policy() {
            Ok(p) => match macos_pf::apply_as_root(&p) {
                Ok(()) => wire::Response {
                    ok: true,
                    err: None,
                    helper: None,
                },
                Err(e) => wire::Response {
                    ok: false,
                    err: Some(e),
                    helper: None,
                },
            },
            Err(e) => wire::Response {
                ok: false,
                err: Some(e),
                helper: None,
            },
        },
    };

    let mut stream = stream;
    let body = serde_json::to_string(&resp).map_err(|e| e.to_string())?;
    stream
        .write_all(body.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn normal_request_passes_through() {
        let body = br#"{"op":"ping"}"#;
        let mut input = body.to_vec();
        input.push(b'\n');
        let got = read_request_line(&input[..]).unwrap();
        assert_eq!(got.trim(), r#"{"op":"ping"}"#);
    }

    /// The bound has to reject, not truncate: a silently shortened line could
    /// still be valid JSON and would apply a policy the peer never sent.
    #[test]
    fn newline_less_flood_is_rejected_not_truncated() {
        let flood = vec![b'a'; (MAX_REQUEST_BYTES as usize) * 2];
        let err = read_request_line(&flood[..]).unwrap_err();
        assert!(err.contains("size limit"), "{err}");
    }

    /// The guard is `>=`, so a line that fills the ceiling exactly is refused
    /// while one byte less still gets through.
    #[test]
    fn ceiling_is_inclusive() {
        let mut at = vec![b'a'; MAX_REQUEST_BYTES as usize - 1];
        at.push(b'\n');
        assert!(read_request_line(&at[..]).is_err());

        let mut under = vec![b'a'; MAX_REQUEST_BYTES as usize - 2];
        under.push(b'\n');
        assert!(read_request_line(&under[..]).is_ok());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod log_tests {
    use super::*;

    /// launchd never rotates this file, so a caller must not be able to choose how
    /// many bytes land in it.
    #[test]
    fn long_input_is_clamped_and_counted() {
        let huge = "x".repeat(64 * 1024);
        let out = clamp_log(&huge);
        assert!(out.chars().count() < 260, "len {}", out.chars().count());
        assert!(out.contains("+65336 chars"), "{out}");
    }

    #[test]
    fn short_input_is_passed_through() {
        assert_eq!(clamp_log("denied uid 501"), "denied uid 501");
    }

    /// Newlines would let a caller forge extra log lines; control bytes are
    /// flattened to spaces rather than reaching the file.
    #[test]
    fn control_characters_cannot_forge_lines() {
        let out = clamp_log("a\nnexusfwd listening on /tmp/evil\r\tb");
        assert!(!out.contains('\n') && !out.contains('\r') && !out.contains('\t'), "{out}");
        assert_eq!(out, "a nexusfwd listening on /tmp/evil  b");
    }
}
