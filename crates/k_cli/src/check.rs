//! `k check`.
//!
//! `k check` finds the projects named by its paths
//! ([`sources`]), analyses each with [`k_sema`] (every file
//! is parsed, then checked by the semantic stages of
//! `docs/compiler/sema.md`) and prints every diagnostic, human-readable on
//! standard error or as JSON Lines on standard output. It exits with 1 when
//! anything is reported. An app without errors also gets the lockfile
//! checks that need code generation's models
//! ([`k_codegen::project_diagnostics`]: `schema.lock` and the migrations,
//! `routes.lock`), then has its tests run (`docs/design/13-tooling.md`,
//! "Commands"): a program with no errors, lockfiles included, runs every
//! test of its `*.test.k` files as `k test` does ([`crate::test::check_tests`]),
//! and each failed test is a `K7510` diagnostic among the others.
//! `--no-tests` skips them. Only `k check` runs tests: the language server
//! and the warm re-check never do, so their latency doesn't depend on a
//! test suite. The formatting check joins it when the formatter lands
//! (`docs/compiler/architecture.md`).

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use k_diagnostics::{Diagnostic, SourceMap, Span, TextSize, codes, render};
use k_sema::{AnalysisHost, Options, Workspace};

use crate::cli::{CheckArgs, ColorChoice, Format};
use crate::sources::{self, Group, Problem, Scope};

/// The diagnostics of one group, with the map that renders them.
pub struct Report {
    /// The sources the diagnostics point into.
    pub map: SourceMap,
    /// The group's diagnostics, in the order they are printed.
    pub diagnostics: Vec<Diagnostic>,
    /// How many tests ran.
    pub ran: usize,
}

/// Runs `k check`.
pub fn run(args: &CheckArgs) -> ExitCode {
    let sources = sources::load(&args.paths);
    let mut reports = Vec::new();
    let mut files = 0;
    if !sources.missing.is_empty() {
        let mut map = SourceMap::new();
        let mut diagnostics = Vec::new();
        for path in &sources.missing {
            let file = map.add(path.clone(), "");
            diagnostics.push(Diagnostic::new(
                codes::IO_ERROR,
                Span::empty(file, TextSize::from(0)),
                format!("`{path}` does not exist"),
            ));
        }
        reports.push(Report {
            map,
            diagnostics,
            ran: 0,
        });
    }
    for group in sources.groups {
        files += group
            .files
            .iter()
            .filter(|(path, _)| path.kind().is_k_source())
            .count()
            + group.problems.len();
        let root = group.root.clone();
        reports.push(check_group(group, !args.no_tests, |path| {
            sources::display(&root.join(path))
        }));
    }

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
    let total: usize = reports.iter().map(|r| r.diagnostics.len()).sum();
    let errors = reports
        .iter()
        .flat_map(|r| &r.diagnostics)
        .filter(|d| d.is_error())
        .count();
    let warnings = total - errors;
    let ran: usize = reports.iter().map(|r| r.ran).sum();

    match args.format {
        Format::Json => {
            let mut out = std::io::stdout().lock();
            for report in &reports {
                for diagnostic in &report.diagnostics {
                    let _ = writeln!(out, "{}", render::json(diagnostic, &report.map));
                }
            }
        }
        Format::Human => {
            let mut err = std::io::stderr().lock();
            for report in &reports {
                for diagnostic in &report.diagnostics {
                    let _ = writeln!(err, "{}", render::human(diagnostic, &report.map, &options));
                }
            }
            let noun = if files == 1 { "file" } else { "files" };
            let tests = match ran {
                0 => String::new(),
                1 => ", ran 1 test".to_owned(),
                n => format!(", ran {n} tests"),
            };
            if total == 0 {
                let _ = writeln!(err, "checked {files} {noun}{tests}: no problems");
            } else {
                let _ = writeln!(
                    err,
                    "checked {files} {noun}{tests}: {errors} error(s), {warnings} warning(s)"
                );
            }
        }
    }

    if total == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Checks the members `members` (every member for `None`) of the
/// workspace whose files are `files`, each with the libraries it depends
/// on, and merges their diagnostics with the manifests' problems
/// (`k_sema::Workspace::report`). The members are analysed in parallel;
/// with `tests`, each member without errors then runs its tests, one after
/// another. Returns the merged map, the diagnostics and how many tests ran.
fn check_workspace(
    files: Vec<(k_sema::ProjectPath, String)>,
    members: Option<&std::collections::BTreeSet<String>>,
    tests: bool,
    name: impl Fn(&str) -> String,
) -> (SourceMap, Vec<Diagnostic>, usize) {
    let ws = Workspace::from_files(files.into_iter().map(|(p, t)| (p.as_str().to_owned(), t)));
    let wanted: Vec<usize> = (0..ws.members.len())
        .filter(|&i| members.is_none_or(|m| m.contains(&ws.members[i].dir)))
        .collect();
    // Each member's analysis already uses every core; analysing members on
    // their own threads only overlaps their sequential semantic passes.
    let analyses: Vec<(usize, k_sema::Analysis)> = std::thread::scope(|scope| {
        let handles: Vec<_> = wanted
            .iter()
            .map(|&i| {
                let ws = &ws;
                scope.spawn(move || {
                    let host = AnalysisHost::with_files(Options::new(), ws.analysis_files(i));
                    let analysis = host.analysis();
                    // Run the analysis on this thread.
                    let _ = analysis.diagnostics();
                    (i, analysis)
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    let ran = std::cell::Cell::new(0);
    let extra = |analysis: &k_sema::Analysis| {
        let (diagnostics, n) = lockfiles_and_tests(analysis, tests);
        ran.set(ran.get() + n);
        diagnostics
    };
    let (map, diagnostics) = ws
        .report_with(&analyses, extra, name)
        .unwrap_or_else(|_| (SourceMap::new(), Vec::new()));
    (map, diagnostics, ran.get())
}

/// The problems of a single project's own `k.toml` (`K8101`), which a
/// workspace reports through [`Workspace::report_with`] and a project's
/// analysis doesn't: a malformed toolchain requirement, a missing name.
fn manifest_problems(analysis: &k_sema::Analysis) -> Vec<Diagnostic> {
    let Ok(path) = k_sema::ProjectPath::new("k.toml") else {
        return Vec::new();
    };
    let Some(file) = analysis.file_id(&path) else {
        return Vec::new();
    };
    let Some(text) = analysis.text(file) else {
        return Vec::new();
    };
    let (_, problems) = k_sema::workspace::parse_manifest("k.toml", text, true, false);
    problems
        .into_iter()
        .map(|p| {
            let mut d = Diagnostic::new(p.code, Span::new(file, p.range), p.message);
            for n in p.notes {
                d = d.with_note(n);
            }
            d
        })
        .collect()
}

/// What a project adds to its semantic diagnostics: the lockfile checks
/// that need code generation's models ([`k_codegen::project_diagnostics`]:
/// `schema.lock`, `routes.lock`), then, with `tests` and no error so far,
/// the failures of its tests. Returns them and how many tests ran.
fn lockfiles_and_tests(analysis: &k_sema::Analysis, tests: bool) -> (Vec<Diagnostic>, usize) {
    let mut diagnostics = k_codegen::project_diagnostics(analysis);
    if !tests || diagnostics.iter().any(Diagnostic::is_error) {
        return (diagnostics, 0);
    }
    let clean = analysis
        .diagnostics()
        .is_ok_and(|d| !d.iter().any(Diagnostic::is_error));
    if !clean || !analysis.files().iter().any(|(_, p)| p.as_str() == "k.toml") {
        return (diagnostics, 0);
    }
    match crate::test::check_tests(analysis) {
        Some(checked) => {
            diagnostics.extend(checked.diagnostics);
            (diagnostics, checked.ran)
        }
        None => (diagnostics, 0),
    }
}

/// Analyses one group, runs its tests with `tests`, and adds the driver's
/// own diagnostics for files that could not be loaded. Each file of the
/// group is named `name(path)` in the report's source map, `path` relative
/// to the group's root.
pub fn check_group(group: Group, tests: bool, name: impl Fn(&str) -> String) -> Report {
    let (mut map, mut diagnostics, ran) = match group.scope {
        Scope::Project => {
            let host = AnalysisHost::with_files(Options::new(), group.files);
            let analysis = host.analysis();
            let map = analysis.source_map(|path| name(path.as_str()));
            // The host is never changed while it is queried, so the
            // analysis can't be cancelled.
            let mut diagnostics = analysis.diagnostics().unwrap_or_default();
            let manifest = manifest_problems(&analysis);
            if !manifest.is_empty() {
                diagnostics.extend(manifest);
                diagnostics.sort_by(|a, b| {
                    (a.span, a.code, &a.message).cmp(&(b.span, b.code, &b.message))
                });
                diagnostics.dedup();
            }
            let (more, ran) = lockfiles_and_tests(&analysis, tests);
            diagnostics.extend(more);
            (map, diagnostics, ran)
        }
        Scope::Workspace(members) => check_workspace(group.files, members.as_ref(), tests, name),
    };
    for (path, problem) in group.problems {
        let diagnostic = match problem {
            Problem::InvalidUtf8 { text, offset } => {
                let file = map.add(path, text);
                let offset = TextSize::from(u32::try_from(offset).unwrap_or(u32::MAX));
                Diagnostic::new(
                    codes::INVALID_UTF8,
                    Span::empty(file, offset),
                    "source file is not valid UTF-8",
                )
            }
            Problem::Unreadable(error) => {
                let message = format!("cannot read `{path}`: {error}");
                let file = map.add(path, "");
                Diagnostic::new(
                    codes::IO_ERROR,
                    Span::empty(file, TextSize::from(0)),
                    message,
                )
            }
            Problem::InvalidPath(message) => {
                let file = map.add(path, "");
                Diagnostic::new(
                    codes::IO_ERROR,
                    Span::empty(file, TextSize::from(0)),
                    message,
                )
            }
        };
        diagnostics.push(diagnostic);
    }
    diagnostics.sort_by_key(|d| (d.span, d.code));
    Report {
        map,
        diagnostics,
        ran,
    }
}
