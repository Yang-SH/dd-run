# dd-gui 代码结构整理方案：业务代码 / UI 代码分离

> 版本：v2.1（2026-09-05；v2 方案已按批次执行完毕，执行记录见 §10）
> 目标：**纯物理重组**，把 `crates/dd-gui/src/main.rs`（**5123 行**）拆成
> 「业务编排层 `app/`」+「绘制层 `ui/`」+「系统副作用 `platform.rs`」+「纯函数 `text.rs`」。
> 硬约束：**对原有业务逻辑与 UI 界面逻辑零影响**（行为逐字节等价）。

---

## 0. 与 v1 的差异（代码已变动）

代码自 v1 起草后已变动（`eab530b` 提交 + 未提交的 main.rs/Cargo.toml 改动），下表是实测对照。
**结论：方案结构不变，仅「源行号」「字段数」「方法数」「模块数」需校正；方法体、字段含义、调用顺序一律未变。**

| 项 | v1（初版） | v2（本次更新，实测当前工作副本） |
|---|---|---|
| main.rs 行数 | 5017 | **5123**（+106） |
| dd-gui 总行数 | 8442 | **9004** |
| main.rs 占 crate 比例 | 59.4% | **56.9%** |
| 既有模块数 | 12 | **13**（新增 `tray.rs`，设计稿 10C 系统托盘） |
| dd-gui 触及 `egui` 文件 | 9 | **10**（新增 `tray.rs`） |
| `PaletteApp` 字段数 | 45（误数） | **35**（实测；含 3 个新增字段） |
| `impl PaletteApp` 方法数 | 51 | **56**（新增 `poll_tray`、`reopen_ctx_menu_at`） |
| 新增字段 | — | `tray_events`（托盘事件）、`icon_failed`（图标读盘负缓存）、`ctx_row_rects`（右键菜单命中行矩形） |
| 新增方法 | — | `poll_tray`（托盘轮询 → `app/lifecycle.rs`）、`reopen_ctx_menu_at`（右键菜单重开定位 → `app/ctx_menu.rs`） |
| 测试总数 | 219 | **221**（+2：新增 `tray.rs` 使 dd-gui lib 76→81；dd-ext system 8→5） |

---

## 1. 现状：问题定位

### 1.1 代码分布（实测）

| crate | 总行数 | 触及 `egui/eframe` 的文件数 | 结论 |
|---|---:|---:|---|
| **dd-gui** | **9004** | 10 | ⚠️ 唯一需要整治的 crate |
| dd-host | 2347 | **0** | ✅ 已是纯逻辑层 |
| dd-protocol | 626 | **0** | ✅ 已是纯协议层 |
| dd-ext | 3321 | **0** | ✅ 已是纯扩展业务（进程隔离，天然无 UI） |
| dd-run-cli | 323 | **0** | ✅ 无 UI |

**dd-gui 内部，`main.rs` 独占 5123 行（crate 的 56.9%）**，其余 **13 个模块**共 3881 行且分层合理
（`state` / `navigation` / `result` / `theme` / `fuzzy` / `robustness` / `aggregator` / `fallback` /
`hotkey` / `settings` / `embedded` / `tray` / `lib.rs` 入口）。

> 结论：**分层意识早已存在，问题不在"分层缺失"，而在"main.rs 一个文件把两层焊死在一起"**。
> 本次是「把既有分层贯彻到底」，不是推翻重来——这是能做到零影响的前提。

### 1.2 main.rs 内部构成（5123 行，实测）

| 行区间 | 内容 | 归属 | 行数 |
|---|---|---|---:|
| 1–53 | 文件头文档 + `use` | — | 53 |
| 54–97 | 常量（`APP_W` … `LRU_WARM_CAPACITY`）+ `toast_duration_ms` | 业务为主 | 44 |
| 98–248 | 13 个数据结构（`AggregatePayload` / `InvokeOutcome` / `PageOutcome` / `FallbackFetchOutcome` / `ToastKind` / `ToastState` / `ConfirmDialog` / `CtxAction` / `CtxEntry` / `CtxRow` / `CtxMenuState` / `RefreshState`） | 业务为主 | 151 |
| 250–310 | `main()` + `spawn_aggregation()` | 入口 + 业务 | 61 |
| 356–460 | `setup_cjk_fonts()` | 系统副作用 | 105 |
| 461–511 | `IconView` / `icon_is_dark()` / `decode_icon_image()` | UI 绘制 | 51 |
| **512–598** | **`struct PaletteApp`（35 个字段）** | 业务状态 | 87 |
| **599–3240** | **`impl PaletteApp`（56 个方法）** | **业务 + UI 混杂** | **2642** |
| 3241–3881 | 自由函数：`context_menu_rows` / `run_as_admin` / `reveal_in_folder` / 绘制原语 … | 混杂 | 641 |
| 3882–3949 | `impl eframe::App`（`logic` / `ui` 调度表） | 入口 | 68 |
| 3950–4484 | `KeyGroup` + 页脚/键帽/卡片绘制原语 | UI 绘制 | 535 |
| 4485–5123 | `#[cfg(test)] mod tests`（27 个用例） | 测试 | 639 |

### 1.3 混杂的具体证据（`impl PaletteApp` 内，按实测行号）

| 性质 | 方法 | 起始行 |
|---|---|---:|
| 业务 | `poll_aggregate` `poll_invoke` `poll_page` `poll_notifications` `poll_host_requests` `tick_refresh` | 824 / 894 / 950 / 1041 / 1081 / 1157 |
| 业务 | `refresh_health` `record_crash` `reset_crash` `is_crash_tripped` | 1188 / 1220 / 1248 / 1258 |
| 业务 | `dispatch_invoke` `start_invoke` `start_invoke_reheat` `confirm_selected` | 1427 / 1457 / 1484 / 1269 |
| 业务 | `open_page` `dispatch_fetch_page` `fetch_page_warm` `fetch_page_reheat` | 1532 / 1547 / 1580 / 1608 |
| 业务 | `find_ext` `store_warm_process` `evict_warm` `drop_source_to_stub` `mark_source_warm` | 1288 / 1661 / 1672 / 1694 / 1709 |
| 业务 | `sync_fallback` `start_fallback_fetch_chain` `poll_fallback` `rerender_fallback` | 1297 / 1317 / 1359 / 1412 |
| 业务 | `execute_host_request` `apply_action` `handle_keys` | 1101 / 1724 / 1795 |
| 窗口 | `show` `hide` `dismiss` `hide_keep_state` `poll_hotkey` `poll_tray` `send_center_on_cursor`(双 cfg) `handle_focus_loss` | 648 / 671 / 683 / 692 / 698 / 713 / 754,801 / 808 |
| **UI** | `draw_panel` `draw_status_footer` `draw_settings` `draw_list` `draw_toast` `draw_confirm` `draw_context_menu` `resolve_icons` | 1923 / 2070 / 2210 / 2509 / 2706 / 2755 / 3036 / 2437 |

业务方法与绘制方法在同一 `impl` 块内交错排列，是维护风险的主要来源。

---

## 2. 目标结构（推荐方案）

```
crates/dd-gui/src/
├── lib.rs                  # 追加：pub mod app; pub mod ui; pub mod platform; pub mod text;
├── main.rs                 # 5123 → ~55 行：仅 main()（字体 / 视口选项 / eframe 启动）
│
├── app/                    # ★ 业务编排层：状态机、进程池、协议时序。不产出像素
│   ├── mod.rs              # PaletteApp 结构体（35 字段）+ new() + eframe::App impl（调度表）
│   ├── lifecycle.rs        # show/hide/dismiss/hide_keep_state/poll_hotkey/poll_tray/handle_focus_loss
│   ├── aggregate.rs        # AggregatePayload / spawn_aggregation / poll_aggregate
│   ├── invoke.rs           # InvokeOutcome / dispatch_invoke / start_invoke(_reheat) / poll_invoke / confirm_selected
│   ├── page.rs             # PageOutcome / open_page / dispatch_fetch_page / fetch_page_* / poll_page
│   ├── pool.rs             # find_ext / store_warm_process / evict_warm / drop_source_to_stub / mark_source_warm（LRU）
│   ├── health.rs           # refresh_health / record_crash / reset_crash / is_crash_tripped
│   ├── fallback_flow.rs    # FallbackFetchOutcome / sync_fallback / start_fallback_fetch_chain / poll_fallback / rerender_fallback
│   ├── host_actions.rs     # poll_host_requests / execute_host_request / apply_action
│   ├── refresh.rs          # RefreshState / poll_notifications / tick_refresh
│   ├── toast.rs            # ToastKind / ToastState / ConfirmDialog / toast_duration_ms / show_toast*
│   ├── keys.rs             # handle_keys
│   └── ctx_menu.rs         # CtxAction / CtxEntry / CtxRow / CtxMenuState / open_ctx_menu / reopen_ctx_menu_at / activate_ctx_menu / context_menu_rows
│
├── ui/                     # ★ 绘制层：入参 (&mut Ui, theme::Palette, 数据) → 像素
│   ├── mod.rs
│   ├── panel.rs            # draw_panel / draw_status_footer / draw_list / draw_searchbar
│   ├── settings_view.rs    # draw_settings + 设置卡片族
│   ├── toast.rs            # draw_toast
│   ├── confirm.rs          # draw_confirm
│   ├── context_menu.rs     # draw_context_menu
│   ├── icons.rs            # IconView / icon_is_dark / decode_icon_image / resolve_icons / draw_icon_cell
│   ├── states.rs           # draw_empty_state / draw_loading_state / skeleton_fractions / lerp_color / shimmer_color / weak_text_color
│   ├── row.rs              # draw_item_row
│   └── widgets.rs          # KeyGroup / KEY_GROUPS / footer_action_text / 宽度测算 / keycap / paint_keys / 齿轮 / 按钮 / chip
│
├── platform.rs             # ★ 系统副作用：send_center_on_cursor(双 cfg) / run_as_admin(双 cfg) / reveal_in_folder(双 cfg) / setup_cjk_fonts
├── text.rs                 # ★ 纯函数（零 egui 依赖，易测）：path_like / url_like / CTX_GLYPH_* / default_action_glyph / nested_search_placeholder
│
└── （既有 13 个模块 state/navigation/result/theme/fuzzy/robustness/aggregator/fallback/hotkey/settings/embedded/tray —— 保持不动）
```

### 2.1 命名冲突处置

| 冲突 | 处置 |
|---|---|
| 既有 `dd_gui::settings`（设置持久化） vs 新绘制模块 | 新模块命名 **`ui::settings_view`** |
| 既有 `dd_gui::fallback`（`FallbackStore` 逻辑） vs 新编排模块 | 新模块命名 **`app::fallback_flow`** |
| 既有 `dd_gui::theme`（token 源） | 不变，`ui/` 唯一取色来源 |
| 既有 `dd_gui::tray`（系统托盘，设计稿 10C） | 不变，**本方案不触碰**；main.rs 仅消费其 `TrayEvent` |

### 2.2 不变项（关键）

- ✅ `Cargo.toml` **不需要改**：`[[bin]] name="dd-run" path="src/main.rs"` 路径保持有效。
- ✅ `build.rs` / `embedded.rs`（内嵌 5 个扩展 exe）**完全不触碰**。
- ✅ 既有 13 个模块（含新增 `tray.rs`）**一个字不改**。
- ✅ 协议 v1.0 冻结不变，零协议字段新增/变更。
- ✅ dd-host / dd-protocol / dd-ext / dd-run-cli **完全不触碰**。

---

## 3. 三档方案对比

| | 方案 0（保守） | **方案 1（推荐）** | 方案 2（激进） |
|---|---|---|---|
| 做法 | 只搬自由函数，`impl PaletteApp` 留在 main.rs | 完整分层到 `app/` + `ui/` + `platform` + `text` | 抽 `PaletteCore`（零 egui 依赖）+ UI 壳用消息通信 |
| main.rs 剩余 | ~3900 行 | **~55 行** | ~55 行 |
| 业务/UI 边界 | 部分 | **清晰（模块级强制）** | 极清晰（编译期强制） |
| 字段可见性改动 | 无 | **需 35 处 `pub(crate)`** | 需所有权重构 |
| 行为变更风险 | 极低 | **低（可机械验证）** | **高（触碰业务逻辑）** |
| 是否违反"零影响"约束 | 否 | **否** | **是** |
| 建议 | 兜底 | **本次采用** | 列为远期项，见 §8 |

**推荐方案 1 的理由**：它是唯一能在「不触碰任何业务逻辑」前提下达成模块级强制边界的档位。
方案 2 需要重写状态所有权与消息流，必然改动业务逻辑，与本次硬约束冲突。

---

## 4. 为什么方案 1 能做到「零行为影响」

### 4.1 Rust 允许同一类型的 `impl` 块跨模块分布

`impl PaletteApp { … }` 可以物理拆到 `app/lifecycle.rs`、`ui/panel.rs` 等多个文件里，
**只要它们在同一个 crate 内**。Rust 的方法解析在**编译期**完成、不存在重载与动态分发，
因此「方法体与调用点一字不改、仅改变它们在磁盘上的位置」在语义上是恒等变换。

> 这是本方案**不把绘制方法改成自由函数**的原因：改成自由函数就要改 60+ 处调用点，
> 那才是真正的风险来源。保持 `impl` 块 = 调用点零改动。

### 4.2 唯一的结构性改动是可见性（`pub(crate)`）

`struct PaletteApp` 的 35 个字段目前是私有的（仅 main.rs 可见）。
拆分后 `app/lifecycle.rs`、`ui/panel.rs` 需要读写这些字段 → 字段加 `pub(crate)`。

- 性质：**纯可见性标注，编译器保证不改变内存布局与运行时语义**。
- 范围：35 处，机械可枚举，可 100% 复核。
- ✅ 无 `repr`、无 FFI、无 unsafe 依赖字段私有性 → 无副作用。
- 新增字段 `tray_events` / `icon_failed` / `ctx_row_rects` 同样随结构体搬到 `app/mod.rs`，不新增任何可见性负担。

### 4.3 类型定义在 lib，bin 只留 `main()`

`PaletteApp` 移入 `dd_gui::app`（lib crate），`main.rs` 通过 `use dd_gui::app::PaletteApp;` 引用。
bin 与 lib 同属一个 package，编译产物与运行时行为一致。

### 4.4 类型系统兜底

任何漏搬、错搬、签名错位都会导致**编译失败**而非运行时偏差；
任何逻辑改动都无法被"移动"掩盖——因为方法体是原样搬运，diff 可逐行审计。

---

## 5. 精确切割表（按当前 main.rs 实测行号）

> 搬运单位 = 完整函数/常量/结构体定义块（含其文档注释）。**函数体一字不改**。

### 批次 1：`text.rs`（纯函数，零 egui 依赖）

| 源行 | 符号 | 目标 |
|---|---|---|
| 3339 | `path_like` | `text.rs` |
| 3351 | `url_like` | `text.rs` |
| 3241–3246 | `CTX_GLYPH_OPEN/ADMIN/FOLDER/COPY/PLAY/LINK` | `text.rs` |
| 3323 | `default_action_glyph` | `text.rs` |
| 3791 | `nested_search_placeholder` | `text.rs` |
| 88 | `toast_duration_ms` | `app/toast.rs` |

### 批次 2：`platform.rs`（系统副作用）

| 源行 | 符号 | 目标 |
|---|---|---|
| 356 | `setup_cjk_fonts` | `platform.rs` |
| 754 / 801 | `send_center_on_cursor`（`#[cfg(windows)]` + `#[cfg(not(windows))]` **成对搬**，cfg 行 753/800） | `platform.rs` |
| 3359 / 3391 | `run_as_admin`（双 cfg 成对） | `platform.rs` |
| 3397 / 3406 | `reveal_in_folder`（双 cfg 成对） | `platform.rs` |
| 3415 | `invoke_on` | `app/pool.rs` |
| 3420 | `get_items_on` | `app/pool.rs` |

### 批次 3：`ui/` 绘制原语（自由函数部分）

| 源行 | 符号 | 目标 |
|---|---|---|
| 461 / 479 / 500 | `IconView` / `icon_is_dark` / `decode_icon_image` | `ui/icons.rs` |
| 3441 | `weak_text_color` | `ui/states.rs` |
| 3466 | `draw_empty_state` | `ui/states.rs` |
| 3487 / 3494 / 3503 / 3516 | `skeleton_fractions` / `lerp_color` / `shimmer_color` / `draw_loading_state` | `ui/states.rs` |
| 3581 | `draw_item_row` | `ui/row.rs` |
| 3740 | `draw_icon_cell` | `ui/icons.rs` |
| 3799 | `draw_searchbar` | `ui/panel.rs` |
| 3950 / 3956 | `KeyGroup` / `KEY_GROUPS` | `ui/widgets.rs` |
| 3977 | `footer_action_text` | `ui/widgets.rs` |
| 3997 / 4005 / 4012 / 4025 | `text_width` / `keycap_width` / `key_group_width` / `keys_width` | `ui/widgets.rs` |
| 4036 | `draw_settings_gear` | `ui/widgets.rs` |
| 4063 | `draw_dialog_button` | `ui/confirm.rs` |
| 4110 | `draw_back_btn` | `ui/widgets.rs` |
| 4132 / 4156 | `draw_ext_chip` / `draw_version_chip` | `ui/settings_view.rs` |
| 4164 | `draw_settings_card_frame` | `ui/settings_view.rs` |
| 4193 / 4208 | `accent_soft` / `draw_radio_card` | `ui/settings_view.rs` |
| 4305 / 4354 / 4382 | `draw_setting_row_disabled` / `draw_soon_chip_at` / `draw_disabled_switch_at` | `ui/settings_view.rs` |
| 4405 / 4419 / 4456 | `PlaceholderSuffix` / `paint_keys_at` / `paint_keycap` | `ui/widgets.rs` |
| 3249 | `ctx_entry_count` | `ui/context_menu.rs` |
| 3270 | `context_menu_rows` | `app/ctx_menu.rs` |

### 批次 4：`app/` 骨架 + 数据结构

| 源行 | 符号 | 目标 |
|---|---|---|
| **512–598** | **`struct PaletteApp` + 35 字段加 `pub(crate)`** | `app/mod.rs` |
| 600 | `new()` | `app/mod.rs` |
| 98 / 112 / 123 / 134 | `AggregatePayload` / `InvokeOutcome` / `PageOutcome` / `FallbackFetchOutcome` | `app/aggregate.rs` / `invoke.rs` / `page.rs` / `fallback_flow.rs` |
| 151 / 160 / 181 | `ToastKind` / `impl ToastKind` / `ToastState` | `app/toast.rs` |
| 188 | `ConfirmDialog` | `app/toast.rs` |
| 203 / 216 / 225 / 231 | `CtxAction` / `CtxEntry` / `CtxRow` / `CtxMenuState` | `app/ctx_menu.rs` |
| 245 | `RefreshState` | `app/refresh.rs` |
| 54–97 | 尺寸/图标/刷新常量（`APP_W` … `LRU_WARM_CAPACITY`） | 按使用者分派到 `ui/` 与 `app/` |

### 批次 5：`impl PaletteApp` 业务方法拆分

| 源行区间 | 方法 | 目标文件 |
|---|---|---|
| 648–707 | `show` `hide` `dismiss` `hide_keep_state` `poll_hotkey` `poll_tray` `handle_focus_loss` | `app/lifecycle.rs` |
| 824–893 | `poll_aggregate` + `spawn_aggregation`(311) | `app/aggregate.rs` |
| 894–949 | `poll_invoke` | `app/invoke.rs` |
| 950–1040 | `poll_page` | `app/page.rs` |
| 1041–1080 | `poll_notifications` | `app/refresh.rs` |
| 1081–1156 | `poll_host_requests` `execute_host_request` | `app/host_actions.rs` |
| 1157–1187 | `tick_refresh` | `app/refresh.rs` |
| 1188–1268 | `refresh_health` `record_crash` `reset_crash` `is_crash_tripped` | `app/health.rs` |
| 1269–1287 | `confirm_selected` | `app/invoke.rs` |
| 1288–1296 | `find_ext` | `app/pool.rs` |
| 1297–1411 | `sync_fallback` `start_fallback_fetch_chain` `poll_fallback` `rerender_fallback` | `app/fallback_flow.rs` |
| 1427–1531 | `dispatch_invoke` `start_invoke` `start_invoke_reheat` | `app/invoke.rs` |
| 1532–1660 | `open_page` `dispatch_fetch_page` `fetch_page_warm` `fetch_page_reheat` | `app/page.rs` |
| 1661–1723 | `store_warm_process` `evict_warm` `drop_source_to_stub` `mark_source_warm` | `app/pool.rs` |
| 1724–1794 | `apply_action` | `app/host_actions.rs` |
| 1764–1794 | `show_toast` `show_error_toast` `show_toast_kind` | `app/toast.rs` |
| 1795–1910 | `handle_keys` `open_settings` `apply_theme_pref` `apply_open_view` | `app/keys.rs` / `app/lifecycle.rs` |

### 批次 6：`impl PaletteApp` 绘制方法拆分

| 源行区间 | 方法 | 目标文件 |
|---|---|---|
| 1923–2070 | `draw_panel` | `ui/panel.rs` |
| 2070–2210 | `draw_status_footer` | `ui/panel.rs` |
| 2210–2437 | `draw_settings` | `ui/settings_view.rs` |
| 2437–2509 | `resolve_icons` | `ui/icons.rs` |
| 2509–2706 | `draw_list` | `ui/panel.rs` |
| 2706–2755 | `draw_toast` | `ui/toast.rs` |
| 2755–2918 | `draw_confirm` | `ui/confirm.rs` |
| 2918–3036 | `open_ctx_menu` `reopen_ctx_menu_at` `activate_ctx_menu` | `app/ctx_menu.rs` |
| 3036–3240 | `draw_context_menu` | `ui/context_menu.rs` |

### 批次 7：入口与测试收尾

| 源行 | 内容 | 目标 |
|---|---|---|
| 3882–3949 | `impl eframe::App`（`logic` + `ui` **调用序列一字不改**） | `app/mod.rs` |
| 250–310 | `main()` | `main.rs`（保留） |
| 4485–5123 | 27 个测试用例 | 按被测目标分派到对应模块 |

#### 27 个测试用例迁移映射

| 测试（简名） | 数量 | 目标 |
|---|---:|---|
| `nested_placeholder` `skeleton_fractions` `shimmer_color` | 3 | `text.rs` / `ui/states.rs` |
| `footer_action_*`（3 个）`ctx_menu_*`（6 个）`path_like` `url_like` `default_action_glyph` | 11 | `text.rs` / `ui/widgets.rs` |
| `toast_duration`（2 个） | 2 | `app/toast.rs` |
| `icon_darkness` `decode_icon_image`（3 个） | 3 | `ui/icons.rs` |
| `sync_fallback_*`（2）`rerender_fallback` `fallback_fetch_chain` | 4 | `app/fallback_flow.rs` |
| `refresh_health_drops` `consecutive_crashes` `poll_invoke_on_dead`（+辅助 `dying_ext` `dying_process` `make_app` `ctx`） | 3 | `app/health.rs` / `app/invoke.rs` |
| **合计** | **27** | 数量必须守恒（main.rs 测试数本轮仍为 27，不变） |

---

## 6. 分批次执行计划

> 每批次独立可编译、可回滚；任一批次失败不影响已完成的批次。

| 批次 | 内容 | 预估行数 | 门限 |
|---|---|---:|---|
| **0** | 建立基线（已执行 ✅） | — | 记录 fmt/clippy/test 基线 |
| **1** | `text.rs` + `toast_duration_ms` | ~120 | `cargo check` + `test` 通过 |
| **2** | `platform.rs` + `invoke_on`/`get_items_on` | ~200 | 同上 + **双 cfg 分支均编译** |
| **3** | `ui/` 绘制原语（9 个文件） | ~1100 | 同上 |
| **4** | `app/mod.rs` 骨架：结构体 + 35 处 `pub(crate)` + 数据结构归位 | ~350 | 同上 + 字段清单复核 |
| **5** | `app/` 业务方法（11 个文件） | ~1500 | 同上 |
| **6** | `ui/` 绘制方法（6 个文件） | ~1200 | 同上 |
| **7** | `eframe::App` impl + 27 测试迁移 + `main.rs` 瘦身 | ~700 | 同上 + **测试数守恒** |
| **8** | 全量验收 | — | fmt/clippy/test 全绿 + 真机清单 |

**每批次操作顺序（固定套路）**：
1. 创建目标文件，写入模块头文档注释 + 所需 `use`；
2. 从 main.rs **剪切**完整定义块（含文档注释），**原样粘贴**（不改函数体）；
3. 按需加 `pub(crate)` / `pub` 可见性；
4. `cargo check -p dd-gui` 编译；
5. `cargo test -p dd-gui`；
6. 通过后才进入下一批次。

---

## 7. 验收标准

### 7.1 基线（批次 0，实测于本轮代码变动后的当前工作副本 2026-09-05）

| 项 | 基线值 | 状态 |
|---|---|---|
| `cargo fmt --all --check` | **exit 0，全绿** | ✅ |
| `cargo clippy --workspace --all-targets` | **exit 0，零警告**（强制重编译复核） | ✅ |
| `cargo test --workspace` | **221 passed / 0 failed** | ✅ |

> 与 v1 基线（219）相比：本轮 +2。变化来源——新增 `tray.rs`（设计稿 10C）使 dd-gui lib 测试
> 76→81（+5）；dd-ext `system` bin 测试 8→5（-3）；其余目标不变。验收要求仍是 **0 failed**，
> 迁移后总数守恒（221）。

分目标明细（迁移后必须逐项一致）：

| 目标 | 用例数 | | 目标 | 用例数 |
|---|---:|---|---|---:|
| dd-ext lib | 16 | | dd-gui lib | 81 |
| dd-ext apps | 10 | | **dd-gui bin（dd-run）** | **27** |
| dd-ext calc | 9 | | dd-host lib | 37 |
| dd-ext shell | 4 | | roundtrip | 8 |
| dd-ext system | 5 | | roundtrip_builtins | 6 |
| dd-ext websearch | 7 | | dd-protocol lib | 8 |
| dd-ext-sample | 0 | | consistency | 3 |
| dd-run-cli | 0 | | **合计** | **221** |

### 7.2 逐批次门限

| # | 门限 | 判定 |
|---|---|---|
| V1 | `cargo check -p dd-gui --all-targets` 零错误零新警告 | 必过 |
| V2 | `cargo test --workspace` = **0 failed** | 必过 |
| V3 | `cargo fmt --all --check` exit 0 | 必过 |
| V4 | `cargo clippy --workspace --all-targets` **零警告**（基线即零） | 必过 |
| V5 | 符号集合守恒：迁移前后 `main.rs` + 新模块的 `fn` 名集合完全一致（含新增 `poll_tray`/`reopen_ctx_menu_at`） | 必过 |
| V6 | 逻辑行守恒：`git diff --stat` 增删行数基本平衡（纯移动特征） | 必过 |
| V7 | `Cargo.toml` / `build.rs` / `embedded.rs` / `tray.rs` / dd-host / dd-protocol / dd-ext / dd-run-cli **零 diff** | 必过 |

### 7.3 终验真机清单（UI 行为无法被单测覆盖，必须人工过一遍）

| # | 验收项 | 期望 |
|---|---|---|
| R1 | 冷启动 | 无黑框、无跳动；窗口初始隐藏 |
| R2 | 热键 `Win+Alt+Space` 唤起 | 面板居中于光标所在屏；FilterBox 自动聚焦 |
| R3 | 首屏 | 聚合加载骨架 → 数据就绪；失败扩展不阻塞整体 |
| R4 | 键盘导航 | `↑↓` / `Tab` / `Shift+Tab` 循环；`Enter` 执行；`Esc` 非 Root 返回、Root 隐藏 |
| R5 | 鼠标/键盘互不干扰 | 静止鼠标不抢键盘选中；鼠标主动移动才接管 |
| R6 | 五类扩展执行 | 应用 / calc / shell / system / websearch 各验一次 |
| R7 | 嵌套页 | 进入 / 返回 / `GoHome`；空查询占位符带页标题 |
| R8 | 右键菜单（设计稿 10B） | 各类别菜单项正确；以管理员运行、打开所在文件夹、复制链接可用；`Shift+F10` 可唤出 |
| R9 | 托盘（设计稿 10C，新增） | 常驻图标、左键 toggle、右键原生菜单 4 项、退出入口可用 |
| R10 | Toast | 普通 2s、错误固定 3s |
| R11 | 确认对话框 | `Confirm` 结果弹窗，确认后带 `confirmed=true` 重发 |
| R12 | 设置页 | 主题三选生效；首屏视图切换；窗口尺寸 `560×460` ↔ `560×640` 切换 |
| R13 | 失焦隐藏 | 隐藏后复位查询与选中；`Hide`（保留状态）不复位 |
| R14 | 崩溃熔断 | 连续崩溃 N 次 → 显示"暂时不可用"，恢复后清零 |

### 7.4 验收命令（windows-gnu 工具链 + APPDATA 必需）

```bash
cd /g/AI/dd-run
export APPDATA='C:\Users\y7398\AppData\Roaming'   # 缺失会让图标缓存测试假阳性失败

cargo +stable-x86_64-pc-windows-gnu fmt --all --check
cargo +stable-x86_64-pc-windows-gnu clippy --workspace --all-targets
cargo +stable-x86_64-pc-windows-gnu test --workspace
```

> 注：跑 clippy 前若需强制重编译（避免缓存命中导致假绿），先执行
> `find crates -name "*.rs" -exec touch {} +`。

---

## 8. 本次明确不做（防止顺手重构引入回归）

| # | 不做的事 | 原因 |
|---|---|---|
| N1 | 不修改任何函数体逻辑 | 本批次是纯移动 |
| N2 | 不修改任何字段类型 / 结构体定义（仅加 `pub(crate)` 可见性） | 包括新增字段 `tray_events`/`icon_failed`/`ctx_row_rects` 原样搬 |
| N3 | 不修改任何绘制参数（尺寸/颜色/圆角/间距） | 会改变视觉，属设计稿范畴 |
| N4 | 不修改任何常量数值 | 同上 |
| N5 | 不调整 `impl eframe::App` 内 `logic()`/`ui()` 的调用顺序 | 轮询时序与绘制顺序是行为相关项 |
| N6 | 不重命名任何方法 / 字段 | 保持 diff 可审计 |
| N7 | 不新增 / 修改任何测试用例内容（只搬家） | 保证测试语义不变 |
| N8 | 不引入新依赖、不改 `Cargo.toml` | 保持构建产物一致 |
| N9 | 不做方案 2（`PaletteCore` 抽离） | 触碰业务逻辑，违反本次硬约束 |
| N10 | 不处理 dd-host / dd-ext / tray.rs 的内部文件尺寸 | 与"业务/UI 拆分"无关，且已零 egui 耦合或已独立分层 |

---

## 9. 待确认决策

| # | 决策点 | 选项 |
|---|---|---|
| Q1 | 拆分力度 | **方案 1（推荐，main.rs → ~55 行）** / 方案 0（保守，→ ~3900 行） |
| Q2 | `PaletteApp` 落点 | **`dd_gui::app`（lib，可单测、main.rs 最薄）** / bin 内模块（改动更小但 main.rs 仍需持有 mod 声明） |
| Q3 | 交付节奏 | **8 批次逐批验证后交付** / 一次性全量交付 |
| Q4 | 方案 2（远期） | 是否列入后续里程碑（预计需独立设计稿 + 独立验证周期） |

---

## 10. 执行记录（2026-09-05，方案 1 已实施）

### 10.1 结果总览

| 项 | 执行前 | 执行后 |
|---|---|---|
| main.rs | 5123 行（混杂业务+绘制） | **103 行**（仅文件头 + `main()`） |
| dd-gui 模块 | 13 个既有模块 | 13 个既有模块（零改动）+ `app/`(13 文件) + `ui/`(10 文件) + `platform.rs` + `text.rs` + `test_support.rs` |
| 交付 | — | fmt / clippy / test **三项全绿，221 passed / 0 failed** |

### 10.2 门限核验（V1–V7）

| 门限 | 结果 |
|---|---|
| V1 `cargo check -p dd-gui --all-targets` | ✅ exit 0，零错误零警告 |
| V2 `cargo test --workspace` | ✅ **221 passed / 0 failed**（总数守恒） |
| V3 `cargo fmt --all --check` | ✅ exit 0 |
| V4 `cargo clippy --workspace --all-targets`（强制重编译） | ✅ 0 警告 |
| V5 符号集合守恒 | ✅ 脚本审计 **181 = 181**（缺失 0 / 多余 0）；另抽查 `poll_invoke` `poll_page` `draw_panel` `draw_settings` `draw_status_footer` `apply_action` `setup_cjk_fonts` 与 HEAD 版本**逐字一致** |
| V6 逻辑行守恒 | ✅ 纯移动特征：HEAD main.rs 5017 行 + 用户未提交增量 → 全量分布于新文件；main.rs 仅留入口 |
| V7 禁改文件零额外 diff | ✅ Cargo.toml / dd-gui/Cargo.toml / embedded.rs / theme.rs / build.rs / tray.rs / dd-ext / dd-host 的 diff 与执行前锚点逐项一致（lib.rs 仅追加 mod 声明 + `extern crate self as dd_gui;`，属方案允许项） |

### 10.3 与 v2 方案的偏差（3 处，均为落点微调）

| # | 偏差 | 原因 |
|---|---|---|
| 1 | `ctx_entry_count` → `app/ctx_menu.rs`（方案写 ui/context_menu.rs） | 唯一调用方是 `handle_keys` 与 ctx 测试，无 UI 调用者 |
| 2 | `footer_action_text` → `text.rs`（方案写 ui/widgets.rs） | 零 egui 纯映射，被 app（context_menu_rows）与 ui（draw_status_footer）双侧使用，放 text.rs 避免 app→ui 反向依赖 |
| 3 | lib.rs 增加一行 `extern crate self as dd_gui;` | 使搬入 lib 的代码沿用 `dd_gui::` 全路径**一字不改** |

### 10.4 测试分布变化（总数守恒，分目标计数改变）

原 27 个 bin（dd-run）测试按 §5 映射表迁入 lib 模块（`test_support.rs` 承载共享夹具
`item_with` / `dying_ext` / `dying_process` / `make_app` / `ctx`，内容逐字未改）：

text.rs 7、ui/icons.rs 3、ui/states.rs 2、app/ctx_menu.rs 6、app/fallback_flow.rs 4、
app/health.rs 2、app/invoke.rs 1、app/toast.rs 2（合计 27 ✅）。

迁移后分目标计数：**dd-gui bin 27→0，dd-gui lib 81→108**，其余目标不变，总计 221 守恒。

### 10.5 可见性改动（纯标注，编译器保证语义不变）

- `struct PaletteApp` 35 个字段 → `pub(crate)`（方案 §4.2）；
- 数据结构（AggregatePayload / InvokeOutcome / PageOutcome / FallbackFetchOutcome /
  ToastState / ConfirmDialog / CtxEntry / CtxMenuState / RefreshState / KeyGroup）字段 → `pub(crate)`
  （跨模块构造/绘制所需，方案低估了这部分，同性质：纯可见性）；
- 全部 `impl PaletteApp` 固有方法 → `pub(crate)`；bin 需要的入口 → `pub`
  （`PaletteApp` / `new` / `spawn_aggregation` / `AggregatePayload` / `APP_W` / `APP_H` /
  `OFFSCREEN_X` / `OFFSCREEN_Y` / `setup_cjk_fonts`）。

### 10.6 待办

- 真机验收清单 R1–R14（§7.3）——编译与单测无法覆盖 UI 行为，需人工过一遍；
- 方案 2（`PaletteCore` 抽离）仍为远期项，未实施。
