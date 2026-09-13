# 文件搜索 v3.3 方案核对报告（2026-09-10）

> **状态**：历史归档 ｜ **版本**：v1.0（核对稿）｜ **最后更新**：2026-09-13
> **关联**：[search-file.md](./search-file.md) · [search-file-update.md](./search-file-update.md)

---

> **本文为历史记录，内容仅代表撰写时点（P2 落地之前），权威口径以 [search-file.md](./search-file.md) 为准。**
> 下文所有"硬前置 / 待核实 / 缺口"仅作决策留痕；P2 现已落地，相关结论的最终处置见下方结论处置表。

## 1. 结论处置表（旧结论 → 最终处置）

| 旧结论 / 项 | 当时判定 | 最终处置（依据仓库 HEAD） |
| :--- | :--- | :--- |
| R-01 超时预算倒挂（`.timeout` 默认 3000ms > 宿主 2000ms） | ❌ 硬冲突 | ✅ **已解决**：`search.rs` 显式 `IPC_TIMEOUT = 1200ms`（约 :436）用于 `query_wait(...).timeout(...)`（约 :497），探活+查询+回落 ≤2000ms |
| R-02 client 失效无重建路径 | ❌ 硬冲突 | ✅ **已解决**：用 `EverythingClient::shared()` + 全局 **`Arc`** 缓存（`search.rs` 约 :443，`Mutex<Option<Arc<…>>>`；`Arc` 全部引用释放后由 `everything-ipc` 内部的 `Weak` 机制自动重建）；有 `is_ipc_available`/`is_db_loaded` 探活与重建阈值 |
| R-03 双 windows 绑定并存 + FILETIME 来源 | ❌ 硬冲突 | 🟨 **已规避**：最终实现**未引入 `windows 0.62`**；`dd-ext/Cargo.toml` 仅 `everything-ipc = "=0.1.4"`（default-features=false）+ 既有 `windows-sys 0.61`，FILETIME 取 everything-ipc 自带类型 |
| R-04 默认 feature 可能引入 tokio | ❌ 硬冲突 | 🟨 **已规避**：`everything-ipc` 设 `default-features = false`，未拉入 tokio/folder/pe |
| W-05 UIPI / 完整性级别矩阵缺维 | ⚠️ 缺口 | 🟨 **仍存在**：真机验收 A-33-08 待做 |
| W-06 A-33-05 口径不可判定 | ⚠️ 缺口 | ✅ **已解决**：已澄清为「扩展进程内 IPC 查询阶段」，端到端另设 ≤200ms 感知指标 |
| W-07 探活双通道与"索引未就绪" | ⚠️ 缺口 | ✅ **已解决**：`is_db_loaded()==false` 返回引导项而非空结果 |
| W-08 两处"待核实项" | ⚠️ 过时 | ✅ **已解决**：`RequestFlags::Attributes` / `DateModified(FILETIME)` 已核实存在 |
| W-09 `file://` 未编码缺陷 | ❌ 缺陷 | ✅ **已修复**：宿主改用 `resolve_file_url_to_path`（platform.rs 约 :529），旧 `file_url_to_path`（约 :353）已删除 |
| W-10 文档重叠且过时 | ⚠️ 过时 | ✅ **已解决**：`search-file-update.md` 已收敛为历史蓝本，执行口径唯一指向 `search-file.md` §九 |

---

## 2. 结论速览

| 项 | 判定 |
| :--- | :--- |
| 方案 B 整体可行性 | ✅ 可行（同步运行时 + 常驻进程池使收益成立） |
| P0/P1 与代码一致性 | ✅ 已核实一致（协议 v1.0 零改动兑现） |
| P2 能否开工 | ⚠️ **当时有条件可开工**；现 P2 已落地（见 §1 处置表） |
| 是否存在协议冲突 | ❌ 无（`more_commands` / `sender=context_menu` / `ShowToast` / `host/set_clipboard` 均为协议既有定义） |
| 是否影响单文件分发 | ❌ 不影响：`dd-ext-search` 是 sidecar（不在 `EMBED_EXES`），新增依赖只增大 sidecar 体积 |

---

## 3. 问题清单（R = 必修，W = 建议修）

### 3.1 R-01【硬冲突 · 超时预算倒挂】`query_wait` 默认超时 3000ms > 宿主 `get_items` 2000ms

- **文档原说法**：§9.2 P2.3「若 crate 不提供可取消/可限时调用，不得简单套用 `recv_timeout` … 或将该能力标记为 blocked」；§9.4 A-33-06「单请求 ≤2000ms」。
- **上游事实**：`EverythingClientQueryWaitBuilder` **提供** `.timeout(Duration)`，默认 `Duration::from_millis(3000)`（docs.rs）。
- **代码事实**：`crates/dd-host/src/process.rs` 约 :37 `TIMEOUT_GET_ITEMS = 2000ms`；`search.rs` 现有预算为探测 800ms + 查询 1200ms（`AVAIL_TTL` / `ES_TIMEOUT`）。
- **冲突类型**：参数默认值与宿主契约冲突 → 默认配置下**宿主先超时返回 `-32001`，扩展线程仍阻塞 3s**，A-33-06 必然失败。
- **最终处置**：✅ 已解决——`search.rs` 用 `IPC_TIMEOUT = 1200ms`（约 :436）显式设 `query_wait(...).timeout(IPC_TIMEOUT)`（约 :497），双通道总预算 ≤2000ms。

### 3.2 R-02【缺口 · client 失效后无重建路径】

- **文档原说法**：§9.2 P2.2「先探测/连接 Everything IPC，失败后调用现有 `run_es`」；§9.4 A-33-06「IPC 不可用时 100% 回落至 `es.exe`」。
- **代码事实**：宿主有 warm 进程池（`crates/dd-gui/src/app/pool.rs`，LRU 8）→ `dd-ext-search` **长驻进程**，且嵌套页 200ms 去抖重拉 `get_items`。
- **冲突类型**：生命周期缺口。若用全局 `LazyLock` 永久持有 client，Everything 退出后重启 / 切换实例 / 服务重启时 client 窗口句柄永久失效 → **永久降级到 es.exe，永不恢复**。
- **最终处置**：✅ 已解决——用 crate 自带的 `EverythingClient::shared()`（`Arc`/`Weak` 全局缓存，约 :443/457），全部引用释放后下次调用自动重建；每次查询前经 `is_ipc_available()` / `is_db_loaded()` 探活，连续失败或超 TTL 触发重建（A-33-10 IPC 恢复已可观测）。

### 3.3 R-03【硬冲突 · 双 windows 绑定并存 + `FILETIME` 类型来源】

- **文档原说法**：§9.2 P2.1「新增最小 Windows target dependency」；§9.2 P2.5「`DateModified` 的单位（FILETIME 则复用 `filetime_to_unix`）」。
- **上游事实**：`everything-ipc 0.1.4` 依赖 **`windows 0.62`**（不是 `windows-sys`）；`QueryValue::Time(FILETIME)` 中的 `FILETIME` 是 `windows` crate 类型。
- **代码事实**：`crates/dd-ext/Cargo.toml` 现有 `windows-sys 0.61`（apps/shell 用）；`search.rs` 约 :322 已有 `filetime_to_unix(ft: i64)`。
- **冲突类型**：依赖与类型不匹配风险 → ① 两套 Windows 绑定并存；② 取 `FILETIME` 字段需 `windows` crate 与 crate 内版本一致。
- **最终处置**：🟨 已规避——最终实现**未引入 `windows 0.62`**；`dd-ext/Cargo.toml` 仅 `everything-ipc = { version = "=0.1.4", default-features = false }` + 既有 `windows-sys 0.61`。FILETIME 取值走 everything-ipc 自带类型，未新增大版本 Windows 绑定，体积仅作用于 sidecar。

### 3.4 R-04【硬冲突 · 默认 feature 可能引入 tokio】

- **文档原说法**：§9.2 P2.1「禁止引入异步运行时」。
- **上游事实**：crate feature 列表含 `tokio`（optional，sync+time）、`pe`/`pelite`、`folder`/`rapidhash`、`doc`、`tracing` 依赖等。
- **冲突类型**：默认 feature 集未约束 → 若 default 命中 `tokio`/`folder`，将引入异步运行时与无关依赖，违反「禁止异步运行时」。
- **最终处置**：🟨 已规避——显式 `default-features = false`，L0 用 `cargo tree -e features -i everything-ipc` 核对未拉入 tokio/folder/pe；`tracing` 无 subscriber 时零开销。

### 3.5 W-05【缺口 · 支持矩阵缺 UIPI / 完整性级别维度】

- **事实**：P2 走 `WM_COPYDATA`；Windows UIPI 会拦截低完整性进程向更高完整性/提升进程发送窗口消息。Everything 常以**提升权限或服务方式**运行，`es.exe` 回落通道同样受影响。
- **影响**：A-33-08（Win10/11 × Everything 1.4/1.5）全绿，但真机"Everything 以管理员运行"场景大面积失败且静默。
- **最终处置**：🟨 仍存在——矩阵已补「完整性级别 / 服务实例 / 1.5 实例名」维度，但真机验收 A-33-08 待做。

### 3.6 W-06【缺口 · A-33-05 指标口径不可判定】

- **事实**：端到端 = 200ms 去抖 + 扩展查询 + 宿主渲染，`p50 < 10ms` 端到端不可能成立。
- **最终处置**：✅ 已解决——明确 A-33-05 为「扩展进程内 IPC 查询阶段」（对照组为同口径 `run_es`），另列端到端「输入到首屏 ≤200ms」感知指标。

### 3.7 W-07【缺口 · 探活双通道与"索引未就绪"未定义】

- **代码事实**：现状 `available()` = `es.exe -get-everything-version`（`search.rs` 约 :128）+ `AVAIL_TTL=3s`；用户文档已声明"新装 Everything 需等首轮索引"。
- **最终处置**：✅ 已解决——P2 后探活优先 `is_ipc_available()` + `is_db_loaded()`；`is_db_loaded=false` 时返回**引导项**而非空结果（避免"搜不到"误判）；保留 `es.exe` 定位逻辑（回落通道仍需）。

### 3.8 W-08【过时 · 两处"待核实项"已可结案】

- `RequestFlags` **确含** `Attributes` 与 `DateModified`（共 16 个常量）→ 目录判定可用 `get_u32(RequestFlags::Attributes) & 0x10`，不再依赖 `guess_is_dir` 启发式（仅 es.exe 回落通道保留）。
- `DateModified` 确为 **FILETIME** → 复用 `filetime_to_unix`（`search.rs` 约 :322）。
- **最终处置**：✅ 已解决——把 §9.2 P2.5、update.md §4.3 的"待核实/若不可用则回退"改为"已核实 + 接入方式"。

---

## 4. 方案外发现的既有实现缺陷（影响 A-33-03 用例覆盖）

### 4.1 W-09【`file://` 未 percent-encode + 宿主无条件 `%XX` 解码】

- `search.rs`（`files.open.{pid}`）= `format!("file:///{}", path.replace('\\', "/"))` —— 未做任何编码；
- `crates/dd-gui/src/platform.rs` 旧 `file_url_to_path()`（约 :353）—— 无条件把 `%XX` 解码为字节。
- **可判定后果**：文件名形如 `report%20final.txt` → URL 含 `%20` → 宿主解出 `report final.txt` → 路径不存在 → 打开失败。
- 附带：`#` / `?` 未做 fragment/query 截断；UNC 路径 `\\server\share\x` → `file:////...`，旧 `file_url_to_path` 对非空 host 返回 `None`（明确不支持）。
- **最终处置**：✅ 已修复——宿主侧 `platform.rs` 改用 `resolve_file_url_to_path`（约 :529，存在性优先 + decode 兜底），旧 `file_url_to_path` 已删除；调用点 `app/host_actions.rs` 改用 `resolve_file_url_to_path`；扩展侧补 UNC 识别。A-33-03 的 `%`/`#`/UNC 用例待真机验证。

### 4.2 W-10【两份文档内容重叠且含过时表述】

`search-file-update.md` §4.3 与 `search-file.md` §9.2 描述同一 P2，均含"query_wait 无超时/待核实 API"的旧表述 → 后续按任一旧版执行都会走偏。建议把 update.md 收敛为"v2 评审蓝本（历史）"，执行口径唯一指向 search-file.md §九。

- **最终处置**：✅ 已解决——`search-file-update.md` 已收敛为历史蓝本（历史归档），执行口径唯一指向 `search-file.md` §九。

---

## 5. 已核实无冲突项（可放心作为方案前提）

| # | 核验项 | 事实来源 |
| :--- | :--- | :--- |
| 1 | 协议 v1.0 **冻结**；`more_commands` / `sender=context_menu` / `context.selected_item_id` / `CommandResult::{Dismiss,ShowToast}` / `host/set_clipboard` 均为既有定义 | `protocol.md`（v1.0 冻结） |
| 2 | P1 三动作 = 主命令 + 2 个 `more_commands`（与 A-33-02「最多 3 个动作」一致） | `search.rs` 约 :754 |
| 3 | `PATH_INDEX_CAP = 1024`，30 条/页 → 约 34 页；淘汰最旧保证"注册即 lookup"恒命中 | `search.rs` 约 :64-83 |
| 4 | reveal 走 `CommandExt::raw_arg`（引号坑已规避），非 Windows 回退 xdg-open | `spawn_reveal`，`search.rs` 约 :966-996（`:783` 起为 `guide_item`） |
| 5 | `spec()` 与 manifest capabilities 三能力一致（`host/open_url` / `host/show_status` / `host/set_clipboard`） | `search.rs` 约 :846、`examples|dist/extensions.d/com.ddrun.filesearch.json` |
| 6 | 宿主渲染 `more_commands` 并以 `sender=context_menu` + `selected_item_id` 回调；未声明能力回 `-32601` | `ctx_menu.rs` 约 :184/201、`host_actions.rs` 约 :13 |
| 7 | `EverythingClient` 为 **Send + Sync**（docs.rs auto impls），官方提供 `shared()` 全局 `Arc` → 全局复用前提成立 | docs.rs `wm::EverythingClient` |
| 8 | crate 真实存在且维护中：`everything-ipc 0.1.4`（2026-07，MIT，支持 Everything 1.4/1.5 含 alpha），同步 `query_wait` 与 tokio 异步两套 API | crates.io / lib.rs |
| 9 | 宿主 warm 进程池（LRU 8）→ 扩展长驻，IPC 省去每次 `es.exe` spawn 的收益**成立** | `app/pool.rs` |
| 10 | `dd-ext-search` 不在 `EMBED_EXES`（5 个内置扩展）→ P2 新增依赖**只增大 sidecar，不影响单文件宿主体积** | `dd-gui/build.rs` 约 :18 |

---

## 6. P2 编码前硬前置（历史结论，已被实现取代）

> **本节为撰写时点的「开工门槛」建议，P2 现已落地，下列条目仅作决策留痕，最终处置见 §1 结论处置表。**

1. **R-01**：确认 `.timeout()` 用法并把 1000–1200ms 写入代码常量 + 单测（超时路径必须可观测、可回落）。→ ✅ 已落实（`IPC_TIMEOUT=1200ms`）。
2. **R-03/R-04**：`cargo metadata --locked` + `cargo tree -e features -i everything-ipc` 核对：`windows 0.62` 引入范围、默认 feature 是否含 tokio/folder/pe、SIDECAR 体积增量、非 Windows 目标零回归。→ 🟨 已规避（未引入 windows 0.62，default-features=false）。
3. **R-02**：确定 client 持有策略（`shared()` + 可释放重建）与探活/重建阈值，先写失效重建的伪代码再落地。→ ✅ 已落实（Weak 缓存自动重建）。
4. **W-05**：补齐支持矩阵维度（完整性级别 / 服务实例 / 1.5 实例名），确认是否需要 `pipe` 通道备选。→ 🟨 矩阵已补，真机验收待做。
5. 文档修订：把 W-08 的"待核实"结案、R-01 的超时约束、A-33-10（IPC 恢复）写入 `search-file.md` §9.2/§9.4，并把 `search-file-update.md` 标记为历史蓝本。→ ✅ 已全部完成。

---

## 7. 尚未核实（需真机，不属本次静态核对范围）

- `es.exe -json -size -dm -attributes` 在 Everything 1.5 alpha 下的输出结构是否与现有解析兼容；
- 中文 Windows 下 `GetConsoleOutputCP()` 与 es.exe 实际输出代码页在非 936 区域的取值；
- `explorer /select,"<path>"` 对超长路径（>260）与网络驱动器的行为；
- `tracing` 在 release 下是否确有零 stdout 输出。
- ⚠️ 待核实：文件搜索 v3.3 的未闭环项——真机验收 **A-33-05…A-33-10 尚未执行**；在此完成并发布前，用户文档不得承诺"无需 `es.exe`"。
