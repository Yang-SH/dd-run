# dd-run 代码结构整理方案（仅 dd-gui）

> **状态**：执行中（批次 A 已落地，真机冒烟通过） ｜ **版本**：v1.3 ｜ **最后更新**：2026-10-21
> **关联**：[implementation.md](./implementation.md) · [refactor-layering-plan.md](./refactor-layering-plan.md) · [o9-o14-optimization-plan.md](./o9-o14-optimization-plan.md) · [INDEX.md](./INDEX.md)

---

## 1. 结论

本方案只整理 **`dd-gui` crate 的文件组织与命名**，不改任何行为、不改任何对外路径与签名。

- **范围**：`dd-gui`（61 个文件 / 26,394 行，占全仓 58%）。
- **手法**：沿用 O12「目录化拆分」惯例——**搬运单位 = 完整定义块（含文档注释），函数体一字不改**；拆分形态沿用本 crate 已有的 `xxx.rs + xxx/` 共存先例（`platform.rs` + `platform/capture_hook.rs`）。
- **目标**：`dd-gui` 内 **≥800 行（非空行口径）**的单文件从 8 个降到 5 个——本方案拆其中 3 个（`platform` / `aggregator` / `theme`）；`app/mod.rs`（1295）拆分另立项（见 §8），`state`（1015）/ `settings/tests`（936）/ `text`（888）/ `ui/settings_view/appearance`（838）不在本轮范围。
- **口径**：§2 的行数一律为**非空行**（去掉纯空行）；§10.2 的执行记录另按**全行**（含空行）登记，两套口径**不可直接相减**（同一文件两种口径相差约 5–7%）。
- **判据**：`cargo test --workspace` 维持 627 passed / 0 failed；`git diff -M` 能把改动识别为纯移动；release 产物体积对比**即时基线 8,804,352 B** 零漂移（§10.1 已按 HEAD 实测修正，原 9,105,408 B 为 2026-09-24 的过时记录）。

**为什么不是全仓**：`dd-ext` / `dd-host` / `dd-run-cli` 的巨型文件同样存在（§2），但它们的拆分不产生 UI 层收益，且 CLI 无真机腿可验，故本轮不动；编号留在 §8 的后续项里。

---

## 2. 现状取证（2026-10-21 实测）

### 2.1 规模分布

| crate | 文件数 | 行数（非空） | 占比 |
|---|---|---|---|
| `dd-gui` | 61 | 26,394 | 58.3% |
| `dd-ext` | 19 | 9,355 | 20.7% |
| `dd-host` | 9 | 5,722 | 12.6% |
| `dd-protocol` | 8 | 1,839 | 4.1% |
| `dd-run-cli` | 1 | 1,320 | 2.9% |
| `dd-ext-sample` | 1 | 621 | 1.4% |
| **合计** | **99** | **45,251** | — |

### 2.2 `dd-gui` 内 ≥800 行单文件（非空行口径）

> 8 行中前 5 个为 ≥1000 行，`settings/tests` / `text` / `appearance` 三行在 800–1000 之间（一并列入是因为它们与巨型文件同属「单文件职责过密」，但本轮不拆）。

| 文件 | 行数 | 生产代码约 | 混杂职责数 |
|---|---|---|---|
| `crates/dd-gui/src/platform.rs` | 1352 | 1157 | 7 |
| `crates/dd-gui/src/aggregator.rs` | 1440 | 857 | 5 |
| `crates/dd-gui/src/theme.rs` | 1304 | 713 | 4 |
| `crates/dd-gui/src/app/mod.rs` | 1295 | 946 | 9（`PaletteApp` 95 个字段） |
| `crates/dd-gui/src/state.rs` | 1015 | 约 700 | 2 |
| `crates/dd-gui/src/settings/tests.rs` | 936 | 0（纯测试） | 1 |
| `crates/dd-gui/src/text.rs` | 888 | 736（含 600 行 i18n 表） | 2 |
| `crates/dd-gui/src/ui/settings_view/appearance.rs` | 838 | 838 | 3 |

> 「生产代码约」= 去掉文件内 `#[cfg(test)] mod tests` 之后的行数。纯测试文件不计入拆分范围。

---

## 3. 问题清单

### 3.1 A — 巨型单文件且多职责并存（本次处理 A1–A3）

| 编号 | 文件 | 行区间 → 职责 | 本次 |
|---|---|---|---|
| A1 | `crates/dd-gui/src/platform.rs` | 18–300 字体与工作集；301–419 光标屏工作区与居中；423–465 提权；467–583 file URL 解析；590–703 reveal / open；707–952 DWM 材质与 chrome（含 950 的 `wide`）；957–1016 自启动注册表；1019–1037 系统语言；1040–1155 光标隐藏 | ✅ 批次 A |
| A2 | `crates/dd-gui/src/aggregator.rs` | 51–149 常量与类型；150–219 环境注入；220–500 扫描 / 来源 / 信任；502–725 collect 与 spawn；726–856 条目拍平与自定义命令 | ✅ 批次 B |
| A3 | `crates/dd-gui/src/theme.rs` | 18–130 几何 token；134–265 `Palette`；267–330 阴影；340–510 材质与着色；514–712 `Colorization` / `visuals` | ✅ 批次 C |
| A4 | `crates/dd-gui/src/app/mod.rs` | 95 字段横跨热键 / 托盘 / 聚合 / LRU / 图标 / 设置缓冲 / 窗口材质 / 尺寸 / i18n | ❌ §8（P5 另立项） |
| A5 | `crates/dd-gui/src/state.rs` | 列表状态机 + 拼音索引；职责其实单一，仅**命名**误导（见 C4） | ❌ §8（待裁决） |
| A6 | `crates/dd-ext/src/bin/search.rs`（2596）、`crates/dd-ext/src/builtins/apps.rs`（1903）、`crates/dd-host/src/process.rs`（1317）、`crates/dd-run-cli/src/main.rs`（1320） | 同型问题，但不在本轮范围 | ❌ §8 |

### 3.2 B — 分层渗漏

| 编号 | 现状 | 本次 |
|---|---|---|
| B1 | `aggregator.rs` 承担清单扫描、sidecar 目录解析（`load_extension_sources` / `merge_sidecar_scan` / `sidecar_extensions_dir`），而同类职责 `crates/dd-host/src/manifest.rs` 已在 `dd-host` | ❌ 涉及 crate 边界与 `dd-host` API，超范围 |
| B2 | `platform.rs` 把**可单测的纯文本**（`split_file_url` / `percent_decode` / `to_windows_path` / `reveal_path_allowed`）与 Win32 FFI 混在一处 | ✅ 批次 A 拆出 `paths` / `shell` 两个可单测单元 |
| B3 | CLI 的 `conformance_after_open`（约 360 行）内联在 `main.rs` | ❌ 范围外 |

### 3.3 C — 命名重叠（同仓两处同名，只能靠目录区分）

| 编号 | 冲突对 | 影响 | 本次 |
|---|---|---|---|
| C1 | `crates/dd-gui/src/aggregator.rs`（扫描/映射）vs `crates/dd-gui/src/app/aggregate.rs`（聚合任务编排） | 检索与跳转时无法凭名字区分 | ✅ 改 `app/aggregate.rs` → `app/aggregate_job.rs` |
| C2 | `crates/dd-gui/src/navigation.rs`（页面栈）vs `crates/dd-gui/src/app/keys/navigation.rs`（键导航） | 同上 | ✅ 改键侧 → `app/keys/page_keys.rs` |
| C3 | `crates/dd-gui/src/app/toast.rs`（状态与时长）vs `crates/dd-gui/src/ui/toast.rs`（绘制） | 同上 | ✅ 改状态侧 → `app/toast_state.rs` |
| C4 | `crates/dd-gui/src/state.rs` 实为「列表状态」而非 app 状态 | 名不副实 | ❌ 改名需 re-export 桥接，收益不抵，待裁决（§8） |

### 3.4 D — 别名 hack

`crates/dd-gui/src/lib.rs` 的 `extern crate self as dd_gui;` 让 lib 内部可用 `dd_gui::` 全路径。实测：**30 个文件共 135 处** lib 内部自引用（`main.rs` 另有 13 处属 bin → lib 正常引用，不动）。副作用：同名路径两套写法、IDE 重命名/跳转失效。

---

## 4. 批次与逐文件变更清单

五个批次，**逐批独立提交、逐批验收、逐批可回滚**。批内顺序即建议执行顺序。

### 4.1 批次 A — `platform` 目录化（对应 A1 / B2）

| 新文件 | 搬入的完整定义块（源行区间） | 项可见性（子模块内） |
|---|---|---|
| `platform.rs`（保留） | 模块文档、`pub mod capture_hook;`、`mod` 声明、`pub use` 桥接、`#[cfg(test)] mod tests`（1157+ 原位保留） | 不变 |
| `platform/fonts.rs` | `trim_working_set`（win / non-win 两实现）、`setup_cjk_fonts`、`load_font_file`、`load_cjk_font_definitions`（18–300） | `pub` ×2 + 私有 ×2 |
| `platform/window.rs` | `impl PaletteApp { cursor_work_area, center_on_cursor, … }`（301–419，整块） | `pub(crate)` ×2 |
| `platform/shell.rs` | `run_as_admin`、`reveal_path_allowed`、`reveal_in_folder`、`is_allowed_open_url`、`open_path`（423–465、590–703） | `pub(crate)` ×4 + `pub(super)` ×1 |
| `platform/paths.rs` | `split_file_url`、`to_windows_path`、`strip_query_fragment`、`percent_decode`、`to_decoded_str`、`push_unique`、`file_url_candidates`、`resolve_file_url_to_path`、`is_unc_path`、`hex_byte`（467–583） | `pub(crate)` ×2 + 私有 ×8 |
| `platform/dwm.rs` | `SystemBackdrop` + `From` 实现、`apply_system_backdrop`、`set_immersive_dark`、`apply_window_chrome`、`set_window_border`、`system_accent_color`、`is_window_visible`、`wide`（707–952） | `pub` ×7 + `pub(super)` ×1 |
| `platform/autostart.rs` | `set_autostart`（win / non-win）（957–1016） | `pub` ×1 |
| `platform/lang.rs` | `system_ui_lang`（win / non-win）（1019–1037） | `pub` ×1 |
| `platform/cursor.rs` | `CURSOR_IDLE_HIDE`、`MouseHideScope`（含 `new` / `apply` / `pointer_engaged` / `release`）、`cursor_should_hide`、`Default`（1040–1155） | `pub` ×2 + `pub(super)` ×1 |

> 上表「项可见性」= **子模块内部**的声明可见性，**与对外可见性无关**：对外面由 `platform.rs` 的 `pub use` / `pub(crate) use` 桥接决定——13 个 `pub` 项仍为 `pub`、5 个 `pub(crate)` 项仍为 `pub(crate)`。**唯一例外** `file_url_candidates`（见 §10.2）。

**保持对外不变**：`platform.rs` 以 `pub use` / `pub(crate) use` 逐项再导出，`crate::platform::X` 与 `dd_gui::platform::X` 全部路径不变。

### 4.2 批次 B — `aggregator` 目录化（对应 A2）

| 新文件 | 搬入的完整定义块（源行区间） |
|---|---|
| `aggregator.rs`（保留） | 模块文档、`mod` 声明、`pub use` 桥接、`#[cfg(test)] mod tests`（857+ 原位保留） |
| `aggregator/model.rs` | `PROTOCOL_VERSION`、`HOST_VERSION`、`HOST_CUSTOM_EXT_ID`、`CUSTOM_ITEM_PREFIX`、`ExtItems`、`CollectResult`、`SourceSummary`、`SourceStatus` + `impl`（51–149） |
| `aggregator/env.rs` | `inject_websearch_env`、`EXT_CFG_ENV_PREFIX`、`ext_cfg_env_name`、`inject_ext_settings`（150–219） |
| `aggregator/scan.rs` | `ExtensionSources`、`is_trusted`、`pending_count`、`origin_of`、`same_dir`、`load_extension_sources`、`apply_owned_names`、`owned_sidecar_name_key`、`sidecar_extensions_dir`、`merge_scanned_dirs`、`merge_sidecar_scan`、`push_note`（220–500） |
| `aggregator/spawn.rs` | `ExtOutcome`、`collect_top_level`、`spawn_and_initialize`、`spawn_and_initialize_with_info`、`uses_sidecar_assessment`、`assess_for_spawn`、`spawn_and_initialize_with`、`detail_suffix`、`load_one`（502–725） |
| `aggregator/items.rs` | `flatten`、`to_panel_item`、`custom_command_items`、`category_label_for`（726–856） |

### 4.3 批次 C — `theme` 目录化（对应 A3）

| 新文件 | 搬入的完整定义块（源行区间） |
|---|---|
| `theme.rs`（保留） | 模块文档、`mod` 声明、`pub use` 桥接、`#[cfg(test)] mod tests`（713+ 原位保留） |
| `theme/tokens.rs` | 行高 / 圆角 / 搜索栏 / 列表排印 / 页脚键帽 / 设置按钮几何常量与 `ListMetrics`（18–130） |
| `theme/palette.rs` | `Palette` + `impl`、`toast_shadow`、`dialog_shadow`、右键菜单常量、`menu_shadow`、`overlay`（134–330） |
| `theme/backdrop.rs` | `TINT_CAP_*`、`panel_tint_cap`、`BackdropStyleConfig`、`RowFillProfile`、`backdrop_config`、`panel_tint_with_opacity`、`RowFills`、`row_fills`、`CARD_GLASS_ALPHA_*`、`card_fill`（340–510） |
| `theme/visuals.rs` | `Colorization` + `impl`、`tint_mix`、`mix_panel`、`tint_color`、`visuals`、`apply`、`apply_panel_tint`、`theme_preference`、`rgb`、`glass`（514–712） |

### 4.4 批次 D — 命名消歧（对应 C1 / C2 / C3）

| 动作 | 调用点 |
|---|---|
| `app/aggregate.rs` → `app/aggregate_job.rs` | `app/mod.rs`（`mod` 声明 + `pub use`）、`test_support.rs`、文件内 `mod tests` |
| `app/keys/navigation.rs` → `app/keys/page_keys.rs` | `app/keys.rs`（`mod` 声明） |
| `app/toast.rs` → `app/toast_state.rs` | `app/mod.rs`、`app/host_actions.rs`、`app/invoke.rs` |

`app/mod.rs` 的 `pub use aggregate::{spawn_aggregation, AggregatePayload}` 由 `aggregate_job` 再导出，**bin 侧 `dd_gui::app::spawn_aggregation` 路径不变**。

### 4.5 批次 E — 别名清理（对应 D）

| 动作 | 规模 |
|---|---|
| lib 内部 `dd_gui::` → `crate::` | 30 个文件 / 135 处（含 `#[cfg(test)]` 与文档注释内的路径） |
| 删除 `crates/dd-gui/src/lib.rs` 的 `extern crate self as dd_gui;` 及其说明注释 | 1 处 |
| `main.rs`（bin）的 13 处 `dd_gui::` **保持不动** | — |

**顺序理由**：批次 A–D 会产生新的 `dd_gui::` 引用行，故 E 必须最后做，避免重复劳动。

---

## 5. 不变式（红线）

1. **函数体、签名、文档注释一字不改**；只允许新增 `mod` 声明、`pub use` 桥接与 `use` 路径调整。
2. **对外路径与签名零变化**：`lib.rs` 的 `pub mod` 列表、各模块 `pub`/`pub(crate)` 可见性、所有被外部引用的符号路径全部保持。
3. **唯一允许的偏离**：搬入子模块的项按编译器要求改为 `pub(super)`/`pub(crate)`（与 O12 同例，编译器指示的最小可见性改动），逐条列在批次提交说明里。
4. **不新增、不删除任何符号**；`#[allow(dead_code)]` 标注项原样保留。
5. **不动** `Cargo.toml`、依赖、协议字段、清单 schema、`.gitignore`、`tools/`。
6. **不动** 任何 `dd-gui` 之外的 crate。
7. 每个新文件保留其承载定义块原有的文档注释块（`//!` 模块头按新文件职责**重写一句**，不复制原 `//!`）。

---

## 6. 验收判据

### 6.1 每批必跑

| 项 | 命令 | 预期 |
|---|---|---|
| 测试 | `export APPDATA="C:\Users\y7398\AppData\Roaming"` 后 `cargo test --workspace` | **627 passed / 0 failed**（`error 231` 批属环境瞬时限制，非回归） |
| 格式 | `rustfmt --edition 2021 --check <改动文件>` | 零差异（只查自己改的文件，不跑 `cargo fmt -p`） |
| 静态检查 | `cargo clippy -p dd-gui --all-targets -- -D warnings` | 零新增告警（`missing_const_for_thread_local` 为已知上游误报，记档不修） |
| bin 重链 | `cargo build -p dd-gui` | 通过（`cargo test` 不重链 bin，UI 改动必须显式构建） |

### 6.2 纯搬运证明

| 项 | 判据 |
|---|---|
| 移动识别 | `git diff -M --stat` 对被移动文件显示 `rename` 或高相似度 |
| 行数守恒 | 每批「新增行数 − 注释/`mod` 声明行」≈「删除行数」，偏差逐条解释 |
| 语义等价 | 拆分前后 `cargo test` 计数不变（627），无新增 `#[ignore]`、无新增 `#[allow]` |
| 体积 | `cargo build --release -p dd-gui` 产物对比**即时基线**（每批先 stash 本批改动实测 HEAD，见 §10.1）；纯搬运必须零漂移 |

### 6.3 真机冒烟（批次 D / E 后各一次；批次 A 已提前做一次，见 §10.3）

- 热键唤起 → 首屏列表 → 方向键选中 → Enter 执行 → Esc 隐藏
- 设置页：外观（材质 / 圆角）与扩展卡（信任行、启停开关）各走一遍
- `Ctrl+F` 进文件搜索页 → 输入 → 返回根页

> **⚠️ 自动化冒烟必须先读 `%APPDATA%\dd-run\config.json` 取真实热键**：代码默认是 Win+Alt+Space（`HOTKEY_MODS_DEFAULT`），但用户配置可改绑，本机实际为 **Ctrl+Space**（`hotkey_mods=2`）。按默认值发键会得出「热键失效」的错误结论。

---

## 7. 风险与回滚

| 风险 | 触发点 | 处置 |
|---|---|---|
| 子模块项不可见导致编译错误 | 批次 A/B/C | 按编译器提示逐项 `pub(super)`，记入该批提交说明；不改调用方 |
| `#[cfg(test)] mod tests` 引用了搬走的私有项 | 批次 A/B/C | 测试块**原位保留**在拆分后的根文件；缺失项经 `pub use` 桥接补齐 |
| 文档注释里的路径链接失效 | 批次 D/E | `rustdoc` 链接仅作注释，编译不校验；批次 E 一并把 `dd_gui::` 换成 `crate::` |
| 批次 D 遗漏调用点 | 重命名 | 重命名后 `cargo build -p dd-gui` 必然报错，编译即验证；外加 `git grep -n` 复核零残留 |
| 真机行为回归 | 全部 | 每批 `git` 独立提交，回滚粒度 = 一个批次 |

---

## 8. 不做清单（本轮）

| 项 | 理由 | 去向 |
|---|---|---|
| `app/mod.rs` 的 `PaletteApp` 95 字段拆分为子状态结构体 | 会动全部调用点与构造器，非纯搬运；需先出字段归属表 + 迁移映射 | 另立项（P5） |
| `state.rs` 改名为 `panel_state.rs`（C4） | 对外路径已稳定，改名需 re-export 桥接，收益不抵面广 | 待裁决 |
| B1 分层搬迁（扫描职责 `aggregator` → `dd-host`） | 涉及 crate 边界与 `dd-host` API 变更 | 另立项 |
| `dd-ext` / `dd-host` / `dd-run-cli` 的巨型文件 | 范围外（§2.2 已留证） | 后续批次 |
| 死代码删除、重复实现合并 | 未获授权；须先出删除清单逐条确认 | 另立项 |

---

## 9. 版本演进

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.0 | 2026-10-21 | 首版：体检取证、问题清单 A–D、五批次变更清单、不变式与验收判据 |
| v1.1 | 2026-10-21 | 批次 A 落地并实测回写；体积基线按 HEAD 口径修正（9,105,408 → 8,804,352 B） |
| v1.2 | 2026-10-21 | 核对修正 6 处数据缺陷：① 行数口径显式标注（§2 = 非空行 / §10.2 = 全行）；② §2.2 标题由「>1000 行」改为「≥800 行」、§1 目标由「8 → 1」改为「8 → 5」（原表述把 3 个 800–1000 行文件算成 >1000）；③ §3.1 A1 行区间按实测校正 3 处；④ §4.1「可见性」列统一写 `pub(super)` 与实现不符（公开项实为 `pub`），改为子模块内真实可见性并加注「对外由 `pub use` 桥接」；⑤ §4.2 `is_trusted` / `pending_count` 实际在 240/245 行（scan 区间），从 spawn 移入 scan；⑥ §4.3 `rgb` / `glass` 实际在 694/704 行（visuals 区间），从 palette 移入 visuals |
| v1.3 | 2026-10-21 | 批次 A 真机冒烟（自动化）完成并回写：§6.3 加「必须先读 config.json 取真实热键」警示；新增 §10.3 记录三项方法论坑（热键取值、Job 连带杀进程、DPI 虚拟化）与逐模块结论；顺带纠正版本号 0.2.1 与安装版字节一致事实 |

---

## 10. 执行记录

### 10.1 体积基线修正（2026-10-21）

原基线 9,105,408 B 取自 2026-09-24（`implementation.md` 安全加固后重打包）。此后至 HEAD 累计 **115 个提交**（含多条 `feat(gui)`），该数字已不代表当前代码。按「同源对拍」重定：

| 口径 | 体积（B） |
|---|---|
| HEAD（收起批次 A 改动后 `cargo build --release -p dd-gui`） | **8,804,352** |
| 批次 A 完成后同命令 | **8,804,352** |
| 差值 | **0** |

后续批次一律以此法取即时基线：`git stash push -u -- <本批路径>` → 构建 → `git stash pop` → 构建 → 比对。

### 10.2 批次 A — `platform` 目录化（已完成，2026-10-21）

| 文件 | 行数 | 承载内容 |
|---|---|---|
| `platform.rs`（保留） | 322 | 模块文档、`pub mod capture_hook`、8 条 `mod`、`pub use` 桥接、`#[cfg(test)] mod tests`（原样） |
| `platform/fonts.rs` | 293 | `trim_working_set`、`setup_cjk_fonts`、`load_font_file`、`load_cjk_font_definitions` |
| `platform/dwm.rs` | 252 | `SystemBackdrop` + `From`、`apply_system_backdrop`、`set_immersive_dark`、`apply_window_chrome`、`set_window_border`、`system_accent_color`、`is_window_visible`、`wide` |
| `platform/shell.rs` | 160 | `run_as_admin`、`reveal_path_allowed`、`reveal_in_folder`、`is_allowed_open_url`、`open_path` |
| `platform/cursor.rs` | 127 | `CURSOR_IDLE_HIDE`、`MouseHideScope` + `impl`/`Default`、`cursor_should_hide` |
| `platform/paths.rs` | 127 | `split_file_url` 等 10 个纯函数（`file_url_candidates` / `resolve_file_url_to_path` 在此） |
| `platform/window.rs` | 123 | `impl PaletteApp { cursor_work_area, center_on_cursor }`（Win / 非 Win 四实现） |
| `platform/autostart.rs` | 65 | `set_autostart`（Win / 非 Win） |
| `platform/lang.rs` | 22 | `system_ui_lang`（Win / 非 Win） |
| **新 8 文件合计** | **1169** | 拆分前 `platform.rs` 为 **1443 行（全行口径，含空行）**——即 §2.2 表中非空行 1352 + 91 行空行；此文件不含 `capture_hook.rs`（后者本就是独立子模块，未改动） |

**只允许的可见性改动（4 项，编译器要求的最小调整）**：

| 项 | 原 | 现 | 原因 |
|---|---|---|---|
| `paths::file_url_candidates` 的**重导出** | `pub(crate) use` | `#[cfg(test)] use` | 非 test 构建下 crate 内无调用者（唯一消费者是 `resolve_file_url_to_path` 同模块直调 + 测试块），保留 `pub(crate) use` 会产生 `unused_imports` 告警 |
| `shell::reveal_path_allowed` | 模块私有 `fn` | `pub(super)` | 测试块在父模块，跨模块访问 |
| `cursor::cursor_should_hide` | 模块私有 `fn` | `pub(super)` | 同上 |
| `dwm::wide` | 模块私有 `fn` | `pub(super)` | 仅被 `autostart::set_autostart` 使用，跨模块访问 |

**验收实测**：

| 项 | 结果 |
|---|---|
| `cargo test --workspace` | **627 passed / 0 failed**（与拆分前逐项一致，无新增 `#[ignore]`） |
| `rustfmt --edition 2021 --check`（9 个文件） | 零差异 |
| `cargo clippy -p dd-gui --all-targets -- -D warnings` | 仅 `capture_hook.rs:57` 的 `missing_const_for_thread_local`——**已知 clippy 1.96 上游误报**（该文件本批未改动，HEAD 同样报，代码已是 `const { … }` 形式），非回归。⚠️ **本机退出码为 101**（被该误报以 `-D warnings` 阻断），判定口径 = 「告警清单只含该条」，不按退出码判 |
| `cargo build -p dd-gui`（重链 bin） | 通过 |
| release 体积 | **8,804,352 B**，与 HEAD 差值 **0** |
| 代码行守恒（机械校验） | 原 1025 行代码行在新集合中**全部存在**；缺失项全为 `use` 行（按设计不参与比对）；新增 11 行 = `pub use` 续行 3 + `#[cfg(test)]` 3 + `#[cfg(windows)]` 1 + `cursor_should_hide` 因 `pub(super)` 超列宽被 rustfmt 折行 4 |

### 10.3 批次 A 真机冒烟（2026-10-21，自动化执行）

用户要求「全部你来操作」，故用 Python + ctypes + PIL 脚本全自动完成（`Add-Type` 被安全策略拦截，PowerShell 路线不可用）。

**三个方法论坑（过程记录，避免下批复现）**：

| # | 坑 | 现象 | 根因 | 处置 |
|---|---|---|---|---|
| 1 | **按错热键** | 发 Win+Alt+Space 后窗口始终 `visible=false`、位置仍在 −16384 屏外 | 代码默认值 ≠ 生效值：`config.json` 里 `hotkey_mods=2`(Ctrl) → 实际 **Ctrl+Space** | 冒烟前先读 `config.json`；外部探针 `RegisterHotKey` 返回 1409 即「已被占用」 |
| 2 | 子进程被连带杀掉 | 跨脚本查进程恒为「已死」 | 调用方与被调方同处一个 Job 对象；`CREATE_BREAKAWAY_FROM_JOB` 被拒（WinError 5） | 所有观测在**单脚本生命周期内**完成（启动→发键→查窗口→截图→读日志→清理） |
| 3 | DPI 虚拟化错配 | 裁出的图里面板只在右下角一角 | 系统 200%（窗口 DPI=192），Python 非 DPI-aware：`GetWindowRect` 给逻辑坐标，`ImageGrab` 给物理像素 3200×2000 | 裁剪坐标 `× (dpi/96)` |

**冒烟结论**：

| 模块 | 结论 | 取证方式 |
|---|---|---|
| `fonts.rs` | ✅ | 截图：中文、英文、图标（计算器/电源/齿轮）全部正常，无缺字方块 |
| `dwm.rs` | ✅ | `DwmGetWindowAttribute`：`SYSTEMBACKDROP_TYPE`=**2 Mica**、`WINDOW_CORNER_PREFERENCE`=**2 Round**、`USE_IMMERSIVE_DARK_MODE`=0（系统当前浅色）；`BORDER_COLOR` 返回 `E_INVALIDARG` 属正常 |
| `window.rs` | ✅ **逐像素吻合** | 日志 `唤起居中：光标屏工作区=(0,0 3200x1904) 目标=650x492 → (950, 460)`；实测窗口左上角物理 = (950,460)；`(1904−984)/2 = 460` |
| 热键链路 | ✅ | 探针 `Ctrl+Space=occupied` / `Win+Alt+Space=free`；唤起 → Esc 隐藏 → 再唤起往返均正常 |
| 键盘导航 | ✅ | `Down`×2 → 选中「睡眠」，选中态蓝条跟随，页脚提示由「计算」同步变为「打开设置」 |
| `lang.rs` | ✅ | `FollowSystem` 解析为中文界面 |
| `autostart.rs` | ✅ 只读核对 | 注册表 `HKCU\...\Run` 项存在（值指向已安装的 0.2.1）；**未做写入验证**以免改动用户设置 |
| `paths.rs` | ✅ | 10 个纯函数由单测覆盖（含在 627 内） |
| `cursor.rs` | ⚠️ 无法自动化 | 走 egui `set_cursor_icon`，`GetCursorInfo` 的 showing 位不变、GDI 截图也不含硬件光标 → 只能靠单测 |
| `shell.rs` | ⚠️ 未测 | 「在文件夹中显示」「打开 URL」会弹真实窗口，不适合自动化 |

**副产品（两个过时事实已纠正）**：仓库版本为 **0.2.1**（非 0.1.1）；本机 `D:\Program Files\dd-run\dd-run-0.2.1.exe` = **8,804,352 B**，与 target 产物**逐字节相同** —— 故本次冒烟结论同样代表用户实际运行的安装版。系统为 Win11 build 26300。
