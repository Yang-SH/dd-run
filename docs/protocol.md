# dd-run Extension Protocol v1.0

> **状态**：草案冻结（v1.0）——M0 期间按本规范实现，变更需走 §13 协议演进规则。  
> **面向**：写宿主的人与写扩展的人。这是二者之间**唯一的硬契约**。  
> **上游依据**：[`cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md) §5（扩展契约）、§6（宿主模型）。  
> **核验基准**：microsoft/PowerToys `v0.101.2362.0`（核验日期 2026-09-01）。本文引用的上游接口名、`CommandResultKind` 成员均以此为基准核验。

---

## 1. 概述

### 1.1 范围

本协议定义 **dd-run 宿主进程** 与 **单个扩展子进程** 之间的双向通信。宿主把扩展当作独立子进程拉起，二者通过 stdin/stdout 交换 JSON 消息。

**本协议不定义**：

- 宿主的 UI 实现（见设计文档 §4 与 [`cmdpal-ui-mockups.html`](../cmdpal-ui-mockups.html)）；
- 扩展如何被发现（见 [`manifest-schema.md`](./manifest-schema.md)）；
- 扩展的内部实现（任何能读写 stdin/stdout 的语言均可）。

### 1.2 术语

| 术语                       | 含义                            | 出处                           |
| ------------------------ | ----------------------------- | ---------------------------- |
| **host**                 | dd-run 宿主进程（面板本体）             | 设计文档 §3                      |
| **extension / provider** | 扩展子进程，对外暴露一个 provider         | 设计文档 §5.1 `ICommandProvider` |
| **command**              | 可执行单元（`invoke`）或可导航单元（`page`） | 设计文档 §5.3                    |
| **item / CommandItem**   | 列表中的一项，携带展示字段与一个 command 引用   | 设计文档 §4.4 / §5.3             |
| **page**                 | 命令执行后压入的嵌套页                   | 设计文档 §4.5                    |
| **stub**                 | 命令的"仅数据、无活进程"缓存态              | 设计文档 §6.3                    |
| **frozen**               | provider 顶层命令列表不变、可缓存到磁盘      | 设计文档 §6.3                    |


### 1.3 方法总览

**请求类（10 个，有 `id`、必须有响应）**

| 方法                   | 方向         | 用途                       | 章节   |
| -------------------- | ---------- | ------------------------ | ---- |
| `initialize`         | host → ext | 握手 + 版本协商                | §5.1 |
| `top_level_commands` | host → ext | 取首屏聚合命令                  | §6.1 |
| `fallback_commands`  | host → ext | 取"无匹配时"的兜底命令             | §6.2 |
| `get_items`          | host → ext | 按 `page_id` **全量拉取**当前页项 | §6.3 |
| `get_command`        | host → ext | 按 id 取回真实命令（**桩复热**）     | §6.4 |
| `invoke`             | host → ext | 执行命令                     | §6.5 |
| `close`              | host → ext | 释放扩展（优雅退出）               | §6.6 |
| `host/show_status`   | ext → host | 请宿主显示状态/Toast            | §7.2 |
| `host/set_clipboard` | ext → host | 请宿主写剪贴板                  | §7.3 |
| `host/open_url`      | ext → host | 请宿主打开 URL                | §7.4 |

**通知类（2 个，无 `id`、不得有响应）**

| 方法              | 方向         | 用途              | 章节   |
| --------------- | ---------- | --------------- | ---- |
| `initialized`   | ext → host | 扩展就绪（可选）        | §5.2 |
| `items_changed` | ext → host | "集合变了"，宿主随后全量拉取 | §7.1 |

> **为什么 `fallback_commands` 单列**：设计文档 §6.3 规定"含顶层 `IFallbackProvider` 能力者一律视为 **fresh**"，宿主要靠 `FallbackCommands()` **是否非空**来判定。若不设此方法，fresh 判定在协议层无落点。
>
> **命名注记**：`IFallbackProvider` 是 dd-run 的命名；上游 SDK（`v0.101.2362.0`）原名 `IFallbackHandler`（见 `Microsoft.CommandPalette.Extensions.idl` 第 390 行）。本文档涉及"上游如此"的表述一律指 dd-run 重命名后的口径。

---

## 2. 传输层

### 2.1 通道

- 宿主 spawn 扩展进程，通过**子进程的 stdin 写入、stdout 读取**。
- **stdout 只允许出现协议消息**，任何日志、调试输出一律走 stderr（见 §2.5）。
- 宿主与扩展均**不得**假设对端消息的到达顺序与发送顺序在跨请求间一致（但同一请求内响应必然晚于请求）。

### 2.2 成帧：NDJSON

每条消息是**一行紧凑 JSON 对象**，以单个 `\n`（LF, `0x0A`）结尾：

```
{"jsonrpc":"2.0","id":1,"method":"top_level_commands","params":{}}\n
```

规则：

1. **一行一条消息**，行尾单个 `\n`；行首**不得**有 `\r`（若收到 CRLF，接收方应先剥离行尾 `\r`）。
2. JSON 内部**不得出现裸换行**——JSON 字符串中的换行必须转义为 `\n`（两个字符），序列化器天然保证这一点；因此"按行切分"是安全的。
3. 编码为 **UTF-8**。
4. 不允许 pretty-print 的多行 JSON（会被解析成多条不完整的行）。
5. 空行应被**忽略**（不视为错误），以便对端写入容错。

> **为什么选 NDJSON**：协议 payload 全是几十到几百字节的文本，无二进制附件需求；NDJSON 让两端的 I/O 循环都简化为"按行读写"，调试时 `tail -f` 即可肉眼读协议流。日后若有二进制需求，可在握手时协商升级（见 §5.1 的 `transport` 字段）。

### 2.3 消息大小上限

- **默认单条消息上限 1 MiB（1 048 576 字节）**，握手时由宿主通过 `transport.max_message_bytes` 告知，扩展可回更低值。
- 收到超过上限的消息：接收方应回一个 `-32600 Invalid Request` 错误（若无法解析 `id` 则 `id` 为 `null`），**并关闭连接**——继续读取可能导致流错位。

### 2.4 读写循环要求

两端都必须按**增量缓冲**读取：一次 `read` 可能返回半条消息或多条消息，应累积到遇见 `\n` 再切出一条完整消息；未遇见 `\n` 的残留部分必须保留在缓冲区。

### 2.5 stderr 约定

- **stderr 只用于日志/诊断**，接收方不得解析。
- 宿主应捕获扩展 stderr 并在崩溃时写入自身日志（对应验收 A8 的可观测性）。
- 扩展**不得**把结构化协议信息写进 stderr。

---

## 3. 消息格式（JSON-RPC 2.0）

### 3.1 三种消息

**请求**（有 `id`，期待响应）：

```json
{"jsonrpc":"2.0","id":1,"method":"top_level_commands","params":{}}
```

**成功响应**（`id` 与请求一致）：

```json
{"jsonrpc":"2.0","id":1,"result":{"commands":[]}}
```

**错误响应**：

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found","data":{"method":"nope"}}}
```

**通知**（**无 `id` 字段**，接收方不得回复）：

```json
{"jsonrpc":"2.0","method":"items_changed","params":{"page_id":"calc.history"}}
```

### 3.2 字段规则

| 字段        | 规则                                            |
| --------- | --------------------------------------------- |
| `jsonrpc` | **必填**，恒为字符串 `"2.0"`。缺失或不为 `"2.0"` → `-32600` |
| `id`      | 请求/响应必填；**通知中必须不存在此字段**。类型限 **整数**（见 §3.3）    |
| `method`  | 请求/通知必填，字符串                                   |
| `params`  | 可选；若出现必须是**对象**（本协议不用数组形式）。缺省视为 `{}`          |
| `result`  | 成功响应必填；与 `error` **互斥**                       |
| `error`   | 失败响应必填；与 `result` **互斥**；结构见 §9.1             |

### 3.3 `id` 规则

- `id` 为**非负整数**（本协议不使用字符串 id，简化实现）。
- **两端 id 空间独立**：宿主与扩展各自从 `1` 开始自增。实现时必须用"发出方向"区分：收到带 `id` 的消息时，先看 `method`——若 `method` 是**自己能提供的**（对宿主而言是 `host/*`），这是对端发来的**请求**；否则这是**我发出请求的响应**。
- 未匹配到 in-flight 请求的响应：应记日志并**忽略**，不得崩溃。
- 通知无 `id`，**永不回复**；收到未知 method 的通知应忽略（不报错）。

### 3.4 不支持批处理

JSON-RPC 2.0 允许数组形式的批量请求，**本协议不支持**。收到数组 → 回 `-32600 Invalid Request`。

---


## 4. 生命周期状态机

```
                    ┌──────────────────────────────┐
                    ↓                              │
discovered → spawned → initializing → ready ⇄ busy ─┤
                    │                    │         │
                    │                    │      close (host→ext)
                    │                    │         ↓
                    │                    │      closed
                    │                    │         │
                    └─ 失败/超时 ─────────┴─────────┘
                                                   │
                                            进程退出 / 崩溃
                                                   ↓
                                        stub（宿主侧缓存态）
```

| 状态             | 含义                                    | 允许的转换                                             |
| -------------- | ------------------------------------- | ------------------------------------------------- |
| `discovered`   | 宿主扫到 manifest，未启动进程                   | → `spawned`                                       |
| `spawned`      | 进程已拉起，管道已建立，**未完成握手**                 | → `initializing`（发 `initialize`）/ → `closed`（超时）  |
| `initializing` | 已发 `initialize`，等待 result             | → `ready`（成功）/ → `closed`（超时或 `version_mismatch`） |
| `ready`        | 握手完成，空闲                               | → `busy` / → `closed`                             |
| `busy`         | 正处理请求                                 | → `ready` / → `closed`                            |
| `closed`       | 收到 `close` 响应或进程已退出                   | → `spawned`（再次激活时重新拉起）                            |
| `stub`         | **宿主侧命令项状态**（非进程状态）：closed 后回到"仅缓存数据" | → 点击时触发 `spawned`                                 |

> **并发**：MVP 阶段宿主对每个扩展**串行化请求**（同一时刻最多 1 个 in-flight 请求）。扩展侧可不保证串行，但宿主串行化能让超时与重连语义保持简单。

---

## 5. 握手


### 5.1 `initialize`（host → ext，请求）

宿主在进程拉起后**必须先**发送 `initialize`，收到成功 result 前不得发送其他请求。

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol_version":"1.0","host":{"name":"dd-run","version":"0.1.0","platform":"windows"},"transport":{"framing":"ndjson","max_message_bytes":1048576},"capabilities":["host/show_status","host/set_clipboard","host/open_url"],"locale":"zh-CN"}}
```

| 参数                            | 类型       | 必填 | 说明                                  |
| ----------------------------- | -------- | -- | ----------------------------------- |
| `protocol_version`            | string   | ✅  | 宿主支持的**最高**版本，如 `"1.0"`             |
| `host.name`                   | string   | ✅  | 恒为 `"dd-run"`                       |
| `host.version`                | string   | ✅  | 宿主版本（semver）                        |
| `host.platform`               | string   | ✅  | `"windows"` / `"macos"` / `"linux"` |
| `transport.framing`           | string   | ✅  | 成帧方式，v1.0 恒为 `"ndjson"`             |
| `transport.max_message_bytes` | integer  | ✅  | 单条消息上限                              |
| `capabilities`                | string[] | ✅  | 宿主支持的 `host/*` 方法名集合                |
| `locale`                      | string   | ❌  | BCP-47 语言标签，供扩展做本地化                 |

**成功响应：**

```json
{"jsonrpc":"2.0","id":1,"result":{"protocol_version":"1.0","provider":{"id":"com.example.calc","display_name":"Calculator","frozen":true,"has_fallback":false},"capabilities":[],"timeouts":{"get_items_ms":2000}}}
```

| 结果字段                    | 类型       | 必填 | 说明                                          |
| ----------------------- | -------- | -- | ------------------------------------------- |
| `protocol_version`      | string   | ✅  | 扩展**选定**的版本（见 §5.3）                         |
| `provider.id`           | string   | ✅  | provider 唯一 id，应与 manifest 的 `id` 一致        |
| `provider.display_name` | string   | ✅  | 展示名                                         |
| `provider.frozen`       | bool     | ✅  | 顶层命令是否可缓存（设计文档 §6.3）                        |
| `provider.has_fallback` | bool     | ✅  | 是否有兜底命令；`true` 时宿主**必须**视为 fresh（设计文档 §6.3） |
| `capabilities`          | string[] | ✅  | 扩展**需要用到**的 `host/*` 方法；宿主可据此拒绝不支持的扩展       |
| `timeouts.*`            | object   | ❌  | 扩展建议的超时值（毫秒），宿主可覆盖                          |

**失败响应**（版本不兼容）：

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32004,"message":"Unsupported protocol version","data":{"requested":"2.0","supported_versions":["1.0"]}}}
```

### 5.2 `initialized`（ext → host，通知）

扩展在返回 `initialize` 的 result **之后**可发送，表示自身已就绪（例如后台索引已建好）。**可选**——宿主以 `initialize` 的 result 作为就绪判定依据；收到 `initialized` 只用于触发一次额外的 `items_changed` 拉取。

```json
{"jsonrpc":"2.0","method":"initialized","params":{}}
```

### 5.3 版本协商规则

1. 宿主发送它支持的**最高**版本（v1.0 时为 `"1.0"`）。
2. 扩展必须回一个**不高于**宿主所发版本的版本；v1.0 阶段即回 `"1.0"`。
3. 若扩展不支持宿主发来的主版本，回 `-32004 version_mismatch`，`data.supported_versions` 列出自己支持的版本。
4. 若扩展回的版本宿主**不认识**（高于宿主所发，或格式非法），宿主应回 `close`、终止进程，并把该扩展标记为不可用。
5. **`MAJOR` 不兼容递增，`MINOR` 向后兼容递增**：1.0 → 1.1 时，只新增可选方法/可选字段，老扩展无需改动。

---

## 6. 方法：host → extension

### 6.1 `top_level_commands`

取 provider 的顶层命令项，用于首屏聚合（对应设计文档 §5.1 `TopLevelCommands()`、§5.4 步骤 4）。

```json
{"jsonrpc":"2.0","id":2,"method":"top_level_commands","params":{}}
```

```json
{"jsonrpc":"2.0","id":2,"result":{"commands":[{"id":"calc.eval","title":"Calculator","subtitle":"Evaluate an expression","icon":{"type":"glyph","value":"\uE8C8"},"section":"Tools","tags":["math"],"text_to_suggest":"calc ","command":{"kind":"invoke"}}]}}
```

| 结果字段       | 类型            | 说明              |
| ---------- | ------------- | --------------- |
| `commands` | CommandItem[] | 顶层命令项；**可为空数组** |

> **缓存语义**：仅当 `provider.frozen == true` 时，宿主才会把该结果缓存到磁盘并在冷启动时作为**桩**渲染（设计文档 §6.3）。缓存必须带扩展版本号，扩展升级即失效。

### 6.2 `fallback_commands`

取"搜索无匹配时"的兜底命令（对应设计文档 §5.1 `FallbackCommands()`、§5.3）。

```json
{"jsonrpc":"2.0","id":3,"method":"fallback_commands","params":{}}
```

```json
{"jsonrpc":"2.0","id":3,"result":{"commands":[{"id":"calc.eval.query","title":"Calculate “{query}”","subtitle":"Evaluate with Calculator","command":{"kind":"invoke"}}]}}
```

| 结果字段       | 类型            | 说明                   |
| ---------- | ------------- | -------------------- |
| `commands` | CommandItem[] | 兜底命令项；**空数组表示无兜底能力** |

> `title` 中的 `{query}` 为占位符，宿主渲染时替换为当前搜索词。宿主**必须**以此结果**非空**作为"该 provider 具备兜底能力（fresh）"的判定依据（设计文档 §6.3）。

### 6.3 `get_items`

按 `page_id` **全量拉取**当前页的列表项（对应设计文档 §5.5 的"变更事件 + 全量拉取"模式）。

```json
{"jsonrpc":"2.0","id":4,"method":"get_items","params":{"page_id":"calc.history","search_text":"3.14"}}
```

| 参数            | 类型     | 必填 | 说明                                                   |
| ------------- | ------ | -- | ---------------------------------------------------- |
| `page_id`     | string | ✅  | 页标识，来自 `CommandItem.command.page_id` 或 `GoToPage` 结果 |
| `search_text` | string | ❌  | 当前搜索词；扩展自行决定是否过滤                                     |

```json
{"jsonrpc":"2.0","id":4,"result":{"items":[{"id":"h1","title":"3.14159","subtitle":"π","command":{"kind":"invoke"}}],"has_more_items":false,"is_loading":false}}
```

| 结果字段             | 类型            | 说明                                     |
| ---------------- | ------------- | -------------------------------------- |
| `items`          | CommandItem[] | **全量**当前项，不含增量                         |
| `has_more_items` | bool          | 是否还有更多；为 `true` 时宿主可再次调用并带更大的 `offset` |
| `is_loading`     | bool          | 扩展是否仍在后台加载（宿主可显示 Loading 态）            |

> **禁止**：协议层**不得**流式推送增量集合（验收 A9）。列表变化一律走 `items_changed` 通知 + 宿主重新 `get_items`。

### 6.4 `get_command`

按 id 取回真实命令，用于 **frozen 桩复热**（对应设计文档 §6.3）。

```json
{"jsonrpc":"2.0","id":5,"method":"get_command","params":{"id":"calc.eval"}}
```

```json
{"jsonrpc":"2.0","id":5,"result":{"command":{"id":"calc.eval","title":"Calculator","subtitle":"Evaluate an expression","command":{"kind":"invoke"}}}}
```

- 找不到时 `result.command` 为 `null`（**不是错误**）：

```json
{"jsonrpc":"2.0","id":5,"result":{"command":null}}
```

> **复热链路**：用户点击 frozen 桩 → 宿主 spawn 进程 → `initialize` → `get_command` → 取回真实命令后执行。失败或超时则回退 stub 状态并向用户报错（验收 A6）。

### 6.5 `invoke`

执行一条命令（对应设计文档 §5.1 `Invoke(sender)`、§5.4 步骤 5）。

```json
{"jsonrpc":"2.0","id":6,"method":"invoke","params":{"id":"calc.eval","sender":"top_level","context":{"query":"1+1"}}}
```

| 参数                         | 类型     | 必填 | 说明                                                               |
| -------------------------- | ------ | -- | ---------------------------------------------------------------- |
| `id`                       | string | ✅  | 命令 id                                                            |
| `sender`                   | string | ✅  | `"top_level"` / `"list_item"` / `"context_menu"`（设计文档 §5.4 步骤 6） |
| `context.query`            | string | ❌  | 当前搜索词                                                            |
| `context.selected_item_id` | string | ❌  | 当 `sender` 为 `list_item` / `context_menu` 时的目标项 id               |
| `context.form_data`        | object | ❌  | 表单提交内容（`FormPage` 场景）                                            |

```json
{"jsonrpc":"2.0","id":6,"result":{"result":{"kind":"ShowToast","args":{"message":"= 2","duration_ms":2000}}}}
```

`result` 字段为 **CommandResult**，见 §8.3（**8 种 Kind**，对应验收 A4）。

### 6.6 `close`

优雅释放扩展（对应设计文档 §5.1 `IClosable`、§6.3 的"释放 COM 引用"等价物）。

```json
{"jsonrpc":"2.0","id":7,"method":"close","params":{}}
```

```json
{"jsonrpc":"2.0","id":7,"result":{}}
```

后置规则：

1. 宿主发 `close` 后**不再期待其他响应**，等待进程退出。
2. 扩展应在返回 result 后尽快自行退出（建议 ≤ 1s）。
3. 若超时未退出，宿主 **SIGKILL / TerminateProcess** 强杀。
4. 关闭后该 provider 的命令项重新标记为 **stub**（设计文档 §6.3）。

---

## 7. 方法：extension → host

### 7.1 `items_changed`（通知）

扩展告知"我的某个页的集合变了"（对应设计文档 §5.5 `INotifyItemsChanged`）。**不带数据**——宿主收到后自行调 `get_items` 全量拉取。

```json
{"jsonrpc":"2.0","method":"items_changed","params":{"page_id":"calc.history"}}
```

| 参数        | 类型     | 必填 | 说明                |
| --------- | ------ | -- | ----------------- |
| `page_id` | string | ❌  | 变化的页；缺省表示"顶层命令变了" |

宿主行为：

- `page_id` 对应的页**当前可见** → 立即 `get_items` 刷新；
- 不可见 → 标记脏，待该页可见时再拉；
- 顶层变化 → 重新 `top_level_commands`。

> **限流**：宿主应对高频 `items_changed` 做合并（如 100ms 窗口内的多次通知合并为一次拉取）。

### 7.2 `host/show_status`（请求）

请宿主展示状态提示 / Toast（对应设计文档 §5.1 `IExtensionHost.ShowStatus`、§6.4）。

**参数**

| 参数            | 类型      | 必填 | 说明                                                  |
| ------------- | ------- | -- | --------------------------------------------------- |
| `message`     | string  | ✅  | 文本                                                  |
| `state`       | string  | ❌  | `"info"`（默认）/ `"success"` / `"warning"` / `"error"` |
| `duration_ms` | integer | ❌  | 显示时长；`0` 表示常驻直到被替换                                  |

```json
{"jsonrpc":"2.0","id":1,"method":"host/show_status","params":{"message":"Copied to clipboard","state":"success","duration_ms":2000}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{}}
```

### 7.3 `host/set_clipboard`（请求）

请宿主写剪贴板（设计文档 §6.4：后台扩展线程无法直接用标准剪贴板）。

```json
{"jsonrpc":"2.0","id":1,"method":"host/set_clipboard","params":{"text":"3.14159"}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{}}
```

### 7.4 `host/open_url`（请求）

请宿主用系统默认方式打开 URL。

```json
{"jsonrpc":"2.0","id":1,"method":"host/open_url","params":{"url":"https://example.com/search?q=dd-run"}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{}}
```

> **能力前置**：扩展只能在 `initialize` 的 `capabilities` 中声明过的 `host/*` 方法才会被宿主响应；未声明而调用 → `-32601 Method not found`（验收 A12）。

---

## 8. 数据模型（JSON 层）

> 字段与设计文档 §4.4（列表项组成）、§5.3（数据模型）逐项对应。所有未在下方列出的字段均视为**可选扩展字段**，接收方应忽略未知字段。

### 8.1 CommandItem

```json
{"id":"calc.eval","title":"Calculator","subtitle":"Evaluate an expression","icon":{"type":"glyph","value":"\uE8C8"},"section":"Tools","tags":["math","builtin"],"details":{"title":"Calculator","body":"Evaluate arithmetic expressions."},"text_to_suggest":"calc ","more_commands":[{"id":"calc.copy","title":"Copy result","command":{"kind":"invoke"}}],"command":{"kind":"invoke"}}
```

| 字段                | 类型            | 必填 | 说明                                |
| ----------------- | ------------- | -- | --------------------------------- |
| `id`              | string        | ✅  | provider 内唯一；跨进程按 id 寻址           |
| `title`           | string        | ✅  | 主标题                               |
| `subtitle`        | string        | ❌  | 副标题                               |
| `icon`            | Icon          | ❌  | 见 §8.6                            |
| `section`         | string        | ❌  | 分组名，宿主据此分组渲染                      |
| `tags`            | string[]      | ❌  | 标签，宿主以 chip 展示（设计文档 §4.4）         |
| `details`         | Details       | ❌  | 右侧详情面板内容（设计文档 §4.5 `ShowDetails`） |
| `text_to_suggest` | string        | ❌  | 选中后回填搜索框的文本（设计文档 §4.4）            |
| `more_commands`   | CommandItem[] | ❌  | 上下文菜单项，可嵌套（设计文档 §5.6）             |
| `command`         | CommandRef    | ✅  | 该命令的执行目标，见 §8.2                   |

### 8.2 CommandRef

决定"选中这一项会发生什么"（设计文档 §5.3 `IListItem ├─ Command`）。

```json
{"kind":"invoke"}
```

```json
{"kind":"page","page_id":"calc.history"}
```

| `kind`     | 含义                                     | 附加字段          |
| ---------- | -------------------------------------- | ------------- |
| `"invoke"` | 选中即执行 → 宿主调 `invoke`                   | —             |
| `"page"`   | 选中进入嵌套页 → 宿主先 `get_items(page_id)` 再渲染 | `page_id`（必填） |


### 8.3 CommandResult（**8 种 Kind**）

对应设计文档 §5.2，宿主按 `kind` 驱动页面栈与可见性。**验收 A4 要求单测覆盖全部 8 种。**

| `kind`      | 含义           | `args`                                                                         |
| ----------- | ------------ | ------------------------------------------------------------------------------ |
| `Dismiss`   | 关闭面板         | —                                                                              |
| `GoHome`    | 回到根视图        | —                                                                              |
| `GoBack`    | 返回上一级        | —                                                                              |
| `Hide`      | 隐藏（不关闭，保留状态） | —                                                                              |
| `KeepOpen`  | 保持打开         | —                                                                              |
| `GoToPage`  | 跳转到某页        | `{"page_id":"..."}`                                                            |
| `ShowToast` | 弹提示          | `{"message":"...","duration_ms":2000}`                                         |
| `Confirm`   | 需二次确认        | `{"title":"...","description":"...","confirm_label":"...","is_critical":true}` |

**8 种 Kind 的示例（逐条对应上表，供实现与单测照抄）：**

```json
{"kind":"Dismiss"}
```

```json
{"kind":"GoHome"}
```

```json
{"kind":"GoBack"}
```

```json
{"kind":"Hide"}
```

```json
{"kind":"KeepOpen"}
```

```json
{"kind":"GoToPage","args":{"page_id":"calc.history"}}
```

```json
{"kind":"ShowToast","args":{"message":"Copied to clipboard","duration_ms":2000}}
```

```json
{"kind":"Confirm","args":{"title":"Delete entry?","description":"This cannot be undone.","confirm_label":"Delete","is_critical":true}}
```

> `Confirm` 的用户确认结果**不通过本协议回传**：宿主确认后重新发一次 `invoke`，并在 `params.context.confirmed = true` 中带上确认标记。

### 8.4 Sender

`invoke` 的 `params.sender` 取值（设计文档 §5.4 步骤 6）：

| 值                | 场景           |
| ---------------- | ------------ |
| `"top_level"`    | 从首屏顶层命令触发    |
| `"list_item"`    | 从嵌套列表页的某一项触发 |
| `"context_menu"` | 从上下文菜单项触发    |

### 8.5 Page

`get_items` 返回的页元信息（当前随 items 一并返回；设计文档 §4.5）。

```json
{"type":"list","page_id":"calc.history","title":"History","placeholder_text":"Search history","is_loading":false,"show_details":true,"has_more_items":false,"grid":{"columns":4}}
```

| 字段                 | 类型     | 说明                                                         |
| ------------------ | ------ | ---------------------------------------------------------- |
| `type`             | string | `"list"` / `"detail"` / `"form"` / `"markdown"`——**4 类页面** |
| `page_id`          | string | 页标识                                                        |
| `title`            | string | 页标题                                                        |
| `placeholder_text` | string | 搜索框占位文本                                                    |
| `is_loading`       | bool   | 宿主据此显示 Loading 态（设计文档 §5.1 `IPage.IsLoading`）              |
| `show_details`     | bool   | 是否展示右侧详情面板                                                 |
| `has_more_items`   | bool   | 配合 `LoadMore` 语义                                           |
| `grid`             | object | **非空时以网格渲染**——见下方说明                                        |
| `empty_content`    | object | 列表为空时的内容，见 §8.7                                            |

> **Grid 不是独立页面类型**：`grid` 是 `list` 的一种**渲染模式**，由 `ListPage.GridProperties` 控制（设计文档 §4.5 / §5.1）。因此页面类型为 **4 类**（list / detail / form / markdown）+ 1 种渲染模式（grid），而非 5 类。

### 8.6 Icon

```json
{"type":"glyph","value":"\uE8C8"}
```

```json
{"type":"path","value":"/Users/me/icons/calc.png"}
```

```json
{"type":"url","value":"https://example.com/icon.png"}
```

| `type`    | `value` 含义                   |
| --------- | ---------------------------- |
| `"glyph"` | 字体图标码位（如 Segoe Fluent Icons） |
| `"path"`  | 本地文件路径                       |
| `"url"`   | 远程 URL（宿主自行缓存）               |

### 8.7 Details / EmptyContent

**Details**（右侧详情面板）：

```json
{"title":"Calculator","body":"Evaluate arithmetic expressions.","metadata":[{"key":"Version","value":"1.0.0"}]}
```

**EmptyContent**（列表为空时；设计文档 §5.1 `IListPage.EmptyContent`）：

```json
{"title":"No results","body":"Try a different query.","icon":{"type":"glyph","value":"\uE710"},"command":{"kind":"invoke"}}
```

其中 `command` 可选，表示空态上附带的行动按钮（如"清除筛选"）。

---

## 9. 错误

### 9.1 错误对象

```json
{"code":-32602,"message":"Invalid params","data":{"missing":["page_id"]}}
```

| 字段        | 类型      | 必填 | 说明                         |
| --------- | ------- | -- | -------------------------- |
| `code`    | integer | ✅  | 见 §9.2                     |
| `message` | string  | ✅  | 简短英文描述（**面向开发者**，不直接展示给用户） |
| `data`    | any     | ❌  | 结构化补充信息                    |

### 9.2 错误码表

**JSON-RPC 2.0 标准码**

| 码        | 名称               | 触发                                                  |
| -------- | ---------------- | --------------------------------------------------- |
| `-32700` | Parse error      | 收到无法解析为 JSON 的行                                     |
| `-32600` | Invalid Request  | 缺 `jsonrpc` / 非 `"2.0"` / `id` 类型非法 / 批处理数组 / 消息超上限 |
| `-32601` | Method not found | method 不存在，或扩展未声明该 `host/*` 能力                      |
| `-32602` | Invalid params   | 必填参数缺失或类型错误                                         |
| `-32603` | Internal error   | 扩展/宿主内部异常                                           |

**dd-run 自定义码（-32000 起）**

| 码        | 名称                     | 触发                                | 建议宿主行为                |
| -------- | ---------------------- | --------------------------------- | --------------------- |
| `-32001` | `extension_timeout`    | 请求超时未响应                           | 记日志；可重试一次；仍失败则标 stub  |
| `-32002` | `command_not_found`    | `invoke` / `get_command` 的 id 不存在 | 提示用户；从列表移除该项          |
| `-32003` | `provider_unavailable` | 扩展进程已退出或不可用                       | 回退 stub；下次激活时重新 spawn |
| `-32004` | `version_mismatch`     | 协议版本不兼容                           | 关闭进程；标记扩展不可用          |
| `-32005` | `page_not_found`       | `page_id` 不存在或已失效                 | 返回上一级；刷新页面栈           |

> `-32002 command_not_found` 与 `get_command` **返回 `command: null`** 的区别：前者用于 `invoke` 一个确实不存在的 id（视为错误）；后者是 `get_command` 的**正常结果**（桩已失效，回退 stub）。

### 9.3 错误处置通则

- 错误**不是**致命的：收到错误响应后连接应继续保持（除 `-32600` 消息超上限与 `-32004` 版本不兼容外）。
- 对端**不得**因为一次错误就退出进程。
- 宿主应把 `-32603` 与 `-32001` 写入日志，用于诊断（验收 A8）。

---

## 10. 超时

| 阶段                   | 默认值      | 说明               |
| -------------------- | -------- | ---------------- |
| 进程启动                 | 3000 ms  | spawn 后到管道就绪     |
| `initialize`         | 5000 ms  | 含扩展自身初始化（如建索引）   |
| `top_level_commands` | 3000 ms  | —                |
| `fallback_commands`  | 2000 ms  | —                |
| `get_items`          | 2000 ms  | 首屏路径上，**热路径**    |
| `get_command`        | 5000 ms  | 含冷启动进程的 spawn 开销 |
| `invoke`             | 10000 ms | 命令可能耗时（如启动应用）    |
| `close`              | 1000 ms  | 超时即强杀            |

- 数值为**默认建议值**，宿主可配置；扩展可在 `initialize` 的 `result.timeouts` 中建议更宽的值。
- 超时即视为失败：宿主应答 `-32001 extension_timeout` 给调用方（UI 层），并**丢弃**该请求——但若之后收到迟到的响应，应记日志后忽略。
- **心跳**：v1.0 **不做心跳**。进程存活以"子进程是否退出"为准，协议层不引入 `ping`。

---

## 11. 崩溃与恢复

对应验收 **A8**（扩展崩溃后宿主不退出、可恢复）。

**宿主侧检测**：

1. 读取 stdout 遇 **EOF**，或等待子进程退出得到非 0 退出码 → 判定崩溃。
2. 所有 in-flight 请求**立即**以 `-32003 provider_unavailable` 失败，UI 不得卡住。
3. 若该 provider 为 `frozen` **且有磁盘缓存** → 命令项回退为 **stub**，保留在列表中；  
   若无缓存 → 从当前列表移除该 provider 的项（不删除 manifest）。
4. 宿主**继续正常运行**（绝不退出）。

**恢复**：

1. 用户再次点击该 stub 项 → 宿主重新 `spawn` → `initialize` → `get_command` → 执行。
2. 连续崩溃 N 次（建议 N=3，可配置）后，宿主应在 UI 上标记该扩展"暂时不可用"，并在**宿主重启**或用户手动重试后才再次尝试。

**扩展侧义务**：

- 不得在崩溃后残留子进程或临时文件；
- 应把关键异常写 stderr 后再退出（便于宿主记录日志）。

---

## 12. 并发与顺序

- MVP：宿主对**单个扩展串行化**请求（同时最多 1 个 in-flight）。
- 宿主对**多个扩展**应并行处理（首屏聚合时并行拉 `top_level_commands`）。
- `items_changed` 通知可在任意时刻到达，包括某请求处理中；宿主应合并处理（见 §7.1）。
- 响应**可以**乱序（若未来放开串行化），实现不得依赖顺序。

---

## 13. 协议演进

- 版本格式 `MAJOR.MINOR`，`MAJOR` 不兼容、`MINOR` 向后兼容。
- **新增可选方法 / 可选字段** → `MINOR` 递增，老实现无需改动。
- **删除或改变既有语义** → `MAJOR` 递增，走 §5.3 的协商与拒绝路径。
- 接收方**必须忽略未知字段**（不得因未知字段报错）——这是 `MINOR` 演进的基础。
- 变更本协议须同步更新：[`cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md) §8.2 的协议注释、本文件的 §1.3 方法总览与版本号。

---


## 14. 附录：完整会话示例

一个 Calculator 扩展从拉起到执行一次命令的完整协议流（`→` 为 host→ext，`←` 为 ext→host）：

```
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol_version":"1.0","host":{"name":"dd-run","version":"0.1.0","platform":"windows"},"transport":{"framing":"ndjson","max_message_bytes":1048576},"capabilities":["host/show_status","host/set_clipboard","host/open_url"],"locale":"zh-CN"}}
← {"jsonrpc":"2.0","id":1,"result":{"protocol_version":"1.0","provider":{"id":"com.example.calc","display_name":"Calculator","frozen":true,"has_fallback":false},"capabilities":["host/set_clipboard"]}}
← {"jsonrpc":"2.0","method":"initialized","params":{}}
→ {"jsonrpc":"2.0","id":2,"method":"top_level_commands","params":{}}
← {"jsonrpc":"2.0","id":2,"result":{"commands":[{"id":"calc.eval","title":"Calculator","subtitle":"Evaluate an expression","section":"Tools","tags":["math"],"text_to_suggest":"calc ","command":{"kind":"invoke"}}]}}
→ {"jsonrpc":"2.0","id":3,"method":"fallback_commands","params":{}}
← {"jsonrpc":"2.0","id":3,"result":{"commands":[]}}
→ {"jsonrpc":"2.0","id":4,"method":"get_command","params":{"id":"calc.eval"}}
← {"jsonrpc":"2.0","id":4,"result":{"command":{"id":"calc.eval","title":"Calculator","command":{"kind":"invoke"}}}}
→ {"jsonrpc":"2.0","id":5,"method":"invoke","params":{"id":"calc.eval","sender":"top_level","context":{"query":"1+1"}}}
← {"jsonrpc":"2.0","id":1,"method":"host/set_clipboard","params":{"text":"2"}}
← {"jsonrpc":"2.0","id":5,"result":{"result":{"kind":"ShowToast","args":{"message":"= 2","duration_ms":2000}}}}
→ {"jsonrpc":"2.0","id":6,"method":"close","params":{}}
← {"jsonrpc":"2.0","id":6,"result":{}}
```

> 注意第一条 `host/set_clipboard`（`id:10`）是**扩展在 `invoke` 处理过程中反向发起的请求**——它的 `id` 属扩展自己的 id 空间，且**在 `invoke` 的响应之前**到达。宿主必须能同时处理"我发出请求的响应"与"对端发来的请求"（见 §3.3）。
