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
    /// Create an app in a new directory `<name>/`, ready for `k dev`.
    New(NewArgs),
    /// Check the project: syntax, types, layering, lockfiles and tests. Fails on any problem.
    Check(CheckArgs),
    /// Apply the machine-applicable fixes of the project's diagnostics, then report what remains.
    Fix(FixArgs),
    /// Build the server executable and the client assets into `dist/`.
    Build(BuildArgs),
    /// Build for development and serve the app (no rebuild on change).
    Run(RunArgs),
    /// Run the development server: like `run`, rebuilding and restarting on change (no HMR).
    Dev(RunArgs),
    /// Run the project's tests; `--watch` re-runs the affected ones on every save.
    Test(TestArgs),
    /// Print the route table: pages, endpoint pages and redirects.
    Routes(ProjectArgs),
    /// Run the language server on standard input and output (for editors).
    Lsp {
        /// Accepted and ignored: stdio is the only transport, but many LSP
        /// clients (vscode-languageclient among them) pass `--stdio`.
        #[arg(long, hide = true)]
        stdio: bool,
    },
    /// Hash the password read from standard input with Argon2id, printing its PHC string.
    HashPassword,
    /// Make keys for `k.crypto`'s key types.
    Keys {
        /// The keys subcommand.
        #[command(subcommand)]
        command: KeysCommand,
    },
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

/// `k keys` subcommands.
#[derive(Debug, Subcommand)]
pub enum KeysCommand {
    /// Print a new private key as a JWK for the environment, and its public JWK.
    New(KeysNewArgs),
}

/// Arguments of `k keys new`.
#[derive(Debug, Args)]
pub struct KeysNewArgs {
    /// The key type.
    #[arg(value_enum)]
    pub kind: KeyKind,
}

/// The key types `k keys new` makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum KeyKind {
    /// An Ed25519 signing key (`Ed25519PrivateKey`).
    Ed25519,
    /// An X25519 key-agreement key (`X25519PrivateKey`).
    X25519,
    /// An ECDSA P-256 signing key (`EcdsaP256PrivateKey`).
    #[value(name = "p256-ecdsa")]
    P256Ecdsa,
    /// An AES-256-GCM key (`AesGcmKey`).
    #[value(name = "aes-gcm")]
    AesGcm,
}

/// `k migrate` subcommands.
#[derive(Debug, Subcommand)]
pub enum MigrateCommand {
    /// Write the next migration from the difference between the entities and `schema.lock`.
    New(MigrateNewArgs),
    /// Record in `schema.lock` that an entity or field was renamed, keeping its table or column.
    Rename(MigrateRenameArgs),
    /// Apply pending migrations to the development database (`DATABASE_URL`).
    Apply(ProjectArgs),
    /// Show which migrations the development database has applied and which are pending.
    Status(ProjectArgs),
}

/// Arguments of `k new`.
#[derive(Debug, Args)]
pub struct NewArgs {
    /// The project name, and the directory to create: lower case letters,
    /// digits and hyphens, starting with a letter (`my-app`).
    pub name: String,
}

/// Arguments of commands that work on one project.
#[derive(Debug, Args)]
pub struct ProjectArgs {
    /// The project directory (the one with `k.toml`).
    #[arg(default_value = ".")]
    pub project: PathBuf,
}

/// Arguments of `k migrate new`.
#[derive(Debug, Args)]
pub struct MigrateNewArgs {
    /// What the migration does, such as "add note pinning".
    pub description: String,
    /// The project directory (the one with `k.toml`).
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
    /// Write an empty migration (for data changes) instead of the schema difference.
    #[arg(long, conflicts_with_all = ["drop_retired", "rename"])]
    pub empty: bool,
    /// Drop the tables and columns that earlier migrations retired.
    #[arg(long, conflicts_with = "rename")]
    pub drop_retired: bool,
    /// Rename the table of an entity (`notes.Note`) or the column of a field
    /// (`notes.Note.heading`) to follow its current name; repeatable.
    #[arg(long, value_name = "NAME")]
    pub rename: Vec<String>,
}

/// Arguments of `k migrate rename`.
#[derive(Debug, Args)]
pub struct MigrateRenameArgs {
    /// The name `schema.lock` records: `notes.Note` or `notes.Note.title`.
    pub old: String,
    /// The new name: `notes.Memo`, or the field's new name (`heading`).
    pub new: String,
    /// The project directory (the one with `k.toml`).
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
}

/// Arguments of `k test`.
#[derive(Debug, Args)]
pub struct TestArgs {
    /// The project directory (the one with `k.toml`).
    #[arg(default_value = ".")]
    pub project: PathBuf,
    /// Run only the tests whose full name (`Group › description`) contains this text; repeatable.
    #[arg(long, short = 'f')]
    pub filter: Vec<String>,
    /// Keep running: re-run the affected tests whenever a file of the project changes.
    #[arg(long)]
    pub watch: bool,
    /// When to use colors.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
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
    /// Don't run the project's tests (they run by default, for a program without errors).
    #[arg(long)]
    pub no_tests: bool,
}

/// Arguments of `k fix`.
#[derive(Debug, Args)]
pub struct FixArgs {
    /// Files or directories whose projects to fix, as for `k check`.
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,
    /// Apply only the fixes of this diagnostic code (`K2012`); repeatable.
    #[arg(long, value_name = "CODE")]
    pub code: Vec<String>,
    /// Print the changes as a unified diff instead of writing them.
    #[arg(long)]
    pub dry_run: bool,
    /// Output format for the diagnostics that remain.
    #[arg(long, value_enum, default_value_t = Format::Human)]
    pub format: Format,
    /// When to use colors in human-readable output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

/// Arguments of `k build`.
#[derive(Debug, Args)]
pub struct BuildArgs {
    /// The project directory (the one with `k.toml`).
    #[arg(default_value = ".")]
    pub project: PathBuf,
    /// The output directory; `dist` inside the project by default.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// Arguments of `k run` and `k dev`.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// The project directory (the one with `k.toml`).
    #[arg(default_value = ".")]
    pub project: PathBuf,
    /// The port to listen on.
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
    /// Generate the client as `k build` does (minified, tree-shaken, with
    /// short class names and source maps) while keeping the development
    /// wiring, to check the shipped code locally.
    #[arg(long)]
    pub release: bool,
    /// Serve V8's inspector for the server's K code on this port (9229
    /// when no port is given), for Chrome DevTools (`chrome://inspect`) and
    /// VS Code; the server then runs one isolate. At a workspace root,
    /// the first app gets it.
    #[arg(long, value_name = "PORT", num_args = 0..=1, default_missing_value = "9229")]
    pub inspect: Option<u16>,
}

/// Arguments of `k debug`.
#[derive(Debug, Args)]
pub struct DebugArgs {
    /// What to print.
    #[arg(value_enum)]
    pub what: DebugWhat,
    /// The `.k` file to inspect (for `sema`, also a project directory; for `codegen`, a project directory).
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
    /// Every diagnostic of a `sema` corpus case (a `.k` file or a project directory), in `.diag` format.
    Sema,
    /// The generated files of a project directory (development profile), server modules first.
    Codegen,
}
