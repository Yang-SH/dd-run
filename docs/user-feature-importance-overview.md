# dd-run 功能总览（按重要性）

> **状态**：生效中 ｜ **版本**：v1.2 ｜ **最后更新**：2026-11-24
> **关联**：[README.zh-CN.md](../README.zh-CN.md) · [文件搜索用户指南](./search.md) · [设置与个性化方案](./settings-personalization-plan.md) · [扩展开发指南](./extensions.md)

---

## 1. 结论与重要度排序

**dd-run 的核心价值是“全局唤起 → 一次搜索 → 键盘执行”**。应用启动、聚合搜索和文件搜索构成主路径；自定义直达、网页搜索、书签、计算器与文本片段构成高频增强；外观、扩展、更新和诊断属于支撑能力。

重要度按**日常使用频率、对核心流程的影响、用户配置成本**综合排序，不代表开发难度或代码体积。Windows 快速命令与系统组均属于 `com.ddrun.system`，不进入下表，保留位置见 [§8](#8-不纳入功能排序但保留代码的能力)。

| 重要度 | 功能 | 用户价值 | 主要入口 |
|---|---|---|---|
| P0 核心必备 | 全局唤起、托盘与单实例 | 随时打开命令面板，避免重复运行 | 默认 `Alt+Space`；Windows 托盘 |
| P0 核心必备 | 聚合搜索、拼音与命中高亮 | 一个输入框找到不同来源的结果 | 面板输入即搜 |
| P0 核心必备 | 应用启动 | 快速启动桌面应用、开始菜单项与 Steam 游戏 | 搜索应用名后 `Enter` |
| P0 核心必备 | 文件搜索与目录浏览 | 直接找文件、进入目录、打开或复制路径 | `Ctrl+F`、文件搜索入口、无匹配兜底 |
| P0 核心必备 | 纯键盘操作与页面导航 | 不离开键盘完成选择、执行、返回和关闭 | 方向键、`Enter`、`Esc`、`?` |
| P1 高频增强 | 自定义直达命令 | 用短关键词打开常用网站或本地路径 | 设置 → 常规 → 自定义命令 |
| P1 高频增强 | URL / 路径直接打开 | 粘贴链接或路径生成直达项，确认后执行 | 根页直接输入 |
| P1 高频增强 | 网页搜索与搜索引擎管理 | 从面板进入多个搜索引擎 | 无匹配兜底；设置 → 搜索 |
| P1 高频增强 | 浏览器书签 | 搜索并打开 Chromium 系浏览器书签 | 搜索书签或浏览器名 |
| P1 高频增强 | 计算器 | 计算表达式并复制结果 | 无匹配时选择 `= …` |
| P1 高频增强 | 文本片段 | 搜索常用文本并复制到剪贴板 | 设置 → 常规 → 文本片段 |
| P1 高频增强 | 使用频次、置顶与最近文件 | 让常用项更靠前，快速回到最近工作 | 自动排序；右键置顶；空查询首屏 |
| P1 高频增强 | 搜索历史 | 回填最近查询，减少重复输入 | `Ctrl+↑` / `Ctrl+↓` |
| P1 高频增强 | 窗口切换 | 搜索窗口标题并激活窗口 | 搜索窗口名或进程名（Windows） |
| P1 高频增强 | Shell 终端与命令执行 | 打开终端或在面板中执行命令 | 搜索“打开终端”；无匹配兜底 |
| P1 高频增强 | 命令热键 | 无需打开面板直接触发自定义命令 | 设置 → 常规 → 命令热键 |
| P2 效率配置 | 扩展加载、启停与信任管理 | 扩展能力，同时控制噪音、资源与风险 | 设置 → 扩展 |
| P2 效率配置 | 搜索范围与首屏行为 | 控制应用、Steam 游戏和空查询结果 | 设置 → 搜索 / 常规 |
| P2 个性化 | 主题、材质、背景与列表密度 | 调整可读性、信息密度和桌面融合度 | 设置 → 外观 |
| P2 个性化 | 语言、热键、位置与交互习惯 | 让面板适配个人工作方式 | 设置 → 常规 / 搜索 |
| P3 维护保障 | 更新检查与下载 | 发现新版本，下载并校验便携包 | 设置 → 常规 → 关于 |
| P3 维护保障 | 设置备份、迁移与恢复 | 备份偏好、迁移数据、误操作恢复 | 设置 → 常规 |
| P3 维护保障 | 日志、诊断报告与数据重置 | 排查问题并维护本地数据 | 设置 → 常规 |
| P3 运行保障 | 多平台与免安装分发 | 在 Windows 主力使用，在其他平台按能力降级运行 | 发布包与数据目录 |

## 2. 核心使用路径

### 2.1 全局唤起、托盘与单实例

- 默认使用 `Alt+Space` 显示或隐藏面板；可在设置中更改或恢复默认。已有用户配置不会被默认值切换覆盖。
- Windows 托盘左键切换面板，右键提供显示/隐藏、设置和退出。
- 重复启动时提示已有实例，避免多个面板同时运行。
- Linux Wayland 当前无法提供全局热键；托盘仅 Windows 提供。

实现：`crates/dd-gui/src/hotkey.rs`、`crates/dd-gui/src/tray.rs`、`crates/dd-gui/src/main.rs`、`crates/dd-gui/src/settings.rs`。

### 2.2 聚合搜索、拼音与排序

- 各来源结果流式汇入同一列表；单个扩展失败不会阻塞其他来源。
- 支持中文标题全拼、首字母和模糊匹配，例如 `jsq` 可命中“计算器”。
- 标题命中优先于副标题和标签命中；同分时使用频次更高者优先。
- 搜索词在标题中的命中片段会高亮显示。
- 无常规结果时才显示计算器、网页搜索、文件搜索等兜底项。

实现：`crates/dd-gui/src/fuzzy.rs`、`crates/dd-gui/src/state.rs`、`crates/dd-gui/src/app/aggregate_job.rs`、`crates/dd-gui/src/fallback.rs`。

### 2.3 应用启动

- 搜索应用显示名即可启动；支持拼音与首字母。
- Windows 聚合开始菜单快捷方式和应用目录，应用列表会定期刷新。
- 右键提供管理员运行、打开所在位置、复制路径和屏蔽应用；管理员运行与打开所在位置依赖 Windows，屏蔽应用当前也仅在 Windows 生效。
- Steam 游戏默认不混入搜索结果，可在设置 → 搜索中开启。
- macOS / Linux 已有应用枚举实现，但成熟度低于 Windows。

实现：`crates/dd-ext/src/builtins/apps.rs`、`crates/dd-ext/src/win_launch.rs`、`crates/dd-gui/src/app/ctx_menu.rs`。

### 2.4 文件搜索与目录浏览

- `Ctrl+F` 可从任意页面直接进入文件搜索；也可搜索“文件搜索”顶层入口，或选择无匹配时的搜索兜底项。
- Windows 文件检索以 Everything IPC 为主通道，`es.exe` 为可选回落；两者都不可用时提供引导信息。
- 支持 Everything 查询语法、真实 Shell 图标、打开文件、显示所在目录和复制路径。
- 文件结果页支持 `Enter` 进入目录、`Ctrl+Backspace` 删除路径末段、`Right` 补全选中目录名；目录浏览本身不依赖 Everything。
- 文件搜索 sidecar 为 Windows 专属，并与宿主进程隔离。

详细用法见 [文件搜索用户指南](./search.md)。

实现：`crates/dd-ext/src/bin/search.rs`、`crates/dd-gui/src/app/mod.rs`、`crates/dd-gui/src/app/keys/page_keys.rs`。

### 2.5 键盘操作与页面导航

- `↑` / `↓` 移动选择，`Enter` 执行，`Esc` 返回或关闭。
- `Ctrl+1..9` 直接激活可见列表对应行，`Ctrl+Enter` 打开所在位置或复制文本/链接。
- `Shift+F10` 或右键打开上下文菜单。
- `Ctrl+,` 打开设置，`Ctrl+F` 打开文件搜索。
- 根页空搜索框按 `?` 打开键位速查页。
- `Esc` 行为、空搜索框退格返回、单击激活和界面动效均可配置。

## 3. 高频效率功能

### 3.1 自定义直达与直接打开

| 能力 | 使用方式 | 边界 |
|---|---|---|
| 自定义直达命令 | 设置 → 常规 → 自定义命令；用名称或关键词搜索后执行 | 支持 URL 和本地路径、逗号分隔多关键词；不支持参数模板 |
| URL / 路径直接打开 | 根页粘贴 `http(s)` 地址、盘符路径或 UNC 路径，选择直达项后执行 | 不识别裸域名和显式 `file://`；本地路径打开目前仅 Windows |
| 转存为自定义命令 | 右键 URL / 路径直达项并转存 | 复用自定义命令的关键词与目标校验 |
| 命令热键 | 为自定义命令绑定全局组合键 | 最多 3 个；只能绑定已有自定义命令 |

实现：`crates/dd-gui/src/settings.rs`、`crates/dd-gui/src/aggregator/items.rs`、`crates/dd-gui/src/app/host_actions.rs`、`crates/dd-gui/src/app/keys/hotkey_capture.rs`。

### 3.2 网页搜索与书签

- 网页搜索提供 Google、Bing、Baidu、DuckDuckGo、GitHub 五个预设；默认只启用 Google。
- 可添加包含 `{q}` 的自定义 HTTP(S) 搜索模板，也可删除全部引擎。
- 书签搜索支持 Chrome、Edge、Brave、Vivaldi 的本地书签，最多读取 2000 条；不支持 Firefox。
- 打开网页或书签后面板是否关闭取决于具体动作：执行网页搜索会关闭，打开书签通常保持面板。

实现：`crates/dd-ext/src/builtins/websearch.rs`、`crates/dd-ext/src/builtins/bookmarks.rs`、`crates/dd-gui/src/settings/search.rs`。

### 3.3 计算器与文本片段

- 计算器支持四则运算、幂、括号、小数与科学计数法、18 个数学函数、进制字面量与进制转换、日期运算。
- 输入表达式后选择 `= …` 项，结果显示在 Toast 并复制到剪贴板。
- 文本片段用于保存常用文本；搜索片段后 `Enter` 复制，右键可再次复制或删除。
- 片段最多 200 条，单条最多 5000 字符；当前不提供全局 `;keyword` 展开、占位符或批量导入。

实现：`crates/dd-ext/src/builtins/calc.rs`、`crates/dd-gui/src/settings.rs`、`crates/dd-gui/src/app/clipboard_worker.rs`。

### 3.4 使用频次、置顶、最近文件与搜索历史

- 激活结果会累计使用频次，用于空查询首屏和同分结果的排序；可在设置中重置。
- 右键可置顶或取消置顶，最多 20 个键；置顶项在空查询首屏单独成组。
- Windows 空查询首屏可显示最近文件，最多 10 条。
- 搜索历史默认开启，最多保存 50 条；使用 `Ctrl+↑` / `Ctrl+↓` 回填，关闭记录会同时清空历史。
- 搜索历史独立于设置备份，不随设置导出迁移。

实现：`crates/dd-gui/src/usage.rs`、`crates/dd-gui/src/query_history.rs`、`crates/dd-gui/src/aggregator/items.rs`、`crates/dd-gui/src/state.rs`。

### 3.5 窗口切换

- 搜索可见顶层窗口的标题或进程名，`Enter` 激活；最小化窗口会先恢复。
- 最多列出 50 个窗口，过滤工具窗、幽灵窗口和 dd-run 自身。
- Windows 专属；提权窗口受系统权限限制，失败时显示 Toast，不强制切换。
- 当前图标为占位 glyph，不提供窗口内容预览。

实现：`crates/dd-ext/src/builtins/windows.rs`、`crates/dd-host/src/builtin.rs`。

### 3.6 Shell 终端与命令执行

- 搜索“打开终端”可启动新终端；Windows 使用 `cmd`，macOS / Linux 使用各自终端入口。
- 无常规匹配时，选择“运行 …”可在面板中执行命令并回显结果摘要。
- Windows 无头执行使用 `cmd /C`，超时为 3 秒；macOS / Linux 使用 `$SHELL -c`。
- 命令以当前用户权限运行，没有沙箱；破坏性命令会触发二次确认，确认门是提醒而非强制拦截。
- 终端和无头执行默认从用户主目录开始。

实现：`crates/dd-ext/src/builtins/shell.rs`、`crates/dd-ext/src/bin/shell.rs`。

## 4. 设置与个性化

设置统一通过 `Ctrl+,`、面板页脚齿轮或 Windows 托盘菜单进入，分为外观、常规、搜索、扩展四栏。

### 4.1 外观

| 分组 | 可配置项 | 说明 |
|---|---|---|
| 主题 | 跟随系统、亮色、暗色 | 即时切换并持久化 |
| 窗口材质 | 无材质、云母、云母 Alt、亚克力 | 仅 Windows DWM 有实际效果，失败时回退不透明 |
| 着色 | 系统强调色、无着色、自定义颜色与强度 | 控件始终显示；材质未生效或使用背景图时置灰 |
| 材质与轮廓 | 材质不透明度、圆角、面板边框 | 边框和系统圆角效果依赖 Windows |
| 背景图 | 路径、清除、填满/拉伸、不透明度、着色强度 | 需手动输入路径；设置背景图后材质链暂停 |
| 列表密度 | 紧凑、标准、宽松 | 联动行高、字号和图标尺寸 |
| 恢复默认外观 | 恢复主题、材质、背景、圆角、边框和密度 | 当前不会重置自定义着色模式、颜色和强度 |

界面字体来自系统字体栈，没有字体选择器；中文显示需要系统安装 CJK 字体。

实现：`crates/dd-gui/src/ui/settings_view/appearance.rs`、`crates/dd-gui/src/settings/prefs.rs`、`crates/dd-gui/src/platform/fonts.rs`。

### 4.2 常规

| 分组 | 可配置项 | 说明 |
|---|---|---|
| 打开面板时 | 显示所有应用 | 只改变空查询首屏，不限制关键词搜索 |
| 全局热键 | 更改、恢复默认 | 注册成功才保存；默认 `Alt+Space` |
| 开机自启 | 开关 | 仅 Windows 写入当前用户启动项 |
| 语言 | 跟随系统、简体中文、English | 覆盖界面、托盘和扩展显示文案 |
| 按键与交互 | `Esc` 行为、退格返回、单击激活、界面动效 | 用于适配不同操作习惯 |
| 预热容量 | 1–16，默认 8 | 平衡扩展响应速度与常驻内存 |
| 唤起位置 | 鼠标所在屏居中、记住上次位置 | 完整位置记忆仅 Windows |
| 个人效率数据 | 自定义命令、文本片段、命令热键 | 详见 [§3.1](#31-自定义直达与直接打开) 和 [§3.3](#33-计算器与文本片段) |
| 设置维护 | 导出、导入并覆盖、从上次备份恢复 | 导入保留本机热键、尺寸、位置和自启状态 |
| 使用数据 | 重置 | 只清除使用频次，不清搜索历史、片段或自定义命令 |
| 关于与更新 | 检查更新、下载新版、启动时检查开关 | 下载只处理 Windows portable zip，不自动安装或替换程序 |
| 诊断 | 打开日志目录、导出诊断报告 | 打开目录仅 Windows；报告包含完整设置，分享前应检查敏感内容 |

实现：`crates/dd-gui/src/ui/settings_view/general.rs`、`crates/dd-gui/src/app/keys/settings_actions.rs`、`crates/dd-gui/src/settings.rs`。

### 4.3 搜索

| 分组 | 可配置项 | 说明 |
|---|---|---|
| 搜索范围 | 搜索应用、搜索 Steam 游戏 | Steam 识别目前仅 Windows 接线 |
| 搜索引擎 | 启用预设、添加自定义、删除 | 离开设置页后重新聚合生效 |
| 搜索历史 | 记录开关、清空历史 | 默认开启；关闭即清空 |

实现：`crates/dd-gui/src/ui/settings_view/search.rs`、`crates/dd-gui/src/settings/search.rs`。

### 4.4 扩展

- 每个扩展可独立启停；变更在离开设置页重新聚合后生效。
- 可查看来源、运行状态、失败原因、信任状态、签名与清单错误。
- 未信任扩展需要明确允许或阻止；发行方签名有效时可免重复同意，用户明确拒绝优先。
- 连续崩溃熔断后可手动重试。
- 应用扩展提供屏蔽名单编辑，但当前名单消费仅 Windows 生效。
- 没有扩展商店、界面安装/卸载或一键打开扩展目录入口。

内置扩展在宿主进程内运行；第三方扩展和文件搜索 sidecar 使用独立进程，并通过缓存、懒加载和容量限制控制开销。

实现：`crates/dd-gui/src/ui/settings_view/extensions.rs`、`crates/dd-host/src/trust.rs`、`crates/dd-gui/src/ext_client.rs`、`crates/dd-gui/src/ext_inprocess.rs`。

## 5. 平台、数据与分发

### 5.1 平台能力矩阵

| 能力 | Windows | macOS | Linux |
|---|---|---|---|
| 核心搜索、应用启动、计算器、网页搜索、书签、片段 | 支持 | 支持 | 支持 |
| 托盘、开机自启 | 支持 | 不支持 | 不支持 |
| DWM 材质、系统圆角与材质边框 | 支持 | 不适用 | 不适用 |
| 全局热键 | 支持 | 已实现，真机验收进行中 | X11 支持；Wayland 不支持 |
| Everything 文件搜索 sidecar | 支持 | 不随包提供 | 不随包提供 |
| 窗口切换 | 支持 | 空实现 | 空实现 |
| 本地路径打开、资源管理器定位、管理员运行 | 支持 | 仅 Windows | 仅 Windows |
| 自定义背景、主题和列表密度 | 支持 | 支持 | 支持 |
| 跟随系统语言 | 按系统语言 | 当前回退中文 | 当前回退中文 |

Windows 是主力支持平台；macOS / Linux 为免安装预览版，不能把“可编译、可运行”理解为所有能力同等成熟。

### 5.2 数据目录

- Windows：`%APPDATA%\dd-run`
- macOS：`~/Library/Application Support/dd-run`
- Linux：`$XDG_CONFIG_HOME/dd-run`，缺省为 `~/.config/dd-run`

主要数据包括 `config.json`、`usage.json`、`history.json`、`extensions.d/`、`cache/` 和 `logs/`。

### 5.3 免安装分发

- Windows 发布包包含宿主 exe 与 `extensions.d/` 文件搜索 sidecar，解压即用，无安装器。
- macOS / Linux 提供单二进制与运行时依赖说明的 tar.gz。
- “便携”指分发形态；当前没有把用户数据目录跟随 exe 的官方 portable-mode。

## 6. 更新、备份与诊断

| 能力 | 行为 | 注意事项 |
|---|---|---|
| 检查更新 | 通过系统 `curl` 读取最新 Release，与当前版本比较 | 需要网络；手动检查始终可用 |
| 启动时检查 | 可选，默认关闭 | 当前开关缺少明确的可见文字标签 |
| 下载新版 | 下载 Windows portable zip 与 `SHA256SUMS`，校验后定位文件 | 只下载和校验，不安装、不替换运行中的 exe |
| 设置导出 | 写入数据目录的 `dd-settings-backup.json` | 剥离自启、尺寸和位置等机器态 |
| 导入覆盖 | 整体校验备份后应用字段级容错 | 保留本机主热键、命令热键、自启、尺寸和位置 |
| 备份恢复 | 可从滚动备份环恢复 | 配置发生自定义命令变化时轮转旧配置 |
| 日志 | stderr 与文件双后端，滚动保存 | 可通过 `DDRUN_LOG` 调整级别 |
| 诊断报告 | 输出版本、系统、设置、扩展、信任、失败清单与日志尾部 | 包含完整设置，分享前需检查片段、命令目标和路径 |
| 使用数据重置 | 清除 `usage.json` 中的频次数据 | 不影响搜索历史、片段和自定义命令 |

实现：`crates/dd-gui/src/app/update_check.rs`、`crates/dd-gui/src/update_check.rs`、`crates/dd-gui/src/file_log.rs`、`crates/dd-gui/src/app/diagnostic.rs`、`crates/dd-gui/src/settings.rs`。

## 7. 键位速查

| 键位 | 作用 |
|---|---|
| `Alt+Space` | 默认显示/隐藏面板；可自定义 |
| `Ctrl+,` | 打开设置 |
| `Ctrl+F` | 进入文件搜索 |
| `↑` / `↓` | 移动选中项 |
| `Tab` / `Shift+Tab` | 环绕移动选中项 |
| `Home` / `End` | 跳到首项/末项 |
| `PgUp` / `PgDn` | 列表翻页 |
| `Ctrl+1..9` | 激活可见列表第 N 行 |
| `Enter` | 执行选中项 |
| `Ctrl+Enter` | 打开所在位置或复制文本/链接 |
| `Shift+F10` / 右键 | 打开上下文菜单 |
| `Ctrl+↑` / `Ctrl+↓` | 回填更旧/更新的搜索历史 |
| `Ctrl+Backspace` | 文件目录页返回上级 |
| `Right` | 文件目录页补全选中目录名（进入目录按 `Enter`） |
| `Esc` | 返回上级或关闭面板，行为可配置 |
| `?` | 根页空搜索框打开键位速查 |

## 8. 不纳入功能排序但保留代码的能力

> 本节仅登记**文档排除边界与代码保留位置**。本次没有删除、隐藏或停用任何代码，也没有改变软件行为。

已确认排除范围：Windows 快速命令与系统组都是 `com.ddrun.system` 内的功能，本次仅将这一扩展移出功能排序；窗口切换器与 Shell 不属于排除对象。

| 排除项 | 原能力 | 保留实现 | 主要符号 |
|---|---|---|---|
| Windows 快速命令 / 系统组（System） | 锁屏、睡眠、关机、重启、注销；Windows 设置、回收站、音量与媒体 | `crates/dd-ext/src/builtins/system.rs` | `spec`、`commands`、`WIN_SETTINGS`、`handle_get_items`、`handle_invoke` |
| 内置注册与运行接线 | 保证 System 仍可被宿主发现和调用 | `crates/dd-ext/src/builtins/mod.rs`、`crates/dd-host/src/builtin.rs` | `builtin_specs`、`BUILTINS`、`builtin_registrations`、`merge_builtins` |
| System 独立可执行薄壳 | 保留 System 单独运行入口 | `crates/dd-ext/src/bin/system.rs` | `main` |

**不排除项**：窗口切换器（`com.ddrun.windows`）保留在 [§3.5](#35-窗口切换)；Shell 终端与命令执行（`com.ddrun.shell`）保留在 [§3.6](#36-shell-终端与命令执行)。

若未来要真正从软件中移除，需要同时处理扩展注册、设置页启停列表、聚合结果、缓存与测试，不能只删除文档或某一个 `spec()`。

## 9. 已知边界与阅读提醒

- **默认热键口径**：当前代码与 README 中英均为 `Alt+Space`。
- **置顶排序**：置顶项会单独成组，但空查询随后仍按使用频次稳定排序，因此“置顶组内严格保持添加顺序”不是当前最终显示保证。
- **恢复默认外观**：当前不重置着色模式、自定义颜色和着色强度。
- **更新下载**：只支持 Windows portable zip 的下载与 SHA256 校验，不提供自动安装或自替换。
- **诊断隐私**：诊断报告包含完整设置，不应在未检查的情况下直接公开上传。
- **平台降级**：部分控件在非 Windows 仍可见，但材质、自启、本地路径动作、位置记忆等不会产生对应效果。
- **能力未提供**：当前没有扩展商店、剪贴板历史、云同步、账号体系、移动端或 Web 端。

## 10. 实现与延伸阅读索引

| 主题 | 权威实现/文档 |
|---|---|
| 产品概览 | [README.zh-CN.md](../README.zh-CN.md) |
| 当前功能增量背景 | [ALTRun 对照优化方案](./altrun-对照优化方案-2026-11-21.md) |
| 设置与个性化 | [设置与个性化方案](./settings-personalization-plan.md) |
| 文件搜索 | [用户指南](./search.md) · [技术方案](./search-file.md) |
| 扩展协议与开发 | [扩展开发指南](./extensions.md) · [协议](./protocol.md) · [清单 schema](./manifest-schema.md) |
| 安全交互 | [安全审计与修复记录](./security-audit-2026-09-23.md) |
| 平台与遗留验收 | [未完善事项清单](./未完善事项清单-2026-10-25.md) |
| 功能实现取证 | `crates/dd-gui/src/`、`crates/dd-ext/src/builtins/`、`crates/dd-ext/src/bin/search.rs`、`crates/dd-host/src/` |

---

## 版本演进

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.2 | 2026-11-24 | 默认热键提醒同步：README 已修正为 `Alt+Space`，删除残留旧口径 |
| v1.1 | 2026-11-24 | 修正核对发现的 5 处明确错误：右键动作平台归属、URL/路径直达执行语义、着色控件显示方式、按键与交互栏目归属、`Right` 键语义 |
| v1.0 | 2026-11-24 | 首次按用户口径整理全部可见功能；将 Windows 快速命令/系统组移出功能排序并登记代码保留位置；不改功能代码 |
