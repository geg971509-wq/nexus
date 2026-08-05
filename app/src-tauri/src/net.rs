//! Real network probes for node context menu (Throne URL-test / resolve-IP subset).
//! Full proxy URL-test needs Core `Test` RPC + Start() config — not wired yet.
//! This measures TCP connect RTT to server:port (honest reachability, not proxy path).
//!
//! Progressive results: each finished probe is delivered via callback (UI emit),
//! matching Throne QueryURLTest poller — do not wait for the whole batch to paint.

use serde::Serialize;
use serde_json::json;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Shared abort for in-flight URL tests (Throne stopSpeedtest).
static PROBE_ABORT: AtomicBool = AtomicBool::new(false);

pub fn abort_probes() {
    PROBE_ABORT.store(true, Ordering::SeqCst);
}

pub fn clear_abort() {
    PROBE_ABORT.store(false, Ordering::SeqCst);
}

pub fn is_aborted() -> bool {
    PROBE_ABORT.load(Ordering::SeqCst)
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

fn tcp_connect_ms(addrs: &[SocketAddr], timeout: Duration) -> Result<(i64, String), String> {
    let mut last_err = String::from("connect failed");
    for addr in addrs {
        if is_aborted() {
            return Err("test aborted".into());
        }
        let t0 = Instant::now();
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(_stream) => {
                let ms = t0.elapsed().as_millis() as i64;
                return Ok((ms, addr.ip().to_string()));
            }
            Err(e) => last_err = format!("{addr}: {e}"),
        }
    }
    Err(last_err)
}

fn one_probe(id: String, host: String, port: u16, timeout: Duration) -> ProbeResult {
    if is_aborted() {
        return ProbeResult {
            id,
            host,
            port,
            ok: false,
            ms: Some(-1),
            ip: None,
            error: Some("test aborted".into()),
        };
    }
    match parse_target(&host, port) {
        Ok(addrs) => match tcp_connect_ms(&addrs, timeout) {
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
/// (Throne progressive latency paint). Returns all results when the batch ends.
pub fn probe_batch_progressive<F>(
    targets: &[serde_json::Value],
    timeout_ms: u64,
    concurrency: usize,
    mut on_each: F,
) -> Vec<ProbeResult>
where
    F: FnMut(&ProbeResult),
{
    clear_abort();
    let timeout = Duration::from_millis(timeout_ms.clamp(200, 30_000));
    let conc = concurrency.clamp(1, 32);
    if targets.is_empty() {
        return Vec::new();
    }

    let (tx, rx) = mpsc::channel::<ProbeResult>();
    let sem = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::with_capacity(targets.len());

    for t in targets {
        if is_aborted() {
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
            if is_aborted() {
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
        if is_aborted() {
            break;
        }
        handles.push(thread::spawn(move || {
            let res = one_probe(id, host, port, timeout);
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
pub fn resolve_batch_progressive<F>(
    targets: &[serde_json::Value],
    concurrency: usize,
    mut on_each: F,
) -> Vec<ResolveResult>
where
    F: FnMut(&ResolveResult),
{
    let conc = concurrency.clamp(1, 32);
    if targets.is_empty() {
        return Vec::new();
    }
    let (tx, rx) = mpsc::channel::<ResolveResult>();
    let sem = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::with_capacity(targets.len());

    for t in targets {
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
            let mut g = sem_c.lock().unwrap();
            if *g < conc {
                *g += 1;
                break;
            }
            drop(g);
            thread::sleep(Duration::from_millis(2));
        }
        handles.push(thread::spawn(move || {
            let res = match resolve_host(&host) {
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
    json!({ "results": results, "aborted": is_aborted() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    #[test]
    fn probe_localhost_or_fail_honest() {
        let res = probe_batch(&[json!({"id":"a","host":"127.0.0.1","port":1})], 500, 2);
        assert_eq!(res.len(), 1);
        assert!(!res[0].ok);
        assert_eq!(res[0].ms, Some(-1));
    }

    #[test]
    fn resolve_batch_fires() {
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
}
