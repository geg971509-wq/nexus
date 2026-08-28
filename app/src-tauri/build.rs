//! Build: compile libcore.proto.

use std::path::PathBuf;

fn main() {
    compile_libcore_proto();
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
