//! `k fmt`: formats `.k` files canonically with [`k_fmt::format`]
//! (`docs/design/56-formatter.md`, "The command").
//!
//! What it formats:
//!
//! * **no path**: the workspace the working directory is in (every member
//!   and library, from any directory inside it), else the project it is
//!   in, else the working directory;
//! * **a directory**: every `.k` file below it, skipping hidden
//!   directories and build output ([`sources::is_skipped_dir`]);
//! * **a file**: that file, whatever its extension;
//! * **`-`** (alone): standard input, written formatted to standard output.
//!
//! Files are rewritten in place only when they change. With `--check`
//! nothing is written: each file that isn't formatted is printed as a
//! unified diff on standard output, and the status is 1. A file with
//! syntax errors is never formatted: its errors are printed as `k check`
//! prints them and the status is 1. An internal formatter error (the
//! result would have a different syntax tree) leaves the file alone and
//! makes the status 2.
//!
//! `--format json` prints one object per file per line on standard output
//! instead of the diffs and the summary: `file`, `status`
//! (`formatted`, `reformatted`, `unformatted`, `syntax-error` or
//! `internal-error`), and `diff` (`--check`), `diagnostics` (the
//! diagnostics' JSON objects), `message` or, for standard input, `text`.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use k_diagnostics::{SourceMap, render};
use serde_json::{Value, json};

use crate::cli::{ColorChoice, FmtArgs, Format};
use crate::fix::unified_diff;
use crate::sources;

/// What formatting one file found.
enum Outcome {
    /// Already canonical.
    Formatted,
    /// Not canonical: the canonical text.
    Changed(String),
    /// Lexer or parser errors, rendered for the chosen format.
    Syntax(Vec<k_diagnostics::Diagnostic>),
    /// A formatter bug.
    Internal(String),
}

/// Formats `text`.
fn format_text(text: &str) -> Outcome {
    match k_fmt::format(text) {
        Ok(formatted) if formatted == text => Outcome::Formatted,
        Ok(formatted) => Outcome::Changed(formatted),
        Err(k_fmt::Error::Syntax(diagnostics)) => Outcome::Syntax(diagnostics),
        Err(k_fmt::Error::Internal(message)) => Outcome::Internal(message),
    }
}

/// The tallies of a run, and its exit status.
#[derive(Default)]
struct Tally {
    /// Files looked at.
    total: usize,
    /// Files that changed (or, with `--check`, would).
    changed: usize,
    /// Files with syntax errors.
    syntax: usize,
    /// Internal errors.
    internal: usize,
}

/// Runs `k fmt`.
pub fn run(args: &FmtArgs) -> ExitCode {
    let color = match args.color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::stderr().is_terminal(),
    };
    let options = if color {
        render::HumanOptions::colored()
    } else {
        render::HumanOptions::plain()
    };
    let stdin = args.paths.iter().any(|p| p.as_os_str() == "-");
    if stdin && args.paths.len() > 1 {
        eprintln!("error: `-` (standard input) can't be combined with other paths");
        return ExitCode::from(2);
    }
    let mut tally = Tally::default();
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    if stdin {
        let mut text = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut text) {
            eprintln!("error: cannot read standard input: {e}");
            return ExitCode::from(2);
        }
        tally.total = 1;
        let outcome = format_text(&text);
        let mut record = report("<stdin>", &text, &outcome, args, &options, &mut tally);
        match (&outcome, args.format, args.check) {
            (_, Format::Json, _) => {
                if !args.check {
                    let canonical = match &outcome {
                        Outcome::Formatted => Some(text.as_str()),
                        Outcome::Changed(t) => Some(t.as_str()),
                        _ => None,
                    };
                    if let Some(t) = canonical {
                        record["text"] = json!(t);
                    }
                }
                let _ = writeln!(out, "{record}");
            }
            (Outcome::Syntax(_) | Outcome::Internal(_), Format::Human, _) => {
                let _ = write!(err, "{}", record["human"].as_str().unwrap_or_default());
            }
            (Outcome::Changed(_), Format::Human, true) => {
                let _ = write!(out, "{}", record["diff"].as_str().unwrap_or_default());
            }
            (Outcome::Formatted, Format::Human, false) => {
                let _ = write!(out, "{text}");
            }
            (Outcome::Changed(t), Format::Human, false) => {
                let _ = write!(out, "{t}");
            }
            (Outcome::Formatted, Format::Human, true) => {}
        }
        return status(&tally, args.check);
    }

    let (files, missing) = files(&args.paths);
    if !missing.is_empty() {
        for path in &missing {
            eprintln!("error: `{path}` does not exist");
        }
        return ExitCode::from(2);
    }
    for path in &files {
        let shown = sources::display(path);
        tally.total += 1;
        let text = match std::fs::read(path).map(String::from_utf8) {
            Ok(Ok(text)) => text,
            Ok(Err(_)) => {
                let _ = writeln!(err, "error: `{shown}` is not valid UTF-8");
                tally.syntax += 1;
                continue;
            }
            Err(e) => {
                let _ = writeln!(err, "error: cannot read `{shown}`: {e}");
                return ExitCode::from(2);
            }
        };
        let outcome = format_text(&text);
        if let (Outcome::Changed(formatted), false) = (&outcome, args.check)
            && let Err(e) = std::fs::write(path, formatted)
        {
            let _ = writeln!(err, "error: cannot write `{shown}`: {e}");
            return ExitCode::from(2);
        }
        let mut record = report(&shown, &text, &outcome, args, &options, &mut tally);
        match args.format {
            Format::Json => {
                if let Value::Object(map) = &mut record {
                    map.remove("human");
                }
                let _ = writeln!(out, "{record}");
            }
            Format::Human => {
                if let Some(diff) = record["diff"].as_str() {
                    let _ = write!(out, "{diff}");
                }
                if let Some(human) = record["human"].as_str() {
                    let _ = write!(err, "{human}");
                }
            }
        }
    }
    if args.format == Format::Human {
        let _ = writeln!(err, "{}", summary(&tally, args.check));
    }
    status(&tally, args.check)
}

/// The JSON record of one file, counting it in `tally`. The human output
/// rides along in `human` (diagnostics, errors) and `diff`.
fn report(
    shown: &str,
    text: &str,
    outcome: &Outcome,
    args: &FmtArgs,
    options: &render::HumanOptions,
    tally: &mut Tally,
) -> Value {
    match outcome {
        Outcome::Formatted => json!({ "file": shown, "status": "formatted" }),
        Outcome::Changed(formatted) => {
            tally.changed += 1;
            if args.check {
                json!({
                    "file": shown,
                    "status": "unformatted",
                    "diff": unified_diff(shown, text, formatted),
                })
            } else {
                json!({ "file": shown, "status": "reformatted" })
            }
        }
        Outcome::Syntax(diagnostics) => {
            tally.syntax += 1;
            let mut map = SourceMap::new();
            map.add(shown, text);
            let mut human = String::new();
            let mut list = Vec::new();
            for d in diagnostics {
                let _ = writeln!(human, "{}", render::human(d, &map, options));
                list.push(
                    serde_json::from_str::<Value>(&render::json(d, &map)).unwrap_or(Value::Null),
                );
            }
            let _ = writeln!(human, "`{shown}` has syntax errors: not formatted");
            json!({
                "file": shown,
                "status": "syntax-error",
                "diagnostics": list,
                "human": human,
            })
        }
        Outcome::Internal(message) => {
            tally.internal += 1;
            json!({
                "file": shown,
                "status": "internal-error",
                "message": message,
                "human": format!("error: internal formatter error in `{shown}`, left as it is: {message}\n"),
            })
        }
    }
}

/// The last line of the human report.
fn summary(tally: &Tally, check: bool) -> String {
    let noun = |n: usize| if n == 1 { "file" } else { "files" };
    let total = tally.total;
    let mut line = match (check, tally.changed) {
        (true, 0) => format!("checked {total} {}: all formatted", noun(total)),
        (true, n) => format!(
            "{n} of {total} {} would be reformatted: run `k fmt`",
            noun(total)
        ),
        (false, 0) => format!("{total} {} already formatted", noun(total)),
        (false, n) => format!("formatted {n} of {total} {}", noun(total)),
    };
    let failed = tally.syntax + tally.internal;
    if failed > 0 {
        let _ = write!(
            line,
            "; {failed} {} not formatted (see above)",
            noun(failed)
        );
    }
    line
}

/// The exit status: 2 for an internal error, 1 for syntax errors or, with
/// `--check`, an unformatted file.
fn status(tally: &Tally, check: bool) -> ExitCode {
    if tally.internal > 0 {
        ExitCode::from(2)
    } else if tally.syntax > 0 || (check && tally.changed > 0) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// The files `paths` name, sorted, and the paths that don't exist.
fn files(paths: &[PathBuf]) -> (BTreeSet<PathBuf>, Vec<String>) {
    let mut found = BTreeSet::new();
    let mut missing = Vec::new();
    if paths.is_empty() {
        let here = Path::new(".");
        let root = crate::workspace::locate(here)
            .map(|w| w.root)
            .or_else(|| sources::project_root(here))
            .unwrap_or_else(|| here.to_path_buf());
        walk(&root, &mut found);
    }
    for path in paths {
        if path.is_dir() {
            walk(path, &mut found);
        } else if path.exists() {
            found.insert(path.clone());
        } else {
            missing.push(sources::display(path));
        }
    }
    (found, missing)
}

/// Collects the `.k` files below `dir`, skipping hidden directories and
/// build output.
fn walk(dir: &Path, out: &mut BTreeSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !name.starts_with('.') && !sources::is_skipped_dir(&name) {
                walk(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "k") {
            out.insert(path);
        }
    }
}
