# 文件搜索：Ctrl+F 直达 + 真实文件图标方案

> **状态**：已落地（2026-09-19；**E1 已处置达标、E2 待决策**）｜ **版本**：v1.3 ｜ **最后更新**：2026-09-19
> **关联**：[search-file.md](./search-file.md) · [implementation.md](./implementation.md) · [icons-typography-plan.md](./icons-typography-plan.md) · [protocol.md](./protocol.md)
>
> ⚠️ **v1.2 修订（2026-09-19，同日反向变更）**：本文 D1-6 所称「`f ` 前缀直达保留」**已失效** —— 该入口当日被**移除**（`search-file.md` §6.1、§8 v3.7）。下文 §2.1、D1-3、D1-6、§3.2、§3.3 及 §11 中涉及该前缀的表述均为**撰写/落地时点记录**（已就地标记）。本文其余决策（D1-1/2/4/5/7、D2-x）与落地记录不变。

---

## 1. 结论

本轮做两件事，**均不触碰协议、清单与既有键位表**：

| 项 | 目标 | 协议影响 | 新依赖 |
|---|---|---|---|
| **A. 开启方式** | 面板内 `Ctrl+F` 一键进入文件搜索页（现有三条入口**全部保留**） | 无 | 无 |
| **B. 文件图标** | 文件结果由「扩展名类别 glyph」升级为 **Windows Shell 真实图标**（失败回落 glyph，零退化） | 无（复用 `Icon::Path`） | 无（`image[png]` / `windows-sys` 已在 `dd-ext` 依赖内） |

**可行性依据**：GUI 侧 `IconKind::Path → 读盘 → 解码 → 纹理` 链路**已生产验证**（`com.ddrun.apps` 的首屏应用图标即走此链路）；图标抽取管线（`SHGetFileInfoW` → `HICON` → 掩码 alpha → PNG）已在本仓实现并经真机验证。故本轮为**接线与复用**，不是新造链路。

**不冲突结论**：`Ctrl+F` 全仓未被任何层绑定；两条改动分别落在 GUI 键位层（新增一支）与扩展图标出口（替换一处返回值），不改变任何既有键位、页面栈语义、协议消息、能力声明与查询链路。

---

## 2. 现状取证

> ⚠️ **行号为方案撰写时点（实施前）的快照**，实施后已发生位移——**以函数名为准**；实施后的落点见 §11.1。

### 2.1 文件搜索现有进入方式（三条）

| 路径 | 机制 | 位置 |
|---|---|---|
| 顶层入口项「文件搜索」 | `CommandRef::Page { page_id: "files.results" }` | `crates/dd-ext/src/bin/search.rs` 约 :984 |
| 兜底模板「在文件中搜索 {query}」 | `fallback` 模板 → 回车进页并携带 `context.query` | 同上 约 :1010 |
| ~~根页输入 `f ` 前缀自动进页~~（**2026-09-19 已移除**） | ~~`FILE_SEARCH_PREFIX` 命中 → 每帧 `maybe_drill_file_search()` → `open_page`~~ | `crates/dd-gui/src/app/mod.rs`（原约 :161、:503，相关代码已删） |

进页统一收口于 `PaletteApp::open_page()`（`crates/dd-gui/src/app/page.rs` 约 :235）：`push` 嵌套页 → 置 `is_loading` → 聚焦搜索框 → 回填查询 → `dispatch_fetch_page`（warm 直发 / 桩复热）。

### 2.2 面板内键位占用（冲突判定依据）

| 键 | 语义 | 位置 |
|---|---|---|
| `Esc` | 返回上一级 / Root 隐藏 | `crates/dd-gui/src/app/keys.rs` 约 :83 |
| `↑` `↓` / `Tab` / `Shift+Tab` | 列表移动 | 同上 约 :90 |
| `Enter` | 执行 | 同上 约 :98 |
| `Ctrl+,` | 打开设置 | 同上 约 :36、:101 |
| `Shift+F10` | 选中行右键菜单 | 同上 约 :39 |
| 任意键（捕获模式） | 自定义全局热键录制 | 同上 约 :170 |

`Ctrl+F`：**全仓无绑定**（`Key::F` 仅出现在捕获模式的候选码位表 `capture_vk`，不是面板键位）。

### 2.3 图标现状

| 侧 | 现状 | 位置 |
|---|---|---|
| 扩展（文件结果） | 按扩展名分 **12 类 Segoe glyph**（Document/Code/Archive/Image/Audio/Video/Executable/Config/Font/Ebook/目录/兜底），未收录回落 `Page` | `crates/dd-ext/src/bin/search.rs` 约 :731–:832 |
| GUI（渲染） | `IconKind::Path` → 读盘 → PNG/ICO 解码 → 纹理缓存 + 暗色本体检测 + 白底 tile；失败负缓存回落占位 glyph | `crates/dd-gui/src/ui/icons.rs` 约 :84–:198 |
| 已验证的抽取管线 | `SHGetFileInfoW(SHGFI_ICON|SHGFI_LARGEICON)` 32×32 → `GetIconInfo`/`GetDIBits` → BGRA→RGBA + **AND 掩码生成 alpha**（解决掩码型图标黑角）→ PNG bytes | `crates/dd-ext/src/builtins/apps.rs` 约 :1136–:1342 |
| 落盘缓存先例 | `%APPDATA%\dd-run\cache\apps-icons\<hash16>-48.png`，含 **PNG 魔数自愈**（非法文件视为未命中并重抽） | 同上 约 :931–:965 |
| GUI 隐藏期 | 隐藏收尾清空 `icon_cache`，下次唤起**从落盘 PNG 重解码** | `crates/dd-gui/src/app/mod.rs` 约 :625–:635 |
| 计时槽 | `QueryTiming{kind, channel, query_ms, score_ms, items}` + 每次 `get_items` 一行日志 | `crates/dd-ext/src/bin/search.rs` 约 :579–:628、:1037 |

### 2.4 依赖与平台事实

- `dd-ext` 已含 `image = { features = ["png"] }`（PNG 编码）与 `windows-sys` 的 `Win32_UI_Shell` / `Win32_Graphics_Gdi` / `Win32_UI_WindowsAndMessaging` → **本轮零新依赖**。
- 文件搜索是 **sidecar 子进程**（`dd-ext-search.exe` + `dist/extensions.d/com.ddrun.filesearch.json`），图标代码必须落在 `dd-ext` crate（`search.rs` 可调 `dd_ext::*`）。

---

## 3. 方案 A：Ctrl+F 一键进入文件搜索

### 3.1 决策表

| 编号 | 决策项 | 结论 |
|---|---|---|
| D1-1 | 触发键 | `Ctrl+F`（`consume_key(CTRL, Key::F)`，与既有 `Ctrl+,` 同机制、同层级） |
| D1-2 | 生效范围 | **面板可见时任意页生效**；先收敛页面栈到 Root 再进页 → 栈深恒为 2 |
| D1-3 | 查询带入 | 仅来源为 Root 时带入其查询（~~若以 `f ` 开头则去前缀~~ 自 2026-09-19 起**原样**带入）；其他页进页后为空查询 |
| D1-4 | 幂等 | 已在文件搜索页 → 不重复进页，仅聚焦搜索框并**保留**已输入内容 |
| D1-5 | 扩展不可用 | 被禁用 / 未加载 / 熔断 → Toast 明确反馈，不产生空页；不重复 push |
| D1-6 | 既有入口 | 顶层项 / 兜底模板 / ~~`f ` 前缀直达~~ **保留**（其中 `f ` 前缀直达**已于 2026-09-19 移除**，见 v1.2 修订），Ctrl+F 为新增的第 4 条；**移除后现有入口回到三条** |
| D1-7 | 可发现性 | 在文件搜索页空态提示中体现（见 §3.4，需确认） |

### 3.2 状态转移表（Ctrl+F 行为口径）

| 来源页 | 结果 | 页面栈 | 查询 |
|---|---|---|---|
| Root（查询空） | 进文件搜索页 | `[Root, 文件搜索]` | 空（显示「输入关键词搜索文件」hint 项） |
| Root（查询非空） | 进页并带入 | `[Root, 文件搜索]` | = Root 查询（trim；~~去 `f ` 前缀~~ 自 2026-09-19 起不剥离） |
| 文件搜索页 | 幂等 | 不变 | 保留现状 |
| 其他嵌套页 | 回 Root 后进页 | `[Root, 文件搜索]` | 空 |
| 设置页 | 回 Root 后进页 | `[Root, 文件搜索]` | 空；离开设置页的脏标记由**既有收口点**消费（`mod.rs` 约 :719 按 `!want_settings` 触发重聚合，无需新接线） |
| 确认对话框 / 右键菜单活跃 | 不生效（按键被对话框与菜单先行吞掉） | 不变 | 不变 |
| 热键捕获模式 | 不生效（捕获模式优先拦截全部按键，`Ctrl+F` 仍可作为自定义全局热键候选） | 不变 | 不变 |

### 3.3 实现落点

| 文件 | 改动 |
|---|---|
| `crates/dd-gui/src/app/keys.rs` | `handle_keys` 在确认对话框 / 右键菜单分支之后新增 `ctrl_f` 消费（与 `ctrl_comma` 同层），命中即调用 `open_file_search(...)` |
| `crates/dd-gui/src/app/mod.rs` | 新增 `pub(crate) fn open_file_search(&mut self, query: Option<String>)`：可用性判定 → `stack.go_home()` → `open_page(FILE_SEARCH_EXT_ID, FILE_SEARCH_PAGE_ID, query, None, Some(本地化扩展名))` → 清 `file_drill` / `file_drill_armed` → 不可用时 Toast；抽纯决策函数 `file_search_source_query(at_root, root_query) -> Option<String>` 供单测锚定 |
| `crates/dd-gui/src/text.rs` | 仅在采纳 D1-7 时新增 1 个 i18n key（提示文案）；否则不改 |
| **不改** | `open_page` / `dispatch_fetch_page` / `maybe_drill_file_search` / 文件搜索扩展 / 协议 / 清单 / 兜底链路 |

### 3.4 可发现性（D1-7，待确认）

`Ctrl+F` 是**无提示不可发现**的快捷键，建议在文件搜索页空态（现为「输入关键词搜索文件」hint 项）或搜索栏 placeholder 追加 `Ctrl+F` 说明：改 1 处 i18n + 1 处文案，无逻辑影响。

---

## 4. 方案 B：真实文件图标（Shell 图标）

### 4.1 决策表

| 编号 | 决策项 | 结论 |
|---|---|---|
| D2-1 | 图标来源 | Windows Shell 图标（`SHGetFileInfoW`，32×32），按缓存键采样**真实路径**取图 |
| D2-2 | 缓存键分级 | 目录 → `dir`；`.exe`/`.lnk`/`.msi`/`.url` → **真实路径**（每个程序自身图标）；其余 → `ext:<小写扩展名>`（取该扩展名首个真实路径作采样）；无扩展名 → `noext` |
| D2-3 | 落盘缓存 | `%APPDATA%\dd-run\cache\file-icons\<hash16>-32.png`，沿用 apps 的 **PNG 魔数自愈** 规则 |
| D2-4 | 进程内缓存 | `HashMap<key, Option<PathBuf>>`（含负缓存），常驻 sidecar 进程生命周期，避免逐条 `stat` |
| D2-5 | 回落 | 任一环节失败（取值/取位图/解码/编码/写盘）→ **既有 12 类 glyph**（零退化） |
| D2-6 | 非 Windows | 保持 glyph；`cfg` 门控，与现有查询通道一致 |
| D2-7 | 代码复用 | 把 `builtins/apps.rs` 的 `hicon_to_png` / `shfileinfo_png` / `read_mask_bits` / 缓存三函数**上移**为 `dd-ext` 共享模块（`dd_ext::shell_icon`），`apps.rs` 与 `search.rs` 共用；`apps.rs` 仅可见性/位置变化、逻辑零改动 |

**D2-7 理由**：该管线内含 4 处真机坑（掩码型图标 alpha、掩码行 DWORD 对齐、`GetDIBits` 负高 top-down、缓存魔数自愈）。重复实现会产生双份维护与漂移风险；上移是机械重构，`apps` 图标路径有既有测试 + 真机回归（A-IC-05）覆盖。**备选 D2-7b**：新模块独立实现（零回归风险、双份维护）——需确认。

### 4.2 数据流

```text
get_file_items(search_text)
  └─ search()   ← 传输（IPC / es.exe 回落，本轮不动）
      └─ score() 排序取前 30
          └─ to_command_item(entry)
              ├─ icon = entry_icon(entry):
              │     key = cache_key(entry)              ← 纯函数（单测）
              │     命中进程内缓存 → 直接 Path
              │     未命中 → 磁盘 PNG 命中? → Path
              │              ↓ 否
              │            SHGetFileInfoW(采样真实路径, ICON|LARGEICON)
              │              → HICON → hicon_to_png（掩码 alpha）
              │              → 写盘 file-icons/<hash16>-32.png → Path
              │     任一失败 → entry_glyph(entry)（既有 12 类，零退化）
              └─ 计时 icon_ms 写入既有 QueryTiming 槽 → 日志一行
```

### 4.3 性能预算（可量化）

| 场景 | 预算 | 判据 |
|---|---|---|
| 首次查询（30 条、约 8 个新键） | ≤ 40 ms（新增 `icon_ms`） | 3 次采样取中位 |
| 缓存命中（进程内 / 磁盘） | ≤ 2 ms（30 条） | 同上 |
| 查询总耗时不回归 | 现有 A-33-05 口径不变（IPC 主通道 p95 已达标） | 对比改造前后日志 |

抽取仅发生在新键首次出现，**不随结果条数线性增长**；`icon_ms` 与传输/评分分档计时，互不干扰。

---

## 5. 改动清单

| # | 文件 | 类型 | 内容 |
|---|---|---|---|
| 1 | `crates/dd-gui/src/app/keys.rs` | 改 | 新增 `Ctrl+F` 消费 + 分派（约 +8 行） |
| 2 | `crates/dd-gui/src/app/mod.rs` | 改 | `open_file_search()` + 纯决策函数 + 单测（约 +60 行） |
| 3 | `crates/dd-gui/src/text.rs` | 改（可选） | D1-7 的 1 个 i18n key |
| 4 | `crates/dd-ext/src/shell_icon.rs`（新）或 `builtins/apps.rs` 上移 | 改 | 共享图标管线（约 150 行，逻辑为**搬运**） |
| 5 | `crates/dd-ext/src/lib.rs` | 改 | 注册共享模块（1 行） |
| 6 | `crates/dd-ext/src/bin/search.rs` | 改 | `entry_icon()` 替换 `entry_glyph()` 出口、键分级纯函数、`icon_ms` 计时、单测（约 +120 行） |
| 7 | `docs/*` | 改 | 见 §8 回写清单（实施后实际回写面见 §11） |
| 8 | `tools/search_acceptance.py` | 改 | 计时解析新增**可选** `icon=` 段（`TIMING_RE` + `parse_timing`），旧日志仍可解析 |

**不改清单**（明确不动）：`protocol.md` / `manifest-schema.md` / `examples/extensions.d/com.ddrun.filesearch.json`（能力集合不变）/ `open_page` 与页面栈语义 / 传输层（IPC 与 `es.exe`）/ 评分与排序 / `fallback` 模板 / 现有 12 类 glyph 常量（保留为回落档）/ GUI 图标渲染层。

---

## 6. 验收标准

> 编号段 `A-CF-*`（Ctrl+F）与 `A-IC-*`（图标）为本轮新发编号，不与既有 `A-33-*` 冲突。
>
> ⚠️ **测量口径（实施后补记）**：`A-IC-02b/02c` 的「首次」必须在**冷缓存**下测（`tools/icon_acceptance.py --cold`）；热缓存会把 `ext:exe` 从 703.8 ms 降到 98.7 ms，从而把未达标误读成达标。实测值区间与单键成本见 [`search-file.md`](./search-file.md) §10.3。

### 6.1 A-CF：开启方式

| 编号 | 判据 | 方式 |
|---|---|---|
| A-CF-01 | Root 页按 `Ctrl+F` → 栈深 2、栈顶 `page_id = files.results`、placeholder 为本地化「文件搜索」（非原始 id） | 单测（`make_app()` + 注入 `Event::Key`）+ 真机 |
| A-CF-02 | 已在文件搜索页再按 `Ctrl+F` → 栈深不变、已输入查询**不被清空** | 单测 + 真机 |
| A-CF-03 | 从设置页按 `Ctrl+F` → 栈深 2、设置页脏标记被消费（`engines_dirty`/`exts_dirty`/`lang_dirty` 归零并触发一次重聚合） | 单测 + 真机 |
| A-CF-04 | 无修饰 `F`、`Ctrl+Shift+Tab`、`Esc`、`↑↓`、`Tab`、`Enter`、`Ctrl+,`、`Shift+F10` 行为**逐项不变** | 既有测试全绿 + 新增断言 |
| A-CF-05 | 扩展被禁用/未加载时按 `Ctrl+F` → Toast 提示、无 panic、无空白页 | 单测 |
| A-CF-06 | 触达链路：`Ctrl+F` → 输入关键词 → 结果出现；`Esc` 返回 Root 且输入框重新聚焦 | 真机 |
| A-CF-07 | 对话框 / 右键菜单活跃时 `Ctrl+F` 不穿透（按键仍被吞） | 单测 |

### 6.2 A-IC：文件图标

| 编号 | 判据 | 方式 |
|---|---|---|
| A-IC-01 | `.txt` `.pdf` `.exe` `.zip` `.png` 与文件夹六类样本图标**互不相同**且与资源管理器一致 | 真机截图比对 |
| A-IC-02 | 命中磁盘缓存后同扩展名不再抽取：`icon_ms ≤ 2 ms`/30 条；首次 `≤ 40 ms` | 扩展日志实测（3 次取中位） |
| A-IC-03 | 面板隐藏→唤起后图标仍显示（GUI 纹理清空后从落盘 PNG 重解码） | 真机 |
| A-IC-04 | 故障注入：缓存目录不可写/不存在 → 仍显示 glyph 类别图标，无 panic、无空图标列 | 真机（`DDRUN_*` 或只读目录注入） |
| A-IC-05 | 不回归：根页应用图标显示不变；`cargo test` 全绿（基线 **436**，2026-09-19 实测 + 新增） | 三关 + 真机 |
| A-IC-06 | 体积：`dd-ext-search.exe` 增量 ≤ 64 KB | `cargo build --release` 前后实测 |
| A-IC-07 | 非 Windows 目标仍可编译（渲染回落 glyph 路径） | `cargo check`（非 windows target 或 cfg 门控审查） |
| A-IC-08 | **后台补齐生效**：等待 1.5 s 后重查同一 `ext:exe` 查询 → `path` 图标 > 0 且 `icon_ms ≤ 2 ms`（E1 修复，2026-09-19 新增） | `tools/icon_acceptance.py --cold` 自动判定 |
| A-IC-09 | 自动刷新：首屏之后约 0.2–0.8 s 内宿主自动重拉并把 glyph 换成真实图标，且重拉**不重置**滚动/选中/输入（E1/C1，2026-09-19 新增） | 真机观察 + 宿主 `refresh.rs` 既有链路 |
| A-IC-10 | **稳定性**：全部查询 `kind=results`（无异常回落 / 无崩溃）；E1 的 worker 不影响主循环（2026-09-19 新增） | `tools/icon_acceptance.py` 自动判定 |

> ⚠️ **A-IC-02c 口径变更（2026-09-19，E1 修复随附）**：原「按真实路径档**首抽** ≤ 40 ms」在 `path:` 键下沉后台后，其语义变为「该档**同步路径**耗时 ≤ 40 ms」（`path=0 / glyph=30` 属设计预期）；真实图标的最终到达由 **A-IC-08** 覆盖。A-IC-04 的判据同步改为「**等待补齐后**重查 `.lnk` 批」（可访问的变 `path`、不可访问的留 `glyph`）。

### 6.3 三关与构建（强制）

`cargo fmt --check` / `clippy -D warnings` / `cargo test`（debug）+ **`cargo build --release`**（本地教训：`#[cfg(debug_assertions)]` 类 release-only 错误三关与 CI 四关均检不到）；变更触及 `protocol.md` 时须跑 `cargo test -p dd-protocol`（本轮不触及）。

---

## 7. 风险与对策

| 风险 | 影响 | 对策 |
|---|---|---|
| Shell/GDI 调用阻塞查询线程 | 延迟尖峰 | 仅新键抽取 + 进程内缓存 + `icon_ms` 计时；超预算则降级为「先 glyph、后台补图标」（预留，不在本轮） |
| 网络路径 / 离线盘采样慢 | 查询卡顿 | 目录键恒为 `dir`（不按路径）；采样路径优先本地盘；失败立即回落 glyph |
| 彩色图标与灰色 glyph 混排 | 视觉不统一 | 文件结果统一走 Path；hint/guide/error 保持 Search glyph；尺寸由 GUI 缩放到既有 24px 图标格 |
| D2-7 上移 `apps.rs` 管线引入回归 | 根页应用图标损坏 | 仅移动不改逻辑；A-IC-05 真机回归；必要时回退 D2-7b |
| 缓存目录膨胀 | 磁盘占用 | 键空间 = 扩展名 + exe/lnk 路径，量级 10²–10³ 个 ≤ 4 KB PNG（≤ 数 MB）；**清理策略列为开放项**（未做） |
| `Ctrl+F` 与用户既有肌肉记忆（某扩展内期望「查找」） | 预期落差 | 面板内无文本编辑场景；D1-7 提示 + 文档登记 |

---

## 8. 待决策项（动工前需确认）

| # | 问题 | 选项 | 建议 |
|---|---|---|---|
| Q1 | `Ctrl+F` 生效范围 | A) 面板内任意页（先回 Root 再进页，栈深恒 2）／B) 仅 Root | **A** |
| Q2 | 是否加入可发现性提示（D1-7） | A) 加（空态/placeholder 显示 `Ctrl+F`）／B) 不加 | **A** |
| Q3 | 图标管线复用方式（D2-7） | A) 上移共享模块（去重，动 `apps.rs` 可见性）／B) 新模块独立实现（零回归，双份维护） | **A**（真机回归兜底） |
| Q4 | `.exe`/`.lnk` 图标键 | A) 按真实路径（每个程序自身图标）／B) 按扩展名（统一 exe 图标） | **A** |

---

## 9. 落地后文档回写清单

| # | 文档 | 动作 |
|---|---|---|
| 1 | [search-file.md](./search-file.md) | v3.5 → **v3.6**：新增「进入方式（含 Ctrl+F）」与「图标策略」章节；本方案正文并入，规划表述改记录 |
| 2 | [INDEX.md](./INDEX.md) | §3.D 本文件行状态改「已落地」+ 行数重算；元信息版本与日期更新 |
| 3 | [implementation.md](./implementation.md) | §2 批次登记 + §3.1 测试基线台账（436 → 新值，实施时重新实测） |
| 4 | [icons-typography-plan.md](./icons-typography-plan.md) | 新增「Shell 真实图标」档，标注与既有 glyph 档的层级关系 |
| 5 | [CHANGELOG.md](../CHANGELOG.md) | `[Unreleased]` 新增条目 |
| 6 | protocol.md / doc-code-diff | **不改**：零协议改动、无契约差异 → 豁免，理由随 §5 清单登记 |

---

## 10. 版本演进

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.0 | 2026-09-19 | 首版：Ctrl+F 直达方案（D1-x）与真实文件图标方案（D2-x）、验收标准 A-CF/A-IC、待决策项 Q1–Q4。未改任何代码 |
| v1.1 | 2026-09-19 | **落地记录**（见 §11）：Q1–Q4 全按建议项实施；448 passed；两处未达标如实记档（按真实路径档首抽 703.8 ms / sidecar +104.5 KB），**阈值未擅自修订**，选项见 `search-file.md` §10.4 |

---

## 11. 落地记录（v1.1，2026-09-19）

**已按 Q1–Q4 建议项（A/A/A/A）实施完毕**：任意页生效 / 加可发现性提示 / 上移共享管线 / `.exe`·`.lnk` 按真实路径。改动面：`dd-gui`（`app/keys.rs`、`app/mod.rs`、`text.rs`）、`dd-ext`（新增 `src/shell_icon.rs`、`src/lib.rs`、`src/bin/search.rs`、`builtins/apps.rs` 删除已上移实现）、`tools/search_acceptance.py`；**协议与清单零改动**（`protocol.md` / `manifest-schema.md` 未动，符合方案 §5「不改清单」）。

**验收实测**：`fmt` 无差异 / `clippy --workspace --all-targets` 0 告警 / `cargo test --workspace` **448 passed**（基线 436 + 12 新单测：A-CF 5 + A-IC 7）/ `cargo build --release` **exit 0**；真机直驱 release sidecar 的 `icon_ms` 三档与缓存体量、回落真机生效证据见 [`search-file.md`](./search-file.md) §10.3。

| 验收项 | 结果 | 依据 |
|---|---|---|
| A-CF-01…05、07（进入方式与键位） | ✅ | 单测 5 项 + 纯决策 1 项（`app/mod.rs` 测试模块）；A-CF-06 端到端 GUI 走查留待用户真机 |
| A-IC-01（类型区分度） | ✅（取证方式修正） | 由「截图比对」改为**缓存键 → 图标文件 → 内容指纹**三元组对齐（同为观感判据的可机读版本）；截图比对仍建议用户侧抽查 |
| A-IC-02（`icon_ms` 预算） | ⚠️ **部分** | 缓存命中 0.13–0.44 ms ✅（≤2 ms）；扩展名档首抽 12.4–49.1 ms ✅（≈≤40 ms）；**按真实路径档首抽 703.8 ms ❌** |
| A-IC-03 / A-IC-04（隐藏唤起 / 故障回落） | ✅（回落为真机证据） | 回落：`ext:lnk` 13/30 不可访问路径 → 类别 glyph，无 panic；隐藏唤起为落盘 PNG 复用路径，由既有 GUI 链路承担（未单独真机复测） |
| A-IC-05（不回归） | ✅ | 448 passed / 0 failed；根页 apps 图标链路仅位置变化（同实现） |
| A-IC-06（体积 ≤ 64 KB） | ❌ **未达标** | 实测 **+104.5 KB**（876,544 B）；宿主侧 +3,072 B |
| A-IC-07（非 Windows 编译） | ✅（静态门控审查） | `extract_png` 非 Windows 桩 + `cfg` 门控；未在非 Windows 目标实跑 |

**两处未达标项的处置选项见 [`search-file.md`](./search-file.md) §10.4**（E1 建议 O1 限流；E2 建议修订预算或改存储格式）——**本方案不擅自调整已定的验收阈值**，列为待决策。

### 11.1 实现后落点（行号为实施后实测，函数名为准）

| 组件 | 落点 |
| --- | --- |
| 共享图标管线（HICON/HBITMAP → PNG、缓存三函数、文件类型取图入口） | `crates/dd-ext/src/shell_icon.rs`：`cache_dir` :25 · `cache_key` :33 · `cached_png` :55 · `store_png` :69 · `file_type_icon_png` :88 · `shfileinfo_png` :133 · `hicon_to_png` :178 · `bitmap_to_png` :311 |
| 文件图标键分级 / 进程内缓存 / 出口 / 计时 | `crates/dd-ext/src/bin/search.rs`：`icon_cache_key_for` · `ICON_CACHE` · `lookup_icon_png` · `icon_from_png` · `entry_icon` · `QueryTiming.icon_ms` |
| `Ctrl+F` 消费与直达 | `crates/dd-gui/src/app/keys.rs`（约 :42 消费）· `app/mod.rs::open_file_search_from_panel`（约 :565）· `app/mod.rs::file_search_source_query`（约 :192） |
| 验收工具（A-IC 六项判定 + JSON 证据） | `tools/icon_acceptance.py`（`--cold` 冷缓存口径） |
| 应用图标（复用共享管线，48px） | `crates/dd-ext/src/builtins/apps.rs`：`file_icon_png` · `shell_item_icon_png` · `factory_get_image_png` |
