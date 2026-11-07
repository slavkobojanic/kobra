//! Native stdlib bodies: how calls to `@Native` stdlib declarations lower
//! (`docs/compiler/codegen.md` §6.6, `docs/compiler/runtime.md` §12).
//!
//! Members whose JavaScript equivalent has the same semantics are inlined
//! (`filter`, `map`, `count`); the rest call `std.js` helpers through
//! `$k`. Each function returns `None` for members it doesn't know, and the
//! caller falls back to the generic member lowering.

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, MemberId, MemberKind, Resolved, Type};
use k_syntax::SyntaxKind;
use k_syntax::ast;

use crate::js::{self, Expr, Param, Stmt, bin, call, id, member, rt, un};
use crate::lower::{Body, MemberAccess, fmt_num};

fn owner_name(b: &Body<'_, '_>, m: MemberId) -> (String, String) {
    let info = b.g.view.member(m);
    let mut owner = info.owner;
    let d = b.g.view.decl(owner);
    if let Some(t) = d.extends.as_ref().and_then(Type::decl) {
        owner = t;
    }
    (b.g.view.qualified_name(owner), info.name.clone())
}

/// A stdlib top-level function used as a callee.
pub(crate) fn function(b: &mut Body<'_, '_>, d: DeclId) -> Option<Expr> {
    let name = b.g.view.qualified_name(d);
    Some(match name.as_str() {
        "k.core.untracked" => member(id("$k"), "untrack"),
        "k.core.onCleanup" => member(id("$k"), "onCleanup"),
        "k.core.global" => member(id("$k"), "globalClass"),
        _ => return None,
    })
}

/// Calls to names that resolve to nothing (none in a checked program
/// except pseudo-functions).
pub(crate) fn builtin_call(_b: &mut Body<'_, '_>, _name: &str, _c: &ast::CallExpr) -> Option<Expr> {
    None
}

/// A static stdlib value (`Color.white`).
pub(crate) fn static_value(b: &mut Body<'_, '_>, m: MemberId) -> Option<Expr> {
    let (owner, name) = owner_name(b, m);
    Some(match (owner.as_str(), name.as_str()) {
        ("k.core.Color", "white") => js::str("#FFFFFF"),
        ("k.core.Color", "black") => js::str("#000000"),
        ("k.core.Color", "transparent") => js::str("transparent"),
        _ => return None,
    })
}

/// A theme token as CSS text: `var(--k-color-primary)`.
pub(crate) fn token_value(b: &mut Body<'_, '_>, t: &Type, name: &str) -> Expr {
    let category = match t.unwrap_optional().decl() {
        Some(d) => match b.g.view.decl(d).name.as_str() {
            "Color" => "color",
            "Spacing" => "space",
            "Radius" => "radius",
            "FontSize" => "font-size",
            "Font" => "font",
            "Shadow" => "shadow",
            "Duration" => "duration",
            _ => "token",
        },
        None => "token",
    };
    js::str(format!("var(--k-{category}-{})", crate::css::kebab(name)))
}

const DURATIONS: &[(&str, f64)] = &[
    ("milliseconds", 1.0),
    ("millisecond", 1.0),
    ("seconds", 1000.0),
    ("second", 1000.0),
    ("minutes", 60_000.0),
    ("minute", 60_000.0),
    ("hours", 3_600_000.0),
    ("hour", 3_600_000.0),
    ("days", 86_400_000.0),
    ("day", 86_400_000.0),
    ("weeks", 604_800_000.0),
    ("week", 604_800_000.0),
];

const CURRENCIES: &[&str] = &["usd", "eur", "gbp", "jpy", "chf"];

/// Folds a number extension on a literal: `10.minutes` → `600000`,
/// `0.50.usd` → a Money constant.
pub(crate) fn fold_number_member(
    b: &mut Body<'_, '_>,
    recv: &ast::Expr,
    name: &str,
    base: &Type,
) -> Option<Expr> {
    let ast::Expr::Literal(l) = recv else {
        return None;
    };
    let tok = l.token()?;
    let text: String = tok.text().chars().filter(|c| *c != '_').collect();
    if !matches!(tok.kind(), SyntaxKind::IntLit | SyntaxKind::FloatLit) {
        return None;
    }
    let n: f64 = text.parse().ok()?;
    if let Some((_, f)) = DURATIONS.iter().find(|(d, _)| *d == name) {
        return Some(Expr::Num((n * f).round()));
    }
    match name {
        "bytes" => return Some(Expr::Num(n)),
        "kilobytes" => return Some(Expr::Num(n * 1000.0)),
        "megabytes" => return Some(Expr::Num(n * 1_000_000.0)),
        _ => {}
    }
    if CURRENCIES.contains(&name) && (b.g.is_int(base) || b.g.is_double(base)) {
        return Some(rt(
            "money",
            vec![
                js::str(text),
                member(member(id("$k"), "Currency"), name.to_owned()),
            ],
        ));
    }
    None
}

/// A native property read.
#[allow(clippy::too_many_lines)]
pub(crate) fn property(
    b: &mut Body<'_, '_>,
    recv: Expr,
    base: &Type,
    m: MemberId,
    _result: &Type,
) -> Option<Expr> {
    let (owner, name) = owner_name(b, m);
    let len = |e: Expr| member(e, "length");
    Some(match (owner.as_str(), name.as_str()) {
        ("k.core.List", "count") => len(recv),
        ("k.core.List", "isEmpty") => bin("===", len(recv), Expr::Num(0.0)),
        ("k.core.List", "isNotEmpty") => bin("!==", len(recv), Expr::Num(0.0)),
        ("k.core.List", "first" | "last") => {
            let nested = matches!(base.args().first(), Some(Type::Optional(_)));
            let f = match (name.as_str(), nested) {
                ("first", false) => "first",
                ("first", true) => "firstNested",
                ("last", false) => "last",
                _ => "lastNested",
            };
            rt(f, vec![recv])
        }
        ("k.core.String", "count") => rt("graphemeCount", vec![recv]),
        ("k.core.String", "isEmpty") => bin("===", len(recv), Expr::Num(0.0)),
        ("k.core.String", "isNotEmpty") => bin("!==", len(recv), Expr::Num(0.0)),
        ("k.core.String", "utf8") => rt("utf8", vec![recv]),
        ("k.core.Map", "count") => member(recv, "size"),
        ("k.core.Map", "isEmpty") => bin("===", member(recv, "size"), Expr::Num(0.0)),
        ("k.core.Map", "keys") => Expr::Array(vec![Expr::Spread(Box::new(call(
            member(recv, "keys"),
            vec![],
        )))]),
        ("k.core.Map", "values") => Expr::Array(vec![Expr::Spread(Box::new(call(
            member(recv, "values"),
            vec![],
        )))]),
        ("k.core.Set", "count") => member(recv, "size"),
        ("k.core.Set", "isEmpty") => bin("===", member(recv, "size"), Expr::Num(0.0)),
        ("k.core.Int" | "k.core.Double", n) if DURATIONS.iter().any(|(d, _)| *d == n) => {
            let f = DURATIONS
                .iter()
                .find(|(d, _)| *d == n)
                .map_or(1.0, |(_, f)| *f);
            if b.g.is_int(base) {
                rt("checkInt", vec![bin("*", recv, Expr::Num(f))])
            } else {
                call(
                    member(id("Math"), "round"),
                    vec![bin("*", recv, Expr::Num(f))],
                )
            }
        }
        ("k.core.Int", "bytes") => recv,
        ("k.core.Int", "kilobytes") => rt("checkInt", vec![bin("*", recv, Expr::Num(1000.0))]),
        ("k.core.Int", "megabytes") => rt("checkInt", vec![bin("*", recv, Expr::Num(1_000_000.0))]),
        ("k.core.Int" | "k.core.Double", c) if CURRENCIES.contains(&c) => rt(
            "money",
            vec![
                call(id("String"), vec![recv]),
                member(member(id("$k"), "Currency"), c.to_owned()),
            ],
        ),
        ("k.core.Duration", "inSeconds") => call(
            member(id("Math"), "floor"),
            vec![bin("/", recv, Expr::Num(1000.0))],
        ),
        ("k.core.Duration", "inMilliseconds") => call(member(id("Math"), "trunc"), vec![recv]),
        // An `EmailAddress` is its text at run time (k_runtime natives).
        ("k.text.EmailAddress", "address") => recv,
        ("k.text.EmailAddress", "displayName") => Expr::Null,
        ("k.core.Money", "minorUnits") => call(member(recv, "minorUnits"), vec![]),
        ("k.core.Money", "currency") => member(recv, "currency"),
        ("k.core.Currency", "code") => rt("currencyCode", vec![recv]),
        ("k.core.Currency", "minorUnitDigits") => rt("currencyDigits", vec![recv]),
        ("k.core.Date", "unixSeconds") => call(member(recv, "unixSeconds"), vec![]),
        ("k.core.HalfOpenRange" | "k.core.ClosedRange", "lowerBound") => member(recv, "lo"),
        ("k.core.HalfOpenRange" | "k.core.ClosedRange", "upperBound") => member(recv, "hi"),
        ("k.core.Bytes", "count") => len(recv),
        ("k.core.Bytes", "hex") => rt("bytesHex", vec![recv]),
        ("k.core.Bytes", "base64") => rt("bytesBase64", vec![recv]),
        ("k.core.Bytes", "utf8Text") => rt("bytesUtf8", vec![recv]),
        ("k.core.Angle", "degrees") => recv,
        ("k.core.Url", "path") => rt("urlPath", vec![recv]),
        ("k.core.Page", "url") => match b.g.view.lookup("k.core.Page") {
            Some(p) => {
                let f = b.use_decl(p, "Page$href".into());
                call(f, vec![recv])
            }
            None => recv,
        },
        ("k.core.Url", "host") => rt("urlHost", vec![recv]),
        ("k.http.HttpStatus", "code") => rt("httpCode", vec![recv]),
        ("k.http.HttpStatus", "isSuccess") => {
            let c = rt("httpCode", vec![recv]);
            bin(
                "&&",
                bin(">=", c.clone(), Expr::Num(200.0)),
                bin("<", c, Expr::Num(300.0)),
            )
        }
        ("k.http.HttpStatus", "isClientError") => {
            let c = rt("httpCode", vec![recv]);
            bin(
                "&&",
                bin(">=", c.clone(), Expr::Num(400.0)),
                bin("<", c, Expr::Num(500.0)),
            )
        }
        ("k.http.HttpStatus", "isServerError") => {
            bin(">=", rt("httpCode", vec![recv]), Expr::Num(500.0))
        }
        _ => return None,
    })
}

/// Wraps a function argument so JS callbacks receive only the element
/// (`map` passes the index and array too).
fn unary_fn(f: Expr) -> Expr {
    match f {
        Expr::Arrow(_) => f,
        other => js::arrow(vec![Param::new("x")], call(other, vec![id("x")])),
    }
}

fn args_of(b: &mut Body<'_, '_>, c: &ast::CallExpr, m: MemberId, base: &Type) -> Vec<Expr> {
    let sig = b.g.view.member(m).signature.clone();
    let owner_generics = b.g.view.decl(b.g.view.member(m).owner).generics.clone();
    let args = base.args().to_vec();
    b.call_args(c, sig.as_ref(), &move |t| {
        crate::lower::subst(t, &|g| {
            owner_generics
                .iter()
                .position(|x| *x == g)
                .and_then(|i| args.get(i).cloned())
        })
    })
}

/// A native method call.
#[allow(clippy::too_many_lines)]
pub(crate) fn method(
    b: &mut Body<'_, '_>,
    recv: Expr,
    base: &Type,
    m: MemberId,
    c: &ast::CallExpr,
    result: &Type,
) -> Option<Expr> {
    let (owner, name) = owner_name(b, m);
    let labels = b.g.view.labels(m);
    let elem = base.args().first().cloned().unwrap_or(Type::Error);
    let receiver_ast = match c.callee() {
        Some(ast::Expr::MemberExpr(me)) => me.receiver(),
        _ => None,
    };
    // Mutating collection methods go through copy-on-write.
    let mutating = matches!(
        (owner.as_str(), name.as_str()),
        (
            "k.core.List",
            "append" | "insert" | "removeWhere" | "remove" | "removeAll"
        ) | ("k.core.Map", "set" | "remove")
            | ("k.core.Set", "insert" | "remove")
    );
    let a = args_of(b, c, m, base);
    if mutating {
        let place = receiver_ast?;
        let is_list = owner == "k.core.List";
        let is_set = owner == "k.core.Set";
        let op = name.clone();
        let args = a.clone();
        return Some(mutate(b, &place, recv, is_list, move |t| {
            let a0 = args.first().cloned().unwrap_or(Expr::Null);
            match op.as_str() {
                "append" => call(member(t, "push"), args.clone()),
                "insert" if is_set => call(member(t, "add"), vec![a0]),
                "insert" => rt(
                    "insertAt",
                    vec![t, a0, args.get(1).cloned().unwrap_or(Expr::Num(0.0))],
                ),
                "removeWhere" => rt("removeWhere", vec![t, a0]),
                "removeAll" => js::assign(member(t, "length"), Expr::Num(0.0)),
                "set" => call(member(t, "set"), args.clone()),
                "remove" if is_list => rt("removeAt", vec![t, a0]),
                "remove" => call(member(t, "delete"), vec![a0]),
                _ => Expr::Null,
            }
        }));
    }
    let f0 = || a.first().cloned().unwrap_or(Expr::Null);
    Some(match (owner.as_str(), name.as_str()) {
        ("k.core.List", "filter") => call(member(recv, "filter"), vec![unary_fn(f0())]),
        ("k.core.List", "map") => call(member(recv, "map"), vec![unary_fn(f0())]),
        ("k.core.List", "compactMap") => rt("compactMap", vec![recv, f0()]),
        ("k.core.List", "flatMap") => call(member(recv, "flatMap"), vec![unary_fn(f0())]),
        ("k.core.List", "sortedBy") => sort_copy(recv, vec![(f0(), false)]),
        ("k.core.List", "sortedByDescending") => sort_copy(recv, vec![(f0(), true)]),
        ("k.core.List", "thenBy") => sort_copy(recv, vec![(f0(), false)]),
        ("k.core.List", "thenByDescending") => sort_copy(recv, vec![(f0(), true)]),
        ("k.core.List", "sorted") => call(
            member(Expr::Array(vec![Expr::Spread(Box::new(recv))]), "sort"),
            vec![member(id("$k"), "compare")],
        ),
        ("k.core.List", "distinct") => rt(
            "distinct",
            vec![recv, Expr::Bool(b.g.is_primitive_key(&elem))],
        ),
        ("k.core.List", "indexWhere") => rt("indexWhere", vec![recv, f0()]),
        ("k.core.List", "firstWhere") => {
            let nested = matches!(elem, Type::Optional(_));
            rt(
                if nested {
                    "firstWhereNested"
                } else {
                    "firstWhere"
                },
                vec![recv, f0()],
            )
        }
        ("k.core.List", "any") => call(member(recv, "some"), vec![unary_fn(f0())]),
        ("k.core.List", "all") => call(member(recv, "every"), vec![unary_fn(f0())]),
        ("k.core.List", "count") => rt("countWhere", vec![recv, f0()]),
        ("k.core.List", "groupedBy") => {
            let k = match result {
                Type::Named(_, args) => args.first().cloned().unwrap_or(Type::Error),
                _ => Type::Error,
            };
            rt(
                "groupedBy",
                vec![recv, f0(), Expr::Bool(b.g.is_primitive_key(&k))],
            )
        }
        ("k.core.List", "partitioned") => rt("partitioned", vec![recv, f0()]),
        ("k.core.List", "chunked") => rt("chunked", vec![recv, f0()]),
        ("k.core.List", "windowed") => rt("windowed", vec![recv, f0()]),
        ("k.core.List", "prefix") => rt("prefix", vec![recv, f0()]),
        ("k.core.List", "minBy") => rt(
            if matches!(elem, Type::Optional(_)) {
                "minByNested"
            } else {
                "minBy"
            },
            vec![recv, f0()],
        ),
        ("k.core.List", "maxBy") => rt(
            if matches!(elem, Type::Optional(_)) {
                "maxByNested"
            } else {
                "maxBy"
            },
            vec![recv, f0()],
        ),
        ("k.core.List", "sum") if labels.is_empty() => {
            let int = b.g.is_int(&elem);
            let add = bin("+", id("a"), id("b"));
            let add = if int && b.g.checked() {
                rt("checkInt", vec![add])
            } else {
                add
            };
            call(
                member(recv, "reduce"),
                vec![
                    js::arrow(vec![Param::new("a"), Param::new("b")], add),
                    Expr::Num(0.0),
                ],
            )
        }
        ("k.core.List", "sum") => rt(
            "sumMoney",
            vec![recv, f0(), a.get(1).cloned().unwrap_or(Expr::Null)],
        ),
        ("k.core.List", "joined") => {
            if b.g.is_string(&elem) {
                call(member(recv, "join"), vec![f0()])
            } else {
                let f = b.text_fn(&elem);
                call(
                    member(call(member(recv, "map"), vec![unary_fn(f)]), "join"),
                    vec![f0()],
                )
            }
        }
        ("k.core.List", "contains") if labels.first().is_some_and(Option::is_some) => {
            call(member(recv, "some"), vec![unary_fn(f0())])
        }
        ("k.core.List", "contains") => {
            if b.g.is_identity_eq(&elem) {
                call(member(recv, "includes"), vec![f0()])
            } else {
                rt("contains", vec![recv, f0()])
            }
        }
        ("k.core.List", "appending") => Expr::Array(vec![Expr::Spread(Box::new(recv)), f0()]),
        ("k.core.List", "enumerated") => rt("enumerated", vec![recv]),
        ("k.core.String", "trimmed") => call(member(recv, "trim"), vec![]),
        ("k.core.String", "lowercased") => call(member(recv, "toLowerCase"), vec![]),
        ("k.core.String", "uppercased") => call(member(recv, "toUpperCase"), vec![]),
        ("k.core.String", "contains") => call(member(recv, "includes"), vec![f0()]),
        ("k.core.String", "startsWith") => call(member(recv, "startsWith"), vec![f0()]),
        ("k.core.String", "endsWith") => call(member(recv, "endsWith"), vec![f0()]),
        ("k.core.String", "split") => {
            let mut args = vec![recv];
            args.extend(a);
            rt("split", args)
        }
        ("k.core.String", "replace") => call(member(recv, "replaceAll"), a),
        ("k.core.String", "prefix") => rt("strPrefix", vec![recv, f0()]),
        ("k.core.String", "repeated") => rt("repeat", vec![recv, f0()]),
        ("k.core.String", "truncated") => {
            let mut args = vec![recv];
            args.extend(a);
            rt("truncated", args)
        }
        ("k.core.Map", "get") => {
            let nested = matches!(base.args().get(1), Some(Type::Optional(_)));
            rt(
                if nested { "mapGetNested" } else { "mapGet" },
                vec![recv, f0()],
            )
        }
        ("k.core.Map", "containsKey") => call(member(recv, "has"), vec![f0()]),
        ("k.core.Set", "contains") => call(member(recv, "has"), vec![f0()]),
        ("k.core.Double", "rounded") => rt("round", vec![recv]),
        ("k.core.Int" | "k.core.Double", "formatted") => rt("formatNumber", vec![recv, f0()]),
        ("k.core.Money", "multiplied") => call(member(recv, "multiplied"), a),
        ("k.core.Money", "allocated") => call(member(recv, "allocated"), a),
        ("k.core.Money", "formatted") => call(member(recv, "formatted"), a),
        ("k.core.Date", "formatted") => call(member(recv, "formatted"), a),
        ("k.core.HalfOpenRange" | "k.core.ClosedRange", "contains") => {
            call(member(recv, "contains"), vec![f0()])
        }
        ("k.core.HalfOpenRange" | "k.core.ClosedRange", "map") => call(
            member(rt("rangeValues", vec![recv]), "map"),
            vec![unary_fn(f0())],
        ),
        ("k.http.HttpResponse", "body") if b.target == crate::cx::Target::Server => {
            call(member(id("$s"), "responseBody"), vec![recv, f0()])
        }
        ("k.http.HttpResponse", "headers") if b.target == crate::cx::Target::Server => {
            call(member(id("$s"), "responseHeaders"), vec![recv, f0()])
        }
        ("k.http.HttpHeaders", "get") if b.target == crate::cx::Target::Server => {
            call(member(id("$s"), "header"), vec![recv, f0()])
        }
        ("k.http.HttpHeaders", "all") if b.target == crate::cx::Target::Server => {
            call(member(id("$s"), "headerAll"), vec![recv, f0()])
        }
        ("k.http.IncomingRequest", "text") => call(member(id("$s"), "requestText"), vec![recv]),
        ("k.http.IncomingRequest", "json") => {
            let codec = crate::codec::codec_expr(b, result);
            let err = decoding_error(b);
            call(member(id("$s"), "requestJson"), vec![recv, codec, err])
        }
        ("k.core.Secret", "reveal") => call(member(recv, "reveal"), vec![]),
        ("k.core.Secret", "matches") => call(member(recv, "matches"), vec![f0()]),
        ("k.core.Event", "emit") => Expr::OptCall(Box::new(recv), a),
        ("k.core.Stream", "filter") => rt("streamFilter", vec![recv, f0()]),
        ("k.core.Stream", "map") => rt("streamMap", vec![recv, f0()]),
        ("k.core.Stream", "startingWith") => rt("streamStartingWith", vec![recv, f0()]),
        ("k.core.Stream", "next") => call(member(recv, "next"), vec![]),
        ("k.core.Url", "appending") => rt("urlAppending", vec![recv, f0()]),
        ("k.core.Color", "opacity") => rt("colorOpacity", vec![recv, f0()]),
        ("k.core.Color", "mix") => rt(
            "colorMix",
            vec![recv, f0(), a.get(1).cloned().unwrap_or(Expr::Num(50.0))],
        ),
        _ => {
            let _ = un;
            return None;
        }
    })
}

/// `[...xs].sort($k.byKeys(...))`.
fn sort_copy(recv: Expr, keys: Vec<(Expr, bool)>) -> Expr {
    let comparator = if keys.len() == 1 {
        let (k, desc) = keys.into_iter().next().unwrap_or((Expr::Null, false));
        rt(if desc { "byKeyDesc" } else { "byKey" }, vec![k])
    } else {
        rt(
            "byKeys",
            keys.into_iter()
                .map(|(k, d)| Expr::Array(vec![k, Expr::Bool(d)]))
                .collect(),
        )
    };
    call(
        member(Expr::Array(vec![Expr::Spread(Box::new(recv))]), "sort"),
        vec![comparator],
    )
}

/// Lowers `xs.sortedBy(a).thenBy(b)…` to one sort with combined keys.
pub(crate) fn sort_chain(b: &mut Body<'_, '_>, c: &ast::CallExpr) -> Option<Expr> {
    let mut keys: Vec<(ast::Expr, bool)> = Vec::new();
    let mut cur = c.clone();
    loop {
        let ast::Expr::MemberExpr(me) = cur.callee()? else {
            return None;
        };
        let name = me.name()?;
        let Some(Resolved::Member(m)) = b.g.resolved(b.file, name.text_range()) else {
            return None;
        };
        let (owner, n) = owner_name(b, m);
        if owner != "k.core.List" {
            return None;
        }
        let key = cur
            .trailing_lambda()
            .map(ast::Expr::LambdaExpr)
            .or_else(|| {
                cur.arg_list()
                    .and_then(|l| l.args().next())
                    .and_then(|a| a.value())
            })?;
        match n.as_str() {
            "thenBy" | "thenByDescending" => {
                keys.push((key, n == "thenByDescending"));
                let recv = me.receiver()?;
                let ast::Expr::CallExpr(inner) = recv else {
                    return None;
                };
                cur = inner;
            }
            "sortedBy" | "sortedByDescending" => {
                if keys.is_empty() {
                    return None;
                }
                keys.push((key, n == "sortedByDescending"));
                let recv = me.receiver()?;
                let r = b.expr(&recv);
                keys.reverse();
                let lowered: Vec<(Expr, bool)> =
                    keys.into_iter().map(|(k, d)| (b.expr(&k), d)).collect();
                return Some(sort_copy(r, lowered));
            }
            _ => return None,
        }
    }
}

/// Applies a mutating collection method to `place` with copy-on-write
/// (codegen.md §6.3): an owned local mutates in place, an unowned local is
/// copied once first, any other place is copied, mutated and stored back.
fn mutate(
    b: &mut Body<'_, '_>,
    place: &ast::Expr,
    current: Expr,
    is_list: bool,
    op: impl Fn(Expr) -> Expr,
) -> Expr {
    if let ast::Expr::NameExpr(n) = place {
        if let Some(tok) = n.name() {
            if let Some(k_sema::Resolution::Local { name, definition }) =
                b.g.resolution(b.file, tok.text_range())
            {
                let js = b.local_js(&name, definition.range);
                if !b.is_owned(&js) {
                    let copy = if is_list {
                        call(member(id(&js), "slice"), vec![])
                    } else {
                        rt("copy", vec![id(&js)])
                    };
                    b.stmts.push(Stmt::Expr(js::assign(id(&js), copy)));
                    b.set_owned(&js);
                }
                return op(id(&js));
            }
            if let Some(Resolved::Member(mid)) = b.g.resolved(b.file, tok.text_range()) {
                if let Some(MemberAccess::Signal(s)) = b.access.get(&mid).cloned() {
                    if b.ssr {
                        return Expr::Undefined;
                    }
                    let t = b.fresh("c");
                    b.stmts
                        .push(Stmt::Const(t.clone(), rt("copy", vec![current])));
                    let r = b.fresh("r");
                    b.stmts.push(Stmt::Const(r.clone(), op(id(&t))));
                    b.stmts
                        .push(Stmt::Expr(call(member(id(s), "set"), vec![id(&t)])));
                    return id(r);
                }
            }
        }
    }
    let t = b.fresh("c");
    b.stmts
        .push(Stmt::Const(t.clone(), rt("copy", vec![current])));
    let r = b.fresh("r");
    b.stmts.push(Stmt::Const(r.clone(), op(id(&t))));
    b.store(place, id(&t));
    id(r)
}

/// Constructs a stdlib type: `Set(list)`, `Money(minorUnits:currency:)`,
/// `Date(unixSeconds:)`, `Binding(get:set:)`, `Url("…")`, `Secret(v)`.
pub(crate) fn construct(
    b: &mut Body<'_, '_>,
    d: DeclId,
    c: &ast::CallExpr,
    result: &Type,
) -> Option<Expr> {
    let name = b.g.view.qualified_name(d);
    let init =
        b.g.view
            .decl(d)
            .members
            .iter()
            .copied()
            .find(|&m| b.g.view.member(m).kind == MemberKind::Init);
    let sig = init.and_then(|m| b.g.view.member(m).signature.clone());
    let args = b.call_args(c, sig.as_ref(), &|t| t.clone());
    let a0 = args.first().cloned().unwrap_or(Expr::Null);
    Some(match name.as_str() {
        "k.core.Set" => {
            let k = result.args().first().cloned().unwrap_or(Type::Error);
            b.new_set(&k, a0)
        }
        "k.core.Map" if args.is_empty() => {
            let k = result.args().first().cloned().unwrap_or(Type::Error);
            b.new_map(&k, Vec::new())
        }
        "k.core.Money" => rt("moneyFromMinor", args),
        "k.core.Date" => rt("instantFromSeconds", vec![a0]),
        "k.core.Binding" => Expr::Object(
            vec![
                js::Prop::KeyValue("get".into(), a0),
                js::Prop::KeyValue("set".into(), args.get(1).cloned().unwrap_or(Expr::Null)),
            ],
            false,
        ),
        "k.core.Url" => a0,
        "k.core.Angle" => a0,
        // On the server a broadcast spans every worker isolate.
        "k.rpc.Broadcast"
            if b.target == crate::cx::Target::Server
                && b.owner
                    .is_some_and(|o| crate::server::is_server_scoped(b.g, o)) =>
        {
            let owner = b
                .owner
                .map(|o| b.g.view.qualified_name(o))
                .unwrap_or_default();
            let at = u32::from(ast::AstNode::syntax(c).text_range().start());
            Expr::New(
                Box::new(member(id("$s"), "ServerBroadcast")),
                vec![js::str(format!("{owner}@{at}"))],
            )
        }
        "k.rpc.Broadcast" => Expr::New(Box::new(member(id("$k"), "Broadcast")), vec![]),
        "k.http.HttpResponse" if b.target == crate::cx::Target::Server => {
            call(member(id("$s"), "response"), vec![a0])
        }
        "k.core.Secret" => Expr::New(Box::new(member(id("$k"), "Secret")), vec![a0]),
        _ => return None,
    })
}

/// A static stdlib call: `Int.parse`, `Json.encode`, `Log.info`,
/// `Result.of`, `Money.zero`, `Date.parse`, `Currency.parse`.
#[allow(clippy::too_many_lines)]
pub(crate) fn static_call(
    b: &mut Body<'_, '_>,
    m: MemberId,
    c: &ast::CallExpr,
    result: &Type,
) -> Option<Expr> {
    let (owner, name) = owner_name(b, m);
    let info = b.g.view.member(m).clone();
    if info.kind == MemberKind::Case {
        return None;
    }
    let sig = info.signature.clone();
    let subst = sig
        .as_ref()
        .map(|s| b.infer_subst(s, c, result))
        .unwrap_or_default();
    let args = b.call_args(c, sig.as_ref(), &|t| t.clone());
    let a0 = args.first().cloned().unwrap_or(Expr::Null);
    let generic = sig.as_ref().and_then(|s| s.generics.first().copied());
    Some(match (owner.as_str(), name.as_str()) {
        ("k.core.Int", "parse") => rt("intParse", vec![a0]),
        ("k.core.Double", "parse") => rt("doubleParse", vec![a0]),
        ("k.core.Decimal", "parse") => rt("parseDecimal", vec![a0]),
        ("k.core.Date", "parse") => rt("parseInstant", vec![a0]),
        ("k.core.Url", "parse") => rt("parseUrl", vec![a0]),
        ("k.crypto.Uuid", "parse") => rt("parseUuid", vec![a0]),
        ("k.text.EmailAddress", "parse") => rt("parseEmail", vec![a0]),
        ("k.core.Money", "zero") => rt("moneyZero", vec![a0]),
        ("k.core.Currency", "parse") => rt("currencyParse", vec![a0]),
        ("k.core.Color", "hex") => rt("colorHex", vec![a0]),
        ("k.core.Color", "rgb") => rt("colorRgb", args),
        ("k.observe.Log", level) => rt("log", vec![js::str(level), a0]),
        ("k.encoding.Json", "encode") => {
            let t = generic
                .and_then(|g| subst.get(&g).cloned())
                .unwrap_or(Type::Error);
            let w = crate::codec::codec_expr(b, &t);
            call(member(member(id("$k"), "Json"), "encode"), vec![w, a0])
        }
        ("k.encoding.Json", "decode") => {
            let t = generic
                .and_then(|g| subst.get(&g).cloned())
                .unwrap_or_else(|| result.clone());
            let w = crate::codec::codec_expr(b, &t);
            let err = decoding_error(b);
            call(member(member(id("$k"), "Json"), "decode"), vec![w, a0, err])
        }
        ("k.ui.Router", "navigate") => {
            // `navigate(url:)` takes the URL as is; `navigate(to:)` a page.
            let labels = b.g.view.labels(m);
            let to_url = labels.first().is_some_and(|l| l.as_deref() == Some("url"));
            let href = match b.g.view.lookup("k.core.Page") {
                _ if to_url => call(id("String"), vec![a0]),
                Some(p) => {
                    let f = b.use_decl(p, "Page$href".into());
                    call(f, vec![a0])
                }
                None => a0,
            };
            rt("navigate", vec![href])
        }
        ("k.http.HttpResponse", "redirect") if b.target == crate::cx::Target::Server => {
            let labels = b.g.view.labels(m);
            let to_page = labels.first().is_some_and(|l| l.as_deref() == Some("to"));
            let location = if to_page {
                match b.g.view.lookup("k.core.Page") {
                    Some(p) => {
                        let f = b.use_decl(p, "Page$href".into());
                        call(f, vec![a0])
                    }
                    None => a0,
                }
            } else {
                a0
            };
            call(member(id("$s"), "redirect"), vec![location])
        }
        ("k.core.Result", "of") => {
            let r = b.g.b.result.map(|r| b.decl_ref(r)).unwrap_or(Expr::Null);
            rt("resultOf", vec![a0, r])
        }
        _ => return None,
    })
}

/// The `DecodingError` constructor object (generated from the stdlib enum).
pub(crate) fn decoding_error(b: &mut Body<'_, '_>) -> Expr {
    match b.g.b.decoding_error {
        Some(d) => b.decl_ref(d),
        None => Expr::Null,
    }
}

/// `true` for stdlib declarations whose code the generator emits like
/// project code (non-native enums).
pub(crate) fn emits_stdlib_decl(g: &crate::cx::Gen<'_>, d: DeclId) -> bool {
    let info = g.view.decl(d);
    crate::app::is_generated_enum(g, d)
        || (info.stdlib
            && !info.has_annotation("Native")
            && matches!(
                info.named_kind(),
                Some(DeclarationKind::Enum | DeclarationKind::Struct)
            ))
}

#[allow(dead_code)]
fn fmt(n: f64) -> String {
    fmt_num(n)
}
