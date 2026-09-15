//! O4 可观测性：极简 stderr 日志后端 + 级别开关。
//!
//! 三条设计约束（来自协议与分发形态）：
//!
//! 1. **恒写 stderr**：协议 §2.5 规定 stdout 只能是协议消息——扩展子进程若把日志写进
//!    stdout，宿主会当成协议帧解析并报错。本后端固定 `stderr`，从机制上排除该风险。
//! 2. **零重依赖**：只依赖 `log` facade（零传递依赖），不引 `env_logger`（会拖入
//!    anstream / termcolor / anstyle 等一串），符合项目依赖克制基调（O3 已建立治理）。
//! 3. **默认与历史行为等价**：未设环境变量时级别为 [`DEFAULT_LEVEL`]（`Debug`），
//!    与 O4 之前「所有 `eprintln!` 无条件输出」的可观测性等价——**引入分级不应让
//!    既有排障信息静默消失**。
//!
//! 开关：环境变量 [`ENV_VAR`]，取值 `off|error|warn|info|debug|trace`（大小写不敏感），
//! 支持按 target 定向（最长前缀优先），形如：
//!
//! ```text
//! DDRUN_LOG=info                    # 全局 info
//! DDRUN_LOG=warn,dd-gui=debug       # 默认 warn，但 dd-gui 模块 debug
//! ```

use std::io::Write;

use log::{Level, LevelFilter, Log, Metadata, Record};

/// 级别开关的环境变量名。
pub const ENV_VAR: &str = "DDRUN_LOG";

/// 未设置 [`ENV_VAR`] 时的默认级别。
///
/// **刻意取 `Debug`**：O4 之前所有日志都无条件输出，若默认提到 `Info`，会让原本可见的
/// 排障信息（协议流、计数、内部步骤）静默消失。分级能力应是「可降噪」而非「默认更少」。
pub const DEFAULT_LEVEL: LevelFilter = LevelFilter::Debug;

/// 解析后的级别过滤器：一个全局默认 + 若干 target 前缀覆盖。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filters {
    default: LevelFilter,
    /// `(target 前缀, 级别)`；匹配时取**最长前缀**（与 env_logger 同口径）。
    targets: Vec<(String, LevelFilter)>,
}

impl Filters {
    /// 解析 `DDRUN_LOG` 形态的规格串。
    ///
    /// - 裸级别（`info`）设置全局默认；后出现的覆盖先出现的。
    /// - `target=level` 加入定向表；无法解析的片段**静默忽略**（排障开关不该因
    ///   手误而让程序启动失败）。
    pub fn parse(spec: &str) -> Self {
        let mut filters = Self {
            default: DEFAULT_LEVEL,
            targets: Vec::new(),
        };
        for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match part.split_once('=') {
                Some((target, level)) => {
                    let target = target.trim();
                    if target.is_empty() {
                        continue;
                    }
                    if let Some(level) = parse_level(level.trim()) {
                        filters.targets.push((target.to_string(), level));
                    }
                }
                None => {
                    if let Some(level) = parse_level(part) {
                        filters.default = level;
                    }
                }
            }
        }
        filters
    }

    /// 某 target 的生效级别（最长前缀优先；无匹配则用全局默认）。
    pub fn level_for(&self, target: &str) -> LevelFilter {
        self.targets
            .iter()
            .filter(|(prefix, _)| target.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, level)| *level)
            .unwrap_or(self.default)
    }

    /// 该 target 的该级别是否放行。
    pub fn allows(&self, target: &str, level: Level) -> bool {
        level <= self.level_for(target)
    }

    /// 全局默认级别（供 `log::set_max_level` 取上界）。
    pub fn default_level(&self) -> LevelFilter {
        self.default
    }

    /// 全部定向级别中的最高者（供 `log::set_max_level` 取上界）。
    pub fn max_target_level(&self) -> LevelFilter {
        self.targets
            .iter()
            .map(|(_, level)| *level)
            .max()
            .unwrap_or(LevelFilter::Off)
    }
}

/// 解析单个级别名（大小写不敏感）。
pub fn parse_level(s: &str) -> Option<LevelFilter> {
    match s.to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

/// 极简 stderr 后端：**只按级别过滤，不添加任何前缀/颜色**。
///
/// 既有日志消息本身已带 `[dd-gui]` / `[dd-ext-apps]` 这类模块标签，后端再加前缀会
/// 造成格式变化；保持「原样落 stderr」即可让接入前后**逐字一致**。
pub struct StderrLogger {
    filters: Filters,
}

impl StderrLogger {
    /// 以给定过滤器构造后端。
    pub fn new(filters: Filters) -> Self {
        Self { filters }
    }

    /// 后端当前使用的过滤器。
    pub fn filters(&self) -> &Filters {
        &self.filters
    }
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.filters.allows(metadata.target(), metadata.level())
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // 恒 stderr（§2.5）；写入失败无处可报，静默忽略。
        let _ = writeln!(std::io::stderr(), "{}", record.args());
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// 安装全局日志后端（级别取自环境变量 [`ENV_VAR`]）。
///
/// **幂等且失败安全**：`log` 只允许安装一次后端，重复调用（如测试或多入口）返回 `false`
/// 而不 panic。各进程入口可无脑调用。
pub fn init() -> bool {
    let spec = std::env::var(ENV_VAR).unwrap_or_default();
    init_with_spec(&spec)
}

/// 同 [`init`]，但显式给定规格串（便于测试与嵌入式用法）。
pub fn init_with_spec(spec: &str) -> bool {
    install(StderrLogger::new(Filters::parse(spec)))
}

/// 安装指定后端并设定 `max_level`。返回 `false` 表示此前已安装过。
pub fn install(logger: StderrLogger) -> bool {
    let max = logger
        .filters
        .default_level()
        .max(logger.filters.max_target_level());
    // 先设 max_level：即使 set_boxed_logger 失败（已安装），级别上界也已生效。
    log::set_max_level(max);
    log::set_boxed_logger(Box::new(logger)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_level_keeps_legacy_visibility() {
        // 未设环境变量 → 与接入前「所有日志都输出」等价
        assert_eq!(Filters::parse("").default_level(), DEFAULT_LEVEL);
        assert_eq!(Filters::parse("").default_level(), LevelFilter::Debug);
    }

    #[test]
    fn bare_level_sets_default_and_last_wins() {
        assert_eq!(Filters::parse("warn").default_level(), LevelFilter::Warn);
        assert_eq!(
            Filters::parse("info,error").default_level(),
            LevelFilter::Error,
            "后出现的裸级别覆盖先出现的"
        );
    }

    #[test]
    fn level_names_are_case_insensitive_and_tolerant() {
        assert_eq!(parse_level("WARN"), Some(LevelFilter::Warn));
        assert_eq!(parse_level("Warning"), Some(LevelFilter::Warn));
        assert_eq!(parse_level("Debug"), Some(LevelFilter::Debug));
        assert_eq!(parse_level("nonsense"), None);
        // 无法识别的片段被忽略，不影响其余解析
        let f = Filters::parse("warn,bogus,dd-gui=debug");
        assert_eq!(f.default_level(), LevelFilter::Warn);
        assert_eq!(f.level_for("dd-gui"), LevelFilter::Debug);
    }

    #[test]
    fn target_filter_uses_longest_prefix() {
        let f = Filters::parse("info,dd-gui=debug,dd-gui::app=trace");
        assert_eq!(f.level_for("dd-host"), LevelFilter::Info, "无匹配 → 默认");
        assert_eq!(f.level_for("dd-gui"), LevelFilter::Debug);
        assert_eq!(
            f.level_for("dd-gui::app::refresh"),
            LevelFilter::Trace,
            "最长前缀优先"
        );
    }

    #[test]
    fn allows_respects_level_order() {
        let f = Filters::parse("warn");
        assert!(f.allows("any", Level::Error));
        assert!(f.allows("any", Level::Warn));
        assert!(!f.allows("any", Level::Info));
        assert!(!f.allows("any", Level::Debug));
    }

    #[test]
    fn off_silences_everything() {
        let f = Filters::parse("off");
        assert!(!f.allows("any", Level::Error));
        assert_eq!(f.level_for("dd-gui"), LevelFilter::Off);
    }

    #[test]
    fn logger_filters_by_target_and_level() {
        fn md<'a>(target: &'a str, level: Level) -> Metadata<'a> {
            Metadata::builder().target(target).level(level).build()
        }
        let logger = StderrLogger::new(Filters::parse("info,dd-gui=debug"));
        assert!(logger.enabled(&md("dd-gui", Level::Debug)));
        assert!(!logger.enabled(&md("dd-host", Level::Debug)));
        assert!(logger.enabled(&md("dd-host", Level::Info)));
        assert!(logger.filters().allows("dd-gui::app", Level::Debug));
    }

    #[test]
    fn max_level_covers_target_overrides() {
        // max_level 必须覆盖定向表的最高级别，否则定向 debug 会被上界挡掉
        let f = Filters::parse("warn,dd-gui=debug");
        assert_eq!(f.default_level(), LevelFilter::Warn);
        assert_eq!(f.max_target_level(), LevelFilter::Debug);
        assert_eq!(
            f.default_level().max(f.max_target_level()),
            LevelFilter::Debug
        );
    }
}
