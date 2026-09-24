# dd-run 扩展清单 schema v1.0

> **状态**：已冻结 ｜ **版本**：v1.0 ｜ **最后更新**：2026-09-13 ｜ 与 [`protocol.md`](./protocol.md) 配套使用；上手路径见 [`extensions.md`](./extensions.md)。
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

**便携 sidecar（v1.0 补充，M7 批次 7.5）**：宿主还会扫描**与宿主可执行文件同目录**的 `extensions.d/`（免安装分发布局 `dd-run-<版本>.exe + extensions.d/`，解压即用）。两处扫描结果按 `id` 去重：**用户数据目录优先**（用户手动放置的版本覆盖分发自带版本），sidecar 独有者追加；内置扩展仍最优先（由宿主 `merge_builtins` 保证；§7 规则 7 只负责单批扫描内的 id 去重）。`${EXT_DIR}` 在 sidecar 清单中解析为该 sidecar 目录。

- **文件名**任意，建议用 `<id>.json`（如 `com.example.calc.json`）便于排查。
- **加载顺序**：宿主按文件名字典序读取；顺序**不影响**首屏排序（首屏排序由 `CommandItem.section` 与宿主策略决定）。
- ⚠️ **目录异常处置（实现现状）**：目录不存在视为空目录（静默）；其他 IO 错误记 `dir_error` 后继续；单个清单读取失败的条目被**静默丢弃**（`manifest.rs:498-516`）。

---

## 3. 字段表

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `schema_version` | string | ✅ | — | 清单格式版本，v1.0 恒为 `"1.0"` |
| `id` | string | ✅ | — | 全局唯一 id，建议反向域名（`com.example.calc`）。**必须与 `initialize` 响应的 `provider.id` 一致** |
| `name` | string | ✅ | — | 人类可读名称 |
| `version` | string | ✅ | — | 扩展版本，**semver**（`MAJOR.MINOR.PATCH`）。宿主用它判定 frozen 缓存是否失效。⚠️ 实现现状：解析只收**三段纯数字**，prerelease/build 后缀（如 `1.0.0-beta`）判为非法 |
| `description` | string | ❌ | `""` | 一句话描述 |
| `author` | string | ❌ | `""` | 作者 |
| `license` | string | ❌ | `""` | SPDX 标识符，如 `"MIT"` |
| `homepage` | string | ❌ | `""` | 主页 URL |
| `icon` | string | ❌ | — | 图标路径。⚠️ **实现现状（核对至 2026-09-13）**：字段已定义但**无任何消费者**——宿主不展开、不读取，UI 亦未使用（仅 `command`/`cwd` 实际展开） |
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

`entry.command`、`entry.cwd`、`icon` 中的路径按以下顺序展开。⚠️ **实现现状（核对至 2026-09-13）**：实际仅对 `entry.command` / `entry.cwd` 执行展开；`icon` 无消费者（见 §3）。另 Windows 下 `command` 若不带扩展名，按「原样路径 → `.exe` → `.cmd` → `.bat`」顺序补全命中（同名无扩展名文件**优先**于带扩展名者，`manifest.rs:280-296`）。

| 记号 | 展开为 |
|---|---|
| `${EXT_DIR}` | 该清单文件所在目录 |
| `~` | 当前用户 home 目录。⚠️ 仅识别 `~` / `~/` / `~\`；`~user` 不展开，按相对路径拼到清单目录 |
| 相对路径（不以 `/`、`~`、`${` 开头，非 Windows 盘符路径） | 相对 **该清单文件所在目录**。⚠️ 实现额外把 `\` 开头（含 UNC）与 `C:x` 形式判为非相对，按原样使用（不拼清单目录） |

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
| 3 | 五个必填字段（`id` / `name` / `version` / `entry.command` 等）均存在且类型正确 | 跳过，记 `missing_field`。⚠️ `schema_version` 缺失在规则 2 即短路，实际不产生 `missing_field` |
| 4 | `version` 为合法 semver | 跳过，记 `invalid_version` |
| 5 | `platforms` 未声明，或**包含当前平台** | 静默跳过（非错误）。⚠️ 「静默」指不作错误处理；仍写入 `skipped`（`is_error=false`），CLI 以 `-` 打印 |
| 6 | `min_host_version` 未声明，或宿主版本 ≥ 该值 | 跳过，记 `host_too_old`。⚠️ `min_host_version` 本身非法时复用 `invalid_version`（一变体双用） |
| 7 | `id` 在已加载集合中**唯一** | 后加载者跳过，记 `duplicate_id`。⚠️ 「已加载集合」限**单次 `scan_dir`**；跨目录同 id 由合并层静默保留先到者，不记 `duplicate_id` |
| 8 | 展开后的 `entry.command` 存在（可执行性在首次 spawn 失败时才判定，见下方脚注） | 跳过，记 `entry_not_executable`。⚠️ 实现仅 `is_file()`，不查可执行位 |
| 9 | `capabilities` 中不含未知方法名 | 跳过，记 `unknown_capability` |

> **校验 8 的时机**：建议在**扫描阶段只做路径存在性检查**，可执行性在首次 spawn 失败时才判定——避免启动时的文件系统开销拖慢冷启动（验收 A2）。
>
> ⚠️ **实际短路顺序（核对至 2026-09-13）**：1 → 2 → 3 → 4 → 5 → 6 → 9 → 8（capabilities 检查先于 command 存在性）；规则 7 延后到 `scan_dir` 全部 load 完成后统一去重（`manifest.rs:313-384,518-529`）。

---

## 8. 与协议的关联

| 清单字段 | 协议对应 | 一致性要求 |
|---|---|---|
| `id` | `initialize` 响应的 `provider.id` | **必须一致**；不一致时宿主应以清单为准并记警告 |
| `version` | — | 作为 **frozen 缓存的失效键**：`version` 变化 → 磁盘缓存的桩失效 |
| `frozen` | `initialize` 响应的 `provider.frozen` | 清单值作为**预期值**；实际以扩展响应为准（扩展可自行降级为 fresh）。⚠️ 实现现状：内置注册写入 `host_frozen()`——含兜底命令者被宿主**主动降级为 `false`**，与 calc/websearch/shell 自述 `frozen=true` 相反；落盘门禁读取的也是清单侧 `frozen`（见 `protocol.md` §6.1 现状注） |
| `capabilities` | `initialize` 的 `params.capabilities` | 清单声明扩展**需要**的，宿主 params 声明宿主**提供**的；交集为空时宿主可拒绝启动该扩展 |

---

## 9. 版本与演进

- 清单 `schema_version` 与协议版本**独立演进**。
- **新增可选字段** → `MINOR` 递增；**删除或改变既有字段语义** → `MAJOR` 递增。
- 宿主**必须忽略未知字段**（见 §3），保证老宿主能读新清单。
- 宿主遇到不支持的 `schema_version` 时**跳过该扩展并记日志**，不得崩溃。
- **信任与哈希不属于本 schema**（S-05，2026-09-24）：宿主在扫描之后会另做一次**信任判定**
  （来源 + 首方白名单 + `%APPDATA%\dd-run\trust.json` 台账），未获批准的扩展**不会被拉起**。
  该台账是**宿主私有文件**、字段不对外开放，`schema_version` 不因此变动——即扩展作者无需
  为新字段做任何事，但要知道「清单合法 ≠ 一定被加载」。口径见
  [`extensions.md`](./extensions.md) §6.1 与 [`security-audit-2026-09-23.md`](./security-audit-2026-09-23.md) §4.4.1。

---

## 10. 内置扩展如何注册

> **本节已随实现演进更新至 2026-09-13（M7 → M9）**。MVP 设想「内置扩展也走清单扫描」已被现行实现取代；**M9 之后「编译期内嵌 + 子进程 spawn」也不再适用**。以下为当前口径。

**内置 5 扩展（Apps / Calc / System / WebSearch / Shell，见 [`cmdpal-platform-agnostic-design.md`](../cmdpal-platform-agnostic-design.md) §7）不通过清单文件注册**：

- 注册事实源 = `dd-host` 的 `builtin::BUILTINS` 硬编码表（id / 名称 / frozen / capabilities，与各扩展 `initialize` 自述对齐）；GUI 侧据此产出内置注册项，并靠 `merge_builtins` 并入聚合结果——**内置 id 最优先**，用户/sidecar 清单不能覆盖或顶替同名内置扩展。
- **运行形态 = 宿主进程内（in-process）**：内置扩展**不再各自 `spawn` 子进程**，而是由宿主进程内经 `dd_ext::serve_line` 直调。
- ⚠️ **`dd-gui::embedded` 模块现状（M9 B4 起）**：`build.rs` 的 `EMBED_EXES` 已清空为 `&[]`，`EMBEDDED` 恒为空表、`materialize()` 恒返回 `None`，宿主回退「exe 同目录发现」/ sidecar 路径。该模块保留为**将来内嵌 sidecar**（如 `dd-ext-search`）的扩展点；**不再有 5 个内置 exe 被内嵌或物化**，`%APPDATA%/dd-run/cache/embedded/` 亦不再被填充。
- ⚠️ **ADR-1 的适用边界**：进程隔离现在只覆盖**第三方扩展与 sidecar**（如 file-search）——内置扩展走 in-process，不 spawn、不共用子进程生命周期实现。详见 [`implementation.md`](./implementation.md) 的 ADR-1 修订表述与 [`m9-inprocess-builtins.md`](./m9-inprocess-builtins.md)。

**file-search（`com.ddrun.filesearch`）** 是目前唯一的清单注册型官方扩展（同目录另有 `com.example.sample.json`，属开发示例、不随包分发）：清单源码在 `examples/extensions.d/com.ddrun.filesearch.json`，由 `tools/package.sh` 归集进 `dist/extensions.d/` 作为 sidecar 随包分发（位置与优先级见 §2 便携 sidecar）；开发期可将该清单拷入用户扩展目录，并把 `entry.command` 指向本地构建产物进行调试。

**加载优先级**（同 `id` 去重、先者优先）：内置（`merge_builtins`）＞ 用户数据目录 ＞ 便携 sidecar。
