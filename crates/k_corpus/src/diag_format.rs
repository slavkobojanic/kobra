//! The `.diag` expected-diagnostics format.
//!
//! A `.diag` file lists the diagnostics a case must produce. The format is
//! line-based and designed to be written by hand:
//!
//! ```text
//! # Comments start with `#`; blank lines are ignored.
//! error[K0002] 3:13-3:14: unterminated string literal
//! warning[K3001] 7:5-7:12
//! error[K0104] 2:6-2:8: K uses `and`, not `&&`
//!   label 2:1-2:3: condition starts here
//!   note: K spells logical operators as words
//!   help: write `a and b`
//!   fix "Replace `&&` with `and`"
//!     edit 2:6-2:8 "and"
//! ```
//!
//! * A **header** line is `severity[CODE] SPAN` optionally followed by
//!   `: message`. `SPAN` is `line:col-line:col`, 1-based, columns counted in
//!   Unicode scalar values, end exclusive.
//! * Indented **detail** lines belong to the preceding header: `label SPAN`
//!   (optionally `: message`), `note: text`, `help: text`, `fix "message"`
//!   (optionally followed by `maybe-incorrect`) and, below a fix, one or
//!   more `edit SPAN "replacement"` lines.
//!
//! # Matching
//!
//! Expected and actual diagnostics are matched one-to-one; order in the file
//! does not matter. An expected diagnostic matches an actual one when the
//! severity, code and span are equal and:
//!
//! * the message is equal, **or** the header has no message (wildcard);
//! * the detail lines are equal, **or** the expected entry has no detail
//!   lines at all (wildcard).
//!
//! Every actual diagnostic must be matched, so a case without a `.diag` file
//! asserts that there are no diagnostics.

use std::fmt::{self, Write as _};

use k_diagnostics::{Applicability, Diagnostic, LineCol, LineIndex, Severity, TextSize};
use k_syntax::sexpr::{self, Sexpr};

/// A `line:col-line:col` range.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineSpan {
    /// The first position covered.
    pub start: LineCol,
    /// The position just past the end.
    pub end: LineCol,
}

impl fmt::Display for LineSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

/// One detail line of an expected diagnostic.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Detail {
    /// `label SPAN[: message]`
    Label(LineSpan, String),
    /// `note: text`
    Note(String),
    /// `help: text`
    Help(String),
    /// `fix "message" [maybe-incorrect]` with its `edit` lines.
    Fix {
        /// The fix description.
        message: String,
        /// `true` unless marked `maybe-incorrect`.
        machine_applicable: bool,
        /// The edits: span and replacement text.
        edits: Vec<(LineSpan, String)>,
    },
}

/// One diagnostic as written in, or rendered for, a `.diag` file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DiagEntry {
    /// The severity.
    pub severity: Severity,
    /// The code, for example `K0002`.
    pub code: String,
    /// The primary span.
    pub span: LineSpan,
    /// The message; `None` is a wildcard.
    pub message: Option<String>,
    /// The detail lines; empty is a wildcard when the entry is an
    /// expectation.
    pub details: Vec<Detail>,
}

impl DiagEntry {
    /// Returns `true` if this expectation is satisfied by `actual`.
    #[must_use]
    pub fn matches(&self, actual: &DiagEntry) -> bool {
        self.severity == actual.severity
            && self.code == actual.code
            && self.span == actual.span
            && self
                .message
                .as_ref()
                .is_none_or(|m| Some(m) == actual.message.as_ref())
            && (self.details.is_empty() || self.details == actual.details)
    }
}

impl fmt::Display for DiagEntry {
    /// Writes the entry in canonical `.diag` form, including its detail
    /// lines, ending with a newline.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        let _ = write!(out, "{}[{}] {}", self.severity, self.code, self.span);
        if let Some(message) = &self.message {
            let _ = write!(out, ": {}", one_line(message));
        }
        out.push('\n');
        for detail in &self.details {
            match detail {
                Detail::Label(span, message) if message.is_empty() => {
                    let _ = writeln!(out, "  label {span}");
                }
                Detail::Label(span, message) => {
                    let _ = writeln!(out, "  label {span}: {}", one_line(message));
                }
                Detail::Note(text) => {
                    let _ = writeln!(out, "  note: {}", one_line(text));
                }
                Detail::Help(text) => {
                    let _ = writeln!(out, "  help: {}", one_line(text));
                }
                Detail::Fix {
                    message,
                    machine_applicable,
                    edits,
                } => {
                    out.push_str("  fix ");
                    sexpr::write_quoted(&mut out, message);
                    if !machine_applicable {
                        out.push_str(" maybe-incorrect");
                    }
                    out.push('\n');
                    for (span, replacement) in edits {
                        let _ = write!(out, "    edit {span} ");
                        sexpr::write_quoted(&mut out, replacement);
                        out.push('\n');
                    }
                }
            }
        }
        f.write_str(&out)
    }
}

fn one_line(text: &str) -> String {
    text.replace('\n', "\\n")
}

/// An error in a `.diag` file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DiagFormatError {
    /// 1-based line number in the `.diag` file.
    pub line: usize,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for DiagFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for DiagFormatError {}

/// Reads a `.diag` file.
///
/// # Errors
///
/// Returns the first malformed line.
pub fn parse_diag_file(text: &str) -> Result<Vec<DiagEntry>, DiagFormatError> {
    let mut entries: Vec<DiagEntry> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let error = |message: &str| DiagFormatError {
            line: number,
            message: message.to_owned(),
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indented = raw.starts_with([' ', '\t']);
        if !indented {
            entries.push(parse_header(trimmed).map_err(|m| error(&m))?);
            continue;
        }
        let Some(entry) = entries.last_mut() else {
            return Err(error("detail line before the first diagnostic header"));
        };
        if let Some(rest) = trimmed.strip_prefix("label ") {
            let (span, message) = span_and_message(rest).map_err(|m| error(&m))?;
            entry
                .details
                .push(Detail::Label(span, message.unwrap_or_default()));
        } else if let Some(rest) = trimmed.strip_prefix("note:") {
            entry.details.push(Detail::Note(rest.trim().to_owned()));
        } else if let Some(rest) = trimmed.strip_prefix("help:") {
            entry.details.push(Detail::Help(rest.trim().to_owned()));
        } else if let Some(rest) = trimmed.strip_prefix("fix ") {
            let parsed = sexpr::parse(rest).map_err(|e| error(&e.message))?;
            let (message, machine_applicable) = match parsed.as_slice() {
                [Sexpr::String(m)] => (m.clone(), true),
                [Sexpr::String(m), Sexpr::Symbol(s)] if s == "maybe-incorrect" => {
                    (m.clone(), false)
                }
                _ => {
                    return Err(error(
                        "expected `fix \"message\"` optionally followed by `maybe-incorrect`",
                    ));
                }
            };
            entry.details.push(Detail::Fix {
                message,
                machine_applicable,
                edits: Vec::new(),
            });
        } else if let Some(rest) = trimmed.strip_prefix("edit ") {
            let Some(Detail::Fix { edits, .. }) = entry.details.last_mut() else {
                return Err(error("`edit` must follow a `fix` line"));
            };
            let (span_text, replacement) = rest
                .split_once(' ')
                .ok_or_else(|| error("expected `edit SPAN \"text\"`"))?;
            let span = parse_span(span_text).map_err(|m| error(&m))?;
            let parsed = sexpr::parse(replacement.trim()).map_err(|e| error(&e.message))?;
            let [Sexpr::String(replacement)] = parsed.as_slice() else {
                return Err(error("expected a quoted replacement after the edit span"));
            };
            edits.push((span, replacement.clone()));
        } else {
            return Err(error(
                "unknown detail line; expected `label`, `note:`, `help:`, `fix` or `edit`",
            ));
        }
    }
    Ok(entries)
}

fn parse_header(line: &str) -> Result<DiagEntry, String> {
    let (severity, rest) = if let Some(rest) = line.strip_prefix("error[") {
        (Severity::Error, rest)
    } else if let Some(rest) = line.strip_prefix("warning[") {
        (Severity::Warning, rest)
    } else {
        return Err("expected a header starting with `error[` or `warning[`".into());
    };
    let (code, rest) = rest.split_once(']').ok_or("missing `]` after the code")?;
    if k_diagnostics::Code::parse(code).is_none() {
        return Err(format!("`{code}` is not a code like `K0002`"));
    }
    let rest = rest
        .strip_prefix(' ')
        .ok_or("expected a space and a span after the code")?;
    let (span, message) = span_and_message(rest)?;
    Ok(DiagEntry {
        severity,
        code: code.to_owned(),
        span,
        message,
        details: Vec::new(),
    })
}

fn span_and_message(text: &str) -> Result<(LineSpan, Option<String>), String> {
    match text.split_once(": ") {
        Some((span, message)) => Ok((parse_span(span.trim())?, Some(message.trim().to_owned()))),
        None => Ok((parse_span(text.trim().trim_end_matches(':'))?, None)),
    }
}

/// Parses `line:col-line:col`.
///
/// # Errors
///
/// Returns a description of the problem if the text is not a valid span.
pub fn parse_span(text: &str) -> Result<LineSpan, String> {
    let bad = || format!("`{text}` is not a span like `3:5-3:9`");
    let (start, end) = text.split_once('-').ok_or_else(bad)?;
    let position = |p: &str| -> Option<LineCol> {
        let (line, col) = p.split_once(':')?;
        let line: u32 = line.parse().ok()?;
        let col: u32 = col.parse().ok()?;
        (line > 0 && col > 0).then_some(LineCol { line, col })
    };
    let span = LineSpan {
        start: position(start).ok_or_else(bad)?,
        end: position(end).ok_or_else(bad)?,
    };
    if span.end < span.start {
        return Err(format!("span `{text}` ends before it starts"));
    }
    Ok(span)
}

/// Converts actual diagnostics into `.diag` entries, sorted canonically by
/// span, then code, then message. `source` is the text of the case file.
#[must_use]
pub fn entries_for(diagnostics: &[Diagnostic], source: &str) -> Vec<DiagEntry> {
    let index = LineIndex::new(source);
    let span = |start: TextSize, end: TextSize| LineSpan {
        start: index.line_col(source, start),
        end: index.line_col(source, end),
    };
    let mut entries: Vec<DiagEntry> = diagnostics
        .iter()
        .map(|d| {
            let mut details = Vec::new();
            for label in &d.labels {
                details.push(Detail::Label(
                    span(label.span.start(), label.span.end()),
                    label.message.clone(),
                ));
            }
            details.extend(d.notes.iter().cloned().map(Detail::Note));
            details.extend(d.helps.iter().cloned().map(Detail::Help));
            for fix in &d.fixes {
                details.push(Detail::Fix {
                    message: fix.message.clone(),
                    machine_applicable: fix.applicability == Applicability::MachineApplicable,
                    edits: fix
                        .edits
                        .iter()
                        .map(|e| (span(e.span.start(), e.span.end()), e.replacement.clone()))
                        .collect(),
                });
            }
            DiagEntry {
                severity: d.severity,
                code: d.code.to_string(),
                span: span(d.span.start(), d.span.end()),
                message: Some(d.message.clone()),
                details,
            }
        })
        .collect();
    entries.sort_by(|a, b| (a.span, &a.code, &a.message).cmp(&(b.span, &b.code, &b.message)));
    entries
}

/// Renders entries as a complete `.diag` file.
#[must_use]
pub fn render(entries: &[DiagEntry]) -> String {
    entries.iter().map(ToString::to_string).collect()
}

/// Matches expectations against actual entries.
///
/// Returns `Ok(())` if every expectation is matched by a distinct actual
/// entry and every actual entry is matched. Otherwise returns a report
/// listing the unmatched entries on both sides.
///
/// # Errors
///
/// Returns the human-readable mismatch report.
pub fn compare(expected: &[DiagEntry], actual: &[DiagEntry]) -> Result<(), String> {
    let mut used = vec![false; actual.len()];
    let mut missing = Vec::new();
    for want in expected {
        let found = actual
            .iter()
            .enumerate()
            .position(|(i, got)| !used[i] && want.matches(got));
        match found {
            Some(i) => used[i] = true,
            None => missing.push(want),
        }
    }
    let unexpected: Vec<_> = actual
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(a, _)| a)
        .collect();
    if missing.is_empty() && unexpected.is_empty() {
        return Ok(());
    }
    let mut report = String::new();
    if !missing.is_empty() {
        report.push_str("expected diagnostics that were not produced:\n");
        for entry in missing {
            report.push_str(&indent(&entry.to_string()));
        }
    }
    if !unexpected.is_empty() {
        report.push_str("produced diagnostics that were not expected:\n");
        for entry in unexpected {
            report.push_str(&indent(&entry.to_string()));
        }
    }
    Err(report)
}

fn indent(text: &str) -> String {
    text.lines().map(|line| format!("    {line}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use k_diagnostics::{FileId, Fix, Span, TextRange, codes};

    const SAMPLE: &str = "\
# comment

error[K0104] 2:6-2:8: K uses `and`, not `&&`
  label 2:1-2:3: condition starts here
  note: K spells logical operators as words
  help: write `a and b`
  fix \"Replace `&&` with `and`\"
    edit 2:6-2:8 \"and\"
warning[K3001] 7:5-7:12
error[K0002] 1:1-1:2:
";

    #[test]
    fn parses_headers_and_details() {
        let entries = parse_diag_file(SAMPLE).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].code, "K0104");
        assert_eq!(
            entries[0].message.as_deref(),
            Some("K uses `and`, not `&&`")
        );
        assert_eq!(entries[0].details.len(), 4);
        assert!(
            matches!(&entries[0].details[3], Detail::Fix { edits, machine_applicable: true, .. } if edits.len() == 1)
        );
        assert_eq!(entries[1].severity, Severity::Warning);
        assert_eq!(entries[1].message, None);
        assert_eq!(entries[2].message, None);
    }

    #[test]
    fn canonical_rendering_round_trips() {
        let entries = parse_diag_file(SAMPLE).unwrap();
        let rendered = render(&entries);
        assert_eq!(parse_diag_file(&rendered).unwrap(), entries);
    }

    #[test]
    fn rejects_malformed_lines() {
        for (bad, needle) in [
            ("fatal[K0001] 1:1-1:2", "header"),
            ("error[K1] 1:1-1:2", "is not a code"),
            ("error[K0001 1:1-1:2", "missing `]`"),
            ("error[K0001]1:1-1:2", "space"),
            ("error[K0001] 1:1", "not a span"),
            ("error[K0001] 0:1-1:2", "not a span"),
            ("error[K0001] 2:1-1:2", "ends before"),
            ("  note: x", "before the first"),
            ("error[K0001] 1:1-1:2\n  bogus", "unknown detail"),
            ("error[K0001] 1:1-1:2\n  edit 1:1-1:2 \"x\"", "must follow"),
            ("error[K0001] 1:1-1:2\n  fix message", "expected"),
            (
                "error[K0001] 1:1-1:2\n  fix \"m\"\n    edit 1:1-1:2 x",
                "quoted",
            ),
        ] {
            let error = parse_diag_file(bad).unwrap_err();
            assert!(error.message.contains(needle), "{bad:?}: {}", error.message);
        }
    }

    fn actual_entries() -> Vec<DiagEntry> {
        let source = "if a && b {}\n";
        let file = FileId::new(0);
        let at = Span::new(file, TextRange::new(5.into(), 7.into()));
        let d = Diagnostic::new(codes::SYMBOLIC_AND, at, "K uses `and`, not `&&`")
            .with_fix(Fix::replace("Replace `&&` with `and`", at, "and"));
        entries_for(&[d], source)
    }

    #[test]
    fn entries_use_one_based_char_columns() {
        let entries = actual_entries();
        assert_eq!(entries[0].span.to_string(), "1:6-1:8");
        assert_eq!(
            render(&entries),
            "error[K0104] 1:6-1:8: K uses `and`, not `&&`\n  fix \"Replace `&&` with `and`\"\n    edit 1:6-1:8 \"and\"\n"
        );
    }

    #[test]
    fn wildcards_match_and_mismatches_are_reported() {
        let actual = actual_entries();
        let wildcard = parse_diag_file("error[K0104] 1:6-1:8\n").unwrap();
        assert!(compare(&wildcard, &actual).is_ok());
        let exact = parse_diag_file(&render(&actual)).unwrap();
        assert!(compare(&exact, &actual).is_ok());
        let wrong_message = parse_diag_file("error[K0104] 1:6-1:8: nope\n").unwrap();
        let report = compare(&wrong_message, &actual).unwrap_err();
        assert!(
            report.contains("not produced") && report.contains("not expected"),
            "{report}"
        );
        let wrong_details = parse_diag_file("error[K0104] 1:6-1:8\n  note: x\n").unwrap();
        assert!(compare(&wrong_details, &actual).is_err());
        assert!(compare(&[], &actual).is_err());
        assert!(compare(&wildcard, &[]).is_err());
        assert!(compare(&[], &[]).is_ok());
    }

    #[test]
    fn each_actual_matches_at_most_one_expectation() {
        let actual = actual_entries();
        let twice = parse_diag_file("error[K0104] 1:6-1:8\nerror[K0104] 1:6-1:8\n").unwrap();
        assert!(compare(&twice, &actual).is_err());
    }
}
