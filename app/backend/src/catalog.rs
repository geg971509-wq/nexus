//! Application identity, persisted catalog, config preview, and share QR commands.

use crate::{
    data,
    defaults::{APP_IDENTIFIER, APP_NAME, APP_VERSION, MIXED_PORT},
    runtime,
};
use std::sync::OnceLock;

static STARTUP_NETWORK_RECOVERY: OnceLock<Result<(), String>> = OnceLock::new();

pub(crate) fn app_identity() -> serde_json::Value {
    // app_identity is part of the first UI bootstrap. Recover an abnormal prior
    // run before the user can start a new tunnel. Failure remains retryable at
    // the first network write, where ensure_snapshot refuses to overwrite the
    // stale journal unless restoration succeeds.
    let _ = STARTUP_NETWORK_RECOVERY.get_or_init(|| {
        crate::sys::recover_stale_network_state()
            .map(|_| ())
            .map_err(|e| format!("startup network recovery: {e}"))
    });
    serde_json::json!({
        "name": APP_NAME,
        "identifier": APP_IDENTIFIER,
        "version": APP_VERSION,
        "mixed_port": MIXED_PORT,
    })
}

/// Share-link → SVG QR (offline; for the share-QR dialog).
pub(crate) fn qr_svg(text: String) -> Result<serde_json::Value, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("empty qr payload".into());
    }
    if t.len() > 2000 {
        return Err("payload too long for QR".into());
    }
    use qrcode::render::svg;
    use qrcode::QrCode;
    let code = QrCode::new(t.as_bytes()).map_err(|e| format!("qr encode: {e}"))?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(200, 200)
        .dark_color(svg::Color("#111111"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(serde_json::json!({ "svg": svg, "len": t.len() }))
}

pub(crate) async fn store_snapshot() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use crate::data::store::Store;
        let st = Store::load();
        serde_json::to_value(&st).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("store_snapshot join: {e}"))?
}

pub(crate) fn persist_hide_tray(hide: bool) -> Result<String, String> {
    use crate::data::store::Store;
    Store::update(|st| {
        st.hide_tray = hide;
        Ok(())
    })?;
    Ok(if hide {
        "tray hidden".into()
    } else {
        "tray shown".into()
    })
}

pub(crate) async fn generate_preview(
    link: Option<String>,
    outbound: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(move || {
        use crate::data::generate::generate_with_outbound;
        use crate::data::share_link::parse_to_outbound;
        use crate::data::store::Store;
        let st = Store::load();
        let ob = if let Some(v) = outbound {
            if v.get("type").and_then(|t| t.as_str()).is_none() {
                return Err("outbound missing type".into());
            }
            v
        } else if let Some(lk) = link.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            parse_to_outbound(lk)?
        } else {
            return Err("no selected node link/outbound for preview".into());
        };
        Ok(generate_with_outbound(
            ob,
            MIXED_PORT,
            st.tun,
            &st.dns_bootstrap(),
        ))
    })
    .await
    .map_err(|e| format!("generate_preview join: {e}"))?
}

pub(crate) async fn catalog_get() -> Result<serde_json::Value, String> {
    runtime::spawn_blocking(|| {
        use crate::data::store::Store;
        let st = Store::load();
        if let Some(catalog) = st.catalog {
            return Ok(catalog);
        }
        Store::update(|locked| {
            if locked.catalog.is_none() {
                locked.catalog = Some(data::store::default_catalog());
            }
            locked
                .catalog
                .clone()
                .ok_or_else(|| "catalog initialization failed".to_string())
        })
    })
    .await
    .map_err(|e| format!("catalog_get join: {e}"))?
}

pub(crate) async fn catalog_put(blob: serde_json::Value) -> Result<String, String> {
    runtime::spawn_blocking(move || {
        use crate::data::store::Store;
        if !blob.is_object() {
            return Err("catalog blob must be object".into());
        }
        Store::update(|st| {
            st.catalog = Some(blob);
            Ok("ok".into())
        })
    })
    .await
    .map_err(|e| format!("catalog_put join: {e}"))?
}

#[cfg(test)]
mod qr_tests {
    use super::*;

    #[test]
    fn qr_svg_vless_sample() {
        let v = qr_svg(
            "vless://11111111-1111-1111-1111-111111111111@1.1.1.1:443?encryption=none&type=ws#n"
                .into(),
        )
        .unwrap();
        let s = v["svg"].as_str().unwrap();
        assert!(s.contains("<svg"), "{s}");
        assert!(s.len() > 200);
    }
}
