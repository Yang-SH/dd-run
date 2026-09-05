//! 纯文本函数（零 egui 依赖）：路径/URL 形态判断、动作 glyph 映射、
//! 搜索框占位文案、页脚动作文案。

use dd_gui::state::PanelItem;
use dd_protocol::model::CommandRef;

/// 右键菜单 glyph（Segoe Fluent/MDL2，10B.0 解剖图）。
pub(crate) const CTX_GLYPH_OPEN: char = '\u{E8E5}'; // OpenFile

pub(crate) const CTX_GLYPH_ADMIN: char = '\u{E7EF}'; // Admin（盾牌）

pub(crate) const CTX_GLYPH_FOLDER: char = '\u{E8DA}'; // FolderOpen

pub(crate) const CTX_GLYPH_COPY: char = '\u{E8C8}'; // Copy

pub(crate) const CTX_GLYPH_PLAY: char = '\u{E768}'; // Play

pub(crate) const CTX_GLYPH_LINK: char = '\u{E71B}'; // Link

/// 默认动作 glyph（10B.0）：进入页 = FolderOpen、打开类（apps/system/websearch）
/// = OpenFile、执行类 = Play。
pub(crate) fn default_action_glyph(item: &PanelItem) -> char {
    if matches!(item.command, CommandRef::Page { .. }) {
        return CTX_GLYPH_FOLDER;
    }
    let short = item
        .ext_id
        .strip_prefix("com.ddrun.")
        .unwrap_or(&item.ext_id);
    match short {
        "apps" | "system" | "websearch" => CTX_GLYPH_OPEN,
        _ => CTX_GLYPH_PLAY,
    }
}

/// 判断文本是否为**绝对路径形态**（盘符 `X:\` 或 UNC `\\`），去除首尾引号/空白。
/// 仅做形态判断（无 I/O）——路径有效性在动作执行时校验并经 Toast 反馈。
pub(crate) fn path_like(s: &str) -> Option<String> {
    let s = s.trim().trim_matches('"');
    let b = s.as_bytes();
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    let unc = s.starts_with("\\\\");
    (drive || unc).then(|| s.to_string())
}

/// 判断文本是否为 http(s) URL 形态（「复制链接」门控）。
pub(crate) fn url_like(s: &str) -> Option<String> {
    let s = s.trim().trim_matches('"');
    (s.starts_with("http://") || s.starts_with("https://")).then(|| s.to_string())
}

/// ueli 式搜索栏（设计稿 01 界面 + 05 note）：46px 高、圆角 6、`input` 底色、
/// 1px `border-strong` 描边 + 底部 2px（聚焦时换 `accent` 下划线）、左侧 search
/// glyph 前缀（text-2 15px）、placeholder 用 `text-3`、输入 16px。
///
/// **实现要点**：先 `allocate_exact_size` 锁 46px 高度（`Frame::show` + `set_min_height`
/// 在 egui 0.36 + `TextEdit::desired_width(INFINITY)` 组合下会把 searchbar 撑成
/// 「列表剩余高度」——实拍 350px+、list 区被压成空。改成手工绘 fill/stroke/accent，
/// child Ui 用 `Layout::left_to_right(Align::Center)` 强制 46px 内容区）。
///
/// 返回 TextEdit 的 [`egui::Response`]（焦点请求/输入事件由调用方处理——
/// `want_focus` 唤起聚焦机制不变）。组件色一律经 [`theme::Palette`] 取，不写裸色值。
/// 嵌套页搜索框 placeholder（设计稿 §07.1「页标题」行：不占独立行，
/// 放入搜索框 placeholder）。空标题回落「筛选命令…」。
pub(crate) fn nested_search_placeholder(page_title: &str) -> String {
    if page_title.is_empty() {
        "筛选命令…".to_string()
    } else {
        format!("在「{page_title}」中筛选…")
    }
}

/// 页脚上下文动作文本（设计稿 §6.3，批次 4.1）。
///
/// 动词由 `CommandRef` 决定：`Page` → 进入类；`Invoke` → 执行类。
/// 宾语由 `ext_id` 决定——GUI 层硬编码映射表（去 `com.ddrun.` 前缀后匹配，
/// 与 [`aggregator`] 的类别映射同口径）；第三方/未知扩展回退「执行」。
/// 协议层零改动（C9/C13：`CommandItem` 无 kind，映射仅在本层）。
pub(crate) fn footer_action_text(item: &PanelItem) -> String {
    if matches!(item.command, CommandRef::Page { .. }) {
        return "进入".to_string();
    }
    let short = item
        .ext_id
        .strip_prefix("com.ddrun.")
        .unwrap_or(&item.ext_id);
    match short {
        "apps" => "打开应用",
        "system" => "打开设置",
        "shell" => "运行命令",
        "websearch" => "打开网页",
        "calc" => "计算",
        _ => "执行",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::item_with;

    // ── 批次 4.1：页脚上下文动作映射（C7，§6.3 映射表） ────────────

    // ── C 组批次 C1：嵌套页统一顶行（§07.1，验收 A1） ──────────────

    #[test]
    fn nested_placeholder_uses_page_title() {
        assert_eq!(
            nested_search_placeholder("应用列表"),
            "在「应用列表」中筛选…",
            "页标题进 placeholder（D2：不占独立行）"
        );
        assert_eq!(
            nested_search_placeholder(""),
            "筛选命令…",
            "空标题回落通用文案"
        );
    }

    #[test]
    fn footer_action_maps_five_builtin_extensions() {
        let cases = [
            ("com.ddrun.apps", "打开应用"),
            ("com.ddrun.calc", "计算"),
            ("com.ddrun.system", "打开设置"),
            ("com.ddrun.websearch", "打开网页"),
            ("com.ddrun.shell", "运行命令"),
        ];
        for (ext, expected) in cases {
            let item = item_with(ext, CommandRef::Invoke);
            assert_eq!(footer_action_text(&item), expected, "{ext} 动作映射");
        }
    }

    #[test]
    fn footer_action_falls_back_for_third_party_and_empty() {
        // 第三方未命中 → 执行；无 ext_id（如宿主兜底项之外的场合）→ 执行
        let third = item_with("com.acme.thing", CommandRef::Invoke);
        assert_eq!(footer_action_text(&third), "执行");
        let empty = item_with("", CommandRef::Invoke);
        assert_eq!(footer_action_text(&empty), "执行");
    }

    #[test]
    fn footer_action_page_commands_become_enter_verb() {
        // CommandRef::Page 优先于 ext_id 映射：任何来源的页命令都是「进入」
        let page = item_with(
            "com.ddrun.apps",
            CommandRef::Page {
                page_id: "sub".to_string(),
            },
        );
        assert_eq!(footer_action_text(&page), "进入");
        let page_unknown = item_with(
            "com.acme.thing",
            CommandRef::Page {
                page_id: "p".to_string(),
            },
        );
        assert_eq!(footer_action_text(&page_unknown), "进入");
    }

    #[test]
    fn default_action_glyph_follows_command_ref_and_ext() {
        // 进入页 = FolderOpen；打开类 = OpenFile；执行类 = Play
        let page = item_with(
            "com.ddrun.apps",
            CommandRef::Page {
                page_id: "sub".to_string(),
            },
        );
        assert_eq!(default_action_glyph(&page), CTX_GLYPH_FOLDER);
        let app = item_with("com.ddrun.apps", CommandRef::Invoke);
        assert_eq!(default_action_glyph(&app), CTX_GLYPH_OPEN);
        let calc = item_with("com.ddrun.calc", CommandRef::Invoke);
        assert_eq!(default_action_glyph(&calc), CTX_GLYPH_PLAY);
    }

    #[test]
    fn path_like_recognizes_drive_and_unc_only() {
        // 形态判断：盘符（大小写 / 正反斜杠）、UNC、引号包裹；其余形态不命中
        assert_eq!(
            path_like(r"C:\WINDOWS\system32\AgentService.exe").as_deref(),
            Some(r"C:\WINDOWS\system32\AgentService.exe")
        );
        assert_eq!(
            path_like(r#"  "d:/tools/7z.exe"  "#).as_deref(),
            Some(r#"d:/tools/7z.exe"#)
        );
        assert_eq!(
            path_like(r"\\server\share\app.exe").as_deref(),
            Some(r"\\server\share\app.exe")
        );
        assert_eq!(path_like("快捷方式"), None, "中文文案不是路径");
        assert_eq!(path_like("在浏览器中搜索"), None, "提示文案不是路径");
        assert_eq!(path_like(r"tools\7z.exe"), None, "相对路径不命中");
        assert_eq!(path_like("C:"), None, "裸盘符不命中");
    }

    #[test]
    fn url_like_recognizes_http_s_only() {
        assert_eq!(
            url_like("https://example.com/search?q=dd").as_deref(),
            Some("https://example.com/search?q=dd")
        );
        assert!(url_like("http://localhost:8080").is_some());
        assert_eq!(url_like("ftp://example.com"), None, "非 http(s) 不命中");
        assert_eq!(url_like("example.com"), None, "裸域名不命中");
    }
}
