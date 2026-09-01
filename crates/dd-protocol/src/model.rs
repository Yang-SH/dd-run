//! §8 数据模型（JSON 层）的类型投影。
//!
//! 字段与 `docs/protocol.md` §8.1–§8.7 的字段表逐项对应：
//! 文档标 ✅ 的为必填，标 ❌ 的为 `Option`。

use serde::{Deserialize, Serialize};

/// §8.6 Icon：图标三态（glyph / path / url）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Icon {
    #[serde(rename = "type")]
    pub kind: IconKind,
    pub value: String,
}

/// §8.6 `Icon.type` 取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconKind {
    /// 字体图标码位（如 Segoe Fluent Icons）
    Glyph,
    /// 本地文件路径
    Path,
    /// 远程 URL（宿主自行缓存）
    Url,
}

/// §8.7 Details：右侧详情面板内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Details {
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<MetadataEntry>>,
}

/// §8.7 Details.metadata 的一项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataEntry {
    pub key: String,
    pub value: String,
}

/// §8.7 EmptyContent：列表为空时的内容。
/// `command` 可选，表示空态上附带的行动按钮。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyContent {
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandRef>,
}

/// §8.2 CommandRef：决定"选中这一项会发生什么"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandRef {
    /// 选中即执行 → 宿主调 `invoke`
    Invoke,
    /// 选中进入嵌套页 → 宿主先 `get_items(page_id)` 再渲染
    Page { page_id: String },
}

/// §8.1 CommandItem：列表中的一项。
/// `more_commands` 递归嵌套（上下文菜单，设计文档 §5.6）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandItem {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Details>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_to_suggest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more_commands: Option<Vec<CommandItem>>,
    pub command: CommandRef,
}

/// §8.3 CommandResult：命令执行结果，**8 种 Kind**（对应验收 A4）。
///
/// JSON 形如 `{"kind":"ShowToast","args":{...}}`；无参 Kind 不含 `args` 字段
/// （与文档 8 条示例逐条对应，单测照抄覆盖全部 8 种）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "args")]
pub enum CommandResult {
    /// 关闭面板
    Dismiss,
    /// 回到根视图
    GoHome,
    /// 返回上一级
    GoBack,
    /// 隐藏（不关闭，保留状态）
    Hide,
    /// 保持打开
    KeepOpen,
    /// 跳转到某页
    GoToPage { page_id: String },
    /// 弹提示
    ShowToast {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    /// 需二次确认（确认结果不回传，宿主带 `context.confirmed=true` 重新 invoke）
    Confirm {
        title: String,
        description: String,
        confirm_label: String,
        is_critical: bool,
    },
}

/// §8.4 Sender：`invoke` 的 `params.sender` 取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sender {
    /// 从首屏顶层命令触发
    TopLevel,
    /// 从嵌套列表页的某一项触发
    ListItem,
    /// 从上下文菜单项触发
    ContextMenu,
}

/// §8.5 Page：`get_items` 返回的页元信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "type")]
    pub kind: PageKind,
    pub page_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_loading: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_details: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more_items: Option<bool>,
    /// 非空时以网格渲染（Grid 是 list 的渲染模式，非独立页面类型）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridProperties>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_content: Option<EmptyContent>,
}

/// §8.5 `Page.type` 取值：**4 类页面**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    List,
    Detail,
    Form,
    Markdown,
}

/// §8.5 `grid`：网格渲染模式参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridProperties {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<u32>,
}
