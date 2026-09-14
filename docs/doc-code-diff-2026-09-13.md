# 文档描述 ↔ 代码实现 差异清单（2026-09-13）

> **状态**：复核通过，进入修复阶段 ｜ **版本**：v1.3 ｜ **最后更新**：2026-09-14
> **阶段说明**：v1.0 为纯只读核对记录。v1.1 经三路独立复核（P 系 / M+E 系 / I+D+R+S 系逐条验真），修正了 9 处清单自身的不实或偏差条目（见 §1.2），并开始按"文档对齐代码"落实修复。v1.2 追加 §10，记录 09-13 复核之后由**代码侧改动**（而非文档对齐）消除的差异（P-06）。v1.3 追加 §11，记录 O2 方法名常量层落地闭环的 P-18。

---

## 1. 核对范围与方法

| 项 | 内容 |
|---|---|
| 核对对象 | 文档：`docs/` 下 21 份 + 仓库根 3 份<br>代码：`crates/dd-protocol` / `dd-host` / `dd-ext` / `dd-gui` / `dd-run-cli`、`tools/`、`.github/workflows/`、`examples/`、`dist/` |
| 事实源 | **代码为唯一事实源**。文档与代码冲突判 `不一致`；代码有而文档无判 `文档未覆盖`；文档有而代码无判 `悬空`；无法从代码唯一判定判 `存疑` |
| 五个维度 | **接口**（方法/字段/类型是否存在与方向）· **参数**（名/类型/必填/默认值/取值范围/单位）· **数据结构**（字段名/类型/可选性/枚举变体/未知字段处理）· **业务逻辑**（行为顺序/判定规则/优先级/缓存失效/状态流转）· **边界条件**（空值/空数组/超限/并发/编码/平台差异/失败回落） |
| 方法 | 按文档域切成 6 路并行只读核对，每路独立提取可验证条目后逐项与代码比对，再由主核对去重合并 |
| 执行方式 | **纯只读**：未修改、未创建（除本文件）、未删除任何既有文件 |

### 1.1 统计

| 区域 | 文档 | 核对项 | 一致 | 不一致 | 存疑/悬空/未覆盖 |
|---|---|---|---|---|---|
| A | `protocol.md` §2/§3/§10/§11/§12 | 41 | 28 | 8 | 5 |
| B | `protocol.md` §1.3/§4–§9 | 119 | 104 | 11 | 4 |
| C | `manifest-schema.md` | 51 | 33 | 8 | 10 |
| D | `extensions.md` | 约 30 | 22 | 4 | 4 |
| E | `implementation.md` + 设计文档 + README + CHANGELOG | 约 40 | 29 | 9 | 2 |
| F | `search-file.md` + `search.md` + 历史归档 | 46 | 30 | 14 | 2 |
| | **合计** | **约 327** | **约 246** | **54** | **27** |

**去重后差异条目：71 条**（高 3 / 中 26 / 低 42）。

### 1.2 v1.1 复核修正（2026-09-13，三路独立验真）

复核对全部 71 条逐一验真后，修正清单自身的以下条目；其余 62 条复核确认属实：

| ID | 修正内容 |
|---|---|
| P-13 | 代码行号 `framing.rs:19` → `framing.rs:20`（变体定义），产出点为 `framing.rs:73` |
| P-17 | 强断言收窄：**巡检路径**（`app/health.rs:27`）仅以非 0 退出码计崩溃；但 **invoke/get_items 失败路径**在进程已退出（含 EOF/正常退出，`process.rs:442-443`）时同样调用 `record_crash`（`invoke.rs:73`、`page.rs:134`） |
| P-19 | 子断言修正：运行期崩溃（有缓存）经 `app/pool.rs:68-79` `drop_source_to_stub` 回退 **stub** 而非"移除 provider"；`aggregator.rs:495` 属**启动期聚合失败**分支（不产出 items）；"busy 无载体"不成立——`invoke.rs:120,152`、`page.rs:232,265` 有 `ext_busy` Toast 载体。仅"无状态枚举"子断言属实 |
| P-22 | 代码文件引用 `model.rs:146-148` → `messages.rs:146-148`（`GetCommandResult` 的 `skip_serializing_if`） |
| P-24 | **删除**。该条自身为悬空引用（指向 §5 存疑清单，但清单中并无 P-24），无可核对内容 |
| P-08 / E-03 / M-15 | crate 路径更正：`crates/dd-host/src/aggregator.rs` 不存在，实际为 `crates/dd-gui/src/aggregator.rs`（行号不变） |
| E-08 | 删除"文中'10 项'不成立"子断言（`extensions.md` 无"10 项"表述）；"13 项属实、in-process 分支标签为 `1) open` 而非 `1) spawn`"保留成立 |
| E-09 | **删除**。in-process 宿主侧 `InProcessExtension::initialize`（`ext_inprocess.rs:97-106`）**有**完整版本校验（`BadProtocolVersion`），原断言"无版本校验"与代码相反；扩展侧共享运行时不读 `params.protocol_version` 已由 E-06 / P-07 覆盖 |
| S-13 | 反证行号 `search-file.md:591` → `:771`（另 `:11`、`:869` 亦为 `ShellExecuteW` 表述；`:591` 实为 L3 测试 `#[ignore]` 说明） |

另注：`cmdpal-platform-agnostic-design.md` 位于**仓库根**而非 `docs/`（本清单 §3/§4 引用时未写路径前缀，不构成错误）。

---

## 2. 高严重度（建议优先确认）

### P-01 ｜ 边界条件 ｜ 超限消息的处置两侧都未实现

- **位置**：`docs/protocol.md:96`（§2.3）与 `:663-666`（§9.3）
- **文档**：收到超过上限的消息，接收方应回一个 `-32600 Invalid Request`，**并关闭连接**；§9.3 也把「消息超上限」列为错误不致命的两个例外之一。
- **代码**：
  - 宿主 `call` 路径：`crates/dd-host/src/process.rs:575` — 收到 `Frame::TooLarge` 直接返回 `ProtocolError::MessageTooLarge`，**不回 `-32600`、不关闭连接**。
  - 宿主通知轮询：`process.rs:527` — `TooLarge` / `InvalidUtf8` 走 `Ok(_) => {}` **静默丢弃**。
  - 扩展侧：`crates/dd-ext/src/lib.rs:114-137` — `run()` 只处理 `Frame::Message`，`TooLarge` / `InvalidUtf8` 同样静默丢弃、不回复不关闭。
- **差异**：不一致。**两侧都没有实现文档规定的"回错 + 关连接"**。

### S-01 ｜ 边界条件 ｜ 文档体系自我违反自己定的红线

- **位置**：`docs/search-file.md:13`（另 `:888`）
- **文档**：用户承诺红线——**A-33-06 / A-33-08 通过前，不得把「无需 `es.exe`」/「Everything 运行中即可用」写入用户文档**。
- **代码/事实**：用户文档 `docs/search.md:12` 明写「（IPC 主通道）**无需 `es.exe`**」，`:15` 写「仅要求 Everything **在运行**，不要求开启任何网络服务」。
- **差异**：不一致。红线条款与实际发布的用户文档**直接冲突**（同一文档体系内自相矛盾）。

### E-01 ｜ 数据结构 ｜ `-32002` 的归因错误，且与配套示例自相矛盾

- **位置**：`docs/extensions.md:341`（§5.4 注）、`docs/protocol.md:659`（§9.2 注）
- **文档**：称「命令不存在时，`dd-ext` **共享运行时**返回 `ShowToast { message: "未知" }`，故都不产出 `-32002`」。
- **代码**：
  - `crates/dd-ext/src/lib.rs` 的共享运行时**没有**默认分支；`"未知"` 文案只存在于 `lib.rs` 的**单测夹具**里。
  - 默认分支在**各扩展自身**的 handler：`builtins/system.rs:173`、`builtins/websearch.rs:298`、`builtins/shell.rs:180`（各带专属文案）、`dd-ext-sample/src/main.rs:528`（「未知命令」）。
  - **随本指南发布的 Python 示例 `examples/python-minimal/dd_ext_pymin.py:307-308` 确实回 `-32002 command_not_found`**。
- **差异**：不一致。归因层级错误（是各扩展 handler 而非共享运行时），且文档结论（"都不产出该码"）**与自己的配套示例相反**。

> 补充：此条同时是对**上一轮整理修正的再修正**——上一轮把「命令不存在回 `-32002`」整体改写成「实现都不产出该码」，这个结论下得过重。

---

## 3. 中严重度

| ID | 类型 | 位置 | 文档描述 | 代码实现 | 差异 |
|---|---|---|---|---|---|
| P-02 | 数据结构 | `protocol.md:142` | `jsonrpc` 缺失或非 `"2.0"` → `-32600` | `process.rs:596-601`：非 `"2.0"` 入 `unmatched` 忽略；缺失致反序列化失败 → `MalformedEnvelope` 冒泡**中止整个 `call`** | 均不回 `-32600` |
| P-03 | 数据结构 | `protocol.md:143,151` | `id` 限非负整数；类型非法 → `-32600` | `messages.rs:43`：`id: Option<u64>`；负/字符串 id 反序列化失败 → `MalformedEnvelope` 冒泡 | 不回 `-32600`；`id=0` 合法、上界 `u64::MAX` |
| P-04 | 边界条件 | `protocol.md:158` | 收到数组批量请求 → 回 `-32600` | host：反序列化失败冒泡（不回）；ext `serve_line`：回 **`-32700`** | 两侧都不回 `-32600` |
| P-05 | 悬空 | `protocol.md:684,231` | 扩展可在 `result.timeouts` 建议各阶段更宽超时，宿主可覆盖 | `messages.rs:111`：`Timeouts` 仅 `get_items_ms`；宿主**从不读取** `result.timeouts`，无覆盖逻辑 | 文档能力无实现 |
| P-06 | 业务逻辑 | `protocol.md` §7.1（约 :419-425） | 通知轮询返回"本次收到"的 `items_changed` | `ext_inprocess.rs:173-186`：`InProcessExtension::poll_notifications` 遍历全部通知且**从不清空**，每次轮询重复上报同一 `page_id`（子进程路径为一次性消费） | ✅ 已解决（2026-09-14，见 §10；原判「不一致」，可能反复重置 100 ms 合并窗口） |
| P-07 | 业务逻辑 | `protocol.md:253` | §5.3 规则 3：扩展不支持宿主主版本时回 `-32004` + `data.supported_versions` | `dd-ext/src/lib.rs:177-181`：`initialize` **完全不读** `params.protocol_version`，恒回 `"1.0"`；`VERSION_MISMATCH` 无任何产出点 | 规则 3 无实现 |
| P-08 | 业务逻辑 | `protocol.md:277` | §6.1：仅当 `provider.frozen == true` 才落盘缓存 | `crates/dd-gui/src/aggregator.rs:394,411-435`：落桩判定用**清单** `ext.manifest.frozen`；握手回的 `init.provider.frozen` **从未被读取** | 缓存门禁取错来源 |
| P-09 | 参数 | `protocol.md:353,557` | §8.4：`list_item` = 从嵌套列表页某一项触发 | `dd-gui/src/result.rs:107-118`、`app/invoke.rs:100-108`：`confirm_selected` 对所有 Invoke 项一律用 `TopLevel`；`ListItem` **仅出现在单测**，生产路径无产出 | 不一致 |
| P-10 | 业务逻辑 | `protocol.md:401-405` | §7.1：不可见页 → 标记脏，待可见时再拉；可见 → **立即** `get_items` | `app/refresh.rs:37-78`：非当前页的 `page_id` 直接 `Some(_) => {}` **丢弃**，无脏标记；命中页也进 100 ms 合并窗口 | 无脏标记机制，且非"立即" |
| P-11 | 参数 | `protocol.md:415-419` | §7.2：`state` ∈ `info/success/warning/error` | `app/host_actions.rs:39-50`：解析后**丢弃 `state`**，恒走 `ToastKind::Info` | 四取值类型支持、渲染忽略 |
| P-12 | 参数 | `protocol.md:419` | §7.2：`duration_ms = 0` 表示常驻直到被替换 | `app/toast.rs:20-24`、`app/mod.rs:637`：原样返回 0 → `expires = now`，**下一帧即清空** | 不一致 |
| M-01 | 参数 | `manifest-schema.md:49` | `icon` 为图标路径，按 §4 展开 | `dd-host/src/manifest.rs:40-41,385-391`：`Manifest.icon` **全程未经 `expand_path`**，且**无任何消费者**（只展开 `command`/`cwd`） | 字段实际悬空 |
| M-02 | 边界条件 | `manifest-schema.md:64-81` | §4 未提 Windows 自动补扩展名 | `manifest.rs:280-296`：先试原样路径（**同名无扩展名文件优先命中**），再依次 `.exe`/`.cmd`/`.bat` | 文档未覆盖补全机制与命中顺序 |
| E-02 | 边界条件 | `extensions.md:72-73` | §1 第 3 步 Python 片段只 `reconfigure` **stdout** | `examples/python-minimal/dd_ext_pymin.py:53-59`：示例**还** `sys.stdin.reconfigure(encoding="utf-8")` | 缺 stdin 编码 → 宿主发中文 query 时乱码甚至 `UnicodeDecodeError` |
| E-03 | 业务逻辑 | `extensions.md:136-137` | §1 第 4 步：连续 3 次 spawn 失败后熔断 | `crates/dd-gui/src/aggregator.rs:359-360`：spawn 失败只置 `SourceStatus::Failed`，**不进** `crash_guards`；`record_crash` 仅在运行期进程退出时调用（`health.rs:48`、`invoke.rs:73`、`page.rs:134`；`health.rs:97` 为定义处） | spawn 反复失败不计数、无 3 次熔断 |
| I-01 | 业务逻辑 | `implementation.md:142` | LRU 容量 N「建议 8，**可配置**」 | `app/pool.rs:13`：`LRU_WARM_CAPACITY = 8` 为 `const`，`Settings` **无对应字段** | "可配置"不成立 |
| D-01 | 接口 | `cmdpal-platform-agnostic-design.md:486` | A2 验证法：`--bench-startup` | `dd-run-cli/src/main.rs:73-90`：CLI 只有 `--list-extensions/--roundtrip/--conformance/--invoke/--ext-id/--extensions-dir` | 入口不存在 |
| D-02 | 参数 | `cmdpal-platform-agnostic-design.md:485` | 8,553,984 B（strip）/ 含符号 8,601,088 B | 实测 `dist/` 唯一产物 = **8,601,088 B**（`strip = "symbols"`） | 8,553,984 **无对应产物**；两档划分与实测不符 |
| R-01 | 业务逻辑 | `README.md:9,78,112`（中英同） | M7 仍标"进行中"；里程碑写"M0–M8"；下一步仍为"第三方扩展端到端验证" | 实际 M7–M9 全部 ✅ 关闭、v0.1.1 已发布 | README 未随关账更新 |
| R-02 | 业务逻辑 | `.github/workflows/release.yml:4,66` | 注释与 Release 正文称"5 内置扩展内嵌" | `crates/dd-gui/build.rs:20`：`EMBED_EXES = &[]` 恒空，M9 起 in-process | 发布文案误导 |
| S-02 | 参数 | `search-file.md:54`（另 `:407`、`search.md:46`） | 查询参数对 Everything **原样直透，不经任何编码** | `search.rs:548-552`：query 以 `-` 或 `/` 开头时**前置 `^` 转义**，避免被 `es.exe` 当开关 | 转义未在任何章节或用户文档提及 |
| S-03 | 数据结构 | `search-file.md:341,419,579` | 把 `parse_everything_date` 当**既有函数**并列为单测项 | 全仓**不存在**该函数；实际为 `filetime_to_unix`（`search.rs:322`）与 `combine_filetime`（`:332`） | 不存在的函数名被写成既有事实 |
| S-04 | 业务逻辑 | `search-file.md:63,687` | 200 ms 去抖常量 `PAGE_QUERY_DEBOUNCE` 在 `page.rs` 约 :115 | 常量定义在 `app/refresh.rs:13`；`page.rs:115` 只是调用点 | 数值对、位置错 |
| S-05 | 业务逻辑 | `search-file.md:716` + `plan-review.md:16` | client 用全局 **Weak** 缓存（Arc 全释放后自动重建） | `search.rs:443` 是 `Mutex<Option<Arc<EverythingClient>>>`（**Arc** 非 Weak）；Weak 在 `everything-ipc` crate 内部 | 不一致 |
| S-06 | 边界条件 | `search-file.md:768` | 1.5 实例名 `1.5a`、`with_instance`、pipe 通道备选 | `search.rs:475-480,457`：IPC 探活/查询仅 `IpcWindow::new()` / `EverythingClient::shared()`，**无任何实例名处理** | 以"要求"口吻叙述，未标"未实现" |
| S-07 | 参数 | `search.md:92` | 页内二次输入重拉「**不受前 30 条限制**」 | `search.rs:49,556,936`：每次 `get_items` 均 `take(RESULT_LIMIT = 30)` 且 `es -n 30` | 不一致，且与本文 `:91` 自相矛盾 |

---

## 4. 低严重度

| ID | 类型 | 位置 | 摘要 |
|---|---|---|---|
| P-13 | 文档未覆盖 | `protocol.md:85-87` | 非 UTF-8 行：文档未定义处置与错误码；代码 `framing.rs:20` 产出 `Frame::InvalidUtf8`（行产出点 `framing.rs:73`）→ `ProtocolError::InvalidUtf8`，`as_rpc_error` 返回 `None`（无错误码） |
| P-14 | 数据结构 | `protocol.md:145` | `params` 若出现必须是对象；代码 `messages.rs:47` `Option<Value>` 不校验，**数组亦被接受** |
| P-15 | 数据结构 | `protocol.md:146-147` | `result` 与 `error` 互斥；代码 `process.rs:221,617` **未强制**，并存时 `error` 优先 |
| P-16 | 参数 | `protocol.md:682` | `close` 超时 1000 ms、超时即强杀；代码 `process.rs:41-42` 为**两段各 1000 ms**（等 result + 等退出），最坏 2000 ms |
| P-17 | 业务逻辑 | `protocol.md:696` | 崩溃判定含「stdout EOF」；巡检路径 `app/health.rs:27` 仅以非 0 退出码计数，EOF 单独情形不触发 `record_crash`；但 invoke/get_items 失败路径在进程已退出（含 EOF/正常退出，`process.rs:442-443`）时同样调用（`invoke.rs:73`、`page.rs:134`） |
| P-18 | 悬空 | `protocol.md:38-58` | §1.3 方法名无常量层，三处（`dd-ext/lib.rs:176`、`process.rs:173`、`host_actions.rs:38`）按**字符串字面量**分发 → 无常量级核对基线 —— ✅ **已解决**（2026-09-14，见 §11） |
| P-19 | 业务逻辑 | `protocol.md:183-189` | §4 状态机：代码**无**对应状态枚举，状态隐含于进程生命周期与 inflight 集合；`busy` 的宿主侧载体为 `ext_busy` Toast（`invoke.rs:120,152`、`page.rs:232,265`）；运行期崩溃（有缓存）回退 **stub**（`pool.rs:68-79`），启动期聚合失败不产出 items（`crates/dd-gui/src/aggregator.rs:495`）而非"一律 stub" |
| P-20 | 参数 | `protocol.md:231` | `result.timeouts.*` 为对象可建议各阶段；代码 `messages.rs:110-114` 仅 `get_items_ms` 一个字段 |
| P-21 | 参数 | `protocol.md:305-308` | `get_items` 的 `page_id` 必填，缺失属 `-32602`；代码 `dd-ext/lib.rs:272-311` 落入 `_` 分支回 **`-32005`** |
| P-22 | 数据结构 | `protocol.md:334-338` | `get_command` 找不到时返回 `{"command":null}`；代码 `messages.rs:146-148` 带 `skip_serializing_if = is_none`，**线上为 `{"result":{}}`**（字段省略） |
| P-23 | 边界条件 | `protocol.md:477` | `more_commands` 可嵌套且未定义深度；代码 `model.rs:85` 自引用递归，**无深度上限防护**（深嵌套反序列化有栈风险） |

| M-03 | 接口 | `manifest-schema.md:30` | 把「内置最优先」归因于"§7 规则 7 与 `merge_builtins`"；`builtin.rs:178-189` 中规则 7 **只判 id 唯一性**，优先级实际来自 `merge_builtins` |
| M-04 | 边界条件 | `manifest-schema.md:22-30` | 未区分「目录不存在」与「无权限/IO 错误」；`manifest.rs:498-516`：`NotFound` 静默视为空目录、其余记 `dir_error`，读取失败的条目被静默丢弃 |
| M-05 | 存疑 | `manifest-schema.md:44` | `version` 为 semver；`manifest.rs:299-308` `parse_semver` 只收三段纯数字，**拒绝 prerelease/build**（`1.0.0-beta` 判非法） |
| M-06 | 边界条件 | `manifest-schema.md:68-72` | 相对路径＝不以 `/`/`~`/`${` 开头且非盘符；`manifest.rs:265-276` **额外**把 `\` 开头（含 UNC）与盘符相对式 `C:x` 判为非相对 |
| M-07 | 边界条件 | `manifest-schema.md:68-72` | `~` 展开为 home；`manifest.rs:251-256` 仅识别 `~`/`~/`/`~\`，**`~user` 按相对路径**拼到清单目录 |
| M-08 | 业务逻辑 | `manifest-schema.md:107-117` | §7 校验按 1→9 顺序短路；实际为 **1,2,3,4,5,6,9,8**（capabilities 先于 command 存在性），规则 7 延后到 `scan_dir` 全部 load 完 |
| M-09 | 数据结构 | `manifest-schema.md:111` | 规则 3 含 `schema_version`，缺失记 `missing_field`；`manifest.rs:318-331` 其缺失在**规则 2 即短路**为 `UnsupportedSchema`，**永不产生 `MissingField`** |
| M-10 | 存疑 | `manifest-schema.md:112,114` | 规则 4 / 6 分属 `version` / `min_host_version`；`manifest.rs:344-345,363-368` 后者**复用 `InvalidVersion`**（一变体双用） |
| M-11 | 存疑 | `manifest-schema.md:113` | 规则 5「静默跳过（非错误）」；仍写入 `skipped`（`is_error=false`），CLI 以「-」打印 |
| M-12 | 存疑 | `manifest-schema.md:115` | 规则 7「已加载集合」范围未界定；`seen` 仅限单次 `scan_dir`，**跨目录同 id 由 merge 静默丢弃** |
| M-13 | 存疑 | `manifest-schema.md:116,119` | 规则 8 标题写"存在且可执行"、脚注写"只查存在性"；代码仅 `is_file()`，**不查可执行位** |
| M-14 | 存疑 | `manifest-schema.md:129` | `frozen` 清单值为预期、实际以扩展响应为准；内置注册写入 `host_frozen()`（含兜底者 `false`），与 calc/websearch/shell 自述 `frozen=true` **相反**（宿主主动降级） |
| M-15 | 数据结构 | `manifest-schema.md:147` | 内置"不通过清单文件注册"；`builtin.rs:5-13` 与 `crates/dd-gui/src/aggregator.rs:8-9` 的**代码注释仍称"内置同样走清单注册"**（注释陈旧） |
| M-16 | 存疑 | `manifest-schema.md:154` | file-search 为"唯一"清单注册型官方扩展；同目录另有 `examples/extensions.d/com.example.sample.json` |
| E-04 | 业务逻辑 | `extensions.md:120` | §1 第 4 步称 §4「只覆盖 `command`/`cwd`/`icon`」；`icon` 实际未落地（同 M-01） |
| E-05 | 参数 | `extensions.md:247` | §4 称响应「四个字段都是必填」指代不明（表内 4 行且漏 `provider.id`/`display_name`） |
| E-06 | 存疑 | `extensions.md:258` | §4"不认识宿主主版本回 `-32004`"；共享运行时 `initialize` 不读 `params`，无条件回 `"1.0"`（同 P-07） |
| E-07 | 存疑 | `extensions.md:282` | §5.2「`frozen: true` 时**必须**实现 `get_command`」；运行时**无条件提供**（按 `top_level` 查 id），与 `frozen` 无关 |
| E-08 | 接口 | `extensions.md:391-407` | §7 自检表 13 项与 CLI 13 个输出标签一致；但文档未注明 in-process 分支的标签是 `1) open`（`main.rs:646`）而非子进程路径的 `1) spawn`（`main.rs:689`） |
| E-10 | 文档未覆盖 | `extensions.md:450-456` | §8 超时预算表**漏列 `get_command`**（实际 5000 ms）；其余数值与 `process.rs:30-42` 一致 |
| I-02 | 文档未覆盖 | `implementation.md:34` | 称"dd-gui 在 M9 前已更名 dd-run"；crate 仍名 `dd-gui`，只是 `[[bin]] name = "dd-run"`（同文件 `:380`、`README:64` 同） |
| I-03 | 参数 | `implementation.md:197` | M5 4.6 记"650×420 基准"；`app/mod.rs:61` 为 `APP_H = 440`（注释：420 已废，2026-09-06 +20） |
| I-04 | 文档未覆盖 | `implementation.md:389` | R2 行含**未替换的占位符**「`<agg>` ~2ms 达标」（疑整理残留） |
| I-05 | 存疑 | `implementation.md:315,389` | A2 ~2 ms / A3 3.7 ms；代码仅有计时与延迟测试（A3 断言 < 100 ms），**无这两个数值的常量或基准工具** |
| I-06 | 存疑 | `implementation.md:323` | A11 覆盖率 100%；`app/keys.rs:25-39` 有键盘路径，**无覆盖率度量** |
| D-03 | 业务逻辑 | `cmdpal-platform-agnostic-design.md:316,356` | Windows 枚举"开始菜单 `.lnk` + PATH"；代码用 `shell:AppsFolder`（`FOLDERID_AppsFolder`）本体 + `.lnk` 兜底 |
| R-03 | 文档未覆盖 | `implementation.md:215` | 仅记"全局热键"，未描述冲突/注册失败；`hotkey.rs:154,186-198` 启动失败降级无热键、重注册失败回滚旧键并 Toast |
| S-08 | 参数 | `search-file.md:52` | 探活/搜索统一 1200 ms；代码常量确为 1200 ms（`search.rs:53`），但**模块头注释 `search.rs:24` 仍写"探测 800ms、查询 1.2s"**（源码注释残留旧值） |
| S-09 | 数据结构 | `search-file.md:918` | 称「`decode_output()` 实际为 `decode_with_codepage()`」；两函数**并存**，`search.rs:230` 的 `decode_output` 调用 `:245` 的 `decode_with_codepage`（表述误导） |
| S-10 | 参数 | `search-file.md:29` | §2.1 把 es 参数写为 `-json -size -dm -attributes <q>`（**漏 `-n`**）；实际为 `-json -n <limit> -size -dm -attributes <q>`（同文档 `:318` 写对） |
| S-11 | 悬空 | `search-file.md:67,79` | 标题称「A-33-05…A-33-10 唯一未闭环」，表内**只有 05/06/07/08/10**（`:81` 说明 09 已并入 CI） |
| S-12 | 接口 | `search-file.md:193,234` | capabilities = `[host/open_url, host/show_status]`（两项）；实际**三项**，含 `host/set_clipboard` |
| S-13 | 业务逻辑 | `search-file.md:546,413` | `host/open_url` 经 `webbrowser::open` 或 `cmd /c start` 兜底；实际 `ShellExecuteW(verb="open")`（`platform.rs:589`），与同文 `:63`/`:771`（另 `:11`、`:869`）自相矛盾 |
| S-14 | 边界条件 | `search.md:82` | 把「结果为空」归因于「索引尚未建完」；索引未就绪实际**返回引导项**，并非空结果 |
| S-15 | 文档未覆盖 | `search-file.md`（无对应段落） | 查询含**双引号**或**超长**时的行为未提及；`search.rs:556` 对 `"` 未转义、查询长度**无上限**，直接透传 |
| S-16 | 悬空 | `search-file-plan-review.md:121` | reveal 走 `raw_arg`、位置「约 :783-814」；实际 `spawn_reveal` 在 `search.rs:966-996`，`:783-814` 是 `guide_item` |

---

## 5. 存疑项汇总（无法从代码唯一判定，需人工确认）

| ID | 位置 | 存疑内容 |
|---|---|---|
| P-15 | `protocol.md:146-147` | `result`/`error` 并存时"是否违规"无法判定（代码未强制、未报错） |
| P-17 | `protocol.md:696` | 「stdout EOF」是否应单独计为崩溃——文档与实现口径不同，取决于设计意图 |
| P-23 | `protocol.md:477` | `more_commands` 嵌套深度上限是"文档漏写"还是"有意不限制" |
| M-05 | `manifest-schema.md:44` | `1.0.0-beta` 该不该算合法 semver（文档简化定义 vs 标准 semver） |
| M-10 | `manifest-schema.md:112,114` | `InvalidVersion` 一变体双用是"有意复用"还是"漏了变体" |
| M-11 | `manifest-schema.md:113` | 规则 5 的"静默"是指不报错、还是指不记录 |
| M-12 | `manifest-schema.md:115` | 规则 7 的"已加载集合"是否应跨目录（含 sidecar） |
| M-13 | `manifest-schema.md:116,119` | 规则 8 到底该不该查可执行位（标题与脚注要求不一致） |
| M-14 | `manifest-schema.md:129` | 内置 `frozen` 被宿主降级为 `false`（含兜底者），是否有意为之 |
| M-16 | `manifest-schema.md:154` | `com.example.sample.json` 是否计入"官方扩展" |
| E-05 | `extensions.md:247` | 「四个必填字段」究竟指哪四个 |
| E-06 | `extensions.md:258` | 版本协商是否要求"用运行时者必须自行实现" |
| E-07 | `extensions.md:282` | `get_command` 是"运行时义务"还是"扩展义务" |

| I-05 | `implementation.md:315,389` | A2 的 ~2 ms、A3 的 3.7 ms 是否为真机记录值（代码无法证实） |
| I-06 | `implementation.md:323` | A11「100% 键盘可达」的度量口径 |
| S-06 | `search-file.md:768` | 1.5 实例名/pipe 通道是"已定要求"还是"未实现设想" |
| D-02 | `cmdpal-platform-agnostic-design.md:485` | 8,553,984 与 8,601,088 的两档划分依据（实测只存在后者） |
| V3-看 | `manifest-schema.md:20-33` | CLI `--list-extensions` 是否应覆盖 sidecar 目录（文档描述的是宿主行为） |
| P-01 | `protocol.md:96` | 超限后"关闭连接"是否真需实现（宿主 `call` 与 `poll` 两条路径都要改） |

---

## 6. 代码有实现、文档未覆盖（文档侧缺口）

| ID | 代码位置 | 未入文档的行为 |
|---|---|---|
| P-13 | `framing.rs:19`、`process.rs:578` | 非 UTF-8 帧的专门错误类型 `InvalidUtf8`（且无对应 JSON-RPC 错误码） |
| M-02 | `manifest.rs:280-296` | Windows 可执行文件扩展名补全顺序（`.exe` → `.cmd` → `.bat`）与"同名无扩展名文件优先命中"的坑 |
| M-04 | `manifest.rs:498-516` | 目录 `NotFound` 与 IO 错误的差异化处置 |
| E-10 | `process.rs:30-42` | `get_command` 一档超时（5000 ms） |
| R-03 | `hotkey.rs:154,186-198` | 热键注册失败的降级（无热键启动）与重注册失败回滚 |
| S-02 | `search.rs:548-552` | 以 `-`/`/` 开头的查询会被前置 `^` 转义 |
| S-15 | `search.rs:556` | 查询中的 `"` 不转义、查询长度无上限 |
| S-17 | `search.rs:230,245` | `decode_output` / `decode_with_codepage` 两层函数的分工 |

---

## 7. 已确认一致（抽样，供对照）

以下为核对通过、无需处理的关键契约（节选）：

- `protocol.md` §1.3 的 10 请求 + 2 通知与代码分发一一对应；§8「8 种 Kind」属实；§8 数据模型字段名/可选性全部吻合；§9.2 标准 5 码与 `-32001`/`-32003`/`-32005` 的数值及抛出场景吻合；1 MiB 上限、串行化、熔断 N=3、100 ms 合并窗口均一致。
- `manifest-schema.md` §3 的 18 字段（除 `icon` 展开外）、§7 九条规则与 `SkipReason` **名义上一一对应**、§2/§10 的优先级与去重、`EMBED_EXES = &[]` / `materialize() → None` / in-process `serve_line` 均一致。
- `extensions.md` §7 自检 13 项中 11 项与 CLI 实际输出标签一致；§8 超时数值、§9 崩溃处置、§2 三条铁律、§3 三个陷阱的处置方式均与实现吻合。
- `implementation.md` 的 `dist/` 产物清单与字节数（8,601,088 B 等）、6 个 crate 版本号、A1–A12 编号连续性均一致。
- `search-file.md` 的依赖清单（`everything-ipc = "=0.1.4"` + `default-features = false`、`fuzzy-matcher`、`chrono`、`windows-sys 0.61`）、`ES_TIMEOUT = 1200 ms`、探活条件 `is_ipc_available() + is_db_loaded()`、`f ` 前缀进页与 200 ms 去抖、`resolve_file_url_to_path` 往返均一致；历史归档两份文档的最终处置结论亦与代码现状一致。

---

## 8. 复核入口

```bash
# 文档侧结构体检（体积/最长行/标题/表格/代码块 + 跨文件链接可达性）
python tools/docscan.py

# 协议契约一致性（从 protocol.md 抽取全部 JSON 示例做反序列化断言）
cargo +stable-x86_64-pc-windows-gnu test -p dd-protocol
```

---

## 9. 修复记录（2026-09-13，v1.1 修复阶段）

复核验真后按「**文档对齐代码**」落实修复；规范类文档（`protocol.md`）的未实现契约**保留条文**、以「⚠️ 实现现状」注记标注实际行为，沿用该文档既有的注记风格。三处**代码**改动均为纯注释修正，无行为变更。

| 区域 | 落点 | 处置要点 |
|---|---|---|
| `protocol.md`（P 系 18 处） | §2.2/§2.3/§3.2–3.4/§4/§5.1/§5.3/§6.1/§6.3/§6.4/§7.1/§7.2/§8.1/§8.4/§9.2/§9.3/§10/§11 | P-01 超限处置、P-02/03/04 `-32600` 缺口、P-05/P-20 `timeouts`、P-06/P-10 通知处置、P-07 版本协商、P-08 frozen 门禁来源、P-09 sender 产出、P-11/P-12 Toast 参数、P-13 非 UTF-8 帧、P-14/P-15/P-21/P-22/P-23 边界、P-16 close 两段超时、P-17 崩溃口径、P-19 状态机现状；E-01 `-32002` 归因更正（各扩展 handler 兜底 + Python 示例确实回该码） |
| `manifest-schema.md`（M 系 14 处） | §2/§3/§4/§7/§8/§10 | M-01 icon 无消费者、M-02 Windows 补扩展名顺序、M-03 内置优先归因、M-04 目录异常、M-05 semver 简化、M-06/M-07 路径判定边界、M-08 实际短路顺序 1-2-3-4-5-6-9-8、M-09 schema_version 短路、M-10 一变体双用、M-11/M-12/M-13 规则 5/7/8 口径、M-14 内置 frozen 降级、M-16 sample 清单 |
| `extensions.md`（E 系 9 处） | §1/§3/§4/§5.2/§5.4/§7/§8 | E-02 示例补 `stdin.reconfigure`、E-03 spawn 无熔断、E-04 icon、E-05 必填字段指代、E-06 版本协商现状、E-07 `get_command` 与 frozen 无关、E-01 归因、E-08 in-process 标签 `1) open`、E-10 超时表补 `get_command` 5000ms |
| `implementation.md`（I 系 5 处） | §3/§5/§6 | I-01 LRU 常量、I-02 crate 更名表述、I-03 650×440、I-04 `<agg>` 占位符、I-05/I-06 记录口径 |
| 设计文档 / README / release.yml（D/R 系 8 处） | 仓库根设计文档、双语 README、`.github/workflows/release.yml` | D-01 `--bench-startup` 未实现、D-02 体积实测值更正、D-03 `shell:AppsFolder` 枚举口径、R-01 M7–M9 关账与下一步更新、R-02 「内嵌」改「in-process」 |
| `search-file.md` / `search.md` / `search-file-plan-review.md`（S 系 17 处） | §2/§4/§5/§6/§7 等 | **S-01（高）红线冲突：移除 `search.md` 的「无需 es.exe」/「仅要求 Everything 在运行」承诺**，恢复与红线条款一致；S-02 `^` 转义、S-03 幽灵函数名更正、S-04 常量位置、S-05 Arc 非 Weak、S-06 实例名未实现、S-07 30 条口径、S-09 两函数并存、S-10 `-n` 参数、S-11 标题与表对齐、S-12 三能力、S-13 `ShellExecuteW` 口径、S-14 空结果归因、S-15 双引号/超长补录、S-16 reveal 行号 |
| 代码注释（3 处，纯注释） | `dd-ext/src/bin/search.rs:24,26`、`dd-host/src/builtin.rs:5-13`、`dd-gui/src/aggregator.rs:8-9` | S-08 超时注释残留 800ms 更正 + 查询直透补转义例外；M-15 「内置同样走清单注册」陈旧注释更正为「不通过清单文件注册、内存注册等效实现」 |

**未修 / 留人工确认**：P-01（是否实现「回错 + 关连接」）、~~P-18（方法名常量层属代码重构，已登记 INDEX.md §5 未闭环项）~~ ✅ 已于 2026-09-14 由代码侧闭环（见 §11）、§5 存疑项中的设计决策（M-12/M-13/M-14/M-16/E-06/E-07 等已加现状注记，最终口径待人工定夺）。

---

## 10. 后续状态变更（2026-09-14）

v1.2：记录 09-13 复核之后、由**代码侧改动**（而非「文档对齐代码」）消除的差异。

| ID | 变化 | 依据 |
|---|---|---|
| P-06 | ✅ **已解决** | in-process 适配器 `poll_notifications` 改为**消费式取走**（`std::mem::take`，取走即清空）——同一 `items_changed` 只上报一次，与子进程路径的一次性消费语义一致；`unmatched` 同时改为有界（`DIAGNOSTIC_BUS_CAP = 64`，与子进程路径同口径）。[`protocol.md`](./protocol.md) §7.1「实现现状」注记同步更正。 |

**连带变更**：本文 §3 的 P-06 行标注 ✅；§6 的 P-06 行移除（差异已消失，不再属「文档侧缺口」）。

**备注（数值）**：§7 抽样条目提及的「`dist/` 字节数 8,601,088 B」为 2026-09-12 快照，已被 2026-09-14 实测值 **8,623,616 B** 取代（见 [`optimization-plan.md`](./optimization-plan.md) §2.1）。`implementation.md` 同值处标注为「2026-09-12 实测」，属历史记录、本批不改。

**生效前提**：上述代码改动位于**工作副本**，随对应代码批提交后生效。

---

## 11. 后续状态变更（2026-09-14，O2 方法名常量层）

v1.2（续）：记录又一处由**代码侧重构**（而非「文档对齐代码」）消除的差异。触发项 = [`optimization-plan.md`](./optimization-plan.md) Phase 1 首项 **O2**。

| ID | 变化 | 依据 |
|---|---|---|
| P-18 | ✅ **已解决** | 新增 `crates/dd-protocol/src/methods.rs` 常量层——协议 §1.3 全部 **12 个方法**（7 host→ext 请求 + 3 ext→host 请求 + 2 通知）各有常量，另提供 `HOST_METHODS`（清单 `capabilities` 白名单，`dd-host::manifest::HOST_CAPABILITIES` 改为其别名）、`HOST_METHOD_PREFIX`（§3.3 对端请求判别）、`ALL_METHODS`（比对基线）；宿主/扩展/CLI 的**生产代码**统一改为常量引用（逐文件清单与豁免说明见 [`optimization-plan.md`](./optimization-plan.md) §2.3.1）。P-18 指出的「无常量级核对基线」根因随之消失。 |

**新增核对基线**：`crates/dd-protocol/tests/consistency.rs::method_constants_match_protocol_method_table` —— 测试期从 `docs/protocol.md` §1.3 两张表抽取方法名，与 `ALL_METHODS` **逐项同序**断言相等；文档增删方法或常量拼错即测试失败。P-18 的「无常量级核对基线」由此转为**机器可验**。

**协议影响**：无。方法名取值、方向、章节均未变；`protocol.md` 仅在 §1.3 追加「实现侧单一来源」注记（不含语义变更），v1.0 冻结契约不变。

**验证**：`cargo fmt --all -- --check` 无差异；`cargo clippy --workspace --all-targets` 0 warning；`cargo test --workspace` **403 passed / 0 failed**（基线 402，+1）。

**连带变更**：本文 §4 的 P-18 行已标注 ✅；§9「未修 / 留人工确认」中移除 P-18；[`INDEX.md`](./INDEX.md) §5 对应行改 ✅；[`protocol.md`](./protocol.md) §1.3 加实现侧注记；[`implementation.md`](./implementation.md) 里程碑总表与 §2 增记本批（**详述处为 `optimization-plan.md` §2.3.1**，本文与 implementation 只记结论与落点）。

---

> **本阶段约束复述**：v1.0 为纯只读核对记录；v1.1 完成三路复核验真（修正清单自身 9 处）并落实上述修复，**已登记进 [`INDEX.md`](./INDEX.md)**（§3.E 清单与 §5 未闭环项）；v1.2 追加 §10/§11，记录代码侧消除的差异（P-06、P-18）与过期数值的取代。
