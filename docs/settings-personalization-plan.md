# dd-run 设置 / 个性化 / 材料样式优化方案（参照 PowerToys CmdPal）

> **状态**：**B1–B4 已落地**（T1–T8 ✅；T9/T10 未做）｜ **版本**：v1.1 ｜ **最后更新**：2026-09-20
> **关联**：[settings.rs](../crates/dd-gui/src/settings.rs) · [settings_view.rs](../crates/dd-gui/src/ui/settings_view.rs) · [theme.rs](../crates/dd-gui/src/theme.rs) · [platform.rs](../crates/dd-gui/src/platform.rs) · [cmdpal-window-material-effects.html](../cmdpal-window-material-effects.html) · [cmdpal-ui-mockups.html](../cmdpal-ui-mockups.html) · [INDEX.md](./INDEX.md)
> **参照来源**：`microsoft/PowerToys` 仓库 `src/modules/cmdpal/`（`SettingsModel.cs` / `BackdropStyles.cs` / `BackdropStyleConfig.cs` / `AppearanceSettingsViewModel.cs` / `AppearancePage.xaml` / `BackdropControllerKind.cs`，main 分支）与 Microsoft Learn「Command Palette settings」文档。

---

## 0. 结论先行

| # | 优化点 | 方向 | 可行性 | 优先级 |
|---|--------|------|--------|--------|
| M1 | 材料配置**注册表化**（对齐 CmdPal `BackdropStyles`） | 结构重构 | ✅ 高（现有散落常量收口） | P1 |
| M2 | 新增 **Mica Alt** 材质档 | 补齐对齐 | ✅ 高（DWM 常量 4，现有回退链覆盖旧系统） | P1 |
| M3 | **不引入** Acrylic Thin | 对齐 CmdPal「隐藏不提供」 | —（刻意不做，§4） | — |
| P1 | **着色模式**（系统强调色 / 无 / 自定义色 + 强度） | 对齐 CmdPal `ColorizationMode` | ✅ 高（只动材质浓淡基色，不碰 Fluent token） | P2 |
| P2 | **背景图通道**（图片 + 适应/不透明度/着色） | 对齐 CmdPal `ColorizationMode::Image` | ⚠️ 中（一期做「图即背景」互斥语义；无实时模糊） | P3 |
| P3 | 设置页「**重置外观**」按钮 | 对齐 CmdPal 同款按钮 | ✅ 高 | P1 |
| B1 | **Esc 键行为**（当前 / 先清搜索再返回 / 始终隐藏） | 对齐 CmdPal `EscapeKeyBehavior` | ✅ 高（纯决策函数可测） | P1 |
| B2 | **退格键返回**（搜索框空时按 Backspace 返回） | 对齐 CmdPal `BackspaceGoesBack` | ✅ 高 | P1 |
| B3 | **单击激活开关**（关 = 单击选中、双击执行） | 对齐 CmdPal `SingleClickActivates` | ✅ 高（默认保持现状=开） | P2 |
| B4 | **界面动效开关**（下划线/悬停/骨架过渡） | 对齐 CmdPal `DisableAnimations` | ✅ 高（默认保持现状=开） | P2 |

**落地进度（2026-09-20）**：B1 = T1+T2+M4 ✅ ｜ B2 = T3+T4+T5 ✅ ｜ B3 = T6 ✅ ｜ B4 = T7+T8 ✅ ｜ T9（背景图）/ T10（另行立项）**未做**。三关实测：fmt 无差异 / clippy 0 告警 / `cargo test --workspace` **470 passed**（基线 465，+5）/ release exit 0。落地落点与验收记录见 [`implementation.md`](./implementation.md) §2 批次段。

**兼容性总结论**：全部新设置**默认值 = 现有行为**，旧 `config.json` 缺失字段一律回落默认（settings.rs 既有防御性解析），**零迁移、零行为变更**；DWM 不支持新档的系统走既有「回退不透明 + 控件置灰」链，无新风险面。

---

## 1. PowerToys CmdPal 参考要点分析

### 1.1 设置数据层（`Microsoft.CmdPal.UI.ViewModels/SettingsModel.cs`）

单文件 JSON 模型（`System.Text.Json` 源码生成，`WriteIndented` + 字符串枚举 + 大小写不敏感 + 宽容解析），与本项目 `config.json` 单行 JSON + 手工 serde + 防御性回落的哲学一致。与本方案相关的字段组：

| 组 | 字段（CmdPal） | 默认 |
|---|---|---|
| 主题 | `Theme`（Default/Light/Dark）、`ColorizationMode`（None/WindowsAccentColor/CustomColor/Image）、`CustomThemeColor` + `CustomThemeColorIntensity`(100) | Default / None |
| 材料 | `BackdropStyle`（Acrylic 默认 / Transparent(Clear) / Mica / MicaAlt；AcrylicThin 在 UI 中 `Collapsed` 隐藏）、`BackdropOpacity`(0–100，默认 100) | Acrylic / 100 |
| 背景图 | `BackgroundImagePath`、`BackgroundImageFit`(Fill/Stretch)、`BackgroundImageOpacity`(20)、`BackgroundImageBlurAmount`(0)、`BackgroundImageBrightness`(0)、`BackgroundImageTintIntensity`(0) | 空 |
| 行为 | `EscapeKeyBehaviorSetting`（ClearSearchFirstThenGoBack 默认 / AlwaysGoBack / AlwaysDismiss / AlwaysHide）、`BackspaceGoesBack`(false)、`SingleClickActivates`(false)、`ShowAppDetails`(false)、`DisableAnimations`(**true**)、`ShowSystemTrayIcon`(true)、`KeepPreviousQuery`(false)、`IgnoreShortcutWhenFullscreen`(true)、`AllowAltF4`(false) | 见左 |

> 注：CmdPal 的 `DisableAnimations` 默认为 **true**（默认关动画）；`BackdropOpacity` 默认 **100** 且 `Acrylic+100 → 视为不透明`（`AppearanceSettingsViewModel.EffectiveBackdropStyle`）——与本项目默认 40% 浓淡语义不同（§2.2 差距分析）。

### 1.2 个性化页结构（`Microsoft.CmdPal.UI/Settings/AppearancePage.xaml`，即用户截图页面）

```
个性化
├─ CommandPalettePreview（实时预览控件）+ [打开命令面板] [重置外观]
├─ 应用主题模式：ComboBox（系统默认/浅色/深色）
├─ 材料（SettingsExpander）：ComboBox（亚克力(默认)/透明/Mica/Mica Alt；AcrylicThin 隐藏）
│   └─ 不透明度：Slider 0–100（非 Mica 时才显示——Mica 不可调）
├─ 背景（SettingsExpander，ColorizationMode ComboBox）
│   ├─ 无 / 系统强调色（切入时自动把自定义色同步为系统色）
│   ├─ 自定义颜色：ColorPickerButton + 强度 Slider 1–100
│   └─ 图像：文件选择按钮 + 亮度(−100–100) + 模糊(0–50) + 适应(Fill/Stretch) + 着色强度(0–100) + [重置]
└─ 行为
    ├─ 单击激活（开关，默认关 = 单击选中、双击执行）
    ├─ 显示应用详细信息（开关）
    ├─ 退格键返回（开关，默认关）
    ├─ Esc 键行为（ComboBox，默认「先清除搜索内容，然后返回」）
    └─ 禁用动画（开关，默认开）
```

### 1.3 材料体系（`BackdropStyles.cs` 注册表 + `BackdropStyleConfig.cs`）

**最值得借鉴的结构**：所有样式集中注册在一张表里，每档声明**能力位**——

| 样式 | ControllerKind | 基础 Tint/亮度不透明度 | FixedOpacity | SupportsOpacity / Colorization / BackgroundImage |
|---|---|---|---|---|
| Acrylic | Acrylic（DesktopAcrylicKind.Default） | 0.5 / 0.9 | — | ✅/✅/✅ |
| AcrylicThin | Acrylic（Thin） | 0.0 / 0.85 | — | ✅/✅/✅ |
| Mica | MicaKind.Base | 0.0 / 1.0 | **0.96** | **❌（滑杆隐藏）**/✅/✅ |
| MicaAlt | MicaKind.BaseAlt | 0.0 / 1.0 | **0.98** | **❌**/✅/✅ |
| Clear（透明） | Solid（透明染色底） | 1.0 / 1.0 | — | ✅/✅/✅ |

`ComputeEffectiveOpacity(userOpacity)`：Mica 系**强制用 FixedOpacity**（用户滑杆不参与），Solid 系直接用用户值，模糊系 = 基础值 × 用户值。**UI 可见性由能力位驱动**（非 Mica 才显示不透明度滑杆）。

### 1.4 运行时模式（`AppearanceSettingsViewModel.cs`）

- **Effective\* 计算层**：设置 → 派生出 `EffectiveBackdropStyle / EffectiveThemeColor / EffectiveImageOpacity / EffectiveBackgroundImageSource` 等，预览与窗口共用同一份计算（窗口绑定与设置页预览零漂移）。
- **200ms 去抖 `Reapply()`**；系统强调色变更由 `UISettings.ColorValuesChanged` 监听实时刷新。
- 图片合成不透明度 = `(图透明度/100) × √(背景不透明度/100)`（平方根调整，视觉更平缓）。

### 1.5 对本项目可借鉴的四个模式

1. **材料配置注册表 + 能力位**（§1.3）——本项目目前是散落常量（`TINT_CAP_MICA/ACRYLIC`）+ 各处 switch，加档易漂移 → 见 M1/M2。
2. **着色模式枚举驱动子控件显隐**（§1.2 背景区）——本项目可直接复用到「着色模式」新增 → 见 P1。
3. **Effective\* 派生计算层**——本项目是 egui 即时模式，设置页修改**当帧**在全局面板生效，等价于「整个面板就是实时预览」→ **无需引入**该层，只借鉴其「派生函数集中」思想（tint 计算已是纯函数，继续扩大）。
4. **重置外观按钮 + 打开命令面板**——重置做（P3）；预览控件不做（等价于实时生效，§4）。

---

## 2. dd-run 现状盘点与差距分析

### 2.1 现状（取证自代码，详表见文末 §7）

- **设置 15 字段**（settings.rs:364-403）：theme / open_view / search_engines / backdrop / material_opacity / corner_pref / border_mode / hotkey_mods+vk / autostart / disabled_extensions / panel_size（隐式） / lang / density / search_apps。
- **设置页四栏目**（settings_view.rs:55-76）：外观（主题卡 / 材质与边框卡：材质三选 pill + 不透明度滑杆 + 圆角三选 + 边框三选 / 密度卡）、常规（open_view / 热键 / 自启 / 语言）、搜索（search_apps / 引擎）、扩展（启停管理）。
- **材质**：`SystemBackdrop::{None, Mica, Acrylic}`（platform.rs:649-658，DWM 常量 1/2/3）；不透明度 = **浓淡层 alpha**（`alpha = cap × pct/100`，云母 cap 0.75 / 亚克力 1.0，theme.rs:350-376）；圆角/边框 DWM 化；不支持时回退不透明 + 控件置灰（keys.rs:418-478）。
- **主题**：亮/暗/跟随系统三档 + Fluent 2 token 表（theme.rs:133-196）；**无自定义强调色**（浓淡基色恒 = 面板色 × 系统强调色，暗 8% / 亮 30%，theme.rs:467-479）。
- **行为**：Esc = 非 Root 返回、Root 隐藏（keys.rs:89-95，硬编码）；**无** Backspace 处理；行单击即执行（panel.rs:505,535）；界面动效 = 下划线 0.12s / 悬停 / 骨架过渡（无开关）；DWM 过渡恒禁（platform.rs:737-738）。

### 2.2 差距分析表（CmdPal 有 ↔ dd-run 无）

| CmdPal 项 | dd-run 现状 | 判定 | 去向 |
|---|---|---|---|
| 材料档 Acrylic/透明/Mica/MicaAlt | None/Mica/Acrylic 三档（命名/语义映射：None≈不透明、Acrylic≈亚克力、Mica≈云母；dd-run **无「透明」档**——其 material_opacity=0 的 None 即近似「透明色底」） | 补 MicaAlt | **M2** |
| 材料配置注册表 + 能力位 | 散落常量 + 各处 switch | 结构重构 | **M1** |
| AcrylicThin | 无 | 不做（CmdPal 自身也隐藏） | §4 |
| 不透明度（Mica 隐藏滑杆 + FixedOpacity） | dd-run 滑杆对 Mica 仍生效（浓淡层语义，非 DWM 不透明度） | **保留差异**：dd-run 的滑杆调的是浓淡层 alpha 不是材质不透明度，Mica 下同样有意义；不引入「Mica 隐藏滑杆」语义，但在控件 hint 上澄清口径 | §3.1 M4 |
| 着色模式（无/系统强调色/自定义色+强度） | 恒为系统强调色固定比例 | 补齐 | **P1** |
| 背景图通道（路径/适应/不透明度/着色/亮度/模糊） | 无 | 后期补齐（一期互斥语义，无实时模糊） | **P2** |
| 实时预览控件 | 即时生效即实时预览 | 不做 | §4 |
| 重置外观按钮 | 无 | 补齐 | **P3** |
| Esc 行为（4 选） | 硬编码返回/隐藏 | 补齐（3 选） | **B1** |
| 退格键返回 | 无 | 补齐 | **B2** |
| 单击激活 | 单击即执行（=CmdPal 开档） | 补开关（默认保持现状） | **B3** |
| 禁用动画 | 恒禁 DWM 过渡；界面过渡无开关 | 补「界面动效」开关（DWM 恒禁保持） | **B4** |
| 显示应用详细信息 | 无详情窗 | 另行立项（属功能特性，非设置项补齐） | §4 |
| 全屏忽略热键 / 突破三连击 / 低层 hook | 无 | 另行立项评估（需前台窗口探测） | §4 |
| 紧凑模式 / Dock / Toast 位置 / 唤起屏幕 | 无 | 布局二期，另行立项 | §4 |

---

## 3. 优化方案（按方面分章）

> 统一约束：新增设置一律走既有「枚举三件套 + 结构体字段 + Default + parse 回落 + 序列化 + 单测 + i18n + UI 控件 + 生效逻辑」十步链路（settings.rs 既有范式）；**默认值恒 = 现有行为**。

### 3.1 材料样式

#### M1（P1）材料配置注册表化（结构重构，先行）

- **内容**：新增 `Backdrop::config() -> BackdropConfig { dwm_kind: i32, tint_cap: f32, supports_opacity: bool, label_key: &str }` 单一来源，取代散落的 `TINT_CAP_MICA / TINT_CAP_ACRYLIC`（theme.rs:350-351）、pill 选项表（settings_view.rs:434-476）、`refresh_backdrop` 分支（keys.rs:442-468）三处各自硬编码。
- **可行性**：✅ 纯结构收口，行为零变更；三处消费点引用同一份配置，新增档（M2）只改一处。
- **无冲突**：✅ 不新增/不改任何运行时行为；`panel_tint_with_opacity` 签名不变（cap 改从 config 读）。
- **验收**：三关全绿 + 现有三档材质/滑杆/置灰行为与重构前逐项一致（真机对照）；`git diff` 仅为结构移动。

#### M2（P1）新增 Mica Alt 档

- **内容**：`SystemBackdrop::MicaAlt = DWMSBT_TABBEDWINDOW(4)`（platform.rs 枚举 +1）；`Backdrop::MicaAlt`（settings.rs 三件套）；pill 四选（无材质 / 云母 / **云母 Alt** / 亚克力）；tint cap 与云母同（0.75）；圆角/边框/置灰/回退链路不变。
- **可行性**：✅ DWM 常量级差异；Win10/旧 Win11 不支持时走既有 `hr≠0 → 回退不透明`（platform.rs:664-697 + keys.rs:469-477）。
- **无冲突**：✅ 旧 `config.json` 无该值 → 回落默认 Mica；新增枚举值不影响既有值的解析与序列化。
- **验收**：四档真机切换均可生效或可读回退；`settings.json` 往返含 mica_alt；三关 + release 构建 + dist 重打包冒烟。

#### M4（P1，口径澄清）不透明度滑杆语义注记

- **内容**：滑杆行 hint 从「不透明度」澄清为「**浓淡不透明度**（材质色层的浓淡强度，非窗口透明度）」；不引入 CmdPal「Mica 隐藏滑杆 + FixedOpacity」语义（dd-run 的滑杆对 Mica 同样有视觉意义）。
- **可行性/无冲突**：✅ 仅文案（text.rs 双语）。
- **验收**：i18n 两处文案生效；无行为变化。

### 3.2 个性化 / 主题

#### P1（P2）着色模式：系统强调色 / 无 / 自定义色 + 强度

- **内容**：新增 `ColorizationMode { SystemAccent（默认）, None, Custom }` + `custom_tint_color: [u8;3]`（默认首次取系统强调色快照）+ `custom_tint_intensity: u8`(0–100，默认 100)。只影响**材质浓淡基色**（`tint_color` → 改为 `tint_color_for(mode, accent, custom, intensity)` 纯函数）：None = 面板原色不掺强调色；Custom = 面板色 × 自定义色 × 强度。
- **落点**：theme.rs（纯函数 + 单测）、settings.rs（字段 + 解析）、settings_view.rs（材质卡新增「着色」行：模式 pill + 自定义取色按钮 + 强度滑杆，按模式显隐子控件）、keys.rs（应用点）；`text.rs` 双语。
- **可行性**：✅ egui 自带 color picker；计算均为纯函数。**刻意不动** Fluent `accent` token（选中/链接仍按 Fluent 系统强调色），也不动边框强调色（仍 `DwmGetColorizationColor`）——把变更面锁死在「面板浓淡基色」一处，杜绝主题系统级冲突。
- **无冲突**：✅ 默认 SystemAccent + 固定比例 = 当前行为；不透明（backdrop=None）下着色无效（材质才画浓淡层），无需联动；与主题三档、密度、边框三选互不耦合。
- **验收**：三模式切换即时生效并可读（面板色变化）；自定义色 + 强度持久化往返；None 下与「无材质」视觉一致；默认配置与改动前**像素级一致**（真机截图对照）；单测锚定 `tint_color_for` 三分支与 clamp。

#### P2（P3，后期批次）背景图通道

- **内容**：`background_image_path: Option<String>` + `background_image_opacity`(0–100，默认 20，对齐 CmdPal) + `background_image_fit {Fill, Stretch}` + `background_image_tint_intensity`(0–100)。**一期语义（互斥）**：设了背景图 = 该图即面板背景（材质行置灰提示「背景图生效中」），清除后恢复材质链路；**不做**亮度/模糊滑杆（实时模糊在 egui 成本高；如需模糊，加载期一次性 CPU 高斯模糊并入纹理缓存，列入二期评估）。
- **落点**：settings.rs、settings_view.rs（文件选择走系统对话框——复用 `rfd`? 当前无该依赖，一期可用文本输入 + 浏览提示，避免新依赖；二期再评 `rfd`）、platform.rs / 新 `ui/backdrop_image.rs`（解码 → 纹理缓存 → 面板底层绘制）、`icon_cache` 同款失效策略。
- **可行性**：⚠️ 中——解码/纹理缓存有现成模式（icons.rs / E2 的 image 依赖已在 dd-gui），面板底层绘制需要动 draw_panel 背景层（ui/panel.rs）与尺寸重算（Fill 裁剪逻辑）；透明窗口 + DWM 材质与自绘背景图的叠加语义需明确（一期互斥规避）。
- **无冲突**：✅ 默认 `None` → 现状；互斥语义下材质/边框/圆角链路只需「置灰 + 跳过 apply」，无分叉逻辑残留。
- **验收**：设图 → 面板底图按 Fit 显示、不透明度滑杆生效；清图 → 材质链路恢复且置灰解除；图片解码失败回落无图 + 负缓存；resize 时重裁剪；三关 + 真机。

#### P3（P1）「重置外观」按钮

- **内容**：外观栏底部按钮「恢复默认外观」+ 二次确认（复用 `confirm` 对话框既有组件），重置范围 = theme / backdrop / material_opacity / corner_pref / border_mode / density / colorization*(新增字段) —— **不动** 热键/语言/引擎/扩展/自启/尺寸。
- **可行性**：✅ `Settings` 增加 `reset_appearance()`（复用 Default 字段构造），逐个走既有 `apply_*` 即时生效 + 一次落盘。
- **无冲突**：✅ 纯组合操作，无新逻辑。
- **验收**：重置后字段全回默认且当帧生效；确认对话框取消无副作用；落盘往返。

### 3.3 设置 / 行为

#### B1（P1）Esc 键行为三选

- **内容**：`EscBehavior { GoBack（默认=当前）, ClearThenGoBack, AlwaysHide }`：①当前（非 Root 返回、Root 隐藏）；②先清搜索内容，然后返回（搜索框非空 → 清空；空 → 按 ①）；③始终隐藏面板。落点：keys.rs:89-95 改为读设置的**纯决策函数** `decide_esc(page_is_root, query_empty, behavior) -> EscAction`（可单测）。
- **可行性/无冲突**：✅ 默认 = 现状；与 `reset_on_show`、Ctrl+F、确认对话框 Esc 语义不冲突（对话框活跃时 Esc 仍先关对话框，既有守卫优先）。
- **验收**：三模式 × {Root, 非 Root} × {空, 非空} 决策矩阵单测全覆盖；真机三种行为走查；默认档与现状一致。

#### B2（P1）退格键返回

- **内容**：`backspace_go_back: bool`（默认 **false** = 现状）。开启后：嵌套页且搜索框为空时按 Backspace → 返回上一级（Root 无效）。
- **可行性**：✅ keys.rs 增加 `Key::Backspace` 分支（当前零处理，无冲突面）。
- **验收**：开关关 = 零行为变化；开 = 空框返回、非空框删字不受影响；单测 + 真机。

#### B3（P2）单击激活开关

- **内容**：`single_click_activation: bool`（默认 **true** = 现状，**刻意与 CmdPal 默认相反**以保持 dd-run 既有交互）。关 = 单击选中、双击执行（egui `resp.double_clicked()`）。
- **可行性**：✅ panel.rs:505/535 行点击处理处读设置分流；选中态/键盘导航不受影响。
- **无冲突**：✅ 默认开 = 现状；右键菜单/悬停选中逻辑不动。
- **验收**：两档真机走查（关档单击仅选中、双击执行、Enter 仍可执行）；单测决策函数。

#### B4（P2）界面动效开关

- **内容**：`ui_animations: bool`（默认 **true** = 现状）。关 = 下划线宽度过渡（B4 批次 `animate_value_with_time`）、行/控件悬停过渡、骨架脉冲等**装饰性过渡**直出终态。DWM 过渡恒禁（platform.rs:737-738）**不变**——开关只管 egui 侧动效，不放开 DWM 过渡（避免弹窗闪烁）。
- **可行性**：✅ 动效入口集中（`animate_value_with_time` 调用点 + 骨架脉冲参数），加开关参数即可。
- **验收**：关档下无任何过渡帧（逐场景对照）；默认开 = 现状；三关。

---

## 4. 不做项与理由

| 项 | 理由 |
|---|---|
| Acrylic Thin 档 | CmdPal 自身在 UI 中 `Collapsed` 隐藏（预览不佳）→ 跟随不引入 |
| CmdPal「Mica 隐藏不透明度滑杆 + FixedOpacity」语义 | dd-run 滑杆调的是**浓淡层 alpha**（对 Mica 同样有效），语义不同，不照搬；以 M4 文案澄清口径 |
| 设置页实时预览控件 | dd-run 即时生效即「整个面板 = 实时预览」；主题卡已有 ThemeThumb 缩略图，重复投入无收益 |
| 背景图亮度/模糊滑杆（一期） | 实时模糊在 egui 无低成本路径；一期互斥语义 + 可选加载期 CPU 模糊（二期评估） |
| 放开 DWM 过渡动画开关 | borderless 弹窗开过渡有闪烁风险；现有「恒禁」决策（platform.rs:737-738）保留 |
| 显示应用详细信息（ShowAppDetails） | 需要「详情窗」整套 UI 特性，属功能立项，不是设置项补齐 → 另行立项 |
| 全屏忽略热键 / 突破三连击 / 低层 hook | 需前台窗口/全屏探测与热键线程改造，属二期行为特性 → 另行立项评估 |
| 紧凑模式 / Dock / Toast 位置 / 唤起屏幕 / Alt+F4 | 布局与行为二期范围，本方案不覆盖 |
| 系统强调色实时监听（`WM_DWMCOLORIZATIONCOLORCHANGED`） | 需窗口子类化；现有「应用时按需读 + 唤起重读」已够用 → 后期候选 |

---

## 5. 任务清单

> 批次划分原则：同域改动合批、每批三关 + release + 真机 + 文档回写五处（INDEX / implementation / CHANGELOG / 本文档 / 关联文档）+ dist 重打包冒烟。

| # | 任务 | 目标 | 涉及文件 | 优先级 | 顺序 | 验收标准 |
|---|------|------|----------|--------|------|----------|
| ✅ T1 | 材料配置注册表化（M1） | `theme::backdrop_config()` + `Backdrop::ALL` 单一来源，三处硬编码收口 | settings.rs / theme.rs / keys.rs / settings_view.rs | P1 | 1 | 三关绿；三档材质行为与重构前逐项一致（真机对照） |
| ✅ T2 | Mica Alt 档（M2） | 材质四选：无/云母/云母Alt/亚克力 | platform.rs / settings.rs / settings_view.rs / theme.rs / text.rs | P1 | 2 | 四档真机生效或可读回退；config 往返；dist 冒烟 |
| ✅ T3 | Esc 行为三选（B1） | 决策函数 + 三档 dropdown | keys.rs / settings.rs / settings_view.rs / text.rs | P1 | 3 | 决策矩阵单测全覆盖；三档真机走查 |
| ✅ T4 | 退格键返回（B2） | 空框 Backspace 返回开关 | keys.rs / settings.rs / settings_view.rs / text.rs | P1 | 4 | 关=零变化；开=空框返回、删字不受影响 |
| ✅ T5 | 重置外观按钮（P3） | 外观栏底部重置（含二次确认） | settings.rs / settings_view.rs / app（confirm 复用） / text.rs | P1 | 5 | 字段回默认当帧生效；取消无副作用；落盘往返 |
| ✅ T6 | 着色模式三模式（P1 项） | SystemAccent/None/Custom + 自定义色 + 强度 | theme.rs / settings.rs / settings_view.rs / keys.rs / text.rs | P2 | 6 | 三模式即时生效；默认与改前像素级一致；`tint_color_for` 单测 |
| ✅ T7 | 单击激活开关（B3） | 开=现状 / 关=单击选中+双击执行 | panel.rs / settings.rs / settings_view.rs / text.rs | P2 | 7 | 两档真机走查；Enter/键盘不受影响 |
| ✅ T8 | 界面动效开关（B4） | 装饰过渡一键关闭 | app（动效调用点）/ settings.rs / settings_view.rs / text.rs | P2 | 8 | 关档过渡直出终态、开=现状（默认开）；DWM 过渡恒禁不受影响 |
| T9 | 背景图通道一期（P2 项） | 图即背景（互斥）：路径/适应/不透明度/着色 | settings.rs / settings_view.rs / ui/panel.rs / 新 ui/backdrop_image.rs | P3 | 9 | 设图/清图/解码失败回落/resize 重裁剪；材质行置灰与恢复 |
| T10 | 另行立项评估 | ShowAppDetails 详情窗 / 全屏忽略热键 / 强调色实时监听 / 亮度模糊二期 | — | P3 | 10 | 立项评审通过后单独立项，不属本方案执行范围 |

**建议批次**：B1 = T1+T2（材料批）→ B2 = T3+T4+T5（行为+重置批）→ B3 = T6（着色批）→ B4 = T7+T8（行为二期批）→ T9/T10 另议。

---

## 6. 全局验证口径（每批强制）

1. **三关**：`cargo fmt --all -- --check` 无差异 / `cargo clippy --workspace --all-targets` 0 告警 / `cargo test --workspace` passed 只增不减（§3.1 台账追加一行）。
2. **release 构建 + dist 重打包 + 分发级冒烟**（conformance 内置/示例 9 步、GUI 5s 存活、zip 成员校验）——凡动设置持久化或材质链路必做。
3. **兼容性回归**：用**改动前的 config.json** 启动 → 行为与改前一致（缺失字段回落默认的专项用例必加）。
4. **真机走查**：每批按任务验收标准逐条打勾；材质/着色类附截图对照。
5. **文档回写**：本文档状态推进（§5 任务行）+ `implementation.md` 批次段 + `INDEX.md` 行数重算 + `CHANGELOG.md` 新条目；docscan 失效链接 (none)。

---

## 7. 附：dd-run 现状取证要点（供实施期引用，均带行号）

| 面 | 关键位置 |
|---|---|
| Settings 15 字段与默认值 | settings.rs:364-403 / 414-434 |
| 防御性解析（缺失/未知/损坏回落默认） | settings.rs:479-590（单测 681-1130） |
| 设置页四栏目与卡片分发 | settings_view.rs:55-76 / 268-281 |
| 材质卡（材质 pill / 滑杆 / 圆角 / 边框） | settings_view.rs:380-619；滑杆 1764-1834 |
| 主题 token 与密度三档 | theme.rs:133-196 / 42-83 |
| 浓淡层公式（cap × pct/100）与基色 | theme.rs:350-376 / 467-479 |
| SystemBackdrop 枚举与 DWM 应用 | platform.rs:649-697；回退链 keys.rs:418-478 |
| Esc 当前行为 | keys.rs:89-95 |
| 行单击即执行 | ui/panel.rs:505 / 535 |
| 动效调用点（下划线/悬停/骨架） | ui/panel.rs（B4）、ui/widgets.rs、ui/states.rs |
| 新增字段十步链路范本 | 以 density 为例：settings.rs:270-309 + settings_view.rs:625-710 + text.rs |

---

## 8. 版本演进

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.0 | 2026-09-20 | 首版：CmdPal 参考要点分析 + 差距分析 + M/P/B 三方面优化方案 + T1–T10 任务清单（未改任何代码） |
| v1.1 | 2026-09-20 | **B1–B4 落地（T1–T8）**：材料注册表化 + Mica Alt + 滑杆口径文案；Esc 三档 / 退格返回 / 恢复默认外观；着色三档（系统强调色/无/自定义+强度）；单击激活与界面动效开关。470 passed / clippy 0 / release exit 0；**T9（背景图）/ T10 未做**，见 §4 |
