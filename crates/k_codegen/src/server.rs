//! The server entry (`k:/app/$server.js`): the app **spec** that
//! `k:runtime/app` serves, and the dependency-injection containers
//! (`docs/compiler/codegen.md` §13–§15, `docs/compiler/server.md` §5–§10).
//!
//! ```js
//! class $Process { get ProcessHits() { … } get AppConfig() { … } }   // `.server` services, configs
//! class $Request { constructor(st) { this.$r = st; } get Visits() { … } … }
//! const $spec = { pages, redirects, rpcs, endpoints, container, appMeta, … };
//! await $s.serve($s.createApp($spec));
//! ```
//!
//! Everything request-specific is created per request by `$Request`, so
//! no state crosses requests; `.server` services and configs live once per
//! isolate in `$Process`, with their `@State` backed by the process-wide
//! store (`$s.shareState`). The spec's tables are documented on
//! `createApp` in `crates/k_runtime/js/runtime/app.js`.

use std::collections::BTreeSet;

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, MemberId, MemberKind, Type};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Gen, Target};
use crate::js::{self, Class, Expr, Function, MethodKind, Param, Prop, Stmt, call, id, member, rt};
use crate::lower::Body;

/// The placeholder for the asset table, replaced once the client files
/// (and their hashed names) exist.
pub(crate) const ASSETS_PLACEHOLDER: &str = "$ASSETS$";

/// `$s.name`.
fn srt(name: &str, args: Vec<Expr>) -> Expr {
    call(member(id("$s"), name.to_owned()), args)
}

/// `this.name`.
pub(crate) fn this(name: impl Into<String>) -> Expr {
    member(Expr::This, name)
}

/// Runs `f` with `b.file` set to the file of `d`, so resolutions of nodes
/// in that file are found.
fn in_file<R>(b: &mut Body<'_, '_>, d: DeclId, f: impl FnOnce(&mut Body<'_, '_>) -> R) -> R {
    let saved = b.file;
    b.file = b.g.view.decl(d).file;
    let r = f(b);
    b.file = saved;
    r
}

// --------------------------------------------------------------- metadata

/// The HTTP code of an `HttpStatus` case name.
pub(crate) fn status_code(case: &str) -> Option<u16> {
    Some(match case {
        "ok" => 200,
        "created" => 201,
        "accepted" => 202,
        "noContent" => 204,
        "movedPermanently" => 301,
        "seeOther" => 303,
        "notModified" => 304,
        "badRequest" => 400,
        "unauthorized" => 401,
        "forbidden" => 403,
        "notFound" => 404,
        "methodNotAllowed" => 405,
        "conflict" => 409,
        "gone" => 410,
        "payloadTooLarge" => 413,
        "unsupportedMediaType" => 415,
        "unprocessableEntity" => 422,
        "tooManyRequests" => 429,
        "internalServerError" => 500,
        "badGateway" => 502,
        "serviceUnavailable" => 503,
        "gatewayTimeout" => 504,
        _ => return None,
    })
}

/// The BCP 47 tag of a `Locale` case.
fn locale_tag(case: &str) -> &'static str {
    match case {
        "german" => "de",
        "french" => "fr",
        "portuguese" => "pt",
        "japanese" => "ja",
        _ => "en",
    }
}

/// The case name of an implicit member argument (`.notFound`).
fn implicit_name(e: &ast::Expr) -> Option<String> {
    match e {
        ast::Expr::ImplicitMemberExpr(m) => m.name().map(|t| t.text().to_owned()),
        _ => None,
    }
}

/// Lowers the entry calls of a `metadata { }` block (22-metadata.md) to
/// calls on `target`, a `$s.Metadata`: `title(text)`, `canonical(href)`
/// (the page's path, made absolute by the runtime), `status(code)`,
/// `locale(tag)`, `manifest(name, shortName)`, `structuredData(value)`, …
/// Unknown entries are skipped (the checker allows only these).
pub(crate) fn meta_calls(b: &mut Body<'_, '_>, calls: &[SyntaxNode], target: &Expr) -> Vec<Stmt> {
    let mut out = Vec::new();
    for node in calls {
        let Some(c) = ast::CallExpr::cast(node.clone()) else {
            continue;
        };
        let Some(ast::Expr::NameExpr(n)) = c.callee() else {
            continue;
        };
        let Some(name) = n.name().map(|t| t.text().to_owned()) else {
            continue;
        };
        let args: Vec<(Option<String>, ast::Expr)> = c
            .arg_list()
            .map(|l| {
                l.args()
                    .filter_map(|a| Some((a.label().map(|t| t.text().to_owned()), a.value()?)))
                    .collect()
            })
            .unwrap_or_default();
        let first = args.first().map(|(_, v)| v.clone());
        let by_label = |l: &str| {
            args.iter()
                .find(|(n, _)| n.as_deref() == Some(l))
                .map(|(_, v)| v.clone())
        };
        let (pre, values) = b.capture(|b| -> Option<Vec<Expr>> {
            Some(match name.as_str() {
                "themeColor" => {
                    // `<meta name="theme-color">` needs a color, not a CSS
                    // variable: a theme token becomes its literal value.
                    let v = first?;
                    let literal = match &v {
                        ast::Expr::ImplicitMemberExpr(m) => m
                            .name()
                            .and_then(|n| crate::css::color_literal(b.g, n.text())),
                        _ => None,
                    };
                    vec![literal.map_or_else(|| b.expr(&v), js::str)]
                }
                "title" | "description" | "icon" => vec![b.expr(&first?)],
                "canonical" => vec![crate::app::href_expr(b, &first?)],
                "status" => {
                    let v = first?;
                    match implicit_name(&v).and_then(|n| status_code(&n)) {
                        Some(code) => vec![Expr::Num(f64::from(code))],
                        None => match &v {
                            ast::Expr::CallExpr(inner) => {
                                let a = inner.arg_list()?.args().next()?.value()?;
                                vec![b.expr(&a)]
                            }
                            other => vec![b.expr(other)],
                        },
                    }
                }
                "locale" => vec![js::str(locale_tag(
                    &implicit_name(&first?).unwrap_or_default(),
                ))],
                "manifest" => vec![b.expr(&by_label("name")?), b.expr(&by_label("shortName")?)],
                "socialPreview" => vec![b.expr(&by_label("image")?), b.expr(&by_label("title")?)],
                // A `StructuredData` value, emitted as JSON-LD by the runtime.
                "structuredData" => vec![b.expr(&first?)],
                _ => return None,
            })
        });
        let Some(values) = values else { continue };
        out.extend(pre);
        out.push(Stmt::Expr(call(member(target.clone(), name), values)));
    }
    out
}

/// The browser's share of metadata calls: a `title(_:)` becomes
/// `$k.title(() => text)`, which keeps `document.title` on the innermost
/// mounted title (runtime.md §10). With `ready` (a page's or layout's
/// loads), the title has no value until it is truthy, since the text
/// usually reads loaded data. The other entries only shape the server's
/// document `<head>`.
pub(crate) fn client_title(
    b: &mut Body<'_, '_>,
    calls: &[SyntaxNode],
    ready: Option<Expr>,
) -> Vec<Stmt> {
    let mut out = Vec::new();
    for node in calls {
        let Some(c) = ast::CallExpr::cast(node.clone()) else {
            continue;
        };
        let is_title = matches!(c.callee(), Some(ast::Expr::NameExpr(n)) if n.name().is_some_and(|t| t.text() == "title"));
        let Some(text) = c
            .arg_list()
            .and_then(|l| l.args().next())
            .and_then(|a| a.value())
            .filter(|_| is_title)
        else {
            continue;
        };
        let (mut pre, x) = b.capture(|b| b.expr(&text));
        let value = match &ready {
            Some(r) => Expr::Cond(Box::new(r.clone()), Box::new(x), Box::new(Expr::Null)),
            None => x,
        };
        let get = if pre.is_empty() {
            js::arrow(vec![], value)
        } else {
            pre.push(Stmt::Return(Some(value)));
            js::arrow_block(vec![], pre, false)
        };
        out.push(Stmt::Expr(rt("title", vec![get])));
    }
    out
}

/// The entry calls of a declaration's `metadata { }` member block.
pub(crate) fn metadata_block_calls(node: &SyntaxNode) -> Option<Vec<SyntaxNode>> {
    let block = node
        .children()
        .find(|c| c.kind() == SyntaxKind::MemberList)?
        .children()
        .find(|c| c.kind() == SyntaxKind::MetadataBlock)?;
    let inner = block.children().find(|c| c.kind() == SyntaxKind::Block)?;
    Some(
        inner
            .children()
            .filter(|c| c.kind() == SyntaxKind::CallExpr)
            .collect(),
    )
}

// ----------------------------------------------------------------- guards

/// The guards of a `requires` clause that is a direct child of `node`
/// (a page, protocol or rpc), as runtime guard arrays:
/// `["auth"]`, `["role", StaffRole.admin]`, `["rate", key, count, windowMs]`.
pub(crate) fn guards(b: &mut Body<'_, '_>, node: &SyntaxNode, key: &str) -> Vec<Expr> {
    let Some(clause) = node
        .children()
        .find(|c| c.kind() == SyntaxKind::RequiresClause)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, g) in clause
        .children()
        .filter(|c| c.kind() == SyntaxKind::Guard)
        .enumerate()
    {
        let name = g
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind().is_name_like())
            .map(|t| t.text().to_owned())
            .unwrap_or_default();
        let arg = g
            .children()
            .find_map(ast::ArgList::cast)
            .and_then(|l| l.args().next())
            .and_then(|a| a.value());
        match name.as_str() {
            "Authenticated" => out.push(Expr::Array(vec![js::str("auth")])),
            // The secret is read when the guard runs, from the stdlib's
            // `ServiceConfig` (40-workspaces-and-services.md).
            "ServiceAuthenticated" => out.push(Expr::Array(vec![
                js::str("service"),
                js::arrow(
                    vec![],
                    srt("configSecret", vec![js::str("K_SERVICE_TOKEN")]),
                ),
            ])),
            "Role" => {
                let role = arg
                    .as_ref()
                    .and_then(implicit_name)
                    .and_then(|case| role_value(b, &case))
                    .unwrap_or(Expr::Null);
                out.push(Expr::Array(vec![js::str("role"), role]));
            }
            "RateLimit" => {
                let (count, window) = arg.as_ref().and_then(rate_of).unwrap_or((0.0, 60_000.0));
                out.push(Expr::Array(vec![
                    js::str("rate"),
                    js::str(format!("{key}#{i}")),
                    Expr::Num(count),
                    Expr::Num(window),
                ]));
            }
            _ => {}
        }
    }
    out
}

/// The guards of a page's `requires` clause (a direct child of `node`)
/// that the browser can check against the `Session` mirror, for the
/// client route table (codegen.md §14): `["auth"]` for `Authenticated`
/// and `["role", "admin"]` for `Role(.admin)`, the role by its case name
/// as the mirror lists roles. `RateLimit` counts requests only the server
/// sees, so it is left out; the server checks every guard when it answers
/// the page.
pub(crate) fn client_guards(node: &SyntaxNode) -> Vec<Expr> {
    let Some(clause) = node
        .children()
        .find(|c| c.kind() == SyntaxKind::RequiresClause)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for g in clause.children().filter(|c| c.kind() == SyntaxKind::Guard) {
        let name = g
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind().is_name_like())
            .map(|t| t.text().to_owned())
            .unwrap_or_default();
        match name.as_str() {
            "Authenticated" => out.push(Expr::Array(vec![js::str("auth")])),
            "Role" => {
                let role = g
                    .children()
                    .find_map(ast::ArgList::cast)
                    .and_then(|l| l.args().next())
                    .and_then(|a| a.value())
                    .as_ref()
                    .and_then(implicit_name);
                // Without a case (a program the checker refused) the
                // browser can't tell, so the server decides.
                out.push(Expr::Array(vec![
                    js::str("role"),
                    role.map_or(Expr::Null, js::str),
                ]));
            }
            _ => {}
        }
    }
    out
}

/// `4.perMinute` → (4, 60000).
fn rate_of(e: &ast::Expr) -> Option<(f64, f64)> {
    let ast::Expr::MemberExpr(m) = e else {
        return None;
    };
    let unit = m.name()?;
    let count: f64 = m
        .receiver()?
        .syntax()
        .text()
        .to_string()
        .replace('_', "")
        .parse()
        .ok()?;
    let window = match unit.text() {
        "perSecond" => 1000.0,
        "perMinute" => 60_000.0,
        "perHour" => 3_600_000.0,
        "perDay" => 86_400_000.0,
        _ => return None,
    };
    Some((count, window))
}

/// The app's role enum: its one own enum implementing `k.auth.AuthRole`.
fn role_enum(g: &Gen<'_>) -> Option<DeclId> {
    let iface = g.view.lookup("k.auth.AuthRole")?;
    g.own.iter().copied().find(|&d| {
        g.view.decl(d).named_kind() == Some(DeclarationKind::Enum) && g.implements(d, iface)
    })
}

/// The app's user id type: its one own type implementing `k.auth.AuthUserId`.
fn user_id_type(g: &Gen<'_>) -> Option<DeclId> {
    let iface = g.view.lookup("k.auth.AuthUserId")?;
    g.own.iter().copied().find(|&d| g.implements(d, iface))
}

/// `StaffRole.admin` for role case `case`.
fn role_value(b: &mut Body<'_, '_>, case: &str) -> Option<Expr> {
    let e = role_enum(b.g)?;
    let m = b.g.find_member(e, case)?;
    Some(b.static_ref(m))
}

// ----------------------------------------------------------------- params

/// The runtime parser of a route parameter of type `t`: `$s.param.int`
/// on the server, `$k.routeParam.int` on the client (the same parsers,
/// shared in `std.js`, so both sides read a URL alike).
fn param_parser(b: &mut Body<'_, '_>, t: &Type) -> Expr {
    let g = b.g;
    let client = b.target == Target::Client;
    let p = |n: &str| {
        if client {
            member(member(id("$k"), "routeParam"), n.to_owned())
        } else {
            member(member(id("$s"), "param"), n.to_owned())
        }
    };
    let base = t.unwrap_optional();
    if g.is_int(base) {
        return p("int");
    }
    if g.is_double(base) {
        return p("double");
    }
    if g.is_bool(base) {
        return p("bool");
    }
    if g.is(base, g.b.uuid) {
        return p("uuid");
    }
    if let Some(d) = base.decl() {
        if Some(d) == g.view.lookup("k.core.Page") {
            // The table the path is matched against: the spec's pages on
            // the server, the client entry's `routes`.
            let table = if client { "routes" } else { "$spec" };
            return call(p("page"), vec![js::arrow(vec![], id(table))]);
        }
        if g.view.decl(d).named_kind() == Some(DeclarationKind::Enum)
            && g.cases(d).iter().all(|&c| g.case_fields(c).is_empty())
        {
            let e = b.decl_ref(d);
            let cases = g
                .cases(d)
                .iter()
                .map(|&c| {
                    let n = g.view.member(c).name.clone();
                    Prop::KeyValue(n.clone(), member(e.clone(), n))
                })
                .collect();
            return call(p("cases"), vec![Expr::Object(cases, false)]);
        }
    }
    p("string")
}

/// The `route(u, segments, params)` matcher of a page or endpoint with
/// URL `template` (`$s.route` on the server, `$k.route` on the client).
pub(crate) fn matcher(b: &mut Body<'_, '_>, page: DeclId, template: &str) -> Expr {
    let g = b.g;
    let segments: Vec<Expr> = template
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| js::str(s.to_owned()))
        .collect();
    let in_path: BTreeSet<String> = template
        .split('/')
        .filter_map(|s| s.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
        .map(str::to_owned)
        .collect();
    let mut params = Vec::new();
    for (m, name, t, has_default) in crate::app::params(g, page) {
        let parser = param_parser(b, &t);
        let required = in_path.contains(&name) || (!has_default && !matches!(t, Type::Optional(_)));
        let default = if required {
            Expr::Null
        } else {
            let init = g
                .member_node(m)
                .and_then(|n| n.children().find_map(ast::Expr::cast));
            match init {
                Some(e) => {
                    let (pre, x) = in_file(b, page, |b| b.capture(|b| b.expr(&e)));
                    if pre.is_empty() {
                        js::arrow(vec![], x)
                    } else {
                        let mut s = pre;
                        s.push(Stmt::Return(Some(x)));
                        js::arrow_block(vec![], s, false)
                    }
                }
                None => js::arrow(vec![], Expr::Null),
            }
        };
        params.push(Expr::Array(vec![
            js::str(name),
            parser,
            Expr::Bool(required),
            default,
        ]));
    }
    let args = vec![id("u"), Expr::Array(segments), Expr::Array(params)];
    let route = if b.target == Target::Client {
        rt("route", args)
    } else {
        srt("route", args)
    };
    js::arrow(vec![Param::new("u")], route)
}

/// `(p) => Page.product(p.id, p.tab)`: the `Page` value of a matched
/// route's parameters.
pub(crate) fn page_value(b: &mut Body<'_, '_>, page: DeclId) -> Expr {
    let g = b.g;
    let Some(p) = g.view.lookup("k.core.Page") else {
        return js::arrow(vec![Param::new("p")], Expr::Null);
    };
    let cases = b.decl_ref(p);
    let case = member(cases, crate::app::case_name(&g.view.decl(page).name));
    let ps = crate::app::params(g, page);
    let value = if ps.is_empty() {
        case
    } else {
        call(
            case,
            ps.into_iter()
                .map(|(_, n, _, _)| member(id("p"), n))
                .collect(),
        )
    };
    js::arrow(vec![Param::new("p")], value)
}

/// `Page$href(value)`.
pub(crate) fn href_of(b: &mut Body<'_, '_>, value: Expr) -> Expr {
    match b.g.view.lookup("k.core.Page") {
        Some(p) => {
            let f = b.use_decl(p, "Page$href".into());
            call(f, vec![value])
        }
        None => value,
    }
}

// ---------------------------------------------------------------- layouts

/// The layout named by an `@Layout(Name)` annotation directly on `node`.
fn layout_annotation(b: &Body<'_, '_>, d: DeclId, node: &SyntaxNode) -> Option<DeclId> {
    let ann = node
        .children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|t| t.text() == "Layout"))?;
    let arg = ann.arg_list()?.args().next()?.value()?;
    let ast::Expr::NameExpr(n) = arg else {
        return None;
    };
    let tok = n.name()?;
    match b.g.resolved(b.g.view.decl(d).file, tok.text_range()) {
        Some(k_sema::view::Resolved::Decl(l)) => Some(l),
        _ => {
            // By name among the project's layouts.
            b.g.own.iter().copied().find(|&l| {
                b.g.view.decl(l).named_kind() == Some(DeclarationKind::Layout)
                    && b.g.view.decl(l).name == tok.text()
            })
        }
    }
}

/// The layouts a page renders inside, outermost first: its own
/// `@Layout(…)` or the app's, none with `@NoLayout`.
pub(crate) fn layouts_of(b: &Body<'_, '_>, page: DeclId) -> Vec<DeclId> {
    let g = b.g;
    let info = g.view.decl(page);
    if info.has_annotation("NoLayout") {
        return Vec::new();
    }
    let own = g
        .decl_node(page)
        .and_then(|n| layout_annotation(b, page, &n));
    let chosen = own.or_else(|| {
        let app = g
            .own
            .iter()
            .copied()
            .find(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::App))?;
        let node = g.decl_node(app)?;
        layout_annotation(b, app, &node)
    });
    chosen.into_iter().collect()
}

// --------------------------------------------------------------- containers

/// The `@Inject` dependencies of a declaration: the injected types' decls.
pub(crate) fn injected(g: &Gen<'_>, d: DeclId) -> Vec<DeclId> {
    g.view
        .decl(d)
        .members
        .iter()
        .filter(|&&m| g.view.member(m).has_annotation("Inject"))
        .filter_map(|&m| g.view.member(m).ty.as_ref().and_then(Type::decl))
        .collect()
}

/// The scope of a service, from its `@Scope(.x)` annotation: `"app"` (the
/// default), `"request"`, `"route"`, `"component"` or `"server"`.
pub(crate) fn scope_of(g: &Gen<'_>, d: DeclId) -> &'static str {
    let Some(n) = g.decl_node(d) else {
        return "app";
    };
    for a in n.children().filter_map(ast::Annotation::cast) {
        let t = a.syntax().text().to_string().replace([' ', '\n'], "");
        for s in ["request", "route", "component", "server"] {
            if t == format!("@Scope(.{s})") {
                return s;
            }
        }
    }
    "app"
}

/// `true` for a `@Scope(.server)` service.
pub(crate) fn is_server_scoped(g: &Gen<'_>, d: DeclId) -> bool {
    scope_of(g, d) == "server"
}

/// The implementing service of protocol `p`: the app's own, never a
/// test's fake (a fake is bound by a test's `use` rule instead).
pub(crate) fn implementation(g: &Gen<'_>, p: DeclId) -> Option<DeclId> {
    g.own.iter().copied().find(|&s| {
        let info = g.view.decl(s);
        info.named_kind() == Some(DeclarationKind::Service)
            && !info.is_test
            && !info.in_test_module
            && g.implements(s, p)
    })
}

/// The getters of every injected type that has no getter of its own yet:
///
/// * an interface resolves to its implementation's getter: the app's `use`
///   rule for the selected environment (a rule without `in` applies to
///   every environment), else its single implementation among the
///   project's services (docs/design/10-services.md), else the stdlib's
///   default ([`stdlib_default`]: `FetchHttpClient`, `SmtpEmailProvider`,
///   `DatabaseJobQueue`, or their `k.test` doubles in a test build);
/// * `k.server.PublicUrls` is `$s.publicUrls(Page$href)`, absolute URLs
///   from `K_PUBLIC_ORIGIN` (docs/design/12-routing.md, "Absolute URLs");
/// * a stdlib config the app injects is read from the environment like
///   the app's own (`crate::ui::config_object`), outside tests;
/// * a stdlib service with a server body is that body
///   ([`server_platform_service`]), memoized per container;
/// * anything else (a stdlib service the v0 server doesn't provide yet) is
///   `$s.missingNative(name)`: injecting it works, and calling it panics
///   with its name instead of failing with a `TypeError` on `undefined`.
fn bound_services(b: &mut Body<'_, '_>, provided: &BTreeSet<String>) -> Vec<Function> {
    let g = b.g;
    let mut provided = provided.clone();
    let mut wanted: Vec<DeclId> = Vec::new();
    for &d in &g.own {
        for t in injected(g, d) {
            if !wanted.contains(&t) {
                wanted.push(t);
            }
        }
    }
    // A test build also provides what the test groups' `use` rules name,
    // for the groups' server containers (`crate::suite::server_groups`).
    if tests(g) {
        for grp in crate::suite::groups(g) {
            for t in crate::suite::rule_types(g, grp) {
                if !wanted.contains(&t) {
                    wanted.push(t);
                }
            }
        }
    }
    let rules = use_rules(g);
    let mut out = Vec::new();
    let mut i = 0;
    while i < wanted.len() {
        let t = wanted[i];
        i += 1;
        let name = g.name(t);
        if provided.contains(&name) {
            continue;
        }
        let qualified = g.view.qualified_name(t);
        let body = if qualified == "k.ui.Theme" {
            // Client-only, but it exists during server rendering with the
            // remembered variant.
            memo(&name, srt("theme", vec![this("$r")]))
        } else if qualified == "k.server.PublicUrls" {
            let href = match g.view.lookup("k.core.Page") {
                Some(p) => b.use_decl(p, "Page$href".into()),
                None => js::arrow(vec![Param::new("p")], id("p")),
            };
            let mut args = vec![href];
            // With endpoint pages, their absolute URLs too.
            if !crate::app::endpoints(g).is_empty() {
                if let Some(e) = g.view.lookup("k.core.Endpoint") {
                    args.push(b.use_decl(e, "Endpoint$href".into()));
                }
            }
            memo(&name, srt("publicUrls", args))
        } else if g.view.decl(t).named_kind() == Some(DeclarationKind::Interface)
            && let Some(imp) = binding(g, &rules, t).or_else(|| stdlib_default(g, &qualified))
        {
            if !provided.contains(&g.name(imp)) && !wanted.contains(&imp) {
                wanted.push(imp);
            }
            this(g.name(imp))
        } else if g.view.decl(t).named_kind() == Some(DeclarationKind::Config) && !tests(g) {
            // A stdlib config the app injects (`k.rpc.ServiceConfig`): read
            // from the environment like the app's own configs. Tests never
            // read the environment, so there it stays a stand-in whose use
            // panics with its name.
            memo(&name, crate::ui::config_object(b, t))
        } else if let Some(body) = server_platform_service(b, &qualified) {
            if tests(g) && qualified.starts_with("k.test.") && qualified != "k.test.InlineJobQueue"
            {
                // One double per test, whichever request reaches it, so the
                // test inspects what the code under test used.
                per_test(&name, body)
            } else {
                memo(&name, body)
            }
        } else {
            memo(&name, srt("missingNative", vec![js::str(qualified)]))
        };
        provided.insert(name.clone());
        out.push(getter(name, body));
    }
    out
}

/// The stdlib's implementation of platform interface `qualified` when the
/// app binds none: the production one, or the `k.test` double a test build
/// installs by default (docs/design/21-testing.md, "Test doubles and
/// defaults").
fn stdlib_default(g: &Gen<'_>, qualified: &str) -> Option<DeclId> {
    let test = g.options.environment == crate::Environment::Test;
    let imp = match (qualified, test) {
        ("k.http.HttpClient", false) => "k.http.FetchHttpClient",
        ("k.http.HttpClient", true) => "k.test.TestHttpClient",
        ("k.server.EmailProvider", false) => "k.server.SmtpEmailProvider",
        ("k.server.EmailProvider", true) => "k.test.RecordingEmailProvider",
        ("k.server.JobQueue", false) => "k.server.DatabaseJobQueue",
        ("k.server.JobQueue", true) => "k.test.RecordingJobQueue",
        _ => return None,
    };
    g.view.lookup(imp)
}

/// The `HttpStatus` codes in declaration order (`null` for `other`), the
/// table the runtime maps received statuses with.
pub(crate) fn http_status_codes(g: &Gen<'_>) -> Expr {
    Expr::Array(
        case_names(g, "k.http.HttpStatus")
            .iter()
            .map(|n| status_code(n).map_or(Expr::Null, |c| Expr::Num(f64::from(c))))
            .collect(),
    )
}

/// References to the generated stdlib enums `names`, `null` for one that
/// doesn't exist.
fn stdlib_enums(b: &mut Body<'_, '_>, names: &[&str]) -> Vec<Expr> {
    names
        .iter()
        .map(|n| match b.g.view.lookup(n) {
            Some(d) => b.decl_ref(d),
            None => Expr::Null,
        })
        .collect()
}

/// The server body of a stdlib service by qualified name
/// (`k:runtime/outbound.js`, docs/compiler/server.md §18), or `None`.
fn server_platform_service(b: &mut Body<'_, '_>, qualified: &str) -> Option<Expr> {
    let http = |b: &mut Body<'_, '_>, f: &str| {
        let mut args = stdlib_enums(b, &["k.http.HttpError", "k.http.HttpStatus"]);
        args.push(http_status_codes(b.g));
        srt(f, args)
    };
    Some(match qualified {
        "k.http.FetchHttpClient" => http(b, "fetchHttpClient"),
        "k.test.TestHttpClient" => http(b, "testHttpClient"),
        "k.server.SmtpEmailProvider" => {
            let mut args = stdlib_enums(b, &["k.server.EmailError"]);
            args.push(member(srt("info", vec![]), "config"));
            srt("smtpEmailProvider", args)
        }
        "k.test.RecordingEmailProvider" => srt(
            "recordingEmailProvider",
            stdlib_enums(b, &["k.server.EmailError"]),
        ),
        "k.server.DatabaseJobQueue" => srt("jobQueue", vec![this("Database")]),
        "k.test.RecordingJobQueue" => srt("recordingJobQueue", vec![]),
        "k.test.InlineJobQueue" => srt(
            "inlineJobQueue",
            vec![member(this("$r"), "spec"), Expr::This, this("Database")],
        ),
        // The test's virtual clock, shared with its browser container.
        "k.test.TestClock" => srt("clock", vec![this("$r")]),
        // The verified caller of a `ServiceAuthenticated` rpc.
        "k.rpc.ServiceCaller" => srt("serviceCaller", vec![this("$r")]),
        "k.test.TestClient" => {
            let mut args = vec![this("$r")];
            args.extend(stdlib_enums(b, &["k.http.HttpError", "k.http.HttpStatus"]));
            args.push(http_status_codes(b.g));
            srt("testClient", args)
        }
        _ => return None,
    })
}

/// One `use Impl for Target in .env, …` rule of the app.
pub(crate) struct UseRule {
    /// The implementation named before `for`, `None` for a remote rule or one
    /// that doesn't resolve.
    imp: Option<DeclId>,
    /// For `use remote(Config) for Protocol`: `Some` of the config, itself
    /// `None` when it doesn't resolve; `None` for a rule that isn't remote.
    pub(crate) remote: Option<Option<DeclId>>,
    /// The declaration named after `for` (an interface or protocol), `None`
    /// when it doesn't resolve.
    pub(crate) target: Option<DeclId>,
    /// The environments' case names; empty for every environment.
    envs: Vec<String>,
}

/// The `use` rules of the project's `app` declaration, in source order.
pub(crate) fn use_rules(g: &Gen<'_>) -> Vec<UseRule> {
    let Some(app) = g
        .own
        .iter()
        .copied()
        .find(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::App))
    else {
        return Vec::new();
    };
    let Some(node) = g.decl_node(app) else {
        return Vec::new();
    };
    let file = g.view.decl(app).file;
    let resolve = |ty: &ast::Type| -> Option<DeclId> {
        let tok = ty
            .syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind() == SyntaxKind::Ident)
            .last()?;
        match g.resolved(file, tok.text_range()) {
            Some(k_sema::view::Resolved::Decl(d)) => Some(d),
            _ => g
                .own
                .iter()
                .copied()
                .find(|&d| g.view.decl(d).name == tok.text()),
        }
    };
    node.descendants()
        .filter_map(ast::UseDecl::cast)
        .map(|u| UseRule {
            imp: u.implementation().and_then(|t| resolve(&t)),
            remote: u.value().and_then(|e| remote_config(g, file, &e)),
            target: u.target().and_then(|t| resolve(&t)),
            envs: u
                .environments()
                .iter()
                .filter_map(|e| {
                    e.syntax()
                        .descendants_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .find(|t| t.kind() == SyntaxKind::Ident)
                        .map(|t| t.text().to_owned())
                })
                .collect(),
        })
        .collect()
}

/// The config of a `remote(Config)` operand: `Some(None)` when it names
/// nothing, `None` when the operand isn't `remote(…)`.
fn remote_config(
    g: &Gen<'_>,
    file: k_diagnostics::FileId,
    e: &ast::Expr,
) -> Option<Option<DeclId>> {
    let ast::Expr::CallExpr(c) = e else {
        return None;
    };
    let callee = c.syntax().children().find_map(ast::Expr::cast)?;
    if callee.syntax().text() != "remote" {
        return None;
    }
    let arg = c
        .syntax()
        .children()
        .find_map(ast::ArgList::cast)
        .and_then(|l| l.args().next())
        .and_then(|a| a.value());
    Some(
        arg.and_then(|a| match g.resolved(file, a.syntax().text_range()) {
            Some(k_sema::view::Resolved::Decl(d)) => Some(d),
            _ => None,
        }),
    )
}

/// The config of the `remote` rule that binds protocol `p` in the
/// selected environment, if one does.
pub(crate) fn remote_binding(g: &Gen<'_>, rules: &[UseRule], p: DeclId) -> Option<DeclId> {
    let env = match g.options.environment {
        crate::Environment::Production => "production",
        crate::Environment::Development => "development",
        crate::Environment::Test => "test",
    };
    rules
        .iter()
        .find(|r| r.target == Some(p) && (r.envs.is_empty() || r.envs.iter().any(|e| e == env)))
        .and_then(|r| r.remote.flatten())
}

/// The manifest's [`crate::RemoteEntry`]s: every protocol a `remote` rule
/// binds in the selected environment, with its config's `url` and `token`
/// variables.
pub(crate) fn remote_entries(g: &Gen<'_>) -> Vec<crate::RemoteEntry> {
    let rules = use_rules(g);
    let mut out: Vec<crate::RemoteEntry> = Vec::new();
    for &p in g.remote_protocols() {
        let Some(c) = remote_binding(g, &rules, p) else {
            continue;
        };
        let field_env = |name: &str| {
            g.view
                .decl(c)
                .members
                .iter()
                .find(|&&m| g.view.member(m).name == name)
                .and_then(|&m| g.member_node(m))
                .and_then(|n| env_name(&n))
        };
        if let (Some(url_env), Some(token_env)) = (field_env("url"), field_env("token")) {
            out.push(crate::RemoteEntry {
                protocol: g.view.qualified_name(p),
                url_env,
                token_env,
            });
        }
    }
    out.sort_by(|a, b| a.protocol.cmp(&b.protocol));
    out
}

/// The qualified names of the protocols the app implements, sorted.
pub(crate) fn served_protocols(g: &Gen<'_>) -> Vec<String> {
    let mut out: Vec<String> = g
        .own
        .iter()
        .copied()
        .filter(|&p| {
            g.view.decl(p).named_kind() == Some(DeclarationKind::Protocol)
                && implementation(g, p).is_some()
        })
        .map(|p| g.view.qualified_name(p))
        .collect();
    out.sort();
    out
}

/// The implementation interface `iface` is bound to in the selected
/// environment: the first matching `use` rule, else the single project
/// service implementing it.
pub(crate) fn binding(g: &Gen<'_>, rules: &[UseRule], iface: DeclId) -> Option<DeclId> {
    let env = match g.options.environment {
        crate::Environment::Production => "production",
        crate::Environment::Development => "development",
        crate::Environment::Test => "test",
    };
    if let Some(r) = rules
        .iter()
        .find(|r| r.target == Some(iface) && (r.envs.is_empty() || r.envs.iter().any(|e| e == env)))
    {
        return r.imp;
    }
    // Test code (fakes in test files and test-only modules) is bound only
    // by a `use` rule, never as the single implementation.
    let mut impls = g.own.iter().copied().filter(|&s| {
        let info = g.view.decl(s);
        !info.is_test
            && !info.in_test_module
            && info.named_kind() == Some(DeclarationKind::Service)
            && (crate::bundle::allowed(g, s, Target::Server)
                || crate::bundle::allowed(g, s, Target::Client))
            && g.implements(s, iface)
    });
    let first = impls.next()?;
    impls.next().is_none().then_some(first)
}

/// `get Name() { return body; }`.
pub(crate) fn getter(name: String, body: Expr) -> Function {
    Function {
        name,
        params: Vec::new(),
        body: js::Body::Block(vec![Stmt::Return(Some(body))]),
        is_async: false,
        kind: MethodKind::Getter,
    }
}

/// `true` when generating the test bundle (`k test`).
fn tests(g: &Gen<'_>) -> bool {
    g.options.entry == crate::Entry::Tests
}

/// `$s.perTest(this, "Name", () => value)`: one instance per test of
/// `k test`, shared by every container of the test (outside a test, one
/// per container, like [`memo`]).
pub(crate) fn per_test(name: &str, value: Expr) -> Expr {
    srt(
        "perTest",
        vec![Expr::This, js::str(name), js::arrow(vec![], value)],
    )
}

/// `this.$Name ??= value`.
pub(crate) fn memo(name: &str, value: Expr) -> Expr {
    Expr::Assign("??=", Box::new(this(format!("${name}"))), Box::new(value))
}

/// The two container classes and the in-process protocol clients.
fn containers(b: &mut Body<'_, '_>) -> Vec<Stmt> {
    let g = b.g;
    let mut process: Vec<Function> = Vec::new();
    let mut request: Vec<Function> = vec![Function {
        kind: MethodKind::Constructor,
        params: vec![Param::new("st")],
        body: js::Body::Block(vec![Stmt::Expr(js::assign(this("$r"), id("st")))]),
        ..Function::default()
    }];
    let mut extra = Vec::new();
    let services: Vec<DeclId> = g
        .own
        .iter()
        .copied()
        .filter(|&d| {
            g.view.decl(d).named_kind() == Some(DeclarationKind::Service)
                && crate::bundle::allowed(g, d, Target::Server)
        })
        .collect();
    for &s in &services {
        let name = g.name(s);
        let server = is_server_scoped(g, s);
        let owner = if server { "$Process" } else { "$Request" };
        let _ = owner;
        let deps: Vec<Prop> = injected(g, s)
            .into_iter()
            .map(|t| {
                let n = g.name(t);
                Prop::KeyValue(n.clone(), this(n))
            })
            .collect();
        let class = b.decl_ref(s);
        let construct = Expr::New(Box::new(class), vec![Expr::Object(deps, false)]);
        if server {
            let state: Vec<Expr> = g
                .view
                .decl(s)
                .members
                .iter()
                .filter(|&&m| g.view.member(m).has_annotation("State"))
                .map(|&m| js::str(g.view.member(m).name.clone()))
                .collect();
            let shared = srt(
                "shareState",
                vec![
                    construct,
                    js::str(g.view.qualified_name(s)),
                    Expr::Array(state),
                ],
            );
            process.push(getter(name.clone(), memo(&name, shared)));
            request.push(getter(name.clone(), member(id("$process"), name)));
        } else if scope_of(g, s) == "component" {
            // A new instance per injecting component, which provides it
            // to its subtree (`$k.provide`).
            request.push(getter(name.clone(), construct));
        } else if tests(g) && (g.view.decl(s).is_test || g.view.decl(s).in_test_module) {
            // A fake is one instance per test: the one the test injects is
            // the one every request of the test uses.
            request.push(getter(name.clone(), per_test(&name, construct)));
        } else {
            request.push(getter(name.clone(), memo(&name, construct)));
        }
    }
    // Repositories: per request, with the request's `Database`.
    for &r in &g.own {
        if g.view.decl(r).named_kind() != Some(DeclarationKind::Repository) {
            continue;
        }
        let name = g.name(r);
        let mut deps: Vec<Prop> = vec![Prop::KeyValue("Database".into(), this("Database"))];
        for t in injected(g, r) {
            let n = g.name(t);
            deps.push(Prop::KeyValue(n.clone(), this(n)));
        }
        let class = b.decl_ref(r);
        let construct = Expr::New(Box::new(class), vec![Expr::Object(deps, false)]);
        request.push(getter(name.clone(), memo(&name, construct)));
    }
    // Configs: read once per isolate.
    for &d in &g.own {
        if g.view.decl(d).named_kind() != Some(DeclarationKind::Config) {
            continue;
        }
        let name = g.name(d);
        let load = b.use_decl(d, format!("{name}$load"));
        process.push(getter(name.clone(), memo(&name, call(load, vec![]))));
        request.push(getter(name.clone(), member(id("$process"), name)));
    }
    // Protocols: the in-process client of the implementation, through the
    // rpc table (`$s.callLocal`), so guards and validation run.
    for &p in &g.own {
        if g.view.decl(p).named_kind() != Some(DeclarationKind::Protocol) {
            continue;
        }
        if implementation(g, p).is_none() {
            continue;
        }
        let pname = g.name(p);
        let local = format!("{pname}$local");
        let mut methods = vec![Function {
            kind: MethodKind::Constructor,
            params: vec![Param::new("c")],
            body: js::Body::Block(vec![Stmt::Expr(js::assign(this("c"), id("c")))]),
            ..Function::default()
        }];
        for &m in &g.view.decl(p).members {
            if g.view.member(m).kind != MemberKind::Rpc {
                continue;
            }
            let mname = g.member_name(m);
            // Through the rpc's table entry, so its guards and validation
            // run as over HTTP (28-rpc-wire-format.md).
            let path = crate::app::rpc_path(g, p, m);
            let entry = Expr::Index(
                Box::new(member(id("$spec"), "rpcs")),
                Box::new(js::str(path)),
            );
            methods.push(Function {
                name: mname.clone(),
                params: vec![
                    Param::new("$c"),
                    Param {
                        name: "a".into(),
                        default: None,
                        rest: true,
                    },
                ],
                body: js::Body::Block(vec![Stmt::Return(Some(srt(
                    "callLocal",
                    vec![this("c"), entry, id("$c"), id("a")],
                )))]),
                is_async: false,
                kind: MethodKind::Plain,
            });
        }
        extra.push(Stmt::Class(
            Class {
                name: local.clone(),
                extends: None,
                methods,
            },
            false,
        ));
        request.push(getter(
            pname.clone(),
            memo(&pname, Expr::New(Box::new(id(local)), vec![Expr::This])),
        ));
    }
    // Protocols called in another service (40-workspaces-and-services.md):
    // the remote client of the environment's `remote` rule, or the
    // `TestRemote` double in a test build, which never opens a socket.
    let rules = use_rules(g);
    let caller = g.project_name();
    for &p in g.remote_protocols() {
        if implementation(g, p).is_some() {
            continue;
        }
        let pname = g.name(p);
        let binding = if tests(g) {
            Some(srt("testRemote", vec![js::str(caller.clone())]))
        } else {
            remote_binding(g, &rules, p)
                .map(|c| srt("remoteOf", vec![js::str(caller.clone()), this(g.name(c))]))
        };
        let Some(binding) = binding else {
            continue;
        };
        let class = b.use_decl(p, format!("{pname}$remote"));
        request.push(getter(
            pname.clone(),
            memo(&pname, Expr::New(Box::new(class), vec![binding])),
        ));
    }
    // Interfaces and stdlib services that something injects.
    let mut provided: BTreeSet<String> = request.iter().map(|f| f.name.clone()).collect();
    provided.extend(
        ["Session", "SessionManager", "Database", "Clock"]
            .iter()
            .map(|n| (*n).to_owned()),
    );
    for f in bound_services(b, &provided) {
        request.push(f);
    }
    // The stdlib's request services.
    request.push(getter(
        "Session".into(),
        memo("Session", srt("session", vec![this("$r")])),
    ));
    request.push(getter(
        "SessionManager".into(),
        memo("SessionManager", srt("sessionManager", vec![this("$r")])),
    ));
    // One `Database` per request: its transaction binds every repository
    // call of the request (db.md §9).
    request.push(getter(
        "Database".into(),
        memo("Database", srt("database", vec![])),
    ));
    // In a test, the test's virtual clock (`st.test.clock`).
    let clock_args = if tests(g) {
        vec![this("$r")]
    } else {
        Vec::new()
    };
    request.push(getter(
        "Clock".into(),
        memo("Clock", srt("clock", clock_args)),
    ));
    let new_process = Expr::New(Box::new(id("$Process")), vec![]);
    let mut out = vec![
        Stmt::Class(
            Class {
                name: "$Process".into(),
                extends: None,
                methods: process,
            },
            false,
        ),
        // Each test of a test build starts a new process (`$test.reset`).
        if tests(g) {
            Stmt::Let("$process".into(), Some(new_process))
        } else {
            Stmt::Const("$process".into(), new_process)
        },
        Stmt::Class(
            Class {
                name: "$Request".into(),
                extends: None,
                methods: request,
            },
            false,
        ),
    ];
    out.extend(extra);
    out
}

// --------------------------------------------------------------------- rpc

/// The Protobuf package of declaration `d`: its module, prefixed with the
/// name of the package that declares it like rpc paths (`rpc_wire.catalog`;
/// a library's declarations take the library's name, so every app of a
/// workspace agrees on them).
pub(crate) fn package_of(g: &Gen<'_>, d: DeclId) -> String {
    let module = g.view.decl(d).module.clone().unwrap_or_default();
    let project = g.package_name(d).replace('-', "_");
    if project.is_empty() || module == project || module.starts_with(&format!("{project}.")) {
        module
    } else if module.is_empty() {
        project
    } else {
        format!("{project}.{module}")
    }
}

/// `true` for a struct type that crosses the wire as the message itself.
pub(crate) fn is_message_struct(g: &Gen<'_>, t: &Type) -> bool {
    !matches!(t, Type::Optional(_))
        && t.decl().is_some_and(|d| {
            let info = g.view.decl(d);
            !info.stdlib && info.named_kind() == Some(DeclarationKind::Struct)
        })
}

/// The response encoder of a result type: `{}` for none, the message for
/// a struct, `{"value": …}` otherwise.
pub(crate) fn result_encoder(b: &mut Body<'_, '_>, t: &Type) -> Expr {
    let g = b.g;
    if t.decl() == g.b.void || matches!(t, Type::Error) {
        return js::arrow(vec![], Expr::Object(Vec::new(), false));
    }
    let c = crate::codec::codec_expr(b, t);
    let encoded = call(member(c, "encode"), vec![id("v")]);
    let body = if is_message_struct(g, t) {
        encoded
    } else {
        Expr::Object(vec![Prop::KeyValue("value".into(), encoded)], false)
    };
    js::arrow(vec![Param::new("v")], body)
}

/// The element type of a `Stream<T>`, if `t` is one.
pub(crate) fn stream_element(g: &Gen<'_>, t: &Type) -> Option<Type> {
    let s = g.view.lookup("k.core.Stream")?;
    (t.decl() == Some(s)).then(|| t.args().first().cloned().unwrap_or(Type::Error))
}

/// A server-side check function of a `Validatable` struct:
/// `async X$check(v, p, f, c, $c)` pushes `{path, message}` for each
/// field's first failing constraint, checks nested `Validatable` values
/// (also inside optionals, lists and sets) under their paths, then runs
/// `validate(rules:)` when the fields pass (15-forms-and-validation.md:
/// "The server can't be bypassed"). `c` is the request container and `$c`
/// the request's signal, for `@Check` rules, which call a service.
fn validator(
    b: &mut Body<'_, '_>,
    d: DeclId,
    out: &mut Vec<Stmt>,
    done: &mut BTreeSet<DeclId>,
) -> String {
    let g = b.g;
    let name = format!("{}$check", g.name(d));
    if !done.insert(d) {
        return name;
    }
    let mut body = vec![Stmt::Const("n".into(), member(id("f"), "length"))];
    for f in g.stored_fields(d) {
        let fi = g.view.member(f).clone();
        let ft = fi.ty.clone().unwrap_or(Type::Error);
        let Some(node) = g.member_node(f) else {
            continue;
        };
        let rules: Vec<Expr> = in_file(b, d, |b| {
            node.children()
                .filter_map(ast::Annotation::cast)
                .filter_map(|a| {
                    let n = a.name()?.text().to_owned();
                    if n == "Check" {
                        return check_rule(b, d, a.arg_list().as_ref());
                    }
                    crate::app::constraint(b, &n, a.arg_list().as_ref(), &ft)
                })
                .collect()
        });
        let value = member(id("v"), fi.name.clone());
        let path = js::bin("+", id("p"), js::str(format!(".{}", fi.name)));
        if !rules.is_empty() {
            // for (const r of [rules]) { const m = await r(v.field); if (m != null) { f.push(…); break; } }
            let push = call(
                member(id("f"), "push"),
                vec![Expr::Object(
                    vec![
                        Prop::KeyValue("path".into(), path.clone()),
                        Prop::KeyValue("message".into(), id("m")),
                    ],
                    false,
                )],
            );
            let inner = vec![
                Stmt::Const(
                    "m".into(),
                    Expr::Await(Box::new(call(id("r"), vec![value.clone()]))),
                ),
                Stmt::If(
                    js::bin("!=", id("m"), Expr::Null),
                    vec![Stmt::Expr(push), Stmt::Break(None)],
                    None,
                ),
            ];
            let lp = Stmt::ForOf("r".into(), Expr::Array(rules), inner, false);
            body.push(if matches!(ft, Type::Optional(_)) {
                Stmt::If(js::bin("!==", value.clone(), Expr::Null), vec![lp], None)
            } else {
                lp
            });
        }
        body.extend(nested_checks(b, &ft, value, path, out, done, 0));
    }
    if let Some(v) = g
        .find_member(d, "validate")
        .filter(|&v| !g.view.member(v).is_static)
    {
        let f = b.fn_ref(v);
        let rules = Expr::Object(
            vec![Prop::KeyValue(
                "check".into(),
                js::arrow_block(
                    vec![Param::new("ok"), Param::new("field"), Param::new("message")],
                    vec![Stmt::If(
                        js::un("!", id("ok")),
                        vec![Stmt::Expr(call(
                            member(id("f"), "push"),
                            vec![Expr::Object(
                                vec![
                                    Prop::KeyValue(
                                        "path".into(),
                                        js::bin(
                                            "+",
                                            js::bin("+", id("p"), js::str(".")),
                                            id("field"),
                                        ),
                                    ),
                                    Prop::KeyValue("message".into(), id("message")),
                                ],
                                false,
                            )],
                        ))],
                        None,
                    )],
                    false,
                ),
            )],
            false,
        );
        body.push(Stmt::If(
            js::bin("===", member(id("f"), "length"), id("n")),
            vec![Stmt::Expr(call(f, vec![id("v"), rules]))],
            None,
        ));
    }
    out.push(Stmt::Function(
        Function {
            name: name.clone(),
            params: vec![
                Param::new("v"),
                Param::new("p"),
                Param::new("f"),
                Param::new("c"),
                Param::new("$c"),
            ],
            body: js::Body::Block(body),
            is_async: true,
            ..Function::default()
        },
        false,
    ));
    name
}

/// `true` when values of type `t` contain a `Validatable` value: it is
/// one, or an optional, list or set of one.
fn needs_check(g: &Gen<'_>, t: &Type, depth: usize) -> bool {
    let Some(v) = g.view.lookup("k.validation.Validatable") else {
        return false;
    };
    if depth > 8 {
        return false;
    }
    match t {
        Type::Optional(inner) => needs_check(g, inner, depth + 1),
        _ => match t.decl() {
            Some(d) if g.implements(d, v) => true,
            Some(d) if Some(d) == g.b.list || Some(d) == g.b.set => t
                .args()
                .first()
                .is_some_and(|e| needs_check(g, e, depth + 1)),
            _ => false,
        },
    }
}

/// The statements that check the `Validatable` values inside `value` (of
/// type `t`, at JSON path `path`), pushing failures to `f`: the type's
/// `X$check` for a `Validatable` value, each element with its index
/// (`requests[1]`) for a list or set, the inner value of a non-`nil`
/// optional. Empty when `t` holds no `Validatable` value.
fn nested_checks(
    b: &mut Body<'_, '_>,
    t: &Type,
    value: Expr,
    path: Expr,
    out: &mut Vec<Stmt>,
    done: &mut BTreeSet<DeclId>,
    depth: usize,
) -> Vec<Stmt> {
    let g = b.g;
    if !needs_check(g, t, depth) {
        return Vec::new();
    }
    if let Type::Optional(inner) = t {
        let inner = nested_checks(b, inner, value.clone(), path, out, done, depth + 1);
        return vec![Stmt::If(js::bin("!==", value, Expr::Null), inner, None)];
    }
    let Some(d) = t.decl() else {
        return Vec::new();
    };
    if Some(d) == g.b.list || Some(d) == g.b.set {
        let el = t.args().first().cloned().unwrap_or(Type::Error);
        let (x, i) = (format!("$x{depth}"), format!("$i{depth}"));
        let at = js::bin(
            "+",
            js::bin("+", js::bin("+", path, js::str("[")), id(i.clone())),
            js::str("]"),
        );
        let mut inner = nested_checks(b, &el, id(x.clone()), at, out, done, depth + 1);
        inner.push(Stmt::Expr(Expr::Unary("++", Box::new(id(i.clone())))));
        return vec![Stmt::Block(vec![
            Stmt::Let(i, Some(Expr::Num(0.0))),
            Stmt::ForOf(x, value, inner, false),
        ])];
    }
    let f = validator(b, d, out, done);
    vec![Stmt::Expr(Expr::Await(Box::new(call(
        id(f),
        vec![value, path, id("f"), id("c"), id("$c")],
    ))))]
}

/// The target of a `@Check(Service.function, message:)` rule on a field of
/// struct `d`: the service (one of this program's, the struct's own module
/// first), its function and the failure message (the rule's literal, else
/// the stdlib's default). `None` when the name doesn't resolve (sema
/// reports that). Shared by the server's validator and the browser's form
/// (`crate::app::form`).
pub(crate) fn check_target(
    g: &Gen<'_>,
    d: DeclId,
    args: Option<&ast::ArgList>,
) -> Option<(DeclId, MemberId, String)> {
    let list = args?;
    let target = list.args().find(|a| a.label().is_none())?.value()?;
    let text = target
        .syntax()
        .text()
        .to_string()
        .replace(char::is_whitespace, "");
    let (owner, func) = text.rsplit_once('.')?;
    let owner = owner.rsplit('.').next().unwrap_or(owner);
    let module = g.view.decl(d).module.clone();
    let mut candidates: Vec<DeclId> = g
        .own
        .iter()
        .copied()
        .filter(|&s| {
            let info = g.view.decl(s);
            info.name == owner && info.named_kind() == Some(DeclarationKind::Service)
        })
        .collect();
    // A service of the struct's own module wins over one of another module.
    candidates.sort_by_key(|&s| g.view.decl(s).module != module);
    let service = *candidates.first()?;
    let m = g.find_member(service, func)?;
    let message = list
        .args()
        .find(|a| a.label().is_some_and(|l| l.text() == "message"))
        .and_then(|a| a.value())
        .and_then(|v| match v {
            ast::Expr::StringLit(s) => s.plain_value(),
            _ => None,
        })
        .unwrap_or_else(|| "Check this field".to_owned());
    Some((service, m, message))
}

/// The call of a `@Check` target's function on `x` through container `c`
/// (`c.Service.f($c, x)`, the signal only for an async function).
pub(crate) fn check_call(g: &Gen<'_>, c: Expr, service: DeclId, m: MemberId) -> Expr {
    let callee = member(member(c, g.name(service)), g.member_name(m));
    let mut call_args = Vec::new();
    if g.view
        .member(m)
        .signature
        .as_ref()
        .is_some_and(|s| s.is_async)
    {
        call_args.push(id("$c"));
    }
    call_args.push(id("x"));
    call(callee, call_args)
}

/// A `@Check(Service.function, message:)` rule on the server: an async
/// function of the field's value that calls the named service function
/// through the request container `c` and returns the message when it
/// answers `false` (15-forms-and-validation.md: "On the server it always
/// runs in full"). `None` when the name doesn't resolve to a member of a
/// service of this program that the server has (sema reports that).
fn check_rule(b: &mut Body<'_, '_>, d: DeclId, args: Option<&ast::ArgList>) -> Option<Expr> {
    let g = b.g;
    let (service, m, message) = check_target(g, d, args)?;
    // Only a service the server's request container has can be called.
    if !crate::bundle::allowed(g, service, Target::Server) {
        return None;
    }
    Some(js::arrow_block(
        vec![Param::new("x")],
        vec![Stmt::Return(Some(Expr::Cond(
            Box::new(Expr::Await(Box::new(check_call(g, id("c"), service, m)))),
            Box::new(Expr::Null),
            Box::new(js::str(message)),
        )))],
        true,
    ))
}

/// Every rpc path the server answers (protocols with an implementation),
/// sorted.
pub(crate) fn rpc_paths(g: &Gen<'_>) -> Vec<String> {
    let mut out = BTreeSet::new();
    for &p in &g.own {
        let info = g.view.decl(p);
        if info.named_kind() != Some(DeclarationKind::Protocol) || implementation(g, p).is_none() {
            continue;
        }
        for &m in &info.members {
            if g.view.member(m).kind == MemberKind::Rpc {
                out.insert(crate::app::rpc_path(g, p, m));
            }
        }
    }
    out.into_iter().collect()
}

/// The rpc table: path → handler spec.
fn rpc_table(b: &mut Body<'_, '_>, helpers: &mut Vec<Stmt>) -> Expr {
    let g = b.g;
    let rpc_only = g.view.lookup("k.rpc.RpcOnlyError");
    let mut done = BTreeSet::new();
    let mut entries = Vec::new();
    for &p in &g.own {
        let info = g.view.decl(p).clone();
        if info.named_kind() != Some(DeclarationKind::Protocol) {
            continue;
        }
        let Some(imp) = implementation(g, p) else {
            continue;
        };
        let pnode = g.decl_node(p);
        for &m in &info.members {
            let mi = g.view.member(m).clone();
            if mi.kind != MemberKind::Rpc {
                continue;
            }
            let path = crate::app::rpc_path(g, p, m);
            let sig = mi.signature.clone();
            let mut decoders = Vec::new();
            let mut checks = Vec::new();
            for (i, param) in sig.iter().flat_map(|s| s.params.iter()).enumerate() {
                let c = crate::codec::codec_expr(b, &param.ty);
                decoders.push(call(
                    member(c, "decode"),
                    vec![
                        Expr::Index(Box::new(id("j")), Box::new(js::str(param.name.clone()))),
                        js::str(param.name.clone()),
                    ],
                ));
                let value = Expr::Index(Box::new(id("a")), Box::new(Expr::Num(i as f64)));
                checks.extend(nested_checks(
                    b,
                    &param.ty,
                    value,
                    js::str(param.name.clone()),
                    helpers,
                    &mut done,
                    0,
                ));
            }
            let ret = sig.as_ref().map(|s| s.ret.clone()).unwrap_or(Type::Error);
            let (stream, encode) = match stream_element(g, &ret) {
                Some(el) => (true, result_encoder(b, &el)),
                None => (false, result_encoder(b, &ret)),
            };
            let error = match sig.as_ref().and_then(|s| s.throws.clone()) {
                Some(t) if t.decl().is_some() && t.decl() != rpc_only => {
                    let d = t.decl().unwrap_or(p);
                    let di = g.view.decl(d);
                    let name = format!("{}.{}", package_of(g, d), di.name);
                    let text = b.text_fn(&t);
                    // A Codable error's fields are its codec's form, which
                    // the client stub decodes with the same codec.
                    let fields = if crate::codec::has_codec(g, d) {
                        let c = crate::codec::codec_expr(b, &t);
                        js::arrow(
                            vec![Param::new("e")],
                            call(
                                member(id("$k"), "payloadFields"),
                                vec![id("e"), member(c, "encode")],
                            ),
                        )
                    } else {
                        member(id("$k"), "payloadFields")
                    };
                    Expr::Object(
                        vec![
                            Prop::KeyValue("name".into(), js::str(name)),
                            Prop::KeyValue("text".into(), text),
                            Prop::KeyValue("fields".into(), fields),
                        ],
                        false,
                    )
                }
                _ => Expr::Null,
            };
            // `(f) => E.rpcFailure(f)`: an `RpcFailure` as the rpc's own
            // error, for in-process calls (`$s.callLocal`).
            let failure = match sig
                .as_ref()
                .and_then(|s| s.throws.clone())
                .and_then(|t| t.decl())
            {
                Some(e) if g.view.decl(e).named_kind() == Some(DeclarationKind::Enum) => {
                    let r = b.decl_ref(e);
                    js::arrow(
                        vec![Param::new("f")],
                        call(member(r, "rpcFailure"), vec![id("f")]),
                    )
                }
                _ => Expr::Null,
            };
            let mut gs = match &pnode {
                Some(n) => in_file(b, p, |b| guards(b, n, &format!("protocol:{}", info.name))),
                None => Vec::new(),
            };
            if let Some(n) = g.member_node(m) {
                gs.extend(in_file(b, p, |b| guards(b, &n, &path)));
            }
            let imp_ref = g.name(imp);
            let method = g.member_name(m);
            let invoke = js::arrow(
                vec![Param::new("c"), Param::new("$c"), Param::new("a")],
                call(
                    member(member(id("c"), imp_ref), method),
                    vec![id("$c"), Expr::Spread(Box::new(id("a")))],
                ),
            );
            let validate = if checks.is_empty() {
                Expr::Null
            } else {
                let mut body = vec![Stmt::Const("f".into(), Expr::Array(Vec::new()))];
                body.extend(checks);
                body.push(Stmt::Return(Some(id("f"))));
                js::arrow_block(
                    vec![Param::new("a"), Param::new("c"), Param::new("$c")],
                    body,
                    true,
                )
            };
            entries.push(Prop::KeyValue(
                path,
                Expr::Object(
                    vec![
                        Prop::KeyValue("guards".into(), Expr::Array(gs)),
                        Prop::KeyValue(
                            "decode".into(),
                            js::arrow(vec![Param::new("j")], Expr::Array(decoders)),
                        ),
                        Prop::KeyValue("validate".into(), validate),
                        Prop::KeyValue("call".into(), invoke),
                        Prop::KeyValue("encode".into(), encode),
                        Prop::KeyValue("error".into(), error),
                        Prop::KeyValue("failure".into(), failure),
                        Prop::KeyValue("stream".into(), Expr::Bool(stream)),
                    ],
                    true,
                ),
            ));
        }
    }
    Expr::Object(entries, true)
}

// ---------------------------------------------------------------- routes

/// The redirects the server answers, as (old URL template, `Page` case):
/// those `routes.lock` records and the automatic one of each page renamed
/// since ([`crate::routelock::redirects`]).
pub(crate) fn lock_redirects(g: &Gen<'_>) -> Vec<(String, String)> {
    crate::routelock::redirects(g)
}

/// The view pages in route-table order (codegen.md §14).
pub(crate) fn ordered_pages(g: &Gen<'_>) -> Vec<DeclId> {
    let mut ordered = crate::app::pages(g);
    ordered.sort_by_key(|&p| {
        let t = crate::app::template(g, p);
        let dynamic = t.matches('{').count();
        (
            dynamic,
            std::cmp::Reverse(t.len()),
            g.view.decl(p).name.clone(),
        )
    });
    ordered
}

/// The server's page table: one `{ name, match, value, render, layouts,
/// guards }` object per view page, in route-table order
/// ([`ordered_pages`]), with the page's SSR renderer and those of its
/// layouts.
fn page_table(b: &mut Body<'_, '_>) -> Expr {
    let g = b.g;
    let mut routes = Vec::new();
    for p in ordered_pages(g) {
        let template = crate::app::template(g, p);
        let name = g.name(p);
        let m = matcher(b, p, &template);
        let value = page_value(b, p);
        let render = b.use_decl(p, format!("{name}$ssr"));
        let layouts: Vec<Expr> = layouts_of(b, p)
            .into_iter()
            .map(|l| {
                let n = g.name(l);
                b.use_decl(l, format!("{n}$ssr"))
            })
            .collect();
        let gs = match g.decl_node(p) {
            Some(n) => in_file(b, p, |b| guards(b, &n, &format!("page:{name}"))),
            None => Vec::new(),
        };
        routes.push(Expr::Object(
            vec![
                Prop::KeyValue(
                    "name".into(),
                    js::str(crate::app::case_name(&g.view.decl(p).name)),
                ),
                Prop::KeyValue("match".into(), m),
                Prop::KeyValue("value".into(), value),
                Prop::KeyValue("render".into(), render),
                Prop::KeyValue("layouts".into(), Expr::Array(layouts)),
                Prop::KeyValue("guards".into(), Expr::Array(gs)),
            ],
            true,
        ));
    }
    Expr::Array(routes)
}

/// The server's redirect table: one `{ match, to }` object per redirect
/// recorded in `routes.lock` whose target page still exists, where `to`
/// builds the current URL from the matched parameters.
fn redirect_table(b: &mut Body<'_, '_>) -> Expr {
    let g = b.g;
    let mut out = Vec::new();
    for (from, to) in lock_redirects(g) {
        let Some(page) = crate::app::pages(g)
            .into_iter()
            .find(|&p| crate::app::case_name(&g.view.decl(p).name) == to)
        else {
            continue;
        };
        let m = matcher(b, page, &from);
        let value = page_value(b, page);
        let href = href_of(b, call(value, vec![id("p")]));
        out.push(Expr::Object(
            vec![
                Prop::KeyValue("match".into(), m),
                Prop::KeyValue("to".into(), js::arrow(vec![Param::new("p")], href)),
            ],
            false,
        ));
    }
    Expr::Array(out)
}

/// `true` if an endpoint page injects `Session`.
pub(crate) fn uses_session(g: &Gen<'_>, d: DeclId) -> bool {
    let session = g.view.lookup("k.auth.Session");
    session.is_some() && injected(g, d).contains(&session.unwrap_or(d))
}

/// The server's endpoint table: one `{ match, methods, guards, session }`
/// object per endpoint page, with a handler per `@Get`, `@Post`, `@Put`,
/// `@Patch` and `@Delete` function, keyed by the upper-case method.
fn endpoint_table(b: &mut Body<'_, '_>) -> Expr {
    let g = b.g;
    let mut out = Vec::new();
    for p in crate::app::endpoints(g) {
        let template = crate::app::template(g, p);
        let name = g.name(p);
        let m = matcher(b, p, &template);
        let mut methods = Vec::new();
        for &mm in &g.view.decl(p).members {
            let mi = g.view.member(mm);
            if mi.kind != MemberKind::Func {
                continue;
            }
            if let Some(v) = ["Get", "Post", "Put", "Patch", "Delete"]
                .iter()
                .find(|v| mi.has_annotation(v))
            {
                let f = b.use_decl(p, format!("{name}${}", g.member_name(mm)));
                methods.push(Prop::KeyValue(v.to_uppercase(), f));
            }
        }
        let gs = match g.decl_node(p) {
            Some(n) => in_file(b, p, |b| guards(b, &n, &format!("endpoint:{name}"))),
            None => Vec::new(),
        };
        out.push(Expr::Object(
            vec![
                Prop::KeyValue("match".into(), m),
                Prop::KeyValue("methods".into(), Expr::Object(methods, false)),
                Prop::KeyValue("guards".into(), Expr::Array(gs)),
                Prop::KeyValue("session".into(), Expr::Bool(uses_session(g, p))),
            ],
            true,
        ));
    }
    Expr::Array(out)
}

// ----------------------------------------------------------------- entry

/// The tag-ordered case names of a stdlib enum (`k.http.HttpBody`).
fn case_names(g: &Gen<'_>, qualified: &str) -> Vec<String> {
    g.view
        .lookup(qualified)
        .map(|d| {
            g.cases(d)
                .iter()
                .map(|&c| g.view.member(c).name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The server entry: containers, the spec, and the call to `serve`.
pub(crate) fn server_entry(g: &Gen<'_>) -> (Vec<Stmt>, BTreeSet<(DeclId, String)>) {
    let app = g
        .own
        .iter()
        .copied()
        .find(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::App));
    let file = app
        .or_else(|| g.own.first().copied())
        .map_or(k_diagnostics::FileId::new(0), |d| g.view.decl(d).file);
    let mut b = Body::new(g, file, Target::Server);
    let mut stmts = containers(&mut b);
    let mut helpers = Vec::new();
    let client = g.client_rendered();
    let pages = if client {
        // Client rendering: every page answers the shell; matching still
        // decides 404s, and the page's guards run before the shell is sent,
        // as before a server render, so the router can leave the first
        // page of the document unchecked (26-auth-and-sessions.md).
        let mut routes = Vec::new();
        for p in ordered_pages(g) {
            let template = crate::app::template(g, p);
            let m = matcher(&mut b, p, &template);
            let value = page_value(&mut b, p);
            let name = g.name(p);
            let gs = match g.decl_node(p) {
                Some(n) => in_file(&mut b, p, |b| guards(b, &n, &format!("page:{name}"))),
                None => Vec::new(),
            };
            routes.push(Expr::Object(
                vec![
                    Prop::KeyValue(
                        "name".into(),
                        js::str(crate::app::case_name(&g.view.decl(p).name)),
                    ),
                    Prop::KeyValue("match".into(), m),
                    Prop::KeyValue("value".into(), value),
                    Prop::KeyValue("render".into(), Expr::Null),
                    Prop::KeyValue("layouts".into(), Expr::Array(Vec::new())),
                    Prop::KeyValue("guards".into(), Expr::Array(gs)),
                ],
                true,
            ));
        }
        Expr::Array(routes)
    } else {
        page_table(&mut b)
    };
    let redirects = redirect_table(&mut b);
    let rpcs = rpc_table(&mut b, &mut helpers);
    let endpoints = endpoint_table(&mut b);
    let app_meta = match app.and_then(|a| g.decl_node(a).map(|n| (a, n))) {
        Some((a, n)) => match metadata_block_calls(&n) {
            Some(calls) => {
                let body = in_file(&mut b, a, |b| meta_calls(b, &calls, &id("m")));
                js::arrow_block(vec![Param::new("m")], body, false)
            }
            None => Expr::Null,
        },
        None => Expr::Null,
    };
    // `@SignInPage(.signIn)`: where a refused `Authenticated` page goes.
    let sign_in = app
        .and_then(|a| g.decl_node(a))
        .and_then(|n| {
            n.children()
                .filter_map(ast::Annotation::cast)
                .find(|a| a.name().is_some_and(|t| t.text() == "SignInPage"))
        })
        .and_then(|a| a.arg_list()?.args().next()?.value())
        .and_then(|v| implicit_name(&v))
        .and_then(|case| {
            crate::app::pages(g)
                .into_iter()
                .find(|&p| crate::app::case_name(&g.view.decl(p).name) == case)
        });
    let sign_in = match sign_in {
        Some(page) => {
            let page_enum = g.view.lookup("k.core.Page");
            let ps = crate::app::params(g, page);
            let takes_page = ps
                .iter()
                .any(|(_, n, t, _)| n == "returnTo" && t.decl() == page_enum);
            let value = match page_enum {
                Some(pe) => {
                    let cases = b.decl_ref(pe);
                    let case = member(cases, crate::app::case_name(&g.view.decl(page).name));
                    if ps.is_empty() {
                        case
                    } else {
                        let args = ps
                            .iter()
                            .map(|(_, n, _, _)| {
                                if takes_page && n == "returnTo" {
                                    id("r")
                                } else {
                                    Expr::Undefined
                                }
                            })
                            .collect();
                        call(case, args)
                    }
                }
                None => Expr::Null,
            };
            let href = href_of(&mut b, value);
            js::arrow(vec![Param::new("r")], href)
        }
        None => Expr::Null,
    };
    let roles = match role_enum(g) {
        Some(e) => {
            let r = b.decl_ref(e);
            Expr::Object(
                g.cases(e)
                    .iter()
                    .map(|&c| {
                        let n = g.view.member(c).name.clone();
                        Prop::KeyValue(n.clone(), member(r.clone(), n))
                    })
                    .collect(),
                false,
            )
        }
        None => Expr::Object(Vec::new(), false),
    };
    let decode_user = match user_id_type(g) {
        Some(u) if crate::codec::has_codec(g, u) => {
            let c = crate::codec::codec_expr(&mut b, &Type::Named(u, Vec::new()));
            js::arrow(
                vec![Param::new("j")],
                call(member(c, "decode"), vec![id("j"), js::str("user")]),
            )
        }
        _ => js::arrow(vec![Param::new("j")], id("j")),
    };
    let http_status: Vec<Expr> = case_names(g, "k.http.HttpStatus")
        .iter()
        .map(|n| status_code(n).map_or(Expr::Null, |c| Expr::Num(f64::from(c))))
        .collect();
    let http_body: Vec<Expr> = case_names(g, "k.http.HttpBody")
        .into_iter()
        .map(js::str)
        .collect();
    let spec = Expr::Object(
        vec![
            Prop::KeyValue("name".into(), js::str(g.project_name())),
            Prop::KeyValue(
                "rendering".into(),
                js::str(if client { "client" } else { "server" }),
            ),
            Prop::KeyValue("pages".into(), pages),
            Prop::KeyValue("redirects".into(), redirects),
            Prop::KeyValue("rpcs".into(), rpcs),
            Prop::KeyValue("endpoints".into(), endpoints),
            Prop::KeyValue(
                "container".into(),
                js::arrow(
                    vec![Param::new("st")],
                    if tests(g) {
                        // A test's requests use its group's server container.
                        Expr::Cond(
                            Box::new(member(id("st"), "test")),
                            Box::new(call(
                                member(member(id("st"), "test"), "container"),
                                vec![id("st")],
                            )),
                            Box::new(Expr::New(Box::new(id("$Request")), vec![id("st")])),
                        )
                    } else {
                        Expr::New(Box::new(id("$Request")), vec![id("st")])
                    },
                ),
            ),
            Prop::KeyValue("appMeta".into(), app_meta),
            Prop::KeyValue("signIn".into(), sign_in),
            Prop::KeyValue(
                "session".into(),
                Expr::Object(
                    vec![
                        Prop::KeyValue("roles".into(), roles),
                        Prop::KeyValue("decodeUser".into(), decode_user),
                    ],
                    false,
                ),
            ),
            Prop::KeyValue("jobs".into(), crate::jobs::table(&mut b)),
            Prop::KeyValue("httpStatus".into(), Expr::Array(http_status)),
            Prop::KeyValue("httpBody".into(), Expr::Array(http_body)),
            Prop::KeyValue("assets".into(), id(ASSETS_PLACEHOLDER)),
        ],
        true,
    );
    stmts.extend(helpers);
    stmts.push(Stmt::Const("$spec".into(), spec));
    if tests(g) {
        // `k test` drives the app in-process (`k:testing/kit`, `run`).
        stmts.extend(crate::suite::server_groups(&mut b));
    } else {
        stmts.push(Stmt::Expr(Expr::Await(Box::new(srt(
            "serve",
            vec![srt("createApp", vec![id("$spec")])],
        )))));
    }
    let _ = rt;
    (stmts, b.uses)
}

/// The JSON text of the asset table for [`ASSETS_PLACEHOLDER`]: the
/// stylesheets and the app chunk of the client files.
pub(crate) fn assets_json(client: &[crate::OutputFile]) -> String {
    let mut styles: Vec<String> = client
        .iter()
        .filter(|f| f.kind == crate::FileKind::Css)
        .map(|f| format!("/assets/{}", f.path))
        .collect();
    styles.sort();
    let scripts: Vec<String> = client
        .iter()
        .filter(|f| f.kind == crate::FileKind::JavaScript && f.path.starts_with("app."))
        .map(|f| format!("/assets/{}", f.path))
        .collect();
    serde_json::json!({ "styles": styles, "scripts": scripts }).to_string()
}

// ----------------------------------------------------------------- config

/// The K type of a config field as manifest text: `Int`, `List<String>`,
/// `Secret<String>`, `Mode?`.
fn type_text(g: &Gen<'_>, t: &Type) -> String {
    match t {
        Type::Optional(inner) => format!("{}?", type_text(g, inner)),
        Type::Named(d, args) => {
            let name = g.view.decl(*d).name.clone();
            if args.is_empty() {
                name
            } else {
                let a: Vec<String> = args.iter().map(|x| type_text(g, x)).collect();
                format!("{name}<{}>", a.join(", "))
            }
        }
        _ => "Unknown".to_owned(),
    }
}

/// The default of a config field as environment text (`5m`, `relaxed`,
/// `red,green`), or `None` for no default or `nil`.
fn default_text(e: &ast::Expr) -> Option<String> {
    match e {
        ast::Expr::StringLit(s) => s.plain_value(),
        ast::Expr::Literal(l) => {
            let t = l.token()?;
            match t.kind() {
                SyntaxKind::NilKw => None,
                _ => Some(t.text().replace('_', "")),
            }
        }
        ast::Expr::ImplicitMemberExpr(m) => m.name().map(|t| t.text().to_owned()),
        ast::Expr::MemberExpr(m) => {
            // `90.seconds`, `30.days`.
            let n = m.receiver()?.syntax().text().to_string().replace('_', "");
            let unit = match m.name()?.text() {
                "milliseconds" | "millisecond" => "ms",
                "seconds" | "second" => "s",
                "minutes" | "minute" => "m",
                "hours" | "hour" => "h",
                "days" | "day" => "d",
                _ => return None,
            };
            Some(format!("{n}{unit}"))
        }
        ast::Expr::ListExpr(l) => {
            let items: Option<Vec<String>> = l
                .syntax()
                .children()
                .filter_map(ast::Expr::cast)
                .map(|x| default_text(&x))
                .collect();
            Some(items?.join(","))
        }
        _ => {
            let text = e.syntax().text().to_string();
            (text == "[]").then(String::new)
        }
    }
}

/// `@Range(lo...hi)` bounds on a field node.
fn range_of(node: &SyntaxNode) -> Option<(i64, i64)> {
    let ann = node
        .children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|t| t.text() == "Range"))?;
    let v = ann.arg_list()?.args().next()?.value()?;
    let ast::Expr::BinaryExpr(be) = v else {
        return None;
    };
    let num = |e: Option<ast::Expr>| -> Option<i64> {
        e?.syntax().text().to_string().replace('_', "").parse().ok()
    };
    Some((num(be.lhs())?, num(be.rhs())?))
}

/// The `@Env` variable of a config field node.
fn env_name(node: &SyntaxNode) -> Option<String> {
    let ann = node
        .children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|t| t.text() == "Env"))?;
    match ann.arg_list()?.args().next()?.value()? {
        ast::Expr::StringLit(s) => s.plain_value(),
        _ => None,
    }
}

/// The `@Env` variable of field `field` of the stdlib config `d`
/// (`k.rpc.ServiceConfig.token` is `K_SERVICE_TOKEN`), read from the
/// stdlib's source; `None` for an app config or an unknown field.
pub(crate) fn stdlib_config_env(g: &Gen<'_>, d: DeclId, field: &str) -> Option<String> {
    let qualified = g.view.qualified_name(d);
    let path = format!("{}.k", qualified.replace('.', "/"));
    let stdlib = k_sema::stdlib::Stdlib::get();
    let file = stdlib.files().iter().find(|f| f.path == path)?;
    let name = g.view.decl(d).name.clone();
    let ident = |n: &SyntaxNode| {
        n.children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::Ident)
            .map(|t| t.text().to_owned())
    };
    let root = file.parse.syntax();
    let config = root
        .children()
        .filter(|c| c.kind() == SyntaxKind::ConfigDecl)
        .find(|c| ident(c).as_deref() == Some(name.as_str()))?;
    let decl = config
        .descendants()
        .filter(|c| c.kind() == SyntaxKind::FieldDecl)
        .find(|f| ident(f).as_deref() == Some(field))?;
    env_name(&decl)
}

/// The stdlib configs the production graph reads: the server's
/// (`ServerConfig`), sessions' when the app signs users in, the database's
/// when it has entities or repositories, and the job worker's when it has
/// jobs.
fn stdlib_configs(g: &Gen<'_>) -> Vec<&'static str> {
    let uses = |q: &str| {
        g.view
            .lookup(q)
            .is_some_and(|t| g.own.iter().any(|&d| injected(g, d).contains(&t)))
    };
    let mut out = vec!["k/server/ServerConfig.k"];
    if uses("k.auth.Session") || uses("k.auth.SessionManager") {
        out.push("k/auth/SessionConfig.k");
    }
    if g.own.iter().any(|&d| {
        matches!(
            g.view.decl(d).named_kind(),
            Some(DeclarationKind::Entity | DeclarationKind::Repository)
        )
    }) {
        out.push("k/db/DatabaseConfig.k");
    }
    if let Some(job) = g.view.lookup("k.server.Job") {
        if g.own.iter().any(|&d| g.implements(d, job)) {
            out.push("k/server/JobConfig.k");
        }
    }
    // `SmtpConfig` only when the reachable graph sends email through the
    // stdlib's SMTP provider (31-email.md, "SMTP configuration").
    let reached = reachable(g);
    let rules = use_rules(g);
    let smtp = g.view.lookup("k.server.SmtpEmailProvider");
    let provider = g.view.lookup("k.server.EmailProvider");
    let sends_smtp = reached.iter().any(|&d| {
        injected(g, d).into_iter().any(|t| {
            Some(t) == smtp
                || (Some(t) == provider
                    && binding(g, &rules, t)
                        .or_else(|| stdlib_default(g, "k.server.EmailProvider"))
                        == smtp)
        })
    });
    if sends_smtp {
        out.push("k/server/SmtpConfig.k");
    }
    // `K_SERVICE_TOKEN` when a protocol the app serves admits only other
    // services (`requires ServiceAuthenticated`).
    let guarded = g.own.iter().any(|&p| {
        g.view.decl(p).named_kind() == Some(DeclarationKind::Protocol)
            && implementation(g, p).is_some()
            && g.decl_node(p).is_some_and(|n| {
                n.descendants()
                    .filter(|c| c.kind() == SyntaxKind::RequiresClause)
                    .any(|c| c.text().to_string().contains("ServiceAuthenticated"))
            })
    });
    if guarded {
        out.push("k/rpc/ServiceConfig.k");
    }
    // Any other stdlib config the app's code injects itself
    // (`@Inject let service: ServiceConfig`).
    let stdlib = k_sema::stdlib::Stdlib::get();
    for &d in &reached {
        for t in injected(g, d) {
            if g.view.decl(t).named_kind() != Some(DeclarationKind::Config) {
                continue;
            }
            let path = format!("{}.k", g.view.qualified_name(t).replace('.', "/"));
            if let Some(file) = stdlib.files().iter().find(|f| f.path == path)
                && !out.contains(&file.path)
            {
                out.push(file.path);
            }
        }
    }
    out
}

/// One manifest entry from a config field's syntax: `ty` is the field's
/// type as written (`Secret<String>?`, `List<String>`).
fn entry_of(
    node: &SyntaxNode,
    declared_at: String,
    cases: Vec<String>,
) -> Option<crate::ConfigEntry> {
    let env = env_name(node)?;
    let ty = node
        .children()
        .find(|c| matches!(c.kind(), SyntaxKind::PathType | SyntaxKind::OptionalType))
        .map(|t| t.text().to_string().split_whitespace().collect::<String>())
        .unwrap_or_default();
    let init = node.children().find_map(ast::Expr::cast);
    let default = init.as_ref().and_then(default_text);
    let optional = ty.ends_with('?');
    let public_origin = env == "K_PUBLIC_ORIGIN";
    Some(crate::ConfigEntry {
        declared_at,
        secret: ty.starts_with("Secret<"),
        required: !optional && init.is_none() && !public_origin,
        default,
        cases,
        range: range_of(node),
        ty,
        env,
    })
}

/// The project declarations the selected environment's dependency graph
/// reaches (docs/design/25-config-and-secrets.md, "Startup validation").
///
/// The roots are every non-test declaration that runs on its own: pages,
/// layouts, components, endpoints, the app, and the services that
/// implement a protocol or a stdlib interface (rpc handlers, jobs).
/// Services, repositories and configs are otherwise reached only through
/// `@Inject`; an injected project interface reaches the implementation its
/// binding chooses ([`binding`]), or every implementation when none is
/// chosen.
fn reachable(g: &Gen<'_>) -> BTreeSet<DeclId> {
    let rules = use_rules(g);
    let injectable = |d: DeclId| {
        matches!(
            g.view.decl(d).named_kind(),
            Some(DeclarationKind::Service | DeclarationKind::Repository | DeclarationKind::Config)
        )
    };
    let runs_alone = |d: DeclId| {
        let info = g.view.decl(d);
        if info.is_test {
            return false;
        }
        if !injectable(d) {
            return true;
        }
        info.named_kind() == Some(DeclarationKind::Service)
            && info.implements.iter().filter_map(Type::decl).any(|i| {
                let iface = g.view.decl(i);
                iface.stdlib || iface.named_kind() == Some(DeclarationKind::Protocol)
            })
    };
    let mut seen: BTreeSet<DeclId> = BTreeSet::new();
    let mut stack: Vec<DeclId> = g.own.iter().copied().filter(|&d| runs_alone(d)).collect();
    while let Some(d) = stack.pop() {
        if !seen.insert(d) {
            continue;
        }
        for t in injected(g, d) {
            let info = g.view.decl(t);
            if info.named_kind() == Some(DeclarationKind::Protocol) {
                // A remote client reads its config (40-workspaces-and-services.md).
                if let Some(c) = remote_binding(g, &rules, t) {
                    stack.push(c);
                }
            }
            if info.named_kind() == Some(DeclarationKind::Interface) && !info.stdlib {
                match binding(g, &rules, t) {
                    Some(imp) => stack.push(imp),
                    None => stack.extend(g.own.iter().copied().filter(|&s| {
                        g.view.decl(s).named_kind() == Some(DeclarationKind::Service)
                            && g.implements(s, t)
                    })),
                }
            } else {
                stack.push(t);
            }
        }
    }
    seen
}

/// The manifest's config entries, one per `@Env` field, sorted by
/// variable, each variable once (25-config-and-secrets.md).
pub(crate) fn config_entries(g: &Gen<'_>) -> Vec<crate::ConfigEntry> {
    let mut out: Vec<crate::ConfigEntry> = Vec::new();
    let mut push = |e: crate::ConfigEntry| {
        if !out.iter().any(|x| x.env == e.env) {
            out.push(e);
        }
    };
    let reached = reachable(g);
    for &c in &g.own {
        let info = g.view.decl(c);
        if info.named_kind() != Some(DeclarationKind::Config) || !reached.contains(&c) {
            continue;
        }
        for &m in &info.members {
            let mi = g.view.member(m);
            if mi.kind != MemberKind::Field {
                continue;
            }
            let Some(node) = g.member_node(m) else {
                continue;
            };
            let t = mi.ty.clone().unwrap_or(Type::Error);
            let mut element = t.unwrap_optional().clone();
            if g.is(&element, g.b.secret) {
                element = element.args().first().cloned().unwrap_or(Type::Error);
            }
            if g.is_list(&element) {
                element = element.args().first().cloned().unwrap_or(Type::Error);
            }
            let cases = element
                .unwrap_optional()
                .decl()
                .filter(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::Enum))
                .map(|d| {
                    g.cases(d)
                        .iter()
                        .map(|&x| g.view.member(x).name.clone())
                        .collect()
                })
                .unwrap_or_default();
            if let Some(mut e) = entry_of(
                &node,
                format!("{}.{}", g.view.qualified_name(c), mi.name),
                cases,
            ) {
                e.ty = type_text(g, &t);
                e.secret = g.is(t.unwrap_optional(), g.b.secret);
                e.required = e.required && !mi.has_default && !matches!(t, Type::Optional(_));
                push(e);
            }
        }
    }
    let stdlib = k_sema::stdlib::Stdlib::get();
    for path in stdlib_configs(g) {
        let Some(file) = stdlib.files().iter().find(|f| f.path == path) else {
            continue;
        };
        let root = file.parse.syntax();
        for config in root
            .children()
            .filter(|c| c.kind() == SyntaxKind::ConfigDecl)
        {
            let name = config
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::Ident)
                .map(|t| t.text().to_owned())
                .next()
                .unwrap_or_default();
            for field in config
                .descendants()
                .filter(|c| c.kind() == SyntaxKind::FieldDecl)
            {
                let fname = field
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident)
                    .map(|t| t.text().to_owned())
                    .unwrap_or_default();
                let at = format!("{}.{name}.{fname}", file.module);
                // An enum field's cases, from the stdlib enum of the
                // config's module (`SmtpSecurity`).
                let ty = field
                    .children()
                    .find(|c| matches!(c.kind(), SyntaxKind::PathType | SyntaxKind::OptionalType))
                    .map(|t| t.text().to_string().trim().trim_end_matches('?').to_owned())
                    .unwrap_or_default();
                let cases = g
                    .view
                    .lookup(&format!("{}.{ty}", file.module))
                    .filter(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::Enum))
                    .map(|d| {
                        g.cases(d)
                            .iter()
                            .map(|&x| g.view.member(x).name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(e) = entry_of(&field, at, cases) {
                    push(e);
                }
            }
        }
    }
    out.sort_by(|a, b| a.env.cmp(&b.env));
    out
}
