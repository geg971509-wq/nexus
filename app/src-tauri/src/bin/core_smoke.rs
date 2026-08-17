//! Phase B smoke: listen → spawn NexusCore → QueryState → CheckConfig(minimal) → stop
use nexus_lib::core_smoke_run;

fn main() {
    if let Err(e) = core_smoke_run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
    println!("PASS: core ipc smoke");
}
