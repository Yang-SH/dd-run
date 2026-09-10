# 文件搜索 v3.3 方案核对报告（2026-09-10）

> **核对对象**：[`search-file.md`](./search-file.md) §九（v3.3 变更方案，P2 everything-ipc 直连）+ [`search-file-update.md`](./search-file-update.md)（v2 蓝本）
> **核对方法**：以仓库代码（`crates/dd-ext/src/bin/search.rs`、`crates/dd-host/src/process.rs`、`crates/dd-gui/src/app/*`）、`docs/protocol.md`（v1.0 冻结）、`docs/manifest-schema.md`、上游 crate `everything-ipc` docs.rs / crates.io 事实逐条交叉验证，**不采信文档自述**。
> **结论**：**方案 B（everything-ipc 直连 + more_commands）可行**，P0/P1 已落地且与文档一致；P2 的"硬前置 = 先核 docs.rs"已可结案（`RequestFlags::Attributes` / `DateModified` 存在），但暴露 **4 项编码前必须解决的硬冲突**（超时预算、client 失效重建、双 windows 绑定与 FILETIME 类型来源、默认 feature 可能拉入 tokio）与 4 项中风险缺口。
>
> **修订落地状态（2026-09-10）**：本报告的 R-01~R-04、W-05~W-08 与新增 A-33-10 已写入
> [`search-file.md`](./search-file.md) §9.0 / §9.2 P2 / §9.3 L0 / §9.4 / §9.5，既有缺陷（W-09）记入
> §9.6；[`search-file-update.md`](./search-file-update.md) 已标记为 v2 评审蓝本（历史），执行口径
> 唯一指向 `search-file.md` §九。

---

## 一、结论速览

| 项 | 判定 |
| :--- | :--- |
| 方案 B 整体可行性 | ✅ 可行（同步运行时 + 常驻进程池使收益成立） |
| P0/P1 与代码一致性 | ✅ 已核实一致（协议 v1.0 零改动兑现） |
| P2 能否开工 | ⚠️ **有条件可开工**：先解决 R-01 ~ R-04 四项硬冲突 |
| 是否存在协议冲突 | ❌ 无（`more_commands` / `sender=context_menu` / `ShowToast` / `host/set_clipboard` 均为协议既有定义） |
| 是否影响单文件分发 | ❌ 不影响：`dd-ext-search` 是 sidecar（不在 `EMBED_EXES`），新增 `windows` 依赖只增大 sidecar 体积 |

---

## 二、问题清单（R = 必修，W = 建议修）

### R-01【硬冲突 · 超时预算倒挂】`query_wait` 默认超时 3000ms > 宿主 `get_items` 2000ms

- **文档原说法**：§9.2 P2.3「若 crate 不提供可取消/可限时调用，不得简单套用 `recv_timeout` … 或将该能力标记为 blocked」；§9.4 A-33-06「单请求 ≤2000ms」。
- **上游事实**：`EverythingClientQueryWaitBuilder` **提供** `.timeout(Duration)`，默认 `Duration::from_millis(3000)`（docs.rs）。
- **代码事实**：`crates/dd-host/src/process.rs:37` `TIMEOUT_GET_ITEMS = 2000ms`；`search.rs` 现有预算为探测 800ms + 查询 1200ms（`AVAIL_TTL` / `ES_TIMEOUT`）。
- **冲突类型**：参数默认值与宿主契约冲突 → 默认配置下**宿主先超时返回 `-32001`，扩展线程仍阻塞 3s**，A-33-06 必然失败。
- **修正方向**：① 删除"crate 无超时"的假设表述，改为"**必须显式** `.timeout(Duration::from_millis(1000..=1200))`"；② L0 审查增加"未显式设置 timeout 即阻断"；③ 双通道总预算（探活 + 查询 + 回落）须 ≤2000ms，写入 A-33-06 口径。

### R-02【缺口 · client 失效后无重建路径，A-33-06 只验回落不验恢复】

- **文档原说法**：§9.2 P2.2「先探测/连接 Everything IPC，失败后调用现有 `run_es`」；§9.4 A-33-06「IPC 不可用时 100% 回落至 `es.exe`」。
- **代码事实**：宿主有 warm 进程池（`crates/dd-gui/src/app/pool.rs`，LRU 8）→ `dd-ext-search` **长驻进程**，且嵌套页 200ms 去抖重拉 `get_items`。
- **冲突类型**：生命周期缺口。若按 §9.2 用全局 `LazyLock` 永久持有 client，Everything 退出后重启 / 切换实例 / 服务重启时 client 窗口句柄永久失效 → **永久降级到 es.exe，永不恢复**，而 A-33-06 只覆盖"回落"，不覆盖"恢复"，测试全绿但用户实际收益为 0。
- **修正方向**：① 优先用 crate 自带的 `EverythingClient::shared()`（`Arc<EverythingClient>`，全部 Arc 释放后下次调用自动重建），**不自建 `LazyLock` 静态持有**；② 每次查询前经 `is_ipc_available()` / `is_db_loaded()` 探活，连续失败 N 次或超 TTL 则释放 Arc 触发重建；③ 新增验收项 **A-33-10：IPC 恢复**——Everything 退出→重启后，N 次查询内自动回到 IPC 通道（有日志为证）。

### R-03【硬冲突 · 双 windows 绑定并存 + `FILETIME` 类型来源未定义】

- **文档原说法**：§9.2 P2.1「新增最小 Windows target dependency」；§9.2 P2.5「`DateModified` 的单位（FILETIME 则复用 `filetime_to_unix`）」。
- **上游事实**：`everything-ipc 0.1.4` 依赖 **`windows 0.62`**（不是 `windows-sys`）；`QueryValue::Time(FILETIME)`、`QueryValue::U32(u32)`（attributes）中的 `FILETIME` 是 `windows` crate 类型；`QueryItem::get_time()` / `get_u32()` 为其取值入口。
- **代码事实**：`crates/dd-ext/Cargo.toml` 现有 `windows-sys 0.61`（apps/shell 用）；`search.rs:284` 已有 `filetime_to_unix(ft: i64)`。
- **冲突类型**：依赖与类型不匹配风险 → ① 两套 Windows 绑定并存（编译时间与体积，仅作用于 sidecar）；② 取 `FILETIME` 字段需 `windows` crate 与 crate 内版本一致，否则类型不互认。
- **修正方向**：`[target.'cfg(windows)'.dependencies]` 增加 `everything-ipc = { version = "=0.1.4", default-features = false }` 与同版本 `windows = { version = "0.62", features = ["Win32_Foundation"] }`；`FILETIME` → `(dwHigh<<32|dwLow)` 组合后复用既有 `filetime_to_unix`。版本用 **精确等号 `=0.1.4`**（0.1.x、一年 5 个版本，API 不稳定）并存档 docs.rs 快照。

### R-04【硬冲突 · 默认 feature 可能引入 tokio，与"禁止异步运行时"直接冲突】

- **文档原说法**：§9.2 P2.1「禁止引入异步运行时」。
- **上游事实**：crate feature 列表含 `tokio`（optional，sync+time）、`pe`/`pelite`、`folder`/`rapidhash`、`doc`、`tracing` 依赖等。
- **冲突类型**：默认 feature 集未约束 → 若 default 命中 `tokio`/`folder`，将引入异步运行时与无关依赖，违反 §9.2 P2.1 与 §9.3 L0「锁文件变化仅包含批准的 crate」。
- **修正方向**：显式 `default-features = false`，并在 L0 用 `cargo tree -e features -i everything-ipc` 逐项核对默认 feature；`tracing` 无 subscriber 时为零开销（不污染扩展 stdout 的 NDJSON 通道），需在单测中确认无 stdout 输出。

### W-05【缺口 · 支持矩阵缺 UIPI / 完整性级别维度】

- **事实**：P2 走 `WM_COPYDATA`；Windows UIPI 会拦截低完整性进程向更高完整性/提升进程发送窗口消息。Everything 常以**提升权限或服务方式**运行，`es.exe` 回落通道同样受影响。
- **影响**：A-33-08（Win10/11 × Everything 1.4/1.5）全绿，但真机"Everything 以管理员运行"场景大面积失败且静默。
- **修正方向**：矩阵增加「完整性级别 / 服务实例 / 1.5 实例名」维度；把 crate 的 `pipe`（Everything 1.5+ 命名管道）通道列为 v1.5 备选；失败必须可观测并回落（禁止静默）。

### W-06【缺口 · A-33-05 指标口径不可判定】

- **事实**：端到端 = 200ms 去抖 + 扩展查询 + 宿主渲染，`p50 < 10ms` 端到端不可能成立。
- **修正方向**：明确 A-33-05 为「扩展进程内 IPC 查询阶段」（对照组为同口径 `run_es`），另列端到端感知指标（如 ≤200ms 输入到首屏）。

### W-07【缺口 · 探活双通道与"索引未就绪"未定义】

- **代码事实**：现状 `available()` = `es.exe -get-everything-version`（`search.rs:109`）+ `AVAIL_TTL=3s`；用户文档已声明"新装 Everything 需等首轮索引"。
- **修正方向**：P2 后探活优先 `is_ipc_available()` + `is_db_loaded()`；`is_db_loaded=false` 时返回**引导项**而非空结果（避免"搜不到"误判）；保留 `es.exe` 定位逻辑（回落通道仍需）。

### W-08【过时 · 两处"待核实项"已可结案】

- `RequestFlags` **确含** `Attributes` 与 `DateModified`（共 16 个常量）→ 目录判定可用 `get_u32(RequestFlags::Attributes) & 0x10`，不再依赖 `guess_is_dir` 启发式（仅 es.exe 回落通道保留）。
- `DateModified` 确为 **FILETIME** → 复用 `filetime_to_unix`（`search.rs:284`）。
- 修正：把 §9.2 P2.5、update.md §3.3 的"待核实/若不可用则回退"改为"已核实 + 接入方式"。

---

## 三、方案外发现的既有实现缺陷（影响 A-33-03 用例覆盖）

### W-09【`file://` 未 percent-encode + 宿主无条件 `%XX` 解码，含 `%` 文件名打开失败】

- `search.rs`（`files.open.{pid}`）= `format!("file:///{}", path.replace('\\', "/"))` —— 未做任何编码；
- `crates/dd-gui/src/platform.rs:353` `file_url_to_path()` —— 无条件把 `%XX` 解码为字节。
- **可判定后果**：文件名形如 `report%20final.txt` → URL 含 `%20` → 宿主解出 `report final.txt` → 路径不存在 → 打开失败。
- 附带：`#` / `?` 未做 fragment/query 截断；UNC 路径 `\\server\share\x` → `file:////...`，`file_url_to_path` 对非空 host 返回 `None`（明确不支持）。
- **修正方向**：扩展侧做最小 percent-encode（至少 `%` `#` `?`），或宿主侧改为"先按原串存在性校验、失败再解码"；A-33-03 用例补 `%` / `#` / UNC 三类输入。

### W-10【两份文档内容重叠且含过时表述】

`search-file-update.md` §3.3 与 `search-file.md` §9.2 描述同一 P2，均含"query_wait 无超时/待核实 API"的旧表述 → 后续按任一旧版执行都会走偏。建议把 update.md 收敛为"v2 评审蓝本（历史）"，执行口径唯一指向 search-file.md §九。

---

## 四、已核实**无冲突**项（可放心作为方案前提）

| # | 核验项 | 事实来源 |
| :-- | :--- | :--- |
| 1 | 协议 v1.0 **冻结**；`more_commands` / `sender=context_menu` / `context.selected_item_id` / `CommandResult::{Dismiss,ShowToast}` / `host/set_clipboard` 均为既有定义 | `protocol.md`:3、49、349、351、471、498、504 |
| 2 | P1 三动作 = 主命令 + 2 个 `more_commands`（与 A-33-02「最多 3 个动作」一致） | `search.rs:551/574` |
| 3 | `PATH_INDEX_CAP = 1024`，30 条/页 → 约 34 页；淘汰最旧保证"注册即 lookup"恒命中 | `search.rs:59-81` |
| 4 | reveal 走 `CommandExt::raw_arg`（引号坑已规避），非 Windows 回退 xdg-open | `search.rs:783-814` |
| 5 | `spec()` 与 manifest capabilities 三能力一致（`host/open_url` / `host/show_status` / `host/set_clipboard`） | `search.rs:666`、`examples|dist/extensions.d/com.ddrun.filesearch.json` |
| 6 | 宿主渲染 `more_commands` 并以 `sender=context_menu` + `selected_item_id` 回调；未声明能力回 `-32601` | `ctx_menu.rs:184/201`、`host_actions.rs:13` |
| 7 | `EverythingClient` 为 **Send + Sync**（docs.rs auto impls），官方提供 `shared()` 全局 `Arc` → 全局复用前提成立 | docs.rs `wm::EverythingClient` |
| 8 | crate 真实存在且维护中：`everything-ipc 0.1.4`（2026-07，MIT，支持 Everything 1.4/1.5 含 alpha），同步 `query_wait` 与 tokio 异步两套 API | crates.io / lib.rs |
| 9 | 宿主 warm 进程池（LRU 8）→ 扩展长驻，IPC 省去每次 `es.exe` spawn 的收益**成立** | `app/pool.rs` |
| 10 | `dd-ext-search` 不在 `EMBED_EXES`（5 个内置扩展）→ P2 新增依赖**只增大 sidecar，不影响单文件宿主体积** | `dd-gui/build.rs:18` |

---

## 五、P2 编码前硬前置（建议作为开工门槛）

1. **R-01**：确认 `.timeout()` 用法并把 1000–1200ms 写入代码常量 + 单测（超时路径必须可观测、可回落）。
2. **R-03/R-04**：`cargo metadata --locked` + `cargo tree -e features -i everything-ipc` 核对：`windows 0.62` 引入范围、默认 feature 是否含 tokio/folder/pe、SIDECAR 体积增量、非 Windows 目标零回归。
3. **R-02**：确定 client 持有策略（`shared()` + 可释放重建）与探活/重建阈值，先写失效重建的伪代码再落地。
4. **W-05**：补齐支持矩阵维度（完整性级别 / 服务实例 / 1.5 实例名），确认是否需要 `pipe` 通道备选。
5. 文档修订：把 W-08 的"待核实"结案、R-01 的超时约束、A-33-10（IPC 恢复）写入 `search-file.md` §9.2/§9.4，并把 `search-file-update.md` 标记为历史蓝本。

---

## 六、尚未核实（需真机，不属本次静态核对范围）

- `es.exe -json -size -dm -attributes` 在 Everything 1.5 alpha 下的输出结构是否与现有解析兼容；
- 中文 Windows 下 `GetConsoleOutputCP()` 与 es.exe 实际输出代码页在非 936 区域的取值；
- `explorer /select,"<path>"` 对超长路径（>260）与网络驱动器的行为；
- `tracing` 在 release 下是否确有零 stdout 输出。
