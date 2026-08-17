//! Unused Windows stand-in (netsh-class 4 policies). Product `apply()` never
//! calls this — `platform_support()` is Unsupported on non-macOS.

use super::Policy;
use std::net::IpAddr;
use std::sync::Mutex;

static INSTALLED: Mutex<bool> = Mutex::new(false);

pub fn apply_policy(policy: &Policy) -> Result<(), String> {
    if !is_elevated() {
        return Err("Windows firewall needs elevated process (run as Administrator)".into());
    }
    match policy {
        Policy::Reset => reset_filters(),
        Policy::Connecting {
            peer, mixed_port, ..
        }
        | Policy::Connected {
            peer, mixed_port, ..
        } => apply_permit_peer(peer.ip, peer.port, *mixed_port, true),
        Policy::Blocked {
            peer, mixed_port, ..
        } => {
            if let Some(p) = peer {
                apply_permit_peer(p.ip, p.port, *mixed_port, true)
            } else {
                apply_permit_peer(
                    IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                    0,
                    *mixed_port,
                    false,
                )
            }
        }
    }
}

fn is_elevated() -> bool {
    // Token elevation check via windows-sys
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut elev = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut n = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elev as *mut _ as *mut _,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut n,
            );
            CloseHandle(token);
            ok != 0 && elev.TokenIsElevated != 0
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// ponytail: full FWPM filter graph is large; v1 uses netsh advfirewall rules tagged NexusFw
/// as a complete 4-policy stand-in that apply+clear. Not as strong as WFP ALE callout, but
/// ships all policies without half-Active. Upgrade path: FWPM sublayer when needed.
fn apply_permit_peer(
    peer: IpAddr,
    port: u16,
    mixed_port: u16,
    with_peer: bool,
) -> Result<(), String> {
    reset_filters()?;
    // Block outbound by default via rule, then allow peer/LAN/local.
    // Windows Firewall profiles: use netsh with rule names NexusFw-*.
    run_netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=NexusFw-block-out",
        "dir=out",
        "action=block",
        "enable=yes",
        "profile=any",
    ])?;
    run_netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=NexusFw-allow-lo",
        "dir=out",
        "action=allow",
        "enable=yes",
        "profile=any",
        "remoteip=127.0.0.1",
    ])?;
    // LAN
    for cidr in ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "169.254.0.0/16"] {
        run_netsh(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name=NexusFw-allow-lan-{cidr}"),
            "dir=out",
            "action=allow",
            "enable=yes",
            "profile=any",
            &format!("remoteip={cidr}"),
        ])?;
    }
    if with_peer && port != 0 {
        let ip = peer.to_string();
        run_netsh(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            "name=NexusFw-allow-peer",
            "dir=out",
            "action=allow",
            "enable=yes",
            "profile=any",
            &format!("remoteip={ip}"),
            &format!("remoteport={port}"),
            "protocol=TCP",
        ])?;
        run_netsh(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            "name=NexusFw-allow-peer-udp",
            "dir=out",
            "action=allow",
            "enable=yes",
            "profile=any",
            &format!("remoteip={ip}"),
            &format!("remoteport={port}"),
            "protocol=UDP",
        ])?;
    }
    let _ = mixed_port;
    *INSTALLED.lock().unwrap_or_else(|e| e.into_inner()) = true;
    Ok(())
}

fn reset_filters() -> Result<(), String> {
    // Delete by name prefix — ignore missing
    for name in [
        "NexusFw-block-out",
        "NexusFw-allow-lo",
        "NexusFw-allow-peer",
        "NexusFw-allow-peer-udp",
        "NexusFw-allow-lan-10.0.0.0/8",
        "NexusFw-allow-lan-172.16.0.0/12",
        "NexusFw-allow-lan-192.168.0.0/16",
        "NexusFw-allow-lan-169.254.0.0/16",
    ] {
        let _ = run_netsh(&[
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("name={name}"),
        ]);
    }
    *INSTALLED.lock().unwrap_or_else(|e| e.into_inner()) = false;
    Ok(())
}

fn run_netsh(args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = Command::new("netsh")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("netsh: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "netsh {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}
