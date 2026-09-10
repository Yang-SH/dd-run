# dd-run

[English](./README.md) | [简体中文](./README.zh-CN.md)

**用 Rust 从零构建的跨平台命令面板 / 启动器。** 设计参考自 [PowerToys Command Palette（CmdPal）](https://github.com/microsoft/PowerToys/tree/main/src/modules/cmdpal) 模块。

> **当前状态**：MVP 里程碑 M0–M4 已全部关闭并通过真机验收；M5（ueli 风格 UI 重构）已完成；M6（设置与系统集成）已落地并通过真机验收（2026-09-08 M6 集中回归 A–F 全部通过）——设置页、Mica/Acrylic 窗口材质、托盘图标、自定义全局热键、拼音搜索、文件搜索扩展。里程碑进度见 [`docs/implementation.md`](./docs/implementation.md) §5，遗留项台账见其 §6.1。

---

## 这是什么

`dd-run` 是一个**跨平台（Windows / macOS / Linux）命令面板 / 启动器**：全局热键唤起，输入即搜，键盘直达，靠扩展生态扩展能力。

它的架构与扩展契约**提炼自微软 PowerToys 的 CmdPal 模块**，但不是 CmdPal 的移植——CmdPal 深度绑定 Windows（WinRT / COM / XAML），`dd-run` 把其中的平台无关部分（UI 模型、扩展契约、宿主生命周期）抽象出来，换成 Rust 生态的等价实现：

| CmdPal（Windows） | dd-run（跨平台） |
|---|---|
| 进程外 COM / WinRT | **子进程 + NDJSON 上的 JSON-RPC**（见 [`docs/protocol.md`](./docs/protocol.md)） |
| AppExtensionCatalog / 注册表发现 | **清单文件扫描**（`extensions.d/*.json`，见 [`docs/manifest-schema.md`](./docs/manifest-schema.md)） |
| WinUI 3 / XAML | egui（见 [`docs/implementation.md`](./docs/implementation.md) ADR-2） |
| C# 扩展 | 任意能读写 stdin/stdout 的语言 |

## 特性一览

- **键盘优先**：核心路径（唤起 → 搜索 → 选择 → 执行 → 返回 → 关闭）100% 可纯键盘完成。
- **6 个内置扩展**：应用、计算器、系统、网页搜索、Shell、文件搜索（基于 Everything，仅 Windows；根视图输入 `f ` 前缀直达）。
- **拼音匹配**：中文名按全拼与首字母模糊命中（如 `jsq` → 计算器）。
- **设置页**（`Ctrl+,` 打开）：外观（亮暗主题、Mica/Acrylic 材质）、常规（开机自启、自定义全局热键——默认 `Win+Alt+Space`）、搜索引擎管理（预设 + 自定义 `{q}` 模板引擎）、扩展管理（按扩展启停）。
- **多语言**：面板 UI 跟随系统显示语言。
- **快且隔离**：冷启动读磁盘"命令桩"，扩展进程懒加载（frozen / stub / LRU 机制）；每个扩展运行在独立进程中，崩溃不影响宿主。
- **便携分发**：`dist/dd-run-0.1.1.exe`（约 10 MB，`strip = "symbols"` + fat LTO），5 个内置扩展在编译期内嵌进宿主字节，首次启动物化到用户缓存目录——**进程隔离（ADR-1）完整保留**。文件搜索扩展以 sidecar 形态随 `dist/extensions.d/` 携带，宿主自动扫描可执行文件同目录。无安装器：解压发版 zip 即用。

## 目标与非目标

### 目标

1. **跨平台**：三平台同一套契约与代码，平台差异收敛在适配层。
2. **键盘优先**：核心路径 100% 可纯键盘完成（验收 A11）。
3. **快**：冷启动走磁盘缓存的"命令桩"，扩展进程懒加载（frozen / stub 机制，见设计文档 §6.3）。
4. **可扩展**：第三方扩展是独立进程，崩溃不影响宿主，用文本协议即可接入。
5. **简单可实现**：MVP 只做最小可用集，不预先为假想需求付复杂度。

### 非目标（MVP 阶段明确不做）

- **不做 Windows 专属扩展的跨平台移植**：§7 清单里 9 个 `🪟` 项（注册表、Windows 设置、WinGet 等）无跨平台等价物，不在范围内。
- **不做扩展商店（Gallery）**：可选模块，非 MVP（设计文档 §6.6）。
- **不做 WASM 沙箱 / 进程注册发现**：进阶可选，MVP 用最简路径（ADR-1 / ADR-3）。
- **不做移动端 / Web 端**。
- **不做云端同步、遥测、账号体系**。

## 构建与打包

> **硬原则：宿主是单文件——5 个内置扩展已内嵌进宿主字节。**
> 发版布局 = `dist/dd-run-<version>.exe` + `dist/extensions.d/`（文件搜索 sidecar），整体 zip 附着 GitHub Release。解压即用，无安装器。

| 项 | 说明 |
|---|---|
| 入口产物 | `dist/dd-run-0.1.1.exe`（Windows，约 10 MB；版本随 `crates/dd-gui/Cargo.toml` 升） |
| 内嵌方式 | 5 个内置扩展 exe（`dd-ext-{apps,calc,system,websearch,shell}`）经 `dd-gui/build.rs` 编译期内嵌进宿主字节（`assets/embed/` 为打包脚本的临时输入，已 gitignore） |
| 运行机制 | 首次启动由 `dd-gui::embedded::materialize` 物化到 `%APPDATA%/dd-run/cache/embedded/`（`.host-version` 内容指纹标记幂等刷新），宿主按原 `ensure_builtins` + `ExtensionProcess::spawn` 拉起子进程——**进程隔离完整保留** |
| Sidecar | 文件搜索扩展（`dd-ext-search.exe` + `com.ddrun.filesearch.json`）随 `dist/extensions.d/` 携带；宿主扫描可执行文件同目录的 `extensions.d/`（批次 7.5），zip 解压即用——优先级：用户数据目录 > sidecar > 内置 |
| 入口命名 | `dd-run.exe` = GUI 宿主（crate 名仍 `dd-gui`）；M0 CLI 改名 `dd-run-cli.exe`（保留自检能力、让出产物名） |
| 一键出包 | `bash tools/package.sh`（先 build `dd-ext` → 拷 5 exe → build `dd-gui --bin dd-run` → 产物与 sidecar 归集到 `dist/`） |
| 发版自动化 | tag 推送 → `.github/workflows/release.yml`：校验 tag 与 crate 版本一致 → `package.sh` → 打 zip `dd-run-<ver>.exe + extensions.d/` → 附着 GitHub Release |
| 仓库纯净度 | 源码树不含任何二进制（`/dist/`、`/crates/dd-gui/assets/embed/*.exe` 均 gitignore） |
| 验证口径 | `cargo fmt --check` / `cargo clippy --workspace --all-targets` / `cargo test --workspace` **全绿** + 脱离源码树冒烟（隔离目录 → 物化 → warm 握手） |

**为什么不做安装器 / 文件搜索为何保持 sidecar**：项目 MVP 阶段显式选择"简单可实现"——免安装绿色 zip 满足"零安装仪式"，且不必为注册表 / 安装卸载 / 升级脚本付复杂度（M7 批次 7.3 曾探索 Inno Setup 安装器，后按决策放弃，不留安装器代码）。文件搜索不纳入内嵌：它依赖用户环境依赖（Everything），sidecar 让「带不带文件搜索」与宿主本体解耦（M7 批次 7.4 取舍记档，见 [`docs/implementation.md`](./docs/implementation.md) §5）。

## 文档导航

| 文档 | 内容 | 读者 |
|---|---|---|
| [`cmdpal-platform-agnostic-design.md`](./cmdpal-platform-agnostic-design.md) | **设计参考（上游来源）**：CmdPal 的 UI 模型、扩展契约、宿主模型、内置扩展清单、Rust 参照实现、验收标准 A1–A12 | 想理解"为什么这样设计" |
| [`cmdpal-ui-mockups.html`](./cmdpal-ui-mockups.html) | **交互设计稿**（v4.13）：亮暗双主题组件库——根视图 / 搜索 / 列表与详情页 / 设置卡片 / 上下文菜单 / Dialog / Toast / Loading 骨架，可键盘走查。后续批次（v4.14–v4.17a）记录于 [`docs/implementation.md`](./docs/implementation.md) §7，尚未回同步到本 HTML | 想看界面长什么样 |
| [`docs/implementation.md`](./docs/implementation.md) | **实施方案**：里程碑 M0–M7、ADR 决策记录、A1–A12 验收映射、当前进度与遗留项台账 | 要动手写代码 |
| [`docs/protocol.md`](./docs/protocol.md) | **dd-run Extension Protocol v1.0**：NDJSON 成帧、JSON-RPC 信封、方法、错误码、生命周期状态机、超时与崩溃恢复 | 写宿主或写扩展 |
| [`docs/manifest-schema.md`](./docs/manifest-schema.md) | **扩展清单 schema**：字段表、三平台配置目录、最小可拷贝示例 | 写扩展 |
| [`docs/search-file.md`](./docs/search-file.md) | **文件搜索扩展方案**（v3.2）：Everything 优先 Provider、`f ` 前缀直达、速度打磨 | 写扩展 |
| [`docs/search.md`](./docs/search.md) | **文件搜索用户指南**（v0.1.0+）：Everything/es.exe 配置、语法速查、故障排查 | 最终用户 |

**阅读顺序建议**：设计文档 §1–§7（理解模型）→ [`docs/implementation.md`](./docs/implementation.md)（知道先做什么）→ [`docs/protocol.md`](./docs/protocol.md) + [`docs/manifest-schema.md`](./docs/manifest-schema.md)（照着实现）。

## 已定设计决策（摘要）

完整记录与理由见 [`docs/implementation.md`](./docs/implementation.md) 的 ADR 部分。

| # | 决策 | 结论 |
|---|---|---|
| ADR-1 | 扩展隔离方式 | **子进程 + NDJSON JSON-RPC**（WASM 沙箱降为进阶可选） |
| ADR-2 | GUI 框架 | **egui**（Slint / iced 为备选） |
| ADR-3 | 扩展发现方式 | **清单文件扫描**（进程注册 / WASM 内嵌降为可选） |
| ADR-4 | 协议成帧 | **NDJSON**（一行一条 JSON、`\n` 结尾；握手预留 `transport` 字段） |

## 上游引用与许可

- 本项目的设计文档大量**提炼、改写自** [`microsoft/PowerToys`](https://github.com/microsoft/PowerToys) 仓库中 CmdPal 模块的官方文档（README / SDK Spec / UI 解剖 / 设计原则 / Gallery 说明），出处逐条列于设计文档 §11。
- **核验基准**：设计文档与规范中对上游事实的引用，均以 `microsoft/PowerToys` tag **v0.101.2362.0**（2026-09-01 核验）为准；上游仍处 preview，可能演进（见设计文档 §11 与 implementation.md R6）。
- PowerToys 采用 **MIT License**；`dd-run` 自身同样采用 **MIT License**（根 `LICENSE` + 各 crate `license = "MIT"`）。引用边界复核为遗留项 L6（见 implementation.md §6.1）。
- `dd-run` 不含任何 PowerToys 源码；所有 Rust 代码为独立实现。

## 下一步

- **M0–M4 已全部关闭**：协议冻结 → 最小面板 → 命令执行与状态机 → 缓存懒加载 → 5 内置扩展与健壮性（commit `757f3b4`）。
- **M5 已完成**：ueli 风格 UI 重构，批次 1–4.2 + 六轮真机反馈修复（commit `5cf32b7`）+ 设计稿 C 组占位（Loading 骨架 / Dialog 遮罩 / Toast 意图色）。
- **M6 已完成**（2026-09-08 M6 集中回归 A–F 真机验收全部通过）：设置页（外观 / 常规 / 搜索 / 扩展）、Mica/Acrylic 窗口材质、托盘图标、拖动与缩放、自定义全局热键、开机自启、拼音搜索、多语言、冷启动 CJK 字体后台加载、文件搜索扩展（`f ` 直达）（commits `a007656`…`2e5b3da`；设计稿批次 v4.17/v4.17a——设计稿 HTML 文件本身停在 v4.13）。
- **M7 进行中**（发布工程，2026-09-08 立项）：GitHub Actions CI（windows-gnu 上 build / test / clippy `-D warnings` / fmt 四关，首跑绿灯）、`assets/app.ico` 增 256px 档。分发定案：**仅免安装单文件绿色版**（曾探索 Inno Setup 安装器后按决策放弃，不留安装器代码）；文件搜索扩展以 sidecar 形态随 `dist/extensions.d/` 携带。宿主会扫描**可执行文件同目录的** `extensions.d/`（批次 7.5），绿色 zip **解压即用**、无需人工拷贝。剩余：真机走查（解压 → `f ` 前缀可进文件搜索）+ 一次 tag → GitHub Release 演练。
- **候选方向**：第三方扩展端到端验证、A2 冷启动 GUI 瓶颈（异步字体修复前 wgpu + 22MB 字体 ~2.8s）、跨平台。完整进度与遗留项台账见 [`docs/implementation.md`](./docs/implementation.md) §5 / §6.1。

