//! `cargo xtask`: the repository's release automation
//! (`docs/compiler/release.md`). Run from anywhere in the repository; it
//! works on the repository it was built from.
//!
//! ```text
//! cargo xtask release prepare 0.2.0-rc.1   # bump, compile the fragments, check the versions
//! cargo xtask release check                # the local gate
//! cargo xtask release start-next 0.3.0     # main's next minor
//! cargo xtask docs-gate origin/main        # each fragment's book pages are in the diff
//! cargo xtask fragments                    # check every fragment
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::{Parser, Subcommand};

use xtask::{Version, fragment, gate, release, version};

/// The K repository's release automation.
#[derive(Debug, Parser)]
#[command(name = "cargo xtask")]
struct Cli {
    /// The task.
    #[command(subcommand)]
    task: Task,
}

/// The tasks.
#[derive(Debug, Subcommand)]
enum Task {
    /// Cutting a release.
    Release {
        /// The release step.
        #[command(subcommand)]
        step: ReleaseStep,
    },
    /// Check that each user-visible fragment added since BASE lists book pages the diff changes.
    DocsGate {
        /// The commit the change is based on (`origin/main`).
        base: String,
    },
    /// Check every changelog fragment and list them.
    Fragments,
}

/// `release` steps.
#[derive(Debug, Subcommand)]
enum ReleaseStep {
    /// Set VERSION everywhere and compile the fragments into CHANGELOG.md, the changelog page and the upgrade guide.
    Prepare {
        /// The version: `0.2.0-rc.1` (fragments kept) or `0.2.0` (fragments deleted).
        version: String,
        /// The release date, `YYYY-MM-DD`; today (UTC) by default.
        #[arg(long)]
        date: Option<String>,
        /// Don't run the version consistency test afterwards.
        #[arg(long)]
        no_verify: bool,
    },
    /// Run the local gate: formatting, lints, the tests, real Chrome, the book.
    Check {
        /// Print the commands instead of running them.
        #[arg(long)]
        list: bool,
        /// Also require that no fragment remains, as on a release tag.
        #[arg(long)]
        release: bool,
    },
    /// Set main's version to the next minor, leaving the fragments as they are.
    StartNext {
        /// The next minor's version, `0.3.0`.
        version: String,
    },
}

/// The repository root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Parses a version argument.
fn version_arg(text: &str) -> Result<Version, ExitCode> {
    Version::parse(text).ok_or_else(|| {
        eprintln!("error: `{text}` is not a version such as `0.2.0` or `0.2.0-rc.1`");
        ExitCode::from(2)
    })
}

/// Prints `problems` and returns exit status 1.
fn failed(problems: &[String]) -> ExitCode {
    for p in problems {
        eprintln!("error: {p}");
    }
    ExitCode::from(1)
}

/// Runs a command in the repository, printing it first.
fn run(env: &[(&str, &str)], command: &[String]) -> bool {
    let shown: Vec<String> = env
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .chain(command.iter().cloned())
        .collect();
    eprintln!("$ {}", shown.join(" "));
    let Some((program, args)) = command.split_first() else {
        return false;
    };
    Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .current_dir(root())
        .status()
        .is_ok_and(|s| s.success())
}

/// `mdbook` on the `PATH`, else in `~/.cargo/bin`.
fn mdbook() -> String {
    let in_cargo = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".cargo/bin/mdbook"))
        .filter(|p| p.is_file());
    let on_path = Command::new("mdbook").arg("--version").output().is_ok();
    match (on_path, in_cargo) {
        (false, Some(p)) => p.to_string_lossy().into_owned(),
        _ => "mdbook".to_owned(),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let root = root();
    match cli.task {
        Task::Fragments => {
            let (fragments, problems) = fragment::load(&root);
            for f in &fragments {
                println!("{:<12} changes/{}.md", f.kind.name(), f.slug);
            }
            if problems.is_empty() {
                eprintln!("{} fragments, all valid", fragments.len());
                ExitCode::SUCCESS
            } else {
                failed(&problems)
            }
        }
        Task::DocsGate { base } => match gate::run(&root, &base) {
            Ok(()) => {
                eprintln!("docs gate: every fragment's book pages are in the diff");
                ExitCode::SUCCESS
            }
            Err(problems) => failed(&problems),
        },
        Task::Release { step } => match step {
            ReleaseStep::Prepare {
                version: text,
                date,
                no_verify,
            } => {
                let v = match version_arg(&text) {
                    Ok(v) => v,
                    Err(code) => return code,
                };
                let date = date.unwrap_or_else(version::today);
                if !version::is_date(&date) {
                    eprintln!("error: `--date {date}` is not a `YYYY-MM-DD` date");
                    return ExitCode::from(2);
                }
                match release::prepare(&root, &v, &date) {
                    Ok(done) => {
                        for w in &done.warnings {
                            eprintln!("warning: {w}");
                        }
                        for p in &done.written {
                            println!("wrote   {p}");
                        }
                        for p in &done.deleted {
                            println!("deleted {p}");
                        }
                        if v.is_pre() {
                            eprintln!(
                                "prepared {v}: review CHANGELOG.md's draft, fix wording in the fragments, write changes/_highlights.md"
                            );
                        } else {
                            eprintln!(
                                "prepared {v}: run `cargo xtask release check`, then commit and tag v{v}"
                            );
                        }
                        if !no_verify
                            && !run(
                                &[],
                                &[
                                    "cargo",
                                    "test",
                                    "-p",
                                    "k_cli",
                                    "--test",
                                    "it",
                                    "--",
                                    "version::",
                                ]
                                .map(str::to_owned),
                            )
                        {
                            eprintln!("error: the versions disagree; see the test output above");
                            return ExitCode::from(1);
                        }
                        ExitCode::SUCCESS
                    }
                    Err(problems) => failed(&problems),
                }
            }
            ReleaseStep::StartNext { version: text } => {
                let v = match version_arg(&text) {
                    Ok(v) => v,
                    Err(code) => return code,
                };
                match release::start_next(&root, &v) {
                    Ok(written) => {
                        for p in &written {
                            println!("wrote   {p}");
                        }
                        ExitCode::SUCCESS
                    }
                    Err(problems) => failed(&problems),
                }
            }
            ReleaseStep::Check { list, release: tag } => {
                if tag {
                    let left = release::remaining_fragments(&root);
                    if !left.is_empty() {
                        return failed(&[format!(
                            "no fragment may remain on a release: {} (run `cargo xtask release prepare <version>`)",
                            left.join(", ")
                        )]);
                    }
                }
                let (_, problems) = fragment::load(&root);
                if !problems.is_empty() {
                    return failed(&problems);
                }
                for step in release::gate(&mdbook()) {
                    if list {
                        let env: Vec<String> =
                            step.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
                        let words: Vec<String> =
                            env.into_iter().chain(step.command.clone()).collect();
                        println!("{}  # {}", words.join(" "), step.what);
                        continue;
                    }
                    if !run(&step.env, &step.command) {
                        eprintln!("error: the gate failed: {}", step.what);
                        return ExitCode::from(1);
                    }
                }
                if !list {
                    eprintln!("the local gate passed");
                }
                ExitCode::SUCCESS
            }
        },
    }
}
