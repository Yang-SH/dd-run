# 文件搜索（dd-run）

> **状态**：生效中 ｜ **版本**：v1.0 ｜ **最后更新**：2026-09-13
> **关联**：[search-file.md](./search-file.md)（开发者权威方案）

---

## 1. 前置条件（一次性，约 2 分钟）

文件搜索通过 Everything 检索本地文件，结果直接呈现在 dd-run 主面板中。

- **传输通道**：dd-run 优先用 `everything-ipc` 直接连接正在运行的 Everything；当 IPC 不可用时自动回落到 `es.exe`（Everything 官方命令行工具）。无需开启 Everything 的 HTTP 服务器。
- **平台**：仅支持 Windows（依赖 Everything）。

1. 安装并运行 [Everything](https://www.voidtools.com/)（1.4 及以上）。无需开启 Everything 的 HTTP 服务器。
2. 安装 `es.exe`（Everything 命令行工具，不随 Everything 安装包附带）：
   - 推荐：`winget install --id=voidtools.Everything.Cli`
   - 或手动从 voidtools 下载 `es.exe`，放到 Everything 安装目录（默认 `C:\Program Files\Everything\`）。
   - 说明：`es.exe` 在 IPC 主通道可用时是回落通道，**仍受支持**；在 IPC 真机验收完成并发布前，仍建议安装 `es.exe`，确保各种环境下都能搜索。
3. dd-run 按以下顺序自动定位 `es.exe`，**正常情况下无需手动配置**：
   - 环境变量 `DDRUN_ES_PATH`（完整路径）
   - 系统 `PATH`（winget 安装后 `es.exe` 已在 PATH）
   - 环境变量 `DDRUN_EVERYTHING_DIR`（你给出的 Everything 安装目录）
4. 验证：终端执行 `es.exe -get-everything-version`，应返回 Everything 的版本号。

> ⚠️ `es.exe` 走 IPC 与 Everything 通信，**索引必须已由 Everything 建好**；新装 Everything 后请稍等其完成首轮索引。

---

## 2. 三种进入方式

| 方式 | 操作 | 说明 |
| :--- | :--- | :--- |
| **直达（推荐）** | 输入 `f` + 空格 + 关键词，如 `f report` | 自动进入文件结果页，**无需再按回车**；搜索框保留关键词 |
| 顶层入口 | 输入「文件搜索」后选中该项 | 适合记不住前缀时使用 |
| 兜底入口 | 输入任意关键词 → 选中「在文件中搜索 xxx」 | 由 fallback 模板自动生成 |

**结果页内操作**：`↑` `↓` 导航，`Enter` 用系统默认程序打开，`Esc` 返回上级；**右键**（或菜单键）呼出上下文菜单：打开 / 显示所在目录 / 复制路径。

结果项图标按文件类型区分：文档 / 代码 / 压缩包 / 图片 / 音频 / 视频 / 可执行 / 配置 / 字体 / 电子书等 12 类（目录显示文件夹图标，未识别类型显示通用文件图标）。

---

## 3. 搜索语法速查

关键词**原样透传**给 Everything（例外：以 `-` 或 `/` 开头的查询会被前置 `^` 转义，避免被 `es.exe` 当作命令行开关），因此其全部语法均可直接使用：

| 语法 | 示例 | 含义 |
| :--- | :--- | :--- |
| 空格 | `report 2024` | 同时包含（AND） |
| `\|` | `report \| summary` | 任一包含（OR） |
| `!` | `report !draft` | 排除 |
| `ext:` | `ext:rs` | 按扩展名筛选 |
| `path:` | `path:dd-run` | 路径包含 |
| `dm:` | `dm:today`、`dm:last7days` | 按修改时间筛选 |
| `size:` | `size:>1mb` | 按文件大小筛选 |
| `*` `?` | `*.log`、`file?.txt` | 通配符 |
| `regex:` | `regex:^v\d+` | 正则表达式 |
| `folder:` | `folder:src` | 仅匹配目录名 |

完整语法参见 Everything 官方帮助文档。

---

## 4. 配置项

| 环境变量 | 默认值 | 用途 |
| :--- | :--- | :--- |
| `DDRUN_ES_PATH` | 空 | `es.exe` 的完整路径；仅在它不在 `PATH`、也不在 Everything 目录时使用 |
| `DDRUN_EVERYTHING_DIR` | 空 | Everything 安装目录（用于定位同目录的 `es.exe`） |

> 环境变量在**扩展进程启动时**读取，修改后需**重启 dd-run** 生效。正常情况下无需设置任何变量——winget 安装 `es.exe` 后即可直接使用。

---

## 5. 故障排查

| 现象 | 可能原因 | 处理 |
| :--- | :--- | :--- |
| 提示「未检测到 Everything」 | Everything 未运行，或 `es.exe` 不在 PATH/指定位置 | 启动 Everything；执行 `es.exe -get-everything-version` 确认识别；必要时设 `DDRUN_ES_PATH` |
| 搜不到文件 / 结果乱码 | Everything 未运行、索引未建完，或 `es.exe` 缺失/版本过旧 | 等待索引完成；重装 `es.exe`（`winget install --id=voidtools.Everything.Cli`）；确认 `es.exe -get-everything-version` 正常 |
| 结果为空 | 关键词过窄（索引尚未建完时 dd-run 返回的是引导项而非空结果） | 换更宽泛的关键词；等待索引完成后再试 |
| 有结果但打不开文件 | 系统未关联默认程序，或文件已被移动/删除 | 手动关联程序；确认文件仍在原路径 |
| 搜索长时间无响应 | Everything 进程异常 | 重启 Everything；dd-run 会在约 1.2 秒后按超时处理，不会卡死 |

---

## 6. 已知边界

- **仅支持 Windows**：依赖 Everything。跨平台能力（fd 兜底 Provider）计划于 v0.2 提供。
- 每次最多返回前 **30** 条结果，按「文件名 > 路径」的相关度并叠加近因加分排序。
- **结果页二次输入会重新搜索**：在结果页搜索框继续输入时，宿主会做 **200ms 去抖**后重新向扩展请求，经 `everything-ipc` 或 `es.exe` 重新拉取；每次仍**至多返回前 30 条**（与上一条同口径）；输入期间结果会刷新。
- **Everything 未运行 / `es.exe` 缺失时的引导项**：点击会弹出**专属提示 Toast**（写明需安装/启动 Everything 或 `es.exe`），不再是通用「未知命令」文案。
- 单个搜索请求超过 **2 秒**未响应时，宿主按超时处理（不挂起、不崩溃）。

---

## 7. 后续升级说明

开发计划 [`search-file.md`](./search-file.md) §九 规划了两项升级，当前状态：

- **更多操作（✅ 已提供）**：结果项右键 = 打开（默认）/ **显示所在目录**（资源管理器中定位文件）/ **复制路径**（复制完整路径到剪贴板）。
- **IPC 直连（✅ 已实现）**：现已优先通过 `everything-ipc` 直连正在运行的 Everything，失败自动回落 `es.exe`。在 Windows 真机兼容性、回落与性能验收（A-33-05…A-33-10）完成并发布前，仍建议安装并保留 `es.exe` 作为回落通道；届时用户文档会再同步更新。

升级细节见 [`search-file.md`](./search-file.md) §九。当前实现不使用 Everything HTTP 服务，用户只需按本页说明准备 Everything（并建议安装 `es.exe`）。

---

## 8. 常见问题

- **搜不到文件怎么办？**
  1. 确认 Everything 已运行（`es.exe -get-everything-version` 能返回版本号）；新装 Everything 请等首轮索引完成。
  2. 确认 `es.exe` 已安装并在 PATH，或通过 `DDRUN_ES_PATH` / `DDRUN_EVERYTHING_DIR` 指定位置。
  3. 结果为空也可能是关键词过窄；换更宽泛的关键词重试。
  4. 搜索长时间无响应时，dd-run 会在约 1.2 秒后按超时处理，不会卡死。
- **还需要安装 `es.exe` 吗？**
  **需要（建议）**。`everything-ipc` 直连主通道已实现，但 `es.exe` 仍作为回落通道受支持；在 IPC 真机验收完成并发布前，已发布版本仍依赖 `es.exe`。建议保持安装，确保各种环境下都能搜索。
- **要开 Everything 的 HTTP 服务器吗？**
  不需要。无论是 IPC 直连还是 `es.exe` 回落，都走本机 IPC，不监听端口、无局域网暴露风险。
- **文件打不开 / 打开成了错误文件？**
  文件名含 `%` `#` `?` 或位于网络共享（UNC）等特殊路径，此前存在解析缺陷，**已在代码侧修复**（宿主改用 `resolve_file_url_to_path` 做存在性优先匹配）。如仍异常，请确认系统已为该类型关联默认程序、且文件未被移动/删除。
