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
//! cargo xtask docs build --version next     # the versioned site, into target/site
//! cargo xtask dist archive --target <triple> # a release archive; also checksums, index, smoke
//! cargo xtask ci changed --base <rev> <path>… # did these paths change (for the workflows)
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::Parser;

use xtask::cli::{CiStep, Cli, DistStep, DocsStep, ReleaseStep, Task};
use xtask::{Version, bump, dist, docs, fragment, gate, release, version};

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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let root = root();
    match cli.task {
        Task::Fragments { none_left } => {
            if none_left {
                let left = release::remaining_fragments(&root);
                if !left.is_empty() {
                    return failed(&[format!(
                        "no fragment may remain on a release: {} (run `cargo xtask release prepare <version>`)",
                        left.join(", ")
                    )]);
                }
            }
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
            ReleaseStep::Nightly { date } => {
                let date = date.unwrap_or_else(|| version::today().replace('-', ""));
                match release::nightly(&root, &date) {
                    Ok((v, written)) => {
                        for p in &written {
                            println!("wrote   {p}");
                        }
                        eprintln!("the version is now {v} (don't commit this)");
                        outputs(&[("version", v.to_string())])
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
                for step in release::gate() {
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
        Task::Docs { step } => docs_task(&root, step),
        Task::Dist { step } => dist_task(&root, step),
        Task::Ci { step } => match step {
            CiStep::Changed { base, paths } => {
                let changed = gate::changed_below(&root, &base, &paths);
                outputs(&[("changed", changed.to_string())])
            }
            CiStep::Version => match bump::current(&root) {
                Ok(v) => outputs(&[
                    ("version", v.to_string()),
                    ("minor", v.minor_text()),
                    ("prerelease", v.is_pre().to_string()),
                ]),
                Err(e) => failed(&[e]),
            },
            CiStep::Notes { version, out } => {
                let version = version.strip_prefix('v').unwrap_or(&version).to_owned();
                let changelog =
                    std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap_or_default();
                let Some(text) = release::notes(&changelog, &version) else {
                    return failed(&[format!(
                        "CHANGELOG.md has no `## {version} (…)` section: run `cargo xtask release prepare {version}`"
                    )]);
                };
                let out = if out.is_absolute() {
                    out
                } else {
                    root.join(out)
                };
                match std::fs::write(&out, text) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => failed(&[format!("{}: {e}", out.display())]),
                }
            }
        },
    }
}

/// Prints `name=value` lines and appends them to `$GITHUB_OUTPUT` when
/// set, for the workflow steps after this one.
fn outputs(pairs: &[(&str, String)]) -> ExitCode {
    use std::io::Write as _;
    let text: String = pairs.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    print!("{text}");
    if let Some(out) = std::env::var_os("GITHUB_OUTPUT") {
        let written = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(out)
            .and_then(|mut f| f.write_all(text.as_bytes()));
        if let Err(e) = written {
            return failed(&[format!("GITHUB_OUTPUT: {e}")]);
        }
    }
    ExitCode::SUCCESS
}

/// `cargo xtask docs …`.
fn docs_task(root: &Path, step: DocsStep) -> ExitCode {
    let DocsStep::Build {
        version,
        out,
        from,
        tags,
        releases,
    } = step;
    let Some(site_version) = docs::SiteVersion::parse(&version) else {
        eprintln!(
            "error: `--version {version}` is not `next`, `latest`, a minor (`0.6`) or a version"
        );
        return ExitCode::from(2);
    };
    let Some(mdbook) = docs::find_mdbook() else {
        return failed(&[
            "mdbook isn't installed: `cargo install mdbook --locked`, or set MDBOOK".to_owned(),
        ]);
    };
    let out = if out.is_absolute() {
        out
    } else {
        root.join(out)
    };
    let options = docs::Options {
        root: root.to_owned(),
        book: None,
        version: site_version,
        from,
        out: out.clone(),
        tags: tags.map(|t| t.into_iter().filter(|s| !s.is_empty()).collect()),
        releases,
        mdbook,
    };
    match docs::build(&options) {
        Ok(built) => {
            match &built.dir {
                Some(dir) => eprintln!(
                    "built {}/{dir}/ ({} chapters); live: {}",
                    out.display(),
                    built.pages,
                    if built.versions.live.is_empty() {
                        "none yet".to_owned()
                    } else {
                        built.versions.live.join(", ")
                    }
                ),
                None => eprintln!(
                    "nothing is released yet: only the root files of {} were written",
                    out.display()
                ),
            }
            ExitCode::SUCCESS
        }
        Err(problems) => failed(&problems),
    }
}

/// `cargo xtask dist …`.
fn dist_task(root: &Path, step: DistStep) -> ExitCode {
    let current = match bump::current(root) {
        Ok(v) => v,
        Err(e) => return failed(&[e]),
    };
    let rooted = |p: PathBuf| if p.is_absolute() { p } else { root.join(p) };
    let result = match step {
        DistStep::Archive {
            target,
            binary,
            out,
        } => {
            let exe = if target.contains("windows") {
                "k.exe"
            } else {
                "k"
            };
            let binary = binary.map_or_else(
                || root.join("target").join(&target).join("release").join(exe),
                rooted,
            );
            dist::archive(root, &current, &target, &binary, &rooted(out))
                .map(|p| println!("wrote   {}", p.display()))
        }
        DistStep::Checksums { dir } => dist::checksums(&rooted(dir)).map(|t| print!("{t}")),
        DistStep::Index {
            dir,
            index,
            download,
            date,
        } => {
            let date = date.unwrap_or_else(version::today);
            dist::add_release(&rooted(dir), &rooted(index), &download, &date).map(|_| ())
        }
        DistStep::MergeIndex { dir, out } => dist::merge(&rooted(dir), &rooted(out)).map(|_| ()),
        DistStep::Smoke { k, port } => {
            dist::smoke(&rooted(k), port).map(|()| eprintln!("the smoke test passed"))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => failed(&[e]),
    }
}
