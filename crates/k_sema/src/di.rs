//! Dependency injection, layering, scopes and the server/client boundary
//! (sema.md §12.6).
//!
//! The pass runs once over the whole program after every body is checked:
//!
//! * every `@Inject` field's type is classified ([`Injectable`]) and checked
//!   against the layering table (K4005), interface implementations and the
//!   app's `use` rules (K4001, K4002, K4010, K4015) and service scopes
//!   (K4004);
//! * the service graph is searched for cycles (K4003, Tarjan's SCC);
//! * every declaration gets its side ([`side`], memoized), which answers
//!   `side_of` and drives K4007, K4008 and K5504.
//!
//! # Invariants
//!
//! * [`side`] never recurses forever: a declaration being computed counts as
//!   shared while its own computation is in progress.

use std::collections::{HashMap, HashSet};

use k_diagnostics::{Diagnostic, FileId, TextRange, TextSize, codes};
use k_syntax::SyntaxKind;
use k_syntax::ast::{self, AstNode};

use crate::cx::Cx;
use crate::framework::is_endpoint_page;
use crate::lookup::{Resolution, Side};
use crate::program::{DeclId, FileIx, MemberId, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// Which sides of the boundary a declaration needs.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) struct Sides {
    /// It needs the server (server-only).
    pub(crate) server: bool,
    /// It needs the browser (client-only).
    pub(crate) client: bool,
}

impl Sides {
    /// Both sides' needs combined.
    fn union(self, other: Sides) -> Sides {
        Sides {
            server: self.server || other.server,
            client: self.client || other.client,
        }
    }

    /// The side reported by `side_of`; a declaration on both sides is
    /// reported (K4008) and answers server-only.
    pub(crate) fn side(self) -> Side {
        match (self.server, self.client) {
            (true, _) => Side::ServerOnly,
            (false, true) => Side::ClientOnly,
            (false, false) => Side::Shared,
        }
    }
}

/// What an `@Inject` field's type is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Injectable {
    /// A service.
    Service(DeclId),
    /// A user interface. The services bound to it, one per environment,
    /// are its [`choices`]; none means K4001/K4002.
    Interface(DeclId),
    /// A protocol (remote calls).
    Protocol(DeclId),
    /// A repository.
    Repository(DeclId),
    /// A config.
    Config(DeclId),
    /// A stdlib platform interface (`HttpClient`, `LocalStorage`).
    Platform(DeclId),
    /// Server infrastructure: `Database`, `SessionManager`.
    Infra(DeclId),
    /// Not injectable at all (a struct, an enum).
    Not,
    /// `{error}` or unresolved: reported elsewhere.
    Unknown,
}

/// One `use X for I [in .env, …]` rule of `App.k`.
#[derive(Clone, Debug)]
struct UseRule {
    /// The implementation, `None` when it doesn't resolve (K1202) or the
    /// rule is a `remote` rule.
    imp: Option<DeclId>,
    /// For `use remote(Config) for Protocol`: the config, `None` when the
    /// argument names nothing (K4025).
    remote: Option<Option<DeclId>>,
    /// The target.
    target: Option<DeclId>,
    /// The environments; empty means every one.
    envs: Vec<String>,
    /// The target type's range.
    target_range: TextRange,
}

/// The analysis-wide DI state: sides, choices.
#[derive(Default)]
pub(crate) struct DiState {
    /// Memoized sides.
    sides: std::cell::RefCell<HashMap<DeclId, Sides>>,
    /// Declarations whose side is being computed (cycle guard).
    busy: std::cell::RefCell<HashSet<DeclId>>,
    /// The app's `use` rules, collected once.
    rules: std::cell::OnceCell<Vec<UseRule>>,
    /// The implementations of each interface, computed once.
    impls: std::cell::RefCell<HashMap<DeclId, Vec<DeclId>>>,
    /// For every declaration, the non-test services that declare it
    /// through their `implements` clauses (themselves included), in
    /// declaration order: built once, by one walk of each service's
    /// `implements` closure, so [`implementations`] costs no scan of the
    /// declarations per interface.
    declared_by: std::cell::OnceCell<HashMap<DeclId, Vec<DeclId>>>,
    /// The per-environment choices of each interface, computed once.
    choices: std::cell::RefCell<HashMap<DeclId, Vec<Choice>>>,
    /// The declarations each declaration's `@Inject` fields name, computed
    /// once per declaration.
    injected: std::cell::RefCell<HashMap<DeclId, std::rc::Rc<[DeclId]>>>,
    /// The declarations from which an app config is reachable, computed
    /// once.
    reaches_config: std::cell::OnceCell<HashSet<DeclId>>,
}

/// One service an interface is bound to, with the environments that bind
/// it (in [`ENVIRONMENTS`] order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    /// The implementing service.
    pub(crate) imp: DeclId,
    /// The environments whose `use` rule (or single implementation)
    /// selects it.
    pub(crate) envs: Vec<&'static str>,
}

/// The environments of `k.core.Environment`.
const ENVIRONMENTS: &[&str] = &["development", "test", "production"];

/// Returns the scope of a service: its `@Scope(.x)` argument, `app` by
/// default.
pub(crate) fn scope_of(cx: &Cx<'_, '_>, d: DeclId) -> String {
    let decl = cx.p.decl(d);
    for a in decl.node.children().filter_map(ast::Annotation::cast) {
        if a.name().is_some_and(|n| n.text() == "Scope") {
            if let Some(arg) = a
                .arg_list()
                .and_then(|l| l.args().next())
                .and_then(|a| a.value())
            {
                if let Some(name) = arg
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident)
                {
                    // An unknown scope is K2006 at the argument; it counts
                    // as the default.
                    if ["app", "request", "route", "component", "server"].contains(&name.text()) {
                        return name.text().to_owned();
                    }
                }
            }
        }
    }
    "app".to_owned()
}

/// How long a scope lives, longest first.
fn lifetime(scope: &str) -> u8 {
    match scope {
        "server" => 4,
        "app" => 3,
        "request" => 2,
        "route" => 1,
        _ => 0,
    }
}

/// The `side:` argument of a stdlib `@Platform(side: .x)`.
fn platform_side(cx: &Cx<'_, '_>, d: DeclId) -> Option<String> {
    let decl = cx.p.decl(d);
    for a in decl.node.children().filter_map(ast::Annotation::cast) {
        if a.name().is_some_and(|n| n.text() == "Platform") {
            let arg = a
                .arg_list()
                .and_then(|l| l.args().next())
                .and_then(|a| a.value())?;
            return arg
                .syntax()
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::Ident)
                .last()
                .map(|t| t.text().to_owned());
        }
    }
    None
}

/// Returns `true` if `d` is the stdlib declaration `qualified`.
fn is_named(cx: &Cx<'_, '_>, d: DeclId, qualified: &str) -> bool {
    cx.p.lookup_qualified(qualified) == Some(d)
}

/// The `@Inject` fields of a declaration, with their type nodes' ranges.
pub(crate) fn inject_fields(cx: &Cx<'_, '_>, d: DeclId) -> Vec<(MemberId, TextRange)> {
    cx.p.decl(d)
        .members
        .iter()
        .copied()
        .filter(|m| {
            let m = cx.p.member(*m);
            m.kind == MemberKind::Field && m.has_annotation("Inject")
        })
        .map(|m| {
            let member = cx.p.member(m);
            let range = member
                .node
                .as_ref()
                .and_then(|n| ast::FieldDecl::cast(n.clone()))
                .and_then(|f| f.ty())
                .map_or(member.name_range, |t| t.syntax().text_range());
            (m, range)
        })
        .collect()
}

/// The declarations `d`'s `@Inject` fields name, in field order (memoized;
/// a field whose type doesn't name a declaration is left out).
pub(crate) fn injected_decls(cx: &Cx<'_, '_>, d: DeclId) -> std::rc::Rc<[DeclId]> {
    if let Some(found) = cx.di.injected.borrow().get(&d) {
        return std::rc::Rc::clone(found);
    }
    let out: std::rc::Rc<[DeclId]> = inject_fields(cx, d)
        .iter()
        .filter_map(|(m, _)| cx.field_ty(*m).decl())
        .collect();
    cx.di
        .injected
        .borrow_mut()
        .insert(d, std::rc::Rc::clone(&out));
    out
}

/// Returns `true` if a project (non-stdlib) config is `d` or reachable from
/// it through the `@Inject` fields of services and repositories and from a
/// protocol to its server implementations, the edges K7503 follows
/// (`app::test_group`). Computed once for the program by a backward search
/// from the configs, linear in the injections.
pub(crate) fn reaches_config(cx: &Cx<'_, '_>, d: DeclId) -> bool {
    cx.di
        .reaches_config
        .get_or_init(|| {
            let p = cx.p;
            let mut back: HashMap<DeclId, Vec<DeclId>> = HashMap::new();
            let mut out: HashSet<DeclId> = HashSet::new();
            let mut stack: Vec<DeclId> = Vec::new();
            for (i, decl) in p.decls.iter().enumerate() {
                let x = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
                match decl.named_kind() {
                    Some(K::Service | K::Repository) => {
                        for t in injected_decls(cx, x).iter() {
                            back.entry(*t).or_default().push(x);
                        }
                    }
                    Some(K::Protocol) => {
                        for s in protocol_implementations(cx, x) {
                            back.entry(s).or_default().push(x);
                        }
                    }
                    Some(K::Config) if !p.is_stdlib(x) && out.insert(x) => stack.push(x),
                    _ => {}
                }
            }
            while let Some(x) = stack.pop() {
                for y in back.get(&x).map_or(&[][..], Vec::as_slice) {
                    if out.insert(*y) {
                        stack.push(*y);
                    }
                }
            }
            out
        })
        .contains(&d)
}

/// Services in non-test code that implement interface `iface`
/// (memoized).
pub(crate) fn implementations(cx: &Cx<'_, '_>, iface: DeclId) -> Vec<DeclId> {
    if let Some(found) = cx.di.impls.borrow().get(&iface) {
        return found.clone();
    }
    let out = compute_implementations(cx, iface);
    cx.di.impls.borrow_mut().insert(iface, out.clone());
    out
}

/// Computes [`implementations`].
///
/// For any interface but `Equatable` and `Hashable`, [`Cx::declares`] is
/// reachability through `implements` clauses, so the answer is read from
/// the index of [`declared_by`]. Those two are also declared implicitly
/// (payload-less enums, sema.md §9.2), so they keep the direct test of every
/// service.
fn compute_implementations(cx: &Cx<'_, '_>, iface: DeclId) -> Vec<DeclId> {
    if Some(iface) == cx.b.equatable || Some(iface) == cx.b.hashable {
        return own_services(cx)
            .filter(|&d| cx.declares(d, iface))
            .collect();
    }
    declared_by(cx).get(&iface).cloned().unwrap_or_default()
}

/// The services of non-test files outside test-only modules, in
/// declaration order: the candidates of [`implementations`].
fn own_services<'c>(cx: &'c Cx<'_, '_>) -> impl Iterator<Item = DeclId> + 'c {
    cx.p.decls.iter().enumerate().filter_map(|(i, decl)| {
        if decl.named_kind() != Some(K::Service) {
            return None;
        }
        let file = cx.p.file(decl.file);
        let test_module = file
            .module
            .is_some_and(|m| cx.p.modules[m.0 as usize].test_only);
        (!file.is_test && !test_module).then(|| DeclId(u32::try_from(i).unwrap_or(u32::MAX)))
    })
}

/// The index behind [`compute_implementations`]: each declaration reachable
/// from a candidate service through `implements` clauses (the service
/// itself included), mapped to those services in declaration order.
/// Linear in the services and their `implements` closures.
fn declared_by<'c>(cx: &'c Cx<'_, '_>) -> &'c HashMap<DeclId, Vec<DeclId>> {
    cx.di.declared_by.get_or_init(|| {
        let mut out: HashMap<DeclId, Vec<DeclId>> = HashMap::new();
        let mut seen: HashSet<DeclId> = HashSet::new();
        let mut stack: Vec<DeclId> = Vec::new();
        for service in own_services(cx).collect::<Vec<_>>() {
            seen.clear();
            stack.push(service);
            while let Some(d) = stack.pop() {
                if !seen.insert(d) {
                    continue;
                }
                out.entry(d).or_default().push(service);
                stack.extend(cx.implements_list(d).iter().filter_map(|(t, _)| t.decl()));
            }
        }
        out
    })
}

/// The app's `use` rules.
fn rules<'c>(cx: &'c Cx<'_, '_>) -> &'c [UseRule] {
    cx.di.rules.get_or_init(|| collect_rules(cx))
}

/// The argument of a `remote(Name)` operand of a `use` rule: `Some(None)`
/// for a `remote(…)` call whose argument is not one name, `None` when the
/// operand is not a `remote(…)` call at all.
pub(crate) fn remote_arg(e: &ast::Expr) -> Option<Option<(String, TextRange)>> {
    let ast::Expr::CallExpr(call) = e else {
        return None;
    };
    let callee = call.syntax().children().find_map(ast::Expr::cast)?;
    let ast::Expr::NameExpr(n) = &callee else {
        return None;
    };
    if n.syntax().text() != "remote" {
        return None;
    }
    let args: Vec<ast::Arg> = call
        .syntax()
        .children()
        .find_map(ast::ArgList::cast)
        .map(|l| l.args().collect())
        .unwrap_or_default();
    let [arg] = args.as_slice() else {
        return Some(None);
    };
    if arg
        .syntax()
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .any(|t| t.kind() == SyntaxKind::Colon)
    {
        return Some(None);
    }
    match arg.value() {
        Some(ast::Expr::NameExpr(name)) => {
            let text = name.syntax().text().to_string();
            Some(Some((text.trim().to_owned(), name.syntax().text_range())))
        }
        _ => Some(None),
    }
}

/// `true` when the app has a `remote` rule for protocol `p`, in any
/// environment: its injections are remote clients, which are server-only.
pub(crate) fn has_remote_rule(cx: &Cx<'_, '_>, p: DeclId) -> bool {
    rules(cx)
        .iter()
        .any(|r| r.remote.is_some() && r.target == Some(p))
}

/// `true` when declaration `d` comes from a library the project depends
/// on: its file is mounted below `.deps/` (`crate::workspace`).
pub(crate) fn from_dependency(cx: &Cx<'_, '_>, d: DeclId) -> bool {
    cx.p.file(cx.p.decl(d).file)
        .path
        .is_some_and(|path| path.dependency().is_some())
}

/// The services of the app's own code (not tests, not test-only modules)
/// that implement protocol `p`.
pub(crate) fn protocol_implementations(cx: &Cx<'_, '_>, p: DeclId) -> Vec<DeclId> {
    implementations(cx, p)
        .into_iter()
        .filter(|&s| {
            let decl = cx.p.decl(s);
            let file = cx.p.file(decl.file);
            !file.is_test
                && !file.stdlib
                && !cx
                    .p
                    .decl_module(s)
                    .is_some_and(|m| cx.p.modules[m.0 as usize].test_only)
        })
        .collect()
}

/// The project's `app` declaration, if it is an app.
fn app_decl(cx: &Cx<'_, '_>) -> Option<DeclId> {
    cx.p.decls.iter().enumerate().find_map(|(i, d)| {
        (d.named_kind() == Some(K::App) && !cx.p.file(d.file).stdlib)
            .then(|| DeclId(u32::try_from(i).unwrap_or(u32::MAX)))
    })
}

/// Collects the `use` rules of `App.k`'s app, resolving types quietly (the rules are reported by [`check`]).
fn collect_rules(cx: &Cx<'_, '_>) -> Vec<UseRule> {
    let Some(app) = app_decl(cx) else {
        return Vec::new();
    };
    let d = cx.p.decl(app);
    let mut out = Vec::new();
    for node in d
        .node
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::UseDecl)
    {
        let Some(u) = ast::UseDecl::cast(node.clone()) else {
            continue;
        };
        let imp = u
            .implementation()
            .and_then(|t| cx.resolve_type_quiet(d.file, &[], &t).decl());
        let remote = u.value().and_then(|e| remote_arg(&e)).map(|arg| {
            arg.and_then(|(name, _)| match cx.lookup(d.file, &name) {
                crate::cx::Found::Decl(c) => Some(c),
                _ => None,
            })
        });
        let Some(target_node) = u.target() else {
            continue;
        };
        let target = cx.resolve_type_quiet(d.file, &[], &target_node).decl();
        let envs = u
            .environments()
            .iter()
            .filter_map(|e| {
                e.syntax()
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident)
                    .map(|t| t.text().to_owned())
            })
            .collect();
        out.push(UseRule {
            imp,
            remote,
            target,
            envs,
            target_range: target_node.syntax().text_range(),
        });
    }
    out
}

/// Classifies the type of an `@Inject` field.
pub(crate) fn classify(cx: &Cx<'_, '_>, ty: &Ty) -> Injectable {
    let Some(d) = ty.decl() else {
        return if ty.contains_error() {
            Injectable::Unknown
        } else {
            Injectable::Not
        };
    };
    let decl = cx.p.decl(d);
    match decl.named_kind() {
        Some(K::Service) => {
            if is_named(cx, d, "k.db.Database") || is_named(cx, d, "k.auth.SessionManager") {
                Injectable::Infra(d)
            } else {
                Injectable::Service(d)
            }
        }
        Some(K::Protocol) => Injectable::Protocol(d),
        Some(K::Repository) => Injectable::Repository(d),
        Some(K::Config) => Injectable::Config(d),
        Some(K::Interface) if decl.has_annotation("Platform") => Injectable::Platform(d),
        Some(K::Interface) => {
            if cx.p.is_stdlib(d) {
                return Injectable::Not;
            }
            Injectable::Interface(d)
        }
        _ => Injectable::Not,
    }
}

/// The services interface `iface` is bound to across every environment
/// (memoized), the way codegen binds it for each one
/// (`k_codegen::server::binding`): per environment, the first `use` rule
/// for `iface` that names the environment or has no `in`, else the single
/// project implementation. A rule whose implementation doesn't resolve
/// binds nothing (K1202 is reported at it).
///
/// Every check that follows an interface to its implementation (sides,
/// scopes, cycles, sessions) runs over all of them, so a program is
/// accepted only when it holds in every environment `k check` can't tell
/// apart. Distinct services in first-environment order; empty when no
/// environment binds one.
pub(crate) fn choices(cx: &Cx<'_, '_>, iface: DeclId) -> Vec<Choice> {
    if let Some(found) = cx.di.choices.borrow().get(&iface) {
        return found.clone();
    }
    let rules = rules(cx);
    let mut out: Vec<Choice> = Vec::new();
    for env in ENVIRONMENTS.iter().copied() {
        let rule = rules.iter().find(|r| {
            r.target == Some(iface) && (r.envs.is_empty() || r.envs.iter().any(|e| e == env))
        });
        let imp = match rule {
            Some(r) => r.imp,
            None => match implementations(cx, iface).as_slice() {
                [only] => Some(*only),
                _ => None,
            },
        };
        let Some(imp) = imp else { continue };
        match out.iter_mut().find(|c| c.imp == imp) {
            Some(c) => c.envs.push(env),
            None => out.push(Choice {
                imp,
                envs: vec![env],
            }),
        }
    }
    cx.di.choices.borrow_mut().insert(iface, out.clone());
    out
}

/// The services an injection resolves to: a service itself, every
/// environment's choice for an interface, nothing for anything else.
fn bound_services(cx: &Cx<'_, '_>, what: &Injectable) -> Vec<DeclId> {
    match what {
        Injectable::Service(t) => vec![*t],
        Injectable::Interface(i) => choices(cx, *i).into_iter().map(|c| c.imp).collect(),
        _ => Vec::new(),
    }
}

/// The note naming the environments in which interface `iface` is bound
/// to `imp`, when the environments bind different services:
/// "in `.production`, `PaymentProvider` is `StripePayments`". `None` when
/// every environment binds the same one.
fn environment_note(cx: &Cx<'_, '_>, iface: DeclId, imp: DeclId) -> Option<String> {
    let all = choices(cx, iface);
    if all.len() < 2
        && all
            .first()
            .is_none_or(|c| c.envs.len() == ENVIRONMENTS.len())
    {
        return None;
    }
    let choice = all.iter().find(|c| c.imp == imp)?;
    let envs: Vec<String> = choice.envs.iter().map(|e| format!("`.{e}`")).collect();
    let envs = match envs.as_slice() {
        [] => return None,
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    };
    Some(format!(
        "in {envs}, `{}` is `{}`",
        cx.p.decl_name(iface),
        cx.p.decl_name(imp)
    ))
}

/// The side of a declaration (memoized).
pub(crate) fn side(cx: &Cx<'_, '_>, d: DeclId) -> Sides {
    if let Some(s) = cx.di.sides.borrow().get(&d) {
        return *s;
    }
    if !cx.di.busy.borrow_mut().insert(d) {
        return Sides::default();
    }
    let s = compute_side(cx, d);
    cx.di.busy.borrow_mut().remove(&d);
    cx.di.sides.borrow_mut().insert(d, s);
    s
}

/// Server-only.
const SERVER: Sides = Sides {
    server: true,
    client: false,
};
/// Client-only.
const CLIENT: Sides = Sides {
    server: false,
    client: true,
};

/// Computes [`side`]: stdlib markers, declaration kinds, and the sides of what a service injects.
fn compute_side(cx: &Cx<'_, '_>, d: DeclId) -> Sides {
    let decl = cx.p.decl(d);
    if decl.has_annotation("ServerOnly") {
        return SERVER;
    }
    if decl.has_annotation("ClientOnly") {
        return CLIENT;
    }
    if decl.has_annotation("Platform") {
        return match platform_side(cx, d).as_deref() {
            Some("server") => SERVER,
            Some("client") => CLIENT,
            _ => Sides::default(),
        };
    }
    match decl.named_kind() {
        Some(K::Entity | K::Repository | K::Config) => SERVER,
        Some(K::Page) if is_endpoint_page(cx, d) => SERVER,
        Some(K::Service) => {
            let mut s = Sides::default();
            let implements_protocol = cx.implements_list(d).iter().any(|(t, _)| {
                t.decl()
                    .is_some_and(|i| cx.p.decl(i).named_kind() == Some(K::Protocol))
            });
            let job = cx
                .implements_list(d)
                .iter()
                .any(|(t, _)| t.decl().is_some_and(|i| is_named(cx, i, "k.server.Job")));
            let scope = scope_of(cx, d);
            if implements_protocol || job || scope == "request" || scope == "server" {
                s.server = true;
            }
            for (m, _) in inject_fields(cx, d) {
                s = s.union(injected_side(cx, &cx.field_ty(m)));
            }
            s
        }
        _ => Sides::default(),
    }
}

/// The side an injected field's type brings along.
fn injected_side(cx: &Cx<'_, '_>, ty: &Ty) -> Sides {
    match classify(cx, ty) {
        Injectable::Service(t) | Injectable::Platform(t) | Injectable::Infra(t) => side(cx, t),
        // The union over every environment's choice: the declaration's side
        // is one for all of them (10-services.md, "The boundary is drawn
        // per declaration").
        Injectable::Interface(i) => choices(cx, i)
            .iter()
            .fold(Sides::default(), |s, c| s.union(side(cx, c.imp))),
        Injectable::Repository(_) | Injectable::Config(_) => SERVER,
        // A remote client holds a service token (40-workspaces-and-services.md).
        Injectable::Protocol(p) if has_remote_rule(cx, p) => SERVER,
        Injectable::Protocol(_) | Injectable::Not | Injectable::Unknown => Sides::default(),
    }
}

/// The members of a client-only platform interface that code may read
/// while the server renders, which reads `false` and `.prompt` from them
/// (45-web-platform.md, "Server rendering and the server").
pub(crate) const SERVER_READABLE: &[&str] = &["isSupported", "permission"];

/// Returns `true` when `field` is a `@Derived` field whose initializer
/// reads nothing client-only but [`SERVER_READABLE`] members of the
/// client-only platform interfaces its declaration injects: of the
/// declaration's own members it reads only such `@Inject` fields (as the
/// receiver of `isSupported` or `permission`), other such `@Derived`
/// fields, and plain `let` fields. Its value during server rendering is
/// then the one the browser starts from, so reading it isn't K4013.
pub(crate) fn server_readable_derived(cx: &Cx<'_, '_>, field: MemberId) -> bool {
    server_readable_inner(cx, field, &mut HashSet::new())
}

/// The recursion of [`server_readable_derived`]; `seen` stops cycles.
fn server_readable_inner(cx: &Cx<'_, '_>, field: MemberId, seen: &mut HashSet<MemberId>) -> bool {
    let p = cx.p;
    let m = p.member(field);
    if !m.has_annotation("Derived") || !seen.insert(field) {
        return m.has_annotation("Derived");
    }
    let Some(node) = &m.node else { return false };
    let decl = p.decl(m.owner);
    let mut reads_platform = false;
    for n in node.descendants() {
        if n.kind() != SyntaxKind::NameExpr
            || n.ancestors().any(|a| a.kind() == SyntaxKind::Annotation)
        {
            continue;
        }
        let Some(tok) = n.first_token() else { continue };
        // `self.x`: the member is the parent's name.
        let (name, expr) = if tok.kind() == SyntaxKind::SelfKw {
            let Some(parent) = n.parent().and_then(ast::MemberExpr::cast) else {
                continue;
            };
            let Some(name) = parent.name() else { continue };
            (name.text().to_owned(), parent.syntax().clone())
        } else {
            (tok.text().to_owned(), n.clone())
        };
        let Some(&other) = decl.member_names.get(&name) else {
            continue;
        };
        let om = p.member(other);
        if om.has_annotation("Inject") {
            let platform = cx.field_ty(other).decl().is_some_and(|t| {
                let s = side(cx, t);
                p.is_stdlib(t) && p.decl(t).has_annotation("Platform") && s.client && !s.server
            });
            let readable = expr
                .parent()
                .and_then(ast::MemberExpr::cast)
                .and_then(|r| r.name())
                .is_some_and(|n| SERVER_READABLE.contains(&n.text()));
            if !(platform && readable) {
                return false;
            }
            reads_platform = true;
        } else if om.has_annotation("Derived") {
            if !server_readable_inner(cx, other, seen) {
                return false;
            }
            reads_platform = true;
        } else if om.kind != MemberKind::Field || om.is_var || !om.annotations.is_empty() {
            return false;
        }
    }
    reads_platform
}

/// Returns `true` when the project's app renders in the browser only
/// (`@Rendering(.client)`).
pub(crate) fn client_rendered(cx: &Cx<'_, '_>) -> bool {
    app_decl(cx).is_some() && server_rendered_app(cx).is_none()
}

/// The app declaration when it renders on the server (the default, no
/// `@Rendering(.client)`), with the offset where an annotation goes.
pub(crate) fn server_rendered_app(cx: &Cx<'_, '_>) -> Option<(DeclId, TextSize)> {
    let app = app_decl(cx)?;
    let node = &cx.p.decl(app).node;
    let client = node.children().filter_map(ast::Annotation::cast).any(|a| {
        a.name().is_some_and(|n| n.text() == "Rendering")
            && a.syntax()
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .any(|t| t.text() == "client")
    });
    if client {
        return None;
    }
    Some((app, crate::program::first_token_range(node).start()))
}

/// Returns `true` for declarations whose code reaches the browser:
/// components, layouts and pages with a view.
pub(crate) fn reaches_client(cx: &Cx<'_, '_>, d: DeclId) -> bool {
    match cx.p.decl(d).named_kind() {
        Some(K::Component | K::Layout) => true,
        Some(K::Page) => !is_endpoint_page(cx, d),
        _ => false,
    }
}

// ------------------------------------------------------------ checks

/// The DI and boundary checks over the whole program.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let mut edges: Vec<(DeclId, DeclId, MemberId, TextRange)> = Vec::new();
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib {
            continue;
        }
        let kind = decl.named_kind();
        for (m, range) in inject_fields(cx, d) {
            let ty = cx.field_ty(m);
            let what = classify(cx, &ty);
            if !layering(cx, d, kind, &what, decl.file, range) {
                continue;
            }
            if let Injectable::Interface(iface) = &what {
                if kind != Some(K::TestSuite) && choices(cx, *iface).is_empty() {
                    missing_or_ambiguous(cx, decl.file, range, *iface);
                }
            }
            if let Injectable::Protocol(proto) = &what {
                missing_remote_rule(cx, kind, decl.file, range, *proto);
            }
            if kind == Some(K::Service) {
                let via = match what {
                    Injectable::Interface(i) => Some(i),
                    _ => None,
                };
                for t in bound_services(cx, &what) {
                    edges.push((d, t, m, range));
                    scopes(cx, d, t, via, decl.file, range);
                }
            }
            // An imported server-only declaration is reported once, at the
            // import (`client_references`). An interface is shared itself;
            // what makes it server-only is a choice, reported here.
            let imported = ty
                .decl()
                .is_some_and(|t| imported_into(cx, decl.file, t) && side(cx, t).server);
            if reaches_client(cx, d) && injected_side(cx, &ty).server && !imported {
                let mut diag = Diagnostic::new(
                    codes::SERVER_ONLY_IN_CLIENT,
                    p.span(decl.file, range),
                    format!(
                        "`{}` is server-only and can't be injected into code that reaches the browser",
                        ty.render(p)
                    ),
                );
                if let Some(path) = server_path_of(cx, &what, &mut HashSet::new()) {
                    let mut names = vec![p.decl_name(d)];
                    names.extend(path.decls.iter().map(|t| p.decl_name(*t)));
                    diag = diag.with_note(format!("{} ({})", names.join(" → "), path.reason));
                    if let Some(note) = path.environment {
                        diag = diag.with_note(note);
                    }
                }
                cx.report(decl.file, diag);
            }
        }
        if kind == Some(K::Service) {
            both_sides(cx, d);
            job_session(cx, d);
        }
    }
    cycles(cx, &edges);
    use_rules(cx);
}

/// The layering table (K4005); returns `false` when reported.
fn layering(
    cx: &Cx<'_, '_>,
    owner: DeclId,
    kind: Option<K>,
    what: &Injectable,
    file: FileIx,
    range: TextRange,
) -> bool {
    use Injectable as I;
    if matches!(what, I::Unknown) {
        return false;
    }
    let endpoint = is_endpoint_page(cx, owner);
    let ok = match kind {
        Some(K::TestSuite) => !matches!(what, I::Not),
        Some(K::Component | K::Layout) => matches!(what, I::Service(_) | I::Interface(..)),
        Some(K::Page) if endpoint => !matches!(what, I::Protocol(_) | I::Repository(_) | I::Not),
        Some(K::Page) => matches!(what, I::Service(_) | I::Interface(..)),
        Some(K::Service) => !matches!(what, I::Not),
        Some(K::Repository) => matches!(what, I::Repository(_) | I::Config(_) | I::Infra(_)),
        _ => true,
    };
    if !ok {
        let owner_kind = match kind {
            Some(K::Component) => "a component",
            Some(K::Layout) => "a layout",
            Some(K::Page) if endpoint => "an endpoint page",
            Some(K::Page) => "a page",
            Some(K::Repository) => "a repository",
            _ => "this declaration",
        };
        let target = match what {
            I::Protocol(_) => "a protocol",
            I::Repository(_) => "a repository",
            I::Config(_) => "a config",
            I::Platform(_) => "a platform interface",
            I::Infra(_) => "server infrastructure",
            I::Service(_) | I::Interface(..) => "a service",
            I::Not | I::Unknown => "a value that isn't injectable",
        };
        cx.error(
            file,
            codes::LAYERING_VIOLATION,
            range,
            format!("{owner_kind} can't inject {target}"),
        );
    }
    ok
}

/// K4023: an injected protocol from a library dependency that the app
/// neither implements nor binds with a `remote` rule has nothing to call.
fn missing_remote_rule(
    cx: &Cx<'_, '_>,
    kind: Option<K>,
    file: FileIx,
    range: TextRange,
    proto: DeclId,
) {
    if app_decl(cx).is_none()
        || kind == Some(K::TestSuite)
        || cx.p.file(file).is_test
        || !from_dependency(cx, proto)
        || has_remote_rule(cx, proto)
        || !protocol_implementations(cx, proto).is_empty()
    {
        return;
    }
    let p = cx.p;
    let name = p.decl(proto).name.clone();
    cx.report(
        file,
        Diagnostic::new(
            codes::MISSING_REMOTE_RULE,
            p.span(file, range),
            format!(
                "`{}` comes from a dependency and this app doesn't implement it, so it needs a `remote` rule",
                p.decl_name(proto)
            ),
        )
        .with_note(format!(
            "add `use remote(Config) for {name}` to `App.k`, where `Config` is a config implementing `RemoteService`"
        )),
    );
}

/// The checks of one `use remote(Config) for Target` rule: K4024 for a
/// target that isn't a library protocol the app doesn't implement, K4025
/// for an argument that isn't a `RemoteService` config.
fn remote_rule(
    cx: &Cx<'_, '_>,
    file: FileIx,
    call: &ast::Expr,
    target: Option<(DeclId, TextRange)>,
) {
    let p = cx.p;
    match remote_arg(call).flatten() {
        None => cx.error(
            file,
            codes::NOT_A_REMOTE_SERVICE,
            call.syntax().text_range(),
            "`remote(…)` takes the name of a config that implements `RemoteService`",
        ),
        Some((name, range)) => match cx.lookup(file, &name) {
            crate::cx::Found::Decl(c) => {
                cx.record_resolution(file, range, cx.decl_resolution(c));
                if p.decl(c).named_kind() != Some(K::Config) {
                    cx.error(
                        file,
                        codes::NOT_A_REMOTE_SERVICE,
                        range,
                        format!(
                            "`{}` is not a config; `remote(…)` takes a config that implements `RemoteService`",
                            p.decl_name(c)
                        ),
                    );
                } else if !is_remote_service(cx, c) {
                    cx.error(
                        file,
                        codes::NOT_A_REMOTE_SERVICE,
                        range,
                        format!("`{}` doesn't implement `RemoteService`", p.decl_name(c)),
                    );
                }
            }
            _ => cx.error(
                file,
                codes::NOT_A_REMOTE_SERVICE,
                range,
                format!("`{name}` doesn't name a config that implements `RemoteService`"),
            ),
        },
    }
    let Some((t, range)) = target else { return };
    if p.decl(t).named_kind() != Some(K::Protocol) {
        cx.error(
            file,
            codes::REMOTE_RULE_NOT_ALLOWED,
            range,
            format!(
                "`{}` is not a protocol; a `remote` rule binds a protocol",
                p.decl_name(t)
            ),
        );
        return;
    }
    if let Some(&imp) = protocol_implementations(cx, t).first() {
        cx.report(
            file,
            Diagnostic::new(
                codes::REMOTE_RULE_NOT_ALLOWED,
                p.span(file, range),
                format!(
                    "this app implements `{}` (`{}`), so it can't have a `remote` rule",
                    p.decl_name(t),
                    p.decl_name(imp)
                ),
            )
            .with_note("a `remote` rule is for a protocol the app calls in another service"),
        );
        return;
    }
    if !from_dependency(cx, t) && !p.is_stdlib(t) {
        cx.report(
            file,
            Diagnostic::new(
                codes::REMOTE_RULE_NOT_ALLOWED,
                p.span(file, range),
                format!(
                    "`{}` is declared in this app, so no other service implements it",
                    p.decl_name(t)
                ),
            )
            .with_note("a `remote` rule is for a protocol from a library dependency; move the protocol into a library both services depend on"),
        );
    }
}

/// `true` when config `c` implements `k.rpc.RemoteService`.
fn is_remote_service(cx: &Cx<'_, '_>, c: DeclId) -> bool {
    cx.p.lookup_qualified("k.rpc.RemoteService")
        .is_some_and(|r| cx.declares(c, r))
}

/// K4025 for every config implementing `RemoteService` without `url: Url`
/// and `token: Secret<String>`, or with a `timeout` that isn't a
/// `Duration`.
pub(crate) fn remote_services(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let Some(remote) = p.lookup_qualified("k.rpc.RemoteService") else {
        return;
    };
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib
            || decl.named_kind() != Some(K::Config)
            || !cx.declares(d, remote)
        {
            continue;
        }
        let field = |name: &str| {
            decl.members.iter().copied().find(|&m| {
                let member = p.member(m);
                member.name == name && member.kind == MemberKind::Field
            })
        };
        for (name, wanted, written) in [
            ("url", "k.core.Url", "Url"),
            ("token", "k.core.Secret", "Secret<String>"),
            ("timeout", "k.core.Duration", "Duration"),
        ] {
            let Some(m) = field(name) else {
                if name != "timeout" {
                    cx.error(
                        decl.file,
                        codes::NOT_A_REMOTE_SERVICE,
                        decl.name_range,
                        format!(
                            "`{}` implements `RemoteService`, so it needs `{name}: {written}`",
                            p.decl_name(d)
                        ),
                    );
                }
                continue;
            };
            let ty = cx.field_ty(m);
            if ty.contains_error() {
                continue;
            }
            let is = |t: Option<DeclId>, q: &str| t.is_some() && t == p.lookup_qualified(q);
            let ok = is(ty.decl(), wanted)
                && (name != "token"
                    || matches!(&ty, Ty::Named(_, args) if is(args.first().and_then(Ty::decl), "k.core.String")));
            if !ok {
                let member = p.member(m);
                let range = member
                    .node
                    .as_ref()
                    .and_then(|n| n.children().find_map(ast::Type::cast))
                    .map_or(member.name_range, |t| t.syntax().text_range());
                cx.error(
                    decl.file,
                    codes::NOT_A_REMOTE_SERVICE,
                    range,
                    format!("`{name}` of a `RemoteService` is a `{written}`"),
                );
            }
        }
    }
}

/// K4001 / K4002 for an interface without a single implementation.
fn missing_or_ambiguous(cx: &Cx<'_, '_>, file: FileIx, range: TextRange, iface: DeclId) {
    let p = cx.p;
    if rules(cx).iter().any(|r| r.target == Some(iface)) {
        return;
    }
    let impls = implementations(cx, iface);
    let name = p.decl_name(iface);
    if impls.is_empty() {
        cx.error(
            file,
            codes::MISSING_IMPLEMENTATION,
            range,
            format!("no service implements `{name}`"),
        );
        return;
    }
    let mut d = Diagnostic::new(
        codes::AMBIGUOUS_IMPLEMENTATION,
        p.span(file, range),
        format!("several services implement `{name}`; choose one with `use` in `App.k`"),
    );
    for i in &impls {
        let decl = p.decl(*i);
        d = d.with_label(p.span(decl.file, decl.name_range), "implemented here");
    }
    cx.report(file, d);
}

/// K4004: a service injects only services that live at least as long;
/// `via` is the interface `target` was chosen for, if any.
fn scopes(
    cx: &Cx<'_, '_>,
    owner: DeclId,
    target: DeclId,
    via: Option<DeclId>,
    file: FileIx,
    range: TextRange,
) {
    let p = cx.p;
    if is_named(cx, target, "k.time.Clock") {
        return;
    }
    let outer = scope_of(cx, owner);
    let inner = scope_of(cx, target);
    if lifetime(&inner) >= lifetime(&outer) {
        return;
    }
    let mut diag = Diagnostic::new(
        codes::SCOPE_VIOLATION,
        p.span(file, range),
        format!(
            "a `.{outer}` service can't inject `{}`, which is `.{inner}` and may end sooner",
            p.decl_name(target)
        ),
    )
    .with_note(format!("{} → {}", p.decl_name(owner), p.decl_name(target)));
    if let Some(note) = via.and_then(|i| environment_note(cx, i, target)) {
        diag = diag.with_note(note);
    }
    cx.report(file, diag);
}

/// K4008: a service that needs both sides; K5504 for a protocol
/// implementation that needs the browser.
fn both_sides(cx: &Cx<'_, '_>, d: DeclId) {
    let p = cx.p;
    let decl = p.decl(d);
    let mut client = Sides::default();
    let mut server = Sides::default();
    for (m, _) in inject_fields(cx, d) {
        let s = injected_side(cx, &cx.field_ty(m));
        client = client.union(Sides {
            server: false,
            client: s.client,
        });
        server = server.union(Sides {
            server: s.server,
            client: false,
        });
    }
    let protocol = cx
        .implements_list(d)
        .iter()
        .find(|(t, _)| {
            t.decl()
                .is_some_and(|i| p.decl(i).named_kind() == Some(K::Protocol))
        })
        .map(|(_, r)| *r);
    if let (Some(range), true) = (protocol, client.client) {
        cx.error(
            decl.file,
            codes::RPC_IMPLEMENTATION_NOT_SERVER,
            range,
            "a protocol implementation runs on the server, but this service needs the browser",
        );
        return;
    }
    let s = side(cx, d);
    // Only the service that joins the sides is reported: one whose own
    // injections bring both.
    if s.server && s.client && client.client && (server.server || own_server(cx, d)) {
        cx.error(
            decl.file,
            codes::SERVICE_ON_BOTH_SIDES,
            decl.name_range,
            format!("`{}` needs both the server and the browser", decl.name),
        );
    }
}

/// K4009: a job (a service implementing `k.server.Job`) whose dependency
/// graph reaches `Session` or `SessionManager`: a job runs without a
/// session (30-background-jobs.md). Reported at the job's name, with a note
/// naming the injection path.
fn job_session(cx: &Cx<'_, '_>, d: DeclId) {
    let is_job = cx
        .implements_list(d)
        .iter()
        .any(|(t, _)| t.decl().is_some_and(|i| is_named(cx, i, "k.server.Job")));
    if !is_job {
        return;
    }
    let mut environment = None;
    let Some(path) = session_path(cx, d, &mut environment, &mut HashSet::new()) else {
        return;
    };
    let decl = cx.p.decl(d);
    let mut names = vec![cx.p.decl_name(d)];
    names.extend(path.iter().map(|t| cx.p.decl_name(*t)));
    let last = path
        .last()
        .map_or_else(String::new, |t| cx.p.decl(*t).name.clone());
    let mut diag = Diagnostic::new(
        codes::JOB_REACHES_SESSION,
        cx.p.span(decl.file, decl.name_range),
        format!(
            "the job `{}` reaches `{last}`, but a job runs without a session",
            decl.name
        ),
    )
    .with_note(names.join(" → "));
    if let Some(note) = environment {
        diag = diag.with_note(note);
    }
    cx.report(decl.file, diag);
}

/// The injection path from `d` (excluded) to `Session` or `SessionManager`
/// (included), if `d`'s dependency graph reaches one. A depth-first search
/// over services and every environment's choice for interfaces; `seen`
/// keeps it linear in the graph. `environment` receives the
/// [`environment_note`] of the first interface on the path whose choice
/// differs between environments.
fn session_path(
    cx: &Cx<'_, '_>,
    d: DeclId,
    environment: &mut Option<String>,
    seen: &mut HashSet<DeclId>,
) -> Option<Vec<DeclId>> {
    if !seen.insert(d) {
        return None;
    }
    for (m, _) in inject_fields(cx, d) {
        let what = classify(cx, &cx.field_ty(m));
        let (via, next) = match what {
            Injectable::Infra(t) => (None, vec![t]),
            Injectable::Interface(i) => (Some(i), bound_services(cx, &what)),
            _ => (None, bound_services(cx, &what)),
        };
        for next in next {
            let note = || via.and_then(|i| environment_note(cx, i, next));
            if is_named(cx, next, "k.auth.Session") || is_named(cx, next, "k.auth.SessionManager") {
                *environment = note();
                return Some(vec![next]);
            }
            if let Some(mut rest) = session_path(cx, next, environment, seen) {
                if environment.is_none() {
                    *environment = note();
                }
                rest.insert(0, next);
                return Some(rest);
            }
        }
    }
    None
}

/// Returns `true` for a service whose own scope keeps it on the server.
fn own_server(cx: &Cx<'_, '_>, d: DeclId) -> bool {
    let scope = scope_of(cx, d);
    scope == "request" || scope == "server"
}

/// K4003: cycles in the service graph, at every field that closes one.
fn cycles(cx: &Cx<'_, '_>, edges: &[(DeclId, DeclId, MemberId, TextRange)]) {
    let mut index: HashMap<DeclId, usize> = HashMap::new();
    for (a, b, _, _) in edges {
        let n = index.len();
        index.entry(*a).or_insert(n);
        let n = index.len();
        index.entry(*b).or_insert(n);
    }
    let mut graph = vec![Vec::new(); index.len()];
    for (a, b, _, _) in edges {
        graph[index[a]].push(index[b]);
    }
    let comp = crate::check::tarjan(&graph);
    // The first injection of each edge inside a cycle, for the labels.
    let mut first: HashMap<(usize, usize), (DeclId, DeclId, MemberId, TextRange)> = HashMap::new();
    for &(a, b, m, range) in edges {
        let (ia, ib) = (index[&a], index[&b]);
        if comp[ia] == comp[ib] {
            first.entry((ia, ib)).or_insert((a, b, m, range));
        }
    }
    let mut paths: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (a, b, m, range) in edges {
        let (ia, ib) = (index[a], index[b]);
        if comp[ia] != comp[ib] {
            continue;
        }
        let file = cx.p.decl(cx.p.member(*m).owner).file;
        let a_name = cx.p.decl_name(*a);
        if a == b {
            cx.error(
                file,
                codes::DEPENDENCY_CYCLE,
                *range,
                format!("`{a_name}` injects itself"),
            );
            continue;
        }
        // The rest of the cycle: the shortest path from `b` back to `a`.
        let path = paths
            .entry((ib, ia))
            .or_insert_with(|| crate::check::shortest_path_in_component(&graph, &comp, ib, ia));
        let mut names = vec![a_name.clone()];
        let mut labels = Vec::new();
        for pair in path.windows(2) {
            if let Some(&(x, y, xm, xr)) = first.get(&(pair[0], pair[1])) {
                if names.len() == 1 {
                    names.push(cx.p.decl_name(x));
                }
                names.push(cx.p.decl_name(y));
                labels.push((cx.p.decl(cx.p.member(xm).owner).file, x, y, xr));
            }
        }
        if names.len() == 1 {
            names.push(cx.p.decl_name(*b));
        }
        let mut diag = Diagnostic::new(
            codes::DEPENDENCY_CYCLE,
            cx.p.span(file, *range),
            format!(
                "`{a_name}` is part of a dependency cycle: {}",
                names.join(" → ")
            ),
        );
        for (lfile, x, y, xr) in labels {
            diag = diag.with_label(
                cx.p.span(lfile, xr),
                format!(
                    "`{}` injects `{}` here",
                    cx.p.decl_name(x),
                    cx.p.decl_name(y)
                ),
            );
        }
        cx.report(file, diag);
    }
}

/// `use` rules in `App.k`: the implementation fits the target (K4010),
/// expressions are for tests (K4011), environments are covered (K4015).
fn use_rules(cx: &Cx<'_, '_>) {
    let Some(app) = app_decl(cx) else { return };
    let p = cx.p;
    let d = p.decl(app);
    let env = p
        .lookup_qualified("k.core.Environment")
        .map_or(Ty::Error, Ty::named);
    let mut checker = crate::body::Checker::new(
        cx,
        crate::body::BodyOwner {
            file: d.file,
            decl: None,
            self_ty: None,
            generics: Vec::new(),
        },
    );
    for node in d
        .node
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::UseDecl)
    {
        let Some(u) = ast::UseDecl::cast(node.clone()) else {
            continue;
        };
        for e in u.environments() {
            checker.check_initializer(&e, Some(&env));
        }
        if let Some(e) = u.value() {
            if remote_arg(&e).is_some() {
                let target = u.target().and_then(|t| {
                    cx.resolve_type(d.file, &[], &t)
                        .decl()
                        .map(|decl| (decl, t.syntax().text_range()))
                });
                remote_rule(cx, d.file, &e, target);
                continue;
            }
            if u.implementation().is_none() {
                cx.error(
                    d.file,
                    codes::USE_EXPRESSION_NOT_ALLOWED,
                    e.syntax().text_range(),
                    "a value is bound with `use` only in a test, and only to a `config`",
                );
                continue;
            }
        }
        let imp = u
            .implementation()
            .map(|t| (cx.resolve_type(d.file, &[], &t), t));
        let target = u.target().map(|t| cx.resolve_type(d.file, &[], &t));
        let (Some((imp, imp_node)), Some(target)) = (imp, target) else {
            continue;
        };
        let (Some(i), Some(t)) = (imp.decl(), target.decl()) else {
            continue;
        };
        if i != t && !cx.declares(i, t) {
            cx.error(
                d.file,
                codes::USE_TARGET_MISMATCH,
                imp_node.syntax().text_range(),
                format!(
                    "`{}` doesn't implement `{}`",
                    p.decl_name(i),
                    p.decl_name(t)
                ),
            );
        }
    }
    checker.finish();
    // Environments per target.
    let mut by_target: Vec<(DeclId, Vec<&UseRule>)> = Vec::new();
    for r in rules(cx) {
        let Some(t) = r.target else { continue };
        match by_target.iter_mut().find(|(x, _)| *x == t) {
            Some((_, list)) => list.push(r),
            None => by_target.push((t, vec![r])),
        }
    }
    for (_, list) in by_target {
        if list.iter().any(|r| r.envs.is_empty()) {
            continue;
        }
        let covered: HashSet<&str> = list
            .iter()
            .flat_map(|r| r.envs.iter().map(String::as_str))
            .collect();
        let missing: Vec<&str> = ENVIRONMENTS
            .iter()
            .copied()
            .filter(|e| !covered.contains(e))
            .collect();
        if missing.is_empty() {
            continue;
        }
        let first = list[0];
        let names: Vec<String> = missing.iter().map(|m| format!("`.{m}`")).collect();
        cx.report(
            d.file,
            Diagnostic::new(
                codes::ENVIRONMENT_NOT_COVERED,
                p.span(d.file, first.target_range),
                "the `use` rules for this interface leave environments uncovered",
            )
            .with_note(format!("no rule covers {}", names.join(", "))),
        );
    }
}

/// K4007 for server-only declarations named in code that reaches the
/// browser: imports in files that hold only such code, and type and name
/// references inside it. Runs after every body recorded its resolutions.
///
/// `names` holds the program's declaration names, rendered once per
/// revision (the result keeps them for rendering types).
pub(crate) fn client_references(cx: &Cx<'_, '_>, names: &crate::ty::NameTable) {
    let p = cx.p;
    let mut by_name: HashMap<&str, DeclId> = HashMap::new();
    for i in 0..p.decls.len() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let decl = p.decl(d);
        if decl.is_type() || decl.named_kind() == Some(K::Function) {
            by_name.entry(names.decl(d)).or_insert(d);
        }
    }
    // A use already reported (K3021 at a `@Load` type, …) isn't reported
    // again.
    let reported_at: HashSet<(FileId, TextRange)> = cx
        .diags
        .borrow()
        .iter()
        .map(|d| (d.span.file, d.span.range))
        .collect();
    // Declaration references by file, built once (linear in the
    // resolutions).
    let mut refs_by_file: HashMap<FileId, Vec<(TextRange, DeclId)>> = HashMap::new();
    let live = cx.resolutions.borrow();
    let chunks = cx.body_resolutions.borrow();
    for ((f, r), res) in live.iter().chain(chunks.iter().flat_map(|c| c.iter())) {
        if let Resolution::Declaration { name } = res {
            if let Some(d) = by_name.get(name.as_str()) {
                refs_by_file.entry(*f).or_default().push((*r, *d));
            }
        }
    }
    drop(live);
    drop(chunks);
    let client = client_code(cx, &refs_by_file);
    for (ix, file) in p.files.iter().enumerate() {
        if file.stdlib {
            continue;
        }
        let fix = FileIx(u32::try_from(ix).unwrap_or(u32::MAX));
        let client_decls: Vec<DeclId> = file
            .decls
            .iter()
            .copied()
            .filter(|d| client.contains(d))
            .collect();
        if client_decls.is_empty() {
            continue;
        }
        let all_client = file
            .decls
            .iter()
            .all(|d| p.decl(*d).named_kind().is_none() || client_decls.contains(d));
        let mut reported: HashSet<DeclId> = HashSet::new();
        let inject_ranges: HashSet<TextRange> = client_decls
            .iter()
            .flat_map(|d| inject_fields(cx, *d).into_iter().map(|(_, r)| r))
            .collect();
        let mut refs: Vec<(TextRange, DeclId)> = refs_by_file.remove(&file.id).unwrap_or_default();
        refs.sort_by_key(|(r, _)| (r.start(), r.end()));
        let imports: Vec<TextRange> = file.imports.iter().map(|i| i.range).collect();
        // A declaration named only by `@Inject` types is a layering matter
        // (K4005, or K4007 at the field).
        // A declaration named only by `@Inject` types that break the
        // layering table is reported there (K4005).
        let used_outside_injects = |d: DeclId| {
            refs.iter().any(|(r, x)| {
                *x == d
                    && !imports.iter().any(|i| i.contains_range(*r))
                    && (!inject_ranges.iter().any(|i| i.contains_range(*r))
                        || matches!(
                            classify(cx, &Ty::named(d)),
                            Injectable::Service(_) | Injectable::Interface(..)
                        ))
            })
        };
        if all_client {
            for (i, res) in cx.imports[ix].iter().enumerate() {
                let crate::cx::ImportRes::Bound(_, d) = res else {
                    continue;
                };
                if side(cx, *d).server && !side(cx, *d).client && used_outside_injects(*d) {
                    reported.insert(*d);
                    let mut diag = Diagnostic::new(
                        codes::SERVER_ONLY_IN_CLIENT,
                        p.span(fix, file.imports[i].range),
                        format!(
                            "`{}` is server-only; this file reaches the browser",
                            p.decl_name(*d)
                        ),
                    );
                    // Through what it is server-only, when that is a
                    // `remote` client deeper down (40-workspaces-and-services.md).
                    if let Some(path) = server_path(cx, *d, &mut HashSet::new()) {
                        if path.decls.len() > 1
                            && path
                                .decls
                                .last()
                                .is_some_and(|&l| p.decl(l).named_kind() == Some(K::Protocol))
                        {
                            let names: Vec<String> =
                                path.decls.iter().map(|t| p.decl_name(*t)).collect();
                            diag =
                                diag.with_note(format!("{} ({})", names.join(" → "), path.reason));
                        }
                    }
                    cx.report(fix, diag);
                }
            }
        }
        for (range, d) in refs {
            if reported.contains(&d) || inject_ranges.iter().any(|r| r.contains_range(range)) {
                continue;
            }
            let inside = client_decls
                .iter()
                .any(|c| p.decl(*c).node.text_range().contains_range(range));
            let s = side(cx, d);
            if inside && s.server && !s.client && !reaches_client(cx, d) {
                // A generic type is reported whole (`Secret<String>`).
                let range = file
                    .root
                    .covering_element(range)
                    .ancestors()
                    .find(|n| n.kind() == SyntaxKind::PathType)
                    .filter(|n| n.text_range().start() == range.start())
                    .map_or(range, |n| n.text_range());
                if reported_at.contains(&(file.id, range)) {
                    continue;
                }
                cx.error(
                    fix,
                    codes::SERVER_ONLY_IN_CLIENT,
                    range,
                    format!(
                        "`{}` is server-only and can't be used in code that reaches the browser",
                        p.decl_name(d)
                    ),
                );
            }
        }
    }
}

/// The project declarations whose code runs in the browser (sema.md
/// §12.6): components, layouts and pages with a view ([`reaches_client`]),
/// and, transitively, the top-level functions they reference and the
/// shared services they inject (whose methods then run in the browser too).
/// References inside `@Load` initializers are not followed: the server
/// render awaits them.
///
/// A worklist over the recorded references, linear in them: each
/// declaration's references are visited once, when it joins the set.
/// Server-side services are not followed; injecting one is K4007 at the
/// injection already.
fn client_code(
    cx: &Cx<'_, '_>,
    refs_by_file: &HashMap<FileId, Vec<(TextRange, DeclId)>>,
) -> HashSet<DeclId> {
    let p = cx.p;
    let mut set: HashSet<DeclId> = HashSet::new();
    let mut stack: Vec<DeclId> = Vec::new();
    for i in 0..p.decls.len() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if !p.file(p.decl(d).file).stdlib && reaches_client(cx, d) && set.insert(d) {
            stack.push(d);
        }
    }
    let shared_service = |t: DeclId| {
        p.decl(t).named_kind() == Some(K::Service) && !p.is_stdlib(t) && !side(cx, t).server
    };
    while let Some(d) = stack.pop() {
        let decl = p.decl(d);
        let mut reached: Vec<DeclId> = Vec::new();
        for (m, _) in inject_fields(cx, d) {
            let what = classify(cx, &cx.field_ty(m));
            reached.extend(
                bound_services(cx, &what)
                    .into_iter()
                    .filter(|t| shared_service(*t)),
            );
        }
        let file_id = p.file(decl.file).id;
        let range = decl.node.text_range();
        // `@Load` initializers are awaited by the server render and their
        // values serialized (K3021 guards what crosses), so what they call
        // isn't followed.
        let loads: Vec<TextRange> = decl
            .members
            .iter()
            .map(|m| p.member(*m))
            .filter(|m| m.has_annotation("Load"))
            .filter_map(|m| m.node.as_ref().map(rowan::SyntaxNode::text_range))
            .collect();
        for (r, t) in refs_by_file.get(&file_id).map_or(&[][..], Vec::as_slice) {
            if !range.contains_range(*r)
                || p.is_stdlib(*t)
                || loads.iter().any(|l| l.contains_range(*r))
            {
                continue;
            }
            match p.decl(*t).named_kind() {
                Some(K::Function) => reached.push(*t),
                Some(K::Service) if shared_service(*t) => reached.push(*t),
                _ => {}
            }
        }
        for t in reached {
            if set.insert(t) {
                stack.push(t);
            }
        }
    }
    set
}

/// Why a declaration is server-only, for K4007's notes.
struct ServerPath {
    /// The injection path to the declaration that makes it server-only:
    /// `[Indirect, Server, Keys]`. An interface is stepped over: the path
    /// names the service chosen for it.
    decls: Vec<DeclId>,
    /// Why the last one is server-only: "a config is server-only".
    reason: String,
    /// The [`environment_note`] of the first interface on the path whose
    /// choice differs between environments.
    environment: Option<String>,
}

/// The [`ServerPath`] of what an injection resolves to; for an interface,
/// through the first environment's choice that is server-only.
fn server_path_of(
    cx: &Cx<'_, '_>,
    what: &Injectable,
    seen: &mut HashSet<DeclId>,
) -> Option<ServerPath> {
    match what {
        Injectable::Interface(i) => {
            for c in choices(cx, *i) {
                if !side(cx, c.imp).server {
                    continue;
                }
                if let Some(mut path) = server_path(cx, c.imp, seen) {
                    if path.environment.is_none() {
                        path.environment = environment_note(cx, *i, c.imp);
                    }
                    return Some(path);
                }
            }
            None
        }
        Injectable::Service(t)
        | Injectable::Platform(t)
        | Injectable::Infra(t)
        | Injectable::Repository(t)
        | Injectable::Config(t) => server_path(cx, *t, seen),
        Injectable::Protocol(p) if has_remote_rule(cx, *p) => Some(ServerPath {
            decls: vec![*p],
            reason: "a `remote` client, which holds a service token, is server-only".to_owned(),
            environment: None,
        }),
        Injectable::Protocol(_) | Injectable::Not | Injectable::Unknown => None,
    }
}

/// The [`ServerPath`] from `d`. `None` when `d` isn't server-only through
/// anything this can name.
fn server_path(cx: &Cx<'_, '_>, d: DeclId, seen: &mut HashSet<DeclId>) -> Option<ServerPath> {
    if !seen.insert(d) {
        return None;
    }
    let decl = cx.p.decl(d);
    let own = if decl.has_annotation("ServerOnly") {
        Some("it is `@ServerOnly`".to_owned())
    } else if decl.has_annotation("Platform") && platform_side(cx, d).as_deref() == Some("server") {
        Some("it is a server platform interface".to_owned())
    } else {
        match decl.named_kind() {
            Some(K::Entity) => Some("an entity is server-only".to_owned()),
            Some(K::Repository) => Some("a repository is server-only".to_owned()),
            Some(K::Config) => Some("a config is server-only".to_owned()),
            Some(K::Service) => {
                let scope = scope_of(cx, d);
                let protocol = cx.implements_list(d).iter().any(|(t, _)| {
                    t.decl()
                        .is_some_and(|i| cx.p.decl(i).named_kind() == Some(K::Protocol))
                });
                if protocol {
                    Some("it implements a protocol".to_owned())
                } else if scope == "request" || scope == "server" {
                    Some(format!("its scope is `.{scope}`"))
                } else {
                    None
                }
            }
            _ => None,
        }
    };
    if let Some(reason) = own {
        return Some(ServerPath {
            decls: vec![d],
            reason,
            environment: None,
        });
    }
    for (m, _) in inject_fields(cx, d) {
        let ty = cx.field_ty(m);
        if !injected_side(cx, &ty).server {
            continue;
        }
        if let Some(mut path) = server_path_of(cx, &classify(cx, &ty), seen) {
            path.decls.insert(0, d);
            return Some(path);
        }
    }
    None
}

/// Returns `true` if `file` imports declaration `d`.
fn imported_into(cx: &Cx<'_, '_>, file: FileIx, d: DeclId) -> bool {
    cx.imports[file.0 as usize]
        .iter()
        .any(|r| matches!(r, crate::cx::ImportRes::Bound(_, x) if *x == d))
}

/// The sides of every top-level declaration name, for `side_of`.
pub(crate) fn sides(cx: &Cx<'_, '_>) -> HashMap<(FileId, TextRange), Side> {
    let mut out = HashMap::new();
    for (i, decl) in cx.p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let file = cx.p.file(decl.file);
        if file.stdlib || decl.name.is_empty() || decl.named_kind().is_none() {
            continue;
        }
        out.insert((file.id, decl.name_range), side(cx, d).side());
    }
    out
}

// ------------------------------------------------------------ client-rendered components

/// The `@Rendering` annotation written on `node`, with `true` when its mode
/// is `.client` (47-client-rendered-components.md).
pub(crate) fn rendering_annotation(node: &k_syntax::SyntaxNode) -> Option<(ast::Annotation, bool)> {
    node.children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|n| n.text() == "Rendering"))
        .map(|a| {
            let client = a
                .syntax()
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .any(|t| t.text() == "client");
            (a, client)
        })
}

/// Returns `true` for a component marked `@Rendering(.client)`: the server
/// renders its `loading` block and the browser its `view`
/// (47-client-rendered-components.md).
pub(crate) fn client_component(cx: &Cx<'_, '_>, d: DeclId) -> bool {
    let decl = cx.p.decl(d);
    decl.named_kind() == Some(K::Component)
        && rendering_annotation(&decl.node).is_some_and(|(_, client)| client)
}

/// The ranges of `node` that render in the browser only: for a
/// client-rendered component everything but its `loading` block, and in
/// any view the slot content (lambdas) passed to a client-rendered
/// component.
pub(crate) fn browser_regions(cx: &Cx<'_, '_>, d: DeclId) -> Vec<TextRange> {
    let decl = cx.p.decl(d);
    let mut out = Vec::new();
    if client_component(cx, d) {
        let loading = decl
            .node
            .descendants()
            .find(|n| {
                n.kind() == SyntaxKind::LoadingBlock
                    && n.parent()
                        .is_some_and(|p| p.kind() == SyntaxKind::MemberList)
            })
            .map(|n| n.text_range());
        let whole = decl.node.text_range();
        match loading {
            Some(l) => {
                out.push(TextRange::new(whole.start(), l.start()));
                out.push(TextRange::new(l.end(), whole.end()));
            }
            None => out.push(whole),
        }
    }
    for call in decl.node.descendants().filter_map(ast::CallExpr::cast) {
        let Some(ast::Expr::NameExpr(callee)) = call.callee() else {
            continue;
        };
        let Some(name) = callee
            .syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::Ident)
        else {
            continue;
        };
        if !name.text().starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let crate::cx::Found::Decl(target) = cx.lookup(decl.file, name.text()) else {
            continue;
        };
        if !client_component(cx, target) {
            continue;
        }
        for lambda in call
            .syntax()
            .children()
            .flat_map(|c| {
                if c.kind() == SyntaxKind::ArgList {
                    c.descendants().collect::<Vec<_>>()
                } else {
                    vec![c]
                }
            })
            .filter(|n| n.kind() == SyntaxKind::LambdaExpr)
        {
            out.push(lambda.text_range());
        }
    }
    out
}

/// The components that render only in the browser: those whose every
/// use is inside a client-rendered component's subtree, transitively
/// (47-client-rendered-components.md, "Rules"). A client-rendered
/// component itself is not in the set; its `loading` block is
/// server-rendered.
pub(crate) fn browser_only_components(cx: &Cx<'_, '_>) -> std::rc::Rc<HashSet<DeclId>> {
    if let Some(set) = cx.fw.browser_only.borrow().as_ref() {
        return std::rc::Rc::clone(set);
    }
    let p = cx.p;
    let ids = || (0..p.decls.len()).map(|i| DeclId(u32::try_from(i).unwrap_or(u32::MAX)));
    let user = |d: DeclId| !p.file(p.decl(d).file).stdlib;
    let is_component = |d: DeclId| p.decl(d).named_kind() == Some(K::Component);
    // Each user declaration's references to components, split into the
    // server-rendered ones and those in a browser-only region.
    let mut server_refs: HashMap<DeclId, Vec<DeclId>> = HashMap::new();
    let mut referenced: HashSet<DeclId> = HashSet::new();
    for d in ids().filter(|d| user(*d)) {
        let decl = p.decl(d);
        let regions = browser_regions(cx, d);
        let mut refs = Vec::new();
        for t in decl
            .node
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
        {
            if t.kind() != SyntaxKind::Ident
                || !t.text().starts_with(|c: char| c.is_ascii_uppercase())
            {
                continue;
            }
            let crate::cx::Found::Decl(target) = cx.lookup(decl.file, t.text()) else {
                continue;
            };
            if target == d || !is_component(target) {
                continue;
            }
            referenced.insert(target);
            let at = t.text_range();
            if !regions.iter().any(|r| r.contains_range(at)) {
                refs.push(target);
            }
        }
        server_refs.insert(d, refs);
    }
    // Server-rendered: every user declaration that isn't a component,
    // every component nothing references, and what they reach.
    let mut server: HashSet<DeclId> = HashSet::new();
    let mut stack: Vec<DeclId> = ids()
        .filter(|d| user(*d) && (!is_component(*d) || !referenced.contains(d)))
        .collect();
    // A client-rendered component's `loading` is server-rendered wherever
    // the component is.
    stack.extend(ids().filter(|d| user(*d) && client_component(cx, *d)));
    while let Some(d) = stack.pop() {
        if !server.insert(d) {
            continue;
        }
        if let Some(refs) = server_refs.get(&d) {
            stack.extend(refs.iter().copied());
        }
    }
    let set: HashSet<DeclId> = ids()
        .filter(|d| user(*d) && is_component(*d) && !server.contains(d))
        .collect();
    let set = std::rc::Rc::new(set);
    *cx.fw.browser_only.borrow_mut() = Some(std::rc::Rc::clone(&set));
    set
}
