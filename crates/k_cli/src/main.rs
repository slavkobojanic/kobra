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
//! | 2 | Usage error or an internal error. |
//!
//! When the executable carries an appended app (it is a `dist/app` built by
//! `k build`), it runs that app instead: see `app_mode`.

#![warn(clippy::missing_docs_in_private_items)]

#[cfg(feature = "server")]
mod app_mode;
mod build;
mod check;
mod cli;
mod debug;
mod migrate;
mod project;
mod routes;
mod sources;
mod test;
mod workspace;

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
        // The language server speaks JSON-RPC on stdin/stdout until `exit`.
        Command::Lsp => ExitCode::from(u8::try_from(k_lsp::run_stdio()).unwrap_or(1)),
        Command::Test(args) => test::run(&args),
        Command::Routes(args) => routes::run(&args),
        Command::Migrate { command } => match command {
            MigrateCommand::New(args) => migrate::new(&args),
            MigrateCommand::Apply(args) => migrate::apply(&args),
            MigrateCommand::Status(args) => migrate::status(&args),
        },
    }
}
