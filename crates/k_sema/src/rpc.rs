//! Protocols and rpcs (sema.md §12.8).
//!
//! * Every rpc declares `throws` (K5501), and its error type is
//!   `RpcOnlyError` or an enum with exactly one `rpcFailure(RpcFailure)` case
//!   (K5502).
//! * Parameters, results (stream elements included) and error payloads are
//!   `Codable` (K5503).
//! * A protocol is implemented by a service (K5504) whose handlers repeat
//!   each rpc's signature, `throws` included (K5505); a missing handler is a
//!   missing member (K2010).

use k_diagnostics::{Diagnostic, Fix, Span, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Cx, SigKey};
use crate::program::{DeclId, FileIx, MemberId, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// The protocol checks over the whole program.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib {
            continue;
        }
        if decl.named_kind() == Some(K::Protocol) {
            for &m in &decl.members {
                if p.member(m).kind == MemberKind::Rpc {
                    rpc(cx, decl.file, m);
                }
            }
        }
        implementations(cx, d);
    }
}

/// The type node of a `throws` clause.
fn throws_type(node: &SyntaxNode) -> Option<ast::Type> {
    node.children()
        .find(|c| c.kind() == SyntaxKind::ThrowsClause)
        .and_then(|c| c.children().find_map(ast::Type::cast))
}

/// One rpc: `Codable` parameters and result (K5503), `throws` (K5501) and its error type (K5502, K5503).
fn rpc(cx: &Cx<'_, '_>, file: FileIx, m: MemberId) {
    let p = cx.p;
    let member = p.member(m);
    let Some(node) = &member.node else { return };
    let sig = cx.sig(SigKey::Member(m));
    let codable = cx.b.codable;
    let is_codable = |t: &Ty| t.contains_error() || codable.is_none_or(|c| cx.implements(t, c));
    // Parameters.
    if let Some(list) = node.children().find_map(ast::ParamList::cast) {
        for (param, sp) in list.params().zip(sig.params.iter()) {
            if !is_codable(&sp.ty) {
                let range = param
                    .syntax()
                    .children()
                    .find_map(ast::Type::cast)
                    .map_or(param.syntax().text_range(), |t| t.syntax().text_range());
                not_codable(cx, file, range, &sp.ty, "an rpc parameter");
            }
        }
    }
    // The result, or a stream's elements.
    if let Some(ret) = node
        .children()
        .find(|c| c.kind() == SyntaxKind::ReturnType)
        .and_then(|r| r.children().find_map(ast::Type::cast))
    {
        let element = match &sig.ret {
            Ty::Named(d, args) if Some(*d) == cx.b.stream => {
                args.first().cloned().unwrap_or(Ty::Error)
            }
            other => other.clone(),
        };
        if !is_codable(&element) {
            not_codable(
                cx,
                file,
                ret.syntax().text_range(),
                &element,
                "an rpc result",
            );
        }
    }
    // `throws`.
    let Some(error_node) = throws_type(node) else {
        // A misplaced `throws` is a syntax error the parser reported.
        if node
            .next_sibling()
            .is_some_and(|n| n.kind() == SyntaxKind::ErrorNode)
        {
            return;
        }
        let at = node
            .children()
            .find(|c| c.kind() == SyntaxKind::ParamList)
            .map_or(member.name_range.end(), |l| l.text_range().end());
        cx.report(
            file,
            Diagnostic::new(
                codes::RPC_WITHOUT_THROWS,
                p.span(file, member.name_range),
                format!(
                    "rpc `{}` must declare `throws`: every remote call can fail",
                    member.name
                ),
            )
            .with_fix(with_import(
                cx,
                file,
                "k.rpc.RpcOnlyError",
                Fix::replace(
                    "Add `throws RpcOnlyError`",
                    Span::empty(p.file(file).id, at),
                    " throws RpcOnlyError",
                ),
            )),
        );
        return;
    };
    let Some(error) = sig.throws.clone() else {
        return;
    };
    if error.contains_error() {
        return;
    }
    let range = error_node.syntax().text_range();
    let Some(e) = error.decl() else { return };
    if p.lookup_qualified("k.rpc.RpcOnlyError") == Some(e) {
        return;
    }
    let failure = p.lookup_qualified("k.rpc.RpcFailure");
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
    let named_ok = failures.len() == 1 && p.member(failures[0]).name == "rpcFailure";
    if !cx.is_enum(e) || !named_ok {
        let edecl = p.decl(e);
        let mut d = Diagnostic::new(
            codes::RPC_ERROR_WITHOUT_FAILURE_CASE,
            p.span(file, range),
            format!(
                "an rpc's error is `RpcOnlyError` or an enum with exactly one `rpcFailure(RpcFailure)` case; `{}` isn't",
                p.decl_name(e)
            ),
        )
        .with_label(p.span(edecl.file, edecl.name_range), "declared here");
        let has_case = cases.iter().any(|c| p.member(*c).name == "rpcFailure");
        if cx.is_enum(e) && !has_case && !p.is_stdlib(e) {
            if let Some(close) = edecl
                .node
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::RBrace)
                .last()
            {
                d = d.with_fix(with_import(
                    cx,
                    edecl.file,
                    "k.rpc.RpcFailure",
                    Fix::replace(
                        "Add `case rpcFailure(RpcFailure)`",
                        Span::empty(p.file(edecl.file).id, close.text_range().start()),
                        "    case rpcFailure(RpcFailure)\n",
                    ),
                ));
            }
        }
        cx.report(file, d);
        return;
    }
    // Error payloads cross the wire too.
    let payloads_ok = cases
        .iter()
        .all(|c| cx.case_sig(*c).params.iter().all(|x| is_codable(&x.ty)));
    if !payloads_ok {
        not_codable(cx, file, range, &error, "an rpc error's payload");
    }
}

/// Adds to `fix` the edit importing `path` into `file`, when the file
/// doesn't see it yet: a fix that writes `RpcOnlyError` must not leave it
/// unresolved (K1202). The edits are then sorted, so they stay in source
/// order also when the file's imports follow its declarations (K0111).
/// The import is an insertion at a line start among the imports, so it
/// never overlaps the fix's own edits.
fn with_import(cx: &Cx<'_, '_>, file: FileIx, path: &str, mut fix: Fix) -> Fix {
    if let Some(edit) = cx.import_edit(file, path) {
        fix.edits.push(edit);
        fix.edits
            .sort_by_key(|e| (e.span.file, e.span.range.start(), e.span.range.end()));
    }
    fix
}

/// K5503 at `range`.
fn not_codable(cx: &Cx<'_, '_>, file: FileIx, range: TextRange, ty: &Ty, what: &str) {
    cx.error(
        file,
        codes::RPC_PAYLOAD_NOT_CODABLE,
        range,
        format!(
            "{what} crosses the wire, so it must be `Codable`; `{}` isn't",
            ty.render(cx.p)
        ),
    );
}

/// `implements P` for a protocol `P`: only services (K5504), with every rpc
/// handled (K2010) by a handler of the same signature (K5505).
fn implementations(cx: &Cx<'_, '_>, d: DeclId) {
    let p = cx.p;
    let decl = p.decl(d);
    for (ty, range) in cx.implements_list(d).iter() {
        let Some(proto) = ty.decl() else { continue };
        if p.decl(proto).named_kind() != Some(K::Protocol) {
            continue;
        }
        if decl.named_kind() != Some(K::Service) {
            cx.error(
                decl.file,
                codes::RPC_IMPLEMENTATION_NOT_SERVER,
                *range,
                "a protocol is implemented by a service, which runs on the server",
            );
            continue;
        }
        let mut missing = Vec::new();
        for &r in &p.decl(proto).members {
            let req = p.member(r);
            if req.kind != MemberKind::Rpc {
                continue;
            }
            let Some(&found) = decl.member_names.get(&req.name) else {
                missing.push(format!("`{}`", req.name));
                continue;
            };
            let f = p.member(found);
            if !matches!(f.kind, MemberKind::Rpc | MemberKind::Func) {
                missing.push(format!("`{}`", req.name));
                continue;
            }
            let a = cx.sig(SigKey::Member(r));
            let b = cx.sig(SigKey::Member(found));
            let same = |x: &Ty, y: &Ty| x == y || x.contains_error() || y.contains_error();
            let throws_same = match (&a.throws, &b.throws) {
                (Some(x), Some(y)) => same(x, y),
                (None, None) => true,
                _ => false,
            };
            let matches = a.params.len() == b.params.len()
                && a.params
                    .iter()
                    .zip(b.params.iter())
                    .all(|(x, y)| x.label == y.label && same(&x.ty, &y.ty))
                && same(&a.ret, &b.ret)
                && throws_same;
            if !matches {
                let pfile = p.decl(proto).file;
                cx.report(
                    decl.file,
                    Diagnostic::new(
                        codes::RPC_HANDLER_MISMATCH,
                        p.span(decl.file, f.name_range),
                        format!(
                            "`{}` doesn't repeat the signature of `{}.{}`",
                            f.name,
                            p.decl_name(proto),
                            req.name
                        ),
                    )
                    .with_label(p.span(pfile, req.name_range), "declared here"),
                );
            }
        }
        if !missing.is_empty() {
            cx.error(
                decl.file,
                codes::MISSING_INTERFACE_MEMBER,
                *range,
                format!("`{}` is missing {}", decl.name, missing.join(", ")),
            );
        }
    }
}
