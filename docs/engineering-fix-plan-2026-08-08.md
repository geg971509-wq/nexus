# Engineering Fix Plan — 2026-08-08 (DONE)

**Status:** DONE  
**Scope:** pure engineering only. Zero product features.  
**Agents:** B + C + D consensus (≥3). A late OK.

## Diff summary

1. **WP1 Core:** `lifeMu` + idempotent `cleanupAll`; Start error-defer cleanup (skip on already-started); Query*/TestCurrent snapshot under RLock; process_owner captures box pointer for AfterFunc.
2. **WP2 UI:** `runSessionOp` queue+coalesce; power/ctx/Tun/blocklist via queue; `setConnected` poll only on edge + `sideEffects:false` for render; catalog promise chain; ordered boot hydrate→session.
3. **WP3 Rust:** Windows `kill_stray` honors `except` via tasklist+`/PID`; disconnect always teardown+clear proxy; dead-child heal; Unix accept fail kill+wait.

## Formerly deferred — landed later

| Item | Status | Where |
|------|--------|--------|
| D1 SESSION not held across long RPC | **DONE** | package1 + cycle2 CONNECT_GEN / take-put Start; arch cycle3 6A poll take/put |
| D2 Windows pipe real timeouts | **DONE** | package1 winpipe |
| D3 Store flock | **DONE** | package1 flock + cycle2 `Store::update` RMW |

Superseded by package1 (`5ba8608`) / cycle2 residual eng / architecture cycle3. Do not re-open as residual P0s.

## Verify

- `cargo check` (app/src-tauri) OK  
- `go test -count=1 .` (core/server) OK  
