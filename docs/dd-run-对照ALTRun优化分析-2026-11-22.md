# dd-run 对照 ALTRun 优化分析（独立复核版）

> **性质**：第三方独立分析文档（基于两仓当前 HEAD 的代码级核对产出，已修正初稿 3 处错漏；第二轮独立复核另修正 2 处引用错漏并精化口径，见 §4）｜ **版本**：v1.1（复核修正）｜ **日期**：2026-11-22
> **分析基线**：dd-run `de8225d`（2026-11-22，ALTRun 对照波次一/二批）；ALTRun 最新 main
> **与项目既有文档的关系**：本文所有建议均与 `altrun-对照优化方案 v1.3`、`后续开发路线图 v1.4`、`memory-optimization-plan`、`future-features-plan` 的既有决策逐一核对，零冲突；不新增编号、不改变任何"不做/缓办"边界，落地时应走项目既有渠道（future-features-plan §2 立项判据复核）。

---

## 1. 两项目总体对比

| 维度 | ALTRun（zhugecaomao） | dd-run（Yang-SH） |
|---|---|---|
| 定位 | Windows 启动器（仿 Alfred），面向终端用户开箱即用 | 命令面板/启动器（CmdPal 设计启发），平台无关核心 + Windows 主平台 |
| 技术栈 | AutoHotkey v2，134 个 .ahk，约 5.1 万行（wc 口径，含 Tests 约 1.8 万） | Rust，workspace **6 个 crate**（5 产品：dd-gui/dd-host/dd-ext/dd-ext-sample/dd-run-cli + 协议独立版本线 dd-protocol 1.0.0），约 5.7 万行 |
| 许可证 | **GPL-3.0** | **MIT**（因此 dd-run 只能做设计级对照，不能直接移植代码——既有对照方案的处理方式正确） |
| 扩展机制 | 进程内脚本（AHK 原生）、Scripts 文件夹 | 子进程 + NDJSON JSON-RPC（ADR-1/4）；内置扩展走进程内直调但同协议（M9）；manifest 扫描（ADR-3）、签名计划（O16）、fuzz 夜跑 |
| GUI | AHK 原生 GUI，**22 主题 × 3 密度，4 界面语言** | egui，亮暗 + Mica/Acrylic + 着色 + 背景图 + 3 密度，**2 界面语言**（ZhCn/EnUs，`Lang` 枚举），键表实查 198 条 |
| 网页搜索 | 多引擎 | **5 引擎预设（Google/Bing/Baidu/DuckDuckGo/GitHub），默认仅启用 1 个**（`default_search_engines()` 返回 `presets[0]`），设置页可增删 |
| 分发/签名 | winget + Scoop，SignPath 代码签名（README Code signing policy 节） | winget manifest 生成器已入库（O15 落地 `tools/gen-winget-manifest.mjs`）；签名评估未做（O16 待） |
| 工程纪律 | 8000+ 自动化测试（README:44）、CI、开发指南 wiki | 真机走查 708 单测 / 0 失败 / 7 忽略（HEAD commit 记录）、UI 金图回归、协议 fuzz、bench 趋势、docscan 机检、三平台阻塞 CI、30+ 份文档（ADR/审计/走查/判据化） |
| 体量 | 发行 exe 约 2.3 MB | 发行 exe 约 8.5 MB（v0.1.1 发行版；HEAD 构建实测 10.3 MB）；数据集中 `%APPDATA%`，零遥测 |

**架构取向差异（本质）**：ALTRun 是"单进程单体 + 脚本生态"，功能密度高、上手快；dd-run 是"协议化多进程 + 强工程门禁"，隔离性、可测试性、跨平台潜力更强，但功能面目前小于 ALTRun。dd-run 的文档与门禁体系显著超过 ALTRun，属工程成熟度领先项。

## 2. 功能面对照结论（经代码核实）

### 2.1 已对齐甚至更优

全局热键（含失败回滚降级、启动注册失败不 panic——`hotkey.rs` 约 :8–14/:123）、拼音全拼/首字母匹配、应用索引（dd-run 额外覆盖 UWP + Steam）、网页搜索 + 无结果兜底、自定义直达命令、浏览器书签（Chrome/Edge → 已扩 Brave/Vivaldi，N9 落地）、Everything 文件搜索（双通道 + 语法透传）、右键动作面板、系统命令 + 危险确认（N7 落地 52 条嵌套页）、计算器（N8 落地增强）、面板拖动/尺寸/位置记忆（N21 落地）、主题个性化、频次排序、设置导入导出、更新检查。

> 此面双方基本打平，与 `altrun-对照优化方案 v1.3` §2.1 判定一致。

### 2.2 ALTRun 有、dd-run 尚未落地（经符号级 grep 确认）

8 项 + 1 个子项：空查询首屏（N11）、文件搜索目录导航（N12）、窗口切换器（N16）、第二热键基建（N17）、文本片段（N18）、搜索历史（N20）、更新下载闭环（O17）、签名评估（O16）、`?` 键位速查页（N14 子项）。

已落地（波次一 + 波次二前五）：N6/N7/N8/N9/N10/N13/N14/N15/N19/N21/O15。

### 2.3 已明确"不做/缓办"（本文建议全部避开）

Quick Switch、汇率/单位换算、文本转换、.msc、魔法词、文件混入主面板、拖出复制、大字显示、日期戳、文件夹同步等 22 项有书面理由（方案 §3.1）；剪贴板历史、选中内容操作、浏览器历史等缓办项有明确前置条件（方案 §3.2；future-features-plan §5 仅含剪贴板历史行，其余两行系方案 §3.2 新增）。**以下建议零冲突。**

## 3. dd-run 可优化点

### 3.1 A 组：承接既有方案未落地项（波次三建议排期）

| 优先建议 | 项 | 既有口径（已核对） | 理由与实现要点 |
|---|---|---|---|
| ① | **O17 更新下载 + SHA256SUMS 校验闭环** | P2 · 小 · 系统 curl 既有取径 · 零依赖 | 工作量最小的安全链路收口：Available 态 → 下载 zip → 校验 SHA256SUMS → **只提示不自替换**（保持既有边界）。复用 `update_check.rs` 现有模块化结构 |
| ② | **N17 第二热键基建 + 命令热键（≤3 槽）** | P2 · 中 · 零依赖增量 | 战略价值最高：它是剪贴板历史（B4）、选中内容操作（B5）两个缓办大项的**共同前置**（方案 §3.2 两行分列该前置、§6 明示公共依赖），早落地早解锁评审通道。`hotkey.rs` 单热键 + `PostThreadMessageW` 重注册是干净的槽位化起点 |
| ③ | **N16 窗口切换器** | P2 · 中 · windows-sys 特性 +2 | 用户可感知价值最大的单项（Alt+Tab 替代）：枚举可见窗口、模糊/拼音搜、切前台。沿用 M9 in-process 模式即不触碰 ADR-1 |
| ④ | **N12 文件搜索目录导航**（Enter 进目录 / Ctrl+Backspace 回上级 / Right 补全） | P2 · 中 · 跨文档联动 | **须先完成 search-file.md（权威口径）的回写再动代码**，避免双口径——既有方案明确要求 |
| ⑤ | **N11 空查询首屏**（置顶分组 + 最近文件分组） | P2 · 中 · 零 | 纯聚合层改动；注意与 N16 落地后的窗口置顶分组共用数据通路 |
| ⑥ | **N18 文本片段一档**（面板内搜片段 → Enter 复制） | P2 · 中 · 零 | 纯面板内能力；**二档（`;keyword` 全局展开）缓办边界保持**，不触碰输入伪造面 |
| ⑦ | **N14 的 `?` 速查页子项** + **N20 搜索历史** | N20 P3 · 小 | 均为小项，可作收尾批（`?` 速查页为 PageStack 宿主页变体决策） |
| ⑧ | **O16 签名信任链评估**（SignPath / Azure Trusted Signing） | P3 · 小（评估记档） | 无代码风险，随时可做 |

### 3.2 B 组：真机走查遗留三项异常（C.3）的可落地修复设计

既有文档中这三项仅"记档待维护者处置"，建议直接立项：

1. **LRU 级联驱逐 → 全 stub 态**（warm_capacity=8 与 8 个扩展临界，瞬态超额后全部回落 stub，表达式查询连兜底行都没有）
   - 设置页 `warm_capacity.desc` 增加动态提示：实时显示"已装扩展 N 个 / 当前容量 M"，N ≥ M 时给出"建议调大容量"警告文案（`text.rs` 加 2 条 key，机检查漏用 i18n_check.mjs——既有文档已规划、**拟新建**）；
   - `pool.rs::store_warm_process`（39–53 行）现状已核实为"单归还单驱逐"（先 push 后 `lru.access`，超容返回 1 个 victim 且 `victim != ext_id` 才驱逐）——**补一条回归单测固化该约束**，防未来重构引入级联；
   - **不改默认值 8、范围 1–16**——内存基线（M1–M4）依赖该默认（依据在 `pool.rs` 约 :13–15 注释与 future-features-plan N4；memory-optimization-plan 本身未记该约束），改动会破坏基线复测口径，用提示代替。
2. **配置转换窗口数据异常**（两条测试自定义命令在保存 → 发现为空的窗口内消失，机制未定论，方案标 P3）
   - 第一步先加**观测**：`Settings::save_to` 成功路径补 `log::info`（custom_commands 条目数 + 字节数），下次异常可从 `file_log.rs` grep 出时间线；
   - 第二步做**恢复**：见 3.3-C1 自动滚动备份环；启动加载时若发现 `custom_commands` 数量较上次备份骤降（如 >50%），toast 提示并提供一键恢复。
3. **失效 ⚠ 前缀在直达行型不渲染**（⚠ 为走查记号，实际前缀文案「路径不存在」）：N19 失效检测的行内可见性目前只靠 invoke 拒绝 toast（hover tooltip 仍携带副标题但发现性低；走查注记实证："items 层前缀仅数据层携带，行渲染不展示该行型副标题"）——`row.rs` 行渲染层按数据层前缀字段补渲染，小改动 + 金图回归。

### 3.3 C 组：本次代码审查新增发现（不在任何既有文档，零冲突）

1. **显式保存差异化 fsync + 自动滚动备份环**
   - 现状取证：`settings.rs::atomic_write`（约 965–985 行）注释明确"刻意不做 fsync"，理由是 `persist_panel_size` 每次隐藏面板都写盘（`app/mod.rs` 约 :994、`lifecycle.rs` 约 :126–127）、优先低延迟——该理由**只对高频路径成立**；用户在设置页点保存（低频）走同一条无 fsync 路径，恰是 B-2 异常最可疑的窗口。
   - 方案：`atomic_write` 增加 `fsync: bool` 参数（或新增 `atomic_write_durable`）；`save()`/`save_to()` 显式保存与 `save_backup_to` 开启 `sync_all`；`persist_panel_size` 保持快速路径——**与现有注释设计意图不冲突，是按频率分层**。
   - 备份环：每次成功 `save()` 前把旧 settings.json 轮转写入 `settings.json.bak.1..3`（复用既有 `atomic_write`），约 40 行 + 3 条单测；可先只对含 `custom_commands` 变更的保存触发以控制写放大。
2. **warm_capacity 设置页临界实时提示**：如 3.2-B1 所述，是把 C.3 观察项转化为用户可见防呆的最小实现，无需动池逻辑。
3. **保存审计日志 + 启动骤降检测**：如 3.2-B2 所述，观测先行、恢复兜底，均为小改动。
4. **协议层 `section` 一致性机检**（初稿 C-3 的修正版）
   - 取证更正：`result_category` **不是协议字段**——`dd-protocol/src/model.rs` 的 `CommandResult` 无此字段（协议层只有 `section`，`model.rs` 约 :77）；它是宿主侧 GUI 字段（`state.rs` 约 :34），由 `aggregator/items.rs` 约 :96 的 `category_label_for(ext_id)` 组装填充。
   - N10 走查暴露的"sidecar 只设 section、宿主类别门控失配"问题，修复方向已正确落在**形状门控**（`ctx_menu.rs` 约 :322/:399，path_like/url_like）。
   - 增量建议收窄为：把"文件/侧车类扩展返回值必须携带 `section`（或明确缺失时的宿主兜底规则）"纳入 `dd-run-cli --conformance` 断言（CI 已有 conformance smoke 任务可挂），把这次踩过的坑固化为机检。
5. **核对轮已确认"不需要"的方向**（避免重复提案）：CI 已含 conformance smoke（calc in-process + spawn sample + scaffold e2e，`ci.yml:211-223`），无需新立"样例扩展烟测"项。

### 3.4 D 组：结论

dd-run 的工程纪律（测试三关、fuzz、金图、文档判据化）已领先于 ALTRun，且所有 ALTRun 特性缺口都已有编号化处置——**真正的优化空间在"收口"而非"新立"**：按 3.1 表顺序落完波次三的 8 项，功能面即与 ALTRun 打平（缓办项除外）；3.2/3.3 共 7 个加固点全部是小到中工作量、可单测验证、不触碰任何"不做"边界的收敛项。

## 4. 核对记录（本文准确性说明）

初稿经逐条复核，修正 3 处后定稿：

| # | 初稿原文 | 核对发现 | 定稿口径 |
|---|---|---|---|
| 1 | "workspace 7 个 crate" | `Cargo.toml` members = `crates/*`，实际 6 个（Cargo.toml 注释："5 个产品 crate + dd-protocol 独立版本线"） | §1 表已改 |
| 2 | "五引擎网页搜索" | `default_search_engines()` 仅返回 `presets[0]`（Google 1 个）；5 个为预设可选池（`settings/search.rs::preset_search_engines`） | §1 表已改 |
| 3 | "把 `result_category` 在 protocol.md 升格为协议必填" | `result_category` 非协议字段（协议层只有 `section`）；是宿主 GUI 字段（`state.rs` 约 :34 / `items.rs` 约 :96） | §3.3-4 已改写 |

**第二轮独立复核**（2026-11-22，v1.0 → v1.1；独立核对轮：全部代码锚点逐行重验 + 两仓实测 + 文档引用重验）再修正 2 处引用错漏：

| # | v1.0 原文 | 复核发现 | 定稿口径 |
|---|---|---|---|
| 4 | "tools/i18n_check.mjs 机检查漏"（§3.2-B1） | 该工具**不存在**——既有文档均标"拟新建"（改进分析 §5 / 方案 L130） | §3.2-B1 已改为"拟新建，届时落地" |
| 5 | "与方案 §2 表逐行一致"（本节锚点） | 方案无 §2 优先级表——该表实为 §1.1「提案总表」（方案 L27–40），内容逐行吻合 | 已改指 §1.1 |

同轮口径/措辞精化（不改任何结论）：ALTRun 行数实测 5.1 万（原"约 4.9 万"，含 Tests 约 1.8 万）；体量口径注明为**发行 exe**（ALTRun.exe 2.3 MB / dd-run v0.1.1 8.5 MB，HEAD 构建实测 10.3 MB）；N17 公共前置明示位置补注方案 §6；§2.3 合引注明 future-features-plan §5 仅含剪贴板历史；B1 冲突依据改指 `pool.rs` 约 :13–15 注释 + future-features-plan N4（memory-optimization-plan 本身未记 warm_capacity 约束）；B3 补注实际前缀文案「路径不存在」与 hover tooltip 可见性。

核对确认无误的关键锚点：

- **A 组优先级/工作量**：与方案 §1.1「提案总表」逐行一致（N11 P2·中 / N12 P2·中·跨文档 / N16 P2·中·特性+2 / N17 P2·中 / N18 P2·中 / N20 P3·小 / O16 P3·评估 / O17 P2·小·curl）；未落地项经符号级 grep 确认（`pinned_keys/search_history/command_hotkeys/windows_builtin/SHA256SUMS` 等零命中）。
- **B 组代码锚点**：`pool.rs` 约 :39–53（单归还单驱逐现状属实）、`settings.rs` 约 :965–985（无 fsync 注释属实）+ `app/mod.rs` 约 :994、`lifecycle.rs` 约 :126–127（persist_panel_size 高频落盘）、`general.rs` 约 :361–422（设置页无临界提示）、`save_backup_to` 仅手动导出（`BACKUP_FILE_NAME`）、⚠ 前缀数据层实证（方案走查注记）、`hotkey.rs` 约 :8–14/:123（降级回滚属实）。
- **两项目概览数据**：ALTRun GPL-3.0（LICENSE）/ SignPath（README:221）/ 22×3 主题（README:43/117）/ 4 语言 / 8000+ 测试（README:44）/ winget+Scoop；dd-run MIT / 708-0-7（HEAD commit）/ ZhCn+EnUs / 键表 198 条（方案引"约 194"，波次落地后 +4，量级一致）/ 默认引擎 1 个。

残留不确定项（如实说明）：

1. Windows 运行时行为（LRU 级联现场、配置丢失窗口）无法在本环境复现，只能核实代码逻辑可致该形态；3.2/3.3 的建议据此保守（提示 + 日志 + 备份，不改默认值、不改池语义）。
2. 行数统计为 wc 口径近似（ALTRun 实测 5.11 万，含 Tests 约 1.8 万 / dd-run 约 5.7 万），量级可信。
3. O15 产物一致性、N21 走查腿等"真机记录"引自项目文档（附录 B/C），本环境无法重放验证。
