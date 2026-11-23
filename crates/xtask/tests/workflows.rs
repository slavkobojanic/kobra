//! The CI workflows (`.github/workflows/`, `docs/compiler/release.md` §9
//! and §12 step 13), checked without running them: they parse, every one
//! runs only when someone starts it (`workflow_dispatch`), the Pages and
//! image-pushing jobs wait for their own switches, the release workflow
//! creates its tag in one job after the gate and the builds, every command
//! exists, and every row of the gate in
//! `docs/design/48-releases-and-versioning.md` is run by the workflows its
//! columns name, so the gate and the pipeline can't drift.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use xtask::workflows::{
    Job, PAGES_SWITCH, PUBLISH_SWITCH, TRIGGER, Workflow, check, commands, job_commands, load,
    parse,
};

/// Every workflow file.
const WORKFLOWS: [&str; 4] = ["ci.yml", "nightly.yml", "publish.yml", "release.yml"];

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
fn the_workflows_exist_and_check_clean() {
    for name in WORKFLOWS {
        workflow(name);
    }
    let problems = check(&repo());
    assert!(problems.is_empty(), "{problems:#?}");
}

/// CI runs only on request (owner decision, 2026-11-23; design 48, "CI on
/// request"): from the Actions page or `gh workflow run`, never on a pull
/// request, a push, a tag, a published release or a schedule. Dispatching
/// is the switch, so the old `K_CI_ENABLED` variable is gone.
#[test]
fn every_workflow_runs_only_on_request() {
    for w in workflows().iter().filter(|w| !w.action) {
        assert_eq!(w.events, vec![TRIGGER.to_owned()], "{}", w.path);
    }
    for w in workflows() {
        let text = fs::read_to_string(repo().join(&w.path)).unwrap();
        assert!(
            !text.contains("K_CI_ENABLED"),
            "{} names K_CI_ENABLED",
            w.path
        );
        for job in &w.jobs {
            let c = job.condition.as_deref().unwrap_or_default();
            assert!(!c.contains("K_CI_ENABLED"), "{} `{}`: {c}", w.path, job.id);
        }
    }
}

/// The Pages deploys wait for `K_PAGES_ENABLED` (set since the repository
/// went public, design 48, "Public distribution"), and pushing toolchain
/// images to GHCR for `K_PUBLISH_ENABLED`.
#[test]
fn deploying_pages_and_pushing_images_wait_for_their_switches() {
    for w in workflows().iter().filter(|w| !w.action) {
        for job in &w.jobs {
            let at = format!("{} `{}`", w.path, job.id);
            let c = job.condition.as_deref().unwrap_or_default();
            let deploys = job.steps.iter().any(|s| {
                s.uses
                    .as_deref()
                    .is_some_and(|u| u.starts_with("actions/deploy-pages@"))
            });
            if deploys {
                assert!(c.contains(PAGES_SWITCH), "{at} deploys Pages: {c}");
            }
            for step in &job.steps {
                let run = step.run.as_deref().unwrap_or_default();
                if run.contains("docker push") || run.contains("imagetools create") {
                    let sc = step.condition.as_deref().unwrap_or_default();
                    assert!(
                        c.contains(PUBLISH_SWITCH) || sc.contains(PUBLISH_SWITCH),
                        "{at} pushes an image without {PUBLISH_SWITCH}: {run}"
                    );
                }
            }
        }
    }
}

fn job<'a>(w: &'a Workflow, id: &str) -> &'a Job {
    w.jobs
        .iter()
        .find(|j| j.id == id)
        .unwrap_or_else(|| panic!("{} has no job `{id}`", w.path))
}

/// Every job `id` waits for, directly or through others.
fn ancestors(w: &Workflow, id: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut todo = job(w, id).needs.clone();
    while let Some(n) = todo.pop() {
        if seen.insert(n.clone()) {
            todo.extend(job(w, &n).needs.iter().cloned());
        }
    }
    seen
}

/// `gh workflow run release.yml -f version=0.1.0`: the version is an
/// input, checked first (`cargo xtask ci check-version`: a version,
/// prepared on the dispatched branch, not already tagged), and every job
/// takes it from that check, never from the name of a pushed tag.
#[test]
fn the_release_is_started_with_a_version() {
    let w = workflow("release.yml");
    let version = w
        .inputs
        .iter()
        .find(|i| i.name == "version")
        .expect("release.yml has a `version` input");
    assert!(version.required, "the version is required");
    assert_eq!(version.kind, "string");
    let draft = w
        .inputs
        .iter()
        .find(|i| i.name == "draft")
        .expect("release.yml has a `draft` input");
    assert_eq!(draft.kind, "boolean");
    assert_eq!(draft.default.as_deref(), Some("true"), "a draft by default");

    let text = fs::read_to_string(repo().join(".github/workflows/release.yml")).unwrap();
    assert!(
        !text.contains("github.ref_name"),
        "release.yml reads a ref's name"
    );
    assert!(
        !text.contains("refs/tags/v'"),
        "release.yml waits for a tag push"
    );

    let check = job(&w, "check");
    let commands: Vec<String> = job_commands(check)
        .into_iter()
        .map(|(_, c)| c.join(" "))
        .collect();
    assert!(
        commands
            .iter()
            .any(|c| c.starts_with("cargo xtask ci check-version")),
        "{commands:#?}"
    );
    assert!(
        check
            .steps
            .iter()
            .any(|s| s.uses.as_deref() == Some("actions/checkout@v4")
                && s.with.contains(&("fetch-depth".to_owned(), "0".to_owned()))),
        "the check sees the tags (fetch-depth: 0)"
    );
    for j in w.jobs.iter().filter(|j| j.id != "check") {
        assert!(
            j.needs.iter().any(|n| n == "check"),
            "release.yml `{}` doesn't wait for (and read) the version check",
            j.id
        );
        // Every job checks out the dispatched commit, the one the tag goes on.
        for s in &j.steps {
            if s.uses
                .as_deref()
                .is_some_and(|u| u.starts_with("actions/checkout@"))
            {
                assert!(
                    !s.with.iter().any(|(k, _)| k == "ref"),
                    "release.yml `{}` checks out another commit",
                    j.id
                );
            }
        }
    }
}

/// The release workflow creates the annotated tag `v<version>` on the
/// gated commit and pushes it, in one job, the only one that may write the
/// repository's contents, and only once the gate, the builds, their smoke
/// tests and the signature check passed.
#[test]
fn the_release_job_alone_creates_the_tag_after_the_gate_and_the_builds() {
    let w = workflow("release.yml");
    let tagging: Vec<&Job> = w
        .jobs
        .iter()
        .filter(|j| {
            job_commands(j).iter().any(|(_, c)| {
                c[0] == "scripts/ci/tag.sh" || (c[0] == "git" && c.contains(&"tag".to_owned()))
            })
        })
        .collect();
    assert_eq!(
        tagging.iter().map(|j| j.id.as_str()).collect::<Vec<_>>(),
        ["release"],
        "exactly one job tags"
    );
    let release = tagging[0];
    for j in &w.jobs {
        let writes_contents = j.writes.iter().any(|g| g == "contents");
        assert_eq!(
            writes_contents,
            j.id == "release",
            "release.yml `{}`: only `release` may write contents",
            j.id
        );
    }
    let before = ancestors(&w, "release");
    for id in w
        .jobs
        .iter()
        .map(|j| j.id.as_str())
        .filter(|id| *id != "release")
    {
        assert!(before.contains(id), "`release` doesn't wait for `{id}`");
    }
    // A skipped job (no performance runner) doesn't stop it; a failed one does.
    let c = release.condition.as_deref().unwrap_or_default();
    assert!(
        c.contains("!cancelled()") && c.contains("!failure()"),
        "{c}"
    );

    // The tag before the release, which must find it.
    let commands: Vec<String> = job_commands(release)
        .into_iter()
        .map(|(_, c)| c.join(" "))
        .collect();
    let tag = commands
        .iter()
        .position(|c| c.starts_with("scripts/ci/tag.sh"))
        .expect("`release` runs scripts/ci/tag.sh");
    let create = commands
        .iter()
        .position(|c| c.starts_with("gh release create"))
        .expect("`release` runs `gh release create`");
    assert!(tag < create, "{commands:#?}");
    assert!(
        commands[create].contains("--verify-tag"),
        "{}",
        commands[create]
    );
    let script = fs::read_to_string(repo().join("scripts/ci/tag.sh")).unwrap();
    assert!(script.contains("tag -a"), "an annotated tag");
    assert!(
        script.contains("git push origin \"refs/tags/$tag\""),
        "pushed"
    );
}

/// `scripts/ci/tag.sh` against a local `origin`: it creates and pushes the
/// annotated tag, a re-run on the same commit changes nothing (a failed
/// release job can be re-run), and a tag on another commit is never moved.
#[cfg(unix)]
#[test]
fn the_tag_script_tags_once_and_never_moves_a_tag() {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("xtask_tag_script_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let origin = root.join("origin.git");
    let work = root.join("work");
    fs::create_dir_all(&work).unwrap();
    let git = |dir: &Path, args: &[&str]| {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    git(&root, &["init", "--bare", "-q", "origin.git"]);
    git(&work, &["init", "-q"]);
    git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&work, &["commit", "-q", "--allow-empty", "-m", "one"]);
    let first = git(&work, &["rev-parse", "HEAD"]);
    git(&work, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let second = git(&work, &["rev-parse", "HEAD"]);
    let tag = |sha: &str| {
        Command::new("sh")
            .arg(repo().join("scripts/ci/tag.sh"))
            .args(["v0.1.0", sha])
            .current_dir(&work)
            .output()
            .unwrap()
    };
    let out = tag(&first);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(git(&origin, &["rev-parse", "v0.1.0^{commit}"]), first);
    assert_eq!(
        git(&origin, &["cat-file", "-t", "v0.1.0"]),
        "tag",
        "annotated"
    );
    assert!(git(&origin, &["cat-file", "-p", "v0.1.0"]).contains("Kobra 0.1.0"));
    let again = tag(&first);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    let moved = tag(&second);
    assert!(!moved.status.success(), "the tag was moved");
    assert!(String::from_utf8_lossy(&moved.stderr).contains("never moved"));
    assert_eq!(git(&origin, &["rev-parse", "v0.1.0^{commit}"]), first);
    let _ = fs::remove_dir_all(&root);
}

/// `gh workflow run publish.yml -f version=0.1.0`, after `attach-local`:
/// publishes the draft (refusing one without the macOS archive unless
/// told), refreshes its `index.json` and, behind their switches, tags the
/// multi-arch image and deploys Pages.
#[test]
fn the_publish_workflow_publishes_an_existing_release() {
    let w = workflow("publish.yml");
    let version = w
        .inputs
        .iter()
        .find(|i| i.name == "version")
        .expect("publish.yml has a `version` input");
    assert!(version.required);
    let text = ran(&w);
    assert!(text.contains("cargo xtask ci check-version"), "{text}");
    assert!(text.contains("--tagged"), "{text}");
    assert!(text.contains("scripts/ci/publish-release.sh"), "{text}");
    assert!(text.contains("scripts/ci/release-index.sh"), "{text}");
    for j in &w.jobs {
        let writes_contents = j.writes.iter().any(|g| g == "contents");
        assert_eq!(writes_contents, j.id == "publish", "publish.yml `{}`", j.id);
    }
}

/// The docs site is GitHub Pages: `ci.yml` deploys it for a run of main
/// and `publish.yml` for each published release, both behind
/// `K_PAGES_ENABLED`, each building the whole versioned site with the
/// install scripts and the toolchain index at its root (the site action:
/// `cargo xtask docs build` copies `scripts/install.sh` and `install.ps1`
/// there, `release-index.sh` writes `index.json`), which the install
/// one-liners pipe and read.
#[test]
fn the_pages_jobs_deploy_the_site_with_the_install_scripts_and_the_index() {
    let site = workflows()
        .into_iter()
        .find(|w| w.path == ".github/actions/site/action.yml")
        .expect("the site action");
    let built: Vec<String> = site
        .jobs
        .iter()
        .flat_map(job_commands)
        .map(|(_, c)| c.join(" "))
        .collect();
    for expected in [
        "cargo xtask docs build --version latest --out target/site",
        "cargo xtask docs build --version next --from origin/main --out target/site",
        "scripts/ci/release-index.sh target/site/index.json",
    ] {
        assert!(
            built.iter().any(|c| c == expected),
            "{expected}: {built:#?}"
        );
    }
    for name in ["ci.yml", "publish.yml"] {
        let w = workflow(name);
        let pages: Vec<&Job> = w
            .jobs
            .iter()
            .filter(|j| {
                j.steps.iter().any(|s| {
                    s.uses
                        .as_deref()
                        .is_some_and(|u| u.starts_with("actions/deploy-pages@"))
                })
            })
            .collect();
        assert_eq!(pages.len(), 1, "{name} has one Pages deploy");
        let job = pages[0];
        let c = job.condition.as_deref().unwrap_or_default();
        assert!(c.contains(PAGES_SWITCH), "{name}: {c}");
        if name == "ci.yml" {
            assert!(c.contains("github.ref == 'refs/heads/main'"), "{name}: {c}");
        }
        let uses: Vec<&str> = job.steps.iter().filter_map(|s| s.uses.as_deref()).collect();
        let at = |prefix: &str| uses.iter().position(|u| u.starts_with(prefix));
        let (Some(build), Some(upload)) = (
            at("./.github/actions/site"),
            at("actions/upload-pages-artifact@"),
        ) else {
            panic!("{name}: the Pages job doesn't build and upload the site: {uses:?}");
        };
        assert!(build < upload, "{name}: {uses:?}");
        let upload = job
            .steps
            .iter()
            .find(|s| {
                s.uses
                    .as_deref()
                    .is_some_and(|u| u.starts_with("actions/upload-pages-artifact@"))
            })
            .unwrap();
        assert!(
            upload
                .with
                .iter()
                .any(|(k, v)| k == "path" && v == "target/site"),
            "{name}: {:?}",
            upload.with
        );
    }
}

#[test]
fn only_signing_releasing_images_and_publishing_may_write() {
    for w in workflows().iter().filter(|w| !w.action) {
        for job in &w.jobs {
            for grant in &job.writes {
                let allowed = match grant.as_str() {
                    "id-token" => {
                        job.id == "sign" || job.id.starts_with("publish") || job.id == "pages"
                    }
                    "contents" => job.id == "release" || job.id.starts_with("publish"),
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
        ("Kobra formatting", _) => &["fmt --check examples"],
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

/// The release gate runs each target over `fuzz/corpus/<target>`, which is
/// git-ignored: `fuzz/seed-corpus.sh` fills it from committed files, and
/// must seed every target (it fails when one is left empty), or a fresh
/// checkout, CI's, has nothing to run. The 0.1.0-rc.1 dry run, in a fresh
/// clone, found `lsp_edits` unseeded. (Running the script takes over a
/// minute, so this reads its target list.)
#[test]
fn the_seed_script_seeds_every_fuzz_target() {
    let script = fs::read_to_string(repo().join("fuzz/seed-corpus.sh")).unwrap();
    let seeded: Vec<&str> = script
        .lines()
        .find_map(|l| l.strip_prefix("all_targets="))
        .expect("an `all_targets=` line")
        .trim_matches('"')
        .split_whitespace()
        .collect();
    assert!(script.contains("for target in $all_targets; do\n    mkdir -p"));
    let toml = fs::read_to_string(repo().join("fuzz/Cargo.toml")).unwrap();
    let mut lines = toml.lines();
    let mut targets = Vec::new();
    while let Some(line) = lines.next() {
        if line.trim() == "[[bin]]" {
            targets.push(lines.next().unwrap().split('"').nth(1).unwrap().to_owned());
        }
    }
    let mut a: Vec<&str> = targets.iter().map(String::as_str).collect();
    let mut b = seeded.clone();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(
        a, b,
        "fuzz/Cargo.toml's targets and the script's `all_targets`"
    );
}

#[test]
fn the_checker_catches_what_it_should() {
    // A trigger other than `workflow_dispatch`, the old switch, an input
    // pasted into a script, an unpinned action, a missing script, an
    // unknown xtask command, a missing test module and a fuzz target that
    // doesn't exist are each reported.
    let bad = "on: push\njobs:\n  a:\n    if: vars.K_CI_ENABLED == 'true'\n    runs-on: x\n    steps:\n      - run: cargo xtask ci check-version ${{ inputs.version }}\n      - uses: actions/checkout@main\n      - run: |\n          scripts/no-such.sh\n          cargo xtask no-such-task\n          cargo test -p k_cli --test it -- no_such_module::\n          cargo +nightly fuzz run no_such_target\n          cargo +nightly fuzz run lex fuzz/corpus/lex -- -runs=0\n          curl https://example.invalid\n";
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
        "must run only on `workflow_dispatch`",
        "`K_CI_ENABLED` is gone",
        "pass `inputs.version` through `env:`",
        "must be pinned",
        "scripts/no-such.sh",
        "isn't a `cargo xtask` command",
        "no test module `no_such_module`",
        "no fuzz target `no_such_target`",
        "`fuzz/corpus/lex` is git-ignored",
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

/// No macOS runners (design 48, "Private distribution", kept when the
/// repository went public): no workflow runs on
/// macOS (the macOS archive is built and attached on the owner's Mac with
/// `cargo xtask release attach-local`), and `ci.yml` runs on Linux only.
#[test]
fn no_job_runs_on_macos_and_ci_runs_on_linux() {
    for name in WORKFLOWS {
        let text = fs::read_to_string(repo().join(".github/workflows").join(name)).unwrap();
        assert!(!text.contains("macos-"), "{name} names a macOS runner");
        assert!(
            !text.contains("apple-darwin"),
            "{name} builds a macOS target"
        );
    }
    let ci = fs::read_to_string(repo().join(".github/workflows/ci.yml")).unwrap();
    for line in ci
        .lines()
        .filter(|l| l.contains("runs-on:") || l.contains("os:"))
    {
        assert!(line.contains("ubuntu-"), "ci.yml: {line}");
    }
    let release = fs::read_to_string(repo().join(".github/workflows/release.yml")).unwrap();
    // Windows is left out until the suite passes there (design 48,
    // "Windows: deferred").
    assert!(
        !release.contains("windows-"),
        "the release builds no Windows target yet"
    );
    assert!(
        release.contains("x86_64-unknown-linux-gnu"),
        "the release builds Linux"
    );
}

/// Each release and nightly attaches its book and the whole toolchain
/// index (`k toolchain` reads the newest release's `index.json`, and the
/// install scripts when the docs site has none).
#[test]
fn releases_attach_the_book_and_the_index() {
    for name in ["release.yml", "nightly.yml"] {
        let text = ran(&workflow(name));
        assert!(
            text.contains("cargo xtask dist docs"),
            "{name}: no book archive"
        );
        assert!(
            text.contains("scripts/ci/release-index.sh"),
            "{name}: no index"
        );
        let create = text
            .lines()
            .find(|l| l.starts_with("gh release create"))
            .unwrap_or_else(|| panic!("{name}: no `gh release create`"));
        for file in [
            "target/dist/docs-",
            "target/dist/index.json",
            "target/dist/SHA256SUMS",
        ] {
            assert!(
                create.contains(file),
                "{name}: the release lacks {file}: {create}"
            );
        }
        assert!(
            !text.contains("K_DEFAULT_TOOLCHAIN_INDEX"),
            "{name}: a built-in index"
        );
    }
}

/// The JetBrains plugin (`editors/intellij`) is tested, verified and built
/// on Linux by `ci.yml`, and `release.yml` builds it the same way and
/// attaches it as `kobra-intellij-<version>.zip` next to the `.vsix`, in
/// the `release` job (still the only one that writes).
#[test]
fn the_jetbrains_plugin_is_tested_built_and_attached() {
    let gradle = |w: &Workflow, id: &str| {
        let j = job(w, id);
        let ran: Vec<(Option<String>, String)> = job_commands(j)
            .into_iter()
            .map(|(dir, c)| (dir, c.join(" ")))
            .collect();
        assert!(
            ran.iter()
                .any(|(dir, c)| dir.as_deref() == Some("editors/intellij")
                    && c.starts_with("./gradlew")
                    && ["test", "buildPlugin", "verifyPlugin"]
                        .iter()
                        .all(|t| c.split(' ').any(|w| w == *t))),
            "{} `{id}` doesn't run `./gradlew test buildPlugin verifyPlugin` in editors/intellij: {ran:#?}",
            w.path
        );
        assert!(
            j.steps.iter().any(|s| s
                .uses
                .as_deref()
                .is_some_and(|u| u.starts_with("actions/setup-java@"))),
            "{} `{id}` installs no Java",
            w.path
        );
        assert!(j.writes.is_empty(), "{} `{id}` may write", w.path);
    };
    gradle(&workflow("ci.yml"), "intellij");
    let release = workflow("release.yml");
    gradle(&release, "intellij");
    let commands: Vec<String> = job_commands(job(&release, "intellij"))
        .into_iter()
        .map(|(_, c)| c.join(" "))
        .collect();
    assert!(
        commands
            .iter()
            .any(|c| c.starts_with("cp ") && c.contains("target/dist/kobra-intellij-$VERSION.zip")),
        "{commands:#?}"
    );
    let text = ran(&release);
    let create = text
        .lines()
        .find(|l| l.starts_with("gh release create"))
        .unwrap();
    for file in ["target/dist/*.vsix", "target/dist/kobra-intellij-*.zip"] {
        assert!(create.contains(file), "the release lacks {file}: {create}");
    }
    assert!(
        job(&release, "release")
            .needs
            .iter()
            .any(|n| n == "intellij"),
        "`release` doesn't wait for the plugin"
    );
}
