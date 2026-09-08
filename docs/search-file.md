# 📋 dd-run 文件搜索执行方案（v3.2：Everything 优先版 · 架构对齐 + f 前缀自动进页 + 速度打磨）

> **现状（核对至 2026-09-08）**：本文档的「便捷 + 速度优化」（§7）与 v0.1 文件搜索核心已随 M6/M7 落地——`crates/dd-ext/src/bin/search.rs`（bin 名 `dd-ext-search`）、宿主 `f ` 前缀自动进页（`crates/dd-gui/src/app/mod.rs`）、免安装 sidecar 分发（`tools/package.sh` → `dist/extensions.d/`，见 [`implementation.md`](./implementation.md) §5 M6/M7 行）。下文 §二 时间线、§三 任务分解、§六 D0–D6 验收清单为 **v3.1 计划期口径**，保留作 v0.1 完整规划与 Release 验收门的原始记录；与落地实现不符处已在文内标注「已落地 / 差异见 §7.x」。
> **v3.1 修订说明（对齐 2026-09-07 同步后的仓库架构）**：原 v3 基于"自创 `search`/`search_status` 方法 + 异步 Provider trait + 独立 Ctrl+F 面板"假设撰写，与当前仓库实际严重冲突。本次修订已逐条核对 [`docs/protocol.md`](./protocol.md)（v1.0 **冻结**）、[`docs/manifest-schema.md`](./manifest-schema.md)（v1.0）、`crates/dd-ext`（同步运行时）、`crates/dd-gui`（宿主）后重写技术章节。核心修正：
> 1. **不新增协议方法**：v1.0 协议已冻结且无 `search`/`search_status`。本方案完全复用 provider 模型（`initialize`/`top_level_commands`/`fallback_commands`/`invoke`）+ 协议已定义但**运行时尚未实现**的 `get_items`（§6.3）。实现 `get_items` 属于"补齐协议合规"，**不是**协议变更（无需走 §13 演进）。
> 2. **运行时是同步的**：`dd_ext::run` + `ExtensionSpec`，所有处理器为纯函数 `fn`。原方案的 `async_trait`/`reqwest` 异步/`tokio`/`.await` 无法编译 → 改用标准库 `TcpStream`（HTTP/1.1 `Connection: close`）。
> 3. **扩展落位修正**：作为第 6 个**内置扩展**放入 `crates/dd-ext/src/bin/search.rs`（沿用 apps/calc/websearch 模式），经清单 `com.ddrun.filesearch.json` 注册。M7 批次 7.4/7.5 定案**免安装 sidecar**：清单源码在 `examples/extensions.d/`，由 `tools/package.sh` 归集进 `dist/extensions.d/`，宿主扫描可执行文件同目录的 `extensions.d/`；开发期指向本地构建产物。原 `extensions/dd-ext-search/` + 仓库根 `extensions.d/search.json` 与实际不符。
> 4. **结果列表改为 `get_items(search_text)`**：当前运行时 `get_items` 返回 `-32005`，故"扩展 `dd_ext` 运行时支持子页"是必备前置（小改动）。文件结果对外一律是 `CommandItem[]`，删除虚构的 `results: SearchResult[]` 协议响应。
> 5. **打开文件走 `host/open_url`（`file://`）**：协议无 `host/open_file`；删除原 `open::that(path)` 直开假设，改为经宿主打开，并标注需实测 `webbrowser::open` 对 `file://` 的行为。
> 6. **删除独立 Ctrl+F 面板**：真实宿主是单一聚合面板，文件结果随主搜索框经 `fallback_commands`+`get_items` 自然呈现；防抖由宿主侧控制，扩展内不做。
> 7. **跨平台矛盾修正**：Everything 仅 Windows 可用，故 v0.1 **仅 Windows**；原"三平台产物"与"v0.1 依赖 Everything"自相矛盾 → 改为 v0.2（fd Provider）再谈跨平台。
> 8. **评分/类型 bug 修复**：`SkimMatcherV2::fuzzy_match` 返回 `Option<i64>`，原 `name_score + path_score*0.3` 存在 i64/f64 混算编译错误且 `score` 量级不对 → 归一化到 0~1；`date_modified` 必须规范化为 Unix 秒后再与 `now_7days()` 比较（原文直接比较会错）。
> 9. **依赖修正**：相对 dd-ext 基线，**本功能新增的第三方依赖仅 `fuzzy-matcher` + `chrono`**（`anyhow`/`serde`/`serde_json` 为既有基线依赖，search.rs 复用）；HTTP 用标准库 `TcpStream`、URL 编码用扩展内手写 `pct_encode`；**不引入** `tokio`、`async-trait`、`reqwest`、`urlencoding`（同步模型 + 最小依赖）。
> **v3.2 追注**：上列第 2、9 条中「`reqwest::blocking` / `urlencoding`」表述已被 §7.2 落地实现取代（实际 dd-ext 依赖仅 `fuzzy-matcher` / `chrono`，HTTP 用标准库 `TcpStream`、URL 编码用扩展内手写 `pct_encode`）；第 3 条「由安装器写入」已被 M7 批次 7.4/7.5 的**免安装 sidecar** 定案取代（清单源码在 `examples/extensions.d/`，由 `tools/package.sh` 归集进 `dist/extensions.d/`）。其余各条为 v3.1 相对原 v3 的修正记录，保留。

> **调整原因**：本地已安装 Everything，先接入 Everything 可以**用最小代价最快跑通全链路**（运行时 → Provider → UI），且 Everything 的搜索性能天花板最高，适合作为架构的第一个验证者。
> **架构不变**：provider 抽象已在 `dd_ext` 运行时固化（`ExtensionSpec`），fd 作为 v0.2 的第二个 Provider 接入（届时只是"加一个命令分支 / 新 bin"，不动运行时与协议）。
> **总工期**：**约 5~6 个工作日**（含运行时 `get_items` 补齐这一必备前置）。

---

## 一、调整后的 Provider 优先级与演进路线

```mermaid
flowchart LR
    subgraph v0.1["v0.1（本方案，5~6天，Windows-only）"]
        A1["Everything（同步 HTTP）<br>唯一 Provider"]
    end
    subgraph v0.2["v0.2（后续，跨平台）"]
        A2["+ fd Provider<br>（自动下载，兜底未装Everything的用户）"]
    end
    subgraph v0.3["v0.3+（远期）"]
        A3["+ ripgrep 内容搜索<br>+ 自研轻量索引"]
    end
    v0.1 --> v0.2 --> v0.3
```

**v0.1 的 Provider 选择逻辑（极简）**：

```
扩展启动 → initialize 返回 provider（has_fallback=true）
用户在主面板输入 → 宿主对该 Provider 调 fallback_commands（得"在文件中搜索 {query}"入口）
  ├─ 选中该入口 → 宿主进 page（files.results），调 get_items(search_text=query)
  │     ├─ Everything 在线 → 返回前 N 条文件 CommandItem（可↑↓导航、回车打开）
  │     └─ Everything 不在线 → 返回单条引导项（点击显示开启方法 Toast）
  └─ 不选中 → 无动作
```

> ⚠️ **明确取舍**：v0.1 阶段，没装 Everything 或未开启 HTTP 服务的用户无法使用文件搜索。这对**你自己使用**完全没问题，对公开发布而言 v0.2 补上 fd 即可闭环。Everything 仅 Windows，故 v0.1 仅 Windows 构建与发布。

---

## 二、时间线（5~6 天）

```mermaid
timeline
    title Everything 接入执行时间线
    section 前置：运行时补齐（第1天上午）
        D0 : 扩展 dd_ext 运行时支持 get_items（子页）
    section 阶段一：扩展骨架（第1~2天）
        D1 : dd-ext-search 内置扩展骨架
        D1 : ExtensionSpec + 清单 + NDJSON 联调
        D2 : EverythingProvider（探测/搜索/映射）
    section 阶段二：Everything Provider（第3~4天）
        D3 : 同步 HTTP 探测 + search 实现
        D4 : 字段映射 + date_modified 规范化 + 评分排序
    section 阶段三：联调与UI（第5天）
        D5 : 宿主聚合 + fallback/page 渲染 + 回车打开
    section 阶段四：测试与发布（第6天）
        D6 : 异常场景测试 + 文档 + 发布 v0.1.0
```

---

## 三、详细任务分解

### ✅ 前置：运行时补齐 `get_items`（第 1 天上午，必备）

当前 `crates/dd-ext/src/lib.rs` 的 `get_items` 分支**硬编码返回 `-32005 Page not found`**（5 个内置扩展均无子页）。文件搜索需要"可浏览结果列表"，协议 §6.3 已定义 `get_items(page_id, search_text)`，因此只需**在运行时补一个子页注册点**，不改变协议。

```rust
// crates/dd-ext/src/lib.rs（v3.2 已落地，签名与实现一致）
pub type PageHandler = fn(&GetItemsParams) -> GetItemsResult;

// ExtensionSpec 增加字段：
//   pub pages: Option<PageHandler>,
// serve_line 的 "get_items" 分支：
//   声明了 pages → 调 handler 返回 GetItemsResult（整页 CommandItem[]，含 has_more_items / is_loading）
//   未声明（pages: None，如 5 个内置扩展）→ -32005 Page not found
```

- 验收：新增 `PageHandler` 后，原 5 个内置扩展 `pages: None` → 行为不变（仍 `-32005`）；文件搜索扩展 `pages: Some(get_file_items)` → 返回 `GetItemsResult`（已落地，单测见 `crates/dd-ext/src/lib.rs`）。
- 该变更为协议兼容（方法已存在），**不修改 `docs/protocol.md`**。

### ✅ 阶段一：扩展骨架（第 1~2 天）

#### 任务 1.1：项目结构与注册位置（第 1 天）

```
dd-run/
├── crates/
│   └── dd-ext/                      # 复用既有内置扩展 crate
│       ├── Cargo.toml               # 依赖见下；[[bin]] 追加 dd-ext-search
│       └── src/
│           ├── bin/
│           │   ├── apps.rs          # 既有
│           │   ├── calc.rs          # 既有
│           │   ├── shell.rs         # 既有
│           │   ├── system.rs        # 既有
│           │   ├── websearch.rs     # 既有（最接近的类比，照此写）
│           │   └── search.rs        # ⭐ 文件搜索内置扩展（bin 名 dd-ext-search）
│           ├── lib.rs               # 运行时（含前述 get_items 补齐）
│           └── i18n.rs              # 既有
├── examples/extensions.d/
│   └── com.ddrun.filesearch.json    # 清单（源码版，${EXT_DIR} 相对定位 exe）
└── （发布归集，M7 批次 7.4/7.5 定案：免安装 sidecar）
    dist/extensions.d/               # dd-ext-search.exe + 清单（tools/package.sh 产出）
```

> 💡 开发期可用一份指向本地构建产物的清单（`entry.command` 指向 `target\release\dd-ext-search.exe`）放到扩展目录做联调；发布由 `tools/package.sh` 归集进 `dist/extensions.d/`（M7 批次 7.4/7.5：免安装 sidecar，宿主扫描**可执行文件同目录**的 `extensions.d/`，清单 `${EXT_DIR}` 自动定位 exe，解压即用）。清单 schema 见 [`docs/manifest-schema.md`](./manifest-schema.md)。

**`Cargo.toml`（dd-ext）依赖**（同步方案，已修正）：

```toml
[dependencies]
dd-protocol = { path = "../dd-protocol" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
fuzzy-matcher = "0.3"          # 提供 skim::SkimMatcherV2（评分归一化，任务 2.3）
chrono = "0.4"                 # date_modified 规范化（任务 2.2）
# 注：HTTP 用标准库 std::net::TcpStream、URL 编码用扩展内手写 pct_encode（§7.2 依赖最小）；
#     不使用 reqwest / urlencoding / tokio / async-trait（同步模型 + 最小依赖）
```

#### 任务 1.2：协议对齐（第 1 天，不新增协议方法）

**复用 v1.0 已定义的方法**，不向 `docs/protocol.md` 追加方法：

| 协议方法（已实现/将实现） | 文件搜索中的用途 |
| :--- | :--- |
| `initialize` | 返回 `provider.id=com.ddrun.filesearch`、`has_fallback=true`、`capabilities=[host/open_url, host/show_status]` |
| `top_level_commands` | 首屏（query 为空）：可返回一条"文件搜索"说明项；或留空由 fallback 承担 |
| `fallback_commands` | **每个非空 query** 返回一条入口项 `"在文件中搜索 {query}"`，`command=Page{files.results}`（宿主渲染时替换 `{query}`） |
| `get_items` | 进入 `files.results` 页后，宿主带 `search_text` 调用 → 返回前 N 条文件 `CommandItem`（**前置任务已补齐**） |
| `invoke` | `files.open.<u64>` → 经 `host/open_url`（`file://`）打开文件，`Dismiss` |
| `host/open_url` | 扩展反向请求，打开 `file://` 文件；`host/show_status` 已声明（清单合规）但**当前扩展未实际发起**——Everything 不可用时的引导经 `get_items` 返回列表项 `files.guide`，点击落回通用「未知命令」Toast（见 §7.4） |

> ❌ **删除原 v3 的 `search` / `search_status` 两张表**：它们不在 v1.0 协议里，且会触发协议冻结约束。Everything 可用性与结果列表改由 `fallback_commands` + `get_items` 表达。

**统一结果条目（扩展内部 `FileEntry`，对外映射为 `CommandItem`）**：

> v3.2 起 `id` 为 `files.open.<u64>`：路径经进程内 `register_path` 索引、`invoke` 时 `lookup_path` 取回——不再把路径字符内嵌 id（避免转义/超长）。

```json
{
  "id": "files.open.17",
  "title": "main.rs",
  "subtitle": "C:\\proj\\src\\main.rs",
  "icon": { "type": "glyph", "value": "\uE7C3" },
  "section": "文件",
  "command": { "kind": "invoke" },
  "details": { "title": "main.rs", "body": "大小 1.0 KB · 修改 2024-05-30 12:34" }
}
```

#### 任务 1.3：ExtensionSpec 与子页处理器（第 2 天，替代原 Provider trait）

原 v3 的 `SearchProvider` trait + `async_trait` 与运行时不符。本方案直接声明 `ExtensionSpec`（与 websearch 同构），并新增 `pages` 处理器：

```rust
// crates/dd-ext/src/bin/search.rs（骨架，与实际实现同构）
use dd_ext::{i18n::tr, run, Effect, ExtensionSpec};
use dd_protocol::messages::{GetItemsParams, GetItemsResult, InvokeParams};
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.filesearch",
        display_name: tr("文件搜索", "File Search"),
        description: tr(
            "基于 Everything 的本地文件搜索（输入 f 后空格直接进入）",
            "Local file search powered by Everything (type 'f ' to jump in)",
        ),
        frozen: false,                 // 结果随 query 变化，不可冻结缓存
        has_fallback: true,
        capabilities: &["host/open_url", "host/show_status"],
        log_tag: "dd-ext-filesearch",
        top_level: top_level_commands,
        fallback: Some(fallback_commands),
        invoke: handle_invoke,
        pages: Some(get_file_items),   // 前置任务新增的字段
    }
}

fn fallback_commands() -> Vec<CommandItem> {
    vec![CommandItem {
        id: "files.search.query".into(),
        title: tr("在文件中搜索 {query}", "Search files for {query}").into(),
        subtitle: Some(tr("用 Everything 搜索本地文件", "Search local files with Everything").into()),
        icon: Some(Icon { kind: IconKind::Glyph, value: file_glyph(false).to_string() }), // 文件字形
        section: Some(tr("文件", "Files").into()),
        tags: Some(vec!["files".into()]),
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Page { page_id: "files.results".into() },
    }]
}

fn get_file_items(params: &GetItemsParams) -> GetItemsResult {
    let query = params.search_text.as_deref().unwrap_or("").trim();
    if query.is_empty() {
        return GetItemsResult { items: vec![hint_item()], has_more_items: false, is_loading: false };
    }
    if !everything_available() {
        // Everything 未启动：返回单条引导项（title/subtitle 写明开启方法；点击文案见 §7.4）
        return GetItemsResult { items: vec![guide_item()], has_more_items: false, is_loading: false };
    }
    match search(query, RESULT_LIMIT) {          // HTTP 用标准库 TcpStream（§7.2 依赖最小）
        Ok(entries) => {
            let items: Vec<CommandItem> = score_and_sort(entries, query)
                .into_iter().map(to_command_item).collect();
            GetItemsResult { items, has_more_items: false, is_loading: false }
        }
        Err(e) => GetItemsResult { items: vec![error_item(&e.to_string())], has_more_items: false, is_loading: false },
    }
}

fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
    if let Some(n_str) = params.id.strip_prefix("files.open.") {
        if let Ok(n) = n_str.parse::<u64>() {
            if let Some(path) = lookup_path(n) {   // 进程内路径索引；id 不内嵌路径
                let url = format!("file:///{}", path.replace('\\', "/"));
                return (CommandResult::Dismiss, vec![Effect::HostRequest {
                    method: "host/open_url",
                    params: serde_json::json!({ "url": url }),
                }]);
            }
        }
    }
    (CommandResult::ShowToast { message: tr("未知命令", "Unknown command").to_string(), duration_ms: Some(2000) }, Vec::new())
}

fn main() { run(&spec()); }
```

> 💡 **架构验证点**：v0.2 接 fd 时，只需在 `get_file_items` 内增加"Everything 不可用 → 调 fd 兜底"的分支（或新增 `files.search.fd` 入口），运行时、协议、清单结构**零改动**。这就是先定 provider 模型的价值。

### ✅ 阶段二：Everything Provider（第 3~4 天）

#### 任务 2.1：同步 HTTP 探测与搜索请求（第 3 天）

```rust
// crates/dd-ext/src/bin/search.rs（v3.2 已落地：标准库 TcpStream，零 HTTP 依赖）
fn everything_base() -> (String, u16) {
    // 默认 127.0.0.1:8080；可用环境变量 DDRUN_EVERYTHING_URL=http://host:port 覆盖
    // （文档「配置项说明」同源；Everything HTTP 默认端口为 80，本扩展默认 8080）
}

/// Everything 是否在线（availability TTL 3s 缓存：窗口内跳过重复探测）。
fn everything_available() -> bool {
    // std::net::TcpStream::connect 最简探测：GET /?json=1&count=1，
    // 读写超时均 800ms（快速失败）——Everything 侧配置见 2.4
}

/// 极简 HTTP/1.1 GET（Connection: close → 读到 EOF 即 body 结束，规避分块解析）。
fn http_get(rel: &str) -> anyhow::Result<String> {
    // 读超时 3s（Everything 本地通常 <50ms，3s 仅作异常兜底；宿主 get_items 超时 2s，见 §7.4）、
    // 写超时 800ms；body 从首个 CRLFCRLF 之后截取。
}

/// 仅编码 URL 不安全字节（RFC 3986 非保留字符保留），供 Everything 查询拼 URL。
fn pct_encode(input: &str) -> String {
    // 与 urlencoding crate 语义等价：保留 -_.~ 与字母数字，其余 %XX 大写十六进制。
}

fn search(q: &str, limit: usize) -> anyhow::Result<Vec<FileEntry>> {
    let rel = format!(
        "/?search={}&json=1&count={}&path_column=1&size_column=1&date_modified_column=1",
        pct_encode(q), limit
    );
    let body = http_get(&rel)?;
    // serde 解析 EvResponse{ results: Vec<RawEntry> } → Vec<FileEntry>
}
```

**Everything JSON 响应字段映射（RawEntry → FileEntry）**：

| Everything 字段 | FileEntry 字段 | 说明 |
| :--- | :--- | :--- |
| `results[].name` | `name` | 文件名 |
| `results[].path` | `dir` | 所在目录，需与 `name` 拼接为完整路径 |
| `results[].size` | `size` | 需开启 `size_column=1`，单位字节 |
| `results[].date_modified` | `modified` | 见任务 2.2 **必须规范化为 Unix 秒** |
| `results[].type` | `is_dir` | 目录判定首选（`"folder"`）；缺失时退回「无扩展名且 size==0」启发式（见 2.2） |

> ⚠️ **超时与阻塞**：运行时主循环是同步的，`available()`/`search()` 同步阻塞。Everything 本地响应通常 <50ms，远低于协议 `get_items` 默认 2000ms，可接受。若担心阻塞面板，宿主侧对 `get_items` 已有串行化与超时保护（协议 §10）。

#### 任务 2.2：字段映射 + `date_modified` 规范化（第 4 天）

> ⚠️ **已知坑（保留并修正）**：Everything 的 `date_modified` 返回格式随版本/设置而异（常见为本地时间字符串 `2024-05-30 12:34:56.789` 或 FILETIME）。**D4 当天务必用你本机实际响应锁定解析逻辑**，写一个针对真实响应样例的解析单元测试；解析失败则 `modified = 0`（不报错、不 panic）。

```rust
/// 把 Everything 的 date_modified 规范化为 Unix 秒；任一步失败返回 0（不 panic）。
/// 实现：按空格切分日期/时间 → 按 `-`/`/` 切年月日 → 按 `:` 切时分秒
/// （秒可带 `.fff` 子秒，截断即可）→ chrono NaiveDate/NaiveDateTime 构造 → UTC 秒。
/// 配套单测 date_parse_known_formats / date_parse_invalid_returns_zero。
fn parse_everything_date(s: &str) -> i64 { ... }

/// is_dir 启发式（best-effort，v0.1）：`type=="folder"` 优先；否则无扩展名且 size==0 粗判。
fn guess_is_dir(name: &str, size: u64, type_field: &str) -> bool {
    if type_field.eq_ignore_ascii_case("folder") {
        return true;
    }
    !name.contains('.') && size == 0
}
```

> 💡 `chrono` 已在 dd-ext 依赖（任务 1.1）：`date_modified` 规范化统一用它，避免手搓 civil→days 日期算法易错。

#### 任务 2.3：评分排序（第 4 天，**修复类型与归一化**）

原 v3 的 `score()` 有两处硬伤：`SkimMatcherV2::fuzzy_match` 返回 `Option<i64>`，与 `f64` 混算会编译失败；且 skim 分数量级大（非 0~1），与示例 `score: 0.92` 不一致。`date_modified` 也须为 Unix 秒才能比较。修正如下：

```rust
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// 把 skim 原始分（i64，量级数百~数千）归一化到 0~1。
fn norm(skim: i64) -> f64 {
    if skim <= 0 { 0.0 } else { (skim as f64 / 1000.0).min(1.0) }
}

fn score(entry: &FileEntry, query: &str) -> f64 {
    let matcher = SkimMatcherV2::default();
    let name_score = norm(matcher.fuzzy_match(&entry.name, query).unwrap_or(0));
    let path_score = norm(matcher.fuzzy_match(&entry.full_path(), query).unwrap_or(0)) * 0.3;
    // entry.modified 必须是 Unix 秒（任务 2.2 已规范化）；full_path = dir + "\\" + name
    let recency = if entry.modified > now_7days_unix() { 0.1 } else { 0.0 };
    (name_score + path_score + recency).clamp(0.0, 1.0)
}

fn now_7days_unix() -> i64 {
    let seven_days = 7 * 24 * 3600;
    chrono::Utc::now().timestamp() - seven_days
}
```

Everything 查询本身毫秒级返回，30 条评分开销可忽略。归一化阈值（`/1000.0`）为启发式，应在 D4 用真实样例标定。

#### 任务 2.4：Everything 侧的适配工作（第 4 天，**用户操作清单**）

在你本机 Everything 中确认以下设置（一次性，2 分钟）：

1. `工具 → 选项 → HTTP服务器`：✅ 启用 HTTP 服务器，端口 `8080`（Everything 默认端口为 `80`，此处改用 8080 需在 Everything 内显式设置）
2. `HTTP服务器 → 用户名/密码`：留空（仅监听 localhost 时可接受；文档中注明安全建议）
3. 验证：浏览器访问 `http://localhost:8080/?search=test&json=1&count=5`，应返回 JSON
4. 安全：在 `工具 → 选项 → HTTP服务器` 中确认仅绑定 `127.0.0.1`（若 Everything 暴露到 `0.0.0.0` 会被局域网其他机器访问，存在安全隐患）

### ✅ 阶段三：宿主联调与 UI（第 5 天）

在 dd-run 宿主（`crates/dd-gui`）中**无需新增面板或热键**——文件搜索结果随主搜索框自然呈现：

1. **入口**：用户在主面板输入任意词 → 宿主对该 Provider 调 `fallback_commands` → 出现"在文件中搜索 {query}"项（来自 `files.search.query`）。
2. **进入结果页**：选中该项（或输入后直接展开）→ 宿主进 `files.results` 页，调 `get_items(search_text=query)` → 渲染前 30 条文件项。
3. **结果渲染**（egui，沿用既有 `CommandItem` 渲染）：
   - 列表项：`title` = 文件名，`subtitle` = 灰色完整路径，`details` = 大小/修改时间。
   - 键盘：↑↓ 导航，Enter 打开（`invoke` → `host/open_url` `file://`），Esc 返回/关闭面板。
   - 加载态：Everything 响应快（通常 <50ms），感知不明显；`is_loading` 由 `GetItemsResult` 控制。
4. **Everything 不可用**：`get_items` 返回单条引导项（title/subtitle 直接写明开启方法）→ 点击触发 Toast；**不卡死不 panic**。（实现注：v3.2 落地后 `handle_invoke` 对 `files.guide` 尚无专属文案，点击显示通用「未知命令」Toast——见 §7.4 已知边界。）
5. **防抖**：由宿主侧决定何时调 `fallback_commands`/`get_items`（宿主已有聚合与串行化），扩展内不做防抖。

> ✅ **与原 v3 的差异**：原"Ctrl+F 打开独立搜索面板"在当前单一聚合面板宿主模型中不成立，已删除。若未来需要"专注文件搜索"的快捷键，属宿主侧增强（新增热键聚焦文件 Provider），不在 v0.1 扩展范围内。

### ✅ 阶段四：测试与发布（第 6 天）

#### 测试清单（基于本机 Everything）

| 测试项 | 方法 | 预期 |
| :--- | :--- | :--- |
| 基础搜索 | 搜 `readme` | 返回全盘匹配，<100ms |
| 中文搜索 | 搜 `文档`、`项目` | UTF-8 正常，无乱码 |
| 特殊字符 | 搜 `dd-run`、`v0.1` | `pct_encode`（RFC 3986）正确处理，结果准确 |
| Everything 语法透传 | 搜 `ext:rs dm:today` | query 直接透传给 Everything（免费获得高级搜索）✨ |
| Everything 未启动 | 退出 Everything 进程 | `get_items` 返回引导项；点击弹 Toast（当前为通用「未知命令」文案，见 §7.4）；不卡死不 panic |
| 端口被改 | 改成 9090 后重启扩展 | 通过环境变量 `DDRUN_EVERYTHING_URL=http://127.0.0.1:9090` 适配 |
| 空结果 | 搜不存在关键词 | 返回空数组，UI 显示"无结果" |
| limit 截断 | 搜 `e`（海量结果） | 只返回 30 条，评分排序稳定 |
| `file://` 打开 | 回车打开某文件 | 宿主 `host/open_url` 用系统默认程序打开（实现：`crates/dd-gui/src/app/host_actions.rs` 经 `webbrowser::open`；`file://` 真机实测属 D6 / §7.3 验收，若不支持则宿主/扩展侧 `cmd /c start` 兜底） |
| 长时间运行 | 扩展连续响应 100 次请求 | 无内存泄漏、无句柄泄漏 |
| `date_modified` 解析 | 用本机真实响应样例 | 单测覆盖；异常格式不 panic，置 0 |

> 💡 **意外收获**：由于 query 直接透传给 Everything，**Everything 的全部搜索语法（`ext:`、`dm:`、`size:`、`path:`、通配符、正则）在 v0.1 就自动可用**——建议在用户文档中列出常用语法速查表。

#### 单元测试重点（随扩展代码提交）

- `parse_everything_date`：本机真实响应样例（多格式）+ 异常格式返回 0（`date_parse_known_formats` / `date_parse_invalid_returns_zero`）。
- `score`：归一化后落在 0~1；`搜 dd` 时 `dd-run` 排前；`name` 权重高于 `path`；近因加分（`score_normalizes_and_ranks_name_over_path` / `score_recency_bonus_for_recent`）。
- `guess_is_dir`：`type=="folder"` 优先；无扩展名且 size==0 的启发式边界。
- `to_command_item`：`files.open.<u64>` id 经 `register_path`/`lookup_path` **round-trip**（`path_index_roundtrip`）；v3.2 起 id 不内嵌路径，已无 `parse_open_id`。
- `pct_encode`：保留 URL 安全字节、编码特殊字符与中文（`pct_encode_keeps_unreserved_and_encodes_rest`）。
- `get_file_items`：空查询→`files.hint`、Everything 不可用→`files.guide`（`get_file_items_empty_query_returns_hint` / `everything_unavailable_returns_guide`）。

#### 发布 v0.1.0（Windows-only）

1. `bash tools/package.sh`（gnu 工具链）产出 `dist/dd-run-<版本>.exe`（5 内置扩展内嵌）+ 归集 `dist/extensions.d/`（file-search **sidecar**）——免安装绿色包布局。
2. `docs/search.md`（用户文档）：
   - 前置条件：Everything 安装 + 开启 HTTP 服务（图文教程，含仅绑定 127.0.0.1 安全提示）
   - 常用搜索语法速查表（`ext:rs`、`dm:today`、`path:dd-run` 等）
   - 配置项说明（环境变量 `DDRUN_EVERYTHING_URL`，默认 `127.0.0.1:8080`，端口非默认/换主机的改法）
   - 明确标注：**v0.1 仅支持 Windows（依赖 Everything）**；跨平台需等 v0.2 fd Provider
3. CHANGELOG 注明："v0.1 文件搜索依赖 Everything（Windows）；fd 兜底 Provider 计划于 v0.2 提供"
4. 真机走查（M7 批次 7.5 收尾）：解压绿色 zip 直接运行 → `f ` 前缀可进文件搜索、升级覆盖 → tag → GitHub Release。

---

## 四、v0.1 → v0.2 的衔接（fd 兜底，跨平台）

本期为 fd 预留的接口，v0.2 接入时只需：

1. 在 `get_file_items` 内增加分支：`Everything 不可用 → 调 fd`（新增 `src/fetcher.rs` 自动下载 fd，约 150~200 行）；或新增 `files.search.fd` 入口项。
2. 清单 `platforms` 由 `["windows"]` 扩展为 `["windows","macos","linux"]`；fd 为对应平台二进制。
3. `capabilities` 不变（`host/open_url` 同样适用于 fd 结果的本地文件路径）。
4. 协议/运行时**零改动**——这是先定 provider 模型的价值。

**预计 v0.2 增量工期：2~3 天**。v0.2 起再谈"三平台产物"。

---

## 五、风险与预案（Everything 优先版，已更新）

| 风险 | 概率 | 预案 |
| :--- | :--- | :--- |
| 用户未开启 Everything HTTP 服务 | 高（对公开发布而言） | `get_items` 返回引导项 + `host/show_status` Toast；v0.2 用 fd 彻底解决 |
| `date_modified` 格式随 Everything 版本变化 | 中 | D4 用本机真实响应锁定解析；异常置 0 不报错（单测覆盖） |
| HTTP 服务被防火墙/杀软拦截 | 中 | 探测超时 800ms 快速失败，引导项提示检查端口 |
| HTTP 服务暴露局域网的安全隐患 | 低（localhost默认） | 文档安全提示（仅绑 127.0.0.1）；v0.2+ 可支持 Basic Auth |
| Everything 查询语法特殊字符与 URL 编码冲突 | 低 | `pct_encode`（扩展内手写 RFC 3986）统一处理 + 特殊字符用例测试（单测已含 `a+b=c`、中文） |
| **`host/open_url` 对 `file://` 行为不确定** | 中 | **v0.1 实测 `webbrowser::open("file:///...")`**（宿主 `crates/dd-gui/src/app/host_actions.rs`）；若宿主侧不支持，预案为宿主侧改用 `cmd /c start` 相对可执行打开（或扩展侧兜底），仍经现有 `invoke` 链路 |
| **同步 HTTP 阻塞主循环** | 低 | Everything 本地 <50ms，远低于 `get_items` 2000ms 超时；宿主侧已串行化保护 |

---
> **口径说明**：D0–D6 为 v3.1 计划期验收清单（v0.1 Release 验收门）；v3.2 落地后核心行为由 §7.3 验收清单承接（部分已 [x]）。D 清单中与代码已实现不一致处（引导项点击文案、超时 3s vs 2s）已在 §7.4 已知边界记档，不重复标注。

## 六、每日验收清单（命令已对齐真实协议信封）

- [ ] **D0 结束**（前置）：`dd_ext` 运行时 `get_items` 支持 `PageHandler`；既有 5 扩展 `pages: None` 行为不变（仍 `-32005`），单测覆盖。
- [ ] **D2 结束**：`echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run -p dd-ext --bin dd-ext-search` 返回正确 `provider`（id=com.ddrun.filesearch, has_fallback=true, capabilities 含 host/open_url）。
- [ ] **D3 结束**：扩展 `get_items`（`pages`）能从本机 Everything 拿到 JSON 结果并映射为 `CommandItem`。
- [ ] **D4 结束**：`date_modified` 等字段解析正确（单测）；搜 `dd` 时 `dd-run` 排前；退出 Everything 后 `get_items` 返回引导项不挂起；`score` 归一化单测通过。
- [ ] **D5 结束**：主面板输入即出"在文件中搜索 {query}" → 回车进结果页 → ↑↓ 导航 → 回车用系统程序打开文件 → Esc 返回。
- [ ] **D6 结束**：测试清单全过（含 `file://` 打开实测）；用户文档发布；GitHub Release v0.1.0（Windows）。

---

## 七、便捷 + 速度优化（v3.2：f 前缀自动进页 · 已落地）

> **用户诉求**（2026-09-07）：文件搜索是高频操作，要"更便捷、更快速"。
> **决策（grill-me 收敛）**：采用**直达前缀 + 自动进页**方案——根视图输入以 `f ` 开头即由宿主自动进文件结果页并回填剩余查询，省去"fallback 模板 → 选中 → 进页"的第二次 Enter；扩展侧再做速度打磨。

### 7.1 便捷：根视图 `f ` 前缀自动进页（宿主侧，零协议改动）

- **触发**：根查询以 `f `（`FILE_SEARCH_PREFIX`）开头，且 `com.ddrun.filesearch` 已加载且未禁用 → 每帧 `ui()` 经 `maybe_drill_file_search()` 自动 `open_page` 到 `files.results`，并把 `f ` 之后的文本作为 `search_text` 回填到结果页搜索框。
- **防抖/防重**：仅栈顶为 Root 时触发；同一查询已进页（`file_drill` 命中）不再重复进页；查询不再以 `f ` 开头（如清空）即清除 `file_drill`。Esc 返回 Root 时**不**重置 `file_drill`（否则会立即重进页、抵消 Esc），由上述 else 分支在"前缀消失"时自然复位。
- **协议零改动**：完全复用 `get_items(files.results, search_text)`；新增的只是宿主一处纯函数 `file_search_drill_target` + `PaletteApp` 两个方法（`maybe_drill_file_search` / `file_search_present`）。
- **既有入口保留**：顶层「文件搜索」入口 + `fallback_commands` 模板「在文件中搜索 {query}」仍可用（发现性与非前缀路径）。

### 7.2 速度：扩展侧打磨（零宿主改动即可生效）

| 打磨点 | 做法 | 预期 |
| --- | --- | --- |
| availability 探测缓存 | `AVAIL` TTL 缓存（默认 3s），窗口内跳过重复 TCP 探测 | 连续按键不每次探测 Everything |
| 超时收紧 | 探测 800ms；搜索 3s（Everything 本地通常 <50ms）；⚠️ 宿主 `get_items` 超时仅 2000ms，见 §7.4 | 异常时快速失败，不挂起 UI |
| 结果数自适应 | 默认取前 30 条，skim 评分排序稳定 | 海量结果只取前 N，列表瞬时填充 |
| 查询直透 | Everything 全部语法（`ext:`/`dm:`/`path:`/通配符/正则）原样透传 | 扩展侧零解析、零损耗 |
| 依赖最小 | HTTP 用标准库 `TcpStream`（HTTP/1.1 `Connection: close`）；评分用 `fuzzy-matcher`；时间用 `chrono`（手搓 civil→days 易错，改用成熟库） | 相对 dd-ext 基线**仅新增** fuzzy-matcher/chrono（anyhow/serde/serde_json 复用既有基线），不引入 reqwest/urlencoding 等重依赖 |

### 7.3 验收（acceptance）

- [x] **便捷**：根视图输入 `f report` → 无需选中、无需第二次 Enter，即进入文件结果页并展示 Everything 对 `report` 的前 30 条结果；搜索框保留显示 `report`（`poll_page` 落地时回填 `file_drill_armed` 标记的查询，已落地消除"结果回来后框被清空"的边角，dev 构建通过）。真机核对显示与二次输入行为。
- [ ] **防重**：对 `f report` 连续多帧 `ui()` 不重复 `open_page`（仅一次进页）；按 Esc 返回 Root 后框内仍含 `f report` 不会立即重进页；清空查询后 `file_drill` 复位，再次输入 `f x` 可正常进页。
- [ ] **速度**：Everything 在线时，从输入完成到结果填充主观 <200ms（本地回环）；`everything_available()` 在 TTL 内对同一状态不再发起新探测。
- [ ] **健壮**：关闭 Everything HTTP 服务 → `f x` 进页后返回「未检测到 Everything」引导项，不挂起、不 panic；扩展进程不崩溃（运行时熔断不误触发）。
- [x] **回归**：既有 5 扩展 `pages: None` 行为不变（仍 `-32005`）；`cargo test -p dd-ext` 全过（含 `get_items` PageHandler、日期解析、评分归一化、路径索引 round-trip）。**2026-09-08 实证**：`cargo test --workspace` **274 passed / 0 failed**。
- [x] **单测新增**：`search.rs` 覆盖 `pct_encode` / 日期多格式解析 / `score` 归一化与近因加分 / 路径索引 round-trip / `get_file_items` 空查询→hint、Everything 不可用→guide。**2026-09-08 补齐**：另增 `guess_is_dir` 边界、`handle_invoke` 分发与 Toast、`to_command_item` 与三个占位项 shape、`parse_response`（fixture 离线）、路径索引容量淘汰、`spec`/commands 契约——`dd-ext-search` 单测由 9 → **21**。

### 7.4 已知边界（v3.2 记录，部分已在本轮解决）

- **初始进页搜索框回填（已解决）**：`maybe_drill_file_search` 进页后标记 `file_drill_armed`，`poll_page` 结果落地时把 `f ` 之后的查询写回搜索框（`dd-gui/src/app/page.rs` + `mod.rs`）。仅文件结果页、且 `file_drill` 命中时生效，落地即消耗；用户先 Esc 离开再手动进页不会误回填旧查询（else 分支也清 `file_drill_armed`）。
- **结果页不随页内二次输入实时重拉（仍属 v3.3）**：当前宿主嵌套页仅在 `open_page` / `items_changed` 时取数，页内搜索框二次输入只做本地过滤（受前 30 条限制）。彻底"边打边搜"需宿主在嵌套页 query 变化时重发 `get_items`——属宿主改动，留待 v3.3 评估。
- **`f ` 前缀会劫持根视图字面查询**：用户若想在主面板搜字面 `f report` 文本，会被自动进文件页。属设计取舍（便捷前缀），如需可改为可配置前缀或仅在空 Root 时触发。
- **扩展搜索超时（3s）被宿主 `get_items` 超时（2000ms）截断（v3.2 记录，未处理）**：宿主 `TIMEOUT_GET_ITEMS=2000ms`（`crates/dd-host/src/process.rs`，协议 §10）固定不变；Everything 若超过 2s 无响应，宿主先判 `-32001 extension_timeout` 并丢弃迟到响应，表现为超时错误而非引导项——§7.2「搜索 3s 兜底」实际到不了。如需引导项兜底，v3.3 把 `search.rs` 的 3000ms 对齐到 ≤2000ms（宿主侧不改）。
- **引导项/占位项点击文案未打磨（v3.2 记录，未处理）**：`files.hint` / `files.guide` / `files.error` / `files.search` 的 `command` 均为 `Invoke`，但 `handle_invoke` 仅分发 `files.open.<u64>`，其余落回通用「未知命令」Toast——列表内 title/subtitle 已把信息讲清，点击提示可后续打磨（v3.3）。

---

## 八、AI Agent 可执行最小任务分解（按依赖优先级排序）

> **目的**：把 §二/§三/§六 的"人读计划"重写为**原子、可独立提交、可机器验收**的任务序列，供 AI agent 顺序（或同层并行）执行。每个任务都是「打开这些文件 → 做这些确定性改动 → 跑这条命令验收」的最小单元，不夹带歧义。
> **落地基线**：§7.3 已 `[x]` 的项（便捷进页、f 前缀回填、`file://` 打开实测）视为已完成。AI agent 执行时将其作为**回归验收基线**——跳过实现、仅做核对；其余任务从零落地。
> **优先级规则**：优先级 = 依赖拓扑层（**P0 最底层前置 → P7 最高层收尾**）。同层任务互不依赖、可由不同 agent 并行；跨层必须**先完成低层全部任务**再开始高层。任务 ID 全局唯一（`T-01`…`T-25`），`依赖` 列列出前置任务 ID。每个任务完成后跑对应单测再提交，不符合验收不进入下一层。

### 8.1 依赖拓扑图（优先级即层号）

```mermaid
graph TD
    T01[T01 运行时PageHandler] --> T02[T02 运行时单测]
    T01 --> T04[T04 search.rs骨架]
    T03[T03 Cargo bin+依赖] --> T04
    T03 --> T05[T05 清单json]
    T04 --> T06[T06 探测]
    T04 --> T07[T07 http_get]
    T04 --> T08[T08 pct_encode]
    T04 --> T13[T13 路径索引]
    T04 --> T26[T26 契约冒烟]
    T07 --> T09[T09 FileEntry/search解析]
    T08 --> T09
    T09 --> T10[T10 日期解析]
    T09 --> T11[T11 guess_is_dir]
    T09 --> T12[T12 评分]
    T10 --> T12
    T09 --> T16[T16 to_command_item]
    T06 --> T14[T14 get_file_items]
    T09 --> T14
    T10 --> T14
    T11 --> T14
    T12 --> T14
    T13 --> T14
    T13 --> T15[T15 handle_invoke]
    T14 --> T17[T17 编译+测试]
    T15 --> T17
    T16 --> T17
    T04 --> T18[T18 宿主f前缀进页]
    T01 --> T18
    T18 --> T19[T19 poll_page回填]
    T19 --> T27[T27 防重与Esc语义]
    T15 --> T20[T20 host/open_url file://]
    T17 --> T21[T21 package.sh归集]
    T05 --> T21
    T17 --> T22[T22 docs/search.md]
    T17 --> T23[T23 CHANGELOG]
    T21 --> T24[T24 发布构建+走查]
    T20 --> T24
    T22 --> T24
    T23 --> T24
    T17 --> T28[T28 性能与健壮回归]
    T06 --> T28
    T19 --> T28
    T27 --> T24
    T28 --> T24
    T24 --> T25[T25 GitHub Release]
```

### 8.2 任务清单（按优先级层 P0 → P7）

#### P0 · 前置（运行时补齐 `get_items`）— 第 1 天上午

| ID | 任务 | 目标文件 | 具体动作（AI 可执行） | 验收 | 依赖 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **T-01** | 运行时支持 `PageHandler` | `crates/dd-ext/src/lib.rs` | 新增 `pub type PageHandler = fn(&GetItemsParams) -> GetItemsResult;`；为 `ExtensionSpec` 增加字段 `pub pages: Option<PageHandler>`；在 `serve_line` 的 `"get_items"` 分支：若 `pages: Some(h)` 调 `h(params)` 返回 `GetItemsResult`（含 `has_more_items`/`is_loading`），否则维持原 `-32005 Page not found`。**不修改 `docs/protocol.md`**。 | 既有 5 扩展 `pages: None` 行为不变（仍 `-32005`）。 | — |
| **T-02** | 运行时 `PageHandler` 单测 | `crates/dd-ext/src/lib.rs`（`#[cfg(test)]`） | 新增 `page_handler_none_still_32005`（5 扩展之一构造 spec 调 `get_items` 期望 `-32005`）；`page_handler_returns_items`（注册临时 `PageHandler` 返回非空 `GetItemsResult`）。 | `cargo test -p dd-ext` 新增测试通过。 | T-01 |

#### P1 · 扩展骨架（bin + 清单）— 第 1~2 天

| ID | 任务 | 目标文件 | 具体动作（AI 可执行） | 验收 | 依赖 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **T-03** | 注册 bin 与依赖 | `crates/dd-ext/Cargo.toml` | `[[bin]]` 区追加 `name = "dd-ext-search"` / `path = "src/bin/search.rs"`；`[dependencies]` 确保含 `fuzzy-matcher = "0.3"` 与 `chrono = "0.4"`（不重复添加已存在的 `anyhow`/`serde`/`serde_json`）。 | `cargo metadata` 解析无误；与 §三 1.1 清单一致。 | — |
| **T-04** | `search.rs` 骨架 | `crates/dd-ext/src/bin/search.rs`（新建） | 照 §三 1.3 写 `spec()`（`id=com.ddrun.filesearch`、`frozen=false`、`has_fallback=true`、`capabilities=[host/open_url, host/show_status]`、`pages=Some(get_file_items)`、`log_tag=dd-ext-filesearch`）；`fallback_commands()` 返回 `CommandRef::Page { page_id: "files.results" }` 入口项（`id=files.search.query`）；`top_level_commands()` 返回顶层文件搜索说明项（`§7.1` 保留的发现性入口，不得省略）；`main(){ run(&spec()) }`；`get_file_items`/`handle_invoke` 先放可编译占位（如返回 hint 项）。**所有用户可见文案必须走 `dd_ext::i18n::tr(中文, English)` 双语**，禁止硬编码单语字符串。 | `cargo build -p dd-ext --bin dd-ext-search` 编译通过；`spec()` 字段与 §三 1.3 逐字段一致。 | T-01, T-03 |
| **T-05** | 扩展清单 | `examples/extensions.d/com.ddrun.filesearch.json`（新建） | 照 `docs/manifest-schema.md` 写：`id=com.ddrun.filesearch`、`display_name`、`entry.command` 指向 `dd-ext-search.exe`（`${EXT_DIR}` 相对定位）、`platforms=["windows"]`、`capabilities=["host/open_url","host/show_status"]`。 | `python -m json.tool` 合法；字段与 manifest-schema 一致。 | T-03 |
| **T-26** | NDJSON 契约冒烟 | `crates/dd-ext/src/bin/search.rs`、`examples/extensions.d/` | 以 stdin/stdout 管道驱动扩展，逐个校验协议 v1.0 契约（**不新增字段**）：`initialize` 返回 `provider.id=com.ddrun.filesearch` / `has_fallback=true` / `capabilities` 含 `host/open_url`；`fallback_commands` 返回含 `Page{files.results}` 的入口项；`get_items` 返回 `GetItemsResult`（空 query→hint、异常→guide）。命令形如 `echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \| cargo run -p dd-ext --bin dd-ext-search`。 | 三条契约响应字段与 §三 1.2 表逐项一致；`initialize` 的 `id`/`has_fallback` 断言存在（对应 §六 **D2** 验收）。 | T-04, T-05 |

#### P2 · Provider 基础（探测 / HTTP / 编码 / 解析）— 第 3 天

| ID | 任务 | 目标文件 | 具体动作（AI 可执行） | 验收 | 依赖 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **T-06** | Everything 可用探测 | `crates/dd-ext/src/bin/search.rs` | 实现 `everything_base()`（默认 `127.0.0.1:8080`，环境变量 `DDRUN_EVERYTHING_URL` 覆盖）；`everything_available()`（`TcpStream::connect` 探测 `GET /?json=1&count=1`，读/写超时 800ms，3s TTL 缓存跳过重复探测）。**探测目标必须可注入**（函数接受 base 地址参数或读 env），以便单测用本地临时 `TcpListener`（`127.0.0.1:0`）模拟 up/down。 | 单测 `everything_available_true_when_up` / `_false_when_down`：**用本地临时 TcpListener 模拟**，不依赖真实 Everything（规则见 §8.4.2）。 | T-04 |
| **T-07** | 同步 `http_get` | `crates/dd-ext/src/bin/search.rs` | 实现 `http_get(rel)`：标准库 `TcpStream` 发 HTTP/1.1 GET，`Connection: close`，读超时 3s / 写超时 800ms，body 取自首个 `\r\n\r\n` 之后；返回 `anyhow::Result<String>`。连接地址同样需可注入（同 T-06）。 | 单测 `http_get_reads_body_after_crlf`：起本地临时 `TcpListener` 返回固定响应，断言 body 正确截取；另覆盖**连接失败返回 Err 而非 panic**。不依赖真实 Everything。 | T-04 |
| **T-08** | `pct_encode` | `crates/dd-ext/src/bin/search.rs` | 实现 `pct_encode(input)`：保留 `-_.~` 与字母数字，其余按 RFC 3986 大写 `%XX` 编码（含 UTF-8 字节）。 | 单测 `pct_encode_keeps_unreserved_and_encodes_rest`（覆盖中文、`a+b=c`、空格）。 | T-04 |
| **T-09** | `FileEntry` + `search` 解析 | `crates/dd-ext/src/bin/search.rs` | 定义 `RawEntry`/`FileEntry`（`name,dir,size,is_dir,modified`）与 `EvResponse`；实现 `search(q, limit)`：拼 `/?search={pct_encode(q)}&json=1&count={limit}&path_column=1&size_column=1&date_modified_column=1` → `http_get` → 解析 → 映射 `Vec<FileEntry>`（`dir + "\\" + name` 为 `full_path`）。**必须把解析抽为纯函数 `parse_response(&str) -> anyhow::Result<Vec<FileEntry>>`**，与传输解耦以保证可离线单测（§8.4.2 第 2 条）。 | 单测 `search_parses_everything_response` / `parse_response_malformed_returns_err`：以**本机真实响应样例字符串作 fixture**（存入 `#[cfg(test)]` 常量），断言字段映射与 `full_path` 拼接；畸形 JSON 返回 Err 不 panic。 | T-07, T-08 |

#### P3 · 字段映射 / 评分 / 路径索引 — 第 4 天

| ID | 任务 | 目标文件 | 具体动作（AI 可执行） | 验收 | 依赖 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **T-10** | `date_modified` 规范化 | `crates/dd-ext/src/bin/search.rs` | 实现 `parse_everything_date(s)`：按空格切日期/时间 → 按 `-`/`/` 切年月日 → 按 `:` 切时分秒（截断 `.fff`）→ `chrono` 构造 UTC 秒；失败返回 `0` 不 panic。 | 单测 `date_parse_known_formats`（多格式）/ `date_parse_invalid_returns_zero`。 | T-09 |
| **T-11** | `guess_is_dir` | `crates/dd-ext/src/bin/search.rs` | 实现 `guess_is_dir(name, size, type_field)`：`type=="folder"` 优先；否则无扩展名且 `size==0`。 | 单测 `guess_is_dir_folder_priority` / `_heuristic_boundary`。 | T-09 |
| **T-12** | 评分排序 | `crates/dd-ext/src/bin/search.rs` | 实现 `norm(skim)`（`/1000.0` 收敛 0~1）、`score(entry, query)`（`name` + `path*0.3` + 近因 `0.1`）、`now_7days_unix()`（`chrono` UTC - 7d）、`score_and_sort()`。 | 单测 `score_normalizes_and_ranks_name_over_path` / `score_recency_bonus_for_recent`。 | T-09, T-10 |
| **T-13** | 进程内路径索引 | `crates/dd-ext/src/bin/search.rs` | 实现 `register_path(path)->u64` / `lookup_path(u64)->Option<PathBuf>`（线程安全）；`id` 用 `files.open.<u64>`，**不内嵌路径**。**必须带容量上限与淘汰**（如环形/LRU，或每次 `get_items` 重建本页索引），否则扩展长驻进程内索引会随调用次数无限增长，与「连续 100 次请求无内存泄漏」（§三测试清单）直接冲突。 | 单测 `path_index_roundtrip`（register → lookup 一致）；`path_index_evicts_beyond_capacity`（超容量后旧项被淘汰、不无限增长）。 | T-04 |

#### P4 · 组装 `get_items` / `invoke` / 映射 — 第 4~5 天

| ID | 任务 | 目标文件 | 具体动作（AI 可执行） | 验收 | 依赖 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **T-14** | `get_file_items` | `crates/dd-ext/src/bin/search.rs` | 实现 `get_file_items(params)`：空 query → `hint_item()`；`!everything_available()` → `guide_item()`；否则 `search` → `score_and_sort` → `to_command_item`；错误 → `error_item`。 | 单测 `get_file_items_empty_query_returns_hint` / `everything_unavailable_returns_guide`。 | T-06, T-09, T-10, T-11, T-12, T-13 |
| **T-15** | `handle_invoke` | `crates/dd-ext/src/bin/search.rs` | 实现 `handle_invoke(params)`：`files.open.<u64>` → `lookup_path` → `file:///` + 路径（`\`→`/`）→ `Effect::HostRequest { method:"host/open_url", params: json!({"url":url}) }` + `CommandResult::Dismiss`；其余 → `ShowToast` 未知命令。 | 单测 `handle_invoke_opens_file_via_host_request` / `_unknown_command_toast`。 | T-13 |
| **T-16** | `to_command_item` + 静态项 | `crates/dd-ext/src/bin/search.rs` | 实现 `to_command_item(entry)`：输出 `CommandItem { id:"files.open.<n>", title:name, subtitle:full_path, section:"文件", icon glyph \uE7C3, command: invoke, details: 大小/修改 }`；`hint_item()`/`guide_item()`/`error_item()` 三个静态项（§7.4 所列 `files.search.query` 属 fallback 入口项，非本任务的页内占位项）。 | 单测 `to_command_item_maps_fields`（含 `id` 形如 `files.open.<n>`、`details` 含大小与修改时间）；`hint_guide_error_items_shape`（三项 `command` 均为 `Invoke`、文案双语走 `tr()`）。 | T-09, T-10, T-11 |

#### P5 · 编译 + 全量测试 — 第 5 天

| ID | 任务 | 目标文件 | 具体动作（AI 可执行） | 验收 | 依赖 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **T-17** | 编译 + 全量门禁 | 仓库根 | 运行完整门禁（§8.4.3 命令集）：`cargo fmt --all --check` → `cargo clippy --workspace --all-targets` → `export APPDATA=...` 后 `cargo test --workspace` → `cargo +stable-x86_64-pc-windows-gnu build -p dd-ext --bin dd-ext-search`；修复全部错误与警告至全绿。 | **三项基线全绿**：fmt exit 0、clippy **零 warning**、`cargo test --workspace` **0 failed**（含 PageHandler、日期、评分、路径 round-trip、`get_file_items`）；扩展 build exit 0。任一不达标均视为 T-17 未完成。 | T-14, T-15, T-16 |

#### P6 · 宿主联调（f 前缀 / 打开）— 第 5 天

| ID | 任务 | 目标文件 | 具体动作（AI 可执行） | 验收 | 依赖 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **T-18** | 宿主 `f ` 前缀自动进页 | `crates/dd-gui/src/app/mod.rs`、`page.rs` | 新增常量 `FILE_SEARCH_PREFIX="f "`、`file_search_drill_target()`（返回 provider id + `files.results`）、`PaletteApp` 方法 `maybe_drill_file_search()`（栈顶 Root 且查询以前缀开头且 provider 已加载未禁用 → `open_page` 并标记 `file_drill`）、`file_search_present()`；状态字段 `file_drill`/`file_drill_armed`。 | §7.3 便捷项已 `[x]`；以该验收为回归基线核对（输入 `f report` 自动进页、回填 query）。 | T-04, T-01 |
| **T-19** | `poll_page` 回填查询 | `crates/dd-gui/src/app/page.rs`、`mod.rs` | 在 `poll_page` 结果落地时，若 `file_drill_armed` 命中则将 `f ` 之后的查询写回搜索框并消耗标记；Esc 返回 Root 的 else 分支清 `file_drill_armed`。 | §7.3「结果回来后框被清空」边角已落地；回归核对。 | T-18 |
| **T-20** | `host/open_url` 开 `file://` | `crates/dd-gui/src/app/host_actions.rs` | 确认 `host/open_url` 经 `webbrowser::open` 处理 `file://`；若实测不支持，改为 `cmd /c start` 兜底；保持经现有 `invoke` 链路。 | §7.3「`file://` 打开实测」；真机回车打开文件。 | T-15 |
| **T-27** | 防重与 Esc 语义 | `crates/dd-gui/src/app/mod.rs`、`page.rs` | 落地并验证 §7.3「防重」项：`f report` 连续多帧 `ui()` 仅 `open_page` **一次**；Esc 返回 Root 后框内仍含 `f report` 时**不**立即重进页；清空查询后 `file_drill` 复位，再次输入 `f x` 可正常进页。 | 单测覆盖 `file_search_drill_target` 前缀判定与复位；真机按 §8.4.5 第 11 项勾选。 | T-18, T-19 |
| **T-28** | 性能与健壮回归 | `crates/dd-ext/src/bin/search.rs`、宿主 | 落地并验证 §7.3「速度 / 健壮 / 回归」三项：① 速度——Everything 在线时输入完成到结果填充主观 <200ms，且 `everything_available()` 在 TTL 内不重复发起探测；② 健壮——关闭 Everything HTTP 后 `f x` 进页返回引导项，不挂起、不 panic、扩展进程不崩溃（运行时熔断不误触发）；③ 回归——既有 5 扩展 `pages: None` 仍返回 `-32005`，`cargo test -p dd-ext` 全过。 | §8.4.5 第 5/10/12 项真机勾选；`cargo test --workspace` 0 failed。 | T-17, T-19, T-06 |

#### P7 · 打包 / 文档 / 发布 — 第 6 天

| ID | 任务 | 目标文件 | 具体动作（AI 可执行） | 验收 | 依赖 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **T-21** | `package.sh` 归集 | `tools/package.sh`、`dist/` | 确认 `package.sh` 把 `dd-ext-search.exe` 与 `examples/extensions.d/com.ddrun.filesearch.json` 归集进 `dist/extensions.d/`；跑 `bash tools/package.sh` 验证产出。 | `dist/extensions.d/` 含 `dd-ext-search.exe` + `com.ddrun.filesearch.json`；宿主扫描同目录 `extensions.d` 能加载。 | T-17, T-05 |
| **T-22** | 用户文档 | `docs/search.md`（新建） | 写用户文档：Everything 安装 + 开 HTTP（图）、常用语法速查（`ext:`/`dm:`/`path:`）、配置项 `DDRUN_EVERYTHING_URL`（默认 `127.0.0.1:8080`、仅绑 `127.0.0.1` 安全提示）、明确 v0.1 仅 Windows。 | 文档可被用户照做完成 Everything 配置并搜到结果。 | T-17 |
| **T-23** | CHANGELOG | `CHANGELOG.md` | 追加 v0.1 条目："文件搜索依赖 Everything（Windows）；fd 兜底 Provider 计划 v0.2"。 | CHANGELOG 含该条目。 | T-17 |
| **T-24** | 发布构建 + 走查 | `dist/`、仓库 | `bash tools/package.sh` 产出 `dist/dd-run-<ver>.exe` + `dist/extensions.d/`；真机走查：解压直接运行 → `f ` 进文件搜索、`file://` 打开、Esc 返回、升级覆盖。 | **§8.4.5 真机验收清单 12 项逐项勾选全过**（无一项未勾即不得进入 T-25）。 | T-21, T-20, T-22, T-23, T-27, T-28 |
| **T-25** | GitHub Release | GitHub | tag + 推送 + 创建 Release v0.1.0（Windows 绿色包）。 | Release 资产含 dist 产物，下载解压可用。 | T-24 |

### 8.3 执行纪律

1. **同层并行、跨层串行**：P0→P1→…→P7 严格递进；P2 内 T-06/T-07/T-08 互不依赖可并行，但都需先完成 P1 的 T-04；T-09 必须等 T-07+T-08。
2. **三层门禁**：每个任务落地的同时提交对应单测；`cargo fmt --all --check` + `cargo clippy --workspace --all-targets`（零 warning）+ `cargo test --workspace`（0 failed）三项未全绿，不得进入 P5/P6（命令见 §8.4.3）。
3. **零协议改动**：T-01 仅补齐运行时 `get_items`，不得向 `docs/protocol.md` 追加方法或字段（协议 v1.0 冻结）；契约测试断言的必须是 v1.0 已定义字段。
4. **最小依赖**：仅新增 `fuzzy-matcher`/`chrono`；不引入 `reqwest`/`urlencoding`/`tokio`/`async-trait`（同步模型）。红线见 §8.4.2 第 8 条。
5. **回归基线**：§7.3 已 `[x]` 的 P6 任务（T-18/T-19/T-20）以现有实现为基线，AI 执行时优先核对而非重写。
6. **边界不越界**：§7.4 记录的 v3.3 遗留项（搜索超时 3s 与宿主 2000ms 对齐、引导项点击文案打磨、结果页二次输入实时重拉）**不在本分解范围**，除非任务显式要求，否则不得顺手实现——避免与 v3.3 规划冲突。
7. **验收唯一口径**：所有任务的「验收」列以 §8.4 为准；出现分歧时以 §8.4.4 量化红线与 §8.4.6 DoD 判定。

### 8.4 验证标准与测试规则（§八 判定基准）

> 本节是 §八 的**唯一判定基准**：所有任务的「验收」列均引用至此。AI agent 在判定任一任务完成前，必须能通过本节对应层级的门禁。

#### 8.4.1 测试分层（L1–L4）

| 层 | 类型 | 覆盖范围 | 依赖 Everything | 运行方式 |
| :--- | :--- | :--- | :--- | :--- |
| **L1** | 纯函数单测 | `pct_encode` / `parse_everything_date` / `guess_is_dir` / `norm` / `score` / `register_path`+`lookup_path` / `parse_response` / `to_command_item` / `handle_invoke` / `file_search_drill_target` | ❌ 否（**必须可离线运行**） | `cargo test --workspace` |
| **L2** | 契约测试（NDJSON） | 以 stdin/stdout 管道驱动扩展进程，校验 `initialize` / `top_level_commands` / `fallback_commands` / `get_items` / `invoke` 的请求-响应 | ❌ 否（可注入假响应） | `echo '<json>' \| cargo run -p dd-ext --bin dd-ext-search` |
| **L3** | 集成测试（真实 Everything） | 端到端：真实 HTTP 探测 → 搜索 → 评分 → 映射 | ✅ 是 | `#[ignore]` + `cargo test -- --ignored` |
| **L4** | 真机验收 | GUI 行为：`f ` 进页、↑↓/Enter/Esc、文件打开、长跑、防重 | ✅ 是 | 真机走查（§8.4.5 清单） |

**硬规则**：L1/L2 必须在**无 Everything、无 GUI** 的干净 CI 上全绿；L3/L4 允许被跳过，但被跳过的用例必须在 §8.4.5 清单中被人工执行并勾选，否则不得进入 T-24/T-25。

#### 8.4.2 测试规则（硬性，违反即视为任务未完成）

1. **可离线性**：L1 单测**禁止**发起真实 TCP 连接、禁止依赖 Everything 进程、禁止依赖真实文件系统结果。需要网络语义时用**本地临时 `TcpListener`**（绑定 `127.0.0.1:0`）或**注入 fixture 字符串**。
2. **解析与传输解耦**：`search()` 必须拆为 `http_get()`（传输）+ `parse_response(&str)`（纯解析）；解析以 `&str` 入参单测，fixture 取自本机 Everything 真实响应样例（存 `#[cfg(test)]` 常量或 `tests/fixtures/`）。
3. **命名规范**：`<被测函数>_<场景>_<预期>`，如 `date_parse_invalid_returns_zero`、`pct_encode_keeps_unreserved_and_encodes_rest`。§三「单元测试重点」已列出的名字**保持不变，不得重命名**。
4. **L3 标记**：依赖真实 Everything 的测试一律 `#[ignore]`，注释写明 `// L3: requires local Everything HTTP server; run with --ignored`。
5. **禁 panic**：`parse_everything_date` 对异常输入必须返回 `0`，不得 `unwrap()`/`expect()`/`panic!`；单测须覆盖空串、缺字段、FILETIME 数值串、超长输入四类。
6. **确定性**：评分/排序单测须给出确定输入与确定期望顺序；recency 场景用**固定 `modified` 时间戳**构造，不得依赖"当前真实文件"的修改时间。
7. **不改协议**：L2 契约测试断言的是**协议 v1.0 已定义字段**；出现新增字段一律视为失败。
8. **依赖红线**：测试中出现 `reqwest` / `tokio` / `async-trait` / `urlencoding` 即视为违规（同步模型 + 最小依赖）。

#### 8.4.3 门禁命令（提交前必跑，顺序执行）

```bash
# 1) 格式（必须 exit 0）
cargo fmt --all --check

# 2) 静态检查（workspace 全 target，必须零 warning）
cargo clippy --workspace --all-targets

# 3) 测试：Git Bash 下 APPDATA 为空会导致 dd-gui 图标缓存测试假阳性失败，必须显式导出
export APPDATA='C:\Users\y7398\AppData\Roaming'
cargo test --workspace

# 4) 扩展构建（self-contained 目录必须前置到 PATH，否则 dlltool not found）
export PATH="/c/Users/y7398/.rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained:$PATH"
cargo +stable-x86_64-pc-windows-gnu build -p dd-ext --bin dd-ext-search

# 5) 可选：L3 端到端（需本机 Everything HTTP 服务已开启）
cargo test --workspace -- --ignored
```

> ⚠️ **incremental 缓存 ICE**：若 clippy/test 报 `rustc_metadata rmeta encoder panic`（`os error 5 ... metadata.rmeta`），属 incremental 缓存目录权限损坏而非代码问题，统一加 **`CARGO_INCREMENTAL=0`** 重跑即可，不要改代码。

#### 8.4.4 量化验收基准（数值红线）

| 指标 | 基准值 | 来源 |
| :--- | :--- | :--- |
| 可用性探测超时 | 读 / 写均 **800ms** | §7.2 |
| 可用性 TTL 缓存 | **3s**（窗口内不重复探测） | §7.2 |
| 搜索读超时 | **3s**（⚠️ 实际被宿主 2000ms 截断，见 §7.4） | §7.2 / §7.4 |
| 宿主 `get_items` 超时 | **2000ms**（固定，不改） | §7.4 / 协议 §10 |
| 结果条数上限 | **30 条** | §7.2 |
| 评分区间 | **0.0 ~ 1.0**（`norm = skim/1000.0`，`clamp`） | §三 2.3 |
| 路径权重 | `path_score × 0.3` | §三 2.3 |
| 近因加分 | `modified > now − 7d` → **+0.1** | §三 2.3 |
| 本地搜索响应 | **<100ms** | §三 测试清单 |
| 端到端（输入完成 → 结果填充） | 主观 **<200ms**（本地回环） | §7.3 |
| 长跑 | 连续 **100 次**请求无内存 / 句柄泄漏 | §三 测试清单 |

#### 8.4.5 真机验收清单（L4，T-24 发布前逐项勾选）

| # | 场景 | 操作 | 判定标准 |
| :--- | :--- | :--- | :--- |
| 1 | 基础搜索 | 输入 `f readme` | 结果页展示匹配文件，响应 **<100ms** |
| 2 | 中文搜索 | 输入 `f 文档` | UTF-8 正常，无乱码 |
| 3 | 特殊字符 | 输入 `f dd-run`、`f v0.1` | `pct_encode` 正确，结果准确 |
| 4 | 语法透传 | 输入 `f ext:rs dm:today` | Everything 语法原样透传并生效 |
| 5 | Everything 未启动 | 退出 Everything 后进页 | 返回「未检测到 Everything」引导项；**不挂起、不 panic** |
| 6 | 端口变更 | 改端口 9090 + `DDRUN_EVERYTHING_URL=http://127.0.0.1:9090` | 能正常搜索 |
| 7 | 空结果 | 搜不存在关键词 | 返回空数组，UI 显示「无结果」 |
| 8 | limit 截断 | 输入 `f e` | 仅返回 **30 条**，排序稳定 |
| 9 | `file://` 打开 | 回车打开某文件 | 系统默认程序打开成功（否则走 `cmd /c start` 兜底） |
| 10 | 长跑 | 连续 **100 次**请求 | 无内存 / 句柄泄漏 |
| 11 | 防重 | 停留 `f report` 多帧 → Esc 返回 → 清空 → 重输 `f x` | 仅进页 **一次**；Esc 后不立即重进；清空复位后可再进 |
| 12 | 回归 | 5 个既有扩展 | `pages: None` 行为不变（仍 **-32005**） |

#### 8.4.6 完成判定（Definition of Done）

一个任务**只有全部满足**以下各项才算完成：

- [ ] 代码改动落在任务声明的「目标文件」范围内，未越界修改无关文件；
- [ ] 对应的 L1/L2 测试已写且通过（必测点见 §8.4.2 第 3 条命名规范）；
- [ ] §8.4.3 三项基线全绿：fmt exit 0、clippy 零 warning、`cargo test --workspace` 0 failed；
- [ ] 涉及量化指标的任务满足 §8.4.4 对应红线；
- [ ] 未修改 `docs/protocol.md`（协议 v1.0 冻结）；
- [ ] 未新增 `fuzzy-matcher` / `chrono` 之外的第三方依赖。

> **不在本分解范围（§7.4 已知边界，留给 v3.3）**：① 搜索超时 3s 与宿主 2000ms 的对齐（把 `search.rs` 的 3000ms 改为 ≤2000ms）；② `files.hint` / `files.guide` / `files.error` 点击文案打磨（当前落回通用「未知命令」Toast）；③ 结果页随页内二次输入实时重拉 `get_items`。AI agent 执行 §八 **不得顺手实现**这三项。

---

**方案核心变化总结**：v0.1 砍掉 fd 与自动下载模块、**不新增协议方法**、复用 v1.0 provider 模型 + 补齐运行时 `get_items`，**5~6 天内交付一个基于 Everything 的 Windows 文件搜索**（随主面板自然呈现、回车打开、Esc 返回），你本机即可日常使用；fd 兜底与跨平台作为 v0.2 的增量（2~3 天）补上，架构无缝扩展。v3.2 在不变更协议的前提下，通过宿主 `f ` 前缀自动进页 + 扩展侧速度打磨，把"文件搜索"从两步操作收敛为一步、并消除每次按键的可用性探测开销。
