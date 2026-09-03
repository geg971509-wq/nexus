//! Firewall status, connection statistics, and network diagnostic commands.

use crate::{
    core::{frame::LibcoreControl, proto_min::ConnRow, session::SESSION},
    defaults::MIXED_PORT,
    exit_ip, firewall, net, runtime,
    session_access::{current_connect_gen, reinstall_poll_session},
    tunnel_sm,
};
use std::sync::Mutex;

static URL_TEST_CONTROL: Mutex<Option<LibcoreControl>> = Mutex::new(None);

fn connection_rows_json(rows: Vec<ConnRow>) -> Vec<serde_json::Value> {
    rows.into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "created_at": r.created_at,
                "process": r.process,
                "process_path": r.process_path,
                "process_id": r.process_id,
                "dest": r.dest,
                "domain": r.domain,
                "network": r.network,
                "protocol": r.protocol,
                "outbound": r.outbound,
                "upload": r.upload,
                "download": r.download,
            })
        })
        .collect()
}

pub(crate) fn firewall_status_json() -> serde_json::Value {
    let st = firewall::status();
    // 6A: desired (SM) vs applied (last successful apply) + mismatch.
    let desired = firewall::desired_policy_name();
    let applied = st.last_policy.clone();
    let mismatch = !desired.is_empty() && !applied.is_empty() && desired != applied;
    serde_json::json!({
        "support": "active",
        "last_policy": st.last_policy,
        "desired_policy": desired,
        "applied_policy": applied,
        "policy_mismatch": mismatch,
        "last_error": st.last_error,
        "peer": st.peer,
        "tun_if": st.tun_if,
        "tunnel_state": tunnel_sm::state().as_str(),
        "helper_installed": st.helper_installed,
        "helper_running": st.helper_running,
        "helper_detail": st.helper_detail,
    })
}

pub(crate) async fn firewall_status() -> Result<serde_json::Value, String> {
    Ok(firewall_status_json())
}

pub(crate) async fn firewall_helper_install() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        firewall::install_helper()?;
        Ok(firewall_status_json())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

pub(crate) async fn firewall_helper_uninstall() -> Result<serde_json::Value, String> {
    // Uninstall boots the daemon out and flushes the PF anchor, so doing it mid
    // tunnel silently removes the kill switch while traffic keeps flowing: if Core
    // then dies there is nothing left to fail closed. Refuse rather than degrade —
    // disconnecting first is one click and leaves the user in a defined state.
    require_tunnel_idle("Uninstalling the firewall helper")?;
    runtime::spawn_blocking(|| {
        firewall::uninstall_helper()?;
        Ok(firewall_status_json())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Live connections from Core TrafficManager (needs experimental.clash_api).
/// 6A: take/put + gen so poll does not hold SESSION across Core RPC.
pub(crate) async fn query_connections() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use crate::core::session::SESSION;
        let (mut session, gen) = {
            let mut g = SESSION.lock().map_err(|e| e.to_string())?;
            let gen = current_connect_gen();
            let s = g.take().ok_or_else(|| "core not started".to_string())?;
            (s, gen)
        };
        let rows = match session.query_connections() {
            Ok(r) => r,
            Err(e) => {
                reinstall_poll_session(session, gen);
                return Err(e);
            }
        };
        reinstall_poll_session(session, gen);
        let list = connection_rows_json(rows);
        Ok(serde_json::json!({ "active": list, "count": list.len() }))
    })
    .await
    .map_err(|e| format!("query_connections join: {e}"))?
}

pub(crate) async fn query_runtime_metrics() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        let (mut session, gen) = {
            let mut g = SESSION.lock().map_err(|e| e.to_string())?;
            let gen = current_connect_gen();
            let s = g.take().ok_or_else(|| "core not started".to_string())?;
            (s, gen)
        };
        let connections = session.query_connections();
        let stats = session.query_stats_proxy();
        reinstall_poll_session(session, gen);

        let (active, connections_error) = match connections {
            Ok(rows) => (Some(connection_rows_json(rows)), None),
            Err(e) => (None, Some(e)),
        };
        let (upload, download, stats_error) = match stats {
            Ok((upload, download)) => (Some(upload), Some(download), None),
            Err(e) => (None, None, Some(e)),
        };
        Ok(serde_json::json!({
            "connections_ok": active.is_some(),
            "active": active,
            "connections_error": connections_error,
            "stats_ok": upload.is_some(),
            "upload": upload,
            "download": download,
            "stats_error": stats_error,
        }))
    })
    .await
    .map_err(|e| format!("query_runtime_metrics join: {e}"))?
}

/// Cumulative proxy outbound traffic (Core QueryStats / TrafficManager).
/// 6A: take/put + gen so poll does not hold SESSION across Core RPC.
pub(crate) async fn query_stats() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use crate::core::session::SESSION;
        let (mut session, gen) = {
            let mut g = SESSION.lock().map_err(|e| e.to_string())?;
            let gen = current_connect_gen();
            let s = g.take().ok_or_else(|| "core not started".to_string())?;
            (s, gen)
        };
        let stats = match session.query_stats_proxy() {
            Ok(r) => r,
            Err(e) => {
                reinstall_poll_session(session, gen);
                return Err(e);
            }
        };
        reinstall_poll_session(session, gen);
        let (upload, download) = stats;
        Ok(serde_json::json!({
            "upload": upload,
            "download": download,
        }))
    })
    .await
    .map_err(|e| format!("query_stats join: {e}"))?
}
/// Exit IP + country as seen from the far end, fetched through the mixed inbound.
/// Errors when the tunnel cannot carry it — the UI then shows nothing rather than
/// this machine's own address.
pub(crate) async fn exit_ip_probe() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| exit_ip::probe(MIXED_PORT))
        .await
        .map_err(|e| format!("exit_ip join: {e}"))?
}
/// Err unless the tunnel is fully down, for actions that are only safe then.
///
/// Direct NIC probes must not run beside a live tunnel. Enforced here, not in QML.
pub(crate) fn require_tunnel_idle(action: &str) -> Result<(), String> {
    let st = tunnel_sm::state();
    if st == tunnel_sm::State::Idle {
        return Ok(());
    }
    Err(format!(
        "{action} requires the tunnel to be fully disconnected (currently {})",
        st.as_str()
    ))
}

/// Abort in-flight TCP probes (upstream stopSpeedtest).
pub(crate) fn net_tcp_probe_stop() -> Result<(), String> {
    net::abort_probes();
    Ok(())
}

/// Core TestCurrent: URL test via live box proxy/default outbound only.
/// take/reinstall session so poll/disconnect is not blocked for the whole Test.
pub(crate) async fn core_url_test_current(
    url: Option<String>,
    timeout_ms: Option<i32>,
) -> Result<serde_json::Value, String> {
    let url = url.unwrap_or_default();
    let timeout_ms = timeout_ms.unwrap_or(3000);
    runtime::spawn_blocking(move || {
        let (taken, gen) = match SESSION.lock() {
            Ok(mut g) => {
                let gen = current_connect_gen();
                (g.take(), gen)
            }
            Err(_) => (None, 0),
        };
        let Some(mut s) = taken else {
            return Err("no core session".into());
        };
        let result = s.test_current_url(&url, timeout_ms, |control| {
            let mut slot = URL_TEST_CONTROL.lock().unwrap_or_else(|e| e.into_inner());
            *slot = Some(control);
        });
        URL_TEST_CONTROL
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        reinstall_poll_session(s, gen);
        let rows = result?;
        let results: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "tag": r.tag,
                    "ms": r.ms,
                    "error": r.error,
                })
            })
            .collect();
        Ok(serde_json::json!({ "results": results }))
    })
    .await
    .map_err(|e| format!("url test join: {e}"))?
}

/// Cancel in-flight Core URL test (StopTest).
pub(crate) async fn core_url_test_stop() -> Result<(), String> {
    runtime::spawn_blocking(|| {
        let control = URL_TEST_CONTROL
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(mut control) = control {
            control
                .send("StopTest", &[])
                .map_err(|e| format!("stop url test: {e}"))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("url test stop join: {e}"))?
}
#[cfg(test)]
mod idle_guard_tests {
    use super::*;

    /// The direct TCP probe binds the physical NIC and the uninstall flushes PF,
    /// so both are only safe with the tunnel fully down.
    #[test]
    fn only_idle_passes() {
        let _g = tunnel_sm::test_lock();
        let _ = tunnel_sm::apply(tunnel_sm::Event::ResetIdle);
        assert!(require_tunnel_idle("probe").is_ok());

        for state in [
            tunnel_sm::State::Connecting,
            tunnel_sm::State::Connected,
            tunnel_sm::State::Disconnecting,
            tunnel_sm::State::Error,
        ] {
            tunnel_sm::set_state(state);
            let err = require_tunnel_idle("probe").unwrap_err();
            // The caller has to be able to tell the user which state blocked it.
            assert!(err.contains(state.as_str()), "{state:?}: {err}");
        }

        let _ = tunnel_sm::apply(tunnel_sm::Event::ResetIdle);
        assert!(require_tunnel_idle("probe").is_ok(), "recovers after reset");
    }
}
