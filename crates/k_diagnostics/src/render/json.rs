//! Machine-readable (JSON) diagnostic rendering.
//!
//! The JSON writer is hand-written to keep this crate free of dependencies;
//! the output is small and its shape is fixed.

use std::fmt::Write as _;

use crate::diagnostic::Diagnostic;
use crate::source_map::SourceMap;
use crate::span::Span;

/// Renders `diagnostic` as a single-line JSON object (no trailing newline).
///
/// The shape is:
///
/// ```json
/// {"code":"K0104","name":"symbolic-and","severity":"error",
///  "message":"K uses `and`, not `&&`",
///  "span":{"file":"src/A.k","start":10,"end":12,
///          "startLine":1,"startColumn":11,"endLine":1,"endColumn":13},
///  "labels":[{"span":{...},"message":"..."}],
///  "notes":["..."],"helps":["..."],
///  "fixes":[{"message":"Replace `&&` with `and`","applicability":"machine-applicable",
///            "edits":[{"span":{...},"replacement":"and"}]}]}
/// ```
///
/// `start` and `end` are UTF-8 byte offsets; lines and columns are 1-based,
/// columns counted in Unicode scalar values, and `endColumn` is exclusive.
/// When the file is unknown to `sources`, `file` is `null` and the
/// line/column fields are omitted. `name` is omitted for unregistered codes.
///
/// # Example
///
/// ```
/// use k_diagnostics::{codes, render, Diagnostic, SourceMap, Span, TextRange};
///
/// let mut sources = SourceMap::new();
/// let file = sources.add("src/A.k", "let a = \"abc\n");
/// let span = Span::new(file, TextRange::new(8.into(), 9.into()));
/// let diagnostic = Diagnostic::new(codes::UNTERMINATED_STRING, span, "unterminated string literal");
///
/// let line = render::json(&diagnostic, &sources);
/// assert!(line.starts_with(r#"{"code":"K0002","#));
/// assert!(line.contains(r#""startLine":1,"startColumn":9"#));
/// assert!(!line.contains('\n'));
/// ```
#[must_use]
pub fn json(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("{\"code\":");
    json_string(&mut out, &diagnostic.code.to_string());
    if let Some(info) = diagnostic.code.info() {
        out.push_str(",\"name\":");
        json_string(&mut out, info.name);
    }
    out.push_str(",\"severity\":");
    json_string(&mut out, diagnostic.severity.as_str());
    out.push_str(",\"message\":");
    json_string(&mut out, &diagnostic.message);
    out.push_str(",\"span\":");
    span_json(&mut out, diagnostic.span, sources);

    out.push_str(",\"labels\":[");
    for (i, label) in diagnostic.labels.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"span\":");
        span_json(&mut out, label.span, sources);
        out.push_str(",\"message\":");
        json_string(&mut out, &label.message);
        out.push('}');
    }
    out.push_str("],\"notes\":");
    string_array(&mut out, &diagnostic.notes);
    out.push_str(",\"helps\":");
    string_array(&mut out, &diagnostic.helps);

    out.push_str(",\"fixes\":[");
    for (i, fix) in diagnostic.fixes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"message\":");
        json_string(&mut out, &fix.message);
        out.push_str(",\"applicability\":");
        json_string(&mut out, fix.applicability.as_str());
        out.push_str(",\"edits\":[");
        for (j, edit) in fix.edits.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str("{\"span\":");
            span_json(&mut out, edit.span, sources);
            out.push_str(",\"replacement\":");
            json_string(&mut out, &edit.replacement);
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

/// Appends the JSON encoding of a span.
fn span_json(out: &mut String, span: Span, sources: &SourceMap) {
    let start = u32::from(span.start());
    let end = u32::from(span.end());
    match sources.get(span.file) {
        Some(file) => {
            let s = file.line_col(span.start());
            let e = file.line_col(span.end());
            out.push_str("{\"file\":");
            json_string(out, file.path());
            let _ = write!(
                out,
                ",\"start\":{start},\"end\":{end},\"startLine\":{},\"startColumn\":{},\"endLine\":{},\"endColumn\":{}}}",
                s.line, s.col, e.line, e.col
            );
        }
        None => {
            let _ = write!(out, "{{\"file\":null,\"start\":{start},\"end\":{end}}}");
        }
    }
}

/// Appends a JSON array of strings.
fn string_array(out: &mut String, items: &[String]) {
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json_string(out, item);
    }
    out.push(']');
}

/// Appends `value` to `out` as a quoted JSON string literal.
///
/// `"`, `\` and all control characters are escaped; everything else,
/// including non-ASCII text, is written as-is (JSON text is UTF-8).
pub fn json_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Diagnostic, FileId, Fix, Span, TextRange, codes};

    #[test]
    fn json_includes_every_part() {
        let mut sources = SourceMap::new();
        let file = sources.add("src/A.k", "if a && b {}\n");
        let at = Span::new(file, TextRange::new(5.into(), 7.into()));
        let d = Diagnostic::new(codes::SYMBOLIC_AND, at, "K uses `and`, not `&&`")
            .with_label(at, "here")
            .with_note("n")
            .with_help("h")
            .with_fix(Fix::replace("Replace `&&` with `and`", at, "and"));
        let span = r#"{"file":"src/A.k","start":5,"end":7,"startLine":1,"startColumn":6,"endLine":1,"endColumn":8}"#;
        let expected = format!(
            r#"{{"code":"K0104","name":"symbolic-and","severity":"error","message":"K uses `and`, not `&&`","span":{span},"labels":[{{"span":{span},"message":"here"}}],"notes":["n"],"helps":["h"],"fixes":[{{"message":"Replace `&&` with `and`","applicability":"machine-applicable","edits":[{{"span":{span},"replacement":"and"}}]}}]}}"#
        );
        assert_eq!(json(&d, &sources), expected);
    }

    #[test]
    fn unknown_file_has_null_path() {
        let d = Diagnostic::new(
            codes::Code::new(4242),
            Span::new(FileId::new(3), TextRange::new(1.into(), 2.into())),
            "m",
        );
        let text = json(&d, &SourceMap::new());
        assert!(
            text.contains(r#""span":{"file":null,"start":1,"end":2}"#),
            "{text}"
        );
        assert!(!text.contains("\"name\""), "{text}");
    }

    #[test]
    fn strings_are_escaped() {
        let mut out = String::new();
        json_string(&mut out, "a\"b\\c\nd\te\u{1}f\u{7f}é");
        assert_eq!(out, r#""a\"b\\c\nd\te\u0001f\u007fé""#);
    }
}
