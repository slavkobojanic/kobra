//! Expressions (grammar.md §13): a Pratt loop over the precedence table of
//! §13.2, prefix operators, postfix chains (§13.3), trailing lambdas
//! (§13.4), literals, strings and interpolations, collections and lambdas.

use k_diagnostics::{Applicability, Diagnostic, Fix, TextEdit, TextRange, TextSize, codes};

use super::engine::{CompletedMarker, EOF, Parser};
use super::items::{self, is_name_like};
use super::{statements, types};
use crate::kind::SyntaxKind as K;

/// Associativity of a binary operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Assoc {
    /// `a - b - c` is `(a - b) - c`.
    Left,
    /// `a ?? b ?? c` is `a ?? (b ?? c)`.
    Right,
    /// Chaining operators of the same level, such as `a < b < c`, reports
    /// [`codes::NON_ASSOCIATIVE_CHAIN`].
    None,
}

/// Binding power of prefix `not` operands: comparisons and tighter.
const NOT_OPERAND: u8 = 5;
/// Binding power of prefix `-` operands: postfix expressions only.
const MINUS_OPERAND: u8 = 11;

/// Returns `(left binding power, right binding power, associativity)` of a
/// binary operator token (grammar.md §13.2). The symbolic recovery forms
/// `&&`, `||` and `..` have the precedence of `and`, `or` and `..<`.
fn binary_op(kind: K) -> Option<(u8, u8, Assoc)> {
    Some(match kind {
        K::OrKw | K::PipePipe => (2, 3, Assoc::Left),
        K::AndKw | K::AmpAmp => (3, 4, Assoc::Left),
        K::EqEq | K::BangEq | K::Lt | K::LtEq | K::Gt | K::GtEq => (5, 6, Assoc::None),
        K::QuestionQuestion => (6, 6, Assoc::Right),
        K::DotDotLt | K::DotDotDot | K::DotDot => (7, 8, Assoc::None),
        K::Plus | K::Minus => (8, 9, Assoc::Left),
        K::Star | K::Slash | K::Percent => (9, 10, Assoc::Left),
        _ => return None,
    })
}

/// Returns `true` if the current token can start an expression.
pub(crate) fn can_start_expr(p: &Parser<'_>) -> bool {
    can_start_expr_at(p, 0)
}

/// Returns `true` if the token `n` ahead can start an expression.
fn can_start_expr_at(p: &Parser<'_>, n: usize) -> bool {
    let kind = p.nth(n);
    if kind == K::UseKw {
        // `use(x)` calls a function named `use`; `use A for B` is a binding.
        return p.nth(n + 1) == K::LParen;
    }
    matches!(
        kind,
        K::Ident
            | K::IntLit
            | K::FloatLit
            | K::UnitLit
            | K::ColorLit
            | K::StringStart
            | K::TrueKw
            | K::FalseKw
            | K::NilKw
            | K::SelfKw
            | K::UseKw
            | K::Dot
            | K::LParen
            | K::LBrack
            | K::LBrace
            | K::Minus
            | K::NotKw
            | K::Bang
            | K::TryKw
            | K::AwaitKw
            | K::BindKw
            | K::IfKw
            | K::MatchKw
            | K::Error
    )
}

// ----------------------------------------------------------- entry points

/// `Expr`, with a trailing `when` parsed as a `WhenExpr` and reported as
/// misplaced (K0109).
pub(crate) fn expr(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    expr_with_when(p, false)
}

/// Parses an expression the grammar requires, reporting K0122 if none
/// starts here.
pub(crate) fn required_expr(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    if can_start_expr(p) {
        expr(p)
    } else {
        p.expected(codes::EXPECTED_EXPRESSION, "an expression");
        None
    }
}

/// An `expr_nt`: an expression with no trailing lambda at its top level
/// (conditions, iterables, scrutinees and guards; grammar.md §13.4).
pub(crate) fn expr_no_lambda(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    // A `{` here opens the statement's block, never a lambda.
    if !can_start_expr(p) || p.at(K::LBrace) {
        p.expected(codes::EXPECTED_EXPRESSION, "an expression");
        return None;
    }
    // An `if` or `match` in a condition or scrutinee recurses before any
    // block opens, so every `expr_nt` counts one nesting level (RA-34).
    if !p.enter() {
        return None;
    }
    let saved = p.no_trailing_lambda;
    p.no_trailing_lambda = true;
    let result = expr_bp(p, 0);
    p.no_trailing_lambda = saved;
    p.leave();
    result
}

/// `Expr ('when' Expr)?`. `allowed` is `true` in argument values and list
/// elements; elsewhere the `when` is K0109.
fn expr_with_when(p: &mut Parser<'_>, allowed: bool) -> Option<CompletedMarker> {
    let lhs = expr_bp(p, 0)?;
    if p.at(K::WhenKw) && !p.nl() {
        if !allowed {
            let span = p.span(p.current_range());
            p.report(
                Diagnostic::new(
                    codes::MISPLACED_WHEN,
                    span,
                    "`when` is only allowed in a call argument or a list element",
                )
                .with_help("use `if condition { ... }` to make a value conditional"),
            );
        }
        let m = lhs.precede(p);
        p.bump();
        if can_start_expr(p) {
            expr_bp(p, 0);
        } else {
            p.expected(codes::EXPECTED_EXPRESSION, "a condition after `when`");
        }
        return Some(m.complete(p, K::WhenExpr));
    }
    Some(lhs)
}

// ------------------------------------------------------------- Pratt loop

/// Parses an expression whose operators all bind at least as tightly as
/// `min_bp`.
fn expr_bp(p: &mut Parser<'_>, min_bp: u8) -> Option<CompletedMarker> {
    let mut lhs = unary(p, min_bp)?;
    let mut nonassoc_level: Option<u8> = None;
    loop {
        let kind = p.current();
        if kind == K::Question && min_bp <= 1 {
            lhs = ternary(p, lhs);
            nonassoc_level = None;
            continue;
        }
        let Some((lbp, rbp, assoc)) = binary_op(kind) else {
            break;
        };
        if lbp < min_bp {
            break;
        }
        // `-` at the start of a line starts a new statement (rule N3). Other
        // operators continue the expression only when an operand follows
        // them, so a stray line such as `+ +` is not glued to the previous
        // statement.
        if p.nl() && (kind == K::Minus || !can_start_expr_at(p, 1)) {
            break;
        }
        let op = p.current_range();
        if assoc == Assoc::None && nonassoc_level == Some(lbp) {
            let span = p.span(op);
            p.report(
                Diagnostic::new(
                    codes::NON_ASSOCIATIVE_CHAIN,
                    span,
                    format!("`{}` cannot be chained", p.nth_text(0)),
                )
                .with_help("add parentheses or split the expression with `and`"),
            );
        }
        symbolic_operator(p, kind, op);
        let m = lhs.precede(p);
        p.bump();
        if can_start_expr(p) {
            if assoc == Assoc::Right {
                if p.enter() {
                    expr_bp(p, rbp);
                    p.leave();
                }
            } else {
                expr_bp(p, rbp);
            }
        } else {
            p.expected(
                codes::EXPECTED_EXPRESSION,
                "an expression after the operator",
            );
        }
        lhs = m.complete(p, K::BinaryExpr);
        nonassoc_level = (assoc == Assoc::None).then_some(lbp);
    }
    Some(lhs)
}

/// Reports the symbolic operators K accepts only for recovery: `&&`, `||`
/// and `..`.
fn symbolic_operator(p: &mut Parser<'_>, kind: K, op: TextRange) {
    let span = p.span(op);
    match kind {
        K::AmpAmp => {
            let fix = p.fix(
                "Replace `&&` with `and`",
                op,
                word_replacement(p, op, "and"),
            );
            p.report(
                Diagnostic::new(
                    codes::SYMBOLIC_AND,
                    span,
                    "`&&` is not an operator; write `and`",
                )
                .with_fix(fix),
            );
        }
        K::PipePipe => {
            let fix = p.fix("Replace `||` with `or`", op, word_replacement(p, op, "or"));
            p.report(
                Diagnostic::new(
                    codes::SYMBOLIC_OR,
                    span,
                    "`||` is not an operator; write `or`",
                )
                .with_fix(fix),
            );
        }
        K::DotDot => {
            let half_open = p
                .fix("Use the half-open range `..<`", op, "..<")
                .maybe_incorrect();
            let closed = p
                .fix("Use the closed range `...`", op, "...")
                .maybe_incorrect();
            p.report(
                Diagnostic::new(
                    codes::INVALID_RANGE_OPERATOR,
                    span,
                    "`..` is not an operator; use `..<` or `...`",
                )
                .with_fix(half_open)
                .with_fix(closed),
            );
        }
        _ => {}
    }
}

/// Returns `word`, padded with a space on each side where the text that
/// `range` replaces touches an identifier or number character, so the fix
/// never glues words together (`b&&c` becomes `b and c`, not `bandc`).
fn word_replacement(p: &Parser<'_>, range: TextRange, word: &str) -> String {
    let bytes = p.text().as_bytes();
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    let before = start
        .checked_sub(1)
        .and_then(|i| bytes.get(i))
        .is_some_and(|&b| glues(b));
    let after = bytes.get(end).is_some_and(|&b| glues(b));
    let mut out = String::with_capacity(word.len() + 2);
    if before {
        out.push(' ');
    }
    out.push_str(word);
    if after {
        out.push(' ');
    }
    out
}

/// Returns `true` for a byte that would join a keyword written next to it
/// into one longer word.
pub(crate) fn glues(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `a ? b : c` (K0107): the parts are kept in an `ErrorNode`.
fn ternary(p: &mut Parser<'_>, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    let span = p.span(p.current_range());
    p.report(
        Diagnostic::new(codes::TERNARY, span, "there is no ternary operator in K")
            .with_help("use `if condition { a } else { b }`"),
    );
    p.bump();
    if p.enter() {
        if can_start_expr(p) {
            expr_bp(p, 2);
        } else {
            p.expected(codes::EXPECTED_EXPRESSION, "an expression");
        }
        if p.eat(K::Colon) {
            if can_start_expr(p) {
                expr_bp(p, 2);
            } else {
                p.expected(codes::EXPECTED_EXPRESSION, "an expression");
            }
        } else {
            p.expected_token("`:`");
        }
        p.leave();
    }
    m.complete(p, K::ErrorNode)
}

/// Prefix operators, then a postfix expression.
fn unary(p: &mut Parser<'_>, min_bp: u8) -> Option<CompletedMarker> {
    match p.current() {
        K::NotKw | K::Bang => {
            let m = p.start();
            if p.at(K::Bang) {
                let range = p.current_range();
                let ws = p.whitespace_after(u32::from(range.end()));
                let edit = TextRange::new(range.start(), ws.end());
                let glued_left = usize::from(range.start())
                    .checked_sub(1)
                    .and_then(|i| p.text().as_bytes().get(i))
                    .is_some_and(|&b| glues(b));
                let replacement = if glued_left { " not " } else { "not " };
                let fix = p.fix("Replace `!` with `not`", edit, replacement);
                let span = p.span(range);
                p.report(
                    Diagnostic::new(
                        codes::SYMBOLIC_NOT,
                        span,
                        "`!` is not an operator; write `not`",
                    )
                    .with_fix(fix),
                );
            }
            p.bump();
            operand(p, NOT_OPERAND);
            Some(m.complete(p, K::PrefixExpr))
        }
        K::Minus => {
            let m = p.start();
            p.bump();
            operand(p, MINUS_OPERAND);
            Some(m.complete(p, K::PrefixExpr))
        }
        K::TryKw | K::AwaitKw => {
            let kind = if p.at(K::TryKw) {
                K::TryExpr
            } else {
                K::AwaitExpr
            };
            let keyword = p.current_range();
            let misplaced = min_bp > 0;
            let m = p.start();
            p.bump();
            operand(p, 0);
            // In an `expr_nt` (a condition), a top-level `else` belongs to
            // the enclosing `guard`, not to the `try` (grammar.md §12).
            if kind == K::TryExpr && p.at(K::ElseKw) && !p.no_trailing_lambda {
                p.bump();
                items::required_block(p);
            }
            let node = m.complete(p, kind);
            if misplaced {
                misplaced_try(p, keyword, node);
            }
            Some(node)
        }
        K::BindKw => Some(bind_expr(p, false)),
        _ => postfix(p),
    }
}

/// Parses the operand of a prefix operator, with the nesting check.
fn operand(p: &mut Parser<'_>, bp: u8) {
    if !can_start_expr(p) {
        p.expected(codes::EXPECTED_EXPRESSION, "an operand");
        return;
    }
    if p.enter() {
        expr_bp(p, bp);
        p.leave();
    }
}

/// K0114 for a `try`/`await` that does not start its expression, with a fix
/// that parenthesizes it.
fn misplaced_try(p: &mut Parser<'_>, keyword: TextRange, node: CompletedMarker) {
    let end = node.range(p).end();
    let fix = Fix {
        message: "Wrap the operand in parentheses".to_owned(),
        edits: vec![
            TextEdit {
                span: p.span(TextRange::empty(keyword.start())),
                replacement: "(".to_owned(),
            },
            TextEdit {
                span: p.span(TextRange::empty(end)),
                replacement: ")".to_owned(),
            },
        ],
        applicability: Applicability::MaybeIncorrect,
    };
    let text = p
        .text()
        .get(usize::from(keyword.start())..usize::from(keyword.end()))
        .unwrap_or("try")
        .to_owned();
    let span = p.span(keyword);
    p.report(
        Diagnostic::new(
            codes::MISPLACED_TRY,
            span,
            format!("`{text}` must start the expression it covers"),
        )
        .with_fix(fix),
    );
}

/// `BindExpr ::= 'bind' Expr`. Outside an argument value it is K0110.
fn bind_expr(p: &mut Parser<'_>, allowed: bool) -> CompletedMarker {
    if !allowed {
        let span = p.span(p.current_range());
        p.report(
            Diagnostic::new(
                codes::MISPLACED_BIND,
                span,
                "`bind` is only allowed directly as a call argument",
            )
            .with_help("pass the binding as an argument: `Stepper(value: bind quantity)`"),
        );
    }
    let m = p.start();
    p.bump();
    operand(p, 0);
    m.complete(p, K::BindExpr)
}

// ------------------------------------------------------------ postfix

/// A primary expression followed by member accesses, calls, indexing and
/// trailing lambdas (precedence level 11).
fn postfix(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let mut lhs = primary(p)?;
    let mut has_lambda = lhs.kind() == K::LambdaExpr;
    loop {
        // Calls, indexing and trailing lambdas never apply directly to an
        // `if` or `match` (their blocks end them); members do.
        let block_like = matches!(lhs.kind(), K::IfExpr | K::MatchExpr);
        match p.current() {
            K::Dot | K::QuestionDot => {
                let m = lhs.precede(p);
                p.bump();
                if is_name_like(p.current()) {
                    p.bump();
                } else {
                    p.expected_token("a member name");
                }
                lhs = m.complete(p, K::MemberExpr);
                has_lambda = false;
            }
            K::LParen if !p.nl() && !block_like => {
                let m = lhs.precede(p);
                arg_list(p);
                has_lambda = false;
                if p.at(K::LBrace) && !p.nl() && !p.no_trailing_lambda {
                    lambda(p);
                    has_lambda = true;
                }
                lhs = m.complete(p, K::CallExpr);
            }
            K::LBrack if !p.nl() && !block_like => {
                let m = lhs.precede(p);
                index(p);
                lhs = m.complete(p, K::IndexExpr);
                has_lambda = false;
            }
            K::LBrace if !p.nl() && !p.no_trailing_lambda && !has_lambda && !block_like => {
                let m = lhs.precede(p);
                lambda(p);
                lhs = m.complete(p, K::CallExpr);
                has_lambda = true;
            }
            _ => break,
        }
    }
    Some(lhs)
}

/// `'[' Expr ']'` of an `IndexExpr`.
fn index(p: &mut Parser<'_>) {
    let open = p.current_range();
    p.bump();
    if !p.enter() {
        return;
    }
    let saved = (p.newlines, p.no_trailing_lambda);
    p.newlines = false;
    p.no_trailing_lambda = false;
    required_expr(p);
    if !p.eat(K::RBrack) {
        if at_list_end(p) {
            p.unclosed(open);
        } else {
            p.expected_token("`]`");
            p.skip_until(|p| p.at(K::RBrack) || at_list_end(p));
            if !p.eat(K::RBrack) {
                p.unclosed(open);
            }
        }
    }
    (p.newlines, p.no_trailing_lambda) = saved;
    p.leave();
}

// ------------------------------------------------------------ primaries

/// The unit suffixes a `UnitLit` may carry (grammar.md, "Unit literals");
/// any other suffix reports [`codes::UNKNOWN_UNIT`].
const KNOWN_UNITS: &[&str] = &[
    "px", "em", "rem", "ch", "vw", "vh", "vmin", "vmax", "dvh", "svh", "lvh", "fr", "%", "deg",
    "ms", "s",
];

/// Returns the unit suffix of a `UnitLit` token's text (`px` in `-1.5px`).
pub(crate) fn unit_suffix(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'-') {
        i += 1;
    }
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
        i += 1;
    }
    if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
        i += 1;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
            i += 1;
        }
    }
    if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        let after = bytes.get(i + 2).copied().unwrap_or(0);
        if next.is_ascii_digit() || (matches!(next, b'+' | b'-') && after.is_ascii_digit()) {
            i += 2;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                i += 1;
            }
        }
    }
    text.get(i..).unwrap_or("")
}

/// Returns `true` if `a` and `b` are at most one edit apart.
fn within_one_edit(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let prefix = short.iter().zip(long).take_while(|(x, y)| x == y).count();
    if short.len() == long.len() {
        short[prefix..]
            .iter()
            .skip(1)
            .eq(long[prefix..].iter().skip(1))
    } else {
        short[prefix..] == long[prefix + 1..]
    }
}

/// Returns the prefix letter of `0X1F`, `0B101` or `0O17`: a `0` followed by
/// an upper-case base letter and at least one digit valid for that base
/// (and `_`), which the lexer reads as `0` with a unit suffix.
fn uppercase_base_prefix(text: &str) -> Option<char> {
    let rest = text.strip_prefix('0')?;
    let mut chars = rest.chars();
    let letter = chars.next()?;
    let digits = chars.as_str();
    let valid: fn(char) -> bool = match letter {
        'X' => |c| c.is_ascii_hexdigit(),
        'B' => |c| matches!(c, '0' | '1'),
        'O' => |c| matches!(c, '0'..='7'),
        _ => return None,
    };
    (digits.chars().any(valid) && digits.chars().all(|c| c == '_' || valid(c))).then_some(letter)
}

/// Reports K0112 if the current `UnitLit` token has an unknown suffix.
pub(crate) fn check_unit(p: &mut Parser<'_>) {
    let text = p.nth_text(0);
    let suffix = unit_suffix(text);
    if KNOWN_UNITS.contains(&suffix) {
        return;
    }
    let range = p.current_range();
    let span = p.span(range);
    if let Some(prefix) = uppercase_base_prefix(text) {
        let letter = TextRange::at(range.start() + TextSize::from(1), TextSize::from(1));
        let lower = prefix.to_ascii_lowercase();
        let fix = p.fix(format!("Write `0{lower}`"), letter, lower.to_string());
        p.report(
            Diagnostic::new(
                codes::UNKNOWN_UNIT,
                span,
                format!("`0{prefix}` is not a number prefix; write `0{lower}`"),
            )
            .with_fix(fix),
        );
        return;
    }
    let mut diagnostic = Diagnostic::new(
        codes::UNKNOWN_UNIT,
        span,
        format!("unknown unit `{suffix}`"),
    )
    .with_help(
        "the units are px, em, rem, ch, vw, vh, vmin, vmax, dvh, svh, lvh, fr, %, deg, ms and s",
    );
    let candidates: Vec<&str> = KNOWN_UNITS
        .iter()
        .copied()
        .filter(|unit| within_one_edit(unit, suffix))
        .collect();
    if let [unit] = candidates.as_slice() {
        let number = text.get(..text.len() - suffix.len()).unwrap_or("");
        diagnostic = diagnostic.with_fix(
            p.fix(format!("Use `{unit}`"), range, format!("{number}{unit}"))
                .maybe_incorrect(),
        );
    }
    p.report(diagnostic);
}

/// `PrimaryExpr`: parses a literal, string, name, implicit member,
/// parenthesized expression or tuple, list or map, lambda, `if` or `match`
/// at the current token.
///
/// A run of lexer `Error` tokens becomes one `ErrorNode`. Returns `None`,
/// after reporting [`codes::EXPECTED_EXPRESSION`] and consuming nothing,
/// when the current token cannot start a primary expression.
fn primary(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    match p.current() {
        K::IntLit | K::FloatLit | K::UnitLit | K::ColorLit | K::TrueKw | K::FalseKw | K::NilKw => {
            Some(literal(p))
        }
        K::StringStart => Some(string_lit(p)),
        // `use` is a keyword only for `use` bindings in `app` and `test`
        // bodies; in expressions it is an ordinary name (`use(x)`).
        K::Ident | K::SelfKw => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, K::NameExpr))
        }
        K::UseKw if p.nth(1) == K::LParen => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, K::NameExpr))
        }
        K::Dot => Some(implicit_member(p)),
        K::LParen => Some(paren_or_tuple(p)),
        K::LBrack => Some(list_or_map(p)),
        K::LBrace => Some(lambda(p)),
        K::IfKw => Some(statements::if_expr(p)),
        K::MatchKw => Some(statements::match_expr(p)),
        K::Error => {
            let m = p.start();
            while p.at(K::Error) {
                p.bump();
            }
            Some(m.complete(p, K::ErrorNode))
        }
        _ => {
            p.expected(codes::EXPECTED_EXPRESSION, "an expression");
            None
        }
    }
}

/// `Literal ::= INT_LIT | FLOAT_LIT | UNIT_LIT | COLOR_LIT | 'true' | 'false' | 'nil'`
pub(crate) fn literal(p: &mut Parser<'_>) -> CompletedMarker {
    if p.at(K::UnitLit) {
        check_unit(p);
    }
    let m = p.start();
    p.bump();
    m.complete(p, K::Literal)
}

/// `ImplicitMemberExpr ::= '.' name`
pub(crate) fn implicit_member(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    let dot = p.current_range();
    p.bump();
    if is_name_like(p.current()) {
        p.bump();
    } else if matches!(p.current(), K::IntLit | K::FloatLit | K::UnitLit) && p.joined() {
        missing_leading_zero(p, dot);
    } else {
        p.expected_token("a member name");
    }
    m.complete(p, K::ImplicitMemberExpr)
}

/// `.5em` is `Dot UnitLit`, not a number: K0100 at the number, with a fix
/// that writes the leading zero (`0.5em`).
fn missing_leading_zero(p: &mut Parser<'_>, dot: TextRange) {
    let number = p.nth_text(0);
    let span = p.span(p.current_range());
    let fix = p.fix(
        format!("Write `0.{number}`"),
        TextRange::empty(dot.start()),
        "0",
    );
    p.report(
        Diagnostic::new(
            codes::EXPECTED_TOKEN,
            span,
            format!("expected a member name, found `{number}`"),
        )
        .with_help(format!(
            "a number needs a digit before the `.`: write `0.{number}`"
        ))
        .with_fix(fix),
    );
}

/// `StringLit ::= STRING_START ( STRING_TEXT | STRING_ESCAPE | Interpolation )* STRING_END`
pub(crate) fn string_lit(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    loop {
        match p.current() {
            K::StringText | K::StringEscape => p.bump(),
            K::InterpStart => interpolation(p, true),
            K::StringEnd => {
                p.bump();
                break;
            }
            // Unterminated: the lexer reported K0002.
            _ => break,
        }
    }
    m.complete(p, K::StringLit)
}

/// `Interpolation ::= INTERP_START Expr INTERP_END`, in a string or (with
/// `in_string == false`) in a style value.
pub(crate) fn interpolation(p: &mut Parser<'_>, in_string: bool) {
    let m = p.start();
    let open = p.current_range();
    let open_index = p.position();
    p.bump();
    let limit = p.enter_interpolation(open_index);
    interpolation_body(p, in_string, open);
    p.leave_interpolation(limit);
    m.complete(p, K::Interpolation);
}

/// The contents of an interpolation after its `InterpStart`, up to and
/// including its `InterpEnd`.
fn interpolation_body(p: &mut Parser<'_>, in_string: bool, open: TextRange) {
    if p.at(K::InterpEnd) {
        let range = TextRange::new(open.start(), p.current_range().end());
        let span = p.span(range);
        let mut diagnostic = Diagnostic::new(
            codes::EMPTY_INTERPOLATION,
            span,
            "an interpolation must contain an expression",
        );
        if in_string {
            diagnostic = diagnostic
                .with_help("write `{{` for a literal `{`")
                .with_fix(p.fix("Write the braces literally", range, "{{}"));
        }
        p.report(diagnostic);
        p.bump();
        return;
    }
    if p.enter() {
        let saved = (p.newlines, p.no_trailing_lambda);
        p.newlines = in_string;
        p.no_trailing_lambda = false;
        required_expr(p);
        if !p.at(K::InterpEnd) && !p.at_eof() && !(in_string && p.on_new_line()) {
            p.expected_token("`}` to close the interpolation");
            p.skip_until(|p| {
                p.at(K::InterpEnd) || (in_string && p.on_new_line()) || p.at(K::StringEnd)
            });
        }
        p.eat(K::InterpEnd);
        (p.newlines, p.no_trailing_lambda) = saved;
        p.leave();
    }
}

/// `ParenExpr ::= '(' Expr ')'` or `TupleExpr ::= '(' Expr ',' Expr ... ')'`.
fn paren_or_tuple(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    let mut count = 0usize;
    let mut comma = false;
    let parsed = delimited(
        p,
        K::RParen,
        "`,` or `)`",
        |p| {
            if count > 0 {
                comma = true;
            }
            count += 1;
            expr(p);
        },
        can_start_expr,
    );
    if count == 0 && parsed {
        // `()`: there is no unit value.
        let span = p.span(p.prev_range());
        p.report(Diagnostic::new(
            codes::EXPECTED_EXPRESSION,
            span,
            "expected an expression inside the parentheses",
        ));
    }
    let kind = if comma || p.prev_trailing_comma() {
        K::TupleExpr
    } else {
        K::ParenExpr
    };
    m.complete(p, kind)
}

/// `ListExpr` or `MapExpr` (grammar.md §13.1).
fn list_or_map(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    if p.nth(1) == K::Colon && p.nth(2) == K::RBrack {
        p.bump();
        p.bump();
        p.bump();
        return m.complete(p, K::MapExpr);
    }
    let mut map = false;
    let mut first = true;
    delimited(
        p,
        K::RBrack,
        "`,` or `]`",
        |p| {
            if first {
                first = false;
                let Some(key) = expr_bp(p, 0) else {
                    return;
                };
                if p.at(K::Colon) {
                    map = true;
                    let entry = key.precede(p);
                    p.bump();
                    required_expr(p);
                    entry.complete(p, K::MapEntry);
                } else if p.at(K::WhenKw) && !p.nl() {
                    let w = key.precede(p);
                    p.bump();
                    if can_start_expr(p) {
                        expr_bp(p, 0);
                    } else {
                        p.expected(codes::EXPECTED_EXPRESSION, "a condition after `when`");
                    }
                    w.complete(p, K::WhenExpr);
                }
            } else if map {
                let entry = p.start();
                expr(p);
                if p.eat(K::Colon) {
                    required_expr(p);
                } else {
                    p.expected_token("`:` and a value");
                }
                entry.complete(p, K::MapEntry);
            } else {
                expr_with_when(p, true);
            }
        },
        can_start_expr,
    );
    m.complete(p, if map { K::MapExpr } else { K::ListExpr })
}

/// `ArgList ::= '(' ( Arg ( ',' Arg )* ','? )? ')'`
pub(crate) fn arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    delimited(p, K::RParen, "`,` or `)`", arg, can_start_arg);
    m.complete(p, K::ArgList);
}

/// Returns `true` if the current token can start an argument: an
/// expression or a `label:` prefix.
fn can_start_arg(p: &Parser<'_>) -> bool {
    can_start_expr(p) || (is_name_like(p.current()) && p.nth(1) == K::Colon)
}

/// `Arg ::= ( name ':' )? ( BindExpr | WhenExpr | Expr )`
fn arg(p: &mut Parser<'_>) {
    let m = p.start();
    if is_name_like(p.current()) && p.nth(1) == K::Colon {
        p.bump();
        p.bump();
    }
    if p.at(K::BindKw) {
        bind_expr(p, true);
    } else if can_start_expr(p) {
        expr_with_when(p, true);
    } else {
        p.expected(codes::EXPECTED_EXPRESSION, "an argument value");
    }
    m.complete(p, K::Arg);
}

/// `LambdaExpr ::= '{' LambdaParamList? lines( stmt ) '}'`
pub(crate) fn lambda(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    let open = p.current_range();
    p.bump();
    if p.enter() {
        let saved = (p.newlines, p.no_trailing_lambda);
        p.newlines = true;
        p.no_trailing_lambda = false;
        if lambda_params_ahead(p) {
            lambda_param_list(p);
        }
        statements::statement_list(p, open, false);
        (p.newlines, p.no_trailing_lambda) = saved;
        p.leave();
    }
    m.complete(p, K::LambdaExpr)
}

/// Returns `true` if the tokens after a lambda's `{` form a parameter list:
/// `IDENT (, IDENT)* =>`, or a parenthesized list followed by `=>` or `->`.
///
/// The scan has no length limit, so parameter lists of any length are
/// recognized, yet stays linear over a whole file: it stops at the first
/// `{` or `}` (types and parameter names contain no braces), so the scans of
/// different lambdas never cover the same tokens. It uses
/// [`Parser::peek`], which burns no fuel, because it may legitimately look
/// thousands of tokens ahead.
fn lambda_params_ahead(p: &Parser<'_>) -> bool {
    match p.current() {
        K::Ident => {
            let mut i = 0;
            loop {
                if p.peek(i) != K::Ident {
                    return false;
                }
                i += 1;
                match p.peek(i) {
                    K::FatArrow => return true,
                    K::Comma => i += 1,
                    _ => return false,
                }
            }
        }
        K::LParen => {
            let mut depth = 0usize;
            let mut i = 0;
            loop {
                match p.peek(i) {
                    K::LParen => depth += 1,
                    K::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            return matches!(p.peek(i + 1), K::FatArrow | K::Arrow);
                        }
                    }
                    K::LBrace | K::RBrace | EOF => return false,
                    _ => {}
                }
                i += 1;
            }
        }
        _ => false,
    }
}

/// `LambdaParamList ::= ( LambdaParam,* | '(' LambdaParam,* ')' ReturnType? ) '=>'`
fn lambda_param_list(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(K::LParen) {
        delimited(
            p,
            K::RParen,
            "`,` or `)`",
            |p| lambda_param(p, true),
            |p| p.at(K::Ident),
        );
        if p.at(K::Arrow) {
            items::return_type(p);
        }
    } else {
        loop {
            lambda_param(p, false);
            if !p.eat(K::Comma) {
                break;
            }
        }
    }
    p.expect(K::FatArrow, "`=>`");
    m.complete(p, K::LambdaParamList);
}

/// `LambdaParam ::= IDENT ( ':' Type )?`, where the type annotation is
/// allowed only when `typed` (in the parenthesized form).
fn lambda_param(p: &mut Parser<'_>, typed: bool) {
    let m = p.start();
    if p.at(K::Ident) {
        p.bump();
    } else {
        p.expected_token("a parameter name");
    }
    if typed && p.eat(K::Colon) {
        types::ty(p);
    }
    m.complete(p, K::LambdaParam);
}

// ------------------------------------------------------- delimited lists

/// Returns `true` for tokens that end any delimited list without belonging
/// to it: closing delimiters of enclosing constructs, the end of file, and
/// declaration keywords (which cannot appear inside an expression, except
/// as argument labels).
pub(crate) fn at_list_end(p: &Parser<'_>) -> bool {
    match p.current() {
        EOF | K::RParen | K::RBrack | K::RBrace | K::InterpEnd => true,
        K::FuncKw
        | K::StructKw
        | K::EnumKw
        | K::ClassKw
        | K::InterfaceKw
        | K::ExtendKw
        | K::ImportKw
        | K::InitKw
        | K::RpcKw
        | K::PublicKw
        | K::PrivateKw
        | K::StaticKw => p.nth(1) != K::Colon,
        _ => false,
    }
}

/// Parses `open element (',' element)* ','? close`, with the current token
/// being `open`. Newlines are insignificant and trailing lambdas allowed
/// inside (rules N1 and §13.4). A missing `close` is K0101 at `open`; a
/// missing `,` is K0100; junk is skipped into an `ErrorNode`.
///
/// Returns `false` if the nesting limit was reached and the content was
/// skipped (K0134), so callers do not report the list as empty.
pub(crate) fn delimited(
    p: &mut Parser<'_>,
    close: K,
    separators: &str,
    mut element: impl FnMut(&mut Parser<'_>),
    can_start: fn(&Parser<'_>) -> bool,
) -> bool {
    let open = p.current_range();
    p.bump();
    if !p.enter() {
        p.eat(close);
        return false;
    }
    let saved = (p.newlines, p.no_trailing_lambda);
    p.newlines = false;
    p.no_trailing_lambda = false;
    loop {
        if p.eat(close) {
            break;
        }
        if at_list_end(p) {
            p.unclosed(open);
            break;
        }
        if p.at(K::Comma) {
            p.expected_token("an element before `,`");
            p.bump();
            continue;
        }
        if p.eat_error_tokens() {
            continue;
        }
        if !can_start(p) {
            p.expected_token(separators);
            p.skip_until(|p| p.at(K::Comma) || p.at(close) || at_list_end(p) || can_start(p));
            continue;
        }
        let before = p.position();
        element(p);
        if p.position() == before {
            p.skip_until(|_| true);
        }
        if p.eat(K::Comma) {
            continue;
        }
        if p.eat(close) {
            break;
        }
        if at_list_end(p) {
            p.unclosed(open);
            break;
        }
        p.eat_error_tokens();
        if p.at(close) || p.at(K::Comma) || at_list_end(p) {
            continue;
        }
        p.expected_token(separators);
        if !can_start(p) {
            p.skip_until(|p| p.at(K::Comma) || p.at(close) || at_list_end(p) || can_start(p));
        }
    }
    (p.newlines, p.no_trailing_lambda) = saved;
    p.leave();
    true
}

impl Parser<'_> {
    /// Returns `true` if the token before the previous one is a `,` (a
    /// trailing comma before a just-consumed closing delimiter).
    pub(crate) fn prev_trailing_comma(&self) -> bool {
        let pos = self.position();
        pos >= 2 && self.kind_at(pos - 2) == K::Comma
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_suffixes() {
        assert_eq!(unit_suffix("16px"), "px");
        assert_eq!(unit_suffix("-1.5rem"), "rem");
        assert_eq!(unit_suffix("50%"), "%");
        assert_eq!(unit_suffix("1em"), "em");
        assert_eq!(unit_suffix("1e5px"), "px");
        assert_eq!(unit_suffix("1_000apples"), "apples");
        assert_eq!(unit_suffix("2n"), "n");
    }

    #[test]
    fn edit_distance_one() {
        assert!(within_one_edit("px", "pz"));
        assert!(within_one_edit("px", "pxx"));
        assert!(within_one_edit("px", "p"));
        assert!(within_one_edit("ms", "ms"));
        assert!(!within_one_edit("px", "zz"));
        assert!(!within_one_edit("rem", "r"));
    }
}
