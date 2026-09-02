# M2 实施记录 — 命令执行与结果状态机（逻辑层）

> **状态**：✅ **已关闭**（M2 十项真机复验全部通过，2026-09-02，见 §4.5）。**逻辑层 + UI 接线（P5/P6）全部落地**：
> 页面栈 + 8 种 Kind 裁决 + Confirm 挂起重发 + invoke 参数 + Enter 分派 / Esc 语义 / Toast /
> Confirm 对话框 / `get_items` 拉取 / `items_changed` 100ms 合并全量拉取；**24/24 单测**（含
> P1 修复 2 条回归）+ 工程验收全绿，全 workspace 构建无回归。真机第一轮 5/10 通过，其后 7 轮修复
> （Toast 渲染裁剪 → 滚动跟随 → 键鼠冲突 → 滚顶回归，见 §4.1–§4.4）已落地（13:38 版 `dd-gui.exe`），
> 全量复验使用 14:34 版（14:21 结构性修复已回滚，交互行为 = 14:05 版；"鼠标滑动不跟手"留作独立优化项）。
>
> **修复记录（2026-09-02，详见 [`m2-verification-report.md`](./m2-verification-report.md) §4 P1）**：
> `Confirm` 重发原丢失原始 `sender`/`context`（硬编码 `top_level` + 空 context），已改为
> `result::pending_confirm_for` 沿用原始 `invoke` 的 sender/context（仅补 `confirmed=true`），
> 并补 2 条回归测试。
>
> **范围说明**：沿用"逻辑层先行"模式——本里程碑核心判据 A4（8 种 Kind 单测）与
> A5（页面栈导航单测）全部由框架无关的纯逻辑满足，即使 M1 R1 人工验收不过而换框架，
> 本层代码不浪费。P5/P6 的 UI 接线建立在此之上，复用 M1 已保活的扩展进程。

---

## 1. 分阶段实施计划与进度

| 阶段 | 内容 | 状态 | 验收标准（量化） | 结果 |
|---|---|---|---|---|
| P1 命令信息透传 | `PanelItem` 补 `id` + `command`（协议 SSOT，`CommandRef::Invoke/Page`）；`to_panel_item` 透传；`PanelItem::new` 默认 id=标题 | ✅ | 编译 + 单测 | ✅ |
| P2 页面栈 | `navigation.rs`：`PageState`（`page_id` None=Root / title / list / is_loading / empty）+ `PageStack`（root / push / go_back / go_home / current / root_mut） | ✅ | 5 个单测全过（A5） | ✅ 5/5 |
| P3 8 种 Kind 裁决 | `result.rs`：`resolve(CommandResult) -> HostAction`（Dismiss/Hide/GoHome/GoBack/KeepOpen/GoToPage/ShowToast/Confirm 一一对应） | ✅ | 8 种全部裁决单测（A4） | ✅ |
| P4 Confirm 挂起 + invoke 参数 | `PendingConfirm`（重发 `invoke` 带 `context.confirmed=true`）+ `invoke_params(id, query)`（§6.5 字段表） | ✅ | 4 个单测全过 | ✅ 4/4 |
| P5 UI 接线 | Enter 分派（`Invoke` → 后台 `invoke` 并裁决 8 种 Kind；`Page` → 推页 + 后台 `get_items`，进程 take/收回保证串行）；Esc 改"非 Root 先返回、Root 再隐藏"；Toast 浮动条；Confirm 对话框（Enter 确认/鼠标点 确认 均重发 `invoke`，Esc 取消）；嵌套页标题栏 | ✅ | 编译 + 工程验收 | ✅ |
| P6 全量拉取 | `items_changed` 通知轮询（协议 §7.1）→ 100ms 窗口合并（A9）→ 全量重拉 `get_items`；`ext_id → process` 索引复用 M1 保活的进程 | ✅ | 编译 + 协议审查（A9） | ✅ |
| P7 工程验收 | build 0 告警 / 测试全过 / clippy `-D warnings` 0 / fmt | ✅ | 四项全绿 + 全 workspace 构建无回归 | ✅ |

## 2. 产出文件清单

| 文件 | 变更 | 内容 |
|---|---|---|
| `crates/dd-gui/src/state.rs` | 修改 | `PanelItem` 补 `id` / `ext_id` / `command`；`PanelState` 补 `PartialEq`；测试字面量更新 |
| `crates/dd-gui/src/navigation.rs` | 新建 | 页面栈：`PageState`（Root / 嵌套页，`ext_id` 记录来源扩展）+ `PageStack`（push / go_back / go_home）；5 单测 |
| `crates/dd-gui/src/result.rs` | 新建 | `HostAction` + `resolve`（8 种 Kind，A4）+ `PendingConfirm::confirmed_params` + `invoke_params`；4 单测 |
| `crates/dd-gui/src/aggregator.rs` | 修改 | `to_panel_item` 透传 `id` / `ext_id` / `command`；`ExtItems::id` / `is_ready` 访问器 |
| `crates/dd-gui/src/main.rs` | 重写（M2 UI 接线） | `PageStack` 替换单层 `PanelState`；Enter 按 `command` 分派（`Invoke`→后台 `invoke`+裁决 / `Page`→推页+`get_items`）；Esc 非 Root 先返回、Root 隐藏；Toast 浮动条；Confirm 对话框（Enter/鼠标确认均重发）；嵌套页标题栏与 `is_loading`/空态；`items_changed` 轮询 + 100ms 合并重拉 |
| `crates/dd-gui/Cargo.toml` | 修改 | 新增 `serde_json` 依赖（`invoke`/`get_items` 参数序列化） |
| `crates/dd-host/src/process.rs` | 修改 | 新增 `TIMEOUT_GET_ITEMS=2s` / `TIMEOUT_INVOKE=10s`（对齐 protocol.md §10 超时表）；`poll_notifications()` 非阻塞消费 `items_changed` 通知，返回 `page_id` 列表（`None`=顶层） |

> 纯逻辑层（state / navigation / result / aggregator）不依赖 egui，可独立单测；
> `main.rs` 的 UI 接线复用这些逻辑，并通过后台线程 + channel 与扩展进程通信，保证 UI 不阻塞。

## 3. 测试方法与结果（`cargo test -p dd-gui`，21/21 全过）

| 模块 | 测试 | 验证点 |
|---|---|---|
| state（9） | 原有 9 个 | 过滤/选中/环绕/夹紧（M1） |
| aggregator（3） | 原有 3 个 + `id/command 透传` 断言 | 映射与错误隔离（M1） |
| navigation（5） | `root_starts_at_depth_one` | Root 页深 1、page_id=None |
| navigation | `push_enters_nested_page_and_back_returns` | 进入嵌套页 / `GoBack` 弹栈回上级（**A5**） |
| navigation | `go_back_on_root_returns_none` | Root 上 `GoBack` → None（关闭由 UI 决定） |
| navigation | `go_home_clears_all_nested_pages` | `GoHome` 清空到只剩 Root（**A5**） |
| navigation | `root_mut_replaces_items_keeping_stack` | 聚合结果替换 Root 列表不影响嵌套页 |
| result（4+2） | `resolves_all_eight_kinds` | **8 种 Kind 一一裁决（A4）** |
| result | `pending_confirm_reinvokes_with_confirmed_flag` | Confirm 重发带 `confirmed=true`、保留 query/目标 id |
| result | `pending_confirm_without_context_still_confirms` | 无 context 时重发仍带 `confirmed=true` |
| result | `invoke_params_carry_id_sender_and_query` | `sender=top_level`、query 透传、首发不带 confirmed |
| result（P1 修复） | `pending_confirm_for_preserves_original_sender_and_context` | 确认重发**保留原 sender + context**（仅补 `confirmed=true`，协议 §8.3） |
| result（P1 修复） | `pending_confirm_for_falls_back_when_no_history` | `last_invoke=None` 兜底回退 `top_level` + 空 context |

### 工程验收（本机，`CARGO_INCREMENTAL=0`）

| 项 | 命令 | 结果 |
|---|---|---|
| 构建（crate） | `cargo build -p dd-gui` | ✅ 0 告警 0 错误 |
| 构建（workspace） | `cargo build`（全量） | ✅ 0 告警 0 错误（确认 `Cargo.toml`/`Cargo.lock` 改动无回归） |
| 单测 | `cargo test -p dd-gui` | ✅ 23/23 通过（含 P1 修复 2 条回归） |
| clippy | `cargo clippy -p dd-gui --all-targets -- -D warnings` | ✅ 0 告警 |
| 格式 | `cargo fmt --check` | ✅ 通过 |

> UI 接线（P5/P6）为运行时行为，无新增单测（所有可单测逻辑已在 state/navigation/result 覆盖）；
> UI 的正确性依赖工程验收（编译/类型/clippy）与真机人工验收（沙箱无法保活 GUI 进程）。

## 4. 人工验收清单（真机复验通过，2026-09-02，A4/A5/A9）

> 启动方式（需真机，沙箱内后台进程会被清理）：
> ```
> cd /d/AI/project/dd-run
> ./target/x86_64-pc-windows-gnu/debug/dd-gui.exe
> ```
> 按 `Win+Alt+Space` 唤起面板。逻辑层（A4 8 种 Kind 单测 / A5 页面栈单测 / A9 协议审查）已自动化覆盖，
> 本清单验证其 **UI 运行时行为**（沙箱无法保活 GUI 进程，须真机）。
>
> 注：示例扩展 `dd-ext-sample` 已升级为 **M2 验收扩展**（`invoke` / `get_items` / `items_changed` 全部实现，
> 运行时验证见 `dd-host/tests/roundtrip.rs::roundtrip_m2_*`）。唤起后 Root 共 **11 条命令**
> （Sample 2 条 + 「M2 验收」9 条），页脚应显示 `✓ Sample Ext（11 命令）`。
>
> Hide vs Dismiss 已按协议 §8.3 在宿主区分（可观察）：`Hide` 隐藏但保留状态（再次唤起仍在当前页/查询）；
> `Dismiss` 关闭并清空（再次唤起回 Root）；Esc/热键/失焦隐藏仍复位（M1 清单第 10 项不变）。
> 每次 `invoke` / `get_items` / `items_changed` 在终端打印 `[dd-gui]` 前缀日志（invoke 成功打印 `Kind → 动作`），
> 示例扩展同步打印 `-> invoke <cmd> => <Kind>`。

### 通用判定规则

1. **必须从终端启动** `dd-gui.exe`（双击运行看不到日志，无法按本清单判定）。
2. 凡涉及 `invoke` / `get_items` / `items_changed` 的项，**以终端 `[dd-gui]` 日志为最终权威**；
   面板行为与日志同时符合才算通过。
3. 纯本地导航（Esc/热键/输入过滤）无终端日志，只看面板行为。
4. 每项标注 ✅/❌ 于「结果」；出现任一「失败信号」即记 ❌ 并注明现象。

---

#### #1 Enter 执行 Invoke 命令（A4）——结果：✅

- **步骤**：① 唤起面板；② 输入 `toast` 过滤出 `Kind：ShowToast`；③ 按 `Enter`。
- **预期（面板）**：底部出现提示条「ShowToast：3 秒后自动消失」，约 3 秒后自动消失；面板保持打开、列表不变。
- **预期（终端）**：
  `[dd-gui] invoke 发起：ext=dev.sample-ext cmd=m2.kind.show_toast` →
  `[dd-gui] invoke 成功：ShowToast { … } → 动作 ShowToast { … }`；扩展侧 `-> invoke m2.kind.show_toast => ShowToast`。
- **失败信号**：终端出现 `[dd-gui] invoke 失败：…`；或面板弹「命令执行失败：…」；或按 Enter 毫无反应。

#### #2 Enter 进入 Page 命令（A5）——结果：✅

- **步骤**：① Root 清空查询；② 选中 `Page：进入子页`；③ 按 `Enter`。
- **预期（面板）**：进入子页——顶部出现页名 `m2.page` 与 `[Esc] 返回`；先显示「正在加载…」，随后列出 **4 条**子命令（GoBack/GoHome/Dismiss/通知），每条副标题含「本页第 **1** 次被拉取」。
- **预期（终端）**：`[dd-gui] get_items 成功：page=m2.page items=4`。
- **失败信号**：`正在加载…` 超 2 秒不消失（get_items 超时 2s）；出现「拉取失败」空态；终端 `get_items 失败`。

#### #3 Esc 返回 / 关闭（A5）——结果：✅

- **步骤**：① 在子页内按 `Esc`；② 回到 Root 后再按 `Esc`；③ 热键唤起。
- **预期（面板）**：① 返回 Root（11 条、页名标题栏消失）；② 面板隐藏；③ 查询框为空、选中第一项（Esc 属用户主动隐藏 → 复位）。
- **终端**：无新增日志（纯本地导航），只看面板行为。
- **失败信号**：子页 `Esc` 无反应；Root `Esc` 不隐藏；唤起后查询仍保留。

#### #4 GoHome 回首页（A5）——结果：✅

- **步骤**：① 进入子页（#2）；② 执行 `GoHome：回首页`。
- **预期（面板）**：立即回 Root 11 条列表。
- **预期（终端）**：`invoke 发起：…cmd=m2.page.home` → `invoke 成功：GoHome → 动作 GoHome`。
- **失败信号**：仍留在子页；终端失败行。

#### #5 Toast 提示条与时长（A4 ShowToast）——结果：✅

- **步骤**：① 执行 `Kind：ShowToast`，估算出现到消失的时长；② 执行 `Say Hello`（Sample 分组），同样估算。
- **预期（面板）**：① ≈3 秒消失（`duration_ms=3000`）；② ≈2 秒消失（未指定 → 宿主默认 2s）。
- **预期（终端）**：两条 `invoke 成功：ShowToast { … }`（duration 分别 `Some(3000)` / `None`）。
- **失败信号**：Toast 不消失、立即消失或不出现；时长明显偏离。

#### #6 Confirm 二次确认（A4 Confirm）——结果：✅

- **步骤**：① 执行 `Kind：Confirm`；② 在对话框上按 `Esc`；③ 再次执行，改按 `Enter`（或点「确认执行」）。
- **预期（面板）**：① 居中弹「二次确认」对话框（描述 + 「确认执行」按钮 + 底部 `[Enter] 确认 [Esc] 取消`）；② 对话框消失、无后续；③ 对话框关闭 + Toast「确认流程闭环：已收到 confirmed=true 重发」。
- **预期（终端）**：② **无**第二次 `invoke 发起`；③ 有第二次 `invoke 发起：…cmd=m2.kind.confirm` → `invoke 成功：ShowToast { … }`（证明重发带 `confirmed=true` 且扩展闭环应答）。
- **失败信号**：确认后无反应；终端无第二次 `invoke 发起`；重发报 `invoke 失败`。

#### #7 8 种 Kind 全覆盖（A4）——结果：✅

每条以终端 `invoke 成功：<Kind> → 动作 <X>` 为判定权威：

| 命令（位置） | 终端 Kind → 动作 | 面板可观察结果 |
|---|---|---|
| `Kind：Dismiss`（Root） | Dismiss → Dismiss | 面板消失 + `[dd-gui] Dismiss：清空页面栈回 Root 后隐藏`；**唤起后回首页** |
| `Kind：Hide`（Root） | Hide → Hide | 面板消失 + `[dd-gui] Hide：保留状态隐藏（下次唤起不复位）`；**唤起后仍在原页/查询** |
| `Kind：GoHome`（Root） | GoHome → GoHome | 无视觉变化（已在 Root，属正常） |
| `GoBack：返回上一级`（子页内） | GoBack → GoBack | 回 Root |
| `Kind：KeepOpen`（Root） | KeepOpen → KeepOpen | 无任何变化（**属预期成功**，看终端即可） |
| `Kind：GoToPage`（Root） | GoToPage → GoToPage | 进入 m2.page |
| `Kind：ShowToast`（Root） | ShowToast → ShowToast | Toast（见 #5） |
| `Kind：Confirm`（Root） | Confirm → Confirm | 确认对话框（见 #6） |

- **Hide/Dismiss 对比要点**：先后各执行一次并分别唤起——Hide 回原位置、Dismiss 回首页，两者可区分即为通过。
- **失败信号**：任一命令终端出现失败行；或面板行为与上表不符。

#### #8 `items_changed` 刷新（A9）——结果：✅

- **步骤**：① 进入子页，记下副标题「第 **1** 次被拉取」；② 执行 `通知：本页 items_changed`；③ 观察副标题。
- **预期（面板）**：Toast「已发送本页 items_changed，观察副标题计数 +1」；约 0.1 秒后列表副标题变为「第 **2** 次被拉取」（全量重拉的证据）。
- **预期（终端）**：`[dd-gui] 收到 items_changed page=m2.page → 100ms 后全量重拉` + 第二条 `[dd-gui] get_items 成功：page=m2.page items=4`。
- **补充**：Root 上执行 `通知：顶层 items_changed` → 终端 `收到顶层 items_changed（Root 重聚合属遗留，仅提示）` + Toast「扩展命令已更新」；列表**不**刷新属已知遗留（§6），不算失败。
- **失败信号**：副标题计数不变；终端无 `收到 items_changed` 行。

#### #9 命令失败反馈（—）——结果：✅

- **场景 A（进程串行化）**：① 进入子页；② **快速连续**按两次 `Enter`（第一条 in-flight 时立刻执行第二条）。
  预期：第二条弹 Toast「扩展进程不可用（可能正在处理上一个请求）」+ 终端 `[dd-gui] invoke 失败：ext=… 进程不可用（可能 in-flight）`（协议 §12 单扩展串行化）。
- **场景 B（扩展进程退出）**：① 任务管理器结束 `dd-ext-sample.exe`；② 隐藏面板后再唤起。
  预期：页脚出现红字 `✗ Sample Ext：扩展进程已退出`。
- **失败信号（即缺陷）**：场景 A 面板卡死或崩溃；场景 B 页脚仍显示 ✓。

#### #10 再次唤起复位（A1/A5）——结果：✅

- **步骤**：① Root 输入查询 `kind`（列表被过滤）；② 按 `Esc` 隐藏；③ 热键唤起。
- **预期（面板）**：查询框为空、列表恢复 11 条、选中第一项。
- **对比**：若此前面板是经扩展 `Hide` 隐藏（#7），唤起**不**复位——用户隐藏复位 / `Hide` 保留 / `Dismiss` 清空，三者语义可区分。
- **失败信号**：唤起后查询仍为 `kind`；或此前在子页、唤起后仍在子页。

### 4.5 全量复验结论与里程碑关闭（2026-09-02）

> **结论：M2 十项真机复验全部通过（#1–#10 均 ✅），里程碑关闭。**
> 复验使用 `dd-gui.exe` 版本 = 14:34（14:21 结构性"跟手"修复已按用户要求回滚，
> 交互行为 = 14:05 版；"鼠标滑动不跟手"的体验反馈留作后续独立优化项，不阻断 M2 关闭）。

| 验收项 | 判定 | 关键证据 |
|---|---|---|
| #1 Enter 执行 Invoke | ✅ | 终端 `invoke 成功：ShowToast → 动作 ShowToast`，面板 Toast 3s 消失 |
| #2 Enter 进入 Page | ✅ | 进入 m2.page，加载后列出 4 条、副标题「本页第 1 次被拉取」 |
| #3 Esc 返回/关闭 | ✅ | 子页 Esc 返回 Root，Root Esc 隐藏，唤起复位 |
| #4 GoHome 回首页 | ✅ | `GoHome → GoHome`，立即回 Root 11 条 |
| #5 Toast 提示条与时长 | ✅ | ShowToast 3s、Say Hello 2s（默认），均独立 Area 浮动显示 |
| #6 Confirm 二次确认 | ✅ | 取消无二次 invoke；Enter/点确认触发重发 `confirmed=true` 闭环 Toast |
| #7 8 种 Kind 全覆盖 | ✅ | 8 种 Kind 终端均 `invoke 成功：<Kind> → 动作 <X>`；Hide/Dismiss 行为可区分 |
| #8 items_changed 刷新 | ✅ | 页级通知触发 100ms 后全量重拉，副标题计数 +1；顶层 items_changed 已知遗留（§6）不计入失败 |
| #9 命令失败反馈 | ✅ | 进程串行 Toast + 扩展退出页脚红字 `✗ Sample Ext` 均可见 |
| #10 再次唤起复位 | ✅ | Esc 隐藏后唤起查询清空、回 11 条；Hide 保留 / Dismiss 清空语义可区分 |

**完成判据对照**：A4（8 种 Kind 单测 + 真机覆盖）✅；A5（页面栈单测 + 真机导航）✅；
A9（事件 + 100ms 合并全量拉取 + 真机计数 +1）✅。**下一项**：进入 M3 UI 接线
（冷启动读桩 / 点击桩复热 / LRU 超容驱逐标 stub）与 A6/A7/A2 真机实测。

### 4.1 修复记录（2026-09-02，随本清单补测发现）

示例扩展升级后跑 M2 roundtrip 测试，暴露宿主**潜在解析缺陷**：`dd-gui::start_invoke` 把
`call()` 返回的内层 `result`（即 §8.3 `CommandResult` 本体）再按 `InvokeResult`
（要求 `result` 字段）解析——**任何成功的 `invoke` 都会报"响应解析失败：missing field `result`"**。
此前未暴露，因为旧示例扩展从未成功响应过 `invoke`（只有失败路径）。已改为直接解析
`CommandResult`（`fetch_page` 的 `GetItemsResult` 载荷形状一致、本就正确，未动）。

验证：`dd-gui` 23/23、`dd-host` lib 31 + roundtrip 6（**新增 M2 全链路测试**）全绿；
clippy `-D warnings` 0 告警、`fmt --check` 通过；`dd-gui.exe` / `dd-ext-sample.exe` 均已重编。

### 4.2 优化记录（2026-09-02，验收反馈驱动）

- **Hide/Dismiss 可观察区分**：宿主此前把两者同等处理（`Dismiss | Hide => hide`），
  无法从行为分辨结果。现按协议 §8.3 语义区分：
  `Dismiss` → `dismiss()`（清空页面栈回 Root + 复位 Root 列表再隐藏，再次唤起回首页）；
  `Hide` → `hide_keep_state()`（隐藏但保留状态，`reset_on_show=false`，再次唤起仍在当前页/查询）。
  用户主动隐藏（Esc/热键/失焦）行为不变（复位，M1 清单第 10 项）。
- **终端结果日志（成功/失败明确输出）**：invoke 发起/成功（`Kind → 动作`）/失败、
  get_items 成功（条数）/失败（含进程不可用）、items_changed（页级/顶层）均打印
  `[dd-gui]` 前缀日志；示例扩展打印 `-> invoke <cmd> => <Kind>`。
- 验证：dd-gui 23/23、dd-host lib 31 + roundtrip 6 全绿，clippy `-D warnings` 0、
  fmt 通过；两 exe 已重编（11:55）。

### 4.3 真机反馈修复（2026-09-02，`M2-测试文档.md`）

真机 10 项结果：#2/#3/#4/#7/#10 通过；#1/#5/#6/#8 共用一个根因；#9 待修复后复测。

| 反馈 | 根因与修复 |
|---|---|
| #1/#5/#6/#8 **Toast 从不显示**（终端均见 `invoke 成功：ShowToast → 动作 ShowToast`，状态与裁决都对） | **渲染裁剪 bug**：`draw_toast` 在 `CentralPanel` 之后画到根 `Ui` 上，布局落到面板矩形之外被整帧裁剪。改为独立 `egui::Area`（ctx 层、`Foreground` 序、底部居中锚点 -48px）。#6 的 Confirm 流程本身通过（日志证明对话框/Esc 取消/Enter 确认/重发闭环全对），仅最后 Toast 不可见 |
| #9 失败反馈"不清楚是否成功" | 失败提示（进程不可用/扩展退出）也是 Toast → 同因不可见。修复后按 #9 场景 A/B 重测即可观察 |
| #8 GoBack 后仍出现一次 `get_items` 且无日志可诊断 | 防御加固：`tick_refresh` 校验通知来源页仍是当前页、否则丢弃并打日志（`items_changed 刷新作废`）；`poll_page` 收到非当前页结果不再静默丢弃（`get_items 结果作废：已离开 page=…`） |

验证：dd-gui 23/23、dd-host lib 31 + roundtrip 6 全绿；clippy `-D warnings` 0、fmt 通过；
`dd-gui.exe` 重编（13:12，重编前需先结束真机测试遗留的 dd-gui/dd-ext-sample 进程——运行中的 exe 锁定导致链接 Permission denied）。

### 4.4 鼠标/键盘选择冲突修复（2026-09-02，`M2-测试文档.md` 后续反馈）

反馈：通过 `Tab`/`↓` 键盘选择时，鼠标静止悬在某行会"抢回"选中项，二者互相干扰。

| 项 | 说明 |
|---|---|
| 根因 | `draw_list` 每帧把 `resp.hovered()` 为 true 的行写入选中（`else if let Some(idx) = hovered { set_selected(idx) }`）。鼠标光标**静止**在某行上时 `hovered()` 每帧都为真，于是下一帧立刻把键盘刚改的选中项覆盖回鼠标位置 → 干扰 |
| 修复（初版） | `PaletteApp` 新增 `last_hovered_index`；**仅当本帧悬停行与上一帧不同**（`hovered != last_hovered_index`）时才接管选中，静止鼠标不再每帧抢占键盘选中。`clicked` 始终直接执行（与 `Enter` 等价），不受此规则影响。用户确认保留"悬停高亮+单击执行"模型，仅消除冲突 |
| 回归根因（`一直按 ↑ 滚到顶部`） | 初版仍只看"悬停行是否变化"——键盘向上滚动时**内容在静止的鼠标下方滑过**，鼠标本身没动，但鼠标下方变成了另一行（更高的项），于是 `hovered` 变了、`hovered != last_hovered_index` 成立，鼠标又把选中抢回"鼠标所在位置的行"，而不是键盘选定的最顶项 |
| 修复（终版，2026-09-02 13:38 后） | `PaletteApp` 新增 `last_pointer_pos`；**仅当本帧鼠标指针屏幕坐标与上一帧不同（`current_hover_pos != last_pointer_pos`，即鼠标真的移动过）** 且 `hovered != last_hovered_index` 时，hover 才接管选中。指针静止时，内容滚动导致的"悬停行变化"被忽略 → 键盘选中（含滚到顶部）不被鼠标拽回。`clicked` 仍始终直接执行 |
| 验收要点 | 列表多于一屏：① 鼠标静止放某行、用 `Tab`/`↓`/`↑` 移动选中（含一直按 ↑ 滚到最顶）→ 选中应跟随键盘、不被鼠标拽回；② **鼠标真正移到"不同行"时**才接管选中；③ 单击任意项仍立即执行 |
| 回归反馈（`鼠标滑动切换不跟手`） | 鼠标在列表上快速滑过若干行时，高亮行**滞后一帧**且偶尔**错位一格**，视觉上"不跟手"。两因叠加：① hover 选中在帧末回写，高亮下一帧才绘制；egui 按需重绘——鼠标一停无新输入事件就不再跑下一帧，高亮「卡」在旧行直到再动鼠标；② hover 接管选中也触发 `scroll_to_me`，鼠标滑到半可见边缘行时内容被滚上去、鼠标下变成新行但指针未动 → 高亮与光标错位一格 |
| 修复（14:05 版） | ① `state.rs` 的 `set_selected` 返回 `bool`（是否真变化）；hover 接管且选中确实变化时调用 `ctx.request_repaint()`，强制下一帧重绘消除「滞后一帧」；② 新增 `scroll_follow: bool` 字段——**滚动跟随仅归键盘**（`handle_keys` 中 `move_down/move_up` 后设 `true`），鼠标驱动选中（`clicked` 或 hover 接管）时设 `false`，`draw_list` 仅当 `scroll_follow` 为真才对选中行 `scroll_to_me`，杜绝鼠标滑过时内容位移错位 |
| 根因（14:21 版，仍「不跟手」） | 14:05 的 `request_repaint` 只是"补一帧"，但高亮渲染依赖的 `selected` 是**帧首快照**：鼠标移到的那一帧，该行仍用旧 `selected` 画、下一帧才追上 → 滑过高亮始终慢一帧。要真跟手，高亮必须在**本帧内**依据 `resp.hovered()` 即时绘制，而非等帧末回写 + 下帧重绘 |
| 修复（14:21 版，结构性，**已回滚**） | `draw_item_row` 重写：先 `Frame::show` 布局内容（透明底）取 `frame_resp`，用**本帧已知**的 `frame_resp.hovered()` 判定高亮并立即 `ui.painter().rect_filled` 填充——高亮与鼠标同帧，彻底消除"慢一帧"。高亮判定：`hovered` 即时高亮（鼠标在列表区）`‖` `keyboard_selected && !mouse_over_list`（键盘导航）；`mouse_over_list` 由 `ui.cursor().min + ui.available_size()` 围成的列表区矩形与指针位置比较得出，避免静止鼠标在列表区时与键盘选中"双高亮"。回写 `selected`（Enter/执行目标）仍走原 `pointer_moved && hovered != last_hovered` 门控，`request_repaint` 不再需要（已移除）。**⚠️ 2026-09-02 14:34 应要求回滚**（用户未认可该结构性改动方向）：`draw_item_row` 还原为 `selected` 参数版（`Frame.fill` 画选中底 + `ui.interact` 注册点击），`draw_list` 移除 `list_rect`/`mouse_over_list` 判定，hover 回写恢复 `set_selected` 返回 bool 且仅变化时 `request_repaint` 的 14:05 行为 |

验证（回滚后 14:34 版）：dd-gui **24/24 单测**、clippy `-D warnings` 0、fmt 通过；`dd-gui.exe` 重编（14:34）。

## 5. 完成判据对照（implementation.md M2）

| 判据 | 状态 | 说明 |
|---|---|---|
| 单测覆盖**全部 8 种 Kind**（A4） | ✅ 已满足 | `result.rs` `resolves_all_eight_kinds`（含 ShowToast 带/不带 duration、Confirm 全字段） |
| 页面栈 `GoBack` / `GoHome` 导航单测通过（A5） | ✅ 已满足 | `navigation.rs` 4 项导航单测（含 Root 上 GoBack 返回 None 的边界） |
| 协议审查确认：列表更新走"事件 + 全量拉取"，**无增量集合推送**（A9） | ✅ 已满足 | 协议 §6.3 `get_items` 返回全量、§7.1 `items_changed` 仅带 `page_id`（无增量内容）；运行时 `poll_notifications` 轮询 + 100ms 窗口合并（A9）+ `fetch_page` 全量重拉已落地 |

## 6. 遗留与下一步

| 项 | 说明 |
|---|---|
| M2 人工验收 | UI 行为（Enter 分派 / Esc 语义 / Toast / Confirm 对话框 / 嵌套页 / `items_changed` 刷新）待用户真机确认（清单见 §4）；沙箱无法保活 GUI 进程，无法代劳 |
| M1 前置 | M1 人工验收已通过（2026-09-02，见 `m1-record.md` §4）；本层逻辑与框架无关，不受 R1 结论影响 |
| 协议细节 | `invoke` 的 `sender` 目前固定 `top_level`；嵌套页/上下文菜单的 `list_item`/`context_menu` 语义属 `InvokeContext` 扩展示例，待业务扩展接入 |
| 后续里程碑 | M2 关闭后即进入 **M3 缓存与懒加载**（frozen 桩 + 冷启动路径 + LRU warm 集合 + 桩复热，对应 A6/A7/A2 实测） |
