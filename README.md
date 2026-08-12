# Nexus

Dual-arch VPN / proxy client. UI: Apple-minimal HTML. Engine: Go core (sing-box + xray) + framed IPC. Shell: **Tauri 2 + Rust**.

![Nexus UI](assets/screenshot.png)

| Item | Value |
|------|--------|
| Product | Nexus |
| Version | 0.2.3 |
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
- `core/server/` — Go core (`module NexusCore`, **GPLv3** combined work)
- `licenses/` — full GPLv3 / MPL-2.0 texts
- `THIRD_PARTY_NOTICES.md` — third-party inventory
- `bin/` — build outputs (gitignored)

## License

**Multi-license (Plan A).** Not a single proprietary blanket.

| Part | Terms |
|------|--------|
| **NexusCore** (`core/server`, `NexusCore` binary) | **GPLv3+** — see [`core/server/LICENSE`](core/server/LICENSE), [`licenses/GPL-3.0.txt`](licenses/GPL-3.0.txt) |
| **Shell / original product code** (`app/`, …) | Nexus original terms in root [`LICENSE`](LICENSE), with carve-outs for GPL/MPL rights |
| **Third-party** | [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) |

Distributing an app that embeds NexusCore requires GPLv3 compliance for Core
(Corresponding Source for that build). MPL-2.0 applies to xray-core covered files.
This is an engineering layout, not legal advice.

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

## Capabilities (0.2.3)

- Connect: selected node → generate → Core `Start` (share link or outbound JSON)
- Tun chip + system proxy; exit clears OS proxy on `:2080` and stops Core
- Catalog (groups/nodes) in store via `catalog_get` / `catalog_put`
- Node **Traffic** column: Core `QueryStats` deltas accumulated **per node** (survives node switch / Tun re-Start; only Reset traffic zeros)
- Honest UI: tunnel ≠ selected shows mismatch; TCP probe labeled Connectivity (not a proxy-path test)
- **Firewall (OS fail-closed):** sidebar **防火墙** + macOS **NexusFwD** LaunchDaemon (PF anchor `nexus` in main ruleset); Windows elevated firewall rules (4 policies). Domain/process blocklist removed. Orthogonal to sing-box routing/Core/Tun/proxy. Install helper before connect on mac.
- Connection table: merge by Core id, multi-select like nodes (copy), process + PID columns
- **i18n:** UI chrome + runtime log panel in `zh-CN` / `en` / `ru` / `zh-TW` (live language switch)
- Advanced routing/DNS settings hidden until generate is wired to them
- Windows: elevated shell, no console flash on helper spawns

## Status

Operational for **macOS arm64** and **Windows x86_64** internal use. Not a notarized App Store / public-download build.

## License

**Multi-license.** NexusCore is **GPLv3+**; shell original code has separate terms; third-party list in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md). See root [`LICENSE`](LICENSE). Distributing Core requires Corresponding Source.
