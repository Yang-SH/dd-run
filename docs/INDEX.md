# dd-run 文档索引

> **状态**：生效中 ｜ **版本**：v1.2 ｜ **最后更新**：2026-09-14
> **用途**：全部文档的**唯一导航入口**——按模块分组、标注状态与读者，并定义文档格式规约。

---

## 1. 怎么用这份索引

| 你的目的 | 阅读路径 |
|---|---|
| **写一个扩展** | [`extensions.md`](./extensions.md)（路径）→ [`protocol.md`](./protocol.md)（规范）→ [`manifest-schema.md`](./manifest-schema.md)（清单） |
| **了解项目现状** | [`README.zh-CN.md`](../README.zh-CN.md) → [`implementation.md`](./implementation.md) §里程碑状态总表 |
| **接手维护代码** | [`cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md)（抽象模型与验收 A1–A12）→ [`implementation.md`](./implementation.md)（ADR 与遗留台账）→ 各 [`mX-record.md`](./m0-record.md)（过程与踩坑） |
| **排查文件搜索问题** | [`search.md`](./search.md)（用户视角）→ [`search-file.md`](./search-file.md)（权威技术口径） |
| **理解某个设计为什么这样做** | 先查 [`implementation.md`](./implementation.md) §4 ADR-1~ADR-4，再查设计文档对应章节 |
| **改协议 / 改清单格式** | [`protocol.md`](./protocol.md) §13（演进规则）→ [`manifest-schema.md`](./manifest-schema.md) §9，**两者均为冻结契约，改动须走版本协商** |

---

## 2. 文档地图

```mermaid
flowchart TD
    README["README 系列<br/>仓库入口"] --> IMPL
    IMPL["implementation.md<br/>实施主线 · 里程碑 · ADR"]
    DESIGN["cmdpal-platform-agnostic-design.md<br/>平台无关设计 · 验收 A1–A12"] --> IMPL
    IMPL --> REC["m0–m4 / m9 记录<br/>过程与验收证据"]

    PROTO["protocol.md<br/>扩展协议 v1.0（冻结）"] --- MANIFEST["manifest-schema.md<br/>清单 schema v1.0（冻结）"]
    MANIFEST --> EXT["extensions.md<br/>扩展开发指南"]
    PROTO --> EXT

    PLAN["专题方案<br/>search-file / refactor / apps / memory / icons / optimization"] --> IMPL
    REVIEW["search-file-plan-review.md<br/>方案核对报告（历史）"] --> PLAN
    USERDOC["search.md<br/>用户文档"] --> PLAN
    ARCHIVE["search-file-update.md<br/>历史蓝本（归档）"] --> PLAN
```

---

## 3. 全量文档清单

### A. 规范契约（冻结，硬契约）

改动须走版本协商；**不得为一处方便而破坏既有语义**。

| 文档 | 状态 | 版本 | 行数 | 读者 | 职责 |
|---|---|---|---|---|---|
| [`protocol.md`](./protocol.md) | 已冻结 | v1.0 | 781 | 宿主与扩展作者 | 进程间通信：JSON-RPC over NDJSON、生命周期、8 种执行结果、错误码、超时 |
| [`manifest-schema.md`](./manifest-schema.md) | 已冻结 | v1.0 | 157 | 扩展作者 | 扩展发现：清单字段、扫描目录、路径展开、9 条校验规则、内置注册 |

### B. 设计与实施（主线）

| 文档 | 状态 | 版本 | 行数 | 读者 | 职责 |
|---|---|---|---|---|---|
| [`../cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md) | 生效中 | v1.0 | 547 | 设计者与维护者 | 平台无关抽象模型、页面/扩展/宿主模型、上下游对照、**验收项 A1–A12 的定义源** |
| [`implementation.md`](./implementation.md) | 生效中 | v0.1.1 | 550 | 维护者 | 实施主线：里程碑状态总表、目标与范围、ADR-1~4、验收映射、遗留台账、实施沿革 |

### C. 开发指南与用户文档

| 文档 | 状态 | 版本 | 行数 | 读者 | 职责 |
|---|---|---|---|---|---|
| [`extensions.md`](./extensions.md) | 生效中 | v1.0 | 485 | 扩展作者 | 从零写一个扩展：快速上手、三条铁律、三个 Windows 陷阱、方法实现顺序、自检表 |
| [`search.md`](./search.md) | 生效中 | v1.0 | 122 | 最终用户 | 文件搜索的安装前置、进入方式、动作、常见问题与排障 |

### D. 专题方案

均为**已落地**的实施记录。方案细节保留作决策留痕；正文中的"计划时"表述已按实现校正。

| 文档 | 状态 | 行数 | 职责 |
|---|---|---|---|
| [`search-file.md`](./search-file.md) | 生效中（v3.3） | 922 | **文件搜索的权威技术口径**：双通道传输层、依赖、落地范围、真机验收待办、版本演进 |
| [`refactor-layering-plan.md`](./refactor-layering-plan.md) | 已落地 | 499 | `dd-gui` 业务代码 / UI 代码分层方案与切割表 |
| [`apps-filtering-plan.md`](./apps-filtering-plan.md) | 已落地 | 131 | Apps 扩展垃圾项过滤（黑/白名单、UWP 豁免、去重键） |
| [`memory-optimization-plan.md`](./memory-optimization-plan.md) | 已落地 | 129 | 运行时内存占用优化 M1–M4 与实测数据 |
| [`icons-typography-plan.md`](./icons-typography-plan.md) | 已落地（I3 未做） | 167 | 图标与字体展示优化：列表密度三档、占位 glyph、token 化 |
| [`optimization-plan.md`](./optimization-plan.md) | 规划中（O2 已落地） | 165 | **项目可优化方案总览（增量）**：体积/协议健壮性/方法名常量/可观测性/依赖治理/跨平台，含已证伪项与落地记录 |

### E. 核查与评审报告

| 文档 | 状态 | 行数 | 职责 |
|---|---|---|---|
| [`doc-audit-2026-09-13.md`](./doc-audit-2026-09-13.md) | 生效中 | 265 | **全量文档内容核对说明**：逐条列出与实现不符的错误、遗漏、矛盾，含依据与处置；并汇总待人工决策项 |
| [`doc-code-diff-2026-09-13.md`](./doc-code-diff-2026-09-13.md) | 生效中 | 291 | **文档 ↔ 代码差异清单与修复记录**（71 条差异，三路独立复核验真后按「文档对齐代码」逐条落实修复，含每条修复落点）；§10/§11 记代码侧闭环项（P-06/P-18），遗留待人工确认项见其 §5 |
| [`optimization-plan-review-2026-09-14.md`](./optimization-plan-review-2026-09-14.md) | 生效中 | 152 | **`optimization-plan.md` 二轮核对报告**：4 处事实错误 / 6 处不严谨 / 2 处规范不符 / 3 处存疑，逐条附依据与修改建议；对应修订已落实（该方案升至 v1.1） |
| [`search-file-plan-review.md`](./search-file-plan-review.md) | 历史归档 | 150 | 文件搜索 v3.3 方案的编码前核对报告；每项结论已附**最终处置**（已解决 / 已规避 / 仍存在 / 已被取代） |

### F. 里程碑记录（历史证据，内容保持原样）

**刻意不做格式重排**——这些是已通过验收的历史证据，重写会破坏可追溯性。各记录的"最后更新"即其关闭时点。

| 文档 | 行数 | 里程碑 | 关闭 |
|---|---|---|---|
| [`m0-record.md`](./m0-record.md) | 145 | M0 地基与协议冻结 | 2026-09-01 |
| [`m0-verification-report.md`](./m0-verification-report.md) | 110 | M0 验证报告 | 2026-09-01 |
| [`m1-record.md`](./m1-record.md) | 169 | M1 最小可用面板 | 2026-09-02 |
| [`m2-record.md`](./m2-record.md) | 292 | M2 命令执行与结果状态机 | 2026-09-02 |
| [`m2-verification-report.md`](./m2-verification-report.md) | 96 | M2 验证报告 | 2026-09-02 |
| [`m3-record.md`](./m3-record.md) | 154 | M3 缓存与懒加载 | 2026-09-02 |
| [`m4-record.md`](./m4-record.md) | 339 | M4 内置扩展与健壮性 | 2026-09-04 |
| [`m9-inprocess-builtins.md`](./m9-inprocess-builtins.md) | 146 | M9 内置扩展进程内化 | 2026-09-11 |

> M5–M8 的结论直接记于 [`implementation.md`](./implementation.md) §2，未单独建记录文件。

### G. 归档

| 文档 | 状态 | 行数 | 说明 |
|---|---|---|---|
| [`search-file-update.md`](./search-file-update.md) | 历史归档 | 107 | 文件搜索 v2 升级设计的评审蓝本。**内容仅代表撰写时点**，权威口径见 [`search-file.md`](./search-file.md) |

### H. 仓库入口

| 文档 | 状态 | 行数 | 说明 |
|---|---|---|---|
| [`../README.md`](../README.md) | 生效中 | 114 | 英文 README |
| [`../README.zh-CN.md`](../README.zh-CN.md) | 生效中 | 114 | 中文 README（与英文版逐节对应） |
| [`../CHANGELOG.md`](../CHANGELOG.md) | 生效中 | 177 | 版本变更日志；无日期条目标注为待补 |

> 设计稿与交互演示（HTML）不在本索引收录范围内：`cmdpal-ui-mockups.html`、`cmdpal-ui-optimization-v5.html`、`cmdpal-window-material-effects.html`（窗口材质与边框方案效果页）。设计稿的**版本权威**在文件自身标题，最新决策见 [`implementation.md`](./implementation.md) §2 对应批次。

---

## 4. 文档格式规约

新增或修改文档时遵循以下约定，以维持全局一致。

### 4.1 元信息块（必需）

H1 之后**必须**紧跟此块（上下各留一个空行）：

```markdown
# 文档标题

> **状态**：生效中 ｜ **版本**：v1.0 ｜ **最后更新**：YYYY-MM-DD
> **关联**：[protocol.md](./protocol.md) · [manifest-schema.md](./manifest-schema.md)

---
```

**状态取值仅限五种**，含义如下：

| 取值 | 含义 |
|---|---|
| `生效中` | 描述当前实现，是最新口径 |
| `已冻结` | 契约类，改动须走版本协商 |
| `已落地` | 方案已实施完毕，保留作决策留痕 |
| `历史归档` | 内容仅代表撰写时点，**权威口径在别处** |
| `规划中` | 尚未实施 |

### 4.2 结构与表达

| 项 | 规约 |
|---|---|
| 标题层级 | 一级 `## 1. 标题`、二级 `### 1.1 标题`，**连续编号** |
| 结论先行 | 文档开头（元信息块之后）先给结论/状态总表，历史沿革下沉到文末 |
| 事实唯一 | 同一事实**只在一处详述**，其余位置用链接引用，禁止重复叙述 |
| 追注处理 | **不得叠加追注**。新结论直接改写正文，旧结论收进文末"版本演进"表 |
| 代码块 | 一律标注语言（`json` / `rust` / `bash` / `toml` / `text` / `mermaid` / `python`） |
| 表格 | 统一 `\|---\|---\|`，表前给一句说明 |
| 任务 | 未完成 `- [ ]`、已完成 `- [x]` |
| 状态标记 | 只用 `✅` / `⚠️` / `❌` / `🟨`，**不使用装饰性 emoji** |
| 行号 | 写成「约 :NNN」，避免硬编码行号随代码漂移 |
| 数字 | 版本号、字节数、测试数、测试阈值**必须实测取证**，不得沿用历史快照 |

### 4.3 新增文档流程

1. 判断归类——规范契约 / 设计实施 / 开发指南 / 专题方案 / 核查报告 / 里程碑记录 / 归档；
2. 套用 §4.1 元信息块与 §4.2 格式规约；
3. 在本文 §2 地图与 §3 清单中登记，并更新受影响的关联文档链接；
4. 若新增的是契约类文档，须同步 [`protocol.md`](./protocol.md) §13 与 [`implementation.md`](./implementation.md) 的验收映射。

---

## 5. 当前已知的未闭环项

整理过程中核对出的、**需要人工跟进**的开放项：

| 项 | 位置 | 状态 |
|---|---|---|
| 消息超上限「回 `-32600` + 关闭连接」两侧均未实现 | [`protocol.md`](./protocol.md) §2.3 现状注 | ⚠️ 待人工确认是否补实现（差异清单 P-01） |
| §1.3 方法名无常量层（字符串字面量分发） | `dd-ext`/`dd-host`/`dd-gui` 分发点 | ✅ 已解决（2026-09-14）：新增 `dd-protocol::methods` 常量层（12 方法 + `HOST_METHODS`/`HOST_METHOD_PREFIX`/`ALL_METHODS`），全仓生产代码改用常量；一致性测试锁定常量 ↔ `protocol.md` §1.3（差异清单 P-18，方案 O2） |
| 文件搜索 P2 真机验收 A-33-05…A-33-10 | [`search-file.md`](./search-file.md) §4 | ⚠️ 未做（唯一阻塞"无需 es.exe"用户承诺的项） |
| `-32002 command_not_found` 已定义但无抛出点 | [`protocol.md`](./protocol.md) §9.2 | ⚠️ 保留码待接线（需实现与文档二选一） |
| `PageInfo` 类型已定义但运行时不传递 | [`protocol.md`](./protocol.md) §8.5 | ⚠️ 待接线或从协议中移除 |
| 图标字体优化 I3（回退 32px） | [`icons-typography-plan.md`](./icons-typography-plan.md) | ⚠️ 未做（视真机效果再定） |
| macOS / Linux 实际构建与验收 | [`../cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md) §8/§9 | ⚠️ 仅设计层，无产物 |
| CHANGELOG 部分条目缺日期 | [`../CHANGELOG.md`](../CHANGELOG.md) | ⚠️ 待补 |
| 全仓文档行尾统一 CRLF（原 7 份偏离：[`protocol.md`](./protocol.md) 与 `examples/python-minimal/README.md` 全 LF；`extensions.md` 5 处、`manifest-schema.md` 3 处、`README.md`/`README.zh-CN.md` 各 2 处、`search-file.md` 1 处裸 LF） | 全仓 30 份 md | ✅ 已解决（2026-09-14）：7 份全部归一化，复检 bare-LF=0；`core.autocrlf=true` 下为工作区行为、不影响提交内容 |
