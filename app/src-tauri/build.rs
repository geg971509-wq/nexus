fn main() {
    // Windows: replace default asInvoker-style manifest with requireAdministrator
    // (Tun/wintun). Keep Common Controls v6 dependency for Tauri dialogs.
    let mut windows = tauri_build::WindowsAttributes::new();
    windows = windows.app_manifest(include_str!("windows/app.manifest"));
    let attrs = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
