//! The tooling queries that read the program view and the syntax:
//! definitions of resolutions, implementors of interfaces and page URLs
//! (`docs/compiler/sema.md` §17). [`Analysis`] exposes
//! them; the reverse index is in [`crate::refs`].

use std::collections::VecDeque;

use k_diagnostics::{FileId, Span, TextRange};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::host::Analysis;
use crate::lookup::Resolution;
use crate::stdlib::DeclarationKind;
use crate::view::{DeclId, MemberKind, ProgramView, Resolved, Type};

/// Where `target` is declared ([`Analysis::definition`]).
pub(crate) fn definition(
    analysis: &Analysis,
    view: &ProgramView,
    target: &Resolution,
) -> Option<Span> {
    match target {
        Resolution::Declaration { .. } | Resolution::Member { .. } => {
            match view.resolve(target)? {
                Resolved::Decl(d) => {
                    let info = view.decl(d);
                    Some(Span::new(info.file, info.name_range))
                }
                Resolved::Member(m) => member_definition(analysis, view, view.member(m)),
            }
        }
        Resolution::Local { definition, .. } | Resolution::Parameter { definition, .. } => {
            Some(*definition)
        }
        Resolution::Token { category, name } => token_definition(view, category, name),
        Resolution::Class { component, name } => {
            style_definition(analysis, view, component, name, SyntaxKind::ClassSelector)
        }
        Resolution::Keyframes { component, name } => {
            style_definition(analysis, view, component, name, SyntaxKind::KeyframesRule)
        }
        Resolution::Event { owner, name } => {
            let d = view.lookup_display(owner)?;
            let info = view.decl(d);
            info.members
                .iter()
                .map(|&m| view.member(m))
                .find(|m| m.name == *name)
                .map(|m| Span::new(info.file, m.name_range))
        }
        Resolution::Element { .. }
        | Resolution::Attribute { .. }
        | Resolution::CssProperty { .. } => None,
    }
}

/// Where a member is declared: its name token in its owner's file. A
/// generated member's name range is that of the declaration generating
/// it, which can be in another file: a page case (`Page.signIn`) points at
/// its page (`SignIn`). Such a member is defined by the declaration with
/// that name range and a name it generates; one whose source is not found
/// (`copy(…)`) has no definition.
fn member_definition(
    analysis: &Analysis,
    view: &ProgramView,
    member: &crate::view::MemberInfo,
) -> Option<Span> {
    let file = view.decl(member.owner).file;
    let spelled = analysis.text(file).and_then(|text| {
        text.get(usize::from(member.name_range.start())..usize::from(member.name_range.end()))
    });
    if spelled == Some(member.name.as_str()) {
        return Some(Span::new(file, member.name_range));
    }
    view.decls
        .iter()
        .find(|d| {
            !d.stdlib
                && d.name_range == member.name_range
                && (d.name == member.name || lower_first(&d.name) == member.name)
        })
        .map(|d| Span::new(d.file, d.name_range))
}

/// `SignIn` → `signIn`: the case name a page generates in `Page`.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The theme token `name` of `category` (`Spacing`, matched against the
/// token's category word `spacing`): a project theme's first, else the
/// stdlib's.
fn token_definition(view: &ProgramView, category: &str, name: &str) -> Option<Span> {
    let mut fallback = None;
    for member in &view.members {
        if member.kind != MemberKind::Token || member.name != name {
            continue;
        }
        let matches = match member.ty.as_ref().and_then(Type::decl) {
            Some(d) => view.decl(d).display_name == category,
            None => member
                .category
                .as_deref()
                .is_none_or(|c| c.eq_ignore_ascii_case(category)),
        };
        if !matches {
            continue;
        }
        let owner = view.decl(member.owner);
        let span = Span::new(owner.file, member.name_range);
        if !owner.stdlib {
            return Some(span);
        }
        fallback.get_or_insert(span);
    }
    fallback
}

/// The name token of the first class selector (`kind` is
/// [`SyntaxKind::ClassSelector`]) or keyframes rule
/// ([`SyntaxKind::KeyframesRule`]) called `name` in the style block of
/// `component`.
fn style_definition(
    analysis: &Analysis,
    view: &ProgramView,
    component: &str,
    name: &str,
    kind: SyntaxKind,
) -> Option<Span> {
    let d = view.lookup_display(component)?;
    let info = view.decl(d);
    let node = decl_node(analysis, info.file, info.range)?;
    let style = node
        .descendants()
        .find(|n| n.kind() == SyntaxKind::StyleBlock)?;
    style
        .descendants()
        .filter(|n| n.kind() == kind)
        .flat_map(|n| {
            n.children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::Ident)
                .take(1)
                .collect::<Vec<_>>()
        })
        .find(|t| t.text() == name)
        .map(|t| Span::new(info.file, t.text_range()))
}

/// The syntax node of the item at `range` in project file `file`.
fn decl_node(analysis: &Analysis, file: FileId, range: TextRange) -> Option<SyntaxNode> {
    let parse = analysis.parse(file).ok()??;
    let root = parse.syntax();
    if !root.text_range().contains_range(range) {
        return None;
    }
    let mut node = match root.covering_element(range) {
        rowan::NodeOrToken::Node(n) => n,
        rowan::NodeOrToken::Token(t) => t.parent()?,
    };
    while node.text_range() != range {
        node = node.parent()?;
    }
    Some(node)
}

/// Every declaration implementing `decl`, transitively
/// ([`Analysis::implementations`]). One pass over the declarations builds
/// the direct-implementor lists; a breadth-first walk follows them.
pub(crate) fn implementations(view: &ProgramView, decl: DeclId) -> Vec<DeclId> {
    let mut direct: Vec<Vec<DeclId>> = vec![Vec::new(); view.decls.len()];
    for (i, info) in view.decls.iter().enumerate() {
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        for t in &info.implements {
            if let Some(Type::Named(iface, _)) = Some(t.unwrap_optional()) {
                if let Some(list) = direct.get_mut(iface.0 as usize) {
                    list.push(id);
                }
            }
        }
    }
    let mut seen = vec![false; view.decls.len()];
    let mut out = Vec::new();
    let mut queue = VecDeque::from([decl]);
    while let Some(next) = queue.pop_front() {
        for &d in direct.get(next.0 as usize).into_iter().flatten() {
            if let Some(flag) = seen.get_mut(d.0 as usize) {
                if !*flag && d != decl {
                    *flag = true;
                    out.push(d);
                    queue.push_back(d);
                }
            }
        }
    }
    out.sort_unstable();
    out
}

/// The URL template of page `decl` ([`Analysis::page_url`]), by the rules
/// `crate::routing` checks.
pub(crate) fn page_url(analysis: &Analysis, view: &ProgramView, decl: DeclId) -> Option<String> {
    let info = view.decls.get(decl.0 as usize)?;
    if info.named_kind() != Some(DeclarationKind::Page) {
        return None;
    }
    if let Some(node) = decl_node(analysis, info.file, info.range) {
        if let Some(template) = template(&node) {
            return Some(template);
        }
    }
    let mut url = format!("/{}", crate::routing::kebab(&info.name));
    for &m in &info.members {
        let member = view.member(m);
        let optional = matches!(member.ty, Some(Type::Optional(_)));
        if member.kind == MemberKind::Field
            && member.has_annotation("Param")
            && !member.has_default
            && !optional
        {
            url.push_str("/{");
            url.push_str(&member.name);
            url.push('}');
        }
    }
    Some(url)
}

/// The text of a page's `at` template, interpolations written `{name}`.
fn template(page: &SyntaxNode) -> Option<String> {
    let clause = page.children().find(|c| c.kind() == SyntaxKind::AtClause)?;
    let lit = clause
        .children()
        .find(|c| c.kind() == SyntaxKind::StringLit)?;
    let mut out = String::new();
    for el in lit.children_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t)
                if matches!(t.kind(), SyntaxKind::StringText | SyntaxKind::StringEscape) =>
            {
                out.push_str(t.text());
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::Interpolation => {
                let name = n
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident)
                    .map(|t| t.text().to_owned())
                    .unwrap_or_default();
                out.push('{');
                out.push_str(&name);
                out.push('}');
            }
            _ => {}
        }
    }
    Some(out)
}
