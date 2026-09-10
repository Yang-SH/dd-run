# dd-run 文件搜索扩展升级设计（v2）

> **文档定位（2026-09-10 更新）**：本文是 v3.3 变更方案的**评审蓝本（历史记录）**。
> **执行口径以 [`search-file.md`](./search-file.md) §九 为唯一来源**（含 §9.0 核对修订）。
> 本次核对（见 [`search-file-plan-review.md`](./search-file-plan-review.md)）已修正本文中的过时表述：
> ① `query_wait` **确提供** `.timeout(Duration)`（默认 3000ms，须显式设为 1000–1200ms）；
> ② `RequestFlags::Attributes` 与 `DateModified`（FILETIME）**均已确认存在**，"待核实项"结案；
> ③ client 不应使用自建全局 `LazyLock`，改用 crate 的 `EverythingClient::shared()`（可失效重建）；
> ④ 依赖 `everything-ipc` 需 `default-features = false` 并同版本引入 `windows 0.62`（FILETIME）。
> 下文的"待核实"与"LazyLock"表述保留作历史记录，**不得据以编码**。

## 1. 背景与目标
`dd-run` 已内建文件搜索扩展 `com.ddrun.filesearch`：`frozen:false`、`has_fallback:true`、经 `PageHandler` 供子页 `files.results`，传输走 `es.exe` 子进程 IPC，侧车（sidecar）随绿色包 `dist/extensions.d/` 分发不内嵌。对标 lin-ycv/EverythingCommandPalette（ECP），缺口在**命令丰富度**（仅回车打开，无 reveal/copy）与**传输性能**（每次按键 spawn es.exe）。
目标：ECP 覆盖度从 30% 提升至 70%（打开/显示/复制三高频动作）；消除每次按键的进程 spawn 开销；**协议 v1.0 零改动**；分发形态不变。
> **口径修正（2026-09-10 核对）**：按 ECP 命令条目数（12 项）计，本方案实为 **3/12 = 25%**；
> 原文 30%→70% 属"高频使用加权"口径。两口径须同时写明，详见
> [`search-file.md`](./search-file.md) §9.7（含 ECP 12 项命令与 dd-run 对照表）。
## 2. 方案总览
| 维度 | A：现状直用 | **B：everything-ipc 直连 + more_commands** | C：HTTP JSON API | D：自研索引 |
| - | | | |
| 通道 | `es.exe` 子进程 | WM_COPYDATA 直连（依赖 API 与版本兼容性待核实） | `TcpStream` | `walkdir`+`fuzzy` |  |
| 单次查询 | 约 50–80ms | **<10ms** | 10–20ms | 冷启动秒级 |  |
| 用户门槛 | 需 `es.exe` | **仅需 Everything 运行** | 需手动启用 HTTP 服务器 | 零依赖 |  |
| 协议改动 | 零 | **零** | 零 | 零 |  |
| 覆盖文件数 | Everything 索引 | 同左 | 同左 | 难达百万级 |  |
**暂定选型 B**。方案 C 的用户门槛较高（还需启用 HTTP 服务器）；方案 D 与当前
Everything 集成范围不符。方案 B 只有在依赖 API、许可证、Everything 版本兼容性、超时回收
和性能目标均通过 P0/P2 验收后，才能替代方案 A；否则继续使用方案 A。
## 3. 实施设计（三阶段递进，每阶段独立可验收）
### 3.1 P0 —— 修复仓库现存缺陷（零功能风险）
1. `guide_item()` 的文案改为：安装并运行 Everything 以及 `es.exe`（或设置现有的 `DDRUN_ES_PATH` /
   `DDRUN_EVERYTHING_DIR`）；P2 只有在 IPC 真机验收通过后，才可改为仅需启动 Everything。
2. 模块文档头注释删除 "Everything HTTP / `TcpStream`" 的旧描述，同步为 `es.exe` IPC（P2 后更新为 everything-ipc）。
### 3.2 P1 —— 补齐 more_commands（对齐 ECP 高频三动作）
**协议映射（全部为既有字段，无协议改动）**：
| 动作 | more_commands item | `CommandResult` | `Effect` |
| - | | | |
| 打开 | 无（默认主命令，现状） | `Dismiss` | `host/open_url`（file://） |
| 显示 | `files.reveal.{pid}` | `Dismiss` | 无：扩展进程直接 `explorer /select` |
| 复制路径 | `files.copy.{pid}` | `ShowToast`（已复制） | `host/set_clipboard` |
**实现要点**：
- `spec().capabilities` 追加 `host/set_clipboard`；**manifest 同步追加**——依据能力前置规则：未声明的 `host/*` 请求将被宿主回 `-32601`。
- pid 复用现有 `PATH_INDEX` 注册：三个命令共享同一路径 pid，每页 30 条 = 30 个 pid，1024 容量约 34 页缓存；"注册后立即 `lookup`"恒安全（invoke 唯一路径）。
- invoke 路由：`handle_invoke` 增加 `files.reveal.` / `files.copy.` 前缀分支；宿主从上下文菜单触发的调用 `sender="context_menu"`，但 pid 已嵌入 `id` 中，`context.selected_item_id` 仅作冗余校验。
- **`explorer /select` 引号坑**：必须使用 `std::os::windows::process::CommandExt::raw_arg`，否则 Rust 自动加引号会导致含空格路径双引号异常：`Command::new("explorer.exe").raw_arg(format!("/select,\"{path}\"")).spawn()`。
- ShowToast 与 Effect 的执行顺序：dd-ext 运行时在响应发送后再按序发送副作用，故 `host/set_clipboard` 在 Toast 结果返回后发出，顺序合理。
### 3.3 P2 —— everything-ipc 直连（性能优化）
**双通道策略（目标）**：若依赖 API 和版本矩阵验证通过，优先使用可复用的
Everything WM_COPYDATA 客户端；探测或查询失败时，再走现有 `es.exe` 回落路径。只有在该
回落链路和兼容性验收通过后，才可宣称用户门槛从“需 `es.exe`”降为“Everything 运行中”。
实现要点：
- client 满足 `Send`/`Sync`（已核实）但**不得**用自建全局 `LazyLock` 永久持有：宿主 warm 进程池使
  扩展长驻，Everything 重启后静态 client 永久失效 → 改用 `EverythingClient::shared()` 的 `Arc`
  语义（释放后自动重建）+ 探活重建阈值。详见 [`search-file.md`](./search-file.md) §9.2 P2.2。
- `query_wait` **已确认提供 `.timeout(Duration)`**（默认 3000ms > 宿主 `get_items` 2000ms）→
  **必须显式设为 1000–1200ms**，未设置即视为缺陷并阻断；不得自行套用 `recv_timeout` 兜底。
- UTF-16 天然返回：`decode_output` / `split_path` 在 IPC 通道上删除（es.exe 回落通道保留 GBK 解码）。
- **待核实项（已于 2026-09-10 结案）**：`RequestFlags` **暴露** `Attributes`（16 个常量之一）→
  用 `get_u32(Attributes) & 0x10` 精确判定目录，IPC 通道不再用 `guess_is_dir`（es.exe 回落通道保留）；
  `DateModified` 单位为 **FILETIME**（`QueryValue::Time(FILETIME)`）→ 复用 `filetime_to_unix`。
  另需注意：依赖 `windows 0.62`（非 `windows-sys`）→ dd-ext 须同版本引入 `windows`（仅
  `Win32_Foundation`）才能读取 FILETIME 字段；`dd-ext-search` 为 sidecar，体积影响不波及单文件宿主。
- Everything 未运行自动拉起（注册表/默认路径定位 `Everything.exe`，spawn 后再试一次 IPC）列为
  **P3 可选**，本次不实现。
## 4. 打包与分发
- 侧车（sidecar）机制不变：`dd-ext-search.exe` + `com.ddrun.filesearch.json` 随 `dist/extensions.d/` 分发，`${EXT_DIR}` 解析到本目录。P1 唯一打包相关改动是 manifest 的 `capabilities` 数组。
- `es.exe` 不随包（合规与体积考虑，保持用户自装/winget）；P2 后 `es.exe` 仅为回落通道，非必需。
## 5. 测试与验收
- 现有 12+ 项单测继续有效（`parse_response` 纯函数 fixture 模式是分层解耦的范本）。
- 新增：more_commands 的 id 构造（pid 嵌入）、handle_invoke 三分支（`PATH_INDEX` mock）、spec/manifest capabilities 一致性断言。
- 回归项：连续 100 次请求无内存泄漏（`PATH_INDEX` 上限生效）、协议 A9 无流式推送、A12 能力前置。
---
文档到此。**P0（改文案）与 P1（三个动作）改动集中在 `search.rs` + manifest 两处，预计 <150 行变更；P2 新增 `everything-ipc` 依赖后需先核实 docs.rs 两处字段——这一步是 P2 的硬前置，建议开工前做**。
