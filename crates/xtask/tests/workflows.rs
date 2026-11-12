//! The CI workflows (`.github/workflows/`, `docs/compiler/release.md` §9
//! and §12 step 13), checked without running them: they parse, every job
//! is off until the owner sets `K_CI_ENABLED` (and the Pages and publish
//! jobs their own switches), every command exists, and every row of the
//! gate in `docs/design/48-releases-and-versioning.md` is run by the
//! workflows its columns name, so the gate and the pipeline can't drift.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::workflows::{
    CI_SWITCH, PAGES_SWITCH, PUBLISH_SWITCH, Workflow, check, commands, job_commands, load, parse,
};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn workflows() -> Vec<Workflow> {
    load(&repo()).unwrap_or_else(|p| panic!("{p:#?}"))
}

fn workflow(name: &str) -> Workflow {
    workflows()
        .into_iter()
        .find(|w| w.path == format!(".github/workflows/{name}"))
        .unwrap_or_else(|| panic!("no .github/workflows/{name}"))
}

/// Everything a workflow runs, as one text: each command, each
/// environment variable and service, each action.
fn ran(w: &Workflow) -> String {
    let mut out = String::new();
    for job in &w.jobs {
        for (_, c) in job_commands(job) {
            out.push_str(&c.join(" "));
            out.push('\n');
        }
        for (k, v) in &job.env {
            out.push_str(&format!("{k}={v}\n"));
        }
        for s in &job.steps {
            if let Some(u) = &s.uses {
                out.push_str(u);
                out.push('\n');
            }
        }
    }
    // The local actions' steps run where they are used.
    for a in workflows().iter().filter(|a| a.action) {
        let name = a.path.trim_end_matches("/action.yml");
        if w.jobs
            .iter()
            .flat_map(|j| &j.steps)
            .any(|s| s.uses.as_deref() == Some(&format!("./{name}")))
        {
            for job in &a.jobs {
                for (_, c) in job_commands(job) {
                    out.push_str(&c.join(" "));
                    out.push('\n');
                }
            }
        }
    }
    out
}

#[test]
fn the_three_workflows_exist_and_check_clean() {
    for name in ["ci.yml", "nightly.yml", "release.yml"] {
        workflow(name);
    }
    let problems = check(&repo());
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_job_is_off_until_the_owner_switches_it_on() {
    for w in workflows().iter().filter(|w| !w.action) {
        for job in &w.jobs {
            let at = format!("{} `{}`", w.path, job.id);
            let c = job.condition.as_deref().unwrap_or_default();
            assert!(c.starts_with(CI_SWITCH), "{at}: {c}");
            let deploys = job.steps.iter().any(|s| {
                s.uses
                    .as_deref()
                    .is_some_and(|u| u.starts_with("actions/deploy-pages@"))
            });
            if deploys {
                assert!(c.contains(PAGES_SWITCH), "{at} deploys Pages: {c}");
            }
            if job.id.starts_with("publish") {
                assert!(c.contains(PUBLISH_SWITCH), "{at} publishes: {c}");
            }
        }
    }
    // Release jobs run for a pushed `v*` tag only (or its published draft).
    for job in &workflow("release.yml").jobs {
        let c = job.condition.as_deref().unwrap_or_default();
        assert!(
            c.contains("startsWith(github.ref, 'refs/tags/v')")
                || c.contains("github.event_name == 'release'"),
            "release.yml `{}`: {c}",
            job.id
        );
    }
}

#[test]
fn the_pages_jobs_exist() {
    for name in ["ci.yml", "release.yml"] {
        let w = workflow(name);
        assert!(
            w.jobs.iter().any(|j| j.steps.iter().any(|s| s
                .uses
                .as_deref()
                .is_some_and(|u| u.starts_with("actions/deploy-pages@")))),
            "{name} has no Pages deploy"
        );
    }
}

#[test]
fn only_signing_drafting_images_and_publishing_may_write() {
    for w in workflows().iter().filter(|w| !w.action) {
        for job in &w.jobs {
            for grant in &job.writes {
                let allowed = match grant.as_str() {
                    "id-token" => {
                        job.id == "sign" || job.id.starts_with("publish") || job.id == "pages"
                    }
                    "contents" => job.id == "draft" || job.id.starts_with("publish"),
                    "packages" => job.id == "image" || job.id.starts_with("publish"),
                    "pages" => job.id == "pages" || job.id.starts_with("publish"),
                    "attestations" => job.id == "sign",
                    "issues" => job.id == "report",
                    _ => false,
                };
                assert!(allowed, "{} `{}` may write `{grant}`", w.path, job.id);
            }
        }
    }
}

/// What each gate row of design 48 runs, by its "Check" column: the text a
/// workflow must contain where the row's column says it runs. A row added
/// to the gate fails `every_gate_row_is_in_the_pipeline` until it is
/// listed here and run.
fn needles(check: &str, column: &str) -> Option<&'static [&'static str]> {
    Some(match (check, column) {
        ("formatting and lints", _) => &[
            "cargo fmt --all --check",
            "cargo clippy --workspace --all-targets",
        ],
        (c, _) if c.starts_with("full test suite") => &["cargo test --workspace"],
        ("on Postgres", _) => &["K_TEST_POSTGRES_URL=", "service:postgres"],
        ("real Chrome", _) => &["K_REQUIRE_CHROME=1"],
        ("long property runs", _) => &["PROPTEST_CASES="],
        ("fuzzing", "release") => &["cargo +nightly fuzz run", "-runs=0"],
        ("fuzzing", _) => &["cargo +nightly fuzz run", "-max_total_time=600"],
        ("performance budgets", _) => &["scripts/bench-check.sh"],
        ("the book builds with no warnings", _) => &["cargo xtask docs build"],
        ("docs freshness", _) => &["cargo test --workspace"],
        ("docs ownership", _) => &["cargo xtask docs-gate"],
        ("the versioned site builds", _) => &["cargo xtask docs build"],
        // On pull requests, inside the full suite.
        ("compatibility fixtures", "pr") => &["cargo test --workspace"],
        ("compatibility fixtures", _) => &["compat::"],
        ("version consistency, changelog fragments", "release") => {
            &["cargo xtask fragments --none-left", "version::"]
        }
        ("version consistency, changelog fragments", _) => {
            &["cargo test -p xtask", "cargo xtask docs-gate"]
        }
        ("install scripts", _) => &["install_script::"],
        ("artifacts' smoke test", _) => &["cargo xtask dist smoke"],
        ("toolchain image", _) => &["docker/toolchain.Dockerfile", "examples/shop/Dockerfile"],
        _ => return None,
    })
}

#[test]
fn every_gate_row_is_in_the_pipeline() {
    let design =
        fs::read_to_string(repo().join("docs/design/48-releases-and-versioning.md")).unwrap();
    let start = design
        .find("### The gate")
        .expect("design 48 has a gate section");
    let table: Vec<&str> = design[start..]
        .lines()
        .skip_while(|l| !l.starts_with('|'))
        .take_while(|l| l.starts_with('|'))
        .skip(2)
        .collect();
    assert!(table.len() >= 15, "{table:#?}");
    let columns = [
        ("ci.yml", "pr", 2),
        ("nightly.yml", "nightly", 3),
        ("release.yml", "release", 4),
    ];
    let texts: Vec<String> = columns.iter().map(|(f, _, _)| ran(&workflow(f))).collect();
    let mut missing = Vec::new();
    for row in table {
        let cells: Vec<&str> = row.trim_matches('|').split('|').map(str::trim).collect();
        let check = cells[0];
        for ((file, column, at), text) in columns.iter().zip(&texts) {
            let cell = cells.get(*at).copied().unwrap_or_default();
            if cell == "no" || cell == "—" || cell.is_empty() {
                continue;
            }
            let Some(list) = needles(check, column) else {
                missing.push(format!(
                    "gate row `{check}` isn't mapped in tests/workflows.rs `needles`"
                ));
                break;
            };
            for needle in list {
                if !text.contains(needle) {
                    missing.push(format!(
                        "gate row `{check}` ({column}: {cell}): {file} doesn't run `{needle}`"
                    ));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn every_fuzz_target_runs_nightly() {
    let toml = fs::read_to_string(repo().join("fuzz/Cargo.toml")).unwrap();
    let nightly = ran(&workflow("nightly.yml"));
    let release = ran(&workflow("release.yml"));
    let mut lines = toml.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "[[bin]]" {
            let name = lines.next().unwrap().split('"').nth(1).unwrap();
            assert!(
                nightly.contains(&format!("fuzz run {name} ")),
                "nightly: {name}"
            );
            assert!(
                release.contains(&format!("fuzz run {name} fuzz/corpus/{name}")),
                "release: {name}"
            );
        }
    }
}

#[test]
fn the_checker_catches_what_it_should() {
    // A job without the switch, an unpinned action, a missing script, an
    // unknown xtask command, a missing test module and a fuzz target that
    // doesn't exist are each reported.
    let bad = "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@main\n      - run: |\n          scripts/no-such.sh\n          cargo xtask no-such-task\n          cargo test -p k_cli --test it -- no_such_module::\n          cargo +nightly fuzz run no_such_target\n          curl https://example.invalid\n";
    let root = std::env::temp_dir().join(format!("xtask_workflows_bad_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".github/workflows")).unwrap();
    fs::write(root.join(".github/workflows/bad.yml"), bad).unwrap();
    // The crates and fuzz targets are the real repository's.
    for (from, to) in [
        ("crates/k_cli/Cargo.toml", "crates/k_cli/Cargo.toml"),
        ("fuzz/Cargo.toml", "fuzz/Cargo.toml"),
    ] {
        fs::create_dir_all(root.join(to).parent().unwrap()).unwrap();
        fs::copy(repo().join(from), root.join(to)).unwrap();
    }
    let problems = check(&root).join("\n");
    let _ = fs::remove_dir_all(&root);
    for expected in [
        "must start with `vars.K_CI_ENABLED == 'true'`",
        "must be pinned",
        "scripts/no-such.sh",
        "isn't a `cargo xtask` command",
        "no test module `no_such_module`",
        "no fuzz target `no_such_target`",
        "`curl` is neither",
    ] {
        assert!(problems.contains(expected), "{expected}:\n{problems}");
    }
    assert!(parse("x.yml", "jobs: [").is_err());
}

#[test]
fn commands_are_split_as_the_shell_splits_them() {
    assert_eq!(
        commands(
            "FOO=1 cargo test -p a \\\n  --test it -- m:: && scripts/x.sh # c\n# only a comment\necho 'a b'"
        ),
        vec![
            vec!["cargo", "test", "-p", "a", "--test", "it", "--", "m::"],
            vec!["scripts/x.sh"],
            vec!["echo", "a b"],
        ]
    );
}
