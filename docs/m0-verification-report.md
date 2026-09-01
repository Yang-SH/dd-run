# M0 里程碑验证报告

> **验证日期**：2026-09-01
> **验证依据**：`docs/implementation.md` §2 M0 任务表 + 完成判据（行 45–49）+ 验收映射 A12；`docs/m0-record.md` P1–P8 阶段表与验收标准
> **验证方法（TDD 视角）**：以测试为规格——测试只经**公共接口**验证行为，不耦合内部实现。本次重跑真实构建/测试/CLI（非读取既有结论），并据此逐项判定。
> **验证环境**：rustc/cargo **1.96.0**（stable-x86_64-pc-windows-gnu）、Windows 10、无 VS Build Tools；为消除增量编译噪音，验收统一用 `CARGO_INCREMENTAL=0`。

---

## 1. 实跑命令与原始结果

| 命令 | 结果 |
|---|---|
| `CARGO_INCREMENTAL=0 cargo build` | ✅ Finished；`warning` = 0、`error` = 0 |
| `CARGO_INCREMENTAL=0 cargo test` | ✅ **40 passed / 0 failed**（24 host + 5 roundtrip + 8 framing + 3 一致性） |
| `CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings` | ✅ Finished；0 告警 |
| `cargo fmt --check` | ✅ FMT_OK |
| `dd-run --list-extensions` | ✅ exit=0（扫描报 1 条预期 ✗ + 内置兜底显示示例） |
| `dd-run --roundtrip` | ✅ exit=0；spawn→initialize→top_level_commands→close 四步全绿，总耗时 ~175ms |

---

## 2. M0 任务目标逐项核对（任务表 8 项）

| # | 任务目标 | 对应交付物 | 验收方式（公共接口） | 结论 |
|---|---|---|---|---|
| T1 | Cargo workspace 可构建 | 根 `Cargo.toml` `members = ["crates/*"]` | `cargo build` 0 error | ✅ 通过 |
| T2 | `dd-protocol` 数据模型（§8） | `crates/dd-protocol/src/model.rs` | 一致性测试 46/46 文档示例反序列化到具体类型 | ✅ 通过 |
| T3 | NDJSON 编解码（§2.4，1 MiB 上限） | `crates/dd-protocol/src/framing.rs` | 单测：多消息切分/跨 push 残留/CRLF/超限 `TooLarge`/UTF-8 校验 | ✅ 通过 |
| T4 | JSON-RPC 信封（id 空间判别 §3.3） | `crates/dd-protocol/src/messages.rs` + `process.rs` | 单测：响应/通知/`host/*` 对端请求三态判别 | ✅ 通过 |
| T5 | 错误码（标准 5 + 自定义 -32001…-32005） | `messages.rs` 常量已定义 5 个 | 单测：超时→`-32004` 映射 | ✅ 通过 |
| T6 | 示例扩展（响应 initialize/top_level_commands，2 条命令） | `crates/dd-ext-sample/src/main.rs` | 集成测试 `roundtrip_*` 真实 spawn 子进程 | ✅ 通过 |
| T7 | CLI `dd-run --list-extensions` | `crates/dd-run/src/main.rs` | CLI 实跑 exit=0 + 扫描/校验输出 | ✅ 通过 |
| T8 | 协议一致性测试（抽取 protocol.md 全部示例断言） | `crates/dd-protocol/tests/consistency.rs` | 3 个集成测试（计数/逐条类型化/§2 示例同步） | ✅ 通过 |

---

## 3. M0 完成判据核对（implementation.md 行 45–49）

| 判据 | 实测 | 结论 |
|---|---|---|
| `cargo build` / `cargo test` / `cargo clippy` 全绿 | 三命令均 0 错误/0 警告/40 测试全过 | ✅ 通过 |
| protocol.md 每条示例消息都能被 `dd-protocol` 正确解析（不一致即失败） | 一致性测试 46/46 逐条类型化断言全过（含请求/响应/通知三形态） | ✅ 通过 |
| 宿主 spawn 示例扩展 → initialize → top_level_commands → close 全链路往返成功 | `dd-run --roundtrip` 四步全绿、exit=0 | ✅ 通过 |

**验收映射 A12（代码层双向方法齐全）**：运行时全链路仅覆盖 3/12 方法（initialize / top_level_commands / close），其余 9 方法目前仅有类型定义 + 文档示例反序列化断言，无运行时路径——见 §4 问题项 P3。

---

## 4. 已通过项汇总

- 构建/测试/lint/fmt 四项工程验收**全绿**；
- 8 项 M0 任务目标**全部达成**；
- 3 条 M0 完成判据**全部达成**；
- 协议一致性测试形成「文档即契约」闭环：示例与实现不一致即失败（SSOT 永不漂移）；
- 测试反哺：本轮即上一轮集成测试已抓出握手版本协商误用三段 semver（协议版本实为两段式 §13）的真实缺陷，已修复固化。

---

## 5. 未通过 / 存在问题项

### ⛔ P1（高·阻塞）— LICENSE 缺失，与风险 R4 冲突 → ✅ 已修复（2026-09-01）
- **现象（修复前）**：仓库根目录**无 LICENSE 文件**；所有 `Cargo.toml` 的 `license` 字段**均留空**（grep 无结果）。
- **原因**：`implementation.md` R4 明确「公开仓库前必须补 LICENSE」，但代码在补许可证前已被推送到公开 GitHub（commit c41773d）。
- **影响**：公开仓库在无许可证下，他人**无权使用/分发**代码（默认「保留所有权利」），且 README 已声明的「MIT License」表述与仓库实际（无 LICENSE）自相矛盾。
- **修复**：新增根 `LICENSE`（MIT，Copyright (c) 2026 Yang-SH）；4 个 crate 的 `Cargo.toml` 均补 `license = "MIT"`（workspace 根无 `[package]`，逐 crate 声明）；`implementation.md` R4 行改为「已采用 MIT，引用边界复核留待专项」。README 既有 MIT 声明现已与仓库一致。
- **判定**：✅ **已通过（阻塞解除）**。

### ⚠️ P2（低·观察）— 示例清单 `entry.command` 在仓库内不可直接扫描解析
- **现象**：`dd-run --list-extensions` 首行打印 `✗ ... entry.command 不存在：examples/extensions.d\dd-ext-sample`，随后内置兜底显示示例。
- **原因**：示例清单 `entry.command = "${EXT_DIR}/dd-ext-sample"` 描述的是**部署形态**（扩展二进制与清单同目录），仓库不含构建产物。`dd-run` 在未显式传 `--extensions-dir` 且扫描无果时兜底用与自身同目录的 `dd-ext-sample.exe`。属**设计内的预期行为**，但示例本身不自洽。
- **影响**：演示可用（exit=0），但 `--list-extensions` 输出含一条「错误」行，易误读为缺陷。
- **判定**：⚠️ **通过（带观察项）**。

### ⚠️ P3（中·范围外）— 12 方法运行时覆盖仅 3/12
- **现象**：除 initialize / top_level_commands / close 外，其余 9 个 method 仅类型定义 + 文档示例反序列化断言，无运行时调用路径。
- **原因**：M0 判据只要求「一次命令拉取」全链路，未要求全方法运行时覆盖。A12「双向方法齐全」的实现层运行时部分尚待 M1+ 落地。
- **影响**：验收层面 M0 通过；但 A12 的代码层运行时覆盖度为部分，后续 method 的契约正确性未被实跑验证。
- **判定**：⚠️ **部分通过（M0 范围外，M1 必须补）**。

### ⚠️ P4（低·工具链噪音）— 默认增量编译会刷伪「警告」
- **现象**：Windows 下默认 `cargo build`（开启 incremental）偶发 `error copying object file ... (os error 5)` 的缓存拷贝告警（文件锁/杀软干扰），非代码告警。
- **原因**：incremental 目录对象文件拷贝被并发锁/杀软拦截。
- **影响**：会污染「0 警告」判定。本次用 `CARGO_INCREMENTAL=0` 复验为 0 告警。
- **判定**：✅ **通过（有条件：验收须禁用 increment）**。

### 🔹 P5（极低·cosmetic）— 内置示例版本显示 `v0.0.0`
- **现象**：`--list-extensions` 兜底行显示 `v0.0.0`，而 `dd-ext-sample` 清单声明 `1.0.0`。
- **原因**：`manifest::from_executable`（兜底构造）版本字段取默认值 0.0.0，未读取真实清单。
- **影响**：仅展示不一致；`--roundtrip` 的 initialize 取自进程自身声明，不受影响。
- **判定**：✅ **通过（带观察项）**。

---

## 6. 后续改进建议

| 优先级 | 建议 | 解除项 |
|---|---|---|
| ~~**P0**~~ ✅ 已做 | 补 `LICENSE`（MIT）+ 回填各 crate `license = "MIT"`；README 声明现已一致 | P1（R4）已解除 |
| P1 | 让示例清单**构建后**可被 `--list-extensions` 直接列出：构建产物分发为清单同目录二进制，或在文档明确 `${EXT_DIR}` 语义，使扫描无需兜底即成功，输出更干净 | P2 |
| P1 | M1 起为剩余 9 个 method 补运行时路径测试（至少 `execute_command` / `list_navigate` / `list_refresh` 等核心），把 A12 运行时覆盖从 3/12 提升 | P3（A12） |
| P2 | CI/验收统一 `CARGO_INCREMENTAL=0`（或加 `--frozen` + 缓存），保证「0 警告」稳定；可选把 workspace 级 incremental 目录加入杀软排除 | P4 |
| P3 | `from_executable` 接受版本参数或回读真实清单，消除 `v0.0.0` 展示不一致 | P5 |

---

## 7. 结论

**M0 里程碑的工程验收（构建/测试/clippy/fmt + 3 条完成判据 + 8 项任务目标）全部达成，可判定完成。** 原唯一硬性阻塞 **P1 LICENSE 缺失** 已于 2026-09-01 修复（根 `LICENSE` MIT + 各 crate `license = "MIT"`，R4 解除），合规风险关闭；其余 P2–P5 为观察项/范围外项，不阻断 M0 结论，但应在 M1 前或 M1 中处理。
