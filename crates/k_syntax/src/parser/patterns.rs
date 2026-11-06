//! Patterns (grammar.md §14), used by `match`, `catch`, `for`, `failed` and
//! `expect throws`.

use k_diagnostics::{Diagnostic, codes};

use super::engine::Parser;
use super::expressions::{self, delimited};
use super::items::{decl_name, is_name_like};
use crate::kind::SyntaxKind as K;

/// Returns `true` if the current token can start a pattern.
fn can_start_pattern(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        K::Underscore
            | K::LetKw
            | K::Minus
            | K::IntLit
            | K::FloatLit
            | K::UnitLit
            | K::StringStart
            | K::TrueKw
            | K::FalseKw
            | K::NilKw
            | K::Dot
            | K::Ident
            | K::LParen
            | K::Error
    )
}

/// Parses one pattern. In a `for` loop (`for_loop`), a bare identifier is an
/// `IdentPattern`; elsewhere it is K0115 (and still an `IdentPattern`).
pub(crate) fn pattern(p: &mut Parser<'_>, for_loop: bool) {
    // Nesting is counted by the delimited lists that recurse (tuples and
    // case arguments), once per level.
    match p.current() {
        K::Underscore => {
            let m = p.start();
            p.bump();
            m.complete(p, K::WildcardPattern);
        }
        K::LetKw => {
            let m = p.start();
            p.bump();
            decl_name(p);
            m.complete(p, K::BindingPattern);
        }
        K::Minus
        | K::IntLit
        | K::FloatLit
        | K::UnitLit
        | K::StringStart
        | K::TrueKw
        | K::FalseKw
        | K::NilKw => literal_or_range(p),
        K::Dot => {
            let m = p.start();
            p.bump();
            if is_name_like(p.current()) {
                p.bump();
            } else {
                p.expected_token("an enum case name");
            }
            case_arguments(p, for_loop);
            m.complete(p, K::EnumCasePattern);
        }
        K::Ident if p.nth(1) == K::Dot => {
            let m = p.start();
            p.bump();
            while p.at(K::Dot) {
                p.bump();
                if is_name_like(p.current()) {
                    p.bump();
                } else {
                    p.expected_token("an enum case name");
                    break;
                }
            }
            case_arguments(p, for_loop);
            m.complete(p, K::EnumCasePattern);
        }
        K::Ident => {
            let range = p.current_range();
            let name = p.nth_text(0).to_owned();
            let m = p.start();
            p.bump();
            m.complete(p, K::IdentPattern);
            if !for_loop {
                let span = p.span(range);
                let bind = p
                    .fix(
                        format!("Bind it: `let {name}`"),
                        range,
                        format!("let {name}"),
                    )
                    .maybe_incorrect();
                let case = p
                    .fix(
                        format!("Match the case `.{name}`"),
                        range,
                        format!(".{name}"),
                    )
                    .maybe_incorrect();
                p.report(
                    Diagnostic::new(
                        codes::BARE_IDENTIFIER_PATTERN,
                        span,
                        format!("a bare name is not a pattern: `{name}`"),
                    )
                    .with_help("write `let name` to bind a value or `.name` to match an enum case")
                    .with_fix(bind)
                    .with_fix(case),
                );
            }
        }
        K::LParen => {
            let m = p.start();
            delimited(
                p,
                K::RParen,
                "`,` or `)`",
                |p| pattern(p, for_loop),
                can_start_pattern,
            );
            m.complete(p, K::TuplePattern);
        }
        K::Error => {
            p.eat_error_tokens();
        }
        _ => p.expected(codes::EXPECTED_PATTERN, "a pattern"),
    }
}

/// The optional `( pattern_arg, ... )` of an enum case pattern, with
/// `pattern_arg ::= IDENT ':' Pattern | Pattern`.
fn case_arguments(p: &mut Parser<'_>, for_loop: bool) {
    if !p.at(K::LParen) || p.nl() {
        return;
    }
    delimited(
        p,
        K::RParen,
        "`,` or `)`",
        |p| {
            if p.at(K::Ident) && p.nth(1) == K::Colon {
                let m = p.start();
                p.bump();
                p.bump();
                pattern(p, for_loop);
                m.complete(p, K::PatternField);
            } else {
                pattern(p, for_loop);
            }
        },
        can_start_pattern,
    );
}

/// `LiteralPattern ( ( '..<' | '...' ) LiteralPattern )?`
fn literal_or_range(p: &mut Parser<'_>) {
    let Some(lo) = literal_pattern(p) else {
        return;
    };
    if matches!(p.current(), K::DotDotLt | K::DotDotDot | K::DotDot) {
        let m = lo.precede(p);
        if p.at(K::DotDot) {
            let op = p.current_range();
            let span = p.span(op);
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
        p.bump();
        if matches!(
            p.current(),
            K::Minus
                | K::IntLit
                | K::FloatLit
                | K::UnitLit
                | K::StringStart
                | K::TrueKw
                | K::FalseKw
                | K::NilKw
        ) {
            literal_pattern(p);
        } else {
            p.expected(codes::EXPECTED_PATTERN, "the end of the range");
        }
        m.complete(p, K::RangePattern);
    }
}

/// `LiteralPattern ::= '-'? ( INT_LIT | FLOAT_LIT | UNIT_LIT ) | StringLit | 'true' | 'false' | 'nil'`
fn literal_pattern(p: &mut Parser<'_>) -> Option<super::engine::CompletedMarker> {
    let m = p.start();
    match p.current() {
        K::StringStart => {
            expressions::string_lit(p);
        }
        K::Minus => {
            p.bump();
            if matches!(p.current(), K::IntLit | K::FloatLit | K::UnitLit) {
                if p.at(K::UnitLit) {
                    expressions::check_unit(p);
                }
                p.bump();
            } else {
                p.expected(codes::EXPECTED_PATTERN, "a number after `-`");
            }
        }
        K::UnitLit => {
            expressions::check_unit(p);
            p.bump();
        }
        _ => p.bump(),
    }
    Some(m.complete(p, K::LiteralPattern))
}
