//! Completion of event labels in `.on(…)` (`docs/compiler/lsp.md`,
//! "Completion").
//!
//! The receiver of `.on` is found in the syntax: the chain of calls it
//! ends (`CardView(card: card).class(.card).on(`) leads to a base name,
//! a built-in element (`button`, whose events are in the stdlib's element
//! table) or a component (whose `@Emits` fields are its events). Events
//! another `.on(…)` of the same chain already handles are left out, since
//! each event is handled once (K3104). A call takes one event (K3108), so
//! nothing is offered after a first one's comma.

use k_diagnostics::{FileId, TextRange, TextSize};
use k_sema::stdlib::{DeclarationKind, elements};
use k_sema::view::{DeclId, DeclKind, MemberKind, ProgramView, Resolved, Type};
use k_sema::{Analysis, Cancelled, Resolution};
use k_syntax::{SyntaxKind, SyntaxNode};

use super::super::hover::doc_lines;
use super::super::render;
use super::{CompletionItem, ItemKind};

/// The events offered at `start` (the start of the prefix) when it is the
/// label position of an `.on(` call: `Some` of them (empty after a first
/// event's comma, or for a receiver that fires none), `None` anywhere
/// else.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub(super) fn on_items(
    analysis: &Analysis,
    view: &ProgramView,
    root: &SyntaxNode,
    text: &str,
    file: FileId,
    start: TextSize,
) -> Result<Option<Vec<CompletionItem>>, Cancelled> {
    let before = text[..usize::from(start).min(text.len())].trim_end();
    if !before.ends_with(['(', ',']) {
        return Ok(None);
    }
    let Ok(at) = u32::try_from(before.len() - 1) else {
        return Ok(None);
    };
    let at = TextSize::from(at);
    let Some(punct) = root.token_at_offset(at).find(|t| {
        t.text_range().start() == at && matches!(t.kind(), SyntaxKind::LParen | SyntaxKind::Comma)
    }) else {
        return Ok(None);
    };
    let Some(list) = punct.parent().filter(|p| p.kind() == SyntaxKind::ArgList) else {
        return Ok(None);
    };
    let Some(call) = list.parent().filter(|c| c.kind() == SyntaxKind::CallExpr) else {
        return Ok(None);
    };
    let Some(callee) = call
        .first_child()
        .filter(|c| c.kind() == SyntaxKind::MemberExpr)
    else {
        return Ok(None);
    };
    if link_name(&callee).as_deref() != Some("on") {
        return Ok(None);
    }
    if punct.kind() == SyntaxKind::Comma {
        return Ok(Some(Vec::new()));
    }
    let Some((base, links)) = callee.first_child().as_ref().and_then(chain) else {
        return Ok(Some(Vec::new()));
    };
    let handled: Vec<String> = links
        .iter()
        .filter(|l| l.first_child().and_then(|c| link_name(&c)).as_deref() == Some("on"))
        .flat_map(arg_labels)
        .collect();
    let mut items = events_of(analysis, view, file, &base)?;
    items.retain(|i| {
        let name = i.label.trim_end_matches(':');
        !handled.iter().any(|h| h == name)
    });
    Ok(Some(items))
}

/// The name a member expression's link has (`on` in `x.on`).
pub(super) fn link_name(member: &SyntaxNode) -> Option<String> {
    member
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| super::super::is_name(t.kind()))
        .last()
        .map(|t| t.text().to_owned())
}

/// The base name of a modifier chain ending in `node`, and the chain's
/// calls from the last to the first: `CardView(…).class(.x).on(e)` gives
/// the name expression `CardView` and the calls of `.on` and `.class`.
pub(super) fn chain(node: &SyntaxNode) -> Option<(SyntaxNode, Vec<SyntaxNode>)> {
    let mut links = Vec::new();
    let mut node = node.clone();
    // Chains are short; the bound only guards against odd recoveries.
    for _ in 0..256 {
        match node.kind() {
            SyntaxKind::CallExpr => {
                let callee = node.first_child()?;
                match callee.kind() {
                    SyntaxKind::MemberExpr => {
                        links.push(node.clone());
                        node = callee.first_child()?;
                    }
                    SyntaxKind::NameExpr => return Some((callee, links)),
                    _ => return None,
                }
            }
            SyntaxKind::MemberExpr => node = node.first_child()?,
            SyntaxKind::NameExpr => return Some((node, links)),
            _ => return None,
        }
    }
    None
}

/// The labels of the arguments of call `call` (`edit` in `.on(edit: …)`).
fn arg_labels(call: &SyntaxNode) -> Vec<String> {
    call.children()
        .filter(|c| c.kind() == SyntaxKind::ArgList)
        .flat_map(|list| list.children().filter(|a| a.kind() == SyntaxKind::Arg))
        .filter_map(|arg| {
            let first = arg
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| !t.kind().is_trivia())?;
            let colon = first.next_token()?;
            (super::super::is_name(first.kind()) && colon.kind() == SyntaxKind::Colon)
                .then(|| first.text().to_owned())
        })
        .collect()
}

/// The name token's range and text of a name expression.
fn name_of(expr: &SyntaxNode) -> Option<(TextRange, String)> {
    expr.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| super::super::is_name(t.kind()))
        .map(|t| (t.text_range(), t.text().to_owned()))
}

/// The component (or page or layout) name expression `base` names: as the
/// checker resolved it, else the one declaration of that name the file
/// can see (half-typed code may not have been resolved).
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub(super) fn component_of(
    analysis: &Analysis,
    view: &ProgramView,
    file: FileId,
    base: &SyntaxNode,
) -> Result<Option<DeclId>, Cancelled> {
    let Some((range, name)) = name_of(base) else {
        return Ok(None);
    };
    if let Some(r @ Resolution::Declaration { .. }) = analysis.resolve_at(file, range)? {
        if let Some(Resolved::Decl(d)) = view.resolve(&r) {
            return Ok(Some(d));
        }
    }
    let module = view
        .decls
        .iter()
        .find(|d| d.file == file)
        .and_then(|d| d.module.clone());
    let candidates = view.decls.iter().enumerate().filter(|(_, d)| {
        d.name == name
            && d.kind != DeclKind::Extension
            && matches!(
                d.named_kind(),
                Some(DeclarationKind::Component | DeclarationKind::Page | DeclarationKind::Layout)
            )
    });
    let mut fallback = None;
    for (i, d) in candidates {
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if d.module == module {
            return Ok(Some(id));
        }
        fallback.get_or_insert(id);
    }
    Ok(fallback)
}

/// The events of the element or component named by `base`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn events_of(
    analysis: &Analysis,
    view: &ProgramView,
    file: FileId,
    base: &SyntaxNode,
) -> Result<Vec<CompletionItem>, Cancelled> {
    let Some((_, name)) = name_of(base) else {
        return Ok(Vec::new());
    };
    if name.starts_with(|c: char| c.is_ascii_lowercase()) {
        let Some(def) = elements::element(&name) else {
            return Ok(Vec::new());
        };
        return Ok(def
            .events
            .iter()
            .map(|e| event_item(e.name, Some(e.payload.to_owned()), None))
            .map(|mut i| {
                i.documentation = Some(format!(
                    "The `{}` event of `{}` (`<{}>`).",
                    i.label.trim_end_matches(':'),
                    def.name,
                    def.html
                ));
                i
            })
            .collect());
    }
    let Some(d) = component_of(analysis, view, file, base)? else {
        return Ok(Vec::new());
    };
    let decl = view.decl(d);
    let source = analysis.text(decl.file);
    let mut items = Vec::new();
    for &m in &decl.members {
        let member = view.member(m);
        if member.kind != MemberKind::Field || !member.has_annotation("Emits") {
            continue;
        }
        let payload = match &member.ty {
            Some(Type::Named(_, args)) => args.first().map(|t| render::ty(view, t)),
            _ => None,
        };
        let docs = match (source, member.range) {
            (Some(text), Some(range)) => {
                let lines: Vec<&str> = doc_lines(text, range.start())
                    .into_iter()
                    .map(|(_, body)| body)
                    .collect();
                (!lines.is_empty()).then(|| lines.join("\n"))
            }
            _ => None,
        };
        items.push(event_item(&member.name, payload, docs));
    }
    Ok(items)
}

/// The item of event `name`: the label `name:`, inserting `name: `.
fn event_item(name: &str, payload: Option<String>, docs: Option<String>) -> CompletionItem {
    CompletionItem {
        label: format!("{name}:"),
        kind: ItemKind::Event,
        detail: payload,
        import: None,
        group: 0,
        documentation: docs,
        insert_text: Some(format!("{name}: ")),
    }
}
