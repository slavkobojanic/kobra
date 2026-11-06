//! `k debug`: print compiler internals in the golden-file formats.
//!
//! `k debug tokens file.k`, `k debug ast file.k` and `k debug diag file.k`
//! print exactly what the corpus harness compares against `.tokens`, `.ast`
//! and `.diag` files, which makes writing and debugging corpus cases easy.

use std::fs;
use std::process::ExitCode;

use k_corpus::diag_format;
use k_diagnostics::FileId;
use k_syntax::dump;

use crate::cli::{DebugArgs, DebugWhat};

/// Runs `k debug`.
pub fn run(args: &DebugArgs) -> ExitCode {
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
    };
    print!("{output}");
    ExitCode::SUCCESS
}
