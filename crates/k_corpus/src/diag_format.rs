//! The `.diag` expected-diagnostics format.
//!
//! A `.diag` file lists the diagnostics a case must produce. The format is
//! line-based and designed to be written by hand:
//!
//! ```text
//! # Comments start with `#`; blank lines are ignored.
//! error[K0002] 3:13-3:14: unterminated string literal
//! warning[K3204] 7:5-7:12
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
//! # File paths
//!
//! Every `SPAN` above may be prefixed with a file path and a colon,
//! `src/shop/Cart.k:3:5-3:9` (a [`Place`]). Multi-file cases (the `sema`
//! suite's projects) write the path on every header. A label or edit
//! without a path is in the same file as its header; writing the header's
//! own path there is allowed and means the same. Paths may not contain
//! spaces. Single-file cases omit paths, except for places in other files
//! such as the standard library (`<stdlib>/k/core/List.k:4:1-4:5`).
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

use std::collections::HashMap;
use std::fmt::{self, Write as _};

use k_diagnostics::{Applicability, Diagnostic, FileId, LineCol, LineIndex, Severity, Span};
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

/// A span, optionally in a named file: `3:5-3:9` or
/// `src/shop/Cart.k:3:5-3:9`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Place {
    /// The file, as written in the `.diag` file; `None` means the default
    /// file (the header's file for details, the case's file for headers).
    pub file: Option<String>,
    /// The span in that file.
    pub span: LineSpan,
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{file}:{}", self.span),
            None => write!(f, "{}", self.span),
        }
    }
}

/// One detail line of an expected diagnostic.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Detail {
    /// `label PLACE[: message]`
    Label(Place, String),
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
        /// The edits: place and replacement text.
        edits: Vec<(Place, String)>,
    },
}

/// One diagnostic as written in, or rendered for, a `.diag` file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DiagEntry {
    /// The severity.
    pub severity: Severity,
    /// The code, for example `K0002`.
    pub code: String,
    /// The file of the primary span; `None` for the case's own file.
    pub file: Option<String>,
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
            && self.file == actual.file
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
        let place = Place {
            file: self.file.clone(),
            span: self.span,
        };
        let _ = write!(out, "{}[{}] {place}", self.severity, self.code);
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

/// Escapes the newlines in a message, label or note as `\n` so that it fits
/// on one line of a `.diag` file.
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
///
/// # Example
///
/// Checking a parse against hand-written expectations, as the `parser`
/// suite does:
///
/// ```
/// use k_corpus::diag_format::{compare, entries_for, parse_diag_file};
/// use k_diagnostics::FileId;
///
/// let source = "let a = b && c\n";
/// let parse = k_syntax::parse(FileId::new(0), source);
/// let actual = entries_for(parse.diagnostics(), source);
///
/// // No message given: any message matches.
/// let expected = parse_diag_file("# `&&` is spelled `and`\nerror[K0104] 1:11-1:13\n").unwrap();
/// assert_eq!(compare(&expected, &actual), Ok(()));
///
/// let wrong = parse_diag_file("error[K0104] 1:10-1:13\n").unwrap();
/// assert!(compare(&wrong, &actual).is_err());
///
/// assert_eq!(parse_diag_file("error[K0104]").unwrap_err().line, 1);
/// ```
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
            let (place, message) = place_and_message(rest).map_err(|m| error(&m))?;
            let place = relative_to(place, entry.file.as_deref());
            entry
                .details
                .push(Detail::Label(place, message.unwrap_or_default()));
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
            let header_file = entry.file.clone();
            let Some(Detail::Fix { edits, .. }) = entry.details.last_mut() else {
                return Err(error("`edit` must follow a `fix` line"));
            };
            let (span_text, replacement) = rest
                .split_once(' ')
                .ok_or_else(|| error("expected `edit SPAN \"text\"`"))?;
            let place = parse_place(span_text).map_err(|m| error(&m))?;
            let place = relative_to(place, header_file.as_deref());
            let parsed = sexpr::parse(replacement.trim()).map_err(|e| error(&e.message))?;
            let [Sexpr::String(replacement)] = parsed.as_slice() else {
                return Err(error("expected a quoted replacement after the edit span"));
            };
            edits.push((place, replacement.clone()));
        } else {
            return Err(error(
                "unknown detail line; expected `label`, `note:`, `help:`, `fix` or `edit`",
            ));
        }
    }
    Ok(entries)
}

/// Parses a header line, `error[K0104] 2:6-2:8: message` or
/// `warning[K3204] 7:5-7:12`, into an entry with no details.
///
/// # Errors
///
/// Returns a message if the severity prefix, the `]`, a well-formed code,
/// the place or the space before it is missing or malformed.
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
    let (place, message) = place_and_message(rest)?;
    Ok(DiagEntry {
        severity,
        code: code.to_owned(),
        file: place.file,
        span: place.span,
        message,
        details: Vec::new(),
    })
}

/// Splits `PLACE: message` (or a bare `PLACE`, optionally followed by
/// `:`) into the parsed place and the trimmed message.
///
/// # Errors
///
/// Returns the message of [`parse_place`] if the place is malformed.
fn place_and_message(text: &str) -> Result<(Place, Option<String>), String> {
    match text.split_once(": ") {
        Some((place, message)) => Ok((parse_place(place.trim())?, Some(message.trim().to_owned()))),
        None => Ok((parse_place(text.trim().trim_end_matches(':'))?, None)),
    }
}

/// Drops a detail's file when it is the header's file, so both spellings
/// compare equal.
fn relative_to(place: Place, header_file: Option<&str>) -> Place {
    if place.file.is_some() && place.file.as_deref() == header_file {
        Place {
            file: None,
            span: place.span,
        }
    } else {
        place
    }
}

/// Parses a [`Place`]: a span (`3:5-3:9`), optionally prefixed with a
/// file path and `:` (`src/a/A.k:3:5-3:9`).
///
/// # Errors
///
/// Returns a message if the text is neither form, or the path is empty or
/// contains a space.
pub fn parse_place(text: &str) -> Result<Place, String> {
    let bad = || format!("`{text}` is not a span like `3:5-3:9` or `src/a/A.k:3:5-3:9`");
    let (before, end) = text.rsplit_once('-').ok_or_else(bad)?;
    let mut parts = before.rsplitn(3, ':');
    let (Some(col), Some(line)) = (parts.next(), parts.next()) else {
        return Err(bad());
    };
    let file = parts.next();
    if file.is_some_and(|f| f.is_empty() || f.contains(' ')) {
        return Err(bad());
    }
    let span = parse_span(&format!("{line}:{col}-{end}")).map_err(|message| {
        if message.contains("ends before") {
            format!("span `{text}` ends before it starts")
        } else {
            bad()
        }
    })?;
    Ok(Place {
        file: file.map(str::to_owned),
        span,
    })
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

/// Converts diagnostics of a single-file case into entries: every place is
/// in `source`, and no entry names a file.
#[must_use]
pub fn entries_for(diagnostics: &[Diagnostic], source: &str) -> Vec<DiagEntry> {
    let index = LineIndex::new(source);
    let mut entries = Vec::with_capacity(diagnostics.len());
    for d in diagnostics {
        let span = |s: Span| LineSpan {
            start: index.line_col(source, s.start()),
            end: index.line_col(source, s.end()),
        };
        entries.push(entry(d, None, &|s| (None, span(s))));
    }
    sort(&mut entries);
    entries
}

/// Converts diagnostics that may span several files into entries.
///
/// `files` maps a file id to its display path and text (`None` for an
/// unknown id, shown as `<unknown>` with positions clamped to `1:1`). A
/// header names its file unless it is `home`; a label or edit names its
/// file unless it is the header's file. Entries are sorted by file, then
/// span, code and message.
#[must_use]
pub fn entries_for_files<'a>(
    diagnostics: &[Diagnostic],
    files: impl Fn(FileId) -> Option<(&'a str, &'a str)>,
    home: Option<FileId>,
) -> Vec<DiagEntry> {
    // One line index per file that any place points into.
    let mut indexes: HashMap<FileId, (String, &'a str, LineIndex)> = HashMap::new();
    let mut add = |file: FileId| {
        indexes.entry(file).or_insert_with(|| {
            let (name, text) = files(file).unwrap_or(("<unknown>", ""));
            (name.to_owned(), text, LineIndex::new(text))
        });
    };
    for d in diagnostics {
        add(d.span.file);
        d.labels.iter().for_each(|l| add(l.span.file));
        d.fixes
            .iter()
            .flat_map(|f| &f.edits)
            .for_each(|e| add(e.span.file));
    }
    let mut entries = Vec::with_capacity(diagnostics.len());
    for d in diagnostics {
        let place = |s: Span| {
            let (name, text, index) = &indexes[&s.file];
            let file = (s.file != d.span.file).then(|| name.clone());
            let span = LineSpan {
                start: index.line_col(text, s.start()),
                end: index.line_col(text, s.end()),
            };
            (file, span)
        };
        let header = (Some(d.span.file) != home).then(|| indexes[&d.span.file].0.clone());
        entries.push(entry(d, header, &place));
    }
    sort(&mut entries);
    entries
}

/// Builds one entry; `place` converts a span to its file (`None` for the
/// header's file) and line span.
fn entry(
    d: &Diagnostic,
    file: Option<String>,
    place: &dyn Fn(Span) -> (Option<String>, LineSpan),
) -> DiagEntry {
    let to_place = |s: Span| {
        let (file, span) = place(s);
        Place { file, span }
    };
    let mut details = Vec::new();
    for label in &d.labels {
        details.push(Detail::Label(to_place(label.span), label.message.clone()));
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
                .map(|e| (to_place(e.span), e.replacement.clone()))
                .collect(),
        });
    }
    DiagEntry {
        severity: d.severity,
        code: d.code.to_string(),
        file,
        span: place(d.span).1,
        message: Some(d.message.clone()),
        details,
    }
}

/// Sorts entries into the canonical order of a `.diag` file: by file,
/// then span, code and message.
fn sort(entries: &mut [DiagEntry]) {
    entries.sort_by(|a, b| {
        (&a.file, a.span, &a.code, &a.message).cmp(&(&b.file, b.span, &b.code, &b.message))
    });
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

/// Indents every line of `text` by four spaces, ending each with a
/// newline.
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

    const PROJECT_SAMPLE: &str = "\
error[K1101] src/shop/Cart.k:3:8-3:20: `shop.Missing` doesn't exist
  label src/shop/Other.k:1:1-1:5: declared here
  label 3:1-3:7: in this import
  fix \"Remove the import\"
    edit src/shop/Cart.k:3:1-4:1 \"\"
    edit <stdlib>/k/core/List.k:1:1-1:1 \"x\"
warning[K1002] App.k:1:1-1:4
";

    #[test]
    fn places_may_name_a_file() {
        let entries = parse_diag_file(PROJECT_SAMPLE).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].file.as_deref(), Some("src/shop/Cart.k"));
        assert_eq!(entries[0].span.to_string(), "3:8-3:20");
        let Detail::Label(other, _) = &entries[0].details[0] else {
            panic!("{:?}", entries[0].details);
        };
        assert_eq!(other.file.as_deref(), Some("src/shop/Other.k"));
        let Detail::Label(same, _) = &entries[0].details[1] else {
            panic!();
        };
        assert_eq!(same.file, None, "no path means the header's file");
        let Detail::Fix { edits, .. } = &entries[0].details[2] else {
            panic!();
        };
        // Naming the header's own file is normalized away.
        assert_eq!(edits[0].0.file, None);
        assert_eq!(edits[1].0.file.as_deref(), Some("<stdlib>/k/core/List.k"));
        assert_eq!(entries[1].file.as_deref(), Some("App.k"));
        let rendered = render(&entries);
        assert!(rendered.contains("edit 3:1-4:1"), "{rendered}");
        assert_eq!(parse_diag_file(&rendered).unwrap(), entries);
    }

    #[test]
    fn rejects_malformed_places() {
        for bad in [
            "error[K0001] :1:1-1:2",
            "error[K0001] a.k:1-1:2",
            "error[K0001] a.k:x:1-1:2",
            "error[K0001] a.k:1:1-",
        ] {
            assert!(parse_diag_file(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn entries_for_files_name_other_files() {
        let a = FileId::new(4);
        let b = FileId::new(7);
        let texts = |file: FileId| match file.raw() {
            4 => Some(("src/a/A.k", "let x = 1\nlet y = 2\n")),
            7 => Some(("src/b/B.k", "let z = 3\n")),
            _ => None,
        };
        let d = Diagnostic::new(
            codes::SYMBOLIC_AND,
            Span::new(a, TextRange::new(14.into(), 15.into())),
            "m",
        )
        .with_label(Span::new(b, TextRange::new(4.into(), 5.into())), "there")
        .with_label(Span::new(a, TextRange::new(0.into(), 3.into())), "here");
        let unknown = Diagnostic::new(
            codes::SYMBOLIC_AND,
            Span::new(FileId::new(99), TextRange::new(3.into(), 3.into())),
            "u",
        );
        // In project mode (no home file) every header names its file.
        let entries = entries_for_files(&[d.clone(), unknown], texts, None);
        assert_eq!(
            render(&entries),
            "error[K0104] <unknown>:1:1-1:1: u\n\
             error[K0104] src/a/A.k:2:5-2:6: m\n  label src/b/B.k:1:5-1:6: there\n  label 1:1-1:4: here\n"
        );
        // With a home file, its headers have no path.
        let home = entries_for_files(&[d], texts, Some(a));
        assert_eq!(home[0].file, None);
        assert_eq!(
            home[0].to_string().lines().next(),
            Some("error[K0104] 2:5-2:6: m")
        );
        let expected = parse_diag_file(&render(&home)).unwrap();
        assert!(compare(&expected, &home).is_ok());
        // The file is part of the match.
        let elsewhere = parse_diag_file("error[K0104] src/b/B.k:2:5-2:6\n").unwrap();
        assert!(compare(&elsewhere, &home).is_err());
    }
}
