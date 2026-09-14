# dd-run 项目可优化方案（总览）

> **状态**：规划中 ｜ **版本**：v1.2 ｜ **最后更新**：2026-09-14
> **进度**：O2 方法名常量层 ✅ 已落地（2026-09-14，见 §2.3 落地记录）；其余项状态见 §1 状态列。
> **关联**：[implementation.md](./implementation.md) · [INDEX.md](./INDEX.md) · [optimization-plan-review-2026-09-14.md](./optimization-plan-review-2026-09-14.md) · [doc-audit-2026-09-13.md](./doc-audit-2026-09-13.md) · [doc-code-diff-2026-09-13.md](./doc-code-diff-2026-09-13.md) · [memory-optimization-plan.md](./memory-optimization-plan.md)

---

## 0. 编写依据与范围

本方案基于 **2026-09-14 对 `crates/`（73 个 .rs、约 3.1 万行）与 `docs/` 的新一轮取证** 整理，是对既有优化专题（内存 / 分层 / apps 过滤 / 图标字体 / 文件搜索）的**增量补充**，不重复已落地内容。

取证方法：静态扫描（`unwrap`/`expect`/`panic`/`allow`/`todo!`、协议方法分发、依赖实际引用面、`#[test]` 覆盖、日志框架）、交叉核对 `protocol.md` 现状注记与 `INDEX.md §5` 已知开放项，并对每条「可优化点」做**可行性证伪**（例如体积优化需先排除误判项）。

> 结论先行：项目已高度优化（体积 8.2 MB、内存 M1–M3 已落地「长会话真机复验待做」、过滤 3.7 ms 达标「M4 真机记录值，见 m4-record §3.7」、测试覆盖合理）。**剩余可优化空间集中在「协议健壮性/错误处理」「代码可维护性（方法名常量层）」「可观测性」「依赖治理」「跨平台」五处**，其中前三项为低风险确定性改进，后两项为工程化与战略级投入；另含体积边际项（O5）、真机验收项（O7）与文档卫生项（O8）。

---

## 1. 结论总表（按优先级）

| 编号 | 维度 | 机会 | 预期收益 | 改动量 | 风险 | 优先级 | 状态 |
|---|---|---|---|---|---|---|---|
| O1 | 协议错误码/校验接线 | 消息超上限、批处理、`jsonrpc≠2.0`、`result/error` 互斥等 `-32600` 处置未实现；`-32002` 保留码无产出 | 协议与文档一致、流错位防护 | 中（~150 行 + 单测） | 低（加法为主，协议冻结需走版本协商） | 🔴 高 | 已知（INDEX §5，P-01） |
| O2 | 方法名常量层 | 协议方法全用字符串字面量分发，无单一来源 | 消除拼写漂移、协议演进单一来源 | 小（~60 行 + 全仓替换） | 极低 | 🔴 高 | ✅ **已落地**（2026-09-14，§2.3） |
| O3 | 依赖治理/安全 | 未接入 `cargo audit`/`udeps`/`outdated` | 漏洞与死依赖可观测 | 小（CI + 本地） | 低 | 🟠 中 | 新发现 |
| O4 | 可观测性/日志 | 无 `log`/`tracing` 框架，散落 `eprintln!` | 真机排障与复现效率 | 中（多模块） | 低（stdout 协议纯净已天然满足） | 🟠 中 | 新发现 |
| O5 | 体积再压缩 | 评估移除 `eframe default_fonts`；`panic=abort` 证伪 | 边际体积下降 | 小→中 | 中（tofu 风险） | 🟡 低 | 新发现（含证伪） |
| O6 | 跨平台（macOS/Linux） | 仅设计层，无产物/构建矩阵 | 平台覆盖 | 大（独立里程碑 M10） | 高 | ⚪ 战略 | 已知（INDEX §5） |
| O7 | 文件搜索 P2 真机验收 | A-33-05…A-33-10 未做，阻塞「无需 es.exe」承诺 | 兑现用户承诺 | 中（真机） | 中 | 🔴 高 | 已知（INDEX §5） |
| O8 | 文档卫生 | CHANGELOG 缺日期、`examples/python-minimal/README.md` 行尾 LF、I3 32px 回退未做、LRU 容量不可配 | 一致性 | 小 | 极低 | 🟡 低 | 已知（INDEX §5；LRU 项见 §2.7） |

> 优先级图例：🔴 高 / 🟠 中 / 🟡 低 / ⚪ 战略。优先级结合「收益 × 风险 × 已取证确定性」评定。

---

## 2. 各维度详述与取证

### 2.1 二进制体积与分发（已高度优化，余量小）

**现状**：`dist/dd-run-0.1.1.exe` = 8,623,616 B（2026-09-14 实测；release `lto=fat` + `strip=symbols` + `codegen-units=1`，较 M9 前内嵌态工件 10,857,984 B 约 ↓21%）。字节数随重编漂移，引用前须重新实测。`Cargo.toml [profile.release]` 已是最优编译器层参数。

**已证伪的误判**（避免无效改动）：
- `everything-ipc` / `fuzzy-matcher` / `chrono` **仅被 `crates/dd-ext/src/bin/search.rs` 使用**（全仓 grep 仅 1 个文件命中）。它们是 `dd-ext` crate 的依赖（`fuzzy-matcher`/`chrono` 在 `[dependencies]`，`everything-ipc` 在 `[target.'cfg(windows)'.dependencies]`），但 `dd-gui` 仅链接 `dd-ext` **库**（in-process 5 内置），不链接 search sidecar bin；经 `lto=fat` 跨 crate 死代码消除，这些依赖不会进入主 exe。**结论：主 exe 体积不受其拖累，无需拆分 crate。**

**机会**：
- **O5-a（待评估）**：`eframe` 当前 `features = ["glow","default_fonts"]`。`platform.rs` 约 :60/约 :146 用 `egui::FontDefinitions::default()` 作基底再叠加自设 CJK 字体。若自设字体已覆盖全部 UI 字形，可移除 `default_fonts` 省去内置字体嵌入体积。⚠️ 需先在真机穷举字形确认无 tofu，否则退回。属「收益未证实，不得盲目删」。
- **O5-b（明确不做）**：**`panic = "abort"` 不可行**——`ext_inprocess.rs` 约 :262 与 `dd-run-cli/src/main.rs` 约 :472/约 :501 依赖 `catch_unwind` 实现 M9 内置扩展崩溃隔离；`panic=abort` 下 `catch_unwind` 失效，内置 panic 将拖垮宿主，直接破坏已验收的容错能力。明确记档排除。
- **O5-c（边际）**：对非热路径 crate 单独设 `opt-level="z"/"s"`（egui 保持 3）进一步压体积，收益小、回报低，建议暂缓。

### 2.2 协议健壮性 / 错误处理（O1，低风险确定性改进）

交叉核对 `protocol.md` 现状注记（均为 2026-09-13 核对结论），**实现侧普遍未接线**：

| 协议规定 | 实现现状（protocol.md 注记） | 落点 |
|---|---|---|
| 消息 > 1 MiB → 回 `-32600` + 关闭连接 | 两侧均未实现；宿主 `call` 内部返 `MessageTooLarge`，通知轮询与扩展侧**静默丢弃** | §2.3（P-01） |
| `jsonrpc≠"2.0"` → `-32600` | 被**静默忽略** | §3.3 |
| `jsonrpc` 缺失 / `id` 类型非法 → `-32600` | 按 `MalformedEnvelope` 处置（`call` 失败，不回 `-32600`） | §3.3 |
| 批处理数组 → `-32600` | 宿主按反序列化失败冒泡，扩展侧 `serve_line` 回 `-32700`，均未回 `-32600` | §3.3 |
| `result`/`error` 互斥 | 未强制校验，宿主以 `error` 优先 | §3.3 |
| `-32600`/`-32004` 应为致命（关连接） | 当前按**非致命**处置（超限帧静默丢弃） | §10 |
| `-32002 command_not_found` | 保留码，v1.0 **无产出点**（与 `-32004` 边界已厘清） | §9.2 |

**机会**：在协议 v1.0 冻结约束下，区分两类动作——
1. **纯实现侧**（不破坏契约）：把「回 `-32600` + 关连接」在宿主 `process.rs` 与扩展 `serve_line` 两侧补齐；批处理/非法 `jsonrpc` 走该路径；超限帧在 `framing.rs` 解析阶段即拦截。
2. **需版本协商**（若改语义）：`-32002` 是否启用、`PageInfo` 是否接线（运行时不传递，`GetItemsResult` 仅 `items/has_more_items/is_loading`，接线须 `MINOR` 递增）——按 INDEX §5 待人工决策二选一（实现或明确保留）。

**验收**：补协议一致性测试 + 两侧单测（超限帧、批处理、非法信封）。

### 2.3 代码可维护性：方法名常量层（O2，P-18）— ✅ 已落地

**结论（2026-09-14）**：协议 §1.3 的 12 个方法名已收敛为 `dd-protocol::methods` 单一来源，宿主/扩展/CLI 的**生产代码零裸方法字面量**，并由一致性测试锁定常量 ↔ 文档。详见 §2.3.1。

**改动前现状（2026-09-14 取证，作为决策留痕）**：协议方法名以裸字符串字面量出现在分发与调用各处——
- 扩展侧分发（7 臂）：`dd-ext/src/lib.rs` 约 :177（`initialize`）/约 :182（`top_level_commands`）/约 :189（`fallback_commands`）/约 :200（`get_command`）/约 :230（`invoke`）/约 :272（`get_items`）/约 :312（`close`）；
- in-process 调用（7 处）：`dd-gui/src/ext_inprocess.rs` 约 :99/:118/:125/:133/:150/:157/:167；
- 子进程客户端（6 处）：`dd-host/src/process.rs` 约 :358/:382/:397/:411/:426/:433；
- CLI：`dd-run-cli/src/main.rs` 多处；示例：`dd-ext-sample/src/main.rs` 约 :108 起。

**改动前风险**：拼写漂移只在运行时暴露；协议方法演进需全仓手工搜改，易漏。

#### 2.3.1 落地记录（2026-09-14，首项落地）

**常量层**：新增 [`crates/dd-protocol/src/methods.rs`](../crates/dd-protocol/src/methods.rs)，覆盖协议 §1.3 全部 **12 个方法**（10 请求 + 2 通知），并把两类聚合视图一并收口：

| 常量 | 用途 | 消费方 |
|---|---|---|
| `METHOD_*`（7 个 host→ext 请求） | 分发 `match` 臂 + RPC 调用首参 | `dd-ext` `serve_line`、`dd-host` `ExtensionProcess`、`dd-gui` `InProcessExtension`/`ExtClient`、`dd-run-cli`、`dd-ext-sample` |
| `METHOD_HOST_*`（3 个 ext→host 请求） | 反向请求 `method` 字段 + `capabilities` 声明 | `dd-ext` 各扩展 `spec()`/`Effect::HostRequest`、`dd-host` `builtin.rs`、`dd-gui` `execute_host_request` 分发 |
| `NOTIFY_*`（2 个通知） | 通知构造 + 通知判别 | `dd-ext` `make_items_changed`、`dd-host`/`dd-gui` 的 `items_changed` 比较 |
| `HOST_METHODS` | `host/*` 白名单（清单校验规则 9） | `dd-host::manifest::HOST_CAPABILITIES` 改为**别名**（不再重复声明字符串） |
| `HOST_METHOD_PREFIX` | §3.3「带 id 且 method 以 `host/` 开头 → 对端请求」判别 | `dd-host::process::classify` |
| `ALL_METHODS` | 全 12 项（**顺序即 §1.3 表格顺序**） | 一致性测试比对基线 |

**替换面（9 个文件的生产代码）**：`dd-ext/src/lib.rs`（7 个分发臂 + 通知）、`dd-ext/src/builtins/{calc,websearch}.rs`、`dd-ext/src/bin/search.rs`、`dd-ext-sample/src/main.rs`、`dd-gui/src/ext_inprocess.rs`（7 处调用 + 通知判别 + `close` 帧）、`dd-gui/src/ext_client.rs`、`dd-gui/src/app/host_actions.rs`（3 个臂）、`dd-gui/src/app/invoke.rs`、`dd-host/src/{process,manifest,builtin}.rs`、`dd-run-cli/src/main.rs`。

**新增验证（可复跑）**：`crates/dd-protocol/tests/consistency.rs::method_constants_match_protocol_method_table` —— 运行时从 `docs/protocol.md` §1.3 两张表抽取方法名，断言与 `ALL_METHODS` **逐项同序**相等（文档增删方法或常量拼错即红），并校验常量无重复、`host/*` 子集与前缀判别自洽。

**残留字面量的三类豁免（已逐条分类核验）**：

1. `#[cfg(test)]` / `tests/` 内的字面量——**刻意保留**，充当「独立交叉校验」（常量若被改错，引用常量的生产代码会与写死字面量的断言冲突而失败）；
2. `docs/protocol.md` 的 ```json 样例（SSOT 本体）；
3. 日志文案里**内嵌**的方法名（如 `eprintln!("[dd-gui] host/open_url 浏览器打开失败…")`）——非协议身份位，改常量会牺牲可读性。

> ⚠️ **同形不同义陷阱（已加注释防误改）**：`dd-run-cli` 的 `command.kind` 判别值 `"invoke"` / `"page"`（§8.2 `CommandRef`）与协议方法名 `invoke` 同形但**不同命名空间**，不得替换为 `METHOD_INVOKE`。

**验收结果**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **403 passed / 0 failed**（基线 402，+1 为本次新增一致性测试）。协议 v1.0 零改动（无字段/方法增删，纯实现侧重构）。

**纪律备注**：本批仅改工作副本，**未 commit**（按项目约定由人工审核提交）；改动文件的**行尾保持原状**（仓库 `.rs` 工作区本就 CRLF/LF 混存，rustfmt 默认 `newline_style=Auto` 会保留各自原行尾——已实测确认，勿被 `git` 的 LF→CRLF 提示误导而做整体转换）。

### 2.4 可观测性 / 日志（O4，新发现）

**现状**：全仓 grep `use log`/`tracing::`/`env_logger` 等均**无命中**；日志以散落 `eprintln!` 为主（如 `dd-gui/src/platform.rs` 约 :81 起、`app/invoke.rs` 约 :50 起、`dd-run-cli` 等）。协议约束「stdout 仅协议消息，日志走 stderr」当前**天然满足**（`eprintln!`→stderr），故无正确性缺陷，缺的是**分级/开关/结构化**。

**机会**：引入 `log`（facade）+ 轻量 stderr 后端（如 `env_logger` 或自写 `eprintln!` 包装），提供 `debug/info/warn/error` 级别与运行时开关；sidecar（子进程）继续严守 stdout 纯净。

**验收**：`cargo test` 全绿；真机开启 debug 日志可复现一次扩展崩溃/超时链路。

### 2.5 依赖治理 / 安全（O3，新发现）

**现状**：依赖整体克制（`everything-ipc` 锁 `=0.1.4`、各 `image` 仅开 `png`/`ico`、`eframe` `default-features=false`）。但**无自动化治理**。

**机会**：
- CI 增 `cargo audit`（RUSTSEC 漏洞扫描）；
- 本地/CI 增 `cargo udeps`（死依赖）、`cargo outdated`（可升级）；
- 对 `Cargo.lock` 提交约束（已提交，保持）。

**验收**：CI job 绿；无 HIGH/CRITICAL advisory。

### 2.6 跨平台（O6，战略级）

**现状**：仅 `cmdpal-platform-agnostic-design.md` 设计层（验收 A8/A9 为设计口径），**无 macOS/Linux 构建矩阵与产物**。Windows 专属点：`DWM` 窗口材质/圆角/边框（`platform.rs`）、`windows-sys` 调用、托盘/热键的 Win 分支。

**机会（建议立为 M10 独立里程碑）**：补全 `target.{macos,linux}` 平台守卫、egui `wgpu`/`glow` 后端选择、全局热键（`rdev`/`global-hotkey`，M1 已规划）、托盘与材质仅 Win 生效的降级。属高投入、需真机，单独排期。

### 2.7 已知未闭环项收敛（重排优先级）

| 项 | 优先级 | 备注 |
|---|---|---|
| 文件搜索 P2 真机验收 A-33-05…A-33-10 | 🔴 高 | 阻塞「无需 es.exe」用户承诺 |
| 协议错误码接线（见 2.2 / O1） | 🔴 高 | 已纳入 O1 |
| 方法名常量层（见 2.3 / O2） | ✅ 已落地 | 2026-09-14 落地，12 个方法名常量 + 一致性测试（原 🔴 高） |
| macOS/Linux 构建验收 | ⚪ 战略 | 见 2.6 |
| I3 图标 32px 回退 | 🟡 低 | 视真机效果 |
| CHANGELOG 缺日期 / example README CRLF | 🟡 低 | 文档卫生 |
| LRU 容量可配置（Settings 无字段） | 🟡 低 | 承 `doc-code-diff` I-01（文档表述已修），此处提为功能项 |

---

## 3. 推荐实施顺序（分阶段，先确定性的）

- **Phase 1（低风险·确定性，建议首批）**：~~O2 方法名常量~~ ✅ 已落地（2026-09-14）→ **下一步 O1 协议错误码接线** → O3 依赖治理 → O8 文档卫生。
- **Phase 2（中风险·需真机）**：O4 可观测性 → O7 文件搜索 P2 真机验收。
- **Phase 3（战略）**：O6 跨平台 M10。
- **O5 体积**：仅 O5-a 评估后视收益决定，O5-b/O5-c 明确不做/暂缓（见 2.1）。

> 每批遵循项目既有纪律：**设计稿/方案先行 → 仅改工作副本、不自动 commit → fmt/clippy/test 全绿 + 对应单测/一致性测试 + 体积/内存类重编 dist 实测**。

---

## 4. 与既有文档关系

- 不重复 `memory-optimization-plan.md`（内存 M1–M3 已落地）、`refactor-layering-plan.md`、`apps-filtering-plan.md`、`icons-typography-plan.md`、`search-file.md`。
- 本方案为**增量总览**，承接 `doc-code-diff-2026-09-13.md` 已修 71 条之后的「下一轮优化」入口；其 §5（P-01）与 [`INDEX.md`](./INDEX.md) §5（`-32002`/`PageInfo`）的待人工决策项，此处归入 O1/O2 并给出处置建议。
- 落地任一项后，回写 `implementation.md` §2 对应里程碑与 `INDEX.md §5`。
