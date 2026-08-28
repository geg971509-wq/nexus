//! Tun interface-name discovery (macOS utun). Pure lookup: no session, no policy.
//!
//! Connect pins the next free `utunN` into the Core config; these are the fallbacks
//! for when the pin failed or Core picked a different one. The firewall needs the
//! real ifname before it can widen from peer-only to pass-on-tun.

pub(crate) fn detect_tun_ifname(before: &[String]) -> Option<String> {
    // Fallback only: planned interface_name should already be set. Order:
    // (1) 172.19.0.0/24 on utun (2) new utun vs pre-Start (3) live core.log utun.
    #[cfg(target_os = "macos")]
    {
        detect_nexus_tun_ifname(20, before) // ~1s; late via spawn_tun_if_rebind
    }
}

/// Poll for Core's tun after Start (iface appears slightly after LoadConfig).
#[cfg(target_os = "macos")]
pub(crate) fn detect_nexus_tun_ifname(attempts: u32, before: &[String]) -> Option<String> {
    for i in 0..attempts {
        if let Some(name) = ifname_nexus_tun_by_addr() {
            return Some(name);
        }
        if let Some(name) = new_utun_since(before) {
            return Some(name);
        }
        if let Some(name) = tun_if_from_core_log() {
            return Some(name);
        }
        if i + 1 < attempts {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    tun_if_from_core_log().or_else(|| new_utun_since(before))
}

/// Next free utunN (same algorithm as sing-tun CalculateInterfaceName on darwin).
#[cfg(target_os = "macos")]
pub(crate) fn next_free_utun() -> Option<String> {
    let mut max_idx: i32 = -1;
    for name in list_utun_names() {
        if let Some(rest) = name.strip_prefix("utun") {
            if let Ok(n) = rest.parse::<i32>() {
                if n > max_idx {
                    max_idx = n;
                }
            }
        }
    }
    let candidate = format!("utun{}", max_idx + 1);
    if crate::firewall::is_safe_ifname(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn if_exists(name: &str) -> bool {
    unsafe {
        let c = match std::ffi::CString::new(name) {
            Ok(c) => c,
            Err(_) => return false,
        };
        libc::if_nametoindex(c.as_ptr()) != 0
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn list_utun_names() -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 || ifap.is_null() {
            return names;
        }
        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_name.is_null() {
                continue;
            }
            if let Ok(name) = std::ffi::CStr::from_ptr(ifa.ifa_name).to_str() {
                if name.starts_with("utun")
                    && crate::firewall::is_safe_ifname(name)
                    && !names.iter().any(|n| n == name)
                {
                    names.push(name.to_string());
                }
            }
        }
        libc::freeifaddrs(ifap);
    }
    names.sort();
    names
}

#[cfg(target_os = "macos")]
fn new_utun_since(before: &[String]) -> Option<String> {
    let after = list_utun_names();
    let mut added: Vec<String> = after
        .into_iter()
        .filter(|n| !before.iter().any(|b| b == n))
        .collect();
    // Prefer highest utunN (sing-box tends to pick next free)
    added.sort_by(|a, b| {
        let na = a.trim_start_matches("utun").parse::<u32>().unwrap_or(0);
        let nb = b.trim_start_matches("utun").parse::<u32>().unwrap_or(0);
        na.cmp(&nb)
    });
    added.pop()
}

#[cfg(target_os = "macos")]
fn ifname_nexus_tun_by_addr() -> Option<String> {
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 || ifap.is_null() {
            return None;
        }
        let mut found = None;
        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() || ifa.ifa_name.is_null() {
                continue;
            }
            if (*ifa.ifa_addr).sa_family as i32 != libc::AF_INET {
                continue;
            }
            let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
            let ip = u32::from_be(sin.sin_addr.s_addr).to_be_bytes();
            // 172.19.0.0/24 (generate TUN_V4)
            if ip[0] != 172 || ip[1] != 19 || ip[2] != 0 {
                continue;
            }
            if let Ok(name) = std::ffi::CStr::from_ptr(ifa.ifa_name).to_str() {
                if crate::firewall::is_safe_ifname(name) && name.starts_with("utun") {
                    found = Some(name.to_string());
                    break;
                }
            }
        }
        libc::freeifaddrs(ifap);
        found
    }
}

/// Last live "started at utunN" from core.log. Ignores stale names (iface gone).
#[cfg(target_os = "macos")]
fn tun_if_from_core_log() -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let path = crate::paths::log_dir().join("core.log");
    let mut f = std::fs::File::open(&path).ok()?;
    let len = f.seek(SeekFrom::End(0)).ok()?;
    let start = len.saturating_sub(65536);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = String::new();
    f.read_to_string(&mut buf).ok()?;
    let mut last: Option<String> = None;
    for line in buf.lines() {
        // inbound/tun[tun-in]: started at utun5
        if let Some(idx) = line.find("started at utun") {
            let rest = &line[idx + "started at ".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if name.starts_with("utun") && crate::firewall::is_safe_ifname(&name) && if_exists(&name) {
                last = Some(name);
            }
        }
    }
    last
}
