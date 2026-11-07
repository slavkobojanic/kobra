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
//!
//! When the executable carries an appended app (it is a `dist/app` built by
//! `k build`), it runs that app instead: see `app_mode`.

#[cfg(feature = "server")]
mod app_mode;
mod build;
mod check;
mod cli;
mod debug;
mod sources;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command, MigrateCommand};

fn main() -> ExitCode {
    // `dist/app` is this executable with an app appended: serve it.
    #[cfg(feature = "server")]
    if let Some(code) = app_mode::run_if_app() {
        return code;
    }
    let cli = Cli::parse();
    match cli.command {
        Command::Check(args) => check::run(&args),
        Command::Debug(args) => debug::run(&args),
        Command::Build(args) => build::build(&args),
        Command::Run(args) => build::run(&args),
        Command::Dev(args) => build::dev(&args),
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
