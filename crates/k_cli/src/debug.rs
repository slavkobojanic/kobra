//! `k debug`: print compiler internals in the golden-file formats.
//!
//! `k debug tokens file.k`, `k debug ast file.k` and `k debug diag file.k`
//! print exactly what the corpus harness compares against `.tokens`, `.ast`
//! and `.diag` files, which makes writing and debugging corpus cases easy.
//! `k debug sema case` prints the `.diag` of a `sema` case: a single file,
//! or a project directory (with paths), exactly as the suite compares it.

use std::fs;
use std::process::ExitCode;

use k_corpus::diag_format;
use k_corpus::discover::Case;
use k_corpus::sema_suite;
use k_corpus::suites::Suite;
use k_diagnostics::FileId;
use k_syntax::dump;

use crate::cli::{DebugArgs, DebugWhat};

/// Runs `k debug`.
pub fn run(args: &DebugArgs) -> ExitCode {
    if args.what == DebugWhat::Sema {
        return sema(args);
    }
    let text = match fs::read(&args.file).map(String::from_utf8) {
        Ok(Ok(text)) => text,
        Ok(Err(_)) => {
            eprintln!("error: `{}` is not valid UTF-8", args.file.display());
            return ExitCode::from(2);
        }
        Err(error) => {
            eprintln!("error: cannot read `{}`: {error}", args.file.display());
            return ExitCode::from(2);
        }
    };
    let file = FileId::new(0);
    let output = match args.what {
        DebugWhat::Tokens => dump::dump_tokens(&k_syntax::lex(file, &text)),
        DebugWhat::Ast => dump::dump_tree(&k_syntax::parse(file, &text).syntax()),
        DebugWhat::Diag => {
            let parse = k_syntax::parse(file, &text);
            diag_format::render(&diag_format::entries_for(parse.diagnostics(), &text))
        }
        DebugWhat::Sema => return sema(args),
    };
    print!("{output}");
    ExitCode::SUCCESS
}

/// Prints the diagnostics of a `sema` case in `.diag` format.
fn sema(args: &DebugArgs) -> ExitCode {
    let project = args.file.is_dir();
    let case = Case {
        suite: Suite::Sema,
        source: args.file.clone(),
        name: args.file.display().to_string(),
        project,
    };
    match sema_suite::render_diagnostics(&case) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}
