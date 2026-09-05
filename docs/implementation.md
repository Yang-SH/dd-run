# dd-run 实施方案

> **状态**：M0 已完成（`dd-protocol` / `dd-host` / `dd-ext-sample` / `dd-run` CLI 均已落地，40/40 测试全绿）；M1（`dd-gui` 窗口骨架 + 热键 + 首屏聚合 + 键盘全流程）已关闭（2026-09-02 真机人工验收通过，A1/A11/A12 全过，见 [`./m1-record.md`](./m1-record.md)）；M2（命令执行 + 8 种 Kind 状态机 + 页面栈 + UI 接线）已关闭（2026-09-02 十项真机复验全部通过，A4/A5/A9 达成，24/24 单测 + 全 workspace 构建无回归，见 [`./m2-record.md`](./m2-record.md) §4.5）。M3（缓存与懒加载）逻辑层 `cache.rs` + 协议 `get_command` 接线 + UI 接线（冷启动读桩不拉起 / 桩复热 / LRU 保活 / A2 计时）已完成并通过工程验收（73/73 测试全绿）；**真机反馈 5 处修复已落地**（◌ 补 seguisym 字体后援、空态改 vertical_centered 不撑满页脚、#5 步骤改写、A2 拆计时定位瓶颈、列表长时页脚移至 `Panel::bottom` 独立底栏，见 [`./m3-record.md`](./m3-record.md) §3.4），**A6/A2 真机复验已通过（2026-09-02，见 [`./m3-record.md`](./m3-record.md) §4）**。M4（5 内置扩展与健壮性）已关闭（2026-09-04，commit `757f3b4`，见 [`./m4-record.md`](./m4-record.md)）。M5（ueli 风格 UI 重构，插队于 M4 后）批次 1–4.2 与六轮真机反馈修复完成（2026-09-04，commit `5cf32b7`，设计稿 v4.3）；剩余设计稿 C 组占位见 §6.1 L8。
> **单文件分发（2026-09-04 实施）**：`tools/package.sh` 产出单文件 `dist/dd-run-0.1.0.exe` —— 5 个内置扩展 exe 经 `dd-gui` build.rs + `assets/embed/` 内嵌进宿主，运行时由 `dd-gui::embedded` 物化到 `%APPDATA%/dd-run/cache/embedded/` 再 spawn（进程隔离 ADR-1 不变）。对外入口产物由 `dd-gui` 更名 `dd-run`；M0 CLI 更名 `dd-run-cli`。
> **关联**：[`protocol.md`](./protocol.md)（扩展协议 v1.0）· [`manifest-schema.md`](./manifest-schema.md)（清单 v1.0）· [`../cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md)（设计参考与验收标准 A1–A12）。

---

## 1. 目标与范围

本方案把设计文档中的抽象模型拆成**可依次交付的里程碑**。每个里程碑都有明确的完成判据，判据直接映射到设计文档 §10 的验收项 A1–A12。

**MVP 范围**（M0–M4 全部完成即 MVP）：

- 全局热键唤起的跨平台面板（Windows / macOS / Linux）
- 4 类页面中的 **ListPage + DetailPage**（设计文档 §4.5）
- 5 个内置扩展：Apps / Calc / System / WebSearch / Shell
- 扩展机制：**清单发现 + 子进程 JSON-RPC**，支持第三方扩展
- frozen / stub / LRU 懒加载

**明确不在 MVP**（见 README 非目标）：Windows 专属扩展（§7 中 9 个 `🪟` 项）、Gallery 商店、WASM 沙箱、进程注册发现、FormPage/MarkdownPage（按需追加，非阻塞）。

---

## 2. 里程碑

### M0 — 地基与协议冻结

**目标**：Cargo workspace 可构建，宿主能与一个示例扩展完成完整握手与一次命令拉取。

> **实施记录**：M0 已完成（2026-09-01）。分两步实施：第一步 workspace 脚手架 + `dd-protocol` 协议类型 + 协议一致性测试；第二步 `dd-host` 清单扫描/进程管理 + `dd-ext-sample` 示例扩展 + `dd-run` CLI + 全链路往返。过程、验收标准与测试结果（40/40 测试全绿 + CLI 实跑）见 [`./m0-record.md`](./m0-record.md)。

**为什么先做协议**：协议是宿主与扩展之间**唯一的硬契约**。它一旦变动，两侧代码都要改；先冻结再写业务，可避免返工。

| 任务 | 说明 |
|---|---|
| Cargo workspace | crates 建议：`dd-run`（宿主 bin）、`dd-protocol`（协议类型 + NDJSON 读写）、`dd-host`（宿主逻辑：进程管理 / 缓存 / 页面栈）、`dd-ext-sample`（示例扩展） |
| `dd-protocol` 数据模型 | `CommandItem` / `CommandRef` / `CommandResult`（**8 种 Kind**）/ `Page` / `Sender` / `Icon` / `Details`，字段对齐 [`protocol.md`](./protocol.md) §8 |
| NDJSON 编解码 | 增量缓冲按行切分（见 [`protocol.md`](./protocol.md) §2.4），单行上限 1 MiB |
| JSON-RPC 信封 | 请求 / 响应 / 通知 / 错误对象，含 §3.3 的 id 空间判别逻辑 |
| 错误码 | 标准 5 个 + 自定义 5 个（`-32001`…`-32005`） |
| 示例扩展 | 响应 `initialize` / `top_level_commands`，返回 2 条硬编码命令 |
| CLI | `dd-run --list-extensions` 打印扫描结果与校验错误 |
| **协议一致性测试** | 把 [`protocol.md`](./protocol.md) 中**所有** JSON 示例抽出来，逐个做反序列化断言 |

**完成判据**：
- `cargo build` / `cargo test` / `cargo clippy` 全绿；
- [`protocol.md`](./protocol.md) 的每条示例消息都能被 `dd-protocol` 正确解析（示例与实现不一致即视为失败）；
- 宿主 spawn 示例扩展 → `initialize` → `top_level_commands` → `close` 全链路往返成功。

**验收映射**：A12（代码层：双向方法齐全）。

---

### M1 — 最小可用面板

**目标**：热键唤起面板，能用键盘走完"唤起 → 搜索 → 选择 → 关闭"。

| 任务 | 说明 |
|---|---|
| GUI 骨架 | egui 窗口（无边框、置顶、失焦隐藏），对应设计稿界面 01 |
| 全局热键 | Windows `windows-sys`（`RegisterHotKey`）；macOS/Linux `rdev` / `global-hotkey` |
| Root View | FilterBox + 分组列表 + 页脚键位提示条；渲染 `section` 分组与 `tags` chip |
| 清单扫描 | 三平台目录（见 [`manifest-schema.md`](./manifest-schema.md) §2）+ §7 的 9 条校验规则 |
| 进程管理 | spawn / 握手 / `close` / 崩溃检测骨架 |
| 键盘导航 | `↑↓` 移动、`Tab`/`Shift+Tab` 跨列表项、`Enter` 选中、`Esc` 关闭（对齐设计文档 §4.3） |
| 首屏聚合 | 并行拉取各扩展 `top_level_commands` |

**完成判据**：
- 热键可唤起/隐藏（A1）；
- 全程键盘完成"唤起 → 搜索 → 选择 → 关闭"，无需鼠标（A11，覆盖率 100%）；
- 协议双向方法齐全且能力调用不阻塞 UI（A12）。

> ⚠️ **本里程碑的主要风险**：egui 是即时模式，键盘焦点管理需自建。A11 要求 100% 键盘可达，**必须在 M1 验证**，不能拖到 M4——若 egui 无法满足，需在此处重新评估 ADR-2。

**验收映射**：A1、A11、A12。

---

### M2 — 命令执行与结果状态机

**目标**：命令能执行，8 种 `CommandResultKind` 驱动页面栈。

| 任务 | 说明 |
|---|---|
| `invoke` | 传 `sender` 与 `context`；处理扩展反向发来的 `host/*` 请求 |
| 8 种 Kind | `Dismiss` / `GoHome` / `GoBack` / `Hide` / `KeepOpen` / `GoToPage` / `ShowToast` / `Confirm` |
| 页面栈 | push / pop / `GoHome`；实现 ListPage + DetailPage |
| 全量拉取 | `get_items` + `items_changed` 通知合并（100ms 窗口） |
| `Confirm` 二次确认 | 宿主确认后重发 `invoke`，带 `context.confirmed = true` |
| Loading 与 Empty 态 | 对应 `is_loading` 与 `empty_content`（设计稿界面 10/11） |

**完成判据**：
- 单测覆盖**全部 8 种 Kind**（A4）；
- 页面栈 `GoBack` / `GoHome` 导航单测通过（A5）；
- 协议审查确认：列表更新走"事件 + 全量拉取"，**无增量集合推送**（A9）。

**验收映射**：A4、A5、A9。

---

### M3 — 缓存与懒加载

**目标**：冷启动走磁盘桩，扩展按需拉起。

| 任务 | 说明 |
|---|---|
| frozen 桩缓存 | `top_level_commands` 结果落盘，键 = 扩展 id + `version`（版本变即失效） |
| 冷启动路径 | 先渲染磁盘桩 → 再懒加载 fresh 扩展（并行，不阻塞首屏） |
| LRU warm 集合 | 容量 N（建议 8，可配置）；超出则 `close` + 终止进程，命令重新标 stub |
| 桩复热 | 点击桩 → spawn → `initialize` → `get_command` → 执行；失败/超时回退 stub 并报错 |
| 启动埋点 | 为 A2 的实测提供计时数据 |

**完成判据**：
- 进程监视器确认：frozen 扩展在冷启动时**没有**进程被拉起；点击桩项后复热成功（A6）；
- LRU 行为单测：超出 N 后释放并重新标 stub（A7）；
- 实测首屏冷启动耗时，记录是否达成 A2 的 200ms 目标（**未达成则记录实测值与瓶颈，不修改目标值**）。

**验收映射**：A6、A7、A2（实测）。

---

### M4 — 内置扩展与健壮性

**目标**：MVP 内置 5 个扩展，扩展崩溃不影响宿主。

| 任务 | 说明 |
|---|---|
| 内置扩展 ×5 | Apps / Calc / System / WebSearch / Shell——**全部为 ✅ 跨平台或 ⚙️ 平台相关，无一是 🪟 Windows 专属**（见设计文档 §7 平台列） |
| 平台适配 | Apps 索引按 OS 分路径（Win `shell:AppsFolder` 应用本体 + 开始菜单 `.lnk` 兜底 / macOS `/Applications` / Linux `.desktop`+PATH）；System 与 Shell 按 OS 分命令 |
| 崩溃恢复 | stdout EOF / 非 0 退出码检测 → in-flight 请求立即失败 → stub 回退 → 宿主继续运行 |
| 连续崩溃保护 | 连续 N 次（建议 3）后标记"暂时不可用"，宿主重启或手动重试才恢复 |
| 能力注入接 UI | `host/show_status`（Toast）、`host/set_clipboard`、`host/open_url` |
| 过滤性能 | 模糊匹配（如 `nucleo` / `skim`）；帧耗时采样埋点 |

**完成判据**：
- 故障注入（kill 子进程）后宿主不退出、可恢复（A8）——P1 代码完成，待真机复验（m4-record §4 #1–#3）；
- 5 个内置扩展功能清单核对通过（A10）——P4 扩展侧 + 宿主 fallback 轮代码完成（153/153 全绿，
  含无匹配渲染 + `{query}` 替换 + `context.query` 透传），真机清单见 m4-record §4 #9–#13；
- 实测结果列表过滤帧耗时，记录是否达成 A3 的 16ms/帧目标（**未达成则记录实测值**）——**P5 完成
  （2026-09-03）**：nucleo 模糊过滤 + 按分数重排（D11）+ 可见索引表/未变早退，
  **2000 项 ×6 字段一次重算实测 3.7ms（debug）< 16ms/帧，达标**；真机日志复核见 m4-record §3.7。

**验收映射**：A8、A10、A3（实测）。

---

## 3. 验收映射总表

| 验收项 | 内容 | 里程碑 |
|---|---|---|
| A1 | 全局热键可唤起/隐藏 | M1 |
| A2 | 冷启动首屏 < 200ms（**目标值，需实测**） | M3 实测 |
| A3 | 输入过滤 < 16ms/帧（**目标值，需实测**） | M4 实测 ✅（2000 项 3.7ms，debug；见 m4-record §3.7） |
| A4 | 单测覆盖 8 种 `CommandResultKind` | M2 |
| A5 | 页面栈 `GoBack` / `GoHome` | M2 |
| A6 | frozen 冷启动不拉起，点击桩项复热成功 | M3 |
| A7 | LRU 保活 N 个，超出释放并标 stub | M3 |
| A8 | 扩展崩溃后宿主不退出、可恢复 | M4 |
| A9 | 列表更新走"事件 + 全量拉取" | M2 |
| A10 | 内置扩展覆盖 Apps/Calc/System/WebSearch/Shell | M4 |
| A11 | 核心路径 100% 键盘可达 | M1 |
| A12 | 协议双向方法齐全，能力调用不阻塞 UI | M0 + M1 |

> A2 / A3 为**设计预期、非已证事实**（设计文档 §8.3、§11）。实测不达标时，做法是**记录实测值与瓶颈并据此决策**，而不是下调目标值以通过验收。

---

## 4. 架构决策记录（ADR）

### ADR-1：扩展隔离用「子进程 + NDJSON JSON-RPC」，WASM 沙箱降为进阶可选

- **状态**：已接受
- **背景**：CmdPal 在 Windows 上用进程外 COM 隔离扩展（设计文档 §6.2）。跨平台无 COM，需选择等价物。
- **决策**：每个扩展 = 独立子进程，通过 stdin/stdout 上的 NDJSON JSON-RPC 通信。
- **理由**：
  1. 零沙箱运行时——宿主不需要内嵌 WASM 引擎；
  2. 跨平台直出，Windows/macOS/Linux 同一套实现；
  3. 调试简单——`tail -f` 就能看协议流；
  4. **扩展可用任意语言编写**（只要能读写 stdin/stdout），符合设计文档 §2 的生态目标。
- **代价与缓解**：
  - 进程启动有开销 → 用 frozen/stub 磁盘缓存 + LRU 保活缓解（M3）；
  - 需自己定义并维护协议 → 产出 [`protocol.md`](./protocol.md)，M0 冻结 v1.0。
- **备选方案**：WASM 沙箱（`wasmtime` / `extism`）——隔离更强、启动更快，但宿主需实现沙箱运行时，且扩展必须用能编译到 WASM 的语言。

### ADR-2：GUI 框架用 egui

- **状态**：已接受（**M1 需验证键盘焦点，不通过则重新评估**）
- **背景**：需一个纯 Rust、跨平台、无重依赖的 GUI 方案，且不引入 Visual Studio / Windows SDK 依赖（设计文档 §8.3）。
- **决策**：egui（glow / wgpu 后端）。
- **理由**：
  1. 纯 Rust，自带渲染后端绑定，无需系统 GUI 工具链；
  2. 即时模式天然适合"列表内容频繁随搜索变化"的面板；
  3. 生态成熟、示例充足。
- **代价与风险**：
  - 即时模式下**键盘焦点需自建**——而 A11 要求核心路径 100% 键盘可达，这是本 ADR 的主要风险，必须在 M1 验证；
  - 无障碍（屏幕阅读器）支持弱于保留模式框架。
- **备选方案**：Slint（声明式、有商业授权考量）、iced（Elm 架构、生态较小）。

### ADR-3：扩展发现用「清单文件扫描」

- **状态**：已接受
- **背景**：CmdPal 用 `AppExtensionCatalog` / 注册表发现扩展（设计文档 §6.1）。跨平台需等价物。
- **决策**：扫描扩展目录下的 `*.json` 清单文件（见 [`manifest-schema.md`](./manifest-schema.md)）。
- **理由**：最简单、零运行时、易调试；扩展的安装/卸载就是文件的增删。
- **代价**：扩展需自行安装到目录，无自动发现能力。
- **备选方案**：进程注册（socket/命名管道，类 LSP）、WASM 内嵌——均列为可选进阶，MVP 不做。

### ADR-4：协议成帧用 NDJSON

- **状态**：已接受
- **背景**：JSON-RPC 2.0 不定义成帧，需自选。
- **决策**：一行一条紧凑 JSON，以 `\n` 结尾（见 [`protocol.md`](./protocol.md) §2.2）。
- **理由**：协议 payload 全是几十到几百字节的文本，无二进制附件需求；按行读写让两端 I/O 循环最简；调试时肉眼可读。
- **代价**：消息内不得有裸换行（JSON 转义天然保证）；不支持 pretty-print 的多行 JSON。
- **备选方案**：LSP 式 `Content-Length` 头（需自己解析 header 与半包）、长度前缀二进制（为未来二进制升级预留，但当前无需求）。

---

## 5. 当前进度

| 里程碑 | 状态 | 说明 |
|---|---|---|
| M0 地基与协议冻结 | ✅ 已完成 | 全部任务完成：workspace + `dd-protocol`（一致性测试 46/46）+ `dd-host`（清单扫描 §7 九规则 / 进程管理）+ `dd-ext-sample` + `dd-run` CLI；验收 40/40 测试全绿、clippy 0 告警、CLI 全链路实跑通过，见 [`./m0-record.md`](./m0-record.md) |
| M1 最小可用面板 | ✅ 已关闭 | 全部任务完成：GUI 骨架 + 全局热键 + Root View 渲染 + 清单扫描接入 + 进程管理（保活/崩溃检测骨架）+ 键盘导航（↑↓/Tab/Enter/Esc）+ **首屏并行聚合 `top_level_commands`**（错误隔离 + 示例扩展兜底）；12/12 单测 + 工程验收全绿；**2026-09-02 真机人工验收 11 项全过，R1 通过、ADR-2 成立**，见 [`./m1-record.md`](./m1-record.md)。残留：启动一帧闪屏（视觉瑕疵，见其 §4.6，未排期） |
| M2 命令执行与状态机 | ✅ 已关闭 | 逻辑层（页面栈 `navigation.rs` A5 + 8 种 Kind 裁决 `result.rs` A4 + Confirm 挂起重发 + invoke 参数 + `PanelItem` 透传 id/ext_id/command）+ UI 接线（`main.rs`：Enter 分派 `Invoke`→后台 `invoke` 并裁决 / `Page`→推页+`get_items`；Esc 非 Root 先返回、Root 隐藏；Toast（独立 Area）；Confirm 对话框；嵌套页；`items_changed` 100ms 合并全量重拉 A9）；**24/24 单测** + 工程验收全绿 + 全 workspace 构建无回归。十项真机复验全部通过（2026-09-02，见 [`./m2-record.md`](./m2-record.md) §4.5），完成判据 A4/A5/A9 全部达成。下一项进入 M3 UI 接线 |
| M3 缓存与懒加载 | ✅ 已关闭 | 逻辑层 `cache.rs`（FrozenCache/LruWarmSet/ColdStartTimer，对齐 A6/A7/A2，8 单测）+ 协议 `get_command` 接线（process 封装 / 示例扩展 handler / roundtrip 7 测）+ UI 接线（冷启动 frozen 读桩**不拉起进程** / 点击桩项复热 spawn→initialize→get_command→执行 / LRU 保活 8 个超容驱逐回落 stub / A2 计时日志 / 页脚三态 ◌✓✗）；**真机反馈 5 处修复**（见 [`./m3-record.md`](./m3-record.md) §3.4）：① 补 seguisym.ttf 解决 ◌ tofu；② 空态改 vertical_centered 不撑满页脚；③ #5 步骤改写为"启动后改名/杀进程"才能复现复热失败；④ A2 拆 agg_ms 分项日志定位 GUI/字体瓶颈；⑤ 列表长时页脚被挤出窗口 → 把 sources+键位提示移到 `egui::containers::Panel::bottom` 独立底栏。全 workspace **73/73 测试** + clippy/fmt 0 告警；`dd-gui.exe` 重编（16:05）。**A6/A2 真机复验已通过（2026-09-02）** |
| M4 内置扩展与健壮性 | ✅ 已关闭（2026-09-04，commit `757f3b4`） | 实施决策已定（2026-09-02，见 [`./m4-record.md`](./m4-record.md) §0）：D1 共享扩展运行时 / D2 本轮只做健壮性基础层 / D3 过滤用 nucleo。**P1–P3 代码完成**：崩溃恢复链 A8（`refresh_health` 每帧检测 + poll 死进程丢弃回落 stub）/ `host/*` 执行端接 UI（Toast + arboard 剪贴板 + webbrowser，空闲轮询也应答 HostRequest）/ 连续崩溃保护 §11（`robustness.rs` CrashGuard 熔断 + dispatch 拦截）。全 workspace **79/79 测试** + clippy/fmt 0 告警。P4（5 内置扩展 A10）/ P5（A3 nucleo 过滤，实测 3.7ms/2000 项）均已完成，真机复验通过 |
| ueli 风格 UI 重构（M5 设计轮，插队于 M4 后） | ✅ 批次 1–4.2 + 设计稿 v4.3 对齐 + 六轮真机反馈修复完成（2026-09-04，commit `5cf32b7`）；剩余 C 组占位见 §6.1 L8 | 基于设计稿 v2 [`cmdpal-ui-mockups.html`](../cmdpal-ui-mockups.html)。**批次 1 启动黑框**（2026-09-03）：屏幕外初始定位 + `with_active(false)` + 居中并入 `show()`（GetCursorPos/MonitorFromPoint 自算）；**批次 2 图标链路**：`CommandItem.icon` 三态（glyph/path/url）→ `PanelItem` 透传 → `IconView` 渲染 + 路径纹理缓存 + SegoeIcons/MDL2 字体回退链 + `system` 内置扩展 Path 图标验收项；**批次 3 整体换肤**：新建 `theme.rs`（05 表 token 唯一源：`Palette` 亮暗双套 + 几何常量 + `visuals()`/`apply()`，5 parity 单测）+ 绘制层落地（searchbar 46px glyph+focus 下划线 / 行 44px badge 化 + 选中 3px accent 条 / section 11px-600 / footer 状态点）。**批次 3.5 apps 真实图标抽取**（2026-09-03）：用户真机截图反馈 apps 全是占位 glyph（apps.rs:191 写死 U+E7C4），新增 `mod sys::icon`：SHGetFileInfoW → HICON → GetIconInfo + CreateCompatibleDC + GetDIBits → RGBA → PngEncoder → `%APPDATA%\dd-run\cache\apps-icons\apps-<hash>-32.png` 落盘；`top_level_commands` 改用 `item_icon(app)`；cache 含 PNG magic 校验自愈；新增 `image[png]` + `windows-sys 0.61`（5 features）依赖。**真机探针 stdout：total=400 path=400 glyph=0 = 100% 真实图标**（含 .lnk/.exe 全覆盖）。**176/176 全绿**（172 + apps 4 新测试）+ clippy/fmt 0；GUI 启动冒烟进程稳定。视觉真机验收清单见批次 3/3.5 报告 |

**已就绪的前置资产**：

- ✅ 设计参考 [`cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md)（§1–§11，含验收 A1–A12）
- ✅ 交互设计稿 [`cmdpal-ui-mockups.html`](../cmdpal-ui-mockups.html)（11 屏，可键盘走查）
- ✅ 扩展协议 v1.0 [`docs/protocol.md`](./protocol.md)
- ✅ 清单 schema v1.0 [`docs/manifest-schema.md`](./manifest-schema.md)
- ✅ 内置扩展清单与平台标记（设计文档 §7，21 项已按 API 复核）

---

## 6. 风险与未决

| # | 风险 / 未决项 | 处置 |
|---|---|---|
| R1 | **egui 键盘焦点**可能无法满足 A11 的 100% 键盘可达 | ✅ 已关闭（2026-09-02）：R1 尖峰 `ctx.input_mut(consume_key)` 拦截方案经真机人工验收通过（`↑↓/Tab/Enter/Esc` 在 FilterBox 有焦点时仍生效），R1 通过、ADR-2 成立，见 [`./m1-record.md`](./m1-record.md) §4/§5 |
| R2 | **冷启动 A2 < 200ms** 数据就绪实测 ~2ms 达标；total 高因 GUI/wgpu+msyh 字体加载 | ✅ 已关闭（2026-09-02）：A2 数据就绪 <agg> ~2ms 达标；total 高因 GUI/wgpu+msyh 22MB 字体（R2 记录瓶颈、不调目标） |
| R3 | **A3 < 16ms/帧** 在大结果集下可能不达标 | ✅ 已关闭（2026-09-03，M4 P5）：nucleo 打分排序，2000 项 ×6 字段实测 **3.7ms** < 16ms/帧（debug 构建，5 倍真实规模裕量），基准测试持续守卫 |
| R4 | 上游 PowerToys 文档引用边界需复核（许可证本身已定） | 已采用 **MIT**（根 `LICENSE` + 各 crate `license = "MIT"`），与 README 声明一致；引用边界复核留待专项一轮 |
| R5 | 设计文档 §7 中 **9 个 `🪟` Windows 专属扩展**不可移植 | 已在 §7 加平台列标记；MVP 不纳入 |
| R6 | CmdPal 仍处于 **preview**，上游接口可能演进 | 设计文档已标注 ✅ 核验日期；协议 v1.0 冻结后以本协议为准 |
| R7 | 设计稿字体依赖 Google Fonts（国内可能不可达） | 已改为本地优先分层字体栈（Archivo → Segoe UI Variable Display → …），CDN 仅作渐进增强 |

### 6.1 遗留项台账（2026-09-04 汇总）

> 各里程碑收尾后散落各 record 的未排期项，统一收敛于此。新里程碑规划时从此表取材。

| # | 项 | 来源 | 说明 / 归属 |
|---|---|---|---|
| L1 | 启动一帧闪屏 | m1-record §4.6 | 视觉瑕疵，未排期 |
| L2 | 熔断后手动重试入口 | m4-record §5 | 协议 §11「用户手动重试」需 UI 入口，当前仅重启宿主恢复 |
| L3 | 顶层 `items_changed` 不重聚合 | m3-record §5 / m4-record §5 | 扩展通知 ItemsChanged 后 Root 全量重拉未接（100ms 合并逻辑已有，A9 路径待接顶层） |
| L4 | 拼音匹配 | m4-record D12-B | nucleo 原生不支持，需 pinyin 转换 crate，独立后续项 |
| L5 | LRU 驱逐后 fallback 复热失败 | m4-record §3.6 / §5 | 第三方多扩展场景（内置 5 < LRU 8 不触发），复热走 `get_command` 不回 fallback 模板 |
| L6 | 上游 PowerToys 引用边界复核 | §6 R4 | MIT 许可证已定（根 LICENSE + 各 crate），文档引用边界专项一轮 |
| L7 | `dd-ext/apps.rs` clippy 风格警告 1 条 | 2026-09-03 批次记录 | `chunks_exact_to_as_chunks`（apps.rs:338 附近），既有、未顺手修 |
| L8 | 设计稿 v4 C 组占位实施 | 设计稿 v4.3 §12 | ✅ 代码完成（2026-09-04，C1–C3 三批次，未 commit）：C1 嵌套页顶行统一（返回 28×28 + 页标题进 placeholder + ext_id 徽标落页脚右端）/ C2 Loading 骨架（Spinner 22px + 3 骨架行）/ C3 Dialog 遮罩（全屏 Area 捕获层 + 点击取消 + 420px 面板 shadow64）+ Toast 意图接口（ToastKind Success/Error/Info）。**焦点态留档**：D9 列表行无焦点环已满足；返回按钮键盘可达 = Esc（Tab 保持列表导航语义，批次 4.0 既定决策），不实现返回按钮 Tab 焦点环。**真机验收待做**（A1–A5 / C1–C3） |
| L9 | IME 交互中文输入环境人工复验 | 2026-09-03 记录 | legacy_visuals=false + 选中态修复后，真实 IME 组合需人工确认（SendInput 无法模拟） |
| L10 | A2 冷启动 GUI 瓶颈 | §6 R2 | 数据就绪 ~2ms 达标；total ~2.8s 瓶颈在 wgpu + msyh 22MB 字体加载（记录不调目标，优化属候选 M6） |

---

## 7. 下一步

**M1 已关闭**（2026-09-02 真机人工验收 11 项全过，R1 通过、ADR-2 成立，见 [`./m1-record.md`](./m1-record.md)；残留启动一帧闪屏，见其 §4.6，未排期）。

**M2 已关闭**（2026-09-02 十项真机复验全部通过，A4/A5/A9 达成，见 [`./m2-record.md`](./m2-record.md) §4.5）。

**M3 已关闭（2026-09-02 真机复验 A6/A2 通过）**（逻辑层 `cache.rs` + `get_command` 协议接线 + UI 接线 + 5 处真机反馈修复——◌ 字体、空态撑满、#5 步骤、A2 拆计时、列表长时页脚被挤出窗口（移至 `Panel::bottom` 独立底栏）；73/73 测试全绿，见 [`./m3-record.md`](./m3-record.md) §3.4 / §4）：用 16:05 版 `./target/x86_64-pc-windows-gnu/debug/dd-gui.exe`（终端启动）按 §4 清单复验——首启落盘 → 重启读桩不拉起（A6，日志+页脚 ◌ 正常渲染，**页脚现在独立底栏**始终可见）→ 点击桩项复热成功（Invoke/Page 两条路径）→ 复热失败回退 stub（**先启动再改名 exe / 杀进程**）→ 记录 A2 冷启动分项耗时（`total = data_ready + gui_init`，15:53 实测 `2 ms + ~2861 ms`——瓶颈在 wgpu+msyh 22MB 字体加载）。**已进入 M4 内置扩展与健壮性**。

**当前状态（2026-09-03 17:40）**：M1 已关闭；M2 已关闭；M3 已关闭（已提交推送，commit `0c42465`）。**M4 代码全部完成**（见 [`./m4-record.md`](./m4-record.md)）。**M5 插队批次 3.9 已完成**——搜索结果右侧类型标签（apps→应用 / calc→命令 / system→设置 / websearch→网页 / shell→命令，第三方回退「命令」），协议层零改动；`cargo fmt/clippy/test` 全绿，真机截图验证标签居右显示。**中文输入法候选框位置修复**——`draw_searchbar` 在搜索框聚焦时显式覆盖 `PlatformOutput::ime`，以搜索框矩形/光标矩形强制 egui-winit 更新 `set_ime_cursor_area`，解决 Microsoft Pinyin 候选窗漂到屏幕左上角的问题（IME 交互需在中文输入环境人工复验）。

**当前状态（2026-09-03 22:20）**：**M5 批次 4.0 + 4.1 代码完成**（详见上方批次表实施记录）：批次 4.0 设置入口（`settings.rs` 主题三选 + `config.json` 持久化 + PageStack 推页 + Ctrl+, 快捷键，Tab 语义不变）+ 批次 4.1 上下文页脚（严格单行 35px、动作文本 + `↵ Enter` 键帽、无选中回退键位图例、仅异常显示源诊断）。**全 workspace 测试全绿**（dd-gui lib 66 + bin 12，dd-host 37 + 集成 14，dd-protocol 11，其余 0 失败）+ `fmt --check` 通过 + clippy 对 dd-gui/dd-host 0 告警（dd-ext/apps.rs:338 存在 1 条既有风格警告 `chunks_exact_to_as_chunks`，与本批次无关、未顺手修改）。**待真机验收**：齿轮可见可点 / Ctrl+, 打开设置 / 主题三选即时生效且重启保留 / 页脚动作随选中项切换 / 无选中回退图例 / 全 ok 无状态点 / 35px 高度。

**当前状态（2026-09-03 22:40）**：**M5 批次 4.0 + 4.1 真机验收通过**——用 Python ctypes SendInput（hardware source + 时序）+ PIL ImageGrab 自动唤起 `Win+Alt+Space` → 截图 → 点击齿轮 → 截图，对比设计稿渲染，**C1/C2/C6/C7/C8/C12 全部通过**（截图见 `.workbuddy/tmp/step*-crop.png`）：① 面板唤起显示搜索栏 + 空态 + 页脚键位图例 + ⚙ 齿轮；② Ctrl+, 唤出设置页（标题「设置」/「主题外观」/三选 selectable_label / 底部「Esc 返回 ｜ Ctrl+, 打开设置」提示）；③ Esc 返回根页（A5 语义）；④ 第二次 toggle 隐藏面板；⑤ 写入 `%APPDATA%\dd-run\config.json` `theme=dark` 重启 → 面板背景变 #292929，「暗色」选项高亮（accent #479ef5）→ **设置→主题→落盘→重启链路完整**；⑥ 齿轮 24×24 热区 click（屏幕坐标 712,740）→ 同样唤出设置页（C12 同窗口确认）。**冒烟脚本**留档于 `.workbuddy/tmp/smoke.py`（`launch/toggle/down/ctrlcomma/esc/gear/all/allgear` 子命令）。

**当前状态（2026-09-03 23:30）**：**选中态蓝块修复（第三轮）**（用户截图反馈：搜索框全选文字整段被蓝色覆盖）。根因与 IME 蓝块同源但机制不同：egui `text_selection/visuals.rs` 的 `paint_text_selection` 会把**选中字形重绘为 `selection.stroke.color`**——此前 `bg_fill` 与 `stroke.color` 均设为 accent 蓝 → 字形与背景同色 → 文字不可见。修复 = Fluent/Windows 原生风格：`selection.bg_fill = accent.gamma_multiply(0.30)`（半透明 accent 底 30%）+ `selection.stroke = text`（选中字形用主文本色，亮色深字/暗色白字均可读）。**真机双验证通过**（SendInput 打字 + IME 组合 + 全选截图 `step8-selection-crop.png`）：① IME 组合文本 `x'y'z'z'y` 渲染为深色文字 + 深蓝下划线（前轮 legacy_visuals=false 修复生效）；② 全选文本深色可读，无蓝块。`cargo build` + dd-gui 78 测试 + fmt 全绿。

**当前状态（2026-09-03 23:20）**：**设计稿全面核对 + 4 处对齐**（用户指令「参考设计文档内容核对现在实际效果，修复不一致的地方」；期间用户询问过换 Tauri 2 做 UI，经对比分析后决策**继续 egui 精修**——Tauri 方案 WebView2 冷启动大概率破 A2 200ms 硬指标、M1–M4 已验收行为全部重验，成本大于收益）。逐段核对 §01–§06 与实现：token 13 字段、键帽（text-2 色/10.5px monospace/下边 2px）、页脚 11.5px/间距 14/状态点亮暗双色、类型标签 12px text-3 最右、行 padding 9/10/9/8 + gap 12 + 图标 20 + badge 220 + accent 条 3px、几何 note（560/8/44/46）均一致 ✅。**修复 4 处不一致**：① 空态重设计（原单行 weak 文本 → 设计稿 02 屏 `.empty` 规格：搜索图标 30px/text-3 + 标题 15px/text + 描述 12.5px/text-3，`draw_empty_state` 辅助函数；文案对齐「未发现命令/未找到匹配的命令 + 试试其他关键词。」）；② section-label 间距对齐 `padding: 10px 10px 4px`（原上 4/下 2/无左缩进 → 上 10/下 4/左缩进 10）；③ `.results` 容器内边距 `6px 6px 8px` 落地（ScrollArea 内 Frame inner_margin，行相对搜索栏再内收 6px、列表底 8px）；④ 搜索栏 margin-top 对齐 12px（CentralPanel 上边距 10→12）。**留档平台限制 1 项**：`.row .name` font-weight 500——egui `FontId` 不携带字重、`.strong()` 只变色，无法等价实现。复验：真机截图空态（图标+标题+描述）与设置页（键帽提示行）均符合设计稿；`cargo build` + dd-gui 78 测试 + fmt/clippy 全绿。

**当前状态（2026-09-03 23:05）**：**真机反馈 2 处修复（第二轮）**（用户截图反馈：① 搜索框输入中文组合文本变成蓝色实心方块；② 设置页提示行「Esc 返回 ｜ Ctrl+, 打开设置」样式与设计稿键帽语言不一致）：① 根因 = egui 0.36 在 Windows 上 `Visuals::ime_composition.legacy_visuals` **默认 true**（因 winit 韩文光标 bug，见 egui style.rs 注释），组合文本按「选区」方式渲染 = 整段涂 `selection.bg_fill`（= accent 蓝）；修复 = `theme.rs` `apply()` 显式置 `legacy_visuals = false`，改用新版 IME visuals（active/inactive 下划线 + 组合内光标，专为中日韩设计；该 winit bug 仅影响韩文，微软拼音正常）。② `draw_settings` 提示行改键帽样式（`draw_keycap("Esc")` + 「返回」+ `draw_keycap("Ctrl+,")` + 「打开设置」，chip 底 + 1px 描边 + 下边 2px + monospace，与 §6.3 页脚键帽同规格）；顺手对齐「主题外观」section 标签到设计稿 §05 全局规格（11px / 600 / text-3，原 12px 常规）。复验：设置页截图确认键帽提示行 + 11px section 标签；IME 组合渲染需用户中文输入环境人工复验（SendInput 无法模拟真实 IME 组合事件）。`cargo build` + dd-gui 78 测试 + fmt/clippy 全绿。

**当前状态（2026-09-03 23:00）**：**批次 4.0/4.1 真机反馈 2 处修复**（用户截图反馈：① 页脚左侧诊断文本与右侧键位图例重叠；② 设置页底部仍显示页脚）：① `draw_status_footer` 左块动作文本与 `draw_footer_entries` 诊断文本全部改 `egui::Label::truncate()`——egui label 默认不裁剪、超宽溢出盖到键位区（`new_child` 的 max_rect 不收窄 clip rect），truncate 后超宽文本在左块边界省略号截断；② `draw_panel` 页脚 BottomPanel 整块包进 `!is_settings` 条件——设置页不渲染页脚（用户确认设置视图不含齿轮行）。复验（冒烟脚本重跑 + 截图）：根页脚「未找到内置扩展可执行文件（G:\AI\dd-ru…」正确截断、图例完整；设置页（Ctrl+, 与齿轮点击两路径）底部均无页脚。`cargo build` + dd-gui 78 测试全绿 + fmt/clippy 0 告警。

**构建环境记录（2026-09-03，windows-gnu 链接修复）**：本机原仅 msvc 工具链，仓库 `.cargo/config.toml` 强制 `x86_64-pc-windows-gnu` → 安装 rustup `stable-x86_64-pc-windows-gnu`（minimal + clippy/rustfmt，不改默认工具链）。链接两个坑：① rustup self-contained bin（dlltool/ld 2.44）**缺 `as.exe`，且仅靠 PATH 前置 msys2 as.exe 仍 CreateProcess 失败——as.exe 必须与 dlltool 同目录**（已复制 `as.exe`（msys2 binutils 2.47）入 rustup self-contained bin 目录）；② self-contained lib 缺 `libshlwapi.a`（arboard/clipboard_win 依赖）→ 从 TUNA 镜像 msys2 `mingw-w64-ucrt-x86_64-crt-git` 包提取该单文件补入 self-contained lib 目录（**只补缺失文件、不覆盖 rustup 自带 42 个库**，避免 msys2 2.47 全套与 rustup ld 版本混搭）。修复后 `cargo test --workspace` 全绿。临时文件位于 `C:\Users\y7398\.workbuddy\binaries\mingw64\`（as-only/、tool/、ucrt64/、crt.pkg.tar.zst）。

**ueli 风格 UI 重构（2026-09-03，插队批次，未 commit）**：基于设计稿 v2（`cmdpal-ui-mockups.html`，ueli/Fluent 9 视觉语言 + 亮暗双 token）分三批推进并各自验收。**批次 1 启动黑框修复**——根因是 eframe 0.36.1 `post_rendering` 首帧后无条件 `set_visible(true)`（egui PR #2279），修复取「初始定位屏幕外（`OFFSCREEN_*`）+ `with_active(false)` + 居中并入 `show()`（Win32 GetCursorPos→MonitorFromPoint 自算，屏外时 `center_on_screen` 会取错屏故不可复用）」；删除启动期逐帧 `recenter_if_needed`。**批次 2 图标链路**——`CommandItem.icon` 三态透传（state.rs 增字段 + aggregator 透传 + 单测）→ `IconView`（Empty/Glyph/Texture）+ `resolve_icons`（ScrollArea 闭包外预解析）+ 路径纹理缓存；字体链追加 `SegoeIcons.ttf`→`segmdl2.ttf` 回退（码位兼容）；`image` crate（png/ico feature）解码；`dd-ext-system` 内置「UI 验收：PNG 图标」Path 演示项（`CARGO_MANIFEST_DIR` 编译期锚定资产）；wire 层 E2E 探针确认 serde 将 `IconKind` 重命名为 `type`。**批次 3 整体换肤**——新建 `theme.rs` 为 token 唯一源：`Palette` 13 字段亮/暗双套（暗 panel `#292929`/accent `#479ef5`，亮 `#ffffff`/`#0f6cbd`，row 态暗色半透明白 alpha 15/21）+ 几何常量（行 44/圆角 6/指示条 3/搜索栏 46）+ `visuals()`→`Visuals` 覆盖 + `apply()` 注册双主题跟随系统；绘制层全部改经 `Palette` 取色：searchbar（glyph U+E721 前缀 + 底部 2px 聚焦 accent 下划线）、行重绘（subtitle 上移为名称右侧 desc-badge 限宽 220、tags 药丸 chip、hover/selected 预算行矩形同帧判定、选中 3px accent 条）、section 标题 11px/600/text-3、页脚状态点化、toast 底显式 token。egui 0.36 API 踩坑留档：`Margin` 字段为 i8、`TextEdit::frame` 收 `Frame`、`Color32` 预乘存储（断言须 `r()==a()`）。**批次 3.5 apps 真实图标抽取**（本轮续）——用户真机截图反馈 apps 全部占位 glyph（apps.rs:189-191 写死 `Icon { kind: Glyph, value: U+E7C4 }`），新增 `mod sys::icon`：shell 抽取链路 `SHGetFileInfoW(SHGFI_ICON|SHGFI_LARGEICON)` → HICON → `GetIconInfo` 拆 hbmColor → `CreateCompatibleDC` + `GetDIBits(BGRA)` → `RgbaImage` → `PngEncoder`（BGRA→RGBA + alpha=255）→ 落 `%APPDATA%\dd-run\cache\apps-icons\apps-<16hex>-32.png`，重抽命中按 `DefaultHasher` cache key；cache 含 PNG magic 校验自愈（坏文件重抽覆盖）；失效回退原 `U+E7C4` glyph。`top_level_commands` 改用 `item_icon(app)`；新增依赖 `image[png]` + `windows-sys 0.61`（`Win32_Foundation/Storage_FileSystem/UI_Shell/UI_WindowsAndMessaging/Graphics_Gdi`）。**Windows API 踩坑（注释留档）**：`SHGetFileInfoW` cfg 是 `Win32_Storage_FileSystem+Win32_UI_WindowsAndMessaging` 复合；**`GetDIBits` 的 hdc 必须有效 DC**（`null_mut()` → 返回 0）必须 `CreateCompatibleDC(NULL)`；`SHGFI_LARGEICON = 0x0` 是 Win32 原义；`image::ImageEncoder::write_image` 收 `self`（值）不是 `&self`——需 `encoder.write_image(...)` 移走；HICON 已是 `*mut c_void`，不要多余 `as HICON`。**全 workspace 176/176 测试全绿**（上批 172 + apps 4 新测试：单 exe 抽 PNG、`item_icon` 倾向 Path、覆盖率 ≥90%、落盘 PNG magic 校验）、clippy/fmt 0、GUI 启动冒烟进程稳定。**用户真机复验结果**：apps 全覆盖 100% 真实图标（probe stdout `total=400 path=400 glyph=0`），含 .lnk（SHGetFileInfoW 自动跳目标）/ .exe / Windows 系统图标。设计偏差留档（批次 3）：页脚未用 panel-2 底、keys 未 chip 化、desc-badge 截断为 Label.truncate 近似。**未 commit**（与 M5 批次叠加统一由用户审核提交）。

### M5 后续批次：设置入口 / 类型标签 / 上下文页脚

依据用户真机截图与 [ueli](https://github.com/oliverschwendener/ueli) 实际界面，在 [`cmdpal-ui-mockups.html`](../cmdpal-ui-mockups.html) 中追加 §6「优化项」规格，并在此记录实施任务与验收。

| 批次 | 任务 | 说明 | 验收标准 |
|---|---|---|---|
| 3.9 ✅ | 搜索结果类型标签 | `PanelItem` 新增 `result_category: Option<String>`（协议层不改）；`aggregator::category_label_for` 按 `ext_id` 去 `com.ddrun.` 前缀映射（apps→应用 / calc→命令 / system→设置 / websearch→网页 / shell→命令），第三方回退「命令」；`draw_item_row` 在 `right_to_left` 布局中先画类型标签（最右）、再画 tags（其左），12px/text-3，最长 90px 截断。 | C3–C5、C9、C11、C13 通过；行高保持 44px；`cargo fmt/clippy/test` 全绿；真机截图验证类型标签居右显示。 |
| 4.0 ✅ | 左下角设置按钮 | 页脚最左侧常驻齿轮图标（U+E713，16px）；点击在**同窗口**以子视图/弹出层打开设置（复用 `PageStack` 推 `SettingsPage` 或 `egui::Window`），不新起 eframe 实例；键盘 Tab/Enter 可达；所有页面栈层级显示（页脚位于应用级 `Panel::bottom`，已覆盖）。**实施记录（2026-09-03）**：新增 `dd-gui/src/settings.rs`（`ThemePref` 三选 + `config.json` 解析/落盘，损坏/未知值回落默认永不失败，4 单测）+ `dd-host` `config_file()`；`PageState::settings()` 经 PageStack 推页复用 Esc 返回；主题立即生效（`theme::apply` 参数化）并持久化到 `%APPDATA%\dd-run\config.json`；**决策记录：Tab 保持列表导航语义（保护 M1 已验收行为），键盘可达改由 Ctrl+, 达成（C2 口径调整）**。 | C1–C2、C12 通过（C2 按 Ctrl+, 口径）；设置视图能正常弹出并关闭；不占用全局热键。**代码+单测完成，真机验收待做（齿轮点击 / Ctrl+, / 主题切换与重启保留）**。 |
| 4.1 ✅ | 页脚上下文动作提示 | 有选中项时页脚显示当前项默认动作 + 快捷键（动作由 GUI 硬编码 `ext_id→动作` 映射：apps→打开应用 / websearch→打开网页 / calc→计算 / system→打开设置 / shell→运行命令 / Page→进入）；无选中时回退全局键位图例；**源健康诊断保留但仅异常时显示**（存在 stub/err 源时追加状态点）；高度严格 35px（绘制层 `ui.horizontal` 强制单行 + `set_min_height(35)`，禁止 `horizontal_wrapped`）。**实施记录（2026-09-03）**：`draw_status_footer` 重写为严格单行（`allocate_exact_size` + 左右 `new_child` 切分，左块 max_rect 截断），有选中项右侧显示 `↵ Enter` 键帽、无选中回退键位图例；`footer_entries` 跳过 Warm 仅输出 Stub/Failed/note；`footer_action_text` 映射与 3.9 `category_label_for` 同口径（去 `com.ddrun.` 前缀）；3 个映射单测。 | C6–C8、C13 通过；切换选中项时动作文本实时更新；全 ok 时不显示状态点。**代码+单测完成，真机验收待做（动作随选中项切换 / 35px 高度 / 异常源诊断）**。 |
| 4.2 ✅ | 搜索结果右键菜单（设计稿 10B，v4.4） | 右键结果行 / Shift+F10 弹出上下文菜单（D19）；菜单项 = GUI 层按 `result_category` 静态硬编码（D18，协议 v1.0 冻结零新增字段，与页脚动作同口径）；容器 = panel 底 + 1px border + 圆角 4 + shadow8（官方 elevation 核实）；项高 32、hover/焦点 = bg1Hover、图标 16 + 名称 body1 + 快捷键 caption1/fg3；指针锚点 + 面板内翻转夹紧 8px（D20）；Esc / 点击外部 / 滚动关闭；键盘上下文移交（↑↓ 移动、Enter 激活、Esc 返还）。**实施记录（2026-09-05）**：`CtxMenuState/CtxEntry/CtxRow/CtxAction` + `open_ctx_menu` / `activate_ctx_menu` / `draw_context_menu`（尺寸经 `text_width` 确定性预量同帧 clamp）+ 纯映射 `context_menu_rows` / 门控 `path_like` / `url_like` / `default_action_glyph` + 平台动作 `run_as_admin`（ShellExecuteW runas，windows-sys 增 `Win32_UI_Shell` feature）/ `reveal_in_folder`（explorer /select）/ 复制（`ctx.copy_text` + info Toast）；激活前校验可见索引未漂移（防列表刷新陈旧激活）；`theme.rs` 增 `menu_shadow` + `CTX_*` 几何常量（9 项 parity 单测）。**偏离记档：① 键盘触发仅 Shift+F10**（egui 0.36 键表无 Menu 键）；② 网页类「复制链接」按 URL 形态 subtitle 门控（现行 websearch 顶层项 subtitle 为提示文案，故实际仅默认动作，未来扩展提供 URL 后自动出现）；③ 路径型动作按盘符/UNC 形态 subtitle 门控，数据不可得不渲染该项。 | E1–E3 通过（菜单几何 / 类型映射一致性 / 关闭与定位行为）；`fmt/clippy/test` 全绿（新增 11 测试）；**真机验收待做**。 |

**前置依赖**：批次 3.8 已落地页脚 `panel-2` 底 + 顶部 1px `border` + 键帽样式（见 `.workbuddy/memory/2026-09-03.md` §批次 3.8）。类型标签与设置按钮批次不依赖页脚改造，可并行。

**数据模型变更**：仅 GUI 状态层。`dd-protocol` 冻结 v1.0，不新增字段。

**验收映射**：新增 C1–C13 校验表（见 `cmdpal-ui-mockups.html` §6.4；含 C11 类型与 tags 次序、C12 同窗口设置视图、C13 字段口径一致）。

**当前状态（2026-09-04 14:40）**：**Apps 扩展对齐 PowerToys CmdPal（未 commit）**——用户反馈「本项目运行后列的多是快捷方式且图标难看，PowerToys 是应用本身、ueli 图标美观」。重写 `crates/dd-ext/src/bin/apps.rs`：① 主源改为 `shell:AppsFolder` 应用本体枚举（`FOLDERID_AppsFolder` → `SHCreateItemFromIDList` → `BindToHandler(BHID_SFObject, IID_IShellFolder)` → `EnumObjects`/`IEnumIDList` → `SHCreateItemWithParent`；过滤 parsing name 含路径分隔符的非应用项）；② 开始菜单 `.lnk` 降为兜底源（按显示名去重），**删除 PATH `*.exe` 扫描**；③ 图标升 48px：AppsFolder 项 QI `IShellItemImageFactory::GetImage(48, ICONONLY|BIGGERSIZEOK)`（返回句柄类型不统一，双路径解释：HICON 或 32bpp HBITMAP），`.lnk` 回退 `SHGetFileInfoW`；④ alpha 修复：保留真实 per-pixel alpha，全零时读 AND 掩码生成（旧实现强制 alpha=255 → 黑角/锯齿）。真机：AppsFolder 86 应用 + 兜底共 188 项（旧链路 400 项噪音），图标覆盖率 ≥90%（守卫单测），`cargo test -p dd-ext` 全过、工作区构建通过。坑位记录：windows-sys 需新增 features `Win32_System_Com`/`Win32_UI_Shell_Common`；手绘 COM vtable 函数指针必须 `extern "system"`（Rust 默认 ABI 实测 STATUS_ACCESS_VIOLATION）；`BHID_EnumItems` 在本机返回 E_NOINTERFACE（经典 IShellFolder 路径可用）。

**当前状态（2026-09-04 14:50）**：**Apps 真机反馈第二轮 3 处修复（未 commit）**——用户截图反馈：① 7-Zip File Manager 仍显示「快捷方式箭头」图标；② 「Add a new TAP virtual ethernet adapter」等非正式应用混入；③ 长名称时副标题与右侧「应用」类型标签重叠、右列不垂直对齐。修复：① `apps.rs` 兜底 `.lnk` 经 `CoCreateInstance(CLSID_ShellLink)` + `IPersistFile::Load` + `IShellLinkW::GetPath(SLGP_RAWPATH)` 解析目标，**仅收录目标为 .exe 的项**，图标/副标题取目标 exe（真机验证：7-Zip → `D:////Program Files\7-Zip\7zFM.exe` 真图标，7-Zip Help(chm) 被过滤）；② AppsFolder 过滤 `Microsoft.AutoGenerated.*` 伪应用（Shell 从 lnk 自动注册的非安装应用）+ 文件路径 parsing name（真机：AppsFolder 86→60，总列表 188→116 噪音清除）；③ `dd-gui/main.rs` `draw_item_row` 右列宽度预留：先测类型标签 + tag chips 宽度并从左侧可用宽扣除，标题/副标题在剩余空间内截断（副标题剩余 <48px 时整段省略），右列贴右且逐行垂直对齐。验收：`cargo test -p dd-ext -p dd-gui` 132 项全过（新增 `lnk_items_point_to_exe_targets` 守卫）。

**当前状态（2026-09-04 15:25）**：**真机反馈第三轮 3 处修复（未 commit）**——① 暗色主题下深色图标不清晰（ChatGPT 黑 glyph 贴暗背景不可见）：`dd-gui/main.rs` 解码时新增 `icon_is_dark`（不透明像素 max(r,g,b) 均值 <90 → 暗；用 max 通道而非感知亮度，饱和红 AMD max=188 不误判），`IconView::Texture` 携带 dark 标志、`icon_cache` 存 `(tex, dark)`，暗色主题下暗图标垫近白圆角底（#F5F5F5，round 4）。真机：ChatGPT=0→垫底、AMD=188/Clash Verge=126→不垫。② 计算器 `=1+1` 报「缺少操作数」：`dd-ext/calc.rs` `query_after_prefix` 统一剥离前导 `=`（只剥一层，`==1+1` 仍正确报错）；真机协议验证 `=1+1`→`= 2`、`calc = 2^8`→`= 256`、`1+2*3`→`= 7` 回归正常。③ 打开面板默认不铺全部应用：`state.rs` `PanelState` 新增 `EmptyQueryView`（All/WithoutApps，空查询隐藏 `result_category=="应用"` 项，查询时应用照常参与匹配）；`settings.rs` 新增 `OpenView`（default/all，默认 default）持久化到 config.json `open_view` 字段；设置页新增「打开面板时显示」卡片（checkbox「显示所有应用」），变更即时生效（`apply_open_view` 重算 root 可见表）+ 落盘。验收：dd-ext + dd-gui 全部测试通过（新增 4 个守卫测试）；debug + release bin 已重链（注意：链接期 Permission denied = 面板进程占用，先关 dd-gui 再编）。

**当前状态（2026-09-04 16:50）**：**真机反馈第四轮 4 处修复（未 commit）**——① Shell 执行报错乱码：cmd 中文输出为 GBK(CP936)，`from_utf8_lossy` 产生乱码 toast；`dd-ext/shell.rs` 新增 `decode_output`（合法 UTF-8 直接采用 → 否则 `GetConsoleOutputCP()` + `MultiByteToWideChar` 转码 → 失败回落 lossy），windows-sys 加 features `Win32_System_Console`/`Win32_Globalization`；真机协议验证 `'1+1'` → 「'1+1' 不是内部或外部命令，也不是可运行的程序或批处理文件。」（行为正确，仅编码修复）。② 设置页打开时隐藏（热键 Toggle/失焦）后再唤起仍停留设置页：`show()` 复位分支只 reset 了 root 列表未清页面栈，改为 `stack.go_home()` + root reset（扩展 `Hide` 保留状态语义不变）。③ 计算兜底项双等号（`=1+11` → `= =1+11`）：`dd-gui/fallback.rs render_title` 对 `"= "` 开头模板把替换值剥前导 `=`（与 calc 求值端语义一致）；普通查询与非 `= ` 模板（shell）不受影响。④ 设置页内容显示不全：窗口原固定 560×460，设置卡片被截断；新增 `SETTINGS_W/H=560×640`，`ui()` 按栈顶 `is_settings` 帧间 diff 发 `ViewportCommand::InnerSize`（进设置页放大、返回缩回，所有路径统一收口）。验收：dd-ext + dd-gui 138 测试全过（新增 GBK 转码、calc 模板剥等号 2 个守卫）；debug + release 全部重链；新 dd-gui 已拉起待视觉确认。环境坑：rustc ICE → 删 target/x86_64-pc-windows-gnu/debug/incremental 重编即愈；release 构建偶发沙箱拦截链接器，重试即可。

**当前状态（2026-09-04 18:00，文档一致性清理）**：**六轮真机反馈修复已提交推送（commit `5cf32b7`，11 文件 +3399/-776，未跟踪 shot_*.png 不入库）**；M5 主体完成。本轮收尾：① §5 进度表 M4/M5 行与头部状态对齐为已关闭（M4 = `757f3b4`）；② R3（A3 过滤）按 P5 实测结果关闭；③ 删除 16:50 重复段落；④ 新增 §6.1 遗留项台账（L1–L10）；⑤ README 状态与「下一步」从「M0 之前」更新至当前、许可证说明对齐 MIT 现状；⑥ m4-record 头部状态关闭。**下一步候选（M6，待 grill-me 收敛）**：第三方扩展端到端验证 / 打包分发 / A2 冷启动 GUI 瓶颈（L10）/ 跨平台；较近的一项为 C 组 UI 占位（L8）。

**当前状态（2026-09-04 18:30，C 组占位实施，未 commit）**：**设计稿 v4.3 C 组占位按三个垂直切片全部落地**（对应 §12 验收 A1–A5 / C1–C3，详见 §6.1 L8）：

- **C1 嵌套页顶行统一**（§07.1）：删除旧嵌套页独立标题行（页标题 12px + "[Esc] 返回"），改为 40px 统一顶行 = `draw_back_btn` 28×28（ChevronLeft、点击 = `go_back`）+ 搜索框（`draw_searchbar` 增 `placeholder` 参数，嵌套页 = `nested_search_placeholder(title)` = 「在「{页标题}」中筛选…」）；Root 搜索框占满，两形态同高零位移（A5）。`draw_version_chip` 泛化为 `draw_ext_chip`（monospace mini 10 + chip 底 + 胶囊，对齐 `.ext-chip` CSS）；嵌套页页脚右端常驻 ext_id 徽标，键位图例整体左移 `chip 宽 + FOOTER_GAP` 让位。
- **C2 Loading 骨架**（§07.2）：`draw_list` 的 `is_loading` 分支纯文本「正在加载…」→ `draw_loading_state`：Spinner（22×22、accent_soft 底环 + accent 90° 旋转弧 0.9s/圈、20 段折线逼近）+ caption 文案 + 3 条骨架行（行高 `ROW_H`=40 无布局跳动；图标块 20×20 圆角 6 / 名称条 12px / 描述条 10px 右对齐；shimmer 1.4s `input_fill↔panel_2` 平滑往返、各行相位错开）。纯函数 `skeleton_fractions` / `shimmer_color` 配单测；加载期间 33ms 重绘驱动动画；拉取时序与超时语义零改动。Root `aggregating` 文案保持（设计稿无该场景规格）。
- **C3 Dialog 遮罩 + Toast 意图**（§09/§10）：`draw_confirm` 从 `egui::Window` 重写为**全屏 `Area` 捕获层**（Foreground 层遮罩 `theme::overlay` = blackAlpha[50]/[40] + `Sense::click`，点击遮罩且指针在面板外 = 取消）+ 对话框 `Area`（Tooltip 层，420px 面板：panel 底 + 1px border + 圆角 8 + `theme::dialog_shadow`（shadow64）、padding 20/20/16、标题 16/600、正文 14/text-2 换行、按钮区右对齐：`Enter`/`Esc` 键帽提示最左 + 取消 secondary（card 底 + border-strong 描边 + hover row-hover）+ 确认 accent/critical danger 底白字，高 32 圆角 4，`draw_dialog_button` 经 `ui.interact` 绝对矩形注册点击）。键盘语义不变（`handle_keys` 已有：Enter=确认 / Esc=取消 / 其余键吞掉）。Toast：`ToastKind{Success,Error,Info}`（图标 E73E/E783/E946 + success/danger/text2 语义色），`show_toast` 默认 info（扩展 `ShowToast` 路径不变），失败/熔断/进程不可用 5 处宿主路径改 Error；宽度下限 80→250px（§9.1 最小宽，顺手修掉 clippy `manual_clamp`）。egui 0.36 API 坑：`Context::style()` 已移除 → `ctx.theme()`；`InputState.screen_rect` 非公开 → `ctx.viewport_rect()`；`Area::fixed_rect` 不存在 → anchor LEFT_TOP + `allocate_exact_size(screen.size())`。
- **验证**：`cargo fmt` 干净、`cargo clippy -p dd-gui --all-targets` 0 告警（含顺手修掉既有 `manual_clamp`）、全 workspace **205 测试全绿**（新增 7：nested placeholder 1 + skeleton/shimmer 2 + overlay/dialog_shadow parity 2 + 既有回归）；debug + release 全部重链。**真机验收待做**：A1–A5（顶行几何/返回行为/骨架/复用一致/零位移）+ C1–C3（Toast 意图与几何/Dialog 遮罩点击取消/critical danger 底）。

**当前状态（2026-09-05，右键菜单 10B 实施批次，未 commit）**：**设计稿 v4.4 10B「搜索结果右键菜单」egui 侧全量落地**（上一轮先补设计稿+HTML 交互演示，本轮写代码）。数据模型：`dd-protocol` 零改动（协议 v1.0 冻结不变，D18）；`dd-gui` 新增 `CtxMenuState/CtxEntry/CtxRow/CtxAction` 与 `PaletteApp.ctx_menu`/`want_ctx_menu_for_selected` 两个字段。触发链路：① 指针——`draw_item_row` 的 `ui.interact` 响应补 `secondary_clicked()` 捕获（含 `interact_pointer_pos` 锚点），右键即置选中 + `open_ctx_menu`（锚点 = 右键点 + 2,2，D20）；② 键盘——`handle_keys` 消费 Shift+F10 置旗标，`draw_list` 绘制选中行后以行底边左缘为锚点落位（行矩形只在绘制期可得）。渲染：`draw_context_menu` = 透明点击捕获层（Foreground，`allocate_exact_size` 返回 `(Rect, Response)` 二元组——0.36 API 坑，对齐 C3 批次写法）+ 菜单本体（Tooltip 层：panel 底 + 1px border + 圆角 4 + `theme::menu_shadow`（shadow8，官方 elevation 低层 ramp 暗 28%/亮 14%）+ `CTX_*` 几何常量）；尺寸经 `text_width` **确定性预量**（`ctx.fonts` 拿不到 `layout_no_wrap`——FontsView 无此方法，故复用 painter 测量），同帧完成翻转/夹紧（下越界先向上翻转，水平仅夹紧 8px，绝不溢出）。行为：Esc/点击外部/`smooth_scroll_delta` 非零（滚动）即关闭（D19）；菜单打开期间 `handle_keys` 早返回吞掉全部导航键（键盘上下文移交：↑↓ 移动焦点、Enter 激活、Esc 返还列表；Tab/Ctrl+, 同吞）；`activate_ctx_menu` 激活前校验 `visible_idx + item_id` 与当前 `filtered()` 一致（防 items_changed/fallback 刷新后陈旧激活），默认动作 = 置选中后走 `confirm_selected()`（与 Enter 完全同款）；`RunAsAdmin` = `ShellExecuteW` verb=runas（windows-sys 增 `Win32_UI_Shell` feature，返回值 ≤32 视为取消/失败 → Error Toast）；`RevealInFolder` = `explorer /select,<path>`；`CopyText` = `ctx.copy_text` + info Toast「已复制到剪贴板」。隐藏收口：`hide()` 清 `ctx_menu` + 旗标（Dismiss/Hide/热键/失焦全路径覆盖）。映射（D18，`context_menu_rows` 纯函数 + 门控）：应用 = 打开/以管理员身份运行/打开所在位置/─/复制路径；文件 = 少管理员项；网页 = 打开/─/复制链接；命令/设置 = 仅默认动作；第一项恒为默认动作（label 复用 `footer_action_text`，shortcut `↵ Enter`）。**验证**：`fmt` 干净、`clippy -p dd-gui` 0 告警、workspace 全部测试通过（dd-gui 27，新增 11：映射 5 + 门控 2 + 焦点/glyph 2 + shadow8/几何 parity 2）；`tools/package.sh` release 重链 → `dist/dd-run-0.1.0.exe`（34M）。**真机验收项（E1–E3 对照）**：① 右键应用行（有路径 subtitle）弹 5 项菜单、命令/设置行弹单项；② Esc/点击空白/滚轮关闭；③ 菜单不溢出面板（右下角行右键验证翻转夹紧）；④ Shift+F10 对选中行开菜单 + ↑↓/Enter/Esc 键盘链路；⑤ 以管理员身份运行弹 UAC、取消 → Error Toast；⑥ 打开所在位置 explorer 定位；⑦ 复制路径 → Toast + 剪贴板粘贴验证；⑧ 亮暗双主题截图（shadow8/描边/hover 色）。

**当前状态（2026-09-05 11:30，4.2 真机反馈修复，未 commit）**：**两项修复**：① 菜单开着时右键另一行不再失效——根因 = 全屏捕获层（Foreground `Sense::click()` 同时感知两键）吞掉了目标行的 `secondary_clicked`，行侧永远收不到右键；修复 = `draw_list` 每帧存档行矩形（新字段 `ctx_row_rects`），捕获层捕获 `secondary_clicked()` 后关闭旧菜单并按本帧行矩形命中（`reopen_ctx_menu_at`：置选中 + 就地重开，锚点仍 +2,2；未命中行则仅关闭）。② 移除「UI 验收：PNG 图标」演示行（M5 批次 2 的 Path 图标验收项已历史使命完成）——删除 `system.rs` 的 `DEMO_ICON_ITEM_ID` / `DEMO_ICON_BYTES` / `demo_icon_path` / `data_cache_dir` / 顶层 push / decide 演示分支 / 3 个 demo 测试（`top_level_has_png_path_icon_demo_with_existing_asset` / `demo_item_invoke_toasts_without_confirm` / `demo_icon_asset_is_decodable`）及 `crates/dd-ext/assets/ui-accept-icon.png` 资产；`image` 依赖保留（apps.rs 测试仍在用）。**验证**：`fmt` 干净、`clippy --workspace --all-targets` 0 告警、全 workspace **216 测试全绿**（219 − 3 个删除的 demo 测试）；`dist/dd-run-0.1.0.exe` 已重打包。真机复验项：菜单开着右键另一行就地重开、右键空白处仅关闭、顶层列表不再出现演示行。

**当前状态（2026-09-05 11:46，4.2 真机反馈修复二，未 commit）**：**三项修复**：① 发布版启动弹命令行——根因 = bin 为 console 子系统；修复 = `main.rs` 顶部 `#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]`（release 无控制台；debug/测试构建保留控制台看 eprintln 探针与冷启动计时）。② `path 图标读盘/解码失败` 每帧刷屏——根因 = 失败只回落占位 glyph 不缓存，每帧重读盘；修复 = 新增负缓存字段 `PaletteApp.icon_failed: HashSet<String>`，失败路径记入后本次会话直接占位、不再重试（eprintln 只打一次）。③ 「UI 验收」demo 项残留在列表——根因 = 内置扩展桩缓存键 = 宿主包版本（`builtin.rs::builtin_version` = `CARGO_PKG_VERSION` 0.1.0 未变），`system.rs` 删 demo 后旧桩 `com_ddrun_system.0_1_0.json`（6 命令，指向已删资产）持续命中；按设计「宿主升级即桩缓存自然失效」，开发期一次性定向清除 `%APPDATA%\dd-run\cache\com_ddrun_system.0_1_0.json` + `ui-accept-icon.png`（下轮冷启动重新拉起并落 5 命令新桩）。**验证**：fmt 干净、clippy 全仓 0 告警、216/216 全绿；`dist/dd-run-0.1.0.exe` 重打包。真机复验：双击 dist exe 不再弹命令行、System 列表 5 条命令且无失败日志刷屏。

**当前状态（2026-09-05 11:53，4.2 真机反馈修复三，未 commit）**：**两项根因修复**：① 启动弹**多个**命令行窗口——根因 = 上一轮把宿主改成 windows 子系统后，console 子系统的 5 个扩展子进程不再有父控制台可继承，各自弹独立控制台（此前被宿主 console 掩盖）；修复 = `dd-host::process::ExtensionProcess::spawn` 增加 `CREATE_NO_WINDOW`（0x0800_0000，cfg windows；stdio 全 piped，不影响 NDJSON 通道）。② 「UI 验收」行未消失——根因 = 内嵌扩展物化标记 = 宿主版本号（未变）→ `%APPDATA%\dd-run\cache\embedded\` 里的旧 `dd-ext-system.exe` 一直被 spawn，又落回 6 命令桩污染列表；修复 = `dd-gui::embedded::host_marker` 改为**内嵌内容 FNV-1a 指纹**（over 文件名+字节，内存计算无磁盘读开销）：内嵌字节任何变化都触发重写，开发期迭代不再需要手动清 embedded 缓存（同时删除被旧 exe 重新污染的 `com_ddrun_system.0_1_0.json` + `ui-accept-icon.png`）。**验证**：fmt 干净、clippy 全仓 0 告警、216/216 全绿；重打包。真机复验：启动零控制台窗口、System 组 5 条命令。

**当前状态（2026-09-05 12:20，托盘设计稿 v4.5，未 commit）**：**设计稿先行——新增 v4.5 10C「系统托盘」章（只改设计稿 + HTML 交互演示，代码留后续批次，D26）**。动机（真机驱动）：应用常驻后无鼠标入口、无常驻可见性、且无任何显式退出路径。核心决策：D22 图标 = 应用 .ico（16/20/24/32 = DPI 档；**前置依赖：当前 exe/build.rs 无任何图标资源，实施批次须先产出多尺寸 .ico 嵌入 exe**），无 badge 无状态；D23 左键单击 = toggle 面板（与热键 Win+Alt+Space 同一语义，不区分单击双击）；D24 右键 = **系统原生菜单**（Win32 TrackPopupMenu，Win11 自渲染，不自绘 Fluent——宿主是 shell 非面板；须前置 SetForegroundWindow 否则外点不关闭），固定 4 项（显示/隐藏面板、设置、─、退出），**退出 = 唯一显式退出入口、不二次确认**（低危）；D25 Tooltip 静态「dd-run — Win+Alt+Space 呼出」；D26 本批只改设计稿。协议 v1.0 冻结不变、零字段新增（托盘为宿主平台层能力，落点对齐 refactor-layering-plan 的 platform.rs 分层，建议独立 tray.rs）。文档改动：`cmdpal-ui-mockups.html` 新增 10C 章（10C.0 静态解剖 + 10C.0b 交互演示：左键 toggle/右键原生菜单/退出演示态 + 重置按钮；10C.1 规格表；10C.2 菜单项映射表）+ 决策 D22–D26（00.2）+ 事实来源（00.4：Shell_NotifyIcon/TrackPopupMenu 官方文档 + hotkey.rs/build.rs 现状核验）+ 11 组件映射表托盘行（无 Fluent 组件 = OS shell 表面）+ 12 验收表新增 F1–F3 组（v4.4→v4.5、01–10B→01–10C 全量同步）。真机验收预留 F1–F3（图标 DPI 档位/常驻、左键 toggle 与热键逐项一致、右键菜单 4 项与退出语义）。

**当前状态（2026-09-05 13:05，图标定稿 10C.3，未 commit）**：**应用图标视觉定稿（结合应用名，v4.5 同批细化，仍未动代码）**——「DD 快进」字标：两个大写 D 并排 = 应用名首音节 dd，轮廓同构快进符号（▶▶ = run 语义）。规格（10C.3）：32×32 网格、圆角 7.5（≈23% Win11 惯例）、渐变 #3AA0FF→#0F5FC0（brand 蓝亮化档，图标资产独立于主题 token）、mark = 竖杠 3 + 半圆 r6 × 2、间距 2、居中 x6–26；16px 适配（杠 1.5px/半圆 3px/间距 1px）；ico 产出 16/20/24/32/48（DPI 档）+ 256（安装器）。文档改动：`cmdpal-ui-mockups.html` 新增 10C.3 节（112px 定稿渲染 + 48/32/24/20/16 多尺寸缩略 + 深/浅任务栏对照 + 几何规格表 + 落选方案记档：搜索框占位/d»混排/wordmark）；10C.0 与 10C.0b 的占位 SVG 全量替换为新字标（复用同一渐变 id）；D22 决策文本、头部 v4.5 说明、00.4 事实来源同步。实施批次依赖不变：按 10C.3 几何导出多尺寸 .ico → 嵌入 exe 资源 → 托盘 LoadImage 复用。

**当前状态（2026-09-05 13:25，托盘 10C 实施，未 commit）**：**设计稿 v4.5 10C「系统托盘」egui/Win32 侧全量落地**。新增 `crates/dd-gui/src/tray.rs`（lib 模块，对齐 hotkey 的「独立线程 + channel + request_repaint」模式）：TrayEvent{Toggle, OpenSettings, Exit}；线程内 GetModuleHandleW/RegisterClassW/CreateWindowExW 建**隐藏窗口**（不 ShowWindow）→ `ensure_icon_file` 把 `include_bytes!` 的 `assets/app.ico` 物化到 `cache_dir()/app.ico`（内容 diff 幂等）→ `LoadImageW(LR_LOADFROMFILE)` 按 DPI 取档 HICON（GetDC+LOGPIXELSX 取 DPI，96/120/144/192→16/20/24/32，GWLP_USERDATA 注入 TrayState）→ `Shell_NotifyIconW(NIM_ADD, NIF_MESSAGE|NIF_ICON|NIF_TIP)`，Tooltip=D25 静态「dd-run — Win+Alt+Space 呼出」→ GetMessage 循环。窗口过程：TRAY_MSG(=WM_APP+1) 经典模式，WM_LBUTTONUP→Toggle（D23）、WM_RBUTTONUP→show_menu（**SetForegroundWindow 前置 + CreatePopupMenu/AppendMenuW 4 项（accel 经 \t）+ TrackPopupMenu(TPM_RETURNCMD) + DestroyMenu + PostMessage(WM_NULL)**，D24）；WM_DESTROY→PostQuitMessage。接线：PaletteApp 增 `tray_events` 字段 + `poll_tray`（logic() 中，隐藏时也消费）——Toggle=与热键同款 toggle；OpenSettings=隐藏时先 show()（复位到 Root）再 open_settings()、可见时直接推设置页（同 Ctrl+,）；Exit=ViewportCommand::Close→run_native 返回→进程结束（唯一显式退出入口，不二次确认）。图标资产：`tools/gen_icon.py`（Pillow，venv）按 10C.3 几何 256 母版 LANCZOS 缩 48/32/24/20/16，BMP 条目 ICO（18526B）+ `assets/app.ico` 入库。**偏离记档**：.ico 不走 winres（gnu 无 windres）→ 字节内嵌+物化；256 档留打包批次；托盘失败降级不 panic（区别热键 fail-fast）。**验证**：`cargo fmt --all --check` 干净、clippy 全仓 0 告警（touch 强制重编）、workspace **221 测试全绿**（216 基线 + tray 新增 5：DPI 档位/菜单映射/ico 五档条目/Tooltip 长度/to_wide）；package.sh 重链 `dist/dd-run-0.1.0.exe`（9.4M）。windows-sys 增 feature `Win32_System_LibraryLoader`（GetModuleHandleW）。**真机验收项（F1–F3 对照）**：① 启动后托盘图标出现（DPI 档位正确、Tooltip 正确）；② 左键单击呼出（焦点在搜索框、光标屏居中）再击隐藏，与热键逐项一致；③ 右键菜单 4 项：显示/隐藏面板（Win+Alt+Space accel）、设置（进设置视图）、退出；④ 菜单外点击可关闭（SetForegroundWindow 生效）；⑤ 「退出」结束进程且托盘图标消失；⑥ 隐藏面板 ≠ 退出（进程常驻、托盘仍在）；⑦ 托盘不可用场景（注册失败）不影响热键使用（降级日志一次）。

**当前状态（2026-09-05 14:58，托盘 toggle 竞态修复，未 commit）**：**真机反馈修复——托盘左键第二次点击「闪黑又展示」**。根因 = 托盘 Toggle 与失焦自动隐藏的竞态：面板可见时点击托盘，任务栏在鼠标按下瞬间夺焦 → `handle_focus_loss` 先 `hide()`（闪黑），随后 `WM_LBUTTONUP` 的 `Toggle` 到达时 `visible==false` → 又 `show()`。双保险修复：① **点击在途旗标**——`Arc<AtomicBool>` 由 tray.rs 发送 `Toggle` 前置位（左键 + 菜单「显示/隐藏面板」），`handle_focus_loss` 遇旗标跳过本次失焦隐藏，`poll_tray` 消费 Toggle 后复位（旗标与事件严格成对，无陈旧风险）；② **失焦隐藏时间戳兜底**——鼠标按下（夺焦）可能早于抬起（WM_LBUTTONUP），旗标来不及置位时 `handle_focus_loss` 照常 hide 并记 `last_focus_loss_hide`，`poll_tray` 的 Toggle 发现面板刚因失焦隐藏（<300ms）则维持隐藏不再 show。两条路径收敛为「恰好一次 hide、零次 show」，任意时序均无闪黑。改动：tray.rs（TrayState 增 click_flag、spawn 签名增参）、app/mod.rs（增 tray_click_flag + last_focus_loss_hide 字段）、app/lifecycle.rs（poll_tray Toggle 分支 + handle_focus_loss 跳过 + hidden_by_recent_focus_loss）、main.rs/test_support.rs（注入旗标）。已知可接受边界：用户点击别处失焦隐藏后 300ms 内点托盘想再呼出会被抑制（保持隐藏），再次点击即正常呼出。验证：fmt 干净、clippy 0 告警（touch 重编）、221/221 全绿；dist 重打包。真机复验：面板可见时连点托盘 = 干净隐藏无闪黑；托盘呼出/热键呼出行为不变。

**当前状态（2026-09-05 15:10，隐藏闪黑优化，未 commit）**：**真机反馈修复——面板隐藏时「先变黑再隐藏」闪一下**。根因（读 eframe 0.36.1 `src/native/glow_integration.rs::run_ui_and_paint` 确认顺序）：窗口**仍可见**时 eframe 先 `clear(clear_color)` → 跑 `update()`（我们的 `logic()` 里 hide 后 `ui()` 因 `!visible` 空帧返回）→ **present 该纯色帧** → 之后才把 `ViewportCommand::Visible(false)` 应用到 winit 窗口。所以隐藏必然多 present 一帧纯色（暗色主题下即黑）= 闪黑。修复：新增 `PaletteApp.paint_hide_frame` 旗标——`hide()` 置位，`ui()` 在 `!visible && paint_hide_frame` 时**仍绘制一次真实面板内容**（`draw_panel`，不做交互处理）后复位并 return；该帧 present 的是面板本身，随后窗口隐藏 → 视觉上直接消失，无纯色帧。`show()` 复位旗标防残留。改动：`app/mod.rs`（字段 + ui 分支）、`app/lifecycle.rs`（hide 置位 / show 复位）。验证：fmt 干净、clippy 0 告警、221/221 全绿；dist 重打包。真机复验：Esc / 热键 / 托盘左键 / 失焦 四条隐藏路径均无闪黑。

**当前状态（2026-09-05 17:02，设置页设计稿 v4.6，未 commit）**：**设计稿先行——重构 v4.6 08「设置页」为左栏分组 + 右侧设置项（只改设计稿 + HTML 交互演示，代码留后续批次，D29）**。动机（用户决策）：设置项随功能增加持续增多（已 3 实际 + 3 占位），单列卡片滚动页过长；改为**左栏分组导航 + 右侧设置项**（Fluent NavigationView 语义），分组为后续新增配置预留归组位。核心决策（用户三问三答确认）：D27 信息架构 = 左栏固定四类「**外观**（主题外观）/ **常规**（打开面板时显示 + 全局热键·开机自启占位）/ **搜索**（搜索引擎）/ **扩展**（扩展管理占位）」，点击切换右栏、默认选中首栏、进入设置页重置、栏目纯视图状态不落盘不改协议；D28 窗口 = **640×640**（替换 v4.2 的 560×640——左栏占 168 后内容区保持 ~440px；根页/子页仍 560×460，is_settings 帧间 diff 放大/缩回机制不变）；D29 本批只改设计稿 + HTML 演示，egui `draw_settings`（`ui/settings_view.rs`）左右布局实施留后续批次。左栏规格：项高 36、圆角 4、图标 16 + 文字 14/20、选中 = row-selected 实色底 + 左缘 3×16 accent 指示条（与列表行选中语言 D9 同构）、零新 token。文档改动：`cmdpal-ui-mockups.html` 重写 08 章（交互式演示：左栏四类 + 右栏四组 settings-view，点击/↑↓ 切换脚本；8.1 规格表全量重写：窗口/顶行/布局/左栏/栏目映射/卡片/主题单选/搜索引擎卡/占位项/键盘 十行）+ 新 CSS（`.settings-panel/.settings-split/.settings-nav/.settings-content/.engine-*`）+ 决策 D27–D29（00.2）+ 事实来源 v4.6 增补（WinUI NavigationView）+ 00.1 面板骨架设置页例外更新为 640×640 + 11 组件映射表设置页行（NavigationView）+ 12 验收表 B 组扩充至 B1–B6（左栏几何/栏目映射/窗口尺寸）+ A5 零位移措辞修正（设置页为唯一尺寸例外，消除与 00.1 的既有矛盾）+ 版本沿革 v4.6（title 顺带修正 v4.4→v4.6 陈旧值）。**顺带补齐文档缺口：「搜索引擎」卡片此前未进稿**（代码 2026-09-05 已实现于 settings_view.rs），本轮补齐演示与规格（预设勾选/自定义添加删除/校验错误/DD_WEBSEARCH_ENGINES 通道）。旧稿中设置页 body 内的「Esc 返回 | Ctrl+, 打开设置」键帽行一并移除（批次 4.0 起键位提示已由全局页脚统一渲染，演示稿与实现对齐）。真机验收预留 B4–B6（实施批次执行）。

**当前状态（2026-09-05 17:46，设置页 v4.6 实施，未 commit）**：**设计稿 v4.6 08「设置页」egui 侧全量落地**——`draw_settings` 重构为左右布局（D27/D28），真机验收 B4–B6 通过。改动三文件：① `ui/settings_view.rs`：新增 `SettingsCategory`（Appearance/General/Search/Extensions，`pub(crate)` 纯视图枚举 + `NAV_CATS` 栏目表 + `NAV_W=168/NAV_ITEM_H=36/NAV_GAP=4/SPLIT_GAP=8` 几何常量）；`draw_settings` 顶行保留（真机 2026-09-04 手动锚定中心线修复原样继承），顶行下方改 `[左栏 168][间距 8][内容区 flex 1]` 手动分栏（`available_rect_before_wrap` + 两个 `new_child` max_rect 子区，右/下留 12px），左栏项 = `allocate_exact_size(168×36, click)` + row_selected 实色底 / row_hover + 左缘 3×16 accent 指示条（radius 2，D9 同语言）+ 图标 16px（左+20 中心）+ 文字 14px（左+40）；内容区独立 `ScrollArea`，按 `settings_category` match 分发到四个新方法 `draw_appearance_card`（主题三选卡，原卡 #1 逐字迁移）/ `draw_general_cards`（打开面板时显示 + 热键/自启占位卡，原 #1.5 + #2 拆分）/ `draw_search_engine_card`（引擎卡，原 #1.7 逐字迁移，`ctx` 经参数传入）/ `draw_extensions_card`（扩展管理占位卡）；原四卡闭包内容逐字保留（真机 2026-09-04 item_spacing.x 清零 / 卡内实测三联卡宽度等修复不回退）；本期键盘口径 = 点击为唯一栏目切换交互，↑↓ 焦点切换留可选增强（8.1 键盘行）。② `app/mod.rs`：`SETTINGS_W` 560→**640**（D28，H 仍 640）；`PaletteApp` 增 `settings_category` 字段（构造置 default）。③ `app/keys.rs`：`open_settings()` 推页前 `settings_category = default()`（每次进入重置首栏「外观」，B5；Ctrl+, / 齿轮 / 托盘三入口同收口）。新增 2 单测（默认选中外观 / NAV_CATS 顺序标签与 8.1 映射一致）。**验证**：`cargo fmt --all --check` 干净、clippy 全仓 0 告警、workspace **234 测试全绿**（dd-gui 115）；`package.sh` release 重链 `dist/dd-run-0.1.0.exe`（9.5M）。**真机验收（B4–B6 对照，Win+Alt+Space 唤起 + Ctrl+, 实测）**：① 设置页 640×640、Esc 返回缩回 560×460，帧间 diff 无跳动；② 左栏四项 168 宽/36 高/圆角 4，选中 = 实色底 + 3×16 accent 指示条 + 文字转色（放大截图核对）；③ 四栏点击切换逐项正确（外观=主题三选卡且「跟随系统」按 config 选中 / 常规=打开面板时显示 checkbox 未勾（open_view=default）+ 两禁用占位 / 搜索=引擎卡仅 Bing 勾选（与 config.json 一致，放大核对排除误读）+ 自定义行与添加表单 / 扩展=管理占位卡）；④ Esc 后 Ctrl+, 重新进入重置到「外观」（B5）；⑤ 页脚「设置修改自动保存；搜索引擎更改返回首屏后生效 + Esc 返回」语义不变；用户配置零改动（验证全程只点纯视图栏目，未碰主题 radio / checkbox）。**环境注**：本机 rustup 默认工具链被重置回 msvc（`.cargo/config` 仓库注记要求 gnu 为默认），本轮以 `cargo +stable-x86_64-pc-windows-gnu` 命令级覆盖构建，未改全局默认（如需还原：`rustup default stable-x86_64-pc-windows-gnu`）。
