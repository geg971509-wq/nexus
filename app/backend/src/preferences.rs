//! Persist and apply the Tun and system-proxy controls.

use crate::{
    core::session::{CoreSession, SESSION},
    data,
    defaults::MIXED_PORT,
    sys,
};

/// Persist chip intent; OS apply only when Core is running (or always on disable).
pub(crate) fn set_system_proxy_cmd_sync(enabled: bool) -> Result<String, String> {
    use crate::core::session::SESSION;
    use crate::data::store::Store;
    // set_spmode_system_proxy: always persist intent; OS write only if profile running.
    Store::update(|st| {
        st.system_proxy = enabled;
        Ok(())
    })?;
    let port = MIXED_PORT;
    // Short lock: query only — never hold across networksetup.
    let core_running = {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        g.as_mut()
            .and_then(|s| s.query_state().ok().map(|(r, _)| r))
            .unwrap_or(false)
    };
    if enabled && !core_running {
        return Ok(format!(
            "system_proxy intent=on (OS apply on Start · mixed 127.0.0.1:{port})"
        ));
    }
    // enable+running → point OS at mixed; disable → clear OS always (upstream ClearSystemProxy)
    // primary service sync (~0.2s); other NICs background — chip must not wait ~1s for all.
    sys::set_system_proxy(enabled, port)
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
