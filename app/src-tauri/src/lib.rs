// Phase A: shell only — MVP commands land in later phases.
#[tauri::command]
fn app_identity() -> serde_json::Value {
    serde_json::json!({
        "name": "Nexus",
        "identifier": "app.nexus.desktop",
        "phase": "A-skeleton",
        "warp": "official-cloudflare-app",
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![app_identity])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
