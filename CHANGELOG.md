# 更新日志（Changelog）

本文件记录 dd-run 的版本变更。

## [Unreleased]

### 新增（文件搜索 v3.3 P1：上下文菜单三动作，协议 v1.0 零改动）

- 文件结果项新增 **`more_commands` 上下文菜单**（§8.1）：右键 = 打开（默认）/ **显示所在目录**（`files.reveal.{pid}`，扩展侧 `explorer /select` 经 `raw_arg` 规避引号坑）/ **复制路径**（`files.copy.{pid}`，Toast + `host/set_clipboard`）。
- 宿主右键菜单支持渲染扩展声明的 `more_commands`（`PanelItem` 透传），激活时以 `sender=context_menu` + `context.selected_item_id` 回调 `invoke`；扩展自带动作时 GUI 静态类别映射让位（避免重复）。
- `spec()` 与 sidecar manifest（`examples/extensions.d/com.ddrun.filesearch.json`）capabilities 同步追加 `host/set_clipboard`（能力前置 §7.4：未声明将被宿主回 `-32601`），启动时一致性断言覆盖。

### 修复

- 引导项 `files.hint` / `files.guide` / `files.error` 点击给出**专属文案** Toast（此前落回通用「未知命令」）；`files.guide` 的 subtitle 同步改为 es.exe 通道文案（移除过时的「启用 HTTP 服务器」表述）。
- 嵌套页支持**边打边搜**（200ms 去抖重拉 `get_items`）+ 过期结果补偿 + 落地保留页内 query；嵌套页跳过宿主本地二次过滤（`passthrough`），进页自动聚焦搜索框。
- 文件搜索结果右键菜单与页脚 Enter 提示的默认动作文案从「执行」改为「打开文件」（**遗漏修复**：`footer_action_text` 的 ext_id 映射表自 `com.ddrun.filesearch` 加入以来未同步，文件项被回退到 `footer.invoke`；实际 `files.open.{pid}` 走 `host/open_url` 调 ShellExecute 用系统默认程序打开，与 `apps` 扩展「打开应用」同语义）。`text.rs` 新增 `footer.open_file` i18n 条目，`footer_action_maps_five_builtin_extensions` 与 `ctx_menu_more_commands_override_static_mapping` 测试期望同步。
- 文件搜索结果按 Enter 真正「打开文件」（v3.3 P1.5 修复）：此前 `host/open_url` 对 `file://` URL 一律走 `webbrowser::open`——目录→浏览器索引页；`.txt`/`.html`/`.mp4` 等走浏览器而非关联程序；`.exe`/未知扩展名浏览器报错。改为检测 `file://` 协议后改走 `ShellExecuteW(verb="open")` 自动派发（**目录 → Explorer 窗口；文件 → 关联程序**），`http(s)://` 保持 webbrowser 默认浏览器（websearch 不受影响）。**协议 v1.0 零改动**（复用既有 `host/open_url`）—— 新增 `platform::file_url_to_path` + `platform::open_path`（仿 `run_as_admin` 风格），5 个单测覆盖基础/编码/CJK/非 file 协议/远程 file URL。

## [0.1.0]

### 新增

- **文件搜索**（扩展 `com.ddrun.filesearch`）：基于 Everything 的本地文件搜索。
  - 根视图输入 `f ` + 关键词自动进入文件结果页；同时保留顶层入口与 fallback 模板入口。
  - 每次最多返回前 30 条，按「文件名 > 路径」相关度并叠加近因加分排序；回车经 `host/open_url` 用系统默认程序打开。
  - Everything 搜索语法（`ext:` / `dm:` / `path:` / 通配符 / 正则）原样透传。
  - 以 sidecar 形式随绿色包分发（`dist/extensions.d/`），免安装。
- 用户文档：[`docs/search.md`](./docs/search.md)（Everything 配置、语法速查、故障排查）。

### 说明

- 文件搜索依赖 Everything，**仅支持 Windows**；跨平台 fd 兜底 Provider 计划于 **v0.2** 提供。
- 协议 v1.0 **冻结**：本版本未新增任何协议方法，文件结果复用协议已定义的 `get_items` 子页能力。

### 传输层（实现说明：es.exe 通道切换（`6133d96`）晚于 v0.1.0 标签最初落库（2026-09-06）；该标签 2026-09-09 经用户决策删除并重打至 es.exe 实现之后的提交，随重打首次触发 GitHub Release）

- **经 Everything 官方命令行工具 `es.exe`（IPC 通道）检索**，**无需开启 Everything HTTP 服务器**；仅需 Everything 在运行 + `es.exe` 已安装（`winget install --id=voidtools.Everything.Cli`）。
- 配置项改为 `DDRUN_ES_PATH` / `DDRUN_EVERYTHING_DIR`（默认自动定位，无 HTTP/端口概念）。
- es.exe 经管道输出为系统 OEM 代码页（中文 Windows = 936/GBK）而非 UTF-8，已由 `decode_output()`（`MultiByteToWideChar` + `GetConsoleOutputCP`，复用 `shell.rs` 惯例）按代码页转 UTF-8，单测 `decode_with_codepage_converts_gbk_filename` 覆盖——修复此前中文路径乱码导致「搜不到文件」的问题。
