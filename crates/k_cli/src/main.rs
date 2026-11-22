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
mod archive;
mod build;
mod check;
mod cli;
mod debug;
#[cfg(feature = "server")]
mod devenv;
mod fix;
mod fmt;
mod index;
mod migrate;
mod new;
mod project;
mod routes;
mod sources;
mod test;
mod toolchain;
mod toolchains;
mod upgrade;
mod workspace;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command, MigrateCommand};

/// `k hash-password`: reads one line from standard input (the password,
/// its line ending removed) and prints its Argon2id hash as a PHC string,
/// with the parameters `k.crypto.Password` uses
/// (`docs/design/13-tooling.md`, "Commands"). Scripts generating an
/// admin password's hash use it instead of the `argon2` tool. An empty
/// password exits 2.
fn hash_password() -> ExitCode {
    let mut line = String::new();
    if let Err(e) = std::io::stdin().read_line(&mut line) {
        eprintln!("error: cannot read the password: {e}");
        return ExitCode::from(2);
    }
    let password = line.strip_suffix('\n').unwrap_or(&line);
    let password = password.strip_suffix('\r').unwrap_or(password);
    if password.is_empty() {
        eprintln!("error: the password is empty; write it on standard input");
        return ExitCode::from(2);
    }
    #[cfg(feature = "server")]
    match k_server::ops::password_hash(password) {
        Ok(hash) => {
            println!("{hash}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
    #[cfg(not(feature = "server"))]
    {
        eprintln!("error: this `k` was built without the server runtime (feature `server`)");
        ExitCode::from(2)
    }
}

/// `k keys new <kind>`: prints a new key of `kind` as a private JWK, for a
/// `Secret<K>` config field, and its public JWK, each after a comment
/// line naming the K type (`docs/design/33-crypto.md`, "Keys in
/// configuration"), so nobody needs an external tool to make a key.
fn keys_new(kind: cli::KeyKind) -> ExitCode {
    let (name, private, public) = match kind {
        cli::KeyKind::Ed25519 => ("ed25519", "Ed25519PrivateKey", "Ed25519PublicKey"),
        cli::KeyKind::X25519 => ("x25519", "X25519PrivateKey", "X25519PublicKey"),
        cli::KeyKind::P256Ecdsa => ("p256-ecdsa", "EcdsaP256PrivateKey", "EcdsaP256PublicKey"),
        cli::KeyKind::AesGcm => ("aes-gcm", "AesGcmKey", ""),
    };
    #[cfg(feature = "server")]
    match k_server::crypto::new_key_jwks(name) {
        Ok(Some((secret, public_jwk))) => {
            println!(
                "# The {private}, for a `Secret<{private}>` config field. Keep it out of git."
            );
            println!("{secret}");
            if let Some(p) = public_jwk {
                println!("# Its {public}, for an `{public}` field or for anyone who verifies.");
                println!("{p}");
            }
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("error: no key type `{name}`");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (name, private, public);
        eprintln!("error: this `k` was built without the server runtime (feature `server`)");
        ExitCode::from(2)
    }
}

fn main() -> ExitCode {
    // `dist/app` is this executable with an app appended: serve it.
    #[cfg(feature = "server")]
    if let Some(code) = app_mode::run_if_app() {
        return code;
    }
    // The project's own toolchain runs the command
    // (docs/design/48-releases-and-versioning.md, "Pinning a toolchain").
    if let Some(code) = toolchain::before_parse() {
        return code;
    }
    let cli = Cli::parse();
    if let Some(code) = toolchain::before_command(&cli) {
        return code;
    }
    match cli.command {
        Command::New(args) => new::run(&args),
        Command::Check(args) => check::run(&args),
        Command::Fix(args) => fix::run(&args),
        Command::Fmt(args) => fmt::run(&args),
        Command::Upgrade(args) => upgrade::run(&args),
        Command::Debug(args) => debug::run(&args),
        Command::Build(args) => build::build(&args),
        Command::Run(args) => build::run(&args),
        Command::Dev(args) => build::dev(&args),
        // The language server speaks JSON-RPC on stdin/stdout until `exit`.
        Command::Lsp { .. } => ExitCode::from(u8::try_from(k_lsp::run_stdio()).unwrap_or(1)),
        // The MCP server speaks JSON-RPC lines on stdin/stdout until the
        // input ends.
        Command::Mcp { root } => {
            let root = root.map(|r| std::path::absolute(&r).unwrap_or(r));
            ExitCode::from(u8::try_from(k_mcp::run_stdio(root)).unwrap_or(1))
        }
        Command::Test(args) => test::run(&args),
        Command::Routes(args) => routes::run(&args),
        Command::HashPassword => hash_password(),
        Command::Toolchain { command } => toolchains::run(&command),
        Command::Itself { command } => toolchains::run_self(&command),
        Command::Keys { command } => match command {
            cli::KeysCommand::New(args) => keys_new(args.kind),
        },
        Command::Migrate { command } => match command {
            MigrateCommand::New(args) => migrate::new(&args),
            MigrateCommand::Rename(args) => migrate::rename(&args),
            MigrateCommand::Apply(args) => migrate::apply(&args),
            MigrateCommand::Status(args) => migrate::status(&args),
        },
    }
}
