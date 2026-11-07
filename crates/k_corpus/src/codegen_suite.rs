//! The `codegen` suite: what the code generator emits for a program.
//!
//! A case is a single file `name.k` or a project directory (as in the
//! `sema` suite, [`crate::sema_suite::load`]) with at least one of these
//! expectations:
//!
//! | File | Meaning |
//! |---|---|
//! | `name.client.js` | the client bundle without the runtime chunk, rendered by [`render_files`] |
//! | `name.server.js` | the server bundle, rendered the same way |
//! | `name.css` | the stylesheets, rendered the same way |
//! | `name.expect` | assertions about the output ([`parse_expect`]), for cases that pin a property rather than exact text |
//! | `name.pending` | test-first: expected to fail until the generator handles the case |
//!
//! Projects use `expected.client.js`, `expected.server.js`, … inside the
//! directory. Code is generated with [`k_codegen::Profile::Test`]
//! (readable, placeholder hashes) for [`k_codegen::Environment::Production`].
//!
//! Every case also checks the client/server split
//! ([`k_codegen::check_client_bundle`]), and must analyse without errors,
//! pending or not: a test-first case is still a valid program.
//!
//! Comparison normalizes both sides: line endings become `\n`, trailing
//! whitespace is removed from every line, and trailing blank lines are
//! dropped. With `K_BLESS=1`, a failing non-pending case rewrites the
//! exact-output files it already has (it never creates new ones).

use std::fs;

use k_codegen::{
    Entry, Environment, FileKind, Options, OutputFile, Profile, check_client_bundle, generate,
};
use k_sema::{AnalysisHost, Options as SemaOptions};

use crate::diff::line_diff;
use crate::discover::Case;
use crate::sema_suite::load;
use crate::suites::read_expectation;

/// The path prefix of the runtime chunk in the client bundle, which golden
/// files leave out: it is the embedded runtime, tested on its own.
pub const RUNTIME_CHUNK_PREFIX: &str = "runtime.";

/// One assertion of a `.expect` file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Assertion {
    /// `client`, `server` or `css`.
    pub target: &'static str,
    /// `true` for `contains`, `false` for `lacks`.
    pub contains: bool,
    /// The text, everything after `contains ` / `lacks `.
    pub text: String,
    /// The 1-based line.
    pub line: usize,
}

/// Parses a `.expect` file: one assertion per line,
/// `client|server|css contains|lacks TEXT`; `#` comments and blank lines
/// are ignored.
///
/// # Errors
///
/// A message naming the first malformed line.
pub fn parse_expect(text: &str) -> Result<Vec<Assertion>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, ' ');
        let target = match parts.next() {
            Some("client") => "client",
            Some("server") => "server",
            Some("css") => "css",
            _ => {
                return Err(format!(
                    "line {}: expected `client`, `server` or `css`",
                    i + 1
                ));
            }
        };
        let contains = match parts.next() {
            Some("contains") => true,
            Some("lacks") => false,
            _ => return Err(format!("line {}: expected `contains` or `lacks`", i + 1)),
        };
        let text = parts.next().unwrap_or("");
        if text.is_empty() {
            return Err(format!("line {}: the assertion has no text", i + 1));
        }
        out.push(Assertion {
            target,
            contains,
            text: text.to_owned(),
            line: i + 1,
        });
    }
    Ok(out)
}

/// Normalizes text for comparison: `\r\n` → `\n`, no trailing whitespace on
/// any line, no trailing blank lines, and a final newline unless empty.
#[must_use]
pub fn normalize(text: &str) -> String {
    let mut lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.trim_end_matches('\r').trim_end())
        .collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}

/// Renders generated files as one text: each file, sorted by path, under a
/// header line (`// file: PATH` for JavaScript and source maps,
/// `/* file: PATH */` for CSS) and normalized.
#[must_use]
pub fn render_files(files: &[&OutputFile]) -> String {
    let mut sorted: Vec<&&OutputFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut out = String::new();
    for f in sorted {
        match f.kind {
            FileKind::Css => out.push_str(&format!("/* file: {} */\n", f.path)),
            FileKind::JavaScript | FileKind::SourceMap => {
                out.push_str(&format!("// file: {}\n", f.path))
            }
        }
        out.push_str(&normalize(&f.contents));
    }
    normalize(&out)
}

/// Runs one `codegen` case.
///
/// # Errors
///
/// A failure report.
pub fn run(case: &Case, bless: bool) -> Result<(), String> {
    let loaded = load(case)?;
    let pending = match read_expectation(&case.expectation("pending"))? {
        Some(reason) if reason.trim().is_empty() => {
            return Err(format!(
                "{} is empty; say what is missing",
                case.expectation("pending").display()
            ));
        }
        Some(reason) => Some(reason.trim().to_owned()),
        None => None,
    };
    let exact: Vec<(&str, Option<String>)> = ["client.js", "server.js", "css"]
        .into_iter()
        .map(|ext| read_expectation(&case.expectation(ext)).map(|t| (ext, t)))
        .collect::<Result<_, _>>()?;
    let expect = read_expectation(&case.expectation("expect"))?;
    if exact.iter().all(|(_, t)| t.is_none()) && expect.is_none() {
        return Err(
            "a codegen case needs at least one of .client.js, .server.js, .css or .expect"
                .to_owned(),
        );
    }
    let assertions = match &expect {
        Some(text) => parse_expect(text)
            .map_err(|e| format!("{}: {e}", case.expectation("expect").display()))?,
        None => Vec::new(),
    };

    let host = AnalysisHost::with_files(SemaOptions::new(), loaded.files);
    let analysis = host.analysis();
    let diagnostics = analysis
        .diagnostics()
        .map_err(|_| "the analysis was cancelled".to_owned())?;
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    if !errors.is_empty() {
        return Err(format!(
            "a codegen case must analyse without errors (pending or not):\n  {}",
            errors.join("\n  ")
        ));
    }
    let options = Options {
        profile: Profile::Test,
        environment: Environment::Production,
        entry: Entry::App,
    };

    let outcome = (|| -> Result<(), String> {
        let output = generate(&analysis, &options).map_err(|e| e.to_string())?;
        let mut failures = Vec::new();
        for v in check_client_bundle(&output, &[]) {
            failures.push(format!(
                "client file {} contains `{}`, which must stay on the server",
                v.file, v.found
            ));
        }
        let client: Vec<&OutputFile> = output
            .client
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript && !f.path.starts_with(RUNTIME_CHUNK_PREFIX))
            .collect();
        let server: Vec<&OutputFile> = output
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .collect();
        let css: Vec<&OutputFile> = output
            .client
            .iter()
            .filter(|f| f.kind == FileKind::Css)
            .collect();
        let rendered = [
            ("client.js", render_files(&client)),
            ("server.js", render_files(&server)),
            ("css", render_files(&css)),
        ];
        for ((ext, expected), (_, actual)) in exact.iter().zip(rendered.iter()) {
            let Some(expected) = expected else { continue };
            if normalize(expected) != *actual {
                if bless && pending.is_none() {
                    fs::write(case.expectation(ext), actual).map_err(|e| e.to_string())?;
                    continue;
                }
                failures.push(format!(
                    "{} differs (- expected, + actual):\n{}",
                    case.expectation(ext).display(),
                    line_diff(&normalize(expected), actual)
                ));
            }
        }
        for a in &assertions {
            let haystack = match a.target {
                "client" => &rendered[0].1,
                "server" => &rendered[1].1,
                _ => &rendered[2].1,
            };
            if haystack.contains(&a.text) != a.contains {
                failures.push(format!(
                    "line {} of the .expect file: the {} output {} `{}`",
                    a.line,
                    a.target,
                    if a.contains { "lacks" } else { "contains" },
                    a.text
                ));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n\n"))
        }
    })();

    match (pending, outcome) {
        (None, outcome) => outcome,
        (Some(_), Err(_)) => Ok(()),
        (Some(reason), Ok(())) => Err(format!(
            "this pending case passes now ({reason}); delete {}",
            case.expectation("pending").display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(normalize("a  \r\nb\n\n\n"), "a\nb\n");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("\n\n"), "");
    }

    #[test]
    fn expect_files() {
        let a = parse_expect("# c\nclient lacks Secret\n\nserver contains export function main\n")
            .expect("parses");
        assert_eq!(a.len(), 2);
        assert_eq!(
            (a[0].target, a[0].contains, a[0].text.as_str(), a[0].line),
            ("client", false, "Secret", 2)
        );
        assert!(parse_expect("browser lacks x").is_err());
        assert!(parse_expect("client has x").is_err());
        assert!(parse_expect("client lacks").is_err());
    }

    #[test]
    fn rendering_sorts_files_and_marks_them() {
        let a = OutputFile {
            path: "b.js".into(),
            contents: "x  \n".into(),
            kind: FileKind::JavaScript,
        };
        let b = OutputFile {
            path: "a.css".into(),
            contents: ".a {}\n".into(),
            kind: FileKind::Css,
        };
        assert_eq!(
            render_files(&[&a, &b]),
            "/* file: a.css */\n.a {}\n// file: b.js\nx\n"
        );
        assert_eq!(render_files(&[]), "");
    }
}
