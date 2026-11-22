//! `explain_diagnostic`: what a diagnostic code means and how to fix it.
//!
//! The answer is assembled from the sources of truth, never written
//! twice: the code registry (`k_diagnostics::codes`: name, severity,
//! summary), the code's section of the book's generated diagnostics
//! reference (`book/reference/diagnostics.md`: an example with the
//! compiler's real output, and the fix), and the lines of the compiler's
//! diagnostics catalogue (`compiler/diagnostics.md`) that name it (the
//! primary span and the quick fix).

use k_diagnostics::{Code, CodeInfo, codes};

use crate::corpus;

/// The registry entry for `code`: `K2011`, `k2011`, `2011` or the code's
/// name (`assignment-to-let`).
#[must_use]
pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    let code = code.trim().trim_matches('`');
    let upper = code.to_ascii_uppercase();
    if let Some(found) = Code::parse(&upper).and_then(Code::info) {
        return Some(found);
    }
    if !code.is_empty() && code.len() <= 4 && code.bytes().all(|b| b.is_ascii_digit()) {
        return Code::parse(&format!("K{code:0>4}")).and_then(Code::info);
    }
    codes::lookup_name(&code.to_ascii_lowercase())
}

/// The explanation of a registered code, as Markdown.
#[must_use]
pub fn explain(info: &CodeInfo) -> String {
    let code = info.code.to_string();
    let mut out = format!(
        "# {code} `{}` ({})\n\n{}\n",
        info.name, info.severity, info.summary
    );
    let docs = corpus::embedded();
    let find = |path: &str| docs.iter().find(|d| d.path == path);
    if let Some(reference) = find("book/reference/diagnostics.md") {
        let section = section(&reference.text, &format!("### {code} "));
        if !section.is_empty() {
            out.push_str("\n## From the diagnostics reference (book/reference/diagnostics.md)\n\n");
            out.push_str(&section);
        }
    }
    if let Some(catalogue) = find("compiler/diagnostics.md") {
        let lines: Vec<&str> = catalogue
            .text
            .lines()
            .filter(|l| l.contains(&format!("| {code} |")) || l.contains(&format!("**{code}**")))
            .collect();
        if !lines.is_empty() {
            out.push_str(
                "\n## From the compiler's catalogue (compiler/diagnostics.md)\n\n\
                 Columns of a table row: code, name, severity, primary span, quick fix.\n\n",
            );
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// The lines of `text` from the heading line starting with `start` up to
/// the next heading of the same or a higher level, without the heading.
fn section(text: &str, start: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if inside {
            if line.starts_with("## ") || line.starts_with("### ") {
                break;
            }
            out.push_str(line);
            out.push('\n');
        } else if line.starts_with(start) {
            inside = true;
        }
    }
    out.trim().to_owned() + if out.trim().is_empty() { "" } else { "\n" }
}
