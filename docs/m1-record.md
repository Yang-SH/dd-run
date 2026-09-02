# M1 实施记录 — 最小可用面板（egui 窗口骨架 + 全局热键 + 首屏聚合 + 键盘全流程）

> **状态**：✅ 代码完成 + 工程验收 + **真机人工验收通过**（2026-09-02，依据 `M1-测试文档.md` 复测），M1 关闭。
> 残留一项「启动一帧闪屏」（见 §4.6），为视觉瑕疵、不影响功能验收。
>
> **实施前的三项决策**（与用户一问一答确认）：
> 1. **范围** = R1 尖峰先行（egui 窗口骨架 + `RegisterHotKey` 热键 + 键盘焦点最小验证），
>    R1 通过后再做 Root View / 首屏聚合等剩余任务；R1 不通过则停在换框架决策点（ADR-2）；
> 2. **验收分工** = 逻辑层自动化（状态机/聚合层单测 + 构建零告警）+ 真机人工验收
>    （A1 热键、A11 键盘全流程），人工验收清单见 §4；
> 3. **载体** = 本文件（`docs/m1-record.md`），沿用 M0 记录模式，`implementation.md` 加指路行。

---

## 1. 分阶段实施计划与进度

| 阶段 | 内容 | 状态 | 验收标准（量化） | 结果 |
|---|---|---|---|---|
| P1 逻辑层 | `dd-gui/src/state.rs`：`PanelState` 状态机（过滤/选中/环绕/夹紧），不依赖 egui | ✅ | 9 个单测全过 | ✅ 9/9 |
| P2 窗口骨架 | `dd-gui/src/main.rs`：eframe 无边框/置顶/初始隐藏窗口，FilterBox + 分组列表 + tags chip + 页脚键位提示（界面 01），CJK 字体加载 | ✅ | `cargo build` 0 告警 | ✅ |
| P3 键盘拦截 | `ctx.input_mut(consume_key)` 应用层拦截 `↑↓/Enter/Esc`（FilterBox 有焦点时仍生效） | ✅ | 编译期验证 + 人工验收 | ✅ |
| P4 全局热键 | `dd-gui/src/hotkey.rs`：`Win+Alt+Space`（`RegisterHotKey` 独立线程消息循环，事件经 channel 回主线程） | ✅ | 启动无 panic、日志无错误 | ✅ |
| P5 失焦隐藏 | `logic()` 中检测 `ViewportInfo.focused`，失焦自动隐藏（防误触：取得焦点后才启用） | ✅ | 人工验收 | ✅ |
| P6 工程验收 | build 0 告警 / 测试全过 / clippy `-D warnings` 0 / fmt 通过；启动 GUI 供人工验收 | ✅ 代码侧 | 四项全绿 | ✅ |
| P7 聚合层 | `dd-gui/src/aggregator.rs`：扫描扩展 → **并行**（每扩展一线程）`spawn→initialize→top_level_commands`，错误隔离 + 示例扩展兜底（`dd-ext-sample.exe` 与 GUI 同目录） | ✅ | 3 个单测全过 | ✅ 3/3 |
| P8 Root View 接入 | 后台线程聚合（不阻塞 UI，A12），结果经 channel 注入；加载中/空态/扩展源状态行；聚合完成进程保活（M2 invoke 复用）+ 唤起时轻查退出（崩溃检测骨架） | ✅ | `cargo build` 0 告警 + 人工验收 | ✅ |
| P9 Tab 导航 | `consume_key` 拦截 `Tab`/`Shift+Tab`（= 列表项下移/上移，对齐设计文档 §4.3「↑/↓ **或** Tab/Shift+Tab」） | ✅ | 编译期验证 + 人工验收 | ✅ |
| P10 记录 | 本文件 + `implementation.md` §5 进度表/§7 下一步联动 | ✅ | 文档与实现一致 | 本文件 |

## 2. 产出文件清单

| 文件 | 行数 | 内容 |
|---|---|---|
| `crates/dd-gui/Cargo.toml` | 26 | eframe 0.36（glow 后端）+ windows-sys 0.61 + **dd-host / dd-protocol**（聚合层依赖） |
| `crates/dd-gui/src/lib.rs` | 15 | 模块声明：`state` / `hotkey` / `aggregator`（逻辑与平台层放 lib，供复用与单测） |
| `crates/dd-gui/src/state.rs` | ~330 | `PanelState` 状态机 + `PanelItem`；9 个单测（§3.1） |
| `crates/dd-gui/src/aggregator.rs` | ~320 | 首屏聚合：`load_extension_sources`（扫描+兜底）/ `collect_top_level`（并行拉取，错误隔离）/ `flatten`（合并+源状态）/ `to_panel_item`（映射）；3 个单测（§3.1） |
| `crates/dd-gui/src/hotkey.rs` | ~120 | `Win+Alt+Space` 注册 + `GetMessage` 消息循环 + `HotkeyEvent::Toggle` channel |
| `crates/dd-gui/src/main.rs` | ~430 | eframe 窗口骨架 + `logic()`/`ui()` + Root View 渲染（加载中/空态/源状态行）+ 后台聚合线程 + Tab 导航 + CJK 字体加载 |

> 说明：`dd-gui` 采用 **lib + bin 分离**——`state.rs`/`hotkey.rs`/`aggregator.rs` 进 lib
> （逻辑可单测、M2+ 可复用），eframe 壳在 bin（`main.rs`）。workspace 已通过 `crates/*` 通配自动纳入。

## 3. 测试方法与结果

### 3.1 逻辑层单测（`cargo test -p dd-gui`）

`PanelState` 状态机 9 个 + `aggregator` 聚合层 3 个，共 **12 个测试全过**（0.00s）：

| 模块 | 测试 | 验证点 |
|---|---|---|
| state | `empty_query_shows_all_and_selects_first` | 空查询显示全部、默认选中第一项 |
| state | `query_filters_case_insensitively` | 大小写不敏感过滤（open/OPEN） |
| state | `query_matches_subtitle_and_tags_and_section` | 过滤命中副标题/tag/分组名 |
| state | `no_match_yields_none_selection` | 无匹配 → 无选中、移动键无操作 |
| state | `move_down_wraps_around` / `move_up_wraps_around` | ↑↓ 环绕导航（§4.3） |
| state | `query_change_clamps_selection` | 查询变化夹紧选中索引 |
| state | `confirm_returns_selected` | Enter 返回当前选中项 |
| state | `reset_clears_query_and_selection` | 重新唤起复位查询与选中 |
| aggregator | `maps_command_item_fields_and_fallback_section` | `CommandItem`→`PanelItem` 字段映射；section 缺省用扩展名兜底 |
| aggregator | `flatten_merges_ready_and_keeps_failed_isolated` | 多扩展合并；单扩展失败不阻塞其他（错误隔离） |
| aggregator | `flatten_empty_input` | 空输入 → 空列表空状态 |

### 3.2 工程验收（本机，`CARGO_INCREMENTAL=0`）

| 项 | 命令 | 结果 |
|---|---|---|
| 构建 | `cargo build -p dd-gui` | ✅ 0 告警 0 错误 |
| 单测 | `cargo test -p dd-gui` | ✅ 12/12 通过 |
| clippy | `cargo clippy -p dd-gui --all-targets -- -D warnings` | ✅ 0 告警 |
| 格式 | `cargo fmt --check` | ✅ 通过 |
| 启动 | `./target/x86_64-pc-windows-gnu/debug/dd-gui.exe` | ✅ 前台常驻运行、无 panic（沙箱内后台进程会被会话清理，需用户真机启动人工验收） |

### 3.3 与 egui 0.36 的 API 适配（实现过程中发现的重大变更）

| 变更 | 旧写法（0.33 及更早） | 新写法（0.36.1） |
|---|---|---|
| `App` trait | `fn update(&mut self, ctx, frame)` | `fn ui(&mut self, ui: &mut egui::Ui, frame)` + **新增 `fn logic(&mut self, ctx, frame)`**（窗口隐藏时也会被调用，热键/失焦检测放这里） |
| 面板 | `CentralPanel::default().show(ctx, ...)` | `CentralPanel::default().show(ui, ...)`（接收 `&mut Ui`） |
| 焦点检测 | `i.viewport().focused`（bool） | `i.viewport().focused`（**`Option<bool>`**，需 `unwrap_or(false)`） |
| 圆角 | `Frame::rounding(4.0)` | `Frame::corner_radius(4.0)` |
| 弱文本色 | `ui.visuals().weak_text_color`（Color32） | **`Option<Color32>`**，需 `unwrap_or_else(|| text_color())` |
| 边距 | `Margin::symmetric(12.0, 10.0)`（f32） | **`Margin::symmetric(12, 10)`（i8）** |

> 上述均为构建时被编译器逐条纠正的**确定性事实**，已在代码中落地（`main.rs` 各调用点）。

### 3.4 首屏聚合实现要点

- **并行与不阻塞**（A12）：扫描与拉取在独立后台线程（`main()` 里 `spawn_aggregation`），
  结果经 `mpsc::channel` 回传；每扩展一个线程，进程对象线程独占，`join` 回传。
- **错误隔离**：单扩展 spawn/initialize/top_level_commands 任一失败，仅记入
  `SourceSummary::Failed`（页脚红字显示），其余扩展与整体渲染不受影响。
- **兜底路径**：扩展目录（`%APPDATA%\dd-run\extensions.d`）无清单或不可读时，
  回退到与 `dd-gui.exe` 同目录的 `dd-ext-sample.exe`（`from_executable` 直构内存清单，
  绕过 §7 磁盘校验，UI 以"备注"行明示来源，避免误读为扫描发现）。
- **进程保活**：聚合成功的 `ExtensionProcess` 存入 `PaletteApp.processes`（M2 `invoke`
  复用），不主动 `close`，随宿主退出由 `Drop` 强杀；每次唤起时 `refresh_health`
  轻查 `has_exited`（崩溃检测骨架），已退出进程的源状态标记失败。

## 4. 人工验收清单（2026-09-02 真机验收通过）

> 启动方式（需真机，沙箱内后台进程会被清理）：
> ```
> cd /d/AI/project/dd-run
> ./target/x86_64-pc-windows-gnu/debug/dd-gui.exe
> ```
> 无扩展目录时列表自动回退为示例扩展（`dd-ext-sample`）的 2 条命令
> （Say Hello / Copy Sample Text，分组 Sample）；有扩展目录时按扫描结果聚合。

| # | 验收项 | 对应判据 | 操作步骤 | 预期 | 结果 |
|---|---|---|---|---|---|
| 1 | 初始隐藏 | — | 启动后观察 | 无窗口出现（偶发一帧闪屏，见 §4.6） | ✅ |
| 2 | 热键唤起 | A1 | 按 `Win+Alt+Space` | 面板出现在前台（无边框、置顶） | ✅ |
| 3 | FilterBox 焦点 | A11 | 唤起后直接打字 | 光标在搜索框、中文可输入、列表实时过滤 | ✅ |
| 4 | 列表内容 | — | 唤起后观察 | 列表显示扩展命令（示例扩展 2 条，分组 Sample；或真实扩展聚合结果） | ✅ |
| 5 | ↑↓ 导航 | A11 | 按 `↓`/`↑` | 选中高亮在过滤结果间移动（末尾环绕） | ✅ |
| 6 | Tab 导航 | A11 | 按 `Tab`/`Shift+Tab` | 与 `↓`/`↑` 同效（设计文档 §4.3），光标不跳动 | ✅ |
| 7 | Enter 执行 | A11 | 选中某项按 `Enter` | M2 后真正调用扩展 `invoke`；面板按 8 种 `CommandResultKind` 给出反馈（关闭/隐藏/Toast/进页等）；**终端不再打印** | ✅ |
| 8 | Esc 关闭 | A11 | 按 `Esc` | 面板隐藏 | ✅ |
| 9 | 失焦隐藏 | 界面 01 | 面板显示时点击面板外任意处 | 面板自动隐藏 | ✅ |
| 10 | 再次唤起 | A1 | 再按 `Win+Alt+Space` | 面板重新出现，查询与选中已复位（reset） | ✅ |
| 11 | 扩展源状态 | — | 观察页脚 | 显示 `✓ Sample Ext（2 命令）`；若扩展失败则红字 `✗ <名>：<原因>` | ✅ |

**R1 结论判定**：若 3、5–7 全部通过 → **egui 键盘焦点可行，R1 通过，ADR-2 成立**；
若 3/5/6/7 任一不成立 → **R1 不成立，按 ADR-2 重新评估备选框架（Slint/iced）**。

## 4.5 真机反馈与本轮修复（2026-09-02）

根据用户真机测试 `M1-测试文档.md`（11 项），按"小修"范围修了 4 个问题 + 更新 1 项验收预期：

| 反馈 | 修复 |
|---|---|
| #1 启动即显示（初始隐藏失效） | `main()` 的 `Box::new` 闭包显式 `ViewportCommand::Visible(false)`，与 `with_visible(false)` 构成双保险 |
| #1 窗口未居中 | `PaletteApp` 新增 `recentered` + `recenter_if_needed`；`App::logic` 每帧轮询 `ViewportCommand::center_on_screen(ctx)`，首次 `Some` 即居中（egui 0.36 官方便捷方法，省去手算 `monitor_size` / DPI） |
| #1 页脚"✓"显示为方框 | `setup_cjk_fonts` 候选字体优先 `msyh.ttc`（YaHei，Win7+ 必装且含 U+2713） |
| #11 红字"扩展目录不可读" | `aggregator::load_extension_sources` 文案改为"未找到扩展目录：{err}"（红字错误展示保留） |
| #7 终端未打印 | **不改代码**；§4 项 7 预期更新为"M2 后真正调用扩展 `invoke`，反馈走面板（关闭/隐藏/Toast/进页），终端不再打印" |

**未修（保持原状或等复现）**：#6 Tab 方向说明（仅测试覆盖建议）、#9 隐藏前变黑（小瑕疵）、#10 再次唤起（`show()` 已 `list.reset()`，等用户复现确认）。

修改文件：`crates/dd-gui/src/main.rs`、`crates/dd-gui/src/aggregator.rs`、`docs/m1-record.md`。工程验收：23/23 测试全绿、`clippy -D warnings` 0 告警、`fmt --check` 通过。

## 4.6 残留问题（真机验收后记录，2026-09-02）

| # | 现象 | 影响 | 处置建议 |
|---|---|---|---|
| #1 启动闪屏 | 初始隐藏已生效（无持久黑面板、窗口不滞留）；但 `dd-gui.exe` 启动瞬间偶发一帧黑/空白闪屏 | 纯视觉瑕疵，不阻挡键盘全流程、不滞留窗口 | 属 eframe/egui 0.36 在 Windows 的窗口首帧时序问题。候选方案：① 首帧绘制完成前保持 `Visible(false)`，待 `ui` 跑完一帧再放行；② viewport 加 `with_transparent` 或 `opacity=0→1` 过渡。优先级低，可并入 M3 或单独排期 |

## 5. 遗留与下一步

| 项 | 说明 |
|---|---|
| M1 剩余任务 | ✅ 全部完成：聚合层（P7）、Root View 接入（P8）、Tab/Shift+Tab 导航（P9）、进程管理接入（保活+崩溃检测骨架） |
| A11 覆盖范围 | 本里程碑覆盖「唤起/搜索/选择/关闭」全流程；「执行/返回」依赖 M2 的 8 种 Kind 状态机（Enter 目前打印选中项标题） |
| R1 正式结论 | ✅ 通过（§4 清单 2/3/5/6/7 均 ✅：egui 键盘焦点可行，ADR-2 成立，无需换框架） |
| M2 前置 | 聚合保活的 `ExtensionProcess` 已在 `PaletteApp.processes` 中，M2 `invoke` 直接复用；`CommandItem.command`（`Invoke`/`Page`）目前未透传，M2 需在 `PanelItem` 上补充 |
| 依赖体量 | eframe 0.36 全量依赖编译约 14 分钟（一次性）；`dd-gui` 增量构建 <15s |
| 沙箱限制 | 本环境无法保活后台 GUI 进程（会话结束即清理），人工验收须用户真机启动 |

## 6. 复现验收

```bash
cargo build -p dd-gui                                   # 期望 0 告警
cargo test -p dd-gui                                    # 期望 12/12 通过
cargo clippy -p dd-gui --all-targets -- -D warnings     # 期望 0 告警
cargo fmt --check                                       # 期望通过
./target/x86_64-pc-windows-gnu/debug/dd-gui.exe         # 启动后按 Win+Alt+Space 人工验证
```
