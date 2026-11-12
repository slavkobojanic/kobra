//! The corpus suites and how each one runs a case.

use std::fs;
use std::path::Path;

use k_diagnostics::FileId;
use k_syntax::dump::{self, TokenLine};
use k_syntax::sexpr;

use crate::diag_format;
use crate::diff::line_diff;
use crate::discover::Case;

/// A corpus suite: a directory under `tests/corpus` and the stage it tests.
///
/// Later stages add suites here (`codegen`, `fmt`, ...); the plan is in
/// `docs/compiler/testing.md`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Suite {
    /// `tests/corpus/lexer`: token streams (`.tokens`) and lexer diagnostics.
    Lexer,
    /// `tests/corpus/parser`: syntax tree dumps (`.ast`) and lexer plus
    /// parser diagnostics.
    Parser,
    /// `tests/corpus/sema`: every diagnostic of a project (syntax and
    /// semantic) and caret annotations ([`crate::annotations`]); single
    /// files and multi-file projects. See [`crate::sema_suite`].
    Sema,
    /// `tests/corpus/codegen`: generated client JS, server JS and CSS,
    /// compared after normalization, or assertions about them. See
    /// [`crate::codegen_suite`].
    Codegen,
    /// `tests/corpus/run`: programs compiled and executed in V8, compared
    /// on their log output and result. Executed by the `k_server` test
    /// binary `corpus_run`, which has the engine; see [`crate::run_suite`].
    Run,
    /// `tests/corpus/db`: apps with entities, repositories and migrations,
    /// served on a fresh SQLite database and driven by their
    /// `requests.http` script. Executed by the `k_server` test binary `e2e`,
    /// which has the server.
    Db,
}

impl Suite {
    /// Every suite.
    pub const ALL: &'static [Suite] = &[
        Suite::Lexer,
        Suite::Parser,
        Suite::Sema,
        Suite::Codegen,
        Suite::Run,
        Suite::Db,
    ];

    /// Returns the directory name of the suite.
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Suite::Lexer => "lexer",
            Suite::Parser => "parser",
            Suite::Sema => "sema",
            Suite::Codegen => "codegen",
            Suite::Run => "run",
            Suite::Db => "db",
        }
    }

    /// Returns the suite whose directory is called `name`.
    #[must_use]
    pub fn from_dir_name(name: &str) -> Option<Suite> {
        Suite::ALL.iter().copied().find(|s| s.dir_name() == name)
    }

    /// Returns the expectation file extensions this suite uses (without the
    /// dot). For `lexer` and `parser` the first one is required for every
    /// case; every `sema` expectation is optional.
    #[must_use]
    pub const fn expectation_extensions(self) -> &'static [&'static str] {
        match self {
            Suite::Lexer => &["tokens", "diag"],
            Suite::Parser => &["ast", "diag"],
            Suite::Sema => &["diag", "fixed", "pending"],
            Suite::Codegen => &["client.js", "server.js", "css", "expect", "pending"],
            Suite::Run => &["stdout", "result", "pending"],
            Suite::Db => &["pending"],
        }
    }

    /// Returns `true` if a directory holding a `k.toml` is one multi-file
    /// case in this suite.
    #[must_use]
    pub const fn has_projects(self) -> bool {
        matches!(self, Suite::Sema | Suite::Codegen | Suite::Run | Suite::Db)
    }
}

/// Runs one case. With `bless`, mismatching expectation files are rewritten
/// from the actual output and the case passes.
///
/// # Errors
///
/// Returns a failure report: what differs and how to inspect it.
pub fn run_case(case: &Case, bless: bool) -> Result<(), String> {
    // These cases may be directories, so they are read by their own
    // drivers.
    match case.suite {
        Suite::Sema => return crate::sema_suite::run(case, bless),
        Suite::Codegen => return crate::codegen_suite::run(case, bless),
        Suite::Run => {
            return Err(
                "run cases execute JavaScript and are run by `cargo test -p k_server --test corpus_run`"
                    .to_owned(),
            );
        }
        Suite::Db => {
            return Err(
                "db cases serve an app on a database and are run by `cargo test -p k_server --test e2e -- db/`"
                    .to_owned(),
            );
        }
        Suite::Lexer | Suite::Parser => {}
    }
    let bytes = fs::read(&case.source)
        .map_err(|e| format!("cannot read {}: {e}", case.source.display()))?;
    let source = String::from_utf8(bytes).map_err(|_| {
        format!(
            "{} is not valid UTF-8; corpus sources must be UTF-8",
            case.source.display()
        )
    })?;
    let file = FileId::new(0);
    let mut failures = Vec::new();

    let diagnostics = match case.suite {
        Suite::Lexer => {
            let lexed = k_syntax::lex(file, &source);
            let rebuilt: String = lexed.iter().map(|(_, text)| text).collect();
            if rebuilt != source {
                failures.push("the token stream is not lossless: token texts do not concatenate to the source".to_owned());
            }
            if let Err(failure) = check_tokens(case, &dump::token_lines(&lexed), bless) {
                failures.push(failure);
            }
            lexed.into_diagnostics()
        }
        Suite::Parser => {
            let parse = k_syntax::parse(file, &source);
            if parse.syntax().text() != source.as_str() {
                failures.push(
                    "the syntax tree is not lossless: its text differs from the source".to_owned(),
                );
            }
            if let Err(failure) = check_ast(case, &dump::tree_sexpr(&parse.syntax()), bless) {
                failures.push(failure);
            }
            parse.diagnostics().to_vec()
        }
        // Handled before the source is read; kept for exhaustiveness.
        Suite::Sema | Suite::Codegen | Suite::Run | Suite::Db => {
            return Err(format!(
                "{} cases are run by their own driver",
                case.suite.dir_name()
            ));
        }
    };

    if let Err(failure) = check_diagnostics(case, &source, &diagnostics, bless) {
        failures.push(failure);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

/// Compares the actual token lines of a `lexer` case with its `.tokens`
/// file, parsed rather than compared as text so that layout does not
/// matter. With `bless`, a missing or differing file is (re)written.
///
/// # Errors
///
/// Returns a message for an unreadable or malformed expectation file, a
/// missing file (without `bless`), or a line diff of the token streams.
fn check_tokens(case: &Case, actual: &[TokenLine], bless: bool) -> Result<(), String> {
    let path = case.expectation("tokens");
    let actual_text: String = actual.iter().map(|t| format!("{t}\n")).collect();
    let Some(expected_text) = read_expectation(&path)? else {
        return missing(&path, &actual_text, bless);
    };
    let expected =
        dump::parse_tokens_dump(&expected_text).map_err(|e| format!("{}: {e}", path.display()))?;
    if expected == actual {
        return Ok(());
    }
    if bless {
        return write(&path, &actual_text);
    }
    let expected_canonical: String = expected.iter().map(|t| format!("{t}\n")).collect();
    Err(format!(
        "token stream differs from {} (- expected, + actual):\n{}",
        path.display(),
        line_diff(&expected_canonical, &actual_text)
    ))
}

/// Compares the actual tree dump of a `parser` case with its `.ast` file,
/// which must hold exactly one S-expression; the comparison is structural,
/// so layout and comments do not matter. With `bless`, a missing or
/// differing file is (re)written in canonical layout.
///
/// # Errors
///
/// Returns a message for an unreadable or malformed expectation file, a
/// missing file (without `bless`), or a diff of the pretty-printed trees.
fn check_ast(case: &Case, actual: &sexpr::Sexpr, bless: bool) -> Result<(), String> {
    let path = case.expectation("ast");
    let actual_text = sexpr::pretty(actual);
    let Some(expected_text) = read_expectation(&path)? else {
        return missing(&path, &actual_text, bless);
    };
    let parsed = sexpr::parse(&expected_text).map_err(|e| format!("{}:{e}", path.display()))?;
    let [expected] = parsed.as_slice() else {
        return Err(format!(
            "{} must contain exactly one S-expression, found {}",
            path.display(),
            parsed.len()
        ));
    };
    if expected == actual {
        return Ok(());
    }
    if bless {
        return write(&path, &actual_text);
    }
    Err(format!(
        "syntax tree differs from {} (- expected, + actual; both in canonical layout):\n{}",
        path.display(),
        line_diff(&sexpr::pretty(expected), &actual_text)
    ))
}

/// Compares `diagnostics` (converted to `.diag` entries against `source`)
/// with the case's `.diag` file; a missing file expects no diagnostics.
/// With `bless`, a mismatch rewrites the file, or removes it when there
/// are no actual diagnostics.
///
/// # Errors
///
/// Returns a message for an unreadable or malformed `.diag` file, a
/// failed write or removal, or the report of [`diag_format::compare`].
fn check_diagnostics(
    case: &Case,
    source: &str,
    diagnostics: &[k_diagnostics::Diagnostic],
    bless: bool,
) -> Result<(), String> {
    let path = case.expectation("diag");
    let actual = diag_format::entries_for(diagnostics, source);
    let expected = match read_expectation(&path)? {
        Some(text) => {
            diag_format::parse_diag_file(&text).map_err(|e| format!("{}: {e}", path.display()))?
        }
        None => Vec::new(),
    };
    match diag_format::compare(&expected, &actual) {
        Ok(()) => Ok(()),
        Err(_) if bless => {
            if actual.is_empty() {
                fs::remove_file(&path).map_err(|e| format!("cannot remove {}: {e}", path.display()))
            } else {
                write(&path, &diag_format::render(&actual))
            }
        }
        Err(report) => Err(format!(
            "diagnostics differ from {}:\n{report}",
            path.display()
        )),
    }
}

/// Reads an expectation file, returning `Ok(None)` if it does not exist.
///
/// # Errors
///
/// Returns a message for any read error other than "not found".
pub(crate) fn read_expectation(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Handles a missing required expectation file: with `bless` it writes
/// `actual` to `path`.
///
/// # Errors
///
/// Without `bless`, always returns a message that explains how to create
/// the file and shows the current output; with `bless`, the error of
/// [`write()`].
pub(crate) fn missing(path: &Path, actual: &str, bless: bool) -> Result<(), String> {
    if bless {
        return write(path, actual);
    }
    Err(format!(
        "missing expectation file {}.\nWrite it by hand (see docs/compiler/testing.md), or run with K_BLESS=1 to \
         create it from the current output and then review it. Current output:\n{actual}",
        path.display()
    ))
}

/// Writes `text` to `path`, replacing the file.
///
/// # Errors
///
/// Returns a message naming the path if the write fails.
pub(crate) fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_case(tag: &str, suite: Suite, source: &str) -> (PathBuf, Case) {
        let dir = std::env::temp_dir().join(format!("k_corpus_suite_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("case.k");
        fs::write(&path, source).unwrap();
        (
            dir,
            Case {
                suite,
                source: path,
                name: "case.k".into(),
                project: false,
            },
        )
    }

    #[test]
    fn missing_expectation_fails_then_bless_creates_it() {
        let (dir, case) = temp_case("bless", Suite::Lexer, "abc");
        let error = run_case(&case, false).unwrap_err();
        assert!(error.contains("missing expectation file"), "{error}");
        run_case(&case, true).unwrap();
        assert!(case.expectation("tokens").is_file());
        run_case(&case, false).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn mismatched_tokens_report_a_diff() {
        let (dir, case) = temp_case("tokens", Suite::Lexer, "abc");
        // The lexer yields `Ident "abc"`; a wrong expectation must fail with
        // a diff showing both lines.
        fs::write(case.expectation("tokens"), "IntLit \"abc\"\n").unwrap();
        let error = run_case(&case, false).unwrap_err();
        assert!(error.contains("- IntLit \"abc\""), "{error}");
        assert!(error.contains("+ Ident \"abc\""), "{error}");
        // The correct expectation passes.
        fs::write(case.expectation("tokens"), "Ident \"abc\"\n").unwrap();
        run_case(&case, false).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unexpected_diag_file_entries_fail_and_bless_removes_them() {
        let (dir, case) = temp_case("diag", Suite::Parser, "");
        fs::write(case.expectation("ast"), "(SourceFile)").unwrap();
        fs::write(case.expectation("diag"), "error[K0001] 1:1-1:2\n").unwrap();
        let error = run_case(&case, false).unwrap_err();
        assert!(error.contains("not produced"), "{error}");
        run_case(&case, true).unwrap();
        assert!(!case.expectation("diag").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn malformed_ast_file_is_reported() {
        let (dir, case) = temp_case("ast", Suite::Parser, "");
        fs::write(case.expectation("ast"), "(SourceFile) (Extra)").unwrap();
        let error = run_case(&case, false).unwrap_err();
        assert!(error.contains("exactly one S-expression"), "{error}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn suite_names_round_trip() {
        for suite in Suite::ALL {
            assert_eq!(Suite::from_dir_name(suite.dir_name()), Some(*suite));
        }
        assert_eq!(Suite::from_dir_name("nope"), None);
    }
}
