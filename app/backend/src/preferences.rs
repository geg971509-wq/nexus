//! Persist and apply the Tun and system-proxy controls.

use crate::{
    core::session::{CoreSession, SESSION},
    defaults::MIXED_PORT,
    network_restore, tunnel_sm,
};

/// Persist the system-proxy chip. The OS is changed only while a Core profile is
/// stably Connected; an idle preference toggle must never disable or overwrite
/// unrelated proxy/PAC configuration owned by the user or another application.
pub(crate) fn set_system_proxy_cmd_sync(enabled: bool) -> Result<String, String> {
    use crate::data::store::Store;

    let prev = Store::update(|st| {
        let prev = st.system_proxy;
        st.system_proxy = enabled;
        Ok(prev)
    })?;
    let port = MIXED_PORT;
    let tunnel_gen = tunnel_sm::current_gen();
    if tunnel_sm::state() != tunnel_sm::State::Connected {
        return Ok(format!(
            "system_proxy intent={} (OS unchanged until a profile is stably connected)",
            if enabled { "on" } else { "off" }
        ));
    }

    // Short lock: query only — never hold SESSION across networksetup.
    let core_running = {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        g.as_mut()
            .and_then(|s| s.query_state().ok().map(|(r, _)| r))
            .unwrap_or(false)
    };
    if !core_running {
        return Ok(format!(
            "system_proxy intent={} (OS unchanged because Core is not running)",
            if enabled { "on" } else { "off" }
        ));
    }

    let still_same_tunnel = || {
        tunnel_sm::state() == tunnel_sm::State::Connected
            && tunnel_sm::current_gen() == tunnel_gen
    };
    let result = if enabled {
        network_restore::apply_proxy_if(still_same_tunnel, port)
    } else {
        network_restore::restore_proxy_if(still_same_tunnel).map(|result| {
            result.map(|note| {
                note.unwrap_or_else(|| {
                    "system proxy intent=off · no Nexus-owned proxy state".into()
                })
            })
        })
    };

    match result {
        None => Ok(format!(
            "system_proxy intent={} (OS unchanged because tunnel lifecycle changed)",
            if enabled { "on" } else { "off" }
        )),
        Some(Ok(note)) => Ok(note),
        Some(Err(e)) => match Store::update(|st| {
            st.system_proxy = prev;
            Ok(())
        }) {
            Ok(()) => Err(e),
            Err(store_err) => Err(format!(
                "{e}; system_proxy preference rollback failed: {store_err}"
            )),
        },
    }
}

/// Persist Tun chip + elevate Core (osascript password sheet).
/// Live tunnel re-Start is UI-side (needs node payload); here only privilege + flag.
pub(crate) fn set_tun_cmd_sync(enabled: bool) -> Result<serde_json::Value, String> {
    use crate::data::store::Store;
    let prev = Store::update(|st| {
        let prev = st.tun;
        st.tun = enabled;
        Ok(prev)
    })?;
    if !enabled {
        return Ok(serde_json::json!({
            "tun": false,
            "elevated": false,
            "note": "tun=off (applied on next generate/start)",
        }));
    }
    // Tun needs root Core. Bundle may be on nosuid → Application Support setuid copy.
    match CoreSession::ensure_privileged_core() {
        Ok(path) => {
            // If Core already running unprivileged, recycle so next Start is root.
            let mut recycled = false;
            if let Ok(mut g) = SESSION.lock() {
                if let Some(s) = g.as_mut() {
                    let priv_now = s.is_privileged().unwrap_or(false);
                    if !priv_now {
                        match s.recycle_privileged(&path) {
                            Ok(()) => recycled = true,
                            Err(e) => {
                                let _ = Store::update(|st| {
                                    st.tun = prev;
                                    Ok(())
                                });
                                return Err(format!("Tun elevate recycle failed: {e}"));
                            }
                        }
                    }
                }
            }
            Ok(serde_json::json!({
                "tun": true,
                "elevated": true,
                "recycled": recycled,
                "core": path.display().to_string(),
                "note": if recycled {
                    "tun=on · Core elevated (re-Start to apply Tun inbound)"
                } else {
                    "tun=on · Core setuid ready (re-Start to apply Tun inbound)"
                },
            }))
        }
        Err(e) => {
            let _ = Store::update(|st| {
                st.tun = prev;
                Ok(())
            });
            Err(format!("Tun needs admin: {e}"))
        }
    }
}
