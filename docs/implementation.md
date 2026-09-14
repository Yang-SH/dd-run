# dd-run 实施方案

> **状态**：生效中 ｜ **版本**：v0.1.1 ｜ **最后更新**：2026-09-13
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

**方案**（演示页 [`docs/file-search-result-redesign.html`](./file-search-result-redesign.html)，未提交）：

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

> 构建环境记档：本机 windows-gnu 链接需补 `as.exe`（与 dlltool 同目录）与 `libshlwapi.a`（2026-09-03 修复）；跑测试前须 `export APPDATA`（否则 apps 图标抽取测试必失败，见 CHANGELOG）。
