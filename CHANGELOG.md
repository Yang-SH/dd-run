# 更新日志（Changelog）

本文件记录 dd-run 的版本变更。

## [Unreleased]

### 修复（文件搜索图标：按真实路径档首抽 191–704 ms → 分层异步 + 自动刷新，E1，2026-09-19）

- **根因**：`get_items` 在同步路径上逐条 `SHGetFileInfoW` 抽图，且 `.exe`/`.lnk`/`.msi`/`.url` 按**真实路径**分键 → 键数随结果条数线性增长（30 条 = 30 次抽取 × 6–25 ms，冷缓存实测 191–704 ms）。在 `A-IC-02` 的「首次 ≤ 40 ms」预算下，**单键最坏成本已占 62%** —— 同步路径几乎没有优化空间（抽 1 个 exe 就吃掉大半预算）。
- **修复（分层异步 + 自动刷新）**：① **按键分层** —— `path:` 键**一律不进同步路径**（立即返回类别 glyph + 投递后台），`ext:`/`dir`/`noext` 键同键复用留在同步路径，并受 `ICON_SYNC_BUDGET = 15 ms` 时间预算兜底；② **后台 worker** 4 线程（首次入队惰性启动、按键去重、取任务时才持队列锁）抽图并写落盘 + 进程内缓存；③ 补齐后按 `ICON_NOTIFY_THROTTLE = 200 ms` 节流发 §7.1 `items_changed(files.results)` → 宿主**既有**链路（命中当前页 → 100 ms 合并窗口 → 重拉）自动把 glyph 换成真实图标。**协议零改动、宿主零改动**；`dd-ext/src/lib.rs` 新增跨线程通知器（共享 stdout 加锁，`write_all + flush` 同临界区，消息不交错；未安装时 no-op）。
- **实测**（`tools/icon_acceptance.py --cold`，release sidecar 直驱）：按真实路径档**同步** `icon_ms` **191–704 ms → 0.32 ms** ✅；后台补齐后重查 **`path=30 / glyph=0`，0.09 ms** ✅；`.lnk` 批补齐后 `path=17 / glyph=13`（不可访问路径回落仍生效）✅；按扩展名首抽 32.23 ms、缓存命中 0.10 ms、10 次查询全 `kind=results` ✅。
- **行为变更**：首次查询中的 exe/lnk/msi/url 行**先显示类别图标**，约 0.2–0.8 s 后自动替换为真实图标（用户无需再输入）。
- **代价与新证据**：`dd-ext-search.exe` 876,544 → **912,384 B（+35,840 B）**；**E2 探针**（临时剥离 `image` 调用后构建）实测 **876,544 → 779,776 B（−96,768 B）** → 若实施 E2 选项 ②（自写零依赖 PNG 编码器 + `image` 转 dev-dependency），sidecar ≈ **818 KB**、增量 ≈ **48 KB**，回到 64 KB 预算内。**E2 待决策**。
- **验证**：`cargo test --workspace` **452 passed / 0 failed**（447 基线 + 5 新增单测）；`fmt` 无差异 / `clippy --workspace --all-targets` **0 告警** / `cargo build --release` **exit 0**；`tools/icon_acceptance.py --cold` 8 项判定中 7 项 PASS（仅 A-IC-06 体积项因 E2 未实施而 FAIL）。

### 文档（文件搜索用户指南解除 `es.exe` 前置表述，2026-09-19）

- **口径变更（零代码改动）**：`docs/search.md` 由「`es.exe` 仍受支持 / 仍建议安装 / 已发布版本仍依赖」改为 —— **唯一必需依赖是 Everything 在运行**；`es.exe` 降为**可选回落通道**（只在 IPC 直连不可用时出场，未安装则给可读提示、不静默失败）。改动面：§1 标题「前置条件」→「准备事项」、步骤 1 标「必需」/步骤 2 标「可选（推荐）」、步骤 4 验证改可选、配置项注、故障排查两行、已知边界引导项、§7 升级说明、§8 FAQ（原「还需要安装 `es.exe` 吗？**需要（建议）**」→「**不必须（可选，推荐）**」）。
- **决策依据**：真机验收两轮完成（A-33-05 / A-33-06 / A-33-07 / A-33-10 **通过**，回落静默失败缺陷已修）→ 原「待决策项」（`search-file.md` §1 / `INDEX.md` §5）**定稿解除**；**A-33-08 仍为唯一红线**（Win10 / Everything 1.5 / 提升 / 命名实例未覆盖），故文档以「直连不可用时回落」表述承接，**不作全覆盖承诺**。
- **同步回写**：`README.md` / `README.zh-CN.md` 文档表、`search-file.md`（v3.8：§1 口径 + §8 版本演进行）、`INDEX.md`（v1.15：§5 行 + 两处行数重算）、验收报告 §1 与 `doc-audit` / `doc-code-diff` 的红线注记（**就地追加「已解除」注、历史结论保留**）、`implementation.md` §2 + §7。

### 新增（文件搜索：面板内 `Ctrl+F` 直达 + 查询结果显示真实图标，2026-09-19）

- **`Ctrl+F` 一键直达**：面板内**任意页**按 `Ctrl+F` 进入文件搜索页（此前须输入 `f ` 前缀、点顶层入口项或兜底模板）。已在文件搜索页时**幂等**（仅聚焦、输入保留）；其他位置先回 Root 再进页（**栈深恒为 2**）；仅 Root 的查询带入（trim 后非空、**原样**带入——前缀剥离逻辑已于同日随 `f ` 前缀直达一起移除，见下条变更），设置页 / 其他嵌套页进空查询；扩展未加载或被禁用 → Error Toast，不产生空页。键位取证：`Ctrl+F` 此前**全仓无绑定**（面板内仅 Esc / ↑↓ / Tab / Shift+Tab / Enter / `Ctrl+,` / Shift+F10）；确认对话框与右键菜单活跃时不穿透，热键捕获模式下仍可作为自定义全局热键候选。根页 placeholder 追加 `Ctrl+F 搜文件` 提示。
- **文件结果图标 → Windows Shell 真实图标**：由「按扩展名的 12 类 Segoe glyph」改为 `dd_ext::shell_icon` 抽 32×32 Shell 图标 → PNG 落盘缓存（`%APPDATA%\dd-run\cache\file-icons\`），经协议**零改动**的 `IconKind::Path` 回传（复用宿主既有 path 图标链路）。缓存键分级：目录 → `dir`；`.exe`/`.lnk`/`.msi`/`.url` → 真实路径（每个程序自身图标）；其余 → `ext:<小写扩展名>`。抽取 / 编码 / 写盘任一失败 → 回落类别 glyph（零退化）；非 Windows 目标编译通过并恒走 glyph。
- **图标管线去重**：`HICON/HBITMAP → PNG`（掩码 alpha、掩码行 DWORD 对齐、`GetDIBits` 负高 top-down）与缓存三函数自 `builtins/apps.rs` **上移**为新模块 `dd-ext/src/shell_icon.rs`，应用图标与文件图标共用一份实现；apps 行为不变（仅可见性与位置）。
- **可观测性**：`get_items` 计时日志新增 `icon=` 段（结果项构造 / 取图耗时；`score_ms` 自本次起不含构造期）；`tools/search_acceptance.py` 的计时解析同步支持（`icon=` 为**可选**段，旧日志仍可解析）。
- **测试竞态修复（既有缺陷，本批暴露）**：`search.rs::path_index_evicts_beyond_capacity` 按 **id 阈值**清空 `PATH_INDEX`，会连带清掉并行用例的 pid（`invoke_copy…` 因此拿到 0 条副作用）；此前偶发，接入真实图标后**并行必现**（实测 5/5 失败、跳过该用例 3/3 通过、单线程 48/48 通过）。修法：8 个读回索引的用例统一持测试内串行锁（生产逻辑零改动）。
- **验证**：`fmt` 无差异 / `clippy --workspace --all-targets` **0 告警** / `cargo test --workspace` **448 passed**（基线 436 + 本批 12）；`cargo build --release` **exit 0**；真机直驱 release sidecar（`--cold` 冷缓存口径）实测 `icon_ms`：缓存命中 **0.09–0.44 ms**（30 条）、按扩展名首抽 **冷启首档 40.29 ms（压线）/ 其余 9.3–10.8 ms**、**按真实路径首抽 191–704 ms（30 新键，随文件构成波动）**；`file-icons/` 39–88 文件 / 34–63 KB。
- **验收工具（可复现）**：新增 `tools/icon_acceptance.py` —— A-IC-01/02a/02b/02c/04/06 **六项自动判定** + JSON 证据（`target/acceptance/icon-acceptance.json`）+ 退出码（0 全过 / 1 有 FAIL / 2 环境未就绪）。**`--cold` 是「首抽」判据的必要口径**：热缓存会把 `ext:exe` 从 703.8 ms 降到 98.7 ms，从而把未达标误读成达标。
- **两处未达标（如实记档，处置待决策，详见 `docs/search-file.md` §10.4）**：① 按真实路径档首次抽取 **703.8 ms**（预算 ≤ 40 ms）；② `dd-ext-search.exe` **769,536 → 876,544 B（+104.5 KB，预算 ≤ 64 KB）**，宿主 `dd-run.exe` 仅 **+3,072 B**。

### 变更（文件搜索：移除 `f ` 前缀直达，2026-09-19）

- **移除**：根页输入 `f ` + 空格**自动进文件搜索页**的便捷触发整条删除 —— `FILE_SEARCH_PREFIX` 常量、纯函数 `file_search_drill_target()`、`PaletteApp::maybe_drill_file_search()` 及其在 `ui()` 的每帧调用、状态 `file_drill` / `file_drill_armed`、`poll_page` 的落地回填分支，全部清掉（**无死代码残留**）；`Ctrl+F` 的查询带入不再剥离前缀。**`f ` 自此是普通搜索词**（`f report` = 搜同时含 `f` 与 `report` 的项）。
- **动机**：前缀会**劫持根视图的字面查询**（想搜字面 `f report` 也会被强制进页），而 v3.6 的 `Ctrl+F` 已提供更明确的一键直达 → 面板内直达入口收敛为一条，进入方式由四条变为**三条**（顶层入口项 / 兜底模板 / `Ctrl+F`）。
- **未改动**：`open_page` 统一回填与 `poll_page` 的 v3.3 query 保留逻辑（`Ctrl+F` 带词进页不变）、「已在文件搜索页 → 幂等」判定、栈深恒 2、扩展不可用 Toast、顶层入口项、兜底模板、扩展侧与协议（零改动）；其余键位与页面栈语义不变。
- **验证**：`fmt --all -- --check` 无差异 / `clippy --workspace --all-targets` **0 告警** / `cargo test --workspace` **447 passed / 0 failed**（基线 448 − 删除 1 个前缀专属用例 `file_drill_prefix_still_works`；`ctrl_f_query_source_rules` 改锚定「`f ` 开头原样带入」）。

### 变更（应用列表：剔除 Windows SDK / 驱动包装的调试诊断工具，2026-09-19）

- **现象**：默认「应用」列表里混入 `Application Verifier (WOW)` / `Application Verifier (X64)`、`Windows App Cert Kit`、`Debuggable Package Manager (1)`、`Developer PowerShell for VS 2022`（含 `(1)`）、`AMD Bug Report Tool`、`Windows Software Development Kit` —— 这些由 Windows SDK / 驱动包 / VS 安装器写进开始菜单，普通用户不会启动，出现在「应用」列表里只会稀释检索质量。
- **修复**：新增 `DEV_TOOL_TITLE_KEYWORDS`（与 `JUNK_TITLE_KEYWORDS` 同走 `is_junk_title` 判定，两类规则各有出处、便于日后增删），全部采用**精确短语** —— `application verifier` / `app cert kit` / `debuggable package manager` / `developer powershell` / `bug report` / `software development kit`，避免 `sdk` / `debug` 这类宽词误杀。
- **验证**：真机 **122** 条样本实测**命中 8 条、误杀 0 条**（`Visual Studio 2022` / `Visual Studio Code` / `Visual Studio Installer` / `PowerShell 7 (x64)` / `Windows 工具` / `Git Bash` 均保留）；新增单测 `dev_tool_titles_are_filtered`（8 条正例 + 6 条反例）；三关 `fmt` 无差异 / `clippy` 0 告警 / `cargo test --workspace` **436 passed**。

### 修复（模糊排序字段分层：标题命中被副标题命中压过，2026-09-19）

- **现象**：输入 `steam` 时，标题精确匹配的「Steam」排在第 5 位 —— `Dead Cells`、`Family Crush` 等 Steam 游戏都在它前面，看起来像「没有按匹配度排序」。
- **根因**：`dd-gui::fuzzy::FuzzyMatcher::score` 把 `title` / `subtitle` / `section` / `pinyin` / `tags` **混在同一池里取最高分**，而 nucleo 对**前缀连续匹配不区分 haystack 长度** —— 实测 `Steam`（标题精确，`140` 分）与 `steam://rungameid/588650`（Steam 游戏副标题，`140` 分）**完全同分**。`recompute_visible` 按分降序**稳定排序**，同分遂回落到扩展侧字母序（`apps.rs` 按标题小写排序），把标题命中项挤到后面。**排序机制本身正常**，问题在「字段无权重」。
- **修复**：**字段分层** —— `score()` 返回 `(层 << 32) | nucleo 分`：**层 1 = 标题层**（`title` 与其派生拼音索引 `pinyin`），**层 0 = 附属层**（`subtitle` / `section` / `tags`）。层不同绝不互比，层内仍按匹配质量排，同层同分继续由稳定排序保持原序。改后 `Steam` 与 `Steam Support Center`（均标题命中）恒排在所有 `steam://` 游戏之前，二者同分且字母序在前 → **`Steam` 落到第 1**。
- **验证**：新增 2 个单测（`fuzzy::tests::title_hit_outranks_subtitle_hit`、`state::tests::query_ranks_title_hit_above_subtitle_hit`）锁定「标题命中恒先于副标题命中」；三关 `fmt` 无差异 / `clippy` 0 告警 / `cargo test --workspace` **435 passed**（基线 433 + 本批 2）。

### 修复（文件搜索回落通道静默失败：`es.exe` 失败被当成「无结果」，2026-09-17）

- **现象**：IPC 不可用且 `es.exe` 也失败时，文件搜索页显示**空列表**（「无匹配结果」），**没有任何提示** —— 用户无从判断是「没搜到」还是「工具坏了」。
- **根因**：`run_es` 把 `es.exe` 的 stderr 丢弃（`Stdio::null()`）且**忽略退出码**，而 `parse_response("")` 被刻意设计为「空输出 = 无匹配结果」→ 二者叠加使「es 失败」与「真的没搜到」**无法区分**。实测失败形态：`es.exe` 以 **`rc=8`** 退出，错误只写在 stderr（`Error 8: Everything IPC not found. Please make sure Everything is running.`）。
- **修复**：新增纯函数 `interpret_es_output(status, stdout, stderr)` 收口判定 —— 仅 `rc=0` 按「无结果」处理（保持既有语义），`rc≠0` 一律转 `Err` 并**保留 es 的 stderr 文案**，由上层落成可读的 `files.error` 项（文案指向安装/运行 Everything 与 `DDRUN_ES_PATH`）；`run_es` 改为 **stdout/stderr 双管道并发消费**（避免写满管道阻塞）并带出退出码。
- **验证**：新增单测 `interpret_es_output_separates_no_result_from_failure`（5 个断言面，含「`rc=8` + 空 stdout 必须报错且带 stderr 文案与退出码」）；三关 `fmt` 无差异 / `clippy` 0 / `cargo test --workspace` **433 passed**。

### 修复（嵌套页标题显示原始 `page_id`，2026-09-17）

- **现象**：进入嵌套页后搜索框 placeholder 显示 **「在「files.results」中筛选…」** —— 把扩展内部的页 id 直接暴露给用户。
- **根因**：`open_page` 把 `page_id` **同时当作页标题**（`PageState::nested(page_id, page_id, …)`），而该标题被渲染进 placeholder。
- **修复**：标题改由**宿主侧信息**提供 —— 点击入口项进页 = **被点击项标题**；`f ` 前缀直达 = 本地化扩展名（`ext.name.filesearch`）；扩展 `GoToPage` 无来源 → 空 → placeholder 回落「筛选命令…」。同步修正 `navigation.rs` 中「标题来自 `PageInfo.title`」的过期注释（协议侧 `PageInfo` 定为**不传递**，见下）。

### 变更（协议两项定稿：`-32002` 与 `PageInfo`，2026-09-17）

- **`-32002 command_not_found`**：改为**终态口径** —— 该码**由扩展 handler 产出**（随指南发布的 Python 示例即产出）、**宿主与内置运行时不产出**（内置回 `ShowToast` 属合法实现选择）；不再是「保留码、待接线」。协议 v1.0 **零改动**（仅注记改写）。
- **`PageInfo`**：明确**维持不传递**（v1.0 预留定义）—— `GetItemsResult` 不携带页元信息，宿主页标题取自宿主侧信息；将来启用须走 §13 `MINOR`。

### 修复（`--conformance` 对 `has_fallback=false` 扩展误报红灯，2026-09-17）

- **现象**：对 `dd-ext-sample`（清单 `has_fallback: false`）跑 `--conformance`，第 `4) fallback` 步**必红** `-32601`，整体自检失败 —— 而该示例**完全合规**。
- **根因**：自检器无条件调用 `fallback_commands`；而 §6.2 的判据是「返回非空 ⟺ `has_fallback`」，扩展在 `has_fallback=false` 时**不必实现**该分发臂（宿主也不会调用）。
- **修复**：`has_fallback=false` 时**跳过**该步并明示原因；`has_fallback=true` 仍校验一致性。内置扩展（`has_fallback=true`）路径无回归，示例扩展自检由「红灯」变为 **9 步全过**。
- **顺带**：`dd-gui` 测试内的 `route_serve_line` 由手工复刻路由改为**委托生产实现** `dd_host::process::route_messages`（单一来源），消除「测试辅助与真实路由不同步」的潜在脆弱点。

### 修复（release 构建阻塞：A3 埋点 cfg 守卫漏配导致分发包无法产出，2026-09-16）

- **症状**：`cargo build --release` 在 `dd-gui` 报 `error[E0425]: cannot find value 'start' in this scope`（`crates/dd-gui/src/state.rs:395`）→ **单文件分发包自 2026-09-13 起无法产出**（`dist/` 长期停留在 `378b89d` 之前的构建，`tools/package.sh` 静默失败在宿主构建步）。
- **根因**：`378b89d`（perf：热路径去重复开销）按要求把 `recompute_visible` 的 A3 计时埋点改为 `#[cfg(debug_assertions)]`（release 免一次 `Instant::now()`/帧），但**只改了 `let start` 定义、漏改使用它的日志调用** —— release（`debug_assertions` 关闭）下该变量不存在。三关（fmt / clippy / test）与 CI 四关**均为 debug profile**，不覆盖 release 编译，故长期未暴露。
- **修复**：为日志调用补上同一 `#[cfg(debug_assertions)]` 守卫（`state.rs` +1 行），即补全 `378b89d` 已声明的设计意图；**行为不变**（该埋点本就仅 debug 构建输出，release 下不产生该次计时与日志）。
- **重新产出**：`bash tools/package.sh` → `dist/dd-run-0.1.1.exe`（8.70 MB）+ `dist/dd-run-0.1.1-portable.zip`（4.36 MB，含 `extensions.d/` sidecar 与清单）。
- **验证**：release 宿主启动正常（事件循环存活）；`dist/extensions.d/dd-ext-search.exe` 经协议驱动正常（`hint`/`empty`/`results` 三类响应齐全，IPC 往返 p50 11.27 ms）；`dd-run-cli --conformance --ext-id com.ddrun.calc` 9 步全绿；三关复跑 fmt 无差异 / clippy 0 告警 / `cargo test --workspace` 432 passed。

### 优化（窗口材质与边框：材质单选 pill + 不透明度滑杆 + 圆角/边框三选 P1–P4，2026-09-13）

- **设置卡更名「窗口材质与边框」**，参考 DeskBox「外观 → 窗口材质与边框」能力面（源码取证），原「云母/亚克力」两互斥开关行重构为四行结构；视觉契约与规格表见根目录 `cmdpal-window-material-effects.html` 效果页。
- **P1 材质单选化**：两互斥开关 → 三段 pill「无材质 / 云母 / 亚克力」（复用 F2 密度卡 pill 口径），选中值仍落单值 `backdrop`，切换防闪链路（倒计时清 DWM）不变。
- **P2 不透明度滑杆（v2 直控式，同日真机反馈修订）**：新增 0–100% 滑杆，语义 = **面板底不透明度直控** `alpha = cap(主题,材质) × pct/100`（cap = 暗·云母 0.75 / 暗·亚克力 1.0 / 亮·云母 0.25 / 亮·亚克力 0.45）——0% = 纯材质，100% = 面板最实（亮色 cap 刻意压低，延续 M1「厚白浓淡=白漆」防线）。**默认 40% 精确复现 M1 真机调定四档锚点**（0.30/0.40/0.10/0.18，默认观感零变化）。修订原因：初版 `基准 × (0.25 + 0.75×pct/100)` 在云母上有效带仅 0.075–0.30，叠在近乎不透明的 DWM 云母上感知极弱（真机反馈「调透明度效果不明显」）；参考 DeskBox 强度滑杆直扫 tint 全带（云母 0.04→0.46）改为全带直控。只走 egui 浓淡层重注册，不碰 DWM，零闪烁面；拖动即时生效不落盘、松手落盘一次（egui 0.36 为 `drag_stopped`）。P2 未发版，换算重定义无迁移。
- **P2 v3 浓淡层基色带系统强调色 + v4 亮色增强（同日显示对齐反馈）**：tint 层基色由纯面板色改为**面板色 × 系统强调色混合**（暗 8% / 亮 30%，DeskBox `BuildContentTintColor` 配方 + 亮度层补偿；复用 P4 的 `DwmGetColorizationColor` 取色，取不到回落纯面板色）。真机截图对比：纯中性白浓淡把 DWM 云母「洗」成无材质感的白平面，同桌面下 DeskBox 呈灰蓝材质面——带色基色后浓淡层在任何 alpha 下都有材质色相；v4 进一步把亮色 cap 放宽（云母 0.25→0.55 / 亚克力 0.45→0.70，默认档 0.10/0.18→0.22/0.28）：v3 带色基色已根治 M1「厚白浓淡=白漆」问题，亮色可安全加厚逼近 DeskBox 存在感（仍低于暗色一档保留采色语义）。`theme::tint_color` 单点收口，visuals 契约单测同步。
- **P3 窗口圆角三选**：圆角 / 小圆角 / 方角（`DWMWCP_ROUND / ROUNDSMALL / DONOTROUND`），默认圆角 = 现状；改选即时重设 DWM 属性（幂等），描边自动贴合新形状（DWM 自绘）。
- **P4 面板边框三选**：中性 / 强调色 / 关（`DWMWA_BORDER_COLOR`），默认中性 = 现状；强调色 = `DwmGetColorizationColor`（系统强调色等价近似，不引 WinRT；取不到回落 `Palette::accent`），跨主题恒色；描边色经 `border_color()` 单点收口，`apply_theme_pref` 同步点同源（中性随明暗、强调色不变）。
- **兼容与降级**：旧配置 `material_opacity`/`corner_pref`/`border_mode` 三键缺失 → 默认值（观感逐像素一致），越界 clamp、未知字符串回落（单测覆盖）；材质未生效（Win10 / 22621-）时滑杆与边框行置灰、圆角行恒可用。明确非目标：MicaAlt、AcrylicBase、边框粗细、材质强度第二滑杆、Win10 旧版亚克力（理由见效果页「非目标」节与 implementation.md P1–P4 小节）。
- **P2 v5 两主题统一 cap + 行填充材质适配（同日反馈）**：①cap 改为按材质定档（云母 0.75 / 亚克力 1.0，不再分主题）——亮色独立低档（0.55/0.70）压低了材质存在感（真机对比 DeskBox 仍差一档），带色基色下统一后亮色默认档 0.22/0.28 → **0.30/0.40**；②结果行填充由单一 `row_hover_glass` 改为 `theme::row_fills` 材质适配对（hover + selected 成对）——同一块 31% 白/灰玻璃在近白云母上不可辨（「不明显」）、在亚克力动态模糊上形成亮块（「太突兀」），现 hover 云母加权（亮·黑 7% / 暗·白 7.8% 玻璃）、亚克力减重（3.5%/3.9%）；**selected 材质档同走玻璃**（比 hover 重一档，回退仍实色）——原不透明 `row_selected` 实色块在半透明材质上随鼠标扫动逐行蹦跳，即真机反馈「鼠标滑动时应用栏目闪烁」，玻璃化后扫动为半透明层平滑过渡（accent 竖条不变，选中辨识度不变）；③**底部页脚同步浓淡层**——页脚原为纯 TRANSPARENT（D31 时代面板无 tint 的一致态），M1 起面板 tint 只由 CentralPanel 绘制、不覆盖页脚矩形，页脚带露出的是未 tint 的原始材质，形成底部色差带（真机反馈「底部栏未同步修复效果」），改为与面板同源 `panel_fill`，材质态页脚与面板一体（回退仍 `panel_2` 实色）；卡片/设置项等其余 hover 不变。
- **P2 v6 设置卡控件设计风格对齐（同日反馈）**：①不透明度滑杆改自绘 Fluent 规格（egui 默认 Slider 的细灰轨 + 行内百分比后缀与整套 pill/开关控件脱节）——轨 4px 圆角 2（未选段 `--border` / 已选段 accent_stroke 小面积强调口径）、钮 16px 白底 border-strong 描边（悬停/拖动加粗 2px），百分比值右对齐行头，禁用置灰不响应；②修复边框三选 pill **竖排**排版缺陷（`add_enabled_ui` 闭包内漏包 `horizontal`）；③`draw_density_pill` 增 `dim` 置灰参数（去 accent、标签 text3、无 hover 反馈），补齐禁用态视觉。
- 零新依赖（`Win32_Graphics_Dwm` 特性既有）；`cargo test --workspace` 全绿（dd-gui 187，新增 settings/theme/hover 单测 5 项），编译 0 警告；真机验收（材质/浓度/圆角/边框矩阵 + Win10 降级）待做。

### 优化（字体与材质显示：F3 拉丁主字纠偏 + M1–M4 材质浓淡/描边/圆角/禁过渡，2026-09-13）

- **F3 字体（参考 DeskBox = WinUI 3 系统排印管线）**：Proportional 族显式重排为 `[segoe, NotoEmoji-Regular, emoji-icon-font, cjk, sym, icons]`——拉丁/数字主字由 egui 内置 **Ubuntu-Light** 纠偏为 **Segoe UI**（此前英文应用名/路径/设置页全用 Ubuntu-Light，与行名 semibold 拉丁的 Segoe UI Semibold 同屏异字体混排）。重排仅动拉丁一处：CJK/emoji/Geometric Shapes（◌）/PUA glyph 回退路由逐一不变；缺文件成员自动剔除（降级安全）。semibold 链继承新序，regular/semibold 从此同为 Segoe 系。落点仅 `platform.rs`（约 20 行 + doc），无字号/密度变化（F1/F2 成果不动）。
- **M1 材质浓淡层**：材质生效时面板底由纯 `TRANSPARENT` 改为**面板色半透明浓淡层**（云母 ×0.35 / 亚克力 ×0.50，`theme.rs` 常量 `TINT_ALPHA_*` 可调）——DWM 材质仍从底下透出，面板轮廓与内容对比度不再随壁纸漂移。D31 切换防闪逻辑不变；`visuals/apply/apply_panel_tint` 签名 bool → `Option<f32>`（3 处调用点同步）。
- **M2 材质描边**：材质生效时 `DWMWA_BORDER_COLOR`（Win11 文档化属性）画 1px 外描边，取色复用 `border_strong` token（暗 `0x666666`/亮 `0xd1d1d1`），跟随 M3 圆角形状；主题切换在 `apply_theme_pref` 同步；材质关闭/Win10 恢复不描边。
- **M3 圆角显式化**：`DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND`（≈8px，对齐设计 token `window_corner_radius=8`），HWND 捕获后经 `refresh_backdrop` 一次性应用——消除无边框工具窗圆角「随系统判定」的不确定性。
- **M4 禁过渡**：`DWMWA_TRANSITIONS_FORCEDISABLED`——唤起/隐藏瞬时（A1），不随系统「窗口动画」设置漂移。DeskBox 的动画开关不做：启动器无「要动画」需求面，记档直接禁用。
- 均无新设置项/依赖/配置迁移；无材质与 Win10 路径观感不变。`cargo test -p dd-gui` 181 通过、`cargo clippy` 全仓 0 警告；真机截图验收（亮暗 × 三材质矩阵）待做。
- **真机反馈修订一（同日）**：「不用加粗字体」——`theme::semibold()` 回落 **regular 字形**（函数与全部调用点保留，恢复字重只改一处）；`seguisb.ttf` / `msyhbd.ttc` 加粗字体**停载**（热替换少读 ~19MB，更快），`SEMIBOLD_FAMILY` 保留注册指向 regular 链防未知族；0.05em 标题字距作为排印规格保留。
- **真机反馈修订二（同日）**：「云母/亚克力材质没有整个面板和设置页生效」——像素取证（设置页背景 RGB 带壁纸色偏 G/B>R）确认材质**其实已激活**，被 M1 初版单值浓淡层洗白 + 设置卡实色遮挡，属感知问题非链路故障。修复：①浓淡层**分主题四档**（亮 云母 0.10 / 亚克力 0.18；暗 0.30 / 0.40——亮色厚白浓淡等于白漆）；②材质生效时**设置卡片玻璃化**（`theme::card_fill`：card 色 × alpha 暗 190 / 亮 210，材质从卡面透出）；根页行/搜索框/页脚本就透材质，不受影响。`cargo test` 182 通过、clippy 0 警告。

### 文档（文档 ↔ 代码差异清单修复：71 条差异经三路复核验真后逐条落实，2026-09-13）

- **核对与修复记录**：新增 [`docs/doc-code-diff-2026-09-13.md`](./docs/doc-code-diff-2026-09-13.md)（v1.1，已登记 INDEX.md）——P/M/E/I/D/R/S 七系列 71 条差异，三路独立复核验真（修正清单自身 9 处）后按「文档对齐代码」落实约 84 处修复。规范类文档未实现契约**保留条文**、加「⚠️ 实现现状」注记；含 3 处纯注释代码修正（search.rs 超时注释、builtin.rs / aggregator.rs 陈旧注册注释），无行为变更。
- **要点**：`search.md` 移除与 A-33 红线冲突的「无需 es.exe」承诺（高严重度 S-01）；`protocol.md` §9.2 `-32002` 归因更正（兜底在各扩展 handler，Python 示例确实回该码）；`extensions.md` Python 示例补 `stdin.reconfigure(utf-8)`（中文 query 乱码隐患）；README 双语 M7–M9 关账同步；release.yml 「内嵌」表述改「in-process」。
- **遗留待人工确认**：超限消息「回 `-32600` + 关连接」是否补实现（P-01）、方法名常量层（P-18）等，见 INDEX.md §5。

### 修复（返回后搜索框失焦：嵌套页返回重新聚焦，2026-09-12 真机反馈）

- **现象**：从文件搜索页（或任意嵌套页/设置页）返回后，搜索框没有焦点——无法直接键入，须先点一下输入框。
- **修复**：新增 `PaletteApp::go_back_focused()`（出栈成功即置位 `want_focus`，由搜索框绘制统一消费），Esc、嵌套页「←」按钮、设置页返回、扩展 `GoBack` 动作四条返回路径统一走它；已在 Root 时返回 `None`（隐藏语义不变）。

### 撤销（「优先搜索文件」开关：同日加入、同日移除，用户反馈）

- **原因**：自动进页会**劫持常规搜索**——开启后输入任意关键词都被弹进文件结果页，应用/网页搜索被遮蔽，交互过强。
- **移除范围**：`Settings.prioritize_files` 字段与 JSON 读写、`file_search_drill_target` 的优先模式分支（回到纯 `f ` 前缀语义）、设置页开关行、i18n 文案。**保留**：`f ` 前缀直达文件搜索、进页回填（上一条修复）、「搜索应用」开关。配置中残留的 `prioritize_files` 字段按未知字段忽略，零迁移。

### 修复（搜索引擎配置失效：M9 in-process 后环境变量通道断裂，2026-09-12 真机反馈）

- **现象**：设置页删除引擎后（如只留 Bing），返回首屏「网络搜索」仍显示**全部 5 个引擎**。
- **根因**：M9 把内置 websearch 从 spawn 子进程改为 **in-process**（`serve_line` 直调），但引擎配置通道是进程环境变量 `DD_WEBSEARCH_ENGINES`——宿主只把它写进 manifest `entry.env`（**spawn 子进程路径才消费**），in-process 扩展在宿主进程里 `std::env::var` 永远读不到 → 配置被静默忽略、恒回落内置全表。
- **修复**：dd-ext 新增**内存注入通道** `websearch::set_configured_engines_json(Option<String>)`（`RwLock`，优先级：内存注入 → 环境变量 → 内置默认）；dd-gui 聚合线程（首启与重聚合同路径）在 collect 前注入 `Settings::search_engines_env()`。独立 exe 运行（无宿主注入）仍走环境变量，通道②保持兼容。
- **语义修正**：引擎 JSON 为**合法空数组**时原会回落内置全表（「全部关闭」关不干净）——现返回空表尊重用户意图；仅非法 JSON / 非空数组条目全非法才回落默认。
- **验证**：`cargo test -p dd-ext` 77 通过（新增 resolve 优先级 / 注入往返清除 / 空数组语义 3 条），`cargo clippy` 0 警告。

### 修复（文件搜索进页回填：根查询不再丢失，2026-09-12 真机反馈）

- **现象**：根页输入「测试」后点「文件搜索」进入 `files.results` 页，页内搜索框**为空**——根查询虽已作为 `search` 传给 `get_items`（扩展按其过滤），但页内输入框不显示，用户须重打一遍。
- **修复**：`open_page` 统一回填——带非空查询进页时把查询写入页内搜索框（`f ` 前缀 drill 原有的单独回填随之收编为同一路径）。落地后 `poll_page` 的 v3.3 保留逻辑沿用该值：页内 query 与请求 `search` 一致 → 不触发过期补偿重拉（零多余请求）。所有 `Page` 进页路径（列表点击/Enter、`f ` 前缀、页内嵌套进页）语义一致。

### 新增（搜索行为设置：「搜索应用」开关 + 默认仅 Google，2026-09-12 用户决策；「优先搜索文件」后撤销见上）

- **设置 → 搜索新增「搜索应用」开关卡**（开关行，排版同「窗口材质」卡）：
  - **搜索应用**（默认开）：关闭后「应用」类项在根页空查询首屏与关键词匹配中**均排除**（`PanelState::set_apps_hidden`，两条可见表分支都过滤；聚合落地与应用内切换两处接线）。`Settings.search_apps` 持久化；旧配置缺字段/类型损坏回落默认。
- **默认搜索引擎只开 Google**：`Settings::default().search_engines` 由预设 5 引擎改为**仅 Google**（新增 `default_search_engines()`；`preset_search_engines()` 保留为设置页「添加引擎」下拉的可添加目录）。已落盘配置（含完整引擎列表）不受影响——只改默认值。
- **验证**：`cargo test -p dd-gui` 通过（新增：apps 隐藏过滤 1 条、search_apps JSON 往返 1 条、Google 默认 1 条改写），`cargo clippy` 0 警告。

### 新增（图标与字体展示优化：参考 DeskBox 的「图标/文字大小可调」，方案 [`docs/icons-typography-plan.md`](./docs/icons-typography-plan.md)）

- **列表密度三档（F2）**：设置 → 外观新增「列表密度」卡（紧凑/标准/宽松），一档联动行高（36/40/44）、行名与标签字号（13/14/15、11/12/13）、图标格与 glyph 字号（20/24/28、16/20/24）——`theme::ListMetrics` 单点定义，标准档硬性等于既有常量（parity 单测守卫，默认观感与上一版逐像素一致）。`Settings.density` 持久化（手工 JSON 读写沿用 `label/as_str/parse` 枚举模式，旧配置缺字段/未知值回落标准档，零迁移）。**关键联动**：面板默认高度折算 `base_height_for_workarea` / `root_panel_size` / `settings_panel_size` 增 `row_h` 参数（`lifecycle.rs` 唤起与 `ui()` 页面 diff 两处调用点接线），宽/松档默认窗口不会一行显示不下；Loading 骨架行随档同源，加载完成无布局跳动。设置页三选 pill 复用 radio-card 视觉口径（选中 accent_soft + 2px accent_stroke，hover `control_hover` B7 几何判定），zh/en 文案各 5 条。
- **无图标/url 项回落占位 glyph（I1）**：结果行图标列由「空列」改为极弱色（text4）占位 glyph（U+E7C3）——对齐不变，观感从「图标缺失」变为「本来就没有」；与解码失败的 text2 占位区分层级。修订设计稿 04「空列」决策（代码注释同步记档）。
- **glyph 用色显式化（I2）**：图标格 glyph 改为显式取 `Palette::text2`。核实发现 `visuals.weak_text_color` 已被 theme.rs 显式接管为 text2（weak 档≠更弱），**行为零变化**，仅消除经由 states 辅助函数的间接引用（该函数已无调用者，删除）。
- **列表排印 token 化（F1）**：`theme.rs` 新增 `LIST_TITLE_PT/LIST_CAT_PT/LIST_ICON_GAP/LIST_ICON_CELL/LIST_GLYPH_PT`，列表绘制路径（row.rs / icons.rs）全部引用常量——初值 = 现状硬编码值，纯收拢零观感变化，为 F2 铺路。设置页/页脚字号（各有机理与单测锚点）有意不动。
- **明确不做**（同 DeskBox 的取舍，理由见方案 §3/§4）：256px Shell 图标源（48px 源对 24px 格是精确 2×）、url favicon 网络下载（M5 决策不翻案）、两行文件名（与 B5 单行 + tooltip 决策冲突）、逐组件独立字号。
- **验证**：`cargo test -p dd-gui` 178 通过（新增 density 往返/回落 1 条、ListMetrics parity 1 条、密度档高度折算 1 条），`cargo check --workspace` 干净。

### 新增（M8 扩展生态验证：非 Rust 扩展走通协议全链路，协议 v1.0 零改动）

- **扩展开发指南** [`docs/extensions.md`](./docs/extensions.md)：面向第三方开发者的「30 秒心智模型 → 10 分钟上手」路径。含**三条铁律**（stdout 只出协议消息 / UTF-8 且行内无裸换行 / 通知不回复）、**三个 Windows 陷阱**（文本模式的 `\n`→`\r\n`；stdout 默认编码可能是 cp936 而非 UTF-8，后者不容忍）、各方法按「值得实现」排序、`host/*` 反向请求（不必阻塞等应答）、自检说明、**调试手册（8 条症状→原因对照）**、崩溃语义与超时预算。只写规范里没有的东西，重复处一律引用 `protocol.md` / `manifest-schema.md` 并声明「冲突以规范为准」。
- **Python 全表面示例** [`examples/python-minimal/`](./examples/python-minimal/)：仅标准库的 Python 3 扩展，覆盖 7 个 host→ext 方法 + `items_changed` 通知 + 3 种 `host/*` 请求 + `ShowToast`/`Dismiss`/`Confirm`（含确认后重发 `invoke` 的幂等两段式）。附 Windows `.cmd` 启动器——因为清单的 `entry.args` **不支持** `${EXT_DIR}` 展开，无法表达「解释器 + 脚本」两段式命令行；实测宿主 `resolve_executable` 会为无扩展名路径补 `.cmd`，且 Rust `Command` 能直接 spawn `.cmd`。
- **`dd-run-cli --conformance`**：把扩展自检从 4 步（`--roundtrip`）扩到**全表面**——新增 `fallback_commands` 一致性（非空 ⟺ `has_fallback`）、`get_command` 可复热性（顶层 id 不得返回 `null`）、`get_items` 结构、`invoke` 返回值 ∈ §8.3 八种 `kind`、`host/*` 声明一致性、`close`。逐项输出 `✓ / ⚠ / ✗`，**任一 ✗ 退出码非 0**。新增 `--invoke`（`invoke` 有真实副作用，默认跳过）与 `--ext-id`（目录内有多个扩展时指定）。自检刻意基于**原始 JSON** 而非强类型反序列化——规范约束的是线上形状，强类型成功反而掩盖多字段/错类型问题。新增单测 3 条（兜底一致性四象限 / 8 种 `kind` 无重复 / 诊断截断按字符）。
- **实证**：`dd-run-cli --conformance --extensions-dir examples/python-minimal --invoke` → **10 项全 ✓（2386 ms）**，即**一个非 Rust 扩展走通了协议全链路**——「协议与语言无关」由声明变为事实。

### 修复（PyMin 示例：GUI 下无法启动 —— 启动器依赖 PATH，2026-09-10 真机反馈）

- **现象**：示例装进扩展目录后能在「设置 → 扩展」看到，但标为**暂时不可用**且点「重试」无效。
- **根因**：`dd-ext-pymin.cmd` 调用**裸 `python`**，依赖解释器位于 PATH 上；而 **Python 不在 Windows PATH**（`where python` 无输出）。GUI 从资源管理器启动时继承的是**系统 PATH** → cmd 报「'python' 不是内部或外部命令」→ spawn 失败 → 连续 3 次熔断。自检之所以全绿，是因为它从**终端**启动、PATH 里恰好有解释器。
- **修复**：示例改为**安装脚本生成清单**（`install.py`）——把 `sys.executable` 与脚本的**绝对路径**写进 `entry.command` / `entry.args`（宿主直接 spawn、不经 shell → **零 PATH 依赖**）。移除示例目录里的静态清单（一份固定清单对解释型扩展写不对，且正是假阳性来源）；`.cmd` 降级为**备选**，文件头写明前提与陷阱。
- **指南同步**：`docs/extensions.md` 第 4 步改为推荐绝对路径清单；§3 由「两个 Windows 陷阱」扩为**三个**（新增「解释器不在 PATH 上」，并点明 `--conformance` 全绿 ≠ GUI 能启动——两者继承的环境不同）；§8 症状表补两行（「暂时不可用 + 重试无效」、「终端全绿但 GUI 启动失败」）。
- **验证**：把 PATH 摘到只剩 `System32`（**完全无 python**）后 `dd-run-cli --conformance --ext-id com.example.pymin --invoke` → **10 项全 ✓（902 ms）**，证明清单已与 PATH 解耦。
- **真机确认（2026-09-10）**：用户点「重试」后扩展正常启动，PyMin 命令在 GUI 内可用。

### 改进（扩展失败原因可观测：Failed 原因透出到卡片与日志，2026-09-10 真机反馈驱动）

- **问题**：扩展启动失败/崩溃时用户只能看到「暂时不可用 + 重试」，**根因无处可见**——`SourceStatus::Failed.error` 字段早已存在，但设置页只读 `is_failed()` 布尔、**从未渲染过**；而崩溃路径（`refresh_health`）在丢弃进程前**没有读取已捕获的 stderr**（§2.5 本来就抓了），只打一行「已退出」。两次真机排查（解释器不在 PATH、`invoke` 响应形状）都因此绕了远路。
- **`dd-host`**：新增 `ExtensionProcess::failure_detail()`（`退出码 N / 被信号终止；stderr: <末行>`）与 `stderr_last_line()`；配套纯函数 `last_nonempty_line`（取**末条非空行**、按**字符**截断 + 省略号，中文不破字、CRLF 的 `\r` 被 trim）与 `compose_failure_detail`（两部分皆空 → `None`，不产出空壳提示）。新增单测 3 条。
- **`dd-gui` 失败信息带可操作线索**：`spawn 失败` 附**被尝试的命令路径**（PATH / 路径类问题一眼可见）；`initialize 失败` / `top_level_commands 失败` 附 stderr 末行（`（诊断：…）`，无诊断不留空括号）。`refresh_health` / `invoke` / `get_items` 三处崩溃路径**在进程 drop 前**取出摘要，并入日志与 Failed 原因。新增单测 1 条（后缀拼装）。
- **设置页扩展卡片**：新增第三行**失败原因**（11px `danger` 色，单行截断 + 悬停看全文）；`重试` 按钮的显示判据改由失败原因驱动；行数据抽成纯函数 `extension_rows` + `failed_reason`，新增单测 1 条（Failed 透出 / warm·stub 无原因 / 停用集决定开关）。
- **协议 v1.0 零改动**：纯宿主侧可观测性。**效果**：同类问题定位从「抓包两轮」降为「看一眼卡片」。

### 修复（invoke 响应形状：同一个错误被文档/类型/单测/自检器四处固化，2026-09-10 真机反馈）

- **现象**：PyMin 示例装好后，`当前时间` / `添加便签` / `清空便签` / `打开文档` 四条命令点击即报 `命令执行失败：非法 JSON-RPC 信封：missing field ``kind```（`便签列表` 是嵌套页命令、不经 `invoke`，故不受影响）。
- **根因**：§6.5 规定 `invoke` 成功的 JSON-RPC `result` 字段**就是** `CommandResult` 本体（**单层**），但有**四处**都写成了多一层的 `{"result":{"kind":...}}`——`docs/protocol.md` §6.5 与 §14 的示例、`dd-protocol` 的 `InvokeResult` 结构（`result` 域内再嵌一层）、协议一致性单测 `consistency.rs` 的 §6.5 断言、以及 M8 新写的 `docs/extensions.md` 代码示例。M2 已在**宿主侧**修掉（改为直接解析 `CommandResult`），但文档/类型/单测未同步——于是照着文档写的 Python 示例必然踩中。
- **自检器同样是错的**：`--conformance` 的 `invoke` 判据读的是 `v["result"]["kind"]`，**与错误形状恰好吻合** → 对不合规的扩展判绿。两端互相印证，形成"绿灯骗局"。
- **修复**：① 示例改为 `reply(mid, result)`（单层）；② 协议文档 §6.5 / §14 示例改单层并加显式告诫；③ **删除虚构的 `InvokeResult`**（工作区内除一致性单测外无引用），单测改为断言 `Resp<CommandResult>`；④ `--conformance` 判据改判内层 `kind`，并抽成纯函数 `command_result_kind`——**显式识别并指名「多包了一层」**；⑤ 指南 §5.4 明确「信封的 `result` 就是 CommandResult 本体」，§8 症状表补该条。
- **验证**：抓包探针确认四条 `invoke` 响应均为单层；`--conformance --invoke` **10 项全 ✓**；**反向验证**——把示例改回双层后，自检器正确报 `✗ result 被多包了一层...`（修复前对同一份扩展判绿）。新增单测 1 条（单层通过 / 双层被指名 / 缺 `kind` / 非对象）。
- **教训**：形状类契约一旦「文档 + 类型 + 单测 + 工具」四方同错，就没有任何一处会报红——**必须让至少一个环节面对真实线缆**（本次是抓包探针）。协议 v1.0 零改动（只修错误的描述与断言，不改线上契约）。
- **真机确认（2026-09-10）**：修复版脚本装入 `dist/extensions.d/` 后，用户实测四条 `invoke` 命令（`当前时间` / `添加便签` / `清空便签` / `打开文档`）在 GUI 内**全部正常执行**。

### 性能（方向 C 打磨：warm 进程空闲超时回收，协议 v1.0 零改动）

- **warm 进程空闲超时回收**：`LRU_WARM_CAPACITY`(8) 大于扩展总数（内置 5 + 官方 sidecar 1 = 6）→ LRU **永不触发驱逐**，保活集行为上"只增不减"（稳态常驻全部扩展）。`LruWarmSet` 增空闲判定：每次触达记录「最后触达时刻」，`idle_victims(ttl)` 只读返回空闲超阈值者；宿主 `warm_idle_reclaim()` **复用既有驱逐路径**（`close` + 回落 stub），下次使用走桩复热。
- 阈值 `WARM_IDLE_TTL = 120s`；**仅在面板隐藏时**回收（用户正在看列表时不回收）；在途请求（进程已 take 出保活集）跳过；隐藏期以 `WARM_IDLE_HIDDEN_TICK = 15s` 请求重绘驱动巡检（保活集清空后自动停止请求，恢复静默）。
- 新增单测 6 条：`LruWarmSet` 3 条（触达刷新不判空闲 / `ttl` 边界「≥ 即回收」/ 队尾优先 + `last_access` + `remove` 生效）、宿主 2 条（隐藏态仅回收超阈值者且源回落 Stub、可见态不回收）、兜底模板识别 1 条。

### 修复（L5：LRU 驱逐后点兜底模板必失败，协议 v1.0 零改动）

- **现象**：扩展被驱逐成 stub 后，点击其**兜底（模板）项**必然失败——报「命令已失效」，且 `invoke` 从未发出。
- **根因**：协议 §6.4 的 `get_command` **只查顶层命令**（`dd-ext/src/lib.rs`），而兜底模板 id（如 `calc.eval.query`）天然不在顶层注册表 → 桩复热链路拿到 `Ok(None)` 即判「陈旧」并短路。
- **修复**：`FallbackStore` 新增 `contains_template(ext_id, id)`；`dispatch_invoke` / `dispatch_fetch_page` 判定为模板时**跳过 §6.4 校验直接执行**（invoke 侧走 `skip_lookup`，page 侧传 `command_id=None`，等价「无对应命令点击」语义）。**常规命令保持 §6.4 陈旧校验不变**。
- 该缺陷此前不触发（`LRU=8` > 扩展数 6，永不驱逐）；本次空闲回收让驱逐真实发生，故与上一条**同批修复**。

### 新增（M9 内置扩展进程内化：单文件分发，内置不再 spawn 子进程）

- 5 个内置扩展（apps / calc / system / websearch / shell）自 M9 起改为**宿主进程内**运行：宿主直接驱动 `dd_ext::serve_line`，不再 spawn 子进程、不再内嵌扩展 exe（`dd-gui/build.rs` 内嵌空表，`assets/embed/` 仅遗留目录）。
- 发版产物收敛为**单一宿主文件** `dist/dd-run-<ver>.exe`（不再含内置 exe）；文件搜索 sidecar（`dd-ext-search.exe`）仍按 ADR-1 以子进程运行，随绿色包 `dist/extensions.d/` 携带。
- 配套：`dd-run-cli --conformance` 对内置扩展改走 in-process 分支（直驱 `serve_line`）；websearch 引擎配置改由宿主内存注入（`set_configured_engines_json`），不再依赖 spawn 子进程才生效的 `DD_WEBSEARCH_ENGINES` 环境变量（详见上文「搜索引擎配置失效」修复）。
- 协议 v1.0 零改动（in-process 与子进程走同一套 NDJSON 路由，字节等价）；`tools/package.sh` 改为 M9 单文件出包流程（见仓库 `README.md` 构建与打包一节）。

### 新增（apps 扩展垃圾项过滤：安装附属工具黑名单 + 同 exe+参数去重 + AppsFolder 伪应用拦截）

- 应用扩展结果新增**垃圾项过滤**：屏蔽安装器写入的「附属工具 / 卸载程序」等噪音项、按「同 exe + 参数」去重、拦截 `Applications` / `AppsFolder` 伪应用条目，使根视图与关键词匹配结果更干净。

### 性能（运行时内存优化 M1+M2+M3：隐藏后工作集修剪 + 字体栈 mmap 化 + 图标纹理缓存清空）

- 面板隐藏后执行**工作集修剪**；字体栈改为 `mmap` 化（按需分页，降低常驻内存）；图标纹理缓存由宿主在隐藏态清空，避免长期占用显存/内存。三项合计降低空闲期内存占用（协议 v1.0 零改动，纯宿主侧）。

### 文档

- README 双语修正**跨平台宣称口径**：原文「cross-platform (Windows / macOS / Linux)」是设计目标而非现状，改为「核心与平台无关；**当前仅发行 Windows 版**，macOS / Linux 计划 v0.2+」；对比表表头同步为「平台无关设计」，Goals 首条标注「计划中，当前仅 Windows」。

## [0.1.1] - 2026-09-10

### 新增（文件搜索 v3.3 P2：everything-ipc 主通道，es.exe 降级为回落，协议 v1.0 零改动）

- 传输层从「每次按键 spawn `es.exe` 子进程」升级为 **`everything-ipc` crate 直连 Everything IPC**（WM_COPYDATA，纯 Rust，**无外部进程、无 DLL 随包**）为**主通道**；`es.exe` 降级为**回落通道**（主通道不可用 / 查询失败 / 超时自动回落）。
- 依赖形态（`crates/dd-ext/Cargo.toml`）：`everything-ipc = { version = "=0.1.4", default-features = false }`，置于 `[target.'cfg(windows)'.dependencies]`（该 crate 无平台守卫，仅 Windows 可编译）；版本锁死 `=0.1.4` 防 0.1.x 年更 API 漂移，默认 feature 为空（不引入 tokio/pe/folder）。
- client 采用 `EverythingClient::shared()`（全局 Weak，Arc 全释放后自动重建）+ 本模块 `Arc` 缓存；**连续失败 3 次即释放缓存触发重建**，使 Everything 重启 / 实例切换后能从回落态恢复到 IPC（§9.4 A-33-10）。
- 显式 `timeout(1200ms)`（< 宿主 `get_items` 2000ms）；`RequestFlags = FileName|Path|Size|DateModified|Attributes`，目录用 `Attributes & 0x10` 精确判定、修改时间用 `FILETIME`（复用 `filetime_to_unix`）。
- 可用性探活改为 **IPC 轻量探活优先**（`FindWindowW` + `is_ipc_available` + `is_db_loaded`，不建 client / 不起线程），回落 `es.exe -get-everything-version`。
- 抽出两通道共用的纯函数 `make_entry()` / `combine_filetime()`，新增 4 条 L1 单测（FILETIME 高低位合并 / 属性判目录 / 无属性启发式 / FILETIME→Unix）。
- **真机验证**：L2 NDJSON 冒烟——`get_items("cargo")` 返回 30 条真实索引结果，stdout 全为合法 NDJSON（**无 tracing 污染**），stderr 无回落日志（即 IPC 主通道生效）。**协议 v1.0 零改动**。
- 说明：`es.exe` 仍保留为回落通道；待真机验收 A-33-06（回落）/ A-33-10（恢复）/ A-33-08（版本矩阵）通过并发布后，普通用户方可不再安装 `es.exe`。

### 新增（文件搜索 v3.3 P1：上下文菜单三动作，协议 v1.0 零改动）

- 文件结果项新增 **`more_commands` 上下文菜单**（§8.1）：右键 = 打开（默认）/ **显示所在目录**（`files.reveal.{pid}`，扩展侧 `explorer /select` 经 `raw_arg` 规避引号坑）/ **复制路径**（`files.copy.{pid}`，Toast + `host/set_clipboard`）。
- 宿主右键菜单支持渲染扩展声明的 `more_commands`（`PanelItem` 透传），激活时以 `sender=context_menu` + `context.selected_item_id` 回调 `invoke`；扩展自带动作时 GUI 静态类别映射让位（避免重复）。
- `spec()` 与 sidecar manifest（`examples/extensions.d/com.ddrun.filesearch.json`）capabilities 同步追加 `host/set_clipboard`（能力前置 §7.4：未声明将被宿主回 `-32601`），启动时一致性断言覆盖。

### 新增（文件搜索：结果项按文件类型显示图标，协议 v1.0 零改动）

- 文件结果项图标从「目录/文件两态」升级为**按扩展名 12 类映射**（`crates/dd-ext/src/bin/search.rs` 的 `file_category` + `category_glyph`）：文档 / 代码 / 压缩包 / 图片 / 音频 / 视频 / 可执行 / 配置 / 字体 / 电子书 / 文件夹 / 兜底 Page（未收录扩展名、无扩展名、dotfile 均回落 Page）。
- 引导项（`files.hint` / `files.guide` / `files.error`）与顶层 / fallback 入口统一为**搜索图标**（Segoe `Search` U+E721），文件搜索链路图标语义一致。
- 全部 13 个码位已核验在两代图标字体（Win11 `SegoeIcons.ttf` / Win10 `segmdl2.ttf`）的 cmap 中存在；映射为扩展端纯查表（零 IO、零新依赖），不影响 <100ms 搜索响应红线。**协议 v1.0 零改动**（复用 `Icon::Glyph`）。
- 新增 L1 单测 7 条：类别映射 / 大小写不敏感 / dotfile·尾点·多重点边界 / 12 类码位两两互异 / 目录优先 / 图标随类型 / 入口统一搜索图标。

### 修复

- 引导项 `files.hint` / `files.guide` / `files.error` 点击给出**专属文案** Toast（此前落回通用「未知命令」）；`files.guide` 的 subtitle 同步改为 es.exe 通道文案（移除过时的「启用 HTTP 服务器」表述）。
- 嵌套页支持**边打边搜**（200ms 去抖重拉 `get_items`）+ 过期结果补偿 + 落地保留页内 query；嵌套页跳过宿主本地二次过滤（`passthrough`），进页自动聚焦搜索框。
- 文件搜索结果右键菜单与页脚 Enter 提示的默认动作文案从「执行」改为「打开文件」（**遗漏修复**：`footer_action_text` 的 ext_id 映射表自 `com.ddrun.filesearch` 加入以来未同步，文件项被回退到 `footer.invoke`；实际 `files.open.{pid}` 走 `host/open_url` 调 ShellExecute 用系统默认程序打开，与 `apps` 扩展「打开应用」同语义）。`text.rs` 新增 `footer.open_file` i18n 条目，`footer_action_maps_five_builtin_extensions` 与 `ctx_menu_more_commands_override_static_mapping` 测试期望同步。
- 文件搜索结果按 Enter 真正「打开文件」（v3.3 P1.5 修复）：此前 `host/open_url` 对 `file://` URL 一律走 `webbrowser::open`——目录→浏览器索引页；`.txt`/`.html`/`.mp4` 等走浏览器而非关联程序；`.exe`/未知扩展名浏览器报错。改为检测 `file://` 协议后改走 `ShellExecuteW(verb="open")` 自动派发（**目录 → Explorer 窗口；文件 → 关联程序**），`http(s)://` 保持 webbrowser 默认浏览器（websearch 不受影响）。**协议 v1.0 零改动**（复用既有 `host/open_url`）—— 新增 `platform::file_url_to_path` + `platform::open_path`（仿 `run_as_admin` 风格），5 个单测覆盖基础/编码/CJK/非 file 协议/远程 file URL。

### 修复（文件搜索 v3.3 P2：`file://` URL 解析三缺陷，协议 v1.0 零改动）

源码缺陷见 [`docs/search-file.md`](./docs/search-file.md) §9.6（2026-09-10 记录），本批全部修复：

- **`%` 被无条件解码**（文件名含 `%` 打不开）：`report%20final.txt` → 旧链路 `file_url_to_path` 一律 `%XX` 解码 → `report final.txt` 打开失败。改为**候选 + 存在性优选**：`file_url_candidates` 按「原样 → 去 `?query`/`#fragment` → 上述两者的 percent-decode」产出排序候选，`resolve_file_url_to_path` 取**第一个存在**者（全不存在回退首选）。字面路径存在即命中自身，标准编码 URL（`%20`）仍落到解码版——**两种来源互不干扰**。
- **`#` / `?` 未做 fragment/query 截断**：新增 `strip_query_fragment` 作为次要候选；Windows 文件名允许 `#`（原样候选优先）、不允许 `?`（必然回落到截断候选）。
- **UNC 路径不支持**：`file://server/share/x.txt` 旧链路恒返回 `None` → 误落到 `webbrowser::open` 而失败。改为 `split_file_url` 解析 authority 并映射 UNC `\\server\share\x.txt`；**UNC 不做 `exists()` 优选**（离线 SMB 共享会让 `Path::exists()` 阻塞到网络超时），交给 `ShellExecuteW`。扩展侧 `path_to_file_url` 同步：UNC 输入走 `file://<host>/<rest>`，不再拼成必然打不开的 `file:////server/…`。
- **未做 percent-encode（`search.rs` 侧保持原样）**：刻意选择——宿主侧宽容解析已完全覆盖 `%`/`#`，改动扩展侧只会让新旧版本的扩展与宿主产生不必要的耦合（file-search 是 sidecar，版本可独立演进）。
- 验证：`fmt --all --check` 干净、`clippy --workspace --all-targets -- -D warnings` 0 告警、workspace 测试全绿；宿主侧 **8 条**单测（原 5 条改写为「候选列表」口径 + 新增 3 条，含一条真实文件系统夹具：`report%20final.txt` 存在 → 命中字面文件；删除后 → 回退 `report final.txt`），扩展侧新增 **4 条**（本地/CJK/`%`/`#` 不编码、UNC authority、退化形态、UNC invoke 端到端 URL）。
- **环境洼地记录**：本机 Bash 会话 `APPDATA` 为空（`USERPROFILE`/`TEMP` 正常），`cache_dir()` 依赖它 → 未导出时 `dd-ext-apps` 的图标抽取两条测试必失败（覆盖率 0%，因 PNG 无法落盘），`export APPDATA="C:\Users\<user>\AppData\Roaming"` 后 12/12 全过。测试前先看一眼 APPDATA，别把环境缺失误判成代码回归。

## [0.1.0] - 2026-09-06

### 新增

- **文件搜索**（扩展 `com.ddrun.filesearch`）：基于 Everything 的本地文件搜索。
  - 根视图输入 `f ` + 关键词自动进入文件结果页；同时保留顶层入口与 fallback 模板入口。
  - 每次最多返回前 30 条，按「文件名 > 路径」相关度并叠加近因加分排序；回车经 `host/open_url` 用系统默认程序打开。
  - Everything 搜索语法（`ext:` / `dm:` / `path:` / 通配符 / 正则）原样透传。
  - 以 sidecar 形式随绿色包分发（`dist/extensions.d/`），免安装。
- 用户文档：[`docs/search.md`](./docs/search.md)（Everything 配置、语法速查、故障排查）。

### 说明

- 文件搜索依赖 Everything，**仅支持 Windows**；跨平台 fd 兜底 Provider 计划于 **v0.2** 提供。
- 协议 v1.0 **冻结**：本版本未新增任何协议方法，文件结果复用协议已定义的 `get_items` 子页能力。

### 传输层（实现说明：es.exe 通道切换（`6133d96`）晚于 v0.1.0 标签最初落库（2026-09-06）；该标签 2026-09-09 经用户决策删除并重打至 es.exe 实现之后的提交，随重打首次触发 GitHub Release）

- **经 Everything 官方命令行工具 `es.exe`（IPC 通道）检索**，**无需开启 Everything HTTP 服务器**；仅需 Everything 在运行 + `es.exe` 已安装（`winget install --id=voidtools.Everything.Cli`）。
- 配置项改为 `DDRUN_ES_PATH` / `DDRUN_EVERYTHING_DIR`（默认自动定位，无 HTTP/端口概念）。
- es.exe 经管道输出为系统 OEM 代码页（中文 Windows = 936/GBK）而非 UTF-8，已由 `decode_output()`（`MultiByteToWideChar` + `GetConsoleOutputCP`，复用 `shell.rs` 惯例）按代码页转 UTF-8，单测 `decode_with_codepage_converts_gbk_filename` 覆盖——修复此前中文路径乱码导致「搜不到文件」的问题。
