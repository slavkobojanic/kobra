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
    /// A user interface and the service chosen for it, if any.
    Interface(DeclId, Option<DeclId>),
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
    /// The implementation, `None` when it doesn't resolve (K1202).
    imp: Option<DeclId>,
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
fn compute_implementations(cx: &Cx<'_, '_>, iface: DeclId) -> Vec<DeclId> {
    let mut out = Vec::new();
    for (i, decl) in cx.p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if decl.named_kind() != Some(K::Service) {
            continue;
        }
        let file = cx.p.file(decl.file);
        let test_module = file
            .module
            .is_some_and(|m| cx.p.modules[m.0 as usize].test_only);
        if file.is_test || test_module {
            continue;
        }
        if cx.declares(d, iface) {
            out.push(d);
        }
    }
    out
}

/// The app's `use` rules.
fn rules<'c>(cx: &'c Cx<'_, '_>) -> &'c [UseRule] {
    cx.di.rules.get_or_init(|| collect_rules(cx))
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
            let chosen = rules(cx)
                .iter()
                .find(|r| r.target == Some(d))
                .map(|r| r.imp)
                .unwrap_or_else(|| {
                    let impls = implementations(cx, d);
                    if impls.len() == 1 {
                        impls.first().copied()
                    } else {
                        None
                    }
                });
            Injectable::Interface(d, chosen)
        }
        _ => Injectable::Not,
    }
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
        Injectable::Service(t)
        | Injectable::Interface(_, Some(t))
        | Injectable::Platform(t)
        | Injectable::Infra(t) => side(cx, t),
        Injectable::Repository(_) | Injectable::Config(_) => SERVER,
        Injectable::Protocol(_)
        | Injectable::Interface(_, None)
        | Injectable::Not
        | Injectable::Unknown => Sides::default(),
    }
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
            if let Injectable::Interface(iface, None) = &what {
                if kind != Some(K::TestSuite) {
                    missing_or_ambiguous(cx, decl.file, range, *iface);
                }
            }
            let target = match what {
                Injectable::Service(t) | Injectable::Interface(_, Some(t)) => Some(t),
                _ => None,
            };
            if let Some(t) = target {
                if kind == Some(K::Service) && !p.is_stdlib(t)
                    || kind == Some(K::Service) && p.is_stdlib(t)
                {
                    edges.push((d, t, m, range));
                    scopes(cx, d, t, decl.file, range);
                }
            }
            // An imported declaration is reported once, at the import
            // (`client_references`).
            let imported = ty.decl().is_some_and(|t| imported_into(cx, decl.file, t));
            if reaches_client(cx, d) && injected_side(cx, &ty).server && !imported {
                let mut diag = Diagnostic::new(
                    codes::SERVER_ONLY_IN_CLIENT,
                    p.span(decl.file, range),
                    format!(
                        "`{}` is server-only and can't be injected into code that reaches the browser",
                        ty.render(p)
                    ),
                );
                let path = ty
                    .decl()
                    .and_then(|t| server_path(cx, t, &mut HashSet::new()));
                if let Some((path, reason)) = path {
                    let mut names = vec![p.decl_name(d)];
                    names.extend(path.iter().map(|t| p.decl_name(*t)));
                    diag = diag.with_note(format!("{} ({reason})", names.join(" → ")));
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

/// K4004: a service injects only services that live at least as long.
fn scopes(cx: &Cx<'_, '_>, owner: DeclId, target: DeclId, file: FileIx, range: TextRange) {
    let p = cx.p;
    if is_named(cx, target, "k.time.Clock") {
        return;
    }
    let outer = scope_of(cx, owner);
    let inner = scope_of(cx, target);
    if lifetime(&inner) >= lifetime(&outer) {
        return;
    }
    cx.report(
        file,
        Diagnostic::new(
            codes::SCOPE_VIOLATION,
            p.span(file, range),
            format!(
                "a `.{outer}` service can't inject `{}`, which is `.{inner}` and may end sooner",
                p.decl_name(target)
            ),
        )
        .with_note(format!("{} → {}", p.decl_name(owner), p.decl_name(target))),
    );
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
    let Some(path) = session_path(cx, d, &mut HashSet::new()) else {
        return;
    };
    let decl = cx.p.decl(d);
    let mut names = vec![cx.p.decl_name(d)];
    names.extend(path.iter().map(|t| cx.p.decl_name(*t)));
    let last = path
        .last()
        .map_or_else(String::new, |t| cx.p.decl(*t).name.clone());
    cx.report(
        decl.file,
        Diagnostic::new(
            codes::JOB_REACHES_SESSION,
            cx.p.span(decl.file, decl.name_range),
            format!(
                "the job `{}` reaches `{last}`, but a job runs without a session",
                decl.name
            ),
        )
        .with_note(names.join(" → ")),
    );
}

/// The injection path from `d` (excluded) to `Session` or `SessionManager`
/// (included), if `d`'s dependency graph reaches one. A depth-first search
/// over services and the services chosen for interfaces; `seen` keeps it
/// linear in the graph.
fn session_path(cx: &Cx<'_, '_>, d: DeclId, seen: &mut HashSet<DeclId>) -> Option<Vec<DeclId>> {
    if !seen.insert(d) {
        return None;
    }
    for (m, _) in inject_fields(cx, d) {
        let next = match classify(cx, &cx.field_ty(m)) {
            Injectable::Service(t) | Injectable::Interface(_, Some(t)) | Injectable::Infra(t) => t,
            _ => continue,
        };
        if is_named(cx, next, "k.auth.Session") || is_named(cx, next, "k.auth.SessionManager") {
            return Some(vec![next]);
        }
        if let Some(mut rest) = session_path(cx, next, seen) {
            rest.insert(0, next);
            return Some(rest);
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
pub(crate) fn client_references(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let mut by_name: HashMap<String, DeclId> = HashMap::new();
    for i in 0..p.decls.len() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let decl = p.decl(d);
        if decl.is_type() || decl.named_kind() == Some(K::Function) {
            by_name.entry(p.decl_name(d)).or_insert(d);
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
            if let Some(d) = by_name.get(name) {
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
                    cx.error(
                        fix,
                        codes::SERVER_ONLY_IN_CLIENT,
                        file.imports[i].range,
                        format!(
                            "`{}` is server-only; this file reaches the browser",
                            p.decl_name(*d)
                        ),
                    );
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
            match classify(cx, &cx.field_ty(m)) {
                Injectable::Service(t) | Injectable::Interface(_, Some(t)) if shared_service(t) => {
                    reached.push(t);
                }
                _ => {}
            }
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

/// The injection path from `d` to the declaration that makes it
/// server-only, with the reason, for K4007's note: `[Indirect, Server,
/// Keys]` and "a config is server-only". `None` when `d` isn't server-only
/// through anything this can name.
fn server_path(
    cx: &Cx<'_, '_>,
    d: DeclId,
    seen: &mut HashSet<DeclId>,
) -> Option<(Vec<DeclId>, String)> {
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
        return Some((vec![d], reason));
    }
    for (m, _) in inject_fields(cx, d) {
        let ty = cx.field_ty(m);
        if !injected_side(cx, &ty).server {
            continue;
        }
        let next = match classify(cx, &ty) {
            Injectable::Service(t)
            | Injectable::Interface(_, Some(t))
            | Injectable::Platform(t)
            | Injectable::Infra(t)
            | Injectable::Repository(t)
            | Injectable::Config(t) => t,
            _ => continue,
        };
        if let Some((mut path, reason)) = server_path(cx, next, seen) {
            path.insert(0, d);
            return Some((path, reason));
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
