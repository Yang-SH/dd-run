# dd-run 后续功能规划（N1–N5）

> **状态**：规划中 ｜ **版本**：v1.0 ｜ **最后更新**：2026-09-28
> **关联**：[implementation.md](./implementation.md) · [optimization-plan.md](./optimization-plan.md) · [settings-personalization-plan.md](./settings-personalization-plan.md) · [protocol.md](./protocol.md) · [manifest-schema.md](./manifest-schema.md) · [security-audit-2026-09-23.md](./security-audit-2026-09-23.md) · [INDEX.md](./INDEX.md)

---

## 1. 结论总表

本文为 v0.1.1（M0–M9 关闭、审计 11/11 闭环）之后的**功能向**增量规划。立项筛选遵循两条硬判据：**可行**（零冻结契约改动、有既有代码惯例可复用、依赖克制、可通过单测与真机走查验收）与**无冲突**（不与冻结契约、README 非目标、既有方案在途项、已证伪项、已占用编号空间冲突）。逐条判据见 §2，冲突面核查矩阵见 §2.3。

下表为 30 秒结论；各提案的动机、设计、取证与冲突检查见 §4。

| 编号 | 提案 | 一句话说明 | 协议/清单 | 依赖增量 | 优先级 | 改动量 |
|---|---|---|---|---|---|---|
| N1 | 自定义直达命令 | 关键词 → URL / 本地路径，选中即执行 | 零改动 | 零 | 🟠 P1 | 中 |
| N2 | 内置扩展配置通道 | 宿主声明设置项 + 环境变量注入（泛化 `DD_WEBSEARCH_ENGINES` 惯例） | 零改动 | 零 | 🟠 P2 | 中 |
| N3 | 浏览器书签搜索 | 新内置 in-process 扩展，本地读 Chromium 系书签 | 零改动 | 零 | 🟠 P2 | 中 |
| N4 | LRU 预热容量可配置 | 正式承接 `optimization-plan.md` §2.7.1 转功能项 | 零改动 | 零 | 🔴 P1 | 小 |
| N5 | 设置导入 / 导出 | 本机迁移；机器态与信任台账明确不随行 | 零改动 | 零 | 🟡 P3 | 小 |

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
| ② 设置文件向前兼容 | `Settings::parse_json` 对垃圾输入回落默认值、**容忍未知字段**（有单测锁定）——新增配置字段不破坏旧版本读新配置 | `crates/dd-gui/src/settings.rs` 约 :703 起、约 :994 起 |
| ③ 内置扩展注册表 | 5 个内置扩展以 `spec()` 注册（`com.ddrun.{apps,calc,system,websearch,shell}`），in-process 运行、不经 spawn、不涉信任台账；新增内置扩展 = 新模块 + 注册表加行 | `crates/dd-host/src/builtin.rs` 约 :64-100 |
| ④ `host/open_url` 白名单 | S-03 后仅放行 `http` / `https` / `file`；`file://` 走 ShellExecute「双击等价」（含 UNC），文件搜索打开动作依赖它 | `crates/dd-gui/src/app/host_actions.rs` 约 :111-136 |
| ⑤ 宿主级虚拟条目 | 宿主在聚合结果中直接提供「设置」等非扩展条目（`CommandRef::Invoke` 走宿主内部分发） | `crates/dd-gui/src/aggregator.rs` 约 :1066、`crates/dd-gui/src/navigation.rs` 约 :121 |

另两项相关事实：信任台账（S-05）以**清单文件字节流**为哈希对象，运行时对内存副本的改写不影响信任判定；设置持久化为 `Settings::save`（约 :932）。宿主对扩展进程环境变量的注入受 S-10 关键变量保护约束（不得覆盖 `PATH` 等敏感键）。

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

**验收草案**：`- [ ]` 配置往返与未知字段兼容单测；`- [ ]` url/path 执行各一例真机走查（含 UNC 路径）；`- [ ]` 拼音 / 别名命中走查；`- [ ]` S-03 白名单外 scheme（如 `javascript:`）被拒走查。

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

**验收草案**：`- [ ]` 通道单测（注入次序、S-10 键名拒绝、解析失败回落）；`- [ ]` 屏蔽名单真机走查（命中屏蔽项不出现在结果、清空恢复）；`- [ ]` 旧配置文件读入兼容。

### 4.3 N3 · 浏览器书签搜索（新内置 in-process 扩展）

**动机**：书签是「我知道它存在但记不住 URL」的高频目标；现网络上仅有搜索引擎、本机无入口。

**设计**：

- 形态：新增第 6 个内置扩展 `com.ddrun.bookmarks`（in-process，§3 惯例 ③ 注册表加行 + `dd-ext/src/builtins/bookmarks.rs`），**不经 spawn、不涉信任台账**（随宿主单文件分发，符合「宿主是单文件」硬原则）。
- 数据：只读解析 Chromium 系书签 JSON（Chrome / Edge 的 `%LOCALAPPDATA%\...\User Data\<profile>\Bookmarks`），聚合期按文件 mtime 门控增量重建；索引条目数设上限（超限截断并在结果尾部提示），文件夹路径进 `subtitle`。
- 图标与交互：v1 用 glyph 占位（图标管线已有回落档）；点击 → `host/open_url` `https` 档打开；不引入网络请求（favicon 拉取判不做）。
- 隐私姿态：纯本地文件读取、零网络、零遥测，与 README 非目标及审计信任边界一致。

**可行性依据**：惯例 ③④ 命中；JSON 解析 `serde_json` 已在依赖树内（零新增）；M9 in-process 形态即为蓝本（[m9-inprocess-builtins.md](./m9-inprocess-builtins.md)）。

**冲突检查**：协议 / 清单零改动；不新增 sidecar（不触发 S-05 白名单变更）；不做 Firefox（mozlz4/sqlite 解码需引依赖，列缓办 §5）；内存纪律遵守 M1–M4 口径（mtime 门控 + 条目上限，无常驻大索引）。

**范围与工作量**：估约 300 行 + 单测（书签 JSON 解析 fixture、截断、mtime 门控）；`dd-ext` / `dd-host/src/builtin.rs` / `aggregator.rs` / `text.rs`。

**验收草案**：`- [ ]` 解析 fixture 单测（Chrome/Edge 两份真实结构样例、损坏文件回落空）；`- [ ]` 真机走查（Chrome 与 Edge 各命中一例、拼音命中、打开）；`- [ ]` 冷启动耗时无回归（桩路径不解析书签）。

### 4.4 N4 · LRU 预热容量可配置（承接既有转功能项）

**动机**：[optimization-plan.md](./optimization-plan.md) §2.7.1 取证确认预热池容量为编译期常量（`crates/dd-gui/src/app/pool.rs` `LRU_WARM_CAPACITY = 8`），判为**功能项待单独立项**——本文即该立项。

**设计**：`Settings` 新增 `warm_capacity`（默认 8 = 现值，钳位 1–16）；常规页新增一行（D42 行规格，预算惯例 `- 40 - 16`）；`pool.rs` 改为读配置；`Settings::save` 往返覆盖。

**冲突检查**：[memory-optimization-plan.md](./memory-optimization-plan.md) 的实测基线在默认 8 下维持有效（默认值不动，只加可调）；O8 已明确该项不属文档卫生、转功能项——本文为其承接处，不与 O8 冲突。

**范围与工作量**：估约 100 行 + 单测（默认值、钳位、持久化往返、消费点）。

**验收草案**：`- [ ]` 单测四项；`- [ ]` 真机走查（调低 → 驱逐回落桩态；调高 → 第三方扩展保活数增加）。

### 4.5 N5 · 设置导入 / 导出（本机迁移）

**动机**：绿色单文件、无安装器、无云同步（非目标）——换机迁移目前只能手工拷配置文件，且用户不知道哪些文件该拷、哪些不能拷。

**设计**：

- 导出：设置页「常规」底部新增「导入 / 导出」卡（两步确认范式复用恢复默认外观卡）。导出物 = 数据目录下 `dd-settings-backup.json`：`config.json` 全量字段**减去机器态**（`autostart`——注册表状态随机器；`panel_size`——分辨率相关），并写 `exported_from` 版本号；**`trust.json` 永不导出**（S-05 fail-closed：信任判定绑定本机清单/exe 哈希，搬走即失效，导出只会制造虚假迁移预期）。
- 导入：该文件存在时「导入并覆盖」→ `parse_json` 容错解析（惯例 ②）→ 两步确认 → 落盘并应用；失败 toast（复用既有 toast 组件）。
- 零依赖取径：不引文件对话框 crate（`rfd` 需走 O3 依赖评审），v1 固定路径 + UI 展示路径便于用户拷贝；如后续要系统对话框，单独立项评估 `rfd`。

**冲突检查**：零契约改动、零依赖；不触碰 trust.json 与 S-05 判据；与 README 非目标（云同步）无涉——这是**手动、本地、单文件**迁移。

**范围与工作量**：估约 150 行 + 单测（导出字段裁剪、导入容错、机器态剔除）。

**验收草案**：`- [ ]` 导出内容单测（无 autostart / panel_size / trust 数据）；`- [ ]` 导入兼容单测（旧版本配置、垃圾文件）；`- [ ]` 真机走查（导出 → 清配置 → 导入 → 设置页逐项还原）。

---

## 5. 明确不做与缓办清单

**缓办 = 有真实需求但存在前置条件；不做 = 与既有承诺冲突或需求不成立。** 二者均不在本文五项范围内，列出以固化边界。

| 项 | 定性 | 理由与前置条件 |
|---|---|---|
| 扩展商店 / 远程获取扩展 | ❌ 不做（远期可复议） | README「非目标」（Gallery 为可选模块）；S-05 信任模型以「本机文件哈希」为锚，远程分发需先有签名 / 发布哈希机制，属供应链级前置 |
| 剪贴板历史 | ⏸ 缓办 | 隐私面大（S-07 已证剪贴板敏感），需独立隐私评审 + 本地加密存储设计 + 明确保留期限，未过评审不立项 |
| 第三方扩展设置 schema（manifest v1.1） | ⏸ 缓办 | 需走 [manifest-schema.md](./manifest-schema.md) §9 版本协商（MINOR 加法可行但有流程成本）；N2 的宿主声明通道已覆盖内置扩展需求，待真实第三方需求出现再启动 |
| 带参数的关键词直达（`kw xxx` 语法） | ⏸ 缓办 | 与 2026-09-19 移除的 `f ` 前缀直达同形（劫持常规搜索的用户投诉前科，[search-file.md](./search-file.md) §6.1）；如复议须先复核该决策并重新真机取证 |
| 自定义命令「启动任意 exe」档 | ⏸ 缓办 | 新攻击面（宿主直接 spawn 任意可执行文件），现有 shell 扩展已覆盖该能力且带 S-06 确认；如要做需安全评审 |
| Firefox 书签 | ⏸ 缓办 | `mozlz4` / `sqlite` 解码需引依赖，违背本文零依赖判据；待 Chromium 系验证需求后评估 |
| 主题市场 / 在线主题 | ❌ 不做 | 云端分发与非目标冲突；本地自定义主题属 T10「另行立项」范围，不在此重复立项 |

**已实现、不再立项的能力**（避免重复提案的对照表）：拼音全拼/首字母匹配（M6，`fuzzy.rs` + `state.rs pinyin_haystack`）、系统命令（锁屏/睡眠/关机/重启/注销，含 S-06 确认，`dd-ext/src/builtins/system.rs`）、自定义搜索引擎 `{q}` 模板、Ctrl+F 文件直达、Shell 真实图标、窗口材质与着色三档、两步确认/上下文菜单/Toast、信任审批 UI（S-05）。

---

## 6. 建议实施顺序

按「确定性 → 用户价值 → 生态地基」排序，每项独立成批、独立可回退：

1. **N4**（最小、判据最硬——optimization-plan 已取证转功能项）；
2. **N1**（用户价值最大、复用面最全）；
3. **N2**（生态地基，试点先行）；
4. **N3**（内置扩展增量，独立可拆）；
5. **N5**（迁移补全）。

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
