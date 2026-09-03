use super::{
    connect_finalize::finish_connect,
    connect_start::{start_core, StartOutcome},
};
use crate::{
    core::session::CoreSession,
    defaults::MIXED_PORT,
    firewall,
    session_access::{action_is_current, commit_if_action_current},
    sys, tun_if, tunnel_sm,
};
use std::path::PathBuf;

pub(super) struct PreparedConnect {
    pub(super) action_gen: u64,
    pub(super) use_tun: bool,
    pub(super) use_sys_proxy: bool,
    pub(super) dns_bootstrap: Vec<String>,
    pub(super) port: u16,
    pub(super) profile_id: i32,
    pub(super) cfg: serde_json::Value,
    pub(super) json: String,
    pub(super) planned_tun_if: Option<String>,
    pub(super) privileged_core: Option<PathBuf>,
    pub(super) params: tunnel_sm::ConnectParams,
    pub(super) connect_gen: u64,
    pub(super) utun_before: Vec<String>,
}

/// engine-aligned connect: BuildSingBoxConfig → Start(LoadConfigReq).
/// UI passes selected node share `link` or raw `outbound` JSON; no invent credentials.
/// Tun/system-proxy follow upstream `spmode_vpn` / `spmode_system_proxy` (UI chips → optional args → store).
pub(crate) fn connect_selected_sync(
    action_gen: u64,
    link: Option<String>,
    outbound: Option<serde_json::Value>,
    profile_id: Option<i32>,
    tun: Option<bool>,
    system_proxy: Option<bool>,
) -> Result<serde_json::Value, String> {
    use crate::data::generate::generate_with_outbound;
    use crate::data::share_link::parse_to_outbound;
    use crate::data::store::Store;

    // A crash-recovery journal is a hard gate before a new session. `nexus_init`
    // already attempts this synchronously, but recovery can fail transiently
    // (for example while a network service is changing). Retry here and refuse
    // the connect if the old OS state is still unresolved. When this process
    // already owns the current journal, the check is an inexpensive no-op.
    if !action_is_current(action_gen) {
        return Err("connect superseded".into());
    }
    sys::recover_stale_network_state()
        .map_err(|e| format!("cannot connect before system network recovery: {e}"))?;
    if !action_is_current(action_gen) {
        return Err("connect superseded".into());
    }

    // Start uses current checkbox state, not a stale disk flag.
    // Prefer explicit UI args; persist so next cold Start matches chips.
    // One read: config and PF must agree on the resolver list or PF blocks the
    // server the config just chose.
    let (use_tun, use_sys_proxy, dns_bootstrap) = commit_if_action_current(action_gen, || {
        Store::update(|st| {
            if let Some(v) = tun {
                st.tun = v;
            }
            if let Some(v) = system_proxy {
                st.system_proxy = v;
            }
            Ok((st.tun, st.system_proxy, st.dns_bootstrap()))
        })
    })
    .ok_or_else(|| "connect superseded".to_string())??;
    let port = MIXED_PORT;
    let pid = profile_id.unwrap_or(1);

    let ob = if let Some(v) = outbound {
        if v.get("type").and_then(|t| t.as_str()).is_none() {
            return Err("outbound missing type".into());
        }
        v
    } else if let Some(lk) = link.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        parse_to_outbound(lk)?
    } else {
        return Err(
            "selected node has no share link — import a subscription or paste a vless/trojan/ss link"
                .into(),
        );
    };

    // generate.cpp: tun inbound only if spmode_vpn
    let mut cfg = generate_with_outbound(ob.clone(), port, use_tun, &dns_bootstrap);

    // mac Tun: pin next free utunN into Core config + PF Connected.
    // Detection-only (172.19 / new-utun / stale core.log) left pure-Tun dead under
    // fail-closed: no reliable `pass quick on utun…` while sysproxy still worked.
    #[cfg(target_os = "macos")]
    let planned_tun_if: Option<String> = if use_tun {
        let name = tun_if::next_free_utun().ok_or_else(|| "no free utun for Tun".to_string())?;
        if let Some(arr) = cfg.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
            for ib in arr.iter_mut() {
                if ib.get("type").and_then(|t| t.as_str()) == Some("tun") {
                    if let Some(obj) = ib.as_object_mut() {
                        obj.insert(
                            "interface_name".into(),
                            serde_json::Value::String(name.clone()),
                        );
                    }
                }
            }
        }
        Some(name)
    } else {
        None
    };
    let json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;

    // Tun: setuid Core before LoadConfig (upstream profile_start elevation).
    // osascript password sheet runs here if setuid copy missing — outside SESSION.
    // The path is kept so the recycle below cannot re-enter elevation under the lock.
    if !action_is_current(action_gen) {
        return Err("connect superseded".into());
    }
    let privileged_core = if use_tun {
        Some(CoreSession::ensure_privileged_core()?)
    } else {
        None
    };
    if !action_is_current(action_gen) {
        return Err("connect superseded".into());
    }

    // Firewall: helper ready → peer → Connecting before Core Start (C2 L3/L5).
    firewall::require_ready_for_connect()?;
    if !action_is_current(action_gen) {
        return Err("connect superseded".into());
    }
    // Residual Blocked (esp. peer-less) can leave getaddrinfo dead if DNS was closed.
    // Soft-open once so hostname peers (VMess CDN etc.) can resolve, then Connecting.
    let peer = match firewall::peer_from_outbound(&ob) {
        Ok(p) => p,
        Err(e) => {
            if commit_if_action_current(action_gen, || firewall::apply(firewall::Policy::Reset))
                .is_none()
            {
                return Err("connect superseded".into());
            }
            firewall::peer_from_outbound(&ob).map_err(|e2| format!("{e}; after reset: {e2}"))?
        }
    };
    let connect_params = tunnel_sm::ConnectParams {
        peer: peer.clone(),
        tun: use_tun,
        mixed_port: port,
        // Planned ifname is known before Start; Connected uses it so pass-on-utun
        // is not gated on post-Start detection races.
        tun_if: planned_tun_if.clone(),
        dns: dns_bootstrap.clone(),
    };
    let connect_gen = commit_if_action_current(action_gen, || {
        let tr = tunnel_sm::apply(tunnel_sm::Event::BeginConnect(connect_params.clone()));
        if let Err(e) = firewall::apply(firewall::policy_from_sm(tr.to, Some(&connect_params))) {
            // Connecting never applied — safe to Reset (network open).
            let _ = tunnel_sm::apply(tunnel_sm::Event::Fail(e.clone()));
            let _ = firewall::apply(firewall::Policy::Reset);
            return Err(format!("firewall connecting: {e}"));
        }
        Ok(tr.gen)
    })
    .ok_or_else(|| "connect superseded".to_string())??;
    // Snapshot utun names before Start — gvisor often never assigns 172.19.0.1
    // on the kernel iface; we detect by new utun + core.log "started at utunN".
    #[cfg(target_os = "macos")]
    let utun_before = if use_tun {
        tun_if::list_utun_names()
    } else {
        Vec::new()
    };
    let prepared = PreparedConnect {
        action_gen,
        use_tun,
        use_sys_proxy,
        dns_bootstrap,
        port,
        profile_id: pid,
        cfg,
        json,
        planned_tun_if,
        privileged_core,
        params: connect_params,
        connect_gen,
        utun_before,
    };
    match start_core(action_gen, &prepared)? {
        StartOutcome::Started {
            running,
            profile_id,
        } => finish_connect(prepared, running, profile_id),
        StartOutcome::Failed { error } => Ok(serde_json::json!({
            "started": false,
            "start_error": error,
            "config": prepared.cfg,
            "profile_id": prepared.profile_id,
            "tun": prepared.use_tun,
            "system_proxy": prepared.use_sys_proxy,
            "tunnel_state": tunnel_sm::state().as_str(),
        })),
        StartOutcome::Superseded { profile_id } => Ok(serde_json::json!({
            "started": false,
            "start_error": "connect superseded",
            "running": false,
            "profile_id": profile_id,
            "listen_port": prepared.port,
            "proxy_note": "session discarded: connect superseded",
            "tun": prepared.use_tun,
            "system_proxy": prepared.use_sys_proxy,
            "config": prepared.cfg,
        })),
    }
}
