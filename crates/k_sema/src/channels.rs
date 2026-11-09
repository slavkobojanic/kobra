//! Channels: typed realtime between a tab and the server
//! (`docs/design/44-channels.md`; sema.md §12.8).
//!
//! A channel is an rpc whose result is `Channel<C, S, E>`. The rpc rules
//! of [`crate::rpc`] check its messages and refusal (K5502, K5503); this
//! module checks where a `Channel` may appear and who uses which side:
//!
//! * **K5509**: a `Channel` result anywhere but a protocol's rpc and the
//!   handler that serves it; an optional `Channel?` result of an rpc too.
//! * **K5510**: a `Channel` as a type argument (`List<Channel<…>>`,
//!   `Stream<Channel<…>>`); `Channel<…>?` is allowed, since a client
//!   service holds one in an optional field.
//! * **K5511** (from the body checker): server-only code opens a channel
//!   (calls a channel rpc) or uses the client's members of one
//!   (`state`, `messages`, `send`, `close`).
//! * **K5512** (from the body checker): `Channel(sending:receiving:)`
//!   outside the handler of a channel rpc.
//! * **K5513** ([`opened_during_ssr`], a warning): a `@Load` that opens
//!   a channel, which would panic while the server renders: a call of a
//!   channel rpc in its initializer, or of an app function that calls one,
//!   directly or through other app functions.

use std::collections::HashMap;

use k_diagnostics::{Diagnostic, FileId, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxToken};

use crate::cx::{Cx, Found, SigKey};
use crate::lookup::Resolution;
use crate::program::{DeclId, MemberId, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// The members of a `Channel` that only the browser uses.
const CLIENT_MEMBERS: [&str; 4] = ["state", "messages", "send", "close"];

/// `k.rpc.Channel`.
pub(crate) fn channel_decl(cx: &Cx<'_, '_>) -> Option<DeclId> {
    cx.p.lookup_qualified("k.rpc.Channel")
}

/// The `[C, S, E]` of a `Channel<C, S, E>`.
pub(crate) fn channel_args<'t>(cx: &Cx<'_, '_>, ty: &'t Ty) -> Option<&'t [Ty]> {
    match ty {
        Ty::Named(d, args) if Some(*d) == channel_decl(cx) && args.len() == 3 => Some(args),
        _ => None,
    }
}

/// `true` for `Channel<…>` and `Channel<…>?`.
fn is_channel_or_optional(cx: &Cx<'_, '_>, ty: &Ty) -> bool {
    match ty {
        Ty::Optional(inner) => channel_args(cx, inner).is_some(),
        other => channel_args(cx, other).is_some(),
    }
}

/// `true` for an rpc member whose result is a `Channel`.
pub(crate) fn is_channel_rpc(cx: &Cx<'_, '_>, m: MemberId) -> bool {
    cx.p.member(m).kind == MemberKind::Rpc
        && channel_args(cx, &cx.sig(SigKey::Member(m)).ret).is_some()
}

/// `true` when `member` is one of the client's members of a `Channel`.
pub(crate) fn is_client_member(cx: &Cx<'_, '_>, member: MemberId) -> bool {
    let m = cx.p.member(member);
    Some(m.owner) == channel_decl(cx) && CLIENT_MEMBERS.contains(&m.name.as_str())
}

/// The declaration-level checks over the whole program (K5509, K5510).
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let Some(channel) = channel_decl(cx) else {
        return;
    };
    for (i, decl) in p.decls.iter().enumerate() {
        if p.file(decl.file).stdlib {
            continue;
        }
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        for &m in &decl.members {
            result(cx, d, m);
        }
    }
    for (i, file) in p.files.iter().enumerate() {
        if file.stdlib {
            continue;
        }
        let ix = crate::program::FileIx(u32::try_from(i).unwrap_or(u32::MAX));
        for path in file.root.descendants().filter_map(ast::PathType::cast) {
            let Some(list) = path.generic_arg_list() else {
                continue;
            };
            for arg in list.types() {
                if names_channel(cx, ix, &arg, channel) {
                    cx.report(
                        ix,
                        Diagnostic::new(
                            codes::NESTED_CHANNEL,
                            p.span(ix, arg.syntax().text_range()),
                            "a `Channel` can't be inside another type",
                        )
                        .with_help(
                            "open one channel per subject and hold it in a field or a local of type `Channel<C, S, E>` or `Channel<C, S, E>?`",
                        ),
                    );
                }
            }
        }
    }
}

/// `true` when type syntax `ty` (through `?` and parentheses) names
/// `k.rpc.Channel` in file `ix`.
fn names_channel(
    cx: &Cx<'_, '_>,
    ix: crate::program::FileIx,
    ty: &ast::Type,
    channel: DeclId,
) -> bool {
    match ty {
        ast::Type::PathType(path) => {
            let segments: Vec<SyntaxToken> = path.segments();
            match segments.as_slice() {
                [] => false,
                [one] => matches!(cx.lookup(ix, one.text()), Found::Decl(d) if d == channel),
                many => {
                    let text: Vec<&str> = many.iter().map(SyntaxToken::text).collect();
                    cx.p.lookup_qualified(&text.join(".")) == Some(channel)
                }
            }
        }
        ast::Type::OptionalType(o) => o
            .inner()
            .is_some_and(|inner| names_channel(cx, ix, &inner, channel)),
        ast::Type::ParenType(p) => p
            .syntax()
            .children()
            .find_map(ast::Type::cast)
            .is_some_and(|inner| names_channel(cx, ix, &inner, channel)),
        ast::Type::TupleType(_) | ast::Type::FuncType(_) => false,
    }
}

/// K5509: member `m` of `d` returns a `Channel` but isn't a protocol's
/// rpc or a handler serving one; or an rpc returns `Channel<…>?`.
fn result(cx: &Cx<'_, '_>, d: DeclId, m: MemberId) {
    let p = cx.p;
    let member = p.member(m);
    if !matches!(member.kind, MemberKind::Func | MemberKind::Rpc) {
        return;
    }
    let Some(node) = &member.node else { return };
    let Some(ret) = node
        .children()
        .find(|c| c.kind() == SyntaxKind::ReturnType)
        .and_then(|r| r.children().find_map(ast::Type::cast))
    else {
        return;
    };
    let sig = cx.sig(SigKey::Member(m));
    if !is_channel_or_optional(cx, &sig.ret) {
        return;
    }
    let decl = p.decl(d);
    let file = decl.file;
    let range = ret.syntax().text_range();
    let rpc_of_protocol = member.kind == MemberKind::Rpc
        && (decl.named_kind() == Some(K::Protocol) || !crate::rpc::served_by(cx, d, m).is_empty());
    if !rpc_of_protocol {
        cx.report(
            file,
            Diagnostic::new(
                codes::CHANNEL_RESULT_MISPLACED,
                p.span(file, range),
                format!(
                    "`{}` returns a `Channel`, but only a protocol's rpc and the handler that serves it do",
                    member.name
                ),
            )
            .with_help(
                "declare the channel as an rpc of a protocol; a client service opens it by calling that rpc and keeps it in a field",
            ),
        );
    } else if matches!(sig.ret, Ty::Optional(_)) {
        cx.report(
            file,
            Diagnostic::new(
                codes::CHANNEL_RESULT_MISPLACED,
                p.span(file, range),
                format!(
                    "rpc `{}` returns an optional `Channel`; a channel rpc returns `Channel<C, S, E>` and throws when it can't open",
                    member.name
                ),
            ),
        );
    }
}

/// What calling a function opens, for K5513: the channel rpc it calls,
/// directly or through other app functions, if any.
type Opens = Option<MemberId>;

/// The K5513 pass: every `@Load` initializer of the app (outside a
/// client-rendered app or component, whose loads run in the browser) that
/// opens a channel, at the call that does.
pub(crate) fn opened_during_ssr(cx: &Cx<'_, '_>) {
    let p = cx.p;
    if channel_decl(cx).is_none() || crate::di::client_rendered(cx) {
        return;
    }
    // The members a call can name: channel rpcs and the app's functions.
    let mut members: HashMap<Resolution, MemberId> = HashMap::new();
    for decl in &p.decls {
        let stdlib = p.file(decl.file).stdlib;
        for &m in &decl.members {
            let member = p.member(m);
            let wanted = match member.kind {
                MemberKind::Rpc => is_channel_rpc(cx, m),
                MemberKind::Func => !stdlib,
                _ => false,
            };
            if wanted {
                members.insert(cx.member_resolution(m), m);
            }
        }
    }
    // Copied out, so nothing below holds the borrow while a signature is
    // resolved (which records resolutions).
    let res: HashMap<(FileId, TextRange), Resolution> = {
        let live = cx.resolutions.borrow();
        let chunks = cx.body_resolutions.borrow();
        let mut res = HashMap::new();
        for (k, r) in live.iter().chain(chunks.iter().flat_map(|c| c.iter())) {
            if matches!(r, Resolution::Member { .. }) {
                res.entry(*k).or_insert_with(|| r.clone());
            }
        }
        res
    };
    let browser_only = crate::di::browser_only_components(cx);
    let mut memo: HashMap<MemberId, Opens> = HashMap::new();
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib || browser_only.contains(&d) {
            continue;
        }
        let id = p.file(decl.file).id;
        for &m in &decl.members {
            let member = p.member(m);
            if member.kind != MemberKind::Field || !member.has_annotation("Load") {
                continue;
            }
            let Some(init) = member
                .node
                .as_ref()
                .and_then(|n| ast::FieldDecl::cast(n.clone()))
                .and_then(|f| f.initializer())
            else {
                continue;
            };
            for (callee, range) in calls(init.syntax(), id, &res, &members) {
                let Some(rpc) = opens(cx, callee, &res, &members, &mut memo) else {
                    continue;
                };
                let name = &p.member(callee).name;
                let message = if callee == rpc {
                    format!(
                        "`{name}` opens a channel, and channels open only in the browser, but a `@Load` runs while the server renders"
                    )
                } else {
                    format!(
                        "`{name}` opens a channel (it calls `{}`), and channels open only in the browser, but a `@Load` runs while the server renders",
                        p.member(rpc).name
                    )
                };
                cx.report(
                    decl.file,
                    Diagnostic::new(
                        codes::CHANNEL_OPENED_DURING_SSR,
                        p.span(decl.file, range),
                        message,
                    )
                    .with_help(
                        "open the channel from an `@Effect`, which runs only in the browser",
                    ),
                );
            }
        }
    }
}

/// The calls in `node` that name one of `members`, with the call's range.
fn calls(
    node: &k_syntax::SyntaxNode,
    id: FileId,
    res: &HashMap<(FileId, TextRange), Resolution>,
    members: &HashMap<Resolution, MemberId>,
) -> Vec<(MemberId, TextRange)> {
    let mut out = Vec::new();
    for call in node.descendants().filter_map(ast::CallExpr::cast) {
        let name = match call.callee() {
            Some(ast::Expr::NameExpr(n)) => n.syntax().first_token(),
            Some(ast::Expr::MemberExpr(m)) => m.name(),
            _ => None,
        };
        let Some(name) = name else { continue };
        if let Some(&m) = res
            .get(&(id, name.text_range()))
            .and_then(|r| members.get(r))
        {
            out.push((m, call.syntax().text_range()));
        }
    }
    out
}

/// The channel rpc that calling `m` opens, if any: `m` itself, or one its
/// body calls, directly or through other app functions. Memoized; a
/// function being computed counts as opening none, which ends recursion.
fn opens(
    cx: &Cx<'_, '_>,
    m: MemberId,
    res: &HashMap<(FileId, TextRange), Resolution>,
    members: &HashMap<Resolution, MemberId>,
    memo: &mut HashMap<MemberId, Opens>,
) -> Opens {
    if is_channel_rpc(cx, m) {
        return Some(m);
    }
    if let Some(known) = memo.get(&m) {
        return *known;
    }
    memo.insert(m, None);
    let p = cx.p;
    let member = p.member(m);
    let Some(node) = &member.node else {
        return None;
    };
    let id = p.file(p.decl(member.owner).file).id;
    let mut found = None;
    for (callee, _) in calls(node, id, res, members) {
        if let Some(rpc) = opens(cx, callee, res, members, memo) {
            found = Some(rpc);
            break;
        }
    }
    memo.insert(m, found);
    found
}
