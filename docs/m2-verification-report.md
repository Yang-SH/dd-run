# M2 验证报告 — 命令执行与结果状态机

> 验证日期：2026-09-02
> 验证方法：实际运行 `cargo build` / `cargo test` / `cargo clippy -D warnings` / `cargo fmt --check`（本机 `CARGO_INCREMENTAL=0`），并逐文件核对实现与 `docs/implementation.md` §M2 完成判据（A4/A5/A9）及 `cmdpal-platform-agnostic-design.md` 语义。
> 参照技能：`@skill:tdd`（测试应经由公共接口验证**行为**，而非实现细节；本报告据此指出 UI 接线层缺少行为级单测）。

---

## 1. 工程验收（本次实测）

| 项 | 命令 | 结果 |
|---|---|---|
| 构建（`dd-gui`） | `cargo build -p dd-gui` | ✅ `Finished`，0 告警 0 错误 |
| 构建（全 workspace） | `cargo build` | ✅ `Finished`，0 告警 0 错误 |
| 单测（`dd-gui`） | `cargo test -p dd-gui` | ✅ **21 passed; 0 failed**（`state` 9 + `aggregator` 3 + `navigation` 5 + `result` 4） |
| 单测（`dd-host`） | `cargo test -p dd-host` | ✅ **24 passed; 0 failed**（process）+ 5 passed（manifest），0 失败 |
| clippy | `cargo clippy -p dd-gui --all-targets -- -D warnings` | ✅ `Finished`，0 告警 |
| 格式 | `cargo fmt --check` | ✅ FMT OK |

---

## 2. 完成判据逐项核对（implementation.md §M2 → A4/A5/A9）

| 判据 | 要求 | 状态 | 证据 |
|---|---|---|---|
| **A4** | 单测覆盖全部 8 种 `CommandResultKind` | ✅ 已满足 | `crates/dd-gui/src/result.rs`：`resolve()` 8 分支一一映射；测试 `resolves_all_eight_kinds` 断言 `Dismiss/GoHome/GoBack/Hide/KeepOpen/GoToPage/ShowToast(带/不带 duration)/Confirm` 共 8 条，全过 |
| **A5** | 页面栈 `GoBack`/`GoHome` 导航单测 | ✅ 已满足 | `crates/dd-gui/src/navigation.rs`：`push_enters_nested_page_and_back_returns`、`go_back_on_root_returns_none`（Root 上返回 `None` 边界）、`go_home_clears_all_nested_pages`、`root_mut_replaces_items_keeping_stack` 共 4 项导航单测全过 |
| **A9** | 协议审查：列表更新走"事件 + 全量拉取"，**无增量集合推送** | ✅ 已满足（协议层 + 运行时两层） | 协议层：`protocol.md` §6.3 `get_items` 返回全量、§7.1 `items_changed` 仅携带 `page_id`（无增量内容）；运行时：`dd-host/src/process.rs::poll_notifications()` 非阻塞消费通知 + `main.rs` 100ms 合并窗口（`REFRESH_WINDOW`）+ `fetch_page()` 全量重拉 |

**结论**：M2 三条核心判据（A4/A5/A9）均已满足，工程验收四项全绿，全 workspace 构建无回归。

---

## 3. 已通过的项（逐任务）

| 任务 | 状态 | 说明 |
|---|---|---|
| P1 命令信息透传 | ✅ | `PanelItem` 含 `id`/`ext_id`/`command`；`to_panel_item` 透传；`state.rs` 9 单测 + `aggregator.rs` 映射断言覆盖 |
| P2 页面栈 | ✅ | `navigation.rs` 5 单测全过（A5） |
| P3 8 种 Kind 裁决 | ✅ | `result.rs::resolve` 8 分支 + `resolves_all_eight_kinds`（A4） |
| P4 Confirm 挂起 + invoke 参数 | ✅（逻辑层） | `PendingConfirm::confirmed_params()` 正确补 `context.confirmed=true` 并保留 query/目标 id；`invoke_params()` 带 `sender=top_level`——**单测通过**；但 UI 接线层未正确使用（见 §4 问题 P1） |
| P5 UI 接线 | 🟨 编译/类型/clippy 通过，行为靠真机验收 | Enter 分派、`Esc` 非 Root 先返回、Toast、Confirm 对话框、`GoToPage` 推页 + `get_items` 均已在 `main.rs` 落地并通过编译期验证；缺行为级单测（见 P2） |
| P6 全量拉取 | ✅（协议 + 运行时） | `poll_notifications` + 100ms 合并 + `fetch_page` 全量重拉落地；`poll_notifications` 本身缺单测（见 P3） |
| P7 工程验收 | ✅ | 四项全绿 + 全 workspace 构建无回归 |

---

## 4. 未通过 / 存在问题项

### ✅ P1（原 MEDIUM 缺陷，已修复）：`Confirm` 重发丢失原始 `sender` 与 `context`

- **现象（修复前）**：协议 §8.3 注明"宿主确认后**重新 invoke（context.confirmed = true）**"，语义是**沿用原请求的 context 仅置 `confirmed=true`**。`result.rs::PendingConfirm` 设计正确（持有 `sender` + `context`，`confirmed_params()` 会保留 query/`selected_item_id`/`form_data`）。但 `main.rs::apply_action` 的 `Confirm` 分支原重新构造了一个 **`sender: Sender::TopLevel` + `context: None`** 的 `PendingConfirm`，丢弃了原始 invoke 的真实 `sender` 与 `context`。
- **后果（修复前）**：用户确认后重发的 `invoke` 实际携带空 context，原搜索词与选中项丢失；对示例扩展无害，但对真实扩展会破坏"确认时上下文连续性"。
- **修复（2026-09-02）**：
  1. `result.rs` 新增纯函数 `pending_confirm_for(command_id, last_invoke: Option<&InvokeParams>) -> PendingConfirm`，**沿用原始 `invoke` 的 `sender` 与 `context`**，仅在 `confirmed_params()` 补 `confirmed=true`；`last_invoke` 为 `None` 时回退 `top_level` + 空 context（兜底）。
  2. `main.rs` 增加 `last_invoke: Option<InvokeParams>` 字段，`start_invoke` 时记录完整参数；`apply_action` 的 `Confirm` 分支改用 `result::pending_confirm_for`，并删除不再使用的 `Sender` 导入。
- **回归测试**（2 条，现 23/23 的一部分）：`pending_confirm_for_preserves_original_sender_and_context`（断言保留 `ListItem` sender + query + `selected_item_id`，重发仅补 `confirmed=true`）、`pending_confirm_for_falls_back_when_no_history`（兜底回退）。
- **证据**：`crates/dd-gui/src/result.rs`（`pending_confirm_for` + 2 测试）、`crates/dd-gui/src/main.rs`（`last_invoke` 字段、`start_invoke` 记录、`apply_action` Confirm 分支）。

### ⚠️ P2（MEDIUM，测试覆盖缺口）：P5/P6 的 UI 行为无行为级单测

- **现象**：Enter 分派、`Esc` 语义（非 Root 先 `go_back`、Root 再 `hide`）、Toast/Confirm 弹层、`items_changed` → 合并 → 全量重拉，这些**可观察行为**目前只被"编译通过 + clippy 通过 + 真机人工验收"覆盖，没有 headless 单测。
- **依据（TDD 原则）**：测试应经由公共接口验证**行为**而非实现；`PaletteApp` 当前与 `egui::Context` 紧耦合，无法脱离 GUI 做单元测试。
- **建议**：把调度/裁决/刷新逻辑抽到不依赖 egui 的 `Controller`/`Session` 类型（仅依赖 `dd-host`/`dd-gui` 逻辑模块 + 一个最小 `View` 回调），再单测：
  - `invoke` 返回 `Confirm` → 用户确认 → 重发带 `confirmed=true`（且保留 context，与 P1 修复联动）；
  - 嵌套页按 `Esc` → `go_back`；Root 按 `Esc` → `hide`；
  - `items_changed(page_id)` → 进入 100ms 合并窗口 → 窗口到期触发 `fetch_page`（全量）。

### ⚠️ P3（LOW，测试覆盖缺口）：`poll_notifications` 缺单测

- **现象**：`dd-host/src/process.rs::poll_notifications()`（通知消费循环：`try_recv` → 过滤 `jsonrpc` 版本 → 提取 `items_changed.page_id`）运行时逻辑无单测；`dd-host` 现有测试仅覆盖 `items_changed` 参数解析与协议版本，未覆盖整个通知轮询/返回 `Vec<Option<String>>` 的行为。
- **建议**：用本地 channel 喂入若干 `Frame::Message`，断言返回的 `page_id` 列表（含 `None`=顶层）正确、非 `items_changed` 方法被忽略。这能补齐 A9 的"运行时一半"单测证据。

### 备注（非阻塞，已知限制）

- 顶层 `items_changed`（`page_id=None`）当前仅弹 Toast "扩展命令已更新"，**未触发 Root 列表重聚合**（`m2-record.md` §5 已记录）。属后续项，不阻塞 M2 关闭，但建议在 M3 前补齐（Root 重聚合代价低，且是 A9"事件 + 全量拉取"在顶层命令上的完整闭环）。

---

## 5. 后续改进建议

1. **先修 P1（缺陷）**：改动小、风险低，建议在 M2 关闭前修复（约 10 行 + 1 个回归单测）。
2. **补 P2 / P3 行为单测**：把 `PaletteApp` 的控制器逻辑解耦为可测类型，使 M2 的"UI 行为"在合并前具备自动化证据，而非仅靠真机人工验收。
3. **真机验收（沙箱无法代劳）**：A1/A11 属 M1、Enter 分派/Esc/Toast/Confirm/`items_changed` 刷新属 M2 UI 行为，需在真机运行 `./target/x86_64-pc-windows-gnu/debug/dd-gui.exe` 确认。
4. **顶层 `items_changed` 重聚合**：纳入 M3 或 M2 收尾，闭合 A9 在顶层命令上的全链路。

---

## 6. 结论

- **工程验收**：✅ 全绿（build / test **23+24** / clippy 0 / fmt OK / workspace 无回归）。
- **核心判据 A4/A5/A9**：✅ 全部满足。
- ** blockers**：无。M2 可判定为"代码完成、待人工验收"。
- **P1 已修复**（2026-09-02）：Confirm 重发现正确沿用原始 `sender` + `context`（仅补 `confirmed=true`），并补 2 条回归测试。
- **仍建议（非阻塞）**：P2（把 `PaletteApp` 控制器逻辑解耦为可 headless 单测的 `Session` 类型，覆盖 Enter 分派/Esc 语义/`items_changed` 刷新等行为）/ P3（`poll_notifications` 单测）；二者为测试质量改进，建议在合并前补齐以符合 TDD 的"行为可测"原则。
