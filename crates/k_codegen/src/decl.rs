//! Lowering of declarations (`docs/compiler/codegen.md` §5, §9, §13):
//! functions, constants, structs, enums, classes, interfaces, extensions
//! and services. Components, pages and layouts are [`crate::ui`].
//!
//! Each declaration lowers to an [`Item`]: the module-level statements it
//! contributes to one side's file, and the symbols of other declarations
//! it references (for imports and reachability).

use std::collections::BTreeSet;

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, DeclKind, MemberId, MemberKind, Type};
use k_syntax::SyntaxNode;
use k_syntax::ast::{self, AstNode};

use crate::cx::{Gen, Target, mangle};
use crate::js::{
    self, Class, Expr, Function, MethodKind, Param, Prop, Stmt, bin, call, id, member, rt,
};
use crate::lower::{Body, SelfMode};

/// What one declaration contributes to one side.
#[derive(Clone, Debug, Default)]
pub(crate) struct Item {
    /// The module-level statements of the declaration's own code, in
    /// output order.
    pub stmts: Vec<Stmt>,
    /// The symbols of other declarations it references.
    pub uses: BTreeSet<(DeclId, String)>,
    /// Module-level templates and constants it needs before its code.
    pub prelude: Vec<Stmt>,
    /// Code emitted only when one of its symbols is referenced: text
    /// functions, codecs, interface adapters and witness tables.
    pub on_demand: Vec<OnDemand>,
}

/// A piece of a declaration's code that is emitted only when used.
#[derive(Clone, Debug, Default)]
pub(crate) struct OnDemand {
    /// The symbols it defines (`Point$codec`, `Point$encode`, …).
    pub symbols: Vec<String>,
    /// The statements that define [`OnDemand::symbols`].
    pub stmts: Vec<Stmt>,
    /// The symbols of other declarations it references.
    pub uses: BTreeSet<(DeclId, String)>,
}

/// Lowers declaration `d` for `target`; `None` when it contributes
/// nothing there.
pub(crate) fn lower(g: &Gen<'_>, d: DeclId, target: Target) -> Option<Item> {
    let info = g.view.decl(d);
    let file = info.file;
    let mut b = Body::new(g, file, target);
    let stmts = if crate::app::is_generated_enum(g, d) {
        if g.view.lookup("k.core.Page") == Some(d) {
            crate::app::page_enum(&mut b)
        } else {
            crate::app::theme_variant_enum(&mut b)
        }
    } else {
        match info.kind {
            DeclKind::Extension => extension(&mut b, d),
            DeclKind::Named(kind) => match kind {
                DeclarationKind::Function => function_decl(&mut b, d),
                DeclarationKind::Constant => constant(&mut b, d),
                DeclarationKind::Struct | DeclarationKind::Entity => struct_decl(&mut b, d),
                DeclarationKind::Enum => {
                    let mut stmts = enum_decl(&mut b, d);
                    // The app's endpoint pages: their URLs too.
                    if g.view.lookup("k.core.Endpoint") == Some(d) {
                        stmts.push(crate::app::endpoint_href(&mut b));
                    }
                    stmts
                }
                DeclarationKind::Class => class_decl(&mut b, d),
                DeclarationKind::Interface => interface_decl(&mut b, d),
                DeclarationKind::Service if crate::workers::is_worker(g, d) => {
                    crate::workers::lower(&mut b, d)
                }
                DeclarationKind::Service => {
                    let mut stmts = crate::ui::service(&mut b, d);
                    if target == Target::Server {
                        stmts.extend(crate::jobs::kind_stmt(g, d));
                    }
                    stmts
                }
                DeclarationKind::Component | DeclarationKind::Page | DeclarationKind::Layout => {
                    crate::ui::component(&mut b, d)
                }
                DeclarationKind::Theme => Vec::new(),
                DeclarationKind::Protocol => crate::ui::protocol(&mut b, d),
                DeclarationKind::Config => crate::ui::config(&mut b, d),
                DeclarationKind::Repository => crate::ui::repository(&mut b, d),
                DeclarationKind::TestSuite
                    if g.options.entry == crate::Entry::Tests
                        && target == Target::Client
                        && !info.name.is_empty() =>
                {
                    crate::suite::group(&mut b, d)
                }
                DeclarationKind::App | DeclarationKind::Migration | DeclarationKind::TestSuite => {
                    Vec::new()
                }
            },
        }
    };
    let on_demand = std::mem::take(&mut b.on_demand);
    if stmts.is_empty() && on_demand.is_empty() {
        return None;
    }
    let prelude = std::mem::take(&mut b.prelude);
    Some(Item {
        stmts,
        uses: b.uses,
        prelude,
        on_demand,
    })
}

/// Lowers the parameters of `sig` into `b` (binding their names) and
/// returns them, with `$c` first for async functions and witnesses after.
pub(crate) fn params(
    b: &mut Body<'_, '_>,
    node: Option<&SyntaxNode>,
    sig: Option<&k_sema::view::Signature>,
    leading: Vec<Param>,
) -> Vec<Param> {
    let mut out = leading;
    let Some(sig) = sig else { return out };
    if sig.is_async {
        out.push(Param::new("$c"));
        b.has_c = true;
    }
    for g in &sig.generics {
        if needs_witness_param(b, *g) {
            let name = format!("$w_{}", b.g.view.generics[g.0 as usize].name);
            b.witnesses.insert(*g, name.clone());
            out.push(Param::new(name));
        }
        if b.is_error_generic(*g) {
            let name = format!("$e_{}", b.g.view.generics[g.0 as usize].name);
            b.error_witnesses.insert(*g, name.clone());
            out.push(Param::new(name));
        }
    }
    let defaults: Vec<Option<ast::Expr>> = node
        .and_then(|n| n.children().find_map(ast::ParamList::cast))
        .map(|l| l.params().map(|p| p.default_value()).collect())
        .unwrap_or_default();
    for (i, p) in sig.params.iter().enumerate() {
        let js = b.bind_name(&p.name, p.name_range.start());
        let default = defaults.get(i).cloned().flatten().map(|e| {
            let x = b.expr(&e);
            let from = b.ty(e.syntax());
            b.coerce(x, &from, &p.ty)
        });
        out.push(Param {
            name: js,
            default,
            rest: false,
        });
    }
    out
}

/// `true` if the generic parameter `g` of a function needs a witness
/// parameter (`$w_T`): it is constrained to `Codable` (the witness is the
/// codec) or to a user interface whose operations go through a method
/// table ([`Body::needs_witness`]).
fn needs_witness_param(b: &Body<'_, '_>, g: k_sema::view::GenId) -> bool {
    let Some(info) = b.g.view.generics.get(g.0 as usize) else {
        return false;
    };
    info.constraints
        .iter()
        .any(|c| b.g.is(c, b.g.b.codable) || c.decl().is_some_and(|i| b.needs_witness(i)))
}

/// Lowers a function body (block) into statements.
pub(crate) fn body_stmts(b: &mut Body<'_, '_>, node: &SyntaxNode) -> Vec<Stmt> {
    let block = node.children().find_map(ast::Block::cast);
    match block {
        Some(block) => b.block_stmts(&block),
        None => Vec::new(),
    }
}

/// A function-like member or declaration as a JS function.
pub(crate) fn function(
    b: &mut Body<'_, '_>,
    name: String,
    node: Option<&SyntaxNode>,
    sig: Option<&k_sema::view::Signature>,
    leading: Vec<Param>,
) -> Function {
    b.push_fn_scope();
    let saved_c = b.has_c;
    let ps = params(b, node, sig, leading);
    // A returned value is converted to the result type (a computed
    // property's is its own type).
    let ret = match (sig, node) {
        (Some(s), _) => Some(s.ret.clone()),
        (None, Some(n)) if n.kind() == k_syntax::SyntaxKind::FieldDecl => computed_type(b, n),
        _ => None,
    };
    let saved_ret = std::mem::replace(&mut b.ret, ret);
    let body = match node {
        Some(n) => {
            // A computed property's getter block is its value.
            if n.kind() == k_syntax::SyntaxKind::FieldDecl {
                getter_stmts(b, n)
            } else {
                body_stmts(b, n)
            }
        }
        None => Vec::new(),
    };
    b.ret = saved_ret;
    b.has_c = saved_c;
    b.pop_fn_scope();
    Function {
        name,
        params: ps,
        body: js::Body::Block(body),
        is_async: sig.is_some_and(|s| s.is_async),
        kind: MethodKind::Plain,
    }
}

/// The type of the computed property declared by `field`, a member of the
/// file being lowered.
fn computed_type(b: &Body<'_, '_>, field: &SyntaxNode) -> Option<Type> {
    let range = field.text_range();
    b.g.view
        .members
        .iter()
        .find(|m| m.range == Some(range) && b.g.view.decl(m.owner).file == b.file)
        .and_then(|m| m.ty.clone())
}

/// The statements of a computed property's getter: its block's value is
/// returned.
pub(crate) fn getter_stmts(b: &mut Body<'_, '_>, field: &SyntaxNode) -> Vec<Stmt> {
    let Some(block) = field.children().find_map(ast::Block::cast) else {
        return Vec::new();
    };
    let stmts: Vec<ast::Stmt> = block.statements().collect();
    let (out, ()) = b.capture(|b| {
        let n = stmts.len();
        for (i, s) in stmts.iter().enumerate() {
            if i + 1 == n
                && let ast::Stmt::Expr(e) = s
            {
                b.return_value(e);
                continue;
            }
            b.stmt(s);
        }
    });
    out
}

/// Lowers a top-level `func` to one exported function declaration.
fn function_decl(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let info = b.g.view.decl(d).clone();
    let node = b.g.decl_node(d);
    let f = function(
        b,
        b.g.name(d),
        node.as_ref(),
        info.signature.as_ref(),
        Vec::new(),
    );
    vec![Stmt::Function(f, true)]
}

/// Lowers a top-level `let` to `export const name = value;`, preceded by
/// any statements its value needs; a constant without a value is `null`.
fn constant(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let Some(node) = b.g.decl_node(d) else {
        return Vec::new();
    };
    let value = node.children().find_map(ast::Expr::cast);
    let (pre, e) = b.capture(|b| match &value {
        Some(v) => b.expr(v),
        None => Expr::Null,
    });
    let mut out = pre;
    out.push(Stmt::ExportConst(b.g.name(d), e));
    out
}

/// Lowers the members of a value type or extension `d` to exported
/// module-level functions: each instance `func` and computed property
/// takes the receiver as a leading `self` parameter (read as
/// `self_mode`), static ones take none, and a static field becomes an
/// exported constant. Native members of stdlib declarations are skipped;
/// their bodies live in the runtime.
fn member_fns(b: &mut Body<'_, '_>, d: DeclId, self_mode: &SelfMode) -> Vec<Stmt> {
    let info = b.g.view.decl(d).clone();
    let mut out = Vec::new();
    for &m in &info.members {
        let mi = b.g.view.member(m).clone();
        if b.g.is_native_member(m) && info.stdlib {
            continue;
        }
        let node = b.g.member_node(m);
        match mi.kind {
            MemberKind::Func => {
                let leading = if mi.is_static {
                    Vec::new()
                } else {
                    b.self_mode = self_mode.clone();
                    vec![Param::new("self")]
                };
                if mi.is_static {
                    b.self_mode = SelfMode::None;
                }
                let f = function(
                    b,
                    b.g.fn_name(m),
                    node.as_ref(),
                    mi.signature.as_ref(),
                    leading,
                );
                out.push(Stmt::Function(f, true));
            }
            MemberKind::Computed => {
                let leading = if mi.is_static {
                    Vec::new()
                } else {
                    vec![Param::new("self")]
                };
                b.self_mode = if mi.is_static {
                    SelfMode::None
                } else {
                    self_mode.clone()
                };
                let f = function(b, b.g.fn_name(m), node.as_ref(), None, leading);
                out.push(Stmt::Function(f, true));
            }
            MemberKind::Field if mi.is_static => {
                b.self_mode = SelfMode::None;
                let value = node
                    .as_ref()
                    .and_then(|n| n.children().find_map(ast::Expr::cast));
                let (pre, e) = b.capture(|b| match &value {
                    Some(v) => b.expr(v),
                    None => Expr::Null,
                });
                out.extend(pre);
                out.push(Stmt::ExportConst(b.g.fn_name(m), e));
            }
            _ => {}
        }
    }
    out
}

/// The memberwise constructor of struct (or config) `d`: one parameter per
/// stored field, in order, with the field's default.
pub(crate) fn memberwise_constructor(b: &mut Body<'_, '_>, d: DeclId) -> Stmt {
    let name = b.g.name(d);
    // A `@Native` field of a stdlib struct is computed (`File.size`).
    let fields: Vec<MemberId> =
        b.g.stored_fields(d)
            .into_iter()
            .filter(|&f| !b.g.view.member(f).has_annotation("Native"))
            .collect();
    b.push_fn_scope();
    let mut ps = Vec::new();
    let mut props = Vec::new();
    for &f in &fields {
        let mi = b.g.view.member(f).clone();
        let js = b.bind_name(&mi.name, mi.name_range.start());
        // A stdlib struct's defaults are read from the stdlib's source,
        // lowered with that file's types (`LineSeries.dashed = false`).
        let (file, node) = match b.g.member_node(f) {
            Some(n) => (b.file, Some(n)),
            None => {
                b.g.stdlib_member_node(f)
                    .map_or((b.file, None), |(file, n)| (file, Some(n)))
            }
        };
        let saved_file = std::mem::replace(&mut b.file, file);
        let default = node
            .and_then(|n| n.children().find_map(ast::Expr::cast))
            .map(|e| {
                let x = b.expr(&e);
                let from = b.ty(e.syntax());
                b.coerce(x, &from, mi.ty.as_ref().unwrap_or(&Type::Error))
            });
        b.file = saved_file;
        ps.push(Param {
            name: js.clone(),
            default,
            rest: false,
        });
        props.push(Prop::KeyValue(mi.name.clone(), id(js)));
    }
    b.pop_fn_scope();
    Stmt::Function(
        Function {
            name,
            params: ps,
            body: js::Body::Block(vec![Stmt::Return(Some(Expr::Object(props, false)))]),
            ..Function::default()
        },
        true,
    )
}

/// Lowers a `struct` or `entity` to its memberwise constructor, one function per
/// declared initializer, its member functions and its on-demand extras
/// (text, codecs, adapters). An `@Native` struct lowers to nothing: the
/// runtime provides it.
fn struct_decl(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let info = b.g.view.decl(d).clone();
    if info.has_annotation("Native") {
        return Vec::new();
    }
    b.owner = Some(d);
    // The memberwise constructor.
    let mut out = vec![memberwise_constructor(b, d)];
    // Declared initializers.
    for &m in &info.members {
        let mi = b.g.view.member(m).clone();
        if mi.kind != MemberKind::Init {
            continue;
        }
        let node = b.g.member_node(m);
        let mut ib = Body::new(b.g, b.file, b.target);
        ib.begin_struct_init(d);
        ib.push_fn_scope();
        let ps = params(&mut ib, node.as_ref(), mi.signature.as_ref(), Vec::new());
        let mut body: Vec<Stmt> = ib
            .init_fields_names()
            .into_iter()
            .map(|n| Stmt::Let(n, None))
            .collect();
        if let Some(n) = &node {
            body.extend(body_stmts(&mut ib, n));
        }
        if !crate::lower::ends_flow(&body) {
            body.push(Stmt::Return(Some(ib.init_object())));
        }
        ib.pop_fn_scope();
        b.uses.extend(ib.uses);
        out.push(Stmt::Function(
            Function {
                name: b.g.fn_name(m),
                params: ps,
                body: js::Body::Block(body),
                ..Function::default()
            },
            true,
        ));
    }
    out.extend(member_fns(b, d, &SelfMode::Value));
    out.extend(value_extras(b, d));
    out
}

/// Text, codecs, adapters and witness tables of a struct or enum.
fn value_extras(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let name = b.g.name(d);
    on_demand(b, vec![format!("{name}$$text")], |b| vec![text_fn(b, d)]);
    if crate::codec::has_codec(b.g, d) {
        on_demand(
            b,
            vec![
                format!("{name}$codec"),
                format!("{name}$encode"),
                format!("{name}$decode"),
            ],
            |b| crate::codec::emit_codec(b, d),
        );
    }
    for (symbols, stmts) in adapters(b, d) {
        b.on_demand.push(OnDemand {
            symbols,
            stmts,
            uses: BTreeSet::new(),
        });
    }
    Vec::new()
}

/// Lowers `f` as an on-demand piece defining `symbols`.
pub(crate) fn on_demand(
    b: &mut Body<'_, '_>,
    symbols: Vec<String>,
    f: impl FnOnce(&mut Body<'_, '_>) -> Vec<Stmt>,
) {
    let saved = std::mem::take(&mut b.uses);
    let stmts = f(b);
    let uses = std::mem::replace(&mut b.uses, saved);
    b.on_demand.push(OnDemand {
        symbols,
        stmts,
        uses,
    });
}

/// `T$$text(v)`: the interpolation text (enum case names, struct fields).
/// Two `$`, so it can't collide with the member function `T$text` of a
/// member named `text`.
fn text_fn(b: &mut Body<'_, '_>, d: DeclId) -> Stmt {
    let info = b.g.view.decl(d).clone();
    let name = b.g.name(d);
    let body = if info.named_kind() == Some(DeclarationKind::Enum) {
        let names: Vec<Expr> =
            b.g.cases(d)
                .iter()
                .map(|&c| js::str(b.g.view.member(c).name.clone()))
                .collect();
        vec![Stmt::Return(Some(Expr::Index(
            Box::new(Expr::Array(names)),
            Box::new(member(id("v"), "$")),
        )))]
    } else {
        let mut parts = vec![js::TemplatePart::Text(format!("{}(", info.name))];
        let fields = b.g.stored_fields(d);
        for (i, &f) in fields.iter().enumerate() {
            let mi = b.g.view.member(f).clone();
            let sep = if i == 0 { "" } else { ", " };
            parts.push(js::TemplatePart::Text(format!("{sep}{}: ", mi.name)));
            let t = mi.ty.clone().unwrap_or(Type::Error);
            let v = b.text_of(member(id("v"), mi.name.clone()), &t);
            parts.push(js::TemplatePart::Expr(v));
        }
        parts.push(js::TemplatePart::Text(")".into()));
        vec![Stmt::Return(Some(Expr::Template(parts)))]
    };
    Stmt::Function(
        Function {
            name: format!("{name}$$text"),
            params: vec![Param::new("v")],
            body: js::Body::Block(body),
            ..Function::default()
        },
        true,
    )
}

/// Every user interface `d` implements, parents included; with `stdlib`,
/// also the non-native stdlib interfaces (for their default methods).
fn user_interfaces(g: &Gen<'_>, d: DeclId, stdlib: bool) -> Vec<DeclId> {
    let mut out: Vec<DeclId> = Vec::new();
    let mut stack: Vec<Type> = g.view.decl(d).implements.clone();
    if let Some(exts) = g.view.extensions_of.get(&d) {
        for &e in exts {
            stack.extend(g.view.decl(e).implements.iter().cloned());
        }
    }
    while let Some(t) = stack.pop() {
        let Some(i) = t.decl() else { continue };
        let info = g.view.decl(i);
        // With `stdlib`, also the stdlib interfaces whose default methods
        // have runtime bodies ([`stdlib_default_method`]).
        if (info.stdlib && (!stdlib || info.has_annotation("Native")))
            || info.named_kind() != Some(DeclarationKind::Interface)
            || out.contains(&i)
        {
            continue;
        }
        out.push(i);
        stack.extend(info.implements.iter().cloned());
    }
    out.sort();
    out
}

/// The function members of an interface and its parents.
fn interface_methods(g: &Gen<'_>, iface: DeclId) -> Vec<MemberId> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    let mut stack = vec![iface];
    while let Some(i) = stack.pop() {
        if seen.contains(&i) {
            continue;
        }
        seen.push(i);
        let info = g.view.decl(i);
        for &m in &info.members {
            let mi = g.view.member(m);
            if matches!(mi.kind, MemberKind::Func | MemberKind::Computed)
                && !mi.is_static
                && !out
                    .iter()
                    .any(|&o: &MemberId| g.view.member(o).name == mi.name)
            {
                out.push(m);
            }
        }
        for t in &info.implements {
            if let Some(p) = t.decl() {
                stack.push(p);
            }
        }
    }
    out
}

/// `true` when interface member `m` has a default body.
fn has_body(g: &Gen<'_>, m: MemberId) -> bool {
    g.member_node(m).is_some_and(|n| {
        n.children()
            .any(|c| c.kind() == k_syntax::SyntaxKind::Block)
    })
}

/// Finds the implementation of interface member `m` on value type `d`.
fn implementation(g: &Gen<'_>, d: DeclId, m: MemberId) -> Option<MemberId> {
    let name = &g.view.member(m).name;
    let mut owners = vec![d];
    if let Some(exts) = g.view.extensions_of.get(&d) {
        owners.extend(exts.iter().copied());
    }
    owners
        .into_iter()
        .flat_map(|o| g.view.decl(o).members.iter().copied())
        .find(|&x| {
            let xi = g.view.member(x);
            xi.name == *name
                && matches!(xi.kind, MemberKind::Func | MemberKind::Computed)
                && !xi.is_static
        })
}

/// Adapter classes (`Book$as$Describable`, the existential box) and
/// witness tables (`Book$w$Describable`) of a struct or enum
/// (codegen.md §9).
fn adapters(b: &mut Body<'_, '_>, d: DeclId) -> Vec<(Vec<String>, Vec<Stmt>)> {
    let mut out = Vec::new();
    let name = b.g.name(d);
    for iface in user_interfaces(b.g, d, false) {
        let iname = b.g.name(iface);
        let mut methods = vec![Function {
            kind: MethodKind::Constructor,
            params: vec![Param::new("v")],
            body: js::Body::Block(vec![Stmt::Expr(js::assign(
                member(Expr::This, "v"),
                id("v"),
            ))]),
            ..Function::default()
        }];
        let mut table = Vec::new();
        for m in interface_methods(b.g, iface) {
            let mi = b.g.view.member(m).clone();
            let mname = b.g.member_name(m);
            let is_async = mi.signature.as_ref().is_some_and(|s| s.is_async);
            let target = match implementation(b.g, d, m) {
                Some(imp) => {
                    let f = b.g.fn_name(imp);
                    let owner = b.g.view.member(imp).owner;
                    b.use_decl(owner, f)
                }
                None if has_body(b.g, m) => {
                    let f = b.g.fn_name(m);
                    // Defaults take the interface value.
                    let r = b.use_decl(iface, f);
                    let adapter = format!("{name}$as${iname}");
                    js::arrow(
                        vec![
                            Param::new("s"),
                            Param {
                                name: "a".into(),
                                default: None,
                                rest: true,
                            },
                        ],
                        call(
                            r,
                            vec![
                                Expr::New(Box::new(id(adapter)), vec![id("s")]),
                                Expr::Spread(Box::new(id("a"))),
                            ],
                        ),
                    )
                }
                None => continue,
            };
            let is_computed = mi.kind == MemberKind::Computed;
            let call_target = call(
                target.clone(),
                vec![member(Expr::This, "v"), Expr::Spread(Box::new(id("a")))],
            );
            methods.push(Function {
                name: mname.clone(),
                params: if is_computed {
                    Vec::new()
                } else {
                    vec![Param {
                        name: "a".into(),
                        default: None,
                        rest: true,
                    }]
                },
                body: js::Body::Block(vec![Stmt::Return(Some(if is_computed {
                    call(target.clone(), vec![member(Expr::This, "v")])
                } else {
                    call_target
                }))]),
                is_async,
                kind: if is_computed {
                    MethodKind::Getter
                } else {
                    MethodKind::Plain
                },
            });
            table.push(Prop::KeyValue(mname, target));
        }
        out.push((
            vec![format!("{name}$as${iname}")],
            vec![Stmt::Class(
                Class {
                    name: format!("{name}$as${iname}"),
                    extends: None,
                    methods,
                },
                true,
            )],
        ));
        out.push((
            vec![format!("{name}$w${iname}")],
            vec![Stmt::ExportConst(
                format!("{name}$w${iname}"),
                Expr::Object(table, false),
            )],
        ));
    }
    out
}

/// Lowers an `enum` to its exported constructor object: each payload-free
/// case is a `$k.unitCase(tag)` singleton, each payload case an arrow that
/// builds `{ $: tag, …fields }`; then its member functions and extras.
/// `Currency` is the runtime's `$k.Currency`.
fn enum_decl(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let info = b.g.view.decl(d).clone();
    b.owner = Some(d);
    let name = b.g.name(d);
    let mut out = Vec::new();
    if b.g.b.currency == Some(d) {
        out.push(Stmt::ExportConst(
            name.clone(),
            member(id("$k"), "Currency"),
        ));
        let n = b.g.name(d);
        on_demand(b, vec![format!("{n}$$text")], |b| vec![text_fn(b, d)]);
        return out;
    }
    let mut props = Vec::new();
    for (tag, &c) in b.g.cases(d).iter().enumerate() {
        let cname = b.g.view.member(c).name.clone();
        let fields = b.g.case_fields(c);
        let value = if fields.is_empty() {
            rt("unitCase", vec![Expr::Num(tag as f64)])
        } else {
            let mut obj = vec![Prop::KeyValue("$".into(), Expr::Num(tag as f64))];
            let mut ps = Vec::new();
            // A field's default fills an omitted argument, and a field a
            // decoded message leaves out (`codec.rs`).
            let defaults: Vec<Option<ast::Expr>> =
                b.g.member_node(c)
                    .map(|n| {
                        n.children()
                            .filter(|f| f.kind() == k_syntax::SyntaxKind::CaseField)
                            .map(|f| f.children().find_map(ast::Expr::cast))
                            .collect()
                    })
                    .unwrap_or_default();
            for (i, f) in fields.iter().enumerate() {
                let pn = mangle(f);
                let default = defaults.get(i).cloned().flatten().map(|e| b.expr(&e));
                ps.push(Param {
                    name: pn.clone(),
                    default,
                    rest: false,
                });
                obj.push(Prop::KeyValue(f.clone(), id(pn)));
            }
            let obj = Expr::Object(obj, false);
            // A development client names its payload cases for DevTools'
            // custom formatters (`runtime/devtools.js`).
            let obj = if b.g.options.profile == crate::Profile::Development
                && b.g.options.entry == crate::Entry::App
                && b.target == Target::Client
            {
                rt("devCase", vec![obj, js::str(cname.clone())])
            } else {
                obj
            };
            js::arrow(ps, obj)
        };
        props.push(Prop::KeyValue(cname, value));
    }
    out.push(Stmt::ExportConst(name, Expr::Object(props, true)));
    if !(info.stdlib && info.has_annotation("Native")) {
        out.extend(member_fns(b, d, &SelfMode::Value));
    }
    out.extend(value_extras(b, d));
    out
}

/// Lowers a `class` to a JavaScript class whose methods read
/// the receiver as `this`; field initializers run in the constructor. An
/// `@Native` class lowers to nothing.
fn class_decl(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let info = b.g.view.decl(d).clone();
    if info.has_annotation("Native") {
        return Vec::new();
    }
    b.owner = Some(d);
    b.self_mode = SelfMode::This;
    let name = b.g.name(d);
    let mut methods = Vec::new();
    let fields = b.g.stored_fields(d);
    let inits: Vec<MemberId> = info
        .members
        .iter()
        .copied()
        .filter(|&m| b.g.view.member(m).kind == MemberKind::Init)
        .collect();
    // Field initializers.
    let field_inits = |b: &mut Body<'_, '_>, params_for_fields: bool| -> (Vec<Param>, Vec<Stmt>) {
        let mut ps = Vec::new();
        let mut stmts = Vec::new();
        for &f in &fields {
            let mi = b.g.view.member(f).clone();
            let default =
                b.g.member_node(f)
                    .and_then(|n| n.children().find_map(ast::Expr::cast));
            if params_for_fields {
                let js = b.bind_name(&mi.name, mi.name_range.start());
                let dv = default.map(|e| b.expr(&e));
                ps.push(Param {
                    name: js.clone(),
                    default: dv,
                    rest: false,
                });
                stmts.push(Stmt::Expr(js::assign(
                    member(Expr::This, mi.name.clone()),
                    id(js),
                )));
            } else {
                let (pre, v) = b.capture(|b| match &default {
                    Some(e) => b.expr(e),
                    None => Expr::Null,
                });
                stmts.extend(pre);
                stmts.push(Stmt::Expr(js::assign(
                    member(Expr::This, mi.name.clone()),
                    v,
                )));
            }
        }
        (ps, stmts)
    };
    if inits.is_empty() {
        b.push_fn_scope();
        let (ps, stmts) = field_inits(b, true);
        b.pop_fn_scope();
        methods.push(Function {
            kind: MethodKind::Constructor,
            params: ps,
            body: js::Body::Block(stmts),
            ..Function::default()
        });
    } else {
        let first = inits[0];
        let node = b.g.member_node(first);
        let mi = b.g.view.member(first).clone();
        b.push_fn_scope();
        let ps = params(b, node.as_ref(), mi.signature.as_ref(), Vec::new());
        let (_, mut stmts) = field_inits(b, false);
        if let Some(n) = &node {
            stmts.extend(body_stmts(b, n));
        }
        b.pop_fn_scope();
        methods.push(Function {
            kind: MethodKind::Constructor,
            params: ps,
            body: js::Body::Block(stmts),
            ..Function::default()
        });
    }
    let mut statics = Vec::new();
    for &m in &info.members {
        let mi = b.g.view.member(m).clone();
        let node = b.g.member_node(m);
        match mi.kind {
            MemberKind::Func => {
                b.self_mode = if mi.is_static {
                    SelfMode::None
                } else {
                    SelfMode::This
                };
                let mut f = function(
                    b,
                    b.g.member_name(m),
                    node.as_ref(),
                    mi.signature.as_ref(),
                    Vec::new(),
                );
                if mi.is_static {
                    f.kind = MethodKind::Static;
                }
                methods.push(f);
            }
            MemberKind::Computed => {
                b.self_mode = SelfMode::This;
                let mut f = function(b, mi.name.clone(), node.as_ref(), None, Vec::new());
                f.kind = MethodKind::Getter;
                methods.push(f);
            }
            MemberKind::Field if mi.is_static => {
                b.self_mode = SelfMode::None;
                let value = node
                    .as_ref()
                    .and_then(|n| n.children().find_map(ast::Expr::cast));
                let (pre, e) = b.capture(|b| match &value {
                    Some(v) => b.expr(v),
                    None => Expr::Null,
                });
                statics.extend(pre);
                statics.push(Stmt::Expr(js::assign(
                    member(id(name.clone()), b.g.member_name(m)),
                    e,
                )));
            }
            _ => {}
        }
    }
    // Interface default methods the class doesn't define.
    for iface in user_interfaces(b.g, d, true) {
        for m in interface_methods(b.g, iface) {
            let mi = b.g.view.member(m).clone();
            if methods.iter().any(|f| f.name == b.g.member_name(m)) {
                continue;
            }
            if b.g.view.decl(iface).stdlib {
                if let Some(f) = stdlib_default_method(b, iface, m) {
                    methods.push(f);
                }
                continue;
            }
            if !has_body(b.g, m) {
                continue;
            }
            let f = b.g.fn_name(m);
            let r = b.use_decl(iface, f);
            let is_computed = mi.kind == MemberKind::Computed;
            methods.push(Function {
                name: b.g.member_name(m),
                params: if is_computed {
                    Vec::new()
                } else {
                    vec![Param {
                        name: "a".into(),
                        default: None,
                        rest: true,
                    }]
                },
                body: js::Body::Block(vec![Stmt::Return(Some(if is_computed {
                    call(r, vec![Expr::This])
                } else {
                    call(r, vec![Expr::This, Expr::Spread(Box::new(id("a")))])
                }))]),
                is_async: mi.signature.as_ref().is_some_and(|s| s.is_async),
                kind: if is_computed {
                    MethodKind::Getter
                } else {
                    MethodKind::Plain
                },
            });
        }
    }
    let mut out = vec![Stmt::Class(
        Class {
            name: name.clone(),
            extends: None,
            methods,
        },
        true,
    )];
    out.extend(statics);
    out
}

/// The methods a service or class `d` gets for the default methods of the
/// stdlib interfaces it implements and doesn't define itself
/// ([`stdlib_default_method`]); `existing` are its own methods.
pub(crate) fn stdlib_defaults(
    b: &mut Body<'_, '_>,
    d: DeclId,
    existing: &[Function],
) -> Vec<Function> {
    let mut out = Vec::new();
    for iface in user_interfaces(b.g, d, true) {
        if !b.g.view.decl(iface).stdlib {
            continue;
        }
        for m in interface_methods(b.g, iface) {
            let name = b.g.member_name(m);
            if existing
                .iter()
                .chain(out.iter())
                .any(|f: &Function| f.name == name)
            {
                continue;
            }
            if let Some(f) = stdlib_default_method(b, iface, m) {
                out.push(f);
            }
        }
    }
    out
}

/// The method a user type gets for default method `m` of stdlib interface
/// `iface` when it doesn't define it: the runtime's body of the default
/// (`HttpClient.fetchJson` is the native `HttpClient$fetchJson` over the type's own
/// `send`), or `None` for a member without a default.
fn stdlib_default_method(b: &mut Body<'_, '_>, iface: DeclId, m: MemberId) -> Option<Function> {
    let qualified = b.g.view.qualified_name(iface);
    let name = b.g.view.member(m).name.clone();
    match (qualified.as_str(), name.as_str()) {
        ("k.http.HttpClient", "fetchJson") => {
            let error = match b.g.view.lookup("k.http.HttpError") {
                Some(e) => b.decl_ref(e),
                None => Expr::Null,
            };
            let body = crate::native::http_native(
                b,
                "HttpClient",
                "fetchJson",
                vec![Expr::This, error, Expr::Spread(Box::new(id("a")))],
            );
            Some(Function {
                name: b.g.member_name(m),
                params: vec![Param {
                    name: "a".into(),
                    default: None,
                    rest: true,
                }],
                body: js::Body::Block(vec![Stmt::Return(Some(body))]),
                is_async: true,
                kind: MethodKind::Plain,
            })
        }
        _ => None,
    }
}

/// Lowers a user `interface` to its default methods (exported functions
/// taking the interface value as `self`) and, on demand, its dynamic
/// method table `Name$dyn`, which calls each operation on the value
/// itself. A stdlib interface lowers to nothing.
fn interface_decl(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let info = b.g.view.decl(d).clone();
    if info.stdlib {
        return Vec::new();
    }
    b.owner = Some(d);
    let mut out = Vec::new();
    // Default methods take the interface value as `self`.
    for &m in &info.members {
        let mi = b.g.view.member(m).clone();
        if !matches!(mi.kind, MemberKind::Func | MemberKind::Computed) || !has_body(b.g, m) {
            continue;
        }
        b.self_mode = SelfMode::Value;
        let node = b.g.member_node(m);
        let sig = if mi.kind == MemberKind::Func {
            mi.signature.clone()
        } else {
            None
        };
        let f = function(
            b,
            b.g.fn_name(m),
            node.as_ref(),
            sig.as_ref(),
            vec![Param::new("self")],
        );
        out.push(Stmt::Function(f, true));
    }
    // The table for classes and existentials: method calls.
    let mut table = Vec::new();
    for m in interface_methods(b.g, d) {
        let mi = b.g.view.member(m).clone();
        let mname = b.g.member_name(m);
        let e = if mi.kind == MemberKind::Computed {
            js::arrow(vec![Param::new("s")], member(id("s"), mname.clone()))
        } else {
            js::arrow(
                vec![
                    Param::new("s"),
                    Param {
                        name: "a".into(),
                        default: None,
                        rest: true,
                    },
                ],
                call(
                    member(id("s"), mname.clone()),
                    vec![Expr::Spread(Box::new(id("a")))],
                ),
            )
        };
        table.push(Prop::KeyValue(mname, e));
    }
    let dyn_name = format!("{}$dyn", b.g.name(d));
    b.on_demand.push(OnDemand {
        symbols: vec![dyn_name.clone()],
        stmts: vec![Stmt::ExportConst(dyn_name, Expr::Object(table, false))],
        uses: BTreeSet::new(),
    });
    let _ = bin;
    out
}

/// Lowers an `extension` of a project type to member functions of the
/// extended type ([`member_fns`]); extensions in the stdlib lower to
/// nothing.
fn extension(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let info = b.g.view.decl(d).clone();
    if info.stdlib {
        return Vec::new();
    }
    let target = info.extends.clone().unwrap_or(Type::Error);
    b.owner = target.decl();
    member_fns(b, d, &SelfMode::Value)
}

impl Body<'_, '_> {
    /// The JS names of a struct initializer's field locals.
    pub(crate) fn init_fields_names(&self) -> Vec<String> {
        match self.init_object() {
            Expr::Object(props, _) => props
                .into_iter()
                .filter_map(|p| match p {
                    Prop::KeyValue(_, Expr::Ident(n)) => Some(n),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}
