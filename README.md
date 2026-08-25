# Nexus

macOS VPN / proxy client. GUI: Qt Quick (`app/qt`). Engine: Go core (**sing-box**) + framed IPC.

> xray-core is linked into NexusCore and the Core gates it on `need_xray`, but the
> shell never sets that flag and has no Xray config generator — so **Xray never
> runs today**. Nodes that need it (VLESS with `type=xhttp`, an `encryption` other
> than `none`, or `extra=`) are refused at import rather than accepted and left to
> fail at connect. The MPL-2.0 obligation below still applies: the code ships
> whether or not it executes.

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

Windows is not a product this round. Old HTML GUI and Windows pack scripts live under [`archive/`](archive/).

### Data directories

| OS | Data | Core log |
|----|------|----------|
| macOS | `~/Library/Application Support/Nexus` | `~/Library/Logs/Nexus/core.log` |

The data and log directories are chmodded to `0700`: Core logs every outbound destination at
`info`, so the log and sing-box's `cache.db` beside it are a record of where the
traffic went. `core.log` rolls to `core.log.1` past 16 MB at spawn. The macOS
firewall daemon logs separately to `/var/log/nexusfwd.log`, created `0600` at
install and removed on uninstall.

## Layout

- `app/qt/` — Qt Quick GUI (C++ host + QML)
- `app/src-tauri/` — Rust engine + JSON C ABI (`nexus_invoke`) the Qt host links
- `app/src-tauri/src/core/` — framed IPC + session spawn
- `app/src-tauri/src/data/` — JSON store + pure generate
- `app/src-tauri/src/sys.rs` — system proxy (macOS networksetup)
- `core/server/` — Go core (`module NexusCore`, **GPLv3** combined work)
- `licenses/` — full GPLv3 / MPL-2.0 texts
- `THIRD_PARTY_NOTICES.md` — third-party inventory
- `archive/` — former HTML GUI and Windows pack scripts (not built)
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

### Full rebuild (macOS host)

```bash
./build.sh   # no flags — always full rebuild
```

Produces `bin/NexusCore` and `bin/Nexus.app` (Qt host).

Requires: macOS, Xcode CLT, `go`, `cargo`/`rustc`, `cmake`, Homebrew Qt 6.11.

QML is loaded from source (`NEXUS_QML_DIR`). This path produces an **unsigned** local/internal build (no Apple notarization). Fine for your machines. Gatekeeper will warn on other hosts until you sign with your own credentials (not wired in `build.sh`). Do not run `macdeployqt`; the `.app` is for this Mac.

## Dev

```bash
export PATH="$HOME/.cargo/bin:$PATH"
./build.sh          # when you need a fresh Core + mac .app
cmake --build app/qt/build --target nexus && app/qt/build/nexus
```

## Capabilities (0.2.3)

- Connect: selected node → generate → Core `Start` (share link or outbound JSON)
- Tun chip + system proxy; exit clears OS proxy on `:2080` and stops Core
- Catalog (groups/nodes) in store via `catalog_get` / `catalog_put`
- Node **Traffic** column: Core `QueryStats` deltas accumulated **per node** (survives node switch / Tun re-Start; only Reset traffic zeros)
- Honest UI: tunnel ≠ selected shows mismatch; TCP probe labeled Connectivity (not a proxy-path test)
- **Runtime status** shows the exit IP and country as the far end sees them, fetched *through* the tunnel — a direct lookup would report this machine and be the wrong answer stated confidently. Blank when the tunnel cannot carry it.
- **Import** parses share links for vless / vmess / trojan / ss / socks / http(s) / anytls / tuic / hysteria / hysteria2, and Clash YAML for the same set. Entries it cannot use are reported by protocol instead of silently lowering the count — including `vless-xray` for the Xray-only VLESS above.
- **Idle-only actions:** the direct TCP probe binds the physical NIC, and uninstalling the firewall helper flushes the PF anchor. Both are refused unless the tunnel is fully disconnected. While connected, use the per-node URL test, which measures via the node.
- **Firewall (OS fail-closed, macOS only):** sidebar Firewall + **NexusFwD** LaunchDaemon (PF anchor `nexus`). Domain/process blocklist removed. Orthogonal to sing-box routing/Core/Tun/proxy. Install helper before connect.
- Connection table: merge by Core id, multi-select like nodes (copy), process + PID columns
- **i18n:** UI chrome + runtime log panel in `zh-CN` / `en` / `ru` / `zh-TW` (live language switch)
- Unwired settings (routing/DNS, inbound, mux, updater, autostart) hidden until store/generate is wired; mixed inbound is `127.0.0.1:2080`

## Status

Operational for **macOS arm64** internal use. Not a notarized App Store / public-download build.
