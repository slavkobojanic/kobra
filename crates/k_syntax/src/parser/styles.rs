//! Style blocks (grammar.md §16): rules, selectors, at-rules and
//! declarations, over the tokens the lexer produces in style mode (§4.4).

use k_diagnostics::{Diagnostic, TextRange, TextSize, codes};

use super::engine::{EOF, Marker, Parser};
use super::expressions::{self, at_list_end};
use crate::kind::SyntaxKind as K;

/// `StyleBlock ::= 'style' '{' css_item* '}'`, at `style` (followed by `{`).
pub(crate) fn style_block(p: &mut Parser<'_>, m: Marker) {
    p.bump();
    let open = p.current_range();
    p.bump();
    if p.enter() {
        css_items(p, open, true);
        p.leave();
    }
    m.complete(p, K::StyleBlock);
}

/// Returns `true` if the current token can start a compound selector.
fn can_start_compound(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        K::Dot | K::Amp | K::Colon | K::LBrack | K::Star | K::Ident | K::ColorLit
    )
}

/// Returns `true` if the current token can start a style item.
fn can_start_item(p: &Parser<'_>) -> bool {
    p.at(K::At) || can_start_compound(p)
}

/// The items of a style block or nested block after its `{`, through the
/// closing `}`. `top` is `true` directly in the `style` block, where
/// declarations are K0133.
fn css_items(p: &mut Parser<'_>, open: TextRange, top: bool) {
    loop {
        if p.eat(K::RBrace) {
            return;
        }
        if p.at_eof() {
            p.unclosed(open);
            return;
        }
        let before = p.position();
        match p.current() {
            K::Semicolon => p.bump(),
            K::At => at_rule(p),
            K::Ident => declaration(p, top),
            K::Error => {
                p.eat_error_tokens();
            }
            _ if can_start_item(p) => style_rule(p),
            _ => {
                p.expected_token("a style rule or declaration");
                p.skip_until(|p| can_start_item(p) || p.at(K::RBrace) || p.at(K::Semicolon));
            }
        }
        if p.position() == before {
            p.skip_until(|_| true);
        }
    }
}

// ------------------------------------------------------------ declarations

/// `Declaration ::= IDENT ':' CssValue ( ',' CssValue )* ';'?`
fn declaration(p: &mut Parser<'_>, top: bool) {
    if p.nth(1) == K::InterpStart {
        element_rule(p);
        return;
    }
    let m = p.start();
    p.bump();
    if !p.eat(K::Colon) {
        p.expected_token("`:` after the property name");
    }
    let has_value = at_value_start(p);
    if has_value {
        loop {
            css_value(p, true);
            if !p.eat(K::Comma) {
                break;
            }
        }
    } else if !p.at(K::Bang) {
        p.expected_token("a value");
    }
    if p.at(K::Bang) {
        important(p, has_value);
    }
    if !p.eat(K::Semicolon) && !p.at(K::RBrace) && !p.at_eof() {
        let gap = TextRange::empty(TextSize::from(p.prev_end()));
        let fix = p.fix("Insert `;`", gap, ";");
        let span = p.span(gap);
        p.report(
            Diagnostic::new(
                codes::MISSING_SEMICOLON,
                span,
                "expected `;` after the declaration",
            )
            .with_help("every declaration except the last in its block ends with `;`")
            .with_fix(fix),
        );
    }
    let node = m.complete(p, K::Declaration);
    if top {
        let span = p.span(node.range(p));
        p.report(
            Diagnostic::new(
                codes::DECLARATION_OUTSIDE_RULE,
                span,
                "a declaration must be inside a rule",
            )
            .with_help("put it in a class rule such as `.root { ... }`"),
        );
    }
}

/// `button { ... }` at the start of a statement: the lexer treated `button`
/// as a property name and `{` as an interpolation (grammar.md RA-8). The
/// name becomes a `TypeSelector` (K0116) and the braces' content is skipped.
fn element_rule(p: &mut Parser<'_>) {
    let rule = p.start();
    let selector = p.start();
    let compound = p.start();
    type_selector(p);
    compound.complete(p, K::CompoundSelector);
    selector.complete(p, K::Selector);
    p.bump();
    if !p.at(K::InterpEnd) {
        p.skip_until(|p| p.at(K::InterpEnd));
    }
    p.eat(K::InterpEnd);
    rule.complete(p, K::StyleRule);
}

/// `!important` (K0118), wrapped in an `ErrorNode`. The fix that deletes it
/// is offered only after a value (`has_value`): deleting the whole value of
/// `color: !important;` would leave the invalid `color:;`.
fn important(p: &mut Parser<'_>, has_value: bool) {
    let ws = p.whitespace_before();
    let bang = p.current_range();
    let m = p.start();
    p.bump();
    if p.at_word("important") && p.joined() {
        let range = TextRange::new(bang.start(), p.current_range().end());
        p.bump();
        let span = p.span(range);
        let diagnostic = Diagnostic::new(
            codes::IMPORTANT_NOT_SUPPORTED,
            span,
            "`!important` is not supported",
        );
        let diagnostic = if has_value {
            let fix = p.fix(
                "Remove `!important`",
                TextRange::new(ws.start(), range.end()),
                "",
            );
            diagnostic
                .with_help("scoped styles make it unnecessary")
                .with_fix(fix)
        } else {
            diagnostic
                .with_help("write the value here; scoped styles make `!important` unnecessary")
        };
        p.report(diagnostic);
    } else {
        p.expected_token("`important` after `!`");
    }
    m.complete(p, K::ErrorNode);
}

/// Returns `true` if the current token can start a value component.
fn at_value_start(p: &Parser<'_>) -> bool {
    match p.current() {
        K::IntLit
        | K::FloatLit
        | K::UnitLit
        | K::ColorLit
        | K::StringStart
        | K::InterpStart
        | K::Dot
        | K::Plus
        | K::Minus
        | K::Star
        | K::Slash
        | K::Error => true,
        K::Ident => p.nth(1) != K::Colon,
        _ => false,
    }
}

/// `CssValue ::= css_component+`. `check_units` is `false` inside
/// pseudo-class arguments, where unit suffixes are not validated.
fn css_value(p: &mut Parser<'_>, check_units: bool) {
    let m = p.start();
    let mut any = false;
    loop {
        match p.current() {
            K::IntLit | K::FloatLit | K::UnitLit | K::ColorLit => {
                if check_units {
                    expressions::literal(p);
                } else {
                    let l = p.start();
                    p.bump();
                    l.complete(p, K::Literal);
                }
            }
            K::StringStart => {
                expressions::string_lit(p);
            }
            K::Ident => {
                if p.nth(1) == K::Colon {
                    break;
                }
                if p.nth(1) == K::LParen && p.nth_joined(1) {
                    css_function(p, check_units);
                } else {
                    p.bump();
                }
            }
            K::InterpStart => expressions::interpolation(p, false),
            K::Dot => css_expr(p),
            K::Plus | K::Minus | K::Star | K::Slash => p.bump(),
            K::Error => {
                p.eat_error_tokens();
            }
            _ => break,
        }
        any = true;
    }
    if any {
        m.complete(p, K::CssValue);
    } else {
        m.abandon(p);
    }
}

/// `css_expr ::= ImplicitMemberExpr ( ( '.' name ) | ArgList )*`, with every
/// part written without trivia before it.
fn css_expr(p: &mut Parser<'_>) {
    let mut node = expressions::implicit_member(p);
    loop {
        if p.at(K::Dot) && p.joined() && p.nth(1).is_name_like() {
            let m = node.precede(p);
            p.bump();
            p.bump();
            node = m.complete(p, K::MemberExpr);
        } else if p.at(K::LParen) && p.joined() {
            let m = node.precede(p);
            expressions::arg_list(p);
            node = m.complete(p, K::CallExpr);
        } else {
            break;
        }
    }
}

/// `CssFunction ::= IDENT '(' CssValue ( ',' CssValue )* ')'`
fn css_function(p: &mut Parser<'_>, check_units: bool) {
    let m = p.start();
    p.bump();
    let open = p.current_range();
    p.bump();
    if !p.enter() {
        m.complete(p, K::CssFunction);
        return;
    }
    loop {
        if p.at(K::RParen) {
            break;
        }
        if at_value_start(p) {
            css_value(p, check_units);
        } else if !p.at(K::Comma) {
            break;
        } else {
            p.expected_token("a value before `,`");
        }
        if !p.eat(K::Comma) {
            break;
        }
    }
    close_paren(p, open);
    p.leave();
    m.complete(p, K::CssFunction);
}

/// Consumes the `)` of a style parenthesis opened at `open`, reporting
/// K0101 when it is missing.
fn close_paren(p: &mut Parser<'_>, open: TextRange) {
    if p.eat(K::RParen) {
        return;
    }
    if !at_list_end(p) && !p.at(K::LBrace) && !p.at(K::Semicolon) {
        p.expected_token("`)`");
        p.skip_until(|p| {
            p.at(K::RParen) || at_list_end(p) || p.at(K::LBrace) || p.at(K::Semicolon)
        });
        if p.eat(K::RParen) {
            return;
        }
    }
    p.unclosed(open);
}

// --------------------------------------------------------------- selectors

/// `StyleRule ::= Selector ( ',' Selector )* '{' css_item* '}'`
fn style_rule(p: &mut Parser<'_>) {
    let m = p.start();
    loop {
        selector(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    if p.at(K::LBrace) {
        let open = p.current_range();
        p.bump();
        if p.enter() {
            css_items(p, open, false);
            p.leave();
        }
    } else {
        p.expected_token("`{` to open the rule");
        if !p.at(K::RBrace) && !p.at(EOF) && !can_start_item(p) {
            p.skip_until(|p| p.at(K::RBrace) || can_start_item(p));
        }
    }
    m.complete(p, K::StyleRule);
}

/// `Selector ::= CompoundSelector ( ( '>' | '+' | '~' )? CompoundSelector )*`
fn selector(p: &mut Parser<'_>) {
    let m = p.start();
    let mut need = true;
    loop {
        if can_start_compound(p) {
            compound(p);
            need = false;
        } else {
            if need {
                p.expected_token("a selector");
            }
            break;
        }
        if matches!(p.current(), K::Gt | K::Plus | K::Tilde) {
            p.bump();
            need = true;
            continue;
        }
        if can_start_compound(p) && !p.joined() {
            continue;
        }
        break;
    }
    m.complete(p, K::Selector);
}

/// `CompoundSelector ::= simple_selector+` without trivia in between.
fn compound(p: &mut Parser<'_>) {
    let m = p.start();
    simple_selector(p);
    while p.joined() && can_start_compound(p) {
        simple_selector(p);
    }
    m.complete(p, K::CompoundSelector);
}

fn simple_selector(p: &mut Parser<'_>) {
    match p.current() {
        K::Dot => {
            let m = p.start();
            p.bump();
            if p.at(K::Ident) {
                p.bump();
            } else {
                p.expected_token("a class name");
            }
            m.complete(p, K::ClassSelector);
        }
        K::Amp => {
            let m = p.start();
            p.bump();
            m.complete(p, K::NestingSelector);
        }
        K::Colon if p.nth(1) == K::Colon && p.nth_joined(1) => {
            let m = p.start();
            p.bump();
            p.bump();
            if p.at(K::Ident) {
                p.bump();
            } else {
                p.expected_token("a pseudo-element name");
            }
            m.complete(p, K::PseudoElement);
        }
        K::Colon => pseudo_class(p),
        K::LBrack => attribute_selector(p),
        K::Star => {
            let m = p.start();
            p.bump();
            m.complete(p, K::UniversalSelector);
        }
        K::Ident => type_selector(p),
        K::ColorLit => {
            let range = p.current_range();
            let m = p.start();
            p.bump();
            m.complete(p, K::ErrorNode);
            let span = p.span(range);
            p.report(
                Diagnostic::new(codes::ID_SELECTOR, span, "style blocks cannot select by id")
                    .with_help("give the element a class and select that"),
            );
        }
        _ => p.expected_token("a selector"),
    }
}

/// `TypeSelector ::= IDENT`, always K0116.
fn type_selector(p: &mut Parser<'_>) {
    let range = p.current_range();
    let name = p.nth_text(0).to_owned();
    let m = p.start();
    p.bump();
    m.complete(p, K::TypeSelector);
    let span = p.span(range);
    p.report(
        Diagnostic::new(
            codes::TYPE_SELECTOR,
            span,
            format!("style blocks cannot select elements by name: `{name}`"),
        )
        .with_help("give the element a class and select that"),
    );
}

/// `PseudoClass ::= ':' IDENT ( '(' ( Selector,* | CssValue ) ')' )?`
fn pseudo_class(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    let name = p.nth_text(0);
    let selector_list = matches!(name, "not" | "is" | "where" | "has");
    if p.at(K::Ident) {
        p.bump();
    } else {
        p.expected_token("a pseudo-class name");
    }
    if p.at(K::LParen) && p.joined() {
        let open = p.current_range();
        p.bump();
        if p.enter() {
            if selector_list {
                loop {
                    selector(p);
                    if !p.eat(K::Comma) {
                        break;
                    }
                }
            } else if at_value_start(p) {
                css_value(p, false);
            } else {
                p.expected_token("an argument");
            }
            close_paren(p, open);
            p.leave();
        }
    }
    m.complete(p, K::PseudoClass);
}

/// `AttributeSelector ::= '[' IDENT ( '=' ( StringLit | IDENT ) )? ']'`
fn attribute_selector(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.current_range();
    p.bump();
    if p.at(K::Ident) {
        p.bump();
    } else {
        p.expected_token("an attribute name");
    }
    if p.eat(K::Eq) {
        match p.current() {
            K::StringStart => {
                expressions::string_lit(p);
            }
            K::Ident => p.bump(),
            _ => p.expected_token("an attribute value"),
        }
    }
    if !p.eat(K::RBrack) {
        p.unclosed(open);
    }
    m.complete(p, K::AttributeSelector);
}

// --------------------------------------------------------------- at-rules

/// `AtRule ::= '@' IDENT AtPrelude? ( '{' css_item* '}' | ';' )`
fn at_rule(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    if p.at(K::Ident) {
        p.bump();
    } else {
        p.expected_token("an at-rule name such as `media`");
    }
    if matches!(p.current(), K::Ident | K::LParen) {
        let prelude = p.start();
        loop {
            match p.current() {
                K::Ident => p.bump(),
                K::LParen => media_feature(p),
                _ => break,
            }
        }
        prelude.complete(p, K::AtPrelude);
    }
    if p.at(K::LBrace) {
        let open = p.current_range();
        p.bump();
        if p.enter() {
            css_items(p, open, false);
            p.leave();
        }
    } else if !p.eat(K::Semicolon) {
        p.expected_token("`{` or `;`");
    }
    m.complete(p, K::AtRule);
}

/// `MediaFeature ::= '(' IDENT ( ':' CssValue )? ')'`
fn media_feature(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.current_range();
    p.bump();
    if p.at(K::Ident) {
        p.bump();
    } else {
        p.expected_token("a media feature name");
    }
    if p.eat(K::Colon) {
        if at_value_start(p) {
            css_value(p, true);
        } else {
            p.expected_token("a value");
        }
    }
    close_paren(p, open);
    m.complete(p, K::MediaFeature);
}
