//! `k`: the single binary of the K toolchain.
//!
//! `k` bundles the compiler, the test runner, the development server and the
//! database migration tool. Each subcommand is a thin driver over the
//! library crates; see `docs/compiler/architecture.md`, "Crate layout".
//!
//! # Exit status
//!
//! | Status | Meaning |
//! |---|---|
//! | 0 | Success: no problems found. |
//! | 1 | The command ran and found problems (diagnostics, failing tests). |
//! | 2 | Usage error, unimplemented command, or an internal error. |

mod check;
mod cli;
mod debug;
mod sources;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command, MigrateCommand};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check(args) => check::run(&args),
        Command::Debug(args) => debug::run(&args),
        Command::Build => not_implemented("build"),
        Command::Dev => not_implemented("dev"),
        Command::Test => not_implemented("test"),
        Command::Routes => not_implemented("routes"),
        Command::Migrate { command } => match command {
            MigrateCommand::New { .. } => not_implemented("migrate new"),
            MigrateCommand::Apply => not_implemented("migrate apply"),
            MigrateCommand::Status => not_implemented("migrate status"),
        },
    }
}

/// Reports that a subcommand exists but is not implemented yet.
fn not_implemented(command: &str) -> ExitCode {
    eprintln!("error: `k {command}` is not implemented yet");
    ExitCode::from(2)
}
