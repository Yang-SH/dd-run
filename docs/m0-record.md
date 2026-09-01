# M0 实施记录 — 第一步：协议层落地

> **状态**：✅ 第一步已完成（2026-09-01）。范围 = M0 任务表中的「Cargo workspace + `dd-protocol` 数据模型 + NDJSON 编解码 + 协议一致性测试」四项；示例扩展 / CLI / 宿主全链路**不在本轮**（见 §5 遗留）。
>
> **实施前的三项决策**（与用户一问一答确认）：
> 1. **范围** = M0 第一步（不是全部 M0）；
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
| M0 剩余任务 | 示例扩展（`initialize`/`top_level_commands`/`close`）、清单扫描 CLI（`dd-run --list-extensions`）、宿主 spawn 全链路往返——对应 M0 完成判据第 3 条 |
| 验收 A12 | 「双向方法齐全」的代码层已由 12 method 类型 + 46 示例断言覆盖一半；运行时全链路待 M0 剩余任务 |
| R1（egui 键盘焦点） | 未动，M1 前需预研备选框架退路（ADR-2 补充） |
| R4（LICENSE） | `Cargo.toml` 的 `license` 字段已按此留空；公开前须先定许可证 |

## 6. 复现验收

```bash
cargo clean && cargo build 2>&1 | grep -c "^warning"   # 期望 0
cargo test                                              # 期望 8+3 全过
cargo clippy --all-targets -- -D warnings               # 期望 0 告警
cargo fmt --check                                       # 期望通过
```
