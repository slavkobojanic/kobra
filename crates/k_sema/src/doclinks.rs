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

use k_diagnostics::{Diagnostic, TextRange, TextSize, codes};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Cx, Found, param_list_labels};
use crate::program::{DeclId, FileIx, MemberKind};

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
            if !resolves(cx, file, documented.as_ref(), &name) {
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

fn resolves(cx: &Cx<'_, '_>, file: FileIx, documented: Option<&SyntaxNode>, name: &str) -> bool {
    let (base, labels) = split_labels(name);
    let segments: Vec<&str> = base.split('.').collect();
    if segments.iter().any(|s| s.is_empty()) {
        return false;
    }
    // Members and parameters of the documented declaration.
    if segments.len() == 1 {
        if let Some(node) = documented {
            if labels.is_none() {
                if let Some(list) = node.children().find(|c| c.kind() == SyntaxKind::ParamList) {
                    let mut idents = list
                        .descendants_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token);
                    if idents.any(|t| t.kind() == SyntaxKind::Ident && t.text() == segments[0]) {
                        return true;
                    }
                }
            }
            for decl in context_decls(cx, file, node) {
                if member_matches(cx, decl, segments[0], labels.as_deref()) {
                    return true;
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
                return labels.is_none() || function_matches(cx, d, labels.as_deref());
            }
            if rest_resolves(cx, d, &segments[n..], labels.as_deref()) {
                return true;
            }
        }
    }
    candidates.iter().any(|&d| {
        if segments.len() == 1 {
            labels.is_none() || function_matches(cx, d, labels.as_deref())
        } else {
            rest_resolves(cx, d, &segments[1..], labels.as_deref())
        }
    })
}

fn rest_resolves(cx: &Cx<'_, '_>, decl: DeclId, rest: &[&str], labels: Option<&[String]>) -> bool {
    match rest {
        [member] => member_matches(cx, decl, member, labels),
        _ => false,
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

fn member_matches(cx: &Cx<'_, '_>, decl: DeclId, name: &str, labels: Option<&[String]>) -> bool {
    let d = cx.p.decl(decl);
    match labels {
        None => {
            d.member_names.contains_key(name)
                || cx
                    .tokens
                    .get(&decl)
                    .is_some_and(|t| t.iter().any(|m| cx.p.member(*m).name == name))
        }
        Some(labels) => d.members.iter().any(|m| {
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

fn function_matches(cx: &Cx<'_, '_>, decl: DeclId, labels: Option<&[String]>) -> bool {
    let Some(labels) = labels else { return true };
    let d = cx.p.decl(decl);
    labels_equal(&param_list_labels(&d.node), labels)
}

fn labels_equal(actual: &[Option<String>], wanted: &[String]) -> bool {
    actual.len() == wanted.len()
        && actual
            .iter()
            .zip(wanted)
            .all(|(a, w)| a.as_deref().unwrap_or("_") == w.as_str())
}
