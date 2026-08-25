//! Build: sync package version from Cargo.toml; compile libcore.proto.
//! 7A: one-place version — Cargo.toml [package].version → tauri.conf.

use std::fs;
use std::path::PathBuf;

fn main() {
    sync_version_from_cargo();
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

fn sync_version_from_cargo() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo_toml = fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("read Cargo.toml");
    let ver = parse_package_version(&cargo_toml).expect("Cargo.toml [package].version");

    // tauri.conf.json
    let tauri_path = manifest_dir.join("tauri.conf.json");
    if let Ok(s) = fs::read_to_string(&tauri_path) {
        if let Some(next) = replace_json_version_field(&s, &ver) {
            if next != s {
                let _ = fs::write(&tauri_path, next);
            }
        }
    }

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

fn replace_json_version_field(src: &str, ver: &str) -> Option<String> {
    // Replace first "version": "…" at top-level-ish (product files only have one).
    let key = "\"version\"";
    let idx = src.find(key)?;
    let after = &src[idx + key.len()..];
    let colon = after.find(':')?;
    let rest = &after[colon + 1..];
    let q1 = rest.find('"')?;
    let after_q1 = &rest[q1 + 1..];
    let q2 = after_q1.find('"')?;
    let start = idx + key.len() + colon + 1 + q1 + 1;
    let end = start + q2;
    let mut out = String::with_capacity(src.len() + ver.len());
    out.push_str(&src[..start]);
    out.push_str(ver);
    out.push_str(&src[end..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version() {
        let t = "[package]\nname = \"x\"\nversion = \"0.2.3\"\n";
        assert_eq!(parse_package_version(t).as_deref(), Some("0.2.3"));
    }

    #[test]
    fn json_replace() {
        let s = "{\n  \"version\": \"0.1.0\",\n  \"name\": \"n\"\n}\n";
        let n = replace_json_version_field(s, "0.2.3").unwrap();
        assert!(n.contains("\"version\": \"0.2.3\""));
        assert!(n.contains("\"name\": \"n\""));
    }
}
