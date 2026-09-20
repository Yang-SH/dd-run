# dd-run 实施方案

> **状态**：生效中 ｜ **版本**：v0.1.1 ｜ **最后更新**：2026-09-15
> **关联**：[protocol.md](./protocol.md) · [manifest-schema.md](./manifest-schema.md) · [extensions.md](./extensions.md) · [../cmdpal-platform-agnostic-design.md](../cmdpal-platform-agnostic-design.md)

---

## 里程碑状态总表

> 结论先行：下表 30 秒内可读完。各里程碑的过程细节、真机修复与踩坑记档见对应 `mX-record.md`；本文件只保留最终结论与关键决策。

| 里程碑 | 状态 | 关闭日期 / commit | 一句话结论 | 详情 |
|---|---|---|---|---|
| M0 地基与协议冻结 | ✅ 已关闭 | 2026-09-01 | workspace 可构建，协议 v1.0 冻结，40/40 测试全绿 | [m0-record](./m0-record.md) |
| M1 最小可用面板 | ✅ 已关闭 | 2026-09-02 | 热键唤起 + 全键盘走通，R1/ADR-2 真机验收通过 | [m1-record](./m1-record.md) |
| M2 命令执行与状态机 | ✅ 已关闭 | 2026-09-02 | 8 种 Kind + 页面栈 + invoke，24/24 单测，A4/A5/A9 达成 | [m2-record](./m2-record.md) |
| M3 缓存与懒加载 | ✅ 已关闭 | 2026-09-02 | frozen 桩 / LRU / 冷启动计时，73/73 测试，A6/A7/A2 达标 | [m3-record](./m3-record.md) |
| M4 内置扩展与健壮性 | ✅ 已关闭 | 2026-09-04 `757f3b4` | 5 内置扩展 + 崩溃恢复 + nucleo 过滤（A3 实测 3.7ms），A8 真机通过 | [m4-record](./m4-record.md) |
| M5 ueli 风格 UI 重构 | ✅ 已关闭 | 2026-09-04 `5cf32b7` | 换肤/图标/设置/右键/类型标签/C 组占位全落地（含 L8） | [§2 M5](#m5--ueli-风格-ui-重构插队于-m4-后) |
| M6 日常打磨 | ✅ 已关闭 | 2026-09-08 | 拼音搜索 / 冷启动字体 / 设置占位 / 遗留清扫 / 三子页优化 | [§2 M6](#m6--日常打磨2026-09-05-立项) |
| M7 发布工程 | ✅ 已关闭 | 2026-09-10 `v0.1.1` | CI / 256px 图标 / sidecar 定案 / tag 演练发版（安装器取消） | [§2 M7](#m7--发布工程2026-09-08-立项) |
| M8 扩展生态验证 | ✅ 已关闭 | 2026-09-10 | 非 Rust 扩展走通协议全链路（10 项全 ✓），指南 + conformance | [§2 M8](#m8--扩展生态验证2026-09-10-立项) |
| M9 内置扩展进程内化 | ✅ 已关闭 | 2026-09-11 | 5 内置扩展改 in-process，单文件分发，进程隔离仅第三方 | [m9-inprocess-builtins](./m9-inprocess-builtins.md) |
| v5.2 UI 优化 B1–B8 | ✅ 已关闭 | 2026-09-13 `815e212..da0589c` | 字重/强调色/滚动条/下划线/结果行/设置页/悬停/自适应 | [cmdpal-ui-optimization-v5.html](../cmdpal-ui-optimization-v5.html) |
| 图标与字体优化 I1/I2/F1/F2 | ✅ 已关闭 | 2026-09-12 `3545d3f` | 占位 glyph / I2 显式色 / F1 token 化 / F2 密度三档（I3 未做） | [icons-typography-plan](./icons-typography-plan.md) |
| O2 协议方法名常量层 | ✅ 已落地 | 2026-09-14（已提交 `6ee4931`） | 12 个方法名收敛为 `dd-protocol::methods` 单一来源，全仓生产代码改用常量 + 一致性测试锁 SSOT | [§2 O2](#o2--协议方法名常量层2026-09-14-落地) |
| O1 协议错误码接线 | ✅ 已落地 | 2026-09-15（已提交 `c26035e`） | `-32600`/`-32700` 处置收口为 `dd-protocol::envelope` 单一来源；超限帧由静默丢弃改为「回错 + 关连接」（协议 §2.3/§9.3 补齐，v1.0 零改动） | [§2 O1](#o1--协议错误码接线2026-09-15-落地) |
| O3 依赖治理 | ✅ 已落地 | 2026-09-15（已提交 `c26035e`） | CI 新增 `audit` job（`cargo audit` 漏洞 + `cargo machete` 死依赖）+ Dependabot 管更新；本地实跑均零发现 | [§2 O3](#o3--依赖治理2026-09-15-落地) |
| O8 文档卫生 | ✅ 已落地 | 2026-09-15（已提交 `c26035e`） | CHANGELOG `[0.1.0]` 补日期、行尾确认合规；I3（真机依赖）与 LRU 容量（功能项）经取证不在本项范围，已逐条记档 | [§2 O8](#o8--文档卫生2026-09-15-落地) |
| O4 可观测性/日志 | ✅ 已落地 | 2026-09-15（已提交 `c26035e`） | `log` facade + 自写 stderr 后端；131 处 `eprintln!` 分级（debug 85/warn 39/info 7）；`DDRUN_LOG` 开关实测生效，**默认 debug = 与改造前逐行等价** | [§2 O4](#o4--可观测性日志2026-09-15-落地) |
| O7 文件搜索 P2 真机验收 | ✅ 已落地（两轮完成） | 2026-09-15 / **2026-09-17**（已提交 `c26035e` + `9936ead`；第二轮工作副本） | A-33-05 / A-33-06 / A-33-07 / A-33-10 **通过**（基线降幅 91.27%、回落分支 3/3 返回真实结果）；A-33-08 ⚠️ 部分（跨环境矩阵）＝唯一红线来源；**同批修 `es.exe` 失败静默为空结果的缺陷** | [验收报告](./search-file-p2-acceptance-2026-09-15.md) |
| 协议两项定稿（`-32002` / `PageInfo`） | ✅ 已定稿 | 2026-09-17（工作副本） | `-32002` 判为**扩展侧可用错误码**（宿主不产出）、`PageInfo` **维持不传递**；`protocol.md` §9.2/§8.5 注记改终态口径，INDEX §5 两项关闭 | [§2 协议两项定稿](#协议两项定稿-32002--pageinfo2026-09-17-定稿) |
| 文件搜索 `Ctrl+F` + Shell 真实图标（v3.6） | ✅ 已落地（两项未达标已处置：E1 / E2） | 2026-09-19（工作副本） | 宿主 `Ctrl+F` 任意页直达文件搜索（栈深恒 2、Root 查询带入）；文件结果图标改 **Shell 真实图标**（`Icon::Path` + `file-icons/` 落盘缓存，失败回落 glyph）；图标管线自 apps 上移共享；协议/清单零改动。未达标两项已处置（E1：首抽 703.8 ms → 同步 0.32 ms；E2：sidecar +104.5 KB → 增量 61,952 B ≤ 64 KB） | [§2](#文件搜索-ctrlf-直达--shell-真实图标v36-2026-09-19-落地) · [search-file §10](./search-file.md) |

## 产物与分发（2026-09-13 实测）

- `dist/dd-run-0.1.1.exe` — **8,601,088 B**（单文件宿主；含 5 内置扩展 in-process；M9 起单文件分发）
- `dist/extensions.d/dd-ext-search.exe` — 749,568 B（文件搜索 sidecar，随绿色包 `extensions.d/` 携带）
- `dist/extensions.d/com.ddrun.filesearch.json` — 421 B（sidecar 清单）
- `dist/dev/dd-run-cli.exe` — 1,203,200 B（开发 / 自检工具，`--conformance` 全表面自检）
- `dist/dev/dd-ext-sample.exe` — 432,640 B（示例扩展）
- ⚠️ **不存在 `dist/dd-run.exe`**：旧流水账曾误称该名。实际发版产物名为 `dist/dd-run-<ver>.exe`（crate 仍名 `dd-gui`，经 `[[bin]] name = "dd-run"` 产出 `dd-run-<ver>.exe`），`dist/dev/` 下两个为开发期产物。

---

## 1. 目标与范围

本方案把设计文档中的抽象模型拆成**可依次交付的里程碑**。每个里程碑都有明确的完成判据，判据直接映射到设计文档 §10 的验收项 A1–A12。

**MVP 范围**（M0–M4 全部完成即 MVP）：

- 全局热键唤起的跨平台面板（Windows / macOS / Linux）
- 4 类页面中的 **ListPage + DetailPage**（设计文档 §4.5）
- 5 个内置扩展：Apps / Calc / System / WebSearch / Shell
- 扩展机制：**清单发现 + 子进程 JSON-RPC（第三方 / sidecar）**，**内置扩展走 in-process**（见 ADR-1）；支持第三方扩展
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
| Cargo workspace | crates：`dd-run`（宿主 bin）/ `dd-protocol`（协议类型 + NDJSON 读写）/ `dd-host`（宿主逻辑：进程管理 / 缓存 / 页面栈）/ `dd-ext-sample`（示例扩展） |
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
| LRU warm 集合 | 容量 N（实现为编译期常量 `LRU_WARM_CAPACITY = 8`；「可配置」未实现，Settings 无对应字段）；超出则 `close` + 终止进程，命令重新标 stub |
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
| 过滤性能 | 模糊匹配（nucleo）；帧耗时采样埋点 |

**完成判据**：

- 故障注入（kill 子进程）后宿主不退出、可恢复（A8）——**真机复验通过**（m4-record §4 #1–#3，以 m4-record 为准）；
- 5 个内置扩展功能清单核对通过（A10）——P4 扩展侧 + 宿主 fallback 轮代码完成（153/153 全绿，含无匹配渲染 + `{query}` 替换 + `context.query` 透传）；
- 实测结果列表过滤帧耗时，记录是否达成 A3 的 16ms/帧目标——**P5 完成（2026-09-03）**：nucleo 模糊过滤 + 按分数重排（D11）+ 可见索引表/未变早退，**2000 项 ×6 字段一次重算实测 3.7ms（debug）< 16ms/帧，达标**；真机日志复核见 m4-record §3.7。

**验收映射**：A8、A10、A3（实测）。

---

### M5 — ueli 风格 UI 重构（插队于 M4 后）

基于设计稿 v2/v4（`cmdpal-ui-mockups.html`，ueli/Fluent 9 视觉语言 + 亮暗双 token），分多批推进，2026-09-04 经 `5cf32b7` 提交收尾，M6 集中回归（2026-09-08）补做 C 组（L8）真机验收。各批次最终结论见下表；真机反馈修复细节见 CHANGELOG 与对应设计稿（`cmdpal-ui-mockups.html`）。

| 批次 | 任务 | 说明 | 验收标准 |
|---|---|---|---|
| 1 | 启动黑框修复 | 屏幕外初始定位 + `with_active(false)` + 居中并入 `show()` | 唤起无黑框 |
| 2 | 图标链路 | `CommandItem.icon` 三态（glyph/path/url）→ `PanelItem` 透传 → `IconView` + 路径纹理缓存 + SegoeIcons/MDL2 回退 | 图标对齐 |
| 3 | 整体换肤 | 新建 `theme.rs`（token 唯一源 + 几何常量 + `visuals()`/`apply()`） | parity 单测 |
| 3.5 | apps 真实图标抽取 | `SHGetFileInfoW` → HICON → PngEncoder 落盘缓存 | 100% 真实图标 |
| 3.8 | 页脚底栏 + 键帽样式 | 页脚移 `Panel::bottom` 独立底栏 | 长列表不被挤出 |
| 3.9 | 搜索结果类型标签 | `result_category` 映射（apps→应用/calc→命令/…），协议层不改 | C3–C5/C9/C11/C13 |
| 4.0 | 左下角设置按钮 | 齿轮 → 同窗口子视图（PageStack 推 `SettingsPage`，Ctrl+, 键盘可达） | C1–C2/C12 |
| 4.1 | 页脚上下文动作提示 | 有选中项显示默认动作 + 快捷键，无选中回退键位图例 | C6–C8/C13 |
| 4.2 | 搜索结果右键菜单（10B） | `CtxMenuState` + 静态类别映射（协议零新增） | E1–E3 |
| 4.3 | 页脚键帽重设计（v4.10） | 白底圆角 + 描述前置 | 对齐设计稿 |
| 4.4 | 无边框窗口 chrome（D36） | 全窗拖拽 + 8 向缩放 | 前台控件零拦截 |
| 4.5 | 拖拽恢复 + 打开所在位置 fallback + 移除「选择」提示 | 手动帧检测拖拽；`reveal_in_folder` 存在性校验 | 246 passed |
| 4.6 | 面板尺寸自适应与记忆（D37） | 650×440 基准（`APP_H = 440`，初值 420、2026-09-06 真机反馈 +20）+ 拉伸落盘记忆 | G1–G5 |
| C1 | 嵌套页顶行统一 | 返回 28×28 + 搜索框 placeholder + ext_id 徽标落页脚 | A1–A5/C1 |
| C2 | Loading 骨架 | Spinner 22px + 3 骨架行 | 无布局跳动 |
| C3 | Dialog 遮罩 + Toast 意图 | 全屏 Area 捕获层 + `ToastKind{Success,Error,Info}` | A1–A5/C1–C3 |

> 设计稿 B3「热键/自启/扩展管理为禁用占位」与功能代码矛盾（2026-09-06 文档滞后校正已更正）：设置占位实际在 M6 批次 6.3 落地，非 M5。C 组（L8）真机验收于 2026-09-08 M6 集中回归 B 组通过。

---

### M6 — 日常打磨（2026-09-05 立项）

方向分析结论（用户确认）：Windows 侧「能用」已达成，按投入产出比先做**日常好用**，再做**发布工程**（M7），再做**扩展生态验证**（M8）；跨平台兑现远期启动。

| 批次 | 内容 | 验收标准 | 状态 |
|---|---|---|---|
| 6.1 拼音搜索（L4） | 汉字标题预计算「全拼 + 首字母」索引（`PanelItem.pinyin`，宿主侧、协议零改动），作为独立字段参与 nucleo 模糊匹配 | 输入 `jsq` / `jisuanqi` 命中「计算器」；单测覆盖 | ✅ 2026-09-05 |
| 6.2 冷启动优化（L10） | CJK 字体（msyh ~19.7MB + seguisym ~2.5MB）改后台线程加载，就绪后 `set_fonts` 热替换 + 重绘；首帧用默认字体 | 首帧不阻塞主路径；热替换原子无闪烁 | ✅ 2026-09-05 |
| 6.3 ✅ | 设置占位落地（全局热键 / 开机自启 / 扩展管理） | 三项均在代码层落地并真机验收通过（M6 集中回归 A 组） | ✅ 2026-09-08 |
| 6.4 ✅ | 遗留清扫 | L3 顶层 `items_changed` 重聚合接线 / L2 熔断手动重试入口 / L7 apps.rs clippy 销项；L1 闪屏 / L9 IME 真机销项 | ✅ 2026-09-08 |
| 6.5 三子页 UI 优化（v4.8） | 常规排版 / 搜索交互改版（下拉添加预设 + URL 输入自定义名称）/ 扩展排版（版本并入 id 行） | 三页真机目检 + 单测全绿 | ✅ 代码完成（2026-09-05，交互走查受自动化环境降级影响留目检） |

已做取舍记档（6.1）：多音字取 pinyin crate 默认读音（不做多读音组合）；仅标题参与拼音索引；非汉字字符跳过。已做取舍记档（6.2）：字体就绪前唤起面板时 CJK 文本短暂方块后热替换恢复。

---

### M7 — 发布工程（2026-09-08 立项）

按既定路线（日常好用 → 发布工程 → 扩展生态验证）进入 M7：把项目从「能自用」推向「能分发」。M6 剩余真机验收项以「M6 集中回归清单」集中关账——**A–F 六组已于 2026-09-08 全部真机验收通过，M6 正式关账**。

| 批次 | 内容 | 验收标准 | 状态 |
|---|---|---|---|
| 7.1 CI | GitHub Actions（windows-latest + windows-gnu）：build / test（显式跳过机器相关的 apps steam 测试）/ clippy `-D warnings` / fmt `--check` | push/PR 触发，全 job 绿 | ✅ 2026-09-08 关账（首跑 run #1，4m23s） |
| 7.2 256px 图标 | `tools/gen_icon.py` SIZES 增 256 档，重新生成 `assets/app.ico`；托盘 ICO 容器守卫测试 5→6 档 | ICO 含 16/20/24/32/48/256 六档 | ✅ 2026-09-08 |
| 7.3 安装器 | ~~Inno Setup 脚本~~ **按用户决策取消（2026-09-08）：仅提供免安装单文件绿色版** | —— | ❌ 2026-09-08 取消 |
| 7.4 文件搜索分发定案 | 决策：`dd-ext-search` 保持 sidecar，随绿色包 `dist/extensions.d/` 携带 | 决策记档 + 实施 | ✅ 2026-09-08 定案 |
| 7.5 发布流程 | tag → `package.sh` 产物 + GitHub Release 附着（dist 单文件 + sidecar 目录 zip）；**便携 sidecar 扫描落地** | code + zip 布局验证；tag 演练（v0.1.1 已发布 2026-09-10） | ✅ 2026-09-10 |

已做取舍记档（7.3/7.4）：① 安装器按用户决策取消——分发形态定案为**仅免安装单文件绿色版**（`dist/dd-run-<版本>.exe` + `extensions.d/` sidecar 目录，zip 附着 Release）；② file-search sidecar 定案不纳入内嵌——让「带不带文件搜索」与宿主本体解耦；③ 便携 sidecar 扫描：宿主扫描 exe 同目录 `extensions.d/`，两处扫描按 id 去重（用户目录优先），内置仍最优先。

---

### M8 — 扩展生态验证（2026-09-10 立项）

按既定路线第三站：验证**「协议与语言无关」不是一句声明**——用**非 Rust** 扩展走通协议全链路。

| 批次 | 内容 | 验收标准 | 状态 |
|---|---|---|---|
| 8.1 扩展开发指南 | [`docs/extensions.md`](./extensions.md)：30 秒心智模型 / 10 分钟上手 / 三条铁律 / 三个 Windows 陷阱 / 握手 / `host/*` / 自检 / 调试手册 | 没读过源码的人能照着写出可被加载的扩展 | ✅ 2026-09-10 |
| 8.2 非 Rust 全表面示例 | [`examples/python-minimal/`](../examples/python-minimal/)：7 个 host→ext 方法 + `items_changed` + 3 种 `host/*` + `ShowToast`/`Dismiss`/`Confirm` | `dd-run-cli --conformance` 全绿 | ✅ 2026-09-10 |
| 8.3 `--conformance` 全表面自检 | 自检从 4 步扩到全表面（含 `fallback`/`get_command` 可复热/`invoke` 返回值 ∈ 8 种 / `host/*` / `close`），逐项 `✓/⚠/✗`；新增 `--invoke` / `--ext-id` | 对 8.2 示例 10 项全 ✓ | ✅ 2026-09-10 |
| 8.4 端到端实证 | `dd-run-cli --conformance --extensions-dir examples/python-minimal --invoke` | **10 项全 ✓（2386 ms）** | ✅ 2026-09-10 |

已做取舍记档（8.1）：指南只写规范里没有的东西，重复处一律引用 `protocol.md` / `manifest-schema.md` 并声明「冲突以规范为准」。（8.2）：只给 Python 单语言、做全表面、启动器用 `.cmd` 桥接（清单 `entry.args` 不支持 `${EXT_DIR}` 展开）。（8.3）：`invoke` 默认跳过真实副作用。

**真机反馈修复（2026-09-10）**：PyMin 示例装入 `dist/extensions.d/` 后标「暂时不可用」、点「重试」无效——根因是 `.cmd` 调用裸 `python` 依赖 PATH，而 GUI 继承系统 PATH 无 python。修复 = `install.py` 生成绝对路径清单（零 PATH 依赖），`.cmd` 降级备选；指南补「解释器不在 PATH 上」陷阱。**真机确认**：GUI 内示例命令全部正常执行。

---

### M9 — 内置扩展进程内化（2026-09-11 立项）

设计稿：[`docs/m9-inprocess-builtins.md`](./m9-inprocess-builtins.md)（v1.2）。动机：把 5 个内置扩展从「启动时多个子进程」改为**宿主进程内**调用（ueli / PowerToys CmdPal 主流做法），启动 0 进程、不再物化内嵌 exe、单文件体积下降；**第三方 / sidecar 仍子进程**（保留 M8 语言无关证明与未信任代码崩溃隔离）。协议 v1.0 **零改动**（仅"谁调用 `serve_line`"变化）。

| 批次 | 内容 | 验收标准 | 状态 |
|---|---|---|---|
| B1 规格上移 | `dd-ext::builtins`（5 个 `spec_*()` 上移 + `builtin_specs()`） | `cargo build/test -p dd-ext` 0 warning | ✅ 2026-09-11（`2f0fe19`） |
| B2 in-process 适配器 | `dd-gui/src/ext_inprocess.rs`：`InProcessExtension` 镜像 `ExtensionProcess`，`call` 直驱 `serve_line` + `catch_unwind` | clippy 0；逐字节 parity 单测 | ✅ 2026-09-11 |
| B3 注册路径切换 | `ExtClient`（`Subprocess`/`InProcess` 双后端）+ `builtin_registrations()` + 复热分流 + 去掉内置物化；附修 i18n 可重设（D6） | `cargo test --workspace` 0 失败；内置不再 spawn | ✅ 2026-09-11 |
| B4 构建/分发收敛 | `build.rs` `EMBED_EXES` 清空（`&[]`）+ `package.sh` 移除内嵌 5 内置步骤 + `embedded.rs`/`builtin.rs` 注释同步 | `EMBEDDED` 生成空切片；`dist/dd-run-0.1.1.exe` **8,601,088 B（8.2 MB，2026-09-12 真机修复后重编实测）**，较改动前工件 10,857,984 B（10.4 MB）↓21%；`dist/extensions.d/dd-ext-search.exe` 仍在 | ✅ 2026-09-11 |
| B5 CLI 改造 | `dd-run-cli` 补 in-process 分支 + 共享 `route_messages` | 单测断言内置分支返回 `SUCCESS` | ✅ 2026-09-11 |
| B5 dist | 重生成 dist | `dist/dd-run-0.1.1.exe`（8.2 MB）+ `dist/extensions.d/dd-ext-search.exe` 就位 | ✅ 2026-09-11 |
| B5 CLI 端到端 | `dd-run-cli --conformance --ext-id com.ddrun.{calc,shell,websearch,system,apps}` | **5 项全部 `EXIT=0`**（apps 133 命令） | ✅ 2026-09-11 |
| B5 真机 GUI | 启动 `dist/dd-run-0.1.1.exe`：Task Manager 仅 1 个 `dd-run` 进程；五项功能正常 | ✅ 真机复验通过（2026-09-11） | ✅ 2026-09-11 |

**M9 状态：已关闭**（2026-09-11）——B1–B5 全部 ✅（含真机复验）；交付 `dist/dd-run-0.1.1.exe`（8.2 MB，较内嵌态 ↓21%）+ `dist/extensions.d/`（文件搜索 sidecar）。

---

### v5.2 — UI 优化 B1–B8（2026-09-13 落地）

设计稿：[`cmdpal-ui-optimization-v5.html`](../cmdpal-ui-optimization-v5.html)。八批增量优化，每批独立可交付、可回退；B1–B8 已全部实现（commit 区间 `815e212..da0589c`）。配套真实机反馈修复（进页回填查询、返回聚焦、搜索引擎配置失效）于 `3545d3f` / `da0589c` 落地。

| 批次 | 内容 | commit |
|---|---|---|
| B1 语义字重 | semibold 字体族真实呈现 500/600 字重（标题独占中列） | `fcc3db7` |
| B2 小面积强调色 | 新增 `accent_stroke` token（暗 = brand[100] #479ef5），替代线状小元素取色 | `ce13d91` |
| B3 滚动条 Fluent 化 | floating 细条静止 4px 半透明、悬停展开 8px（ScrollStyle 全局调参） | `b82d13b` |
| B4 聚焦下划线过渡 | 搜索框聚焦下划线 0.12s 宽度过渡（`animate_value_with_time`） | `e930b5a` |
| B5 结果行两列式 | 移出行内副标题列，标题独占中列；信息转 hover tooltip | `16a3b07` |
| B6 设置页三处优化 | ①报错文案 `danger` token 化 ②引擎模板截断 + tooltip + 行 hover 底 ③主题三选双色块 → 迷你面板缩略图 | `b32fefb` |
| B7 悬停反馈体系 | `accent_hover` token + `draw_icon_button` 统一齿轮/返回/导航项/开关 hover 增强 | `815e212` |
| B8 面板宽高自适应 | 面板基准高度按光标所在显示器工作区自适应（记忆值优先） | `f3ae782` |

> 交付顺序建议 `B7 → B2 → B4 → B5 → B3 → B6 → B8 → B1`（B1 涉及字体加载链，放最后单独回归 IME / 多语言 / 冷启动）。

#### B5 修订 — 文件搜索结果「地址常显」（2026-09-14）

**问题**：B5 把行内副标题移入 hover tooltip 后，文件搜索结果的**文件地址完全不可见**（真机反馈）；此前旧布局虽有行内地址，但受「标题剩余 ≥48px 才画 + 最多 240px 截断」门控，长文件名时整段丢弃。

**方案**（演示页 [`docs/file-search-result-redesign.html`](./file-search-result-redesign.html)）：

| 决策 | 说明 |
|---|---|
| 行高不变 | 仍取 `ListMetrics::row_h`（标准 40 / 紧凑 36 / 宽松 44）。**否决双行 56px**——与 D8 契约冲突，且 `icons-typography-plan.md` §5.3 已明确不采纳；`app/mod.rs::base_height_for_workarea` 按单一 `row_h` 折算面板高度，逐行变高会同时失准面板高度与命中矩形 |
| 右列语义分派 | 文件结果 → **所在文件夹路径**（中段省略）；其余结果 → 来源类别标签（B5 原样） |
| 可替换类别标签的理由 | 文件结果的类别来自 `aggregator::category_label_for(com.ddrun.search)`，不在内置映射表 → **恒回退「命令」**，零信息量；分组标题「文件」已由 `section` 表达 |
| 右列只显父目录 | 文件名已在标题，重复无意义；中段省略后首段保盘符、尾段保最靠近文件的父目录 |
| tooltip 兜底 | 文件结果的行 tooltip **恒给完整路径**（含文件名） |
| 零改动 | 协议 v1.0 与 `PanelItem` 结构均不变（未新增 `details` 字段；v1 双行方案的 `details` 透传已撤销） |

**落点**：`crates/dd-gui/src/ui/row.rs` —— 常量 `FILE_PATH_W_FRACTION=0.45` / `FILE_PATH_MIN_W=120` / `CAT_LABEL_MAX_W=90` / `TRAIL_GAP=12`；函数 `is_file_item`（`tags` 含 `files` **且** `subtitle` 形如绝对路径，双条件防误判入口项 `files.search`）/ `looks_like_path` / `file_location` / `middle_ellipsis_chars`（纯函数）/ `middle_ellipsis`（宽度自适应）；`draw_item_row` 右列按 `file_item` 分派。

**验证**：`cargo build -p dd-gui` 0 error（row.rs 0 warning）；`cargo test -p dd-gui` **192 passed / 0 failed**（新增 4 条 row 单测）。

---

#### 页内补拉空窗误显「该页暂无内容」修复（2026-09-14，真机反馈）

**问题**：文件搜索页内输入查询后 ~1s 在飞空窗显示误导性空态「该页暂无内容」，结果到达后才消失。根因：进页首拉由 `open_page` 置 `is_loading=true`（骨架屏），但页内输入 200ms 去抖后的补拉（`schedule_page_query_debounce` → `refetch_page_if_current` → `dispatch_fetch_page`）**不设任何加载标志**，且首拉空结果落地的 `page.empty` 文案一直挂着——`panel.rs` 渲染时 `empty` 优先级高于列表，空窗期直接画了过期空态。

**方案**（stale-while-revalidate + 显式在飞指示）：

| 决策 | 说明 |
|---|---|
| `PageState::begin_refetch()`（新增，可单测） | `dispatch_fetch_page` 入口统一调用：清过期 `empty`；可见列表为空 → `is_loading=true` 走既有骨架屏；有旧结果 → **保留旧结果继续展示**，不闪骨架（边打边搜每次按键闪骨架是反模式） |
| 搜索框右端 spinner | `draw_searchbar` 新增 `busy` 参数：当前嵌套页 ext 有 `get_items` 在途（`inflight`）时，右端画 14px accent Spinner（egui 自带按需重绘）；TextEdit 预留 16px 宽避免文本与环重叠。有旧结果时这是唯一「正在搜索」信号 |
| Err 落地过期补偿 | 失败分支补上与 Ok 分支对称的 v3.3 过期补偿（`cur_query != req_search` → 重武装去抖），否则补拉在飞期间输入的字符会被失败态吞掉 |

**落点**：`crates/dd-gui/src/navigation.rs`（`begin_refetch` + 3 单测）、`crates/dd-gui/src/app/page.rs`（`dispatch_fetch_page` 入口 + Err 分支补偿）、`crates/dd-gui/src/ui/panel.rs`（`page_busy` 判定 + `draw_searchbar` spinner）。

**验证**：`cargo build -p dd-gui` 0 error / 0 新 warning；`cargo test -p dd-gui` **195 passed / 0 failed**（新增 3 条 navigation 单测）。

---

#### 页内补拉空窗误显「该页暂无内容」修复（二轮：过期结果不落地，2026-09-14 真机复报）

**复报**：一轮修复（上方 `begin_refetch` + spinner）后真机仍见空态闪烁。

**新根因（一轮未覆盖的「落地」环节）**：`poll_page` 落地时**先**算 `empty`、**后**才判定「请求查询 `req_search` vs 页内当前 query」是否一致。请求在飞期间用户继续输入 → 旧查询的 `get_items`（尤其 0 命中）照常落地，`page.empty = "该页暂无内容"` 被画上屏；待按最新 query 补拉的结果回来才替换。一轮只覆盖补拉的**发起**，未堵住**旧结果的落地**。
（另经 NDJSON 实测 `dd-ext-search`：冷启动 + `get_items` 共 0.47s、返回即结果、`is_loading=false` → 证「~1s」并非扩展/进程启动，而是 GUI 空窗。）

**方案**（抽纯函数 + 过期即弃）：

| 决策 | 说明 |
|---|---|
| `landing_is_stale(current_query, req_search)`（纯函数，可单测） | 判据 = 页内当前 query 非空且 ≠ 请求查询。空 query 属合法初始态（`GoToPage` 无查询进页）→ 不过期，保留既有语义 |
| 过期结果**不落地** | `poll_page` Ok 分支：过期 → 不替换列表 / 不置 `empty` / 不置 `is_loading`；改调 `begin_refetch()` 保持 in-flight 语义（列表空→骨架 / 非空→保留旧结果） |
| `rearm_page_query_debounce()`（新增） | **绕过 `is_loading` 早退**的强制重武装：过期路径已 `begin_refetch`（列表空时 `is_loading=true`），若复用带守卫的 `schedule_page_query_debounce` 会被早退吞掉 → 骨架卡死。仅置到期时刻 |
| 移除 Ok 分支尾部旧补偿 | 原 `cur_query != req_search → schedule_page_query_debounce()` 前移进 `landing_is_stale` 分支，消除重复 |

**落点**：`crates/dd-gui/src/app/page.rs`（`landing_is_stale` + 单测；Ok 分支重构为 `if stale {丢弃} else {正常落地}`）、`crates/dd-gui/src/app/refresh.rs`（`rearm_page_query_debounce`）。

**验证**：`cargo build -p dd-gui` 0 error；`cargo test -p dd-gui` 199 passed / 0 failed（新增 4 条 `landing_is_stale` 单测）；clippy 无新增 warning。

---

#### 页内补拉「闪骨架」修复（三轮：延迟骨架，2026-09-14 真机再反馈）

**再反馈**：输入文字后**删除一个字符**时「页面闪一下」（截图态 = 空态「该页暂无内容」）。

**根因（二轮引入的副作用）**：二轮的「丢弃过期结果」落地为 `begin_refetch()` —— 其中「列表为空 → 立即 `is_loading=true`」对**快速补拉**（Everything 温热 IPC ~50ms）只显示几帧骨架便被结果替换 → 视觉闪烁；且发起时清 `empty` 会让列表为空的瞬间渲染出**另一种**空态（`empty.no_match`）同样闪烁。

**方案**（延迟骨架 + 发起时不清空态）：

| 决策 | 说明 |
|---|---|
| `PAGE_LOADING_DELAY = 250ms` | 补拉发起时**不立即**切骨架，只记录到期时刻；快速补拉在窗口内落地 → 全程无骨架（慢补拉到期才显示） |
| `PageState::begin_refetch(now)` 改语义 | 列表为空 → `skeleton_after = now + 250ms`；非空 → `None`（保留旧结果）；**不再清 `empty`**（交由落地统一替换） |
| `PageState::skeleton_due(now)` / `clear_refetch()` | 渲染层 `is_loading \|\| skeleton_due(now)` 决定画骨架；落地（成功 / 失败）清标记 |
| 渲染层预约重绘 | 延迟未到期 → `request_repaint_after(deadline−now)`，无输入事件也能按时切骨架 |
| 未真正发起的早退分支清标记 | 忙碌 / 熔断 / 扩展缺失 / 进程不可用 → `clear_refetch()`；否则 250ms 后会误显示骨架 |

**落点**：`crates/dd-gui/src/navigation.rs`（常量 + `skeleton_after` 字段 + 3 方法 + 5 单测）、`crates/dd-gui/src/app/page.rs`（4 处调用 / 清理）、`crates/dd-gui/src/ui/panel.rs`（`skeleton_due` 判定 + 预约重绘）。

**验证**：`cargo build -p dd-gui` 0 error；`cargo test -p dd-gui` 201 passed / 0 failed（`begin_refetch` 单测重写为延迟语义，199→201）；clippy 无新增 warning。

---

### 图标与字体优化 — I1/I2/F1/F2（2026-09-12 落地）

方案：[`docs/icons-typography-plan.md`](./icons-typography-plan.md)（参考 DeskBox「图标/文字大小可调」）。I3 未做。

| 项目 | 状态 | 落点 |
|---|---|---|
| I1 无图标/url 项回落占位 glyph | ✅ | `ui/icons.rs`（用 `Palette::text4` 极弱色，与解码失败占位区分层级） |
| I2 glyph 用色显式化 | ✅ | `ui/icons.rs`（显式取 `Palette::text2`；属等价重构，零视觉变化，原 weak 辅助函数删除） |
| I3 Shell 回退链路 32px → 48px | ❌ 未做 | —（主链路覆盖绝大多数应用，视真机效果再定） |
| F1 列表排印 token 化 | ✅ | `theme.rs`（`LIST_TITLE_PT`/`LIST_CAT_PT`/`LIST_ICON_GAP` 等，初值 = 现值，纯重构） |
| F2 列表密度三档 | ✅ | `theme.rs`/`settings.rs`/`app/mod.rs`/`ui/*`（紧凑/标准/宽松，行高/字号/图标格联动；`Settings.density` 持久化） |

> 明确不采纳（同 DeskBox 取舍）：256px Shell 图标源、url favicon 网络下载、两行文件名、逐组件独立字号。

---

### 窗口材质与边框 — P1–P4（2026-09-13 落地）

方案：视觉契约见根目录 `cmdpal-window-material-effects.html` 效果页（S1–S6 + 规格表；参考 DeskBox「外观 → 窗口材质与边框」源码取证）。设置卡更名「窗口材质与边框」，原两互斥开关行改四行结构。

| 项目 | 状态 | 落点 |
|---|---|---|
| P1 材质单选化（三段 pill：无材质/云母/亚克力） | ✅ | `ui/settings_view.rs` draw_material_card 重构（复用 `draw_density_pill` 口径）+ `text.rs` |
| P2 不透明度滑杆（0–100，默认 40 = 推荐观感档；v2 直控式） | ✅ | `settings.rs` `material_opacity: u8` + `theme.rs` `panel_tint_with_opacity`（`alpha = cap × pct/100`；v5 cap 按材质两主题同档：云母 0.75 / 亚克力 1.0，0 = 纯材质）+ `app/keys.rs` `apply_material_opacity`（拖动不落盘、松手 `drag_stopped` 落盘）；v3/v4 浓淡基色 = `theme::tint_color`（面板色 × 系统强调色，暗 8%/亮 30%，DeskBox `BuildContentTintColor` 配方 + 亮度层补偿）；v5 行填充材质适配 = `theme::row_fills`（hover 分档：云母加权 7%/7.8% 玻璃、亚克力减重 3.5%/3.9%；selected 同走玻璃治扫动闪烁；回退实色）+ 页脚同步浓淡层（材质态 footer 与面板同源 `panel_fill`，消除底部色差带）；v6 设置卡控件风格对齐（自绘 Fluent 滑杆：轨 4px/accent_stroke 已选段/白钮描边、百分比右对齐行头；边框 pill 竖排修复；pill `dim` 置灰参数） |
| P3 窗口圆角三选（圆角/小圆角/方角） | ✅ | `settings.rs` `CornerPref` + `platform.rs` `apply_window_chrome(hwnd, pref)` 映射 `DWMWCP_ROUND/ROUNDSMALL/DONOTROUND` + `apply_corner_pref` |
| P4 面板边框三选（中性/强调色/关） | ✅ | `settings.rs` `BorderMode` + `platform.rs` `system_accent_color`（`DwmGetColorizationColor`，COLORREF 0x00BBGGRR）+ `app/keys.rs` `border_color` 单点收口（refresh_backdrop / apply_theme_pref / apply_border_mode 同源） |

兼容承诺：旧配置三键缺失 → 默认（100/圆角/中性），升级观感零变化；未知值回落（单测覆盖）。材质未生效（Win10/22621- 回退）时滑杆与边框行置灰、圆角行恒可用。明确非目标：MicaAlt（标签页窗口语义误用）、AcrylicBase（无 DWM 对应档）、边框粗细（DWM 描边固定 1px）、材质强度第二滑杆（与 P2 重叠）、Win10 旧版亚克力（accent-policy，Win10 已过支持期）。`cargo test --workspace` 全绿（dd-gui 186）。

---

### O2 — 协议方法名常量层（2026-09-14 落地）

方案与逐文件替换清单：[`docs/optimization-plan.md`](./optimization-plan.md) §2.3.1（Phase 1 首项，对应差异清单 P-18）。属**实现侧重构**：协议 v1.0 零改动（无字段/方法增删）。本表只记落点，细节不重复。

| 项目 | 状态 | 落点 |
|---|---|---|
| 常量层（12 方法 + 3 聚合视图） | ✅ | 新增 `crates/dd-protocol/src/methods.rs`（`METHOD_*` / `METHOD_HOST_*` / `NOTIFY_*` + `HOST_METHODS` / `HOST_METHOD_PREFIX` / `ALL_METHODS`），`lib.rs` 导出 `pub mod methods` |
| 宿主侧替换 | ✅ | `dd-host/src/process.rs`（6 调用 + `items_changed` 判别 + `classify` 前缀）、`manifest.rs`（`HOST_CAPABILITIES` 改常量别名）、`builtin.rs`（2 处 `capabilities`） |
| 扩展侧替换 | ✅ | `dd-ext/src/lib.rs`（7 分发臂 + 通知构造）、`builtins/{calc,websearch}.rs`、`bin/search.rs`、`dd-ext-sample/src/main.rs` |
| GUI / CLI 替换 | ✅ | `dd-gui/src/ext_inprocess.rs`（7 调用 + 通知判别 + `close` 帧）、`ext_client.rs`、`app/host_actions.rs`、`app/invoke.rs`、`dd-run-cli/src/main.rs` |
| 一致性测试（新增核对基线） | ✅ | `crates/dd-protocol/tests/consistency.rs::method_constants_match_protocol_method_table`：运行时抽 `protocol.md` §1.3 两表，与 `ALL_METHODS` 逐项同序断言 |

**验证**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **403 passed / 0 failed**（基线 402，+1）。

---

### O1 — 协议错误码接线（2026-09-15 落地）

方案与逐文件清单：[`docs/optimization-plan.md`](./optimization-plan.md) §2.2.1（Phase 1 第二项，对应差异清单 P-01）。属**实现侧补齐**：协议 v1.0 零改动（方法、字段、错误码取值与章节均未变）。本表只记落点，细节不重复。

| 项目 | 状态 | 落点 |
|---|---|---|
| 共享校验层（单一来源） | ✅ | 新增 `crates/dd-protocol/src/envelope.rs`：`Envelope{Valid/ParseError/InvalidRequest{id,reason}}` + `InvalidReason`（9 种）+ `validate`/`validate_value` + `error_response`；`lib.rs` 导出 `pub mod envelope` |
| 宿主侧接线 | ✅ | `dd-host/src/process.rs`：`call`/`poll_notifications`/`handle_line`/`route_messages` 改用共享校验；新增 `abort_oversized`（回 `-32600` + 关连接）与 `reply_envelope_error`（非致命回错 + 留痕） |
| 扩展侧接线 | ✅ | `dd-ext/src/lib.rs`：`run()` 帧循环处理 `TooLarge`（回错 → 退出）与 `InvalidUtf8`（丢弃）；`serve_line` 改用共享校验（批处理由误回的 `-32700` 纠正为 `-32600`）；`dd-ext-sample/src/main.rs` 同口径 |
| 错误构造去重 | ✅ | `dd-ext` 私有 `make_error` 删除、`dd-ext-sample` 的 `send_error` 改为委托 `error_response`，两侧 `-32700`/`-32600` 形状由同一份代码保证 |
| GUI / CLI | ✅ | **零改动**：分别复用 `serve_line` + `route_messages`（in-process）与 `ExtensionProcess`（子进程），自动继承 |
| 致命性区分（§9.3） | ✅ | 仅「消息超上限」支关连接；批处理 / 非法 `jsonrpc` / `id` 非法 / `params` 非对象 / `result`+`error` 并存均为非致命回错 |
| 端到端验证 | ✅ | `crates/dd-host/tests/roundtrip.rs::oversized_request_closes_connection_with_32600`：**真实子进程**超限 → 回 `-32600` → 关连接 → 进程退出 |

**行为变更（须知悉）**：超限帧从**静默丢弃**改为**回错 + 关连接**；`route_messages` 对非法信封由「完全忽略」改为「留痕进 `unmatched`」（既有断言随之由 1 条改为 2 条）。

**验证**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **422 passed / 0 failed**（基线 403，+19）；`dd-run-cli --conformance --ext-id com.ddrun.calc` 9 步全绿。

**明确未做（O1 范围内）**：`-32002` 启用、`PageInfo` 接线 —— 二者已于 **2026-09-17 定稿**（`-32002` = 扩展侧可用错误码、宿主不产出；`PageInfo` = 维持不传递），不再是开放项，详见 [§2 协议两项定稿](#协议两项定稿-32002--pageinfo2026-09-17-定稿)。

---

### O3 — 依赖治理（2026-09-15 落地）

方案与选型依据：[`docs/optimization-plan.md`](./optimization-plan.md) §2.5.1（Phase 1 第三项）。本表只记落点。

| 项目 | 状态 | 落点 |
|---|---|---|
| CI：漏洞扫描 | ✅ | `.github/workflows/ci.yml` 新增 `audit` job（`ubuntu-latest`；`taiki-e/install-action@v2` 取预编译二进制）：`cargo audit` 发现 vulnerability 即失败 |
| CI：死依赖 | ✅ | 同 job 的 `cargo machete`——**替换方案原建议的 `cargo udeps`**（后者需 nightly 工具链且冷编译整棵依赖树，CI 成本高） |
| 可升级 | ✅ | 新增 `.github/dependabot.yml`（cargo + github-actions，按月检查）——**替换 `cargo outdated`**（需编译元数据且无提醒机制） |
| 本地取证 | ✅ | `cargo machete` **零死依赖**；`cargo audit` 载入 1246 条 advisory、扫描 339 个依赖、**零漏洞** |

**替换说明**：两处替换（udeps → machete、outdated → Dependabot）的理由与实测依据见方案 §2.5.1。

**验证**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **422 passed / 0 failed**（本批无 Rust 代码改动，与上批持平）。**CI 实跑已确认**（2026-09-16）：`dependencies (audit + machete)` job 在 runner 上 **success**（main@`9936ead`），Dependabot 已开 4 个更新 PR。

**未覆盖**：unmaintained / unsound / yanked 类告警当前只警告不阻塞；收紧方式（`--deny warnings`）与权衡见方案 §2.5.1。

---

### O8 — 文档卫生（2026-09-15 落地）

方案与取证处置：[`docs/optimization-plan.md`](./optimization-plan.md) §2.7.1（Phase 1 第四项）。

| 项目 | 状态 | 落点 |
|---|---|---|
| CHANGELOG 缺日期 | ✅ | `CHANGELOG.md` 的 `[0.1.0]` 标题补日期 `2026-09-06`（取自 `git for-each-ref` 的 tag creatordate，实测值） |
| `examples/python-minimal/README.md` 行尾 | ✅ | **已合规**（全仓 `.md` 均 `i/lf w/crlf`）——2026-09-14 的行尾归一化已覆盖该文件，本批仅作确认 |
| I3 图标 32px 回退 | ⏸ 保留不做 | 设计文档 [`icons-typography-plan.md`](./icons-typography-plan.md) 已定「视真机效果再定，不阻塞交付」——属**真机依赖项**（Phase 2 范畴） |
| LRU 容量可配 | 🔵 转功能项 | `pool.rs` 为编译期常量；需 Settings 字段 + 设置页行 + 持久化 + 消费点改造，**明确不在 O8 范围**，待单独立项 |

**验证**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **422 passed / 0 failed**（本批无代码改动）。

**结论**：Phase 1 四项（O2/O1/O3/O8）全部落地。

---

### O4 — 可观测性/日志（2026-09-15 落地）

方案与逐文件清单：[`docs/optimization-plan.md`](./optimization-plan.md) §2.4.1（Phase 2 首项）。

| 项目 | 状态 | 落点 |
|---|---|---|
| 日志基础设施 | ✅ | 新增 `crates/dd-protocol/src/logging.rs`：自写 `StderrLogger`（恒 stderr，§2.5）+ `Filters` 级别解析（8 条单测）+ `init()`（幂等失败安全） |
| 依赖 | ✅ | `dd-protocol` 加 `log = { version = "0.4", features = ["std"] }`（**显式开 std**：`set_boxed_logger` 在 `alloc` 之下，默认 feature 在 workspace 合并后可能被关掉，实测踩到）；`dd-gui`/`dd-ext`/`dd-ext-sample`/`dd-run-cli` 各加 `log = "0.4"`。**不引 `env_logger`**（避免 anstream 等依赖树） |
| 入口接线 | ✅ | `dd-gui/src/main.rs`、`dd-run-cli/src/main.rs`、`dd-ext/src/lib.rs::run`、`dd-ext-sample/src/main.rs` 各加 `dd_protocol::logging::init()` |
| 分级替换 | ✅ | **131 处**（24 个文件）：`debug!` 85、`warn!` 39、`info!` 7；保留 `[模块标签] 消息` 文本，后端不加前缀，**输出逐字一致** |
| 开关 | ✅ | `DDRUN_LOG=off/error/warn/info/debug/trace`，支持 target 定向 `warn,dd_ext=debug`（最长前缀优先） |
| 默认值 | ✅ | **`debug`**——与改造前「所有 `eprintln!` 都输出」等价，引入分级不减少既有可观测性 |

**验证**：`DDRUN_LOG=debug` → stderr 7 行、`info`/`warn`/`off` → 0 行、不设环境变量 → 7 行（= 默认等价）；四档下 stdout 恒 14 行且 `dd-run-cli --conformance` 全绿（**协议纯度保持**）；`fmt --all -- --check` 无差异；`clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **430 passed / 0 failed**（基线 422，+8 日志单测）。

**保留/未做**：测试与 `build.rs` 的 `eprintln!` 保留（非运行时日志）；GUI release 无控制台，排障需重定向 `2> dd-run.log`（已注于 `main.rs`）；方案原文的「结构化日志」未做（验收只要求分级与开关）；**真机复现（扩展崩溃/超时链路）待用户执行**。

---

### O7 文件搜索 P2 真机验收（2026-09-15 首轮）

方案：[`docs/optimization-plan.md`](./optimization-plan.md) §1 O7（Phase 2 第二项，对应 `search-file.md` §7.4 的 A-33-05/06/07/08/10）。**逐项实测、原始数据与复现命令见 [`search-file-p2-acceptance-2026-09-15.md`](./search-file-p2-acceptance-2026-09-15.md)**；此处只记落点。

| 项目 | 状态 | 落点 |
|---|---|---|
| 验收脚本 | ✅ | 新增 `tools/search_acceptance.py`：按协议直驱 sidecar（`env` / `breakdown` / `bench` / `fault` 四模式），证据落 `target/acceptance/`（JSON + CSV） |
| A-33-07 稳定性 | ✅ 通过 | 1000 次查询无崩溃；RSS 稳态 +5.97%（<10%）且序列震荡非单调；线程恒 5、句柄恒 156 |
| A-33-10 恢复 | ✅ 通过 | 两轮注入均恢复（第 10 次 / 18.06 s，经「3 次失败 → 释放 client → 重建」；第 22 次 / 10.55 s）；通道切换日志按到达时刻留证 |
| A-33-06 引导项分支 | ✅ 通过 | 「两者均不可用 → 100% 引导项」20/20，p50 0.077 ms、max 1.587 ms（≤ 2000 ms） |
| A-33-05 性能 | ✅ 达标（阈值修订后） | **扩展自身 ≲ 1 ms**（探活 0.00 + 评分 0.3 + 映射/JSON ≲0.7）；瓶颈全在 **IPC 查询往返 11–25 ms**（随查询词变化）→ 1000 次 p50 **26.87 ms** / p95 **31.49 ms**，对 **2026-09-16 修订后**的门禁（< 30 / < 50 ms）达标；`run_es` 基线仍 🟨 阻塞 |
| A-33-08 矩阵 | ⚠️ 部分 | 本机单元（Win11 × Everything 1.4 × 非提升 × 默认实例）有结果；Win10 / 1.5 / 提升(UIPI) / 命名实例未覆盖 |
| 环境前置阻塞 | 🟨 | 本机无 `es.exe` → A-33-05 的 `run_es` 基线与 A-33-06 的回落分支**无法验证** |

**2026-09-16 补充（定因插桩 + 阈值修订）**：在 `dd-ext/src/bin/search.rs` 落地**分阶段计时日志**——`QueryTiming` 线程局部槽 + 每次 `get_items` 一行 `get_items 计时: kind=… total=… probe=… channel=… query=… score=… items=…`（debug 级），并加 2 项离线单测。实测**更正**了首轮报告 §4.1 的耗时分解：**评分仅 ≈ 0.3 ms**（原推断「≈16 ms」有误 —— 差异实为 IPC 查询耗时随查询词变化），扩展自身成本 ≲ 1 ms。据此**用户决策修订 A-33-05 门禁**为 p50 < 30 ms / p95 < 50 ms（p99/max 转观察项），依据与余量核算见报告 §4.1.1；`search-file.md` 升至 **v3.4** 记入该修订，A-33-05 由「❌ 未达标」改判「⚠️ 部分通过」（IPC 半达标、基线仍阻塞）。

**红线复述**：A-33-06 / A-33-08 未**全**通过前，用户文档不得承诺「无需 `es.exe`」。

**未闭环 3 项**（待环境 / 待真机 GUI）：A-33-08 剩余矩阵单元（Win10 / Everything 1.5 / 提升 / 命名实例，**当前唯一红线来源**）、GUI 端到端「输入到首屏 ≤200 ms」、A-33-03 的 `%`/`#` 打开验证 —— 见 [验收报告](./search-file-p2-acceptance-2026-09-15.md) §5。

**2026-09-17 第二轮（补 `es.exe`，报告 v1.3）**：安装 ES 1.1.0.37（`%LOCALAPPDATA%\Microsoft\WindowsApps\es.exe`，在用户 PATH 上）后补齐两项此前「无法验证」的半项，并把测量口径修硬：

| 项目 | 结果 |
|---|---|
| A-33-05 基线半 | 同参 `es.exe` ×200 次 p95 **324.213 ms** vs IPC p95 **28.300 ms** → **降幅 91.27 % ≥ 50 %** ✅（IPC 门禁复测 1000 次 p50 24.156／p95 28.300，通道 `ipc` 1000/1000） |
| A-33-06 回落半 | **3/3 回落 `es.exe` 且返回 30 条真实结果**（通道 `ipc->es`，total 225–282 ms）✅；形态①（两者均不可用）复测 20/20 引导项 |
| A-33-10 | 恢复判据改为**通道级**（`channel == "ipc"`）：第 8 次查询／15.16 s 回 IPC（旧「有响应」判据会在 8.59 s 误报恢复） |
| 产品缺陷（新发现，已修） | `es.exe` 以 `rc=8` 失败时错误只在 **stderr**，而旧 `run_es` 丢弃 stderr + 忽略退出码 → **静默当成「无结果」**（用户见空列表无提示）。修法：新增纯函数 `interpret_es_output(status, stdout, stderr)`（仅 `rc=0` 按无结果，`rc≠0` 转 `Err` 并保留 stderr 文案 → 落成可读 `files.error` 项），`run_es` 改**双管道并发消费**并带出退出码；+1 单测（5 个断言面） |
| 测量闸门（脚本） | ① `wait_ipc_ready`：计时前必须等到 `channel=ipc`（否则测得的是 **0.14 ms 的引导项**，得出「假达标」——本轮实测踩到）；② 通道直方图（门禁以 `all_queries_over_ipc` 为前提）；③ 通道级恢复判据；④ `run_es` 基线实采 |

**验证**（首轮）：本批**无 Rust 代码改动**（新增 1 个 Python 验收脚本 + 文档回写）。**2026-09-16 追加** `search.rs` 计时插桩（+2 单测）后三关复跑：`fmt --all -- --check` 无差异、`clippy --workspace --all-targets` 0 warning、`cargo test --workspace` **432 passed / 0 failed**（430 → +2）。

---

### 协议两项定稿（`-32002` / `PageInfo`，2026-09-17 定稿）

出处：`INDEX.md` §5 两项开放项 + `doc-audit-2026-09-13.md` §6.1 #1/#2。**均不改协议语义**（版本仍 v1.0），只把注记从「待接线」改写为**终态口径**并关闭开放项。

| 项目 | 状态 | 落点 |
|---|---|---|
| `-32002 command_not_found` | ✅ 定稿：**扩展侧可用错误码、宿主不产出** | `protocol.md` §9.2 表格行 + 注记改写（去掉「保留码、待接线」）。实测依据：Python 示例确实产出该码（`examples/python-minimal/dd_ext_pymin.py`），内置扩展与 `dd-ext-sample` 回 `ShowToast` 属**合法实现选择**（各带专属文案） |
| `PageInfo` | ✅ 定稿：**维持不传递**（v1.0 预留定义） | `protocol.md` §8.5 注记改写：`GetItemsResult` 不携带页元信息，宿主页标题取自**宿主侧信息**；启用须走 §13 `MINOR` |
| 页标题来源（`PageInfo` 决策的落地） | ✅ 已修 | 嵌套页标题此前**直接用原始 `page_id`**（`app/page.rs` 把 `page_id` 同时当标题）→ placeholder 显示「在「files.results」中筛选…」；现改为「被点击项标题 / 本地化扩展名（无入口项场景，如 `Ctrl+F` 直达）/ 空（扩展 `GoToPage`）→ 回落『筛选命令…』」，并修正 `navigation.rs` 中「标题来自 `PageInfo.title`」的**过期注释** |

**验证**：`protocol.md` 的 JSON 示例未动（`cargo test -p dd-protocol` 通过，workspace **433 passed**）。

### 文件搜索回落通道静默失败修复 + 工具一致性（2026-09-17）

出处：[验收报告](./search-file-p2-acceptance-2026-09-15.md) §4.2.1。**属产品缺陷修复**（用户可见行为变更）。

| 项目 | 状态 | 落点 |
|---|---|---|
| `es.exe` 失败被静默当成「无结果」 | ✅ 已修 | `dd-ext/src/bin/search.rs`：新增纯函数 `interpret_es_output(status, stdout, stderr)`（仅 `rc=0` 按无结果；`rc≠0` → `Err` 并保留 stderr 文案）；`run_es` 改 **stdout/stderr 双管道并发消费** + 带出退出码；`search_via_es` 不再自行解析。触发场景实测为 `rc=8`「Everything IPC not found」（错误只在 stderr） |
| 新增单测 | ✅ | `interpret_es_output_separates_no_result_from_failure`（rc=0 空输出仍为无结果 / rc=0 合法 JSON / **rc=8 必须 Err 且带文案与退出码** / 无退出码 / rc=0 但非法 JSON 仍报错） |
| `--conformance` 对 `has_fallback=false` 扩展**必红** | ✅ 已修 | `dd-run-cli/src/main.rs`：`has_fallback=false` 时**跳过** step 4（宿主按声明不会调用该方法，扩展不实现该分发臂合法）→ 示例扩展自检由「`-32601` 红灯」改为**9 步全过**；内置（`has_fallback=true`）路径无回归（step 4 仍校验一致性） |
| `ext_inprocess.rs` 测试辅助与生产路由不同步 | ✅ 已修 | 测试内的 `route_serve_line` 由**手工复刻** `classify` + `jsonrpc` 检查改为**委托生产实现** `dd_host::process::route_messages`（单一来源；旧版不过 `envelope::validate`、不收集 `unmatched`，喂非法信封会静默跳过） |
| `doc-code-diff` P-13 复核 | ✅ 已消除 | `protocol.md` §2.2 规则 6 已定义非 UTF-8 帧处置 → §4/§6 两行标 ✅（见 `doc-code-diff` §13） |

**行为变更（须知悉）**：`es.exe` 自身失败时，用户由「**空结果列表（无提示）**」改为看到**可读错误项** `files.error`（文案指向安装/运行 Everything 与 `DDRUN_ES_PATH`）。正常路径（IPC 成功 / es 成功）行为不变。

**验证**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` **0 warning**；`cargo test --workspace` **433 passed / 0 failed**（432 + 1 新单测）；`--conformance --ext-id com.ddrun.calc` 9 步全绿；`--conformance --extensions-dir <示例目录>` 9 步全过（此前第 4 步必红）。

---

### 文件搜索 Ctrl+F 直达 + Shell 真实图标（v3.6，2026-09-19 落地）

出处：专题方案 [`search-file-ctrl-f-icons-plan.md`](./search-file-ctrl-f-icons-plan.md)（决策 D1-x / D2-x）；**生效口径**见 [`search-file.md`](./search-file.md) §10。用户诉求：文件搜索改由「启动面板后 `Ctrl+F`」触发 + 查询结果显示文件图标。

| 项目 | 状态 | 落点 |
|---|---|---|
| 面板内 `Ctrl+F` 一键直达（任意页；栈深恒 2；仅 Root 查询带入；扩展不可用给 Error Toast） | ✅ | `dd-gui/src/app/keys.rs`（按键消费，与 `Ctrl+,` 同层）、`app/mod.rs`（`open_file_search_from_panel` + 模块级纯决策 `file_search_source_query`）、`text.rs`（`ph.root` 加提示 + 新增 `toast.filesearch_unavailable`） |
| 文件结果图标 → Windows Shell 真实图标（32×32，`Icon::Path`，失败回落类别 glyph） | ✅ | `dd-ext/src/shell_icon.rs`（**新增**共享管线 + `file_type_icon_png`）、`dd-ext/src/bin/search.rs`（`icon_cache_key_for` / `ICON_CACHE` / `icon_from_png` / `entry_icon` / `icon_ms` 计时） |
| 图标管线去重（apps ↔ 文件搜索共用一份实现） | ✅ | `dd-ext/src/lib.rs` 注册模块；`builtins/apps.rs` 删除已上移的 4 个函数（`hicon_to_png`/`bitmap_to_png`/`shfileinfo_png`/`read_mask_bits`）与 3 个缓存函数，改调 `crate::shell_icon`（`−337/+72` 行，行为不变） |
| 既有测试竞态修复（本批暴露） | ✅ | `search.rs` 测试模块新增 `PATH_INDEX_TEST_LOCK`：`path_index_evicts_beyond_capacity` 按 id 阈值清空 `PATH_INDEX`，会连带清掉并行用例的 pid（接入真实图标后**并行 5/5 必现**）；8 个读回索引的用例统一持锁，生产逻辑零改动 |
| 验收工具同步 | ✅ | `tools/search_acceptance.py`：`TIMING_RE` 增**可选** `icon=` 段、`parse_timing` 输出 `icon_ms`（旧日志仍可解析）；**新增 `tools/icon_acceptance.py`**（A-IC 六项自动判定 + JSON 证据 + 退出码，`--cold` 冷缓存口径） |

**实测（本机 windows-gnu）**：`fmt` 无差异 / `clippy --workspace --all-targets` **0 告警** / `cargo test --workspace` **448 passed**（436 + 12）；`cargo build --release` **exit 0**；真机直驱 release sidecar：缓存命中 `icon_ms` **0.13–0.44 ms**（30 条）、按扩展名首抽 **12.4–49.1 ms**、**按真实路径首抽 703.8 ms（30 新键）**；`file-icons/` 54 文件 / 36,841 B。

**两处未达标（如实记档，处置待用户决策，详见 `search-file.md` §10.4）**：① 按真实路径档首次抽取 **703.8 ms** vs 预算 ≤ 40 ms（建议 O1 限流）；② `dd-ext-search.exe` **769,536 → 876,544 B（+104.5 KB）** vs 预算 ≤ 64 KB（宿主 `dd-run.exe` 仅 +3,072 B = 8,792,576 B）。

---

### 文件搜索：移除 `f ` 前缀直达（v3.7，2026-09-19）

用户诉求：根视图输入 `f ` + 空格会**劫持字面查询**（想搜 `f report` 也被强制进文件搜索页）——v3.6 的 `Ctrl+F` 已提供更明确的直达入口，故整条移除。

| 项目 | 状态 | 落点 |
|---|---|---|
| 前缀触发与全部配套代码删除 | ✅ | `dd-gui/src/app/mod.rs`：`FILE_SEARCH_PREFIX`、`file_search_drill_target()`、`PaletteApp::maybe_drill_file_search()`、`ui()` 每帧调用、字段 `file_drill` / `file_drill_armed`（含 `new()` 初始化）；`app/page.rs`：`poll_page` 落地回填分支 + 「离开来源页清标记」 |
| `Ctrl+F` 查询带入不再剥离前缀 | ✅ | `app/mod.rs::file_search_source_query`（仅 Root + trim 非空）；`open_file_search_from_panel` 去掉借 `file_drill` 的去重接线（「已在文件搜索页」幂等改由 `page_id` 判定，行为不变） |
| 测试同步 | ✅ | 删 `file_drill_prefix_still_works`；`ctrl_f_query_source_rules` 改锚定「`f ` 开头**原样**带入」；Ctrl+F 其余 5 个用例（A-CF-01…05）不变 |
| 注释与文档同步 | ✅ | 注释：`refresh.rs`（去抖说明）、`page.rs`（标题来源 / 回填语义）、`navigation.rs`（标题来源）；文档：`search.md`、`search-file.md`（§1 / §5.5.2 / §5.6.1 / §6.1 / §6.4 / §8 v3.7 / §10.1）、`search-file-ctrl-f-icons-plan.md`（v1.2 修订）、`protocol.md` §8.5、`README.md` / `README.zh-CN.md`、`CHANGELOG.md`、`INDEX.md` |

**实测（本机 windows-gnu）**：`cargo check --workspace --all-targets` exit 0 / `fmt --all -- --check` 无差异 / `clippy --workspace --all-targets` **0 告警** / `cargo test --workspace` **447 passed / 0 failed**（448 − 1：删前缀专属用例）；**release 构建 exit 0**（2026-09-19 复跑：`dd-run.exe` **8,790,528 B**，较 v3.6 批次后的 8,792,576 B **−2,048 B**；`dd-ext-search.exe` 876,544 B 不变；`--conformance --ext-id com.ddrun.calc` **9 步通过**）。

**未改动（零行为变化）**：顶层入口项、兜底模板、`open_page` 统一回填、`poll_page` 的 v3.3 query 保留与过期补偿、`Ctrl+F` 的幂等 / 栈深恒 2 / Toast 语义、扩展侧与协议（完全零改动）。

---

### 文件搜索：用户文档解除 `es.exe` 前置表述（v3.8，2026-09-19，零代码改动）

用户诉求：真机验收两轮完成后，把用户文档里「仍需安装 `es.exe`」的前置表述解除。

| 项目 | 状态 | 落点 |
|---|---|---|
| 用户指南口径改写 | ✅ | `docs/search.md`（v1.3）：§1「前置条件」→「准备事项」，步骤 1 标**必需**（Everything）、步骤 2 标**可选（推荐）**（`es.exe` = 仅直连不可用时的回落通道），步骤 4 验证改可选；配置项注、故障排查两行、已知边界引导项、§7 升级说明、§8 FAQ 同步 |
| 权威口径与登记 | ✅ | `search-file.md`（v3.8：§1 承诺口径定稿 + §8 新增 v3.8 行）、`INDEX.md`（v1.15：§5 行改「已解除」+ `search.md`/`search-file.md` 行数与版本重算）、`README.md` / `README.zh-CN.md` 文档表 |
| 历史文档注记（保留原结论） | ✅ | 验收报告 §1、`doc-audit-2026-09-13.md`（§5 第 4 项与其后续动作）、`doc-code-diff-2026-09-13.md` §7 红线段 —— 均**就地追加「2026-09-19 已解除」**，不重写历史判定 |

**红线边界（未越界承诺）**：A-33-08 仍为唯一红线（Win10 / Everything 1.5 / 提升权限 / 命名实例未覆盖）；文档以「直连不可用时回落」承接这些环境，**未承诺「任何环境都无需 `es.exe`」**。

**验证**：零代码改动 → 三关无需复跑（`cargo test` 基线仍 **447 passed**）；`tools/docscan.py` 失效链接 (none)；10 份改动文档 bare LF = 0；`git diff --numstat` 增删对称（**无整文件重写**）。

**dist 重打包（同批）**：原 `dist/extensions.d/dd-ext-search.exe` = **769,536 B** 是「Shell 真实图标」批次**之前**的旧构建（`dist/` mtime 停在 14:29），与当前源码不符 → 重跑 `cargo build --release -p dd-run-cli -p dd-ext-sample` + `bash tools/package.sh`（**rc=0**），并本地重建绿色包 zip（`package.sh` 只产 exe 与 `extensions.d/`，**不产 zip**）。实测：宿主 **8,790,528 B** / sidecar **876,544 B** / zip **4,428,378 B**（sha256 `9f74910a…`）。

**分发级冒烟（全绿）**：`dist/dev/dd-run-cli.exe --conformance --ext-id com.ddrun.calc` **9 步通过（exit 0）**；`dist/dd-run-0.1.1.exe` Popen 后 5 s 事件循环存活、terminate 干净；sidecar 直驱 `tools/search_acceptance.py breakdown` 通道 **`ipc`**（ready 0.11 s），三档 kind 齐全 —— `hint` p50 0.169 ms / `empty` 12.46 ms / `results`（窄命中）27.16 ms、宽命中 14.71 ms，与 09-17 验收基线一致；`--cold` 图标三档沿用 §10.3 记录。

### 文件搜索：图标按真实路径档首抽 191–704 ms 修复（E1，2026-09-19）

用户决策：采纳「**P1 分层异步 + C1 自动刷新**」（不动 40 ms 预算、不改「每程序自身图标」的键分级决策）。

| 项目 | 状态 | 落点 |
|---|---|---|
| 按键分层（同步 / 后台） | ✅ | `dd-ext/src/bin/search.rs`：`key_needs_background()`（纯函数）+ `IconBudget`（`ICON_SYNC_BUDGET = 15 ms`）+ `lookup_icon_png(entry, budget)` |
| 后台 worker | ✅ | `search.rs`：`enqueue_icon_job()`（按键去重 `ICON_INFLIGHT`、惰性起 4 线程，`IconJob` / `IconJobSender` 别名）+ `icon_worker_loop()`（取任务时才持锁） |
| 自动刷新（C1） | ✅ | `search.rs::notify_icons_ready()`（`ICON_NOTIFY_THROTTLE = 200 ms` 节流）→ §7.1 `items_changed`；宿主 `refresh.rs` 既有链路零改动 |
| 跨线程通知器（框架） | ✅ | `dd-ext/src/lib.rs`：`install_notifier()` / `clear_notifier()` / `notify_items_changed()` + `SharedOut` 共享 stdout（`write_message()` 取代 `send()`；`make_host_request` / `make_items_changed` 改收 `log_tag`） |
| 验收工具适配 | ✅ | `tools/icon_acceptance.py`：A-IC-02c 改「同步口径」、新增 **A-IC-08**（后台补齐）/ **A-IC-10**（稳定性）、A-IC-04 改「等待补齐后重查」 |
| 文档回写 | ✅ | `search-file.md` v3.9（§10.4 标已处置 + 新增 §10.5）、`search-file-ctrl-f-icons-plan.md` v1.3（§6.2 新增 A-IC-08/09/10 + 口径变更注）、`CHANGELOG`、`INDEX` |

**实测（`tools/icon_acceptance.py --cold`，release sidecar）**：A-IC-02c 同步 `icon_ms` **0.32 ms**（原 191–704 ms）、A-IC-08 补齐后 `path=30 / glyph=0`（0.09 ms）、A-IC-04 `.lnk` 补齐后 `path=17 / glyph=13`、A-IC-02b 32.23 ms、A-IC-02a 0.10 ms、A-IC-10 十次查询全 `results`；**A-IC-06 仍 FAIL**（912,384 B，增量 142,848 B，E2 未实施）。

**E2 探针**：临时剥离 `shell_icon` 的 `image` 调用后构建 → sidecar **876,544 → 779,776 B（−96,768 B）**；实施 E2 选项 ② 后预期 ≈ 818 KB、增量 ≈ 48 KB ✅；→ **已于同日实施（实测 831,488 B / 增量 61,952 B，与探针预期之差 ≈ 15.9 KB 为新编码器自身代码），见下节**。

**行为变更**：首次查询的 exe/lnk 行先显示类别图标，约 0.2–0.8 s 后自动替换为真实图标。

**验证**：`cargo test --workspace` **452 passed / 0 failed**（447 + 5 新增）/ `fmt` 无差异 / `clippy --workspace --all-targets` **0 告警** / `release` 构建 exit 0。

### 文件搜索：sidecar 体积回到预算内——零依赖 PNG 编码器（E2，2026-09-19）

用户决策：采纳 `search-file.md` §10.4 选项 ②（自写零依赖 PNG 编码器；未采纳「修订预算」）。

| 项目 | 状态 | 落点 |
|---|---|---|
| 零依赖 PNG 编码器 | ✅ | 新增 `crates/dd-ext/src/png.rs`：`encode_rgba`（IHDR/IDAT/IEND + 块 CRC-32 + zlib 流[固定 Huffman deflate + 贪心 LZ77] + Adler-32；行滤波 None/Sub/Up 取最小） |
| 编码点切换 | ✅ | `dd-ext/src/shell_icon.rs`：`hicon_to_png` / `bitmap_to_png` 尾部改调 `png::encode_rgba`（apps 经共享模块继承） |
| 依赖治理 | ✅ | `dd-ext/Cargo.toml`：`image[png]` 由 `[dependencies]` 移至 `[dev-dependencies]`（仅单测解码 oracle，不进交付产物） |
| 验收 | ✅ | `A-IC-06` 转 **PASS**（831,488 B，增量 61,952 B ≤ 65,536 B，余量压线）；八项判定 7 PASS + A-IC-02b ⚠️（负载敏感压线，判因与复测建议见 [`search-file.md`](./search-file.md) §10.6） |

**实测**：sidecar 912,384 → **831,488 B**（−80,896 B）；宿主 `dd-run.exe` −12,800 B；`cargo test --workspace` **462 passed**（452 + 10）；fmt 无差异 / clippy 0 告警 / release exit 0。独立第三方校验（Python zlib + CRC）：file-icons 51/51、apps-icons 88/88；apps 真机回归 88 项 path 图标齐全。**详述唯一出处：[`search-file.md`](./search-file.md) §10.6**。

**dist 重打包（已闭环，同日稍后）**：`bash tools/package.sh` exit 0，`dist/extensions.d/dd-ext-search.exe` **831,488 B** 与源码构建同源；分发级冒烟全过：conformance 内置 9 步 exit 0 / 示例扩展 9 步（step 4 = 「has_fallback=false → 宿主不调用该方法，跳过（§6.2）」）/ GUI 5s 事件循环存活 / sidecar breakdown 通道 `ipc` 三档齐全（hint p50 0.137 / empty 13.30 / results 27.8 ms）/ zip 重建后成员与未压缩体积一致。（详述见 [`search-file.md`](./search-file.md) §10.6）。

### GUI 端到端首屏计时插桩（A-33-05 感知指标，2026-09-19）

验收报告 §5 #4「输入到首屏 ≤ 200 ms」补齐宿主侧插桩（此前仅扩展侧往返）；**真机采样待做**。

| 项目 | 状态 | 落点 |
|---|---|---|
| E2E 样本与结算 | ✅ | 新增 `dd-gui/src/app/e2e.rs`：`E2eSample`（等待/往返/渲染三段分解纯函数）+ `e2e_report`（`draw_panel` 完成后结算，可见/隐藏帧两处）+ 3 单测 |
| 埋点接线 | ✅ | `app/mod.rs`（3 计时字段）、`app/page.rs`（分派点 / 非过期落地建样本 / 失败与离页清理 / 带词进页起点）、`ui/panel.rs`（页内 query 变化 = 感知起点） |
| 判读工具 | ✅ | `tools/gui_e2e_parse.py`：解析 `E2E 首屏:` 日志 → min/p50/p95/max + 200ms 门禁判定（p95 < 200 → exit 0） |

**口径**：`input→paint` 上界不含 egui 呈现 / 垂直同步；常驻 `log::debug!`（无 debug_assertions 守卫）。**详述唯一出处：[`search-file.md`](./search-file.md) §10.7**。

**验证**：`cargo test --workspace` **465 passed**（462 + 3）/ `fmt` 无差异 / `clippy` 0 告警 / release 构建 exit 0。

### 设置 / 个性化 / 材料样式（B1–B4，2026-09-20，参照 PowerToys CmdPal）

方案（设计稿先行）见 [`settings-personalization-plan.md`](./settings-personalization-plan.md) v1.1；四批一次落地，新设置全部**默认值 = 现有行为**（旧 `config.json` 零迁移）。

| 批次 | 任务 | 落点 |
|---|---|---|
| B1 | T1 材料注册表化 | `settings.rs`（`Backdrop::ALL` + `name_key/desc_key`）、`theme.rs`（`BackdropStyleConfig`/`backdrop_config`：tint_cap/支持位/行填充分档）、`platform.rs`（`From<Backdrop> for SystemBackdrop` 唯一映射）、`keys.rs`/`settings_view.rs` 改由注册表派生 |
| B1 | T2 Mica Alt | `platform.rs`（`DWMSBT_TABBEDWINDOW`）、`settings.rs`（第 4 档 + `mica_alt` 标识）、材质 pill 四选（宽度按档数均分） |
| B1 | M4 滑杆口径 | `text.rs`：不透明度描述澄清为「材质色层的浓淡强度（非窗口透明度）」 |
| B2 | T3 Esc 行为 | `settings.rs`（`EscBehavior`/`EscAction` + `decide` 纯函数）、`keys.rs`（按键分支按决策执行、`clear_current_query`）、设置页「按键与交互」卡 |
| B2 | T4 退格键返回 | `settings.rs`（`backspace_go_back`，默认关）、`keys.rs`（嵌套页 + 空框 → 返回） |
| B2 | T5 恢复默认外观 | `keys.rs::apply_reset_appearance`（复用既有 `apply_*`，默认值唯一来源 `Settings::default()`）、`app/mod.rs`（两步确认武装字段）、外观栏底部卡片（5s 超时自动撤销） |
| B3 | T6 着色模式 | `settings.rs`（`ColorizationMode` + `custom_tint_color`/`custom_tint_intensity`）、`theme.rs`（`Colorization` 投影 + `tint_mix`/`mix_panel` 纯函数 + `tint_color(dark, cz)`）、`keys.rs`（三个 apply + `repaint_panel_tint` 统一出口）、材质卡「着色」行（自定义档显隐颜色/强度） |
| B4 | T7 单击激活 | `settings.rs`（`single_click_activation`，默认 **true** = 既有行为）、`panel.rs`（关档：单击选中 / 双击执行）、按键交互卡开关行 |
| B4 | T8 界面动效 | `settings.rs`（`ui_animations`，默认 true）、`panel.rs::draw_searchbar`（关档直出终态，不再驱动过渡重绘）、同一卡开关行 |

**验证**：`cargo test --workspace` **470 passed**（465 基线 + 5 新增：注册表覆盖 / 云母 Alt 往返 / Esc 决策矩阵 / 着色三档与混合比 / 单击与动效默认往返）/ `fmt` 无差异 / `clippy` 0 告警 / `release` exit 0；`tools/docscan.py` 失效链接 (none)。

**未做**（方案 §4 / §5 T9–T10）：背景图通道（P2 项，一期互斥语义未实施）、ShowAppDetails 详情窗、全屏忽略热键、强调色实时监听、紧凑模式/Dock —— 均需另行立项。

## 3. 验收映射总表

| 验收项 | 内容 | 里程碑 |
|---|---|---|
| A1 | 全局热键可唤起/隐藏 | M1 |
| A2 | 冷启动首屏 < 200ms（**目标值，需实测**） | M3 实测 |
| A3 | 输入过滤 < 16ms/帧（**目标值，需实测**） | M4 实测 ✅（2000 项 3.7ms，debug；真机记录值，见 m4-record §3.7） |
| A4 | 单测覆盖 8 种 `CommandResultKind` | M2 |
| A5 | 页面栈 `GoBack` / `GoHome` | M2 |
| A6 | frozen 冷启动不拉起，点击桩项复热成功 | M3 |
| A7 | LRU 保活 N 个，超出释放并标 stub | M3 |
| A8 | 扩展崩溃后宿主不退出、可恢复 | M4 |
| A9 | 列表更新走"事件 + 全量拉取" | M2 |
| A10 | 内置扩展覆盖 Apps/Calc/System/WebSearch/Shell | M4 |
| A11 | 核心路径 100% 键盘可达（口径：真机人工走查，无自动化覆盖率度量） | M1 |
| A12 | 协议双向方法齐全，能力调用不阻塞 UI | M0 + M1 |

> A2 / A3 为**设计预期、非已证事实**（设计文档 §8.3、§11）。实测不达标时，做法是**记录实测值与瓶颈并据此决策**，而不是下调目标值以通过验收。

### 3.1 测试基线台账（2026-09-17 建立）

> 由来：`doc-audit-2026-09-13.md` §6.1 #6 —— 各文档的测试数快照跨度大（40 → 433），回归时难以判断「是否变少」。**口径统一为 `cargo test --workspace` 的 passed 总数**（本机 windows-gnu），**每次落地一批后追加一行**；任何下降即视为回归缺陷。

| 时点 | passed | 变化来源 |
|---|---|---|
| 2026-09-14 | 402 | 基线（含当日清理批：fmt + 4 处既有告警） |
| 2026-09-14 | 403 | +1：O2 方法名常量 ↔ `protocol.md` §1.3 一致性测试 |
| 2026-09-15 | 422 | +19：O1 封套校验（`dd-protocol` 11 / `dd-ext` 6 / `dd-host` 1 / roundtrip 端到端 1） |
| 2026-09-15 | 430 | +8：O4 日志（`Filters` 解析等） |
| 2026-09-16 | 432 | +2：O7 计时插桩（`QueryTiming` 槽） |
| 2026-09-17 | **433** | +1：文件搜索回落失败判定（`interpret_es_output`） |
| 2026-09-19 | 435 | +2：模糊排序字段分层（`fuzzy` + `state` 各 1；CHANGELOG 同名条目） |
| 2026-09-19 | 436 | +1：应用列表调试诊断工具过滤（`apps::is_junk_title`；CHANGELOG 同名条目） |
| 2026-09-19 | **448** | +12：本批（Ctrl+F 直达 5 + Shell 真实图标 7，后者含 `shell_icon` 4 项） |
| 2026-09-19 | 447 | −1：移除 `f ` 前缀直达（删其专属用例 `file_drill_prefix_still_works`；`ctrl_f_query_source_rules` 改锚定「前缀不剥离」） |
| 2026-09-19 | **452** | +5：**E1 图标分层异步**（`path_keys_are_deferred_to_background` / `icon_budget_expires` / `icon_job_dedup_keeps_single_inflight` / `notify_without_hook_is_noop` / `installed_notifier_receives_page_id_from_other_thread`） |
| 2026-09-19 | **462** | +10：**E2 零依赖 PNG 编码器**（`png.rs`：往返 6 项[平坦/渐变/圆图标/噪声/1×1/1×300] + 结构走查 + 入参拒绝 + 长度/距离码表边界 + Adler/CRC 已知向量；`image` 转 dev-dependency 作解码 oracle） |
| 2026-09-19 | **465** | +3：**E2E 首屏计时**（`app/e2e.rs`：三段分解 + 饱和不 panic + 系统发起等待为 0） |
| 2026-09-20 | **470** | +5：**设置/个性化/材料 B1–B4**（`theme::backdrop_registry_covers_all_variants` / `settings::backdrop_default_is_mica_and_roundtrips`（含云母 Alt）/ `settings::esc_behavior_and_backspace_defaults_roundtrip_and_decide`（决策矩阵）/ `settings::colorization_defaults_roundtrip_and_sanitize` / `theme::colorization_mix_and_tint_rules` / `settings::click_and_animation_defaults_roundtrip`） |

**两条使用注意**：① `crates/dd-host/tests/roundtrip*.rs` 在 `dd-ext-sample.exe` **未构建时会打印 SKIP 并 return**（计入 passed），故凡涉及协议/扩展行为，先 `cargo build -p dd-ext-sample` 再跑；② 「三关全绿」与 CI 四关**均为 debug profile**，`#[cfg(debug_assertions)]` 类 release-only 编译错误检不到 —— 交付/发布前必须实跑 `cargo build --release`（见 §7 构建环境记档与 `CHANGELOG` 的 release 阻塞条目）。

---

## 4. 架构决策记录（ADR）

### ADR-1：扩展隔离用「子进程 + NDJSON JSON-RPC」，内置扩展例外走 in-process（二分表述）

- **状态**：已接受（M9 起修订为二分表述）
- **背景**：CmdPal 在 Windows 上用进程外 COM 隔离扩展（设计文档 §6.2）。跨平台无 COM，需选择等价物。
- **决策（二分）**：
  1. **第三方扩展 / sidecar：独立子进程 + stdin/stdout NDJSON JSON-RPC**。进程隔离成立——扩展崩溃不影响宿主，且扩展可用任意语言实现（M8 已实证 Python 全表面走通协议全链路）。
  2. **内置扩展（apps / calc / system / websearch / shell）：宿主进程内（in-process）**。M9 起由宿主直接驱动 `dd_ext::serve_line`，**不再 spawn 子进程、不再内嵌扩展 exe**——`dd-gui/build.rs` 的 `EMBED_EXES` 已清空为 `&[]`，`embedded::materialize()` 恒返回 `None`。`embedded` 模块**保留为将来内嵌 sidecar 的扩展点**。协议 v1.0 零改动（in-process 与子进程走同一套 NDJSON 路由，字节等价）。
- **理由**（对子进程路径成立）：① 零沙箱运行时；② 跨平台直出；③ 调试简单（`tail -f` 看协议流）；④ 扩展可用任意语言编写。
- **代价与缓解**：进程启动有开销 → 用 frozen/stub 磁盘缓存 + LRU 保活缓解（M3）；内置走 in-process 消除启动开销与单文件分发负担。
- **备选方案**：WASM 沙箱（`wasmtime` / `extism`）——隔离更强、启动更快，但宿主需实现沙箱运行时，且扩展必须能编译到 WASM。

### ADR-2：GUI 框架用 egui

- **状态**：已接受（**M1 需验证键盘焦点，不通过则重新评估**）
- **背景**：需一个纯 Rust、跨平台、无重依赖的 GUI 方案，且不引入 Visual Studio / Windows SDK 依赖（设计文档 §8.3）。
- **决策**：egui（glow / wgpu 后端）。
- **理由**：① 纯 Rust，自带渲染后端绑定；② 即时模式天然适合"列表内容频繁随搜索变化"的面板；③ 生态成熟。
- **代价与风险**：即时模式下键盘焦点需自建（A11 风险，已在 M1 验证通过）；无障碍支持弱于保留模式框架。
- **备选方案**：Slint、iced。

### ADR-3：扩展发现用「清单文件扫描」

- **状态**：已接受
- **背景**：CmdPal 用 `AppExtensionCatalog` / 注册表发现扩展。跨平台需等价物。
- **决策**：扫描扩展目录下的 `*.json` 清单文件（见 [`manifest-schema.md`](./manifest-schema.md)）。
- **理由**：最简单、零运行时、易调试；扩展的安装/卸载就是文件增删。
- **代价**：扩展需自行安装到目录，无自动发现能力。
- **备选方案**：进程注册（socket/命名管道）、WASM 内嵌（MVP 不做）。

### ADR-4：协议成帧用 NDJSON

- **状态**：已接受
- **背景**：JSON-RPC 2.0 不定义成帧，需自选。
- **决策**：一行一条紧凑 JSON，以 `\n` 结尾（见 [`protocol.md`](./protocol.md) §2.2）。
- **理由**：payload 全是几十到几百字节文本，无二进制需求；按行读写 I/O 循环最简；调试肉眼可读。
- **代价**：消息内不得有裸换行（JSON 转义天然保证）；不支持 pretty-print 多行 JSON。
- **备选方案**：LSP 式 `Content-Length` 头、长度前缀二进制。

---

## 5. 当前进度（按里程碑归并）

> 以下仅保留每里程碑**最终结论 + 关键决策 + 遗留项**；过程细节、真机修复与踩坑记档一律见对应 `mX-record.md` / 设计稿 / CHANGELOG，不在本文件重复叙述。

- **M0–M9**：全部 ✅ 已关闭（2026-09-01 ～ 2026-09-11）。结论见 §2 各节与里程碑状态总表。
- **v5.2 UI 优化 B1–B8**：✅ 已关闭（2026-09-13）。八批增量优化全落地，含进页回填/返回聚焦/搜索引擎配置失效三项真机修复。详情见 [`cmdpal-ui-optimization-v5.html`](../cmdpal-ui-optimization-v5.html)。
- **图标与字体优化 I1/I2/F1/F2**：✅ 已关闭（2026-09-12）。I3 未做。详情见 [`icons-typography-plan.md`](./icons-typography-plan.md)。
- **遗留项**：见 §6.1 台账——L1/L2/L3/L4/L5/L6/L7/L8/L9/L10 全部 ✅ 销项，无开放项。
- **版本与发版**：crates 版本 `dd-gui`/`dd-host`/`dd-ext`/`dd-ext-sample`/`dd-run-cli` = **0.1.1**，`dd-protocol` = **1.0.0**；已发布 tag `v0.1.0`、`v0.1.1`，分支 `main`。

---

## 6. 风险与未决

| # | 风险 / 未决项 | 处置 |
|---|---|---|
| R1 | **egui 键盘焦点**可能无法满足 A11 的 100% 键盘可达 | ✅ 已关闭（2026-09-02）：R1 尖峰 `ctx.input_mut(consume_key)` 拦截方案经真机人工验收通过，R1 通过、ADR-2 成立 |
| R2 | **冷启动 A2 < 200ms** 数据就绪实测 ~2ms 达标；total 高因 GUI/wgpu+msyh 字体加载 | ✅ 已关闭（2026-09-02）：A2 数据就绪 ~2ms 达标（真机记录值，代码内无基准工具可复跑）；total 高因 GUI/wgpu+msyh 字体（R2 记录瓶颈、不调目标） |
| R3 | **A3 < 16ms/帧** 在大结果集下可能不达标 | ✅ 已关闭（2026-09-03，M4 P5）：nucleo 打分排序，2000 项 ×6 字段实测 3.7ms < 16ms/帧 |
| R4 | 上游 PowerToys 文档引用边界复核 | ✅ 已采用 **MIT**；引用边界已复核销项（批次 8.3，2026-09-10，见 L6） |
| R5 | 设计文档 §7 中 **9 个 `🪟` Windows 专属扩展**不可移植 | 已在 §7 加平台列标记；MVP 不纳入 |
| R6 | CmdPal 仍处于 **preview**，上游接口可能演进 | 设计文档已标注核验日期；协议 v1.0 冻结后以本协议为准 |
| R7 | 设计稿字体依赖 Google Fonts（国内可能不可达） | 已改为本地优先分层字体栈，CDN 仅作渐进增强 |

### 6.1 遗留项台账（2026-09-13 汇总）

> 各里程碑收尾后散落各 record 的未排期项，统一收敛于此。当前**全部销项，无开放项**。

| # | 项 | 来源 | 说明 / 归属 |
|---|---|---|---|
| L1 | 启动一帧闪屏 | m1-record §4.6 | ✅ 已销项（2026-09-06 用户真机确认：`with_visible(false)` + `Visible(false)` 双保险下无闪屏） |
| L2 | 熔断后手动重试入口 | m4-record §5 | ✅ 已销项（M6 批次 6.4，`a007656`）：设置页「扩展管理」卡对 Failed 扩展显示「重试」→ `reset_crash` + `restart_aggregation`；真机验收通过（M6 集中回归 C 组） |
| L3 | 顶层 `items_changed` 不重聚合 | m3/m4-record §5 | ✅ 已销项（M6 批次 6.4，`a007656`）：`RefreshState.top` 标记 + 顶层分支调 `restart_aggregation`；真机验收通过（M6 集中回归 C 组） |
| L4 | 拼音匹配 | m4-record D12-B | ✅ 已销项（M6 批次 6.1，`a007656`）：`pinyin = "0.10"` + `state::pinyin_haystack` 生成「全拼+首字母」索引串 |
| L5 | LRU 驱逐后 fallback 复热失败 | m4-record §3.6/§5 | ✅ 已销项（方向 C 批次，2026-09-10）：根因 = 协议 §6.4 `get_command` 只查顶层命令；修复 = `FallbackStore::contains_template` + invoke/page 侧跳过校验直执行；与 warm 空闲回收同批修复 |
| L6 | 上游 PowerToys 引用边界复核 | §6 R4 | ✅ 已销项（批次 8.3，2026-09-10）：派生文档已标注「参考设计」+ 核验基准 + 免责声明；代码原创 Rust、协议 v1.0 独立冻结；MIT 已定 |
| L7 | `dd-ext/apps.rs` clippy 风格警告 | 2026-09-03 批次 | ✅ 已销项（2026-09-04 `004b652` 顺手修，`chunks_exact_to_as_chunks` 不再存在） |
| L8 | 设计稿 v4 C 组占位实施 | 设计稿 v4.3 §12 | ✅ 代码完成（2026-09-04，C1–C3）+ **真机验收通过**（2026-09-08，M6 集中回归 B 组：A1–A5/C1–C3） |
| L9 | IME 交互中文输入环境人工复验 | 2026-09-03 记录 | ✅ 已销项（2026-09-08，M6 集中回归 D 组真机复验通过） |
| L10 | A2 冷启动 GUI 瓶颈 | §6 R2 | ✅ 已销项（M6 批次 6.2 L10）：`setup_cjk_fonts` 改后台线程加载，不在主路径 |

---

## 7. 实施沿革（按时间）

> 仅记关键节点，每行 ≤ 60 字。过程细节见对应 `mX-record.md` / CHANGELOG。

| 日期 | 事件 | 结果 |
|---|---|---|
| 2026-09-01 | M0 协议冻结，workspace 构建 | ✅ 40/40 测试 |
| 2026-09-02 | M1/M2/M3 真机验收通过 | ✅ R1/ADR-2 成立 |
| 2026-09-03 | M4 代码完成；M5 插队批次启动 | ✅ 含六轮真机修复 |
| 2026-09-04 | M4 关闭 `757f3b4`；M5 批次1–4.2 `5cf32b7` | ✅ C 组占位 L8 完成 |
| 2026-09-05 | M6 立项；6.1–6.5 + 托盘/设置页/材质 | ✅ 多项真机验收 |
| 2026-09-06 | 字体方块修复；尺寸自适应记忆 | ✅ 文档滞后校正 |
| 2026-09-08 | M6 集中回归关账；M7 立项+CI+256图标 | ✅ 安装器取消 |
| 2026-09-08 | M7 7.5 便携 sidecar 扫描 `818d1db` | ✅ 解压即用 |
| 2026-09-09 | 全库文档核对修正轮 | ✅ v0.1.0 tag 重打 |
| 2026-09-10 | v0.1.1 发版（tag + Release） | ✅ M7 7.5 关账 |
| 2026-09-10 | M8 扩展生态验证（10 项全 ✓） | ✅ 非 Rust 实证 |
| 2026-09-11 | M9 内置扩展 in-process（B1–B5） | ✅ 单文件分发 |
| 2026-09-12 | apps 垃圾项过滤；图标字体优化 | ✅ I1/I2/F1/F2 |
| 2026-09-12 | 运行时内存优化（M1+M2+M3） | ✅ 重编 dist |
| 2026-09-13 | v5.2 UI 优化 B1–B8 全落地 | ✅ `815e212..da0589c` |
| 2026-09-15 | O2 / O1 / O3 / O8 / O4 五批落地；O7 首轮真机验收 | ✅ `6ee4931` + `c26035e` |
| 2026-09-17 | 协议两项定稿（`-32002` / `PageInfo`）；回落静默失败修复 + 工具一致性；O7 第二轮补测 | ✅ `9936ead`（A-33-05/06 改判通过） |
| 2026-09-19 | 模糊排序字段分层；应用列表调试工具过滤；文件搜索 `Ctrl+F` 直达 + Shell 真实图标（v3.6）；移除 `f ` 前缀直达（v3.7）；设计稿回同步 **v4.18** | ✅ 工作副本（447 passed） |
| 2026-09-19 | 文件搜索用户文档**解除 `es.exe` 前置表述**（`search.md` v1.3 / `search-file.md` v3.8）；dist 按 v3.6/v3.7 重打包 | ✅ 工作副本（零代码改动） |
| 2026-09-19 | **E1 修复**：图标按真实路径档首抽 191–704 ms → 分层异步（同步 0.32 ms）+ 后台补齐 + 自动刷新；E2 探针实测可省 −96,768 B | ✅ 工作副本（452 passed） |
| 2026-09-19 | **E2 落地**：零依赖 PNG 编码器（`image` 转 dev-dependency），sidecar 912,384 → 831,488 B、A-IC-06 转 PASS；apps/file 图标 PNG 过第三方校验 | ✅ 工作副本（462 passed） |
| 2026-09-20 | **设置/个性化/材料 B1–B4 落地**（参照 PowerToys CmdPal）：材料注册表化 + 云母 Alt + 着色三档 + Esc/退格/单击/动效行为项 + 恢复默认外观 | ✅ 工作副本（470 passed；真机走查待做） |
| 2026-09-19 | **E2E 首屏计时插桩**：宿主侧 input→paint 三段分解（`app/e2e.rs` + `tools/gui_e2e_parse.py`），A-33-05 感知指标待真机采样 | ✅ 工作副本（465 passed） |
| 2026-09-19 | **dist 重打包**（E2 + E2E 插桩同源）+ 分发级冒烟：conformance 内置 9 步 / 示例 9 步（step 4 按 `has_fallback` 跳过）/ GUI 5s 存活 / sidecar 通道 `ipc` / zip 成员校验 | ✅ `dd-run-0.1.1.exe` 8,780,800 B · sidecar 831,488 B |

> 构建环境记档：本机 windows-gnu 链接需补 `as.exe`（与 dlltool 同目录）与 `libshlwapi.a`（2026-09-03 修复）；跑测试前须 `export APPDATA`（否则 apps 图标抽取测试必失败，见 CHANGELOG）。
