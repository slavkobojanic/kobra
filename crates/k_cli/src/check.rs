//! `k check`.
//!
//! `k check` finds the projects named by its paths
//! ([`sources`]), analyses each with [`k_sema`] (every file
//! is parsed, then checked by the semantic stages of
//! `docs/compiler/sema.md`) and prints every diagnostic, human-readable on
//! standard error or as JSON Lines on standard output. It exits with 1 when
//! anything is reported. The formatting check, tests and lockfile checks
//! join it as those stages land (`docs/compiler/architecture.md`).

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use k_diagnostics::{Diagnostic, SourceMap, Span, TextSize, codes, render};
use k_sema::{AnalysisHost, Options, Workspace};

use crate::cli::{CheckArgs, ColorChoice, Format};
use crate::sources::{self, Group, Problem, Scope};

/// The diagnostics of one group, with the map that renders them.
struct Report {
    /// The sources the diagnostics point into.
    map: SourceMap,
    /// The group's diagnostics, in the order they are printed.
    diagnostics: Vec<Diagnostic>,
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
        reports.push(Report { map, diagnostics });
    }
    for group in sources.groups {
        files += group
            .files
            .iter()
            .filter(|(path, _)| path.kind().is_k_source())
            .count()
            + group.problems.len();
        reports.push(check_group(group));
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
            if total == 0 {
                let _ = writeln!(err, "checked {files} {noun}: no problems");
            } else {
                let _ = writeln!(
                    err,
                    "checked {files} {noun}: {errors} error(s), {warnings} warning(s)"
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
/// (`k_sema::Workspace::report`). The members are analysed in parallel.
fn check_workspace(
    root: &std::path::Path,
    files: Vec<(k_sema::ProjectPath, String)>,
    members: Option<&std::collections::BTreeSet<String>>,
) -> (SourceMap, Vec<Diagnostic>) {
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
    ws.report(&analyses, |path| sources::display(&root.join(path)))
        .unwrap_or_else(|_| (SourceMap::new(), Vec::new()))
}

/// Analyses one group and adds the driver's own diagnostics for files that
/// could not be loaded.
fn check_group(group: Group) -> Report {
    let root = group.root;
    let (mut map, mut diagnostics) = match group.scope {
        Scope::Project => {
            let host = AnalysisHost::with_files(Options::new(), group.files);
            let analysis = host.analysis();
            let map = analysis.source_map(|path| sources::display(&root.join(path.as_str())));
            // The host is never changed while it is queried, so the
            // analysis can't be cancelled.
            (map, analysis.diagnostics().unwrap_or_default())
        }
        Scope::Workspace(members) => check_workspace(&root, group.files, members.as_ref()),
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
    Report { map, diagnostics }
}
