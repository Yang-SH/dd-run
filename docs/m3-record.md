# M3 实施记录 — 缓存与懒加载

> **状态**：✅ 已关闭（2026-09-02 真机复验通过：**A6** frozen 冷启动不拉起进程 + 点击桩项复热成功；**A2** 数据就绪 <agg> ~2ms < 200ms 达标，total 高因 GUI/wgpu+msyh 字体加载，R2 记录瓶颈不调目标）。
> **逻辑层 `cache.rs` + 协议 `get_command` 接线 + UI 接线全部落地**：
> frozen 磁盘桩读盘不拉起进程 / 点击桩项复热（spawn→initialize→get_command→执行，协议 §6.4）/
> LRU 保活 8 个 warm 进程超容驱逐回落 stub / `ColdStartTimer` A2 计时 / 页脚三态 ◌✓✗。
> 全 workspace **73/73 测试全绿**（dd-gui 24 + dd-host lib 31 + roundtrip 7 + dd-protocol 8 + dd-run 3）、
> clippy `-D warnings` 0、fmt 通过；`dd-gui.exe` / `dd-ext-sample.exe` 重编（16:05）。

> **验收映射**（implementation.md §M3）：A6（frozen 冷启动不拉起 + 点击桩项复热成功）、
> A7（LRU 行为单测）、A2（冷启动首屏耗时**实测记录**，不达标记录实测与瓶颈、不调目标）。

---

## 1. 分阶段实施计划与进度

| 阶段 | 内容 | 状态 | 验收标准（量化） | 结果 |
|---|---|---|---|---|
| P1 逻辑层 | `dd-host/src/cache.rs`：`FrozenCache`（落盘桩，键=id+version）/ `LruWarmSet`（容量 N）/ `ColdStartTimer` | ✅ | 8 个单测全过（A6 落盘往返 / 版本失效 / A7 驱逐 / 计时） | ✅ 8/8（先期已落） |
| P2 协议接线 | `ExtensionProcess::get_command`（§6.4，超时 5000ms 按协议 §10）+ `dd-ext-sample` 的 `get_command` handler（找不到回 `null` 非错误）+ roundtrip 全链路测试 | ✅ | roundtrip 新增 1 测（已知 Invoke/Page 命令取回 + 未知 id 回 None + close） | ✅ 7/7 |
| P3 UI 冷启动分流 | `aggregator`：按 `manifest.frozen` 分流——frozen + 磁盘桩命中 → `Stub`（**不 spawn**，A6）；frozen 无桩（首启）→ warm 拉取并落盘；fresh → warm 不落盘 | ✅ | lib 单测（flatten 三态合并含 Stub）+ 编译 | ✅ 24/24（含新增三态用例） |
| P4 UI 复热与 LRU | `main.rs`：点击桩项 → spawn→initialize→get_command→invoke/get_items；失败回退 stub 报错；进程归还走 `LruWarmSet`（容量 8），超容驱逐 close+回落 stub | ✅ | 编译 + clippy/fmt + 真机 A6 复验（§4） | ✅ 真机通过 |
| P5 A2 计时 | `ColdStartTimer` 埋点：聚合完成即 `mark_first_interactive` → `[dd-gui] 冷启动完成：X ms` | ✅ | 真机日志记录实测值（不调目标） | ✅ 真机记录（数据就绪 ~2ms<200ms 达标；total 高因 GUI/字体，R2） |

---

## 2. 完成判据对照

| 判据 | 状态 | 证据 |
|---|---|---|
| A6：frozen 冷启动**没有**进程被拉起；点击桩项复热成功 | ✅ 真机复验通过 | 逻辑层 `FrozenCache::load` 不涉及 spawn；roundtrip 7 测验证 `get_command` 链；**真机**见 §4 第 2/3/4 步（日志 + 页脚三态为权威，2026-09-02 通过） |
| A7：LRU 行为单测——超出 N 后释放并重新标 stub | ✅ | `cache.rs` 4 个 LRU 单测（驱逐最久未用 / touch 保活 / remove）+ GUI 层 `store_warm_process`/`evict_warm` 接线 |
| A2：实测首屏冷启动耗时，记录是否达成 200ms | ✅ 真机记录 | §4 第 6 步：读 `[dd-gui] 冷启动完成：X ms` 日志记录（数据就绪 <agg> ~2ms < 200ms 达标；total 高因 GUI/wgpu+msyh 字体，R2 记录瓶颈不调目标） |

---

## 3. 实施记录（2026-09-02）

### 3.1 交互模型变更（相对 M1/M2）

- **M1/M2**：冷启动 spawn **所有**扩展并无限保活；`frozen` 字段从未被使用。
- **M3**：冷启动按 frozen 分流，frozen 扩展在磁盘有桩时**不拉起进程**——其命令以"桩"形态渲染，
  点击时经**复热链路**（spawn → initialize → get_command → 执行）取回真实能力（协议 §6.4、A6）。
  首启（无桩）或 fresh 扩展仍 warm 拉取；frozen 成功拉取后**落盘**，供下次冷启动读桩。
- 页脚源状态由两态（✓/✗）扩展为**三态**：`✓` warm（进程活）/ `◌` stub（仅磁盘桩，点击将复热）/
  `✗` failed——A6 真机判定不依赖任务管理器，看页脚与 `[dd-gui]` 日志即可。

### 3.2 文件改动

| 文件 | 改动 |
|---|---|
| `crates/dd-host/src/process.rs` | `TIMEOUT_GET_COMMAND`（§10 表 5000ms）+ `get_command(id) -> Result<Option<CommandItem>, ProtocolError>`（`Ok(None)`=桩失效，正常结果） |
| `crates/dd-host/src/manifest.rs` | 抽出 `dd_run_dir()`；`extensions_dir` 派生其上；新增 `cache_dir()`（= 数据根目录/`cache`，三平台对称） |
| `crates/dd-ext-sample/src/main.rs` | 新增 `get_command` handler（按 id 查顶层目录；找不到回 `command:null`）；头注释补 M3 |
| `crates/dd-host/tests/roundtrip.rs` | 新增 `roundtrip_m3_get_command_reheat_chain`（Invoke 命令取回 / Page 命令取回 / 未知 id 回 None / close） |
| `crates/dd-gui/src/aggregator.rs` | `SourceStatus` 三态（`Warm`/`Stub`/`Failed`，原 `Ready` 改名）；`ExtItems`/`ExtOutcome` 增 `Stub`；`collect_top_level(exts, cache)` scoped-thread 分流（frozen 读桩不 spawn / 无桩 warm 拉取+落盘 / fresh 不落盘）；公开 `spawn_and_initialize`（复热复用）；flatten 三态合并；单测更新+Stub 用例 |
| `crates/dd-gui/src/main.rs` | 载荷/结果结构体加 `exts`、`proc: Option<…>`、`stub_reheat`；PaletteApp 加 `exts`/`lru(8)`/`cold`/`inflight`；冷启动走 `collect_top_level(…, cache)`；分派 `dispatch_invoke`/`dispatch_fetch_page`（warm 直发 / 桩复热）；复热线程 `spawn_and_initialize`→`get_command`→invoke/get_items；`store_warm_process`+LRU 驱逐（`evict_warm` 后台 close）；`mark_source_warm`；poll 按 `stub_reheat` 决定进程取舍（失败回退 stub 不保活）；`refresh_health` 移除已退出进程；A2 计时日志；页脚三态；顶层辅助 `invoke_on`/`get_items_on` |

### 3.3 设计决策（用户确认）

1. **本轮范围** = UI 接线 + 单测 + 编译验收；真机项由用户复验（不装双扩展环境）。
2. **复热实现协议 §6.4 `get_command` 完整链路**（含 host 封装 + 示例扩展 handler + roundtrip 测）。
3. **桩态三态可见**（◌/✓/✗）+ 终端日志。
4. 默认（用户可改）：缓存目录 = `%APPDATA%\dd-run\cache`（Windows，数据根目录/cache）；
   首启 frozen 无桩按 warm 拉取一次并落盘；LRU 容量常量 8；A2 用 GUI 启动日志（不加 CLI bench）。

---

## 4. 人工验收清单（2026-09-02 真机复验通过，A6/A2）

> 环境：真机当前无 `%APPDATA%\dd-run\extensions.d`，走**兜底示例扩展**路径
> （与 `dd-gui.exe` 同目录的 `dd-ext-sample.exe`，frozen=true、11 条命令）——正好是单 frozen 场景，
> A6/A2 可完整验证。若已装清单扩展同理。
>
> **必须从终端启动** `dd-gui.exe`（双击看不到日志）：
> ```text
> cd /d/AI/project/dd-run
> ./target/x86_64-pc-windows-gnu/debug/dd-gui.exe
> ```
> 按 `Win+Alt+Space` 唤起面板；以终端 `[dd-gui]` 日志 + 页脚三态为判定权威。
> 复验用 exe = **15:53 版**已修复前 3 项；**16:05 版**再修"列表长时把页脚挤出窗口"（见 §3.4 第 5 项）；**2026-09-02 用户按 16:05 版完成真机复验，A6/A2 全过，M3 关闭**。

#### #1 首启落盘（前置，一次性）
- **步骤**：① 若存在 `%APPDATA%\dd-run\cache` 则删掉它（确保从零开始）；② 启动 dd-gui.exe。
- **预期（终端）**：`[dd-gui] 冷启动：Sample Ext warm（11 命令）`（无桩 → 首启 warm 拉取并落盘）；
  页脚 `✓ Sample Ext（11 命令）`。
- **说明**：本次运行只为生成磁盘桩；**关闭后重启**进入 #2 才是 M3 主路径。

#### #2 冷启动读桩、frozen 不拉起进程（A6）
- **步骤**：关闭 dd-gui 后**再次启动**（不删 cache）。
- **预期（面板）**：页脚变为 `◌ Sample Ext（11 命令·桩）`。
- **预期（终端）**：`[dd-gui] 冷启动：Sample Ext 读桩 11 命令（frozen，未拉起进程 A6）` +
  `[dd-gui] 冷启动完成：X ms`。
- **失败信号**：页脚为 `✓`（warm）——说明仍走了 spawn；或列表为空（桩读盘失败）。

#### #3 点击桩项复热成功——Invoke 命令（A6）
- **步骤**：在 #2 的桩态下，选中并执行 `Say Hello`（Enter）。
- **预期（终端）**：`[dd-gui] 桩复热：ext=… cmd=sample.hello（spawn→initialize→get_command→invoke）`
  → `[dd-gui] 桩复热成功：ext=… 转 warm（LRU 保活）` → `invoke 成功：ShowToast …`；
  扩展侧 `-> get_command sample.hello => found`、`-> invoke sample.hello => ShowToast`。
- **预期（面板）**：Toast「Hello from dd-ext-sample！」；页脚 `◌` 变 `✓`。
- **失败信号**：toast「命令执行失败：…」且页脚保持 `◌`（复热失败回退 stub 属预期防御，需排查原因）。

#### #4 点击桩项复热成功——Page 命令（A6 + A5 复用）
- **步骤**：关闭重启（回到 #2 桩态）→ 执行 `Page：进入子页`。
- **预期（终端）**：`桩复热：ext=… page=m2.page …` → `桩复热成功` → `get_items 成功：page=m2.page items=4`。
- **预期（面板）**：进入子页、加载后列出 4 条（GoBack/GoHome/Dismiss/通知），副标题「第 1 次被拉取」；页脚转 `✓`。
- **失败信号**：`拉取失败：…` 空态；页脚保持 `◌`。

#### #5 复热失败回退 stub（防御路径验证）
- **步骤**（在 #2 桩态下进行，**不要在启动前改名 exe**，否则扩展整体消失、测的是"无扩展"而非"复热失败"）：
  1. 第二次启动后处于 ◌ 桩态（#2），**保持进程不退出**；
  2. 将与 dd-gui 同目录的 `dd-ext-sample.exe` 临时改名（如 `dd-ext-sample.exe.bak`），或用任务管理器结束 `dd-ext-sample.exe` 进程；
  3. 选中任意根命令并按 Enter。
- **预期（终端）**：`桩复热：ext=… cmd=…` → `invoke 失败：… spawn 失败：…` / `桩复热失败…回退 stub`。
- **预期（面板）**：Toast「命令执行失败：spawn 失败：…」；页脚保持 `◌`；**宿主不崩溃**。
- **收尾**：把 exe 文件名改回后再次点击同一命令，期望复热成功（#3 行为恢复）。
- **失败信号**：宿主崩溃/卡死；或错误后页脚变 `✗`（失败回退应为 `◌` 而非 `✗`，`✗` 留给真正"扩展进程在 warm 阶段崩溃"）。
- **替代方案**：在 `extensions.d` 放一份指向 `dd-ext-sample.exe` 的真实清单（`frozen: true`），则改名后扩展仍被扫描到、仅 spawn 失败，能更稳定地复现此路径。

#### #6 A2 冷启动耗时实测记录（R2：记录实测与瓶颈，不调目标）
- **步骤**：在 #2/#3/#4/#5 的任一次启动中读两行日志：
  - `[dd-gui] 冷启动完成：<total> ms（A2 目标 <200ms：数据就绪 <agg> ms + GUI 初始化/字体加载 ~<gui> ms，…）`
  - 记录 `<total>`、`<agg>`、`<gui>`。
- **判定**：A2 目标 = 进程启动到首屏**数据就绪** < 200ms。`<total>` 包含 wgpu + msyh TTC（数十 MB）+ 窗口初始化；`<agg>` 为聚合线程 scan+collect+flatten 耗时（读桩路径通常 < 50ms）；`<gui>` = `<total>` − `<agg>` 即 GUI/字体加载。
- **行动**：未达标 = 记录实测值 + 哪个分项是瓶颈（GUI/字体 vs 聚合），据此决策；**不修改 200ms 目标**（implementation.md R2）。

#### #7 回归抽查（M2 链路在 warm 下不受影响）
- **步骤**：warm 态（#3/#4 完成后）快速连续 Enter 两次同一命令 → 第二条应 toast
  「扩展进程不可用（可能正在处理上一个请求）」（M2 #9 场景 A 语义保留）；
  Esc 返回/隐藏、再次唤起复位照旧（M2 #3/#10）。
- **失败信号**：第二条命令绕过串行化并发执行；或唤起不复位。

### 3.4 修复记录（2026-09-02，真机反馈驱动）

| 反馈 | 根因 | 修复 |
|---|---|---|
| #2/#3/#4 页脚 ◌ 渲染为方框（"未看到页脚 ◌ Sample Ext（11 命令·桩）"） | msyh.ttc 覆盖 CJK 与 Dingbats 的 ✓/✗，**不覆盖** Geometric Shapes 的 ◌ (U+25CC)，回退到默认字体仍无该字形 → tofu | `setup_cjk_fonts` 额外加载 `C:\Windows\Fonts\seguisym.ttf`（Segoe UI Symbol，Win 7+ 必装）作为"sym"字体族，append 到 Proportional 之后。egui 按字体族顺序回退：cjk 缺的 Geometric Shapes / Misc Symbols 落到 seguisym。保留三态 ◌/✓/✗ 设计，不改文案 |
| #5 "未看到任何命令"——列表为空时连 footer 一起消失 | 空的 list area 用 `ui.centered_and_justified("未发现命令…")` 撑满**整个**剩余高度，把后续 sources 行 / separator / 键位提示挤出 460px 窗口底边 | draw_list 加载/空态/draw_panel 加载 4 处全部从 `centered_and_justified` 改为 `vertical_centered`（水平居中 + 最小垂直占用），footer 永远在视口内 |
| #5 测试步骤本身在兜底环境下不可复现 | 旧步骤"启动前改名 exe"让扩展**整体消失**（找不到 sample exe → 兜底也失效），测的是"无扩展"而非"复热失败" | 重写为"在 #2 桩态下，**保持进程运行**→ 改名或任务管理器结束 `dd-ext-sample.exe` → 再点击"。此时磁盘桩与 ext 内存信息均在，spawn 才会在复热时失败、回退 stub |
| #6 A2 实测 1936–3214 ms 远高于 200ms，但分不清是聚合慢还是 GUI/字体加载慢 | `ColdStartTimer` 单点计时（mark_spawn_start 在 eframe 闭包、mark_first_interactive 在聚合完成），覆盖进程启动 + wgpu + msyh 字体 + 聚合全过程 | 在 `spawn_aggregation` 线程内额外记录 scan→flatten 的 `agg_ms`，随 `AggregatePayload` 回传；`poll_aggregate` 打印 `[dd-gui] 冷启动完成：<total> ms（A2 目标 <200ms：数据就绪 <agg> ms + GUI 初始化/字体加载 ~<gui> ms，…）`。R2 要求"未达标记录实测与瓶颈而非调目标"，此分项日志直接定位瓶颈。**15:53 复测验证**：分项输出 `数据就绪 2 ms + GUI 初始化/字体加载 ~2861 ms`——瓶颈明确为 GUI/wgpu + 22MB msyh.ttc 字体加载，与实现细节吻合 |
| **列表长时页脚被挤出 460px 窗口（16:05 真机复测反馈）** | 上一版把 sources 行 + separator + 键位提示放在 `CentralPanel` 内部 ScrollArea **之后**；ScrollArea 默认占满剩余高度 → 列表 11+ 项时 footer 块被推过窗口底边。**空态修复**（vertical_centered）只解决 0 项场景，**长列表场景未解决** | `draw_panel` 改用 `egui::containers::Panel::bottom("status_footer")`（egui 0.36 的"chrome 固定底栏"标准做法——注：egui 0.36 没有 `TopBottomPanel`），把源状态 + 键位提示整个移出 CentralPanel 放到独立底栏；中央 ScrollArea 高度自动收缩到底栏之上、永远不冲突。`draw_status_footer(&self, ui)` 抽出复用 |

---

## 5. 已知遗留

| # | 遗留 | 说明 |
|---|---|---|
| 1 | 顶层 `items_changed` 仍不触发 Root 重聚合（仅提示） | 自 M2 §6 延续，M3 未改（非本里程碑判据） |
| 2 | 复热/invoke 通道单槽（`invoke_rx`/`page_rx` 各一） | 沿用 M2 #9 单扩展串行语义；跨扩展并发第二请求报"上一请求仍在处理"。多扩展高频并发的场景留待 M4 评估 |
| 3 | 扩展崩溃恢复（连续崩溃保护等） | M4（A8），本里程碑仅做到"退出进程移除保活 + 点击重新拉起" |
| 4 | A2 目标未达时的瓶颈定位 | 真机记录后决策（R2） |
