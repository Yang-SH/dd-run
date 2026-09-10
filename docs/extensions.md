# 写一个 dd-run 扩展

> **本文件是「路径」，不是「规范」**。规范只有两份，二者都是硬契约：
>
> | 规范 | 管什么 |
> |---|---|
> | [`protocol.md`](./protocol.md) | 进程间怎么说话（JSON-RPC over NDJSON、生命周期、8 种执行结果） |
> | [`manifest-schema.md`](./manifest-schema.md) | 宿主怎么发现你（清单字段、目录、校验规则） |
>
> 本文只做三件事：**给一条能跑通的路线**、**标出真实踩过的坑**、**告诉你怎么自检**。
> 凡是与规范冲突的表述，以规范为准。

---

## 0. 30 秒心智模型

```
宿主 spawn 你的进程
        │  stdin  ← 宿主发请求（initialize / top_level_commands / invoke / …）
        ▼
   你的扩展进程        stdout → 一行一条 JSON（只允许协议消息！）
        │  stderr  → 你的日志（宿主不解析，但崩溃时会记下来）
        ▼
宿主把结果渲染成列表 / 嵌套页 / Toast
```

**任何语言都能写**：只要能读写 stdin/stdout。仓库里有一个完整的
[Python 示例](../examples/python-minimal/)（仅标准库），它已经通过全表面对照自检。

---

## 1. 10 分钟上手

### 第 1 步：选一个目录

宿主扫描两处（[`manifest-schema.md`](./manifest-schema.md) §2），同名 id 时**用户目录覆盖 sidecar**：

| 场景 | 放哪里 |
|---|---|
| 开发调试 | `%APPDATA%\dd-run\extensions.d\`（Windows） |
| 随绿色包分发 | 与 `dd-run.exe` **同目录**的 `extensions.d\` |

### 第 2 步：写清单

文件名随意，建议 `<id>.json`：

```json
{
  "schema_version": "1.0",
  "id": "com.example.hello",
  "name": "Hello Extension",
  "version": "1.0.0",
  "entry": { "command": "${EXT_DIR}/hello" },
  "frozen": false,
  "capabilities": ["host/show_status"]
}
```

必填字段只有 5 个：`schema_version` / `id` / `name` / `version` / `entry.command`。
`${EXT_DIR}` 展开为**清单所在目录**（§4），`entry.command` 必须指向真实存在的文件（校验规则 8）。

### 第 3 步：写程序

最小可用扩展 = 只实现 `initialize` + `top_level_commands` + `close`。以 Python 为例：

```python
import json, sys

def send(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n")
    sys.stdout.flush()

for line in sys.stdin:                 # 一行一条消息（NDJSON）
    line = line.strip()
    if not line:
        continue                       # 空行忽略（§2.2 规则 5）
    msg = json.loads(line)
    mid, method = msg.get("id"), msg.get("method")
    if method is None or mid is None:
        continue                       # 对端响应 / 通知：不回复
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "protocol_version": "1.0",
            "provider": {"id": "com.example.hello", "display_name": "Hello",
                         "frozen": False, "has_fallback": False},
            "capabilities": ["host/show_status"],
        }})
    elif method == "top_level_commands":
        send({"jsonrpc": "2.0", "id": mid, "result": {"commands": [
            {"id": "hello.greet", "title": "打个招呼", "command": {"kind": "invoke"}},
        ]}})
    elif method == "invoke":
        # §6.5：信封的 `result` 字段**就是** CommandResult 本体，不要再多包一层
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "kind": "ShowToast", "args": {"message": "你好！", "duration_ms": 2000},
        }})
    elif method == "close":
        send({"jsonrpc": "2.0", "id": mid, "result": {}})
        break
```

`provider.id` 必须与清单 `id` 一致（§8 一致性表）。

### 第 4 步：让宿主能启动你

`entry.command` 由宿主**直接 spawn**（**不经 shell**），所以要指向真正的可执行文件：

| 你的实现 | 怎么做 |
|---|---|
| 编译型（Rust/Go/C） | `"entry": {"command": "${EXT_DIR}/my-ext"}` —— Windows 上宿主会自动补 `.exe`（校验规则 8） |
| **解释型（Python/Node）** | **跑一个安装脚本，把「解释器 + 脚本」的绝对路径写进清单**（见下） |

解释型必须**生成**清单，因为清单**表达不了**「解释器 + 脚本」这种两段式命令行：
`entry.args` **不支持** `${EXT_DIR}` 展开（§4 只覆盖 `command` / `cwd` / `icon`）。

推荐做法 —— 安装时把**绝对路径**写死（`examples/python-minimal/install.py` 就是这么做的）：

```python
manifest["entry"] = {
    "command": sys.executable,                  # 解释器绝对路径
    "args": [str(target / "hello.py")],         # 脚本绝对路径
}
```

宿主直接 spawn `command`、不经 shell → **零 PATH 依赖**。

> ⚠️ **不要在 `.cmd` 启动器里写裸 `python`** —— 那**依赖解释器位于 PATH 上**。
> Windows 上这远非默认（Store 版 / 未勾选 Add-to-PATH / IDE 自带解释器都不在 PATH），
> 而且 dd-run 从**资源管理器启动时继承的是系统 PATH**（与你敲命令行的环境不同）。
> 典型症状：**终端里自检全绿，双击 dd-run 却显示扩展"暂时不可用"** —— 实测踩过：
> `.cmd` 里的 `python` 在 GUI 环境下找不到，连续 3 次 spawn 失败后熔断。
>
> 若已确认 `where python` 有输出，`.cmd` 桥接也可用：清单写
> `{"command": "${EXT_DIR}/hello"}`，宿主会依次补 `.exe` / `.cmd` / `.bat` 解析到 `hello.cmd`。

> ⚠️ **不要把无扩展名的 POSIX 启动器与 `.cmd` 放在同一目录**：宿主先做
> `is_file()` 检查，无扩展名者会**先命中**，Windows 上随后 spawn 失败。
> 跨平台时请分成两个目录/两份清单，或在 POSIX 清单里把 `entry.command` 改成
> `${EXT_DIR}/hello.sh`。

### 第 5 步：自检

```bash
dd-run-cli --list-extensions --extensions-dir <你的目录>
dd-run-cli --conformance   --extensions-dir <你的目录>
```

`--conformance` 逐项给出 `✓ / ⚠ / ✗`，**任何 ✗ 都会让退出码非 0**。
看到 `✓ 一致性自检通过` 就可以丢进真正的扩展目录了。

---

## 2. 三条铁律

违反任一条，宿主都会解析失败——而且症状通常很难猜。

### 铁律 1：stdout 只允许出现协议消息

一行一条**紧凑** JSON，行尾一个 `\n`。**任何 `print` 调试都必须写 stderr**：

```python
print(f"[hello] 收到 {method}", file=sys.stderr, flush=True)   # ✅
print(f"[hello] 收到 {method}")                                # ❌ 污染协议流
```

协议 §2.5：stderr 只用于日志。宿主不解析它，但扩展崩溃时会把 stderr 记进自己的日志
（`--conformance` 在握手失败时也会打印 stderr 末尾，方便你定位）。

### 铁律 2：UTF-8，且行内不得有裸换行

JSON 字符串里的换行必须转义成 `\n`（两个字符）——主流序列化器天然如此，
所以「按行切分」才是安全的。**不要** pretty-print 成多行 JSON。

### 铁律 3：通知不回复，响应不当作请求

- 你发出的**通知**（`items_changed` / `initialized`）**没有 `id`**，宿主不会回；
- 宿主对你的请求的**响应**也**没有 `method`**。

判断方法（§3.3）：收到消息先看 `method` —— **有 `method` 且是自己能提供的方法**（对扩展而言是…没有，
扩展只提供宿主调用的那几个）才是请求；**没有 `method`** 就是对你请求的响应，忽略即可。

---

## 3. 三个 Windows 陷阱（都在 Python 示例里处理过）

### 陷阱 1：`\n` 被翻译成 `\r\n`

Windows 文本模式会把 `\n` 写成 `\r\n`。协议 §2.2 规则 1 **容忍** CRLF（接收方会剥掉行尾 `\r`），
所以不算致命——但会污染你的日志与抓包。

### 陷阱 2：stdout 编码不是 UTF-8

管道上的 stdout 默认编码可能是 **cp936**（中文 Windows）。你的中文标题会被按 GBK 写出，
而宿主按 UTF-8 解析 → **乱码**。这条**不容忍**。

两个修法，建议都做：

```python
sys.stdout.reconfigure(encoding="utf-8", newline="\n")   # 显式指定，一劳永逸
```
```python
json.dumps(obj, ensure_ascii=False, ...)                 # 配合上面
# 或者反过来：保留 ensure_ascii=True（默认），输出纯 ASCII 的 \uXXXX，编码无关
```

---

### 陷阱 3：解释器不在 PATH 上（最容易「自检通过却用不了」）

见 §1 第 4 步。根因是**两个环境的 PATH 不同**：自检工具继承你**终端**的环境，
而 GUI 从资源管理器/托盘启动时继承**系统**环境。

```
终端:  python hello.py   ✅（PATH 里有）
GUI :  python hello.py   ✗ 'python' 不是内部或外部命令
```

所以凡是启动器/脚本里出现**裸命令名**（`python` / `node` / `npx` / `java`），
都要按这条怀疑一遍。**`--conformance` 全绿不代表双击 dd-run 能启动** ——
它只能覆盖协议一致性，覆盖不了运行环境差异。

排查一句话：`where python`（Windows）有没有输出。

---

## 4. 握手：`initialize`

宿主在 spawn 后**必须先收到** `initialize` 的成功响应，才会发别的请求。

**宿主发来的**（§5.1）：

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
  "protocol_version":"1.0",
  "host":{"name":"dd-run","version":"0.1.1","platform":"windows"},
  "transport":{"framing":"ndjson","max_message_bytes":1048576},
  "capabilities":["host/show_status","host/set_clipboard","host/open_url"],
  "locale":"zh-CN"}}
```

**你要回的**（四个字段都是必填）：

```json
{"jsonrpc":"2.0","id":1,"result":{
  "protocol_version":"1.0",
  "provider":{"id":"com.example.hello","display_name":"Hello","frozen":false,"has_fallback":false},
  "capabilities":["host/show_status"]}}
```

| 字段 | 怎么定 |
|---|---|
| `protocol_version` | 回**不高于**宿主所发的版本；不认识宿主的主版本时回 `-32004`（§5.3） |
| `provider.frozen` | 顶层命令**不变**就填 `true`（宿主会落磁盘桩，冷启动免拉起） |
| `provider.has_fallback` | 有兜底命令就填 `true`（**宿主据此把你当作 fresh**，见 §6） |
| `capabilities` | 你**要用到**的 `host/*`；没声明的调用会被回 `-32601`（§7.4） |

`params.locale` 可以拿来做本地化。收到后也可以发一条 `initialized` 通知（可选，§5.2）。

---

## 5. 方法：按「值得实现」的顺序

### 5.1 `top_level_commands`（几乎没有扩展能省）

返回 `{"commands": [CommandItem...]}`，**可以是空数组**。
每个 `CommandItem` 必填 `id` / `title` / `command`；`command.kind` 只有两种：

```json
{"kind": "invoke"}                          → 选中即执行
{"kind": "page", "page_id": "hello.list"}   → 选中进嵌套页
```

可选字段：`subtitle` / `icon` / `section` / `tags` / `details` / `text_to_suggest` /
`more_commands`（右键菜单，可嵌套）。

### 5.2 `get_command`（`frozen: true` 时**必须**实现）

冷启动时宿主只读磁盘桩、不拉起你；用户点击某个桩项时才 spawn → `initialize` →
`get_command(id)` → 执行。**返回 `{"command": null}` 是正常结果**（表示桩已失效），
不是错误——但如果你对**刚刚在 `top_level_commands` 里列出的 id** 返回 `null`，
桩态点击就会失败。`--conformance` 的第 5 步专门查这个。

> **兜底为什么绕开 `get_command`**：兜底模板（下一条）**不在**顶层命令表里，
> 而协议 §6.4 规定 `get_command` 只查顶层命令。宿主知道这一点，对兜底项**跳过**
> 该校验直接执行——这是 dd-run 侧修掉的一个真实缺陷（`implementation.md` §6.1 L5）。
> 你不需要为兜底 id 实现 `get_command`。

### 5.3 `fallback_commands`（"搜不到东西"时出场）

搜索词命中不了任何常规项时，宿主展示你的兜底模板。**返回值非空 ⟺ `has_fallback: true`**
——两个方向都要一致，否则 `--conformance` 会报不一致：

| 情况 | 后果 |
|---|---|
| 声明了 `true` 却没给命令 | 宿主拿不到兜底项，白声明 |
| 给了命令却声明 `false` | 宿主按可缓存处理，兜底项永远不会展示 |

占位符**写在 `title` 里**，宿主替换成当前搜索词：

```json
{"id":"hello.greet.query","title":"向「{query}」打招呼","command":{"kind":"invoke"}}
```

执行时从 `invoke` 的 `context.query` 取回原始搜索词。

### 5.4 `invoke`

```json
{"jsonrpc":"2.0","id":6,"method":"invoke","params":{
  "id":"hello.greet","sender":"top_level","context":{"query":"1+1"}}}
```

`sender` ∈ `top_level` / `list_item` / `context_menu`。`context.selected_item_id`
在从列表项或右键菜单触发时给出目标项 id。

响应体是 **`{"result": CommandResult}`**，`CommandResult` 有 **8 种 `kind`**：

| kind | 用途 |
|---|---|
| `Dismiss` / `Hide` | 关闭面板 / 隐藏但保留状态 |
| `GoHome` / `GoBack` / `GoToPage` | 页面栈导航（`GoToPage` 带 `args.page_id`） |
| `KeepOpen` | 什么都不做，保持打开 |
| `ShowToast` | 弹提示：`args.message` / `args.duration_ms` |
| `Confirm` | 二次确认：`args.title` / `description` / `confirm_label` / `is_critical` |

**`Confirm` 的确认结果不通过协议回传**：宿主弹框，用户点确认后**重新发一次 `invoke`**，
并在 `params.context.confirmed = true` 里带标记。所以你的 `invoke` 要写成幂等的两段式：

```python
if not context.get("confirmed"):
    return {"kind": "Confirm", "args": {...}}
# 已经确认过，真正执行
```

命令不存在时回 `-32002`（**不是** `ShowToast`）。

### 5.5 `get_items`（嵌套页）

```json
{"jsonrpc":"2.0","id":4,"method":"get_items","params":{"page_id":"hello.list","search_text":"abc"}}
{"jsonrpc":"2.0","id":4,"result":{"items":[...],"has_more_items":false,"is_loading":false}}
```

三个响应字段都必填。`search_text` 可选，是否过滤由你决定（宿主不会对嵌套页二次过滤）。
**禁止增量推送**：列表变了就发 `items_changed` 通知，让宿主重新全量拉。

### 5.6 `close`

回 `{"result": {}}` 后**尽快自行退出**（建议 ≤ 1 s），否则宿主强杀。

---

## 6. 反向请求：`host/*`

有些事后台进程做不了（弹 Toast、写剪贴板、开浏览器）。**先声明，再调用**：

| 方法 | 参数 |
|---|---|
| `host/show_status` | `message`（必填）、`state`（`info`/`success`/`warning`/`error`）、`duration_ms` |
| `host/set_clipboard` | `text` |
| `host/open_url` | `url` |

```python
# 用你自己的 id 空间（与宿主的互不相干，两边都从 1 开始）
send({"jsonrpc":"2.0","id":1,"method":"host/set_clipboard","params":{"text":"hello"}})
```

**不必阻塞等应答**：宿主按到达顺序处理，你的 `invoke` 响应可以在它之前发出
（协议 §14 的示例正是这个时序）。之后收到那条 `id` 的响应忽略即可。

未在 `initialize.capabilities` 里声明就调用 → 会被回 `-32601`。

---

## 7. 自检

```bash
dd-run-cli --list-extensions  --extensions-dir <DIR>   # 只做清单扫描与校验
dd-run-cli --roundtrip        --extensions-dir <DIR>   # 最小链路：spawn → initialize → top_level → close
dd-run-cli --conformance      --extensions-dir <DIR>   # 全表面（推荐）
dd-run-cli --conformance      --extensions-dir <DIR> --invoke   # 额外真执行一次 invoke
dd-run-cli --conformance      --extensions-dir <DIR> --ext-id <ID>  # 目录内有多个时指定
```

`--conformance` 的每一项对应什么：

| 检查 | 对应规范 | 说明 |
|---|---|---|
| `1) spawn` | §4 | 进程能拉起、管道就绪 |
| `2) initialize` | §5.1 / §5.3 | 握手 + 版本协商 |
| `2a) provider.id` | §8 一致性表 | 与清单 `id` 不一致仅告警（宿主以清单为准） |
| `2b) capabilities` | §7.4 | 自述能力必须在宿主白名单内 |
| `3) top_level` | §6.1 | 顶层命令可拉取 |
| `3a) items` | §8.1 / §8.2 | 每项 `id` / `title` / `command.kind` 齐全合法 |
| `4) fallback` | §6.2 | 返回值与 `has_fallback` 一致 |
| `4a) template` | §6.2 | 兜底 `title` 含 `{query}`（仅告警） |
| `5) get_command` | §6.4 | 顶层 id 必须可复热 |
| `6) get_items` | §6.3 | 每个 page 的响应结构 |
| `7) invoke` | §6.5 / §8.3 | 返回值必须是 8 种 `kind` 之一（**默认跳过**） |
| `8) host/*` | §7 | 实际用到的能力是否都已声明 |
| `9) close` | §6.6 | 优雅退出 |

> **为什么 `invoke` 默认跳过**：它会**真的产生副作用**——写剪贴板、开浏览器、
> 甚至关机。要跑就显式加 `--invoke`。

---

## 8. 调试手册

### 手工喂 NDJSON

最快的定位手段：绕开宿主，直接和你的进程对话。

```bash
python hello.py
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol_version":"1.0"}}
```

回车后应立刻看到一行 `result`。若没反应 → 大概率你没 `flush`（管道是块缓冲）。

### 常见错误对照表

> **先看这里**：扩展启动失败或崩溃后，宿主会把**失败原因**写进
> 「设置 → 扩展」卡片第三行（红色，悬停看全文），形如
> `spawn 失败：…（命令：<被尝试的路径>）` 或 `诊断：退出码 1；stderr: <末行>`。
> 先读那一行，再对照下表——多数情况它已经给出根因。

| 症状 | 多半是 | 处理 |
|---|---|---|
| 扩展**根本没出现**在列表里 | 清单校验没过 | `dd-run-cli --list-extensions` 看跳过原因（`✗` 行） |
| 出现了但标成「暂时不可用」、点「重试」也无效 | **spawn 直接失败**——最常见是启动器里的裸 `python` / `node` 找不到 | **先看卡片第三行**（会给出被尝试的命令路径）；再见 §3 陷阱 3，改用绝对路径清单 |
| 终端自检全绿，GUI 里却启动失败 | 终端与 GUI 的 **PATH 不同** | 同上（`where python` 自查）；卡片失败原因即为准 |
| 出现了但点不动 / 一直 Loading | 请求超时 | 检查是否卡在等 `host/*` 应答；`invoke` 预算只有 10 s |
| `initialize` 超时 | 没写 `id`，或没 `flush` | stdout 必须 flush；`id` 必须原样回填 |
| 列表里中文乱码 | stdout 编码不是 UTF-8 | 见 §3 陷阱 2 |
| 宿主报「收到非法 JSON」 | 往 stdout 打了日志 / 多行 JSON | 见 §3 铁律 1、2 |
| 点命令报 **`非法 JSON-RPC 信封：missing field \`kind\``** | `invoke` 响应把 CommandResult **多包了一层** `{"result": ...}` | 见 §5.4：信封的 `result` 就是 CommandResult 本体 |
| 桩态点击报「命令已失效」 | `get_command` 对顶层 id 回了 `null` | 见 §5.2 |
| 兜底项从不出现 | `has_fallback` 与 `fallback_commands` 不一致 | 见 §5.3 |
| 改代码后没变化 | 磁盘桩/host 缓存未失效 | 升 `version`（桩缓存以版本为键） |

### 超时预算（§10，宿主侧默认值）

| 阶段 | 默认 |
|---|---|
| `initialize` | 5000 ms（含你建索引的时间） |
| `top_level_commands` | 3000 ms |
| `get_items` / `fallback_commands` | 2000 ms |
| `invoke` | 10000 ms |
| `close` | 1000 ms |

`initialize` 的 `result.timeouts` 可以建议更宽的值，但宿主有权覆盖。

---

## 9. 崩溃了会怎样

你崩了，宿主**不会**跟着崩（进程隔离，ADR-1）：

1. 宿主检测到 EOF / 非 0 退出码 → 该扩展的命令回落为**桩**（有缓存时）或直接从列表移除；
2. 在途请求立刻失败，界面不会卡住；
3. 连续崩溃 3 次（可配置）→ 标记「暂时不可用」，需宿主重启或用户在
   **设置 → 扩展管理**点「重试」；
4. 用户再次点击该桩项 → 宿主重新 spawn 你。

**你的义务**：退出前把关键异常写进 stderr；不要留下残留子进程或临时文件。

---

## 10. 参考

| 资源 | 用途 |
|---|---|
| [`../examples/python-minimal/`](../examples/python-minimal/) | Python 全表面示例（含启动器、清单、说明），已通过 `--conformance` |
| [`protocol.md`](./protocol.md) | 协议全文，§14 有一个完整会话的逐行示例 |
| [`manifest-schema.md`](./manifest-schema.md) | 清单字段、目录、9 条校验规则 |
| [`search-file.md`](./search-file.md) | 真实生产扩展（Everything 文件搜索）的设计与实施记录 |
| `crates/dd-ext-sample` | Rust 参考实现（宿主自检用） |
