//! Pages, endpoint pages, URLs and guards (sema.md §12.7).
//!
//! The route table is computed once over the whole program: every page's
//! URL (its `at` template or the default derived from its name and path
//! `@Param`s), checked for conflicts (K5001); `@Param` types (K5002) and
//! templates (K5003); the endpoint-page rules (K5004, K5005, K5009–K5012,
//! K4022); and `requires` guards on pages, protocols and rpcs (K5006–K5008).
//!
//! # URL shapes
//!
//! Two URLs conflict when they match the same paths: their templates are
//! compared with every `{param}` segment replaced by `{}`, so
//! `/product/{id}` and `/product/{slug}` conflict.

use std::collections::HashMap;

use k_diagnostics::{Diagnostic, Fix, Span, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::body::{BodyOwner, Checker};
use crate::cx::{Cx, Found};
use crate::framework::{HANDLER_METHODS, is_endpoint_page};
use crate::program::{DeclId, FileIx, MemberId, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// One segment of an `at` template.
#[derive(Clone, Debug)]
enum Segment {
    /// Literal text.
    Text(String),
    /// `{name}`, with the range of the whole interpolation.
    Param(String, TextRange),
}

/// A `@Param` field.
struct Param {
    /// The field.
    member: MemberId,
    /// Its name.
    name: String,
    /// `true` for path parameters (no default, not optional).
    required: bool,
}

/// Converts a declaration name to kebab case: `SignIn` → `sign-in`.
pub(crate) fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The `@Param` fields of a page.
fn params(cx: &Cx<'_, '_>, page: DeclId) -> Vec<Param> {
    cx.p.decl(page)
        .members
        .iter()
        .copied()
        .filter(|m| {
            let m = cx.p.member(*m);
            m.kind == MemberKind::Field && m.has_annotation("Param")
        })
        .map(|m| {
            let member = cx.p.member(m);
            let ty = cx.field_ty(m);
            Param {
                member: m,
                name: member.name.clone(),
                required: !member.has_default && !matches!(ty, Ty::Optional(_)),
            }
        })
        .collect()
}

/// The `at` clause's string literal and its segments.
fn template(node: &SyntaxNode) -> Option<(TextRange, Vec<Segment>)> {
    let clause = node.children().find(|c| c.kind() == SyntaxKind::AtClause)?;
    let lit = clause
        .children()
        .find(|c| c.kind() == SyntaxKind::StringLit)?;
    let mut segments = Vec::new();
    for el in lit.children_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::StringText => {
                segments.push(Segment::Text(t.text().to_owned()));
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::Interpolation => {
                let name = n
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident)
                    .map(|t| t.text().to_owned())
                    .unwrap_or_default();
                segments.push(Segment::Param(name, n.text_range()));
            }
            _ => {}
        }
    }
    Some((lit.text_range(), segments))
}

/// The URL shape of a page: its template with parameters as `{}`.
fn shape(segments: &[Segment]) -> String {
    let mut out = String::new();
    for s in segments {
        match s {
            Segment::Text(t) => out.push_str(t),
            Segment::Param(..) => out.push_str("{}"),
        }
    }
    if out.len() > 1 {
        while out.ends_with('/') {
            out.pop();
        }
    }
    out
}

/// Returns `true` for `@Param` types a URL can carry.
fn routable(cx: &Cx<'_, '_>, ty: &Ty) -> bool {
    match ty {
        Ty::Error => true,
        Ty::Optional(inner) => routable(cx, inner),
        Ty::Named(d, args) if args.is_empty() => {
            let b = &cx.b;
            if [b.string, b.int, b.page].contains(&Some(*d))
                || cx.p.lookup_qualified("k.crypto.Uuid") == Some(*d)
            {
                return true;
            }
            cx.is_enum(*d) && cx.cases(*d).iter().all(|c| !cx.case_has_payload(*c))
        }
        _ => false,
    }
}

/// The routing checks over the whole program.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let mut urls: HashMap<String, (FileIx, TextRange)> = HashMap::new();
    let mut requires_auth = false;
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib {
            continue;
        }
        match decl.named_kind() {
            Some(K::Page) => {
                page(cx, d, &mut urls);
                if guards(cx, d).iter().any(|(n, _)| n == "Authenticated") {
                    requires_auth = true;
                }
            }
            Some(K::Layout) => guard_on_layout(cx, d),
            Some(K::Protocol) => {
                check_guards(cx, d, &decl.node);
                for &m in &decl.members {
                    if let Some(node) = &p.member(m).node {
                        check_guards(cx, d, node);
                    }
                }
            }
            _ => {}
        }
    }
    sign_in(cx, requires_auth);
}

/// K5014: a layout's `requires` clause. The server checks the guards of
/// the page it answers, never of the layout it renders in, so the clause
/// would guard nothing; its guards aren't resolved either.
fn guard_on_layout(cx: &Cx<'_, '_>, d: DeclId) {
    let p = cx.p;
    let decl = p.decl(d);
    let Some(clause) = decl
        .node
        .children()
        .find(|c| c.kind() == SyntaxKind::RequiresClause)
    else {
        return;
    };
    cx.report(
        decl.file,
        Diagnostic::new(
            codes::GUARD_ON_LAYOUT,
            p.span(decl.file, clause.text_range()),
            "the server never checks a layout's `requires` clause",
        )
        .with_help(format!(
            "the server checks the guards of the page it answers: put the clause on each page that renders in `{}`",
            decl.name
        )),
    );
}

/// The guards of a declaration's `requires` clause: names and nodes.
fn guards(cx: &Cx<'_, '_>, d: DeclId) -> Vec<(String, SyntaxNode)> {
    guard_nodes(&cx.p.decl(d).node)
}

/// The guards of the `requires` clauses directly on `node`.
fn guard_nodes(node: &SyntaxNode) -> Vec<(String, SyntaxNode)> {
    node.children()
        .filter(|c| c.kind() == SyntaxKind::RequiresClause)
        .flat_map(|c| {
            c.children()
                .filter(|g| g.kind() == SyntaxKind::Guard)
                .collect::<Vec<_>>()
        })
        .filter_map(|g| {
            let name = g
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::Ident)?;
            Some((name.text().to_owned(), g))
        })
        .collect()
}

/// One page: `@Param` types, its template or default URL, conflicts, guards and the endpoint rules.
fn page(cx: &Cx<'_, '_>, d: DeclId, urls: &mut HashMap<String, (FileIx, TextRange)>) {
    let p = cx.p;
    let decl = p.decl(d);
    let file = decl.file;
    let endpoint = is_endpoint_page(cx, d);
    let params = params(cx, d);
    // `@Param` types (K5002).
    for param in &params {
        let ty = cx.field_ty(param.member);
        if !routable(cx, &ty) {
            let range = p
                .member(param.member)
                .node
                .as_ref()
                .and_then(|n| ast::FieldDecl::cast(n.clone()))
                .and_then(|f| f.ty())
                .map_or(p.member(param.member).name_range, |t| {
                    t.syntax().text_range()
                });
            cx.error(
                file,
                codes::UNROUTABLE_PARAM,
                range,
                format!("a URL can't carry a `{}`", ty.render(p)),
            );
        }
    }
    let (url_range, segments) = match template(&decl.node) {
        Some((range, segments)) => {
            check_template(cx, d, &params, &segments, endpoint);
            (range, segments)
        }
        None => {
            if endpoint {
                let at = decl.name_range.end();
                let text = format!(" at \"/{}\"", kebab(&decl.name));
                cx.report(
                    file,
                    Diagnostic::new(
                        codes::ENDPOINT_WITHOUT_AT,
                        p.span(file, decl.name_range),
                        "an endpoint page needs an `at` URL",
                    )
                    .with_fix(Fix::replace(
                        format!("Add `{}`", text.trim_start()),
                        Span::empty(p.file(file).id, at),
                        text,
                    )),
                );
            }
            let mut segments = vec![Segment::Text(format!("/{}", kebab(&decl.name)))];
            for param in params.iter().filter(|x| x.required) {
                segments.push(Segment::Text("/".to_owned()));
                segments.push(Segment::Param(param.name.clone(), decl.name_range));
            }
            (decl.name_range, segments)
        }
    };
    // URL conflicts (K5001), at the later page.
    let key = shape(&segments);
    match urls.get(&key) {
        Some((first_file, first)) => {
            cx.report(
                file,
                Diagnostic::new(
                    codes::URL_CONFLICT,
                    p.span(file, url_range),
                    format!("another page already has the URL `{key}`"),
                )
                .with_label(p.span(*first_file, *first), "first declared here"),
            );
        }
        None => {
            urls.insert(key, (file, url_range));
        }
    }
    check_guards(cx, d, &decl.node);
    if endpoint {
        endpoint_rules(cx, d);
    }
}

/// K5003: every path `@Param` exactly once, no unknown segments; endpoint
/// pages list every `@Param`.
fn check_template(
    cx: &Cx<'_, '_>,
    d: DeclId,
    params: &[Param],
    segments: &[Segment],
    endpoint: bool,
) {
    let p = cx.p;
    let file = p.decl(d).file;
    let mut seen: Vec<&str> = Vec::new();
    for s in segments {
        let Segment::Param(name, range) = s else {
            continue;
        };
        if !params.iter().any(|x| &x.name == name) {
            cx.error(
                file,
                codes::AT_TEMPLATE_MISMATCH,
                *range,
                format!("no `@Param` is named `{name}`"),
            );
            continue;
        }
        if seen.contains(&name.as_str()) {
            cx.error(
                file,
                codes::AT_TEMPLATE_MISMATCH,
                *range,
                format!("`{name}` appears twice in the template"),
            );
            continue;
        }
        seen.push(name);
    }
    for param in params {
        if (param.required || endpoint) && !seen.contains(&param.name.as_str()) {
            cx.error(
                file,
                codes::AT_TEMPLATE_MISMATCH,
                p.member(param.member).name_range,
                format!("the `at` template doesn't mention `{}`", param.name),
            );
        }
    }
}

/// `requires` guards: types marked `@Guard` (K5006), their arguments
/// checked like a call; unknown names are K1201.
fn check_guards(cx: &Cx<'_, '_>, d: DeclId, node: &SyntaxNode) {
    let p = cx.p;
    let file = p.decl(d).file;
    let owner = BodyOwner {
        file,
        decl: None,
        self_ty: None,
        generics: Vec::new(),
    };
    for (name, g) in guard_nodes(node) {
        let Some(tok) = g
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::Ident)
        else {
            continue;
        };
        match cx.lookup(file, &name) {
            Found::Decl(guard) => {
                cx.record_resolution(file, tok.text_range(), cx.decl_resolution(guard));
                if !cx.p.decl(guard).has_annotation("Guard") {
                    cx.error(
                        file,
                        codes::NOT_A_GUARD,
                        g.text_range(),
                        format!("`{name}` is not a guard"),
                    );
                    continue;
                }
                let mut checker = Checker::new(cx, owner.clone());
                checker.check_construct(&g, guard, tok.text_range());
                checker.finish();
            }
            Found::Missing => cx.error(
                file,
                codes::UNRESOLVED_NAME,
                tok.text_range(),
                format!("cannot find `{name}` in this scope"),
            ),
            _ => {}
        }
    }
}

/// The first non-trivia token of a node.
fn first_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| !t.kind().is_trivia())
}

/// The endpoint-page rules: no `Authenticated`/`Role` (K5005), no layout
/// annotations (K5009), no view-like blocks (K5010), one handler per
/// method (K5011) with the fixed signature (K5012), and `Session` injected
/// when the dependencies reach it (K4022).
fn endpoint_rules(cx: &Cx<'_, '_>, d: DeclId) {
    let p = cx.p;
    let decl = p.decl(d);
    let file = decl.file;
    for (name, g) in guards(cx, d) {
        if name == "Authenticated" || name == "Role" {
            cx.error(
                file,
                codes::GUARD_ON_ENDPOINT,
                g.text_range(),
                format!("`{name}` doesn't apply to endpoint pages, which have no sign-in redirect"),
            );
        }
    }
    for a in crate::framework::annotations_of(cx, file, &decl.node) {
        let n = &p.decl(a.decl).name;
        if n == "Layout" || n == "NoLayout" {
            cx.error(
                file,
                codes::LAYOUT_ON_ENDPOINT,
                a.range,
                "an endpoint page renders no layout",
            );
        }
    }
    if let Some(list) = decl
        .node
        .children()
        .find(|c| c.kind() == SyntaxKind::MemberList)
    {
        for child in list.children() {
            if matches!(
                child.kind(),
                SyntaxKind::ViewBlock
                    | SyntaxKind::MetadataBlock
                    | SyntaxKind::LoadingBlock
                    | SyntaxKind::FailedBlock
                    | SyntaxKind::StyleBlock
            ) {
                if let Some(kw) = first_token(&child) {
                    cx.error(
                        file,
                        codes::ENDPOINT_MEMBER_NOT_ALLOWED,
                        kw.text_range(),
                        format!("an endpoint page has handlers, not a `{}` block", kw.text()),
                    );
                }
            }
        }
    }
    let request = p.lookup_qualified("k.http.IncomingRequest");
    let response = p.lookup_qualified("k.http.HttpResponse");
    let mut methods: HashMap<String, TextRange> = HashMap::new();
    for &m in &decl.members {
        let member = p.member(m);
        let Some(node) = &member.node else { continue };
        if member.kind != MemberKind::Func {
            continue;
        }
        let handler = crate::framework::annotations_of(cx, file, node)
            .into_iter()
            .find(|a| HANDLER_METHODS.contains(&p.decl(a.decl).name.as_str()));
        let Some(handler) = handler else { continue };
        let method = p.decl(handler.decl).name.clone();
        if let Some(first) = methods.get(&method) {
            cx.report(
                file,
                Diagnostic::new(
                    codes::DUPLICATE_HANDLER_METHOD,
                    p.span(file, handler.range),
                    format!("`@{method}` is handled twice"),
                )
                .with_label(p.span(file, *first), "first declared here"),
            );
        } else {
            methods.insert(method, handler.range);
        }
        let sig = cx.sig(crate::cx::SigKey::Member(m));
        let ok = sig.params.len() == 1
            && sig.params[0].label.as_deref() == Some("request")
            && (sig.params[0].ty.decl() == request || sig.params[0].ty.contains_error())
            && sig.is_async
            && sig.throws.is_none()
            && (sig.ret.decl() == response || sig.ret.contains_error());
        if !ok {
            cx.error(
                file,
                codes::INVALID_HANDLER_SIGNATURE,
                member.name_range,
                "a handler is `func name(request: IncomingRequest) async -> HttpResponse`",
            );
        }
    }
    // K4022: the dependencies reach the session, but the page doesn't
    // inject `Session` itself.
    let session = p.lookup_qualified("k.auth.Session");
    let manager = p.lookup_qualified("k.auth.SessionManager");
    let fields = crate::di::inject_fields(cx, d);
    let direct = fields
        .iter()
        .any(|(m, _)| cx.field_ty(*m).decl().is_some() && cx.field_ty(*m).decl() == session);
    if direct {
        return;
    }
    let mut stack: Vec<DeclId> = fields
        .iter()
        .filter_map(|(m, _)| cx.field_ty(*m).decl())
        .collect();
    let mut seen = std::collections::HashSet::new();
    while let Some(t) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        if Some(t) == session || Some(t) == manager {
            cx.report(
                file,
                Diagnostic::new(
                    codes::ENDPOINT_SESSION_NOT_INJECTED,
                    p.span(file, decl.name_range),
                    "this endpoint page's dependencies use the session; inject `Session` here so its cookies are visible",
                )
                .with_note(format!("{} reaches `{}`", decl.name, p.decl_name(t))),
            );
            return;
        }
        if p.decl(t).named_kind() == Some(K::Service) {
            for (m, _) in crate::di::inject_fields(cx, t) {
                if let Some(x) = cx.field_ty(m).decl() {
                    stack.push(x);
                }
            }
        }
    }
}

/// K5007 (an app whose pages require sign-in names a `@SignInPage`) and
/// K5008 (the sign-in page doesn't itself require it).
fn sign_in(cx: &Cx<'_, '_>, requires_auth: bool) {
    let p = cx.p;
    let Some((app, d)) = p
        .decls
        .iter()
        .enumerate()
        .find(|(_, d)| d.named_kind() == Some(K::App) && !p.file(d.file).stdlib)
    else {
        return;
    };
    let _ = app;
    let annotation = d
        .node
        .children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|n| n.text() == "SignInPage"));
    let Some(annotation) = annotation else {
        if requires_auth {
            cx.error(
                d.file,
                codes::MISSING_SIGN_IN_PAGE,
                d.name_range,
                "pages require a signed-in user, but the app names no `@SignInPage`",
            );
        }
        return;
    };
    let case = annotation
        .arg_list()
        .and_then(|l| l.args().next())
        .and_then(|a| a.value())
        .and_then(|v| {
            v.syntax()
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::Ident)
        });
    let Some(case) = case else { return };
    let Some(page) = p.page_of.iter().find_map(|(m, page)| {
        (p.member(*m).name == case.text() && Some(p.member(*m).owner) == cx.b.page).then_some(*page)
    }) else {
        return;
    };
    for (name, g) in guards(cx, page) {
        if name == "Authenticated" {
            cx.error(
                p.decl(page).file,
                codes::SIGN_IN_PAGE_REQUIRES_AUTHENTICATED,
                g.text_range(),
                "the sign-in page can't require a signed-in user",
            );
        }
    }
}
