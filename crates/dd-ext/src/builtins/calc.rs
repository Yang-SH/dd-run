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
//! 纯函数实现，便于单测（A10 逻辑可复现验证）。
//!
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策（扩展侧先行：
//! 宿主 fallback UI 链路属后续轮，本扩展的兜底能力由 roundtrip 与单测先行验证）。

use crate::{i18n::tr, Effect, ExtensionSpec};
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
    match eval_expr(query) {
        Ok(value) => {
            let text = format!("= {}", format_number(value));
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

/// R-13（2026-09-29）：递归深度上限。calc 在宿主进程内执行，**栈溢出是
/// abort（`catch_unwind` 拦不住）**，必须在递归展开前拒绝。粘贴 ~10^5 个
/// `(`（或一元正负号链）此前可致宿主进程崩溃，现超限回 `EvalError::Domain`。
const MAX_RECURSION_DEPTH: u32 = 256;

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
        match name.as_str() {
            "pi" => Ok(std::f64::consts::PI),
            "e" => Ok(std::f64::consts::E),
            _ => Err(EvalError::Unexpected(self.src[start], start)),
        }
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
}
