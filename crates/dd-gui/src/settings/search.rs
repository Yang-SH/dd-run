//! O12（2026-10-11）拆分自 `settings.rs`：搜索引擎表（SearchEngine / 预设与
//! 默认引擎）。搬运单位 = 完整定义块，函数体一字不改。

/// 搜索引擎配置（2026-09-05 新增设置项：可配置搜索引擎）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchEngine {
    /// 展示名（如 `Google`；也用于扩展侧命令 id 的 slug）。
    pub name: String,
    /// 搜索 URL 模板，含 `{q}` 占位符——dd-ext-websearch 将其替换为
    /// RFC 3986 编码后的关键词。
    pub template: String,
}

impl SearchEngine {
    /// 校验并构造：name 非空、template 含 `{q}` 且以 `http(s)://` 开头。
    pub fn new(name: &str, template: &str) -> Option<Self> {
        let name = name.trim();
        let template = template.trim();
        if name.is_empty()
            || !template.contains("{q}")
            || !(template.starts_with("http://") || template.starts_with("https://"))
        {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            template: template.to_string(),
        })
    }
}

/// 常用预设引擎（设置页下拉可添加项；与 `dd-ext-websearch` 内置默认表保持一致——
/// 两侧各自定义，扩展侧为环境变量缺失时的回落值）。
pub fn preset_search_engines() -> Vec<SearchEngine> {
    [
        ("Google", "https://www.google.com/search?q={q}"),
        ("Bing", "https://www.bing.com/search?q={q}"),
        ("Baidu", "https://www.baidu.com/s?wd={q}"),
        ("DuckDuckGo", "https://duckduckgo.com/?q={q}"),
        ("GitHub", "https://github.com/search?q={q}"),
    ]
    .iter()
    .map(|(n, t)| SearchEngine::new(n, t).expect("预设引擎模板合法"))
    .collect()
}

/// 默认启用的引擎（2026-09-12 用户决策：默认**只开 Google**，其余预设仍可在
/// 设置页「搜索」栏手动添加）。与 [`preset_search_engines`]（可添加目录）解耦。
pub fn default_search_engines() -> Vec<SearchEngine> {
    let presets = preset_search_engines();
    vec![presets[0].clone()]
}
