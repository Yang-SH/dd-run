# PyMin —— dd-run 的 Python 扩展示例（全表面）

一个**只用标准库**的 Python 3 扩展，覆盖 dd-run 协议 v1.0 的完整表面。
它是 [`docs/extensions.md`](../../docs/extensions.md) 的配套实例，也是
「**协议与语言无关**」这句话的实证——它不是 Rust 写的。

## 目录内容

| 文件 | 作用 |
|---|---|
| `dd_ext_pymin.py` | 扩展本体（协议实现，注释逐条对齐 `protocol.md`） |
| `install.py` | **安装脚本**：生成绝对路径清单并拷贝脚本（推荐入口） |
| `dd-ext-pymin.cmd` | 备选的 Windows 启动器（⚠️ 依赖解释器在 PATH 上，见文末） |
| `README.md` | 本文件 |

## 快速开始

```bash
# 1) 安装（默认装到 %APPDATA%\dd-run\extensions.d\）
python install.py
#    也可以指定目标目录，例如分发形态的 exe 同目录：
#    python install.py "<dd-run.exe 所在目录>\extensions.d"

# 2) 自检（不需要 GUI）
dd-run-cli --conformance --extensions-dir "%APPDATA%\dd-run\extensions.d" --ext-id com.example.pymin --invoke

# 3) 打开 dd-run（Win+Alt+Space）→ 输入 pymin
```

第 2 步应看到 10 项全 `✓`。`--invoke` 会真执行一次「当前时间」命令（写剪贴板），
不加则跳过第 7 项。

若扩展之前已经失败过并被熔断：**设置 → 扩展 → PyMin Example → 点「重试」**，或重启 dd-run。

## 覆盖了哪些协议面

| 方向 | 方法 |
|---|---|
| host → ext | `initialize` / `top_level_commands` / `fallback_commands` / `get_items` / `get_command` / `invoke` / `close` |
| ext → host | `items_changed`（通知）/ `host/show_status` / `host/set_clipboard` / `host/open_url`（请求） |
| 执行结果 | `ShowToast` / `Dismiss` / `Confirm`（含确认后重发 `invoke`） |

功能上提供 5 个命令 + 1 条兜底模板：

- **PyMin：当前时间** —— `host/show_status` + `host/set_clipboard` 往返
- **PyMin：便签列表** —— 嵌套页（`get_items`，支持 `search_text` 过滤）
- **PyMin：添加便签** —— 取 `context.query`，改完发 `items_changed` 通知宿主重拉
- **PyMin：清空便签** —— `Confirm` 二次确认演示
- **PyMin：打开扩展文档** —— `host/open_url`
- 兜底模板「添加便签「{query}」」—— 无匹配时出场

## 为什么是「安装脚本生成清单」，而不是给一份固定清单？

因为**一份固定的清单对解释型扩展没法写对**：

1. 清单的 `entry.args` **不支持** `${EXT_DIR}` 展开（`manifest-schema.md` §4 只覆盖
   `entry.command` / `entry.cwd` / `icon`）——表达不了「解释器 + 脚本」这种两段式命令行；
2. 用 `.cmd` 桥接（`dd-ext-pymin.cmd`）虽然能把两段拼起来，但它写的是**裸 `python`**，
   **依赖解释器位于 PATH 上**。Windows 上这远非默认（Store 版 / 未勾选 Add-to-PATH /
   IDE 自带解释器都不在 PATH），而 dd-run 从**资源管理器启动时继承的是系统 PATH**
   ——于是很容易出现「终端自检全绿、双击 dd-run 却启动不了」的假阳性（实测踩到过）。

`install.py` 的做法是：把 `sys.executable` 与脚本的**绝对路径**写进清单。
宿主直接 spawn `command`、**不经 shell**，因此**零 PATH 依赖**。

## 其他平台

`install.py` 会按当前平台的约定选择默认目录（macOS/Linux 回落到
`~/.config/dd-run/extensions.d`），清单里写的是当前解释器的绝对路径，因此在
macOS / Linux 上同样直接可用，无需改动 `.cmd` 或任何手写清单。

## 调试协议流

```bash
python dd_ext_pymin.py
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol_version":"1.0"}}
```

回车后应立刻返回一行 `result`。日志都走 stderr，stdout 只有协议消息。

## 许可

与仓库一致（MIT）。
