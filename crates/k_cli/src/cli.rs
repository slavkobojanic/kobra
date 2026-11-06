//! Command-line definitions.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// The K toolchain: compiler, test runner and development server.
#[derive(Debug, Parser)]
#[command(name = "k", version, about, propagate_version = true)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands of `k`.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check the project: syntax, types, layering and lockfiles. Fails on any problem.
    Check(CheckArgs),
    /// Build the server executable and the client assets into `dist/`.
    Build,
    /// Run the development server.
    Dev,
    /// Run the project's tests.
    Test,
    /// Print the route table.
    Routes,
    /// Manage database migrations.
    Migrate {
        /// The migration subcommand.
        #[command(subcommand)]
        command: MigrateCommand,
    },
    /// Inspect compiler internals (token streams, syntax trees).
    #[command(hide = true)]
    Debug(DebugArgs),
}

/// `k migrate` subcommands.
#[derive(Debug, Subcommand)]
pub enum MigrateCommand {
    /// Generate a migration from the difference between entities and `schema.lock`.
    New {
        /// A short name for the migration, such as `add-user-email`.
        name: String,
    },
    /// Apply pending migrations to the configured database.
    Apply,
    /// Show which migrations are applied and which are pending.
    Status,
}

/// How diagnostics are printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Human-readable text with source snippets.
    Human,
    /// One JSON object per diagnostic per line (JSON Lines).
    Json,
}

/// When to color human-readable output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    /// Color when standard error is a terminal.
    Auto,
    /// Always color.
    Always,
    /// Never color.
    Never,
}

/// Arguments of `k check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Files or directories to check. Directories are searched recursively for `.k` files.
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,
    /// Output format for diagnostics.
    #[arg(long, value_enum, default_value_t = Format::Human)]
    pub format: Format,
    /// When to use colors in human-readable output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

/// Arguments of `k debug`.
#[derive(Debug, Args)]
pub struct DebugArgs {
    /// What to print.
    #[arg(value_enum)]
    pub what: DebugWhat,
    /// The `.k` file to inspect.
    pub file: PathBuf,
}

/// What `k debug` prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum DebugWhat {
    /// The token stream, in `.tokens` format.
    Tokens,
    /// The syntax tree, in `.ast` format.
    Ast,
    /// The lexer and parser diagnostics, in `.diag` format.
    Diag,
}
