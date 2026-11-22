//! `check_snippet`: the compiler's front end on code an assistant wrote.
//!
//! The snippet is analysed exactly as `k check` analyses a project: an
//! [`AnalysisHost`] over the files, every semantic stage, the same
//! diagnostics and renderings (`k check`'s human text and `--format json`
//! objects). What a snippet lacks is supplied the way the book's sample
//! check (`crates/k_sema/tests/book.rs`) supplies it:
//!
//! * **Its path.** `file_name`, when given, is the file's project path
//!   (`App.k`, `src/todos/TodoList.k`, `src/todos/TodoList.test.k`). Else
//!   a snippet declaring an `app` is `App.k`, and any other is
//!   `src/snippet/<Name>.k` (`.test.k` with a `test`), `<Name>` its first
//!   public declaration's name, so the one-public-declaration-per-file
//!   rule holds.
//! * **A theme.** A program with no `theme` gets the book's sample theme
//!   (`docs/book/check/BookTheme.k`, tokens such as `.md`, `.muted`,
//!   `.lg`), so style tokens resolve.
//!
//! `files` adds more files of the same program (path to source), for a
//! snippet that uses declarations of its own other files. Lockfile checks
//! that need code generation and tests are not run: those need a whole
//! project, and `k check` in it is the tool for that.

use std::collections::BTreeMap;

use k_diagnostics::render;
use k_sema::{AnalysisHost, Options, ProjectPath};
use serde_json::{Value, json};

/// The book's sample theme.
const BOOK_THEME: &str = include_str!("../../../docs/book/check/BookTheme.k");
/// Where the sample theme goes.
const BOOK_THEME_PATH: &str = "src/booktheme/BookTheme.k";

/// The result of a check.
pub struct Checked {
    /// The human-readable report.
    pub text: String,
    /// The diagnostics as `k check --format json` objects, with counts.
    pub json: Value,
}

/// A check that couldn't run: a path that can't be a project path.
#[derive(Debug)]
pub struct BadPath(pub String);

/// The project path a snippet without `file_name` gets.
#[must_use]
pub fn default_path(source: &str) -> String {
    if declares(source, "app") {
        return "App.k".to_owned();
    }
    let name = public_name(source).unwrap_or("Snippet");
    if declares(source, "test") {
        format!("src/snippet/{name}.test.k")
    } else {
        format!("src/snippet/{name}.k")
    }
}

/// Checks `source` at `file_name` (or its [`default_path`]) together with
/// `files`.
///
/// # Errors
///
/// [`BadPath`] when a path can't be a project path.
pub fn check(
    source: &str,
    file_name: Option<&str>,
    files: &BTreeMap<String, String>,
) -> Result<Checked, BadPath> {
    let path = file_name.map_or_else(|| default_path(source), |p| p.trim().to_owned());
    let mut all = files.clone();
    all.insert(path.clone(), source.to_owned());
    let themed = all.values().any(|text| declares(text, "theme"));
    if !themed {
        all.insert(BOOK_THEME_PATH.to_owned(), BOOK_THEME.to_owned());
    }
    let mut inputs = Vec::new();
    for (p, text) in &all {
        let project_path = ProjectPath::new(p).map_err(|e| {
            BadPath(format!(
                "`{p}` is not a project path ({e:?}); use `App.k`, `k.toml` or `src/<module>/<Name>.k`"
            ))
        })?;
        inputs.push((project_path, text.clone()));
    }
    let host = AnalysisHost::with_files(Options::new(), inputs);
    let analysis = host.analysis();
    // The host never changes while it is queried, so this can't be
    // cancelled.
    let diagnostics = analysis.diagnostics().unwrap_or_default();
    let map = analysis.source_map(ToString::to_string);
    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
    let warnings = diagnostics.len() - errors;

    let mut text = format!("checked `{path}`");
    if files.is_empty() {
        text.push_str(" alone");
    } else {
        text.push_str(&format!(" with {} other file(s)", files.len()));
    }
    if !themed {
        text.push_str(" (with the book's sample theme, `BookTheme`)");
    }
    if diagnostics.is_empty() {
        text.push_str(": no problems\n");
    } else {
        text.push_str(&format!(": {errors} error(s), {warnings} warning(s)\n\n"));
    }
    let options = render::HumanOptions::plain();
    let mut objects = Vec::new();
    for d in &diagnostics {
        text.push_str(&render::human(d, &map, &options));
        text.push('\n');
        if let Ok(object) = serde_json::from_str::<Value>(&render::json(d, &map)) {
            objects.push(object);
        }
    }
    if errors > 0 {
        text.push_str("\nexplain_diagnostic gives each code's meaning, an example and its fix.\n");
    }
    Ok(Checked {
        text,
        json: json!({
            "file": path,
            "errors": errors,
            "warnings": warnings,
            "diagnostics": objects,
        }),
    })
}

/// The name of the first `public` declaration of `source`.
fn public_name(source: &str) -> Option<&str> {
    source.lines().find_map(|line| {
        let mut words = line.strip_prefix("public ")?.split_whitespace();
        let mut word = words.next()?;
        while matches!(word, "async" | "static") {
            word = words.next()?;
        }
        if word == "extend" {
            return None;
        }
        let name = words.next()?;
        let end = name
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(name.len());
        Some(&name[..end]).filter(|n| !n.is_empty())
    })
}

/// Whether `source` has a top-level declaration of `keyword`.
fn declares(source: &str, keyword: &str) -> bool {
    source.lines().any(|line| {
        let line = line.strip_prefix("public ").unwrap_or(line);
        line.strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(' '))
    })
}
