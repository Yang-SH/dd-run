//! 协议 v1.0 **方法名常量层**（单一来源）。
//!
//! 契约来源：`docs/protocol.md` §1.3「方法总览」（10 个请求 + 2 个通知）。
//!
//! 存在意义（优化项 O2 / `INDEX.md §5` P-18）：协议方法名此前以裸字符串字面量
//! 散落在分发（`match method { "invoke" => … }`）、RPC 调用（`call("get_items", …)`）
//! 与能力白名单（`["host/open_url"]`）三处；拼写漂移只在运行时暴露，协议演进需
//! 全仓手工搜改且易漏。本模块把方法名收敛为常量，全仓生产代码一律引用。
//!
//! ## 使用规约
//!
//! - **生产代码**（非 `#[cfg(test)]`）：一律引用本模块常量，禁止裸方法字面量。
//! - **测试与 JSON 样例**：**刻意保留**裸字面量——它们同时充当「独立交叉校验」，
//!   若常量被改错，引用常量的生产代码会与写死字面量的测试断言不一致而失败。
//! - `docs/protocol.md` 内 ```json 围栏里的 `"method"` 恒为字面量（SSOT 本体）。
//!
//! ## 命名约定
//!
//! | 类别 | 前缀 | 示例 |
//! |---|---|---|
//! | host → ext 请求（§5.1、§6.1–§6.6） | `METHOD_` | [`METHOD_INVOKE`] |
//! | ext → host 请求（§7.2–§7.4） | `METHOD_HOST_` | [`METHOD_HOST_OPEN_URL`] |
//! | 通知（§5.2、§7.1） | `NOTIFY_` | [`NOTIFY_ITEMS_CHANGED`] |

// ─── host → ext 请求（7 个）────────────────────────────────

/// §5.1 握手 + 版本协商。
pub const METHOD_INITIALIZE: &str = "initialize";

/// §6.1 取首屏聚合命令。
pub const METHOD_TOP_LEVEL_COMMANDS: &str = "top_level_commands";

/// §6.2 取「无匹配时」的兜底命令。
pub const METHOD_FALLBACK_COMMANDS: &str = "fallback_commands";

/// §6.3 按 `page_id` 全量拉取当前页项。
pub const METHOD_GET_ITEMS: &str = "get_items";

/// §6.4 按 id 取回真实命令（桩复热）。
pub const METHOD_GET_COMMAND: &str = "get_command";

/// §6.5 执行命令。
pub const METHOD_INVOKE: &str = "invoke";

/// §6.6 释放扩展（优雅退出）。
pub const METHOD_CLOSE: &str = "close";

// ─── ext → host 请求（3 个）────────────────────────────────

/// §7.2 请宿主显示状态/Toast。
pub const METHOD_HOST_SHOW_STATUS: &str = "host/show_status";

/// §7.3 请宿主写剪贴板。
pub const METHOD_HOST_SET_CLIPBOARD: &str = "host/set_clipboard";

/// §7.4 请宿主打开 URL。
pub const METHOD_HOST_OPEN_URL: &str = "host/open_url";

// ─── 通知（2 个，无 `id`、不得有响应）──────────────────────

/// §5.2 扩展就绪（可选）。
pub const NOTIFY_INITIALIZED: &str = "initialized";

/// §7.1 「集合变了」，宿主随后全量拉取。
pub const NOTIFY_ITEMS_CHANGED: &str = "items_changed";

// ─── 聚合视图 ──────────────────────────────────────────────

/// §7.4 宿主可提供的 `host/*` 方法全集。
///
/// 双重身份：既是 [`crate::messages::InitializeParams::capabilities`] 的语义来源，
/// 也是清单 `capabilities` 的白名单（`manifest-schema.md` §7 校验规则 9）。
pub const HOST_METHODS: [&str; 3] = [
    METHOD_HOST_SHOW_STATUS,
    METHOD_HOST_SET_CLIPBOARD,
    METHOD_HOST_OPEN_URL,
];

/// §3.3 对端请求判别前缀：带 `id` 且 `method` 以此开头 → 该消息是对端请求，不是通知。
pub const HOST_METHOD_PREFIX: &str = "host/";

/// 协议 v1.0 全部 12 个方法名（10 请求 + 2 通知）。
///
/// 供一致性测试与工具使用——**顺序即 `protocol.md` §1.3 表格顺序**，
/// 与文档的比对见 `tests/consistency.rs::method_constants_match_protocol_method_table`。
pub const ALL_METHODS: [&str; 12] = [
    METHOD_INITIALIZE,
    METHOD_TOP_LEVEL_COMMANDS,
    METHOD_FALLBACK_COMMANDS,
    METHOD_GET_ITEMS,
    METHOD_GET_COMMAND,
    METHOD_INVOKE,
    METHOD_CLOSE,
    METHOD_HOST_SHOW_STATUS,
    METHOD_HOST_SET_CLIPBOARD,
    METHOD_HOST_OPEN_URL,
    NOTIFY_INITIALIZED,
    NOTIFY_ITEMS_CHANGED,
];
