//! 纯文本函数（零 egui 依赖）：路径/URL 形态判断、动作 glyph 映射、
//! 搜索框占位文案、页脚动作文案、i18n 文案表（v4.13 D38）。

use dd_gui::settings::Lang;
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

// ── i18n 文案表（v4.13 D38）────────────────────────────────────────

/// 静态文案表：`(key, zh, en)` 三元组。新增文案 = 表尾加一行
/// （`i18n_table_complete` 单测自动校验两列非空 + key 唯一）；调用点经 [`t`]
/// 取值。**不引入 fluent/gettext 等重依赖**（文案量 ~150 条、中英无复数规则、
/// 编译期表可测）。
///
/// 含运行时值的文案用 `{name}` 占位符（如 `{id}`/`{e}`/`{n}`/`{title}`），
/// 调用点 `.replace("{id}", …)` 填充——规避 `format!` 要求字面量格式串的
/// 限制，且中英词序可各自成句。**协议数据不进表**：`result_category` /
/// `section` 的中文值（"应用"/"命令"…）是协议侧数据（聚合器生成、扩展返回、
/// 内部比对同源），非界面文案（D38 ③）。
const I18N: &[(&str, &str, &str)] = &[
    // ── 设置页 · 语言卡（v4.13 D38，批次 B）──
    ("settings.lang.name", "语言 Language", "Language"),
    (
        "settings.lang.desc",
        "界面与内置扩展的显示语言；「跟随系统」按 Windows UI 语言自动选择，切换即时生效",
        "Display language for the UI and built-in extensions. \"Use system setting\" follows the Windows UI language; changes apply immediately",
    ),
    ("lang.follow_system", "跟随系统", "Use system setting"),
    // 语言自称恒为原文（两列同值，Fluent 语言选择惯例）。
    ("lang.zh_cn", "简体中文", "简体中文"),
    ("lang.en_us", "English", "English"),
    // ── 通用（对话框 / 加载）──
    ("dialog.confirm", "确认", "Confirm"),
    ("dialog.cancel", "取消", "Cancel"),
    ("panel.loading", "正在加载…", "Loading…"),
    ("page.settings", "设置", "Settings"),
    // ── 主面板：搜索占位 / 空态 ──
    ("ph.root", "搜索命令…", "Search commands…"),
    ("ph.filter", "筛选命令…", "Filter commands…"),
    ("ph.filter_in", "在「{title}」中筛选…", "Filter in \"{title}\"…"),
    (
        "empty.no_commands",
        "未发现命令",
        "No commands found",
    ),
    (
        "empty.no_commands_hint",
        "检查扩展清单或扩展运行状态",
        "Check extension manifests or whether extensions are running",
    ),
    ("empty.no_match", "未找到匹配的命令", "No matching commands"),
    ("empty.no_match_hint", "试试其他关键词。", "Try different keywords."),
    // ── 页脚（动作文本 / 键位图例 / 设置页页脚提示）──
    ("footer.enter", "进入", "Open"),
    ("footer.open_apps", "打开应用", "Open app"),
    ("footer.open_settings", "打开设置", "Open settings"),
    ("footer.run_command", "运行命令", "Run command"),
    ("footer.open_web", "打开网页", "Open web"),
    ("footer.calc", "计算", "Calculate"),
    ("footer.invoke", "执行", "Run"),
    // ── 结果类别徽标（aggregator 按 ext_id 推导，GUI 层硬编码映射）──
    // 设计稿 §6.2 映射；中英词序独立，名称取 Fluent 习惯短词。
    ("cat.apps", "应用", "Apps"),
    ("cat.system", "设置", "Settings"),
    ("cat.websearch", "网页", "Web"),
    ("cat.command", "命令", "Command"),
    ("footer.key_execute", "执行", "Run"),
    ("footer.key_hide", "返回·隐藏", "Back·Hide"),
    ("footer.back", "返回", "Back"),
    (
        "footer.settings_hint",
        "设置修改自动保存；搜索引擎更改返回首屏后生效",
        "Settings save automatically; search engine changes apply after returning to the main screen",
    ),
    // ── 右键菜单（10B.2 映射；默认动作经 footer.* 复用）──
    ("ctx.admin", "以管理员身份运行", "Run as administrator"),
    ("ctx.locate", "打开所在位置", "Open file location"),
    ("ctx.copy_path", "复制路径", "Copy path"),
    ("ctx.copy_link", "复制链接", "Copy link"),
    ("ctx.copied", "已复制到剪贴板", "Copied to clipboard"),
    ("ctx.admin_fail", "以管理员身份运行失败：{e}", "Failed to run as administrator: {e}"),
    ("ctx.locate_fail", "打开所在位置失败：{e}", "Failed to open file location: {e}"),
    // ── Toast（invoke / page / health / refresh / keys）──
    (
        "toast.ext_busy",
        "扩展进程不可用（可能正在处理上一个请求）",
        "Extension unavailable (may still be processing the previous request)",
    ),
    (
        "toast.ext_unavailable",
        "扩展 {id} 暂时不可用，可在设置→扩展管理点击重试",
        "Extension {id} is temporarily unavailable — retry it under Settings → Extensions",
    ),
    (
        "toast.ext_unavailable_crash",
        "扩展 {id} 暂时不可用（连续崩溃 {n} 次），可在设置→扩展管理重试",
        "Extension {id} is temporarily unavailable ({n} consecutive crashes) — retry it under Settings → Extensions",
    ),
    ("toast.ext_missing", "扩展信息缺失，无法执行", "Extension info missing; cannot run"),
    ("toast.invoke_fail", "命令执行失败：{e}", "Failed to run command: {e}"),
    ("toast.cmd_updated", "扩展命令已更新", "Extension commands updated"),
    ("toast.ext_retry", "正在重试扩展 {id}", "Retrying extension {id}"),
    (
        "toast.autostart_fail",
        "开机自启设置失败：{e}",
        "Failed to configure launch at startup: {e}",
    ),
    // ── 子页空态（page.rs）──
    ("page.empty", "该页暂无内容", "This page has no content"),
    ("page.fetch_fail", "拉取失败：{e}", "Failed to fetch: {e}"),
    (
        "page.ext_unavailable_restart",
        "扩展 {id} 暂时不可用，重启宿主后恢复",
        "Extension {id} is temporarily unavailable; restart the host to recover",
    ),
    ("page.ext_missing", "扩展信息缺失，无法打开页面", "Extension info missing; cannot open page"),
    (
        "page.cmd_stale",
        "命令已失效：扩展未找到该命令（get_command 返回 null）",
        "Command is stale: the extension no longer knows it (get_command returned null)",
    ),
    ("page.invoke_crashed", "进程在调用期间崩溃", "The process crashed during the call"),
    // ── 设置页：左栏导航 + 顶行 ──
    ("nav.appearance", "外观", "Appearance"),
    ("nav.general", "常规", "General"),
    ("nav.search", "搜索", "Search"),
    ("nav.extensions", "扩展", "Extensions"),
    // ── 设置页 · 外观（主题 / 材质）──
    ("set.theme.name", "主题外观", "Theme"),
    (
        "set.theme.desc",
        "选择亮暗主题；「跟随系统」随 Windows 主题实时切换",
        "Choose light or dark; \"Use system setting\" follows the Windows theme live",
    ),
    ("set.theme.follow", "跟随系统", "Use system setting"),
    ("set.theme.light", "亮色", "Light"),
    ("set.theme.dark", "暗色", "Dark"),
    ("set.backdrop.name", "窗口材质", "Window material"),
    (
        "set.backdrop.desc",
        "窗口背景使用 Windows 11 系统材质；两者互斥，全关为不透明",
        "Use Windows 11 system materials for the window background; mutually exclusive — all off = opaque",
    ),
    ("set.backdrop.mica", "云母材质", "Mica"),
    (
        "set.backdrop.mica.desc",
        "Mica：随桌面窗口 subtle 染色，性能开销低（默认开启）",
        "Mica: subtle desktop-window tinting with low overhead (on by default)",
    ),
    ("set.backdrop.acrylic", "亚克力材质", "Acrylic"),
    (
        "set.backdrop.acrylic.desc",
        "Acrylic：半透明模糊，视觉更通透、GPU 开销略高",
        "Acrylic: translucent blur — clearer look, slightly higher GPU cost",
    ),
    // ── 设置页 · 常规 ──
    ("set.openview.name", "打开面板时显示", "Shown when the panel opens"),
    (
        "set.openview.desc",
        "「默认功能」只显示计算、网页搜索等入口；输入查询时应用仍会参与匹配",
        "\"Default\" shows only entries like calculator and web search; apps still match once you type",
    ),
    ("set.openview.all", "显示所有应用", "Show all apps"),
    ("set.hotkey.name", "全局热键", "Global hotkey"),
    ("set.hotkey.desc", "自定义唤起/隐藏面板的组合键", "Customize the combo that shows/hides the panel"),
    ("set.hotkey.current", "当前组合", "Current combo"),
    ("set.hotkey.capturing", "请按下新的组合键（Esc 取消）…", "Press the new key combo (Esc to cancel)…"),
    ("set.hotkey.capturing_btn", "捕获中…", "Capturing…"),
    ("set.hotkey.change", "更改", "Change"),
    ("set.hotkey.reset", "恢复默认", "Reset to default"),
    ("set.autostart.name", "开机自启", "Launch at startup"),
    (
        "set.autostart.desc",
        "登录 Windows 后自动后台运行（当前用户注册表）",
        "Run in the background after signing in to Windows (current-user registry)",
    ),
    // ── 设置页 · 搜索引擎 ──
    ("set.search.name", "搜索引擎", "Search engines"),
    (
        "set.search.desc",
        "配置「网络搜索」分组展示的引擎；更改将在返回首屏后生效",
        "Configure the engines in the \"Web search\" group; changes apply after returning to the main screen",
    ),
    ("set.search.delete", "删除", "Delete"),
    ("set.search.none", "未启用任何引擎（「网络搜索」分组为空）", "No engines enabled (the \"Web search\" group is empty)"),
    ("set.search.add_engine", "添加引擎", "Add engine"),
    ("set.search.presets_done", "预设引擎已全部启用", "All preset engines are enabled"),
    ("set.search.pick", "选择预设引擎…", "Pick a preset engine…"),
    ("set.search.add", "添加", "Add"),
    (
        "set.search.url_hint",
        "自定义引擎 URL（名称自动取域名）https://example.com/search?q={q}",
        "Custom engine URL (name taken from the domain) https://example.com/search?q={q}",
    ),
    ("set.search.err_exists", "已存在同名引擎「{name}」", "An engine named \"{name}\" already exists"),
    (
        "set.search.err_url",
        "URL 须以 http(s):// 开头且包含 {q} 占位符",
        "URL must start with http(s):// and contain a {q} placeholder",
    ),
    // ── 设置页 · 扩展管理 ──
    ("set.ext.name", "扩展管理", "Extensions"),
    (
        "set.ext.desc",
        "停用的扩展不再出现在面板中；更改在返回首屏后生效",
        "Disabled extensions no longer appear in the panel; changes apply after returning to the main screen",
    ),
    (
        "set.ext.empty",
        "未发现扩展（检查 extensions.d 清单目录）",
        "No extensions found (check the extensions.d manifest directory)",
    ),
    ("set.ext.retry", "重试", "Retry"),
    // ── 托盘（10C.2；菜单每次右键即席创建，经 TRAY_LANG 原子量取语言）──
    ("tray.toggle", "显示/隐藏面板\tWin+Alt+Space", "Show/Hide panel\tWin+Alt+Space"),
    ("tray.settings", "设置", "Settings"),
    ("tray.exit", "退出", "Exit"),
];

/// 查文案表。`lang` 应传**已解析**的具体语言（`Lang::FollowSystem` 按 ZhCn
/// 兜底——调用方应传 `PaletteApp::lang_effective` 解析值）；key 不存在原样
/// 返回 key（开发期兜底，`i18n_table_complete` 单测保证不发生）。
pub(crate) fn t(lang: Lang, key: &'static str) -> &'static str {
    let Some((_, zh, en)) = I18N.iter().find(|(k, _, _)| *k == key) else {
        return key;
    };
    match lang {
        Lang::ZhCn | Lang::FollowSystem => zh,
        Lang::EnUs => en,
    }
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
pub(crate) fn nested_search_placeholder(lang: Lang, page_title: &str) -> String {
    if page_title.is_empty() {
        t(lang, "ph.filter").to_string()
    } else {
        t(lang, "ph.filter_in").replace("{title}", page_title)
    }
}

/// 页脚上下文动作文本（设计稿 §6.3，批次 4.1）。
///
/// 动词由 `CommandRef` 决定：`Page` → 进入类；`Invoke` → 执行类。
/// 宾语由 `ext_id` 决定——GUI 层硬编码映射表（去 `com.ddrun.` 前缀后匹配，
/// 与 [`aggregator`] 的类别映射同口径）；第三方/未知扩展回退「执行」。
/// 协议层零改动（C9/C13：`CommandItem` 无 kind，映射仅在本层）。
/// 文案经 i18n 表按 `lang` 取（v4.13 D38）。
pub(crate) fn footer_action_text(lang: Lang, item: &PanelItem) -> String {
    if matches!(item.command, CommandRef::Page { .. }) {
        return t(lang, "footer.enter").to_string();
    }
    let short = item
        .ext_id
        .strip_prefix("com.ddrun.")
        .unwrap_or(&item.ext_id);
    let key = match short {
        "apps" => "footer.open_apps",
        "system" => "footer.open_settings",
        "shell" => "footer.run_command",
        "websearch" => "footer.open_web",
        "calc" => "footer.calc",
        _ => "footer.invoke",
    };
    t(lang, key).to_string()
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
            nested_search_placeholder(Lang::ZhCn, "应用列表"),
            "在「应用列表」中筛选…",
            "页标题进 placeholder（D2：不占独立行）"
        );
        assert_eq!(
            nested_search_placeholder(Lang::ZhCn, ""),
            "筛选命令…",
            "空标题回落通用文案"
        );
        // 英文：模板占位符替换 + 词序独立成句
        assert_eq!(
            nested_search_placeholder(Lang::EnUs, "Apps"),
            "Filter in \"Apps\"…"
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
            assert_eq!(
                footer_action_text(Lang::ZhCn, &item),
                expected,
                "{ext} 动作映射"
            );
        }
        // 英文口径抽查处方 + 第三方回退
        assert_eq!(
            footer_action_text(Lang::EnUs, &item_with("com.ddrun.apps", CommandRef::Invoke)),
            "Open app"
        );
    }

    #[test]
    fn footer_action_falls_back_for_third_party_and_empty() {
        // 第三方未命中 → 执行；无 ext_id（如宿主兜底项之外的场合）→ 执行
        let third = item_with("com.acme.thing", CommandRef::Invoke);
        assert_eq!(footer_action_text(Lang::ZhCn, &third), "执行");
        let empty = item_with("", CommandRef::Invoke);
        assert_eq!(footer_action_text(Lang::ZhCn, &empty), "执行");
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
        assert_eq!(footer_action_text(Lang::ZhCn, &page), "进入");
        let page_unknown = item_with(
            "com.acme.thing",
            CommandRef::Page {
                page_id: "p".to_string(),
            },
        );
        assert_eq!(footer_action_text(Lang::ZhCn, &page_unknown), "进入");
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

    // ── i18n 文案表（v4.13 D38，批次 B） ───────────────────────────

    #[test]
    fn i18n_table_complete_both_langs() {
        // 完整性（D38 H5 单测锚）：每个 key 两列非空、key 全表唯一。
        for (i, (k, zh, en)) in I18N.iter().enumerate() {
            assert!(!k.is_empty(), "第 {i} 行 key 为空");
            assert!(!zh.is_empty(), "{k} 缺 zh 文案");
            assert!(!en.is_empty(), "{k} 缺 en 文案");
            assert!(!I18N[..i].iter().any(|(k2, _, _)| k2 == k), "key 重复：{k}");
        }
    }

    #[test]
    fn t_resolves_zh_en_and_falls_back() {
        assert_eq!(t(Lang::ZhCn, "lang.follow_system"), "跟随系统");
        assert_eq!(t(Lang::EnUs, "lang.follow_system"), "Use system setting");
        // 语言自称选项两语言同值（恒原文）。
        assert_eq!(t(Lang::EnUs, "lang.zh_cn"), "简体中文");
        assert_eq!(t(Lang::ZhCn, "lang.en_us"), "English");
        // FollowSystem 未解析时兜底 zh（调用方应传 lang_effective 解析值）。
        assert_eq!(t(Lang::FollowSystem, "lang.follow_system"), "跟随系统");
        // 未知 key 原样返回（开发期兜底，表完整性单测保证正常路径不发生）。
        assert_eq!(t(Lang::EnUs, "no.such.key"), "no.such.key");
    }
}
