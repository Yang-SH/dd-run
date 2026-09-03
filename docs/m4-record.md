# M4 实施记录 — 内置扩展与健壮性

> **状态**：🟨 进行中（2026-09-02 启动）。**P1–P3 健壮性基础层代码完成（79/79 测试全绿），
> 待用户真机复验 §4 清单**；**P4 扩展侧 + 宿主 fallback 轮均完成**（dd-ext 共享运行时 +
> 5 内置扩展 + 宿主内存自注册 + 协议方法封装 + `FallbackStore` 无匹配渲染，146/146 全
> workspace 测试全绿）；P5（A3 nucleo 过滤）为后续轮次。
> **目标**（implementation.md §M4）：MVP 内置 5 个扩展（Apps / Calc / System / WebSearch / Shell），
> 扩展崩溃不影响宿主；连续崩溃受保护；能力注入（`host/*`）接 UI；过滤性能达标。
> **验收映射**：A8（扩展崩溃后宿主不退出、可恢复）、A10（内置扩展功能清单核对）、
> A3（输入过滤 < 16ms/帧，**实测记录、不调目标**）。

---

## 0. 实施决策（2026-09-02 用户确认，grill-me 式一问一答）

| # | 决策点 | 选项 | 结论 |
|---|---|---|---|
| D1 | 5 个内置扩展的代码组织 | A) 共享扩展运行时（1 个 `dd-ext` lib 封装协议样板 + 1 个多命令产物）｜B) 每扩展独立 crate（各自手写样板）｜C) 宿主内置非子进程 | **A 共享扩展运行时**：workspace 新增扩展侧共享 lib（帧解析/信封/分发/状态机只写一份），5 个扩展各自注册命令集合，产物仍是 1 进程/扩展（ADR-1 子进程隔离不变） |
| D2 | 本轮推进范围 | A) 只做健壮性基础层 ｜ B) 基础层 + Calc/WebSearch ｜ C) 只出文档 | **A 只做健壮性基础层**：崩溃恢复链（A8）+ 连续崩溃保护（协议 §11）+ `host/*` 执行端接 UI；5 个扩展与模糊过滤后续轮次做 |
| D3 | A3 过滤引擎 | A) nucleo ｜ B) nucleo-matcher ｜ C) 暂不引入（保留 contains） | **A nucleo**（纯 Rust 模糊匹配，打分排序 + 拼音支持；A3 实测不达标再议）——**本轮不实现，记入后续阶段** |

> **决策依据**：
> - D1-A：`dd-ext-sample` 单文件 ~500 行已手写完整 JSON-RPC 服务端（decode 循环 + 分发 + 信封），
>   5 个真实扩展照抄将产生 ~2500 行重复样板；抽共享运行时一次解决。
> - D2-A：M4 工作量大（6 块任务），基础层是所有扩展与 A8 的前提，先把它闭环到可单测、
>   可验证的状态，扩展按 Calc/WebSearch（无 OS 依赖）→ Apps/System/Shell（平台相关）的顺序后续落地。
> - D3-A：A3 过滤与健壮性正交，不阻塞本轮；选 nucleo 因其为纯 Rust、算法强（subsequence+打分）、
>   被 `helix`/`fuzzy` 类工具验证过，且不引入 C 依赖。

### 0.1 P4 轮补充决策（2026-09-02 用户一问一答确认）

| # | 决策点 | 选项 | 结论 |
|---|---|---|---|
| D4 | P4 推进范围 | A) 扩展侧先行：本轮实现 `dd-ext` lib + 5 扩展（Calc/WebSearch/Shell 写完整 fallback handler，roundtrip/单测验证）｜B) 宿主 UI fallback 链路也做｜C) 只出文档 | **A 扩展侧先行**：宿主 UI 无匹配渲染 + fresh/frozen 接线单列后续轮；A10 真机核对拆两批（Apps/System 本轮可验，Calc/WebSearch/Shell 待宿主 fallback 轮） |
| D5 | 5 内置扩展的发现机制 | A) 宿主自注册（内存构造 LoadedExtension，与第三方扫描并存）｜B) 写磁盘清单（安装器语义） | **A 宿主自注册**：`dd-host/builtin.rs` 注册表（id/name/frozen/capabilities 与扩展 spec 对齐）+ `ensure_builtins`；manifest-schema §10 的"清单由安装器写入"在 MVP 无安装器阶段用内存注册等效替代 |
| D6 | 平台相关扩展代码策略 | A) Windows 优先：Windows 路径本轮实现真机验证｜B) 三平台全做（无法验证） | **A Windows 优先**：macOS/Linux `#[cfg(not(windows))]` 编译恒成立占位 + TODO |
| D7 | 宿主扫描兜底 sample 的去留 | A) 内置扩展取代 dd-ext-sample 兜底｜B) 保留并存｜C) 暂不接 GUI | **A 内置取代 sample**：`load_extension_sources` 改为内置常驻 + 磁盘清单合并；P1–P3 崩溃注入复验改为 kill 任意内置扩展进程（A8 路径等价） |

> **决策依据**：
> - D4-A：P1–P3 遗留的 UI fallback 渲染是一整块 UI 工作，与扩展侧（协议样板 + 命令实现）正交；
>   先让 5 个扩展自身闭环（roundtrip 可验），宿主 UI 接线后续轮一次做完。
> - D5-A：内置扩展与第三方在宿主侧**无特判代码**（同为 `LoadedExtension`，走同一 spawn/缓存/崩溃
>   链路），只是来源不同（内存构造 vs 磁盘扫描）；roundtrip 测试可直接构造，无需真实清单文件。
> - D6-A：本机仅 Windows 可验证（`.lnk` / 开始菜单 / `shutdown.exe` 等），macOS/Linux 分支无法
>   编译期验证功能正确性，避免"看着合理但不可验证"的三平台分支。
> - D7-A：MVP 的 5 个内置扩展取代 dev 示例成为常态源；`dd-ext-sample` 仍保留（roundtrip/协议
>   一致性测试的 fixture），只是不再作为 GUI 启动兜底。

### 0.2 宿主 fallback 轮补充决策（2026-09-03 用户一问一答确认）

| # | 决策点 | 选项 | 结论 |
|---|---|---|---|
| D8 | 含 fallback 能力扩展的 frozen 语义修正 | A) 按设计文档 §6.3 视为 fresh（BuiltinSpec 加 has_fallback 字段 + host_frozen 派生；aggregator load_one 握手后 has_fallback=true 不落桩并清历史桩）｜B) 保留 frozen、无匹配时复热 spawn｜C) 只对 warm 扩展生效 | **A 按设计改 fresh**：`BuiltinSpec.frozen` 保留扩展自述（与 spec() 防漂移对齐），新增 `has_fallback` 字段与 `host_frozen() = frozen && !has_fallback` 派生（宿主落桩策略）；load_one 落桩条件加 `&& !provider.has_fallback` 并清除历史桩——Calc/WebSearch/Shell 冷启动即 spawn 保活，fallback 立即可用 |
| D9 | fallback 触发时机 | A) 全局无匹配才触发（协议字面）｜B) 单扩展各自判定 | **A 全局无匹配**：仅当 Root 页过滤结果为空且查询非空时拉取/展示兜底（`has_regular_match()` 判定） |
| D10 | 模板拉取策略 | A) 每扩展只拉一次并缓存（本地 {query} 替换）｜B) 每次输入重拉 | **A 拉一次缓存**：FallbackStore 状态机 Unknown→Fetching→Ready/Exhausted，`wants()` 去重；渲染时 `render(query)` 本地替换 `{query}` |

> **决策依据**：
> - D8-A：P4 扩展侧把 Calc/WebSearch/Shell 标 `frozen=true` 与设计文档 §6.3 冲突——含兜底能力者
>   若落桩（冷启动读桩、无进程）则永远拉不到 `fallback_commands`，A10 批次 2 无法核对。拆两概念：
>   `frozen`（扩展自述，可缓存）≠ `host_frozen`（宿主是否落桩）；后者按 §6.3 收窄为
>   `frozen && !has_fallback`。第三方磁盘清单同样在握手后按其 `provider.has_fallback` 处理
>   （T8 接线点）。
> - D9-A：协议 §6.2 原文"搜索无匹配时的兜底命令"即全局判定；实现最简、RPC 最少，
>   且覆盖全部内置用例（输入 `1+1` 时常规列表无匹配 → calc 兜底出现）。
> - D10-A：模板含 `{query}` 占位符、与具体输入无关——拉一次缓存、每扩展至多 1 次 RPC；
>   空结果/失败标记 Exhausted 本会话不重试（避免对坏扩展反复 RPC）。

---

## 1. 分阶段实施计划与进度

> 阶段总览（P1–P5）。**P1–P3 为本轮（健壮性基础层）**，P4/P5 为后续轮次。

| 阶段 | 内容 | 状态 | 验收标准（量化） | 结果 |
|---|---|---|---|---|
| P1 崩溃恢复链（A8） | 进程退出（stdout EOF / 非 0 退出码）→ in-flight 请求立即失败 → 命令回落 stub → 宿主继续运行；`refresh_health` 改为**每帧**检测（logic() 入口，此前仅 show() 时查一次） | ✅ 代码完成（2026-09-02） | 编译 + 全 workspace 测试全绿；死进程不再被归还保活集（poll 错误分支按 `exit_status` 判别）；真机 §4 #1–#3 | ✅ 79/79 全绿 |
| P2 `host/*` 执行端 | dd-gui 消费 `host_requests`：`host/show_status` → 既有 Toast；`host/set_clipboard` → 剪贴板（arboard）；`host/open_url` → 默认浏览器（webbrowser）；空闲 rx 轮询同样应答 HostRequest（此前静默丢弃） | ✅ 代码完成（2026-09-02） | roundtrip 新增 1 测（3 个 host 请求被应答+记录，参数完整）；真机 §4 #6–#8 | ✅ roundtrip 8/8 |
| P3 连续崩溃保护 | 协议 §11 规则 2：连续崩溃 N 次（`CrashGuard`，默认 3）→ 熔断"暂时不可用"；dispatch 拦截不再 spawn；warm 恢复清零 | ✅ 代码完成（2026-09-02） | `robustness.rs` 5 个状态机单测（计数/熔断/复位/恢复不误熔）；真机 §4 #4–#5 | ✅ dd-gui 29/29 |
| P4 共享扩展运行时 + 5 内置扩展 | `dd-ext` lib（D1-A）+ Apps/Calc/System/WebSearch/Shell 命令实现；frozen 标记 + 自动安装/发现 | ✅ 扩展侧（2026-09-02）+ 宿主 fallback 轮（2026-09-03）完成 | 扩展侧：`roundtrip_builtins` 6 项 + 全 workspace 132/132；宿主 fallback 轮：FallbackStore 纯逻辑 7 单测 + PanelState 分流 6 单测 + cache remove 1 + main.rs 接线；**146/146 全 workspace 全绿**；A10 拆两批真机核对 | ✅ 146/146 全绿 |
| P5 A3 模糊过滤 | `state.rs` 过滤换 nucleo（打分排序替代 contains）；帧耗时采样埋点 | ⬜ 后续 | 实测过滤 < 16ms/帧（不达标记录实测与瓶颈、不调目标） | ⬜ |

---

## 2. 完成判据对照（M4 全量）

| 判据 | 状态 | 证据 |
|---|---|---|
| A8：故障注入（kill 子进程）后宿主不退出、可恢复 | ✅ 代码完成，真机待验 | roundtrip `stdout_eof_is_reported_as_process_exited`（§11 EOF → `-32003`）+ `refresh_health` 每帧检测 + poll 错误分支丢弃死进程回落 stub；真机 §4 #1–#3 待用户按 17:4x 版 exe 复验 |
| 协议 §11：连续崩溃 N 次后"暂时不可用"，重启/手动重试才恢复 | ✅ 代码完成，真机待验 | `robustness.rs` 5 单测（计数/熔断/复位/恢复不误熔）+ dispatch 熔断拦截；真机 §4 #4–#5 待用户复验 |
| `host/*`：show_status / set_clipboard / open_url 真实副作用 | ✅ 代码完成，真机待验 | roundtrip `roundtrip_m4_host_requests_are_answered_and_recorded`（3 个请求应答+记录、参数完整）；dd-gui 执行端（Toast/arboard 剪贴板/webbrowser）；真机 §4 #6–#8 待用户复验 |
| A10：5 个内置扩展功能清单核对通过 | 🟨 代码完成，真机待验 | `dd-ext` 5 bin + `roundtrip_builtins` 6 项全链路往返；宿主 fallback 轮完成（无匹配渲染 + `{query}` 替换 + `context.query` 透传）；A10 清单核对拆两批：Apps/System 与 Shell 顶层本轮可真机验（枚举真实应用/锁屏/开终端），Calc/WebSearch/Shell 的 fallback 交互（输入 `1+1`→`= 2` 等）也需真机核对（§4 #9–#13） |
| A3：输入过滤 < 16ms/帧（实测记录，不调目标） | ⬜ P5 后 | P5 帧耗时采样日志 + 记录实测值 |

---

## 3. 现状盘点（2026-09-02 启动时核对）

以下为 M4 启动时对既有代码的核对结论（与 P1–P5 的"新建 vs 补齐"边界）：

| 现状 | 位置 | 对 M4 的含义 |
|---|---|---|
| `host_requests` 仅**记录并回 `{}`**（注释自认"真实副作用属 M4"） | `dd-host/src/process.rs` L200 / L485 `answer_host_request` | P2 需在 dd-gui 侧消费 `host_requests` 并执行真实副作用；**应答已由 host 层完成，无需重复** |
| `has_exited()`（非阻塞 try_wait）+ `refresh_health()` 移除已退出进程 | `process.rs` L345 / `main.rs` L694 | 崩溃检测骨架已有，但 in-flight 请求失败处理、回落 stub 的完整链未闭环 → P1 |
| 崩溃恢复注释"属 M4（A8）" | `main.rs` L708 | 确认 P1 是计划内增量，非回归 |
| 过滤为 `contains` 子串匹配，无打分排序 | `dd-gui/src/state.rs` L195 | P5 换 nucleo |
| 扩展侧样板（decode 循环/分发/信封）手写在 `dd-ext-sample` | `crates/dd-ext-sample/src/main.rs` | P4 抽 `dd-ext` lib 复用 |
| stderr 捕获（崩溃诊断，A8 可观测性）已有 | `process.rs` L548 | 直接复用 |

## 3.4 P1–P3 实施记录（2026-09-02）

### 文件改动

| 文件 | 改动 |
|---|---|
| `crates/dd-gui/src/robustness.rs`（新建） | `CrashGuard` 状态机（协议 §11）：`MAX_CONSECUTIVE_CRASHES=3`、`record_crash`（第 N 次返回"新触发熔断"）/`reset`/`is_tripped`；5 单测（计数/熔断/熔断后 noop/复位/崩一次恢复不误熔） |
| `crates/dd-gui/src/lib.rs` | 注册 `robustness` 模块 |
| `crates/dd-host/src/process.rs` | 新增 `exit_status()`（区分崩溃=非 0 退出码 / 正常退出=0，§11）；`poll_notifications` 消费 rx 时对 `HostRequest` 应答+记录（此前空闲到达被静默丢弃）；新增 `drain_host_requests()` |
| `crates/dd-gui/src/main.rs` | P1：`refresh_health` 改**每帧**检测（`logic()` 入口；此前仅 `show()` 时一次）；改用 `exit_status` 区分崩溃/正常退出，崩溃记 `record_crash`、两者都回落 stub；poll_invoke/poll_page 错误分支对死进程**丢弃不归还**（`has_exited` 判别）并 `record_crash`，避免死进程占 warm 集；`drop_source_to_stub` 统一簿记清理。P3：`crash_guards: HashMap<String, CrashGuard>`、`record_crash`/`reset_crash`/`is_crash_tripped`；`dispatch_invoke`/`dispatch_fetch_page` 熔断拦截（不再 spawn，提示重启恢复）；`mark_source_warm` 成功即 `reset_crash`。P2：`poll_host_requests`/`execute_host_request` 执行端（Toast / arboard 剪贴板 / webbrowser），ui() 接线 |
| `crates/dd-gui/Cargo.toml` | + `arboard = "3"`（剪贴板）、`webbrowser = "1"`（开 URL） |
| `crates/dd-ext-sample/src/main.rs` | `initialize_result` 声明 3 个 host 能力（`host/show_status`/`set_clipboard`/`open_url`）；新增 3 条「M4 host」命令（`m4.host.show_status`/`copy`/`open_url`），invoke 回包后经 `send_host_request` 反向发 `host/*` 请求（§3.3）；`sample.copy` 副标题改准确 |
| `crates/dd-host/tests/roundtrip.rs` | 顶层命令 11→14 条断言更新；capabilities 空→3 项断言更新；新增 `roundtrip_m4_host_requests_are_answered_and_recorded`（3 个 host 请求应答+记录、参数完整） |

### 设计要点

- **崩溃判定**（§11）：`exit_status` 非 0 退出码 = 崩溃 → `record_crash`；0 退出码 = 正常退出 → 仅回落 stub。LRU 驱逐/主动 close 的进程已移出保活集，不进崩溃计数。
- **熔断后行为**：dispatch 拦截（toast「暂时不可用，重启宿主后恢复」），不再 spawn；恢复路径 = 宿主重启（CrashGuard 随进程清零）或复热成功 `reset_crash`。
- **poll 时序**：`invoke` 响应可能先于扩展的 host 请求到达 → 测试/轮询先 `poll_notifications`（消费 rx 应答 HostRequest）再 `drain_host_requests`。
- **编译踩坑**：rustc 1.96 rmeta encoder ICE（增量缓存损坏）→ 按项目惯例 `CARGO_INCREMENTAL=0`；roundtrip 测试用旧 sample exe 需先 `cargo build -p dd-ext-sample`。

## 3.5 P4 实施记录（2026-09-02，扩展侧先行）

### 文件改动

| 文件 | 改动 |
|---|---|
| `crates/dd-ext/Cargo.toml`（新建） | 共享扩展运行时 crate（D1-A）：依赖 `dd-protocol` + `serde_json`；`[[bin]]` 声明 5 个 bin（`dd-ext-apps/calc/system/websearch/shell`） |
| `crates/dd-ext/src/lib.rs`（新建） | 共享运行时：`run`/`serve_line`（分发 initialize/top_level_commands/fallback_commands/get_command/invoke/get_items/close；未注册方法 → -32601）；`ExtensionSpec`（id/display_name/frozen/has_fallback/capabilities/top_level/fallback/invoke）；`Effect` 副作用模型（HostRequest / ItemsChanged）；`initialize_result` 回 ProviderInfo；make_host_request 自增 id；16 单测 |
| `crates/dd-ext/src/bin/calc.rs` | 计算器（com.ddrun.calc，frozen+fallback）：手写递归下降求值器（+ - * / % ^、括号、一元、常量 pi/e；^ 右结合）；fallback 模板 `calc.eval.query`（title="= {query}"）；invoke → ShowToast + host/set_clipboard；8 单测 |
| `crates/dd-ext/src/bin/websearch.rs` | 网络搜索（com.ddrun.websearch，frozen+fallback）：ENGINES 5 引擎（google/bing/baidu/duckduckgo/github）；每引擎顶层 + fallback 模板；invoke → host/open_url（RFC 3986 percent-encode）；7 单测 |
| `crates/dd-ext/src/bin/system.rs` | 系统命令（com.ddrun.system，frozen 无 fallback）：COMMANDS 5 条（lock/sleep/shutdown/restart/logoff）；危险 3 项首发 → Confirm{is_critical}；`decide` 纯决策层 + `launch` 分离；`#[cfg(not(windows))]` 占位；5 单测 |
| `crates/dd-ext/src/bin/apps.rs` | 应用启动（com.ddrun.apps，**fresh** frozen=false）：OnceLock 枚举开始菜单 `*.lnk` + PATH `*.exe`，去重排序 + MAX_APPS=400；`.lnk` → `cmd /C start`、`.exe` → spawn；4 单测 |
| `crates/dd-ext/src/bin/shell.rs` | Shell（com.ddrun.shell，frozen+fallback）：顶层 open_terminal；fallback `shell.run.query` 无头执行（cmd /C，3s 超时 kill）+ 摘要；3 单测 |
| `crates/dd-host/src/builtin.rs`（新建） | 内置注册表 `BUILTINS`（5 spec：exe/id/name/frozen/capabilities，与扩展 spec 对齐）+ `ensure_builtins(exe_dir)`（存在 exe 才注册）+ `merge_builtins`（内置优先、磁盘同 id 去重）；4 单测 |
| `crates/dd-host/src/manifest.rs` | 抽 `from_command` 共用构造；`from_executable` 委托（行为不变）；新增 `from_builtin`（支持 frozen/capabilities/version） |
| `crates/dd-host/src/process.rs` | 新增 `fallback_commands()`（§6.2，2000ms）+ `invoke(params)`（§6.5，10000ms）方法封装；`TIMEOUT_FALLBACK_COMMANDS` 常量 |
| `crates/dd-gui/src/aggregator.rs` | `load_extension_sources` 改造：内置 `ensure_builtins`（current_exe 同目录）**常驻** + extensions.d 磁盘清单并入（同 id 内置优先）；删除 sample 兜底（D7-A） |
| `crates/dd-gui/src/main.rs` | `invoke_on` 自由函数改为委托 `proc.invoke()`（协议样板只写一份）；删多余 `TIMEOUT_INVOKE` import |
| `crates/dd-host/tests/roundtrip_builtins.rs`（新建） | 6 项真实进程全链路：握手与注册表一致 / 顶层非空 / fallback 契约（3 有 2 无）/ calc invoke 求值+剪贴板 / websearch URL 编码 / system 危险首发 Confirm |

### 设计要点

- **共享运行时**（D1-A）：5 扩展共用 `dd-ext` lib 的 JSON-RPC 服务端样板，各 bin 只写
  `spec()`（命令集合 + handler）——产物仍是 1 进程/扩展（ADR-1 子进程隔离不变）。
- **注册表防漂移**：`builtin.rs` 的 `BUILTINS` 元数据与各 bin `spec()` 对齐，靠
  `roundtrip_builtins::builtin_initialize_matches_registry`（握手 ProviderInfo 逐字段断言）防漂移。
- **内存自注册**（D5-A）：`from_builtin` 直接构造 LoadedExtension（version=宿主版本，
  宿主升级即桩缓存自然失效）；不写 extensions.d、无持久状态。
- **安全约束**：system 的 confirmed=true 重发会真关机、apps invoke 会真启动应用、shell fallback
  会真执行命令——**均不进 roundtrip 自动测试**（decide 纯函数单测 + A10 真机人工核对）；
  自动测试只到「危险首发 Confirm」为止。
- **calc 求值器修正**：初版入口把空白整体剔除导致 `"2 2"` 被合并成 `"22"`（MissingOperator
  永不触发）→ 改为 trim + Parser 在 token 边界 `skip_ws`。
- **system 测试副作用修正**：初版 `dangerous_command_confirmed_then_executes` 会真发
  `shutdown.exe /s`（测试日志出现关机报错）→ 拆出纯函数 `decide()`，单测只测决策层。

### 验证结果

- `cargo build --workspace`：零警告（CARGO_INCREMENTAL=0）。
- `cargo test --workspace`：**132/132 全绿**（dd-ext 43：lib 16 + apps 4 + calc 8 + shell 3 + system 5
  + websearch 7；dd-gui 29；dd-host 49：lib 35 + roundtrip 8 + roundtrip_builtins 6；dd-protocol 11）。
- `cargo clippy --workspace --all-targets`：零警告；`cargo fmt --check`：干净。
- roundtrip_builtins 6 项 = 5 扩展真实 spawn → 握手 → 顶层 → fallback → invoke 全链路（Calc 求值
  "= 2"+剪贴板、WebSearch 编码 URL、System 危险首发 Confirm 均经**真实进程**验证）。
- 产物：`target/x86_64-pc-windows-gnu/debug/` 下 `dd-gui.exe` + 5 个 `dd-ext-*.exe` 已重编。

## 3.6 宿主 fallback 轮实施记录（2026-09-03）

### 文件改动

| 文件 | 改动 |
|---|---|
| `crates/dd-host/src/builtin.rs` | `BuiltinSpec` 增 `has_fallback` 字段；`frozen` 明确为**扩展自述**（与 spec 防漂移对齐），新增 `host_frozen() = frozen && !has_fallback` 派生（§6.3 宿主落桩策略）；`ensure_builtins` 改传 `host_frozen()`；单测断言拆分（自述 frozen / has_fallback / host_frozen 三组） |
| `crates/dd-host/src/cache.rs` | `FrozenCache` 增 `remove(ext_id)`：删除该扩展**全部版本**桩（D8：含兜底者清历史桩）+ 1 单测 |
| `crates/dd-gui/src/aggregator.rs` | `spawn_and_initialize_with_info`（额外返回 `InitializeResult`）；`load_one` 落桩条件改为 `frozen && !provider.has_fallback`，`has_fallback=true` 时清除历史桩并保持 warm |
| `crates/dd-gui/src/fallback.rs`（新建） | 纯逻辑层：`FallbackStore`（Vec 保序登记 + ExtState Unknown/Fetching→Ready/Exhausted 状态机、`wants`/`begin_fetch`/`store`/`store_failure`/`render`/`is_empty`/`template_count`）+ `render_title`（`{query}` 全替换）+ `fetch_fallback_commands`（2s 超时映射）；7 单测 |
| `crates/dd-gui/src/state.rs` | `PanelState` 增 `fallback` 展示集 + `set_fallback`/`clear_fallback`/`is_fallback_mode`/`has_regular_match`；`filtered()` 空查询返回 Box 迭代器，fallback 模式不二次过滤；`reset()` 清 fallback；6 单测 |
| `crates/dd-gui/src/lib.rs` | 注册 `fallback` 模块 |
| `crates/dd-gui/src/main.rs` | PaletteApp 增 `fallback_store`/`fallback_rx`；`FallbackFetchOutcome`；`sync_fallback`（Root 且查询非空且常规无匹配时渲染/触发）、`start_fallback_fetch_chain`（每轮 1 个 warm 扩展后台拉取）、`poll_fallback`（存模板/失败→Exhausted/归还或丢弃进程/链式续拉/重绘）、`rerender_fallback`；`draw_panel` 查询后同步、`ui()` 轮询接线；`invoke_on` 保留委托 |
| `crates/dd-host/tests/roundtrip_builtins.rs` | `load_builtin` 改传 `host_frozen()`；握手断言按"扩展自述"（provider.frozen == spec.frozen、has_fallback == spec.has_fallback） |
| `crates/dd-host/tests/roundtrip.rs` | M4 P2 host 请求断言改**轮询等待**（初版 25×20ms）；TDD 验证轮进一步加固为**截止时间条件等待**（轮询至 host 请求落袋或超 2s 预算，sleep 10ms）——消除高负载/并行跑多 roundtrip 时固定小预算窗口被放大而偶发错过（非行为变更） |

### 设计要点

- **概念拆分**（D8-A）：`BuiltinSpec.frozen`（扩展自述，防漂移哨兵对齐 spec()）≠ `host_frozen()`
  （宿主落桩策略，§6.3 收窄为 `frozen && !has_fallback`）。`manifest.frozen` 落的是 **host_frozen**
  ——所以 Calc/WebSearch/Shell 冷启动即 spawn（fresh），System 仍可落桩（A6）。
- **第三方同语义**（load_one）：第三方清单即使标 frozen，握手后 `provider.has_fallback=true`
  也不落盘（并清历史桩）——宿主的"含兜底者 fresh"判定对所有扩展一致，无内置特判。
- **触发 = 全局无匹配**（D9-A）：仅 Root 页 + 查询非空 + `has_regular_match()==false` 时展示/拉取；
  嵌套页走扩展自己的 `get_items` search 过滤，不参与。
- **拉一次缓存**（D10-A）：FallbackStore 每扩展至多 1 次 RPC（成功非空→Ready 后续本地渲染；
  空/失败→Exhausted 本会话不重试）；渲染纯本地 `title.replace("{query}", query)`。
- **执行链路零改动**：fallback 项是普通 `PanelItem`（带 ext_id），Enter 走既有
  `confirm_selected` → `invoke_params(id, query)`（已带 `context.query`）→ warm invoke /
  stub 复热；invoke 处理链未动。
- **LRU 边界**：fallback 项在扩展被 LRU 驱逐后点击会走复热 spawn + `get_command(id)`，
  而 `get_command` 只查顶层命令、不回 fallback 模板 → 复热失败 toast。内置 5 扩展 < LRU
  容量 8 且含兜底者 fresh 常驻，实际不触发；第三方多扩展场景留待后续评估。

### 验证结果

- `cargo test --workspace`：**146/146 全绿**（较 132 新增 14：fallback.rs 7 + state.rs 6 + cache remove 1；
  roundtrip 与 roundtrip_builtins 并行跑两次均无竞态失败）。
- `cargo clippy --workspace --all-targets`：零警告；`cargo fmt --check`：干净。
- 产物重编：`dd-gui.exe` + 5 个 `dd-ext-*.exe`（宿主 fallback 轮版，2026-09-03）。

---

## 4. 真机验收清单（M4，人工复验）

> 与 m1/m2/m3 清单同风格：每项给 步骤 / 预期（面板 + 终端双信号）/ 失败信号。用**终端启动** exe 观察 `[dd-gui]` 日志。
>
> **P4 起注意**（D7-A）：`load_extension_sources` 已改为内置 5 扩展常驻 + 磁盘清单并存，
> `dd-ext-sample` **不再是 GUI 启动源**（仍是 roundtrip/协议测试 fixture）。因此 P1–P3 复验
> 中「杀 `dd-ext-sample.exe`」一律改为杀**任意内置扩展**（如 `dd-ext-system.exe` / `dd-ext-apps.exe`），
> A8 路径等价；页面源名相应从 `Sample Ext` 变为对应内置扩展名。
>
> 复验用 exe = **P4 版（2026-09-02 重编）** `dd-gui.exe` + 5 个 `dd-ext-*.exe`
> （`target/x86_64-pc-windows-gnu/debug/`）。

### A8 崩溃恢复（对应 P1）

1. **启动与基线**：终端启动 `dd-gui.exe`，确认页脚有内置扩展源（如 `Apps`/`System`/`Shell` 等）、日志无异常。
2. **执行中崩溃**：任务管理器或 `Stop-Process` 杀掉一个内置扩展进程（如 `dd-ext-system.exe`；面板开着、选中该源一条 Invoke 命令）。
   - 预期：宿主**不退出**；页脚 `✓ → ✗`；日志 `[dd-gui] 扩展进程已退出：com.ddrun.system（…回落 stub）`；
     toast 提示命令失败；再次 Enter 触发**复热 spawn**，进程恢复、页脚回 `✓`（或 `◌`→点击复热）。
   - 失败信号：宿主进程退出 / 面板卡死 / 无任何状态变化。
3. **in-flight 失败**：发出请求后立刻杀进程（或杀进程后立刻 Enter）。
   - 预期：请求失败提示（toast/日志），不挂死、可继续操作。
   - 失败信号：UI 永久等待（转圈不结束）。

### 连续崩溃保护（对应 P3）

4. **连续崩溃熔断**：连续 3 次杀掉刚复热的进程（每次杀后立即点击其命令触发复热 → 再杀）。
   - 预期：第 4 次点击时不再拉起进程；页脚标记"暂时不可用"；日志 `[dd-gui] … 连续崩溃 N 次，标记暂时不可用`。
   - 失败信号：仍无限拉起进程。
5. **恢复**：重启 `dd-gui.exe`（或后续提供"手动重试"入口后点重试）。
   - 预期：扩展恢复可拉取。
   - 失败信号：重启后仍熔断。

### `host/*` 能力（对应 P2）

6. **show_status → Toast**：终端启动 `dd-gui.exe`，搜索并执行 `M4：host/show_status`。
   - 预期：面板出现 Toast「M4 host/show_status：Toast 显示成功」；终端 `[dd-gui] host/show_status（ext=…）` 与 `[dd-ext-sample] -> host/show_status (id=N)`。
7. **set_clipboard → 剪贴板**：执行 `M4：host/set_clipboard`（或原 `Copy Sample Text`）。
   - 预期：终端日志后，任意处粘贴得到 `dd-run M4 clipboard demo：3.14159`。
8. **open_url → 浏览器**：执行 `M4：host/open_url`。
   - 预期：默认浏览器打开 `https://github.com/Yang-SH/dd-run`。

> P2 触发命令已就绪：host/* 命令在 `dd-ext-sample` 顶层「M4 host」分组（`m4.host.show_status` /
> `m4.host.copy` / `m4.host.open_url`）——P4 起 GUI 不再兜底加载 sample，复验 host/* 时需在
> `extensions.d` 放入指向 `dd-ext-sample.exe` 的清单（或待宿主 fallback 轮后改用内置扩展触发：
> Calc 计算→set_clipboard、WebSearch 搜索→open_url）。
> 复验用 exe = **宿主 fallback 轮版（2026-09-03 重编）** `dd-gui.exe` + 5 个 `dd-ext-*.exe`
> （`target/x86_64-pc-windows-gnu/debug/`）。

### A10 内置扩展功能清单（对应 P4）

> 宿主 fallback 轮完成后不再拆批：顶层命令与 fallback 交互均可真机核对
> （Calc/WebSearch/Shell 为 fresh 常驻，无匹配输入即出现兜底项）。

9. **Apps 枚举真实应用**：启动 `dd-gui.exe`，搜索 `apps` 分组命令。
   - 预期：列出开始菜单/PATH 中的真实应用（非空）；Enter 一条 `.lnk` 应用可启动。
   - 失败信号：列表为空 / 启动无反应或报错。
10. **System 锁屏**：搜索并执行 `system.lock`。
    - 预期：工作站锁屏（危险项 shutdown/restart/logoff 首发弹确认框，确认后执行——**人工确认**）。
    - 失败信号：无反应 / 未锁屏。
11. **Calc 求值（fallback）**：输入 `1+1`（常规列表无匹配）→ 面板出现兜底项 `= 1+1` → Enter。
    - 预期：toast 显示 `= 2` 且剪贴板已复制；终端日志 `[dd-gui] 拉取兜底模板：ext=com.ddrun.calc`
      → `兜底模板就绪` 与 calc 侧 `host/set_clipboard`。
    - 失败信号：无兜底项出现 / 显示"没有匹配项"。
12. **WebSearch 搜索（fallback）**：输入 `rust command palette` → 出现「在 Google 搜索 …」等 5 项 → Enter。
    - 预期：默认浏览器打开 `https://www.google.com/search?q=rust%20command%20palette`（面板关闭）。
    - 失败信号：无兜底项 / URL 未编码或错引擎。
13. **Shell 执行（fallback）**：输入 `echo hi` → 出现「运行 echo hi」→ Enter。
    - 预期：toast 回显 `hi`（无头执行）；顶层 `shell.open_terminal` 打开新 cmd 窗口。
    - 失败信号：toast 报错 / 无输出。

---

## 5. 遗留与边界（M4 期间不处理或后续轮次）

| 项 | 说明 | 归属 |
|---|---|---|
| LRU 驱逐后的 fallback 复热 | 含兜底扩展被 LRU 驱逐后点击 fallback 项 → 复热走 `get_command`（只查顶层、不回 fallback 模板）→ toast 失败。内置 5 < LRU 8 不触发；第三方多扩展场景待评估 | 后续评估 |
| Apps/System 平台扩展的 macOS/Linux 实现 | P4 D6-A Windows 优先：非 Windows 为编译恒成立占位 | 对应平台轮 |
| A3 nucleo 过滤 | P5（与健壮性正交） | 后续轮 |
| 手动重试入口 | 协议 §11"用户手动重试"需要 UI 入口（当前只有重启恢复） | P3 收尾时评估 |
| 顶层 `items_changed` 不重聚合 | 延续 M3 遗留（m3-record §5） | 与 P4 扩展联动时处理 |
