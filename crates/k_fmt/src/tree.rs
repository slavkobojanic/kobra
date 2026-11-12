//! Comparing syntax trees modulo whitespace: the formatter's self-check.

use k_syntax::{SyntaxKind, SyntaxNode};
use rowan::WalkEvent;

/// One step of a preorder walk with whitespace left out.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    /// A node starts.
    Enter(SyntaxKind),
    /// A token other than whitespace and newlines, with its text
    /// ([`comment_text`] for comments).
    Token(SyntaxKind, String),
    /// A node ends.
    Leave,
}

/// Returns `true` for the trivia the layout may change freely.
fn is_space(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::Whitespace | SyntaxKind::Newline)
}

/// Returns a comment's text as the layout writes it: `\n` line endings and
/// no trailing whitespace on any line.
pub(crate) fn comment_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').map(str::trim_end).collect();
    lines.join("\n")
}

/// The steps of `root`, whitespace left out.
fn steps(root: &SyntaxNode) -> Vec<Step> {
    let mut out = Vec::new();
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(rowan::NodeOrToken::Node(n)) => out.push(Step::Enter(n.kind())),
            WalkEvent::Leave(rowan::NodeOrToken::Node(_)) => out.push(Step::Leave),
            WalkEvent::Enter(rowan::NodeOrToken::Token(t)) if !is_space(t.kind()) => {
                let text = if t.kind().is_trivia() {
                    comment_text(t.text())
                } else {
                    t.text().to_owned()
                };
                out.push(Step::Token(t.kind(), text));
            }
            WalkEvent::Enter(rowan::NodeOrToken::Token(_))
            | WalkEvent::Leave(rowan::NodeOrToken::Token(_)) => {}
        }
    }
    out
}

/// Describes the steps around index `at` for an error message.
fn context(steps: &[Step], at: usize) -> String {
    let from = at.saturating_sub(4);
    let to = (at + 4).min(steps.len());
    steps[from..to]
        .iter()
        .map(|s| match s {
            Step::Enter(k) => format!("({k:?}"),
            Step::Token(k, t) => format!("{k:?} {t:?}"),
            Step::Leave => ")".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Checks that `before` and `after` have the same nodes and the same
/// tokens in the same places, whitespace aside; comments must match too,
/// in text and in position.
pub(crate) fn same_tree(before: &SyntaxNode, after: &SyntaxNode) -> Result<(), String> {
    let (a, b) = (steps(before), steps(after));
    match a.iter().zip(&b).position(|(x, y)| x != y) {
        Some(i) => Err(format!(
            "expected `{}`, found `{}`",
            context(&a, i),
            context(&b, i)
        )),
        None if a.len() != b.len() => {
            Err(format!("the trees have {} and {} steps", a.len(), b.len()))
        }
        None => Ok(()),
    }
}

/// Checks that `before` and `after` have the same tokens, comments
/// included, in any order: the check after declarations were moved.
pub(crate) fn same_tokens_sorted(before: &SyntaxNode, after: &SyntaxNode) -> Result<(), String> {
    let tokens = |root: &SyntaxNode| {
        let mut v: Vec<Step> = steps(root)
            .into_iter()
            .filter(|s| matches!(s, Step::Token(..)))
            .collect();
        v.sort();
        v
    };
    let (a, b) = (tokens(before), tokens(after));
    if a == b {
        Ok(())
    } else {
        let i = a
            .iter()
            .zip(&b)
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len()));
        Err(format!(
            "the first difference is at sorted token {i}: `{}` and `{}`",
            context(&a, i),
            context(&b, i)
        ))
    }
}
