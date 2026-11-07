//! The static reactive-dependency pass for tooling
//! (`docs/design/03-reactivity.md`, "How effects track dependencies"): what
//! a `@Derived` field or an `@Effect` function reads, for hover.
//!
//! Tracking happens at runtime; this pass approximates it from the source,
//! as the design asks: every reference in the body to a reactive member
//! (`@State`, `@Derived`, `@Binding`, `@Prop`, `@Param`, `@Load`, a
//! computed property, which reads reactive values in turn), in source
//! order, without duplicates. Reads inside `untracked { … }` don't
//! subscribe and are left out, as are plain assignments (`x = …` writes
//! `x`). An `@Effect(on: [a, b])` lists its explicit dependencies instead.

use k_diagnostics::TextRange;
use k_sema::view::{MemberId, MemberKind, ProgramView, Resolved};
use k_sema::{Analysis, Cancelled, Resolution};
use k_syntax::{SyntaxKind, SyntaxNode};

use super::syntax;

/// The annotations that make a field reactive.
const REACTIVE: &[&str] = &["State", "Derived", "Binding", "Prop", "Param", "Load"];

/// What a `@Derived` field or an `@Effect` re-runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dependencies {
    /// The values read by the body, as written (`count`,
    /// `todos.remainingCount`).
    Tracked(Vec<String>),
    /// The explicit list of `@Effect(on: [...])`, as written.
    Explicit(Vec<String>),
}

/// The dependencies of member `m`, or `None` when it is neither a
/// `@Derived` field nor an `@Effect`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn dependencies(
    analysis: &Analysis,
    view: &ProgramView,
    m: MemberId,
) -> Result<Option<Dependencies>, Cancelled> {
    let member = view.member(m);
    let is_effect = member.has_annotation("Effect");
    if !member.has_annotation("Derived") && !is_effect {
        return Ok(None);
    }
    let Some(range) = member.range else {
        return Ok(None);
    };
    let file = view.decl(member.owner).file;
    let Some(root) = syntax(analysis, file) else {
        return Ok(None);
    };
    let Some(node) = node_at(&root, range) else {
        return Ok(None);
    };
    if is_effect {
        if let Some(explicit) = explicit_dependencies(&node) {
            return Ok(Some(Dependencies::Explicit(explicit)));
        }
    }
    let mut out: Vec<String> = Vec::new();
    for (r, resolution) in analysis.file_references(file)? {
        if !range.contains_range(r) || !matches!(&*resolution, Resolution::Member { .. }) {
            continue;
        }
        let Some(Resolved::Member(target)) = view.resolve(&resolution) else {
            continue;
        };
        let info = view.member(target);
        let reactive =
            info.kind == MemberKind::Computed || REACTIVE.iter().any(|a| info.has_annotation(a));
        if !reactive || target == m {
            continue;
        }
        let Some(token) = root.covering_element(r).into_token() else {
            continue;
        };
        if untracked_or_written(&token, &node) {
            continue;
        }
        let shown = match token.parent() {
            Some(p) if p.kind() == SyntaxKind::MemberExpr => {
                let text = p.text().to_string();
                if text.len() <= 60 && !text.contains('\n') {
                    text
                } else {
                    info.name.clone()
                }
            }
            _ => info.name.clone(),
        };
        if !out.contains(&shown) {
            out.push(shown);
        }
    }
    Ok(Some(Dependencies::Tracked(out)))
}

/// The node whose range is exactly `range`.
fn node_at(root: &SyntaxNode, range: TextRange) -> Option<SyntaxNode> {
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

/// The names in `@Effect(on: [a, b])`, if the annotation has that list.
fn explicit_dependencies(member: &SyntaxNode) -> Option<Vec<String>> {
    let annotation = member.children().find(|c| {
        c.kind() == SyntaxKind::Annotation
            && c.descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .any(|t| t.text() == "Effect")
    })?;
    let list = annotation
        .descendants()
        .find(|n| n.kind() == SyntaxKind::ListExpr)?;
    Some(
        list.children()
            .map(|e| e.text().to_string().trim().to_owned())
            .filter(|t| !t.is_empty())
            .collect(),
    )
}

/// Whether the reference `token` is inside `untracked { … }` or is the
/// target of a plain assignment, within `member`.
fn untracked_or_written(token: &k_syntax::SyntaxToken, member: &SyntaxNode) -> bool {
    let start = token.text_range().start();
    for ancestor in token.parent_ancestors() {
        if ancestor == *member {
            break;
        }
        match ancestor.kind() {
            SyntaxKind::CallExpr => {
                let callee = ancestor.first_child();
                if callee
                    .as_ref()
                    .is_some_and(|c| c.kind() == SyntaxKind::NameExpr && c.text() == "untracked")
                {
                    return true;
                }
            }
            SyntaxKind::AssignStmt => {
                let target = ancestor.first_child();
                let plain = ancestor
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .any(|t| t.kind() == SyntaxKind::Eq);
                if plain && target.is_some_and(|t| t.text_range().contains(start)) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}
