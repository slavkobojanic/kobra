//! Inlay hints: each page's URL, shown after its name when the page has no
//! `at` clause, so the URL derived from the name and its `@Param`s is
//! visible where the page is declared (`docs/design/13-tooling.md`: "Each
//! page's current URL is shown inline"). A page with an `at` clause
//! already shows its URL.

use k_diagnostics::{FileId, TextRange, TextSize};
use k_sema::stdlib::DeclarationKind;
use k_sema::view::DeclId;
use k_sema::{Analysis, Cancelled};
use k_syntax::SyntaxKind;

use super::syntax;

/// One hint: where it goes and its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayHint {
    /// The offset the hint is shown at.
    pub offset: TextSize,
    /// The label (`at "/sign-in"`).
    pub label: String,
}

/// The hints of `file` within `range`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn inlay_hints(
    analysis: &Analysis,
    file: FileId,
    range: TextRange,
) -> Result<Vec<InlayHint>, Cancelled> {
    let view = analysis.program()?;
    let root = syntax(analysis, file);
    let mut out = Vec::new();
    for (i, decl) in view.decls.iter().enumerate() {
        if decl.file != file
            || decl.named_kind() != Some(DeclarationKind::Page)
            || range.intersect(decl.name_range).is_none()
        {
            continue;
        }
        let explicit = root.as_ref().is_some_and(|root| {
            root.covering_element(decl.range)
                .ancestors()
                .chain(root.covering_element(decl.name_range).ancestors())
                .find(|n| n.kind() == SyntaxKind::PageDecl)
                .is_some_and(|page| page.children().any(|c| c.kind() == SyntaxKind::AtClause))
        });
        if explicit {
            continue;
        }
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if let Some(url) = analysis.page_url(id)? {
            out.push(InlayHint {
                offset: decl.name_range.end(),
                label: format!("at \"{url}\""),
            });
        }
    }
    Ok(out)
}
