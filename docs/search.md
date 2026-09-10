# 文件搜索（dd-run）

> **适用版本**：v0.1.0+ ｜ **平台**：仅 Windows（依赖 Everything）
> **面向**：使用者。实现方案与任务分解见 [`search-file.md`](./search-file.md)（开发者文档）。

文件搜索通过 Everything 官方命令行工具 **`es.exe`**（IPC 通道）检索本地文件，结果直接呈现在 dd-run 主面板中。**无需开启 Everything 的 HTTP 服务器。**

---

## 一、前置条件（一次性，约 2 分钟）

文件搜索通过 Everything 官方命令行工具 **`es.exe`**（IPC 通道）检索本地文件，**无需开启 Everything 的 HTTP 服务器**。

1. 安装并运行 [Everything](https://www.voidtools.com/)（1.4 及以上）。dd-run 仅要求 Everything **在运行**，不要求开启任何网络服务。
2. 安装 `es.exe`（Everything 命令行工具，不随 Everything 安装包附带）：
   - 推荐：`winget install --id=voidtools.Everything.Cli`
   - 或手动从 voidtools 下载 `es.exe`，放到 Everything 安装目录（默认 `C:\Program Files\Everything\`）。
3. dd-run 按以下顺序自动定位 `es.exe`，**正常情况下无需手动配置**：
   - 环境变量 `DDRUN_ES_PATH`（完整路径）
   - 系统 `PATH`（winget 安装后 `es.exe` 已在 PATH）
   - 环境变量 `DDRUN_EVERYTHING_DIR`（你给出的 Everything 安装目录）
4. 验证：终端执行 `es.exe -get-everything-version`，应返回 Everything 的版本号。

> ⚠️ `es.exe` 走 IPC 与 Everything 通信，**索引必须已由 Everything 建好**；新装 Everything 后请稍等其完成首轮索引。

---

## 二、三种进入方式

| 方式 | 操作 | 说明 |
| :--- | :--- | :--- |
| **直达（推荐）** | 输入 `f` + 空格 + 关键词，如 `f report` | 自动进入文件结果页，**无需再按回车**；搜索框保留关键词 |
| 顶层入口 | 输入「文件搜索」后选中该项 | 适合记不住前缀时使用 |
| 兜底入口 | 输入任意关键词 → 选中「在文件中搜索 xxx」 | 由 fallback 模板自动生成 |

**结果页内操作**：`↑` `↓` 导航，`Enter` 用系统默认程序打开，`Esc` 返回上级；**右键**（或菜单键）呼出上下文菜单：打开 / 显示所在目录 / 复制路径。

结果项图标按文件类型区分：文档 / 代码 / 压缩包 / 图片 / 音频 / 视频 / 可执行 / 配置 / 字体 / 电子书等 12 类（目录显示文件夹图标，未识别类型显示通用文件图标）。

---

## 三、搜索语法速查

关键词**原样透传**给 Everything，因此其全部语法均可直接使用：

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

## 四、配置项

| 环境变量 | 默认值 | 用途 |
| :--- | :--- | :--- |
| `DDRUN_ES_PATH` | 空 | `es.exe` 的完整路径；仅在它不在 `PATH`、也不在 Everything 目录时使用 |
| `DDRUN_EVERYTHING_DIR` | 空 | Everything 安装目录（用于定位同目录的 `es.exe`） |

> 环境变量在**扩展进程启动时**读取，修改后需**重启 dd-run** 生效。正常情况下无需设置任何变量——winget 安装 `es.exe` 后即可直接使用。

---

## 五、故障排查

| 现象 | 可能原因 | 处理 |
| :--- | :--- | :--- |
| 提示「未检测到 Everything」 | Everything 未运行，或 `es.exe` 不在 PATH/指定位置 | 启动 Everything；执行 `es.exe -get-everything-version` 确认识别；必要时设 `DDRUN_ES_PATH` |
| 搜不到文件 / 结果乱码 | `es.exe` 未安装或版本过旧 | 重装 `es.exe`（`winget install --id=voidtools.Everything.Cli`）；确认 `es.exe -get-everything-version` 正常 |
| 结果为空 | 关键词过窄，或 Everything 索引尚未建完 | 等待索引完成；换更宽泛的关键词 |
| 有结果但打不开文件 | 系统未关联默认程序，或文件已被移动/删除 | 手动关联程序；确认文件仍在原路径 |
| 搜索长时间无响应 | Everything 进程异常 | 重启 Everything；dd-run 会在约 1.2 秒后按超时处理，不会卡死 |

---

## 六、已知边界（v0.1）

- **仅支持 Windows**：依赖 Everything。跨平台能力（fd 兜底 Provider）计划于 v0.2 提供。
- 每次最多返回前 **30** 条结果，按「文件名 > 路径」的相关度并叠加近因加分排序。
- 结果页内的二次输入只做**本地过滤**（受前 30 条限制），不会重新请求 Everything。
- Everything 未运行或 `es.exe` 缺失时，引导项的点击提示为通用文案，将在后续版本打磨。
- 单个搜索请求超过 **2 秒**未响应时，宿主按超时处理（不挂起、不崩溃）。

---

## 七、后续升级说明（v3.3，部分已提供）

开发计划 [`search-file.md`](./search-file.md) 规划两项升级，当前状态：

- **更多操作（✅ 已提供）**：结果项右键 = 打开（默认）/ **显示所在目录**（资源管理器中定位文件）/ **复制路径**（复制完整路径到剪贴板）。
- **IPC 直连（尚未提供）**：规划优先连接正在运行的 Everything，失败时仍回落到 `es.exe`；在该功能通过
  Windows 真机兼容性、回落和性能验收前，仍须安装并可调用 `es.exe`。

升级完成后会同步更新本页的前置条件、故障排查和版本号；当前实现不使用 Everything HTTP
服务，用户只需按本页说明准备 `es.exe` 和正在运行的 Everything。
