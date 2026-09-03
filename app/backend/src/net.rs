//! Real network probes for node context menu (upstream URL-test / resolve-IP subset).
//!
//! When Core session is running, UI must NOT call this — use `core_url_test_current`
//! (TestCurrent via live box proxy). This path is only for disconnected URL-menu
//! TCP reachability (direct to server:port).
//!
//! Under Tun, a plain `TcpStream::connect` is accepted by the local gvisor/utun stack
//! in ~0–2 ms (hairpin), so latency looks fake-green. Probe sockets bind the physical
//! NIC via `IP_BOUND_IF` / `IPV6_BOUND_IF` so SYNs leave en0/… and skip utun.
//!
//! Progressive results: each finished probe is delivered via callback (UI emit).

use serde::Serialize;
use serde_json::json;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
#[cfg(test)]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Probe batch ids. Abort marks all issued ids ≤ ABORTED_THROUGH dead;
/// a later begin_probe_batch() issues a higher id and is live again (no global sticky flag).
static NEXT_BATCH: AtomicU64 = AtomicU64::new(1);
static ABORTED_THROUGH: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
pub(crate) fn probe_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Start a new probe batch; returns id. Concurrent batches stay live until abort.
pub fn begin_probe_batch() -> u64 {
    NEXT_BATCH.fetch_add(1, Ordering::SeqCst)
}

/// Abort every batch that has already been issued (UI stopSpeedtest).
pub fn abort_probes() {
    let next = NEXT_BATCH.load(Ordering::SeqCst);
    ABORTED_THROUGH.store(next.saturating_sub(1), Ordering::SeqCst);
}

pub fn is_batch_live(batch_id: u64) -> bool {
    batch_id != 0 && batch_id > ABORTED_THROUGH.load(Ordering::SeqCst)
}

/// True when the most recently issued batch id is already aborted.
#[allow(dead_code)] // retained for ad-hoc diagnostics; production uses is_batch_live
pub fn is_aborted() -> bool {
    let next = NEXT_BATCH.load(Ordering::SeqCst);
    let through = ABORTED_THROUGH.load(Ordering::SeqCst);
    next > 1 && through >= next.saturating_sub(1)
}

#[derive(Clone, Serialize)]
pub struct ProbeResult {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub ok: bool,
    /// TCP connect RTT ms; -1 = fail
    pub ms: Option<i64>,
    pub ip: Option<String>,
    pub error: Option<String>,
}

fn parse_target(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let host = host.trim().trim_matches(|c| c == '[' || c == ']');
    if host.is_empty() {
        return Err("empty host".into());
    }
    if port == 0 {
        return Err("invalid port".into());
    }
    let key = format!("{host}:{port}");
    key.to_socket_addrs()
        .map(|it| it.collect::<Vec<_>>())
        .map_err(|e| format!("dns: {e}"))
        .and_then(|v| {
            if v.is_empty() {
                Err("dns: no addresses".into())
            } else {
                Ok(v)
            }
        })
}

/// True for utun / loopback / Apple peer / virtual faces we must not bind for direct probe.
/// macOS physical bind only; kept under `test` so unit tests still run on other hosts.
#[cfg(any(target_os = "macos", test))]
fn is_virtual_ifname(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "lo0"
        || n.starts_with("lo")
        || n.starts_with("utun")
        || n.starts_with("awdl")
        || n.starts_with("llw")
        || n.starts_with("bridge")
        || n.starts_with("ap")
        || n.starts_with("gif")
        || n.starts_with("stf")
        || n.starts_with("anpi")
        || n.starts_with("ipsec")
        || n.starts_with("ppp")
        || n.starts_with("vmnet")
        || n.starts_with("vmenet")
        || n.starts_with("zt")
        || n.contains("tailscale")
}

/// Default-route interface name (`route -n get default`), if any.
#[cfg(target_os = "macos")]
fn default_route_ifname() -> Option<String> {
    let out = std::process::Command::new("/sbin/route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("interface:") {
            let name = rest.trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn if_nametoindex(name: &str) -> Option<u32> {
    let c = std::ffi::CString::new(name).ok()?;
    let idx = unsafe { libc::if_nametoindex(c.as_ptr()) };
    if idx == 0 {
        None
    } else {
        Some(idx)
    }
}

/// Physical NIC ifindex for direct dial under Tun.
/// Prefer non-virtual default route; else first UP en*/eth* with an IPv4.
#[cfg(target_os = "macos")]
fn physical_ifindex() -> Option<u32> {
    if let Some(name) = default_route_ifname() {
        if !is_virtual_ifname(&name) {
            if let Some(idx) = if_nametoindex(&name) {
                return Some(idx);
            }
        }
    }
    // Tun up → default is often utunN; pick first real en* with IPv4.
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 || ifap.is_null() {
            return None;
        }
        let mut best: Option<(u8, u32)> = None; // rank, ifindex
        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() {
                continue;
            }
            if (*ifa.ifa_addr).sa_family as i32 != libc::AF_INET {
                continue;
            }
            let flags = ifa.ifa_flags as i32;
            if flags & libc::IFF_UP == 0 || flags & libc::IFF_LOOPBACK != 0 {
                continue;
            }
            if ifa.ifa_name.is_null() {
                continue;
            }
            let name = match std::ffi::CStr::from_ptr(ifa.ifa_name).to_str() {
                Ok(s) => s,
                Err(_) => continue,
            };
            if is_virtual_ifname(name) {
                continue;
            }
            let rank = if name == "en0" {
                0u8
            } else if name.starts_with("en") {
                1
            } else if name.starts_with("eth") {
                2
            } else {
                3
            };
            let Some(idx) = if_nametoindex(name) else {
                continue;
            };
            match best {
                None => best = Some((rank, idx)),
                Some((r, _)) if rank < r => best = Some((rank, idx)),
                _ => {}
            }
        }
        libc::freeifaddrs(ifap);
        best.map(|(_, idx)| idx)
    }
}

#[cfg(target_os = "macos")]
fn set_bound_if(sock: &Socket, ifindex: u32, v6: bool) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    let fd = sock.as_raw_fd();
    let idx = ifindex as libc::c_uint;
    let (level, opt) = if v6 {
        (libc::IPPROTO_IPV6, libc::IPV6_BOUND_IF)
    } else {
        (libc::IPPROTO_IP, libc::IP_BOUND_IF)
    };
    let ret = unsafe {
        libc::setsockopt(
            fd,
            level,
            opt,
            &idx as *const _ as *const libc::c_void,
            std::mem::size_of_val(&idx) as libc::socklen_t,
        )
    };
    if ret != 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn tcp_connect_ms(
    addrs: &[SocketAddr],
    timeout: Duration,
    ifindex: Option<u32>,
    batch_id: u64,
) -> Result<(i64, String), String> {
    let mut last_err = String::from("connect failed");
    for addr in addrs {
        if !is_batch_live(batch_id) {
            return Err("aborted".into());
        }
        let domain = if addr.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let t0 = Instant::now();
        let sock = match Socket::new(domain, Type::STREAM, Some(Protocol::TCP)) {
            Ok(s) => s,
            Err(e) => {
                last_err = format!("socket: {e}");
                continue;
            }
        };
        if let Some(idx) = ifindex {
            // Best-effort: if bind-if fails, still try unbound (better than skip).
            let _ = set_bound_if(&sock, idx, addr.is_ipv6());
        }
        let sockaddr = SockAddr::from(*addr);
        match sock.connect_timeout(&sockaddr, timeout) {
            Ok(()) => {
                let ms = t0.elapsed().as_millis() as i64;
                return Ok((ms, addr.ip().to_string()));
            }
            Err(e) => last_err = format!("{addr}: {e}"),
        }
    }
    Err(last_err)
}

fn one_probe(
    id: String,
    host: String,
    port: u16,
    timeout: Duration,
    ifindex: Option<u32>,
    batch_id: u64,
) -> ProbeResult {
    if !is_batch_live(batch_id) {
        return ProbeResult {
            id,
            host,
            port,
            ok: false,
            ms: Some(-1),
            ip: None,
            error: Some("aborted".into()),
        };
    }
    match parse_target(&host, port) {
        Ok(addrs) => match tcp_connect_ms(&addrs, timeout, ifindex, batch_id) {
            Ok((ms, ip)) => ProbeResult {
                id,
                host,
                port,
                ok: true,
                ms: Some(ms),
                ip: Some(ip),
                error: None,
            },
            Err(e) => ProbeResult {
                id,
                host,
                port,
                ok: false,
                ms: Some(-1),
                ip: None,
                error: Some(e),
            },
        },
        Err(e) => ProbeResult {
            id,
            host,
            port,
            ok: false,
            ms: Some(-1),
            ip: None,
            error: Some(e),
        },
    }
}

/// Concurrent TCP probes. `on_each` is called as soon as each target finishes
/// (upstream progressive latency paint). Returns all results when the batch ends.
pub fn probe_batch_progressive<F>(
    targets: &[serde_json::Value],
    timeout_ms: u64,
    concurrency: usize,
    mut on_each: F,
) -> Vec<ProbeResult>
where
    F: FnMut(&ProbeResult),
{
    let batch_id = begin_probe_batch();
    // Once per batch: under Tun, default route is often utun — bind en0/… instead.
    let ifindex = physical_ifindex();
    let timeout = Duration::from_millis(timeout_ms.clamp(200, 30_000));
    // Throne MaxConcurrentTests=100; keep headroom for free-list ~300 nodes
    let conc = concurrency.clamp(1, 100);
    if targets.is_empty() {
        return Vec::new();
    }

    let (tx, rx) = mpsc::channel::<ProbeResult>();
    let sem = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::with_capacity(targets.len());

    for t in targets {
        if !is_batch_live(batch_id) {
            break;
        }
        let id = t
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let host = t
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let port = t
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(443)
            .min(65535) as u16;
        let tx_c = tx.clone();
        let sem_c = Arc::clone(&sem);
        loop {
            if !is_batch_live(batch_id) {
                break;
            }
            let mut g = sem_c.lock().unwrap();
            if *g < conc {
                *g += 1;
                break;
            }
            drop(g);
            thread::sleep(Duration::from_millis(2));
        }
        if !is_batch_live(batch_id) {
            break;
        }
        // Capture batch_id so late workers can skip work after abort/new batch.
        let bid = batch_id;
        handles.push(thread::spawn(move || {
            let res = if is_batch_live(bid) {
                one_probe(id, host, port, timeout, ifindex, bid)
            } else {
                ProbeResult {
                    id,
                    host,
                    port,
                    ok: false,
                    ms: Some(-1),
                    ip: None,
                    error: Some("aborted".into()),
                }
            };
            let _ = tx_c.send(res);
            if let Ok(mut g) = sem_c.lock() {
                *g = g.saturating_sub(1);
            }
        }));
    }
    drop(tx);

    let mut results = Vec::with_capacity(targets.len());
    while let Ok(res) = rx.recv() {
        on_each(&res);
        results.push(res);
    }
    for h in handles {
        let _ = h.join();
    }
    results
}

/// Concurrent TCP probes (no progressive callback).
#[allow(dead_code)]
pub fn probe_batch(
    targets: &[serde_json::Value],
    timeout_ms: u64,
    concurrency: usize,
) -> Vec<ProbeResult> {
    probe_batch_progressive(targets, timeout_ms, concurrency, |_| {})
}

#[derive(Clone, Serialize)]
pub struct ResolveResult {
    pub id: String,
    pub host: String,
    pub ok: bool,
    pub ips: Vec<String>,
    pub error: Option<String>,
}

/// Concurrent DNS. `on_each` fires as soon as each host finishes (same progressive model as TCP probe).
/// Shares probe batch-id abort so stop cancels resolve-all too.
pub fn resolve_batch_progressive<F>(
    targets: &[serde_json::Value],
    concurrency: usize,
    mut on_each: F,
) -> Vec<ResolveResult>
where
    F: FnMut(&ResolveResult),
{
    let batch_id = begin_probe_batch();
    let conc = concurrency.clamp(1, 32);
    if targets.is_empty() {
        return Vec::new();
    }
    let (tx, rx) = mpsc::channel::<ResolveResult>();
    let sem = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::with_capacity(targets.len());

    for t in targets {
        if !is_batch_live(batch_id) {
            break;
        }
        let id = t
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let host = t
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tx_c = tx.clone();
        let sem_c = Arc::clone(&sem);
        loop {
            if !is_batch_live(batch_id) {
                break;
            }
            let mut g = sem_c.lock().unwrap();
            if *g < conc {
                *g += 1;
                break;
            }
            drop(g);
            thread::sleep(Duration::from_millis(2));
        }
        if !is_batch_live(batch_id) {
            break;
        }
        let bid = batch_id;
        handles.push(thread::spawn(move || {
            let res = if !is_batch_live(bid) {
                ResolveResult {
                    id,
                    host,
                    ok: false,
                    ips: Vec::new(),
                    error: Some("aborted".into()),
                }
            } else {
                match resolve_host(&host) {
                    Ok(ips) => ResolveResult {
                        id,
                        host,
                        ok: true,
                        ips,
                        error: None,
                    },
                    Err(e) => ResolveResult {
                        id,
                        host,
                        ok: false,
                        ips: Vec::new(),
                        error: Some(e),
                    },
                }
            };
            let _ = tx_c.send(res);
            if let Ok(mut g) = sem_c.lock() {
                *g = g.saturating_sub(1);
            }
        }));
    }
    drop(tx);
    let mut results = Vec::with_capacity(targets.len());
    while let Ok(res) = rx.recv() {
        on_each(&res);
        results.push(res);
    }
    for h in handles {
        let _ = h.join();
    }
    results
}

pub fn resolve_host(host: &str) -> Result<Vec<String>, String> {
    let host = host.trim().trim_matches(|c| c == '[' || c == ']');
    if host.is_empty() {
        return Err("empty host".into());
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(vec![host.to_string()]);
    }
    let addrs = format!("{host}:0")
        .to_socket_addrs()
        .map_err(|e| format!("dns: {e}"))?;
    let mut ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
    ips.sort();
    ips.dedup();
    if ips.is_empty() {
        Err("dns: no addresses".into())
    } else {
        Ok(ips)
    }
}

#[allow(dead_code)]
pub fn probe_json(
    targets: Vec<serde_json::Value>,
    timeout_ms: u64,
    concurrency: usize,
) -> serde_json::Value {
    let results = probe_batch(&targets, timeout_ms, concurrency);
    let aborted = results
        .iter()
        .any(|r| r.error.as_deref() == Some("aborted"));
    json!({ "results": results, "aborted": aborted })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn probe_localhost_or_fail_honest() {
        let _g = probe_test_lock();
        let res = probe_batch(&[json!({"id":"a","host":"127.0.0.1","port":1})], 500, 2);
        assert_eq!(res.len(), 1);
        assert!(!res[0].ok);
        assert_eq!(res[0].ms, Some(-1));
    }

    #[test]
    fn resolve_batch_fires() {
        let _g = probe_test_lock();
        let hits = Arc::new(Mutex::new(0usize));
        let hits_c = Arc::clone(&hits);
        let res = resolve_batch_progressive(
            &[
                json!({"id":"a","host":"127.0.0.1"}),
                json!({"id":"b","host":"::1"}),
            ],
            2,
            move |_| {
                *hits_c.lock().unwrap() += 1;
            },
        );
        assert_eq!(res.len(), 2);
        assert_eq!(*hits.lock().unwrap(), 2);
        assert!(res.iter().all(|r| r.ok));
    }

    #[test]
    fn resolve_ip_passthrough() {
        let ips = resolve_host("1.1.1.1").unwrap();
        assert_eq!(ips, vec!["1.1.1.1".to_string()]);
    }

    #[test]
    fn progressive_callback_fires_per_result() {
        let _g = probe_test_lock();
        let hits = Arc::new(Mutex::new(0usize));
        let hits_c = Arc::clone(&hits);
        let res = probe_batch_progressive(
            &[
                json!({"id":"a","host":"127.0.0.1","port":1}),
                json!({"id":"b","host":"127.0.0.1","port":1}),
            ],
            400,
            2,
            move |_| {
                *hits_c.lock().unwrap() += 1;
            },
        );
        assert_eq!(res.len(), 2);
        assert_eq!(*hits.lock().unwrap(), 2);
    }

    #[test]
    fn abort_kills_issued_batch_not_next() {
        let _g = probe_test_lock();
        let id = begin_probe_batch();
        assert!(is_batch_live(id));
        abort_probes();
        assert!(!is_batch_live(id));
        let id2 = begin_probe_batch();
        assert!(is_batch_live(id2));
        assert!(!is_batch_live(id));
    }

    #[test]
    fn one_probe_abort_uses_canonical_string() {
        let _g = probe_test_lock();
        let id = begin_probe_batch();
        abort_probes();
        let r = one_probe(
            "x".into(),
            "127.0.0.1".into(),
            1,
            Duration::from_millis(100),
            None,
            id,
        );
        assert!(!r.ok);
        assert_eq!(r.error.as_deref(), Some("aborted"));
    }

    #[test]
    fn resolve_batch_respects_abort() {
        let _g = probe_test_lock();
        // resolve_batch_progressive mints its own batch; abort while it runs.
        let handle = thread::spawn(|| {
            // Many targets + conc=1 stretches the spawn loop so abort can win.
            let mut targets = Vec::new();
            for i in 0..40 {
                targets.push(json!({"id": format!("n{i}"), "host": "127.0.0.1"}));
            }
            resolve_batch_progressive(&targets, 1, |_| {})
        });
        thread::sleep(Duration::from_millis(5));
        abort_probes();
        let res = handle.join().expect("resolve thread");
        assert!(
            res.is_empty()
                || res.iter().any(|r| r.error.as_deref() == Some("aborted"))
                || res.len() < 40,
            "expected abort to cut short, got {} rows",
            res.len()
        );
    }

    #[test]
    fn virtual_ifnames_detected() {
        assert!(is_virtual_ifname("utun3"));
        assert!(is_virtual_ifname("lo0"));
        assert!(is_virtual_ifname("awdl0"));
        assert!(!is_virtual_ifname("en0"));
        assert!(!is_virtual_ifname("en1"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn physical_ifindex_resolves_on_macos() {
        // Developer Macs always have at least lo0; physical may be en0.
        // Just ensure the helper does not panic; ifindex None is ok in CI sandboxes.
        let _ = physical_ifindex();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bound_if_setsockopt_on_socket() {
        let Some(idx) = physical_ifindex() else {
            return; // sandbox / no NIC
        };
        let sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        set_bound_if(&sock, idx, false).expect("IP_BOUND_IF");
    }
}
