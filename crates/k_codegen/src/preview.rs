//! The preview bundle ([`crate::Entry::Previews`];
//! `docs/design/57-previews.md`; `docs/compiler/codegen.md` §21.3).
//!
//! Previews never reach a build: every other entry drops them with the
//! rest of the test code. A preview bundle is a test bundle
//! ([`crate::suite`]) whose groups are the selected previews instead of
//! the test groups, so a preview gets exactly a test's hermetic wiring: a
//! fresh server and browser container with the `k test` defaults, the
//! group's `use` rules (with `App.k`'s `.test` rules under them), an
//! in-memory database with every migration applied and one session.
//!
//! Each preview lowers to a group class (`$group<N>`, its `@Inject` fields
//! read the containers as a test group's do) with two methods:
//!
//! * `$mount($c, values)` binds each parameter to a signal holding
//!   `values[name]` when the key is present, else its default, so a
//!   `bind value` writes back; runs the setup; and returns the kit's
//!   `render` of the view: a component or layout as written, a page inside
//!   its layouts after its guards are checked against the session the setup
//!   left (`previewGuards`), as the router shows it after navigating there;
//! * `$t0($c)`, the group's one test (`mounts`), mounts it with the
//!   defaults and waits for it to settle: what `k check` runs, through the
//!   kit's `run`, reporting a failure as K7520 at the `preview` keyword.
//!
//! The entry exports `$groups` and `$pages` as a test bundle's, and
//! `$previews`: per preview, in the same order as `$groups` and as
//! [`k_sema::Analysis::previews`], `{label, file, start, end, params}`
//! (`start`/`end` are the keyword's offsets, `params` the parameter names).

use k_sema::stdlib::DeclarationKind;
use k_sema::view::DeclId;
use k_syntax::ast::{self, AstNode};

use crate::cx::Gen;
use crate::js::{self, Expr, Function, MethodKind, Param, Prop, Stmt, call, id, member};
use crate::lower::Body;

/// The name of a preview's one test in `$groups`.
pub(crate) const TEST_NAME: &str = "mounts";

/// `true` when `d` is a preview the bundle's entry selects.
pub(crate) fn selected(g: &Gen<'_>, d: DeclId) -> bool {
    let info = g.view.decl(d);
    match &g.options.entry {
        crate::Entry::Previews(file) => {
            info.named_kind() == Some(DeclarationKind::Preview)
                && file.is_none_or(|f| info.file == f)
        }
        _ => false,
    }
}

/// The selected previews, in file path order, then source order.
pub(crate) fn previews(g: &Gen<'_>) -> Vec<DeclId> {
    let mut out: Vec<DeclId> = g.own.iter().copied().filter(|&d| selected(g, d)).collect();
    let path = |d: DeclId| path_of(g, d);
    out.sort_by_key(|&d| (path(d), g.view.decl(d).range.start()));
    out
}

/// The project path of the file declaring `d`.
fn path_of(g: &Gen<'_>, d: DeclId) -> String {
    let f = g.view.decl(d).file;
    g.analysis
        .files()
        .into_iter()
        .find(|(id, _)| *id == f)
        .map(|(_, p)| p.as_str().to_owned())
        .unwrap_or_default()
}

/// The label of preview `d`, as [`k_sema::Analysis::previews`] gives it.
pub(crate) fn label(g: &Gen<'_>, d: DeclId) -> String {
    let info = g.view.decl(d);
    g.analysis
        .previews(info.file)
        .ok()
        .and_then(|list| list.into_iter().find(|p| p.decl == d))
        .map_or_else(|| "Preview".to_owned(), |p| p.label)
}

/// `$mount($c, values)` and `$t0($c)` of preview `d`'s group class.
pub(crate) fn methods(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Function> {
    let g = b.g;
    let Some(node) = g.decl_node(d).and_then(ast::PreviewDecl::cast) else {
        return Vec::new();
    };
    let sig = g.view.decl(d).signature.clone();
    b.owner = Some(d);
    b.self_mode = crate::lower::SelfMode::This;
    b.push_fn_scope();
    let saved_c = b.has_c;
    b.has_c = true;
    let mut body = Vec::new();
    // Each parameter is a signal: `values[name]` when given, else its
    // default.
    let params: Vec<ast::Param> = node
        .param_list()
        .map(|l| l.params().collect())
        .unwrap_or_default();
    let mut cells = Vec::new();
    for (param, sp) in params.iter().zip(sig.iter().flat_map(|s| s.params.iter())) {
        let default = match param.default_value() {
            Some(e) => {
                let (pre, x) = b.capture(|b| {
                    let x = b.expr(&e);
                    let from = b.ty(e.syntax());
                    b.coerce(x, &from, &sp.ty)
                });
                if pre.is_empty() {
                    x
                } else {
                    let mut s = pre;
                    s.push(Stmt::Return(Some(x)));
                    call(js::arrow_block(vec![], s, false), vec![])
                }
            }
            None => Expr::Null,
        };
        let js = b.bind_name(&sp.name, sp.name_range.start());
        let given = call(
            member(id("Object"), "hasOwn"),
            vec![id("$values"), js::str(sp.name.clone())],
        );
        let value = Expr::Cond(
            Box::new(given),
            Box::new(member(id("$values"), sp.name.clone())),
            Box::new(default),
        );
        body.push(Stmt::Const(js.clone(), js::rt("signal", vec![value])));
        b.cell_locals.insert(js.clone());
        cells.push(js);
    }
    // The setup, then the view.
    let stmts: Vec<ast::Stmt> = node
        .body()
        .map(|block| block.statements().collect())
        .unwrap_or_default();
    let (view, setup) = match stmts.split_last() {
        Some((ast::Stmt::Expr(e), setup)) => (Some(e.clone()), setup.to_vec()),
        _ => (None, stmts),
    };
    let (lowered, ()) = b.capture(|b| {
        for s in &setup {
            b.stmt(s);
        }
    });
    body.extend(lowered);
    let (pre, screen) = b.capture(|b| {
        view.as_ref()
            .and_then(|v| crate::suite::render_view(b, v, true))
    });
    body.extend(pre);
    body.push(Stmt::Return(Some(screen.unwrap_or(Expr::Null))));
    for c in cells {
        b.cell_locals.remove(&c);
    }
    b.has_c = saved_c;
    b.pop_fn_scope();
    let mount = Function {
        name: "$mount".into(),
        params: vec![
            Param::new("$c"),
            Param {
                name: "$values".into(),
                default: Some(Expr::Object(Vec::new(), false)),
                rest: false,
            },
        ],
        body: js::Body::Block(body),
        is_async: true,
        kind: MethodKind::Plain,
    };
    // `k check`: mounted with the defaults, settled.
    let test = Function {
        name: "$t0".into(),
        params: vec![Param::new("$c")],
        body: js::Body::Block(vec![
            Stmt::Const(
                "$s".into(),
                Expr::Await(Box::new(call(
                    member(Expr::This, "$mount"),
                    vec![id("$c"), Expr::Object(Vec::new(), false)],
                ))),
            ),
            Stmt::Expr(Expr::Await(Box::new(crate::suite::kit_call(
                "settlePreview",
                vec![id("$s")],
            )))),
        ]),
        is_async: true,
        kind: MethodKind::Plain,
    };
    vec![mount, test]
}

/// `export const $previews = [{label, file, start, end, params}]`, one per
/// group of `groups` (the previews), in order.
pub(crate) fn exports(g: &Gen<'_>, groups: &[DeclId]) -> Stmt {
    let mut out = Vec::new();
    for &d in groups {
        let info = g.view.decl(d);
        let params = info
            .signature
            .iter()
            .flat_map(|s| s.params.iter())
            .map(|p| js::str(p.name.clone()))
            .collect();
        out.push(Expr::Object(
            vec![
                Prop::KeyValue("label".into(), js::str(label(g, d))),
                Prop::KeyValue("file".into(), js::str(path_of(g, d))),
                Prop::KeyValue(
                    "start".into(),
                    Expr::Num(f64::from(u32::from(info.name_range.start()))),
                ),
                Prop::KeyValue(
                    "end".into(),
                    Expr::Num(f64::from(u32::from(info.name_range.end()))),
                ),
                Prop::KeyValue("params".into(), Expr::Array(params)),
            ],
            false,
        ));
    }
    Stmt::ExportConst("$previews".into(), Expr::Array(out))
}
