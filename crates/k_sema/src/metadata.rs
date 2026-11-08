//! The document-metadata warnings (`docs/design/22-metadata.md`, "Rules";
//! sema.md §12.9).
//!
//! * **K3027**: a view page of an app without a `title`. A page has one
//!   when its page-level `metadata` block, the layout it renders in (its
//!   own `@Layout(…)`, else the app's, none with `@NoLayout`) or the app's
//!   `metadata` block calls `title(…)`; the innermost declaration wins at
//!   run time, so any of them is enough. A `title` inside `failed` is the
//!   failure's own and doesn't count. Endpoint pages have no metadata and
//!   are skipped, and so is a program without an `app`, which renders no
//!   document. The quick fix adds `title("…")` from the page's name.
//! * **K3028**: a literal `description(…)` over 160 characters (counted as
//!   characters, not bytes), in any `metadata` block of a page, a layout or
//!   the app, `failed` included. An interpolated description isn't
//!   measured: its length is known only at run time.
//!
//! Both are syntactic over the declarations' `metadata` blocks, so they
//! cost one walk of those blocks.

use k_diagnostics::{Diagnostic, Fix, Span, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Cx, Found};
use crate::program::DeclId;
use crate::stdlib::DeclarationKind as K;

/// The longest description search results show in full.
const DESCRIPTION_LIMIT: usize = 160;

/// Runs the metadata warnings over the whole program.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let mut app = None;
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib {
            continue;
        }
        if matches!(decl.named_kind(), Some(K::App | K::Page | K::Layout)) {
            long_descriptions(cx, d);
        }
        if decl.named_kind() == Some(K::App) && app.is_none() {
            app = Some(d);
        }
    }
    let Some(app) = app else {
        return;
    };
    let app_title = has_title(&p.decl(app).node);
    let default_layout = layout_annotation(cx, app);
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib
            || decl.named_kind() != Some(K::Page)
            || crate::framework::is_endpoint_page(cx, d)
            || app_title
            || has_title(&decl.node)
        {
            continue;
        }
        let layout = if decl.has_annotation("NoLayout") {
            None
        } else {
            layout_annotation(cx, d).or(default_layout)
        };
        if layout.is_some_and(|l| has_title(&p.decl(l).node)) {
            continue;
        }
        missing_title(cx, d);
    }
}

/// The page-level `metadata` blocks of a declaration (its member list's
/// `MetadataBlock` children, never those inside `failed`).
fn metadata_blocks(node: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    node.children()
        .filter(|c| c.kind() == SyntaxKind::MemberList)
        .flat_map(|list| list.children())
        .filter(|c| c.kind() == SyntaxKind::MetadataBlock)
}

/// The entry calls named `name` (`title(…)`) below `node`.
fn entry_calls(node: &SyntaxNode, name: &'static str) -> impl Iterator<Item = ast::CallExpr> {
    node.descendants()
        .filter_map(ast::CallExpr::cast)
        .filter(move |c| {
            matches!(c.callee(), Some(ast::Expr::NameExpr(n))
            if n.syntax().first_token().is_some_and(|t| t.text() == name))
        })
}

/// `true` when a page-level `metadata` block of the declaration at `node`
/// calls `title(…)`.
fn has_title(node: &SyntaxNode) -> bool {
    metadata_blocks(node).any(|b| entry_calls(&b, "title").next().is_some())
}

/// The layout named by the `@Layout(…)` annotation on `d`, if it resolves
/// to a layout.
fn layout_annotation(cx: &Cx<'_, '_>, d: DeclId) -> Option<DeclId> {
    let decl = cx.p.decl(d);
    let ann = decl
        .node
        .children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|t| t.text() == "Layout"))?;
    let Some(ast::Expr::NameExpr(n)) = ann.arg_list()?.args().next()?.value() else {
        return None;
    };
    let name = n.name()?;
    match cx.lookup(decl.file, name.text()) {
        Found::Decl(l) if cx.p.decl(l).named_kind() == Some(K::Layout) => Some(l),
        _ => None,
    }
}

/// K3027 at the page's name, with a fix that adds `title("…")` to its
/// `metadata` block, or a new block holding it.
fn missing_title(cx: &Cx<'_, '_>, d: DeclId) {
    let p = cx.p;
    let decl = p.decl(d);
    let file = decl.file;
    let text = p.file(file).text;
    let title = humanize(&decl.name);
    // The indentation of the line the declaration starts on.
    let start = usize::from(decl.node.text_range().start());
    let line_start = text[..start.min(text.len())]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let indent: String = text[line_start..]
        .chars()
        .take_while(|c| *c == ' ')
        .collect();
    let brace = |node: &SyntaxNode| {
        node.descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::LBrace)
    };
    let fix = match metadata_blocks(&decl.node).next() {
        Some(block) => brace(&block).map(|b| {
            (
                b.text_range().end(),
                format!("\n{indent}        title(\"{title}\")"),
            )
        }),
        None => decl
            .node
            .children()
            .find(|c| c.kind() == SyntaxKind::MemberList)
            .and_then(|list| brace(&list))
            .map(|b| {
                (
                    b.text_range().end(),
                    format!(
                        "\n{indent}    metadata {{\n{indent}        title(\"{title}\")\n{indent}    }}\n"
                    ),
                )
            }),
    };
    let mut diagnostic = Diagnostic::new(
        codes::PAGE_WITHOUT_TITLE,
        p.span(file, decl.name_range),
        format!("the page `{}` has no `title`", decl.name),
    )
    .with_note("neither its `metadata`, nor its layout's, nor the app's sets one");
    if let Some((at, insert)) = fix {
        diagnostic = diagnostic.with_fix(
            Fix::replace(
                format!("Add `title(\"{title}\")`"),
                Span::empty(p.file(file).id, at),
                insert,
            )
            .maybe_incorrect(),
        );
    }
    cx.report(file, diagnostic);
}

/// K3028 for each literal `description(…)` over [`DESCRIPTION_LIMIT`]
/// characters in a `metadata` block of `d`, `failed` included.
fn long_descriptions(cx: &Cx<'_, '_>, d: DeclId) {
    let decl = cx.p.decl(d);
    let blocks = decl
        .node
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::MetadataBlock || is_failed_metadata(n));
    for block in blocks {
        for call in entry_calls(&block, "description") {
            let Some(ast::Expr::StringLit(lit)) = call
                .arg_list()
                .and_then(|l| l.args().next())
                .and_then(|a| a.value())
            else {
                continue;
            };
            let Some(value) = lit.plain_value() else {
                continue;
            };
            let count = value.chars().count();
            if count > DESCRIPTION_LIMIT {
                let range: TextRange = lit.syntax().text_range();
                cx.report(
                    decl.file,
                    Diagnostic::new(
                        codes::LONG_DESCRIPTION,
                        cx.p.span(decl.file, range),
                        format!(
                            "the description has {count} characters; search results show {DESCRIPTION_LIMIT}"
                        ),
                    ),
                );
            }
        }
    }
}

/// `true` for the statement form of a metadata block inside `failed`
/// (`metadata { … }` as view content, a call with a trailing block).
fn is_failed_metadata(node: &SyntaxNode) -> bool {
    let Some(call) = ast::CallExpr::cast(node.clone()) else {
        return false;
    };
    let named = matches!(call.callee(), Some(ast::Expr::NameExpr(n))
        if n.syntax().first_token().is_some_and(|t| t.text() == "metadata"));
    named
        && node
            .ancestors()
            .any(|a| a.kind() == SyntaxKind::FailedBlock)
}

/// A page's name as a title: `OrderHistory` becomes `Order history`.
fn humanize(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::humanize;

    #[test]
    fn a_page_name_becomes_a_sentence_case_title() {
        assert_eq!(humanize("Home"), "Home");
        assert_eq!(humanize("OrderHistory"), "Order history");
        assert_eq!(humanize("ProductDetail"), "Product detail");
    }
}
