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
//! * Only a protocol's rpcs are served: an `rpc` member of anything else
//!   that no protocol in its `implements` clause declares is K5508, with a
//!   fix that makes it a `func`.
//! * Guards are declared on the protocol and its rpcs, which is what the
//!   server checks: a `requires` clause on any other rpc, a service's
//!   handler above all, is K5507, with a fix that moves it to the protocol
//!   and removes the imports of the handler's file that only the clause
//!   used.

use std::collections::HashSet;

use k_diagnostics::{Applicability, Diagnostic, Fix, Span, TextEdit, TextRange, TextSize, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Cx, Found, SigKey};
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
        if decl.named_kind() != Some(K::Protocol) && !from_library(cx, d) {
            for &m in &decl.members {
                if p.member(m).kind == MemberKind::Rpc {
                    unserved_rpc(cx, d, m);
                    handler_guards(cx, d, m);
                }
            }
        }
    }
}

/// `true` when declaration `d` comes from the stdlib or from a library the
/// project depends on, whose source the project doesn't edit.
fn from_library(cx: &Cx<'_, '_>, d: DeclId) -> bool {
    cx.p.is_stdlib(d) || crate::di::from_dependency(cx, d)
}

/// The qualified name of declaration `d` (`k.auth.RateLimit`), also for a
/// prelude type, which `decl_name` renders by its simple name.
fn qualified(cx: &Cx<'_, '_>, d: DeclId) -> String {
    let name = &cx.p.decl(d).name;
    match cx.p.decl_module(d) {
        Some(m) => format!("{}.{name}", cx.p.modules[m.0 as usize].path),
        None => name.clone(),
    }
}

/// The `requires` clause that is a direct child of `node`.
fn requires_clause(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children()
        .find(|c| c.kind() == SyntaxKind::RequiresClause)
}

/// The guards of a `requires` clause: each as its source text with runs of
/// whitespace collapsed (`RateLimit(10.perMinute)`), with its name.
fn clause_guards(clause: &SyntaxNode) -> Vec<(String, String)> {
    clause
        .children()
        .filter(|g| g.kind() == SyntaxKind::Guard)
        .map(|g| {
            let text = g.text().to_string();
            let name = g
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::Ident)
                .map(|t| t.text().to_owned())
                .unwrap_or_default();
            (text.split_whitespace().collect::<Vec<_>>().join(" "), name)
        })
        .collect()
}

/// The end of the last non-trivia token of `node`: where a clause is
/// appended to a declaration without a body.
fn signature_end(node: &SyntaxNode) -> TextSize {
    node.descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .last()
        .map_or(node.text_range().end(), |t| t.text_range().end())
}

/// Joins names for a message: `` `a` ``, `` `a` and `b` ``,
/// `` `a`, `b` and `c` ``.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The protocols in `d`'s `implements` clause that declare an rpc named
/// like member `m`, with that rpc, each protocol once.
fn served_by(cx: &Cx<'_, '_>, d: DeclId, m: MemberId) -> Vec<(DeclId, MemberId)> {
    let p = cx.p;
    let name = &p.member(m).name;
    let mut served: Vec<(DeclId, MemberId)> = Vec::new();
    for (ty, _) in cx.implements_list(d).iter() {
        let Some(proto) = ty.decl() else { continue };
        let pdecl = p.decl(proto);
        if pdecl.named_kind() != Some(K::Protocol) {
            continue;
        }
        if let Some(&r) = pdecl.member_names.get(name) {
            if p.member(r).kind == MemberKind::Rpc && !served.iter().any(|s| s.0 == proto) {
                served.push((proto, r));
            }
        }
    }
    served
}

/// K5508: rpc member `m` of `d`, which isn't a protocol, where no protocol
/// in `d`'s `implements` clause declares it.
///
/// Only a protocol's rpcs are served (11-protocols-and-rpc.md), so such an
/// rpc is never reachable over the wire: in a struct, a component or an
/// extension it can't be, and in a service it needs a protocol the service
/// implements. The diagnostic is at the `rpc` keyword. The fix makes the
/// member a `func` (`maybe-incorrect`: the author may have meant to declare
/// it in a protocol), offered only when that leaves valid code: not with a
/// `requires` clause, which a `func` can't have (K5507 reports the clause),
/// and not without a body outside an interface. A declaration that names a
/// protocol declaring the rpc but can't implement one (a struct) is K5504
/// instead, so it is not reported here.
fn unserved_rpc(cx: &Cx<'_, '_>, d: DeclId, m: MemberId) {
    let p = cx.p;
    let decl = p.decl(d);
    let member = p.member(m);
    let Some(node) = &member.node else { return };
    if !served_by(cx, d, m).is_empty() {
        return;
    }
    let Some(keyword) = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::RpcKw)
    else {
        return;
    };
    let file = decl.file;
    let range = keyword.text_range();
    let clause = requires_clause(node).is_some();
    let service = decl.named_kind() == Some(K::Service);
    let help = match (service, clause) {
        (true, false) => format!(
            "only a protocol's rpcs are served: declare `{}` in a protocol that `{}` implements, or make it a `func`",
            member.name, decl.name
        ),
        (true, true) => format!(
            "only a protocol's rpcs are served: declare `{}` in a protocol that `{}` implements, or make it a `func` without the `requires` clause",
            member.name, decl.name
        ),
        (false, false) => format!(
            "only a protocol's rpcs are served, by a service that implements the protocol: make `{}` a `func`",
            member.name
        ),
        (false, true) => format!(
            "only a protocol's rpcs are served, by a service that implements the protocol: make `{}` a `func` without the `requires` clause",
            member.name
        ),
    };
    let message = if service {
        format!(
            "rpc `{}` is never served: no protocol that `{}` implements declares it",
            member.name, decl.name
        )
    } else {
        format!(
            "rpc `{}` is never served: `{}` isn't a service, and only services serve rpcs",
            member.name, decl.name
        )
    };
    let mut diag =
        Diagnostic::new(codes::RPC_OUTSIDE_PROTOCOL, p.span(file, range), message).with_help(help);
    let has_body = node.children().any(|c| c.kind() == SyntaxKind::Block);
    if !clause && (has_body || decl.named_kind() == Some(K::Interface)) {
        diag = diag.with_fix(Fix {
            message: format!("Make `{}` a `func`", member.name),
            edits: vec![TextEdit {
                span: p.span(file, range),
                replacement: "func".to_owned(),
            }],
            applicability: Applicability::MaybeIncorrect,
        });
    }
    cx.report(file, diag);
}

/// K5507: a `requires` clause on rpc `m` of `d`, which isn't a protocol.
///
/// The server checks the guards of the protocol and of its rpc before any
/// handler runs (11-protocols-and-rpc.md), so a clause on the handler is
/// never checked. The fix moves the guards the protocol doesn't declare yet
/// to every protocol rpc the handler serves, importing them where needed,
/// and deletes the handler's clause; when the protocol declares them all,
/// it only deletes the clause. Either way it also deletes the imports of
/// the handler's file that only the clause used ([`Cx::imports_only_in`]),
/// so applying it leaves no unused import (K1105), keeping those of guards
/// it moves to a protocol in the same file. A protocol from a library or the stdlib
/// can't be edited from the app, and an rpc no protocol declares has
/// nowhere to move to: then a help explains.
fn handler_guards(cx: &Cx<'_, '_>, d: DeclId, m: MemberId) {
    let p = cx.p;
    let decl = p.decl(d);
    let member = p.member(m);
    let Some(node) = &member.node else { return };
    let Some(clause) = requires_clause(node) else {
        return;
    };
    let file = decl.file;
    let range = clause.text_range();
    let mut diag = Diagnostic::new(
        codes::REQUIRES_ON_HANDLER,
        p.span(file, range),
        "the server never checks a `requires` clause on a handler; it runs the guards the protocol declares",
    );
    // The protocol rpcs this handler serves.
    let served = if decl.named_kind() == Some(K::Service) {
        served_by(cx, d, m)
    } else {
        Vec::new()
    };
    if served.is_empty() {
        cx.report(
            file,
            diag.with_help(format!(
                "only a protocol's rpcs are served: declare `{}` in a protocol that `{}` implements, with the clause on its rpc",
                member.name, decl.name
            )),
        );
        return;
    }
    if let Some(&(proto, r)) = served.iter().find(|(proto, _)| from_library(cx, *proto)) {
        let origin = if p.is_stdlib(proto) {
            "the standard library".to_owned()
        } else {
            let library = p
                .file(p.decl(proto).file)
                .path
                .and_then(|path| path.dependency())
                .map(|(dir, _)| dir)
                .unwrap_or_default();
            format!("the library `{library}`")
        };
        cx.report(
            file,
            diag.with_help(format!(
                "`{}` comes from {origin}, which declares its guards: add the guard to its rpc `{}` there, or check the caller in the handler",
                qualified(cx, proto),
                p.member(r).name
            )),
        );
        return;
    }
    let guards = clause_guards(&clause);
    let mut edits = Vec::new();
    let mut moved: Vec<String> = Vec::new();
    let mut targets: Vec<String> = Vec::new();
    // The guards moved to a protocol of the handler's own file, whose
    // imports that file still needs.
    let mut moved_here: Vec<String> = Vec::new();
    for &(proto, r) in &served {
        let pdecl = p.decl(proto);
        let rpc = p.member(r);
        diag = diag.with_label(
            p.span(pdecl.file, rpc.name_range),
            "the server runs the guards declared here",
        );
        let target = format!("`{}.{}`", pdecl.name, rpc.name);
        targets.push(target);
        let Some(rnode) = &rpc.node else { continue };
        let rclause = requires_clause(rnode);
        let declared: Vec<String> = rclause
            .iter()
            .chain(requires_clause(&pdecl.node).iter())
            .flat_map(clause_guards)
            .map(|(text, _)| text)
            .collect();
        let missing: Vec<&(String, String)> = guards
            .iter()
            .filter(|(text, _)| !declared.contains(text))
            .collect();
        if missing.is_empty() {
            continue;
        }
        let list = missing
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let pid = p.file(pdecl.file).id;
        edits.push(match &rclause {
            Some(c) => TextEdit {
                span: Span::new(pid, TextRange::empty(c.text_range().end())),
                replacement: format!(", {list}"),
            },
            None => TextEdit {
                span: Span::new(pid, TextRange::empty(signature_end(rnode))),
                replacement: format!(" requires {list}"),
            },
        });
        for (text, name) in missing {
            if !moved.contains(text) {
                moved.push(text.clone());
            }
            if pdecl.file == file && !moved_here.contains(text) {
                moved_here.push(text.clone());
            }
            if let Found::Decl(g) = cx.lookup(file, name) {
                if let Some(edit) = cx.import_edit(pdecl.file, &qualified(cx, g)) {
                    if !edits.contains(&edit) {
                        edits.push(edit);
                    }
                }
            }
        }
    }
    // The imports only the clause used, except those of the guards moved
    // within the file.
    let kept: HashSet<String> = clause
        .children()
        .filter(|g| g.kind() == SyntaxKind::Guard)
        .filter(|g| {
            let text = g.text().to_string();
            moved_here.contains(&text.split_whitespace().collect::<Vec<_>>().join(" "))
        })
        .flat_map(|g| {
            g.descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::Ident)
                .map(|t| t.text().to_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    edits.extend(cx.imports_only_in(file, range, &kept));
    // The clause and the whitespace before it.
    let text = p.file(file).text;
    let start = usize::from(range.start()).min(text.len());
    let from = text[..start].trim_end().len();
    let from = TextSize::try_from(from).unwrap_or(range.start());
    edits.push(TextEdit {
        span: p.span(file, TextRange::new(from, range.end())),
        replacement: String::new(),
    });
    edits.sort_by_key(|e| (e.span.file, e.span.range.start(), e.span.range.end()));
    let fix = if moved.is_empty() {
        let verb = if targets.len() == 1 {
            "requires"
        } else {
            "require"
        };
        Fix {
            message: format!(
                "Remove the clause: {} already {verb} it",
                and_list(&targets)
            ),
            edits,
            applicability: Applicability::MachineApplicable,
        }
    } else {
        let moved: Vec<String> = moved.iter().map(|g| format!("`{g}`")).collect();
        Fix {
            message: format!("Move {} to {}", moved.join(", "), and_list(&targets)),
            edits,
            applicability: Applicability::MaybeIncorrect,
        }
    };
    cx.report(file, diag.with_fix(fix));
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
