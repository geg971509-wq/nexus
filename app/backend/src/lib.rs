#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
compile_error!("Nexus backend supports Apple Silicon macOS only");

mod catalog;
pub mod core;
mod data;
mod defaults;
mod diagnostics;
mod exit_ip;
pub mod firewall;
mod net;
mod paths;
mod preferences;
pub mod qt_api;
mod quit;
mod runtime;
mod session_access;
mod sub;
mod subscription_commands;
mod sys;
mod tray_spin;
mod tun_if;
mod tunnel_runtime;
pub mod tunnel_sm;

pub(crate) use catalog::{
    app_identity, catalog_get, catalog_put, generate_preview, persist_hide_tray, qr_svg,
    store_snapshot,
};
pub(crate) use diagnostics::{
    core_url_test_current, core_url_test_stop, exit_ip_probe, firewall_helper_install,
    firewall_helper_uninstall, firewall_status, net_tcp_probe_stop, query_connections, query_stats,
    require_tunnel_idle,
};
pub(crate) use preferences::{set_system_proxy_cmd_sync, set_tun_cmd_sync};
pub(crate) use quit::{prepare_quit, teardown_session, tunnel_is_live};
pub(crate) use subscription_commands::{sub_fetch_sync, sub_parse_clash, sub_parse_share};
pub(crate) use tunnel_runtime::{
    connect_selected_sync, disconnect_selected_sync, session_status,
};
