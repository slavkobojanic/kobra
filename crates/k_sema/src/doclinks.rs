//! Intra-doc links in `///` comments (sema.md §7.5, design
//! `39-doc-comments.md`).
//!
//! A link is a code span in square brackets, ``[`Name`]``,
//! ``[`Type.member`]``, ``[`k.http.HttpStatus`]`` or ``[`find(id:)`]``.
//! It resolves, in order, against the members and parameters of the
//! documented declaration, the file's imports, every declaration of the
//! package (test-only modules included), the prelude and fully qualified
//! names. An unresolved link is K1901 (a warning) over the name inside the
//! backticks. Fenced code blocks and plain backticks are never checked.
//!
//! A resolved link records a resolution for each segment that names
//! something (``[`Shape.area`]``: `Shape` and `area`), so the reverse index,
//! go-to-definition and rename see doc links like code references
//! (sema.md §17).

use k_diagnostics::{Diagnostic, TextRange, TextSize, codes};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Cx, Found, param_list_labels};
use crate::lookup::Resolution;
use crate::program::{DeclId, FileIx, MemberId, MemberKind};

/// Checks every doc-comment link of `file`.
pub(crate) fn check_file(cx: &Cx<'_, '_>, file: FileIx) {
    let info = cx.p.file(file);
    let text = info.text;
    let mut offset = 0usize;
    let mut fenced = false;
    let mut block_end = 0usize;
    let mut links: Vec<(usize, usize, String)> = Vec::new();
    let flush = |links: &mut Vec<(usize, usize, String)>, end: usize| {
        if links.is_empty() {
            return;
        }
        let documented = documented_node(&info.root, end);
        for (start, stop, name) in links.drain(..) {
            match resolve(cx, file, documented.as_ref(), &name) {
                Some(segments) => {
                    for (offset, len, resolution) in segments {
                        let range = TextRange::at(size(start + offset), size(len));
                        cx.record_resolution(file, range, resolution);
                    }
                }
                None => {
                    let range = TextRange::new(size(start), size(stop));
                    cx.report(
                        file,
                        Diagnostic::new(
                            codes::UNRESOLVED_DOC_LINK,
                            cx.p.span(file, range),
                            format!("the doc link `{name}` doesn't resolve"),
                        ),
                    );
                }
            }
        }
    };
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(body) = trimmed.strip_prefix("///") {
            let body_start = offset + indent + 3;
            if body.trim_start().starts_with("```") {
                fenced = !fenced;
            } else if !fenced {
                find_links(body, body_start, &mut links);
            }
            block_end = offset + line.len();
        } else {
            fenced = false;
            flush(&mut links, block_end);
        }
        offset += line.len();
    }
    flush(&mut links, block_end);
}

/// Converts a byte offset to a [`TextSize`], saturating at `u32::MAX`.
fn size(n: usize) -> TextSize {
    TextSize::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Collects ``[`name`]`` links of one comment line.
fn find_links(body: &str, base: usize, out: &mut Vec<(usize, usize, String)>) {
    let mut rest = body;
    let mut at = base;
    while let Some(i) = rest.find("[`") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("`]") else { break };
        let name = &after[..j];
        let start = at + i + 2;
        if !name.is_empty() && !name.contains(' ') {
            out.push((start, start + j, name.to_owned()));
        }
        let consumed = i + 2 + j + 2;
        rest = &rest[consumed..];
        at += consumed;
    }
}

/// The declaration or member a doc comment ending at `end` documents.
fn documented_node(root: &SyntaxNode, end: usize) -> Option<SyntaxNode> {
    let len = usize::from(root.text_range().end());
    if end >= len {
        return None;
    }
    let mut token = root.token_at_offset(size(end)).right_biased()?;
    while token.kind().is_trivia() || usize::from(token.text_range().start()) < end {
        token = token.next_token()?;
    }
    token.parent_ancestors().find(|n| {
        matches!(
            n.kind(),
            SyntaxKind::FuncDecl
                | SyntaxKind::FieldDecl
                | SyntaxKind::EnumCase
                | SyntaxKind::InitDecl
                | SyntaxKind::RpcDecl
                | SyntaxKind::ComponentDecl
                | SyntaxKind::PageDecl
                | SyntaxKind::LayoutDecl
                | SyntaxKind::ServiceDecl
                | SyntaxKind::ProtocolDecl
                | SyntaxKind::EntityDecl
                | SyntaxKind::RepositoryDecl
                | SyntaxKind::StructDecl
                | SyntaxKind::EnumDecl
                | SyntaxKind::ClassDecl
                | SyntaxKind::InterfaceDecl
                | SyntaxKind::ThemeDecl
                | SyntaxKind::ConfigDecl
                | SyntaxKind::ExtendDecl
        )
    })
}

/// Splits `find(id:)` into `find` and `Some(["id"])`.
fn split_labels(name: &str) -> (&str, Option<Vec<String>>) {
    match name.find('(') {
        Some(i) if name.ends_with(')') => {
            let inner = &name[i + 1..name.len() - 1];
            let labels = inner
                .split(':')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            (&name[..i], Some(labels))
        }
        _ => (name, None),
    }
}

/// What one segment of a link resolves to: its byte offset in the link
/// text, its length and the resolution recorded for it.
type Segment = (usize, usize, Resolution);

/// Resolves a link's text: `Some` with what each segment names (a
/// qualifying prefix segment, a type and a member: ``[`k.http.HttpStatus`]``
/// records the last segment, ``[`Shape.area`]`` both `Shape` and `area`),
/// or `None` when the link doesn't resolve.
fn resolve(
    cx: &Cx<'_, '_>,
    file: FileIx,
    documented: Option<&SyntaxNode>,
    name: &str,
) -> Option<Vec<Segment>> {
    let (base, labels) = split_labels(name);
    let segments: Vec<&str> = base.split('.').collect();
    if segments.iter().any(|s| s.is_empty()) {
        return None;
    }
    // The byte offset of each segment in `name`.
    let mut offsets = Vec::with_capacity(segments.len());
    let mut at = 0usize;
    for s in &segments {
        offsets.push(at);
        at += s.len() + 1;
    }
    let last = segments.len() - 1;
    let seg = |i: usize, resolution: Resolution| (offsets[i], segments[i].len(), resolution);
    // Members and parameters of the documented declaration.
    if segments.len() == 1 {
        if let Some(node) = documented {
            if labels.is_none() {
                if let Some(list) = node.children().find(|c| c.kind() == SyntaxKind::ParamList) {
                    let ident = list
                        .descendants_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .find(|t| t.kind() == SyntaxKind::Ident && t.text() == segments[0]);
                    if let Some(ident) = ident {
                        let resolution = Resolution::Parameter {
                            name: segments[0].to_owned(),
                            definition: cx.p.span(file, ident.text_range()),
                        };
                        return Some(vec![seg(0, resolution)]);
                    }
                }
            }
            for decl in context_decls(cx, file, node) {
                if let Some(m) = member_matches(cx, decl, segments[0], labels.as_deref()) {
                    return Some(vec![seg(0, cx.member_resolution(m))]);
                }
            }
        }
    }
    // The first segment as a declaration, then members.
    let mut candidates: Vec<DeclId> = Vec::new();
    match cx.lookup(file, segments[0]) {
        Found::Decl(d) | Found::NotVisible(d) => candidates.push(d),
        _ => {}
    }
    for module in &cx.p.modules {
        if module.stdlib {
            continue;
        }
        for names in [&module.names, &module.test_names] {
            if let Some(list) = names.get(segments[0]) {
                candidates.extend(list.iter().copied());
            }
        }
    }
    // Fully qualified: the longest prefix naming a declaration.
    for n in (1..=segments.len()).rev() {
        if let Some(d) = cx.p.lookup_qualified(&segments[..n].join(".")) {
            if n == segments.len() {
                if labels.is_none() || function_matches(cx, d, labels.as_deref()) {
                    return Some(vec![seg(last, cx.decl_resolution(d))]);
                }
                return None;
            }
            if let Some(m) = rest_resolves(cx, d, &segments[n..], labels.as_deref()) {
                return Some(vec![
                    seg(n - 1, cx.decl_resolution(d)),
                    seg(last, cx.member_resolution(m)),
                ]);
            }
        }
    }
    candidates.iter().find_map(|&d| {
        if segments.len() == 1 {
            (labels.is_none() || function_matches(cx, d, labels.as_deref()))
                .then(|| vec![seg(0, cx.decl_resolution(d))])
        } else {
            rest_resolves(cx, d, &segments[1..], labels.as_deref()).map(|m| {
                vec![
                    seg(0, cx.decl_resolution(d)),
                    seg(last, cx.member_resolution(m)),
                ]
            })
        }
    })
}

/// The member `rest` (one segment) names in `decl`.
fn rest_resolves(
    cx: &Cx<'_, '_>,
    decl: DeclId,
    rest: &[&str],
    labels: Option<&[String]>,
) -> Option<MemberId> {
    match rest {
        [member] => member_matches(cx, decl, member, labels),
        _ => None,
    }
}

/// The declarations whose members a comment on `node` sees: the
/// documented type itself, or the type enclosing a documented member.
fn context_decls(cx: &Cx<'_, '_>, file: FileIx, node: &SyntaxNode) -> Vec<DeclId> {
    let info = cx.p.file(file);
    let mut out = Vec::new();
    for &d in &info.decls {
        let decl = cx.p.decl(d);
        if decl.node.text_range().contains_range(node.text_range()) {
            out.push(d);
            if decl.kind == crate::program::DeclKind::Extension {
                if let Some(t) = cx.ext_target(d).decl() {
                    out.push(t);
                }
            }
        }
    }
    out
}

/// The member of `decl` called `name` (with exactly `labels`, when given),
/// theme tokens included.
fn member_matches(
    cx: &Cx<'_, '_>,
    decl: DeclId,
    name: &str,
    labels: Option<&[String]>,
) -> Option<MemberId> {
    let d = cx.p.decl(decl);
    match labels {
        None => d.member_names.get(name).copied().or_else(|| {
            cx.tokens
                .get(&decl)
                .and_then(|t| t.iter().copied().find(|m| cx.p.member(*m).name == name))
        }),
        Some(labels) => d.members.iter().copied().find(|m| {
            let member = cx.p.member(*m);
            member.name == name
                && matches!(
                    member.kind,
                    MemberKind::Func | MemberKind::Rpc | MemberKind::Init
                )
                && member
                    .node
                    .as_ref()
                    .is_some_and(|n| labels_equal(&param_list_labels(n), labels))
        }),
    }
}

/// Whether top-level function `decl` has exactly `labels` (always, when no
/// labels are given).
fn function_matches(cx: &Cx<'_, '_>, decl: DeclId, labels: Option<&[String]>) -> bool {
    let Some(labels) = labels else { return true };
    let d = cx.p.decl(decl);
    labels_equal(&param_list_labels(&d.node), labels)
}

/// Returns `true` if a parameter list's labels (`None` for `_`) are exactly
/// the labels written in a doc link (`_` for an unlabelled parameter).
fn labels_equal(actual: &[Option<String>], wanted: &[String]) -> bool {
    actual.len() == wanted.len()
        && actual
            .iter()
            .zip(wanted)
            .all(|(a, w)| a.as_deref().unwrap_or("_") == w.as_str())
}
