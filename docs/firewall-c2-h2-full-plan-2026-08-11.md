# Nexus firewall C2+H2 — CONSENSUS plan (2026-08-11)

**Status:** EXECUTED — mac Active (NexusFwD + PF); **Windows product = Unsupported** (8A; eng 4B status honesty, windows.rs retained for later).  
**Process:** AGENTS.md §02 · 4 audits all **approve-with-fixes**  
**User locks (shipped):** mac H2 launchd root daemon · preserve sing-box/Core/Tun/system-proxy/catalog · applied/last_policy = success-only (eng 3A)

---

## Capability fence (hard)

Firewall is orthogonal. **Never** rewrite generate route for product filter; **never** move Core Start into daemon; **never** re-add blocklist. Connect still: generate → Core Start → proxy/Tun as today.

---

## Consensus locks (audits A–D)

| # | Lock |
|---|------|
| L1 | **Socket:** root listens; sock **not** root-0600-only. Use **0660** (group wheel/admin) **or** 0666 + **mandatory `getpeereid`** allowlist (console/install UID). Refuse world-writable without peer check. |
| L2 | **Every apply:** `enable` → **`try_add_anchor` into main** (filter; scrub optional deferred) → load rules → **broad state flush**. Never edit `/etc/pf.conf`. Re-assert anchor each apply (survives PF reload). |
| L3 | **No helper (mac) ⇒ block connect** (no Core Start). UI CTA 安装守护. No soft-skip Active. |
| L4 | **Tun on + unknown ifname ⇒ stay Connecting policy**; SM Connected only after Connected policy OK (+ tun_if when Tun on). |
| L5 | **Order:** helper ready → peer → BeginConnect → firewall Connecting → (Tun elevate Core) → Start → Connected FW → proxy. Disconnect: Blocked → Core/proxy down → Reset. |
| L6 | **Daemon = PF only** small bin; pure `rules` module (no I/O) shared; shell = client/install only. Prefer `[[bin]] nexusfwd` with thin deps via cfg. |
| L7 | **Windows:** pure `windows-sys` WFP **all four** policies complete; else keep **Unsupported** — never half-Active. No winfw vendor. Shell already requireAdministrator — still check elevated at apply. |
| L8 | **YAGNI defer:** scrub ruleset, selective state kill (broad `-k` OK), full NDP suite, SMAppService API, auto-install dual path, LAN UI, split-tunnel. |
| L9 | **Classic launchd** install (osascript once): PrivilegedHelperTools + `/Library/LaunchDaemons/app.nexus.firewall.plist`. Status = installed/running/error only. |
| L10 | **Brick-net recovery:** launch reset; uninstall path bootout+flush+release enable; document manual `launchctl bootout` + `pfctl -a nexus -F all`. |

---

## Target shape

- **mac:** `NexusFwD` root LaunchDaemon · unix socket · PF anchors `nexus`  
- **win:** product **Unsupported** (status must not fake helper running); `windows.rs` kept for a future Active ship  
- **shell:** `firewall::apply` → platform backend; SM Connected via `MarkConnected` only (eng 1A/6A) 

### Daemon protocol (JSON line or length-prefix)

`{ "op":"ping"|"apply"|"reset"|"status", "policy": optional }` → `{ "ok":bool, "err"?:string, ... }`

### Policies

Reset | Connecting{peer,tun,mixed} | Connected{peer,tun,mixed,tun_if?} | Blocked{peer?,mixed}

---

## Phase execute (this session)

1. Pure PF rule builder + tests  
2. `nexusfwd` daemon: socket + PF add_anchor/load/reset/flush  
3. mac client + install/uninstall  
4. Wire L3–L5 in lib.rs  
5. Windows WFP four policies or honest fail  
6. UI helper status + install  
7. ACL + tests  
8. No commit until 保存  

---

## Verify

- mac: install → connect → `pfctl -s Anchors` / `-a nexus -s rules` → disconnect clear  
- win: elevated connect filters present; disconnect clear  
- regression: generate no reject; Core Start; proxy; catalog  

---

## Audit trail

- A: socket 0600 bug; re-add_anchor every apply; helper/tun honesty  
- B: sock auth; classic launchd; win elevation gate; missing helper blocks connect  
- C: pure rules vs I/O; no FFI dual path; thin lib.rs  
- D: defer scrub/selective/NDP; WFP complete-or-Unsupported  

All four → fixes locked above.
