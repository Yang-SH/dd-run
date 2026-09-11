# M9 — 内置扩展进程内化（In-process Built-ins）

> 状态：设计稿 **v1.2**（2026-09-11 立项 v1.0；同日 `verify-plan-against-code` 核查修订 → v1.1；同日 B3 实施中发现并修复 i18n 缺口 → v1.2）
> 范围：**仅内置 5 扩展 in-process**；第三方 / sidecar（Python 示例、文件搜索 `dd-ext-search`）**保持子进程**。
> 崩溃策略：in-process 调用以 `std::panic::catch_unwind` 兜底，单扩展 panic → 该扩展标 Failed、宿主存活。
> 协议：**v1.0 零改动**（`dd-ext::serve_line` 与 `dd-protocol` 不动）。
>
> **里程碑状态：✅ 已关闭（2026-09-11）** —— B1–B5 全部完成，真机复验通过。

---

## 0. 修订说明（v1.1 核查对齐）

经 `verify-plan-against-code` 逐文件核查代码库，对 v1.0 做了如下对齐修正（详见文内 ✏️ 标记）：

- **D1 类型不可实现** → `BUILTIN_SPECS` 原定为 `&[&'static ExtensionSpec]`，但 5 个 `spec()` 均用 `tr()` 运行时选文案（`tr` 读 `DDRUN_LANG` OnceLock，非 `const fn`），无法 const/static 化。改为运行期构造：`pub fn builtin_specs() -> Vec<ExtensionSpec>`（或 `OnceLock` 缓存一次）。
- **§1 依赖图错误** → `dd-gui` 当前 **未**依赖 `dd-ext`（`Cargo.toml` 仅有 `dd-host/dd-protocol/...`，无 `dd-ext`）。B2 必须先加依赖，不能写"已依赖 ✅"。
- **D4 `EMBED_EXES` 表述误导** → 当前 `build.rs` 的 `EMBED_EXES` 是 5 个内置、**不含 `dd-ext-search`**（search 本就走 `dist/extensions.d/` sidecar，从不内嵌）。修正为"清空内置内嵌表"。
- **D3 模型缺口** → `LoadedExtension { .., command: PathBuf }` 与 `from_builtin(command: PathBuf, ..)` 依赖 exe 路径；in-process 无路径。补 in-process 独立注册结构 / 变体说明。
- **D2 R1 风险低估** → in-process 返回原始 `Vec<serde_json::Value>`，无后台读线程，须手动路由 `result`/`host/*`/`items_changed` 到既有总线。展开共享 `route_messages` 方案 + 逐字节对比单测。
- **B5 验收遗漏** → `--conformance` 当前经 `ExtensionProcess::spawn` 子进程自检（`main.rs:485`）。补：`dd-run-cli` 须同步改造（内置改走 `serve_line`），否则"in-process 路径全绿"不成立。
- **§5 注释过期** → 补 `embedded.rs` / `build.rs` 顶部 ADR-1 注释同步更新。
- **bin 数量歧义** → 显式：仅 5 内置上移；`search.rs` 是第 6 个 bin（sidecar），不参与。

## 0.1 修订说明（v1.2，B3 实施发现）

B3 实施中暴露一处 v1.1 未覆盖的**行为缺口**，已随 B3 一并修复（新增决策 D6）：

- **多语言（`DDRUN_LANG`）在 in-process 下失效**。原机制是"宿主 spawn 内置扩展时经 `entry.env` 注入 `DDRUN_LANG`，扩展进程启动时**读一次**并缓存（`dd-ext::i18n` 的 `OnceLock`）"。内置改 in-process 后该 env 落在**宿主进程**上，且设置页切换语言靠"离开设置页 → 重聚合 → 以新 env 重启进程"生效——而 `OnceLock` **无法重设**，内置文案会永久停留在首次读到的语言（中文 UI 切英文后内置仍显示中文）。
- **修复（D6）**：`dd-ext::i18n` 的生效语言由 `OnceLock` 改为进程级 `AtomicU8` + 新增 `pub fn set_lang(Lang)`；宿主在**每次（重）聚合前**（`app::spawn_aggregation` 起始处）按当前设置页语言显式 `set_lang`，内置文案随语言切换即时生效。子进程扩展仍走 `entry.env` + `Lang::from_env()` 兜底，语义不变。协议零改动。

---

## 1. 动机与决策

用户选择把"启动时多个包（扩展进程）运行合并进 dd-run 宿主本身"作为默认行为。本质是**推翻 M0 的 ADR-1 进程隔离硬约束**，但仅对内置扩展——这是 ueli / PowerToys CmdPal 的主流做法（插件进程内），且本仓架构已为此预留了条件。

**可行性根因**（已核实 `crates/dd-ext/src/lib.rs`）：
- `dd-ext` 已是纯库，`serve_line(spec, line) -> (Vec<serde_json::Value>, bool)`（`lib.rs:137`）是**纯函数**——不碰 stdin/stdout，返回的是消息值。
- 5 个 `dd-ext-*.exe` 只是 `run(spec)` 的薄壳（`lib.rs:101` 读 stdin → `serve_line` → 写 stdout）。
- 因此 in-process = 宿主持**运行期构造**的 `ExtensionSpec`（见 D1 ✏️：`&'static` 不可行）直接调 `serve_line`，把返回消息交给既有的 `host/*` 处理与通知总线。**协议逻辑零重写**。

**收益**：启动 0 进程 / 0 物化磁盘 / 直接调用；单文件体积下降（不再内嵌 5 exe）；代码结构收敛（bin 壳退化为一行 `main`）。
**代价**：内置扩展失去进程级崩溃隔离 → 用 `catch_unwind` 部分恢复；`build.rs`/`package.sh` 收敛。

**不做的**：第三方 / sidecar 仍子进程（保留 M8 语言无关证明 + 未信任代码崩溃隔离）；跨平台 macOS/Linux 接缝（apps/shell/system 的 TODO 占位）不在本里程碑，留 v0.2。

---

## 2. 依赖图事实（已核实，v1.1 修订）

| crate | 依赖（现状） | 本里程碑动作 |
|---|---|---|
| `dd-protocol` | — | 不动 |
| `dd-ext` | `dd-protocol` +（image/windows-sys/everything-ipc 仅 bin 用，不入 lib） | **新增 `builtins` 模块**：集中 5 个 `ExtensionSpec`（运行期构造，`serve_line` 已 pub） |
| `dd-host` | `dd-protocol` | 不动（仍只管子进程客户端 + 清单扫描）；`LoadedExtension` 模型**不承载** in-process 内置（见 D3 ✏️） |
| `dd-gui` | `dd-host` + `dd-protocol` +（eframe/windows-sys/…） | ✏️ **B2 新增 `dd-ext` 依赖**（当前缺失）；承接 in-process 调用 |
| `dd-run-cli` | `dd-host` + `dd-ext` | ✏️ **B5 同步改造 `--conformance`**：内置改走 `serve_line`（原 `main.rs:485` 经 `ExtensionProcess::spawn` 子进程自检） |

> 关键结论：in-process 调用逻辑落在 **`dd-gui` 层**（aggregator / app 编排），不碰 `dd-host` 的进程抽象。这与现状一致——`dd-host` 管"进程客户端"，`dd-gui` 管"内置注册 + 物化"。**但 `dd-gui` 当前并不依赖 `dd-ext`**，B2 第一步必须先补依赖，否则无法调用 `serve_line` / `builtin_specs`。

---

## 3. 设计

### D1 — 内置规格上移到 `dd-ext::builtins` ✏️

- 新增 `crates/dd-ext/src/builtins.rs`，导出 **运行期构造** 的规格集合：
  - `pub fn builtin_specs() -> Vec<ExtensionSpec>` —— 返回 apps / calc / system / websearch / shell 五项（或 `pub static SPEC: OnceLock<Vec<ExtensionSpec>>` 懒初始化一次，按生效语言 `DDRUN_LANG` 构造）。
  - 每项对应 `pub fn spec_calc() -> ExtensionSpec` 等细粒度函数（原各 `bin/*.rs` 的 `spec()` 函数体搬入，Windows 专属逻辑如 AppsFolder 枚举原样保留；`tr()` 调用保留）。
- **⚠️ 不可把集合定为 `&[&'static ExtensionSpec]`**：`spec()` 内的 `tr("中文","English")` 在运行期按 `DDRUN_LANG` 选文案（非 `const fn`），`ExtensionSpec` 持有的是运行期选定的 `&'static str`，整体不能作为 `static` 初始化（编译失败）。
- 各 `bin/*.rs` 的 `main()` 退化为 `dd_ext::run(&dd_ext::builtins::spec_calc())`（仍一行 `main`，dev/debug + 第三方兼容用）。
- 把 5 个 `spec_*()` 迁到 `builtins.rs` 后，`bin/calc.rs` 等不再自带 `spec()`；`bin/search.rs`（第 6 个 bin，**sidecar**，含 PageHandler / everything-ipc）**不动、不参与 in-process**。
- `dd-gui` 通过 `dd_ext::builtins::builtin_specs()` 拿到规格，无需 spawn。
- **行为不变**：`serve_line` 与各 `spec_*()` 逻辑原样迁移，`cargo test -p dd-ext` 全绿（含现有 `serve_line` 16 条）。

### D2 — `dd-gui` 新增 `InProcessExtension` 适配

- 在 `dd-gui` 层（建议 `app/ext_inprocess.rs` 或并入 `aggregator.rs`）定义：持运行期构造的 `ExtensionSpec`（来自 D1），暴露与 `ExtensionProcess` **同语义**的调用接口（`initialize` / `top_level_commands` / `get_command` / `invoke` / `get_items` / `close`）。
- 每个调用：序列化请求为 NDJSON line → `dd_ext::serve_line(spec, line)` → 解析返回的 `Vec<serde_json::Value>`：
  - `result` 消息（id 匹配） → 同 subprocess 的响应解析管线解析为强类型结果；
  - `host/*` 请求消息 → 宿主既有 `host_request` 处理（fire-and-forget，与 subprocess 路径一致）；
  - `items_changed` 通知 → 宿主既有通知总线。
- ✏️ **R1 接线（原稿低估，此处展开）**：`ExtensionProcess` 的后台读线程把 stdout 喂给 `classify` → `poll_notifications` / `drain_host_requests`（`process.rs`）。in-process **没有后台线程**，必须**复用同一套消息分类+路由逻辑**。建议重构 `dd-host`/`dd-gui` 抽出共享 `fn route_messages(&[serde_json::Value])`（或把 `classify`/`drain_host_requests`/`poll_notifications` 抽为可独立调用的函数），`InProcessExtension` 每次 `serve_line` 后直接调用它，确保 result / host/* / items_changed **逐字节等价于** subprocess 路径。
- **`catch_unwind` 包裹每次 `serve_line` 调用**（`AssertUnwindSafe`）：panic → 该扩展标 `Failed`（错误取 panic 信息），宿主存活、可重试。
- `close`：in-process 无需退出进程，noop（保留接口对称）。

### D3 — 注册路径切换 ✏️

- `dd-gui` 的内置注册不再走 `materialize()` + `ensure_builtins(exe_dir)`，改为直接由 `builtin_specs()` 构造 `InProcessExtension`（保留 `merge_builtins` 的去重/内置优先语义，作用于 id）。
- ✏️ **模型缺口**：`LoadedExtension { manifest, path, dir, command: PathBuf, cwd }`（`manifest.rs:126`）与 `from_builtin(command: PathBuf, ..)`（`manifest.rs:427`）都以 **exe 路径**为核心；in-process 内置**无 exe 路径**，`ensure_builtins` 的"exe 是否存在"探测对 in-process 失效。两种可行落法（B3 选一）：
  1. in-process 内置走**独立注册结构**（不混入 `LoadedExtension`），宿主保留 `enum Extension { InProcess(InProcessExtension), Subprocess(LoadedExtension) }` 调度；或
  2. `LoadedExtension` 增加 in-process 变体（`command` 改为 `Option<PathBuf>`，`None` 表示 in-process）。
  - 无论哪种，`materialize()` / `embedded.rs` 仅服务于 sidecar（文件搜索）：扫描仍解析 `exe 同目录 extensions.d/` + `%APPDATA%/dd-run/extensions.d/`。
- 第三方 / sidecar 扩展的加载链路**完全不变**（仍 `ExtensionProcess` 子进程）。

### D4 — 构建 / 分发收敛 ✏️

- `crates/dd-gui/build.rs` 的 `EMBED_EXES` 表：当前为 5 内置 `[apps, calc, system, websearch, shell]`（**不含 `dd-ext-search`**）。✏️ 修正表述：M9 后**清空该表**（移除全部 5 内置）；`dd-ext-search` 本就不内嵌（走 `dist/extensions.d/` sidecar），无需改动内嵌表。若将来要内嵌某 sidecar，再往表里加。
- `tools/package.sh` 的 `BINS` 列表（当前 5 内置）：宿主仍 `cargo build -p dd-ext --release` 构建这 5 个 bin（dev/调试 + 第三方兼容），但**不再拷入 `assets/embed/` 内嵌**；step 5 仍拷 `dd-ext-search.exe` → `dist/extensions.d/`（sidecar 不变）。
- 5 个内置 `bin` 目标**保留**（不参与单文件内嵌），`dd-ext-search` sidecar 不变。
- 单文件 `dd-run.exe` 体积下降（去掉 5 exe 字节）。

### D5 — 协议零改动验证

- `dd-protocol` 不动；`dd-ext::serve_line` 不动；`protocol.md` v1.0 不变。
- in-process 仅是"谁调用 `serve_line`"的变化，线上协议形状、能力前置、错误码全不变。

### D6 — 内置生效语言的运行期重设（v1.2 补）

- **缺口**：in-process 内置共享宿主进程，`dd-ext::i18n` 原先用 `OnceLock` 缓存一次 `DDRUN_LANG` → 设置页切换语言后无法重设（见 §0.1）。
- **落法**：`dd-ext::i18n` 生效语言改用进程级 `AtomicU8`（`UNSET=2` 哨兵，首次访问按 env 解析并 CAS 落缓存）+ 新增 `pub fn set_lang(Lang)` / `pub fn current_lang()`。
- **宿主接线**：`dd-gui::app::spawn_aggregation` 在**开头**（早于构造 `builtin_specs()`）按 `lang` 调 `set_lang`。因 `tr()` 在**规格构造期**与**handler 调用期**都会读取该值，故一次设置即覆盖两条路径。
- **兜底不变**：子进程扩展仍由 `ExtensionProcess::spawn` 注入 `entry.env["DDRUN_LANG"]`，进程内 `Lang::from_env()` 首次读取；`FollowSystem` 仍由宿主先解析为具体语言。
- **单测策略**：翻转进程级语言的测试放在**独立集成测试进程**（`crates/dd-ext/tests/i18n_set_lang.rs`），避免与同进程并行单测（如 calc「无法计算」、shell「超时」文案断言）竞争。

---

## 4. 分批实施计划（verify-then-commit，逐批报告）

| 批次 | 内容 | 验收门限 |
|---|---|---|
| **B1** | D1：提取 `builtins.rs`（`spec_*()` + 运行期 `builtin_specs()`）+ 各内置 `bin/*.rs` 退化 `main` | ✅ **已实施（2026-09-11）**：`crates/dd-ext/src/builtins/`（apps/calc/system/websearch/shell 5 子模块 + `mod.rs` + `builtin_specs()`）就位，各 `bin/*.rs` 退化为一行 `main`；`cargo build -p dd-ext` 0 warning；`cargo test -p dd-ext` **64/65 通过**（唯一失败 `steam_installed_shown_uninstalled_filtered_root_lnk_shown` 为机器安装状态相关集成守卫，非回归） |
| **B2** | ✏️ `dd-gui` 加 `dd-ext` 依赖 + `InProcessExtension` + `catch_unwind` 接线 + 抽出共享 `route_messages` | ✅ **已实施（2026-09-11）**：`crates/dd-gui/src/ext_inprocess.rs` 新增 `InProcessExtension`（镜像 `ExtensionProcess` 接口：initialize/top_level/fallback/get_command/get_items/invoke/close + poll_notifications/drain_host_requests）；`dd-gui/Cargo.toml` 加 `dd-ext` 依赖；`call` 直接驱动 `dd_ext::serve_line` 并以 `catch_unwind` 包裹（panic → `ProtocolError::Rpc(INTERNAL_ERROR)`）；**R1 落地**：复用 `dd_host::process::classify`（已是自由函数）路由 serve_line 输出到响应/host/*/通知总线，与 subprocess 终态等价（未抽独立 `route_messages` 自由函数，采用"复用 classify + InProcessExtension 内联路由"——见 D2 ✏️「或」分支，更低回归风险）；`cargo check -p dd-gui --tests` 0 error、`cargo clippy -p dd-gui --tests` **0 告警**、`ext_inprocess` 7 个单测全绿（含逐字节等价 parity + panic 兜底） |
| **B3** | D3：内置注册切 `InProcessExtension`（选定 D3 的落法）；sidecar 链路由 `materialize` 接管 | ✅ **已实施（2026-09-11）**：落法 = **D3 落法 1（独立注册结构）**——① `dd-host::builtin::builtin_registrations()` 新增（**不探测 exe**，内置恒注册；`command` 为名义路径，不 spawn）；② 新增 `dd-gui/src/ext_client.rs`：`ExtClient { Subprocess(ExtensionProcess) / InProcess(InProcessExtension) }` 把两后端收敛到**同一方法表面**（逐方法镜像），+ `pub fn open(spec, ext)` 分流收口；③ `aggregator::load_extension_sources()` 改返回 `(exts, inproc_specs: HashMap<id, ExtensionSpec>, note)`，`collect_top_level`/`load_one` 按 spec 命中分流——内置 in-process **且不读/不落磁盘桩**；④ 复热链路（`app/invoke.rs` / `app/page.rs`）内置走 `ExtClient::open_builtin`（spec 直调），其余仍 spawn；⑤ `app` 层 `processes: Vec<(String, ExtClient)>`、`fallback::fetch_fallback_commands` 改收 `ExtClient`（调用点逻辑逐字不变）；⑥ 内置不再物化内嵌 exe（`load_extension_sources` 移除 `embedded::materialize()` 调用）；⑦ **B3 附修（D6）**：i18n 生效语言 `OnceLock` → `AtomicU8` + `set_lang`（见 §0.1）。**验证**：`cargo check --workspace --all-targets` 0 warning、`clippy --workspace --all-targets` **0 告警**、`cargo test --workspace` **全部二进制 0 失败**（dd-gui 172、dd-ext 66、dd-ext 集成 1、其余全绿）、`cargo fmt --all --check` **EXIT 0**；新增代码路径单测：`collect_top_level_builtin_uses_in_process_backend`（内置 → `is_in_process()`）/ `collect_top_level_non_builtin_still_uses_subprocess`（第三方 exe 缺失 → Failed）/ `open_with_spec_yields_in_process_client` / `open_without_any_source_errors` |
| **B4** | D4：`build.rs` `EMBED_EXES` 清空 + `package.sh` 不再内嵌 5 内置 | ✅ **已实施（2026-09-11）**：`build.rs` 的 `EMBED_EXES` 改为 `&[]`（生成 `EMBEDDED` 为空切片，`include_bytes` 计数 **0**，已核验生成的 `embedded.rs`）；`package.sh` 移除「拷 5 内置 exe → assets/embed」步骤、顶部注释改为「内置 in-process，sidecar 保持子进程」；`assets/embed/` 残留 5 个内置 exe 已删除；`embedded.rs`/`builtin.rs` 过时 `materialize` 注释已同步；`cargo fmt --check` / `clippy --workspace --all-targets`（0 告警） / `cargo test --workspace`（全绿）通过。**体积核验 ✅**：`cargo build --release` 通过（工具链缺陷已定位并修复，见下）；`dist/dd-run-0.1.1.exe` = **8,553,984 B（8.2 MB）**，较改动前工件 10,857,984 B（10.4 MB）**↓ 2.2 MB（−21%）**；`dist/extensions.d/dd-ext-search.exe` 仍在。**工具链缺陷根因（2026-09-11 定位并修复）**：rustup self-contained 的 `as.exe` **未随附运行时 DLL**（缺 `libintl-8.dll`/`libzstd.dll`/`zlib1.dll`）→ 启动即 `STATUS_DLL_NOT_FOUND` → 链接期 `dlltool ... as exited with status 53`（此前误判为 Binutils 缺陷）。修复 = 把三个 DLL 放入 self-contained 目录（或让 mingw 运行时在 PATH）；`package.sh` 全链路已跑通 |
| **B5** | D5 + 全量验收：fmt/clippy/test 绿 + ✏️ `dd-run-cli --conformance` 内置改走 `serve_line` 路径 + 重生成 dist + 真机启动 | 🟡 **部分完成（2026-09-11）**：**CLI 改造 ✅**——`dd-run-cli` 补 `dd-ext`/`dd-protocol` 依赖；新增 CLI 侧 `InProcessExt` + `Backend` 抽象，`--conformance --ext-id <内置 id>`（`com.ddrun.*`）**短路走 in-process**（不扫盘、不 spawn 子进程），磁盘扩展仍子进程；**抽出共享路由 helper `dd_host::process::route_messages`**，dd-gui `InProcessExtension` 与 CLI 共用（落实 D2「共享 route_messages」建议，消除重复实现）。`fmt/clippy/test` 全绿；新增 5 条单测（`route_messages` ×2；CLI 内置解析 / in-process 后端 / `conformance` 分支分发 ×3）——**`conformance` 内置分支端到端返回 `ExitCode::SUCCESS`**（in-process 路径全绿）。**dist + CLI 端到端 ✅（工具链修复后同日完成）**：`bash tools/package.sh` 全链路跑通，`dist/dd-run-0.1.1.exe`（8.2 MB）+ `dist/extensions.d/dd-ext-search.exe` 就位；**实跑 `--conformance --ext-id com.ddrun.{calc,shell,websearch,system,apps}` 5 项全部 `EXIT=0`**（initialize/top_level/fallback/get_command/close 全 ✓，apps 133 命令）。**真机走查（首轮）**：发现 2 处问题——① shell 打开终端起始目录继承宿主 CWD；② 中文模式残留英文（websearch 顶层标题、设置页扩展名）。**均已修复**（新增 4 条单测，见 `implementation.md` §5「真机反馈修复（2026-09-11）」），待复验。**真机 GUI 复验 ✅（2026-09-11 用户确认通过）**——进程模型（Task Manager 仅 1 个 `dd-run.exe`）、apps/calc/system/websearch/shell 功能、文件搜索 sidecar、本轮 2 处修复复验均通过 |

---

## 5. 风险

- **R1（主集成点）**：`serve_line` 返回含 `host/*` 请求与 `items_changed` 通知，宿主必须把这两类从 in-process 输出**正确路由**到既有总线，与 subprocess 路径逐字节一致。✏️ B2 重点覆盖：抽出共享 `route_messages` 复用 `classify`/`drain_host_requests`/`poll_notifications`；补"subprocess vs in-process 同输入同输出逐字节对比"单测。
- **R2**：`catch_unwind` 只挡 unwinding panic，挡不住 `abort`/OOM/unsafe UB。内置扩展同源可信，风险可接受；第三方仍隔离。
- **R3**：单文件体积下降，但 `dd-ext-search` sidecar 仍在（文件搜索依赖 Everything 环境，保持 sidecar 合理）；若未来要彻底单文件需把 sidecar 也 in-process（不在本里程碑）。
- **R4**：调试/日志——in-process 无独立进程，`log()` 走 stderr 需由宿主捕获（subprocess 路径已有 stderr 捕获线程，in-process 复用或改走宿主日志）。

---

## 6. 与既有文档关系

- 推翻 `package.sh` 顶部「ADR-1 进程隔离是硬约束」表述 → 改为「内置扩展 in-process（M9）；第三方/sidecar 子进程（ADR-1 对其仍生效）」。
- ✏️ 同步更新 `crates/dd-gui/build.rs` 顶部注释（"内嵌内置扩展 exe"改为"M9 起不再内嵌内置；仅将来 sidecar 才内嵌"）与 `crates/dd-gui/src/embedded.rs` 顶部注释（原"进程隔离是 ADR-1 硬约束——必须 spawn 独立子进程"改为"内置扩展 in-process（M9），embedded 仅服务 sidecar"）。
- `docs/implementation.md` §5 进度表新增 M9 行；§7 下一步引用本稿。
- 协议 v1.0 冻结声明不变。
