//! Blocks and statements (grammar.md §12), plus the block-shaped
//! expressions `if` and `match`.

use k_diagnostics::{Applicability, Diagnostic, Fix, TextEdit, TextRange, TextSize, codes};

use super::engine::{CompletedMarker, EOF, Parser};
use super::expressions::{self, can_start_expr};
use super::items::{self, decl_name, separator};
use super::{patterns, types};
use crate::kind::SyntaxKind as K;

/// Returns `true` if the current token can start a statement.
pub(crate) fn can_start_stmt(p: &Parser<'_>) -> bool {
    can_start_expr(p)
        || matches!(
            p.current(),
            K::LetKw
                | K::VarKw
                | K::ReturnKw
                | K::ThrowKw
                | K::BreakKw
                | K::ContinueKw
                | K::GuardKw
                | K::WhileKw
                | K::ForKw
                | K::DoKw
                | K::ExpectKw
        )
}

/// Returns `true` for tokens that cannot start a statement but start a
/// member or item. Meeting one inside a block means its `}` is missing: the
/// block ends there (K0101) and the token is parsed by the enclosing list.
fn at_block_terminator(p: &Parser<'_>, in_match: bool) -> bool {
    match p.current() {
        K::StyleKw
        | K::FuncKw
        | K::InitKw
        | K::RpcKw
        | K::StructKw
        | K::EnumKw
        | K::ClassKw
        | K::InterfaceKw
        | K::ExtendKw
        | K::ImportKw
        | K::PublicKw
        | K::PrivateKw
        | K::StaticKw
        | K::At => true,
        K::CaseKw | K::DefaultKw => !in_match,
        _ => false,
    }
}

/// `Block ::= '{' lines( stmt ) '}'`, at a `{`.
pub(crate) fn block(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.current_range();
    p.bump();
    if p.enter() {
        let saved = (p.newlines, p.no_trailing_lambda);
        p.newlines = true;
        p.no_trailing_lambda = false;
        statement_list(p, open, false);
        (p.newlines, p.no_trailing_lambda) = saved;
        p.leave();
    }
    m.complete(p, K::Block);
}

/// The statements of a block or lambda body after its `{`, up to and
/// including the closing `}`.
pub(crate) fn statement_list(p: &mut Parser<'_>, open: TextRange, in_match: bool) {
    loop {
        if p.eat(K::RBrace) {
            return;
        }
        if p.at_eof() || at_block_terminator(p, in_match) {
            p.unclosed(open);
            return;
        }
        if p.at(K::Semicolon) {
            items::semicolon(p);
            continue;
        }
        if can_start_stmt(p) {
            let before = p.position();
            stmt(p);
            if p.position() == before {
                p.skip_until(|_| true);
            }
            separator(p, K::RBrace, can_start_stmt);
        } else {
            let span = p.span(p.current_range());
            let message = format!(
                "expected a statement or expression, found {}",
                p.found_token()
            );
            p.report_junk(Diagnostic::new(codes::EXPECTED_EXPRESSION, span, message));
            p.skip_until(|p| {
                can_start_stmt(p) || p.at(K::RBrace) || at_block_terminator(p, in_match)
            });
        }
    }
}

/// `Stmt`: parses one statement at a token for which `can_start_stmt`
/// holds, dispatching on its first token. A `return` takes a value only
/// when one starts on the same line.
fn stmt(p: &mut Parser<'_>) {
    match p.current() {
        K::LetKw | K::VarKw => let_stmt(p),
        K::ReturnKw => {
            let m = p.start();
            p.bump();
            if !p.on_new_line() && can_start_expr(p) {
                expressions::expr(p);
            }
            m.complete(p, K::ReturnStmt);
        }
        K::ThrowKw => {
            let m = p.start();
            p.bump();
            expressions::required_expr(p);
            m.complete(p, K::ThrowStmt);
        }
        K::BreakKw => {
            let m = p.start();
            p.bump();
            m.complete(p, K::BreakStmt);
        }
        K::ContinueKw => {
            let m = p.start();
            p.bump();
            m.complete(p, K::ContinueStmt);
        }
        K::GuardKw => {
            let m = p.start();
            p.bump();
            conditions(p);
            p.expect(K::ElseKw, "`else`");
            items::required_block(p);
            m.complete(p, K::GuardStmt);
        }
        K::WhileKw => {
            let m = p.start();
            p.bump();
            conditions(p);
            items::required_block(p);
            m.complete(p, K::WhileStmt);
        }
        K::ForKw => for_stmt(p),
        K::DoKw => do_stmt(p),
        K::ExpectKw => expect_stmt(p),
        _ => expr_stmt(p),
    }
}

/// An expression statement, or an `AssignStmt` if an assignment operator
/// follows the expression.
fn expr_stmt(p: &mut Parser<'_>) {
    let Some(lhs) = expressions::expr(p) else {
        return;
    };
    if at_assignment_operator(p) {
        let m = lhs.precede(p);
        p.bump();
        expressions::required_expr(p);
        chained_assignment(p);
        m.complete(p, K::AssignStmt);
    }
}

/// Returns `true` if the current token is `=` or a compound assignment
/// operator (`+=`, `-=`, `*=`, `/=`, `%=`).
fn at_assignment_operator(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        K::Eq | K::PlusEq | K::MinusEq | K::StarEq | K::SlashEq | K::PercentEq
    )
}

/// `a = b = c`: assignment is a statement, not an expression, so a second
/// assignment operator on the same line is K0100 at that operator. The
/// operator and its value are kept in an `ErrorNode` inside the statement,
/// so the rest of the line is not a junk run.
fn chained_assignment(p: &mut Parser<'_>) {
    while at_assignment_operator(p) && !p.on_new_line() {
        let span = p.span(p.current_range());
        p.report(
            Diagnostic::new(codes::EXPECTED_TOKEN, span, "assignments cannot be chained")
                .with_help("assign each variable in its own statement"),
        );
        let m = p.start();
        p.bump();
        if can_start_expr(p) && !p.nl() {
            expressions::expr(p);
        }
        m.complete(p, K::ErrorNode);
    }
}

/// `LetStmt ::= ( 'let' | 'var' ) IDENT ( ':' Type )? ( '=' Expr )?`
fn let_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    decl_name(p);
    if p.eat(K::Colon) {
        types::ty(p);
    }
    if p.eat(K::Eq) {
        expressions::required_expr(p);
        chained_assignment(p);
    }
    m.complete(p, K::LetStmt);
}

/// `conditions ::= condition ( ',' condition )*` with
/// `condition ::= LetCondition | expr_nt`.
fn conditions(p: &mut Parser<'_>) {
    loop {
        if p.at(K::LetKw) {
            let m = p.start();
            p.bump();
            decl_name(p);
            if p.eat(K::Eq) {
                expressions::expr_no_lambda(p);
            }
            m.complete(p, K::LetCondition);
        } else {
            expressions::expr_no_lambda(p);
        }
        if !p.eat(K::Comma) {
            break;
        }
    }
}

/// `for a, b in xs`: K0100 at the first `,` with a fix that writes the tuple
/// pattern `(a, b)`. The further patterns are kept in an `ErrorNode`, so
/// the statement then parses normally instead of starting a junk run at
/// `in`. `first` is the start of the first pattern.
fn unparenthesized_for_tuple(p: &mut Parser<'_>, first: TextSize) {
    let span = p.span(p.current_range());
    let message = format!("expected `in`, found {}", p.found_token());
    let errors = p.start();
    while p.eat(K::Comma) {
        if p.at(K::InKw) || p.on_new_line() {
            break;
        }
        patterns::pattern(p, true);
    }
    errors.complete(p, K::ErrorNode);
    let last = TextSize::from(p.prev_end());
    let fix = Fix {
        message: "Wrap the names in parentheses".to_owned(),
        edits: vec![
            TextEdit {
                span: p.span(TextRange::empty(first)),
                replacement: "(".to_owned(),
            },
            TextEdit {
                span: p.span(TextRange::empty(last)),
                replacement: ")".to_owned(),
            },
        ],
        applicability: Applicability::MaybeIncorrect,
    };
    p.report(
        Diagnostic::new(codes::EXPECTED_TOKEN, span, message)
            .with_help("to destructure each element, write a tuple pattern: `for (a, b) in ...`")
            .with_fix(fix),
    );
}

/// `ForStmt ::= 'for' 'await'? for_pattern 'in' expr_nt KeyClause? Block`
fn for_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    // `for await x in stream`: reading a stream (29-server-streaming.md).
    p.eat(K::AwaitKw);
    let first = p.current_range().start();
    patterns::pattern(p, true);
    if p.at(K::Comma) && !p.on_new_line() {
        unparenthesized_for_tuple(p, first);
    }
    if !p.eat(K::InKw) {
        p.expected_token("`in`");
    }
    if can_start_expr(p) && !p.at(K::LBrace) {
        expressions::expr_no_lambda(p);
    } else {
        p.expected(codes::EXPECTED_EXPRESSION, "the sequence to iterate");
    }
    if p.at_word("key") {
        let k = p.start();
        p.bump_remap(K::KeyKw);
        if can_start_expr(p) && !p.at(K::LBrace) {
            expressions::expr_no_lambda(p);
        } else {
            p.expected(codes::EXPECTED_EXPRESSION, "a key expression");
        }
        k.complete(p, K::KeyClause);
    }
    items::required_block(p);
    m.complete(p, K::ForStmt);
}

/// `DoStmt ::= 'do' Block CatchClause+`
fn do_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    items::required_block(p);
    if !p.at(K::CatchKw) {
        p.expected_token("`catch`");
    }
    while p.at(K::CatchKw) {
        let c = p.start();
        p.bump();
        loop {
            patterns::pattern(p, false);
            if !p.eat(K::Comma) {
                break;
            }
        }
        if p.at(K::WhereKw) {
            match_guard(p);
        }
        items::required_block(p);
        c.complete(p, K::CatchClause);
    }
    m.complete(p, K::DoStmt);
}

/// `ExpectStmt ::= 'expect' Expr` and `ExpectThrowsStmt ::= 'expect' 'throws' Pattern Block`
fn expect_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    if p.at(K::ThrowsKw) {
        p.bump();
        patterns::pattern(p, false);
        items::required_block(p);
        m.complete(p, K::ExpectThrowsStmt);
    } else {
        expressions::required_expr(p);
        m.complete(p, K::ExpectStmt);
    }
}

/// `IfExpr ::= 'if' conditions Block ( 'else' ( IfExpr | Block ) )?`
pub(crate) fn if_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    conditions(p);
    items::required_block(p);
    if p.at(K::ElseKw) {
        p.bump();
        if p.at(K::IfKw) {
            if p.enter() {
                if_expr(p);
                p.leave();
            }
        } else {
            items::required_block(p);
        }
    }
    m.complete(p, K::IfExpr)
}

/// `MatchExpr ::= 'match' expr_nt '{' lines( MatchArm ) '}'`
pub(crate) fn match_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    expressions::expr_no_lambda(p);
    p.eat_error_tokens();
    if !p.at(K::LBrace) {
        p.expected_token("`{`");
        return m.complete(p, K::MatchExpr);
    }
    let open = p.current_range();
    p.bump();
    if p.enter() {
        let saved = (p.newlines, p.no_trailing_lambda);
        p.newlines = true;
        p.no_trailing_lambda = false;
        loop {
            if p.eat(K::RBrace) {
                break;
            }
            if p.at_eof() || at_block_terminator(p, true) {
                p.unclosed(open);
                break;
            }
            if p.at(K::Semicolon) {
                items::semicolon(p);
                continue;
            }
            if at_arm_start(p) {
                match_arm(p);
                separator(p, K::RBrace, at_arm_start);
            } else if p.eat_error_tokens() {
            } else {
                p.expected_token("`case` or `default`");
                p.skip_until(|p| at_arm_start(p) || p.at(K::RBrace) || p.at(EOF));
            }
        }
        (p.newlines, p.no_trailing_lambda) = saved;
        p.leave();
    }
    m.complete(p, K::MatchExpr)
}

/// Returns `true` if the current token starts a match arm (`case` or
/// `default`).
fn at_arm_start(p: &Parser<'_>) -> bool {
    matches!(p.current(), K::CaseKw | K::DefaultKw)
}

/// `MatchArm ::= 'case' Pattern ( ',' Pattern )* MatchGuard? Block | 'default' Block`
fn match_arm(p: &mut Parser<'_>) {
    let m = p.start();
    if p.eat(K::DefaultKw) {
        arm_block(p);
        m.complete(p, K::MatchArm);
        return;
    }
    p.bump();
    loop {
        patterns::pattern(p, false);
        if !p.eat(K::Comma) {
            break;
        }
    }
    if p.at(K::WhereKw) {
        match_guard(p);
    }
    arm_block(p);
    m.complete(p, K::MatchArm);
}

/// The block of a match arm; `case` and `default` inside it end it when its
/// `}` is missing.
fn arm_block(p: &mut Parser<'_>) {
    p.eat_error_tokens();
    if !p.at(K::LBrace) {
        p.expected_token("`{`");
        return;
    }
    let m = p.start();
    let open = p.current_range();
    p.bump();
    if p.enter() {
        let saved = (p.newlines, p.no_trailing_lambda);
        p.newlines = true;
        p.no_trailing_lambda = false;
        statement_list(p, open, false);
        (p.newlines, p.no_trailing_lambda) = saved;
        p.leave();
    }
    m.complete(p, K::Block);
}

/// `MatchGuard ::= 'where' expr_nt`
fn match_guard(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    expressions::expr_no_lambda(p);
    m.complete(p, K::MatchGuard);
}
