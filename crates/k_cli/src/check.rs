//! `k check`.
//!
//! Today `k check` lexes and parses every `.k` file and reports syntax
//! diagnostics. Semantic analysis, the formatting check, tests and lockfile
//! checks join it as those stages land (see `docs/compiler/architecture.md`).

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use k_diagnostics::{Diagnostic, render};

use crate::cli::{CheckArgs, ColorChoice, Format};
use crate::sources;

/// Runs `k check`.
pub fn run(args: &CheckArgs) -> ExitCode {
    let sources = sources::load(&args.paths);
    let mut diagnostics: Vec<Diagnostic> = sources.diagnostics.clone();
    for &file in &sources.files {
        let Some(source) = sources.map.get(file) else {
            continue;
        };
        let parse = k_syntax::parse(file, source.text());
        diagnostics.extend_from_slice(parse.diagnostics());
    }
    diagnostics.sort_by_key(|d| (d.span, d.code));

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
    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
    let warnings = diagnostics.len() - errors;

    match args.format {
        Format::Json => {
            let mut out = std::io::stdout().lock();
            for diagnostic in &diagnostics {
                let _ = writeln!(out, "{}", render::json(diagnostic, &sources.map));
            }
        }
        Format::Human => {
            let mut err = std::io::stderr().lock();
            for diagnostic in &diagnostics {
                let _ = writeln!(err, "{}", render::human(diagnostic, &sources.map, &options));
            }
            let files = sources.files.len();
            let noun = if files == 1 { "file" } else { "files" };
            if diagnostics.is_empty() {
                let _ = writeln!(err, "checked {files} {noun}: no problems");
            } else {
                let _ = writeln!(
                    err,
                    "checked {files} {noun}: {errors} error(s), {warnings} warning(s)"
                );
            }
        }
    }

    if diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
