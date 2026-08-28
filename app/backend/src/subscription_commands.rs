//! Subscription download and parsing commands exposed to the Qt bridge.

use crate::{data, runtime, sub};

/// GroupUpdater::HttpGet — download subscription body (no parse).
pub(crate) fn sub_fetch_sync(url: String) -> Result<serde_json::Value, String> {
    sub::fetch(&url)
}

/// Throne RawUpdater::updateClash — YAML proxies → catalog nodes with outbound JSON.
pub(crate) async fn sub_parse_clash(body: String) -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(move || {
        let (nodes, skipped) = data::clash::parse_clash_yaml(&body)?;
        let arr: Vec<serde_json::Value> = nodes
            .into_iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "type": n.type_label,
                    "addr": n.addr,
                    "lat": null,
                    "flow": null,
                    "outbound": n.outbound,
                })
            })
            .collect();
        Ok(serde_json::json!({ "ok": true, "nodes": arr, "count": arr.len(), "skipped": skipped }))
    })
    .await
    .map_err(|e| format!("sub_parse_clash join: {e}"))?
}

/// Free-list / share URI body → catalog nodes with full outbound (vless/vmess/trojan/ss/…).
pub(crate) async fn sub_parse_share(body: String) -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(move || {
        let (nodes, skipped) = data::share_link::parse_share_body(&body);
        let arr: Vec<serde_json::Value> = nodes
            .into_iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "type": n.type_label,
                    "addr": n.addr,
                    "lat": null,
                    "flow": null,
                    "link": n.link,
                    "outbound": n.outbound,
                })
            })
            .collect();
        Ok(serde_json::json!({ "ok": true, "nodes": arr, "count": arr.len(), "skipped": skipped }))
    })
    .await
    .map_err(|e| format!("sub_parse_share join: {e}"))?
}
