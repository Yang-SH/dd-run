# dd-run

**跨平台命令面板（Rust）** —— 设计参考自 [PowerToys Command Palette（CmdPal）](https://github.com/microsoft/PowerToys/tree/main/src/modules/cmdpal)

> **当前状态：MVP（M0–M4）已全部关闭并通过真机验收；M5（ueli 风格 UI 重构）主体完成，设计稿 v4.3。** 里程碑进度见 [`docs/implementation.md`](./docs/implementation.md) §5，遗留项台账见其 §6.1。

---

## 这是什么

`dd-run` 是一个用 Rust 从零构建的**跨平台（Windows / macOS / Linux）命令面板 / 启动器**：全局热键唤起，输入即搜，键盘直达，靠扩展生态扩展能力。

它的架构与扩展契约**提炼自微软 PowerToys 的 CmdPal 模块**，但不是 CmdPal 的移植——CmdPal 深度绑定 Windows（WinRT / COM / XAML），`dd-run` 把其中的平台无关部分（UI 模型、扩展契约、宿主生命周期）抽象出来，换成 Rust 生态的等价实现：

| CmdPal（Windows） | dd-run（跨平台） |
|---|---|
| 进程外 COM / WinRT | **子进程 + NDJSON 上的 JSON-RPC**（见 [`docs/protocol.md`](./docs/protocol.md)） |
| AppExtensionCatalog / 注册表发现 | **清单文件扫描**（`extensions.d/*.json`，见 [`docs/manifest-schema.md`](./docs/manifest-schema.md)） |
| WinUI 3 / XAML | egui（MVP 选型，见 [`implementation.md`](./docs/implementation.md) ADR-2） |
| C# 扩展 | 任意能读写 stdin/stdout 的语言 |

---

## 目标与非目标

### 目标

1. **跨平台**：三平台同一套契约与代码，平台差异收敛在适配层。
2. **键盘优先**：核心路径（唤起 / 搜索 / 选择 / 执行 / 返回 / 关闭）**100% 可纯键盘完成**（验收 A11）。
3. **快**：冷启动走磁盘缓存的"命令桩"，扩展进程懒加载（frozen / stub 机制，见设计文档 §6.3）。
4. **可扩展**：第三方扩展是独立进程，崩溃不影响宿主，用文本协议即可接入。
5. **简单可实现**：MVP 只做最小可用集，不预先为假想需求付复杂度。

### 非目标（MVP 阶段明确不做）

- **不做 Windows 专属扩展的跨平台移植**：§7 清单里 9 个 `🪟` 项（注册表、Windows 设置、WinGet 等）无跨平台等价物，不在范围内。
- **不做扩展商店（Gallery）**：可选模块，非 MVP（设计文档 §6.6）。
- **不做 WASM 沙箱 / 进程注册发现**：进阶可选，MVP 用最简路径（ADR-1 / ADR-3）。
- **不做移动端 / Web 端**。
- **不做云端同步、遥测、账号体系**。

---

## 文档导航

| 文档 | 内容 | 读者 |
|---|---|---|
| [`cmdpal-platform-agnostic-design.md`](./cmdpal-platform-agnostic-design.md) | **设计参考（上游来源）**：CmdPal 的 UI 模型、扩展契约、宿主模型、内置扩展清单、Rust 参照实现、验收标准 A1–A12 | 想理解"为什么这样设计" |
| [`cmdpal-ui-mockups.html`](./cmdpal-ui-mockups.html) | **交互设计稿**：11 屏暗色主题组件（Root View / 搜索 / ListPage / DetailPage / FormPage / Markdown / Grid / 上下文菜单 / Confirm+Toast / Empty / Loading），可键盘走查 | 想看界面长什么样 |
| [`docs/implementation.md`](./docs/implementation.md) | **实施方案**：里程碑 M0–M4、ADR 决策记录、A1–A12 验收映射、当前进度 | 要动手写代码 |
| [`docs/protocol.md`](./docs/protocol.md) | **dd-run Extension Protocol v1.0**：NDJSON 成帧、JSON-RPC 信封、12 个方法、错误码、生命周期状态机、超时与崩溃恢复 | 写宿主或写扩展 |
| [`docs/manifest-schema.md`](./docs/manifest-schema.md) | **扩展清单 schema**：字段表、三平台配置目录、最小可拷贝示例 | 写扩展 |

**阅读顺序建议**：设计文档 §1–§7（理解模型）→ [`docs/implementation.md`](./docs/implementation.md)（知道先做什么）→ [`docs/protocol.md`](./docs/protocol.md) + [`docs/manifest-schema.md`](./docs/manifest-schema.md)（照着实现）。

---

## 已定设计决策（摘要）

完整记录与理由见 [`docs/implementation.md`](./docs/implementation.md) 的 ADR 部分。

| # | 决策 | 结论 |
|---|---|---|
| ADR-1 | 扩展隔离方式 | **子进程 + NDJSON JSON-RPC**（WASM 沙箱降为进阶可选） |
| ADR-2 | GUI 框架 | **egui**（Slint / iced 为备选） |
| ADR-3 | 扩展发现方式 | **清单文件扫描**（进程注册 / WASM 内嵌降为可选） |
| ADR-4 | 协议成帧 | **NDJSON**（一行一条 JSON、`\n` 结尾；握手预留 `transport` 字段） |

---

## 上游引用与许可

- 本项目的设计文档大量**提炼、改写自** [`microsoft/PowerToys`](https://github.com/microsoft/PowerToys) 仓库中 CmdPal 模块的官方文档（README / SDK Spec / UI 解剖 / 设计原则 / Gallery 说明），出处逐条列于设计文档 §11。
- **核验基准**：设计文档与规范中对上游事实的引用，均以 `microsoft/PowerToys` tag **v0.101.2362.0**（2026-09-01 核验）为准；上游仍处 preview，可能演进（见设计文档 §11 与 implementation.md R6）。
- PowerToys 采用 **MIT License**；`dd-run` 自身同样采用 **MIT License**（根 `LICENSE` + 各 crate `license = "MIT"`）。引用边界复核为遗留项 L6（见 implementation.md §6.1）。
- `dd-run` 不含任何 PowerToys 源码；所有 Rust 代码为独立实现。

---

## 下一步

- **M0–M4 已全部关闭**：协议冻结 → 最小面板 → 命令执行与状态机 → 缓存懒加载 → 5 内置扩展与健壮性（commit `757f3b4`）。
- **M5（ueli 风格 UI 重构）主体完成**：设计稿 v4.3 + 批次 1–4.2 + 六轮真机反馈修复（commit `5cf32b7`）；剩余设计稿 C 组占位（嵌套页顶行统一 / Loading 骨架 / Dialog 遮罩 / 焦点态 / Toast 意图色），见 implementation.md §6.1 L8。
- **候选 M6 方向（待定义）**：第三方扩展端到端验证、打包分发、A2 冷启动 GUI 瓶颈（wgpu + 22MB 字体 ~2.8s）、跨平台。
- 完整进度与遗留项台账见 [`docs/implementation.md`](./docs/implementation.md) §5 / §6.1。
