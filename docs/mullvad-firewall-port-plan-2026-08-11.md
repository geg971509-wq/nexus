# Nexus OS firewall port — CONSENSUS plan (2026-08-11)

**Status:** EXECUTED — mac PF + NexusFwD; Windows Unsupported (honest status); SM `MarkConnected`; applied=success-only  
**Process:** `/Users/king/Desktop/AGENTS.md` §02  
**Product:** **P2** — remove 拦截/blocklist; Shell-owned OS fail-closed firewall  
**Also:** **1A** tunnel SM · **2A** reconnect stay non-Idle · **3C** OS firewall  

---

## Locked consensus (from audits)

| # | Decision |
|---|----------|
| L1 | **Clean-room reimplement** policy from Mullvad *behavior* / `security.md` tables — **no** line-by-line `talpid-core` copy into shell. Provenance note only. Avoids Plan A shell↔GPL contamination. |
| L2 | **mac elevation:** PF apply via **privileged `osascript` admin one-shot** (same pattern as Core setuid), not “reuse setuid Core”. No Core `Start` until Connecting policy apply succeeds **or** platform `Unsupported`. |
| L3 | **Crash/exit:** filters **non-persistent**; **reset on every teardown/exit**; **startup clears stale `nexus` anchor**. Never “leave Blocked” without a service. |
| L4 | **v1 policy surface (YAGNI):** only `reset` / `connecting(peer)` / `connected(peer, tun_if)` / `blocked`. Hard-code **allow_lan=true**. No LAN UI, no process-path allow lists unless apply fails without them. |
| L5 | **Mode branch:** if **Tun off** (system-proxy only), Connected policy **allows localhost mixed-port + peer** (not “only utun”). Tun on → Mullvad-like tunnel iface allow. |
| L6 | **SM ownership:** `tunnel_sm` is sole transition API; gen epoch lives with SM; `lib.rs` maps commands → events only (thin). |
| L7 | **Windows v1 = D3:** `Unsupported` + honest UI; no fake protected. mac PF is first ship. |
| L8 | **Peer/tun loop:** resolve peer before Connecting; Start; poll tun (mac utun / win `nexus-tun`) with timeout → Connected; fail → Error/blocked + stop Core. **2A never Idle between retries.** |
| L9 | **Teardown order:** firewall reset → stop Core → clear system proxy (best-effort each). |

---

## Goal

1. Remove blocklist product (store/API/generate reject/`find_process` for blocklist/UI/conn menus).  
2. Nav **拦截 → 防火墙**; status = SM + last firewall apply.  
3. macOS: PF anchor `nexus` fail-closed while Connecting/Connected/Error.  
4. Windows: Unsupported until later D2.  
5. Stability: no sticky proxy; no fake Connected.

---

## Modules

```
app/src-tauri/src/
  tunnel_sm.rs          # State + Event + transition + gen
  firewall/
    mod.rs              # Policy + apply + status enum
    macos.rs            # pfctl admin scripts, anchor nexus
    windows.rs          # Unsupported
    null.rs             # other OS
```

### SM states / events

`Idle | Connecting | Connected | Disconnecting | Error`

Events: `UserConnect{peer,tun_wanted,mixed_port}`, `CoreStarted{tun_if?}`, `TunReady{ifname}`, `UserDisconnect`, `Fail{msg}`, `CoreDied`, `Cancel`.

Transitions apply firewall then side effects (proxy/core) per L8–L9.

### PF v1 rules (mac, clean-room minimal)

**connecting / blocked:** enable PF if needed; anchor drop all; pass lo0; pass DHCP/NDP essentials; pass to peer IP/port proto; pass from/to 127.0.0.1 mixed_port if proxy mode.

**connected (tun on):** + pass on tun iface.

**connected (tun off):** pass lo0 + peer + localhost proxy port; drop other non-LAN outbound (allow_lan hard-true: pass RFC1918).

**reset/idle:** flush delete anchor `nexus`; if we enabled PF globally, leave PF as-found (track `pf_was_enabled`).

---

## Phase order (execute)

1. Add `tunnel_sm` + `firewall` (mac real, win unsupported).  
2. Strip blocklist (store/generate/lib/UI).  
3. Wire connect/disconnect/teardown through SM.  
4. 防火墙 UI + i18n.  
5. Tests: SM transitions; generate no reject; policy builder unit.  
6. README capability line.  
7. No commit unless user 保存.

---

## Verification

- mac: connect → `sudo pfctl -a nexus -s rules` non-empty; disconnect → empty; launch clears stale.  
- UI never shows protected on Windows.  
- `cargo test` shell; no blocklist symbols.  
- System proxy cleared on disconnect/error.

---

## Audit trail

- A: elevation false-eq, license, tun/proxy mode  
- B: elevation close-loop, teardown, peer/tun timeout  
- C: SM out of lib.rs god-object, no GPL module fiction  
- D: shrink v1 policy, defer multi-backend polish  

All four: **approve-with-fixes** → fixes locked above.
