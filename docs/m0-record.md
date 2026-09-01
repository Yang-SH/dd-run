# M0 实施记录

> **状态**：✅ **M0 全部完成**（第一步 2026-09-01；第二步同日完成）。
> 第一步范围 = M0 任务表中的「Cargo workspace + `dd-protocol` 数据模型 + NDJSON 编解码 + 协议一致性测试」四项（P1–P4）；
> 第二步范围 = 「示例扩展 + 清单扫描 + 宿主全链路往返 + CLI」四项（P5–P8），对应 M0 完成判据第 3 条。
>
> **实施前的三项决策**（与用户一问一答确认）：
> 1. **范围** = 分两步实施（先协议层，后全链路）；
> 2. **载体** = 本文件（`docs/m0-record.md`），与 SSOT 规范分离，`implementation.md` 仅加指路行；
> 3. **测试用例来源** = 运行时抽取 `docs/protocol.md` 的全部 ` ```json ` 围栏（不做 fixtures 副本，SSOT 永不漂移）。

---

## 1. 分阶段实施计划与进度

| 阶段 | 内容 | 状态 | 验收标准（量化） | 结果 |
|---|---|---|---|---|
| P1 脚手架 | 根 `Cargo.toml`（workspace）+ `crates/dd-protocol`（serde/serde_json） | ✅ | `cargo build` 0 error | ✅ 通过 |
| P2 协议类型 | 按 protocol.md §3/§5–§9 实现 serde 类型 + §8 数据模型 + §2.2/§2.4 NDJSON 编解码（含 1 MiB 上限） | ✅ | 覆盖 §8 全部模型；clippy `-D warnings` 0 告警 | ✅ 通过 |
| P3 一致性测试 | 运行时抽取 protocol.md 全部 JSON 示例，按 (章节, 序号) 映射到具体类型反序列化断言 | ✅ | 示例覆盖率 100%（46/46）、解析失败 0 | ✅ 通过 |
| P4 验收与记录 | 干净构建（`cargo clean` 后）全量验证，结果写入本文件；`implementation.md` 加指路行 | ✅ | 四项验收全绿 + 记录完整 | ✅ 通过 |
| P5 清单扫描模块 | `dd-host/src/manifest.rs`：清单模型（忽略未知字段）、§4 路径展开（`${EXT_DIR}`/`~`/相对）、§2 三平台目录、§7 全部 9 条校验规则、扫描（不递归、字典序） | ✅ | §7 九条规则逐条有单元测试；clippy 0 告警 | ✅ 通过（14 个单测） |
| P6 进程管理模块 | `dd-host/src/process.rs`：spawn 子进程、§5 握手（版本协商）、§3.3 id 空间判别、`host/*` 反向请求派发、§10 超时、§6.6 close 与强杀 | ✅ | 握手/判别/超时均有单测；clippy 0 告警 | ✅ 通过（7 个单测） |
| P7 示例扩展 | `dd-ext-sample`：stdin/stdout NDJSON 循环，响应 `initialize`（provider id 与清单一致）/`top_level_commands`（2 条硬编码命令）/`close`（响应后退出）；未知 method 回 -32601；日志只走 stderr | ✅ | 全链路集成测试覆盖三 method + 未知 method 分支 | ✅ 通过 |
| P8 CLI 与验收 | `dd-run`：`--list-extensions` / `--roundtrip` / `--extensions-dir` / `--help`，零新外部依赖；干净验收全绿，结果写入本文件 | ✅ | 两个子命令实跑通过；四项验收全绿 | ✅ 通过 |

## 2. 产出文件清单

| 文件 | 行数 | 内容 |
|---|---|---|
| `Cargo.toml` | 3 | workspace 根，members = `crates/*`（许可证字段按 R4 留空） |
| `.cargo/config.toml` | 15 | 锁定 `x86_64-pc-windows-gnu` 目标（免 VS，见 §4） |
| `.gitignore` | — | `/target` |
| `crates/dd-protocol/Cargo.toml` | 10 | crate v1.0.0；serde 1 (derive) + serde_json 1 |
| `crates/dd-protocol/src/lib.rs` | 13 | 模块导出 |
| `crates/dd-protocol/src/model.rs` | 172 | §8 数据模型：`Icon`/`Details`/`EmptyContent`/`CommandRef`/`CommandItem`/`CommandResult`(8 Kind)/`Sender`/`PageInfo`/`GridProperties` |
| `crates/dd-protocol/src/messages.rs` | 239 | §3 信封（`RawMessage`/`RpcError`）+ 12 method 的参数/结果类型 + §9 错误码常量 |
| `crates/dd-protocol/src/framing.rs` | 202 | §2.2/§2.3/§2.4 NDJSON 增量解码器（CRLF 容错、空行忽略、1 MiB 上限、UTF-8 校验）+ `encode`/`decode_message` |
| `crates/dd-protocol/tests/consistency.rs` | 483 | 协议一致性测试（见 §3） |
| `crates/dd-host/Cargo.toml` | 11 | 宿主库 v0.1.0；dd-protocol + serde + serde_json（零新外部依赖） |
| `crates/dd-host/src/lib.rs` | 8 | 模块导出 |
| `crates/dd-host/src/manifest.rs` | 764 | 清单模型（18 字段、忽略未知字段）、§4 路径展开、§2 平台目录、§7 九条校验规则、`scan_dir`（不递归、字典序、JSON only）+ 14 个单元测试 |
| `crates/dd-host/src/process.rs` | 642 | `ExtensionProcess`：spawn、§5 握手与版本协商（两段式 `MAJOR.MINOR`）、§3.3 id 空间判别、`host/*` 派发、§10 超时映射、§6.6 close/强杀 + 7 个单元测试 |
| `crates/dd-host/tests/roundtrip.rs` | 253 | 5 个全链路集成测试（见 §3.4） |
| `crates/dd-ext-sample/Cargo.toml` | 10 | 示例扩展 bin v0.1.0；仅依赖 dd-protocol |
| `crates/dd-ext-sample/src/main.rs` | 228 | stdin/stdout NDJSON 循环：`initialize`/`top_level_commands`/`close`；未知 method 回 -32601；日志只走 stderr |
| `crates/dd-run/Cargo.toml` | 10 | 宿主 CLI v0.1.0；仅依赖 dd-host |
| `crates/dd-run/src/main.rs` | 322 | `--list-extensions` / `--roundtrip` / `--extensions-dir` / `--help`；内置示例扩展兜底 |
| `examples/extensions.d/com.example.sample.json` | 12 | 示例清单（`${EXT_DIR}/dd-ext-sample` 部署形态；仓库不含二进制，扫描报入口不存在属预期，CLI 走内置兜底） |

所有类型遵循 §13「必须忽略未知字段」（serde 默认行为），为协议向前兼容留出空间。

## 3. 测试方法与结果

### 3.1 测试方法

| 测试 | 方法 | 断言什么 |
|---|---|---|
| `framing`（lib 单测 ×8） | 构造字节流喂 `Decoder::push` | 多消息切分、跨 push 残留保留、CRLF 剥离、空行忽略、超限 `TooLarge`、非法 UTF-8、encode 拒绝裸换行、serde 往返 |
| `example_count_matches_mapping` | 运行时抽取 `docs/protocol.md` 全部 ` ```json ` 围栏并按 `### N.M` 章节归组 | 抽取数 == 测试映射表数（46）；数量变化即失败并提示同步 §13 |
| `every_example_deserializes_to_typed_contract` | 逐块按 (章节, 序号) 映射到具体类型 + 字段值断言 | 46/46 示例反序列化成功且字段值与文档一致（含请求信封/响应信封/通知无 id 三种形态） |
| `framing_follows_section_2` | 用 §2.2 的示例行走完整 encode→decode→解析 | 成帧规则与文档示例一致 |

**SSOT 机制**：测试**不持有任何 JSON 副本**，用例 100% 来自 `docs/protocol.md` 当前内容。协议文档一改，测试立即按新内容跑——示例与实现不一致即测试失败（对应 `implementation.md` M0 完成判据第 2 条）。

### 3.2 验收结果（2026-09-01，`cargo clean` 后干净全量）

| 验收项 | 命令 | 结果 |
|---|---|---|
| 构建 0 error / 0 warning | `cargo clean && cargo build` | ✅ warning 行数 = 0 |
| 单元测试 | `cargo test`（lib） | ✅ 8 passed / 0 failed |
| 协议一致性测试 | `cargo test`（integration） | ✅ 3 passed / 0 failed（含 46/46 示例逐条类型化断言） |
| Lint | `cargo clippy --all-targets -- -D warnings` | ✅ 0 告警 |
| 格式 | `cargo fmt --check` | ✅ 通过 |

环境：rustc/cargo **1.96.0**（stable-x86_64-pc-windows-gnu），Windows 10，无 VS Build Tools。

### 3.3 一致性测试反哺文档（协议文档的真实缺陷被测试抓出）

测试首次运行即暴露 `docs/protocol.md` §7.2 的两处缺陷（这正是「文档即契约」要的效果）：

| # | 缺陷 | 修复 |
|---|---|---|
| 1 | 参数表标题误植为「**阿里云镜像**」 | 改为「**参数**」 |
| 2 | **缺 `host/show_status` 请求示例**（只有响应块，而 §7.3/§7.4 均为请求+响应成对） | 补请求示例 `{"jsonrpc":"2.0","id":1,"method":"host/show_status","params":{...}}`（示例数 45 → 46） |

### 3.4 第二步（P5–P8）测试方法与结果

| 测试 | 方法 | 断言什么 |
|---|---|---|
| `manifest`（lib 单测 ×14） | 构造临时目录 + 清单 JSON 逐条喂 `scan_dir` | §4 路径展开（`${EXT_DIR}`/`~`/相对/Windows 盘符绝对路径）、§7 九条规则逐条（非法 JSON、schema 不支持、缺必填、版本非法、他平台静默跳过、host 过旧、id 重复保首个、缺 entry.command、未知 capability）、字典序/不递归/JSON only、未知字段忽略 |
| `process`（lib 单测 ×7） | 构造信封喂消息判别与错误映射逻辑 | §3.3 id 空间判别（响应/通知/`host/*` 对端请求）、通知形状不被误改、协议版本**两段式**（§13）、超时错误 → `-32004` 映射、未知消息忽略不致命 |
| `roundtrip`（集成 ×5） | 真实 spawn `dd-ext-sample` 子进程走完整协议 | 扫描可发现示例扩展并解析入口、`initialize`→`top_level_commands`→`close` 全链路、stdout EOF 报进程退出、未知 method 回 -32601、版本协商拒绝高于宿主所发的版本 |

### 3.5 第二步验收结果（2026-09-01，删 `target/` 后全新构建）

| 验收项 | 命令 | 结果 |
|---|---|---|
| 构建 0 error / 0 warning | `cargo build`（`CARGO_INCREMENTAL=0`，全量重编 4 crate） | ✅ warning 行数 = 0 |
| 全量测试 | `cargo test` | ✅ **40 passed / 0 failed**（8 framing + 3 一致性 + 14 manifest + 7 process + 5 roundtrip + 3 空 bin） |
| Lint | `cargo clippy --all-targets -- -D warnings` | ✅ 0 告警 |
| 格式 | `cargo fmt --check` | ✅ 通过 |
| CLI 实跑 `--list-extensions` | `dd-run --list-extensions` | ✅ exit=0（扫描报错信息符合预期 + 内置示例兜底显示） |
| CLI 实跑 `--roundtrip` | `dd-run --roundtrip` | ✅ exit=0，四步全绿：spawn（~0.6s）→ initialize（协议 1.0）→ top_level（2 条命令）→ close（进程自行退出） |

> 增量编译噪音说明：Windows 下 touch 全量重编时曾出现 `error copying object file ... (os error 5)` 的 incremental 缓存拷贝告警（文件锁/杀软干扰），非代码告警；设 `CARGO_INCREMENTAL=0` 后复验为 0 告警。

### 3.6 第二步测试反哺实现（抓出一处真实缺陷）

| # | 缺陷 | 修复 |
|---|---|---|
| 1 | 握手版本协商误用清单的三段 semver 解析器，而协议版本是**两段式** `MAJOR.MINOR`（protocol.md §13） | 新增两段式解析；集成测试 `protocol_version_is_two_part_not_three` 固化该契约 |

## 4. 环境障碍与解决记录（Windows 免 VS 工具链）

按时间顺序，三次障碍、三层原因，最终方案稳定可复现：

| # | 现象 | 根因 | 解决 |
|---|---|---|---|
| 1 | msvc 目标链接失败：rustc 把 Git Bash 的 coreutils `link.exe` 当链接器 | 本机无 VS Build Tools；PATH 上 `link` 被占用 | 依 §8.3「免 VS」原则，改用 **gnu 工具链**（`rustup default stable-x86_64-pc-windows-gnu`），自带自足链接器，`.cargo/config.toml` 锁定 target |
| 2 | gnu 工具链装完却报 `Missing manifest` / `timeout reading rustc version` | 多次中途终止的安装留下**残缺工具链目录**，rustup 见目录存在就跳过重装 | 卸载并手动删除 `~/.rustup/toolchains/stable-x86_64-pc-windows-gnu` 与 `~/.rustup/downloads`、`~/.rustup/tmp` 后重装 |
| 3 | 重装 12 分钟无进展、下载目录 0 增长 | 本机全局代理（`127.0.0.1`）把阿里云流量绕道海外出口，极慢且卡死 | 下载 `rustup` 时设 `RUSTUP_DIST_SERVER=https://mirrors.aliyun.com/rustup` **并** `NO_PROXY=mirrors.aliyun.com` 直连。修正后 82 秒装完 |

> 经验：本机后续任何 rustup/cargo 组件安装都应同时设**镜像 + NO_PROXY** 两个变量；残缺工具链的唯一可靠恢复方式是删目录重装，`rustup toolchain uninstall` 对残缺目录不可靠。

## 5. 遗留与下一步

| 项 | 说明 |
|---|---|
| 验收 A12 | 「双向方法齐全」：12 method 类型 + 46 示例断言（协议层）+ `initialize`/`top_level_commands`/`close` 运行时全链路（第二步）已覆盖 M0 范围；其余 method 的运行时路径在 M1+ 逐步落地 |
| R1（egui 键盘焦点） | 未动，M1 前需预研备选框架退路（ADR-2 补充） |
| R4（LICENSE） | 各 `Cargo.toml` 的 `license` 字段已按此留空；公开前须先定许可证 |
| M1 | UI 宿主（egui）+ 命令执行链路（§6.2 `execute_command`）——按 `implementation.md` M1 计划推进 |

## 6. 复现验收

```bash
cargo build                                             # 期望 0 error / 0 warning
cargo test                                              # 期望 40 全过
cargo clippy --all-targets -- -D warnings               # 期望 0 告警
cargo fmt --check                                       # 期望通过
./target/x86_64-pc-windows-gnu/debug/dd-run.exe --list-extensions   # 期望 exit=0
./target/x86_64-pc-windows-gnu/debug/dd-run.exe --roundtrip         # 期望 exit=0，四步全绿
```
