# dd-run 实施方案

> **状态**：M0 已完成（`dd-protocol` / `dd-host` / `dd-ext-sample` / `dd-run` CLI 均已落地，40/40 测试全绿）；M1（`dd-gui` 窗口骨架 + 热键 + 首屏聚合 + 键盘全流程）已关闭（2026-09-02 真机人工验收通过，A1/A11/A12 全过，见 [`./m1-record.md`](./m1-record.md)）；M2（命令执行 + 8 种 Kind 状态机 + 页面栈 + UI 接线）已关闭（2026-09-02 十项真机复验全部通过，A4/A5/A9 达成，24/24 单测 + 全 workspace 构建无回归，见 [`./m2-record.md`](./m2-record.md) §4.5）。M3（缓存与懒加载）逻辑层 `cache.rs` + 协议 `get_command` 接线 + UI 接线（冷启动读桩不拉起 / 桩复热 / LRU 保活 / A2 计时）已完成并通过工程验收（73/73 测试全绿）；**真机反馈 5 处修复已落地**（◌ 补 seguisym 字体后援、空态改 vertical_centered 不撑满页脚、#5 步骤改写、A2 拆计时定位瓶颈、列表长时页脚移至 `Panel::bottom` 独立底栏，见 [`./m3-record.md`](./m3-record.md) §3.4），**A6/A2 真机复验已通过（2026-09-02，见 [`./m3-record.md`](./m3-record.md) §4）**。
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
| 平台适配 | Apps 索引按 OS 分路径（Win `.lnk`+PATH / macOS `/Applications` / Linux `.desktop`+PATH）；System 与 Shell 按 OS 分命令 |
| 崩溃恢复 | stdout EOF / 非 0 退出码检测 → in-flight 请求立即失败 → stub 回退 → 宿主继续运行 |
| 连续崩溃保护 | 连续 N 次（建议 3）后标记"暂时不可用"，宿主重启或手动重试才恢复 |
| 能力注入接 UI | `host/show_status`（Toast）、`host/set_clipboard`、`host/open_url` |
| 过滤性能 | 模糊匹配（如 `nucleo` / `skim`）；帧耗时采样埋点 |

**完成判据**：
- 故障注入（kill 子进程）后宿主不退出、可恢复（A8）——P1 代码完成，待真机复验（m4-record §4 #1–#3）；
- 5 个内置扩展功能清单核对通过（A10）——P4 扩展侧 + 宿主 fallback 轮代码完成（146/146 全绿，
  含无匹配渲染 + `{query}` 替换 + `context.query` 透传），真机清单见 m4-record §4 #9–#13；
- 实测结果列表过滤帧耗时，记录是否达成 A3 的 16ms/帧目标（**未达成则记录实测值**）——P5。

**验收映射**：A8、A10、A3（实测）。

---

## 3. 验收映射总表

| 验收项 | 内容 | 里程碑 |
|---|---|---|
| A1 | 全局热键可唤起/隐藏 | M1 |
| A2 | 冷启动首屏 < 200ms（**目标值，需实测**） | M3 实测 |
| A3 | 输入过滤 < 16ms/帧（**目标值，需实测**） | M4 实测 |
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
| M4 内置扩展与健壮性 | 🟨 进行中（P1–P3 完成，待真机） | 实施决策已定（2026-09-02，见 [`./m4-record.md`](./m4-record.md) §0）：D1 共享扩展运行时 / D2 本轮只做健壮性基础层 / D3 过滤用 nucleo。**P1–P3 代码完成**：崩溃恢复链 A8（`refresh_health` 每帧检测 + poll 死进程丢弃回落 stub）/ `host/*` 执行端接 UI（Toast + arboard 剪贴板 + webbrowser，空闲轮询也应答 HostRequest）/ 连续崩溃保护 §11（`robustness.rs` CrashGuard 熔断 + dispatch 拦截）。全 workspace **79/79 测试** + clippy/fmt 0 告警。P4（5 内置扩展 A10）/ P5（A3 nucleo 过滤）后续轮次 |

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
| R3 | **A3 < 16ms/帧** 在大结果集下可能不达标 | M4 实测；考虑异步过滤或结果截断 |
| R4 | 上游 PowerToys 文档引用边界需复核（许可证本身已定） | 已采用 **MIT**（根 `LICENSE` + 各 crate `license = "MIT"`），与 README 声明一致；引用边界复核留待专项一轮 |
| R5 | 设计文档 §7 中 **9 个 `🪟` Windows 专属扩展**不可移植 | 已在 §7 加平台列标记；MVP 不纳入 |
| R6 | CmdPal 仍处于 **preview**，上游接口可能演进 | 设计文档已标注 ✅ 核验日期；协议 v1.0 冻结后以本协议为准 |
| R7 | 设计稿字体依赖 Google Fonts（国内可能不可达） | 已改为本地优先分层字体栈（Archivo → Segoe UI Variable Display → …），CDN 仅作渐进增强 |

---

## 7. 下一步

**M1 已关闭**（2026-09-02 真机人工验收 11 项全过，R1 通过、ADR-2 成立，见 [`./m1-record.md`](./m1-record.md)；残留启动一帧闪屏，见其 §4.6，未排期）。

**M2 已关闭**（2026-09-02 十项真机复验全部通过，A4/A5/A9 达成，见 [`./m2-record.md`](./m2-record.md) §4.5）。

**M3 已关闭（2026-09-02 真机复验 A6/A2 通过）**（逻辑层 `cache.rs` + `get_command` 协议接线 + UI 接线 + 5 处真机反馈修复——◌ 字体、空态撑满、#5 步骤、A2 拆计时、列表长时页脚被挤出窗口（移至 `Panel::bottom` 独立底栏）；73/73 测试全绿，见 [`./m3-record.md`](./m3-record.md) §3.4 / §4）：用 16:05 版 `./target/x86_64-pc-windows-gnu/debug/dd-gui.exe`（终端启动）按 §4 清单复验——首启落盘 → 重启读桩不拉起（A6，日志+页脚 ◌ 正常渲染，**页脚现在独立底栏**始终可见）→ 点击桩项复热成功（Invoke/Page 两条路径）→ 复热失败回退 stub（**先启动再改名 exe / 杀进程**）→ 记录 A2 冷启动分项耗时（`total = data_ready + gui_init`，15:53 实测 `2 ms + ~2861 ms`——瓶颈在 wgpu+msyh 22MB 字体加载）。**已进入 M4 内置扩展与健壮性**。

**当前状态（2026-09-03 09:5x）**：M1 已关闭；M2 已关闭；M3 已关闭（已提交推送，commit `0c42465`）。**M4 进行中**（见 [`./m4-record.md`](./m4-record.md)）：实施决策 D1–D3 已确认；P1–P3 健壮性基础层代码完成（崩溃恢复链 A8 / `host/*` 执行端接 UI / 连续崩溃保护 §11，79/79 全绿）；**P4 扩展侧完成**（补充决策 D4–D7：扩展侧先行 / 宿主内存自注册 / Windows 优先 / 内置取代 sample）——`dd-ext` 共享运行时 + 5 内置扩展（Apps/Calc/System/WebSearch/Shell）+ `dd-host/builtin.rs` 注册表 + `fallback_commands()`/`invoke()` 方法封装 + `roundtrip_builtins` 6 项全链路往返；**宿主 fallback 轮完成**（补充决策 D8–D10：§6.3 含兜底者视为 fresh / 全局无匹配触发 / 模板拉一次缓存）——`BuiltinSpec` 拆 `frozen`(自述) 与 `host_frozen`(策略)、`FrozenCache::remove`、`load_one` 按 `provider.has_fallback` 跳过落桩、`FallbackStore` 模板缓存与 `{query}` 渲染、`PanelState` 无匹配分流、main.rs 拉取/轮询接线。**全 workspace 146/146 测试全绿**（A10 清单真机核对见 m4-record §4 #9–#13，不再拆批）；P5 nucleo 过滤后续轮次。
