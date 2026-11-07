//! Human-readable diagnostic rendering.

use std::fmt::Write as _;

use crate::diagnostic::{Diagnostic, Severity};
use crate::source_map::{SourceFile, SourceMap};
use crate::span::Span;

/// Options for [`human`] rendering.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HumanOptions {
    /// Emit ANSI color escape sequences.
    pub color: bool,
}

impl HumanOptions {
    /// Options for plain, uncolored output (files, tests, pipes).
    #[must_use]
    pub const fn plain() -> HumanOptions {
        HumanOptions { color: false }
    }

    /// Options for colored terminal output.
    #[must_use]
    pub const fn colored() -> HumanOptions {
        HumanOptions { color: true }
    }
}

/// ANSI escape that resets all attributes.
const RESET: &str = "\x1b[0m";
/// ANSI bold, used for the header message and the `note` label.
const BOLD: &str = "\x1b[1m";
/// ANSI bold red, the color of `error` headers and primary error carets.
const RED: &str = "\x1b[1;31m";
/// ANSI bold yellow, the color of `warning` headers and primary warning
/// carets.
const YELLOW: &str = "\x1b[1;33m";
/// ANSI bold blue, used for the gutter, line numbers, `-->` and secondary
/// label underlines.
const BLUE: &str = "\x1b[1;34m";
/// ANSI bold cyan, used for the `help` and `fix` labels.
const CYAN: &str = "\x1b[1;36m";

/// Renders `diagnostic` as human-readable text, ending with a newline.
///
/// The format is:
///
/// ```text
/// error[K0002]: unterminated string literal
///  --> src/Counter.k:3:13
///   |
/// 3 |     let a = "abc
///   |             ^ string starts here
///   |
///   = note: ...
///   = help: ...
///   = fix: Replace `\{` with `{{`
/// ```
///
/// Only the first line of a multi-line span is shown; the carets run to the
/// end of that line.
#[must_use]
pub fn human(diagnostic: &Diagnostic, sources: &SourceMap, options: &HumanOptions) -> String {
    let paint = Paint { on: options.color };
    let mut out = String::new();
    let (severity_color, severity) = match diagnostic.severity {
        Severity::Error => (RED, "error"),
        Severity::Warning => (YELLOW, "warning"),
    };
    let _ = writeln!(
        out,
        "{}{}[{}]{}{}: {}{}",
        paint.on(severity_color),
        severity,
        diagnostic.code,
        paint.off(),
        paint.on(BOLD),
        diagnostic.message,
        paint.off()
    );

    let file = sources.get(diagnostic.span.file);
    let snippets = snippet_lines(diagnostic, file);
    let gutter = snippets.iter().map(|s| digits(s.line)).max().unwrap_or(1);
    let pad = " ".repeat(gutter);

    if let Some(file) = file {
        let at = file.line_col(diagnostic.span.start());
        let _ = writeln!(
            out,
            "{pad}{}-->{} {}:{}",
            paint.on(BLUE),
            paint.off(),
            file.path(),
            at
        );
    }

    if !snippets.is_empty() {
        let _ = writeln!(out, "{pad} {}|{}", paint.on(BLUE), paint.off());
        for snippet in &snippets {
            let _ = writeln!(
                out,
                "{}{:>gutter$} |{} {}",
                paint.on(BLUE),
                snippet.line,
                paint.off(),
                snippet.text
            );
            for mark in &snippet.marks {
                let marker_color = if mark.primary { severity_color } else { BLUE };
                let ch = if mark.primary { '^' } else { '-' };
                let underline: String = std::iter::repeat_n(ch, mark.width.max(1)).collect();
                let message = if mark.message.is_empty() {
                    String::new()
                } else {
                    format!(" {}", mark.message)
                };
                let _ = writeln!(
                    out,
                    "{pad} {}|{} {}{}{}{}{}",
                    paint.on(BLUE),
                    paint.off(),
                    " ".repeat(mark.col),
                    paint.on(marker_color),
                    underline,
                    message,
                    paint.off()
                );
            }
        }
    }

    let has_trailer = !diagnostic.notes.is_empty()
        || !diagnostic.helps.is_empty()
        || !diagnostic.fixes.is_empty();
    if has_trailer && !snippets.is_empty() {
        let _ = writeln!(out, "{pad} {}|{}", paint.on(BLUE), paint.off());
    }
    for note in &diagnostic.notes {
        let _ = writeln!(
            out,
            "{pad} {}={} {}note{}: {note}",
            paint.on(BLUE),
            paint.off(),
            paint.on(BOLD),
            paint.off()
        );
    }
    for help in &diagnostic.helps {
        let _ = writeln!(
            out,
            "{pad} {}={} {}help{}: {help}",
            paint.on(BLUE),
            paint.off(),
            paint.on(CYAN),
            paint.off()
        );
    }
    for fix in &diagnostic.fixes {
        let _ = writeln!(
            out,
            "{pad} {}={} {}fix{}: {}",
            paint.on(BLUE),
            paint.off(),
            paint.on(CYAN),
            paint.off(),
            fix.message
        );
    }
    out
}

/// Conditional ANSI painting: hands out escape codes when color is on and
/// empty strings when it is off, so the renderer has a single code path.
#[derive(Clone, Copy)]
struct Paint {
    /// Whether escapes are emitted; when `false` both methods return `""`.
    on: bool,
}

impl Paint {
    /// Returns `code` when painting is enabled, otherwise the empty string.
    fn on(self, code: &'static str) -> &'static str {
        if self.on { code } else { "" }
    }

    /// Returns the [`RESET`] escape when painting is enabled, otherwise the
    /// empty string.
    fn off(self) -> &'static str {
        if self.on { RESET } else { "" }
    }
}

/// One source line shown in the snippet, with the marks under it.
struct SnippetLine {
    /// The 1-based line number shown in the gutter.
    line: u32,
    /// The line's text without its terminator, tabs replaced by one space
    /// so that display columns equal char indices.
    text: String,
    /// The underlines on this line, sorted by column with the primary mark
    /// first among marks at the same column.
    marks: Vec<Mark>,
}

/// One underline under a snippet line.
struct Mark {
    /// Display column (0-based, in chars, tabs expanded to one char) where
    /// the underline starts.
    col: usize,
    /// Number of display columns underlined; at least 1.
    width: usize,
    /// The label text printed after the underline; empty for the primary
    /// span.
    message: String,
    /// Whether this is the diagnostic's primary span (drawn with `^` in the
    /// severity color) rather than a secondary label (drawn with `-`).
    primary: bool,
}

/// Collects the snippet lines for the primary span and every label in the
/// same file, ordered by line number.
fn snippet_lines(diagnostic: &Diagnostic, file: Option<&SourceFile>) -> Vec<SnippetLine> {
    let Some(file) = file else { return Vec::new() };
    let mut lines: Vec<SnippetLine> = Vec::new();
    let mut add = |span: Span, message: &str, primary: bool| {
        if span.file != diagnostic.span.file {
            return;
        }
        let Some((line, text, col, width)) = locate(file, span) else {
            return;
        };
        let mark = Mark {
            col,
            width,
            message: message.to_owned(),
            primary,
        };
        match lines.iter_mut().find(|l| l.line == line) {
            Some(existing) => existing.marks.push(mark),
            None => lines.push(SnippetLine {
                line,
                text,
                marks: vec![mark],
            }),
        }
    };
    add(diagnostic.span, "", true);
    for label in &diagnostic.labels {
        add(label.span, &label.message, false);
    }
    lines.sort_by_key(|l| l.line);
    for line in &mut lines {
        line.marks.sort_by_key(|m| (m.col, !m.primary));
    }
    lines
}

/// Finds the line containing the start of `span` and the column and width of
/// the underline, clamped to that line.
///
/// Returns `(line, shown_text, col, width)`, where `line` is 1-based,
/// `shown_text` has tabs replaced by spaces, `col` is a 0-based char column
/// and `width` is at least 1. Spans that extend past the end of the line are
/// cut at the line end; out-of-range offsets are clamped by
/// [`SourceFile::line_col`]. Returns `None` when the line index has no start
/// for the computed line or that start is not a char boundary of the text.
fn locate(file: &SourceFile, span: Span) -> Option<(u32, String, usize, usize)> {
    let text = file.text();
    let start = file.line_col(span.start());
    let line_start = u32::from(file.line_index().line_start(start.line)?) as usize;
    let rest = text.get(line_start..)?;
    let line_len = rest.find(['\n', '\r']).unwrap_or(rest.len());
    let line_text = &rest[..line_len];
    let col = start.col.saturating_sub(1) as usize;
    let end = u32::from(span.end()) as usize;
    let line_end = line_start + line_len;
    let width = if end <= line_start + byte_of_char(line_text, col) {
        1
    } else {
        let end_in_line = end.min(line_end) - line_start;
        line_text
            .get(..end_in_line)
            .map_or(1, |s| s.chars().count().saturating_sub(col).max(1))
    };
    let shown: String = line_text
        .chars()
        .map(|c| if c == '\t' { ' ' } else { c })
        .collect();
    Some((start.line, shown, col, width))
}

/// Returns the byte offset of the `n`th char of `s`, or `s.len()`.
fn byte_of_char(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map_or(s.len(), |(i, _)| i)
}

/// Returns the number of decimal digits in `n`.
fn digits(n: u32) -> usize {
    n.checked_ilog10().map_or(1, |d| d as usize + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Diagnostic, FileId, Fix, Span, TextRange, codes};

    fn span(file: FileId, start: u32, end: u32) -> Span {
        Span::new(file, TextRange::new(start.into(), end.into()))
    }

    #[test]
    fn renders_header_location_snippet_and_trailer() {
        let mut sources = SourceMap::new();
        let file = sources.add("src/A.k", "func f() {\n    let a = \"abc\n}\n");
        let d = Diagnostic::new(
            codes::UNTERMINATED_STRING,
            span(file, 23, 24),
            "unterminated string literal",
        )
        .with_label(span(file, 9, 10), "block opened here")
        .with_note("strings cannot span lines")
        .with_help("add a closing `\"`")
        .with_fix(Fix::replace("Close the string", span(file, 27, 27), "\""));
        let text = human(&d, &sources, &HumanOptions::plain());
        let expected = "\
error[K0002]: unterminated string literal
 --> src/A.k:2:13
  |
1 | func f() {
  |          - block opened here
2 |     let a = \"abc
  |             ^
  |
  = note: strings cannot span lines
  = help: add a closing `\"`
  = fix: Close the string
";
        assert_eq!(text, expected);
    }

    #[test]
    fn unknown_file_renders_header_only() {
        let sources = SourceMap::new();
        let d = Diagnostic::new(
            codes::IO_ERROR,
            span(FileId::new(7), 0, 0),
            "cannot read `x`",
        );
        assert_eq!(
            human(&d, &sources, &HumanOptions::plain()),
            "error[K9002]: cannot read `x`\n"
        );
    }

    #[test]
    fn out_of_range_span_does_not_panic() {
        let mut sources = SourceMap::new();
        let file = sources.add("a.k", "abc");
        let d = Diagnostic::new(codes::EXPECTED_TOKEN, span(file, 50, 90), "x");
        let text = human(&d, &sources, &HumanOptions::plain());
        assert!(text.contains("a.k:1:4"), "{text}");
    }

    #[test]
    fn multi_line_span_underlines_to_end_of_first_line() {
        let mut sources = SourceMap::new();
        let file = sources.add("a.k", "abcdef\nghi");
        let d = Diagnostic::new(codes::EXPECTED_TOKEN, span(file, 2, 9), "x");
        let text = human(&d, &sources, &HumanOptions::plain());
        assert!(text.contains("  |   ^^^^\n"), "{text}");
    }

    #[test]
    fn empty_span_gets_one_caret_and_wide_chars_count_once() {
        let mut sources = SourceMap::new();
        let file = sources.add("a.k", "é😀x");
        let x = "é😀".len() as u32;
        let d = Diagnostic::new(codes::EXPECTED_TOKEN, span(file, x, x), "x");
        let text = human(&d, &sources, &HumanOptions::plain());
        assert!(text.contains("a.k:1:3"), "{text}");
        assert!(text.contains("  |   ^\n"), "{text}");
    }

    #[test]
    fn colored_output_contains_escapes() {
        let mut sources = SourceMap::new();
        let file = sources.add("a.k", "x");
        let d = Diagnostic::new(codes::EXPECTED_TOKEN, span(file, 0, 1), "x");
        assert!(human(&d, &sources, &HumanOptions::colored()).contains("\x1b["));
    }

    #[test]
    fn digits_counts_correctly() {
        assert_eq!(digits(0), 1);
        assert_eq!(digits(9), 1);
        assert_eq!(digits(10), 2);
        assert_eq!(digits(12345), 5);
    }
}
