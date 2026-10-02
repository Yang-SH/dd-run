# dd-run 稳定性 · 可用性 · 安全性加固方案（R 系列）

> **状态**：实施中 ｜ **版本**：v1.14 ｜ **最后更新**：2026-10-02
> **关联**：[security-audit-2026-09-23.md](./security-audit-2026-09-23.md)（S-01–S-11 已闭环，本文不重复）· [future-features-plan.md](./future-features-plan.md)（N1–N5 功能向，本文不重叠）· [optimization-plan.md](./optimization-plan.md) · [search-file.md](./search-file.md) · [search.md](./search.md) · [implementation.md](./implementation.md) · [protocol.md](./protocol.md) · [manifest-schema.md](./manifest-schema.md) · [INDEX.md](./INDEX.md)

---

## 1. 结论总表

本文是 S-01–S-11 安全审计闭环之后的**新一轮加固规划**，来源为 2026-09-29 对全仓的三路只读代码审查（安全 / 稳定 / 可用各一路独立取证），全部 file:line 证据已于同日**逐一回读源码核对**（v1.1，更正两处子代理误报，见 §10 版本演进）。编号空间核查：S（审计）、D（设计决策）、N（功能提案）、T/I/O/K/P（各专题）均已占用，**R 系列空闲**，本文取 R（Robustness）。v1.3 增补审查（v1.1 同日）补两个维度面：**进程级防护与可诊断性**（单实例、panic 取证）、**量化验收与门禁制度**（内存基线、兼容矩阵、CI 处置），产出 R-25/R-26 与 V-13~V-16。

下表为 30 秒结论；逐项证据、方案与验收见 §3–§5，实施批次、任务勾选清单与真机走查总清单见 §6，明确不做与缓办见 §7，与既有文档关系见 §9。

| 编号 | 维度 | 级别 | 问题（一句话） | 落点 | 批次 |
|---|---|---|---|---|---|
| R-01 | 稳定 | ❌ P0 | 窗口最小化时 `raw.screen_rect` 为 `None`，`unwrap()` 崩溃整个启动器 | `ui/chrome.rs` 约 :133、:177 | 一 |
| R-02 | 稳定 | ❌ P0 | config.json / trust.json / 冻结缓存**非原子写盘**，崩溃即丢全部设置或审批态 | `settings.rs` 约 :944 · `trust.rs` 约 :232 · `cache.rs` 约 :79 | 一 |
| R-03 | 稳定 | ⚠️ P1 | 面板隐藏期间第三方扩展 stdout **无界入站队列**，流氓扩展可 OOM | `dd-host/process.rs` 约 :402、:789 | 四 |
| R-04 | 稳定 | ⚠️ P1 | in-process 内置调用**无超时**：首屏 apps COM 枚举挂死 → 首屏永不落地 / 扩展永久 busy | `ext_client.rs` 约 :124–:142 · `builtins/apps.rs` 约 :297–:432 | 四 |
| R-05 | 稳定 | 🟨 P2 | 启动期 6 处线程创建 `.expect` 是仅存的 fail-fast 面 | `hotkey.rs` 约 :74（Windows 实路径）· `tray.rs` 约 :138 · `platform.rs` 约 :89 | 一 |
| R-06 | 稳定 | 🟨 P2 | 锁中毒后 `.unwrap()/.expect()` 级联 panic（websearch 会话性死亡 / sidecar 退出） | `websearch.rs` 约 :114、:121 · `search.rs` ×7 | 一 |
| R-07 | 稳定 | 🟨 P2 | `host/set_clipboard` 在 UI 线程 `spawn().join()`，剪贴板被占用时面板卡顿 | `app/host_actions.rs` 约 :89 | 一 |
| R-08 | 稳定 | 🟨 P2 | es.exe 错误信息 200 字节截断可落在多字节字符内 → sidecar panic | `dd-ext/src/bin/search.rs` 约 :459 | 一 |
| R-09 | 稳定 | 🟨 P2 | 测试基线红：sys 测试断言开发机才装的软件（Flowframes），任何其他机器必失败 | `builtins/apps.rs` 约 :1510 | 一 |
| R-10 | 安全 | ⚠️ 中 | S-06 确认门被 `&`/`\|` 多段命令绕过（只查首词）——粘贴型社工即触发 | `builtins/shell.rs` 约 :142–:158 | 二 |
| R-11 | 安全 | ⚠️ 中 | Start Menu `.url` 文件无上限整读（S-04 同类漏网，in-process、每次启动复现） | `builtins/apps.rs` 约 :585、:618 | 二 |
| R-12 | 安全 | ⚠️ 中 | 首方 sidecar **零完整性校验**（白名单短路在哈希之前），便携布局下 S-05 性质对 `dd-ext-search.exe` 不成立 | `dd-host/trust.rs` 约 :277 | 二 |
| R-13 | 安全 | 🟨 低 | calc 递归下降无深度上限 → 宿主进程栈溢出（abort，`catch_unwind` 拦不住） | `builtins/calc.rs` 约 :322–:330 | 二 |
| R-14 | 安全 | 🟨 低 | 清单 / config.json 读盘无体积上限（每次启动，超限 JSON → 启动期 OOM/长挂） | `manifest.rs` 约 :324 · `settings.rs` 约 :915 | 二 |
| R-15 | 可用 | ❌ 高 | **启动时热键注册失败完全无提示**（冲突常见：PowerToys Run / ueli / Listary），面板打不开且无从诊断 | `hotkey.rs` 约 :154–:162 + `app/lifecycle.rs` 约 :186–:198 | 三 |
| R-16 | 可用 | ❌ 高 | `host/open_url` 启动浏览器/文件失败静默——选中「在 Google 搜索」后无任何反应 | `app/host_actions.rs` 约 :135–:142 | 三 |
| R-17 | 可用 | ❌ 高 | 设置保存失败静默——不可写目录下所有修改「看似成功」，重启回滚且无解释 | `settings.rs` 约 :932–:950 | 三 |
| R-18 | 可用 | ⚠️ 中 | 2 条热键 toast 硬编码中文，绕过 i18n（en 用户看中文） | `app/lifecycle.rs` 约 :191、:197 | 三 |
| R-19 | 可用 | ⚠️ 中 | 托盘 tooltip / 菜单硬编码 `Win+Alt+Space`，改绑热键后展示错误组合 | `tray.rs` 约 :67 · `text.rs` 约 :467 | 三 |
| R-20 | 可用 | ⚠️ 中 | 清单 JSON 解析失败的扩展**无声消失**（skipped 列表被丢弃），用户无从排查 | `aggregator.rs` 约 :357–:372 · `app/aggregate.rs` 约 :61–:66 | 三 |
| R-21 | 可用 | ⚠️ 中 | 热键捕获接受系统关键组合（如 Alt+Space 全局劫持窗口菜单）且无警示 | `app/keys.rs` 约 :230–:241 | 三 |
| R-22 | 可用 | ⚠️ 中 | IME 组词期间的 Enter 直接触发选中项——中文输入回车上屏可能误执行命令行 | `app/keys.rs` 约 :34、:135–:137 | 三 |
| R-23 | 可用 | 🟨 低 | 查询无长度上限，粘贴超大文本直入模糊匹配 / es.exe 参数 | `state.rs` 约 :234（`set_query` 入口） | 三 |
| R-24 | 可用 | 🟨 低 | sidecar 自述仍写已移除的 `f ` 前缀（过时文案，描述一旦上屏即误导） | `dd-ext/src/bin/search.rs` 约 :1190–:1192 | 三 |
| R-25 | 稳定 | ⚠️ P1 | 崩溃零痕迹：无 `panic::set_hook`、日志恒写 stderr 且 release 无控制台，未知崩溃（R-01 同级）用户无痕迹可报、无从诊断 | `dd-gui/main.rs` 入口 + `dd-protocol/logging.rs` | 一 |
| R-26 | 稳定 | 🟨 P2 | 无单实例保护：双开导致热键注册失败误导排障（R-15 假阳性）、托盘冲突、config.json 并发写丢失 | `dd-gui/main.rs` 启动早期 | 四 |

> 26 项全部满足：**零冻结契约改动**（protocol/manifest v1.0 字段、方法、错误码不动）、**零新增依赖**、不绕过 S-05 门禁（R-12 反而是补强它）。预计四批累计 diff 约 +810/−215，新增单测约 36（批一 9 / 批二 16 / 批三 7 / 批四 4，见 §6.1）；逐项验收标准内嵌于 §3–§5 各项「验收」（三口径），通用口径见 §6.5。

---

## 2. 审查方法与基线

### 2.1 输入

- **三路独立只读审查**（2026-09-29）：安全路（信任门完整性 / 残余汇点 / unsafe / 解析健壮性）、稳定路（panic 面 / UI 线程阻塞 / 资源生命周期 / 静默吞错）、可用路（错误反馈 / i18n / 交互边界 / 文档一致性），每路均要求 file:line 级证据。
- **人工全量核对**（v1.1）：主审对本文引用的全部 file:line 逐一回读源码确认；子代理证据有两处不准（R-23 文件路径、R-06 计数），已在本文更正（§10）。

### 2.2 当前基线（2026-09-29 实测）

- `cargo clippy --workspace --all-targets`：仅 1 条真实告警（`dd-protocol` thread_local 可 const 化，随批一顺手修），其余为并行构建锁文件噪音。
- `cargo test --workspace`：**dd-ext 105/106，1 失败**——`steam_installed_shown_uninstalled_filtered_root_lnk_shown`（= R-09，断言本机开始菜单存在 Flowframes，本机未装即红）；dd-gui 236/236 通过。
- **兼容矩阵与走查登记**（v1.3 增补）：目标兼容 **Windows 10 / 11（x64）**；具体版本下限未单独验证，发现低版本问题再收窄矩阵。§6.2 每条真机走查执行时须**登记实测 OS 版本/构建号**，避免「开发机稳定、用户机器崩溃」的覆盖盲区（多显示器 / DPI / RDP 类问题即其典型，见 V-14）。

### 2.3 已确认可靠的面（本轮不再动）

S-01–S-11 修复逐项复核与审计记录一致；manifest/exe 双哈希**每次 spawn 重验**、disabled_extensions 过滤先于 collect、framing 毒化逻辑与上限边界、websearch 模板 scheme 强制链、unsafe 全量走查（CNG/COM vtable/GDI/DWM/消息循环）句柄配对正确、子进程退出清理（Drop 强杀 + stdin EOF）、损坏文件启动回退（config 回默认 / trust fail-closed / 冻结桩拒载）均健全。**本方案只补缺口，不推翻既有设计。**

---

## 3. 稳定性（R-01 – R-09、R-25 – R-26）

### R-01（P0）最小化窗口触发 `screen_rect` unwrap panic

- **证据**：`crates/dd-gui/src/ui/chrome.rs` 约 :133 与 :177 均为 `let screen = ctx.input(|i| i.raw.screen_rect).unwrap();`。egui-winit 0.36.2（Cargo.lock 实测版本）在 Windows 窗口最小化（0 尺寸）时**刻意**将 `raw.screen_rect` 置 `None`；而可见路径无条件调 `chrome_begin/chrome_end`（`app/mod.rs` 约 :760、:768）。焦点丢失自隐依赖 `ever_focused`（`app/lifecycle.rs` 约 :246–:264），1 Hz 看门狗重绘与 OS 状态瞬时错位即可触达。
- **影响**：UI 线程 panic → 启动器整体崩溃。这是运行路径上**唯一确认的全应用崩溃向量**。
- **方案**：两处改用本帧已解析的 `i.screen_rect`（egui InputState 提供且首帧后恒有值），不引入新分支语义；抽 `fn current_screen_rect(ctx) -> Rect` 便于复用与单测。
- **验收**（三口径，通用门禁与记录规则见 §6.5）：
  - 单测 `r01_screen_rect_fallback_raw_none` / `r01_screen_rect_prefers_raw`：egui 测试上下文分别注入 `raw.screen_rect = None` 与有值两态——断言辅助函数分别**回落本帧已解析 `screen_rect`（不 panic）**与**优先取 `raw`**（防语义漂移）；
  - 真机 V-1 + V-14（同类触发族同判据）；
  - 回归：遮罩 / 缩放热区既有行为不变（V-1 判据内含），批一全量门禁绿。

### R-02（P0）持久化非原子写盘

- **证据**：`settings.rs` 约 :944（config.json）、`dd-host/src/trust.rs` 约 :232（trust.json）、`dd-host/src/cache.rs` 约 :79（冻结桩）均为直接 `std::fs::write`。写盘中途崩溃/断电 → 半截文件：config 丢全部设置；trust.json 解析失败 → `LedgerState::Corrupt` fail-closed，**所有已批准扩展回 Pending 需重新审批**。写盘频率高（每次设置切换、每次隐藏面板 `persist_panel_size`，`app/lifecycle.rs` 约 :123 起）。
- **方案**：两 crate 各落一个 `atomic_write(path, bytes)` 小助手（同目录写 `.tmp` → `std::fs::rename`，Windows 侧走 `MOVEFILE_REPLACE_EXISTING` 可覆盖），三处写盘接入；不引入 fsync 以免拖慢 UI 线程（崩溃窗口从「整个写入时长」缩到「一次 rename」）。
- **验收**（三口径，见 §6.5）：
  - 单测 3 条：`r02_atomic_write_replaces_existing`（旧文件存在→内容**整体**替换）/ `r02_atomic_write_missing_dir`（目标目录缺失→创建后写入成功）/ `r02_atomic_write_failure_keeps_old`（目录置只读使写入失败→**原文件保持完整旧内容、无 `.tmp` 残留**）；
  - 真机 V-2（判据含 R-17 一次性 toast 口径）；
  - 回归：`persist_panel_size` 高频路径（每次隐藏面板）无引入卡顿——V-2 附带连续 20 次唤起-隐藏无感知延迟。

### R-03（P1）扩展 stdout / host 请求无界入站队列

- **证据**：`dd-host/process.rs` 约 :402–:403 读线程把帧推入**无界** `mpsc::channel`，仅在面板可见的 `ui()` 里 `poll_notifications` 排空；约 :789 的 `host_requests.push` 同构。流氓/有缺陷扩展在面板隐藏期间刷 stdout（每行 ≤1 MiB 不触发 TooLarge）→ 内存无界增长 → OOM 崩溃。
- **方案**：入站帧队列定容（如 128），满则丢新帧并置 `overflown` 标志，消费端合成一条告警通知（日志 + 扩展健康面可见）；`host_requests` 定容 32 并 drop-oldest（溢出需连续 32 条未消费的反向请求，先记日志）。不触碰协议。
- **验收**（三口径，见 §6.5）：
  - 单测 2 条：`r03_inbound_queue_bounded_flag`（注入 >128 帧→队列长度恒 128、`overflown` 标志置位）/ `r03_host_requests_drop_oldest`（>32 条反向请求→保留最新 32 条、最早丢弃）；
  - 真机 V-10；
  - 回归：面板可见期间的正常通知流（`poll_notifications` 排空）行为不变——既有通知单测全绿。

### R-04（P1）in-process 内置调用无超时

- **证据**：`ext_client.rs` 约 :124–:142 注明「in-process 无超时（纯函数调用）」。但首屏 `apps.top_level` 首调做 ~400 应用 COM 枚举 + 逐项图标提取（`builtins/apps.rs` 约 :297 起 `APP_CACHE: OnceLock`），其中 `lnk_target` 的 `target.is_file()`（apps.rs 约 :789）与 `GetImage` 在死 UNC 快捷方式上可阻塞至网络超时；一旦挂死：首屏永不落地（永久「加载中」）或该扩展 `inflight` 永占、回复 busy 直到重启。
- **方案**（改造型，放批四）：in-process invoke/get_items 包一层线程 + `recv_timeout`；超时返回协议层 `Timeout` 错误、清 `inflight`、该扩展本会话标记 Failed（UI 已有 Failed 渲染 + Retry）；超时线程自然滞留（受 OS 网络超时上界约束，不强行 kill），apps 的 `OnceLock` 只在成功路径落缓存、失败路径加**负缓存**避免每次聚合重付 400 应用枚举。panic 隔离面不变（`ext_inprocess.rs` 约 :264 的 `catch_unwind` 继续包裹）。
- **验收**（三口径，见 §6.5）：
  - 单测 2 条：`r04_slow_ext_timeout_failed_reset`（假慢扩展超时→协议 `Timeout` 错误→`inflight` 复位→扩展呈 Failed + Retry）/ `r04_apps_negative_cache`（枚举失败路径落负缓存→二次聚合**不再重付全量枚举**）；
  - 真机 V-11（判据含超时阈值 T——**初值 5 s**，实施时按真机首屏耗时实测校准并在 §10 留痕）；
  - 回归：panic 隔离面不变（`ext_inprocess.rs` 约 :264 `catch_unwind` 路径既有测试全绿）、熔断 / 池化无回归（批四验收门内含）。

### R-05（P2）启动期线程创建 `.expect`

- **证据**：Windows 实路径 3 处——`hotkey.rs` 约 :74（热键线程）、`tray.rs` 约 :138（托盘线程）、`platform.rs` 约 :89（CJK 字体加载线程）；另 3 处为跨平台占位/测试桩（hotkey.rs 约 :103、:120，tray.rs 约 :155），一并治理。线程创建失败即启动崩溃，是 panic 面清点后仅存的 fail-fast 残留。
- **方案**：对齐托盘既有降级口径——失败记 `log::error!` 并继续（无热键 / 无托盘运行）；其中热键降级与 R-15 的「不可用提示」共用同一状态位，用户可感知。
- **验收**（三口径，见 §6.5）：
  - 单测不可达（线程创建失败无法稳定注入）→ **代码评审口径**：grep 确认 6 处启动期线程 `.expect(` 清零，且各降级路径均有 `log::error!`；
  - 真机：无独立 V 项——热键线程降级与 R-15 状态位共位，由 V-3 侧证「无热键运行」用户可感知；
  - 回归：正常启动路径托盘 / 热键 / CJK 字体行为不变（每批五步冒烟隐含覆盖）。

### R-06（P2）锁中毒级联

- **证据**：`websearch.rs` 约 :114、:121 `CONFIGURED_ENGINES_JSON.write()/read().expect("引擎配置锁未中毒")`——任何持锁 panic 后 websearch 整会话死亡（被 `catch_unwind` 接住变 `-32603`，功能报废至重启）；sidecar `search.rs` 共 **7 处**非测试 `lock().unwrap()`（约 :76、:89、:116、:133、:139、:509、:520）中毒即进程退出。
- **方案**：统一改 `lock().unwrap_or_else(|e| e.into_inner())`（PoisonError 取回内层数据；这些锁保护的都是可整体重建的配置/索引缓存，中毒续用安全）。
- **验收**（三口径，见 §6.5）：
  - 单测 2 条：`r06_websearch_poisoned_lock` / `r06_sidecar_poisoned_lock`——持锁线程内 panic 制造中毒后，主路径经 `into_inner()` 取回数据**仍可读写且内容完整**；
  - 真机：无独立 V 项（中毒注入不可达），随批一冒烟覆盖正常读写路径；
  - 回归：websearch 引擎切换 / 搜索功能行为不变。

### R-07（P2）剪贴板写入在 UI 线程 join

- **证据**：`app/host_actions.rs` 约 :89 `METHOD_HOST_SET_CLIPBOARD` 分支 `thread::spawn(...).join()`——`poll_host_requests` 在 `ui()` 内调用，arboard 打开 Win32 剪贴板（与剪贴板管理器争 `OpenClipboard`）期间整个面板冻结。spawn 毫无收益。
- **方案**：去掉 join——写入移交常驻工作线程 + `mpsc`，结果（成功/失败）经既有 toast 通道反馈（与 R-16 同一套失败提示基建）。
- **验收**（三口径，见 §6.5）：
  - 单测 `r07_clipboard_worker_roundtrip`：请求入队→（假）工作线程消费→成功 / 失败结果回传 UI 线程（mpsc 纯逻辑）；
  - 真机 V-12（剪贴板占用器场景：面板无卡顿 + 结果 toast 正常）；
  - 回归：写入内容与来源 toast（S-07 既有行为）不变。

### R-08（P2）es.exe 错误信息截断字节边界 panic

- **证据**：`dd-ext/src/bin/search.rs` 约 :459 `&t[..t.len().min(200)]`——本地化/中文路径下 200 字节可落在多字节字符内 → `not a char boundary` panic（sidecar 进程内，进程退出由宿主熔断兜底，故低）。
- **方案**：改 `t.chars().take(200).collect::<String>()`。
- **验收**（三口径，见 §6.5）：
  - 单测 `r08_error_truncate_multibyte_safe`：构造 >200 字节且截断边界落在多字节字符内的错误串（CJK / emoji 各一）→ 返回合法 `String`、`chars().count() ≤ 200`、**不 panic**；
  - 真机：无独立 V 项，随批一冒烟（es.exe 出错路径）覆盖；
  - 回归：ASCII 错误信息截断展示不变。

### R-09（P2）测试基线依赖开发机环境

- **证据**：`builtins/apps.rs` 约 :1510 sys 测试断言开始菜单含 Flowframes（开发机自装软件），任何其他机器/CI 必失败；本次基线实测即红（§2.2）。顺带：`dd-protocol` 的 thread_local clippy 告警一行修。
- **方案**：该测试的软件枚举断言改为**合成夹具**（临时 Start Menu 目录注入 `.lnk`，枚举函数已可注入目录则直接复用；否则最小参数化），机器相关断言一律 `#[ignore]` 并更名 `machine_*` 注明前提。目标：`cargo test --workspace` 在裸机全绿。
- **验收**（三口径，见 §6.5）：
  - 改造单测 `r09_apps_enum_from_fixture`：临时 Start Menu 目录合成 `.lnk` 夹具→枚举结果**含夹具项**；机器相关断言更名 `machine_*` + `#[ignore]` 并注明运行前提；
  - 门禁：裸机 `cargo test --workspace --no-fail-fast` **连续两次全绿**（debug 构建）——即批一验收门本身；
  - 回归：apps 枚举生产行为不变（夹具仅入测试路径）。

### R-25（P1，v1.3 增补）崩溃零痕迹：无 panic hook，日志对发行用户不可达

- **证据**：全仓 grep 无 `panic::set_hook`（2026-09-29 复核）；O4 日志后端**恒写 stderr**（`dd-protocol/src/logging.rs`，§2.4 设计约束），而 release 构建无控制台（`main.rs` 约 :27 `windows_subsystem = "windows"`）——默认 `debug` 级别的全部日志对发行用户**不存在**。本方案 §5 开头已自认此矛盾，但 R-15~R-17 只解决了**已知失败**的 toast 反馈；R-01 这类未知崩溃修复后，未来任何同级崩溃用户手里无痕迹可报、开发者完全无从诊断——「稳定可靠运行」缺了事后取证这一环。
- **方案**：进程入口挂 `panic::set_hook`，panic 信息（时间戳 + message + location + 线程名）**追加写入** `%APPDATA%\dd-run\logs\panic.log`（目录解析复用 `dd-host::manifest` 的 `%APPDATA%\dd-run` 口径，约 :202–:209）。目录创建/写盘失败一律静默放弃（hook 内**不得二次 panic**）；格式化抽纯函数便于单测。默认行为不变（hook 落盘后仍走标准 panic 流程），全量日志文件化见 §7 缓办行。
- **验收**（三口径，见 §6.5）：
  - 单测 `r25_panic_log_format_states`：格式化纯函数三态断言（含 location / 多行 message / 缺 location）；路径不可写时格式化与降级**不 panic**；
  - 真机 V-16（注入 panic→`logs\panic.log` 追加一条、时间戳 / message / location 齐备；正常退出路径不新增条目）；
  - 回归：默认 panic 流程行为不变（hook 落盘后仍按标准流程展开）。

### R-26（P2，v1.3 增补）无单实例保护

- **证据**：全仓 grep 无 `CreateMutexW` / 单实例检测（2026-09-29 复核）。双开的实际后果与本方案三项直接冲突：① 第二实例热键注册失败 → R-15 toast「可能被其他程序占用」，**误导排障方向**（真凶是自己）；② 托盘双图标；③ config.json 并发写——R-02 原子写只保证文件不损坏，**不解决 last-writer-wins 丢设置**。
- **方案**：启动早期（O4 init 之后、eframe 创建之前）`CreateMutexW`（命名 `Local\dd-run-single-instance`）；`ERROR_ALREADY_EXISTS` → `MessageBoxW`（user32 已链接，零新依赖）提示「dd-run 已在运行」后退出。属启动行为变更，按改造型纪律放批四。
- **验收**（三口径，见 §6.5）：
  - 单测不可达（进程级互斥）→ **代码评审口径**：互斥体命名、句柄随进程退出释放的语义注释声明；
  - 真机 V-15（双开→第二实例弹「已在运行」退出、原实例热键 / 托盘 / 设置三面无损；附带开机自启复启一次）；
  - 回归：单实例启动与自启路径行为不变。

---

## 4. 安全性（R-10 – R-14）

### R-10（中）S-06 确认门被多段命令绕过

- **证据**：`builtins/shell.rs` 约 :142–:158——`is_dangerous_command` 只取**首词**比对（约 :140 注释自认「多段 `&`/`|` 不追查」），约 :217 以此决定是否回 `Confirm`，约 :233 `run_capture("cmd.exe", &["/C", query])` 原样执行。query = `echo hi & rd /s /q C:\...\Documents` → 首词 `echo` 不在名单 → **不弹确认直接执行破坏段**。S-06 的威胁模型（粘贴误触/社工）恰恰最常以一行多段命令出现。
- **方案**：判定前把 query 按 `&&` / `&` / `|` / 换行切段，**每段**首词各自过一遍 `is_dangerous_command`，任一段命中即 `Confirm{is_critical:true}`；纯函数改动，协议零改动（沿用 §8.3 确认重发机制）。
- **验收**（三口径，见 §6.5）：
  - 单测 6 条：`r10_dangerous_seg_amp` / `r10_dangerous_seg_double_amp` / `r10_dangerous_seg_pipe` / `r10_dangerous_seg_newline`（`&` / `&&` / `|` / 换行切段逐段判定，各含「首段无害、次段危险」用例）+ `r10_safe_multiseg_no_confirm` / `r10_safe_single_no_confirm`（全无害多段与普通单段**不误报**两条负例）；
  - 判据：任一段首词命中名单 → `Confirm{is_critical:true}`；纯函数判定，协议零改动；
  - 回归：单段危险命令确认行为不变（S-06 既有测试全绿）。

### R-11（中）Start Menu `.url` 无上限整读（S-04 同类漏网）

- **证据**：`builtins/apps.rs` 约 :585（`url_target`）与约 :618（`url_icon_file`）对 `.url` 直接 `std::fs::read(path).ok()?`，无 metadata 限幅。审计信任假设 C 已明确「Start Menu 内容不受信」；本扩展 M9 起 **in-process**，数 GB `.url`（恶意安装包放置）→ 每次启动首屏聚合整读 → OOM/挂死，无需任何扩展批准、持久复现。
- **方案**：仿 S-04 `read_icon_limited` 口径——先 `fs::metadata`，非普通文件或 `len > 64 KiB` 直接 `None`（两处各一判断）。
- **验收**（三口径，见 §6.5）：
  - 单测 3 条：`r11_url_over_limit_rejected`（>64 KiB → `None`）/ `r11_url_normal_parsed`（小文件正常解析出目标）/ `r11_url_directory_rejected`（目录 / 非普通文件 → `None`）；
  - 真机：无独立 V 项（数 GB `.url` 难以真实放置，以单测构造为准）+ 批二冒烟（正常 `.url` 应用不受影响）；
  - 回归：正常 `.url` 快捷方式图标 / 目标解析不变。

### R-12（中）首方 sidecar 零完整性校验，S-05 对其不成立

- **证据**：`dd-host/trust.rs` 约 :277 `ExtOrigin::Sidecar if FIRST_PARTY_IDS.contains(&id.as_str()) => Trust::AutoTrusted`（白名单现值 = `com.ddrun.filesearch`，约 :55）——短路在哈希**之前**，`dd-ext-search.exe` 从不参与 `hashes_match`。旗舰分发形态是便携 zip（manifest-schema §2），`extensions.d\` 在用户可写位置：同用户程序替换 sidecar exe（或加一份指向自己 exe 的 `com.ddrun.filesearch.json`）→ 每次启动（含开机自启）**静默拉起，零审批、台账零痕迹**，等于给了伪装受信组件的持久化点。
- **方案**：首跑钉扎——sidecar 首次出现时计算双哈希写入 trust.json（`Decision::Allow`、来源标 sidecar、附 `pinned_host_version`）；此后每次 spawn 走 `hashes_match`。哈希不符时按 `pinned_host_version` 是否等于当前宿主版本分流：**版本相同** → `Pending` + 设置页告警（既有 UI 渲染兜底）；**版本不同**（随应用升级正常变更）→ 静默重钉，保持零摩擦升级。信任锚 = 应用分发包本身，残余窗口（升级后首启前被替换）在 §7 声明。trust.json 非冻结契约、永不导出（N5 已定），格式扩展零兼容负担。
- **验收**（三口径，见 §6.5）：
  - 单测 3 态：`r12_sidecar_pin_on_first_sight`（首现→双哈希写入 trust.json、`Allow`、来源标 sidecar）/ `r12_same_version_tamper_pending`（哈希不符且 pinned == 当前版本→`Pending` + 设置页告警位）/ `r12_upgrade_repin_silent`（pinned != 当前版本→静默重钉新哈希）；
  - 真机 V-9（同版替换 / 正常升级两场景分别走查）；
  - 回归：第三方扩展信任门（S-05）行为零变化——既有信任门测试全绿。

### R-13（低）calc 求值器无递归深度上限

- **证据**：`builtins/calc.rs` 约 :322–:330 `parse_atom` 的 `'('` 分支递归回 `parse_expr`，深度 ∝ 输入长度；calc 在宿主进程内执行，**栈溢出是 abort，`catch_unwind`（ext_inprocess.rs 约 :264）拦不住**。入口：粘贴 ~10^5 个 `(` 后选中 `= …` 兜底项。
- **方案**：`Parser` 加 `depth: u32`，递归入口超 256 返回 `EvalError::Domain`。
- **验收**（三口径，见 §6.5）：
  - 单测 2 条：`r13_depth_over_limit_rejected`（257 层 `(` → `EvalError::Domain`）/ `r13_depth_at_limit_ok`（256 层内正常求值）；
  - 真机：无独立 V 项——粘贴超深表达式入查询框、选中 calc 兜底项一次（批二冒烟），宿主不崩；
  - 回归：calc 正常用例全绿。

### R-14（低）清单 / config.json 读盘无体积上限

- **证据**：`dd-host/src/manifest.rs` 约 :324（对 `extensions.d` 下每个 `.json` `fs::read_to_string`）、`settings.rs` 约 :915（config.json）。校验发生在读入**之后**，4 GB 外形合法的 JSON → 每次启动整读+解析 → 启动期 OOM/长挂。
- **方案**：读入前 `fs::metadata` 限幅（清单与 config 各 1 MiB，余量充足），超限记 `ParseError` 跳过 / 回默认。
- **验收**（三口径，见 §6.5）：
  - 单测 2 条：`r14_manifest_over_limit_skipped`（>1 MiB 清单→记 `ParseError` 跳过，**同目录其余扩展正常加载**）/ `r14_config_over_limit_defaults`（>1 MiB config.json→回默认不 panic）；
  - 真机：无独立 V 项，批二冒烟放置一次超限清单后正常启动；
  - 回归：正常体积清单 / config 加载行为不变。

---

## 5. 可用性（R-15 – R-24）

> 背景放大器：release 构建无控制台（`main.rs` 约 :27 `windows_subsystem = "windows"`），日志后端恒写 stderr（约 :48–:51）——一切「记日志即可」的失败对发行用户**不可见**。所以本节把「静默失败」升级为「UI 可见反馈」，并复用既有 toast 基建（`show_toast` / `tr()`），不新造轮子。

### R-15（高）启动热键注册失败完全无提示

- **证据**：`hotkey.rs` 约 :154–:162 启动注册失败仅 `log::debug!` + `ReRegistered(false)`；`app/lifecycle.rs` 约 :186–:198 的处理仅当 `hotkey_prev` 为 `Some` 才 toast，而启动时它是 `None`（`app/mod.rs` 约 :423）→ 无任何提示。设置页热键卡仍正常展示「当前组合」键帽（`ui/settings_view.rs` 约 :985–:1012），无失效标记。装了 PowerToys Run / ueli / Listary 的用户热键冲突极常见，结果就是「面板打不开、无从诊断」。
- **方案**：① 启动 `ReRegistered(false)` → 错误 toast「热键注册失败，可能被其他程序占用，请到设置更换」；② 热键卡加「未注册」状态徽标（复用 Failed 渲染口径）；③ 顺带覆盖 `GetMessageW` 出错导致热键线程静默死亡的同款提示（hotkey.rs 约 :173–:175 `r == -1` 即 break，置同一状态位）。
- **验收**（三口径，见 §6.5）：
  - 单测 `r15_hotkey_failed_state_flags`：启动注册失败→置「未注册」位；`GetMessageW` 返回 -1 退出→置同一状态位；重注册成功→复位（三段流转各一断言）；
  - 真机 V-3（PowerToys Run 占用场景：启动即见错误 toast + 热键卡「未注册」徽标）；
  - 回归：正常注册路径**不出现任何新 toast**（不误报）。

### R-16（高）`host/open_url` 失败静默

- **证据**：`app/host_actions.rs` 约 :135–:142 两处失败仅 `log::debug!/warn!`（`open_path` 失败 debug、`webbrowser::open` 失败 warn）。websearch 选中后面板已 `Dismiss`，浏览器启动失败 = 看起来「应用把命令吃了」。
- **方案**：失败路径 `show_error_toast(tr("toast.open_fail").replace("{e}", …))`（新增 i18n 键，zh/en 同批）。
- **验收**（三口径，见 §6.5）：
  - 真机 V-4（无默认浏览器 / 断网两失败源均出 `toast.open_fail`，文案含失败原因摘要）；
  - 回归：成功路径（默认浏览器正常打开）行为与既有日志不变；新增 i18n 键经 R-18 同款完备性单测覆盖。

### R-17（高）设置保存失败静默

- **证据**：`settings.rs` 约 :932–:950 best-effort 写盘失败仅 `log::warn!`；不可写目录（OneDrive 占位 / AV 锁 / 策略）下所有修改重启即回滚且无解释。
- **方案**：每会话首次写失败弹一次性 toast（含「本次修改可能未保存」口径）；与 R-02 的原子写在同一函数落地。
- **验收**（三口径，见 §6.5）：
  - 真机 V-2 合并口径：配置目录置只读后修改设置→失败 toast **恰出现一次**（会话内不重复轰炸），恢复可写后再修改→不再出现；
  - 回归：正常保存路径无 toast；与 R-02 同落点，其原子写单测覆盖本项代码路径。

### R-18（中）热键 toast 硬编码中文

- **证据**：`app/lifecycle.rs` 约 :191 `self.show_toast("全局热键已更新", None)`、约 :197 `show_toast("新热键注册失败（可能被占用），已恢复原热键", None)`——全 UI 代码仅有的两处绕过 `t()/tr()` 的用户可见字符串（i18n 表本身有双向完备性单测）。
- **方案**：新增 `toast.hotkey_updated` / `toast.hotkey_failed` 键，走 `self.tr()`。
- **验收**（三口径，见 §6.5）：
  - 单测：既有 i18n 双向完备性单测键表扩入 `toast.hotkey_updated` / `toast.hotkey_failed`（zh / en **均存在且非空**即绿）；
  - 真机：V-3 场景在 en 语言下复跑一次→两条热键 toast 均为英文；
  - 回归：zh 语言 toast 文案语义不变。

### R-19（中）托盘热键标签不随改绑更新

- **证据**：`tray.rs` 约 :67 `const TOOLTIP: &str = "dd-run — Win+Alt+Space"`（D38 注释自认静态妥协）；`text.rs` 约 :467 `tray.toggle` 双语内嵌 `\tWin+Alt+Space`。用户改绑后托盘持续展示错误组合（`settings.rs` 约 :660/:678 已有 `hotkey_mods_label`/`hotkey_vk_label` 动态拼装函数可复用）。
- **方案**：菜单创建时按 `settings.hotkey_mods/vk` 动态拼装 tooltip 与菜单项尾缀（NIM_MODIFY 跨线程复杂度维持 D38 结论不动，仅取当前值）。
- **验收**（三口径，见 §6.5）：
  - 真机 V-5：改绑为非默认组合（如 Ctrl+Shift+P）→ tooltip = `dd-run — Ctrl+Shift+P`、菜单尾缀同步展示；**改绑后即时**与**重启后**两条路径都验证；
  - 回归：默认组合（未改绑）展示行为与 D38 静态口径一致。

### R-20（中）清单解析失败的扩展无声消失

- **证据**：`aggregator.rs` 约 :357–:372 只有 `dir_error` 变 note，`outcome.skipped`（manifest.rs 约 :535 的逐清单失败原因）被丢弃；`app/aggregate.rs` 约 :61–:66 又把 note 按 2026-09-04 用户决策丢弃。对比之下信任门 Pending/Blocked、Failed 原因 + Retry 都已优秀呈现——唯独「JSON 写错一个逗号 → 扩展无声蒸发」。
- **方案**：`skipped`（路径 + 原因）以警告行渲染在扩展卡顶部（复用既有 Failed 行样式，只读不可 Retry），不改页脚 note 语义。
- **验收**（三口径，见 §6.5）：
  - 单测 `r20_skipped_aggregated_with_reason`：目录含一条语法错误清单→聚合结果 `skipped` 含（路径, 原因）；全合法目录→`skipped` 为空（正负例）；
  - 真机 V-6（扩展卡顶部警告行 = 路径 + 原因，只读、不可 Retry）；
  - 回归：页脚 note 语义不变（2026-09-04 用户决策口径）。

### R-21（中）热键捕获接受系统关键组合

- **证据**：`app/keys.rs` 约 :230–:241 任何含 Ctrl/Alt 的组合均接受（`mods & 0b0011 != 0` 即通过），如 Alt+Space 会全局劫持所有应用的窗口菜单键且 `RegisterHotKey` 会成功。注册失败的回滚路径（hotkey.rs 约 :180–:201）已健全，缺的是事前警示。
- **方案**：捕获确认时对已知关键组合（Alt+Space / Ctrl+Esc / Ctrl+Shift+Esc / Alt+F4 等）出确认提示（「该组合为系统常用快捷键，仍要使用？」），不硬禁。
- **验收**（三口径，见 §6.5）：
  - 单测 `r21_hotkey_blacklist_matches`：Alt+Space / Ctrl+Esc / Ctrl+Shift+Esc / Alt+F4 命中名单；Win+D 等非名单组合不命中（负例）；
  - 真机 V-8（确认提示出现→选择「仍要使用」→注册成功）；
  - 回归：非黑名单组合捕获流程零摩擦不变（无新增弹窗）。

### R-22（中）IME 组词期 Enter 误触发

- **证据**：`app/keys.rs` 约 :34（`consume_key(Enter)`）与约 :135–:137（`enter → confirm_selected()`）——Enter 无条件消费并激活，全链路无 IME 组合中态检查。中文用户输入 `ipconfig` 回车上屏的那一帧若与激活同帧，可能直接执行选中行（危险命令有确认门，普通命令没有）。
- **方案**：本帧收到过 `Event::Ime`（Commit/Compose 系）则跳过一次激活（吞掉该 Enter，仅上屏），下一帧恢复正常。
- **验收**（三口径，见 §6.5）：
  - 单测 `r22_ime_frame_skips_activate`：同帧含 `Event::Ime`（Commit 系）+ Enter→**不激活**且该 Enter 被吞（仅上屏）；无 Ime 帧 Enter→正常激活（负例）；
  - 真机 V-7（微软拼音 / 搜狗组词中回车上屏）；
  - 回归：非 IME 环境回车逐帧行为一致。

### R-23（低）查询无长度上限

- **证据**：`state.rs` 约 :234 `set_query` 入口无任何 clamp（grep 无 `MAX_QUERY`/`truncate`/`chars().take`）；空白查询已按空处理（约 :366）但长度不设防。超大粘贴直入模糊匹配，文件搜索页经 200ms debounce（`app/refresh.rs` 约 :13）进入 es.exe argv，超长输入只会离谱失败。
- **方案**：`set_query` 入口 clamp 至 256 字符（静默截断，光标置尾）。
- **验收**（三口径，见 §6.5）：
  - 单测 `r23_query_clamp_boundary`：输入 300 字符→查询截为前 256、光标置尾；恰 256→不截断（256 / 257 边界双断言）；
  - 真机：无独立 V 项，随批三冒烟粘贴一次超长文本（文件搜索页不向 es.exe 传入离谱参数）；
  - 回归：正常查询与 200 ms debounce 链路行为不变。

### R-24（低）sidecar 过期的 `f ` 前缀自述

- **证据**：`dd-ext/src/bin/search.rs` 约 :1190–:1192 描述文案仍写「输入 f 后空格直接进入」——`f ` 前缀 2026-09-19 已移除（search.md §2）。当前该字段未上屏，属潜伏误导，描述一旦展示即穿帮。
- **方案**：改为与 search.md 一致的自述（Ctrl+F 口径）。一行改动，随批三。
- **验收**（三口径，见 §6.5）：
  - 单测 `r24_sidecar_description_matches_spec`：sidecar 自述字段断言**不含 `f ` 前缀字样**、含 Ctrl+F 口径关键短语（与 search.md §2 一致）；
  - 真机：无独立 V 项（该字段当前未上屏），随批三代码评审核对文案；
  - 回归：描述渲染路径不变。

---

## 6. 实施批次与真机走查清单

> 排序原则：先**低风险高价值**（机械修复、崩溃面收敛），再**纯函数安全补强**，再**UI 反馈层**，最后**改造型**（需要真机压测的行为变更）。每批独立可交付、独立回滚，批间无耦合依赖。

### 6.1 批次表

| 批次 | 内容 | 改动量预估 | 新增单测 | 验收门 |
|---|---|---|---|---|
| **批一 崩溃与资源面收敛** | R-01、R-02、R-05、R-06、R-07、R-08、R-09、R-25（含 clippy 一行修） | +270/−70 | 约 9 | workspace 测试全绿（R-09 修完基线即绿）· fmt/clippy 零差异零告警 · 真机 V-1/V-2/V-12/V-16 |
| **批二 安全补强** | R-10、R-11、R-12、R-13、R-14 | +150/−40 | 约 16 | 新增单测全绿 · R-12 三态真机走查（V-9）· 重打包体积记录 |
| **批三 可用性与反馈** | R-15、R-16、R-17、R-18、R-19、R-20、R-21、R-22、R-23、R-24 | +220/−60 | 约 7 | 新增 i18n 键双向完备单测绿 · 真机 V-3~V-8 · 亮暗 × 中英四象限抽查 |
| **批四 资源、挂死与进程防护（改造型）** | R-03、R-04、R-26 | +170/−45 | 约 4 | 注入式单测（超量帧 / 慢扩展）· 真机 V-10/V-11/V-15 · 熔断/池化行为无回归 |

批四完成后执行 **V-13（内存基线）** 并回写 [implementation.md](./implementation.md) §3.1 台账；V-14（多显示器 / DPI / RDP）随批一 V-1 同场执行。

### 6.2 真机走查总清单（V 系列）

> v1.3 起：每条走查执行时在备注登记**实测 OS 版本/构建号**（§2.2 兼容矩阵）；涉及内存/性能的判据首轮建立基线后纳入回归。

| 编号 | 场景 | 覆盖项 | 通过判据 |
|---|---|---|---|
| V-1 | Win+D / 任务栏最小化期间反复唤起-隐藏-重绘（含 1 Hz 看门狗帧） | R-01 | 唤起-隐藏循环 ≥ 50 次、进程全程存活无崩溃；遮罩 / 缩放热区行为与改前一致（前后各录一次唤起位置对比） |
| V-2 | 切换设置 ≥ 20 次后检查 config.json 无 `.tmp` 残留、重启设置保留；配置目录置只读再改设置 | R-02、R-17 | 文件完整；失败 toast 出现**一次**且不重复轰炸；连续 20 次唤起-隐藏无感知卡顿（R-02 回归判据） |
| V-3 | 预先占用热键（如启用 PowerToys Run）后启动 | R-15 | 启动即见错误 toast；热键卡出现「未注册」徽标 |
| V-4 | 无默认浏览器 / 断网时选中「在 Google 搜索」 | R-16 | 失败 toast 可见，非无声无息 |
| V-5 | 改绑热键后重启查看托盘 | R-19 | tooltip 与菜单尾缀显示新组合 |
| V-6 | 放置一个 JSON 有语法错误的第三方清单 | R-20 | 扩展卡顶部出现「路径 + 原因」警告行 |
| V-7 | 微软拼音 / 搜狗输入法组词中按回车上屏 | R-22 | 仅上屏，不触发选中项；正常无组词时回车行为不变 |
| V-8 | 捕获 Alt+Space 并确认 | R-21 | 出现系统组合确认提示，确认后正常注册 |
| V-9 | 手工替换 `extensions.d\dd-ext-search.exe` / 正常升级宿主 | R-12 | 同版替换 → 设置页 Pending 告警、Ctrl+F 有明确提示；升级 → 静默重钉零摩擦 |
| V-10 | 面板隐藏期间用 test 扩展高频刷 stdout（注入 ≥ 10 000 帧） | R-03 | RSS 增量收敛至队列上界量级（128 帧 × 1 MiB ≈ ≤128 MiB）且随继续注入**不再增长**；恢复可见后出现一次溢出告警 |
| V-11 | 构造指向死 UNC 的 `.lnk` 后首屏聚合 | R-04 | 超时阈值 T（初值 5 s）内返回 `Timeout`；首屏可降级落地，apps 呈 Failed + Retry，无永久「加载中」 |
| V-12 | 运行剪贴板占用器后执行 `host/set_clipboard` | R-07 | 面板无卡顿，复制结果 toast 正常 |
| V-13 | 挂机 24 h（面板常驻）+ 唤起-隐藏 500 次循环后记录 RSS | 稳定运行全局 | 首轮建立**内存基线**并回写 implementation.md §3.1 台账；此后每次走查对比，RSS 增量超基线 +50 MB（或 +30%，取大者）须排查后方可放行（同时为 §7 icon_cache 缓办项提供转正/继续缓办的数据依据） |
| V-14 | 多显示器热拔插 / DPI 变更 / RDP 断连重连期间反复唤起-隐藏 | R-01（同类触发族） | 无崩溃，`screen_rect` 解析降级正常，窗口落在可见屏 |
| V-15 | 面板运行中再次启动 dd-run.exe（双开） | R-26 | 第二实例弹「已在运行」提示后退出；原实例热键 / 托盘 / 设置读写不受影响 |
| V-16 | 注入一次 panic（debug 构建或临时探针） | R-25 | `%APPDATA%\dd-run\logs\panic.log` 追加一条含时间戳 / message / location 的记录；hook 自身失败不产生二次 panic |

### 6.3 批间纪律

每批完成后：`cargo fmt` + `cargo clippy --workspace --all-targets` + `cargo test --workspace` 全绿 → `tools/package.sh` 重打包 → 回写 [implementation.md](./implementation.md) §3.1 测试基线台账与本方案状态 → CHANGELOG 记录。全程零新增依赖、零冻结契约改动。

### 6.4 任务清单（实施进度勾选）

> 勾选口径：代码落地 + 对应验收门（含指明的 V 项走查）通过后勾选；部分完成在行内注记。

**批一 崩溃与资源面收敛**

- [x] R-01 最小化 `screen_rect` unwrap 崩溃：chrome.rs 两处改用已解析 screen_rect + 辅助函数单测（V-1）——代码 + 单测已落地（2026-09-29：`current_screen_rect` 回落 `viewport_rect`，2 条 r01 单测绿，workspace 516/516 全绿）；**V-1 真机走查完成（2026-10-01，Win11 25H2 build 26200.9457，debug 构建 @ 0258eb1）**：普通唤起-隐藏 30 次 + 「可见→SW_MINIMIZE→驻留 ≥1.6 s（≥1 个 1 Hz 看门狗帧落在最小化态）→最小化态下热键唤起」20 次，全程进程存活、零 panic、stderr 零错误日志、`logs\panic.log` 零新增；基线/终态唤起位置逐位一致（635,250,1285,782 = 1920×1080 工作区居中；遮罩/缩放热区判据内含，截图留档对比一致）。环境注记：本会话合成键盘输入被 UIPI 拦截（GetAsyncKeyState 探针证实），唤起改用 `PostThreadMessageW(WM_HOTKEY)` 直注热键线程——与 `RegisterHotKey` 成功后系统投递的消息同源，覆盖的正是本项修复的 show/hide/最小化重绘路径；真实键盘热键链路已于 9-30 V-3 走查验证
- [x] R-02 持久化原子写：config.json / trust.json / 冻结桩三处接入 `atomic_write`（V-2）——代码 + 单测已落地（2026-09-29：两 crate 各落助手并接入三处写盘，r02 三条单测 × 2 crate = 6 条绿，workspace 522/522 全绿）；**V-2 真机走查完成（2026-10-01，Win11 25H2 build 26200.9457，debug 构建 @ 0258eb1）**：① 20 次「外部 MoveWindow 变尺寸 → 隐藏」触发 `persist_panel_size` → `save_settings_with_feedback` → 原子写，**每次写入内容各不相同**（panel_size 660×540…740×600 逐次变化，rename 替换既有文件 20 次），全程零 panic、`%APPDATA%\dd-run` 零 `.tmp` 残留；② 重启（强杀 + 重启）后 config.json SHA-256 逐字节一致 = 设置保留；③ 连续 20 次唤起-隐藏单次迁移延迟平均 66 ms / 峰值 424 ms（轮询分辨率 40 ms），无感知卡顿；④ 终态还原（尺寸回默认 → panel_size 写回 null）后 config 与基线逐值一致。口径注记：本会话合成鼠标/修饰键输入被 UIPI + winit 异步键态双重拦截（hover 像素级对照实验证实），「切换设置 20 次」以同链路的变尺寸落盘替代设置页开关——`atomic_write` 的 tmp→rename→替换/清除路径与成败反馈完全同源；设置页开关路径由 r02/r17 单测覆盖
- [x] R-05 启动线程创建 `.expect` 降级：Windows 实路径 3 处 + 占位/测试桩 3 处——代码已落地（2026-09-29：grep 复核启动期线程 `.expect(` 清零，hotkey/tray/platform 各降级路径均有 `log::error!`，workspace 522/522 全绿）；**五步冒烟完成（2026-10-01，V-1 同场）**：启动日志确认热键 / 托盘 / CJK 字体三线程全部正常起跳（托盘已注册 16px、CJK 后台加载 27 ms 热替换、扩展全 warm），无降级 `log::error!` 触发；热键线程承载 73 对 show/hide 全程正常
- [x] R-06 锁中毒恢复：websearch ×2 + sidecar ×7 改 `into_inner()` 口径——代码 + 单测已落地（2026-09-29：全部非测试 `.lock().unwrap()` 清零（含文档 :116 实为 cfg(test) 辅助函数的一并治理），`r06_websearch_poisoned_lock` / `r06_sidecar_poisoned_lock` 绿，workspace 524/524 全绿）；**冒烟完成（2026-10-01，V-1 同场）**：启动期 websearch 引擎配置读路径正常（引擎列表 warm 入聚合）、全程日志零锁相关异常；未中毒时 `into_inner()` 闭包不执行，行为与原实现逐位一致（中毒分支由 r06 单测锚定）
- [x] R-07 剪贴板写入移出 UI 线程：常驻工作线程 + 结果 toast（V-12）——代码 + 单测已落地（2026-09-29：新增 `app/clipboard_worker.rs`（常驻线程 + `run_worker_loop` 可注入写入函数，线程创建失败走 R-05 降级口径），`SET_CLIPBOARD` 分支改为入队，`poll_clipboard_results` 于 ui 循环消费结果——成功沿用 S-07 口径、失败新增 `toast.clipboard_fail` 键（i18n 双语完备性单测绿），`r07_clipboard_worker_roundtrip` 绿，workspace 530/530 全绿）；**V-12 真机走查完成（2026-10-01，Win11 25H2 build 26200.9457，debug 构建 @ 0258eb1）**：① 基线复制（calc 查询 `7-3` → Enter → `host/set_clipboard`）——日志 `入队：3 字节 → 成功`，剪贴板内容实测 `= 4`；② 占用器（独立进程持隐藏窗口句柄 `OpenClipboard` 不释放——`OpenClipboard(NULL)` 在本机 200 次尝试恒失败，改真实窗口句柄后即成功；其他进程开启被阻塞实证）持有剪贴板 → calc 查询 `4-1` → Enter → 日志 `入队 → 失败：The native clipboard is not accessible due to being held by another party.`，**失败 toast 可见**（「扩展 com.ddrun.calc 剪贴板写入失败：…」{e} 含原因摘要，截图留档）；③ **核心判据：占用期间工作线程阻塞时 UI 零冻结**——6 次显隐迁移 avg 64.2 ms / max 105 ms（与无占用基线 66 ms 无差别）；④ 占用期写入失败未污染剪贴板（释放后内容完好），用户原剪贴板文本已保存还原
- [ ] R-08 es.exe 错误截断 char-boundary 修复（`chars().take(200)`）——代码 + 单测已落地（2026-09-29：抽 `truncate_chars` 纯函数，`r08_error_truncate_multibyte_safe` 绿，workspace 525/525 全绿）；es.exe 出错路径冒烟随批一收尾。**注记（2026-10-01）**：现场冒烟中止——本机 Everything 运行中（IPC 主通道健康），es.exe 错误分支现场不可达；文件搜索页因会话内合成输入受限（UIPI 拦截 SendInput + winit 不消费投递 WM_CHAR / 修饰键）未能到达，用户回场后为免干扰实时会话主动停止注入。错误路径以 `r08` 单测为准，现场 sidecar 链路冒烟留待后续真机会话补做
- [x] R-09 sys 测试改合成夹具 + thread_local clippy 一行修（裸机测试基线转绿）——2026-09-29：`r09_apps_enum_from_fixture`（COM 现生成根级/子目录 `.lnk` 夹具，枚举含夹具项）；原断言更名 `machine_steam_installed_shown_uninstalled_filtered_root_lnk_shown` + `#[ignore]`（本机复验确红：无 Flowframes，与 §2.2 基线一致）；枚举递归最小参数化为 `collect_lnk_fallback_from_root(root, …)`（逐字搬移，生产行为不变）；thread_local 告警为 clippy 1.96 误报（已是 `const {}` 初始化，三写法均触发）→ 定点 `#[allow]` 留痕；clippy 全仓清零；裸机 `cargo test --workspace` 连续两次 **526/526 全绿**
- [x] R-25 panic 取证落盘：`panic::set_hook` 写 `%APPDATA%\dd-run\logs\panic.log` + 格式化纯函数单测（V-16）——代码 + 单测已落地（2026-09-29：新增 `dd-gui::crashlog`，`take_hook` 保留默认流程，hook 内失败静默放弃；`manifest::logs_dir()` 复用 `%APPDATA%\dd-run` 口径；r25 单测 3 条绿；chrono 复用依赖树既有包，Cargo.lock 仅 +1 行依赖声明、包集合零变化）；**V-16 真机走查完成（2026-10-01，Win11 25H2 build 26200.9457，debug 构建 @ `50d77c6` + 临时探针）**：① 注入 panic——`main.rs` 挂临时环境变量门控探针（`DDRUN_V16_PROBE=1` 时后台线程 `v16-probe` 启动 4 s 后 panic；走查后移除，工作树零残留），`logs\panic.log` 恰新增一条 `[2026-10-01T10:46:52.808+08:00] thread "v16-probe" panicked at crates\dd-gui\src\main.rs:62:17:` + message——时间戳（RFC3339 本地时区毫秒）/ message / location 三要素齐备（另有线程名）；② **回归默认流程不变**——panic 落在后台线程：进程同 PID 存活继续运行（ui 循环日志持续），stderr 同步出现默认 hook 输出（`thread 'v16-probe' panicked …` + `RUST_BACKTRACE` 提示）= 落盘后标准 panic 流程未被替换；③ **正常退出路径零新增**——移除探针重建（`git status` 零残留）后干净实例跑 12 s（冷启动完成 885 ms）经枚举顶层窗口 `PostMessageW(WM_CLOSE)` 优雅退出（进程消失），`panic.log` 保持 2 行（仅探针条目）不变。环境注记：winit/eframe 0.36 事件循环不消费线程级 `PostThreadMessageW(WM_QUIT)`（投递返回 True 但不退出），优雅退出改用 `WM_CLOSE`（CloseRequested → `run_native` 正常返回），与托盘「退出」同汇于正常关机链路

**批二 安全补强**

- [x] R-10 shell 多段命令确认门：`&&`/`&`/`|`/换行切段逐段判定（≥4 单测）——2026-09-29：新增 `is_dangerous_query`（字符级切段，`&&` 产生的空段判定恒 false；引号内分隔符也切段属保守方向），`shell.run.query` 确认门接入；r10 六条单测（4 正例各含「首段无害次段危险」+ 2 负例不误报）全绿，S-06 既有单段判定测试零改动全绿；workspace 536/536 全绿
- [x] R-11 `.url` 读盘限幅：metadata 前置 + 64 KiB 上限（两处）——2026-09-29：抽共享 `read_url_limited`（metadata 非普通文件或 >64 KiB → `None`），`url_target` / `url_icon_file` 两处接入；r11 三条单测（超限拒含合法 URL= 行 / 正常小文件照常解析 / 目录拒）全绿，既有 `url_target_parses_ascii_and_utf16` 回归零改动；workspace 541/541 全绿
- [ ] R-12 首方 sidecar 首跑钉扎：三态单测（首钉/同版篡改/升版重钉；V-9）——代码 + 单测已落地（2026-09-29：`assess` 白名单短路废除（S-05 既有测试同步更名锚定新语义），新增 `assess_sidecar(_with_ledger)`（可注入台账，不触真实 trust.json）+ `TrustEntry.origin/pinned_host_version` 字段（serde default，兼容旧台账）+ `Assessment.sidecar_tampered` 告警位；`TrustLedger::load()` 后聚合与 spawn 双门路由；设置页 `set.ext.tamper_warn` 告警行（zh/en 完备性绿）；r12 四条单测（三态 + Deny 优先不被钉扎覆盖）绿；workspace 546/546 全绿）；V-9 真机走查（同版替换 / 正常升级两场景）待执行
- [x] R-13 calc 递归深度上限 256（`EvalError::Domain`）——2026-09-29：`Parser` 加 `depth` 字段 + `enter()` 守卫（括号嵌套与一元符号链各计一层，**实现期发现 `parse_unary` 的 `'+'/'-'` 自递归同样是源无界向量**，一并纳入）；`r13_depth_over_limit_rejected`（257 层括号 / 257 个负号 → Domain）/ `r13_depth_at_limit_ok`（256 层内正常求值 + 括号符号混合合法用例）绿，calc 既有用例零改动全绿；workspace 538/538 全绿
- [x] R-14 清单 / config.json 读盘限幅 1 MiB——2026-09-29：`load_manifest` 读入前 metadata 检查（超限记 `ParseError` 跳过）；`Settings::load` 最小参数化为 `load_from(path)` 并接入限幅（超限记日志回落默认）；r14 两条单测（超限清单跳过且同目录其余扩展正常加载 / 超限 config 回默认 + 限内正常解析）绿；workspace 543/543 全绿

**批三 可用性与反馈**

- [x] R-15 启动热键失败可见：错误 toast + 热键卡「未注册」徽标 + GetMessageW 死亡同口径（V-3）——2026-09-29：`HotkeyEvent::Died` 变体（`GetMessageW` 返回 -1 线程退出即发送，原仅 `log::debug`）；`PaletteApp.hotkey_unregistered` 状态位（`ReRegistered(false)` 且 `hotkey_prev=None` 的**启动失败分支** / `Died` 置位 + `show_error_toast`，`ReRegistered(true)` 复位）；热键卡标题行「未注册」danger 徽标（失败渲染口径，与扩展卡 shadow_warn 同款）；i18n 2 键 `toast.hotkey_unregistered` / `set.hotkey.unregistered`（zh/en，完备性单测自动覆盖）；`r15_hotkey_failed_state_flags`（三段流转各一断言 + toast 存在性）绿；`test_support::make_app_with` 夹具（注入热键事件通道，不触真实 `RegisterHotKey`）；正常注册路径零新增 toast（不误报）；**V-3 真机走查完成（2026-09-30，本机无 PowerToys，以 `RegisterHotKey` 助手进程等价占用）**：① 启动冲突——助手占 Ctrl+Space 后启动，日志见「全局热键注册失败（2+32）」，托盘唤起面板见错误 toast「全局热键注册失败，可能被其他程序占用——请到设置更换组合键」+ 热键卡红色「未注册」徽标；② 正常路径零误报——干净启动 Ctrl+Space 开面板无任何 toast；③ 复位语义顺带验证——重注册成功后徽标消失；④ 9-30 seq 确认协议冲突录入——助手于候选录入后注册同键（Ctrl+Alt+J），连续两次保存均行内报错「组合键已被其他程序占用」+ 日志 seq=3/4 双失败回滚、对话框保持、设置零写入（「冲突只拦截一次、第二次假成功」根治实证）。环境记录：本机 LL 键盘钩子被安全软件拦截（自回声验证 `Failed`）→ 捕获回落 egui 应用层且回落 toast/红色提示可见；被 `RegisterHotKey` 占用的组合键对 egui 捕获层不可见，故「先占用再录入」在本机不可行，采用「录入后占用再保存」等价场景；Ctrl+Space 在回落模式下被微软拼音 IME 层吃键（9-29 定性复证）
- [ ] R-16 `open_url` 失败 toast：`toast.open_fail` 键 zh/en 同批（V-4）——代码已落地（2026-09-30：`host_actions.rs` 两处失败路径接入 `show_error_toast`（`file://` ShellExecute 失败 log 由 debug 升 warn + toast；`webbrowser::open` 失败同口径），新增 i18n 键 `toast.open_fail`（zh/en，完备性单测自动覆盖）；回归门执行时发现并修复 R-12 遗留的本机环境耦合必红——`spawn_gate_passes_first_party_sidecar` 依赖真实 trust.json 钉扎状态 × 测试假路径 fail-closed 判 Pending，将 spawn 门禁信任判定抽为可注入（`spawn_and_initialize_with` + `assess_for_spawn` / `uses_sidecar_assessment`），测试密闭化不触真实台账并新增路由单测 `spawn_gate_routes_first_party_sidecar_to_sidecar_assess`；workspace **559/559 全绿**，fmt 零差异、clippy 零告警）；V-4 真机走查（无默认浏览器 / 断网两失败源）待执行
- [x] R-17 设置保存失败一次性 toast（与 R-02 同落点；V-2）——代码已落地（2026-09-30：`Settings::save` 改返回持久化成败（可注入核心 `save_to`，r17 单测覆盖可写 `true` / 独占锁注入失败 `false` 两态）；全仓 22 处 `settings.save()` 调用点统一改走 `save_settings_with_feedback`（PaletteApp `report_settings_save_failure` 可注入核心），失败且未提醒过 → 置位 + 错误 toast，**每会话至多一次**、成功路径静默；新增 i18n 键 `toast.settings_save_fail`（zh/en，含「本次修改可能未保存」口径，完备性单测自动覆盖）；r17 两条单测绿，workspace **561/561 全绿**，fmt 零差异、clippy 零告警）；**V-2 真机走查完成（2026-10-01，V-2 同场）**：`config.json` 置只读（`attrib +R`，先以 MoveFileExW 独立实验证实 rename-over-readonly 失败机制成立）→ 变尺寸后隐藏触发保存失败 → **错误 toast「设置保存失败——本次修改可能未保存，请检查配置目录是否可写」可见**（toast 3000ms TTL 内截图留档）；同态 3.3 s 后 toast 过期消失 → **第二次失败保存无新 toast**（每会话恰一次，截图对照）；`attrib -R` 恢复可写 → 再次保存**成功且静默**（panel_size 恢复落盘），无 toast；现场注记：失败经由隐藏路径的 `persist_panel_size → save_settings_with_feedback`（与设置页开关同一反馈链路），时序缺陷首跑发现（两次失败间隔 < toast TTL，残影误判「重复弹」）——补跑拉开 >3 s 间隔后口径通过
- [x] R-18 热键 toast 入 i18n：`toast.hotkey_updated` / `toast.hotkey_failed`——2026-09-30：**核心改动随 seq 确认协议批（`1a20c8f`）先行落地**（两处调用点改走 `t(self.lang_effective, …)`、成功 toast 带动态 `{combo}` 组合名、zh 文案允许超集扩展语义不变），本批收口：全仓 `show_toast/show_error_toast` 调用点复核**零硬编码中文字符串残留**；新增 r18 锚定单测（两键 zh/en 非空 + zh/en 关键短语语义锚定，防漂移），既有 `i18n_table_complete_both_langs` 双向完备性覆盖在表；workspace **562/562 全绿**，fmt 零差异、clippy 零告警；en 语言下两条热键 toast 的真机确认随 V-3 en 复跑待执行
- [ ] R-19 托盘热键标签动态化：tooltip 与菜单尾缀随改绑（V-5）——代码已落地（2026-09-30：`tray.rs` 静态 `TOOLTIP` 改为共享标签 `TRAY_HOTKEY_LABEL`（`Mutex<String>`，语义同 `TRAY_LANG`）+ 纯函数 `tray_tooltip`；NIM_ADD 取当前值、`show_menu` 每次右键托盘线程本机 NIM_MODIFY 懒刷新（无跨线程推送，D38 结论维持）；菜单 `tray.toggle` 键去掉内嵌 `\tWin+Alt+Space` 尾缀、`show_menu` 动态拼接当前组合名；`settings.rs` 新增 `hotkey_combo_label` 纯函数（复用既有 mods/vk label）；主线程启动（main.rs，托盘 spawn 前）与改绑注册成功（`poll_hotkey` 唯一写点，save/reset 共用）经 `sync_tray_hotkey_label` 同步；r19 单测（默认组合 = D38 静态口径回归锚 / 自定义组合进入 tooltip / 空串回落默认 / szTip 128 单元上限守卫含最长组合）绿，workspace **563/563 全绿**，fmt 零差异、clippy 零告警）；V-5 真机走查（改绑非默认组合 → tooltip / 菜单尾缀新组合；重启后 + 右键即时两路径）待执行
- [ ] R-20 扩展卡 skipped 警告行（路径 + 原因，复用 Failed 行样式；V-6）——代码已落地（2026-09-30：`merge_sidecar_scan` 参数化 sidecar 目录并收集 `outcome.skipped`（**仅 `SkipReason::is_error()` 条目**——`OtherPlatform` 类设计上静默的跳过不告警，Display 直出宿主校验原因）；`ExtensionSources.skipped` → `AggregatePayload.skipped` → `PaletteApp.skipped_manifests` → 扩展卡顶部警告行（`set.ext.skipped_warn` 键 zh/en 框架双语，{reason} 为宿主侧 zh 口径；danger 色 11px + 截断 + 悬停全文，复用 Failed 行样式；只读、不可 Retry）；页脚 note 语义零改动（2026-09-04 用户决策）；`r20_skipped_aggregated_with_reason`（语法错误清单 → skipped 恰含（路径, 原因）/ 全合法目录 → 空，正负例）绿，workspace **564/564 全绿**，fmt 零差异、clippy 零告警）；V-6 真机走查（放置 JSON 语法错误清单 → 扩展卡顶部警告行）待执行
- [ ] R-21 热键危险组合确认提示（Alt+Space 等黑名单；V-8）——代码已落地（2026-09-30：警示口径（黄线不阻止、「保存」即确认）已随捕获对话框批先行落地，本批补全**黑名单广度**：`is_system_reserved_combo` 由仅 Alt+Space 扩为方案四组合（Alt+Space 窗口菜单 / Ctrl+Esc 开始菜单 / Ctrl+Shift+Esc 任务管理器 / Alt+F4 关闭窗口）；`set.hotkey.warn_reserved` 文案泛化为「系统常用快捷键……可能被系统优先响应」；`r21_hotkey_blacklist_matches`（四组合逐一命中 + Win+D / 默认热键 Ctrl+Space / Shift+Esc / Ctrl+Alt+P / Alt+E 不误报）绿，workspace **565/565 全绿**，fmt 零差异、clippy 零告警）；V-8 真机走查（捕获 Alt+Space / Ctrl+Shift+Esc → 黄线警示出现 → 仍保存 → 注册成功）待执行
- [ ] R-22 IME 组词期 Enter 守卫（V-7）——代码已落地（2026-09-30：`handle_keys` 键消费后加 `ime_composing` 帧判定（本帧事件表含 `Event::Ime` 的 **Preedit / Commit** 组词系事件），命中则 `enter &&= false`——Enter 已被消费（吞掉，上屏文本经 Commit 事件入框与键事件无关），激活跳过一次、下一帧恢复；确认对话框与右键菜单的 Enter 分支同帧同口径被守卫；r22 单测（同帧 Preedit+Enter 不激活且 Enter 被吞 / 同帧 Commit+Enter 同口径 / 无 Ime 帧 Enter 正常激活回归锚 / 下一帧恢复，Page 命令项栈深断言不依赖扩展进程；headless 帧边界以清空事件表模拟）绿，workspace **566/566 全绿**，fmt 零差异、clippy 零告警）；V-7 真机走查（微软拼音 / 搜狗组词中回车：仅上屏不触发选中；非组词回车行为不变）待执行
- [ ] R-23 查询 clamp 256 字符（`set_query` 入口）——代码已落地（2026-09-30：`state.rs` `PanelState::MAX_QUERY_CHARS = 256`（**字符数**），`set_query` 入口静默截断并返回是否截断（早退语义不变）；`panel.rs` 截断时经 `TextEdit::load_state/store_state` + `set_char_range` 把输入框光标置尾；r23 单测（恰 256 不截断 / 257 与 300 截为前 256 / CJK 按字符数截无半字符（300 汉字 → 256 字符 768 字节）/ 同值重喂早退返回 false / 空串恢复）绿，workspace **567/567 全绿**，fmt 零差异、clippy 零告警）；真机冒烟（粘贴一次超长文本，文件搜索页不向 es.exe 传离谱参数）随批三收尾执行
- [x] R-24 sidecar 过期 `f ` 前缀自述更正——2026-09-30：`dd-ext/src/bin/search.rs` 两处文案更正为 Ctrl+F 口径（spec.description zh「基于 Everything 的本地文件搜索（Ctrl+F 一键直达）」/ en「(Ctrl+F to jump in)」；top_level 副标题 zh「（或按 Ctrl+F 直达）」/ en「(or press Ctrl+F to jump in)」），与 search.md §2 一致；双语文案提为四条常量（`EXT_DESC_*` / `TOP_LEVEL_SUBTITLE_*`）供单测双语锚定；`r24_sidecar_description_matches_spec`（zh/en 四条常量断言不含 `f ` 前缀字样、含 Ctrl+F 关键短语 + spec.description / top_level 副标题与常量同源防内联漂移）绿；无独立真机 V 项（该字段当前未上屏），文案随本批代码评审核对。**门禁注记**：fmt 零差异、clippy 零告警、`r24` 单测及 dd-ext-search 54 条全绿；全量 `cargo test --workspace` 复核被 `Os error 231` 环境批（第二次，>1h 未自愈，spawn 依赖用例挂起/失败，三口径判定与本次改动无关，见 §10 版本演进 v1.5）暂时阻塞——分目标实测 protocol 30 / websearch 4 / dd-host lib 73 / dd-ext lib 118 / dd-gui lib 260（除 6 条 231 环境批）全绿，**2026-10-01 已补跑确认：全量 `cargo test --workspace` 568/568 全绿（21 目标，0 失败，环境批自愈）**

**批四 资源与挂死（改造型）**

- [x] R-03 入站队列定容 + 截断提示（帧队列 128 / host 请求 32；V-10）——2026-10-01：dd-host `process.rs` 读线程改经 `InboundGate`（`mpsc::sync_channel(128)` + `try_send`，**满则丢新帧**并置位 `overflown` / 累计 `dropped`，不阻塞读线程避免反压子进程 stdout 管道）；`ExtensionProcess::take_inbound_overflow()`（episode 语义：观察即复位）由 dd-gui `refresh::poll_notifications` 每帧观察——dd-host `log::warn!` + UI 错误 toast `toast.ext_overflow`（zh/en，完备性单测自动覆盖），**每扩展每会话至多一次**（`PaletteApp::overflow_warned`，与 R-17 `settings_save_warned` 同口径）；`host_requests` 经参数化 `push_capped`（cap 32）drop-oldest + 溢出记日志；in-process 适配器同口径定容（host_requests 32；顺带补齐 notifications 的 `DIAGNOSTIC_BUS_CAP` 截断，与子进程路径 M9 R1 终态等价对齐）；r03 三条单测（>128 帧队列恒 128 + 标志置位 + episode 复位 / 恰 128 不置位负例 / >32 条保留最新 32 条最早丢弃）绿；workspace **571/571 全绿**（+3），fmt 零差异、clippy 零告警；**V-10 真机走查完成（2026-10-02，debug 构建，Win11 25H2 build 26200.9457）**：① **注入规模与对账**——flood 扩展（用户目录 `com.example.flood`，trust.json 预置 allow 双哈希）完成握手后以 **128 KiB/帧** 合法通知帧刷 stdout，扩展侧仪表日志实测共写 **16,000 帧（≈2.0 GB）@ 128 fps**（flood 结束 t≈121 s，先于空闲回收 t≈135 s 判定）；② **RSS 收敛于队列上界且不再增长**——隐藏期 1 s 粒度采样（t≈4–97 s 共 94 点）：队列填满（128 帧 × 128 KiB = 16 MiB）后 PRIV 恒 **106.4–109.0 MB**（波动 ≤2.6 MB、无单调增长），期间 ≥12,000 帧被丢弃而注入持续；同日 640 KiB 帧预跑（同构建）测得完整涨落周期——队列满 PRIV 173.4 MB → warm 空闲回收释放后 89.1 MB（**Δ84.3 MB ≈ 128 × 640 KiB + 分配器余量**），直接标定「队列内容 = 容量 × 单帧上界」；③ **恢复可见恰一次溢出告警**——t=97 s（已注入 ≈12,150 帧 ≥10,000）`WM_HOTKEY` 唤起 → dd-host warn「本 episode 丢弃 **12016** 帧」+ 错误 toast「扩展 com.example.flood 输出过快，已丢弃 12016 条消息（面板隐藏期间积压溢出）」（截图留档 `%TEMP%\ddrun-v10c\v10c-toast-*.png`）；④ **每扩展每会话至多一次**——再次隐藏 15 s 残余注入（≈3,850 帧）再溢出（episode 2 丢弃 1,807 帧，dd-host 逐 episode 留痕）→ 二次唤起**无新 toast**（全日志「toast 告警」行恰 1 条；截图 `%TEMP%\ddrun-v10c\v10c-notoast-*.png`）；⑤ 回归——告警后 3 次显隐切换正常、config.json 逐字节未变、panic.log 零新增。**走查环境注记**：(a) 帧尺寸取 128 KiB 而非判据示例的 1 MiB——`Decoder::push` 的 S-02 增量缓冲每 push 从头重扫找 `\n`，debug 读路径单帧成本 O(F²)（1 MiB 级帧实测 ≈7–17 fps，10,000 帧需 10+ 分钟，超出 warm 池 120 s 空闲回收租期；128 KiB 帧实测 128 fps）；队列上界口径不变（上界 = 容量 × 单帧上界，本跑 = 16 MiB，机制与 ≤128 MiB 示例同源）；(b) 隐藏期注入窗口因此以「单次 warm 租期 ≤120 s」为界——R-03 与 C 批空闲回收构成双层防护（回收本身即清空队列，实测 PRIV 173.4→89.1 MB）；(c) 注入/采样/取证脚本与 flood 扩展均走查后弃置（`%TEMP%\ddrun-v10c\`，仓库零残留）
- [x] R-04 in-process 调用超时 + apps 负缓存（V-11）——2026-10-01：`ext_client.rs` 的 `InProcess` 变体改持 `Option<InProcessExtension>` + `timeout`（可注入，单测 50 ms）；全部协议方法（initialize / top_level / fallback / get_command / get_items / invoke）统一经 `run_with_timeout`（扩展对象移入工作线程 + `recv_timeout`）包装——**按时归还放回槽位；超时返回协议层 `Timeout`，扩展对象随线程自然滞留**（不 kill，受 OS 网络超时上界约束），后续调用答滞留不可用错误；panic 隔离面不变（M9 `catch_unwind` 在扩展侧先行接住，线程消失分支仅为理论兜底）；滞留客户端经 `store_warm_process` **拒绝归回** warm 集（invoke / page / fallback 三条归还链路共用此单点），`mark_source_failed` 把该扩展本会话标 Failed（扩展卡错误行 + Retry，重试经重聚合重建全新客户端）；apps 枚举（`builtins/apps.rs`）抽 `enumerate_apps` + `negative_cached`（枚举体 panic 就地接住 → 空列表随 `OnceLock` 落**负缓存**，二次聚合不再重付全量枚举）；r04 两条单测（假慢扩展 300 ms × 50 ms 超时 → `Timeout` + 滞留 → inflight 复位 + Failed；枚举 panic → 负缓存命中计数恰 1）绿；workspace **573/573 全绿**（+2），fmt 零差异、clippy 零告警；真机冒烟五内置扩展全 warm、冷启动 867 ms 与基线一致；**V-11 真机走查完成（2026-10-01，debug 构建，走查注记三则见下）**：① **T 内超时返回 + 首屏降级落地**——放置指向死 UNC 的 `.lnk`（目标 `\\203.0.113.43\share\dead.exe`，RFC 5737 TEST-NET 保证静默丢包，metadata 实测挂 ~21 s）后启动 → `冷启动完成：5073 ms`（基线 867–990 ms）= apps 首调恰在 T=5 s 超时按期返回（5073 − 5000 = 聚合其余开销），非永久「加载中」；② **apps Failed、其余不受影响**——日志「应用 warm」消失（计算器/系统/网络搜索/Shell 四内置照常 warm，挂死滞留线程不影响聚合 scope 其余线程），Failed 状态经 `ExtOutcome::Failed → flatten → SourceStatus::Failed` 落定（扩展卡 Failed + Retry 渲染为既有已测路径；设置页卡片现场因会话合成输入受限未视觉复核，注记口径同 R-08）；③ 收尾复验——删除 `.lnk` 后冷启动恢复 990 ms、应用 warm 115 命令；全程 panic.log 零新增。**走查环境注记**：(a) 命令行转义经 WScript.Shell 造 .lnk 会把 UNC 存坏（读回 `C:\192.0.2.1`），须脚本文件内干净构造；(b) 本机 SMB 对不可达主机 metadata 挂 ~21 s（raw TCP 同），且**失败按目标地址短期缓存**（同目标二查 5 ms）——探测与 .lnk 目标须用不同 IP；(c) 超时后滞留线程受 OS 网络超时上界自然结束，符合 R-04「不 kill」设计
- [x] R-26 单实例互斥：`CreateMutexW` + 已运行 `MessageBoxW` 提示退出（V-15）——2026-10-01：`main.rs` 启动早期（O4 init 后、eframe 前）`enforce_single_instance()`——`CreateMutexW`（`Local\dd-run-single-instance`，本登录会话命名空间与 per-user 数据目录口径一致），`ERROR_ALREADY_EXISTS` → 双语 `MessageBoxW`（MB_OK|MB_ICONWARNING|MB_SETFOREGROUND，此时 GUI 语言尚未解析故中英并列）后 `process::exit(0)`；**句柄有意不 CloseHandle**（互斥体随进程退出由内核释放，提前关闭会误判「无实例」，函数文档即代码评审口径的命名 + 句柄语义留痕）；创建失败降级不互斥照常启动（护栏不成新单点）；非 Windows 占位 noop；零新依赖零特性变更（windows-sys 既有 Foundation / Threading / WindowsAndMessaging）；**V-15 真机走查完成（2026-10-01，debug 构建）**：① 双开——实例 A（PID 14416）运行中启动实例 B → B 弹「dd-run 已在运行（请用托盘图标或热键唤起面板）…」双语警告弹窗（窗口类 #32770、标题 dd-run，截图留档 `%TEMP%\dd-run-v15-dialog.png`）→ 程序化点「确定」后 B 退出，B 日志「已有 dd-run 实例在运行（单实例互斥命中），弹窗提示后退出」；② 原实例三面无损——A 进程存活、config.json SHA-256 双开前后逐位一致（互斥拦截先于 `Settings::load`，B 未触达设置）、热键链路 alive（`WM_HOTKEY` 注入 2 Toggle → 日志唤起居中 (635,250) → hide 完整一循环，与 V-1 基线位置一致）、panic.log 零新增；③ 附带开机自启复启口径——Run 键复启 = 再次执行 dd-run.exe，与本走查第二实例路径相同（命中互斥弹窗退出），不实际重启验证；workspace 573/573 全绿、fmt 零差异、clippy 零告警
- [ ] V-13 内存基线记录并回写 implementation.md §3.1 台账（批四收尾动作）

### 6.5 验收通用口径（v1.4 增补）

§3–§5 各项「验收」统一按**单测 / 真机 / 回归**三口径书写；本节定义三者共用的执行与记录规则，避免逐项重复。任一项只有三口径全部满足（或按注明的评审 / 冒烟替代口径执行并留痕）方可勾选。

| 口径 | 规则 |
|---|---|
| 单测 | 命名 `r<两位编号>_<行为>`（可 grep 定位所属 R 项）；覆盖该项「验收」声明的正例 + 边界 + 负例组合；与既有测试同模块放置，随批次一并合入 |
| 真机 | 仅执行 §6.2 对应 V 项；无独立 V 项的，按该项「验收」注明的冒烟 / 评审替代口径执行并留痕 |
| 回归 | 每项声明**不得改变**的既有行为面；通用回归门 = `cargo fmt --check` 零差异 + `cargo clippy --workspace --all-targets` 零告警 + `cargo test --workspace` 全绿 + 五步冒烟（启动 → 热键唤起 → 查询 → 执行无害命令 → Esc 隐藏） |
| 记录 | 每条已执行的 V 项登记：日期、实测 OS 版本 / 构建号（§2.2 口径）、实测数值；未达标在 §6.4 行内注记，不得勾选 |
| 阈值 | 实施时调整本方案预设数值（如 R-04 超时初值 5 s、V-13 内存阈值、R-03/R-14 限幅值）须在 §10 版本演进留痕 |

---

## 7. 明确不做与缓办

| 事项 | 处置 | 理由 |
|---|---|---|
| 哈希→spawn 毫秒级 TOCTOU（`aggregator.rs` 约 :490/:505） | **记录为已接受残余** | 同用户攻击者、窗口毫秒级；彻底封堵需 `FILE_SHARE_READ` 句柄建进程的 Win32 改造，收益不成比例 |
| 扫描期与 spawn 期 origin 判定口径不一（junction 场景） | **缓办**（前置：任何新增 `spawn_and_initialize_with_info` 调用点时必须先做） | 当前无可达利用路径（Pending 项不进面板、无点击入口）；修复涉及 aggregator 核心携带链 |
| `dd-run-cli` 绕过信任门 spawn（`main.rs` 约 :284、:695） | **缓办**（dev-only 工具） | 仅显式运行 CLI 场景可达，无特权增益；下轮触碰 CLI 时顺手加 `trust::assess` |
| 图标磁盘缓存 / sidecar ICON_CACHE 无淘汰 | **缓办**（观察项） | 条目小、增长慢、可手动清；等真实占用数据再定 |
| framing.rs 逐行重扫 O(n²)（CPU DoS 面） | **缓办** | 需流氓扩展高频小消息才可达，且 R-03 定容后影响面进一步收窄 |
| 设置窗口最小宽 460 偏挤、导航栏窄屏折叠（`main.rs` 约 :76） | **不做**（本轮） | 纯观感，走 D 系列设计渠道另行立项 |
| 会话内 icon_cache 无淘汰（纹理堆积） | **缓办** | 已有「隐藏清空」兜底，触发依赖用户刷出海量唯一图标；V-13 内存基线建立后凭数据复核 |
| CI 门禁（fmt / clippy / test 远程自动化） | **缓办** | 单机开发阶段以 §6.3 批间纪律为门禁；R-09 落地（裸机全绿）是任何 CI 的前置。若后续转远程协作再立项，本行为显式占位、防「门禁只活在本方案生命周期内」 |
| 全量日志文件后端（O4 后端加文件输出 + 轮转） | **缓办** | R-25 已覆盖崩溃取证这一刚性需求；全量落盘引入磁盘增长/轮转问题，等真实排障需求再定 |
| 依赖供应链（`cargo audit` / Cargo.lock 更新策略） | **缓办** | S 系列审计为代码级取证，当前依赖集合小且已锁定。**约定**：每次新增依赖时跑一次 `cargo audit` 并在 CHANGELOG 记录结论 |

---

## 8. 风险声明

- **R-04（改造型）**为唯一改变并发行为的项：超时线程滞留依赖 OS 网络超时上界、apps 负缓存改变重试节奏，故压轴批四并要求注入式真机验证（V-11）；若走查发现不可接受的边角，可独立回滚该单项而不影响其余 23 项。
- **R-12（sidecar 钉扎）**引入「升级后首启静默重钉」的信任假设（信任锚 = 分发包），已在 §4 R-12 与 §7 声明；若不接受该假设，可退化为「同版篡改 → Pending」单独落地（升级后需一次手工批准）。
- **R-26（单实例）**为启动行为变更：双开第二实例从「行为未定义」变为「弹窗提示后退出」；若存在合法多实例诉求（便携多副本并行），可退化为「仅告警不退出」单独调整，不影响其余项。
- 其余 24 项均为局部机械修复或纯增量反馈，不改变既有交互语义。

---

## 9. 与既有文档关系

| 文档 | 关系 |
|---|---|
| [security-audit-2026-09-23.md](./security-audit-2026-09-23.md) | S-01–S-11 **全部不重复**；R-10 是 S-06 的纵深补强（同威胁模型内收口）、R-11 与 S-04 同类补漏、R-12 把 S-05 的「同意 + 变更检测」性质延伸到首方 sidecar |
| [future-features-plan.md](./future-features-plan.md) | N1–N5 **零重叠**（功能向 vs 本文加固向）；N5「trust.json 永不导出」是 R-12 钉扎字段扩展的前提；编号空间互不侵占（N vs R） |
| [optimization-plan.md](./optimization-plan.md) | O4 日志基建是 R-15~R-17「失败可见性」的落点依赖；R-25 在其 stderr 后端之**上**最小扩展（panic 落盘走独立 hook，不改 O4 后端与 `DDRUN_LOG` 语义），延续零依赖基调；R-04 属其「协议健壮性」方向的残余面收口；不触碰其 Phase 2 在途项 |
| [settings-personalization-plan.md](./settings-personalization-plan.md) / [settings-keys-typography-plan.md](./settings-keys-typography-plan.md) | R-15/R-19/R-20 的 UI 复用其已落地的行块/控件/Failed 渲染口径，零排版改动、零 D 系列新决策 |
| [search-file.md](./search-file.md) / [search.md](./search.md) | R-03/R-04/R-24 涉及 sidecar 生命周期与自述文案，**行为契约与技术口径不变**；R-23 的 256 字符上限为其 200ms debounce 链路的前置护栏 |
| [implementation.md](./implementation.md) | 每批完成后回写 §3.1 测试基线台账；本方案不动其 ADR 与里程碑结论 |
| [INDEX.md](./INDEX.md) §4 | 本文 v1.1 修订按其格式规约对齐（状态取值五选一 / 状态标记 ✅⚠️❌🟨 / 行号「约 :NNN」/ 文末版本演进表） |

---

## 10. 版本演进

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.0 | 2026-09-29 | 初稿：三路只读审查 24 项发现 + 四批实施规划 |
| v1.1 | 2026-09-29 | **全量核对**：引用的 file:line 逐一回读源码——更正 2 处子代理误报（R-23 落点 `app/state.rs` 不存在 → 实为 `state.rs` 约 :234；R-06 search.rs 锁点位 6 → 7，补约 :116）；R-05 区分 Windows 实路径与跨平台占位/测试桩；R-01 补 Cargo.lock 实测 egui-winit 0.36.2 取证。**按 [INDEX.md](./INDEX.md) §4 格式规约对齐**：状态「待评审」→「规划中」（五选一）、级别标记改用 ✅⚠️❌🟨、行号改「约 :NNN」口径；增补 §6.1 单测预估、§6.2 真机走查总清单（V-1~V-12）、§9 与既有文档关系 |
| v1.2 | 2026-09-29 | 补 **§6.4 任务清单**（24 项按批 `- [ ]` 勾选，对齐 [INDEX.md](./INDEX.md) §4.2 任务规约）；修复 §1 R-10 行单元格内未转义的 `\|`（GFM 切列隐患，规约「表格内竖线」条）；同步登记 [INDEX.md](./INDEX.md)（v1.35）与全仓格式修复报告 [doc-audit-2026-09-29.md](./doc-audit-2026-09-29.md) |
| v1.3 | 2026-09-29 | **可诊断性与门禁增补**（「崩溃可诊断 + 进程级防护 + 点状修复」补全）：新增 **R-25**（panic hook 写 `logs\panic.log`，全仓复核无 `panic::set_hook`，落批一）与 **R-26**（`CreateMutexW` 单实例互斥，复核无既有实现，落批四）；§2.2 补兼容矩阵（Win10/11 x64）与走查 OS 版本登记要求；§6.2 走查清单扩至 **V-13~V-16**（24 h 挂机内存基线、多显示器/DPI/RDP、双开、panic 注入）；§6.1 批四更名「资源、挂死与进程防护」，diff/单测预估同步（+810/−215、约 32）；§7 新增三条缓办占位（CI 门禁 / 全量文件日志后端 / cargo audit 约定）；§8 补 R-26 行为变更声明；§9 补 R-25 与 O4 关系 |
| v1.4 | 2026-09-29 | **验收标准细化**：§3–§5 全部 26 项「验收」改写为**单测（`r<编号>_` 命名 + 断言内容）/ 真机（V 项 + 特有观测点）/ 回归（不得改变的行为面）**三口径，逐条可判定通过与否；核对补上 R-24 缺失的验收条目（原方案仅 R-24 无验收）；§6.2 收紧 V-1 / V-2 / V-10 / V-11 判据为可量化口径（循环 ≥50 次、队列上界 ≤128 MiB、超时阈值初值 5 s）；新增 **§6.5 验收通用口径**（单测命名 / 回归门操作化定义 / 记录 / 阈值变更纪律）；单测预估校准 32→36（批一 9 / 批二 16）；代码锚点复核（chrome.rs :133/:177、settings.rs :944、trust.rs :232、cache.rs :79 均一致）；同步 [INDEX.md](./INDEX.md)（v1.37） |
| v1.5 | 2026-09-30 | **R-24 落地 + 门禁环境批留档**：批三最后一项 R-24 完成（§6.4 勾选，含门禁注记）；`Os error 231` 环境批**第二次**（spawn 依赖用例挂起/失败 >1 h，PowerShell 通道同证系统级，三口径判定与本批改动无关）——R-24 分目标门禁全绿、全量 568/568 待环境自愈后补跑确认 |
| v1.6 | 2026-10-01 | **批一真机走查首批收口**：R-01 **V-1 完成**（普通唤起-隐藏 30 次 + 最小化态驻留/最小化态唤起 20 次全程存活，`panic.log` 零新增，基线/终态唤起位置逐位一致；环境注记：本会话 SendInput 被 UIPI 拦截 → 以 `PostThreadMessageW(WM_HOTKEY)` 同源注入替代，真实键盘链路由 9-30 V-3 侧证）；批一收尾冒烟完成（R-05 启动三线程正常起跳 / R-06 锁正常读写路径零异常）；**R-24 遗留门禁收口**：全量 `cargo test --workspace` 568/568 补跑全绿（环境批自愈）；R-08 注记（Everything IPC 健康致错误分支现场不可达 + 用户回场后主动停止注入，以 r08 单测为准）；文档头状态「规划中」→「实施中」 |
| v1.7 | 2026-10-01 | **R-02 + R-17 V-2 完成（批一走查第二项）**：① 20 次「MoveWindow 变尺寸 → 隐藏」同链路原子写（每次内容各异，rename 替换既有文件）零 `.tmp` 残留；② 强杀重启后 config SHA-256 逐字节一致；③ 20 次唤起-隐藏延迟均值 66 ms / 峰值 424 ms 无感知卡顿；④ R-17 口径——config 置只读 → 失败 toast 可见（3000ms）→ 过期后第二次失败**无新 toast**（恰一次）→ 恢复可写保存成功且静默；终态 config 与基线逐值一致（panel_size 还原 null）。环境注记：本会话合成鼠标（hover 像素级对照证实被 winit/egui 忽略）与修饰键（GetAsyncKeyState 真实键态）双重不可用 → 设置页开关无法触达，以同链路 `persist_panel_size` 变尺寸落盘替代（atomic_write 成败反馈与设置页开关同源），设置页开关路径由 r02/r17 单测覆盖；首跑时序缺陷（两次失败间隔 < toast TTL 残影误判）发现后补跑正确时序收口 |
| v1.8 | 2026-10-01 | **R-07 V-12 完成（批一走查第三项）**：① 基线复制（calc `7-3` → Enter）日志 `入队 → 成功`、剪贴板实测 `= 4`；② 占用器（独立进程持**真实隐藏窗口句柄** OpenClipboard 不释放；`OpenClipboard(NULL)` 本机恒失败为环境限制非产品问题）→ calc `4-1` → 日志 `入队 → 失败`，**失败 toast 可见**（{e} 含 arboard 原因摘要）；③ **核心判据**：占用期工作线程阻塞时 UI 零冻结——6 次显隐迁移 avg 64.2 ms / max 105 ms（与无占用 66 ms 无差别）；④ 占用期写入失败未污染剪贴板、用户剪贴板文本保存还原。走查时序教训两则：查询 `/` 字符 0 命中（A3 过滤语义）改用 `-`；失败 toast 截图须在到达（~T+1s）与过期（T+4s）窗口内 |
| v1.9 | 2026-10-01 | **R-25 V-16 完成（批一走查第四项，R-25 三口径收口）**：临时环境变量门控探针（`DDRUN_V16_PROBE=1`，后台线程 `v16-probe` 4 s 后 panic；走查后移除零残留）注入一次 panic——`logs\panic.log` 恰新增一条且时间戳（RFC3339 +08:00 毫秒）/ message / location（`main.rs:62:17`）齐备；回归双证——后台线程 panic 后进程同 PID 存活、stderr 出现默认 hook 输出（标准 panic 流程未变），移除探针重建后干净实例 12 s 运行 + `WM_CLOSE` 优雅退出 `panic.log` 零新增（正常退出路径不写条目）。环境注记：winit/eframe 0.36 不消费线程级 `WM_QUIT`，优雅退出以枚举窗口 `WM_CLOSE` 达成；文档头版本补同步（v1.6→v1.9，v1.7/v1.8 两轮头部漏更正） |
| v1.10 | 2026-10-01 | **批四动工：R-03 落地**（入站队列定容 + 截断提示）：子进程读线程 `InboundGate`（`sync_channel(128)` + `try_send` 丢新帧 + `overflown`/`dropped` 原子量）——面板隐藏期间流氓扩展刷 stdout 不再无界增长（RSS 上界 ≈128 帧 × 1 MiB，不阻塞读线程避免反压管道）；`host_requests` drop-oldest 定容 32（`push_capped` 参数化）；溢出告警 = dd-host `log::warn` + UI 错误 toast（每扩展每会话一次）；in-process 适配器同口径（host_requests 32 + notifications 64 截断补齐）。r03 ×3 单测、workspace **571/571 全绿**（+3）、fmt/clippy 零差异零告警；V-10 待执行。环境注记：本会话裸测 GNU 工具链 `dlltool.exe` 不在 PATH（rustup 工具链 self-contained 目录内含，补 PATH 后门禁正常） |
| v1.11 | 2026-10-01 | **R-04 落地**（in-process 调用超时 + apps 负缓存）：`ExtClient::InProcess` 改持 `Option<InProcessExtension>` + 可注入 `timeout`（初值 5 s），全部协议方法统一 `run_with_timeout`（扩展对象移入工作线程 + `recv_timeout`）——超时返回协议 `Timeout`、对象随线程**自然滞留**（不 kill）；滞留客户端经 `store_warm_process` 单点拒绝归回并 `mark_source_failed`（会话级 Failed + Retry）；apps 枚举 `negative_cached`（panic → 空列表落负缓存，二次聚合不重付）。r04 ×2 单测、workspace **573/573 全绿**（+2）、fmt/clippy 零差异零告警；真机冒烟五内置扩展全 warm（冷启动 867 ms 与基线一致）；V-11 待执行。设计注记：阈值随方案初值取**统一 5 s**（内置 in-process 调用均无真实网络 I/O——websearch 仅产 URL 模板、由宿主开 URL），未按子进程 §10 每方法差异化 |
| v1.12 | 2026-10-01 | **R-26 落地 + V-15 完成（批四代码项收口）**：单实例互斥 `Local\dd-run-single-instance`（启动早期，O4 后 eframe 前）——双开第二实例双语弹窗（截图留档）后退出，原实例三面无损（config SHA-256 逐位一致 / `WM_HOTKEY` 注入 Toggle 完整显隐循环 (635,250) / panic.log 零新增）；句柄不 CloseHandle 语义与命名口径留痕函数文档（单测不可达 → 代码评审口径）；创建失败降级不互斥；零新依赖零特性变更。**批四代码项（R-03/R-04/R-26）全部落地**，余 V-10/V-11 真机走查与 V-13 内存基线（批四收尾动作）待执行 |
| v1.13 | 2026-10-01 | **R-04 V-11 完成（批四走查第一项）**：死 UNC `.lnk`（`\\203.0.113.43`，RFC 5737 黑洞 + SMB 负缓存规避：探测/目标用不同 IP）→ 冷启动 5073 ms = T=5 s 按期超时（基线 867–990 ms），首屏降级落地（四内置 warm 不变、应用 Failed）；删除 `.lnk` 后恢复 990 ms。环境注记：WScript.Shell 命令行转义会存坏 UNC（须脚本文件构造）；SMB metadata 挂 ~21 s 且失败按目标缓存；设置页 Failed 卡片未视觉复核（合成输入受限，注记口径同 R-08）。R-04 至此三口径收口（单测 + V-11 + 回归） |
| v1.14 | 2026-10-02 | **R-03 V-10 完成（批四走查第二项）**：flood 扩展隐藏期注入 16,000 帧（128 KiB/帧 ≈2.0 GB @ 128 fps，扩展侧仪表对账）——队列满后 RSS 平台（PRIV 106.4–109.0 MB 波动 ≤2.6 MB，12,000+ 帧丢弃中注入不辍）；640 KiB 帧预跑标定队列涨落 Δ84.3 MB ≈ 128 × 640 KiB；展示（注入 ≈12,150 帧 ≥10,000）→ 溢出 toast 恰一次（丢弃 12016，截图留档）→ 复注 episode 2（1,807 帧）不再告警；3 次显隐回归正常、config/panic.log 零变化。**口径注记**（§6.5 阈值纪律）：帧尺寸 128 KiB 替代判据示例的 1 MiB——`Decoder::push` S-02 增量缓冲 debug 读路径单帧 O(F²)（1 MiB 帧 ≈7–17 fps，超出 warm 池 120 s 空闲回收租期），队列上界机制同源（容量 × 单帧上界 = 16 MiB）；R-03 与空闲回收构成双层防护（回收即清队列，实测 173.4→89.1 MB）。R-03 至此三口径收口（单测 + V-10 + 回归） |
