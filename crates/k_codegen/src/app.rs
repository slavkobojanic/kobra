//! The application layer of code generation: the generated `Page` and
//! `ThemeVariant` enums, URLs and the route table, `@Load` groups, `@Form`
//! state, RPC paths, repositories as SQL, endpoint pages and the entry
//! modules of both sides (`docs/compiler/codegen.md` §10, §13–§15, §17).

use std::collections::{BTreeMap, BTreeSet};

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, MemberId, MemberKind, Resolved, Type};
use k_syntax::SyntaxKind;
use k_syntax::ast::{self, AstNode};

use crate::cx::{Gen, Target, mangle};
use crate::decl::Item;
use crate::js::{self, Expr, Function, Param, Prop, Stmt, bin, call, id, member, rt};
use crate::lower::{Body, MemberAccess};
use crate::view::VNode;

// ---------------------------------------------------------------- pages

/// The pages of the project (declarations with a view), in a fixed order.
pub(crate) fn pages(g: &Gen<'_>) -> Vec<DeclId> {
    g.own
        .iter()
        .copied()
        .filter(|&d| {
            let info = g.view.decl(d);
            info.named_kind() == Some(DeclarationKind::Page)
                && g.decl_node(d)
                    .is_some_and(|n| n.descendants().any(|c| c.kind() == SyntaxKind::ViewBlock))
        })
        .collect()
}

/// The endpoint pages (pages without a view).
pub(crate) fn endpoints(g: &Gen<'_>) -> Vec<DeclId> {
    g.own
        .iter()
        .copied()
        .filter(|&d| {
            g.view.decl(d).named_kind() == Some(DeclarationKind::Page)
                && !g
                    .decl_node(d)
                    .is_some_and(|n| n.descendants().any(|c| c.kind() == SyntaxKind::ViewBlock))
        })
        .collect()
}

/// The `Page` case name of a page: its name in lower camel case.
pub(crate) fn case_name(name: &str) -> String {
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A page's `@Param`s: `(name, type, has default)`.
pub(crate) fn params(g: &Gen<'_>, page: DeclId) -> Vec<(MemberId, String, Type, bool)> {
    g.view
        .decl(page)
        .members
        .iter()
        .copied()
        .filter(|&m| g.view.member(m).has_annotation("Param"))
        .map(|m| {
            let mi = g.view.member(m);
            (
                m,
                mi.name.clone(),
                mi.ty.clone().unwrap_or(Type::Error),
                mi.has_default,
            )
        })
        .collect()
}

/// The URL template of a page: its `at "…"`, or the derived one
/// (12-routing.md, "Default URLs"): the name in kebab case (`Home` is
/// `/home`; only `at "/"` names the root), required parameters as path
/// segments in declaration order.
pub(crate) fn template(g: &Gen<'_>, page: DeclId) -> String {
    if let Some(node) = g.decl_node(page) {
        if let Some(at) = node.children().find(|c| c.kind() == SyntaxKind::AtClause) {
            if let Some(s) = at.children().find_map(ast::StringLit::cast) {
                if let Some(v) = s.plain_value() {
                    return v;
                }
                // `"/articles/{slug}"` parses as an interpolation; its source
                // text is the template.
                let text = s.syntax().text().to_string();
                let t = text.trim_matches('"');
                if t.starts_with('/') {
                    return t.to_owned();
                }
            }
        }
    }
    let name = &g.view.decl(page).name;
    let mut t = format!("/{}", crate::css::kebab(name));
    for (_, p, _, optional) in params(g, page) {
        if !optional {
            t.push_str(&format!("/{{{p}}}"));
        }
    }
    if t.is_empty() {
        t.push('/');
    }
    t
}

/// The page a `Page` case stands for.
fn page_of_case(g: &Gen<'_>, case: &str) -> Option<DeclId> {
    pages(g)
        .into_iter()
        .find(|&p| case_name(&g.view.decl(p).name) == case)
}

/// A static `href` for a page value without parameters: `.pricing`.
pub(crate) fn static_href(b: &Body<'_, '_>, v: &ast::Expr) -> Option<String> {
    let name = match v {
        ast::Expr::ImplicitMemberExpr(m) => m.name()?,
        ast::Expr::MemberExpr(m) => m.name()?,
        _ => return None,
    };
    let page = page_of_case(b.g, name.text())?;
    if params(b.g, page)
        .iter()
        .any(|(_, _, _, optional)| !optional)
    {
        return None;
    }
    Some(template(b.g, page))
}

/// `Page$href(value)`: the URL of a page value.
pub(crate) fn href_expr(b: &mut Body<'_, '_>, v: &ast::Expr) -> Expr {
    let x = b.expr(v);
    match b.g.view.lookup("k.core.Page") {
        Some(p) => {
            let f = b.use_decl(p, "Page$href".into());
            call(f, vec![x])
        }
        None => x,
    }
}

/// Lowers the generated `Page` enum: one constructor per page with its
/// parameters as fields, `Page$href` and `Page$parse` (codegen.md §14).
pub(crate) fn page_enum(b: &mut Body<'_, '_>) -> Vec<Stmt> {
    let g = b.g;
    // Endpoint pages are `Endpoint` cases, server-only; `Page` has only
    // the pages with a view.
    let all: Vec<DeclId> = pages(g);
    let mut props = Vec::new();
    let mut cases = Vec::new();
    let mut parse_routes = Vec::new();
    let file = b.file;
    for (tag, &p) in all.iter().enumerate() {
        let cname = case_name(&g.view.decl(p).name);
        let ps = params(g, p);
        // Defaults are expressions of the page's own file.
        b.file = g.view.decl(p).file;
        let value = if ps.is_empty() {
            rt("unitCase", vec![Expr::Num(tag as f64)])
        } else {
            let mut obj = vec![Prop::KeyValue("$".into(), Expr::Num(tag as f64))];
            let mut fps = Vec::new();
            for (m, name, _, optional) in &ps {
                let pn = mangle(name);
                let default = if *optional {
                    g.member_node(*m)
                        .and_then(|n| n.children().find_map(ast::Expr::cast))
                        .map(|e| b.expr(&e))
                } else {
                    None
                };
                fps.push(Param {
                    name: pn.clone(),
                    default,
                    rest: false,
                });
                obj.push(Prop::KeyValue(name.clone(), id(pn)));
            }
            js::arrow(fps, Expr::Object(obj, false))
        };
        props.push(Prop::KeyValue(cname.clone(), value));
        // href: the path with required parameters, optionals in the query.
        let tpl = template(g, p);
        let mut pieces: Vec<Expr> = Vec::new();
        let mut rest = tpl.as_str();
        while let Some(i) = rest.find('{') {
            pieces.push(js::str(rest[..i].to_owned()));
            let end = rest[i..].find('}').map_or(rest.len(), |e| i + e);
            let pname = &rest[i + 1..end];
            let t = ps
                .iter()
                .find(|(_, n, _, _)| n == pname)
                .map(|(_, _, t, _)| t.clone())
                .unwrap_or(Type::Error);
            let x = b.text_of(member(id("p"), pname.to_owned()), &t);
            pieces.push(call(id("encodeURIComponent"), vec![x]));
            rest = if end < rest.len() {
                &rest[end + 1..]
            } else {
                ""
            };
        }
        if !rest.is_empty() || pieces.is_empty() {
            pieces.push(js::str(rest.to_owned()));
        }
        let mut query = Vec::new();
        for (m, name, t, optional) in &ps {
            if !*optional {
                continue;
            }
            let default = g
                .member_node(*m)
                .and_then(|n| n.children().find_map(ast::Expr::cast))
                .map_or(Expr::Null, |e| b.expr(&e));
            let x = b.text_of(member(id("p"), name.clone()), t);
            query.push(Expr::Array(vec![
                js::str(format!("{name}=")),
                x,
                member(id("p"), name.clone()),
                default,
            ]));
        }
        let path = pieces
            .into_iter()
            .reduce(|a, c| bin("+", a, c))
            .unwrap_or_else(|| js::str("/"));
        let href = if query.is_empty() {
            path
        } else {
            rt("withQuery", vec![path, Expr::Array(query)])
        };
        b.file = file;
        cases.push(js::Case {
            tests: vec![Expr::Num(tag as f64)],
            body: vec![Stmt::Return(Some(href))],
        });
        let segments: Vec<Expr> = tpl
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| js::str(s.to_owned()))
            .collect();
        parse_routes.push(Expr::Array(vec![js::str(cname), Expr::Array(segments)]));
    }
    cases.push(js::Case {
        tests: Vec::new(),
        body: vec![Stmt::Return(Some(js::str("/")))],
    });
    crate::decl::on_demand(b, vec!["Page$codec".into()], page_codec);
    vec![
        Stmt::ExportConst("Page".into(), Expr::Object(props, true)),
        Stmt::Function(
            Function {
                name: "Page$href".into(),
                params: vec![Param::new("p")],
                body: js::Body::Block(vec![Stmt::Switch(member(id("p"), "$"), cases)]),
                ..Function::default()
            },
            true,
        ),
        Stmt::ExportConst("Page$routes".into(), Expr::Array(parse_routes)),
    ]
}

/// `Page$codec`, emitted only where something encodes or decodes a page
/// (a `SystemNotification`'s `opens`): the wire form of an enum
/// (codegen.md §5.1), a page with parameters as `{"case": "report", "id":
/// …}`, one without as `"home"`.
fn page_codec(b: &mut Body<'_, '_>) -> Vec<Stmt> {
    let g = b.g;
    let mut enc_cases = Vec::new();
    let mut dec_stmts = vec![Stmt::Const(
        "c".into(),
        rt("caseName", vec![id("j"), id("p")]),
    )];
    for (tag, &p) in pages(g).iter().enumerate() {
        let cname = case_name(&g.view.decl(p).name);
        let ps = params(g, p);
        let page = id("Page");
        if ps.is_empty() {
            enc_cases.push(js::Case {
                tests: vec![Expr::Num(tag as f64)],
                body: vec![Stmt::Return(Some(js::str(cname.clone())))],
            });
            dec_stmts.push(Stmt::If(
                bin("===", id("c"), js::str(cname.clone())),
                vec![Stmt::Return(Some(member(page, cname)))],
                None,
            ));
            continue;
        }
        let mut wire = vec![Prop::Quoted("case".into(), js::str(cname.clone()))];
        let mut dargs = Vec::new();
        for (_, name, t, _) in &ps {
            let c = crate::codec::codec_expr(b, t);
            wire.push(Prop::Quoted(
                name.clone(),
                call(
                    member(c.clone(), "encode"),
                    vec![member(id("v"), name.clone())],
                ),
            ));
            dargs.push(call(
                member(c, "decode"),
                vec![
                    Expr::Index(Box::new(id("j")), Box::new(js::str(name.clone()))),
                    bin("+", id("p"), js::str(format!(".{name}"))),
                ],
            ));
        }
        enc_cases.push(js::Case {
            tests: vec![Expr::Num(tag as f64)],
            body: vec![Stmt::Return(Some(Expr::Object(wire, false)))],
        });
        dec_stmts.push(Stmt::If(
            bin("===", id("c"), js::str(cname.clone())),
            vec![Stmt::Return(Some(call(member(page, cname), dargs)))],
            None,
        ));
    }
    enc_cases.push(js::Case {
        tests: Vec::new(),
        body: vec![Stmt::Return(Some(rt("unreachable", vec![])))],
    });
    dec_stmts.push(Stmt::Return(Some(rt(
        "decodeFail",
        vec![id("p"), bin("+", js::str("unknown page "), id("c"))],
    ))));
    vec![Stmt::ExportConst(
        "Page$codec".into(),
        Expr::Object(
            vec![
                Prop::KeyValue(
                    "encode".into(),
                    js::arrow_block(
                        vec![Param::new("v")],
                        vec![Stmt::Switch(member(id("v"), "$"), enc_cases)],
                        false,
                    ),
                ),
                Prop::KeyValue(
                    "decode".into(),
                    js::arrow_block(
                        vec![
                            Param::new("j"),
                            Param {
                                name: "p".into(),
                                default: Some(js::str("")),
                                rest: false,
                            },
                        ],
                        dec_stmts,
                        false,
                    ),
                ),
            ],
            false,
        ),
    )]
}

/// `Endpoint$href(e)`: the path of an `Endpoint` value, the endpoint
/// page's URL template with its path parameters filled in, each encoded
/// (12-routing.md, "Endpoint pages"). The cases are in the enum's order.
pub(crate) fn endpoint_href(b: &mut Body<'_, '_>) -> Stmt {
    let g = b.g;
    let pages = endpoints(g);
    let mut cases = Vec::new();
    let enum_decl = g.view.lookup("k.core.Endpoint");
    let names: Vec<String> = enum_decl
        .map(|e| {
            g.cases(e)
                .iter()
                .map(|&c| g.view.member(c).name.clone())
                .collect()
        })
        .unwrap_or_default();
    for (tag, cname) in names.iter().enumerate() {
        let Some(&p) = pages
            .iter()
            .find(|&&p| case_name(&g.view.decl(p).name) == *cname)
        else {
            continue;
        };
        let ps = params(g, p);
        let tpl = template(g, p);
        let mut pieces: Vec<Expr> = Vec::new();
        let mut rest = tpl.as_str();
        while let Some(i) = rest.find('{') {
            pieces.push(js::str(rest[..i].to_owned()));
            let end = rest[i..].find('}').map_or(rest.len(), |e| i + e);
            let pname = &rest[i + 1..end];
            let t = ps
                .iter()
                .find(|(_, n, _, _)| n == pname)
                .map(|(_, _, t, _)| t.clone())
                .unwrap_or(Type::Error);
            let x = b.text_of(member(id("e"), pname.to_owned()), &t);
            pieces.push(call(id("encodeURIComponent"), vec![x]));
            rest = if end < rest.len() {
                &rest[end + 1..]
            } else {
                ""
            };
        }
        if !rest.is_empty() || pieces.is_empty() {
            pieces.push(js::str(rest.to_owned()));
        }
        let href = pieces
            .into_iter()
            .reduce(|a, c| bin("+", a, c))
            .unwrap_or_else(|| js::str("/"));
        cases.push(js::Case {
            tests: vec![Expr::Num(tag as f64)],
            body: vec![Stmt::Return(Some(href))],
        });
    }
    cases.push(js::Case {
        tests: Vec::new(),
        body: vec![Stmt::Return(Some(js::str("/")))],
    });
    Stmt::Function(
        Function {
            name: "Endpoint$href".into(),
            params: vec![Param::new("e")],
            body: js::Body::Block(vec![Stmt::Switch(member(id("e"), "$"), cases)]),
            ..Function::default()
        },
        true,
    )
}

/// The generated `ThemeVariant` enum: one case per theme variant (plus
/// `base`), each the variant's name.
pub(crate) fn theme_variant_enum(b: &mut Body<'_, '_>) -> Vec<Stmt> {
    let mut props = vec![Prop::KeyValue("base".into(), js::str("base"))];
    for v in crate::css::variants(b.g) {
        props.push(Prop::KeyValue(v.clone(), js::str(v)));
    }
    vec![Stmt::ExportConst(
        "ThemeVariant".into(),
        Expr::Object(props, true),
    )]
}

/// The route table: `[{ name, guards?, parse, value, load }]`
/// (codegen.md §14), with the guards the browser checks on the client.
fn route_table(g: &Gen<'_>, b: &mut Body<'_, '_>, target: Target) -> Expr {
    let mut routes = Vec::new();
    let mut ordered = pages(g);
    ordered.sort_by_key(|&p| {
        let t = template(g, p);
        let dynamic = t.matches('{').count();
        (
            dynamic,
            std::cmp::Reverse(t.len()),
            g.view.decl(p).name.clone(),
        )
    });
    for p in ordered {
        let name = g.name(p);
        let cname = case_name(&g.view.decl(p).name);
        let load = match target {
            Target::Client => {
                // The page with the `k-data` key the server rendered it
                // under (`"0"`), and its layouts, outermost first, which
                // the router renders around it (layout `i` under `"L{i}"`)
                // and keeps mounted while the next page shares them, so
                // hydration claims the whole document and reads each one's
                // loads (runtime.md §10).
                let render = call(
                    member(id("m"), name.clone()),
                    vec![
                        rt("paramProps", vec![id("params")]),
                        Expr::Object(
                            vec![
                                Prop::Spread(id("ctx")),
                                Prop::KeyValue("key".into(), js::str("0")),
                            ],
                            false,
                        ),
                    ],
                );
                let layouts: Vec<Expr> = crate::server::layouts_of(b, p)
                    .iter()
                    .map(|l| b.decl_ref(*l))
                    .collect();
                js::arrow(
                    vec![],
                    call(
                        member(
                            Expr::Import(Box::new(js::str(format!("$page:{}", p.0)))),
                            "then",
                        ),
                        vec![js::arrow(
                            vec![Param::new("m")],
                            Expr::Object(
                                vec![
                                    Prop::KeyValue("layouts".into(), Expr::Array(layouts)),
                                    Prop::KeyValue(
                                        "render".into(),
                                        js::arrow(
                                            vec![Param::new("params"), Param::new("ctx")],
                                            render,
                                        ),
                                    ),
                                ],
                                false,
                            ),
                        )],
                    ),
                )
            }
            Target::Server => b.use_decl(p, format!("{name}$ssr")),
        };
        // The same typed matcher as the server's (`route` in `std.js`),
        // so a parameter is an `Int` on both sides and a URL the server
        // answers with 404 matches no route here either.
        let tpl = template(g, p);
        let file = b.file;
        b.file = g.view.decl(p).file;
        let parse = crate::server::matcher(b, p, &tpl);
        let value = crate::server::page_value(b, p);
        b.file = file;
        let mut props = vec![Prop::KeyValue("name".into(), js::str(cname.clone()))];
        // The guards the browser checks before showing the page
        // (runtime.md §10); an open page has none.
        let guards = match (target, g.decl_node(p)) {
            (Target::Client, Some(n)) => crate::server::client_guards(&n),
            _ => Vec::new(),
        };
        if !guards.is_empty() {
            props.push(Prop::KeyValue("guards".into(), Expr::Array(guards)));
        }
        props.extend([
            Prop::KeyValue("parse".into(), parse),
            Prop::KeyValue("value".into(), value),
            Prop::KeyValue("load".into(), load),
        ]);
        routes.push(Expr::Object(props, false));
    }
    Expr::Array(routes)
}

/// The declarations the client roots of an app reach (codegen.md §3, step
/// 2): its pages, layouts and the `app` itself, then everything their
/// lowered client items use, every `@Inject` dependency, and for an
/// injected interface the implementation bound for the build's
/// environment. The browser's containers are generated from this set.
///
/// An endpoint page (a page without a view, [`endpoints`]) is no root: it
/// runs on the server only, so what only it injects (the shop's
/// `StripeWebhook` and its `StripeEvents` binding) never ships.
pub(crate) fn client_reach(g: &Gen<'_>, items: &BTreeMap<DeclId, Item>) -> BTreeSet<DeclId> {
    let server_only: BTreeSet<DeclId> = endpoints(g).into_iter().collect();
    let roots: Vec<DeclId> = g
        .own
        .iter()
        .copied()
        .filter(|&d| {
            matches!(
                g.view.decl(d).named_kind(),
                Some(DeclarationKind::Page | DeclarationKind::Layout | DeclarationKind::App)
            ) && !server_only.contains(&d)
        })
        .collect();
    reach_from(g, items, roots)
}

/// What [`client_reach`] reaches from `roots` alone: a page and its
/// layouts, or the `app` (the feature chunks a page needs, `bundle.rs`).
pub(crate) fn reach_from(
    g: &Gen<'_>,
    items: &BTreeMap<DeclId, Item>,
    roots: Vec<DeclId>,
) -> BTreeSet<DeclId> {
    let rules = crate::server::use_rules(g);
    let mut reached: BTreeSet<DeclId> = BTreeSet::new();
    let mut work = roots;
    while let Some(d) = work.pop() {
        if !reached.insert(d) {
            continue;
        }
        let mut next: Vec<DeclId> = crate::server::injected(g, d);
        next.extend(form_dependencies(g, d));
        next.extend(native_component_dependencies(g, d));
        if let Some(item) = items.get(&d) {
            next.extend(item.uses.iter().map(|(u, _)| *u));
        }
        if g.view.decl(d).named_kind() == Some(DeclarationKind::Interface) {
            next.extend(crate::server::binding(g, &rules, d));
        }
        work.extend(next.into_iter().filter(|u| !reached.contains(u)));
    }
    reached
}

/// The client entry (`app.[hash].js`): the dependency containers, then
/// hydrate the server's HTML, or render for `@Rendering(.client)`, and
/// start the router.
pub(crate) fn client_entry(
    g: &Gen<'_>,
    reach: &BTreeSet<DeclId>,
) -> (Vec<Stmt>, BTreeSet<(DeclId, String)>) {
    let file = g
        .own
        .first()
        .map_or(k_diagnostics::FileId::new(0), |&d| g.view.decl(d).file);
    let mut b = Body::new(g, file, Target::Client);
    let routes = route_table(g, &mut b, Target::Client);
    let mut stmts = vec![Stmt::Const("routes".into(), routes)];
    let (classes, per_route) = client_containers(&mut b, reach, &[]);
    stmts.extend(classes);
    let container = call(
        member(id("document"), "getElementById"),
        vec![js::str("k-app")],
    );
    stmts.push(Stmt::Const("container".into(), container));
    // The `k-data` payload of the document: a server-rendered one's first
    // page hydrates from it, and `Clock` and `Session` start from it. A
    // client-rendered app's shell carries only the session mirror, and a
    // static-only app's none (`readData` is then `null`).
    stmts.push(Stmt::Const(
        "data".into(),
        rt("readData", vec![id("document")]),
    ));
    stmts.push(Stmt::Const(
        "app".into(),
        Expr::New(Box::new(id("$App")), vec![id("data")]),
    ));
    // Each mounted page gets the app's container, or a `$Route` container
    // over it when the app has `@Scope(.route)` services.
    let make = if per_route {
        js::arrow(vec![], Expr::New(Box::new(id("$Route")), vec![id("app")]))
    } else {
        js::arrow(vec![], id("app"))
    };
    let mut options = vec![Prop::KeyValue("c".into(), make)];
    if g.client_rendered() {
        // The shell's mirror: the router checks the guards against it, as
        // in a server-rendered app, but there is nothing to hydrate.
        options.push(Prop::KeyValue("session".into(), id("data")));
    } else {
        // The router hydrates the first page once its chunk has loaded:
        // its templates claim the server's nodes and its loads start from
        // `k-data` (runtime.md §6, §10).
        options.push(Prop::KeyValue("data".into(), id("data")));
    }
    stmts.push(Stmt::Expr(rt(
        "startRouter",
        vec![id("routes"), id("container"), Expr::Object(options, false)],
    )));
    (stmts, b.uses)
}

/// The browser's dependency containers (codegen.md §13): `$App` holds the
/// `@Scope(.app)` services, the protocols' RPC clients, the bound
/// interfaces and the stdlib's platform services, one instance each per
/// app; `$Route`, generated only when the app has `@Scope(.route)`
/// services, extends it with one instance of each per mounted page. A
/// `@Scope(.component)` getter makes a new instance on every read; the
/// component that injects it provides that instance to its subtree
/// (`$k.provide`).
///
/// Every getter of `$App` memoizes on `this.$a`, the app container, and
/// resolves dependencies there, so reading an app service through a
/// `$Route` (or a provided container) still yields the app's instance.
///
/// Only the services in `reach` ([`client_reach`]) get a getter: a
/// service that only server code injects, such as an interface's
/// implementation bound for the server, or one nothing injects, is not in
/// the browser's containers and so not in any client file (codegen.md §16).
///
/// `extra` are further types something injects directly (a test group's
/// `@Inject` fields and `use` operands, `crate::suite`). In the `.test`
/// environment an interface without a binding gets the stdlib's
/// `@TestDefault` for it, and `Clock` is the shared `TestClock`
/// (21-testing.md, "Test doubles and defaults").
///
/// Returns the classes and whether `$Route` exists.
pub(crate) fn client_containers(
    b: &mut Body<'_, '_>,
    reach: &BTreeSet<DeclId>,
    extra: &[DeclId],
) -> (Vec<Stmt>, bool) {
    use crate::js::{Class, Function, MethodKind};
    use crate::server::{binding, getter, injected, scope_of, this, use_rules};
    let g = b.g;
    let services: Vec<DeclId> = g
        .own
        .iter()
        .copied()
        .filter(|&d| {
            g.view.decl(d).named_kind() == Some(DeclarationKind::Service)
                && reach.contains(&d)
                && crate::bundle::allowed(g, d, Target::Client)
        })
        .collect();
    let app_memo = |name: &str, value: Expr| -> Expr {
        Expr::Assign(
            "??=",
            Box::new(member(this("$a"), format!("${name}"))),
            Box::new(value),
        )
    };
    let deps_of = |b: &mut Body<'_, '_>, s: DeclId, from: &dyn Fn(String) -> Expr| -> Expr {
        let _ = b;
        Expr::Object(
            injected(g, s)
                .into_iter()
                .map(|t| {
                    let n = g.name(t);
                    Prop::KeyValue(n.clone(), from(n))
                })
                .collect(),
            false,
        )
    };
    let mut app: Vec<Function> = vec![Function {
        kind: MethodKind::Constructor,
        params: vec![Param::new("d")],
        body: js::Body::Block(vec![
            Stmt::Expr(js::assign(this("$a"), Expr::This)),
            Stmt::Expr(js::assign(this("$d"), id("d"))),
        ]),
        ..Function::default()
    }];
    let mut route: Vec<Function> = Vec::new();
    let mut provided: BTreeSet<String> = BTreeSet::new();
    for &s in &services {
        let name = g.name(s);
        let class = b.decl_ref(s);
        match scope_of(g, s) {
            "route" => {
                let deps = deps_of(b, s, &|n| this(n));
                let construct = Expr::New(Box::new(class), vec![deps]);
                route.push(getter(name.clone(), crate::server::memo(&name, construct)));
            }
            "component" => {
                let deps = deps_of(b, s, &|n| this(n));
                app.push(getter(name.clone(), Expr::New(Box::new(class), vec![deps])));
            }
            _ => {
                let deps = deps_of(b, s, &|n| member(this("$a"), n));
                // An app-scoped service lives as long as the app, so what
                // its constructor creates (an `@Derived` computed, an
                // effect) has no owner: built while a page renders, it
                // would otherwise be disposed with that page.
                let construct = rt(
                    "runWithOwner",
                    vec![
                        Expr::Null,
                        js::arrow(vec![], Expr::New(Box::new(class), vec![deps])),
                    ],
                );
                app.push(getter(name.clone(), app_memo(&name, construct)));
            }
        }
        provided.insert(name);
    }
    // Everything else that client code injects: bound interfaces and the
    // stdlib's services.
    let mut wanted: Vec<DeclId> = Vec::new();
    for &d in &g.own {
        if !reach.contains(&d) || !crate::bundle::allowed(g, d, Target::Client) {
            continue;
        }
        for t in injected(g, d)
            .into_iter()
            .chain(form_dependencies(g, d))
            .chain(native_component_dependencies(g, d))
        {
            if !wanted.contains(&t) {
                wanted.push(t);
            }
        }
    }
    for &t in extra {
        if !wanted.contains(&t) {
            wanted.push(t);
        }
    }
    let testing = g.options.environment == crate::Environment::Test;
    // Protocols: the generated RPC client, for the protocols client code
    // injects. A protocol only server code calls (a `remote(...)` client
    // holding a service token, or one the server serves to other
    // services) gets no browser client: it would only post to paths the
    // browser can't use. Tests keep every client.
    for &p in &g.own {
        if g.view.decl(p).named_kind() != Some(DeclarationKind::Protocol)
            || !(testing || wanted.contains(&p))
        {
            continue;
        }
        let name = g.name(p);
        let client = b.use_decl(p, format!("{name}$client"));
        app.push(getter(
            name.clone(),
            app_memo(&name, Expr::New(Box::new(client), vec![])),
        ));
        provided.insert(name);
    }
    let rules = use_rules(g);
    let mut i = 0;
    while i < wanted.len() {
        let t = wanted[i];
        i += 1;
        let name = g.name(t);
        if provided.contains(&name) {
            continue;
        }
        provided.insert(name.clone());
        let qualified = g.view.qualified_name(t);
        let body = if g.view.decl(t).named_kind() == Some(DeclarationKind::Interface) {
            // The app's `use` rule or single implementation, else the
            // stdlib's default implementation.
            let imp = binding(g, &rules, t).or_else(|| {
                if testing {
                    if let Some(d) = test_default(g, t) {
                        return Some(d);
                    }
                }
                let default = match qualified.as_str() {
                    "k.storage.LocalStorage" => "k.storage.BrowserStorage",
                    "k.http.HttpClient" => "k.http.FetchHttpClient",
                    "k.http.WebSocketClient" => "k.http.StandardWebSocketClient",
                    "k.device.Files" => "k.device.BrowserFiles",
                    "k.device.Clipboard" => "k.device.BrowserClipboard",
                    "k.device.Share" => "k.device.BrowserShare",
                    "k.device.Geolocation" => "k.device.BrowserGeolocation",
                    "k.media.MediaDevices" => "k.media.BrowserMediaDevices",
                    "k.media.Recorder" => "k.media.BrowserRecorder",
                    "k.media.Audio" => "k.media.BrowserAudio",
                    "k.rtc.PeerConnections" => "k.rtc.BrowserPeerConnections",
                    "k.device.TabMessaging" => "k.device.BrowserTabMessaging",
                    "k.device.Notifications" => "k.device.BrowserNotifications",
                    "k.device.PushSubscriptions" => "k.device.BrowserPushSubscriptions",
                    "k.crypto.KeyStore" => "k.crypto.BrowserKeyStore",
                    _ => return None,
                };
                g.view.lookup(default)
            });
            match imp {
                Some(imp) => {
                    if !wanted.contains(&imp) {
                        wanted.push(imp);
                    }
                    member(this("$a"), g.name(imp))
                }
                None => rt("native", vec![js::str(qualified)]),
            }
        } else if let Some(double) = testing
            .then(|| test_default(g, t))
            .flatten()
            .filter(|&d| d != t)
        {
            // A stdlib service with a `k.test` double (`Clock`, and the
            // environment services of 45-web-platform.md): one instance per
            // test, whichever name reaches it.
            if !wanted.contains(&double) {
                wanted.push(double);
            }
            member(this("$a"), g.name(double))
        } else {
            app_memo(&name, platform_service(b, &qualified))
        };
        app.push(getter(name, body));
    }
    let mut out = vec![Stmt::Class(
        Class {
            name: "$App".into(),
            extends: None,
            methods: app,
        },
        false,
    )];
    let per_route = !route.is_empty();
    if per_route {
        let mut methods = vec![Function {
            kind: MethodKind::Constructor,
            params: vec![Param::new("a")],
            body: js::Body::Block(vec![
                Stmt::Expr(call(
                    Expr::Ident("super".into()),
                    vec![member(id("a"), "$d")],
                )),
                Stmt::Expr(js::assign(this("$a"), id("a"))),
            ]),
            ..Function::default()
        }];
        methods.extend(route);
        out.push(Stmt::Class(
            Class {
                name: "$Route".into(),
                extends: Some(id("$App")),
                methods,
            },
            false,
        ));
    }
    (out, per_route)
}

/// The stdlib service marked `@TestDefault(replaces: iface)`: the
/// hermetic default of `iface` in `k test` (`MemoryStorage` for
/// `LocalStorage`, `TestHttpClient` for `HttpClient`, …).
fn test_default(g: &Gen<'_>, iface: DeclId) -> Option<DeclId> {
    (0..g.view.decls.len())
        .map(|i| DeclId(u32::try_from(i).unwrap_or(u32::MAX)))
        .find(|&d| {
            let info = g.view.decl(d);
            info.stdlib
                && info.named_kind() == Some(DeclarationKind::Service)
                && info.has_annotation("TestDefault")
                && g.implements(d, iface)
        })
}

/// References to the generated stdlib enums `names` (whose cases a
/// platform service throws or returns), `null` for one that doesn't exist.
fn enums(b: &mut Body<'_, '_>, names: &[&str]) -> Vec<Expr> {
    names
        .iter()
        .map(|n| match b.g.view.lookup(n) {
            Some(d) => b.decl_ref(d),
            None => Expr::Null,
        })
        .collect()
}

/// The browser body of a stdlib service by qualified name (`services.js`),
/// or `$k.native(name)`, which panics with the name when used.
fn platform_service(b: &mut Body<'_, '_>, qualified: &str) -> Expr {
    let data = member(crate::server::this("$a"), "$d");
    match qualified {
        "k.time.Clock" => rt("clock", vec![data]),
        "k.ui.Theme" => rt("theme", vec![]),
        "k.auth.Session" => rt("session", vec![data]),
        "k.http.FetchHttpClient" => rt(
            "fetchHttpClient",
            enums(b, &["k.http.HttpError", "k.http.HttpStatus"]),
        ),
        // Keys are namespaced per app (36-local-storage.md).
        "k.storage.BrowserStorage" => {
            let mut args = enums(
                b,
                &["k.storage.StorageReadError", "k.storage.StorageWriteError"],
            );
            args.push(js::str(b.g.storage_namespace()));
            rt("browserStorage", args)
        }
        // The files chunk's (runtime.md §12.4), imported by the entry.
        // The feature chunks' (runtime.md §12.4, §12.6, §12.7), imported by the
        // entry.
        "k.device.BrowserFiles"
        | "k.device.BrowserClipboard"
        | "k.device.BrowserShare"
        | "k.device.BrowserGeolocation"
        | "k.media.BrowserMediaDevices"
        | "k.media.BrowserRecorder"
        | "k.media.BrowserAudio"
        | "k.rtc.BrowserPeerConnections"
        | "k.device.BrowserPushSubscriptions"
        | "k.device.PageVisibility"
        | "k.device.Connectivity"
        | "k.device.Viewport" => match b.g.view.lookup(qualified) {
            Some(d) => {
                let name = b.g.name(d);
                call(b.use_decl(d, name), vec![])
            }
            None => rt("native", vec![js::str(qualified)]),
        },
        // The websocket chunk's (runtime.md §12.13), given the enums it
        // builds values of.
        "k.http.StandardWebSocketClient" => match b.g.view.lookup(qualified) {
            Some(d) => {
                let name = b.g.name(d);
                let args = enums(
                    b,
                    &[
                        "k.http.WebSocketError",
                        "k.http.WebSocketMessage",
                        "k.http.WebSocketState",
                    ],
                );
                call(b.use_decl(d, name), args)
            }
            None => rt("native", vec![js::str(qualified)]),
        },
        // The tabs chunk's (runtime.md §12.10): channels are namespaced per
        // app, as storage keys are, and a message carries the build that
        // posted it, which the entry's URL names (it is hashed from the
        // names of every other client file).
        "k.device.BrowserTabMessaging" => match b.g.view.lookup(qualified) {
            Some(d) => {
                let name = b.g.name(d);
                call(
                    b.use_decl(d, name),
                    vec![js::str(b.g.storage_namespace()), id("import.meta.url")],
                )
            }
            None => rt("native", vec![js::str(qualified)]),
        },
        // The notifications chunk's (runtime.md §12.12): a notification's
        // `opens` page becomes the URL its click opens with `Page$href`.
        "k.device.BrowserNotifications" => match b.g.view.lookup(qualified) {
            Some(d) => {
                let name = b.g.name(d);
                let href = match b.g.view.lookup("k.core.Page") {
                    Some(p) => b.use_decl(p, "Page$href".into()),
                    None => Expr::Null,
                };
                call(b.use_decl(d, name), vec![href])
            }
            None => rt("native", vec![js::str(qualified)]),
        },
        // The crypto chunk's (33-crypto.md, "Keeping keys in the browser"):
        // its IndexedDB database is namespaced per app, as storage keys are.
        "k.crypto.BrowserKeyStore" => match b.g.view.lookup(qualified) {
            Some(d) => {
                let name = b.g.name(d);
                call(b.use_decl(d, name), vec![js::str(b.g.storage_namespace())])
            }
            None => rt("native", vec![js::str(qualified)]),
        },
        // The environment chunk's (runtime.md §12.8): during hydration the
        // preferences the server rendered with (`k-data`'s `p`).
        "k.device.Preferences" => match b.g.view.lookup(qualified) {
            Some(d) => {
                let name = b.g.name(d);
                call(b.use_decl(d, name), vec![data])
            }
            None => rt("native", vec![js::str(qualified)]),
        },
        // The test's one clock, shared with its server container.
        "k.test.TestClock" => crate::suite::kit_call("sharedClock", vec![]),
        "k.test.TestFiles" => crate::suite::kit_call("testFiles", vec![]),
        "k.test.TestClipboard" => crate::suite::kit_call("testClipboard", vec![]),
        "k.test.TestShare" => crate::suite::kit_call("testShare", vec![]),
        "k.test.TestGeolocation" => crate::suite::kit_call("testGeolocation", vec![]),
        // The media doubles share the test's clock (46-media-and-rtc.md,
        // "Tests"; runtime.md §12.14).
        "k.test.TestMediaDevices" => crate::suite::kit_call("testMediaDevices", vec![]),
        "k.test.TestRecorder" => crate::suite::kit_call("testRecorder", vec![]),
        "k.test.TestAudio" => crate::suite::kit_call("testAudio", vec![]),
        // The in-process media engine, on the test's clock
        // (46-media-and-rtc.md, "Loopback peers").
        "k.test.TestPeerConnections" => crate::suite::kit_call("testPeerConnections", vec![]),
        "k.test.TestTabMessaging" => crate::suite::kit_call("testTabMessaging", vec![]),
        // `click` opens the notification's page with `Page$href`.
        "k.test.TestNotifications" => {
            let href = match b.g.view.lookup("k.core.Page") {
                Some(p) => b.use_decl(p, "Page$href".into()),
                None => Expr::Null,
            };
            crate::suite::kit_call("testNotifications", vec![href])
        }
        "k.test.TestPushSubscriptions" => crate::suite::kit_call("testPushSubscriptions", vec![]),
        "k.test.TestWakeLock" => crate::suite::kit_call("testWakeLock", vec![]),
        "k.test.MemoryKeyStore" => crate::suite::kit_call("memoryKeyStore", vec![]),
        "k.test.TestPageVisibility" => crate::suite::kit_call("testPageVisibility", vec![]),
        "k.test.TestConnectivity" => crate::suite::kit_call("testConnectivity", vec![]),
        // The test's in-process channel socket and its other browsers
        // (44-channels.md, "Testing").
        "k.test.TestChannels" => crate::suite::kit_call("testChannels", vec![]),
        "k.test.TestPeers" => crate::suite::kit_call("testPeers", vec![]),
        "k.test.TestPreferences" => crate::suite::kit_call("testPreferences", vec![]),
        "k.test.TestViewport" => crate::suite::kit_call("testViewport", vec![]),
        "k.test.MemoryStorage" => crate::suite::kit_call(
            "memoryStorage",
            enums(
                b,
                &["k.storage.StorageReadError", "k.storage.StorageWriteError"],
            ),
        ),
        "k.test.TestHttpClient" => crate::suite::kit_call(
            "testHttpClient",
            enums(b, &["k.http.HttpError", "k.http.HttpStatus"]),
        ),
        // One per test, shared with the test's server (44-channels.md,
        // "Testing").
        "k.test.TestWebSocketClient" => crate::suite::kit_call(
            "testWebSocketClient",
            enums(
                b,
                &[
                    "k.http.WebSocketError",
                    "k.http.WebSocketMessage",
                    "k.http.WebSocketState",
                ],
            ),
        ),
        "k.test.RecordingJobQueue" => crate::suite::kit_call("recordingJobQueue", vec![]),
        "k.test.RecordingEmailProvider" => crate::suite::kit_call("recordingEmailProvider", vec![]),
        _ => rt("native", vec![js::str(qualified)]),
    }
}

/// An endpoint page's handlers and its table entry (server only).
pub(crate) fn endpoint(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let g = b.g;
    let info = g.view.decl(d).clone();
    let mut out = Vec::new();
    let mut methods = Vec::new();
    // Handlers receive `$e`: the parsed `@Param`s and the request
    // container that `@Inject`s read from (server.md §10). The page's
    // functions are module functions taking `$e` first too.
    for &m in &info.members {
        let mi = g.view.member(m);
        if mi.kind != MemberKind::Field && mi.kind != MemberKind::Func {
            continue;
        }
        if mi.has_annotation("Param") {
            b.access.insert(
                m,
                MemberAccess::Raw(member(member(id("$e"), "params"), mi.name.clone())),
            );
        } else if mi.has_annotation("Inject") {
            let tn = mi
                .ty
                .as_ref()
                .and_then(Type::decl)
                .map(|t| g.name(t))
                .unwrap_or_default();
            b.access
                .insert(m, MemberAccess::Raw(member(member(id("$e"), "c"), tn)));
        } else if mi.kind == MemberKind::Func {
            // A handler calling the page's own function: the generated
            // `Page$fn($e, …)` with this request's `$e` bound first.
            let name = format!("{}${}", g.name(d), g.member_name(m));
            b.access.insert(
                m,
                MemberAccess::Raw(call(member(id(name), "bind"), vec![Expr::Null, id("$e")])),
            );
        }
    }
    for &m in &info.members {
        let mi = g.view.member(m).clone();
        if mi.kind != MemberKind::Func {
            continue;
        }
        let verb = ["Get", "Post", "Put", "Patch", "Delete"]
            .iter()
            .find(|v| mi.has_annotation(v));
        let node = g.member_node(m);
        let name = format!("{}${}", g.name(d), g.member_name(m));
        let f = crate::decl::function(
            b,
            name.clone(),
            node.as_ref(),
            mi.signature.as_ref(),
            vec![Param::new("$e")],
        );
        out.push(Stmt::Function(f, true));
        if let Some(v) = verb {
            methods.push(Prop::KeyValue(v.to_uppercase(), id(name)));
        }
    }
    out.push(Stmt::ExportConst(
        format!("{}$endpoint", g.name(d)),
        Expr::Object(
            vec![
                Prop::KeyValue("template".into(), js::str(template(g, d))),
                Prop::KeyValue("methods".into(), Expr::Object(methods, false)),
            ],
            false,
        ),
    ));
    out
}

/// `true` when a server renderer awaits children (components with loads).
pub(crate) fn has_async_children(g: &Gen<'_>, nodes: &[VNode]) -> bool {
    nodes.iter().any(|n| match n {
        VNode::Component(c) => crate::ui::has_loads(g, c.decl),
        VNode::Elem(e) => has_async_children(g, &e.children),
        _ => false,
    })
}

// ----------------------------------------------------------------- loads

/// The loads a `@Load` initializer reads.
fn load_deps(b: &Body<'_, '_>, init: &ast::Expr, loads: &[MemberId]) -> Vec<String> {
    let mut out = Vec::new();
    for n in init.syntax().descendants() {
        if n.kind() != SyntaxKind::NameExpr {
            continue;
        }
        if let Some(Resolved::Member(m)) = b.g.resolved(b.file, n.text_range()) {
            if loads.contains(&m) {
                let name = b.g.view.member(m).name.clone();
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// The reactive reads of a load's initializer without its request: each
/// `@Param`, `@Prop`, `@State` or `@Derived` member of the declaration it
/// names, so a hydrated group knows its inputs (`inputs` of the spec).
fn load_inputs(b: &mut Body<'_, '_>, init: &ast::Expr, loads: &[MemberId]) -> Vec<Expr> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for n in init.syntax().descendants() {
        if n.kind() != SyntaxKind::NameExpr {
            continue;
        }
        let Some(Resolved::Member(m)) = b.g.resolved(b.file, n.text_range()) else {
            continue;
        };
        if loads.contains(&m) || seen.contains(&m) {
            continue;
        }
        let mi = b.g.view.member(m);
        if !["Param", "Prop", "State", "Derived"]
            .iter()
            .any(|a| mi.has_annotation(a))
        {
            continue;
        }
        let Some(e) = ast::Expr::cast(n.clone()) else {
            continue;
        };
        seen.push(m);
        let (pre, x) = b.capture(|b| b.expr(&e));
        if pre.is_empty() {
            out.push(x);
        }
    }
    out
}

/// The local holding a component instance's `k-data` key (`load_key_init`).
pub(crate) const LOAD_KEY: &str = "$lk";

/// `true` if `d`'s `@Load` values are serialized under a key of their own
/// per instance: a component's. A page's and a layout's key is the one the
/// router gives them (`"0"`, `"L{i}"`); every component below them shares
/// that `ctx`, so each instance draws `"{key}.{n}"` in render order, which
/// the server and the hydrating browser walk alike.
pub(crate) fn is_instance_keyed(g: &Gen<'_>, d: DeclId) -> bool {
    g.view.decl(d).named_kind() == Some(DeclarationKind::Component)
}

/// `const $lk = out.loadKey(ctx)` on the server, a component instance's
/// `k-data` key, drawn before its loads run; `const $lk =
/// $k.loadCtx(ctx)` in the browser, `ctx` with that key for `loadsOf`.
pub(crate) fn load_key_init(target: Target) -> Stmt {
    let x = match target {
        Target::Server => call(member(id("out"), "loadKey"), vec![id("ctx")]),
        Target::Client => rt("loadCtx", vec![id("ctx")]),
    };
    Stmt::Const(LOAD_KEY.into(), x)
}

/// Where `d`'s serialized loads go in `k-data`: its instance key, or the
/// page's or layout's `ctx.key`.
pub(crate) fn load_key(g: &Gen<'_>, d: DeclId) -> Expr {
    if is_instance_keyed(g, d) {
        id(LOAD_KEY)
    } else {
        bin("??", member(id("ctx"), "key"), js::str("0"))
    }
}

/// The client load group: `$k.createLoads([{ name, after, run }],
/// serialized, failed)`. With a `failed` block (`error`, the type it
/// receives), a server render that failed serialized the error, which
/// hydration decodes with the error's codec so the group starts FAILED
/// and the block claims the server's markup (14-async-data.md).
pub(crate) fn client_loads(
    b: &mut Body<'_, '_>,
    d: DeclId,
    loads: &[MemberId],
    error: Option<&Type>,
    hydrates: bool,
) -> Expr {
    let mut specs = Vec::new();
    for &m in loads {
        let mi = b.g.view.member(m).clone();
        let init =
            b.g.member_node(m)
                .and_then(|n| n.children().find_map(ast::Expr::cast));
        let Some(init) = init else { continue };
        let deps = load_deps(b, &init, loads);
        // Inside `run`, other loads are read from its values.
        let saved: Vec<(MemberId, Option<MemberAccess>)> = loads
            .iter()
            .map(|&l| (l, b.access.get(&l).cloned()))
            .collect();
        for &l in loads {
            let n = b.g.view.member(l).name.clone();
            b.access.insert(l, MemberAccess::Raw(member(id("v"), n)));
        }
        let saved_c = b.has_c;
        b.has_c = true;
        let (pre, x) = b.capture(|b| b.expr(&init));
        b.has_c = saved_c;
        for (l, a) in saved {
            match a {
                Some(a) => {
                    b.access.insert(l, a);
                }
                None => {
                    b.access.remove(&l);
                }
            }
        }
        // The load maps to its initializer: a breakpoint there stops each
        // time the load runs (codegen.md §18).
        let loc = b.loc(init.syntax());
        let mut body: Vec<Stmt> = pre
            .into_iter()
            .map(|s| match loc {
                Some(l) if !matches!(s, Stmt::At(..)) => Stmt::At(l, Box::new(s)),
                _ => s,
            })
            .collect();
        body.push(Stmt::Return(Some(js::at(loc, x))));
        let run = js::arrow_block(vec![Param::new("v"), Param::new("$c")], body, true);
        let mut spec = vec![
            Prop::KeyValue("name".into(), js::str(mi.name.clone())),
            Prop::KeyValue(
                "after".into(),
                Expr::Array(deps.into_iter().map(js::str).collect()),
            ),
            Prop::KeyValue("run".into(), run),
        ];
        // `@Load(keepOnReload: true)`: `reload()` keeps its value while
        // its inputs are unchanged (14-async-data.md, "Reloading").
        let keep = b.g.member_node(m).is_some_and(|n| {
            n.children()
                .filter_map(ast::Annotation::cast)
                .filter(|a| a.name().is_some_and(|t| t.text() == "Load"))
                .filter_map(|a| a.arg_list())
                .flat_map(|l| l.args().collect::<Vec<_>>())
                .any(|x| {
                    x.label().is_some_and(|t| t.text() == "keepOnReload")
                        && x.value().is_some_and(|v| v.syntax().text() == "true")
                })
        });
        if keep {
            spec.push(Prop::KeyValue("keep".into(), Expr::Bool(true)));
        }
        // A hydrated group runs nothing, so it watches these reads to
        // know when the load's inputs change (runtime.md §7).
        let reads = load_inputs(b, &init, loads);
        if !reads.is_empty() {
            spec.push(Prop::KeyValue(
                "inputs".into(),
                js::arrow(vec![], Expr::Array(reads)),
            ));
        }
        specs.push(Expr::Object(spec, false));
    }
    // A client-rendered component's loads never ran on the server
    // (47-client-rendered-components.md).
    if !hydrates {
        return rt("createLoads", vec![Expr::Array(specs), Expr::Null]);
    }
    // The server serialized the values with their codecs (`ssr_loads`);
    // hydration decodes them back, so a `Money` is a `Money` again.
    let mut decoded = Vec::new();
    for &m in loads {
        let mi = b.g.view.member(m).clone();
        let t = mi.ty.clone().unwrap_or(Type::Error);
        let c = crate::codec::codec_expr(b, &t);
        decoded.push(Prop::KeyValue(
            mi.name.clone(),
            call(
                member(c, "decode"),
                vec![member(id("$v"), mi.name.clone()), js::str(mi.name.clone())],
            ),
        ));
    }
    let instance = is_instance_keyed(b.g, d);
    let keyed = |which: &str| {
        let mut args = vec![id(if instance { LOAD_KEY } else { "ctx" })];
        if which != "loads" {
            args.push(js::str(which));
        }
        rt("loadsOf", args)
    };
    let serialized = call(
        js::arrow(
            vec![Param::new("$v")],
            bin("&&", id("$v"), Expr::Object(decoded, false)),
        ),
        vec![keyed("loads")],
    );
    let Some(error) = error else {
        return rt("createLoads", vec![Expr::Array(specs), serialized]);
    };
    // The server wrote the error with its codec (`ssr_failed`), or as its
    // structural JSON for an error that isn't Codable.
    let decoded = match error.decl() {
        Some(e) if crate::codec::has_codec(b.g, e) => {
            let c = crate::codec::codec_expr(b, error);
            call(member(c, "decode"), vec![id("$f"), js::str("failed")])
        }
        _ => id("$f"),
    };
    let failed = call(
        js::arrow(
            vec![Param::new("$f")],
            Expr::Cond(
                Box::new(bin("==", id("$f"), Expr::Null)),
                Box::new(Expr::Undefined),
                Box::new(decoded),
            ),
        ),
        vec![keyed("failed")],
    );
    rt("createLoads", vec![Expr::Array(specs), serialized, failed])
}

/// The server's loads: awaited in order before rendering, and serialized
/// into `k-data` with their codecs.
pub(crate) fn ssr_loads(b: &mut Body<'_, '_>, d: DeclId, loads: &[MemberId]) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut data = Vec::new();
    for &m in loads {
        let mi = b.g.view.member(m).clone();
        let init =
            b.g.member_node(m)
                .and_then(|n| n.children().find_map(ast::Expr::cast));
        let Some(init) = init else { continue };
        let (pre, x) = b.capture(|b| b.expr(&init));
        out.extend(pre);
        let n = mangle(&mi.name);
        out.push(Stmt::Const(n.clone(), x));
        let t = mi.ty.clone().unwrap_or(Type::Error);
        data.push(Prop::KeyValue(mi.name.clone(), encode_expr(b, &t, id(n))));
    }
    out.push(Stmt::Expr(js::assign(
        Expr::Index(
            Box::new(member(id("out"), "data")),
            Box::new(load_key(b.g, d)),
        ),
        Expr::Object(
            vec![Prop::KeyValue("loads".into(), Expr::Object(data, false))],
            false,
        ),
    )));
    out
}

/// Encodes `x` of type `t`: `T$encode(x)` for named codecs, else through the
/// codec object.
pub(crate) fn encode_expr(b: &mut Body<'_, '_>, t: &Type, x: Expr) -> Expr {
    if let Some(d) = t.decl() {
        if crate::codec::has_codec(b.g, d) && b.g.view.decl(d).generics.is_empty() {
            let f = b.use_decl(d, format!("{}$encode", b.g.name(d)));
            return call(f, vec![x]);
        }
    }
    let c = crate::codec::codec_expr(b, t);
    call(member(c, "encode"), vec![x])
}

// ----------------------------------------------------------------- forms

/// The state of a `@Form`: `$k.createForm(fields, initial, validate)`.
pub(crate) fn form(b: &mut Body<'_, '_>, m: MemberId, init: Option<&ast::Expr>) -> Expr {
    let g = b.g;
    let ty = g.view.member(m).ty.clone().unwrap_or(Type::Error);
    let Some(d) = ty.decl() else {
        return Expr::Null;
    };
    let mut fields = Vec::new();
    // Whether a field has a `@Check` the browser runs, debounced on the
    // app's `Clock`.
    let mut debounced = false;
    for f in g.stored_fields(d) {
        let fi = g.view.member(f).clone();
        let ft = fi.ty.clone().unwrap_or(Type::Error);
        let node = g.member_node(f);
        let anns: Vec<(String, Option<ast::ArgList>)> = node
            .as_ref()
            .map(|n| {
                n.children()
                    .filter_map(ast::Annotation::cast)
                    .map(|a| {
                        (
                            a.name().map(|t| t.text().to_owned()).unwrap_or_default(),
                            a.arg_list(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let format_message = anns
            .iter()
            .find(|(n, _)| n == "Format")
            .and_then(|(_, l)| message_arg(l.as_ref()));
        let parse = parse_fn(b, &ft, format_message);
        let mut constraints = Vec::new();
        for (name, args) in &anns {
            if let Some(c) = constraint(b, name, args.as_ref(), &ft) {
                constraints.push(c);
            }
        }
        let mut props = vec![
            Prop::KeyValue("name".into(), js::str(fi.name.clone())),
            Prop::KeyValue("parse".into(), parse),
            Prop::KeyValue("constraints".into(), Expr::Array(constraints)),
        ];
        // `@Check` rules the browser can run: the service's function in
        // the component's container, with the form's signal (`$c`).
        let checks: Vec<Expr> = anns
            .iter()
            .filter(|(n, _)| n == "Check" && b.target == Target::Client)
            .filter_map(|(_, args)| browser_check(g, d, args.as_ref()))
            .map(|(service, m, message)| {
                let run = crate::server::check_call(g, member(id("ctx"), "c"), service, m);
                Expr::Object(
                    vec![
                        Prop::KeyValue(
                            "run".into(),
                            js::arrow(vec![Param::new("$c"), Param::new("x")], run),
                        ),
                        Prop::KeyValue("message".into(), js::str(message)),
                    ],
                    false,
                )
            })
            .collect();
        if !checks.is_empty() {
            debounced = true;
            props.push(Prop::KeyValue("checks".into(), Expr::Array(checks)));
        }
        fields.push(Expr::Object(props, false));
    }
    // Initial raw text: `.empty(name: x, age: 18)`.
    let mut initial = Vec::new();
    if let Some(ast::Expr::CallExpr(c)) = init {
        if let Some(list) = c.arg_list() {
            for a in list.args() {
                if let (Some(l), Some(v)) = (a.label(), a.value()) {
                    let x = match &v {
                        ast::Expr::Literal(lit) => {
                            js::str(lit.token().map(|t| t.text().to_owned()).unwrap_or_default())
                        }
                        ast::Expr::StringLit(s) if s.is_plain() => {
                            js::str(s.plain_value().unwrap_or_default())
                        }
                        other => {
                            let t = b.ty(other.syntax());
                            let x = b.expr(other);
                            b.text_of(x, &t)
                        }
                    };
                    initial.push(Prop::KeyValue(l.text().to_owned(), x));
                }
            }
        }
    }
    let validate = match g.find_member(d, "validate") {
        Some(v) if !g.view.member(v).is_static => {
            let f = b.fn_ref(v);
            js::arrow(
                vec![Param::new("v"), Param::new("rules")],
                call(f, vec![id("v"), id("rules")]),
            )
        }
        _ => Expr::Null,
    };
    let mut args = vec![Expr::Array(fields), Expr::Object(initial, false), validate];
    if debounced {
        args.push(member(member(id("ctx"), "c"), "Clock"));
    }
    rt("createForm", args)
}

/// The target of a `@Check` rule on a field of struct `d` that the browser
/// can run: its service is one the browser's containers may hold (not
/// server-only). A server-only service's rule runs on the server alone.
fn browser_check(
    g: &Gen<'_>,
    d: DeclId,
    args: Option<&ast::ArgList>,
) -> Option<(DeclId, MemberId, String)> {
    crate::server::check_target(g, d, args)
        .filter(|(service, _, _)| crate::bundle::allowed(g, *service, Target::Client))
}

/// What the `@Form` fields of declaration `d` reach in the browser beyond
/// its `@Inject`s: the services of the `@Check` rules the browser runs and
/// `Clock`, which debounces them (15-forms-and-validation.md). The
/// containers get a getter for each, as for an injected type.
pub(crate) fn form_dependencies(g: &Gen<'_>, d: DeclId) -> Vec<DeclId> {
    let mut out = Vec::new();
    for &m in &g.view.decl(d).members {
        let info = g.view.member(m);
        if !info.has_annotation("Form") {
            continue;
        }
        let Some(s) = info.ty.as_ref().and_then(Type::decl) else {
            continue;
        };
        for f in g.stored_fields(s) {
            let Some(node) = g.member_node(f) else {
                continue;
            };
            for a in node.children().filter_map(ast::Annotation::cast) {
                if a.name().is_some_and(|n| n.text() == "Check") {
                    if let Some((service, _, _)) = browser_check(g, s, a.arg_list().as_ref()) {
                        if !out.contains(&service) {
                            out.push(service);
                        }
                    }
                }
            }
        }
    }
    if !out.is_empty() {
        out.extend(g.view.lookup("k.time.Clock"));
    }
    out
}

/// The services the stdlib's `@Native` components that `d`'s code uses
/// read from the container they render in (`ctx.c`): `CopyButton` copies
/// with the injected `Clipboard` and times its "Copied" on the `Clock`,
/// `ShareButton` also shares with `Share` (45-web-platform.md). They have
/// no `@Inject` fields, since a component can't inject a platform
/// interface, so the containers learn them here.
pub(crate) fn native_component_dependencies(g: &Gen<'_>, d: DeclId) -> Vec<DeclId> {
    let Some(node) = g.decl_node(d) else {
        return Vec::new();
    };
    let file = g.view.decl(d).file;
    let mut out = Vec::new();
    for n in node.descendants().filter_map(ast::NameExpr::cast) {
        let Some(name) = n.name() else { continue };
        if !matches!(
            name.text(),
            "CopyButton" | "ShareButton" | "KeepScreenAwake" | "LevelMeter"
        ) {
            continue;
        }
        let Some(Resolved::Decl(c)) = g.resolved(file, name.text_range()) else {
            continue;
        };
        let services: &[&str] = match g.view.qualified_name(c).as_str() {
            "k.device.CopyButton" => &["k.device.Clipboard", "k.time.Clock"],
            "k.device.ShareButton" => &["k.device.Share", "k.device.Clipboard", "k.time.Clock"],
            // It says the level at most once a second on the `Clock`
            // (46-media-and-rtc.md, "Audio").
            "k.media.LevelMeter" => &["k.time.Clock"],
            // In the browser it holds the screen's lock itself; `k test`
            // gives it `TestWakeLock` (45-web-platform.md, "KeepScreenAwake").
            "k.device.KeepScreenAwake" if g.options.environment == crate::Environment::Test => {
                &["k.internal.WakeLock"]
            }
            _ => continue,
        };
        for s in services {
            if let Some(t) = g.view.lookup(s) {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    out
}

/// The plain string of a validation annotation's `message:` argument
/// (`@Min(1, message: "…")`), or `None` when it has none or it is not a
/// plain string literal.
fn message_arg(l: Option<&ast::ArgList>) -> Option<String> {
    l?.args()
        .find(|a| a.label().is_some_and(|t| t.text() == "message"))
        .and_then(|a| a.value())
        .and_then(|v| match v {
            ast::Expr::StringLit(s) => s.plain_value(),
            _ => None,
        })
}

/// The `parse` of a form field by its type.
fn parse_fn(b: &mut Body<'_, '_>, t: &Type, message: Option<String>) -> Expr {
    let g = b.g;
    let base = t.unwrap_optional();
    let (kind, default_message) = if g.is_int(base) {
        ("int", "Enter a whole number")
    } else if g.is_double(base) {
        ("double", "Enter a number")
    } else if g.is_bool(base) {
        ("bool", "")
    } else if g.is(base, g.b.email) {
        ("email", "Enter an email address")
    } else if g.is(base, g.b.money) || g.is(base, g.b.decimal) {
        ("decimal", "Enter an amount")
    } else {
        ("text", "")
    };
    let msg = message.unwrap_or_else(|| default_message.to_owned());
    rt(
        "fieldParser",
        vec![
            js::str(kind),
            js::str(msg),
            Expr::Bool(matches!(t, Type::Optional(_))),
        ],
    )
}

/// A constraint annotation as a check function returning a message or
/// `null` (15-forms-and-validation.md).
pub(crate) fn constraint(
    b: &mut Body<'_, '_>,
    name: &str,
    args: Option<&ast::ArgList>,
    t: &Type,
) -> Option<Expr> {
    let message = message_arg(args);
    let arg = |label: &str| -> Option<ast::Expr> {
        args?
            .args()
            .find(|a| a.label().is_some_and(|l| l.text() == label))
            .and_then(|a| a.value())
    };
    let first = args
        .and_then(|l| l.args().next())
        .filter(|a| a.label().is_none())
        .and_then(|a| a.value());
    let num = |e: Option<ast::Expr>| -> Option<f64> {
        let e = e?;
        let text = e.syntax().text().to_string();
        text.replace('_', "").parse().ok()
    };
    let _ = t;
    match name {
        "Length" => {
            let min = num(arg("min"));
            let max = num(arg("max"));
            let default = match (min, max) {
                (Some(lo), None) => {
                    format!("Use at least {} characters", crate::lower::fmt_num(lo))
                }
                (None, Some(hi)) => format!("Use at most {} characters", crate::lower::fmt_num(hi)),
                (Some(lo), Some(hi)) => format!(
                    "Use {} to {} characters",
                    crate::lower::fmt_num(lo),
                    crate::lower::fmt_num(hi)
                ),
                (None, None) => String::new(),
            };
            Some(rt(
                "lengthRule",
                vec![
                    min.map_or(Expr::Null, Expr::Num),
                    max.map_or(Expr::Null, Expr::Num),
                    js::str(message.unwrap_or(default)),
                ],
            ))
        }
        "Range" => {
            let (lo, hi) = match &first {
                Some(ast::Expr::BinaryExpr(be)) => (num(be.lhs()), num(be.rhs())),
                _ => (None, None),
            };
            let default = format!(
                "Enter a number from {} to {}",
                lo.map_or_else(String::new, crate::lower::fmt_num),
                hi.map_or_else(String::new, crate::lower::fmt_num)
            );
            Some(rt(
                "rangeRule",
                vec![
                    lo.map_or(Expr::Null, Expr::Num),
                    hi.map_or(Expr::Null, Expr::Num),
                    js::str(message.unwrap_or(default)),
                ],
            ))
        }
        "Required" | "NotEmpty" => Some(rt(
            "requiredRule",
            vec![js::str(message.unwrap_or_else(|| "Required".into()))],
        )),
        // Generated inline rather than in the runtime: blank after trimming.
        "NotBlank" => Some(js::arrow(
            vec![Param::new("v")],
            Expr::Cond(
                Box::new(bin(
                    "===",
                    call(member(id("v"), "trim"), vec![]),
                    js::str(""),
                )),
                Box::new(js::str(message.unwrap_or_else(|| "Required".into()))),
                Box::new(Expr::Null),
            ),
        )),
        // An email address as `EmailAddress.parse` accepts one.
        "Email" => Some(js::arrow(
            vec![Param::new("v")],
            Expr::Cond(
                Box::new(bin("===", rt("parseEmail", vec![id("v")]), Expr::Null)),
                Box::new(js::str(
                    message.unwrap_or_else(|| "Enter an email address".into()),
                )),
                Box::new(Expr::Null),
            ),
        )),
        // The pattern is a checked string literal (sema); JS `RegExp`
        // syntax without flags, as the design's examples are written.
        "Matches" => {
            let pattern = match &first {
                Some(ast::Expr::StringLit(s)) => s.plain_value(),
                _ => None,
            }?;
            Some(js::arrow(
                vec![Param::new("v")],
                Expr::Cond(
                    Box::new(call(
                        member(
                            Expr::New(Box::new(id("RegExp")), vec![js::str(pattern)]),
                            "test",
                        ),
                        vec![id("v")],
                    )),
                    Box::new(Expr::Null),
                    Box::new(js::str(
                        message.unwrap_or_else(|| "Use the expected format".into()),
                    )),
                ),
            ))
        }
        // `@Check` is asynchronous and runs a service, so it isn't a
        // constraint: the server's validator runs it (`server::validator`),
        // and the browser's form gets it as a field's `checks`, run
        // debounced on blur (`form`, runtime.md §9).
        _ => {
            let _ = b;
            None
        }
    }
}

// ------------------------------------------------------------------ rpc

/// The rpc path of protocol member `m`: `/package.module.Protocol/Method`.
pub(crate) fn rpc_path(g: &Gen<'_>, d: DeclId, m: MemberId) -> String {
    let info = g.view.decl(d);
    let package = crate::server::package_of(g, d);
    let name = &g.view.member(m).name;
    let mut c = name.chars();
    let method = match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    };
    format!("/{package}.{}/{method}", info.name)
}

/// `true` for the stdlib `Page` / `ThemeVariant` declarations, which the
/// generator emits per app.
pub(crate) fn is_generated_enum(g: &Gen<'_>, d: DeclId) -> bool {
    g.view.lookup("k.core.Page") == Some(d) || g.view.lookup("k.core.ThemeVariant") == Some(d)
}

/// A leftover that nothing calls (hence `dead_code` is allowed); it has
/// no effect on the output and can be removed.
#[allow(dead_code)]
fn unused(_: Target) {}
