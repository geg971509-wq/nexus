//! Product defaults — one place to change, used by IPC / store / UI identity.
//! Bump app version only in Cargo.toml (`version`); this re-exports it.

/// Mixed inbound + system-proxy port (sing-box generate + OS proxy).
pub const MIXED_PORT: u16 = 2080;

/// Same as package version in `app/src-tauri/Cargo.toml`.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Desktop identifier (store / bundle id surface).
pub const APP_IDENTIFIER: &str = "app.nexus.desktop";

pub const APP_NAME: &str = "Nexus";
