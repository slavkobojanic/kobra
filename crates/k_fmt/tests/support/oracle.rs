//! The formatter oracle: every property of `k_fmt::format` that can be
//! checked on one file (`docs/design/56-formatter.md`, "Guarantees").
//!
//! `crates/k_fmt/tests/properties.rs` runs [`check`] over the corpus, the
//! examples, the book's samples and generated programs; a fuzz target can
//! include this file with `#[path]` and run it on arbitrary input.
//!
//! | Property | Check |
//! |---|---|
//! | refusal | a file with syntax errors is refused with them, never formatted |
//! | no internal error | an error-free file always formats |
//! | the same tree | the output parses without diagnostics into the tree of `rewrite(x)`, whitespace aside, comments included |
//! | rewrite, then layout | `format(rewrite(x)) == format(x)` |
//! | comments | the output has exactly the comments of `x`, in the order of `rewrite(x)` (which moves a declaration with its comments) |
//! | idempotence | `format(format(x)) == format(x)` |
//! | canonical text | `\n` line endings, no trailing whitespace, no tab in indentation, one final newline, no leading or doubled blank lines |
//! | whitespace invariance | changing the amount of whitespace (not where it is), the indentation, the blank lines and the line endings doesn't change the output |

#![allow(dead_code)]

use k_diagnostics::FileId;
use k_syntax::{SyntaxKind, SyntaxNode};

/// What [`check`] found the input to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It has syntax errors and was refused.
    Refused,
    /// It was formatted and every property holds.
    Formatted,
}

/// Checks every property on `text`; `seed` drives the whitespace
/// perturbation. Returns the first property that fails.
///
/// # Errors
///
/// A description of the failed property.
pub fn check(text: &str, seed: u64) -> Result<Outcome, String> {
    if !syntax_diagnostics(text).is_empty() {
        return match k_fmt::format(text) {
            Err(k_fmt::Error::Syntax(d)) if !d.is_empty() => Ok(Outcome::Refused),
            other => Err(format!(
                "a file with syntax errors wasn't refused: {other:?}"
            )),
        };
    }
    let output = k_fmt::format(text).map_err(|e| format!("formatting failed: {e}"))?;
    let parsed = k_syntax::parse(FileId::new(0), &output);
    if let Some(d) = parsed.diagnostics().first() {
        return Err(format!(
            "the output reports {}: {}\n{output}",
            d.code, d.message
        ));
    }
    let rewritten = k_fmt::rewrite(text).map_err(|e| format!("rewrite failed: {e}"))?;
    let before = shape(&k_syntax::parse(FileId::new(0), &rewritten).syntax());
    let after = shape(&parsed.syntax());
    if before != after {
        let i = before
            .iter()
            .zip(&after)
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        return Err(format!(
            "formatting changed the syntax tree at step {i}: {:?} became {:?}",
            before.get(i),
            after.get(i)
        ));
    }
    match k_fmt::format(&rewritten) {
        Ok(again) if again == output => {}
        other => return Err(format!("format(rewrite(x)) != format(x): {other:?}")),
    }
    // The comments of the output are those of the input; in the same order
    // unless a rewrite moved a declaration, and then in the order of the
    // rewritten text.
    let (mut had, mut has) = (comments(text), comments(&output));
    if comments(&rewritten) != has {
        return Err(format!(
            "layout changed the comments: {:?} became {has:?}",
            comments(&rewritten)
        ));
    }
    had.sort();
    has.sort();
    if had != has {
        return Err(format!(
            "formatting lost or added comments: {had:?} became {has:?}"
        ));
    }
    match k_fmt::format(&output) {
        Ok(twice) if twice == output => {}
        Ok(twice) => {
            return Err(format!(
                "formatting isn't idempotent:\n{}",
                first_difference(&output, &twice)
            ));
        }
        Err(e) => return Err(format!("formatting the output failed: {e}")),
    }
    canonical_text(&output)?;
    match k_fmt::format(&perturbed(text, seed)) {
        Ok(again) if again == output => {}
        Ok(again) => {
            return Err(format!(
                "the amount of whitespace changed the output:\n{}",
                first_difference(&output, &again)
            ));
        }
        Err(e) => return Err(format!("the perturbed input failed: {e}")),
    }
    Ok(Outcome::Formatted)
}

/// Checks the text properties of formatted `output`.
fn canonical_text(output: &str) -> Result<(), String> {
    let problem = if output.contains('\r') {
        Some("a carriage return".to_owned())
    } else if !output.is_empty() && (!output.ends_with('\n') || output.ends_with("\n\n")) {
        Some("not exactly one final newline".to_owned())
    } else if output.starts_with('\n') {
        Some("a leading blank line".to_owned())
    } else if output.contains("\n\n\n") {
        Some("two blank lines in a row".to_owned())
    } else {
        output.lines().enumerate().find_map(|(i, line)| {
            if line.ends_with([' ', '\t']) {
                Some(format!("trailing whitespace on line {}", i + 1))
            } else if line.starts_with('\t') {
                Some(format!("a tab in the indentation of line {}", i + 1))
            } else {
                None
            }
        })
    };
    problem.map_or(Ok(()), |p| {
        Err(format!("the output isn't canonical text: {p}"))
    })
}

/// A comment's text with `\n` line endings and no trailing whitespace on
/// any line, as the formatter writes it.
pub fn normalized_comment(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The tree of `node` as one line per node and token, whitespace and
/// newlines left out, comments kept (normalized). Written independently of
/// the formatter's own check.
pub fn shape(node: &SyntaxNode) -> Vec<String> {
    let mut out = Vec::new();
    shape_into(node, 0, &mut out);
    out
}

/// Appends the shape of `node`, at `depth`, to `out`.
fn shape_into(node: &SyntaxNode, depth: usize, out: &mut Vec<String>) {
    out.push(format!("{:depth$}{:?}", "", node.kind()));
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(n) => shape_into(&n, depth + 1, out),
            rowan::NodeOrToken::Token(t) => {
                if matches!(t.kind(), SyntaxKind::Whitespace | SyntaxKind::Newline) {
                    continue;
                }
                let text = if t.kind().is_trivia() {
                    normalized_comment(t.text())
                } else {
                    t.text().to_owned()
                };
                out.push(format!("{:w$}{:?} {text:?}", "", t.kind(), w = depth + 1));
            }
        }
    }
}

/// The comments of `text`, normalized, in order.
pub fn comments(text: &str) -> Vec<String> {
    k_syntax::lex(FileId::new(0), text)
        .iter()
        .filter(|(kind, _)| {
            matches!(
                kind,
                SyntaxKind::LineComment | SyntaxKind::BlockComment | SyntaxKind::DocComment
            )
        })
        .map(|(_, text)| normalized_comment(text))
        .collect()
}

/// The syntax diagnostics of `text` as `(code, message)` pairs.
pub fn syntax_diagnostics(text: &str) -> Vec<(String, String)> {
    k_syntax::parse(FileId::new(0), text)
        .diagnostics()
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone()))
        .collect()
}

/// `text` with the amount of its whitespace changed but not where it is:
/// every run of spaces becomes one to four spaces or tabs, every line is
/// indented by a few more spaces, blank lines are doubled and line endings
/// become `\r\n`. None of this changes the tokens or the tree.
pub fn perturbed(text: &str, seed: u64) -> String {
    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let tokens: Vec<(SyntaxKind, &str)> = k_syntax::lex(FileId::new(0), text).iter().collect();
    let mut out = String::with_capacity(text.len() * 2);
    for (i, (kind, piece)) in tokens.iter().enumerate() {
        match kind {
            SyntaxKind::Whitespace if piece.starts_with('\u{feff}') => out.push_str(piece),
            SyntaxKind::Whitespace => {
                for _ in 0..=next() % 4 {
                    out.push(if next() % 3 == 0 { '\t' } else { ' ' });
                }
            }
            SyntaxKind::Newline => {
                out.push_str("\r\n");
                if tokens
                    .get(i + 1)
                    .is_some_and(|t| t.0 == SyntaxKind::Newline)
                {
                    out.push_str("\r\n");
                }
                for _ in 0..next() % 3 {
                    out.push(' ');
                }
            }
            _ => out.push_str(piece),
        }
    }
    out
}

/// The first line where `a` and `b` differ, for a failure message.
pub fn first_difference(a: &str, b: &str) -> String {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let i = la
        .iter()
        .zip(&lb)
        .position(|(x, y)| x != y)
        .unwrap_or(la.len().min(lb.len()));
    format!(
        "  line {}:\n  - {:?}\n  + {:?}",
        i + 1,
        la.get(i).copied().unwrap_or("<end>"),
        lb.get(i).copied().unwrap_or("<end>")
    )
}
