//! Caret annotations: assertions about inferred types, resolved names and
//! server/client sides, written as comments inside `sema` corpus sources.
//!
//! ```k
//! let items = cart.items
//! //  ^^^^^ type: List<shop.CartItem>
//! //          ^^^^^ resolves: member shop.Cart.items
//! //@ 1:1-1:4 type: none
//! ```
//!
//! * A **caret line** is a line whose first non-blank characters are `//`,
//!   then optional spaces, then a run of `^`. The carets mark columns of
//!   the *target line*: the nearest line above that is not itself an
//!   annotation. Columns count Unicode scalar values, as in `.diag` files,
//!   so the marked range is `line:first-caret` up to just past the last
//!   caret.
//! * An **explicit line** is `//@ SPAN kind: value`, for ranges a caret
//!   cannot reach (one starting in the first two columns, or spanning
//!   lines). `SPAN` is `line:col-line:col` as in `.diag` files.
//! * After the carets or span comes `kind: value`, where `kind` is
//!   [`Query::Type`] (`type`), [`Query::Resolves`] (`resolves`) or
//!   [`Query::Side`] (`side`). The value `none` asserts that the query
//!   answers nothing at that range.
//!
//! Several annotation lines may follow one target line; each refers to it.
//! A caret line with anything other than a known `kind:` after the carets
//! is an error, so a typo can't silently disable an assertion. Annotations
//! are recognized line by line, without lexing, so a caret line inside a
//! multi-line string literal is an annotation too.

use std::fmt;

use k_diagnostics::LineCol;

use crate::diag_format::{LineSpan, parse_span};

/// What an annotation asks the analysis.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Query {
    /// `type:` — `Analysis::type_at`, rendered as K type syntax.
    Type,
    /// `resolves:` — `Analysis::resolve_at`, rendered as
    /// `k_sema::Resolution::render` does (`member shop.Cart.items`).
    Resolves,
    /// `side:` — `Analysis::side_of`: `server-only`, `client-only` or
    /// `shared`.
    Side,
}

impl Query {
    /// Returns the keyword written before the `:`.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Query::Type => "type",
            Query::Resolves => "resolves",
            Query::Side => "side",
        }
    }

    fn from_keyword(text: &str) -> Option<Query> {
        [Query::Type, Query::Resolves, Query::Side]
            .into_iter()
            .find(|q| q.keyword() == text)
    }
}

/// One assertion found in a source file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Annotation {
    /// The 1-based line the annotation is written on.
    pub line: u32,
    /// The range it asserts about.
    pub target: LineSpan,
    /// What it asks.
    pub query: Query,
    /// The expected answer; `None` for `none`.
    pub expected: Option<String>,
}

impl fmt::Display for Annotation {
    /// Writes `type: List<String> at 3:5-3:10`, for failure reports.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} at {}",
            self.query.keyword(),
            self.expected.as_deref().unwrap_or("none"),
            self.target
        )
    }
}

/// A malformed annotation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AnnotationError {
    /// The 1-based line of the annotation.
    pub line: u32,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for AnnotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for AnnotationError {}

/// Finds every annotation in `source`, in line order.
///
/// # Errors
///
/// Returns the first malformed annotation: an unknown or missing `kind:`,
/// an empty value, a caret line with no target line above it, carets past
/// the end of the target line, or a bad explicit span.
pub fn parse(source: &str) -> Result<Vec<Annotation>, AnnotationError> {
    let mut annotations = Vec::new();
    let mut target: Option<(u32, usize)> = None;
    for (index, raw) in source.split('\n').enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let error = |message: String| AnnotationError {
            line: number,
            message,
        };
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("//@") {
            let rest = rest.trim_start();
            let (span, rest) = rest
                .split_once(' ')
                .ok_or_else(|| error("expected `//@ SPAN kind: value`".into()))?;
            let target = parse_span(span).map_err(error)?;
            let (query, expected) = query_and_value(rest).map_err(error)?;
            annotations.push(Annotation {
                line: number,
                target,
                query,
                expected,
            });
            continue;
        }
        let caret_line = trimmed
            .strip_prefix("//")
            .filter(|rest| !rest.starts_with('/'))
            .map(str::trim_start)
            .filter(|rest| rest.starts_with('^'));
        let Some(rest) = caret_line else {
            target = Some((number, line.chars().count()));
            continue;
        };
        let Some((target_line, target_len)) = target else {
            return Err(error(
                "a caret annotation needs a source line above it".into(),
            ));
        };
        let leading = line.chars().count() - rest.chars().count();
        let carets = rest.chars().take_while(|&c| c == '^').count();
        let first = leading + 1;
        let past = first + carets;
        if past - 1 > target_len {
            return Err(error(format!(
                "the carets extend past the end of line {target_line} ({target_len} characters)"
            )));
        }
        let (query, expected) = query_and_value(&rest[carets..]).map_err(error)?;
        let col = |c: usize| u32::try_from(c).unwrap_or(u32::MAX);
        annotations.push(Annotation {
            line: number,
            target: LineSpan {
                start: LineCol {
                    line: target_line,
                    col: col(first),
                },
                end: LineCol {
                    line: target_line,
                    col: col(past),
                },
            },
            query,
            expected,
        });
    }
    Ok(annotations)
}

fn query_and_value(text: &str) -> Result<(Query, Option<String>), String> {
    let text = text.trim();
    let (keyword, value) = text.split_once(':').ok_or_else(|| {
        format!("expected `type:`, `resolves:` or `side:` after the target, found `{text}`")
    })?;
    let query = Query::from_keyword(keyword.trim()).ok_or_else(|| {
        format!(
            "unknown annotation `{}`; expected `type`, `resolves` or `side`",
            keyword.trim()
        )
    })?;
    let value = value.trim();
    if value.is_empty() {
        return Err(format!(
            "`{}:` needs a value (write `none` to assert no answer)",
            query.keyword()
        ));
    }
    Ok((query, (value != "none").then(|| value.to_owned())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str) -> LineSpan {
        parse_span(text).unwrap()
    }

    #[test]
    fn carets_mark_columns_of_the_line_above() {
        let source = "let items = cart.items\n//  ^^^^^ type: List<shop.CartItem>\n//               ^^^^^ resolves: member shop.Cart.items\n";
        let found = parse(source).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].target, span("1:5-1:10"));
        assert_eq!(found[0].query, Query::Type);
        assert_eq!(found[0].expected.as_deref(), Some("List<shop.CartItem>"));
        assert_eq!(found[1].target, span("1:18-1:23"));
        assert_eq!(found[1].query, Query::Resolves);
        assert_eq!(
            found[1].to_string(),
            "resolves: member shop.Cart.items at 1:18-1:23"
        );
    }

    #[test]
    fn indented_carets_and_crlf_and_unicode_columns() {
        let source = "    let é = 1\r\n    //  ^ type: Int\r\n";
        let found = parse(source).unwrap();
        assert_eq!(found[0].target, span("1:9-1:10"));
    }

    #[test]
    fn explicit_spans_and_none() {
        let source = "x\n//@ 1:1-1:2 side: none\n//@ 1:1-2:1 type: Int\n";
        let found = parse(source).unwrap();
        assert_eq!(found[0].query, Query::Side);
        assert_eq!(found[0].expected, None);
        assert_eq!(found[1].target, span("1:1-2:1"));
    }

    #[test]
    fn ordinary_and_doc_comments_are_not_annotations() {
        let source = "// a comment\n/// ^ doc\n// at 1:1 x\nlet a = 1\n";
        assert!(parse(source).unwrap().is_empty());
    }

    #[test]
    fn annotation_lines_do_not_become_targets() {
        let source = "let a = b\n//  ^ type: Int\n//      ^ resolves: none\n";
        let found = parse(source).unwrap();
        assert!(found.iter().all(|a| a.target.start.line == 1));
    }

    #[test]
    fn malformed_annotations_are_errors() {
        for (bad, needle) in [
            ("// ^ type: Int\n", "source line above"),
            ("ab\n//   ^ type: Int\n", "past the end"),
            ("abcdef\n// ^ tipe: Int\n", "unknown annotation"),
            ("abcdef\n// ^ the bug\n", "expected `type:`"),
            ("abcdef\n// ^ type:\n", "needs a value"),
            ("abc\n//@ 1:1 type: Int\n", "not a span"),
            ("abc\n//@\n", "expected `//@"),
        ] {
            let error = parse(bad).unwrap_err();
            assert!(error.message.contains(needle), "{bad:?}: {error}");
        }
    }
}
