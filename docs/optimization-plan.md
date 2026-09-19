# dd-run 项目可优化方案（总览）

> **状态**：规划中 ｜ **版本**：v1.8 ｜ **最后更新**：2026-09-17
> **进度**：**Phase 1 四项全部落地**（O2 09-14 / O1 09-15 / O3 09-15 / O8 09-15）；**Phase 2：O4 ✅（09-15）、O7 两轮真机验收完成**（09-15 首轮 + **09-17 补 `es.exe` 基线与回落分支**：A-33-05 / A-33-06 / A-33-07 / A-33-10 **通过**，A-33-08 ⚠️ 部分 —— 见 [验收报告](./search-file-p2-acceptance-2026-09-15.md) v1.3）。其余项状态见 §1 状态列。
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
| O1 | 协议错误码/校验接线 | 消息超上限、批处理、`jsonrpc≠2.0`、`result/error` 互斥等 `-32600` 处置未实现；`-32002` 保留码无产出 | 协议与文档一致、流错位防护 | 中（~150 行 + 单测） | 低（加法为主，协议冻结需走版本协商） | 🔴 高 | ✅ **已落地**（2026-09-15，§2.2） |
| O2 | 方法名常量层 | 协议方法全用字符串字面量分发，无单一来源 | 消除拼写漂移、协议演进单一来源 | 小（~60 行 + 全仓替换） | 极低 | 🔴 高 | ✅ **已落地**（2026-09-14，§2.3） |
| O3 | 依赖治理/安全 | 未接入 `cargo audit`/`udeps`/`outdated` | 漏洞与死依赖可观测 | 小（CI + 本地） | 低 | 🟠 中 | ✅ **已落地**（2026-09-15，§2.5） |
| O4 | 可观测性/日志 | 无 `log`/`tracing` 框架，散落 `eprintln!` | 真机排障与复现效率 | 中（多模块） | 低（stdout 协议纯净已天然满足） | 🟠 中 | ✅ **已落地**（2026-09-15，§2.4） |
| O5 | 体积再压缩 | 评估移除 `eframe default_fonts`；`panic=abort` 证伪 | 边际体积下降 | 小→中 | 中（tofu 风险） | 🟡 低 | 新发现（含证伪） |
| O6 | 跨平台（macOS/Linux） | 仅设计层，无产物/构建矩阵 | 平台覆盖 | 大（独立里程碑 M10） | 高 | ⚪ 战略 | 已知（INDEX §5） |
| O7 | 文件搜索 P2 真机验收 | 两轮完成（2026-09-15 首轮；**09-17 补 `es.exe`**）：A-33-05/06/07/10 **✅ 通过**（基线降幅 91.27%、回落分支 3/3）、A-33-08 ⚠️ 部分（跨环境矩阵）；**同批修掉 `es.exe` 失败静默为空结果的产品缺陷** | 兑现用户承诺 | 中（真机） | 中 | 🔴 高 | ✅ **已落地**（[验收报告](./search-file-p2-acceptance-2026-09-15.md) v1.3） |
| O8 | 文档卫生 | CHANGELOG 缺日期、`examples/python-minimal/README.md` 行尾 LF、I3 32px 回退未做、LRU 容量不可配 | 一致性 | 小 | 极低 | 🟡 低 | ✅ **已落地**（2026-09-15，§2.7.1：CHANGELOG 已补日期、行尾已合规；I3 按设计文档保留「视真机」、LRU 判定属功能项，均不在本项范围） |

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

### 2.2 协议健壮性 / 错误处理（O1）— ✅ 已落地

**结论（2026-09-15）**：协议 §2.3 / §3.2 / §3.3 / §3.4 / §9.3 规定的 `-32600`/`-32700` 处置与**致命性区分**已全部接线，校验规则收口为 `dd-protocol::envelope` 单一来源；超限帧由「静默丢弃」改为协议要求的「回 `-32600` + 关闭连接」。详见 §2.2.1。

**改动前现状（2026-09-14 取证，作为决策留痕）**：

| 协议规定 | 改动前实现 | 落点 |
|---|---|---|
| 消息 > 1 MiB → 回 `-32600` + 关闭连接 | 两侧均未实现；宿主 `call` 内部返 `MessageTooLarge`，通知轮询与扩展侧**静默丢弃** | §2.3（P-01） |
| `jsonrpc≠"2.0"` → `-32600` | 宿主侧**静默忽略**（扩展侧已回 `-32600`） | §3.2 |
| `jsonrpc` 缺失 / `id` 类型非法 → `-32600` | 宿主按 `MalformedEnvelope` 处置（`call` 失败，不回 `-32600`） | §3.2/§3.3 |
| 批处理数组 → `-32600` | 宿主按反序列化失败冒泡；扩展侧回 `-32700`，均未回 `-32600` | §3.4 |
| `result`/`error` 互斥、`params` 须为对象 | 未强制校验 | §3.2 |
| `-32600`（超上限支）应为致命 | 超限帧按**非致命**处置（静默丢弃） | §9.3 |

**明确不属本项**：`-32002` 是否启用、`PageInfo` 是否接线——二者须走 §13 `MINOR` 演进，保持 [INDEX](./INDEX.md) §5 的「待人工决策」状态，本项未动。

#### 2.2.1 落地记录（2026-09-15，Phase 1 第二项）

**单一来源**：新增 [`crates/dd-protocol/src/envelope.rs`](../crates/dd-protocol/src/envelope.rs)——把「一行消息 → 合法信封 / `-32700` / `-32600`」的三态判定收口一处，宿主与扩展两侧共用（沿用 O2 `methods`「单一来源」的同一套路）：

| 导出 | 用途 |
|---|---|
| `Envelope`（`Valid` / `ParseError` / `InvalidRequest { id, reason }`） | 三态判定结果；`InvalidRequest.id` 为「能从消息里取到的 id」，取不到则 `None`（序列化为 `null`） |
| `InvalidReason`（9 种） | `batch_array` / `not_an_object` / `missing_jsonrpc` / `unsupported_jsonrpc_version` / `invalid_id` / `invalid_method` / `non_object_params` / `result_and_error` / `invalid_error_object` |
| `validate(&str)` / `validate_value(Value)` | 行入口（子进程路径）与值入口（in-process 路径） |
| `error_response(...)` / `Envelope::to_error_response()` | §9.1 错误对象构造；两侧 `-32700`/`-32600` 形状由同一份代码保证（`dd-ext` 的私有 `make_error` 已删除；`dd-ext-sample` 的 `send_error` 改为委托） |

**接线面**（改动 3 个文件 + 新增 1 个文件；`dd-gui` / `dd-run-cli` 零改动，靠复用路径自动继承）：

| 位置 | 改动 |
|---|---|
| `dd-host/src/process.rs` | `call` / `poll_notifications` / `handle_line` 三处改用 `envelope::validate`；新增 `reply_envelope_error`（非致命回错 + 留痕）与 `abort_oversized`（回 `-32600` + 关连接）；`route_messages` 改用 `validate_value` |
| `dd-ext/src/lib.rs` | `run()` 帧循环显式处理 `TooLarge`（回 `-32600` → 退出）与 `InvalidUtf8`（记日志丢弃，§2.2 规则 6）；`serve_line` 改用 `envelope::validate`（批处理由误回的 `-32700` 纠正为 `-32600`） |
| `dd-ext-sample/src/main.rs` | 与扩展侧同口径（示例扩展代表第三方写法） |
| `dd-gui` / `dd-run-cli` | **无改动**：分别复用 `serve_line` + `route_messages`（in-process）与 `ExtensionProcess`（子进程），自动继承 |

**致命性区分（§9.3 的关键细节）**：`-32600` 有多类触发，§9.3 只把「消息超上限」列为致命：

| 触发 | 处置 | 关连接 |
|---|---|---|
| 消息超上限（§2.3） | `-32600`，`id: null`，`data.reason = message_too_large` | **是**（宿主 `kill` + `wait`；扩展退出进程） |
| 批处理 / 缺 `jsonrpc` / 非 `"2.0"` / `id` 非法 / `params` 非对象 / `result`+`error` 并存 | `-32600`，`data.reason` = 对应 kind | 否（连接保持） |
| 行非合法 JSON | `-32700`，`id: null` | 否 |
| 行非 UTF-8（§2.2 规则 6） | 丢弃（该规则不定义错误码） | 否 |

**⚠️ 一处反直觉但必须的形状**：`"id": null` **判为合法**（视同「无 id」）。理由：本协议自身的 `-32600`/`-32700` 响应在无法确定 id 时即发 `id: null`（§2.3）；若判为非法，对端收到的是**合法**错误响应却被再次判非法并回错，往返互喷形成自激回路。已由 `envelope::tests::accepts_null_id_as_absent` 与 `dd-ext::null_id_response_is_accepted_not_rejected` 双侧锁定。

**行为变更（非纯加法，须知悉）**：超限帧从**静默丢弃**改为**回错 + 关连接**——这是协议 §2.3/§9.3 既有规定的补齐，非新增语义，协议 v1.0 **零改动**。连带一处口径更新：`route_messages` 对非法信封由「完全忽略」改为「留痕进 `unmatched`」（便于诊断、不丢信息），既有断言 `route_messages_splits_by_kind` 随之由 1 条改为 2 条。

**新增验证（可复跑，共 +19 项）**：

| 层 | 测试 | 覆盖 |
|---|---|---|
| `dd-protocol` | `envelope::tests::*`（11 项） | 合法三形态 / 非 JSON / 批处理 / 非对象顶层 / 缺或错 `jsonrpc` / `id` 各非法类型 / `id` 可解析时回带 / `method`、`params`、`error` 形状 / `result`+`error` 互斥 / `id: null` 合法 / 错误响应形状 |
| `dd-ext` | `serve_line` 信封组（6 项） | 批处理纠正为 `-32600`、缺 `jsonrpc`、`id` 类型非法、`params` 非对象、`result`+`error`、`id: null` 回归 |
| `dd-host` | `route_messages_marks_invalid_envelopes_without_fatal`（1 项） | 非法信封留痕，且同批次的合法响应仍匹配成功 |
| `dd-host` 端到端 | `roundtrip::oversized_request_closes_connection_with_32600`（1 项） | **真实子进程**：>1 MiB 请求 → 扩展回 `-32600`（`id: null`、`reason=message_too_large`）→ 关闭连接（宿主收 EOF）→ 进程退出 |

**验收结果**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` **0 warning**；`cargo test --workspace` **422 passed / 0 failed**（基线 403，+19）；`dd-run-cli --conformance --ext-id com.ddrun.calc` **9 步全绿**。

**已知残留（如实记录）**：

1. **宿主侧「扩展发来超限帧」无独立端到端测试**——现有示例扩展不会主动发超限消息。该分支（`call` / `poll_notifications` 的 `TooLarge`）由 `framing` 单测（保证产出 `TooLarge`）与下述扩展侧端到端共同覆盖，代码为「写 `-32600` → `kill` → `wait`」三步无分支逻辑。
2. **`--conformance` 对 `dd-ext-sample` 在第 `4) fallback` 步报 `-32601`**——经核对 HEAD 版本，该示例**从未实现** `fallback_commands` 分发臂（清单 `has_fallback: false`），而 conformance 第 4 步无条件调用该方法。属**既存**的「工具 ↔ 示例」判据不一致，**与本批无关**（内置扩展走 `dd-ext` 库、具备该臂，自检 9 步全绿）。→ **✅ 已修（2026-09-17 批次「工具一致性」随附）**：`has_fallback=false` 时跳过 step 4 并明示原因（`main.rs::fallback_is_consistent` 纯函数 + 四象限单测，见 `implementation.md` §2 对应行）；2026-09-19 打包冒烟复核：示例扩展 conformance **9 步全过**、内置 9 步无回归。

**纪律备注**：本批仅改工作副本，**未 commit**（按项目约定由人工审核提交）。

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

**替换面（13 个文件的生产代码）**：`dd-ext/src/lib.rs`（7 个分发臂 + 通知）、`dd-ext/src/builtins/calc.rs`、`dd-ext/src/builtins/websearch.rs`、`dd-ext/src/bin/search.rs`、`dd-ext-sample/src/main.rs`、`dd-gui/src/ext_inprocess.rs`（7 处调用 + 通知判别 + `close` 帧）、`dd-gui/src/ext_client.rs`、`dd-gui/src/app/host_actions.rs`（3 个臂）、`dd-gui/src/app/invoke.rs`、`dd-host/src/process.rs`、`dd-host/src/manifest.rs`、`dd-host/src/builtin.rs`、`dd-run-cli/src/main.rs`。另 `dd-protocol/src/lib.rs` 为模块导出、`tests/consistency.rs` 为新增一致性测试，**不计入**生产替换面。

**新增验证（可复跑）**：`crates/dd-protocol/tests/consistency.rs::method_constants_match_protocol_method_table` —— 运行时从 `docs/protocol.md` §1.3 两张表抽取方法名，断言与 `ALL_METHODS` **逐项同序**相等（文档增删方法或常量拼错即红），并校验常量无重复、`host/*` 子集与前缀判别自洽。

**残留字面量的三类豁免（已逐条分类核验）**：

1. `#[cfg(test)]` / `tests/` 内的字面量——**刻意保留**，充当「独立交叉校验」（常量若被改错，引用常量的生产代码会与写死字面量的断言冲突而失败）；
2. `docs/protocol.md` 的 ```json 样例（SSOT 本体）；
3. 日志文案里**内嵌**的方法名（如 `eprintln!("[dd-gui] host/open_url 浏览器打开失败…")`）——非协议身份位，改常量会牺牲可读性。

> ⚠️ **同形不同义陷阱（已加注释防误改）**：`dd-run-cli` 的 `command.kind` 判别值 `"invoke"` / `"page"`（§8.2 `CommandRef`）与协议方法名 `invoke` 同形但**不同命名空间**，不得替换为 `METHOD_INVOKE`。

**验收结果**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **403 passed / 0 failed**（基线 402，+1 为本次新增一致性测试）。协议 v1.0 零改动（无字段/方法增删，纯实现侧重构）。

**纪律备注**：本批仅改工作副本，**未 commit**（按项目约定由人工审核提交）；改动文件的**行尾保持原状**（仓库 `.rs` 工作区本就 CRLF/LF 混存，rustfmt 默认 `newline_style=Auto` 会保留各自原行尾——已实测确认，勿被 `git` 的 LF→CRLF 提示误导而做整体转换）。

### 2.4 可观测性 / 日志（O4）— ✅ 已落地

**结论（2026-09-15）**：接入 `log` facade + 自写极简 stderr 后端，生产代码 **131 处** `eprintln!` 全部改为分级日志；开关 `DDRUN_LOG` 实测生效，**默认级别刻意保持 `debug`（与改造前逐行等价）**。详见 §2.4.1。

**改动前现状（2026-09-14 取证）**：全仓 grep `use log` / `tracing::` / `env_logger` 均**无命中**；日志以散落 `eprintln!` 为主（**147 处**，其中 `dd-gui/src/platform.rs` 24、`app/page.rs` 11、`app/invoke.rs` 11、`app/host_actions.rs` 11…）。格式统一为 `[模块标签] 消息`。协议约束「stdout 仅协议消息，日志走 stderr」**天然满足**（`eprintln!`→stderr），故当时**无正确性缺陷**，缺的是**分级/开关**。

#### 2.4.1 落地记录（2026-09-15，Phase 2 首项）

**基础设施**：新增 [`crates/dd-protocol/src/logging.rs`](../crates/dd-protocol/src/logging.rs)——三条设计约束来自协议与分发形态：

| 约束 | 实现方式 |
|---|---|
| **恒写 stderr**（§2.5） | 后端固定 `stderr`，从机制上排除「日志污染扩展 stdout 被宿主当协议帧」的风险 |
| **零重依赖** | 只加 `log` facade（零传递依赖）；**不引 `env_logger`**（它会拖入 anstream / termcolor / anstyle 一串），延续 O3 建立的依赖克制基调 |
| **默认与历史等价** | 未设环境变量时级别为 `Debug`——O4 之前所有日志都无条件输出，若默认提到 `Info` 会让原本可见的排障信息静默消失；**分级应是「可降噪」而非「默认更少」** |

| 导出 | 用途 |
|---|---|
| `logging::init()` | 各进程入口调用；幂等且失败安全（重复调用返回 `false` 不 panic） |
| `DDRUN_LOG` | 级别开关，取值 `off/error/warn/info/debug/trace`（大小写不敏感），支持按 target 定向：`warn,dd_gui::app=debug`（最长前缀优先） |
| `Filters` / `parse_level` / `StderrLogger` | 可测的纯逻辑（8 条单测覆盖默认值、裸级别覆盖、大小写、定向最长前缀、`off` 全静默、`max_level` 上界） |

**接线面（入口 4 处 + 分级替换 24 个文件）**：

| 位置 | 改动 |
|---|---|
| 入口 | `dd-gui/src/main.rs`、`dd-run-cli/src/main.rs`、`dd-ext/src/lib.rs::run`、`dd-ext-sample/src/main.rs` 各加一行 `dd_protocol::logging::init()` |
| 依赖 | `dd-protocol`（`log = { version = "0.4", features = ["std"] }`——`set_boxed_logger` 在 `alloc` 之下，默认 feature 在 workspace 合并后可能被关掉，**实测踩到**故显式声明）、`dd-gui`、`dd-ext`、`dd-ext-sample`、`dd-run-cli` 各加 `log = "0.4"` |
| 分级替换 | **131 处**（24 个文件）：`log::debug!` **85**、`log::warn!` **39**、`log::info!` **7**（`error!` 0 处——现有日志点均属「失败但可继续」，无进程级致命路径） |

**分级口径（保守、可回溯）**：异常路径（失败/错误/拒绝/超时/不可用/回落/熔断）→ `warn!`；生命周期与关键状态 → `info!`；协议流、计数与内部步骤 → `debug!`。因**默认即 `debug`**，分级判断偏差不会造成信息丢失（默认可观测性与改造前一致）——这正是选此默认值的原因。

**消息格式零变化**：保留既有 `[模块标签] 消息` 文本，后端**不加任何前缀/颜色**，接入前后输出逐字一致。

**实测验证（可复跑）**：

| 检查 | 结果 |
|---|---|
| 分级开关 | `dd-run-cli --conformance --ext-id com.ddrun.calc`：`DDRUN_LOG=debug` → stderr 7 行；`info` / `warn` / `off` → stderr **0 行** |
| **默认档等价** | 未设 `DDRUN_LOG` → stderr 7 行（与 `debug` 档一致 = 与改造前逐行等价） |
| 定向过滤 | `DDRUN_LOG=warn,dd_ext=debug` → 7 行（仅放行 `dd_ext` 的 debug 协议流） |
| **协议纯度** | 四档下 stdout **恒 14 行**且「一致性自检通过」——日志从未进入协议通道 |
| 三关 | `fmt --all -- --check` 无差异；`clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **430 passed / 0 failed**（基线 422，+8 为日志单测） |

**豁免与保留（如实记录）**：

1. **测试与构建脚本的 `eprintln!` 保留**（`dd-host/tests/roundtrip*.rs` 14 处、`dd-gui/build.rs` 1 处、`apps.rs` 测试内 1 处）——它们是测试输出/构建期提示，不属运行时日志。
2. **GUI 子系统的可见性限制**：`dd-gui` 为 `windows_subsystem="windows"`，**release 下无控制台**，stderr 默认不可见。真机排障需从终端启动或重定向 `dd-run.exe 2> dd-run.log`（已在 `main.rs` 注释与 §2.4.1 写明）。
3. **方案原文的「结构化」未做**：验收只要求「分级与开关」；结构化字段（`log` 的 kv 语法）需启用 `log/kv` feature 并逐点改造调用，收益未证实，**明确不做**。
4. **真机验收待做**：方案验收含「真机开启 debug 日志可复现一次扩展崩溃/超时链路」——本批已具备能力（开关实测生效 + 崩溃链路日志已分级），**真机复现动作待用户执行**。

**纪律备注**：本批仅改工作副本，**未 commit**（按项目约定由人工审核提交）。

> ⚠️ **过程记录**：批量替换脚本用 `Path.read_text()` 读取（默认做换行规范化）后以 `newline=""` 写回，**曾担心把 CRLF 文件翻成 LF**；随后按 `git show HEAD:<file>` 的行尾风格逐文件核验，确认 24 个受影响文件中仅 3 个原本是 CRLF 且已恢复，`git diff --numstat` 增删对称（无整文件重写）。教训：**批量改 Rust 源码务必用字节模式或显式 `newline=""` 读**，并在改后用 `numstat` 验证。

### 2.5 依赖治理 / 安全（O3）— ✅ 已落地

**结论（2026-09-15）**：CI 新增独立 `audit` job（RustSec 漏洞扫描 + 死依赖检测），并以 Dependabot 承担「可升级」维度；本地实跑两项检查均为**零发现**。详见 §2.5.1。

**改动前现状（2026-09-14 取证）**：依赖整体克制（`everything-ipc` 锁 `=0.1.4`、`image` 仅开 `png`/`ico`、`eframe` `default-features=false`），但**无任何自动化治理**——CI 只有 build / test / clippy / fmt 四关，依赖漏洞与死依赖没有可观测入口。

#### 2.5.1 落地记录（2026-09-15，Phase 1 第三项）

**选型（对方案原建议做了两处务实替换，均附依据）**：

| 维度 | 方案原建议 | 实际落地 | 理由 |
|---|---|---|---|
| 漏洞 | `cargo audit` | ✅ `cargo audit`（照办） | CI 用 `taiki-e/install-action@v2` 取**预编译**二进制，不占 runner 编译时间 |
| 死依赖 | `cargo udeps` | ⚠️ 改用 `cargo machete` | udeps 需 **nightly 工具链**且每次冷编译整棵依赖树（CI 从分钟级变十分钟级）；machete 基于 identifier 匹配、stable 秒级，对 339 包规模足够；误报可用 `[package.metadata.ignore]` 豁免 |
| 可升级 | `cargo outdated` | ⚠️ 改用 **Dependabot** | outdated 需编译元数据且无提醒机制；Dependabot 零维护、发现新版本自动开 PR（按月，低打扰） |
| lock 提交 | 保持提交 | ✅ 保持（`Cargo.lock` 已在版本控制内） | — |

**改动面（2 个文件）**：

| 文件 | 内容 |
|---|---|
| `.github/workflows/ci.yml` | 新增 `audit` job（`ubuntu-latest`——审计与平台无关，linux runner 最省时）：`cargo audit`（发现 vulnerability 即失败；unmaintained / informational 只警告不阻塞，避免第三方软性告警卡死 CI）+ `cargo machete` |
| `.github/dependabot.yml` | **新增**：cargo 与 github-actions 两个生态按月检查更新 |

**本地实测取证（2026-09-15）**：

| 工具 | 结果 |
|---|---|
| `cargo machete` v0.9.2 | **零死依赖**（"didn't find any unused dependencies"） |
| `cargo audit` v0.22.2 | 载入 **1246** 条 RustSec advisory，扫描 **339** 个依赖 → **无任何漏洞**（exit 0） |

> 两个环境注记：①本机 `cargo install cargo-audit`（windows-gnu）**编译失败**（长依赖链，重试无益），改用官方 release 的 `x86_64-pc-windows-msvc` **预编译二进制**——msvc 产物在 Windows 上直接可跑、不依赖工具链，这也正是 CI 里用 `install-action` 的同一理由；②本机代理对 `github.com/.../releases/download` 有 CONNECT 限制（curl 报 `CONNECT tunnel failed, 502`），走 PowerShell 通道可绕过。

**验收结果**：

- **本地**：`cargo audit` 与 `cargo machete` 均**零发现**；`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **422 passed / 0 failed**（本批无 Rust 代码改动，与上批持平）。
- **CI**：新增 job 的语法与缩进已静态核对 → **2026-09-16 已在 runner 上实跑确认**：`dependencies (audit + machete)` job **success**（main@`9936ead`）；Dependabot 亦已生效，开出 4 个更新 PR（eframe 0.36.2 / pinyin 0.11.0 / actions/checkout 7 / action-gh-release 3），其分支 CI 亦全绿。

**未覆盖（如实记录）**：unmaintained / unsound / yanked 类告警当前**只警告不阻塞**。若后续要收紧，可把 `audit` 步骤改为 `cargo audit --deny warnings`，但那会把第三方仓库的软性告警一并变成红灯，建议先评估噪音量再定。

**纪律备注**：本批仅改工作副本，**未 commit**（按项目约定由人工审核提交）。

### 2.6 跨平台（O6，战略级）

**现状**：仅 `cmdpal-platform-agnostic-design.md` 设计层（验收 A8/A9 为设计口径），**无 macOS/Linux 构建矩阵与产物**。Windows 专属点：`DWM` 窗口材质/圆角/边框（`platform.rs`）、`windows-sys` 调用、托盘/热键的 Win 分支。

**机会（建议立为 M10 独立里程碑）**：补全 `target.{macos,linux}` 平台守卫、egui `wgpu`/`glow` 后端选择、全局热键（`rdev`/`global-hotkey`，M1 已规划）、托盘与材质仅 Win 生效的降级。属高投入、需真机，单独排期。

### 2.7 已知未闭环项收敛（重排优先级）

| 项 | 优先级 | 备注 |
|---|---|---|
| 文件搜索 P2 真机验收 A-33-05…A-33-10 | ✅ 已落地 | **两轮完成**（09-15 首轮 / **09-17 补 `es.exe`**）：A-33-05 基线 p95 324.21ms → 降幅 **91.27%**、A-33-06 回落分支 **3/3 返回真实结果** → 与 A-33-07/A-33-10 一并 **通过**；**A-33-08 ⚠️ 部分**（Win10 / Everything 1.5 / 提升 / 命名实例未覆盖）＝当前唯一红线来源 —— 详见 [验收报告](./search-file-p2-acceptance-2026-09-15.md) v1.3 |
| 协议错误码接线（见 2.2 / O1） | ✅ 已落地 | 2026-09-15 落地：`dd-protocol::envelope` 共享校验层 + 超限致命化（原 🔴 高） |
| 方法名常量层（见 2.3 / O2） | ✅ 已落地 | 2026-09-14 落地，12 个方法名常量 + 一致性测试（原 🔴 高） |
| macOS/Linux 构建验收 | ⚪ 战略 | 见 2.6 |
| I3 图标 32px 回退 | ⏸ 保留 | 设计文档已定「视真机效果再定，不阻塞交付」——属**真机依赖项**（Phase 2 范畴），非文档卫生问题（§2.7.1） |
| CHANGELOG 缺日期 / example README CRLF | ✅ 已落地 | 2026-09-15：`[0.1.0]` 补日期；行尾全仓 `.md` 均 `i/lf w/crlf` 合规（§2.7.1） |
| LRU 容量可配置（Settings 无字段） | 🔵 转功能项 | **不属文档卫生（O8）**：需 Settings 字段 + 设置页行 + 持久化 + 消费点改造，待单独立项（§2.7.1） |

#### 2.7.1 落地记录（2026-09-15，Phase 1 第四项 O8）

§1 表格把 O8 列为四项，**取证后发现四者性质不同**，故按「能确定性收尾的做完、不宜在本批做的明确记档」处置：

| 原列项 | 实测状态 | 处置 |
|---|---|---|
| CHANGELOG 缺日期 | `[0.1.0]` 条目确实缺日期（`[0.1.1]` 条目已有） | ✅ **已修**：补为 `## [0.1.0] - 2026-09-06`——日期取自 `git for-each-ref` 的 tag creatordate（实测值，非推测） |
| `examples/python-minimal/README.md` 行尾 LF | **已合规**：`git ls-files --eol` 显示全仓 `.md` 均为 `i/lf w/crlf` | ✅ **无需改动**——2026-09-14 的「全仓 30 份 md 行尾归一化」已覆盖该文件，此项仅作确认 |
| I3 图标 32px 回退 | [`icons-typography-plan.md`](./icons-typography-plan.md) §3/§6 明载「未做；主链路覆盖绝大多数应用，**视真机效果再定，不阻塞交付**」 | ⏸ **保持不做**：属**真机依赖项**（Phase 2 范畴），不是文档卫生缺陷 |
| LRU 容量可配 | `crates/dd-gui/src/app/pool.rs` 为编译期常量 `LRU_WARM_CAPACITY = 8`，`Settings` 无对应字段 | 🔵 **转功能项**：需「Settings 字段 + 设置页行 + 持久化往返 + 消费点改造 + 单测」，投入与风险均超出文档卫生；**明确不在 O8 范围**，待需要时单独立项 |

> **本项实际范围（结论）**：O8 名义四项，实质只有 **1 项是真正的文档卫生缺陷**（CHANGELOG 日期），另 1 项已于前批解决（行尾），剩余 2 项经取证确认**分属真机依赖项与功能项**——本批未越界处理，已逐条记档以便后续排期。这是把不确定性从交付里剔除，而非遗漏。

**验证**：本批仅改 1 处 `CHANGELOG.md`（版本标题补日期）+ 文档口径；`cargo fmt --all -- --check` 无差异、`cargo clippy --workspace --all-targets` 0 warning、`cargo test --workspace` **422 passed / 0 failed**（无代码改动）；`python tools/docscan.py` 失效链接 (none)。

**纪律备注**：本批仅改工作副本，**未 commit**（按项目约定由人工审核提交）。

---

## 3. 推荐实施顺序（分阶段，先确定性的）

- **Phase 1（低风险·确定性，建议首批）**：~~O2 方法名常量~~ ✅（2026-09-14）→ ~~O1 协议错误码接线~~ ✅（2026-09-15）→ ~~O3 依赖治理~~ ✅（2026-09-15）→ ~~O8 文档卫生~~ ✅（2026-09-15）——**四项全部落地，Phase 1 关闭**。
- **Phase 2（中风险·需真机）**：~~O4 可观测性~~ ✅（2026-09-15）→ ~~O7 文件搜索 P2 真机验收~~ ✅ **两轮完成**（2026-09-15 / **2026-09-17 补 `es.exe`**）——**Phase 2 全部落地**。O7 剩余 3 项未闭环（GUI 端到端指标、A-33-03 的 `%`/`#` 打开、A-33-08 剩余矩阵单元）见 [验收报告](./search-file-p2-acceptance-2026-09-15.md) §5。
- **Phase 3（战略）**：O6 跨平台 M10。
- **O5 体积**：仅 O5-a 评估后视收益决定，O5-b/O5-c 明确不做/暂缓（见 2.1）。

> 每批遵循项目既有纪律：**设计稿/方案先行 → 仅改工作副本、不自动 commit → fmt/clippy/test 全绿 + 对应单测/一致性测试 + 体积/内存类重编 dist 实测**。

---

## 4. 与既有文档关系

- 不重复 `memory-optimization-plan.md`（内存 M1–M3 已落地）、`refactor-layering-plan.md`、`apps-filtering-plan.md`、`icons-typography-plan.md`、`search-file.md`。
- 本方案为**增量总览**，承接 `doc-code-diff-2026-09-13.md` 已修 71 条之后的「下一轮优化」入口；该文 §5 的原 P-01（超限处置）已由本项落地，[`INDEX.md`](./INDEX.md) §5 的 `-32002`/`PageInfo` 仍属待人工决策项（须走 §13 `MINOR` 演进）。
- 落地任一项后，回写 `implementation.md` §2 对应里程碑与 `INDEX.md §5`。
