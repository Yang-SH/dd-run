# dd-run 后续功能规划（N1–N5）

> **状态**：**N1–N5 全部落地且真机走查收口**（N4 2026-10-03、N1/N2/N3/N5 2026-10-04；自动化真机腿 10-04 已过；**2026-10-07 用户真机批量走查通过**——N1 path 腿有 stderr 日志实证，其余腿为批量确认口径，非逐腿判据记录，发现问题逐项重开） ｜ **版本**：v1.9 ｜ **最后更新**：2026-10-07
> **关联**：[implementation.md](./implementation.md) · [optimization-plan.md](./optimization-plan.md) · [settings-personalization-plan.md](./settings-personalization-plan.md) · [protocol.md](./protocol.md) · [manifest-schema.md](./manifest-schema.md) · [security-audit-2026-09-23.md](./security-audit-2026-09-23.md) · [INDEX.md](./INDEX.md)

---

## 1. 结论总表

本文为 v0.1.1（M0–M9 关闭、审计 11/11 闭环）之后的**功能向**增量规划。立项筛选遵循两条硬判据：**可行**（零冻结契约改动、有既有代码惯例可复用、依赖克制、可通过单测与真机走查验收）与**无冲突**（不与冻结契约、README 非目标、既有方案在途项、已证伪项、已占用编号空间冲突）。逐条判据见 §2，冲突面核查矩阵见 §2.3。

下表为 30 秒结论；各提案的动机、设计、取证与冲突检查见 §4。

| 编号 | 提案 | 一句话说明 | 协议/清单 | 依赖增量 | 优先级 | 改动量 |
|---|---|---|---|---|---|---|
| N1 | 自定义直达命令 | 关键词 → URL / 本地路径，选中即执行——**✅ 已落地（2026-10-04，见 §4.1 落地注记）** | 零改动 | 零 | P1 | 中 |
| N2 | 内置扩展配置通道 | 宿主声明设置项 + 环境变量注入（泛化 `DD_WEBSEARCH_ENGINES` 惯例）——**✅ 已落地（2026-10-04，见 §4.2 落地注记）** | 零改动 | 零 | P2 | 中 |
| N3 | 浏览器书签搜索 | 新内置 in-process 扩展，本地读 Chromium 系书签——**✅ 已落地（2026-10-04，见 §4.3 落地注记）** | 零改动 | 零 | P2 | 中 |
| N4 | LRU 预热容量可配置 | 正式承接 `optimization-plan.md` §2.7.1 转功能项——**✅ 已落地（2026-10-03，见 §4.4 落地注记）** | 零改动 | 零 | P1 | 小 |
| N5 | 设置导入 / 导出 | 本机迁移；机器态与信任台账明确不随行——**✅ 已落地（2026-10-04，见 §4.5 落地注记）** | 零改动 | 零 | P3 | 小 |

> 五项提案均满足「零冻结契约改动 + 零新增依赖」。建议实施顺序与每批纪律见 §6；**明确不做与缓办清单**（含理由与前置条件）见 §5——其中「带参数的关键词直达」因 `f ` 前缀直达的移除先例（2026-09-19）判为缓办。

---

## 2. 立项筛选判据

### 2.1 「可行」判据（全部满足才立项）

1. **零冻结契约改动**：不触碰 [protocol.md](./protocol.md) §13 与 [manifest-schema.md](./manifest-schema.md) §9 的 v1.0 冻结面（方法、字段、错误码均不增不改）。
2. **有既有代码惯例可复用**：设计必须能指认仓库内已验收的同类实现作为蓝本（§3 列出五个惯例，均附落点）。
3. **依赖克制**：优先零新增依赖（延续 [optimization-plan.md](./optimization-plan.md) §2.5 O3 建立的依赖克制基调；`cargo machete` 当前零死依赖）。
4. **信任边界相容**：不绕过 S-05 的 fail-closed 门禁（spawn 唯一入口），不扩大 `host/*` 反向请求能力（白名单由清单校验规则 9 锁定）。
5. **可验收**：纯逻辑部分可单测、UI 部分有真机走查口径；实施后回写 `implementation.md` §3.1 测试基线台账。

### 2.2 「无冲突」判据

- 不与**冻结契约**冲突：协议与清单均不改字段、不加方法（见 §2.1 判据 1）。
- 不与**README 非目标**冲突：扩展商店（Gallery）、云端同步、遥测、账号体系、移动/Web 端维持不做（[README.zh-CN.md](../README.zh-CN.md)「非目标」节）。
- 不与**在途项**重复立项：T9 / T10（[settings-personalization-plan.md](./settings-personalization-plan.md) §4）、I3（[icons-typography-plan.md](./icons-typography-plan.md)）、O6 跨平台 M10 与 O5-a（[optimization-plan.md](./optimization-plan.md)）维持原渠道推进，本文只引用不重述。
- 不与**已证伪项 / 已移除交互**冲突：`panic = "abort"` 与内置 crate 拆分维持证伪结论（O5-b）；带参数前缀直达须记取 `f ` 前缀移除的先例（§5 缓办清单）。
- 不与**已实现能力**重复：拼音匹配（M6）、系统命令五件套等已落地能力不再立项（§5 尾表）。
- 不占用**已占用的编号空间**：见 §2.3。

### 2.3 冲突面核查矩阵

对仓库既有编号前缀逐一核查后，本文取 **N 系列**（N = New feature）；各提案进入设计稿阶段时再分配各自的 D 编号与设计稿版本，**本文不预占 D43+**。

| 冲突面 | 占用情况 | 本文处置 |
|---|---|---|
| D 系列（设计稿决策，D1…D42） | 已占用至 D42 | 不预占；各提案入设计稿时再取号 |
| M 系列（里程碑，M0…M9）+ O（优化）+ S（安全）+ T/K/B（个性化与排版批次） | 已占用 | 不使用 |
| F 系列（图标字体批次 F1/F2）、E 系列（E1/E2）、A 系列（验收 A1–A12 / A-33 等）、R/L/P 系列 | 已占用 | 不使用 |
| `f ` 前缀直达交互 | 已于 2026-09-19 移除（[search-file.md](./search-file.md) §6.1） | N1 刻意回避该交互形态（§4.1、§5） |
| 协议 §13 / 清单 §9 演进规则 | v1.0 冻结，改动须走版本协商 | 本文五项均零改动；第三方扩展设置 schema 列缓办（§5） |

---

## 3. 现状底座：五个可复用的既有惯例

以下事实为本规划的全部可行性依据，落点均已核验；里程碑脉络见 [implementation.md](./implementation.md) §2，此处不重复叙述。

| 惯例 | 现状 | 落点 |
|---|---|---|
| ① 环境变量配置通道 | 搜索引擎表经 `inject_websearch_env` 在加载期写入扩展 manifest 的 `entry.env` 内存副本，spawn 与 in-process spec 统一消费；扩展侧解析失败回落内置默认表 | `crates/dd-gui/src/aggregator.rs` 约 :139-154 |
| ② 设置文件向前兼容 | `Settings::parse_json` 对垃圾输入回落默认值、**容忍未知字段**（有单测锁定）——新增配置字段不破坏旧版本读新配置 | `crates/dd-gui/src/settings.rs` 约 :709 起、约 :1035 起 |
| ③ 内置扩展注册表 | 5 个内置扩展以 `spec()` 注册（`com.ddrun.{apps,calc,system,websearch,shell}`），in-process 运行、不经 spawn、不涉信任台账；新增内置扩展 = 新模块 + 注册表加行 | `crates/dd-host/src/builtin.rs` 约 :64-100 |
| ④ `host/open_url` 白名单 | S-03 后仅放行 `http` / `https` / `file`；`file://` 走 ShellExecute「双击等价」（含 UNC），文件搜索打开动作依赖它 | `crates/dd-gui/src/app/host_actions.rs` 约 :111-136 |
| ⑤ 宿主级虚拟条目 | 宿主在聚合结果中直接提供「设置」等非扩展条目（`CommandRef::Invoke` 走宿主内部分发） | `crates/dd-gui/src/app/invoke.rs` :102（宿主内部分发）、`crates/dd-gui/src/navigation.rs` 约 :117-127 |

另两项相关事实：信任台账（S-05）以**清单文件字节流**为哈希对象，运行时对内存副本的改写不影响信任判定；设置持久化为 `Settings::save`（约 :960）。宿主对扩展进程环境变量的注入受 S-10 关键变量保护约束（不得覆盖 `PATH` 等敏感键）。

---

## 4. 立项提案

### 4.1 N1 · 自定义直达命令（关键词 → URL / 本地路径）

**动机**：用户常有少量高频直达目标（GitHub 个人页、常用文件夹、某文档）。现网络上可用「自定义搜索引擎 `{q}`」曲线实现、本机路径则无入口；文件搜索每次都要现搜。

**设计**：

- 配置：`Settings` 新增 `custom_commands: Vec<CustomCommand>`，字段 `title`（显示名）/ `keyword`（匹配别名）/ `kind`（`url` 或 `path`）/ `target`（字符串）。经 §3 惯例 ② 持久化，旧版本读新配置自动忽略。
- 注入：聚合期把每条配置转成宿主虚拟条目（§3 惯例 ⑤），`keyword` 并入条目 `tags`、`title` 生成拼音索引（复用 M6 拼音管线）——输入即搜、选中即执行，**无前缀语法、不带参数**。
- 执行：`url` → 复用 `host/open_url` 同一执行函数（S-03 白名单天然生效，`http/https` 走默认浏览器）；`path` → 复用 `file://`「双击等价」路径（ShellExecute，含 UNC）。
- 管理：设置页新增「自定义命令」卡，交互范式复用搜索引擎卡（下拉添加 + 列表删除，D42 行规格）；上下文菜单提供「删除」。

**可行性依据**：惯例 ①②④⑤ 全部命中；匹配层、拼音管线、设置卡范式、两步确认范式均为已验收实现。

**冲突检查**：协议 / 清单零改动；不新增 `host/*` 方法；不做「启动任意 exe」档（见 §5 缓办）；**刻意不做 `{q}` 带参直达**——该交互与已移除的 `f ` 前缀直达同形（劫持常规搜索的前科，2026-09-19 决策），如未来要做须先复核该决策（§5）。

**范围与工作量**：`settings.rs`（结构 + 往返单测）/ `aggregator.rs`（虚拟条目）/ `app/host_actions.rs`（执行复用）/ `ui/settings_view.rs`（管理卡）/ `text.rs`（zh/en 文案）。估约 300–400 行 + 单测。

**验收草案**：`- [x]` 配置往返与未知字段兼容单测（`n1_custom_commands_validate_roundtrip_and_compat`：构造 trim/小写化/空白关键词拒绝/空字段拒绝；解析非法条目跳过、kind 未知跳过、旧版本配置默认空；`to_json_string`→`parse_json` 往返保真——未知字段容忍由既有 `parse_json_tolerates_unknown_fields` 锚定）；`- [x]` 条目构造单测（`custom_command_items_build_host_virtual_entries`：id 前缀 / 保留 ext_id / tags / 副标题 / 拼音 / 「直达」分组 / Invoke）；`- [x]` **首屏呈现腿真机走查（2026-10-04，dist 重建版自动化会话）**：config 预置两条命令（GitHub/url + 系统盘根目录/path）+ 停用 apps/bookmarks 扩容面板 → `WM_HOTKEY` 唤起（窗口矩形 635,250,1285,782 与 V-1 基线逐位一致）→ 截图留档「直达」分组两条命令上屏、徽标「直达」；`- [ ]` url/path 执行腿（需 Enter 真实输入，随用户会话）；`- [ ]` 拼音 / 别名命中（需键入）；`- [ ]` S-03 白名单外 scheme（如 `javascript:`）被拒走查。

**落地注记（2026-10-04）**：设计如上全量兑现，另有两处实现细化——① `host/open_url` 执行体自 `execute_host_request` 抽出为 `open_url_execute(url, source)` 供 url 型命令复用（S-03 白名单 / file:// ShellExecute / R-16 失败 toast 行为逐位一致；path 型直接走其底层 `platform::open_path`「双击等价」）；② 右键菜单「删除此命令」以宿主侧 `CtxAction::DeleteCustomCommand` 实现（`delete_custom_command`：移除 + 落盘 + 重聚合脏标记），非扩展 `more_commands` 通道。条目注入点 = `spawn_aggregation` 末尾 `custom_command_items` 追加（不经扩展、无信任面）；执行路由 = `dispatch_invoke` 对保留 id `com.ddrun.host` 宿主内部分发。设置页管理卡落「常规」第 6 卡（搜索引擎卡范式：32px 列表行 + 删除小按钮 + 类型下拉/名称/关键词/目标输入 + danger 错误行，i18n 14 键 zh/en）。台账 implementation.md §5（578 快照）。

### 4.2 N2 · 内置扩展配置通道（宿主声明 + 环境变量注入）

**动机**：内置扩展的少量行为只能改代码（如 apps 扩展的垃圾项过滤名单是静态的，见 [apps-filtering-plan.md](./apps-filtering-plan.md)）。需要一个**零协议改动**的通用配置通道，让内置扩展获得用户可调项。

**设计**：

- 通道：沿用 §3 惯例 ① 的既有机制——`Settings` 新增 `ext_settings: HashMap<String, serde_json::Map<String, Value>>`（`ext_id → 键值`），聚合期由通用函数合并写入对应扩展 `entry.env` 内存副本，键名约定 `DD_EXT_CFG_<KEY>`（避开 S-10 保护名单）。
- 声明与渲染：设置项的**声明与 UI 由宿主持有**（扩展页行内「设置」入口，仅内置扩展提供），第三方扩展不涉及——刻意不做清单 schema 进 manifest（那需要 [manifest-schema.md](./manifest-schema.md) §9 版本协商，见 §5 缓办）。
- 试点消费者（1 个）：apps 扩展「用户屏蔽名单」（`DD_EXT_CFG_BLOCKLIST`，逗号分隔显示名片段）——把 [apps-filtering-plan.md](./apps-filtering-plan.md) 的静态黑名单扩出用户可编辑层；设置页以搜索引擎卡同款列表 UI 呈现。
- 生效时机：值变更走既有「重聚合 + 进程重注环境」链路（websearch 引擎表变更已是此路径）。

**可行性依据**：`inject_websearch_env`（aggregator.rs 约 :139-154）即本通道的单例实现，泛化为「多扩展 + 多键」是纯增量；信任判定以清单**文件字节流**为哈希对象，运行时内存改写不触碰台账（§3 尾注）。

**冲突检查**：协议 / 清单零改动；S-10 边界以键名前缀约定守住；不改 S-06 危险命令确认行为；与 apps-filtering-plan 不重复（它落静态过滤，本项落用户层开关）。

**范围与工作量**：通道约 150 行（含合并/覆盖次序单测）+ 试点约 100 行；`settings.rs` / `aggregator.rs` / `ui/settings_view.rs` / `dd-ext` apps 内置。

**验收草案**：`- [x]` 通道单测（`n2_ext_settings_roundtrip_and_compat`：默认空 / 旧版本兼容 / 非字符串值与空 ext_id 跳过 / 往返保真；`n2_ext_cfg_env_name_guards_key_shape`：键名白名单 + 前缀不变式锚定；`n2_inject_ext_settings_targets_listed_exts_only`：表内注入 / 非法键跳过 / 表外扩展零改动；`n2_user_blocklist_filters_top_level_with_stable_ids` + `n2_user_blocklist_parse_fragments`：片段包含匹配大小写不敏感 / 空名单零过滤 / **id 保持全量枚举原始下标** / 解析 trim 去空归一）；`- [x]` **屏蔽名单真机走查（2026-10-04，dist 重建版自动化会话，A/B 对比截图）**：A 基线（blocklist 不命中）首屏「应用」组 7-Zip / brave / Clash Verge 等在列；B（blocklist = `7-Zip, brave, clash` 三片段）重启后三行**消失**、其余行原样——多片段逗号解析 + 大小写不敏感包含匹配 + 输出层过滤真机生效；「清空恢复」方向由 A 基线腿覆盖；`- [x]` 旧配置文件读入兼容（单测已锚；走查会话即以旧版本字段缺失的 config 启动，零异常）。

**落地注记（2026-10-04）**：通道如设计兑现，两处实现细化——① `ext_settings` 值类型由设计稿的 `serde_json::Value` 收窄为 `String`（v1 消费者只需字符串，且 `Value` 无 `Eq` 会破坏 `Settings` derive），容器用 `BTreeMap` 保证序列化确定性；② in-process 内置（apps）对 `entry.env` 不敏感，屏蔽名单走**内存通道直接注入**（websearch `set_configured_engines_json` 先例）+ **输出层过滤**（`top_level_commands`，`APP_CACHE` 进程级 OnceLock 使枚举层过滤会滞后到重启；条目 id 保持全量枚举原始下标，`handle_invoke` 索引寻址不变）。通用 env 注入 `inject_ext_settings`（`DD_EXT_CFG_<KEY>`，S-10 键名白名单 + `PROTECTED_ENV_KEYS` 复查双保险）覆盖子进程扩展。设置 UI 落扩展卡 apps 行内「设置」小按钮展开的屏蔽名单编辑器（宿主持有声明与渲染，仅内置扩展提供）。apps-filtering-plan §4.5「不做用户自定义黑名单 UI」由此承接销项。台账 implementation.md §5（583 快照）。

### 4.3 N3 · 浏览器书签搜索（新内置 in-process 扩展）

**动机**：书签是「我知道它存在但记不住 URL」的高频目标；现网络上仅有搜索引擎、本机无入口。

**设计**：

- 形态：新增第 6 个内置扩展 `com.ddrun.bookmarks`（in-process，§3 惯例 ③ 注册表加行 + `dd-ext/src/builtins/bookmarks.rs`），**不经 spawn、不涉信任台账**（随宿主单文件分发，符合「宿主是单文件」硬原则）。
- 数据：只读解析 Chromium 系书签 JSON（Chrome / Edge 的 `%LOCALAPPDATA%\...\User Data\<profile>\Bookmarks`），聚合期按文件 mtime 门控增量重建；索引条目数设上限（超限截断并在结果尾部提示），文件夹路径进 `subtitle`。
- 图标与交互：v1 用 glyph 占位（图标管线已有回落档）；点击 → `host/open_url` `https` 档打开；不引入网络请求（favicon 拉取判不做）。
- 隐私姿态：纯本地文件读取、零网络、零遥测，与 README 非目标及审计信任边界一致。

**可行性依据**：惯例 ③④ 命中；JSON 解析 `serde_json` 已在依赖树内（零新增）；M9 in-process 形态即为蓝本（[m9-inprocess-builtins.md](./archive/m9-inprocess-builtins.md)）。

**冲突检查**：协议 / 清单零改动；不新增 sidecar（不触发 S-05 信任面变更——R-12 起随包首方 sidecar 走**首跑钉扎**而非白名单自动信任，见 §5 扩展商店行）；不做 Firefox（mozlz4/sqlite 解码需引依赖，列缓办 §5）；内存纪律遵守 M1–M4 口径（mtime 门控 + 条目上限，无常驻大索引）。

**范围与工作量**：估约 300 行 + 单测（书签 JSON 解析 fixture、截断、mtime 门控）；`dd-ext` / `dd-host/src/builtin.rs` / `aggregator.rs` / `text.rs`。

**验收草案**：`- [x]` 解析 fixture 单测（`n3_parse_chrome_fixture_flattens_folders`：Chrome 真实结构三根 + 嵌套文件夹扁平化、folder 路径进 subtitle；`n3_parse_corrupt_and_unknown_nodes_are_tolerated`：非 JSON / 缺 roots / roots 非对象回落 None，缺 url / 空标题 / 未知 type 跳过，缺 type 带 children 按文件夹递归；`n3_parse_truncates_at_cap`：恰 2000 条截断）；`- [x]` **Edge 呈现腿真机走查（2026-10-04，dist 重建版自动化会话）**：本机 Edge `User Data\Default\Bookmarks` 真实解析 **1123 条**（日志「书签 warm（1123 命令）」）→ 首屏「书签」分组渲染（必应 / 百度 / 少数派 / 阮一峰的网络日志等真实条目 + glyph 占位 + 徽标「书签」，截图留档）；本机无 Chrome 书签文件——Chrome 腿随用户会话；`- [ ]` 拼音命中 / 打开腿（需键入 / Enter，随用户会话）；`- [x]` 冷启动耗时无回归（走查会话冷启动 **865 ms**，书签解析在聚合后台线程 + mtime 门控，数据就绪 823 ms 与 V-13 基线同量级）。

**落地注记（2026-10-04）**：设计如上全量兑现，三处实现细化——① 根节点不经 `flatten_node` 重入（根 `name` 与根显示键语义重复，直接以其 `children` 为顶层递归，folder 起点 = `浏览器/根显示名`）；② 缺 `type` 但带 `children` 的节点按文件夹递归（真实书签文件结构随版本演进的健壮性）；③ 截断在**解析层**收口（cap 语义单点，顶层构建与 invoke 共用同一快照；截断时追加「仅索引前 2000 条」提示条目，invoke 仅 toast）。mtime 门控 = 进程内 `(路径, mtime) → 条目` 快照（未变文件沿用旧解析）；`frozen=false` → 不落磁盘桩（书签随时可变）。防漂移哨兵 `registry_matches_dd_ext_specs` / `builtin_registrations_needs_no_exe` 同步 6 个内置。台账 implementation.md §5（586 快照）。

### 4.4 N4 · LRU 预热容量可配置（承接既有转功能项）

**动机**：[optimization-plan.md](./optimization-plan.md) §2.7.1 取证确认预热池容量为编译期常量（`crates/dd-gui/src/app/pool.rs` `LRU_WARM_CAPACITY = 8`），判为**功能项待单独立项**——本文即该立项。

**设计**：`Settings` 新增 `warm_capacity`（默认 8 = 现值，钳位 1–16）；常规页新增一行（D42 行规格，预算惯例 `- 40 - 16`）；`pool.rs` 改为读配置；`Settings::save` 往返覆盖。

**冲突检查**：[memory-optimization-plan.md](./memory-optimization-plan.md) 的实测基线在默认 8 下维持有效（默认值不动，只加可调）；O8 已明确该项不属文档卫生、转功能项——本文为其承接处，不与 O8 冲突。

**范围与工作量**：估约 100 行 + 单测（默认值、钳位、持久化往返、消费点）。

**验收草案**：`- [x]` 单测四项（`lru_set_capacity_shrinks_with_lru_tail_victims_and_grows_in_place`：缩容队尾优先驱逐返回受害者 / 同值无操作 / 扩容不追补 + 常规 LRU 驱逐回归；`n4_warm_capacity_defaults_clamps_and_roundtrips`：默认 8 / 缺失损坏回落 / 0、99 clamp 1–16 / `to_json_string`→`parse_json` 往返保真；`n4_warm_capacity_consumed_at_construction`：默认构造容量 8 / `warm_capacity=2` 构造点读配置——dd-host 1 条 + dd-gui 2 条，workspace **576/576 全绿**，fmt 零差异、clippy 零告警，2026-10-03）；`- [x]` **调低腿真机走查（2026-10-04，dist 重建版自动化会话）**：config 预置 `warm_capacity=2` → 冷启动日志 **「LRU 驱逐」×4**（websearch 等超容扩展 close+释放回落 stub）+ 会话后段「warm 空闲回收：com.ddrun.shell（空闲 134s ≥ 120s）」——容量消费、驱逐路径、空闲回收三层真机生效；`- [ ]` 调高腿（调高 → 第三方扩展保活数增加，随用户会话）。

**落地注记（2026-10-03）**：设计如上全量兑现——`Settings.warm_capacity`（u8，1–16，默认 8 = 原 `pool.rs LRU_WARM_CAPACITY` 常量值，M1–M4 内存基线口径不变）；设置页「常规」第 5 卡「预热容量」（D42 行规格 + Fluent 数字下拉 1–16，`set.warm_capacity.name/desc` zh/en）；`apply_warm_capacity` 落盘 + 缩容即时驱逐（`LruWarmSet::set_capacity` 返回受害者走既有 `evict_warm` 路径）；`LRU_WARM_CAPACITY` 常量删除、默认口径移至 `settings::WARM_CAPACITY_DEFAULT`。台账 implementation.md §5（576 快照）。

### 4.5 N5 · 设置导入 / 导出（本机迁移）

**动机**：绿色单文件、无安装器、无云同步（非目标）——换机迁移目前只能手工拷配置文件，且用户不知道哪些文件该拷、哪些不能拷。

**设计**：

- 导出：设置页「常规」底部新增「导入 / 导出」卡（两步确认范式复用恢复默认外观卡）。导出物 = 数据目录下 `dd-settings-backup.json`：`config.json` 全量字段**减去机器态**（`autostart`——注册表状态随机器；`panel_size`——分辨率相关），并写 `exported_from` 版本号；**`trust.json` 永不导出**（S-05 fail-closed：信任判定绑定本机清单/exe 哈希，搬走即失效，导出只会制造虚假迁移预期；R-12 起台账还含随包首方扩展的钉扎哈希与宿主版本，更无导出意义）。
- 导入：该文件存在时「导入并覆盖」→ `parse_json` 容错解析（惯例 ②）→ 两步确认 → 落盘并应用；失败 toast（复用既有 toast 组件）。
- 零依赖取径：不引文件对话框 crate（`rfd` 需走 O3 依赖评审），v1 固定路径 + UI 展示路径便于用户拷贝；如后续要系统对话框，单独立项评估 `rfd`。

**冲突检查**：零契约改动、零依赖；不触碰 trust.json 与 S-05 判据；与 README 非目标（云同步）无涉——这是**手动、本地、单文件**迁移。

**范围与工作量**：估约 150 行 + 单测（导出字段裁剪、导入容错、机器态剔除）。

**验收草案**：`- [x]` 导出内容单测（`n5_backup_export_strips_and_import_tolerates`：导出剔除 autostart / panel_size 键 + `exported_from` 版本标记 + 业务字段保留；trust 数据天然不在 Settings 序列化内）；`- [x]` 导入兼容单测（同测：垃圾文件 / 非对象 → `Err` 不静默回落默认；旧版本 `exported_from` 不设门槛；未知字段容忍；机器态（autostart / panel_size / 热键）保留本机现值；导出→导入往返还原业务字段）；`- [ ]` 真机走查（导出 → 清配置 → 导入 → 设置页逐项还原）。

**落地注记（2026-10-04）**：设计如上全量兑现，一处实现细化——**热键也不随导入**（`hotkey_mods`/`hotkey_vk` 与 autostart / panel_size 一并保留本机现值）：改绑须走捕获改绑流程（seq 确认协议），导入期自动注册有冲突风险且无法走确认对话框；导出物仍含热键字段供参考。导出复用 R-02 原子写（`Settings::save_backup_to`），备份路径 = 数据根目录 `dd-settings-backup.json`（UI mono 上屏 + tooltip 便于整行拷贝，零文件对话框依赖）。导入应用链：主题/材质/不透明度/圆角/边框走既有 apply 链即时生效（先 apply 后整体赋值，避免差值归零跳过 set_theme 副作用）；语言经 `apply_lang`（托盘同步 + 聚合脏标记）；warm 容量即时调整含缩容驱逐；聚合类配置统一 `engines_dirty`。两步确认 5s 自动撤销（`settings_import_armed`，appearance_reset 同范式）。台账 implementation.md §5（587 快照）。

---

## 5. 明确不做与缓办清单

**缓办 = 有真实需求但存在前置条件；不做 = 与既有承诺冲突或需求不成立。** 二者均不在本文五项范围内，列出以固化边界。

| 项 | 定性 | 理由与前置条件 |
|---|---|---|
| 扩展商店 / 远程获取扩展 | ❌ 不做（远期可复议） | README「非目标」（Gallery 为可选模块）；S-05 信任模型以「本机文件哈希」为锚（R-12 起随包首方 sidecar 亦走首跑钉扎——信任锚始终是本机文件），远程分发需先有签名 / 发布哈希机制，属供应链级前置 |
| 剪贴板历史 | 缓办 | 隐私面大（S-07 已证剪贴板敏感），需独立隐私评审 + 本地加密存储设计 + 明确保留期限，未过评审不立项 |
| 第三方扩展设置 schema（manifest v1.1） | 缓办 | 需走 [manifest-schema.md](./manifest-schema.md) §9 版本协商（MINOR 加法可行但有流程成本）；N2 的宿主声明通道已覆盖内置扩展需求，待真实第三方需求出现再启动 |
| 带参数的关键词直达（`kw xxx` 语法） | 缓办 | 与 2026-09-19 移除的 `f ` 前缀直达同形（劫持常规搜索的用户投诉前科，[search-file.md](./search-file.md) §6.1）；如复议须先复核该决策并重新真机取证 |
| 自定义命令「启动任意 exe」档 | 缓办 | 新攻击面（宿主直接 spawn 任意可执行文件），现有 shell 扩展已覆盖该能力且带 S-06 确认；如要做需安全评审 |
| Firefox 书签 | 缓办 | `mozlz4` / `sqlite` 解码需引依赖，违背本文零依赖判据；待 Chromium 系验证需求后评估 |
| 主题市场 / 在线主题 | ❌ 不做 | 云端分发与非目标冲突；本地自定义主题属 T10「另行立项」范围，不在此重复立项 |

**已实现、不再立项的能力**（避免重复提案的对照表）：拼音全拼/首字母匹配（M6，`fuzzy.rs` + `state.rs pinyin_haystack`）、系统命令（锁屏/睡眠/关机/重启/注销，含 S-06 确认，`dd-ext/src/builtins/system.rs`）、自定义搜索引擎 `{q}` 模板、Ctrl+F 文件直达、Shell 真实图标、窗口材质与着色三档、两步确认/上下文菜单/Toast、信任审批 UI（S-05）。

---

## 6. 建议实施顺序

按「确定性 → 用户价值 → 生态地基」排序，每项独立成批、独立可回退：

1. **N4**（最小、判据最硬——optimization-plan 已取证转功能项）——**✅ 已落地（2026-10-03，§4.4）**；
2. **N1**（用户价值最大、复用面最全）——**✅ 已落地（2026-10-04，§4.1）**；
3. **N2**（生态地基，试点先行）——**✅ 已落地（2026-10-04，§4.2）**；
4. **N3**（内置扩展增量，独立可拆）——**✅ 已落地（2026-10-04，§4.3）**；
5. **N5**（迁移补全）——**✅ 已落地（2026-10-04，§4.5）。五项提案全部落地；后续按真机走查结果销项，新提案须重新走 §2 立项判据。**

每批遵循项目既有纪律（[optimization-plan.md](./optimization-plan.md) §3 尾注）：**方案/设计稿先行 → 仅改工作副本、不自动 commit → fmt/clippy/test 三关全绿 + 新增单测 → 真机走查 → 人工审核提交**；实施后回写 `implementation.md` §2 / §3.1 与本文状态列。优先级为建议值，实际排期由维护者决定。

---

## 7. 与既有文档关系

- 本文为**功能向增量**，与 [optimization-plan.md](./optimization-plan.md)（工程向优化总览）互补，不重复其已落地内容；N4 直接承接其 §2.7.1 转功能项。
- 不重复 [settings-personalization-plan.md](./settings-personalization-plan.md)（T9/T10 维持原渠道）、[icons-typography-plan.md](./icons-typography-plan.md)（I3 维持保留）、[search-file.md](./search-file.md)（验收残留项维持原渠道）。
- 五项提案均不改 [protocol.md](./protocol.md) / [manifest-schema.md](./manifest-schema.md)，无需更新其 §13 / §9 演进记录。
- 任一提案落地后：本文对应条目移入「已落地」并回写 [implementation.md](./implementation.md)；若与本文假设冲突（如惯例落点漂移），以实现为准并升本文版本。

---

## 8. 版本演进

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.0 | 2026-09-28 | 首版：筛选判据（可行 × 无冲突）+ 冲突面核查矩阵 + 现状底座五惯例取证 + N1–N5 五项提案（零冻结契约改动、零新增依赖）+ 不做与缓办边界 + 实施顺序建议 |
| v1.1 | 2026-10-02 | 核对式体检修订：惯例 ⑤ 锚点自 `aggregator.rs` 约 :1066（已落入测试模块）改指 `app/invoke.rs` :102 + `navigation.rs` 约 :117-127；惯例 ② 与 `Settings::save` 行号实测刷新（:709 / :1035 / :960）；§4.3 / §5 / N5 补 R-12 首跑钉扎时效注记（「trust.json 永不导出」理由随之加强） |
