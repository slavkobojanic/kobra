//! Types (grammar.md §11) and the generic parameter and `where` clauses of
//! declarations (§10).

use k_diagnostics::{Diagnostic, TextRange, TextSize, codes};

use super::engine::{CompletedMarker, Parser};
use super::expressions::delimited;
use super::items;
use crate::kind::SyntaxKind as K;

/// Returns `true` if the current token can start a type.
pub(crate) fn can_start_type(p: &Parser<'_>) -> bool {
    matches!(p.current(), K::Ident | K::LParen)
}

/// Parses a type, reporting K0123 if none starts here.
pub(crate) fn ty(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    if p.eat_error_tokens() && (!can_start_type(p) || p.on_new_line()) {
        // The `Error` token stands in for the type; K0001 covers it.
        return None;
    }
    if !can_start_type(p) {
        p.expected(codes::EXPECTED_TYPE, "a type");
        return None;
    }
    // Nesting is counted by the delimited lists that recurse (generic
    // arguments and parenthesized types), once per level.
    let mut node = if p.at(K::Ident) {
        path_type(p)
    } else {
        paren_type(p)
    };
    // `?` binds to the type immediately before it. Every further `?` on the
    // same line (`T??`, `T? ?`, `T???`) is K0120: one diagnostic covering all
    // of them, whose fix deletes them all, so applying it leaves `T?`. The
    // node is a single `OptionalType` holding every `?` token.
    if matches!(p.current(), K::Question | K::QuestionQuestion) && !p.nl() {
        let m = node.precede(p);
        let first = p.current_range();
        let mut extra_start = if p.at(K::QuestionQuestion) {
            Some(first.start() + TextSize::from(1))
        } else {
            None
        };
        p.bump();
        let mut end = first.end();
        while matches!(p.current(), K::Question | K::QuestionQuestion) && !p.nl() {
            let range = p.current_range();
            extra_start.get_or_insert(range.start());
            end = range.end();
            p.bump();
        }
        if let Some(start) = extra_start {
            nested_optional(p, TextRange::new(start, end));
        }
        node = m.complete(p, K::OptionalType);
    }
    Some(node)
}

/// Reports K0120 on `extra`, the `?` characters after the first one.
fn nested_optional(p: &mut Parser<'_>, extra: TextRange) {
    let fix = p.fix("Delete the extra `?`", extra, "");
    let span = p.span(extra);
    p.report(
        Diagnostic::new(
            codes::NESTED_OPTIONAL,
            span,
            "optional types cannot be nested",
        )
        .with_fix(fix),
    );
}

/// `PathType ::= IDENT ( '.' IDENT )* GenericArgList?`
fn path_type(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    while p.at(K::Dot) && p.nth(1) == K::Ident {
        p.bump();
        p.bump();
    }
    if p.at(K::Lt) {
        generic_arg_list(p);
    }
    m.complete(p, K::PathType)
}

/// `GenericArgList ::= '<' Type ( ',' Type )* ','? '>'`, at the `<`.
pub(crate) fn generic_arg_list(p: &mut Parser<'_>) {
    let g = p.start();
    let mut count = 0usize;
    let parsed = delimited(
        p,
        K::Gt,
        "`,` or `>`",
        |p| {
            count += 1;
            ty(p);
        },
        can_start_type,
    );
    if count == 0 && parsed {
        let span = p.span(p.prev_range());
        p.report(Diagnostic::new(
            codes::EXPECTED_TYPE,
            span,
            "expected a type argument",
        ));
    }
    g.complete(p, K::GenericArgList);
}

/// Returns `true` if the current token starts the explicit type arguments
/// of a name in an expression (`decode<User>(json)`, `Jwt.verify<Claims>(…)`,
/// `Sealed<Note>.parse(text)`; grammar.md RA-23).
///
/// A comparison can't be chained (K0113), so `a<b>(c)` was never a valid
/// comparison; what stays ambiguous is two arguments such as
/// `f(a<b, c>(d))`. The list is therefore taken only when it is written
/// the way a type is: the `<` directly after the name and the `>` directly
/// before a `(` or `.` (or followed by a trailing lambda's `{` on the same
/// line), with no space inside the brackets' ends, every type argument
/// starting with an uppercase name, and all of it on one line. Anything
/// else stays a comparison.
pub(crate) fn type_args_ahead(p: &Parser<'_>) -> bool {
    if !p.at(K::Lt) || !p.joined() || !p.nth_joined(1) {
        return false;
    }
    let mut depth = 0usize;
    let mut n = 0usize;
    // Whether the next token starts a type argument (after `<` or `,`).
    let mut at_argument = true;
    loop {
        let kind = p.peek(n);
        if n > 0 && p.nth_on_new_line(n) {
            return false;
        }
        match kind {
            K::Lt => {
                depth += 1;
                at_argument = true;
                n += 1;
                continue;
            }
            K::Gt => {
                depth -= 1;
                if depth == 0 {
                    let next = p.peek(n + 1);
                    return p.nth_joined(n)
                        && (matches!(next, K::LParen | K::Dot) && p.nth_joined(n + 1)
                            || next == K::LBrace && !p.nth_on_new_line(n + 1));
                }
            }
            K::Ident => {
                if at_argument && depth == 1 && !p.nth_text(n).starts_with(char::is_uppercase) {
                    return false;
                }
            }
            K::Comma
            | K::Dot
            | K::Question
            | K::QuestionQuestion
            | K::LParen
            | K::RParen
            | K::Arrow
            | K::AsyncKw
            | K::ThrowsKw => {}
            _ => return false,
        }
        at_argument = kind == K::Comma && depth == 1;
        n += 1;
    }
}

/// `TupleType`, `FuncType` or `ParenType`, decided after the `)`.
fn paren_type(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    let mut count = 0usize;
    delimited(
        p,
        K::RParen,
        "`,` or `)`",
        |p| {
            count += 1;
            ty(p);
        },
        can_start_type,
    );
    let trailing_comma = p.prev_trailing_comma();
    if matches!(p.current(), K::AsyncKw | K::ThrowsKw | K::Arrow) {
        // The thrown and return types recurse after the `)` has closed, so
        // they count one nesting level of their own (RA-34): otherwise
        // `() -> () -> ... -> T` would recurse without bound.
        if !p.enter() {
            return m.complete(p, K::FuncType);
        }
        p.eat(K::AsyncKw);
        if p.at(K::ThrowsKw) {
            items::throws_clause(p);
        }
        if p.at(K::Arrow) {
            items::return_type(p);
        } else {
            p.expected_token("`->` and the return type");
        }
        p.leave();
        return m.complete(p, K::FuncType);
    }
    match count {
        0 => {
            p.expected_token("`->` after `()`");
            m.complete(p, K::FuncType)
        }
        1 if !trailing_comma => m.complete(p, K::ParenType),
        _ => m.complete(p, K::TupleType),
    }
}

/// `GenericParamList ::= '<' GenericParam ( ',' GenericParam )* ','? '>'`
pub(crate) fn generic_param_list(p: &mut Parser<'_>) {
    let m = p.start();
    let mut count = 0usize;
    let parsed = delimited(
        p,
        K::Gt,
        "`,` or `>`",
        |p| {
            count += 1;
            let g = p.start();
            p.bump();
            g.complete(p, K::GenericParam);
        },
        |p| p.at(K::Ident),
    );
    if count == 0 && parsed {
        let span = p.span(p.prev_range());
        p.report(Diagnostic::new(
            codes::EXPECTED_TOKEN,
            span,
            "expected a generic parameter name",
        ));
    }
    m.complete(p, K::GenericParamList);
}

/// `WhereClause ::= 'where' WherePredicate ( ',' WherePredicate )*` with
/// `WherePredicate ::= IDENT 'implements' Type`.
pub(crate) fn where_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    loop {
        if !p.at(K::Ident) {
            p.expected_token("a generic parameter name");
            break;
        }
        let w = p.start();
        p.bump();
        if p.eat(K::ImplementsKw) {
            ty(p);
        } else {
            p.expected_token("`implements`");
        }
        w.complete(p, K::WherePredicate);
        if !p.eat(K::Comma) {
            break;
        }
    }
    m.complete(p, K::WhereClause);
}
