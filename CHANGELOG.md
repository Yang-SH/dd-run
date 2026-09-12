# 更新日志（Changelog）

本文件记录 dd-run 的版本变更。

## [Unreleased]

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

## [0.1.0]

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
