# Nexus

Dual-arch VPN / proxy client. UI: Apple-minimal HTML. Engine: Go core (sing-box + xray) + framed IPC. Shell: **Tauri 2 + Rust**.

![Nexus UI](assets/screenshot.png)

| Item | Value |
|------|--------|
| Product | Nexus |
| Version | 0.2.2 |
| Bundle ID | `app.nexus.desktop` |
| Deeplink | `nexus://` |
| Core binary | `NexusCore` (framed IPC) |
| Socket env | `NEXUS_CORE_SOCKET` / `NEXUS_CORE_DEBUG` |

## Platforms

| Target | Arch | Shell / install | Core |
|--------|------|-----------------|------|
| macOS | arm64 | `.app` (unsigned internal) | `NexusCore` (CGO) |
| Windows | x86_64 | `nexus.exe` (compile only) | `NexusCore.exe` (purego cross) |

### Data directories

| OS | Path |
|----|------|
| macOS | `~/Library/Application Support/Nexus` |
| Windows | `%APPDATA%\app.nexus.desktop` (Tauri) / product data under the same family |

### Windows notes

- **Admin required.** The Windows shell embeds `requireAdministrator` so Tun/wintun and system proxy work without a second elevate path. UAC once at launch.
- GUI child processes use `CREATE_NO_WINDOW` (no black console flash): Core, taskkill/tasklist, curl (subscription), cscript.
- Packing from macOS must not ship AppleDouble (`._*`) / `.DS_Store` into the Windows tree (breaks `tauri build` permission UTF-8 scan).

## Layout

- `app/` — Tauri 2 (`ui/index.html` is the only product UI)
- `app/src-tauri/src/core/` — framed IPC + session spawn (unix socket / Windows named pipe)
- `app/src-tauri/src/data/` — JSON store + pure generate
- `app/src-tauri/src/sys.rs` — system proxy (macOS networksetup / Windows WinINet registry)
- `app/src-tauri/src/winhide.rs` — Windows no-console spawn helper
- `app/src-tauri/windows/app.manifest` — Windows elevation + DPI
- `core/server/` — Go core (`module NexusCore`)
- `bin/` — build outputs (gitignored)
## Build

### Full dual rebuild (macOS host)

```bash
./build.sh   # no flags — always full rebuild
```

Produces:

- `bin/NexusCore`, staged Tauri externalBin, `bin/Nexus.app`
- `bin/NexusCore-windows-x86_64.exe` (+ seed under `bin/windows-x86_64/`)

Requires: macOS, Xcode CLT, `go`, `cargo`/`rustc`, `npm`.

### Windows shell (on a Windows machine)

1. Place sources + prebuilt `NexusCore.exe` under a build root (e.g. `NexusBuild`). Include `app/src-tauri/windows/app.manifest`.
2. Run `script/build_windows_remote.ps1 -NexusRoot <root>` (or your local equivalent).
3. Artifact: `app/src-tauri/target/release/nexus.exe` only (`--no-bundle`; no NSIS).

Rust + MSVC + npm required on Windows. Mac cannot fully cross the Tauri GUI.

### Distribution (unsigned internal)

This path produces **unsigned** local/internal builds (no Apple notarization, no Windows EV/SmartScreen reputation). Fine for your machines. Gatekeeper / SmartScreen will warn on other hosts until you sign with your own credentials (not wired in `build.sh`).

## Dev

```bash
export PATH="$HOME/.cargo/bin:$PATH"
./build.sh          # when you need a fresh Core + mac .app
cd app && npm run tauri dev
```

## Capabilities (0.2.2)

- Connect: selected node → generate → Core `Start` (share link or outbound JSON)
- Tun chip + system proxy; exit clears OS proxy on `:2080` and stops Core
- Catalog (groups/nodes) in store via `catalog_get` / `catalog_put`
- Node **流量** column: Core `QueryStats` deltas accumulated **per node** (survives node switch / Tun re-Start; only 重置流量 zeros)
- Honest UI: tunnel ≠ selected shows mismatch; TCP probe labeled 连通 (not proxy path test)
- **拦截:** sidebar list + connection right-click three scopes (目标·全部进程 / 目标·仅此应用 / 应用·全部连接 by full executable path); multi-select bulk block (one put / one reconnect); store → `domain_suffix` / IP / process-only reject on generate; apply reconnects when connected
- Connection table: merge by Core id, multi-select like nodes (bulk block / copy), process + PID columns
- Advanced routing/DNS settings hidden until generate is wired to them
- Windows: elevated shell, no console flash on helper spawns

## Status

Operational for **macOS arm64** and **Windows x86_64** internal use. Not a notarized App Store / public-download build.
