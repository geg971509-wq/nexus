## Agent A — 事实/逻辑

### 建议 1
- 问题：WP1 `cleanupAll` 顺序写反。现 `Stop` 是 **先** `needUnsetDNS`（用 live box InterfaceMonitor）再 `CloseWithTimeout`；计划写成「关 box → … → unset DNS if needed」，会在 box 已 nil 后访问 monitor。另：`Start` 的 `defer` 在 `err!=nil` 时只 `boxInstance=nil`，xray 失败路径不 stop extra（orphan 根因成立）。
- 改计划：cleanupAll 顺序改为 **unset DNS（box 仍活）→ close box → closeXray/gate → stop extra → nil/cancel/profile=-1**；并点名替换 Start 那条「只 nil box」的 defer。

### 建议 2
- 问题：WP3 根因写对一半。`disconnect_selected_sync` 里 `stop_rpc()?` 失败会 **整段提前 return**，kill/SESSION=None/proxy/tray 全跳过——计划 #2 正确。但 #3「dead session heal」只写 `connect_selected`/`core_start`：现路径是 `g.is_none()` 才 spawn，**已有 dead child 的 `Some(session)` 不会 recycle**；Unix accept 失败 `kill` 后 **不 `wait`** 会 zombie——#4 正确。Windows `kill_stray`：`let _ = except` 在函数顶，Unix 分支仍读 `except` 跳过 PID；**仅 Windows `/IM` 无视 except**——证据基事实句正确，勿改成「Unix 也不尊重」。
- 改计划：#3 明确「`g.is_some()` + `try_wait` 已退出 → drop 再 start」；Acceptance 补一句「`stop_rpc` Err 时仍走 kill+proxy+tray」。

### 建议 3
- 问题：未把 defer 标成已修（D1 SESSION 跨 60s Start、D2 Windows pipe timeout、完整状态机均在 Defer/Out of WP）——好。但 **SESSION 持锁跨 Start 与 Windows 无超时仍是审计 P0**，本 PR 明确 defer 后 Acceptance 未声明「残留 P0 接受」。UI 事实：`powerBusy` 不护 ctx restart；`applyBlocklistLive` 与 power 互斥会 **drop**；`renderNodes` 无条件 `selectedName=nodes[0]` 且 `setConnected(connected,{pin:false})`→`startConnPoll` 清 `_coreBase*`；boot 双 fire-and-forget——与计划一致。分支 tip `d256e35` 与 HEAD 一致。
- 改计划：Acceptance 加「已知 defer P0：SESSION 跨 RPC / Win pipe timeout，本 PR 不验收」；WP2.5 保留 selection 修复（代码确有无条件打回 nodes[0]）。

### 可执行结论
- 计划是否可进共识：**是**（修 cleanupAll 顺序 + disconnect 失败仍 teardown 写清后即可执行；defer 的 SESSION/pipe 勿当已修）。

核对摘要：Unix kill_stray 已尊重 except；Windows 否。无「defer 误写成已修」。漏真 P0 风险仅在于 cleanup 顺序写错会修坏 DNS unset。
