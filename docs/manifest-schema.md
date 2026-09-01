# dd-run 扩展清单 schema v1.0

> **状态**：草案冻结（v1.0）——与 [`protocol.md`](./protocol.md) 配套使用。
> **上游依据**：[`cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md) §6.1（扩展发现，MVP 默认 = 清单文件扫描）、§6.3（frozen）。

---

## 1. 概述

每个扩展在宿主的**扩展目录**下放一个 JSON 清单文件。宿主启动时扫描该目录，逐个读取并校验；通过校验的扩展才会被记录为 `discovered` 状态（协议状态机见 [`protocol.md`](./protocol.md) §4）。

清单回答三件事：

1. **你是谁**（`id` / `name` / `version`）；
2. **怎么启动你**（`entry`）；
3. **你有什么特性**（`frozen` / `capabilities` / `platforms`）。

---

## 2. 文件位置

宿主扫描以下目录中的 `*.json` 文件（**不递归子目录**）：

| 平台 | 扩展目录 |
|---|---|
| **Linux** | `$XDG_CONFIG_HOME/dd-run/extensions.d/`；未设 `XDG_CONFIG_HOME` 时为 `~/.config/dd-run/extensions.d/` |
| **macOS** | `~/Library/Application Support/dd-run/extensions.d/` |
| **Windows** | `%APPDATA%\dd-run\extensions.d\` |

- **文件名**任意，建议用 `<id>.json`（如 `com.example.calc.json`）便于排查。
- **加载顺序**：宿主按文件名字典序读取；顺序**不影响**首屏排序（首屏排序由 `CommandItem.section` 与宿主策略决定）。

---

## 3. 字段表

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `schema_version` | string | ✅ | — | 清单格式版本，v1.0 恒为 `"1.0"` |
| `id` | string | ✅ | — | 全局唯一 id，建议反向域名（`com.example.calc`）。**必须与 `initialize` 响应的 `provider.id` 一致** |
| `name` | string | ✅ | — | 人类可读名称 |
| `version` | string | ✅ | — | 扩展版本，**semver**（`MAJOR.MINOR.PATCH`）。宿主用它判定 frozen 缓存是否失效 |
| `description` | string | ❌ | `""` | 一句话描述 |
| `author` | string | ❌ | `""` | 作者 |
| `license` | string | ❌ | `""` | SPDX 标识符，如 `"MIT"` |
| `homepage` | string | ❌ | `""` | 主页 URL |
| `icon` | string | ❌ | — | 图标路径，见 §4 路径展开规则 |
| `entry` | object | ✅ | — | 启动配置（含必填的 `command`） |
| `entry.command` | string | ✅ | — | 可执行文件路径 |
| `entry.args` | string[] | ❌ | `[]` | 启动参数 |
| `entry.env` | object | ❌ | `{}` | 附加环境变量（字符串→字符串） |
| `entry.cwd` | string | ❌ | 清单所在目录 | 子进程工作目录 |
| `frozen` | bool | ❌ | `true` | 顶层命令是否不变、可磁盘缓存（设计文档 §6.3） |
| `capabilities` | string[] | ❌ | `[]` | 扩展要用的 `host/*` 方法，取值见 [`protocol.md`](./protocol.md) §1.3 |
| `platforms` | string[] | ❌ | 全部 | 支持的平台：`"windows"` / `"macos"` / `"linux"`；**不含当前平台则跳过** |
| `min_host_version` | string | ❌ | — | 要求的最低宿主版本（semver）。宿主版本低于此值则跳过 |

> **未知字段**：宿主**必须忽略**未知字段，不得报错——这是后续向后兼容演进的基础。

---

## 4. 路径展开规则

`entry.command`、`entry.cwd`、`icon` 中的路径按以下顺序展开：

| 记号 | 展开为 |
|---|---|
| `${EXT_DIR}` | 该清单文件所在目录 |
| `~` | 当前用户 home 目录 |
| 相对路径（不以 `/`、`~`、`${` 开头，非 Windows 盘符路径） | 相对 **该清单文件所在目录** |

**示例**（清单位于 `~/.config/dd-run/extensions.d/com.example.calc.json`）：

| 写法 | 展开结果（Linux） |
|---|---|
| `"bin/dd-run-calc"` | `~/.config/dd-run/extensions.d/bin/dd-run-calc` |
| `"${EXT_DIR}/bin/dd-run-calc"` | 同上 |
| `"~/tools/dd-run-calc"` | `~/tools/dd-run-calc` |
| `"/usr/local/bin/dd-run-calc"` | 原样（绝对路径不展开） |

---

## 5. 最小示例

可直接拷贝，仅需填 `entry.command`：

```json
{"schema_version":"1.0","id":"com.example.calc","name":"Calculator","version":"1.0.0","entry":{"command":"bin/dd-run-calc"}}
```

---

## 6. 完整示例

```json
{"schema_version":"1.0","id":"com.example.calc","name":"Calculator","version":"1.2.0","description":"Evaluate arithmetic expressions inline.","author":"example","license":"MIT","homepage":"https://example.com/dd-run-calc","icon":"${EXT_DIR}/icon.png","entry":{"command":"${EXT_DIR}/bin/dd-run-calc","args":["--serve"],"env":{"RUST_LOG":"info"},"cwd":"${EXT_DIR}"},"frozen":true,"capabilities":["host/set_clipboard","host/show_status"],"platforms":["windows","macos","linux"],"min_host_version":"0.1.0"}
```

---

## 7. 校验规则

宿主在扫描阶段对每个清单执行以下校验。**任一失败即跳过该扩展**（记日志、不崩溃、不影响其他扩展）：

| # | 校验 | 失败处理 |
|---|---|---|
| 1 | 文件是合法 JSON | 跳过，记 `parse_error` |
| 2 | `schema_version` 存在且宿主支持（v1.0 阶段为 `"1.0"`） | 跳过，记 `unsupported_schema` |
| 3 | 五个必填字段（`schema_version` / `id` / `name` / `version` / `entry.command`）均存在且类型正确 | 跳过，记 `missing_field` |
| 4 | `version` 为合法 semver | 跳过，记 `invalid_version` |
| 5 | `platforms` 未声明，或**包含当前平台** | 静默跳过（非错误） |
| 6 | `min_host_version` 未声明，或宿主版本 ≥ 该值 | 跳过，记 `host_too_old` |
| 7 | `id` 在已加载集合中**唯一** | 后加载者跳过，记 `duplicate_id` |
| 8 | 展开后的 `entry.command` **存在且可执行** | 跳过，记 `entry_not_executable` |
| 9 | `capabilities` 中不含未知方法名 | 跳过，记 `unknown_capability` |

> **校验 8 的时机**：建议在**扫描阶段只做路径存在性检查**，可执行性在首次 spawn 失败时才判定——避免启动时的文件系统开销拖慢冷启动（验收 A2）。

---

## 8. 与协议的关联

| 清单字段 | 协议对应 | 一致性要求 |
|---|---|---|
| `id` | `initialize` 响应的 `provider.id` | **必须一致**；不一致时宿主应以清单为准并记警告 |
| `version` | — | 作为 **frozen 缓存的失效键**：`version` 变化 → 磁盘缓存的桩失效 |
| `frozen` | `initialize` 响应的 `provider.frozen` | 清单值作为**预期值**；实际以扩展响应为准（扩展可自行降级为 fresh） |
| `capabilities` | `initialize` 的 `params.capabilities` | 清单声明扩展**需要**的，宿主 params 声明宿主**提供**的；交集为空时宿主可拒绝启动该扩展 |

---

## 9. 版本与演进

- 清单 `schema_version` 与协议版本**独立演进**。
- **新增可选字段** → `MINOR` 递增；**删除或改变既有字段语义** → `MAJOR` 递增。
- 宿主**必须忽略未知字段**（见 §3），保证老宿主能读新清单。
- 宿主遇到不支持的 `schema_version` 时**跳过该扩展并记日志**，不得崩溃。

---

## 10. 内置扩展如何注册

MVP 的 5 个内置扩展（Apps / Calc / System / WebSearch / Shell，见 [`cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md) §7 与 [`implementation.md`](./implementation.md)）**同样通过清单注册**——与第三方扩展走完全相同的路径，只是清单文件由安装器写入扩展目录。

这样做的收益：内置与第三方在宿主侧**没有任何特判代码**，协议与生命周期逻辑只需一套实现；调试内置扩展时可直接改清单指向本地构建产物。
