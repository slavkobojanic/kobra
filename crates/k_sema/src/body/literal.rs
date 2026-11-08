//! Checked literals (sema.md §9.7; `docs/design/20-standard-library.md`,
//! "Literals and parsing").
//!
//! An initializer marked `@Literal` (`Url("…")`, `Uuid("…")`,
//! `EmailAddress("…")`, `TimeOfDay("…")`, `StorageKey(name:)`,
//! `HttpStatus(code:)`) takes a literal: anything else, an interpolated
//! string included, is **K2403**, with a fix to the type's `parse` when it
//! has one. A literal its type's parser refuses is **K2404**, with the
//! reason; the parsers are the runtime's, ported in [`crate::literals`]. A
//! money amount such as `1.005.usd` with more significant decimal places
//! than its currency is K2404 too, and adding or subtracting amounts whose
//! literal currencies differ (`1.usd + 2.eur`) is **K2407**.

use k_diagnostics::{Diagnostic, Fix, Span, TextRange, codes};
use k_syntax::SyntaxKind;
use k_syntax::ast::{self, AstNode};

use super::{Arg, Checker};
use crate::cx::SigKey;
use crate::literals;
use crate::program::{DeclId, MemberKind};
use crate::ty::Ty;

/// The literal an `@Literal` initializer takes.
enum Wanted {
    /// A plain string literal.
    Text,
    /// An integer literal.
    Integer,
}

/// What an argument of an `@Literal` initializer is.
enum Given {
    /// A plain string literal, decoded.
    Text(String),
    /// An integer literal (negated when written `-n`).
    Integer(i128),
    /// A literal of another kind: the type mismatch is reported already.
    OtherLiteral,
    /// Anything that isn't a literal.
    Computed,
}

/// Classifies the argument expression `e`.
fn given(e: &ast::Expr) -> Given {
    match e {
        ast::Expr::StringLit(s) => s.plain_value().map_or(Given::Computed, Given::Text),
        ast::Expr::Literal(l) => match l.token() {
            Some(t) if t.kind() == SyntaxKind::IntLit => {
                int_value(t.text()).map_or(Given::OtherLiteral, Given::Integer)
            }
            _ => Given::OtherLiteral,
        },
        ast::Expr::PrefixExpr(p) if p.syntax().first_token().is_some_and(|t| t.text() == "-") => {
            match p.operand().as_ref().map(given) {
                Some(Given::Integer(n)) => Given::Integer(-n),
                Some(Given::OtherLiteral) => Given::OtherLiteral,
                _ => Given::Computed,
            }
        }
        _ => Given::Computed,
    }
}

/// The value of an integer literal's text (`1_000`, `0x1F`, `0b101`,
/// `0o17`), or `None` when it doesn't fit.
fn int_value(text: &str) -> Option<i128> {
    let t: String = text.chars().filter(|c| *c != '_').collect();
    let lower = t.to_ascii_lowercase();
    let (digits, radix) = match lower.get(..2) {
        Some("0x") => (&lower[2..], 16),
        Some("0b") => (&lower[2..], 2),
        Some("0o") => (&lower[2..], 8),
        _ => (lower.as_str(), 10),
    };
    i128::from_str_radix(digits, radix).ok()
}

impl Checker<'_, '_, '_> {
    /// K2403 and K2404 for a call of `d`'s `@Literal` initializer, whose
    /// type name is at `name_range`; `args` are the call's arguments. Does
    /// nothing unless the initializer that takes the first argument's label
    /// is marked `@Literal`.
    pub(super) fn check_literal_call(&mut self, d: DeclId, name_range: TextRange, args: &[Arg]) {
        let label = args
            .iter()
            .find(|a| !a.is_lambda)
            .and_then(|a| a.label.as_ref().map(|l| l.0.clone()));
        let literal = self.cx.inits(d).iter().any(|&i| {
            self.cx.p.member(i).has_annotation("Literal")
                && self
                    .cx
                    .sig(SigKey::Member(i))
                    .params
                    .first()
                    .map(|p| p.label.clone())
                    == Some(label.clone())
        });
        if !literal {
            return;
        }
        let Some(arg) = args.iter().find(|a| !a.is_lambda) else {
            return;
        };
        let Some(expr) = &arg.expr else { return };
        // The qualified name, also for prelude types (`decl_name` renders
        // those by their simple name).
        let qualified = match self.cx.p.decl_module(d) {
            Some(m) => format!(
                "{}.{}",
                self.cx.p.modules[m.0 as usize].path,
                self.cx.p.decl(d).name
            ),
            None => self.cx.p.decl(d).name.clone(),
        };
        let type_name = self.cx.p.decl(d).name.clone();
        let wanted = if qualified == "k.http.HttpStatus" {
            Wanted::Integer
        } else {
            Wanted::Text
        };
        let range = expr.syntax().text_range();
        let spelled = match &label {
            Some(l) => format!("{type_name}({l}:)"),
            None => format!("{type_name}(…)"),
        };
        match (given(expr), wanted) {
            (Given::Text(text), Wanted::Text) => self.literal_value(&qualified, &text, range),
            (Given::Integer(code), Wanted::Integer) => {
                if let Err(reason) = literals::http_status_code(code) {
                    self.err(
                        codes::INVALID_LITERAL,
                        range,
                        format!("not an HTTP status: {reason}"),
                    );
                }
            }
            (Given::OtherLiteral, _)
            | (Given::Text(_), Wanted::Integer)
            | (Given::Integer(_), Wanted::Text) => {}
            (Given::Computed, wanted) => {
                let kind = match wanted {
                    Wanted::Text => "a string literal",
                    Wanted::Integer => "an integer literal",
                };
                let mut diag = Diagnostic::new(
                    codes::LITERAL_REQUIRED,
                    self.span(range),
                    format!("`{spelled}` takes {kind}, checked when you compile"),
                );
                if label.is_none() && self.has_parse(d) {
                    diag = diag.with_fix(
                        Fix::replace(
                            format!("Parse it at run time with `{type_name}.parse`, which returns an optional"),
                            Span::empty(self.span(name_range).file, name_range.end()),
                            ".parse",
                        )
                        .maybe_incorrect(),
                    );
                }
                self.report(diag);
            }
        }
    }

    /// K2404 when the parser of the type named `qualified` refuses `text`,
    /// the literal at `range`.
    fn literal_value(&mut self, qualified: &str, text: &str, range: TextRange) {
        let refused = match qualified {
            "k.core.Url" => literals::url(text).err().map(|r| format!("not a URL: {r}")),
            "k.crypto.Uuid" => literals::uuid(text)
                .err()
                .map(|r| format!("not a UUID: {r}")),
            "k.text.EmailAddress" => literals::email_address(text)
                .err()
                .map(|r| format!("not an email address: {r}")),
            "k.time.TimeOfDay" => literals::time_of_day(text)
                .err()
                .map(|r| format!("not a time of day: {r}")),
            _ => None,
        };
        if let Some(message) = refused {
            self.err(codes::INVALID_LITERAL, range, message);
        }
    }

    /// `true` when `d` declares `static func parse(_ text: String)`, the
    /// runtime parse K2403's fix switches to.
    fn has_parse(&self, d: DeclId) -> bool {
        let p = self.cx.p;
        p.decl(d).members.iter().any(|&m| {
            let member = p.member(m);
            member.kind == MemberKind::Func
                && member.is_static
                && member.name == "parse"
                && self
                    .cx
                    .sig(SigKey::Member(m))
                    .params
                    .first()
                    .is_some_and(|param| param.label.is_none())
        })
    }

    /// K2404 for a money literal `amount.code` (`m`, whose receiver is
    /// `recv` and whose type is `ty`) with more significant decimal places
    /// than the currency has: `1.005.usd`.
    pub(super) fn check_money_literal(&mut self, m: &ast::MemberExpr, recv: &ast::Expr, ty: &Ty) {
        if self.cx.b.money.is_none() || ty.decl() != self.cx.b.money {
            return;
        }
        let ast::Expr::Literal(l) = recv else { return };
        let Some(tok) = l
            .token()
            .filter(|t| matches!(t.kind(), SyntaxKind::IntLit | SyntaxKind::FloatLit))
        else {
            return;
        };
        let Some(name) = m.name() else { return };
        let Some(digits) = literals::currency_digits(name.text()) else {
            return;
        };
        let Some(places) = literals::decimal_places(tok.text()) else {
            return;
        };
        if places <= u64::from(digits) {
            return;
        }
        let plural = |n: u64| if n == 1 { "" } else { "s" };
        let code = name.text().to_ascii_uppercase();
        let has = if digits == 0 {
            "none".to_owned()
        } else {
            digits.to_string()
        };
        let range = m.syntax().text_range();
        let text = self.cx.p.file(self.file()).text;
        let written = text
            .get(usize::from(range.start())..usize::from(range.end()))
            .unwrap_or_default()
            .to_owned();
        self.err(
            codes::INVALID_LITERAL,
            range,
            format!(
                "`{written}` has {places} decimal place{}, and {code} has {has}",
                plural(places)
            ),
        );
    }

    /// The currency code (`usd`) of `e` when the checker can see it from
    /// literals alone: a money literal `amount.code` (negated or in
    /// parentheses too), a `Money * Int` scaling of one, or a sum or
    /// difference of such amounts (its left operand's currency, since a
    /// mixed one is reported where it is written). `None` for anything
    /// whose currency only the run time knows. Only called on operands
    /// already typed `Money`, so a number extension named like a currency
    /// is one.
    fn visible_currency(e: &ast::Expr) -> Option<String> {
        match e {
            ast::Expr::ParenExpr(p) => {
                Self::visible_currency(&p.syntax().children().find_map(ast::Expr::cast)?)
            }
            ast::Expr::PrefixExpr(p) => {
                if p.op()?.kind() != SyntaxKind::Minus {
                    return None;
                }
                Self::visible_currency(&p.operand()?)
            }
            ast::Expr::MemberExpr(m) => {
                let recv = match m.receiver()? {
                    ast::Expr::PrefixExpr(p) if p.op()?.kind() == SyntaxKind::Minus => {
                        p.operand()?
                    }
                    other => other,
                };
                let ast::Expr::Literal(l) = recv else {
                    return None;
                };
                l.token()
                    .filter(|t| matches!(t.kind(), SyntaxKind::IntLit | SyntaxKind::FloatLit))?;
                let name = m.name()?;
                literals::currency_digits(name.text())?;
                Some(name.text().to_owned())
            }
            ast::Expr::BinaryExpr(b) => match b.op()?.kind() {
                SyntaxKind::Plus | SyntaxKind::Minus => {
                    Self::visible_currency(&b.lhs()?).or_else(|| Self::visible_currency(&b.rhs()?))
                }
                SyntaxKind::Star => Self::visible_currency(&b.lhs()?),
                _ => None,
            },
            _ => None,
        }
    }

    /// K2407 at `range` for `lhs op rhs` (`op` is `+` or `-`, both operands
    /// `Money`) when both currencies are visible and differ: `1.usd +
    /// 2.eur` would panic at run time (`docs/design/34-money.md`).
    pub(super) fn check_mixed_currencies(
        &mut self,
        op: char,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        range: TextRange,
    ) {
        let (Some(a), Some(b)) = (Self::visible_currency(lhs), Self::visible_currency(rhs)) else {
            return;
        };
        if a == b {
            return;
        }
        let (a, b) = (a.to_ascii_uppercase(), b.to_ascii_uppercase());
        let (verb, to) = if op == '+' {
            ("adds", "to")
        } else {
            ("subtracts", "from")
        };
        self.err(
            codes::MIXED_CURRENCIES,
            range,
            format!(
                "this {verb} {b} {to} {a}; `Money` never mixes currencies, so it would panic at run time"
            ),
        );
    }
}
