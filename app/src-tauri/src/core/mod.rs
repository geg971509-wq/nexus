pub mod elevate;
pub mod frame;
/// 5A: prost-generated wrappers from core/server/gen/libcore.proto.
pub mod proto_gen;
/// Back-compat alias — same API surface as historical hand codec.
pub mod proto_min {
    pub use super::proto_gen::*;
}
pub mod session;
#[cfg(windows)]
pub mod winpipe;
