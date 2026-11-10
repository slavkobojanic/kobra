//! The test bundle of `k test` ([`crate::Entry::Tests`];
//! `docs/design/21-testing.md`; `docs/compiler/codegen.md` §21).
//!
//! A test run is one browser and one server in one isolate. The bundle is
//! the project's client and server bundles, built for the `.test`
//! environment, with test code included:
//!
//! * every test group (`test Name { … }`) lowers to a **group class**
//!   (`$group<N>`), built like a service over the test's container: its
//!   `@Inject` fields read the container, its other fields are its
//!   helpers, and each `test "…" { … }` is an async method `$t<i>($c)`;
//! * the **entry** builds the browser containers (`$App`, with the
//!   hermetic `k.test` defaults in place of the platform services, and one
//!   subclass per group that applies the group's client-side `use` rules
//!   and reads its server-side `@Inject` fields from the test's server
//!   container) and exports `$groups`: each group's name, file, container
//!   factory, class and tests (name, method, and the source range of the
//!   name, where a test that fails outside an `expect` is reported); and
//!   `$pages`, the typed URL matchers of the pages, for
//!   `Screen.currentPage`;
//! * the **server entry** (`k:/app/$server.js`) is the app's, except that
//!   it exports `$test` instead of serving: the request handler, one
//!   server container class per group (`$S<N>`, with the group's
//!   server-side `use` rules, each bound once per test) and a reset that
//!   starts a new process for each test ([`server_groups`]);
//! * the **runtime chunk** re-exports the embedded `k:runtime` and the
//!   test kit (`k:testing/kit`, as `$test`), and answers `native` from
//!   the kit, so the generated code, the kit and the fake DOM share one
//!   runtime instance.
//!
//! `expect` and `expect throws` lower to calls of the kit's `fail` and
//! `failThrows` with the expectation's source range and the values of its
//! operands ([`crate::lower`]); `render(View(…))` lowers to the kit's
//! `render` with the view's renderer ([`render_call`]).

use std::collections::BTreeSet;

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, Resolved};
use k_syntax::SyntaxKind;
use k_syntax::ast::{self, AstNode};

use crate::cx::{Gen, Target};
use crate::js::{self, Class, Expr, Function, MethodKind, Param, Prop, Stmt, call, id, member};
use crate::lower::Body;

/// The test kit's namespace in generated code: `$k.$test`.
pub(crate) fn kit() -> Expr {
    member(id("$k"), "$test")
}

/// `$k.$test.name(args)`.
pub(crate) fn kit_call(name: &str, args: Vec<Expr>) -> Expr {
    call(member(kit(), name), args)
}

/// The JavaScript name of a group class (`Gen` names every test group
/// `$group<N>`).
fn group_name(d: DeclId) -> String {
    format!("$group{}", d.0)
}

/// The test groups of the project: named `test` declarations of test
/// files, in file path order, then source order.
pub(crate) fn groups(g: &Gen<'_>) -> Vec<DeclId> {
    let mut out: Vec<DeclId> = g
        .own
        .iter()
        .copied()
        .filter(|&d| {
            let info = g.view.decl(d);
            info.is_test
                && info.named_kind() == Some(DeclarationKind::TestSuite)
                && !info.name.is_empty()
        })
        .collect();
    let path = |d: DeclId| {
        let f = g.view.decl(d).file;
        g.analysis
            .files()
            .into_iter()
            .find(|(id, _)| *id == f)
            .map(|(_, p)| p.as_str().to_owned())
            .unwrap_or_default()
    };
    out.sort_by_key(|&d| (path(d), g.view.decl(d).range.start()));
    out
}

/// The `test "…" { … }` cases of a group, in order, with their names.
fn cases(g: &Gen<'_>, d: DeclId) -> Vec<(String, k_syntax::SyntaxNode)> {
    let Some(node) = g.decl_node(d) else {
        return Vec::new();
    };
    node.children()
        .filter(|c| c.kind() == SyntaxKind::MemberList)
        .flat_map(|l| {
            l.children()
                .filter(|c| c.kind() == SyntaxKind::TestCaseDecl)
        })
        .map(|c| {
            let name = c
                .children()
                .find_map(ast::StringLit::cast)
                .and_then(|s| s.plain_value())
                .unwrap_or_default();
            (name, c)
        })
        .collect()
}

/// The group class of test group `d`: the service lowering of its fields,
/// plus one async method per test.
pub(crate) fn group(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let mut stmts = crate::ui::service(b, d);
    let name = group_name(d);
    let mut methods = Vec::new();
    for (i, (_, node)) in cases(b.g, d).into_iter().enumerate() {
        b.owner = Some(d);
        b.self_mode = crate::lower::SelfMode::This;
        b.push_fn_scope();
        let saved = b.has_c;
        b.has_c = true;
        let body = crate::decl::body_stmts(b, &node);
        b.has_c = saved;
        b.pop_fn_scope();
        methods.push(Function {
            name: format!("$t{i}"),
            params: vec![Param::new("$c")],
            body: js::Body::Block(body),
            is_async: true,
            kind: MethodKind::Plain,
        });
    }
    match stmts.first_mut() {
        Some(Stmt::Class(class, exported)) => {
            class.name = name;
            class.methods.extend(methods);
            *exported = true;
        }
        _ => {
            stmts = vec![Stmt::Class(
                Class {
                    name,
                    extends: None,
                    methods,
                },
                true,
            )];
        }
    }
    stmts
}

/// The types test group `d` injects (its `@Inject` fields).
fn injected_types(g: &Gen<'_>, d: DeclId) -> Vec<DeclId> {
    g.view
        .decl(d)
        .members
        .iter()
        .filter(|&&m| g.view.member(m).has_annotation("Inject"))
        .filter_map(|&m| {
            g.view
                .member(m)
                .ty
                .as_ref()
                .and_then(k_sema::view::Type::decl)
        })
        .collect()
}

/// One `use … for …` rule of a test group.
enum Rule {
    /// `use Impl for Target`.
    Type(DeclId, DeclId),
    /// `use value for Target`.
    Value(ast::Expr, DeclId),
}

/// The `use` rules directly in group `d`.
fn rules(g: &Gen<'_>, d: DeclId) -> Vec<Rule> {
    let Some(node) = g.decl_node(d) else {
        return Vec::new();
    };
    let file = g.view.decl(d).file;
    let resolve = |ty: &ast::Type| -> Option<DeclId> {
        let tok = ty
            .syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind() == SyntaxKind::Ident)
            .last()?;
        match g.resolved(file, tok.text_range()) {
            Some(Resolved::Decl(x)) => Some(x),
            _ => None,
        }
    };
    node.children()
        .filter(|c| c.kind() == SyntaxKind::MemberList)
        .flat_map(|l| {
            l.children()
                .filter_map(ast::UseDecl::cast)
                .collect::<Vec<_>>()
        })
        .filter_map(|u| {
            let target = u.target().and_then(|t| resolve(&t))?;
            if let Some(imp) = u.implementation().and_then(|t| resolve(&t)) {
                return Some(Rule::Type(imp, target));
            }
            u.value().map(|v| Rule::Value(v, target))
        })
        .collect()
}

/// Every type the `use` rules of group `d` name: their targets and their
/// implementations.
pub(crate) fn rule_types(g: &Gen<'_>, d: DeclId) -> Vec<DeclId> {
    let mut out = Vec::new();
    for r in rules(g, d) {
        match r {
            Rule::Type(imp, target) => {
                out.push(imp);
                out.push(target);
            }
            Rule::Value(_, target) => out.push(target),
        }
    }
    out
}

/// The stdlib types a test injects from its server container: they exist
/// only on the server (21-testing.md, "Which side a test runs on").
const SERVER_STDLIB: &[&str] = &[
    "k.auth.SessionManager",
    "k.auth.PasskeyVerifier",
    "k.test.TestClient",
    "k.test.RecordingEmailProvider",
    "k.test.RecordingJobQueue",
    "k.test.RecordingWebPush",
    "k.test.InlineJobQueue",
    "k.server.PublicUrls",
    "k.server.JobQueue",
    "k.server.EmailProvider",
    "k.server.WebPush",
    "k.db.Database",
];

/// `true` when group `grp` injects `t` from the test's server container:
/// a server-only declaration (repositories, configs, protocol
/// implementations, `.server`-scoped services), a server stdlib service,
/// or the implementation a server-side `use` rule of the group binds.
/// Everything else, protocols included, comes from the browser container.
pub(crate) fn server_side(g: &Gen<'_>, grp: DeclId, t: DeclId) -> bool {
    if server_only(g, t) {
        return true;
    }
    // A fake bound by the group's rule lives where its target is used: on
    // the server when the target is server-only, or when something the
    // group injects from the server depends on it (`use FakeStripeApi for
    // HttpClient` with `@Inject let checkout: StripeCheckout`).
    rules(g, grp).iter().any(|r| match r {
        Rule::Type(imp, target) => {
            *imp == t && (server_only(g, *target) || used_on_server(g, grp, *target))
        }
        Rule::Value(..) => false,
    })
}

/// `true` when server code the group reaches depends on `target`: what it
/// injects, followed through each `@Inject` and from a protocol to its
/// server implementation (a browser service calling `StoreProtocol`
/// reaches `StoreApi` and what that injects).
fn used_on_server(g: &Gen<'_>, grp: DeclId, target: DeclId) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<(DeclId, bool)> = injected_types(g, grp)
        .into_iter()
        .map(|t| (t, server_only(g, t)))
        .collect();
    while let Some((d, on_server)) = stack.pop() {
        if !seen.insert((d, on_server)) {
            continue;
        }
        if g.view.decl(d).named_kind() == Some(DeclarationKind::Protocol) {
            if let Some(imp) = crate::server::implementation(g, d) {
                stack.push((imp, true));
            }
            continue;
        }
        for dep in crate::server::injected(g, d) {
            if dep == target && on_server {
                return true;
            }
            stack.push((dep, on_server || server_only(g, dep)));
        }
    }
    false
}

/// `true` for a declaration only the server container provides, whatever
/// the group's rules: see [`server_side`].
fn server_only(g: &Gen<'_>, t: DeclId) -> bool {
    let info = g.view.decl(t);
    if info.named_kind() == Some(DeclarationKind::Protocol) {
        return false;
    }
    if SERVER_STDLIB.contains(&g.view.qualified_name(t).as_str()) {
        return true;
    }
    if info.stdlib {
        return info.side == Some(k_sema::Side::ServerOnly);
    }
    if info.side == Some(k_sema::Side::ServerOnly)
        || !crate::bundle::allowed(g, t, Target::Client)
        || matches!(
            info.named_kind(),
            Some(DeclarationKind::Repository | DeclarationKind::Config)
        )
        || crate::server::is_server_scoped(g, t)
    {
        return true;
    }
    g.own.iter().any(|&p| {
        g.view.decl(p).named_kind() == Some(DeclarationKind::Protocol)
            && crate::server::implementation(g, p) == Some(t)
    })
}

/// `true` when `d` can be bound in a server container: it is lowered for
/// the server and isn't client-only.
fn on_server(g: &Gen<'_>, d: DeclId) -> bool {
    crate::bundle::allowed(g, d, Target::Server)
        && g.view.decl(d).side != Some(k_sema::Side::ClientOnly)
}

/// The test half of the server entry (`k test`): one server container
/// class per test group (`$S<N>`, the request container with the group's
/// server-side `use` rules, each binding one instance per test), and
/// `$test`, what the kit drives the app with in-process: the spec, the
/// request handler, each group's container factory (by group index) and
/// `reset`, which starts a new process for the next test.
pub(crate) fn server_groups(b: &mut Body<'_, '_>) -> Vec<Stmt> {
    let g = b.g;
    let mut stmts = Vec::new();
    let mut factories = Vec::new();
    for (n, grp) in groups(g).into_iter().enumerate() {
        let file = b.file;
        b.file = g.view.decl(grp).file;
        let mut methods = Vec::new();
        for r in rules(g, grp) {
            let (target, value) = match r {
                // Every rule a server container can hold applies there too
                // (server rendering through `TestClient` uses the fakes).
                Rule::Type(imp, target) => {
                    if !on_server(g, target) || !on_server(g, imp) {
                        continue;
                    }
                    (target, member(Expr::This, g.name(imp)))
                }
                Rule::Value(v, target) => {
                    if !on_server(g, target) {
                        continue;
                    }
                    b.owner = None;
                    b.self_mode = crate::lower::SelfMode::None;
                    let (pre, x) = b.capture(|b| b.expr(&v));
                    let x = if pre.is_empty() {
                        x
                    } else {
                        let mut s = pre;
                        s.push(Stmt::Return(Some(x)));
                        call(js::arrow_block(vec![], s, false), vec![])
                    };
                    (target, crate::server::per_test(&g.name(target), x))
                }
            };
            methods.push(crate::server::getter(g.name(target), value));
        }
        b.file = file;
        let class = format!("$S{n}");
        stmts.push(Stmt::Class(
            Class {
                name: class.clone(),
                extends: Some(id("$Request")),
                methods,
            },
            false,
        ));
        factories.push(js::arrow(
            vec![Param::new("st")],
            Expr::New(Box::new(id(&class)), vec![id("st")]),
        ));
    }
    let reset = js::arrow_block(
        vec![],
        vec![Stmt::Expr(js::assign(
            id("$process"),
            Expr::New(Box::new(id("$Process")), vec![]),
        ))],
        false,
    );
    stmts.push(Stmt::ExportConst(
        "$test".into(),
        Expr::Object(
            vec![
                Prop::KeyValue("spec".into(), id("$spec")),
                Prop::KeyValue(
                    "app".into(),
                    call(member(id("$s"), "createApp"), vec![id("$spec")]),
                ),
                Prop::KeyValue("groups".into(), Expr::Array(factories)),
                Prop::KeyValue("reset".into(), reset),
            ],
            true,
        ),
    ));
    stmts
}

/// The entry of the test bundle: the containers and `$groups`.
pub(crate) fn entry(g: &Gen<'_>) -> (Vec<Stmt>, BTreeSet<(DeclId, String)>) {
    let groups = groups(g);
    let file = groups
        .first()
        .map(|&d| g.view.decl(d).file)
        .or_else(|| g.own.first().map(|&d| g.view.decl(d).file))
        .unwrap_or_else(|| k_diagnostics::FileId::new(0));
    let mut b = Body::new(g, file, Target::Client);
    // Every service of the program may be under test.
    let reach: BTreeSet<DeclId> = g
        .own
        .iter()
        .copied()
        .filter(|&d| crate::bundle::allowed(g, d, Target::Client))
        .collect();
    let mut extra: Vec<DeclId> = Vec::new();
    for &grp in &groups {
        extra.extend(injected_types(g, grp));
        for r in rules(g, grp) {
            match r {
                Rule::Type(imp, target) => {
                    extra.push(imp);
                    extra.push(target);
                }
                Rule::Value(_, target) => extra.push(target),
            }
        }
    }
    let (mut stmts, per_route) = crate::app::client_containers(&mut b, &reach, &extra);
    let mut entries = Vec::new();
    for (n, &grp) in groups.iter().enumerate() {
        let info = g.view.decl(grp).clone();
        b.file = info.file;
        let container = format!("$G{n}");
        let mut methods = Vec::new();
        // What the group injects from the test's server container.
        let mut delegated = BTreeSet::new();
        for t in injected_types(g, grp) {
            if server_side(g, grp, t) && delegated.insert(t) {
                let name = g.name(t);
                methods.push(crate::server::getter(
                    name.clone(),
                    member(kit_call("server", vec![]), name),
                ));
            }
        }
        for r in rules(g, grp) {
            let target = match &r {
                Rule::Type(_, t) | Rule::Value(_, t) => *t,
            };
            // A server-side rule applies to the server container only.
            if delegated.contains(&target)
                || server_side(g, grp, target)
                || !crate::bundle::allowed(g, target, Target::Client)
            {
                continue;
            }
            let (target, value) = match r {
                Rule::Type(imp, target) => (target, member(crate::server::this("$a"), g.name(imp))),
                Rule::Value(v, target) => {
                    b.owner = None;
                    b.self_mode = crate::lower::SelfMode::None;
                    let (pre, x) = b.capture(|b| b.expr(&v));
                    let x = if pre.is_empty() {
                        x
                    } else {
                        let mut s = pre;
                        s.push(Stmt::Return(Some(x)));
                        call(js::arrow_block(vec![], s, false), vec![])
                    };
                    let name = g.name(target);
                    (
                        target,
                        Expr::Assign(
                            "??=",
                            Box::new(member(crate::server::this("$a"), format!("${name}"))),
                            Box::new(x),
                        ),
                    )
                }
            };
            methods.push(crate::server::getter(g.name(target), value));
        }
        // With route-scoped services the test's container is a `$Route`
        // over the group's: its getters of what the group overrides read
        // the group's, as `$App`'s own read the app's through `$a`.
        let overridden: Vec<String> = methods.iter().map(|m| m.name.clone()).collect();
        stmts.push(Stmt::Class(
            Class {
                name: container.clone(),
                extends: Some(id("$App")),
                methods,
            },
            false,
        ));
        let make = if per_route {
            let route = format!("{container}R");
            stmts.push(Stmt::Class(
                Class {
                    name: route.clone(),
                    extends: Some(id("$Route")),
                    methods: overridden
                        .iter()
                        .map(|n| {
                            crate::server::getter(
                                n.clone(),
                                member(crate::server::this("$a"), n.clone()),
                            )
                        })
                        .collect(),
                },
                false,
            ));
            js::arrow(
                vec![],
                Expr::New(
                    Box::new(id(&route)),
                    vec![Expr::New(Box::new(id(&container)), vec![Expr::Null])],
                ),
            )
        } else {
            js::arrow(
                vec![],
                Expr::New(Box::new(id(&container)), vec![Expr::Null]),
            )
        };
        let tests: Vec<Expr> = cases(g, grp)
            .into_iter()
            .enumerate()
            .map(|(i, (name, node))| {
                let range = node
                    .children()
                    .find_map(ast::StringLit::cast)
                    .map_or(node.text_range(), |s| s.syntax().text_range());
                Expr::Object(
                    vec![
                        Prop::KeyValue("name".into(), js::str(name)),
                        Prop::KeyValue("method".into(), js::str(format!("$t{i}"))),
                        Prop::KeyValue(
                            "start".into(),
                            Expr::Num(f64::from(u32::from(range.start()))),
                        ),
                        Prop::KeyValue("end".into(), Expr::Num(f64::from(u32::from(range.end())))),
                    ],
                    false,
                )
            })
            .collect();
        let path = g
            .analysis
            .files()
            .into_iter()
            .find(|(id, _)| *id == info.file)
            .map(|(_, p)| p.as_str().to_owned())
            .unwrap_or_default();
        let class = b.use_decl(grp, group_name(grp));
        entries.push(Expr::Object(
            vec![
                Prop::KeyValue("name".into(), js::str(info.name.clone())),
                Prop::KeyValue("file".into(), js::str(path)),
                Prop::KeyValue("container".into(), make),
                Prop::KeyValue("group".into(), class),
                Prop::KeyValue("tests".into(), Expr::Array(tests)),
            ],
            false,
        ));
    }
    stmts.push(Stmt::ExportConst("$groups".into(), Expr::Array(entries)));
    // The pages by URL, most specific first, for `Screen.currentPage`:
    // the same typed matchers as the router's (`routes` names the table,
    // which a `Page` parameter's matcher reads).
    let mut routes = Vec::new();
    for p in crate::server::ordered_pages(g) {
        b.file = g.view.decl(p).file;
        let tpl = crate::app::template(g, p);
        let parse = crate::server::matcher(&mut b, p, &tpl);
        let value = crate::server::page_value(&mut b, p);
        routes.push(Expr::Object(
            vec![
                Prop::KeyValue("parse".into(), parse),
                Prop::KeyValue("value".into(), value),
            ],
            false,
        ));
    }
    stmts.push(Stmt::Const("routes".into(), Expr::Array(routes)));
    stmts.push(Stmt::ExportConst("$pages".into(), id("routes")));
    (stmts, b.uses)
}

/// `render(View(…))`: the kit's `render` with a renderer of the view,
/// built exactly as a view would build it ([`crate::view::Tree`]): a
/// component or page with its props, modifiers and slots, or an element.
/// `None` when the argument isn't a view call.
///
/// The props are evaluated once, when the test renders (bindings, slots
/// and event handlers stay functions): the rendered view is the one the
/// test asked for, not one that follows the expressions that built it.
pub(crate) fn render_call(b: &mut Body<'_, '_>, c: &ast::CallExpr) -> Option<Expr> {
    let arg = c.arg_list()?.args().next()?.value()?;
    let owner = b.owner?;
    let saved = std::mem::take(&mut b.stmts);
    let v = {
        let mut t = crate::view::Tree { b: &mut *b, owner };
        t.call_node(&arg)
    };
    let early = std::mem::replace(&mut b.stmts, saved);
    let Some(v) = v else {
        return page_render(b, &arg);
    };
    if let crate::view::VNode::Component(comp) = &v {
        snapshot(b, comp.decl, &comp.args);
    }
    let saved = std::mem::replace(&mut b.stmts, early);
    let node = {
        let mut c = crate::view::Client {
            b: &mut *b,
            owner,
            lifetime: None,
        };
        c.fragment(&[v])
    };
    let mut body = std::mem::replace(&mut b.stmts, saved);
    b.clear_overrides();
    body.push(Stmt::Return(Some(node)));
    let mount = js::arrow_block(vec![Param::new("ctx")], body, false);
    Some(kit_call("render", vec![mount]))
}

/// Evaluates the plain `@Prop`/`@Param` arguments of a rendered view now,
/// into temporaries that its props then read.
fn snapshot(b: &mut Body<'_, '_>, decl: DeclId, args: &[(String, ast::Expr)]) {
    for (name, e) in args {
        let member = b.g.find_member(decl, name);
        let binding = member.is_some_and(|m| b.g.view.member(m).has_annotation("Binding"));
        if binding || matches!(e, ast::Expr::BindExpr(_) | ast::Expr::LambdaExpr(_)) {
            continue;
        }
        let x = b.expr(e);
        let t = b.fresh("p");
        b.stmts.push(Stmt::Const(t.clone(), x));
        b.set_override(e.syntax().text_range(), id(&t));
    }
}

/// `render(SomePage(param: value))`: the page's renderer with its
/// parameters as props, without its layout (21-testing.md, "Rendering and
/// querying").
fn page_render(b: &mut Body<'_, '_>, arg: &ast::Expr) -> Option<Expr> {
    // `Page(id: 1).tab(.reviews)`: query parameters chained on the value.
    let mut chained: Vec<(String, ast::Expr)> = Vec::new();
    let mut cur = arg.clone();
    let call_expr = loop {
        let ast::Expr::CallExpr(c) = &cur else {
            return None;
        };
        match c.callee()? {
            ast::Expr::MemberExpr(me) => {
                let name = me.name()?.text().to_owned();
                let value = c.arg_list()?.args().next()?.value()?;
                chained.push((name, value));
                cur = me.receiver()?;
            }
            ast::Expr::NameExpr(_) => break c.clone(),
            _ => return None,
        }
    };
    let ast::Expr::NameExpr(n) = call_expr.callee()? else {
        return None;
    };
    let tok = n.name()?;
    let Some(Resolved::Decl(d)) = b.g.resolved(b.file, tok.text_range()) else {
        return None;
    };
    if b.g.view.decl(d).named_kind() != Some(DeclarationKind::Page) {
        return None;
    }
    let mut comp = crate::view::Comp {
        decl: d,
        args: Vec::new(),
        events: Vec::new(),
        slots: Vec::new(),
        stylables: Vec::new(),
    };
    if let Some(list) = call_expr.arg_list() {
        for a in list.args() {
            if let (Some(l), Some(v)) = (a.label(), a.value()) {
                comp.args.push((l.text().to_owned(), v));
            }
        }
    }
    // A chained parameter given as an implicit member (`.units(.imperial)`)
    // has no expected type in the test's source: it is the case of the
    // parameter's own type.
    for (name, value) in &chained {
        let ast::Expr::ImplicitMemberExpr(im) = value else {
            continue;
        };
        let case = im.name().map(|t| t.text().to_owned());
        let ty =
            b.g.find_member(d, name)
                .and_then(|m| b.g.view.member(m).ty.clone());
        if let (Some(case), Some(owner)) =
            (case, ty.as_ref().and_then(|t| t.unwrap_optional().decl()))
            && let Some(m) = b.g.find_member(owner, &case)
        {
            let v = b.static_ref(m);
            b.set_override(value.syntax().text_range(), v);
        }
    }
    comp.args.extend(chained.into_iter().rev());
    snapshot(b, d, &comp.args);
    let props = crate::view::props_object_inner(b, &comp, Target::Client);
    b.clear_overrides();
    let f = b.decl_ref(d);
    let mount = js::arrow(
        vec![Param::new("ctx")],
        call(f, vec![props.clone(), id("ctx")]),
    );
    // The browser is on the rendered page's URL, as after navigating to
    // it, so `screen.currentPage` starts as that page.
    let value = call(
        crate::server::page_value(b, d),
        vec![kit_call("propValues", vec![props])],
    );
    let href = js::arrow(vec![], crate::server::href_of(b, value));
    Some(kit_call("render", vec![mount, href]))
}

/// The runtime chunk of a test bundle: the embedded runtime and the kit.
pub(crate) const RUNTIME_CHUNK: &str = "export * from \"k:runtime\";\nexport * as $test from \"k:testing/kit\";\nexport { native } from \"k:testing/kit\";\n";
