# URL 测试 · 连接下只从节点测 — EXECUTED（2026-08-12）

**Status:** EXECUTED  
**Process:** AGENTS.md §02 · 4 audits（≥3 返回）均 **approve-with-fixes** → consensus → implement  
**User lock:** **A**  
**Fence:** 不改 generate 产品路由；不移动 Core Start；不重开 blocklist；不为测速开 PF 洞；不做多节点临时 Core  

---

## 审计共识（已执行）

| 来源 | 锁定修正 | 落地 |
|------|----------|------|
| a 事实 | paint **只** 落到 UI `connectedName` 行；忽略 Core tag；error/ms≤0 → fail | `runUrlTest` via 分支 |
| a 事实 | 经节点 **iff** `running && targets.length===1 && id===connectedName` | `onlyCurrent` |
| b/c/d | **单次** `Test(test_current=true)`；不做 Query 轮询 | `core_url_test_current` |
| c | take / reinstall session | `lib.rs` commands |
| c | URL SOT + `call_timeout ≥ timeout+5s` | `DEFAULT_URL_TEST` + session |
| d | `via_node_mode := session_status.running` only | UI 分支 |
| 全体 | `running` → 永不 `net_tcp_probe`；组/多选 → 提示 return | UI 分支 |

---

## 改动文件

- `app/src-tauri/src/core/proto_gen.rs` — encode/decode TestCurrent + tests  
- `app/src-tauri/src/core/session.rs` — `test_current_url` / `stop_test`  
- `app/src-tauri/src/lib.rs` — `core_url_test_current` / `core_url_test_stop`  
- `app/src-tauri/permissions/nexus.toml` — allow 两 command  
- `app/src-tauri/src/net.rs` — 注释：直连仅未连接路径  
- `app/ui/index.html` — `runUrlTest` 硬分支 + stop 双路径 + i18n 四语  
- 本文件 Status → EXECUTED  

---

## 验证

1. `cargo test --lib` → **56 passed**（+2 encode/decode）  
2. `running` 路径源码无 `net_tcp_probe`  
3. session Test/StopTest 已接线  
4. 组测+running：提示 return，不写其它行 latency  
5. **不**自动 commit（保存才提交）  
