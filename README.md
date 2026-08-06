# Nexus

macOS VPN / proxy client. UI: Apple-minimal HTML. Engine: Go core (sing-box + xray) + framed IPC. Shell: **Tauri 2 + Rust**.

## Identity

| Item | Value |
|------|--------|
| Product | Nexus |
| Version | 0.2.0 |
| Bundle ID | `app.nexus.desktop` |
| Deeplink | `nexus://` |
| Data dir | `~/Library/Application Support/Nexus` |
| Socket env | `NEXUS_CORE_SOCKET` / `NEXUS_CORE_DEBUG` |
| Core binary | `NexusCore` (libcore framed IPC) |

## Layout

- `app/` — Tauri 2 (`ui/index.html` is the UI)
- `app/src-tauri/src/core/` — framed IPC client + session spawn
- `app/src-tauri/src/data/` — JSON store + pure generate
- `app/src-tauri/src/sys.rs` — system proxy
- `core/server/` — Go core source (`module NexusCore`)
- `bin/NexusCore` · `bin/Nexus.app` — build outputs (gitignored)
- `docs/core-dependencies.md` — Core dependency pins

## Build (local .app)

```bash
cd .
./build.sh   # always full rebuild → bin/Nexus.app + bin/NexusCore
```

No flags. Always rebuilds NexusCore, runs `npm install`, and packages a release `.app`.

Requires: macOS, Xcode CLT, `go`, `cargo`/`rustc`, `npm`.

### Distribution note (unsigned)

This build path produces an **unsigned** local app (no Developer ID / notarization). Fine for your machine and internal use. Gatekeeper will block copies to other Macs until you codesign + notarize with your Apple credentials (not wired in `build.sh` yet).

## Dev

```bash
export PATH="$HOME/.cargo/bin:$PATH"
./build.sh   # full release build when you need a fresh Core + .app
cd app && npm run tauri dev
```

## Capabilities (0.2.0)

- Connect: UI selected node → generate → Core `Start` (share link or outbound JSON)
- Tun chip + system proxy; exit always clears OS proxy on `:2080` and stops Core
- Catalog (groups/nodes) in `store.json` via `catalog_get` / `catalog_put`
- Node traffic column from `QueryConnections` aggregate while connected
- Honest UI: tunnel ≠ selected shows mismatch; TCP probe labeled 连通 (not proxy path test)
- Advanced routing/DNS settings hidden until generate is wired to them

## Status

Product shell is operational for macOS local use. Not a notarized App Store / public download build.
