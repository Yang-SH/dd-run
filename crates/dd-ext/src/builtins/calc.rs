//! dd-ext-calc —— 内置「计算器」扩展（`com.ddrun.calc`，✅ 跨平台）。
//!
//! 功能（M4 A10 核对点：Calc 表达式求值）：
//! - **顶层**：1 条入口命令 `calc.eval`（Calculator，`text_to_suggest="calc "`）；
//! - **兜底**（§6.2）：模板命令 `calc.eval.query`，`title = "= {query}"`——
//!   `{query}` 占位符由宿主渲染时替换为搜索词；用户选中后宿主带
//!   `context.query` 重新 invoke（§6.5），本扩展对 `query` 做表达式求值；
//! - **invoke**：求值成功 → `ShowToast("= 结果")` + `host/set_clipboard` 复制结果
//!   （§7.3，capabilities 已声明）；失败 → `ShowToast` 报错。
//!
//! 求值器为**手写递归下降**（无第三方依赖）：支持 `+ - * / % ^`、括号、
//! 一元正负号、小数与科学计数法、常量 `pi`/`e`；`^` 右结合且优先级最高。
//! **N8 增强**：数学函数 18 个（`sqrt/abs/round/floor/ceil/exp/ln/log/sin/
//! cos/tan/asin/acos/atan` 单参 + `min/max/pow/mod` 双参）、进制字面量
//! （`0x`/`0b`/`0o`）、进制转换（`X in hex|bin|oct|dec`，须非负整数）、
//! 日期运算（`today` / `now` / `today ± N day|days|d|week|weeks|w`，chrono
//! 树内已有零新增）。纯函数实现，便于单测（A10 逻辑可复现验证）。
//!
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策（扩展侧先行：
//! 宿主 fallback UI 链路属后续轮，本扩展的兜底能力由 roundtrip 与单测先行验证）。

use crate::{i18n::tr, Effect, ExtensionSpec};
use chrono::{Datelike, Duration, Local, Weekday};
use dd_protocol::messages::InvokeParams;
use dd_protocol::methods::METHOD_HOST_SET_CLIPBOARD;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

pub fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.calc",
        display_name: tr("计算器", "Calculator"),
        description: tr(
            "在搜索框输入表达式（如 2+2*3）即时计算",
            "Type an expression in the box (e.g. 2+2*3) for instant calculation",
        ),
        frozen: true,
        has_fallback: true,
        capabilities: &[METHOD_HOST_SET_CLIPBOARD],
        log_tag: "dd-ext-calc",
        pages: None, // 计算器为顶层命令，无子页（§6.3）
        top_level: top_level_commands,
        fallback: Some(fallback_commands),
        invoke: handle_invoke,
    }
}

/// 顶层命令：入口项（invoke 时若带 query 直接求值，否则提示用法）。
fn top_level_commands() -> Vec<CommandItem> {
    vec![CommandItem {
        id: "calc.eval".to_string(),
        title: "Calculator".to_string(),
        subtitle: Some(
            tr(
                "在搜索框输入表达式后选择「= …」结果项，如 2+2*3",
                "Type an expression, then pick the “= …” result item, e.g. 2+2*3",
            )
            .to_string(),
        ),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: "\u{E8EF}".to_string(), // Calculator (Segoe Fluent Icons)
        }),
        section: Some(tr("计算", "Calculator").to_string()),
        tags: Some(vec!["calc".to_string(), "math".to_string()]),
        details: None,
        // 输入以 "calc " 开头时也命中（设计文档 §4.4：选中后回填）
        text_to_suggest: Some("calc ".to_string()),
        more_commands: None,
        command: CommandRef::Invoke,
    }]
}

/// 兜底模板：`title` 的 `{query}` 由宿主渲染替换（§6.2）。
fn fallback_commands() -> Vec<CommandItem> {
    vec![CommandItem {
        id: "calc.eval.query".to_string(),
        title: "= {query}".to_string(),
        subtitle: Some(
            tr(
                "计算表达式（Enter 后显示结果并复制到剪贴板）",
                "Evaluate the expression (Enter shows the result and copies it to clipboard)",
            )
            .to_string(),
        ),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: "\u{E8EF}".to_string(),
        }),
        section: Some(tr("计算", "Calculator").to_string()),
        tags: None,
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Invoke,
    }]
}

/// invoke 分发：calc.eval / calc.eval.query → 用 `context.query` 求值。
fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
    let query = params
        .context
        .as_ref()
        .and_then(|c| c.query.as_deref())
        .unwrap_or("")
        .trim();
    let Some(query) = query_after_prefix(query) else {
        return (
            CommandResult::ShowToast {
                message: tr(
                    "输入表达式后选择「= …」项，例如 2+2*3",
                    "Type an expression, then pick the “= …” item, e.g. 2+2*3",
                )
                .to_string(),
                duration_ms: Some(2_500),
            },
            Vec::new(),
        );
    };
    if query.is_empty() {
        return (
            CommandResult::ShowToast {
                message: tr(
                    "表达式为空：输入如 1+2*3、2^10、pi*2",
                    "Expression is empty: try e.g. 1+2*3, 2^10, pi*2",
                )
                .to_string(),
                duration_ms: Some(2_500),
            },
            Vec::new(),
        );
    }
    match eval(query) {
        Ok(outcome) => {
            // N8：Number → format_number（整数化/科学计数法口径不变）；
            // Radix/Text → 文本原样（转换串 / 日期串）。
            let result = match outcome {
                CalcOutcome::Number(v) => format_number(v),
                CalcOutcome::Radix(text) | CalcOutcome::Text(text) => text,
            };
            let text = format!("= {result}");
            // ShowToast 展示结果 + host/set_clipboard 复制（§7.3）
            (
                CommandResult::ShowToast {
                    message: text.clone(),
                    duration_ms: Some(3_000),
                },
                vec![Effect::HostRequest {
                    method: METHOD_HOST_SET_CLIPBOARD,
                    params: serde_json::json!({ "text": text }),
                }],
            )
        }
        Err(reason) => (
            CommandResult::ShowToast {
                message: tr("无法计算：{reason}", "Cannot evaluate: {reason}")
                    .replace("{reason}", &reason.to_string()),
                duration_ms: Some(3_000),
            },
            Vec::new(),
        ),
    }
}

/// `context.query` 可能带 "calc " 前缀（顶层入口的 `text_to_suggest` 回填）或
/// 前导 `=`（兜底项 `= {query}` 的提示语义，用户会照着输入 "=1+1"）——剥掉后再求值。
fn query_after_prefix(query: &str) -> Option<&str> {
    let q = if let Some(rest) = query.strip_prefix("calc") {
        rest.trim_start_matches(' ')
    } else {
        query
    };
    let q = q.trim_start();
    // 只剥一层 `=`：`==1+1` 的剩余 `=1+1` 交给求值器报 Unexpected（防御性，不静默吞错）
    let q = q.strip_prefix('=').map(str::trim_start).unwrap_or(q);
    Some(q)
}

// ─── 表达式求值器（纯函数，可单测）──────────────────────────

/// 表达式求值失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalError {
    Empty,
    Unexpected(char, usize),
    UnterminatedGroup,
    MissingOperand,
    MissingOperator(usize),
    DivisionByZero,
    Domain,
    /// N8：未知函数名（位置）。
    UnknownFunction(usize),
    /// N8：函数实参数量与 arity 不符（位置）。
    BadArity(usize),
    /// N8：进制转换要求**非负整数**（负数 / 小数 / 超出 i128 精确域）。
    NotInteger,
    /// N8：日期表达式形态不符（`today ± N day|days|d|week|weeks|w`）。
    BadDate,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Empty => write!(f, "表达式为空"),
            EvalError::Unexpected(c, at) => write!(f, "位置 {at} 出现意外字符 `{c}`"),
            EvalError::UnterminatedGroup => write!(f, "括号未闭合"),
            EvalError::MissingOperand => write!(f, "缺少操作数"),
            EvalError::MissingOperator(at) => write!(f, "位置 {at} 缺少运算符"),
            EvalError::DivisionByZero => write!(f, "除以 0"),
            EvalError::Domain => write!(f, "结果超出实数范围"),
            EvalError::UnknownFunction(at) => write!(f, "位置 {at} 未知函数"),
            EvalError::BadArity(at) => write!(f, "位置 {at} 参数数量不符"),
            EvalError::NotInteger => write!(f, "进制转换要求非负整数"),
            EvalError::BadDate => {
                write!(
                    f,
                    "日期形态：today | now | today ± N day|days|d|week|weeks|w"
                )
            }
        }
    }
}

/// 求值入口：trim → 递归下降 → 断言消费完输入。
/// 内部空白仅作 token 分隔符（Parser 在 token 边界跳过），不做全局删除——
/// 否则 `"2 2"` 会被合并成 `"22"`，`MissingOperator` 分支永远不可达。
pub fn eval_expr(input: &str) -> Result<f64, EvalError> {
    let src: Vec<char> = input.trim().chars().collect();
    if src.is_empty() {
        return Err(EvalError::Empty);
    }
    let mut p = Parser {
        src: &src,
        pos: 0,
        depth: 0,
    };
    let value = p.parse_expr()?;
    if p.pos != src.len() {
        return Err(EvalError::MissingOperator(p.pos));
    }
    Ok(value)
}

// ─── N8：求值分发（日期 / 进制转换 / 数值）────────────────────────

/// N8：求值结果——数值 / 进制转换文本（`0x`/`0o`/`0b` 前缀，dec 裸整数串）/
/// 文本（日期串「YYYY-MM-DD 周X」与 now 串「YYYY-MM-DD HH:MM」）。
/// [`handle_invoke`] 按变体格式化（Number 走 [`format_number`]）。f64 无 Eq
///（PartialEq 即可——测试断言用近似/构造值）。
#[derive(Debug, Clone, PartialEq)]
pub enum CalcOutcome {
    Number(f64),
    Radix(String),
    Text(String),
}

/// N8：invoke 消费的求值入口——日期形态 → 进制转换形态 → 常规数值表达式
/// （[`eval_expr`]）。`in` 不是合法表达式 token，常规路径对 `X in y` 恒
/// MissingOperator，故转换分支先于常规（rhs 不识基数时放行回常规）。
pub fn eval(input: &str) -> Result<CalcOutcome, EvalError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(EvalError::Empty);
    }
    if let Some(out) = try_date(trimmed) {
        return out;
    }
    if let Some(out) = try_radix_convert(trimmed)? {
        return Ok(out);
    }
    eval_expr(trimmed).map(CalcOutcome::Number)
}

/// N8：日期形态识别与求值。`None` = 非 today/now 形态（走常规路径——
/// `sunday` / `todayx` 之外的一般文本不受影响）。形态：`today` / `now` /
/// `today (+|-) N day|days|d|week|weeks|w`（空白可有可无）。
fn try_date(s: &str) -> Option<Result<CalcOutcome, EvalError>> {
    if s == "now" {
        return Some(Ok(CalcOutcome::Text(
            Local::now().format("%Y-%m-%d %H:%M").to_string(),
        )));
    }
    let rest = s.strip_prefix("today")?;
    // 粘连词防误吞："todayx" 等后续非空白/符号/结尾 → 放行常规路径
    //（"sunday" 天然不进本函数——前缀不匹配）。
    if !(rest.is_empty()
        || rest.starts_with(char::is_whitespace)
        || rest.starts_with('+')
        || rest.starts_with('-'))
    {
        return None;
    }
    let delta_days = if rest.is_empty() {
        0i64
    } else {
        let rest = rest.trim_start();
        let (sign, rest) = match rest.chars().next() {
            Some('+') => (1i64, &rest[1..]),
            Some('-') => (-1i64, &rest[1..]),
            _ => return Some(Err(EvalError::BadDate)),
        };
        let rest = rest.trim_start();
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if num_end == 0 {
            return Some(Err(EvalError::BadDate));
        }
        let num: i64 = rest[..num_end].parse().ok()?;
        let unit = rest[num_end..].trim();
        let days = match unit {
            "day" | "days" | "d" => num,
            "week" | "weeks" | "w" => num.checked_mul(7)?,
            _ => return Some(Err(EvalError::BadDate)),
        };
        sign.checked_mul(days)?
    };
    Some(Ok(CalcOutcome::Text(date_text(delta_days))))
}

/// N8：`today ± days` 的文本（ISO 日期 + 星期，zh 周X / en Mon..Sun）。
fn date_text(delta_days: i64) -> String {
    let shifted = Local::now().date_naive() + Duration::days(delta_days);
    let weekday = match shifted.weekday() {
        Weekday::Mon => tr("周一", "Mon"),
        Weekday::Tue => tr("周二", "Tue"),
        Weekday::Wed => tr("周三", "Wed"),
        Weekday::Thu => tr("周四", "Thu"),
        Weekday::Fri => tr("周五", "Fri"),
        Weekday::Sat => tr("周六", "Sat"),
        Weekday::Sun => tr("周日", "Sun"),
    };
    format!("{shifted} {weekday}")
}

/// N8：`X in hex|bin|oct|dec` 识别与转换。`rfind(" in ")` 切 lhs/rhs；rhs 不识
/// 基数 → `None`（放行常规路径）；lhs 先按常规求值（失败即传播）。要求
/// **非负整数**且落在 i128 可精确表示域（≥2¹²⁷ 拒绝——同 [`format_number`]
/// 的「貌似正确的错误答案」防御口径）。
fn try_radix_convert(s: &str) -> Result<Option<CalcOutcome>, EvalError> {
    let Some(pos) = s.rfind(" in ") else {
        return Ok(None);
    };
    let radix = match s[pos + 4..].trim().to_ascii_lowercase().as_str() {
        "hex" => Some(16u32),
        "bin" => Some(2),
        "oct" => Some(8),
        "dec" => Some(10),
        _ => None,
    };
    let Some(radix) = radix else {
        return Ok(None);
    };
    let value = eval_expr(s[..pos].trim())?;
    if value < 0.0 || value.fract() != 0.0 || value >= 2f64.powi(127) {
        return Err(EvalError::NotInteger);
    }
    let n = value as i128;
    let text = match radix {
        16 => format!("0x{n:X}"),
        8 => format!("0o{n:o}"),
        2 => format!("0b{n:b}"),
        _ => format!("{n}"),
    };
    Ok(Some(CalcOutcome::Radix(text)))
}

/// R-13（2026-09-29）：递归深度上限。calc 在宿主进程内执行，**栈溢出是
/// abort（`catch_unwind` 拦不住）**，必须在递归展开前拒绝。粘贴 ~10^5 个
/// `(`（或一元正负号链）此前可致宿主进程崩溃，现超限回 `EvalError::Domain`。
const MAX_RECURSION_DEPTH: u32 = 256;

/// N8：函数实现签名——`fn(f64, f64) -> f64`（单参忽略第二参）。
type FuncImpl = fn(f64, f64) -> f64;

/// N8：函数表——`(名称, arity 1|2, 实现)`；非有限结果（NaN/±inf，含 `mod`
/// 除零、`log(0)`、越界 `asin` 等）统一归 [`EvalError::Domain`]。
const FUNCS: &[(&str, u8, FuncImpl)] = &[
    ("sqrt", 1, |a, _| a.sqrt()),
    ("abs", 1, |a, _| a.abs()),
    ("round", 1, |a, _| a.round()),
    ("floor", 1, |a, _| a.floor()),
    ("ceil", 1, |a, _| a.ceil()),
    ("exp", 1, |a, _| a.exp()),
    ("ln", 1, |a, _| a.ln()),
    ("log", 1, |a, _| a.log10()),
    ("sin", 1, |a, _| a.sin()),
    ("cos", 1, |a, _| a.cos()),
    ("tan", 1, |a, _| a.tan()),
    ("asin", 1, |a, _| a.asin()),
    ("acos", 1, |a, _| a.acos()),
    ("atan", 1, |a, _| a.atan()),
    ("min", 2, |a, b| a.min(b)),
    ("max", 2, |a, b| a.max(b)),
    ("pow", 2, |a, b| a.powf(b)),
    ("mod", 2, |a, b| a % b),
];

struct Parser<'a> {
    src: &'a [char],
    pos: usize,
    /// R-13：当前递归深度——括号嵌套（`parse_atom` 的 `'('` 分支）与一元
    /// 符号链（`parse_unary` 的 `'+'/'-'` 自递归）各计一层，共用同一上限。
    depth: u32,
}

impl<'a> Parser<'a> {
    /// R-13：进入一层递归前调用；超限返回 `Domain`。
    fn enter(&mut self) -> Result<(), EvalError> {
        self.depth += 1;
        if self.depth > MAX_RECURSION_DEPTH {
            return Err(EvalError::Domain);
        }
        Ok(())
    }

    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    /// 跳过 token 间空白（不推进语义位置；数字字面量解析前不会调用，
    /// 因此 `2 2` 中数字不会被空格粘连）。
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    /// expr := term (('+' | '-') term)*
    fn parse_expr(&mut self) -> Result<f64, EvalError> {
        let mut lhs = self.parse_term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.bump();
                    let rhs = self.parse_term()?;
                    lhs += rhs;
                }
                Some('-') => {
                    self.bump();
                    let rhs = self.parse_term()?;
                    lhs -= rhs;
                }
                _ => return Ok(lhs),
            }
        }
    }

    /// term := unary (('*' | '/' | '%') unary)*
    fn parse_term(&mut self) -> Result<f64, EvalError> {
        let mut lhs = self.parse_unary()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.bump();
                    let rhs = self.parse_unary()?;
                    lhs *= rhs;
                }
                Some('/') => {
                    self.bump();
                    let rhs = self.parse_unary()?;
                    if rhs == 0.0 {
                        return Err(EvalError::DivisionByZero);
                    }
                    lhs /= rhs;
                }
                Some('%') => {
                    self.bump();
                    let rhs = self.parse_unary()?;
                    if rhs == 0.0 {
                        return Err(EvalError::DivisionByZero);
                    }
                    lhs %= rhs;
                }
                _ => return Ok(lhs),
            }
        }
    }

    /// unary := ('+' | '-') unary | power
    fn parse_unary(&mut self) -> Result<f64, EvalError> {
        self.skip_ws();
        match self.peek() {
            Some('+') => {
                // R-13：一元符号链自递归同样无界，纳入深度守卫
                self.enter()?;
                self.bump();
                let v = self.parse_unary();
                self.depth -= 1;
                v
            }
            Some('-') => {
                self.enter()?;
                self.bump();
                let v = -self.parse_unary()?;
                self.depth -= 1;
                Ok(v)
            }
            _ => self.parse_power(),
        }
    }

    /// power := atom ('^' unary)?   —— 右结合（2^3^2 = 2^(3^2) = 512）
    fn parse_power(&mut self) -> Result<f64, EvalError> {
        let base = self.parse_atom()?;
        self.skip_ws();
        if self.peek() == Some('^') {
            self.bump();
            self.skip_ws();
            let exp = self.parse_unary()?;
            let v = base.powf(exp);
            if !v.is_finite() {
                return Err(EvalError::Domain);
            }
            return Ok(v);
        }
        Ok(base)
    }

    /// atom := number | '(' expr ')' | const
    fn parse_atom(&mut self) -> Result<f64, EvalError> {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                // R-13：括号嵌套是递归深度 ∝ 输入长度的主通道，进入前设限
                self.enter()?;
                self.bump();
                let v = self.parse_expr()?;
                self.depth -= 1;
                self.skip_ws();
                if self.bump() != Some(')') {
                    return Err(EvalError::UnterminatedGroup);
                }
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => self.parse_number(),
            Some(c) if c.is_ascii_alphabetic() => self.parse_ident(),
            _ => Err(EvalError::MissingOperand),
        }
    }

    fn parse_number(&mut self) -> Result<f64, EvalError> {
        let start = self.pos;
        // N8：进制字面量（0x/0X 十六、0b/0B 二、0o/0O 八）——前缀 + 至少一位
        // 该进制数字；经 i128 解析（溢出 → Domain）。`0e5` 等仍走十进制路径
        //（'e' 非进制前缀），既有行为不变。
        if self.peek() == Some('0') {
            let radix = match self.src.get(self.pos + 1) {
                Some('x') | Some('X') => Some(16u32),
                Some('b') | Some('B') => Some(2),
                Some('o') | Some('O') => Some(8),
                _ => None,
            };
            if let Some(radix) = radix {
                self.bump();
                self.bump();
                let dstart = self.pos;
                while let Some(c) = self.peek() {
                    if c.is_digit(radix) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                if self.pos == dstart {
                    return Err(EvalError::Unexpected('0', start));
                }
                let digits: String = self.src[dstart..self.pos].iter().collect();
                return i128::from_str_radix(&digits, radix)
                    .map(|v| v as f64)
                    .map_err(|_| EvalError::Domain);
            }
        }
        let mut seen_dot = false;
        let mut seen_exp = false;
        while let Some(c) = self.peek() {
            match c {
                '0'..='9' => {
                    self.bump();
                }
                '.' if !seen_dot && !seen_exp => {
                    seen_dot = true;
                    self.bump();
                }
                'e' | 'E' if !seen_exp => {
                    seen_exp = true;
                    self.bump();
                    // 指数符号（可选）
                    if matches!(self.peek(), Some('+') | Some('-')) {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
        let text: String = self.src[start..self.pos].iter().collect();
        text.parse::<f64>()
            .map_err(|_| EvalError::Unexpected(self.src[start], start))
    }

    fn parse_ident(&mut self) -> Result<f64, EvalError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() {
                self.bump();
            } else {
                break;
            }
        }
        let name: String = self.src[start..self.pos].iter().collect();
        // N8：函数调用——已知函数名 + 后随 '('；pi/e 常量保持既有形态
        //（后随 '(' 时走旧语义：顶层 MissingOperator，不吞并）。
        self.skip_ws();
        if self.peek() == Some('(') && FUNCS.iter().any(|(n, _, _)| *n == name.as_str()) {
            return self.parse_call(&name, start);
        }
        match name.as_str() {
            "pi" => Ok(std::f64::consts::PI),
            "e" => Ok(std::f64::consts::E),
            _ => Err(EvalError::Unexpected(self.src[start], start)),
        }
    }

    /// N8：函数调用——`name '(' expr (',' expr)? ')'`（arity 2 的逗号 + 第二
    /// 实参必需；非 ')' 收口一律 BadArity——含 arity 1 的多余实参）。参数经
    /// [`Self::parse_expr`]（R-13 深度守卫覆盖）。结果非有限 → [`EvalError::Domain`]。
    fn parse_call(&mut self, name: &str, name_start: usize) -> Result<f64, EvalError> {
        let Some((_, arity, func)) = FUNCS.iter().find(|(n, _, _)| *n == name) else {
            return Err(EvalError::UnknownFunction(name_start));
        };
        self.bump(); // '('
        self.enter()?;
        let a = self.parse_expr()?;
        let b = if *arity == 2 {
            self.skip_ws();
            if self.bump() != Some(',') {
                return Err(EvalError::BadArity(name_start));
            }
            self.parse_expr()?
        } else {
            0.0
        };
        self.depth -= 1;
        self.skip_ws();
        if self.bump() != Some(')') {
            return Err(EvalError::BadArity(name_start));
        }
        let v = func(a, b);
        if !v.is_finite() {
            return Err(EvalError::Domain);
        }
        Ok(v)
    }
}

/// 结果格式化：接近整数的显示为整数（无尾零），否则保留合理精度。
pub fn format_number(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let rounded = v.round();
    if (v - rounded).abs() < 1e-9 {
        // P2（2026-10-09）：去掉 `as i128`——Rust 浮点→整型为饱和转换，
        // |v| ≥ 2¹²⁷（如 2^128 ≈ 3.4e38）一律显示为 i128::MAX，给出貌似
        // 正确的错误答案。f64 Display 是 round-trip 语义（最短唯一确定该
        // f64 的十进制串，~16 位有效数字），非精确整数展开。
        // 负零防御：极小负数（如 -1e-300，非零不走上方 v==0.0 前置分支）
        // round 为 -0.0，Display 输出 "-0"，与原 `as i128` → "0" 行为
        // 不一致，归一处理。
        if rounded == 0.0 {
            return "0".to_string();
        }
        // ≥1e21 回落科学计数法（对齐 JS `Number.toString()` ≥1e21 语义，
        // 2026-10-10 用户定案 C）：f64 仅 ~16 位十进制精度，完整展开会以
        // 尾随 0 伪装精确值——与 i128::MAX 同类「貌似正确的错误答案」；
        // 科学计数法明示精度边界。阈值以下保持整数展开（f64 精确可表示段）。
        if rounded.abs() >= 1e21 {
            return format!("{rounded:e}");
        }
        return format!("{rounded}");
    }
    let mut s = format!("{v:.9}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::Sender;

    fn invoke(id: &str, query: &str) -> InvokeParams {
        InvokeParams {
            id: id.to_string(),
            sender: Sender::TopLevel,
            context: Some(dd_protocol::messages::InvokeContext {
                query: (!query.is_empty()).then(|| query.to_string()),
                selected_item_id: None,
                form_data: None,
                confirmed: None,
            }),
        }
    }

    #[test]
    fn eval_basic_arithmetic() {
        assert_eq!(eval_expr("2+3*4").unwrap(), 14.0);
        assert_eq!(eval_expr("(2+3)*4").unwrap(), 20.0);
        assert_eq!(eval_expr("7%3").unwrap(), 1.0);
        assert_eq!(eval_expr("-3+1").unwrap(), -2.0);
        assert_eq!(eval_expr("1e2+1").unwrap(), 101.0);
    }

    #[test]
    fn eval_power_right_assoc_and_consts() {
        assert_eq!(eval_expr("2^10").unwrap(), 1024.0);
        assert_eq!(eval_expr("2^3^2").unwrap(), 512.0, "^ 右结合");
        assert_eq!(eval_expr("2^0.5").unwrap(), std::f64::consts::SQRT_2);
        let pi = eval_expr("pi*2").unwrap();
        assert!((pi - std::f64::consts::TAU).abs() < 1e-9);
    }

    #[test]
    fn eval_errors() {
        assert_eq!(eval_expr(""), Err(EvalError::Empty));
        assert_eq!(eval_expr("1/0"), Err(EvalError::DivisionByZero));
        assert_eq!(eval_expr("(1+2"), Err(EvalError::UnterminatedGroup));
        assert_eq!(eval_expr("1+"), Err(EvalError::MissingOperand));
        assert_eq!(eval_expr("2 2"), Err(EvalError::MissingOperator(2)));
        assert!(matches!(eval_expr("abc"), Err(EvalError::Unexpected(_, _))));
        assert_eq!(
            eval_expr("(-1)^0.5"),
            Err(EvalError::Domain),
            "非整数幂开负根 → 非有限"
        );
    }

    /// R-13：257 层括号嵌套（以及 257 个一元符号链）→ `EvalError::Domain`
    /// ——递归在展开前被拒，宿主进程不再有栈溢出（abort）风险。
    #[test]
    fn r13_depth_over_limit_rejected() {
        let deep = format!("{}1{}", "(".repeat(257), ")".repeat(257));
        assert_eq!(eval_expr(&deep), Err(EvalError::Domain));
        // 一元符号链同机制守卫（实现期发现的同源无界递归向量）
        let signs = "-".repeat(257);
        assert_eq!(eval_expr(&format!("{signs}1")), Err(EvalError::Domain));
    }

    /// R-13：256 层内正常求值（合法深嵌套不误拒）。
    #[test]
    fn r13_depth_at_limit_ok() {
        let deep = format!("{}1{}", "(".repeat(256), ")".repeat(256));
        assert_eq!(eval_expr(&deep), Ok(1.0));
        // 混合合法用例：括号 + 一元符号均在限内（100 个负号 = 偶次取负 → +1）
        assert_eq!(
            eval_expr(&format!(
                "{}{}1{}",
                "(".repeat(100),
                "-".repeat(100),
                ")".repeat(100)
            )),
            Ok(1.0)
        );
    }

    #[test]
    fn format_number_trim() {
        assert_eq!(format_number(4.0), "4");
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(1.0 / 3.0 * 3.0), "1", "浮点回整");
    }

    /// P2 回归锚：|v| ≥ 2¹²⁷ 越过 i128 上限——原 `as i128` 饱和转换把 2^128
    /// 显示成 i128::MAX（貌似正确的错误答案）。
    ///
    /// 显示形态（2026-10-10 用户定案 C）：≥1e21 回落科学计数法（对齐 JS
    /// `Number.toString()` ≥1e21 语义），明示 f64 ~16 位精度边界，无尾随 0
    /// 伪装精确问题；<1e21 的整数展开保持不变（1e20 在 f64 中精确可表示）。
    #[test]
    fn format_number_beyond_i128_range_is_exact() {
        assert_eq!(format_number(2.0f64.powi(128)), "3.402823669209385e38");
        assert_eq!(format_number(-2.0f64.powi(128)), "-3.402823669209385e38");
        // ≥1e21 阈值边界：1e21 自身即走科学计数法
        assert_eq!(format_number(1e21), "1e21");
        // <1e21 保持整数展开（1e20 = 2²⁰×5²⁰，f64 精确可表示）
        assert_eq!(format_number(1e20), "100000000000000000000");
        // i128 范围内行为回归
        assert_eq!(format_number(4.0), "4");
        // 负零防御：极小负数 round 为 -0.0，须与 -0.0 字面量同归一为 "0"
        assert_eq!(format_number(-1e-300), "0");
    }

    #[test]
    fn invoke_evaluates_query_and_requests_clipboard() {
        let (result, effects) = handle_invoke(&invoke("calc.eval.query", "2+2*3"));
        assert!(matches!(result, CommandResult::ShowToast { .. }));
        if let CommandResult::ShowToast { message, .. } = &result {
            assert_eq!(message, "= 8");
        }
        assert_eq!(effects.len(), 1, "成功求值应复制到剪贴板");
        assert!(matches!(
            &effects[0],
            Effect::HostRequest {
                method: "host/set_clipboard",
                ..
            }
        ));
    }

    #[test]
    fn invoke_strips_calc_prefix() {
        let (result, effects) = handle_invoke(&invoke("calc.eval", "calc 2+2"));
        assert!(matches!(result, CommandResult::ShowToast { .. }));
        assert_eq!(effects.len(), 1, "带 calc 前缀也应成功求值");
    }

    /// 真机反馈（2026-09-04）：用户照兜底项「= {query}」的提示直接输入 "=1+1"
    /// 后选中 → 旧实现把 `=` 一并喂给求值器 → MissingOperand。前导 `=` 应剥离。
    #[test]
    fn invoke_strips_leading_equals() {
        let (result, effects) = handle_invoke(&invoke("calc.eval.query", "=1+1"));
        if let CommandResult::ShowToast { message, .. } = &result {
            assert_eq!(message, "= 2", "前导 = 应剥离后求值");
        } else {
            panic!("expected ShowToast");
        }
        assert_eq!(effects.len(), 1);
        // 前缀组合："calc = 2^8"
        let (result, _) = handle_invoke(&invoke("calc.eval", "calc = 2^8"));
        if let CommandResult::ShowToast { message, .. } = &result {
            assert_eq!(message, "= 256");
        } else {
            panic!("expected ShowToast");
        }
    }

    #[test]
    fn invoke_reports_parse_error_without_effect() {
        let (result, effects) = handle_invoke(&invoke("calc.eval.query", "1/0"));
        assert!(matches!(result, CommandResult::ShowToast { .. }));
        if let CommandResult::ShowToast { message, .. } = &result {
            assert!(message.contains("无法计算"), "got {message}");
        }
        assert!(effects.is_empty());
    }

    #[test]
    fn invoke_without_query_shows_usage() {
        let (result, effects) = handle_invoke(&invoke("calc.eval", ""));
        assert!(matches!(result, CommandResult::ShowToast { .. }));
        assert!(effects.is_empty());
    }

    // ── N8：函数 / 进制 / 日期 ──────────────────────────────────────

    /// N8：函数求值——单参 / 双参 / 嵌套 / 常量实参混用。
    #[test]
    fn n8_functions_evaluate() {
        assert_eq!(eval_expr("sqrt(16)").unwrap(), 4.0);
        assert_eq!(eval_expr("abs(-3)").unwrap(), 3.0);
        assert_eq!(eval_expr("min(2, 5)").unwrap(), 2.0);
        assert_eq!(eval_expr("max(2, 5)").unwrap(), 5.0);
        assert_eq!(eval_expr("pow(2, 10)").unwrap(), 1024.0);
        assert_eq!(eval_expr("sqrt(sqrt(16))").unwrap(), 2.0);
        assert_eq!(eval_expr("round(2.5)").unwrap(), 3.0);
        assert_eq!(eval_expr("floor(2.9)").unwrap(), 2.0);
        assert_eq!(eval_expr("ceil(2.1)").unwrap(), 3.0);
        assert!(
            (eval_expr("log(100)").unwrap() - 2.0).abs() < 1e-9,
            "log = 常用对数"
        );
        assert_eq!(eval_expr("sin(0)").unwrap(), 0.0);
        // 表达式实参 + 常量混用
        assert_eq!(eval_expr("max(2*3, 5)").unwrap(), 6.0);
        assert!((eval_expr("sqrt(pi)").unwrap() - 1.772).abs() < 1e-3);
    }

    /// N8：函数错误面——未知函数 / arity / 定义域（NaN/±inf → Domain）。
    #[test]
    fn n8_function_errors() {
        assert_eq!(eval_expr("foo(1)"), Err(EvalError::Unexpected('f', 0)));
        assert_eq!(eval_expr("sqrt(1, 2)"), Err(EvalError::BadArity(0)));
        assert_eq!(eval_expr("min(1)"), Err(EvalError::BadArity(0)));
        assert_eq!(eval_expr("min(1, 2, 3)"), Err(EvalError::BadArity(0)));
        assert_eq!(eval_expr("sqrt(-1)"), Err(EvalError::Domain));
        assert_eq!(eval_expr("ln(0)"), Err(EvalError::Domain));
        assert_eq!(
            eval_expr("mod(5, 0)"),
            Err(EvalError::Domain),
            "除零经 NaN 归 Domain"
        );
        assert_eq!(eval_expr("exp(1000)"), Err(EvalError::Domain));
        // pi/e 常量后随 '(' 保持旧语义（顶层 MissingOperator），不被吞作函数
        assert!(matches!(
            eval_expr("pi(2)"),
            Err(EvalError::MissingOperator(_))
        ));
        // 深度守卫覆盖函数实参递归（R-13 既有 paren 用例语义在函数通道成立）
        let nested = format!("sqrt({}1{})", "sqrt(".repeat(256), ")".repeat(256));
        assert_eq!(eval_expr(&nested), Err(EvalError::Domain));
        // 单参调用内垃圾 → 统一 BadArity（非 ')' 收口语义，§parse_call）
        assert_eq!(eval_expr("sqrt(2 2)"), Err(EvalError::BadArity(0)));
    }

    /// N8：进制字面量——0x/0b/0o（含大写前缀）；`0e5` 等仍走十进制路径。
    #[test]
    fn n8_radix_literals() {
        assert_eq!(eval_expr("0xff").unwrap(), 255.0);
        assert_eq!(eval_expr("0XFF").unwrap(), 255.0);
        assert_eq!(eval_expr("0b101").unwrap(), 5.0);
        assert_eq!(eval_expr("0o17").unwrap(), 15.0);
        assert_eq!(eval_expr("0x1 + 0x1").unwrap(), 2.0);
        assert_eq!(eval_expr("0xff * 2").unwrap(), 510.0);
        assert!(eval_expr("0x").is_err(), "无数字位");
        assert!(eval_expr("0xg").is_err());
        assert_eq!(eval_expr("0e5").unwrap(), 0.0, "十进制科学计数法回归");
        assert_eq!(eval_expr("00").unwrap(), 0.0, "前导零十进制回归");
    }

    /// N8：进制转换——`X in hex|bin|oct|dec`；负数/小数/未知基数边界。
    #[test]
    fn n8_radix_conversion() {
        assert_eq!(
            eval("255 in hex").unwrap(),
            CalcOutcome::Radix("0xFF".into())
        );
        assert_eq!(
            eval("0xff in dec").unwrap(),
            CalcOutcome::Radix("255".into()),
            "dec 裸整数串"
        );
        assert_eq!(
            eval("0b101 in oct").unwrap(),
            CalcOutcome::Radix("0o5".into())
        );
        assert_eq!(
            eval("100 in bin").unwrap(),
            CalcOutcome::Radix("0b1100100".into())
        );
        assert_eq!(
            eval("16 in hex").unwrap(),
            CalcOutcome::Radix("0x10".into())
        );
        // 大小写不敏感的基数名
        assert_eq!(
            eval("255 in HEX").unwrap(),
            CalcOutcome::Radix("0xFF".into())
        );
        // 表达式 lhs
        assert_eq!(
            eval("2*8 in hex").unwrap(),
            CalcOutcome::Radix("0x10".into())
        );
        // 边界：负数 / 小数 / 未知基数（放行常规路径）
        assert_eq!(eval("-4 in hex"), Err(EvalError::NotInteger));
        assert_eq!(eval("2.5 in hex"), Err(EvalError::NotInteger));
        assert!(matches!(eval("2 in x"), Err(EvalError::MissingOperator(_))));
    }

    /// N8：日期形态——today/now/位移/错误形态；`todayx` 不误吞为日期前缀。
    #[test]
    fn n8_date_forms() {
        use chrono::Duration as CDur;
        // today（零位移）= 本地今天 + 星期后缀
        let expected_today = {
            let d = Local::now().date_naive();
            format!("{d}")
        };
        match eval("today").unwrap() {
            CalcOutcome::Text(t) => {
                assert!(
                    t.starts_with(&expected_today),
                    "today = 本地日期前缀（实得 {t}）"
                );
                assert!(
                    t.contains('周') || t.contains("Mon") || t.contains("Tue"),
                    "带星期后缀"
                );
            }
            other => panic!("today 应为 Text，实得 {other:?}"),
        }
        // now 形态 = YYYY-MM-DD HH:MM
        match eval("now").unwrap() {
            CalcOutcome::Text(t) => {
                assert_eq!(t.len(), 16, "now = YYYY-MM-DD HH:MM（实得 {t}）");
                assert!(t.contains(':'));
            }
            other => panic!("now 应为 Text，实得 {other:?}"),
        }
        // 位移：与 impl 同法计算期望日（格式锁 + 算术锁）
        match eval("today + 30 days").unwrap() {
            CalcOutcome::Text(t) => {
                let want = (Local::now().date_naive() + CDur::days(30)).to_string();
                assert!(
                    t.starts_with(&want),
                    "today + 30 days（实得 {t}，期望前缀 {want}）"
                );
            }
            other => panic!("应为 Text，实得 {other:?}"),
        }
        match eval("today - 2 weeks").unwrap() {
            CalcOutcome::Text(t) => {
                let want = (Local::now().date_naive() - CDur::days(14)).to_string();
                assert!(t.starts_with(&want), "today - 2 weeks（实得 {t}）");
            }
            other => panic!("应为 Text，实得 {other:?}"),
        }
        // 紧凑空白与单位缩写
        assert!(eval("today+30days").is_ok());
        assert!(eval("today + 3 d").is_ok());
        // 错误形态
        assert_eq!(eval("today + 3 months"), Err(EvalError::BadDate));
        assert_eq!(eval("today +"), Err(EvalError::BadDate));
        assert_eq!(eval("today + x days"), Err(EvalError::BadDate));
        // todayx 不进日期分支 → 常规路径 unknown ident
        assert!(matches!(eval("todayx"), Err(EvalError::Unexpected('t', 0))));
    }
}
