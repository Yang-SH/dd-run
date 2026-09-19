# 图标与字体展示优化（参考 DeskBox）

> **状态**：已落地（I3 未做；F3 同日追加落地；G1 于 2026-09-19 追加并落地）｜ **版本**：v1.2 ｜ **最后更新**：2026-09-19
> **关联**：[implementation.md](./implementation.md)

---

## 1. 落地状态总表

| 项目 | 状态 | 落点文件 | 备注 |
|---|---|---|---|
| I1 — 无图标/url 项回落占位 glyph | ✅ | `crates/dd-gui/src/ui/icons.rs` | 用 `Palette::text4` 极弱色 |
| I2 — glyph 色阶提升一档（weak → text2） | ✅ | `crates/dd-gui/src/ui/icons.rs` | 显式取 token 的等价重构，零视觉变化 |
| I3 — Shell 回退链路 32px → 48px | ❌ | — | 未做（主链路覆盖绝大多数应用，视真机效果再定） |
| F1 — 列表路径字号 token 化 | ✅ | `crates/dd-gui/src/theme.rs` | 纯重构，观感零变化 |
| F2 — 列表密度三档 | ✅ | `theme.rs` / `settings.rs` / `app/mod.rs` / `ui/*` | i18n key 为 `set.density.*`（非 `settings.density.*`） |
| F3 — 拉丁主字纠偏（Segoe UI 置顶，2026-09-13 追加） | ✅ | `crates/dd-gui/src/platform.rs` | 族序显式重排，仅拉丁一处变化，其余路由逐一不变（见 §5.4） |
| 真机复验（占位层级 / 密度三档截图） | ⚠️ | — | 待做 |
| G1 — 文件结果改用 Shell 真实图标（2026-09-19 追加，当日落地） | ✅ | `crates/dd-ext/src/shell_icon.rs`（新增）/ `crates/dd-ext/src/bin/search.rs` | 真实图标为默认档、既有 12 类 glyph 降为回落档；渲染侧零改动，详见 §4.5 |

> 参照对象：[Tianyu199509/DeskBox](https://github.com/Tianyu199509/DeskBox)（WinUI 3 桌面整理工具，GPL-3.0）。借鉴其三个核心做法——**所有条目都有真实图标可显示、图标/文字大小可调、文字与行高联动**；技术栈不同（我们 = egui/Fluent token），只借鉴设计决策，不搬实现。
> 原则：可实现、无错漏、无冲突、不复杂、符合需要——默认档所有数值 = 当前现值，落地后默认观感与上一版本逐像素一致。
>
> **实施修正（相对 v1.0 草案）**：
> 1. **I2 前提修正**：核实 `theme.rs` visuals 已设 `weak_text_color = Some(p.text2)`——glyph 本来就渲染在 text2 色，「weak 档偏虚」不成立，I2 实际为**显式取 token 的等价重构**（零视觉变化）；states 辅助函数因此无调用者，已删除。
> 2. **F2 持久化路径修正**：`Settings` 无 serde derive，是手工 `serde_json::Value` 读写——`ListDensity` 沿用既有 `label/as_str/parse` 枚举模式（语义与草案的 `#[serde(default)]` 等价：缺字段/未知值回落标准档）。
> 3. **F2 耦合点核实**：`ROW_H` 引用点实测为 `app/mod.rs`（默认高度折算）、`ui/row.rs`、`ui/states.rs`（Loading 骨架行）三处 + `theme.rs`（定义与单测）；面板默认高度折算（`base_height_for_workarea`/`root_panel_size`/`settings_panel_size`）已增 `row_h` 参数并接线两处调用点（`lifecycle.rs` 唤起、`ui()` 页面 diff）。

---

## 2. 现状盘点（源码核实结论，行号约值）

| 层 | 现状 | 位置（约） | 判定 |
|---|---|---|---|
| 图标源 | dd-ext apps：`IShellItemImageFactory` 抽 48px PNG 落盘缓存（`ICON_SIZE = 48`）；失败回退 `SHGetFileInfoW(SHGFI_LARGEICON)` 仅 **32px** | `dd-ext/src/builtins/apps.rs` 约 :96、约 :978 | 主链路 48px 对 24px 显示格 = 精确 2×，≤200% DPI 够用；**回退链路是短板**（150% DPI 下 24 逻辑 px = 36 物理 px > 32px 源，轻微放大发糊） |
| 图标渲染 | 24×24 格 + glyph 20pt + 暗色图标垫浅色 tile | `dd-gui/src/ui/icons.rs` 约 :12（`ICON_CELL`）、约 :15（`ICON_GLYPH_PT`）、约 :153（`draw_icon_cell`） | ✅ 基线成立，本稿不改几何默认值 |
| glyph 用色 | 单色绘制用 `weak_text_color`（weak 档），与彩色 path 纹理同列时明显偏虚 | `ui/icons.rs` 约 :161-167；`states.rs` 约 :9（`weak_text_color` = `visuals.weak_text_color`） | ⚠️ 可辨识度低于类型标签（text3）之上的层级预期 |
| 无图标/url 项 | `IconKind::Url` 渲染**空列**（M5 决策"不做网络下载"） | `ui/icons.rs` 约 :141 | ⚠️ 视觉上等于"图标丢了"；DeskBox 每行都有图标 |
| 字体栈 | 后台线程 mmap 热替换 msyh.ttc + seguisym + segoeui + SegoeIcons/segmdl2 + B1 semibold 族 | `dd-gui/src/platform.rs` 约 :55（`setup_cjk_fonts`） | ✅ M2 已 mmap 化，质量与内存均到位，**本稿不动** |
| 字号体系 | 列表标题 14pt semibold / 类型标签 12pt 等硬编码散落 ui 层（全仓约 89 处 `size()`/`FontId::` 调用） | `ui/row.rs` 约 :62-63 等 | ⚠️ 无法做 DeskBox 式"文字大小可调" |
| 行高几何 | `ROW_H = 40.0` 编译期常量，parity 单测守卫 | `theme.rs` 约 :18（定义）、约 :726（断言） | 字号可调必须让行高联动 |
| **ROW_H 引用点** | ①面板默认窗口高度按 `avail_h/ROW_H` 折算行数；②行绘制；③空态/加载态行 | `app/mod.rs` 约 :116-117、`ui/row.rs` 约 :22,52、`ui/states.rs` 约 :141 | ⚠️ F2 的关键耦合：**面板默认高度必须随密度档联动**，否则宽松档一行显示不下 |
| 设置持久化 | `Settings` serde JSON；设置页外观分类已有主题/材质 radio-card 惯用法 | `dd-gui/src/settings.rs` 约 :223（`struct Settings`）、`ui/settings_view.rs` 约 :351,369 | F2 直接挂入，无新机制 |

---

## 3. 方案总览

| # | 措施 | 优先级 | 改动量 | 风险 |
|---|---|---|---|---|
| I1 | 无图标/url 项回落极弱色占位 glyph（替代空列） | P1 | <20 行 | 低（需修订设计稿 04"空列"决策记录） |
| I2 | glyph 图标色阶提升一档（weak → text2） | P1 | 一行级 | 极低 |
| I3 | Shell 图标回退链路 32px → 48px（SHGetImageList） | P2（可选） | ~30 行 | 低（主链路覆盖绝大多数应用） |
| F1 | 列表路径字号 token 化（纯重构，观感零变化） | P1 | ~6 处引用 | 极低（初值全部 = 现值） |
| F2 | 列表密度三档（紧凑/标准/宽松），行高/字号/图标联动 | P1（核心） | 6 文件 | 中（面板默认高度耦合，见 §5） |

实施顺序：**第一批 I1+I2 → 第二批 F1 → 第三批 F2 →（I3 视真机效果）**，随批更新设计文档与 CHANGELOG（见 §7）。

---

## 4. 图标展示详设（I1–I3）

### 4.1 I1 — 无图标/url 项回落占位 glyph

- **现状**：`icons.rs` 约 :141 `IconKind::Url => IconView::Empty`，无图标项同样空列；设计稿 04 决策"无图标项保留 20px 空列对齐"。
- **改法**：空列语义改为渲染 `PLACEHOLDER_GLYPH`（U+E7C3，已有常量）但用 **`Palette::text4` 极弱色**——与解码失败的占位 glyph（`text2` 色）区分层级："本来就没有" ≠ "加载失败"。对齐行为不变（仍占满 `ICON_CELL` 格），只补占位符。
- **落点**：`ui/icons.rs`——`IconView::Empty` 分支改为弱色 glyph（或在 `draw_icon_cell` 内处理），`resolve_icons` 的 `Url` 分支保持 `Empty` 形态即可（渲染层统一画占位）。
- **冲突修订**：设计稿 04"空列"决策作废，需同步改 `cmdpal-platform-agnostic-design.md` 对应条目 + `icons.rs` 顶部注释记录新决策与日期。
- **M5 决策不翻案**：url favicon 仍不做网络下载。

### 4.2 I2 — glyph 色阶提升一档

- **改法**：`draw_icon_cell` 的 glyph 分支由 `weak_text_color(ui)` 改为 `theme::Palette::of(ui.visuals().dark_mode).text2`（次级文本档，与 Tag 同级）。
- **落点**：`ui/icons.rs` 约 :161-167 一处。`states.rs::weak_text_color` 其余调用点（空态说明等）不动——它们就该是 weak 档。
- **语义**：单色 Fluent 风格不变，仅把 glyph 从"装饰性弱元素"提到"功能性次级元素"，与彩色图标视觉重量对齐。

### 4.3 I3 —（可选）回退链路 32px → 48px

- **现状**：主链路 `IShellItemImageFactory` 失败时 `SHGetFileInfoW(SHGFI_LARGEICON)` 只有 32px（`apps.rs` 约 :978-980）。
- **改法**：回退改 `SHGetFileInfoW(SHGFI_SYSICONINDEX)` 取系统图标索引 + `SHGetImageList(SHIL_EXTRALARGE)` 取 48px HICON，进现有 `hicon_to_png` 管线。COM 依赖已在使用中（`shell_item_icon_png` 即 IShellItemImageFactory），**无新依赖**。
- **定为可选的依据**：主链路覆盖绝大多数应用，此条只影响少数解析失败的兜底；价值/工作量比中等，不阻塞交付。

### 4.4 明确不采纳（DeskBox 有但不适合启动器场景）

| DeskBox 做法 | 不采纳理由 |
|---|---|
| 256px Shell 图标源 + overlay | 48px 源对 24px 显示格是精确 2×；升 256 只膨胀 `%APPDATA%\dd-run\cache\apps-icons\`，违背"不复杂" |
| url favicon 经 Shell/网络解析 | M5 已有明确决策"不做网络下载"，不翻案 |

---

### 4.5 G1 — 文件结果改用 Shell 真实图标（2026-09-19 追加，当日落地）

**动机**：文件搜索此前只按扩展名给 12 类 Segoe glyph 类别图标，与资源管理器的观感差距明显（`.pdf`/`.zip`/`.png` 各异的应用关联图标全部被压成同一张纸）。

**做法（与 I1–I3 的关系：真实图标优先，glyph 降为回落档）**：

| 档位 | 适用 | 落点 |
|---|---|---|
| ① Shell 真实图标（32×32 → PNG 落盘缓存 → `IconKind::Path`） | 文件搜索结果的**默认**档 | `dd-ext/src/shell_icon.rs`（新增共享管线）、`bin/search.rs`（键分级 + 进程内缓存） |
| ② glyph 类别图标（12 类，本稿 I1/I2 的既有档） | ① 抽取失败 / 非 Windows / hint·guide·error 项 | `bin/search.rs::entry_glyph` |
| ③ 应用图标 48px（`IShellItemImageFactory` 优先） | 首屏「应用」列表（不变） | `builtins/apps.rs` 复用 `shell_icon` 的位图→PNG 段 |

**渲染侧零改动**：宿主 `ui/icons.rs` 的 path 图标链路（读盘 → PNG/ICO 解码 → 纹理缓存 → 暗色本体检测 → 24px 图标格）原样复用；I1 的 `IconKind::Url` 空列策略不变（本次不涉及 url 图标下载）。

**验收与两处未达标**（按真实路径档首抽 703.8 ms、sidecar 体积 +104.5 KB）见 [`search-file.md`](./search-file.md) §10.3–§10.4；I3（回退链路 32px → 48px）仍旧未做，与本档不冲突（本档按扩展名共图，每类只抽一次）。

## 5. 字体展示详设（F1–F2）

### 5.1 F1 — 列表路径字号 token 化（纯重构）

- **改法**：`theme.rs` 增加排印常量组，**仅列表绘制路径**（`ui/row.rs`、`ui/panel.rs` 列表段、`ui/icons.rs`）改引常量：

```rust
// theme.rs 新增（值 = 现状，纯收拢；设置页/页脚字号有各自 parity 单测锚定，不动）
pub const LIST_TITLE_PT: f32 = 14.0; // 行名（B1 semibold，设计稿 `.name`）
pub const LIST_CAT_PT: f32 = 12.0;   // 类型标签 caption1（05.1）
pub const LIST_ICON_GAP: f32 = 12.0; // 图标-标题间距（CSS `.row` gap）
```

- **范围控制**：只收拢列表路径约 5–6 处，**不做全仓 89 处大扫除**——设置页/页脚字号各有机理与单测锚点，动了收益低风险高，违背"不复杂"。
- **无冲突**：所有常量初值 = 当前硬编码值，parity 单测与视觉契约（05.1 字号 ramp）不变；`cargo test` 全绿 + 截图确认零视觉差异即验收。

### 5.2 F2 — 列表密度三档（本方案核心）

DeskBox 的"图标/文字大小可调"落地方。**一档联动四值**，不拆成多个孤立设置项（DeskBox 是逐项独立设置，设置项爆炸，违背"不复杂"）：

| 档 | 行高 ROW_H | 标题字号 | 类型标签字号 | 图标格 ICON_CELL | glyph 字号 |
|---|---|---|---|---|---|
| 紧凑 Compact | 36 | 13 | 11 | 20 | 16 |
| 标准 Standard（默认） | 40 | 14 | 12 | 24 | 20 |
| 宽松 Relaxed | 44 | 15 | 13 | 28 | 24 |

落点① — **`theme.rs`**：`ListMetrics { row_h, title_pt, cat_pt, icon_cell, glyph_pt }` + `ListMetrics::of(ListDensity)`；**标准档硬性等于现有常量**（`ROW_H=40` 等常量保留为标准档锚点）。`theme.rs` 约 :726 的 `assert_eq!(ROW_H, 40.0)` **原样保留**，另加一条"标准档映射 = 各常量"断言。

落点② — **`settings.rs`**：`Settings` 加 `density: ListDensity`（缺省 = Standard）。沿用现有 JSON 持久化，**老配置文件无该字段自动落标准档，零迁移**；`ListDensity` 派生 `Serialize/Deserialize/Copy/PartialEq`。

落点③ — **绘制路径改运行时 metrics**（grep 已核实的全部引用点）：
- `app/mod.rs` 约 :116-117：面板默认高度折算 `avail_h / ROW_H` → 改用 `ListMetrics::of(self.settings.density).row_h`（**关键耦合**：不改则宽松档默认窗口一行显示不下）；
- `ui/row.rs` 约 :22,52：行矩形预算与内容区高度；
- `ui/states.rs` 约 :141：空态/加载态行高（与列表行一致，否则状态页与结果页行高跳变）；
- `ui/icons.rs`：`ICON_CELL`/`ICON_GLYPH_PT` 常量改 `draw_icon_cell` 入参（由 row.rs 传 `metrics.icon_cell` / `metrics.glyph_pt`）。

落点④ — **设置页**：外观分类（主题/材质旁）加三档 radio-card，复制 `settings_view.rs` 约 :351（主题偏好）既有模式；i18n 经 `crate::text::t` 增补 zh/en 各 2 条 key（`set.density.name` / `set.density.desc`，实际代码键名前缀为 `set.` 而非 `settings.`）。

**冲突检查（逐项对照 B1–B8 已实施批次）**：

| 已有契约 | 影响 |
|---|---|
| B1 semibold 族（`theme::semibold()`） | 照用，仅字号入参随档位变化 |
| B5 标题左对齐贴图标 + 类型标签贴右 | 布局不动；类型标签 90px 截断宽度逻辑随 `cat_pt` 重新量宽，`text_width` 本就按 FontId 测量，无需改 |
| B7 `control_hover` / B8 测试 | 不涉及 |
| 键盘导航（按索引选中） | 无行高耦合，不受影响 |
| 面板用户手调尺寸（`persist_panel_size`） | 只在用户真拉伸过时生效，与默认高度折算独立，不冲突 |
| 一屏行数 | 紧凑 10 行 / 标准 9 行 / 宽松 8 行，属预期行为，在设置页 desc 说明 |

### 5.3 明确不采纳

| DeskBox 做法 | 不采纳理由 |
|---|---|
| 两行文件名（一行/两行/隐藏） | B5 刚决策"单行 + 整行 hover tooltip 信息不丢"；两行使行高动态化、与 40px D8 几何契约冲突，启动器场景收益低 |
| 逐组件独立字号（Quick Capture/Todo 各自调） | 组件数量少，三档密度已覆盖需求 |
| 文字边缘处理（text edge treatment） | egui 无对应渲染管线钩子，收益不可感 |

### 5.4 F3 — 拉丁主字纠偏（2026-09-13 追加，当日落地）

- **现状（源码核实）**：epaint 0.36.1 默认 `Proportional = [Ubuntu-Light, NotoEmoji-Regular, emoji-icon-font]`，本仓把 cjk/sym/segoe/icons **追加**其后 ⇒ 拉丁/数字全部由 Ubuntu-Light 渲染（segoe 仅缺字兜底），与行名 semibold 拉丁（Segoe UI Semibold）同屏异字体混排。
- **改法**：`load_cjk_font_definitions` 末尾显式重排 `[segoe, NotoEmoji, emoji-icon, cjk, sym, icons]`——segoe 置顶、Ubuntu-Light 移出、**其余成员相对次序一字不动** ⇒ 仅拉丁一处变化：CJK/emoji/◌/PUA 回退路由逐一保持；`retain` 按 font_data 实际注册过滤，缺文件降级安全（无 segoe → 等价旧观感）。semibold 链 `extend(prop)` 自动继承新序，regular/semibold 从此同为 Segoe 系。
- **验收**：`cargo test -p dd-gui` 181 通过、clippy 0 警告；真机截图清单：英文应用名混排、U+A78B（AMD 项）、✓/◌/PUA glyph、三密度档、亮暗两主题。
- **明确不做**：Segoe UI Variable（egui 无法定位 opsz 实例）、首帧同步预载 segoe（半残态更难看，M6 6.2 取舍已记档）。

---

## 6. 不动的部分（防过度设计）

- **字体栈**（`platform.rs` 约 :55 全链路）：M2 mmap 化 + 后台热替换 + semibold 族已到位，本稿零改动。
- **图标抽取主链路**（`IShellItemImageFactory` 48px + 落盘缓存）：不动。
- **暗色垫底 tile**（`icons.rs` 约 :169-183）：刚按真机反馈修好，不动。
- **面板尺寸持久化 / 亚克力材质 / 主题跟随**：已有机制，不涉及。

---

## 7. 随批文档与验收

1. **第一批（I1+I2）**：改后 `cargo test -p dd-gui` + 真机截图对比（亮暗两主题各一张列表图，验证占位 glyph 层级与 glyph 可辨识度）。
2. **第二批（F1）**：纯重构，`cargo test` 全绿 + 截图确认零视觉差异。
3. **第三批（F2）**：三档各截一张列表图 + 设置页截图；**老配置兼容性验证**——删掉配置文件 `density` 字段后启动应落标准档；宽松/紧凑档下拉伸面板确认默认高度折算正常。
4. **随批收尾**：`cmdpal-platform-agnostic-design.md` 修订（04 空列决策 → 占位 glyph；§8.6 补密度表）、CHANGELOG 追加、`cmdpal-ui-optimization-v5.html` 补"C 批次"规格。
5. **I3** 视真机效果决定做不做，不阻塞交付。

改动总量：约 6 个文件的实质改动 + 3 个文档，无新依赖，默认观感与当前版本逐像素一致，老配置零迁移。
