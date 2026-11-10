//! Lowering of the framework declarations: components, pages and layouts
//! (client factories and server renderers), services, protocols (RPC
//! stubs), configs and repositories (`docs/compiler/codegen.md` §10–§15).

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, MemberId, MemberKind, Type};
use k_syntax::SyntaxNode;
use k_syntax::ast::{self, AstNode};

use crate::cx::{Gen, Target, mangle};
use crate::js::{
    self, Class, Expr, Function, MethodKind, Param, Prop, Stmt, bin, call, id, member, rt,
};
use crate::lower::{Body, MemberAccess, SelfMode};
use crate::view::{Client, Ssr, Tree, VNode};

/// `true` when a component, page or layout has `@Load` fields (its server
/// renderer is async).
pub(crate) fn has_loads(g: &Gen<'_>, d: DeclId) -> bool {
    g.view
        .decl(d)
        .members
        .iter()
        .any(|&m| g.view.member(m).has_annotation("Load"))
}

/// The child blocks of a declaration with `kind` (`ViewBlock`, …).
fn block_of(node: &SyntaxNode, kind: k_syntax::SyntaxKind) -> Option<SyntaxNode> {
    node.descendants().find(|c| {
        c.kind() == kind
            && c.parent()
                .is_some_and(|p| p.kind() == k_syntax::SyntaxKind::MemberList)
    })
}

/// The statements of a view-like block (`view { … }`, `loading { … }`).
fn block_stmts(block: &SyntaxNode) -> Vec<SyntaxNode> {
    block
        .children()
        .find(|c| c.kind() == k_syntax::SyntaxKind::Block)
        .map(|b| b.children().collect())
        .unwrap_or_default()
}

/// The constant name of a `@Load` spec array.
fn load_names(g: &Gen<'_>, d: DeclId) -> Vec<MemberId> {
    g.view
        .decl(d)
        .members
        .iter()
        .copied()
        .filter(|&m| g.view.member(m).has_annotation("Load"))
        .collect()
}

/// A component, page or layout: the client factory or the server renderer.
#[allow(clippy::too_many_lines)]
pub(crate) fn component(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let g = b.g;
    let info = g.view.decl(d).clone();
    let Some(node) = g.decl_node(d) else {
        return Vec::new();
    };
    let view = block_of(&node, k_syntax::SyntaxKind::ViewBlock);
    let is_page = info.named_kind() == Some(DeclarationKind::Page);
    let endpoint = is_page && view.is_none();
    if endpoint {
        return if b.target == Target::Server {
            crate::app::endpoint(b, d)
        } else {
            Vec::new()
        };
    }
    let ssr = b.target == Target::Server;
    if ssr && g.client_rendered() {
        return Vec::new();
    }
    // `@Rendering(.client)`: the server renders only `loading`, from the
    // props (47-client-rendered-components.md).
    let client_only = g.client_component(d);
    b.ssr = ssr;
    b.self_mode = SelfMode::Component;
    b.owner = Some(d);
    b.push_fn_scope();
    for reserved in ["props", "ctx", "out", "$s", "$life"] {
        b.bind_name(reserved, 0.into());
    }
    let name = g.name(d);
    let mut stmts = Vec::new();
    // Pass 1: how every member is reached.
    for &m in &info.members {
        let mi = g.view.member(m).clone();
        let n = mangle(&mi.name);
        let access = match mi.kind {
            MemberKind::Field => {
                if mi.has_annotation("Prop") || mi.has_annotation("Param") {
                    MemberAccess::Prop(mi.name.clone())
                } else if mi.has_annotation("Binding") {
                    MemberAccess::Binding(mi.name.clone())
                } else if mi.has_annotation("State") {
                    if is_struct(g, mi.ty.as_ref()) {
                        MemberAccess::Store(n)
                    } else {
                        MemberAccess::Signal(n)
                    }
                } else if mi.has_annotation("Derived") {
                    MemberAccess::Computed(n)
                } else if mi.has_annotation("Slot") {
                    MemberAccess::Raw(member(id("props"), mi.name.clone()))
                } else if mi.has_annotation("Stylable") {
                    MemberAccess::Raw(call(member(id("props"), mi.name.clone()), vec![]))
                } else if mi.has_annotation("Load") {
                    if ssr {
                        MemberAccess::Local(n)
                    } else {
                        MemberAccess::Raw(member(
                            call(member(id("$loads"), "values"), vec![]),
                            mi.name.clone(),
                        ))
                    }
                } else {
                    MemberAccess::Local(n)
                }
            }
            MemberKind::Func => MemberAccess::Local(n),
            MemberKind::Computed => MemberAccess::Raw(call(id(n), vec![])),
            _ => continue,
        };
        if let MemberAccess::Local(x)
        | MemberAccess::Signal(x)
        | MemberAccess::Computed(x)
        | MemberAccess::Store(x) = &access
        {
            b.bind_name(x, mi.name_range.start());
        }
        b.access.insert(m, access);
    }
    // Pass 2: the members in source order.
    let mut prop_defaults = Vec::new();
    let mut effects = Vec::new();
    let mut derived = Vec::new();
    let mut async_handlers = false;
    // Whether the client renderer refers to the lifetime `$life` is known
    // once its body is lowered; the declaration then goes first.
    b.uses_life = false;
    for &m in &info.members {
        let mi = g.view.member(m).clone();
        let n = mangle(&mi.name);
        let mnode = g.member_node(m);
        let init = mnode
            .as_ref()
            .and_then(|x| x.children().find_map(ast::Expr::cast));
        // Everything a field emits maps to its declaration, and a
        // `@Derived` body to its initializer (codegen.md §18).
        let start = stmts.len();
        let field_loc = match (&mi.kind, &mnode) {
            (MemberKind::Field, Some(node)) => b.loc(node),
            _ => None,
        };
        let init_loc = init.as_ref().and_then(|e| b.loc(e.syntax()));
        // The server never constructs a client-rendered component: only its
        // props' defaults, which `loading` may read.
        if ssr && client_only && !(mi.kind == MemberKind::Field && mi.has_annotation("Prop")) {
            continue;
        }
        match mi.kind {
            MemberKind::Field => {
                if mi.has_annotation("Prop") || mi.has_annotation("Param") {
                    if let Some(e) = &init {
                        // A default: the child evaluates it once.
                        let (pre, x) = b.capture(|b| b.expr(e));
                        let mut set = pre;
                        let v = b.fresh("d");
                        set.push(Stmt::Const(v.clone(), x));
                        set.push(Stmt::Expr(js::assign(
                            member(id("props"), mi.name.clone()),
                            js::arrow(vec![], id(&v)),
                        )));
                        let st = Stmt::If(
                            bin("===", member(id("props"), mi.name.clone()), Expr::Undefined),
                            set,
                            None,
                        );
                        if client_only {
                            prop_defaults.push(st.clone());
                        }
                        stmts.push(st);
                    }
                } else if mi.has_annotation("Slot") && init.is_some() {
                    // An optional slot the parent doesn't fill takes its
                    // default (`nil`); slots are functions, not thunks.
                    let (pre, x) = b.capture(|b| init.as_ref().map_or(Expr::Null, |e| b.expr(e)));
                    let mut set = pre;
                    set.push(Stmt::Expr(js::assign(
                        member(id("props"), mi.name.clone()),
                        x,
                    )));
                    stmts.push(Stmt::If(
                        bin("===", member(id("props"), mi.name.clone()), Expr::Undefined),
                        set,
                        None,
                    ));
                } else if mi.has_annotation("Stylable") {
                    // Optional (06-styles.md): a parent that sets nothing
                    // gives no class.
                    stmts.push(Stmt::If(
                        bin("===", member(id("props"), mi.name.clone()), Expr::Undefined),
                        vec![Stmt::Expr(js::assign(
                            member(id("props"), mi.name.clone()),
                            js::arrow(vec![], js::str("")),
                        ))],
                        None,
                    ));
                } else if mi.has_annotation("State") {
                    let (pre, x) = b.capture(|b| match &init {
                        Some(e) => {
                            let x = b.expr(e);
                            let from = b.ty(e.syntax());
                            b.coerce(x, &from, mi.ty.as_ref().unwrap_or(&Type::Error))
                        }
                        None => Expr::Null,
                    });
                    stmts.extend(pre);
                    if ssr {
                        stmts.push(Stmt::Const(n, x));
                    } else if is_struct(g, mi.ty.as_ref()) {
                        stmts.push(Stmt::Const(n, rt("store", vec![x])));
                    } else {
                        stmts.push(Stmt::Const(n, rt("signal", vec![x])));
                    }
                } else if mi.has_annotation("Derived") {
                    let (pre, x) = b.capture(|b| init.as_ref().map_or(Expr::Null, |e| b.expr(e)));
                    let x = js::at(init_loc, x);
                    if ssr {
                        // After the awaited loads, which it may read.
                        derived.extend(pre);
                        derived.push(Stmt::Const(n, x));
                    } else {
                        let f = if pre.is_empty() {
                            js::arrow(vec![], x)
                        } else {
                            let mut s = pre;
                            s.push(Stmt::Return(Some(x)));
                            js::arrow_block(vec![], s, false)
                        };
                        stmts.push(Stmt::Const(n, rt("computed", vec![f])));
                    }
                } else if mi.has_annotation("Emits") {
                    stmts.push(Stmt::Const(
                        n,
                        bin(
                            "??",
                            member(id("props"), format!("on${}", mi.name)),
                            Expr::Null,
                        ),
                    ));
                } else if mi.has_annotation("Inject") {
                    let t = mi.ty.clone().unwrap_or(Type::Error);
                    let tn = t.decl().map(|x| g.name(x)).unwrap_or_default();
                    stmts.push(Stmt::Const(
                        n.clone(),
                        member(member(id("ctx"), "c"), tn.clone()),
                    ));
                    // A `@Scope(.component)` service: this component's
                    // instance is the one its subtree injects.
                    if t.decl().is_some_and(|x| {
                        g.view.decl(x).named_kind() == Some(DeclarationKind::Service)
                            && crate::server::scope_of(g, x) == "component"
                    }) {
                        stmts.push(Stmt::Expr(js::assign(
                            id("ctx"),
                            rt("provide", vec![id("ctx"), js::str(tn), id(n)]),
                        )));
                    }
                } else if mi.has_annotation("Form") {
                    stmts.push(Stmt::Const(n, crate::app::form(b, m, init.as_ref())));
                } else if mi.has_annotation("Load")
                    || mi.has_annotation("Slot")
                    || mi.has_annotation("Stylable")
                    || mi.has_annotation("Binding")
                {
                } else if !mi.is_static {
                    let (pre, x) = b.capture(|b| init.as_ref().map_or(Expr::Null, |e| b.expr(e)));
                    stmts.extend(pre);
                    stmts.push(Stmt::Let(n, Some(x)));
                }
            }
            MemberKind::Func => {
                let effect = mi.has_annotation("Effect");
                if effect && ssr {
                    continue;
                }
                let sig = mi.signature.clone();
                let f =
                    crate::decl::function(b, n.clone(), mnode.as_ref(), sig.as_ref(), Vec::new());
                if effect {
                    effects.push((m, f));
                } else {
                    if f.is_async {
                        async_handlers = true;
                    }
                    stmts.push(Stmt::Function(f, false));
                }
            }
            MemberKind::Computed => {
                let f = crate::decl::function(b, n, mnode.as_ref(), None, Vec::new());
                stmts.push(Stmt::Function(f, false));
            }
            _ => {}
        }
        if let Some(loc) = field_loc {
            for st in stmts.iter_mut().skip(start) {
                if !matches!(st, Stmt::At(..)) {
                    let inner = std::mem::replace(st, Stmt::Break(None));
                    *st = Stmt::At(loc, Box::new(inner));
                }
            }
        }
    }
    // `@Load`: the load group on the client, awaited values on the server.
    let loads = load_names(g, d);
    let loading = block_of(&node, k_syntax::SyntaxKind::LoadingBlock);
    let failed = block_of(&node, k_syntax::SyntaxKind::FailedBlock);
    if ssr && client_only {
        let loading_nodes = loading.as_ref().map_or_else(Vec::new, |l| {
            let st = block_stmts(l);
            let mut t = Tree { b, owner: d };
            t.nodes(&st)
        });
        let mut s = Ssr { b, owner: d };
        let saved = std::mem::take(&mut s.b.stmts);
        s.nodes(&loading_nodes);
        let body = std::mem::replace(&mut s.b.stmts, saved);
        stmts.push(ssr_marker("OPEN"));
        stmts.extend(body);
        stmts.push(ssr_marker("CLOSE"));
        b.pop_fn_scope();
        return vec![Stmt::Function(
            Function {
                name: format!("{name}$ssr"),
                params: vec![Param::new("props"), Param::new("ctx"), Param::new("out")],
                body: js::Body::Block(stmts),
                is_async: crate::app::has_async_children(g, &loading_nodes),
                kind: MethodKind::Plain,
            },
            true,
        )];
    }
    if !loads.is_empty() {
        if crate::app::is_instance_keyed(g, d) && !client_only {
            stmts.push(crate::app::load_key_init(b.target));
        }
        if ssr {
            let awaited = crate::app::ssr_loads(b, d, &loads);
            match &failed {
                Some(f) => stmts.extend(ssr_failed(b, d, &loads, awaited, f)),
                None => stmts.extend(awaited),
            }
            stmts.append(&mut derived);
        } else {
            let error = failed.as_ref().map(|f| failed_error_type(b, f));
            let group = crate::app::client_loads(b, d, &loads, error.as_ref(), !client_only);
            stmts.push(Stmt::Const("$loads".into(), group));
        }
    }
    stmts.append(&mut derived);
    // The page's or layout's metadata, after its loads (22-metadata.md);
    // in the browser its title, once the loads it reads have values.
    if let Some(calls) = crate::server::metadata_block_calls(&node) {
        if ssr {
            let set = crate::server::meta_calls(b, &calls, &member(id("ctx"), "meta"));
            stmts.extend(set);
        } else {
            let ready = (!loads.is_empty()).then(|| call(member(id("$loads"), "values"), vec![]));
            let set = crate::server::client_title(b, &calls, ready);
            stmts.extend(set);
        }
    }
    // Effects run after the view is built.
    let mut effect_stmts = Vec::new();
    for (m, f) in effects {
        effect_stmts.push(effect(b, m, f, !loads.is_empty()));
    }
    // The view.
    let view_nodes = view.as_ref().map_or_else(Vec::new, |v| {
        let stmts = block_stmts(v);
        let mut t = Tree { b, owner: d };
        t.nodes(&stmts)
    });
    let mut out = Vec::new();
    if ssr {
        let mut s = Ssr { b, owner: d };
        let saved = std::mem::take(&mut s.b.stmts);
        s.nodes(&view_nodes);
        let body = std::mem::replace(&mut s.b.stmts, saved);
        if loads.is_empty() {
            stmts.extend(body);
        } else {
            stmts.push(ssr_marker("OPEN"));
            stmts.extend(body);
            stmts.push(ssr_marker("CLOSE"));
        }
        out.push(Stmt::Function(
            Function {
                name: format!("{name}$ssr"),
                params: vec![Param::new("props"), Param::new("ctx"), Param::new("out")],
                body: js::Body::Block(stmts),
                is_async: !loads.is_empty() || crate::app::has_async_children(g, &view_nodes),
                kind: MethodKind::Plain,
            },
            true,
        ));
    } else {
        let mut c = Client {
            b,
            owner: d,
            lifetime: if async_handlers {
                Some("$life".into())
            } else {
                None
            },
        };
        let root = if loads.is_empty() {
            let saved = std::mem::take(&mut c.b.stmts);
            let root = c.fragment(&view_nodes);
            let body = std::mem::replace(&mut c.b.stmts, saved);
            stmts.extend(body);
            root
        } else {
            // `$k.branch` over the group's state: loading, view, failed.
            let loading_nodes = loading.as_ref().map_or_else(Vec::new, |l| {
                let st = block_stmts(l);
                let mut t = Tree { b: c.b, owner: d };
                t.nodes(&st)
            });
            let failed_nodes = failed.as_ref().map_or_else(Vec::new, |l| {
                let st = block_stmts(l);
                let mut t = Tree { b: c.b, owner: d };
                t.nodes(&st)
            });
            let view_fn = c.render_fn(&view_nodes, Vec::new(), Vec::new());
            let loading_fn = c.render_fn(&loading_nodes, Vec::new(), Vec::new());
            let mut failed_prefix = Vec::new();
            if let Some(f) = &failed
                && let Some(t) = f
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == k_syntax::SyntaxKind::Ident)
            {
                let js_name = c.b.bind(&t);
                failed_prefix.push(Stmt::Const(
                    js_name,
                    call(member(id("$loads"), "error"), vec![]),
                ));
            }
            let failed_fn = c.render_fn(&failed_nodes, Vec::new(), failed_prefix);
            let m = c.b.fresh("m");
            c.b.stmts
                .push(Stmt::Const(m.clone(), rt("markers", vec![])));
            let state = call(member(id("$loads"), "state"), vec![]);
            // LOADING 0, READY 1, RELOADING 2, FAILED 3.
            let select = js::arrow(
                vec![],
                Expr::Index(
                    Box::new(Expr::Array(vec![
                        Expr::Num(0.0),
                        Expr::Num(1.0),
                        Expr::Num(1.0),
                        Expr::Num(2.0),
                    ])),
                    Box::new(state),
                ),
            );
            c.b.stmts.push(Stmt::Expr(rt(
                "branch",
                vec![
                    member(id(&m), "start"),
                    member(id(&m), "end"),
                    select,
                    Expr::Array(vec![loading_fn, view_fn, failed_fn]),
                ],
            )));
            let body = std::mem::take(&mut c.b.stmts);
            stmts.extend(body);
            member(id(m), "node")
        };
        stmts.extend(effect_stmts);
        stmts.push(Stmt::Return(Some(root)));
        if async_handlers || b.uses_life {
            stmts.insert(0, Stmt::Const("$life".into(), rt("lifetime", vec![])));
        }
        if client_only {
            // The component's own factory becomes `Name$view`, and `Name`
            // renders `loading` until the hydration pass is over: both
            // sides render the same nodes during the pass, so the ids and
            // instance keys after it match (runtime.md §6).
            out.push(Stmt::Function(
                Function {
                    name: format!("{name}$view"),
                    params: vec![Param::new("props"), Param::new("ctx")],
                    body: js::Body::Block(stmts),
                    ..Function::default()
                },
                false,
            ));
            out.push(client_swap(b, d, &name, loading.as_ref(), prop_defaults));
        } else {
            out.push(Stmt::Function(
                Function {
                    name: name.clone(),
                    params: vec![Param::new("props"), Param::new("ctx")],
                    body: js::Body::Block(stmts),
                    ..Function::default()
                },
                true,
            ));
        }
    }
    b.pop_fn_scope();
    out
}

/// The browser factory of a client-rendered component: `loading` (from the
/// props) while the hydration pass runs, then `Name$view` once the pass
/// and the `init()`s of the client-only services it created are over
/// (`$k.clientReady()`); on client navigation `view` at once.
fn client_swap(
    b: &mut Body<'_, '_>,
    d: DeclId,
    name: &str,
    loading: Option<&SyntaxNode>,
    prop_defaults: Vec<Stmt>,
) -> Stmt {
    let mut c = Client {
        b,
        owner: d,
        lifetime: None,
    };
    let loading_nodes = loading.map_or_else(Vec::new, |l| {
        let st = block_stmts(l);
        let mut t = Tree { b: c.b, owner: d };
        t.nodes(&st)
    });
    let saved = std::mem::take(&mut c.b.stmts);
    let loading_fn = c.render_fn(&loading_nodes, Vec::new(), Vec::new());
    // Nothing below the component was server-rendered: its subtree's loads
    // run in the browser instead of reading `k-data`.
    let view_ctx = Expr::Object(
        vec![
            Prop::Spread(id("ctx")),
            Prop::KeyValue("data".into(), Expr::Null),
        ],
        false,
    );
    let view_fn = js::arrow(
        vec![],
        call(id(format!("{name}$view")), vec![id("props"), view_ctx]),
    );
    let m = c.b.fresh("m");
    let ready = c.b.fresh("r");
    let mut body = prop_defaults;
    body.extend(std::mem::replace(&mut c.b.stmts, saved));
    body.push(Stmt::Const(m.clone(), rt("markers", vec![])));
    body.push(Stmt::Const(ready.clone(), rt("clientReady", vec![])));
    body.push(Stmt::Expr(rt(
        "branch",
        vec![
            member(id(&m), "start"),
            member(id(&m), "end"),
            js::arrow(
                vec![],
                Expr::Cond(
                    Box::new(call(id(&ready), vec![])),
                    Box::new(Expr::Num(1.0)),
                    Box::new(Expr::Num(0.0)),
                ),
            ),
            Expr::Array(vec![loading_fn, view_fn]),
        ],
    )));
    body.push(Stmt::Return(Some(member(id(m), "node"))));
    Stmt::Function(
        Function {
            name: name.to_owned(),
            params: vec![Param::new("props"), Param::new("ctx")],
            body: js::Body::Block(body),
            ..Function::default()
        },
        true,
    )
}

/// The name a `failed(let error)` block binds its error to.
fn failed_binding(failed: &SyntaxNode) -> Option<k_syntax::SyntaxToken> {
    failed
        .children()
        .find(|c| c.kind() == k_syntax::SyntaxKind::BindingPattern)
        .and_then(|p| {
            p.children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == k_syntax::SyntaxKind::Ident)
        })
}

/// The type of the error a `failed` block receives (`Type::Error` when it
/// binds none).
pub(crate) fn failed_error_type(b: &Body<'_, '_>, failed: &SyntaxNode) -> Type {
    failed_binding(failed).map_or(Type::Error, |t| b.g.type_at(b.file, t.text_range()))
}

/// The server's loads of a declaration with a `failed` block: awaited
/// inside a `try`; a typed error renders the `failed` view (with its own
/// metadata, which replaces the page's) and is serialized for hydration,
/// a panic propagates to the 500 page (14-async-data.md, server.md §7).
fn ssr_failed(
    b: &mut Body<'_, '_>,
    d: DeclId,
    loads: &[MemberId],
    awaited: Vec<Stmt>,
    failed: &SyntaxNode,
) -> Vec<Stmt> {
    let g = b.g;
    let names: Vec<String> = loads
        .iter()
        .map(|&m| mangle(&g.view.member(m).name))
        .collect();
    let mut out: Vec<Stmt> = names.iter().map(|n| Stmt::Let(n.clone(), None)).collect();
    let body: Vec<Stmt> = awaited
        .into_iter()
        .map(|s| match s {
            Stmt::Const(n, x) if names.contains(&n) => Stmt::Expr(js::assign(id(n), x)),
            other => other,
        })
        .collect();
    let caught = b.fresh("e");
    let mut handler = vec![Stmt::If(
        bin(
            "||",
            rt("isPanic", vec![id(&caught)]),
            bin("instanceof", id(&caught), id("Error")),
        ),
        vec![Stmt::Throw(id(&caught))],
        None,
    )];
    let binding = failed_binding(failed);
    let error_ty = failed_error_type(b, failed);
    let error = match &binding {
        Some(t) => {
            let js_name = b.bind(t);
            handler.push(Stmt::Const(js_name.clone(), id(&caught)));
            js_name
        }
        None => caught.clone(),
    };
    let encoded = match error_ty.decl() {
        Some(e) if crate::codec::has_codec(g, e) => {
            crate::app::encode_expr(b, &error_ty, id(&error))
        }
        _ => rt("toJSON", vec![id(&error)]),
    };
    handler.push(Stmt::Expr(js::assign(
        Expr::Index(
            Box::new(member(id("out"), "data")),
            Box::new(crate::app::load_key(g, d)),
        ),
        Expr::Object(vec![Prop::KeyValue("failed".into(), encoded)], false),
    )));
    let nodes = {
        let st = block_stmts(failed);
        let mut t = Tree { b, owner: d };
        t.nodes(&st)
    };
    let mut s = Ssr { b, owner: d };
    let saved = std::mem::take(&mut s.b.stmts);
    s.nodes(&nodes);
    let rendered = std::mem::replace(&mut s.b.stmts, saved);
    // The client's `@Load` branch is a block range: its markers frame
    // whichever state the server rendered.
    handler.push(ssr_marker("OPEN"));
    handler.extend(rendered);
    handler.push(ssr_marker("CLOSE"));
    handler.push(Stmt::Return(None));
    out.push(Stmt::Try(body, caught, handler));
    out
}

/// `out.raw($s.OPEN)` / `out.raw($s.CLOSE)`.
fn ssr_marker(which: &str) -> Stmt {
    Stmt::Expr(call(
        member(id("out"), "raw"),
        vec![member(id("$s"), which.to_owned())],
    ))
}

/// `true` if `t` is a non-optional project struct: `@State` of such a type
/// is held in a path store (`$k.store`), so field writes update only the
/// bindings that read them, rather than in a plain signal.
fn is_struct(g: &Gen<'_>, t: Option<&Type>) -> bool {
    t.is_some_and(|t| {
        !matches!(t, Type::Optional(_))
            && g.kind_of(t) == Some(DeclarationKind::Struct)
            && !g.view.decl(t.decl().unwrap_or(DeclId(0))).stdlib
    })
}

/// An `@Effect` function: `$k.effect(…)`, with `$c` when async, and
/// `untrack` around the body for `@Effect(on: […])`.
/// With `loads`, the effect waits for the declaration's `@Load` values: it
/// reads whether there are any (so it runs again once they arrive, not on
/// every reload) and returns while there are none, because its body may
/// read them.
fn effect(b: &mut Body<'_, '_>, m: MemberId, f: Function, loads: bool) -> Stmt {
    let node = b.g.member_node(m);
    // The `on:` dependencies, if any.
    let deps: Option<Vec<ast::Expr>> = node.as_ref().and_then(|n| {
        n.children()
            .filter(|c| c.kind() == k_syntax::SyntaxKind::Annotation)
            .find(|a| a.text().to_string().starts_with("@Effect"))
            .and_then(ast::Annotation::cast)
            .and_then(|a| a.arg_list())
            .and_then(|l| {
                l.args()
                    .find(|x| x.label().is_some_and(|t| t.text() == "on"))
            })
            .and_then(|x| x.value())
            .map(|v| v.syntax().children().filter_map(ast::Expr::cast).collect())
    });
    let body = match f.body {
        js::Body::Block(s) => s,
        js::Body::Expr(e) => vec![Stmt::Expr(*e)],
    };
    let params = if f.is_async {
        vec![Param::new("$c")]
    } else {
        Vec::new()
    };
    let body = match deps {
        Some(deps) => {
            let mut s: Vec<Stmt> = deps.iter().map(|d| Stmt::Expr(b.expr(d))).collect();
            let inner = js::arrow_block(Vec::new(), body, f.is_async);
            let untracked = rt("untrack", vec![inner]);
            s.push(Stmt::Expr(if f.is_async {
                Expr::Await(Box::new(untracked))
            } else {
                untracked
            }));
            s
        }
        None => body,
    };
    let body = if loads {
        // `ready()` changes only between no values and values, so a
        // reload's new values don't re-run an effect that waits for them
        // (an `@Effect(on:)` re-runs only for its list).
        let mut s = vec![Stmt::If(
            js::un("!", call(member(id("$loads"), "ready"), vec![])),
            vec![Stmt::Return(None)],
            None,
        )];
        s.extend(body);
        s
    } else {
        body
    };
    Stmt::Expr(rt(
        "effect",
        vec![js::arrow_block(params, body, f.is_async)],
    ))
}

/// A service: a class whose constructor receives its dependencies in
/// `@Inject` order; `@State` fields are signals and `@Derived` fields
/// computed values on the client (codegen.md §13).
#[allow(clippy::too_many_lines)]
pub(crate) fn service(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let g = b.g;
    let info = g.view.decl(d).clone();
    if info.stdlib {
        return Vec::new();
    }
    b.owner = Some(d);
    b.self_mode = SelfMode::This;
    let client = b.target == Target::Client;
    let name = g.name(d);
    let mut ctor = Vec::new();
    let mut methods = Vec::new();
    let mut statics = Vec::new();
    for &m in &info.members {
        let mi = g.view.member(m).clone();
        let mnode = g.member_node(m);
        match mi.kind {
            MemberKind::Field if !mi.is_static => {
                let init = mnode
                    .as_ref()
                    .and_then(|x| x.children().find_map(ast::Expr::cast));
                if mi.has_annotation("Inject") {
                    let t = mi.ty.clone().unwrap_or(Type::Error);
                    let tn = t.decl().map(|x| g.name(x)).unwrap_or_default();
                    ctor.push(Stmt::Expr(js::assign(
                        member(Expr::This, mi.name.clone()),
                        member(id("$deps"), tn),
                    )));
                    continue;
                }
                let (pre, x) = b.capture(|b| init.as_ref().map_or(Expr::Null, |e| b.expr(e)));
                if mi.has_annotation("Derived") && client {
                    let f = if pre.is_empty() {
                        js::arrow(vec![], x)
                    } else {
                        let mut s = pre;
                        s.push(Stmt::Return(Some(x)));
                        js::arrow_block(vec![], s, false)
                    };
                    ctor.push(Stmt::Expr(js::assign(
                        member(Expr::This, mi.name.clone()),
                        rt("computed", vec![f]),
                    )));
                    continue;
                }
                ctor.extend(pre);
                let v = if mi.has_annotation("State") && client {
                    rt("signal", vec![x])
                } else {
                    x
                };
                ctor.push(Stmt::Expr(js::assign(
                    member(Expr::This, mi.name.clone()),
                    v,
                )));
            }
            MemberKind::Func | MemberKind::Rpc | MemberKind::Init => {
                let is_static = mi.is_static && mi.kind == MemberKind::Func;
                b.self_mode = if is_static {
                    SelfMode::None
                } else {
                    SelfMode::This
                };
                let mut f = crate::decl::function(
                    b,
                    g.member_name(m),
                    mnode.as_ref(),
                    mi.signature.as_ref(),
                    Vec::new(),
                );
                b.self_mode = SelfMode::This;
                // `Labels.make()`: a static method of the class.
                if is_static {
                    f.kind = MethodKind::Static;
                }
                if (mi.kind == MemberKind::Init || mi.name == "init")
                    && mi.signature.as_ref().is_none_or(|s| s.params.is_empty())
                {
                    // `init()` runs after the fields. A client-only service's
                    // never runs during server rendering, and in the browser
                    // only after the hydration pass, so the first paint
                    // shows its declared initial state on both sides
                    // (10-services.md, "Initialization").
                    methods.push(Function {
                        name: "init".into(),
                        ..f
                    });
                    let run = call(member(Expr::This, "init"), vec![]);
                    match (info.side == Some(k_sema::Side::ClientOnly), client) {
                        (false, _) => ctor.push(Stmt::Expr(run)),
                        (true, true) => ctor.push(Stmt::Expr(rt(
                            "afterHydration",
                            vec![js::arrow(vec![], run)],
                        ))),
                        (true, false) => {}
                    }
                } else {
                    methods.push(f);
                }
            }
            MemberKind::Computed => {
                let mut f =
                    crate::decl::function(b, mi.name.clone(), mnode.as_ref(), None, Vec::new());
                f.kind = MethodKind::Getter;
                methods.push(f);
            }
            // `static let` fields: properties of the class, set once after
            // it, as a struct's or class's are (`decl.rs`).
            MemberKind::Field => {
                b.self_mode = SelfMode::None;
                let init = mnode
                    .as_ref()
                    .and_then(|x| x.children().find_map(ast::Expr::cast));
                let (pre, e) = b.capture(|b| init.as_ref().map_or(Expr::Null, |e| b.expr(e)));
                statics.extend(pre);
                statics.push(Stmt::Expr(js::assign(
                    member(id(name.clone()), g.member_name(m)),
                    e,
                )));
                b.self_mode = SelfMode::This;
            }
            _ => {}
        }
    }
    let mut all = vec![Function {
        kind: MethodKind::Constructor,
        params: vec![Param {
            name: "$deps".into(),
            default: Some(Expr::Object(Vec::new(), false)),
            rest: false,
        }],
        body: js::Body::Block(ctor),
        ..Function::default()
    }];
    let defaults = crate::decl::stdlib_defaults(b, d, &methods);
    methods.extend(defaults);
    all.extend(methods);
    let mut out = vec![Stmt::Class(
        Class {
            name,
            extends: None,
            methods: all,
        },
        true,
    )];
    out.extend(statics);
    out
}

/// A protocol: on the client, the RPC stub class `P$client` whose methods
/// post to the rpc paths; on the server, nothing when the app implements
/// it (the implementing service is the handler, dispatched by the entry),
/// else the remote client class `P$remote`
/// (`docs/design/40-workspaces-and-services.md`): constructed with a
/// remote binding (`$s.remote…`), its methods call `$s.remoteCall` with the
/// same codecs and error mapping as the browser's stub.
pub(crate) fn protocol(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let g = b.g;
    let info = g.view.decl(d).clone();
    if info.stdlib {
        return Vec::new();
    }
    let remote = b.target == Target::Server
        && crate::server::implementation(g, d).is_none()
        && g.remote_protocols().contains(&d);
    let mut methods = Vec::new();
    for &m in &info.members {
        let mi = g.view.member(m).clone();
        if mi.kind != MemberKind::Rpc {
            continue;
        }
        let path = crate::app::rpc_path(g, d, m);
        let sig = mi.signature.clone();
        let mut params = vec![Param::new("$c")];
        let mut fields = Vec::new();
        if let Some(s) = &sig {
            for p in &s.params {
                let pn = mangle(&p.name);
                params.push(Param::new(pn.clone()));
                let c = crate::codec::codec_expr(b, &p.ty);
                // The wire field is the parameter's internal name
                // (28-rpc-wire-format.md), never its label; wire keys are
                // quoted (codegen.md §17.3).
                fields.push(Prop::Quoted(
                    p.name.clone(),
                    call(member(c, "encode"), vec![id(pn)]),
                ));
            }
        }
        let ret = sig.as_ref().map(|s| s.ret.clone()).unwrap_or(Type::Error);
        // A struct result (or stream element) is the message itself;
        // anything else is `{"value": …}` (server.md §6).
        let decode_message = |b: &mut Body<'_, '_>, t: &Type, message: Expr| {
            let c = crate::codec::codec_expr(b, t);
            let message = if crate::server::is_message_struct(g, t) {
                message
            } else {
                member(message, "value")
            };
            call(member(c, "decode"), vec![message, js::str("value")])
        };
        // A server stream (29-server-streaming.md): the runtime decodes
        // each element with `each` and the call returns the open stream.
        let element = crate::server::stream_element(g, &ret);
        let options = element.as_ref().map(|t| {
            let each = js::arrow(vec![Param::new("m")], decode_message(b, t, id("m")));
            // `streamed` is the runtime's streams chunk (a stdlib symbol of
            // `k.core`, bundled from `runtime/streams.js`).
            let streamed = match g.view.lookup("k.core.Stream") {
                Some(s) => b.use_decl(s, "streamed".into()),
                None => id("streamed"),
            };
            call(streamed, vec![each])
        });
        // A channel (44-channels.md): the channels chunk opens it on the
        // page's socket with `C`'s encoder, `S`'s decoder and the refusal
        // `E`'s mapping, and the call returns the open channel.
        let channel = crate::server::channel_types(g, &ret);
        let decode = if element.is_some() || channel.is_some() {
            member(id("o"), "message")
        } else if ret.decl() == g.b.void {
            Expr::Undefined
        } else {
            decode_message(b, &ret, member(id("o"), "message"))
        };
        let call_rpc = if let Some((sent, received, refused)) = &channel {
            let enc = crate::codec::codec_expr(b, sent);
            let dec = crate::codec::codec_expr(b, received);
            let refusal = rpc_error(b, Some(refused));
            let codec = Expr::Object(
                vec![
                    Prop::KeyValue(
                        "encode".into(),
                        js::arrow(
                            vec![Param::new("m")],
                            call(member(enc, "encode"), vec![id("m")]),
                        ),
                    ),
                    Prop::KeyValue(
                        "decode".into(),
                        js::arrow(
                            vec![Param::new("j")],
                            call(member(dec, "decode"), vec![id("j"), js::str("value")]),
                        ),
                    ),
                    Prop::KeyValue("refusal".into(), js::arrow(vec![Param::new("o")], refusal)),
                ],
                false,
            );
            // `openChannel` is the channels chunk (a stdlib symbol of
            // `k.rpc`, bundled from `runtime/channels.js`).
            let open = match g.view.lookup("k.rpc.Channel") {
                Some(d) => b.use_decl(d, "openChannel".into()),
                None => id("openChannel"),
            };
            call(
                open,
                vec![js::str(path), Expr::Object(fields, false), id("$c"), codec],
            )
        } else if remote {
            let mut args = vec![
                member(Expr::This, "r"),
                js::str(path),
                Expr::Object(fields, false),
                id("$c"),
            ];
            args.extend(options);
            call(member(id("$s"), "remoteCall"), args)
        } else {
            let mut args = vec![js::str(path), Expr::Object(fields, false), id("$c")];
            args.extend(options);
            rt("rpc", args)
        };
        let err = sig.as_ref().and_then(|s| s.throws.clone());
        let fail = rpc_error(b, err.as_ref());
        let body = vec![
            Stmt::Const("o".into(), Expr::Await(Box::new(call_rpc))),
            Stmt::If(
                member(id("o"), "ok"),
                vec![Stmt::Return(Some(decode))],
                None,
            ),
            Stmt::Throw(fail),
        ];
        methods.push(Function {
            name: g.member_name(m),
            params,
            body: js::Body::Block(body),
            is_async: true,
            kind: MethodKind::Plain,
        });
    }
    if remote {
        methods.insert(
            0,
            Function {
                kind: MethodKind::Constructor,
                params: vec![Param::new("r")],
                body: js::Body::Block(vec![Stmt::Expr(js::assign(
                    member(Expr::This, "r"),
                    id("r"),
                ))]),
                ..Function::default()
            },
        );
        return vec![Stmt::Class(
            Class {
                name: format!("{}$remote", g.name(d)),
                extends: None,
                methods,
            },
            true,
        )];
    }
    if b.target == Target::Server || methods.is_empty() {
        return Vec::new();
    }
    vec![Stmt::Class(
        Class {
            name: format!("{}$client", g.name(d)),
            extends: None,
            methods,
        },
        true,
    )]
}

/// `$k.rpcError(o, E, codec)`: the failed outcome `o` of an rpc (or a
/// channel's send) as its error enum `err`; a Codable error's detail is
/// decoded with its codec (server.md §6), so its payload has its declared
/// types.
fn rpc_error(b: &mut Body<'_, '_>, err: Option<&Type>) -> Expr {
    match err.and_then(Type::decl) {
        Some(e) if crate::codec::has_codec(b.g, e) => {
            let f = b.decl_ref(e);
            let c = crate::codec::codec_expr(b, err.unwrap_or(&Type::Error));
            rt("rpcError", vec![id("o"), f, c])
        }
        Some(e) => {
            let f = b.decl_ref(e);
            rt("rpcError", vec![id("o"), f])
        }
        None => rt("rpcError", vec![id("o"), Expr::Null]),
    }
}

/// A config: built from the environment at server startup
/// (codegen.md §13, server.md §8).
pub(crate) fn config(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    if b.target == Target::Client {
        return Vec::new();
    }
    let g = b.g;
    let object = config_object(b, d);
    let mut out = vec![Stmt::Function(
        Function {
            name: format!("{}$load", g.name(d)),
            params: Vec::new(),
            body: js::Body::Block(vec![Stmt::Return(Some(object))]),
            ..Function::default()
        },
        true,
    )];
    // Tests bind configs to values they build (`use AdminConfig.sample for
    // AdminConfig`, 25-config-and-secrets.md), with the memberwise
    // constructor.
    if g.options.entry == crate::Entry::Tests {
        b.owner = Some(d);
        out.push(crate::decl::memberwise_constructor(b, d));
    }
    out
}

/// The value of config `d` (an app's own or the stdlib's): an object with
/// each `@Env` field read from the validated configuration, a secret
/// wrapped in `Secret` and an enum by case.
pub(crate) fn config_object(b: &mut Body<'_, '_>, d: DeclId) -> Expr {
    let g = b.g;
    let info = g.view.decl(d).clone();
    let mut props = Vec::new();
    for &m in &info.members {
        let mi = g.view.member(m).clone();
        if mi.kind != MemberKind::Field {
            continue;
        }
        let env = g
            .member_node(m)
            .and_then(|n| {
                let text = n.text().to_string();
                let i = text.find("@Env(\"")?;
                let rest = &text[i + 6..];
                Some(rest[..rest.find('"')?].to_owned())
            })
            .or_else(|| crate::server::stdlib_config_env(g, d, &mi.name));
        let t = mi.ty.clone().unwrap_or(Type::Error);
        // `Secret<T>?` is a secret too: `nil` when unset, else wrapped.
        let secret = g.is(t.unwrap_optional(), g.b.secret);
        let name = js::str(
            env.unwrap_or_else(|| crate::css::kebab(&mi.name).to_uppercase().replace('-', "_")),
        );
        // Values arrive validated and typed from startup (server.md §8):
        // a secret wrapped in `Secret`, an enum by case name.
        let mut element = t.unwrap_optional().clone();
        if g.is_list(&element) {
            element = element.args().first().cloned().unwrap_or(Type::Error);
        }
        let enum_cases = element
            .decl()
            .filter(|&e| g.view.decl(e).named_kind() == Some(DeclarationKind::Enum) && !secret);
        // A key type (33-crypto.md, "Keys in configuration"), held as
        // itself or in a `Secret`, alone or in a list: parsed by
        // `k.crypto` from the text startup checked.
        let mut key = t.unwrap_optional().clone();
        if secret {
            key = key.args().first().cloned().unwrap_or(Type::Error);
        }
        if g.is_list(&key) {
            key = key.args().first().cloned().unwrap_or(Type::Error);
        }
        let key_type = key
            .decl()
            .map(|k| g.view.qualified_name(k))
            .and_then(|q| crate::server::config_key_type(&q).map(str::to_owned));
        let v = if let Some(key_type) = key_type {
            call(
                member(id("$s"), "configKey"),
                vec![name, js::str(key_type), Expr::Bool(secret)],
            )
        } else if secret {
            call(member(id("$s"), "configSecret"), vec![name])
        } else if let Some(e) = enum_cases {
            let r = b.decl_ref(e);
            let cases = g
                .cases(e)
                .iter()
                .map(|&c| {
                    let n = g.view.member(c).name.clone();
                    Prop::KeyValue(n.clone(), member(r.clone(), n))
                })
                .collect();
            call(
                member(id("$s"), "configEnum"),
                vec![name, Expr::Object(cases, false)],
            )
        } else {
            call(member(id("$s"), "config"), vec![name])
        };
        props.push(Prop::KeyValue(mi.name.clone(), v));
    }
    Expr::Object(props, true)
}

/// A repository: a class whose functions run their queries as SQL
/// (`crate::db`, db.md §7).
pub(crate) fn repository(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    if b.target == Target::Client {
        return Vec::new();
    }
    crate::db::repository(b, d)
}

/// `true` for nodes that need an await in the server renderer.
#[allow(dead_code)]
pub(crate) fn needs_await(_n: &VNode) -> bool {
    false
}
