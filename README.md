# Nexus

macOS VPN / proxy client (MVP). UI: Apple-minimal HTML. Engine: Go core (ThroneCore lineage) + sing-box/xray git deps. Shell: **Tauri 2 + Rust**.

## Identity

| Item | Value |
|------|--------|
| Product | Nexus |
| Bundle ID | `app.nexus.desktop` |
| Deeplink | `nexus://` |
| Data dir | `~/Library/Application Support/Nexus` |
| Single-instance prefix | `nexus-` |
| Core binary (target) | `NexusCore` (libcore IPC) |

**Do not edit** `upstream source tree`. This tree is a copy + product worktree.

## WARP

Vendored Throne `warp-client` is **not** shipped. Use official:

`/Applications/Cloudflare WARP.app`

## Layout

- `app/` — Tauri 2 (`ui/` = nexus-vpn-ui.html + icons)
- `core/server/` — Go core (spawn in Phase B)
- `docs/nexus-throne-port-plan.md` — FINAL port plan

## Dev (Phase A)

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd app
npm install
npm run tauri dev
```

## Plan

See `docs/nexus-throne-port-plan.md`. B–E: Core IPC, DB/sub/generate, sysproxy+DNS, WARP polish.
