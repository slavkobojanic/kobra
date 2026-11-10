//! `@Worker` services (`docs/design/55-workers.md`, sema.md §12.6).
//!
//! A `@Worker` service runs in a Web Worker: it is client-only
//! ([`crate::di`]), and calling it while the server renders is K4012
//! (`body.rs`). This pass checks its declaration:
//!
//! * it keeps no state and has no page-facing members besides its methods:
//!   `@State`, `@Derived`, `var` and non-`private` fields, `init()`,
//!   `@Effect`, `@Scope`, an `rpc` and a non-`private` `static` member are
//!   K4035;
//! * it injects nothing (K4036);
//! * every non-`private` instance method is `async throws E`, not generic
//!   (K4037), with `E` `WorkerOnlyError` or an enum with exactly one
//!   `workerFailure(WorkerFailure)` case (K4038), and its parameters,
//!   result and `E` `Codable` (K4039).

use k_diagnostics::{Diagnostic, Fix, Span, TextRange, TextSize, codes};
use k_syntax::ast::{self, AstNode, Visibility};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Cx, SigKey};
use crate::program::{DeclId, FileIx, MemberId, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// `true` for a service marked `@Worker`.
pub(crate) fn is_worker(cx: &Cx<'_, '_>, d: DeclId) -> bool {
    let decl = cx.p.decl(d);
    decl.named_kind() == Some(K::Service) && decl.has_annotation("Worker")
}

/// The worker checks over the whole program.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    for i in 0..p.decls.len() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(p.decl(d).file).stdlib || !is_worker(cx, d) {
            continue;
        }
        let decl = p.decl(d);
        for a in decl.node.children().filter_map(ast::Annotation::cast) {
            if a.name().is_some_and(|n| n.text() == "Scope") {
                not_allowed(
                    cx,
                    decl.file,
                    a.syntax().text_range(),
                    "a `@Worker` service has no scope: there is one worker per app",
                    Some((
                        "Remove `@Scope`",
                        cx.line_range(decl.file, a.syntax().text_range()),
                    )),
                );
            }
        }
        for &m in &decl.members {
            member(cx, decl.file, m);
        }
    }
}

/// The checks of one member of a worker.
fn member(cx: &Cx<'_, '_>, file: FileIx, m: MemberId) {
    let p = cx.p;
    let mi = p.member(m);
    let Some(node) = mi.node.clone() else { return };
    let private = mi.visibility == Visibility::Private;
    let whole = cx.line_range(file, node.text_range());
    match mi.kind {
        MemberKind::Field if mi.has_annotation("Inject") => {
            let span = p.span(file, mi.name_range);
            cx.report(
                file,
                Diagnostic::new(
                    codes::WORKER_INJECTION,
                    span,
                    format!(
                        "a `@Worker` service injects nothing, but `{}` is injected",
                        mi.name
                    ),
                )
                .with_help(
                    "the worker has no container: pass what a method needs as an argument, \
                     and keep rpcs, storage and the page's services on the main thread",
                )
                .with_fix(Fix::replace(
                    "Remove the injected field",
                    p.span(file, whole),
                    "",
                )),
            );
        }
        MemberKind::Field | MemberKind::Computed => {
            let why = if mi.has_annotation("State") || mi.has_annotation("Derived") {
                Some(format!(
                    "a `@Worker` service keeps no state, but `{}` is `@{}`: the worker can be \
                     restarted at any call",
                    mi.name,
                    if mi.has_annotation("State") {
                        "State"
                    } else {
                        "Derived"
                    }
                ))
            } else if mi.kind == MemberKind::Field && mi.is_var {
                Some(format!(
                    "a `@Worker` service keeps no state, but `{}` is a `var`: the worker can be \
                     restarted at any call",
                    mi.name
                ))
            } else if mi.kind == MemberKind::Field && !mi.has_default && !mi.is_static {
                Some(format!(
                    "`{}` has no value: a `@Worker` service's constants are written with their \
                     initializer",
                    mi.name
                ))
            } else if !private {
                Some(format!(
                    "`{}` would be read by the page, but it lives in the worker: make it `private`",
                    mi.name
                ))
            } else {
                None
            };
            if let Some(why) = why {
                let fix = (!private
                    && !(mi.kind == MemberKind::Field && mi.is_var)
                    && !mi.has_annotation("State")
                    && !mi.has_annotation("Derived")
                    && (mi.has_default || mi.kind == MemberKind::Computed || mi.is_static))
                    .then(|| make_private(cx, file, &node));
                report_member(cx, file, mi.name_range, why, fix);
            }
        }
        MemberKind::Init => {
            report_member(
                cx,
                file,
                mi.name_range,
                "a `@Worker` service has no `init()`: the worker can be restarted at any call, \
                 so it keeps no state to set up"
                    .to_owned(),
                Some(Fix::replace("Remove `init()`", p.span(file, whole), "")),
            );
        }
        MemberKind::Rpc => {
            report_member(
                cx,
                file,
                mi.name_range,
                "a `@Worker` service has no rpcs: its methods are `func`s the page calls"
                    .to_owned(),
                None,
            );
        }
        MemberKind::Func if mi.has_annotation("Effect") => {
            report_member(
                cx,
                file,
                mi.name_range,
                "a `@Worker` service has no effects: it reads no reactive state".to_owned(),
                None,
            );
        }
        MemberKind::Func if private => {}
        MemberKind::Func if mi.is_static => {
            report_member(
                cx,
                file,
                mi.name_range,
                format!(
                    "`{}` is static, so the page would run it on the main thread: make it \
                     `private`, or an instance method",
                    mi.name
                ),
                Some(make_private(cx, file, &node)),
            );
        }
        MemberKind::Func => method(cx, file, m, &node),
        _ => {}
    }
}

/// K4035 at a member's name.
fn report_member(cx: &Cx<'_, '_>, file: FileIx, at: TextRange, why: String, fix: Option<Fix>) {
    let mut d = Diagnostic::new(codes::WORKER_MEMBER_NOT_ALLOWED, cx.p.span(file, at), why);
    if let Some(fix) = fix {
        d = d.with_fix(fix);
    }
    cx.report(file, d);
}

/// K4035 at `range`, with an optional fix deleting `remove`.
fn not_allowed(
    cx: &Cx<'_, '_>,
    file: FileIx,
    range: TextRange,
    why: &str,
    remove: Option<(&str, TextRange)>,
) {
    let mut d = Diagnostic::new(
        codes::WORKER_MEMBER_NOT_ALLOWED,
        cx.p.span(file, range),
        why.to_owned(),
    );
    if let Some((title, r)) = remove {
        d = d.with_fix(Fix::replace(title, cx.p.span(file, r), ""));
    }
    cx.report(file, d);
}

/// The fix that writes `private ` before a member's keyword (after its
/// annotations and doc comment).
fn make_private(cx: &Cx<'_, '_>, file: FileIx, node: &SyntaxNode) -> Fix {
    let at = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| {
            matches!(
                t.kind(),
                SyntaxKind::PublicKw
                    | SyntaxKind::StaticKw
                    | SyntaxKind::LetKw
                    | SyntaxKind::VarKw
                    | SyntaxKind::FuncKw
            )
        })
        .map_or(node.text_range(), |t| t.text_range());
    let id = cx.p.file(file).id;
    if node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .any(|t| t.kind() == SyntaxKind::PublicKw)
    {
        return Fix::replace("Make it `private`", Span::new(id, at), "private");
    }
    Fix::replace("Make it `private`", Span::empty(id, at.start()), "private ")
}

/// A method the page calls: `async throws E` and not generic (K4037),
/// `E` a worker error enum (K4038), everything `Codable` (K4039).
fn method(cx: &Cx<'_, '_>, file: FileIx, m: MemberId, node: &SyntaxNode) {
    let p = cx.p;
    let mi = p.member(m);
    let id = p.file(file).id;
    if let Some(list) = node
        .children()
        .find(|c| c.kind() == SyntaxKind::GenericParamList)
    {
        cx.report(
            file,
            Diagnostic::new(
                codes::WORKER_METHOD_SIGNATURE,
                p.span(file, list.text_range()),
                format!(
                    "worker method `{}` can't be generic: its values cross to the worker through \
                     their codecs, which need the concrete types",
                    mi.name
                ),
            ),
        );
        return;
    }
    let sig = cx.sig(SigKey::Member(m));
    let params_end = node
        .children()
        .find(|c| c.kind() == SyntaxKind::ParamList)
        .map_or(mi.name_range.end(), |l| l.text_range().end());
    let async_end: Option<TextSize> = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::AsyncKw)
        .map(|t| t.text_range().end());
    let throws = node
        .children()
        .find(|c| c.kind() == SyntaxKind::ThrowsClause);
    if !sig.is_async || throws.is_none() {
        let (message, insert) = match (sig.is_async, throws.is_some()) {
            (false, false) => (
                format!(
                    "worker method `{}` must be `async throws`: the page awaits its answer from \
                     another thread, which can fail",
                    mi.name
                ),
                " async throws WorkerOnlyError",
            ),
            (false, true) => (
                format!(
                    "worker method `{}` must be `async`: the page awaits its answer from another \
                     thread",
                    mi.name
                ),
                " async",
            ),
            _ => (
                format!(
                    "worker method `{}` must declare `throws`: the worker can fail while it runs \
                     the call",
                    mi.name
                ),
                " throws WorkerOnlyError",
            ),
        };
        let at = if insert == " throws WorkerOnlyError" {
            async_end.unwrap_or(params_end)
        } else {
            params_end
        };
        let mut fix = Fix::replace(
            format!("Add `{}`", insert.trim()),
            Span::empty(id, at),
            insert,
        );
        if insert.contains("WorkerOnlyError")
            && let Some(edit) = cx.import_edit(file, "k.concurrency.WorkerOnlyError")
        {
            fix.edits.push(edit);
            fix.edits
                .sort_by_key(|e| (e.span.file, e.span.range.start(), e.span.range.end()));
        }
        cx.report(
            file,
            Diagnostic::new(
                codes::WORKER_METHOD_SIGNATURE,
                p.span(file, mi.name_range),
                message,
            )
            .with_fix(fix),
        );
    }
    let codable = cx.b.codable;
    let is_codable = |t: &Ty| t.contains_error() || codable.is_none_or(|c| cx.implements(t, c));
    if let Some(list) = node.children().find_map(ast::ParamList::cast) {
        for (param, sp) in list.params().zip(sig.params.iter()) {
            if !is_codable(&sp.ty) {
                let range = param
                    .syntax()
                    .children()
                    .find_map(ast::Type::cast)
                    .map_or(param.syntax().text_range(), |t| t.syntax().text_range());
                not_codable(
                    cx,
                    file,
                    range,
                    &sp.ty,
                    "a worker method's parameter",
                    false,
                );
            }
        }
    }
    if let Some(ret) = node
        .children()
        .find(|c| c.kind() == SyntaxKind::ReturnType)
        .and_then(|r| r.children().find_map(ast::Type::cast))
        && !is_codable(&sig.ret)
    {
        let stream = matches!(&sig.ret, Ty::Named(d, _) if Some(*d) == cx.b.stream);
        not_codable(
            cx,
            file,
            ret.syntax().text_range(),
            &sig.ret,
            "a worker method's result",
            stream,
        );
    }
    let (Some(throws), Some(error)) = (throws, sig.throws.clone()) else {
        return;
    };
    if error.contains_error() {
        return;
    }
    let range = throws
        .children()
        .find_map(ast::Type::cast)
        .map_or(throws.text_range(), |t| t.syntax().text_range());
    error_enum(cx, file, range, &error);
}

/// K4038 and K4039 for a worker method's error `error`, written at
/// `range`.
fn error_enum(cx: &Cx<'_, '_>, file: FileIx, range: TextRange, error: &Ty) {
    let p = cx.p;
    let Some(e) = error.decl() else { return };
    if p.lookup_qualified("k.concurrency.WorkerOnlyError") == Some(e) {
        return;
    }
    let failure = p.lookup_qualified("k.concurrency.WorkerFailure");
    let cases = if cx.is_enum(e) {
        cx.cases(e)
    } else {
        Vec::new()
    };
    let failures: Vec<MemberId> = cases
        .iter()
        .copied()
        .filter(|c| {
            let s = cx.case_sig(*c);
            s.params.len() == 1
                && s.params[0].ty.decl().is_some()
                && s.params[0].ty.decl() == failure
        })
        .collect();
    let named_ok = failures.len() == 1 && p.member(failures[0]).name == "workerFailure";
    if !cx.is_enum(e) || !named_ok {
        let edecl = p.decl(e);
        let mut d = Diagnostic::new(
            codes::WORKER_ERROR_WITHOUT_FAILURE_CASE,
            p.span(file, range),
            format!(
                "a worker method's error is `WorkerOnlyError` or an enum with exactly one \
                 `workerFailure(WorkerFailure)` case; `{}` isn't",
                p.decl_name(e)
            ),
        )
        .with_label(p.span(edecl.file, edecl.name_range), "declared here");
        let has_case = cases.iter().any(|c| p.member(*c).name == "workerFailure");
        if cx.is_enum(e)
            && !has_case
            && !p.is_stdlib(e)
            && let Some(close) = edecl
                .node
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::RBrace)
                .last()
        {
            let mut fix = Fix::replace(
                "Add `case workerFailure(WorkerFailure)`",
                Span::empty(p.file(edecl.file).id, close.text_range().start()),
                "    case workerFailure(WorkerFailure)\n",
            );
            if let Some(edit) = cx.import_edit(edecl.file, "k.concurrency.WorkerFailure") {
                fix.edits.push(edit);
                fix.edits
                    .sort_by_key(|e| (e.span.file, e.span.range.start(), e.span.range.end()));
            }
            d = d.with_fix(fix);
        }
        cx.report(file, d);
        return;
    }
    let codable = cx.b.codable;
    if codable.is_some_and(|c| !cx.implements(error, c)) {
        not_codable(cx, file, range, error, "a worker method's error", false);
    }
}

/// K4039 at `range`.
fn not_codable(cx: &Cx<'_, '_>, file: FileIx, range: TextRange, ty: &Ty, what: &str, stream: bool) {
    let mut d = Diagnostic::new(
        codes::WORKER_PAYLOAD_NOT_CODABLE,
        cx.p.span(file, range),
        format!(
            "{what} crosses to the worker's thread through its codec, so it must be `Codable`; \
             `{}` isn't",
            ty.render(cx.p)
        ),
    );
    if stream {
        d = d.with_note(
            "a worker method returns its whole result once; streamed progress isn't supported yet",
        );
    }
    cx.report(file, d);
}
