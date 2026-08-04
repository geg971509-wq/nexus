# Nexus

macOS VPN / proxy client (MVP skeleton). UI: Apple-minimal HTML. Engine: Go core (ThroneCore lineage) + sing-box/xray git deps. Shell: **Tauri 2 + Rust**.

## Identity

| Item | Value |
|------|--------|
| Product | Nexus |
| Bundle ID | `app.nexus.desktop` |
| Deeplink | `nexus://` |
| Data dir | `~/Library/Application Support/Nexus` |
| Socket prefix | `nexus-` |
| Core binary | `bin/NexusCore` (libcore framed IPC) |

**Do not edit** `upstream source tree`. This tree is a copy + product worktree.

## WARP

Official Cloudflare **`warp-cli`** is staged under `third_party/cloudflare-warp/` and embedded into `Nexus.app/Contents/MacOS/warp-cli` at build time. Nexus calls connect/disconnect via that binary (not a vendored Throne `warp-client`, not a hard dependency on the full GUI.app path).

Tunnel still needs the system Cloudflare WARP daemon when present. Optional GUI: `/Applications/Cloudflare WARP.app` via `warp_open`.

## Layout

- `app/` — Tauri 2 (`ui/` = HTML + icons)
- `app/src-tauri/src/core/` — framed IPC client + session spawn
- `app/src-tauri/src/data/` — JSON store + pure generate
- `app/src-tauri/src/sys.rs` / `warp.rs` — system proxy / bundled warp-cli
- `core/server/` — Go core source
- `bin/NexusCore` · `bin/Nexus.app` — build outputs
- `docs/nexus-throne-port-plan.md` — FINAL port plan

## Build (final .app)

```bash
cd .
./build.sh          # release → bin/Nexus.app + bin/NexusCore
./build.sh --debug  # debug profile
./build.sh --open   # open when done
./build.sh --skip-core --skip-npm  # faster rebuild when core/deps ready
```

Requires: macOS, Xcode CLT, `go`, `cargo`/`rustc`, `npm`.  
Does **not** build Qt/Throne CMake.

## Dev

```bash
export PATH="$HOME/.cargo/bin:$PATH"
./build.sh --skip-core   # if bin/NexusCore already exists
cd app/src-tauri
export NEXUS_CORE_BIN=./bin/NexusCore
cargo run --bin core_smoke
cd .. && npm run tauri dev
```

## Status (2026-08-04)

- Phase A–E skeleton: IPC smoke PASS; store/generate; sys/WARP stubs; UI bridge
- `connect` runs generate → CheckConfig only; **Start() deferred** until import path trusted
- No push to remote unless asked
