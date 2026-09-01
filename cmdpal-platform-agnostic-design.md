# PowerToys Command Palette（CmdPal）参考设计：以 Rust 为主的跨平台实现

> **文档性质**：从 `microsoft/PowerToys` 的 `src/modules/cmdpal`（Command Palette，简称 CmdPal）提炼出的**参考设计**。本文以 **Rust** 为实现目标（跨平台优先），在 §1–§7 抽象出与具体语言尽量解耦的契约/UI/宿主模型，§8 给出 Rust 参照实现。**本文不宣称"语言无关"**——Rust 是主目标，但抽象层尽量不绑定 Rust，供其他语言读者借鉴架构思路。
>
> **事实来源与核验**：本文所有架构、接口、交互模型均来自 PowerToys `main` 分支真实源码与官方文档，抓取日期 **2026-08-31**。具体出处见文末「参考来源」。CmdPal 目前为 **preview**，README 明确说明 v1.0 前扩展 API 仍可能出现 **breaking changes**，落地时请以你拉取时的实际代码为准。
>
> **符号约定**：✅ = 已核验事实；⚠️ = 需注意的偏差/风险；🔧 = 需读者在自己工具链中自行验证的设计点/验证点（含可执行的验证方法）。

---

## 1. 文档目的与适用范围

> **核验基准**：microsoft/PowerToys `v0.101.2362.0`（核验日期 2026-09-01）。本文所有"上游如此"的硬事实断言（接口名、`CommandResultKind` 成员、默认热键、§7 扩展清单等）以此为基准核验；上游仍在 preview，接口可能演进（见 §11 与 `docs/implementation.md` R6）。

CmdPal 是 PowerToys Run 的下一代实现，官方定位为"一站式启动器（launch _anything_）"。它的扩展接口 `Microsoft.CommandPalette.Extensions` **被官方明确设计为语言无关**：

> ✅ "designed to be language-agnostic. Any programming language which supports implementing WinRT interfaces should be able to implement the WinRT interface." —— `src/modules/cmdpal/README.md`

本文把 CmdPal 的**三件事**抽象为与具体语言尽量解耦的描述（Rust 为主目标，但抽象层不绑定 Rust）：

1. **UI / 页面设计**：命令面板的视觉结构、交互模型、页面类型。
2. **扩展契约**：宿主与扩展之间的接口与数据模型（语言无关——指 CmdPal 原生契约的事实属性，见上文 ✅ 引文）。
3. **宿主模型**：宿主如何发现、加载、缓存、跨进程调用扩展。

**本文不覆盖**：CmdPal 的具体 C#/WinUI 代码、MSIX 打包/商店分发、具体模糊搜索算法内部实现（仅给概念模型与 Rust 参照选型）。

---

## 2. 设计原则（CmdPal-Values）

✅ 来自 `src/modules/cmdpal/doc/CmdPal-Values.md`（作者 Mike Griese，2025-03）。任何参照实现都应把这些作为产品目标，而非仅作为功能清单：

| # | 原则 | 对实现的含义 |
|---|------|--------------|
| 1 | **It should be fun** | 交互要顺滑、直觉、有反馈；扩展 API 也要好上手 |
| 2 | **Start _anything_ here** | 不止启动应用，要能"做任意事"——靠扩展生态兜底 |
| 3 | **It is for everyone** | 不只给开发者/高级用户，社区共建 host 与扩展 |
| 4 | **不应为了用电脑而开浏览器** | 用户要的东西应在指尖（面板内）可达 |
| 5 | **Success is measured in disengagement** | 优化指标是"用户多快拿到结果并离开"，而非停留时长 |

🔧 落地点：把以下三项作为验收指标，并已在 §10 落项——"冷启动耗时"→ **A2**、"首次结果呈现时间"（输入后过滤帧耗时）→ **A3**、"键盘可达性覆盖率"→ **A11**。

---

## 3. 总体架构（平台无关）

CmdPal 采用 **宿主（Host）+ 多扩展进程（Extension）** 的隔离架构：

```
┌─────────────────────────────────────────────┐
│  Host（命令面板宿主）                          │
│  - 全局热键 → 唤起浮动面板                      │
│  - 根视图：聚合所有扩展的顶层命令               │
│  - 搜索/过滤、渲染列表与页面                    │
│  - 扩展发现、缓存、生命周期管理                 │
│  - 跨进程调用扩展（IPC）                        │
└───────────┬───────────────────┬──────────────┘
            │ IPC               │ IPC
   ┌────────▼──────┐    ┌───────▼─────────┐
   │ Extension A   │    │ Extension B ...  │   （各自独立进程）
   │ (独立进程)     │    │ (独立进程)        │
   │ 实现扩展契约   │    │ 实现扩展契约      │
   └───────────────┘    └─────────────────┘
```

✅ 关键事实：

- 每个扩展是**独立进程**，通过进程外 COM Server（Windows）与宿主通信；扩展间、扩展与宿主间相互隔离 → 单扩展崩溃不影响宿主。
- 宿主通过 **包目录（AppExtensionCatalog）** 或（未打包时）**注册表**发现扩展，见 §6.1。
- 扩展契约是**语言无关接口**（指 CmdPal 原生契约的事实属性，见 §1 ✅ 引文）：只要某语言能实现该接口并通过 IPC 编组，就能写扩展。本文实现目标为 Rust，故改用"子进程 + JSON-RPC"（MVP 默认，见 §6.2）表达同一意图（§8），因为这是非 Windows / 非 COM 环境下更自然的等价物。

---

## 4. UI / 页面设计（命令面板解剖）

✅ 来自 `src/modules/cmdpal/doc/command-pal-anatomy/command-palette-anatomy.md` 与 `README.md`。

### 4.1 默认唤起方式

⚠️ **文档不一致（已核验）**：`README.md` 写明当前默认绑定为 **`Win+Alt+Space`**；而 anatomy 文档的示例配图/文字使用 **`Win+Ctrl+.`**。以 README 的 `Win+Alt+Space` 为当前默认，示例文档中的 chord 为早期写法。你的参照实现可自选全局热键（见 §8.1）。

### 4.2 可视区域（Visual Regions）

| 区域 | 说明 | 平台无关抽象 |
|------|------|--------------|
| **根视图 Root View** | 面板首次打开时显示，是一个"特殊的 ListPage"，展示所有扩展的**顶层命令** ✅ | 宿主启动时聚合各 provider 的 `TopLevelCommands()` 渲染为首屏列表 |
| **搜索框 FilterBox** | 用户在此输入，实时过滤根视图/当前页结果 ✅ | 受控输入框；输入变化触发查询（见 §4.3 步骤 2） |
| **结果列表 List** | 当前页的 `IListItem[]` 渲染区 ✅ | 虚拟滚动列表，每项含 Icon/Title/Subtitle/Tags/Section |
| **详情/预览 ShowDetails** | `ListPage.ShowDetails` 控制是否显示选中项的详情面板 ✅ | 可选的右/下方面板，渲染 `IDetails`（文本/链接/标签） |
| **上下文菜单 MoreCommands** | 列表项的 `ICommandItem.MoreCommands`（右键或快捷键唤起）✅ | 飞出菜单，可嵌套子菜单（见 §5.6） |
| **页脚/按键提示** | anatomy 文档未命名此区域；实际 CmdPal 底部显示按键提示条 | 🔧 建议实现：底部显示 `↑↓ 选择 / Enter 执行 / Esc 关闭` 等提示 |
| **页面区 Pages** | 命令执行后可打开嵌套页（List/Detail/Form/Markdown；Grid 为 ListPage 渲染模式）✅ | 页面类型见 §4.5；页面栈与回退见 §4.3 |

### 4.3 导航模型（Navigation Model）

✅ 核验的交互流：

1. 打开 → 显示 **Root View**（聚合所有顶层命令的 ListPage）。
2. 在 **FilterBox** 输入 → 实时过滤根视图命令。
3. 选中某项 → 按 `Enter` 或点击执行其**默认命令**（`IInvokableCommand.Invoke`）；若该项是一个 `IPage`，则**进入嵌套页**。
4. 嵌套页内可继续搜索/选择；命令执行后通过 `ICommandResult.Kind` 决定宿主行为（关闭/隐藏不关/回首页/返回/保持打开/跳转页/弹 Toast/确认）——共 8 种，见 §5.2。
5. **返回**：`CommandResultKind::GoBack` 或 `GoHome` 驱动页面栈回退。⚠️ anatomy 文档未显式描述返回键，但 SDK 的 `CommandResultKind` 含 `GoBack`/`GoHome`，故返回导航由结果类型驱动。

🔧 键盘映射建议（参照 CmdPal 思路，自定义）：
- `Win+Alt+Space`（或自选）：唤起/隐藏面板
- `↑`/`↓` 或 `Tab`/`Shift+Tab`：在列表项间移动
- `Enter`：执行默认命令 / 进入页
- `Esc`：关闭或返回上一级
- 另外补充：上下文菜单快捷键由扩展通过 `ICommandContextItem.RequestedShortcut` 声明

### 4.4 列表项组成（ListItem Composition）

✅ 来自 SDK：`IListItem` / `ICommandItem` 字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `Icon` | `IIconInfo` | 图标（支持 URL / 字体图标 / 流） |
| `Title` | `String` | 主标题 |
| `Subtitle` | `String` | 副标题/描述 |
| `Tags` | `ITag[]` | 标签（用于分组或过滤提示） |
| `Section` | `String` | 所属分组名 |
| `Details` | `IDetails` | 详情面板内容（与 `ShowDetails` 配合） |
| `TextToSuggest` | `String` | 该项被选中后回填到搜索框的文本（用于续接查询） |

🔧 渲染建议：列表按 `Section` 分组；`Tags` 以 chip 形式展示；图标统一尺寸（CmdPal 用 16–24px 级别）。

### 4.5 页面类型（Page Types）

✅ 来自 `extensibility-overview` 与 SDK：

| 页面类型 | 用途 | 核心接口 |
|----------|------|----------|
| **ListPage** | 可搜索的选中项列表 | `IListPage`：`GetItems()`、`SearchText`、`PlaceholderText`、`ShowDetails`、`Filters`、`GridProperties`、`HasMoreItems`、`LoadMore()`、`EmptyContent` |
| **DetailPage** | 富内容（分段、标签、链接） | `IContentPage` + `IDetails` |
| **FormPage** | 用户输入表单（交互工作流） | `IFormContent`：`TemplateJson`/`DataJson`/`StateJson` + `SubmitForm(inputs, data)` |
| **MarkdownPage** | 渲染 Markdown | `IMarkdownContent`：`Body` |
| **Grid（渲染模式）** | 画廊/网格布局 | `ListPage.GridProperties`（`IGridProperties`）控制，**不是独立页面类型** |

✅ 页面类型共 **4 类**（List / Detail / Form / Markdown）；Grid 是 ListPage 的一种**渲染模式**（由 `GridProperties` 控制），故不计入独立页面类型（设计稿界面 07 即按此渲染）。

🔧 参照实现最小集：先实现 **ListPage + DetailPage**（覆盖绝大多数启动器场景），Form/Markdown/Grid 可按需追加。

### 4.6 UI 设计图

🔧 各界面（Root View / 搜索过滤态 / ListPage / DetailPage / FormPage / MarkdownPage / **Grid 渲染模式** / 上下文菜单 / Confirm+Toast / EmptyContent / Loading 共 11 屏）的**交互式设计稿**见同目录 [`cmdpal-ui-mockups.html`](./cmdpal-ui-mockups.html)——真实 DOM+CSS 组件（暗色主题，含选中/hover/焦点态与微交互，**支持键盘走查**），浏览器打开即可逐个浏览；页首附设计令牌（accent/panel/字体）供实现取用。

---

## 5. 扩展契约（语言无关接口）

> ✅ 以下接口签名来自 `src/modules/cmdpal/doc/initial-sdk-spec/initial-sdk-spec.md`（2026-08-28）。为平台无关可读性，已用伪接口语法重写（非 C#/WinRT 原貌），语义与字段名保持与原文档一致。
>
> 🔧 **措辞限定**：本节的"语言无关"指 **CmdPal 原生 WinRT 契约的事实属性**（见 §1 的 README ✅ 引文）；本文实现目标为 Rust，故 §8 用 Rust 表达同一契约，不代表本文承诺支持任意语言。

### 5.1 核心接口一览

| 接口 | 继承/要求 | 关键成员 | 语义 |
|------|-----------|----------|------|
| `ICommand` | `INotifyPropChanged` | `Name`, `Id`, `Icon` | 一切可执行/可导航单元的最小契约 |
| `IInvokableCommand` | `ICommand` | `Invoke(sender) -> ICommandResult` | 叶子命令：被选中时执行 |
| `ICommandResult` | — | `Kind: CommandResultKind`, `Args` | 执行后的"宿主应做什么" |
| `IPage` | `ICommand` | `Title`, `IsLoading`, `AccentColor` | 可导航进入的页（命令的一种） |
| `IListPage` | `IPage`, `INotifyItemsChanged` | `SearchText`, `PlaceholderText`, `ShowDetails`, `Filters`, `GridProperties`, `HasMoreItems`, `EmptyContent`, `GetItems() -> IListItem[]`, `LoadMore()` | 列表页 |
| `IDynamicListPage` | `IListPage` | `SearchText` 可写 | 由扩展自己控制过滤逻辑 |
| `IContent` | `INotifyPropChanged` | — | 内容基类 |
| `IFormContent` | `IContent` | `TemplateJson`, `DataJson`, `StateJson`, `SubmitForm(inputs, data)` | 表单页内容 |
| `IMarkdownContent` | `IContent` | `Body` | Markdown 内容 |
| `IContentPage` | `IPage`, `INotifyItemsChanged` | `GetContent() -> IContent[]`, `Details`, `Commands` | 内容/详情页 |
| `IListItem` | `ICommandItem` | `Tags[]`, `Details`, `Section`, `TextToSuggest` | 列表中的一项 |
| `ICommandItem` | `INotifyPropChanged` | `Command`, `MoreCommands[]`, `Icon`, `Title`, `Subtitle` | 列表项/命令项 |
| `IContextItem` | —（标记接口） | — | 上下文菜单项基类 |
| `ICommandContextItem` | `ICommandItem`, `IContextItem` | `IsCritical`, `RequestedShortcut` | 带快捷键/关键标记的上下文项 |
| `ICommandProvider` | `IClosable`, `INotifyItemsChanged` | `Id`, `DisplayName`, `Icon`, `Settings`, `Frozen`, `TopLevelCommands() -> ICommandItem[]`, `FallbackCommands() -> IFallbackCommandItem[]`, `GetCommand(id) -> ICommand`, `InitializeWithHost(host)` | 扩展向宿主暴露命令的入口 |
| `IExtension` | — | `GetProvider() -> ICommandProvider`（按 Learn overview） | 扩展根对象 |
| `IExtensionHost` | — | `ShowStatus(...)` 等（异步） | 宿主能力注入（状态栏、Toast 等） |

### 5.2 命令执行结果（CommandResultKind）

✅ 枚举值（决定宿主在命令执行后的行为）：

`Dismiss`(关闭面板) · `GoHome`(回根视图) · `GoBack`(返回上一级) · `Hide`(隐藏不关闭) · `KeepOpen`(保持打开) · `GoToPage`(跳转到某页，配 `IGoToPageArgs`) · `ShowToast`(弹提示，配 `IToastArgs`) · `Confirm`(需确认，配 `IConfirmationArgs`)

🔧 这是**平台无关的状态机核心**：你的宿主用这个枚举驱动页面栈与可见性，扩展通过返回不同 `Kind` 精确控制 UX，无需宿主硬编码。

### 5.3 数据模型（Data Model）

```
Extension
  └─ ICommandProvider
       ├─ TopLevelCommands()      → ICommandItem[]   （首屏聚合）
       ├─ FallbackCommands()      → IFallbackCommandItem[] （无匹配时的兜底）
       └─ GetCommand(id)          → ICommand          （按 id 取回真实命令）

ICommand 分为两类：
  (a) IInvokableCommand → Invoke() 执行，返回 ICommandResult
  (b) IPage            → 进入嵌套页（List/Detail/Form/Markdown/Grid）

IListItem（列表项）
  ├─ Command           （点击执行或进入页）
  ├─ MoreCommands[]    （右键上下文菜单，可嵌套）
  ├─ Icon / Title / Subtitle
  ├─ Tags[] / Section / Details / TextToSuggest
```

> 🔧 **命名澄清（已对齐 §8）**：`IFallbackCommandItem` 是 `FallbackCommands()` 返回的**兜底命令数据项**；决定 provider 是否视为 fresh 的"是否具备兜底能力"标记，本文统一称 `IFallbackProvider`（原文档曾用 `IFallbackHandler`）。两者概念不同，勿混。

### 5.4 命令发现与调用流程

✅ 来自 SDK spec + Learn overview：

1. 宿主发现扩展 → 解析 manifest → 实例化扩展对象（Windows 下为 `CoCreateInstance` COM 类）。
2. 取 `IExtension` → 调 `GetProvider()` 得 `ICommandProvider`。
3. 调 `InitializeWithHost(host)` 注入宿主能力（可选，异步）。
4. 调 `TopLevelCommands()` 拿到首屏命令项（可缓存，见 §6.3）。
5. 用户选中 → 若是 `IInvokableCommand` 则 `Invoke(sender)`；若是 `IPage` 则压栈进入。
6. `sender` 类型随上下文不同（顶层项 / 列表项 / 上下文菜单项）。
7. 含顶层 `IFallbackProvider` 能力的扩展，无论 `Frozen` 如何都被视为 **fresh**（见 §6.3）。

### 5.5 列表更新的跨进程约束（重要）

✅ SDK spec 明确警告：**WinRT 集合类型跨进程有害**（`IObservableVector` 跨进程不佳）。因此 CmdPal 不用增量集合，而用：

- `INotifyItemsChanged` 事件通知"集合变了"；
- 宿主随后调用 `GetItems()` **全量拉取**当前页项。

🔧 参照实现要点：IPC 协议不要流式推送整个列表，改为"变更事件 + 按需全量拉取"，否则在子进程/网络边界上既慢又易错。

### 5.6 上下文菜单（Context Menu）

✅ `ICommandItem.MoreCommands: IContextItem[]` 提供右键/快捷键飞出菜单；`ICommandContextItem` 可声明 `RequestedShortcut` 与 `IsCritical`（关键操作，如删除，需二次确认）。菜单可嵌套（子命令项再带 `MoreCommands`）。

---

## 6. 宿主模型（构建类 CmdPal 宿主需实现）

> 面向"要从零做类 CmdPal 宿主"的读者。以下把 Windows 特有的 AppExtensionCatalog/COM 抽象成**平台无关的发现与生命周期模型**。

### 6.1 扩展发现（Discovery）

✅ Windows 实现：打包应用在其 `.appxmanifest` 声明 `uap3:AppExtension`，`Name="com.microsoft.commandpalette"`，并在 `CmdPalProvider` 中指定 COM 类 CLSID；宿主用 `AppExtensionCatalog` 枚举。未打包应用写入注册表 `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\DevPal\Extensions`，宿主启动时一并枚举。

🔧 平台无关等价物（**MVP 默认 = 清单文件扫描**，其余为可选进阶）：
- **清单文件扫描（MVP 默认）**：在约定目录（如 `~/.config/<host>/extensions.d/*.json`）读取扩展清单（id、命令入口、显示名、图标）。最简单、零运行时、易调试。**dd-run 的具体目录与清单字段规范见 [`docs/manifest-schema.md`](./docs/manifest-schema.md)。**
- **进程注册（可选）**：扩展启动时向宿主的本地 socket/命名管道注册自身（类 LSP / 类 Flow Launcher 的"外部进程"模式）。
- **WASM 沙箱（可选）**：扩展以 WASM 模块形式直接被宿主加载（见 §8.1 的 `wasmtime` / `extism`），省去独立进程发现；隔离更强但需宿主实现沙箱运行时。

### 6.2 进程模型与隔离

✅ CmdPal 每个扩展独立进程、进程外 COM 通信；扩展崩溃隔离。
🔧 参照实现（**MVP 默认 = 独立子进程 + stdin/stdout JSON-RPC**）：每个扩展 = 独立子进程，通过 stdin/stdout JSON-RPC 通信；零沙箱运行时、跨平台直出、调试简单。WASM 实例为可选进阶：隔离更强、启动更快，但需宿主实现沙箱运行时。

### 6.3 缓存策略（frozen / fresh / stub）

✅ 来自 SDK spec（核心性能设计）：

- **frozen 扩展**（`ICommandProvider.Frozen=true`，助手库默认）：顶层命令列表不变，可缓存到磁盘；冷启动不实例化其进程。
- **fresh 扩展**（`Frozen=false`）：需常驻以实时更新（如媒体控制、动态快捷方式）；含顶层 `IFallbackProvider` 能力者一律视为 fresh。
- **stub 缓存**：冷启动先从磁盘读缓存的"命令桩"（仅数据，无活进程），作为首屏列表项；用户点击 frozen 桩项时再"复热"（`CoCreateInstance` + `GetCommand(id)`）。
- **最近 N 个保活（"microwaved"）**：最近激活的 N 个扩展进程保持 warm，跳过再次查找；超出 N 则释放 COM 引用、命令重新标为 stub。

🔧 参照实现落地点：
- 启动时并行：(a) 加载缓存桩 → 立即渲染首屏；(b) 懒加载 fresh 扩展。
- 用 LRU（容量 N）管理"warm"扩展集合。
- frozen 扩展的顶层命令 JSON 缓存到本地（带版本号，扩展升级即失效）。
- **桩复热的 Rust 等价**：点击桩项 → 拉起子进程 → 走协议 `get_command`（§8.2）取回真实命令；失败或超时则回退 stub 并报错（对应 §10 A6；崩溃场景见 A8）。
- **释放的 Rust 等价**：调协议 `close`、终止子进程，命令重新标为 stub（对应上文 ✅ 的"释放 COM 引用"）。

### 6.4 跨进程通信注意事项

✅ SDK spec 强调：
- 跨进程调用视为**异步**（宿主侧 `ShowStatus` 等标记为 async，提醒作者别当同步用）。
- 避免跨进程传集合类型，用"变更事件 + 全量 `GetItems()`"模式（见 §5.5）。
- 后台扩展线程无法用标准剪贴板，需宿主提供助手能力。

🔧 参照实现：IPC 层所有请求/响应都走 async；宿主暴露的能力（复制文本、显示 Toast、打开 URL）以异步消息提供。

### 6.5 设置与配置

✅ `ICommandProvider.Settings: ICommandSettings` 提供每扩展配置；CmdPal 还有隐藏 `InternalPage` 设置页（如自定义 Gallery feed URL）。
🔧 参照实现：宿主持有一份全局配置（热键、主题、缓存 TTL、gallery URL），每个扩展可读自己的配置段。

### 6.6 扩展 Gallery（可选）

✅ 来自 `doc/extension-gallery.md`：CmdPal 的"扩展商店"页从一个远程 `extensions.json` 拉取（`https://aka.ms/CmdPal-ExtensionsJson`），带本地磁盘缓存（feed TTL 4h、icon TTL 24h、HTTP 超时 15s）、`FromCache`/`UsedFallbackCache`/`RateLimited` 状态旗标，支持 `x-cmdpal://extensions/gallery/{id}` 深链。
🔧 参照实现：若要做"商店"，复用同样思路——单一 JSON feed + 磁盘缓存 + 条件 GET（ETag）；feed 条目含 `id/title/description/author/installSources[]/iconUrl`。注意：这是**可选**模块，MVP 不必做；**故 §8.1 不提供对应 crate 映射**，若日后实现再补。

---

## 7. 内置功能清单（CmdPal 自带扩展）

✅ 来自 `src/modules/cmdpal/ext/` 目录（**核验基准 `v0.101.2362.0`**，GitHub API 目录列表，2026-09-01 复核：**21 项，与下表 21 行逐一对应**）。下表为 CmdPal 自带扩展，可作为你宿主"应内置哪些基础能力"的参考。

> 🔧 **目录名前缀**：表中 `Ext.Xxx` 为简写，**真实目录名为 `Microsoft.CmdPal.Ext.Xxx`**（19 项带此前缀）；末两项 `ProcessMonitorExtension` 与 `SamplePagesExtension` **无前缀**，为真实完整目录名。

**平台列图例**：✅ **跨平台** = 逻辑与 OS 无关，一份代码三平台通用 · ⚙️ **平台相关** = 能力跨平台存在，但实现需按 OS 分路径（可移植，需适配层）· 🪟 **Windows 专属** = 依赖 Windows 独有 API/组件，**无跨平台等价物**

| 扩展（目录名） | 功能 | 平台 | 判定依据 |
|----------------|------|------|----------|
| `Ext.Apps` | 应用启动/列表 | ⚙️ | 应用枚举按 OS 分路径：Win 开始菜单 `.lnk`+PATH / macOS `/Applications` / Linux `.desktop`+PATH |
| `Ext.Calc` | 计算器 | ✅ | 纯表达式求值，无 OS 依赖 |
| `Ext.TimeDate` | 时间/日期 | ✅ | 日期时间格式化，标准库能力 |
| `Ext.System` | 系统命令（锁屏、关机等） | ⚙️ | 锁屏/关机/休眠各有平台命令：Win `shutdown.exe` / macOS `pmset`·`osascript` / Linux `systemctl` |
| `Ext.WebSearch` | 网络搜索 | ✅ | 拼搜索引擎 URL + 调宿主 `open_url` |
| `Ext.Shell` | Shell 命令 | ⚙️ | 解释器按 OS 不同：Win `cmd`/`powershell` / POSIX `sh` |
| `Ext.Indexer` | 文件索引搜索 | ⚙️ | Win Search Indexer / macOS `mdfind` / Linux 需自建索引 |
| `Ext.ClipboardHistory` | 剪贴板历史 | ⚙️ | 依赖各 OS 剪贴板监听 API，需适配层 |
| `Ext.Bookmark` | 书签 | ⚙️ | 浏览器书签文件路径各 OS 不同（Chrome/Edge/Firefox） |
| `Ext.Registry` | 注册表 | 🪟 | Windows 注册表 API，POSIX 无对应物 |
| `Ext.RemoteDesktop` | 远程桌面 | 🪟 | MSTSC / Windows RDP 客户端集成 |
| `Ext.WindowsSettings` | Windows 设置 | 🪟 | `ms-settings:` URI 方案 |
| `Ext.WindowsTerminal` | Windows 终端 | 🪟 | 依赖 Windows Terminal 及其配置 |
| `Ext.WindowWalker` | 窗口遍历 | 🪟 | Win32 `EnumWindows` / 窗口激活 |
| `Ext.WindowsServices` | Windows 服务 | 🪟 | Windows 服务控制管理器（SCM） |
| `Ext.WinGet` | WinGet 包管理 | 🪟 | `winget` CLI / COM API |
| `Ext.PerformanceMonitor` | 性能监视器 | 🪟 | Windows 性能计数器（PDH） |
| `Ext.PowerToys` | PowerToys 自身模块入口 | 🪟 | 依赖 PowerToys 本体存在 |
| `Ext.Actions` | 操作/动作命令 | ✅ | 通用动作框架，语义与 OS 无关 |
| `ProcessMonitorExtension` | 进程监视器 | ⚙️ | 进程枚举按 OS 分实现：Win Toolhelp / Linux `/proc` / macOS `libproc` |
| `SamplePagesExtension` | ⚠️ 示例扩展（非功能，供开发参考） | — | 演示用页面样例 |

**统计**：✅ 跨平台 4 · ⚙️ 平台相关 7 · 🪟 Windows 专属 9 · 示例 1 = **21**

🔧 参照实现 MVP 建议内置（**默认技术路径见 §6.1/§6.2**）：Apps（启动应用）+ Calc + System + WebSearch + Shell，其余作为可插扩展。⚠️ **MVP 这 5 项均为 ✅ 或 ⚙️，无一是 🪟 专属项**——可直接作为 dd-run 的 MVP 范围（见 [`docs/implementation.md`](./docs/implementation.md)）。⚠️ Apps 索引在跨平台下按 OS 分路径：Windows 枚举开始菜单 `.lnk` + PATH；macOS 扫描 `/Applications`；Linux 读 `.desktop` 与 PATH——该步为平台相关，非中立契约的一部分。

---

## 8. Rust 参照实现指南

> 🔧 本节为**设计级参照**，给出组件→crate 映射与协议/结构示意。**未在本环境编译验证**，请在你的 Rust 工具链中落地并编译确认。crate 版本请以 crates.io 当前为准。

### 8.1 组件 → crate 映射

| CmdPal 能力 | Rust 实现 | 说明 |
|-------------|-----------|------|
| 宿主浮动 UI 面板 | `egui` + `eframe`（或 `Slint` / `iced`） | egui 即时模式，出面板最快 |
| 全局热键 | Windows：`windows-sys`（`RegisterHotKey`）；macOS / Linux：`rdev` / `global-hotkey`（二者本身跨平台） | 原生实现，无需外部 AHK；`Win+Alt+Space` 仅为 Windows 默认键位，其他平台自选（见 §4.1） |
| 应用索引 + 模糊搜索 | 按 OS 分路径：Windows 枚举开始菜单 `.lnk` + PATH；macOS 扫描 `/Applications`；Linux 读 `.desktop` + PATH；匹配用 `skim` 或 `nucleo` | 与 §7 跨平台注记一致；已有 q7-rust-launcher 等验证 |
| 扩展 IPC（MVP 默认 JSON-RPC） | **JSON-RPC** 子进程（stdin/stdout，MVP 默认）；**WASM 沙箱**（`wasmtime` / `extism`，可选进阶） | 比 WinRT 接口更轻、更跨平台；JSON-RPC 零沙箱运行时、最简单 |
| 配置/状态 | `serde` + 本地文件 | — |
| 列表变更通知 | 事件 + 全量 `GetItems()` 拉取 | 对应 §5.5 跨进程约束 |

### 8.2 最小宿主骨架（设计示意，非编译产物）

```rust
// 仅表达结构，未经编译验证
struct Host {
    hotkey: HotKey,
    providers: Vec<ExtHandle>,   // MVP 默认：IPC 子进程句柄（见 §6.2）
                                 // 若把扩展做进同进程（非 MVP 默认），改用 Vec<Box<dyn CommandProvider>>
    warm_cache: LruCache<ExtId, RunningExt>,    // 最近 N 个保活
    stub_cache: StubStore,                       // frozen 命令桩（磁盘）
}

trait CommandProvider {          // 对应 §5.1 ICommandProvider
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn icon(&self) -> Option<Icon>;
    fn settings(&self) -> Option<Settings>;
    fn frozen(&self) -> bool { true }
    fn top_level_commands(&self) -> Vec<CommandItem>;
    fn fallback_commands(&self) -> Vec<FallbackCommandItem>;  // §6.3 判定 fresh 的依据
    fn get_command(&self, id: &str) -> Option<Box<dyn Command>>;
    fn initialize_with_host(&mut self, host: Arc<dyn HostHandle>);  // trait 对象须 Arc / Box 包裹
    // 对应 IClosable：宿主释放扩展时调用（Rust 亦可依赖 Drop 自动触发）
    fn close(&mut self) {}
    // 对应 INotifyItemsChanged：列表变化时通知宿主"集合变了"，宿主再全量 GetItems()
    fn on_items_changed(&self, cb: Box<dyn Fn() + Send + Sync>);
}

// 合并 §5.1 的 ICommand（Name / Id / Icon）与 IInvokableCommand（Invoke）
trait Command {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn icon(&self) -> Option<Icon>;
    fn invoke(&self, sender: Sender) -> CommandResult;  // 返回 CommandResultKind
}

// 与 §5.2 的 8 种 CommandResultKind 一一对应（设计示意，未编译验证）
enum CommandResult {
    Dismiss,               // 关闭面板
    GoHome,                // 回根视图
    GoBack,                // 返回上一级
    Hide,                  // 隐藏不关闭
    KeepOpen,              // 保持打开
    GoToPage(Page),        // 跳转页（配页面参数）
    ShowToast(String),     // 弹提示（配文本）
    Confirm(ConfirmationArgs), // 需确认（配确认参数）
}

// ── 契约相关类型（字段对齐 §4.4 / §4.5 / §5.1 / §5.4，设计示意）──────────
// 注：Icon / Tag / Details（§4.4）、ListPage / ContentPage / FormContent（§4.5）、
//     ContextItem（§5.6）、ConfirmationArgs（§5.2）、Settings / FallbackCommandItem（§5.1）、
//     CommandRef（§5.3）均按同名契约定义，此处省略以保 §8 轻量。
//     HotKey / ExtId / RunningExt / StubStore / LruCache / ExtHandle 为 plumbing 类型，
//     按 §6.2（IPC 子进程）与 §6.3（frozen / stub / LRU 保活）的语义自行定义。

struct CommandItem {                  // 对应 §4.4 ICommandItem / IListItem
    command: CommandRef,              // 点击执行或进入页（§5.3）
    icon: Option<Icon>,
    title: String,
    subtitle: String,
    tags: Vec<Tag>,
    section: Option<String>,          // 分组名
    details: Option<Details>,         // 详情面板（配合 ShowDetails）
    text_to_suggest: Option<String>,  // 选中后回填搜索框
    more_commands: Vec<ContextItem>,  // 右键上下文菜单（可嵌套，见 §5.6）
}

enum Sender {                         // 对应 §5.4 步骤 6：sender 随上下文变化
    TopLevelItem,
    ListItem { page_id: String, index: usize },
    ContextMenuItem { parent_id: String, command_id: String },
}

enum Page {                           // 对应 §4.5 的页面类型（Grid 为 ListPage 渲染模式，非独立类型）
    List(ListPage),                   // MVP 必做；Grid 布局由 ListPage.GridProperties 控制
    Detail(ContentPage),              // MVP 必做
    Form(FormContent),                // 按需
    Markdown(String),                 // 按需
}

trait HostHandle: Send + Sync {       // 对应 §5.1 IExtensionHost + §6.4 能力注入
    fn show_status(&self, text: &str);
    fn set_clipboard(&self, text: &str);   // 后台线程无法用标准剪贴板，须宿主代劳
    fn open_url(&self, url: &str);
}

// ── 扩展进程协议（JSON-RPC over stdin/stdout）────────────────────────────
// ⚠️ 此处为**精简示意**（字段名已省略参数细节）。完整规范——NDJSON 成帧、
//    JSON-RPC 信封、错误码、版本协商、生命周期状态机与超时——见
//    docs/protocol.md（dd-run Extension Protocol v1.0），以该文件为准。
// 约束：① 所有方法 async；② 不推增量集合，只走"变更事件 + 全量拉取"（§5.5、§10 A9）
//
// host → extension（宿主调扩展）：
//   request:  {"method":"initialize","protocol_version":"1.0",...}  // 握手，见 protocol.md §5.1
//   request:  {"method":"top_level_commands"}
//   response: {"commands":[{"id","title","subtitle","icon","section"}]}
//   request:  {"method":"fallback_commands"}         // §6.3 判定 fresh 的依据
//   response: {"commands":[...]}                     // 空数组 = 无兜底能力
//   request:  {"method":"get_items","page_id":"..."}          // 全量拉取 §5.5
//   response: {"items":[{"id","title","subtitle","icon","section"}]}
//   request:  {"method":"get_command","id":"..."}             // frozen 桩复热 §6.3
//   response: {"command":{...}}
//   request:  {"method":"invoke","id":"...","sender":"list_item"}
//   response: {"result":"Dismiss" | "Hide" | "GoBack" | ...}  // 8 种，见 §5.2
//   request:  {"method":"close"}                              // 对应 IClosable
//   response: {}                                              // 扩展随后自行退出
//
// extension → host（扩展请宿主代劳，§5.1 IExtensionHost / §6.4）：
//   notify:   {"method":"items_changed","page_id":"..."}      // 只发信号、不带数据
//   request:  {"method":"host/show_status","text":"..."}
//   request:  {"method":"host/set_clipboard","text":"..."}
//   request:  {"method":"host/open_url","url":"..."}
```

🔧 验证清单（在你环境）：`cargo build` 通过；`cargo clippy` 无错；全局热键可唤起/隐藏；点击 frozen 桩项能复热扩展并取回命令；`get_items` 全量拉取与 `items_changed` 事件联动正常；扩展经 `host/*` 反向调用宿主能力不阻塞 UI；关闭面板用 `Esc`。

### 8.3 工具链：彻底摆脱 VS / Windows SDK

🔧 依据前序调研（WebSearch 核验），以下为**设计预期/目标**，落地时须自行验证（数值随功能集与平台浮动）：
- **免 VS / Windows SDK（Windows 开发）**：纯 Rust GUI crate（egui 用 glow/wgpu）自带所需绑定，本地仅需 `rustup` + `cargo`。**验证方法**：在干净 Windows 环境（未装 VS）执行 `cargo build` 成功即通过。
- **跨平台产出原生 `.exe`（cargo-xwin）**：从 Linux/macOS 直接产出 Windows `.exe`，自动下载 MSVC CRT + SDK 库。**验证方法**：`cargo-xwin build --target x86_64-pc-windows-msvc --release` 成功产出 `target/x86_64-pc-windows-msvc/release/<bin>.exe`；CRT 由 cargo-xwin 自动拉取，无需本机 VS/SDK。
- **单文件静态 exe 体积 < 10MB（目标值）**：实际取决于 egui 后端、图标资源与特性开关。**验证方法**：`cargo build --release` 后 `ls -lh target/release/<bin>` 查看；用 `cargo bloat` 复核体积来源。
- **冷启动毫秒级（目标值）**：无 .NET 运行时、无 XAML 解析。**验证方法**：`hyperfine --warmup 3 './target/release/<bin> --bench-startup'` 或 Windows `Measure-Command` 计首屏渲染耗时；§10 A2 的 < 200ms 同为需实测的目标值。

🔧 这些预期与本文"低占用、快启动"目标契合；但均为**待验证目标，非已证事实**——请以你拉取的代码与工具链实测为准。

---

## 9. 设计要点速查（给实现者的 check-list）

- ✅ 宿主与扩展**进程隔离**，单扩展崩溃不影响宿主。
- ✅ 首屏 = 聚合所有 provider 的 `TopLevelCommands()`；输入即过滤。
- ✅ 命令执行结果用 `CommandResultKind` 状态机驱动页面栈（关闭/隐藏不关/回首页/返回/保持/跳转/Toast/确认）——8 种，与 §5.2 / §8.2 一致。
- ✅ frozen 扩展缓存命令桩、冷启动不实例化；fresh 扩展常驻；LRU 保活最近 N 个。
- ✅ 跨进程列表更新用"变更事件 + 全量 `GetItems()`"，**不**推增量集合。
- ✅ 扩展契约与语言解耦（CmdPal 原生属性，见 §1 ✅ 引文），本文以 Rust 实现：宿主侧定义接口，扩展侧实现，通过 IPC 编组。
- ✅ 页面类型至少覆盖 List + Detail；Form/Markdown/Grid 按需。
- ⚠️ 默认热键以 `Win+Alt+Space` 为准（README，**此为 CmdPal 在 Windows 的默认**），anatomy 示例的 `Win+Ctrl+.` 为早期写法；跨平台宿主在其他 OS 上自选键位（见 §8.1）。
- ⚠️ CmdPal 仍 preview，v1.0 前 API 可能 breaking change，落地以你拉取的代码为准。
- ✅ 一致性补强：§8.2 的 Rust `CommandResult` 已补齐至与 §5.2 一致的 8 种 `Kind`；`CommandProvider` 已补 `close()` 与变更事件钩子，对应 `IClosable` / `INotifyItemsChanged`；`IFallbackHandler` 已统一改名为 `IFallbackProvider`（见 §5.3/§6.3）。
- ✅ 完整性补强：§8.2 已补契约相关类型（`CommandItem` / `Sender` / `Page` / `HostHandle`）与双向协议方法（`get_items` / `items_changed` / `get_command` / `close` + extension→host 能力请求）；§10 已扩至 A1–A12，并覆盖 §2 承诺的三项指标（A2 / A3 / A11）。
- ✅ 成员级对齐：`CommandItem` 补 `command`；`Command` 补 `id()` / `icon()`（合并自 `ICommand` + `IInvokableCommand`）；`CommandProvider` 补 `icon()` / `settings()` / `fallback_commands()`（后者为 §6.3 判定 fresh 的依据）；`Host.providers` 改为 **IPC 句柄为主**，以承载 frozen / stub 状态机。

---

## 10. 验收标准（可量化、可核验）

| 编号 | 验收项 | 可核验方法 |
|------|--------|------------|
| A1 | 全局热键可唤起/隐藏面板 | 手动 + 自动化按键模拟 |
| A2 | 首屏在冷启动 < 200ms 内渲染（含从缓存读 frozen 桩）——**目标值，需实测** | 启动埋点计时（`hyperfine` / `Measure-Command`）；取决于 frozen 桩命中率与平台 |
| A3 | 输入查询后结果列表实时过滤（< 16ms/帧）——**目标值，需实测** | 帧耗时采样；取决于结果集规模与匹配库 |
| A4 | 选中 `IInvokableCommand` 执行并返回正确 `CommandResultKind` | 单测覆盖 8 种 Kind |
| A5 | 进入 `IPage` 后页面栈可 `GoBack`/`GoHome` 回退 | 导航单测 |
| A6 | frozen 扩展进程冷启动不启动，点击桩项后复热成功 | 进程监视器验证 |
| A7 | 最近 N 个扩展保活，超出后释放并重新标 stub | LRU 行为单测 |
| A8 | 扩展崩溃（kill 子进程）后宿主不退出、可恢复 | 故障注入测试 |
| A9 | 跨进程列表更新走"事件+全量拉取"，无增量集合推送 | 协议审查 |
| A10 | 内置扩展至少覆盖 Apps/Calc/System/WebSearch/Shell | 功能清单核对 |
| A11 | 核心路径（唤起/搜索/选择/执行/返回/关闭）**全程键盘可完成、无需鼠标** | 逐路径键盘走查，覆盖率目标 100%（对应 §2 落地点） |
| A12 | 协议双向方法齐全（host→extension 与 extension→host）；能力调用异步、不阻塞 UI | 协议审查 + UI 卡顿观察 |

---

## 11. 参考来源（出处均于 2026-08-31 核验）

> 🔧 **"核验"的边界**：下表的"核验"仅指**出处可达、内容已对照**；其中 **§8.3 的体积 / 冷启动等数值为待验证目标、非已证事实**（见 §8.3），须按该节给出的验证方法实测确认。

| 来源 | 路径/URL | 用途 |
|------|----------|------|
| CmdPal README | `src/modules/cmdpal/README.md` | 默认热键、语言无关 SDK 声明、preview 警告 |
| SDK Spec | `src/modules/cmdpal/doc/initial-sdk-spec/initial-sdk-spec.md`（Mike Griese, 2026-08-28） | 全部扩展接口签名、frozen/fresh、stub 缓存、跨进程约束 |
| UI 解剖 | `src/modules/cmdpal/doc/command-pal-anatomy/command-palette-anatomy.md` | Root View、FilterBox、ListPage、页面类型、导航 |
| 设计原则 | `src/modules/cmdpal/doc/CmdPal-Values.md`（2025-03） | 五条核心价值观 |
| 扩展商店 | `src/modules/cmdpal/doc/extension-gallery.md` | Gallery feed 格式与缓存策略 |
| 扩展模型（官方） | `https://learn.microsoft.com/windows/powertoys/command-palette/extensibility-overview` | 进程外 COM、manifest、页面类型、commands/pages |
| 内置扩展目录 | `src/modules/cmdpal/ext/`（GitHub API 目录列表，**2026-09-01 复核：21 项**） | 20 个真实扩展 + 1 示例；目录名前缀与平台标记见 §7 |
| Rust 工具链来源 | WebSearch：WinUI3 dotnet CLI 路径 / cargo-xwin 跨平台编译 | §8.3 依据；**仅核验出处，§8.3 的体积 / 冷启动数值为待验证目标、非已证事实** |

> ⚠️ 再次提醒：CmdPal 处于 preview，接口可能演进。本文为**设计参考**而非逐行代码移植；落地时请重新核验你所用版本的源码，并以本文 §10 验收标准做验证。
