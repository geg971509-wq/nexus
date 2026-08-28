//! Build: validate release metadata; compile libcore.proto.

use std::fs;
use std::path::PathBuf;

fn main() {
    verify_version_matches_tauri();
    compile_libcore_proto();
    tauri_build::try_build(tauri_build::Attributes::new()).expect("failed to run tauri-build");
}

/// 5A: generate Rust prost types from the same core/server/gen/libcore.proto as Go.
fn compile_libcore_proto() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let proto = manifest_dir.join("../../core/server/gen/libcore.proto");
    let proto = proto.canonicalize().unwrap_or(proto);
    println!("cargo:rerun-if-changed={}", proto.display());
    // Include only messages used by the shell IPC path (field numbers identical to full file).
    let mut config = prost_build::Config::new();
    config.btree_map(["."]);
    config
        .compile_protos(&[proto.as_path()], &[proto.parent().unwrap()])
        .expect("compile libcore.proto for Rust");
}

fn verify_version_matches_tauri() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo_toml = fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("read Cargo.toml");
    let ver = parse_package_version(&cargo_toml).expect("Cargo.toml [package].version");

    let tauri_path = manifest_dir.join("tauri.conf.json");
    let tauri_conf = fs::read_to_string(&tauri_path).expect("read tauri.conf.json");
    let tauri_json: serde_json::Value =
        serde_json::from_str(&tauri_conf).expect("parse tauri.conf.json");
    let tauri_ver = tauri_json
        .get("version")
        .and_then(serde_json::Value::as_str)
        .expect("tauri.conf.json version");
    assert_eq!(
        tauri_ver, ver,
        "tauri.conf.json version must match Cargo.toml [package].version"
    );

    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=tauri.conf.json");
}

fn parse_package_version(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package && t.starts_with("version") {
            // version = "0.2.3"
            let rest = t.split_once('=')?.1.trim();
            let v = rest.trim_matches(|c| c == '"' || c == '\'').to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version() {
        let t = "[package]\nname = \"x\"\nversion = \"0.2.3\"\n";
        assert_eq!(parse_package_version(t).as_deref(), Some("0.2.3"));
    }
}
