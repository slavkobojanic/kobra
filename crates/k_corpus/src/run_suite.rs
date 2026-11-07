//! The `run` suite: programs compiled and executed, compared on what they
//! log and what `main` returns. It is the main behavioural suite for the
//! language's semantics.
//!
//! A case is a single file `name.k` (in the implicit library project of
//! the `sema` suite, module `corpus`, entry `corpus.main`) or a project
//! directory with `k.toml` (entry: its one top-level function `main`).
//! `main` takes no parameters and may be `async` and `throws`.
//!
//! | File | Meaning | Absent means |
//! |---|---|---|
//! | `name.stdout` | the log lines, one per `Log`/`console` call: `LEVEL MESSAGE`, with `\n` in a message written as `\\n` | no output |
//! | `name.result` | one line: `returns void`, `returns JSON`, `throws JSON` or `panics MESSAGE` | `returns void` |
//! | `name.pending` | test-first: expected to fail until the feature works | — |
//!
//! `returns` carries the return value in its `Codable` JSON encoding (the
//! wire mapping of `docs/design/28-rpc-wire-format.md`), `throws` the
//! thrown error as `{"error": "corpus.E", "case": "c", "fields": {…}}`,
//! `panics` the panic message. JSON is compared as values, so spacing
//! doesn't matter.
//!
//! Executing needs V8, so this crate only defines the suite; the test
//! binary `crates/k_server/tests/corpus_run.rs` supplies the [`Executor`].
//! Like codegen cases, run cases must analyse without errors.

use std::fs;

use k_codegen::{Entry, Environment, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions};

use crate::diff::line_diff;
use crate::discover::Case;
use crate::sema_suite::{SINGLE_FILE_MODULE, load};
use crate::suites::read_expectation;

/// The result line when `main` returns nothing.
pub const RETURNS_VOID: &str = "returns void";

/// What executing a program produced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Execution {
    /// The log lines, in the `.stdout` format, each ending in `\n`.
    pub stdout: String,
    /// The result line, without a newline.
    pub result: String,
}

/// Runs generated code. Implemented by the `k_server` test binary.
pub trait Executor {
    /// Executes the server bundle of `output`, whose entry module exports
    /// `run()`: an async function returning the result line.
    ///
    /// # Errors
    ///
    /// Anything that kept the program from producing a result line (a load
    /// failure, a timeout), as text.
    fn execute(&mut self, output: &Output) -> Result<Execution, String>;
}

/// Formats one log line for `.stdout`.
#[must_use]
pub fn stdout_line(level: &str, message: &str) -> String {
    format!(
        "{level} {}\n",
        message.replace('\\', "\\\\").replace('\n', "\\n")
    )
}

fn results_match(expected: &str, actual: &str) -> bool {
    let split = |s: &str| s.split_once(' ').map(|(a, b)| (a.to_owned(), b.to_owned()));
    match (split(expected), split(actual)) {
        (Some((ka, va)), Some((kb, vb))) if ka == kb && (ka == "returns" || ka == "throws") => {
            match (
                serde_json::from_str::<serde_json::Value>(&va),
                serde_json::from_str::<serde_json::Value>(&vb),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => va == vb,
            }
        }
        _ => expected == actual,
    }
}

/// Runs one `run` case with `executor`.
///
/// # Errors
///
/// A failure report.
pub fn run(case: &Case, bless: bool, executor: &mut dyn Executor) -> Result<(), String> {
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
    let expected_stdout = read_expectation(&case.expectation("stdout"))?.unwrap_or_default();
    let expected_result = read_expectation(&case.expectation("result"))?
        .map_or_else(|| RETURNS_VOID.to_owned(), |t| t.trim_end().to_owned());
    if expected_result.lines().count() > 1 {
        return Err(format!(
            "{} must be one line",
            case.expectation("result").display()
        ));
    }
    let known = ["returns ", "throws ", "panics "];
    if !known.iter().any(|k| expected_result.starts_with(k)) {
        return Err(format!(
            "the result must start with `returns`, `throws` or `panics`: `{expected_result}`"
        ));
    }

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
            "a run case must analyse without errors (pending or not):\n  {}",
            errors.join("\n  ")
        ));
    }
    let entry = if case.project {
        "main".to_owned()
    } else {
        format!("{SINGLE_FILE_MODULE}.main")
    };
    let options = Options {
        profile: Profile::Development,
        environment: Environment::Test,
        entry: Entry::Main(entry),
    };

    let outcome = (|| -> Result<(), String> {
        let output = generate(&analysis, &options).map_err(|e| e.to_string())?;
        let execution = executor.execute(&output)?;
        let mut failures = Vec::new();
        if execution.stdout != expected_stdout {
            if bless && pending.is_none() {
                fs::write(case.expectation("stdout"), &execution.stdout)
                    .map_err(|e| e.to_string())?;
            } else {
                failures.push(format!(
                    "stdout differs (- expected, + actual):\n{}",
                    line_diff(&expected_stdout, &execution.stdout)
                ));
            }
        }
        if !results_match(&expected_result, &execution.result) {
            if bless && pending.is_none() {
                fs::write(
                    case.expectation("result"),
                    format!("{}\n", execution.result),
                )
                .map_err(|e| e.to_string())?;
            } else {
                failures.push(format!(
                    "result differs:\n  expected: {expected_result}\n  actual:   {}",
                    execution.result
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
    fn stdout_lines_escape_newlines_and_backslashes() {
        assert_eq!(stdout_line("info", "a\nb\\c"), "info a\\nb\\\\c\n");
    }

    #[test]
    fn results_compare_json_as_values() {
        assert!(results_match("returns {\"a\": 1}", "returns {\"a\":1}"));
        assert!(!results_match("returns 1", "returns 2"));
        assert!(!results_match("returns 1", "throws 1"));
        assert!(results_match(
            "panics index out of range",
            "panics index out of range"
        ));
        assert!(!results_match("panics a", "panics b"));
        assert!(results_match(RETURNS_VOID, RETURNS_VOID));
    }
}
