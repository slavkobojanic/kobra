//! The CI workflows in `.github/` (`docs/compiler/release.md` §9), read
//! back so a test can check them without running them: every workflow
//! runs only when someone starts it ([`TRIGGER`]), no script pastes an
//! input in, every action is pinned, and every
//! command a step runs exists in this repository: `cargo xtask …` parses
//! with this crate's command line, `cargo test -p <crate> --test <binary>
//! -- <module>::` names a real crate, binary and module, a fuzz target is
//! in `fuzz/Cargo.toml`, and a script or file named is there.

use std::fs;
use std::path::Path;

use clap::Parser as _;
use yaml_rust2::{Yaml, YamlLoader};

use crate::cli::Cli;

/// The one event every workflow runs on: started by hand, from the
/// Actions page or `gh workflow run` (design 48, "CI on request").
pub const TRIGGER: &str = "workflow_dispatch";
/// The switch of the jobs that deploy the docs to GitHub Pages (a
/// repository variable, set since the repository went public).
pub const PAGES_SWITCH: &str = "vars.K_PAGES_ENABLED == 'true'";
/// The switch of the steps and jobs that push toolchain images to GHCR.
pub const PUBLISH_SWITCH: &str = "vars.K_PUBLISH_ENABLED == 'true'";

/// One workflow file, or a local composite action.
#[derive(Clone, Debug)]
pub struct Workflow {
    /// Its path from the repository root.
    pub path: String,
    /// Its jobs (a composite action is one job, `steps`).
    pub jobs: Vec<Job>,
    /// The events it runs on (the `on:` keys).
    pub events: Vec<String>,
    /// The inputs of its `workflow_dispatch`.
    pub inputs: Vec<Input>,
    /// `true` for a composite action.
    pub action: bool,
}

/// One input of a `workflow_dispatch` (`gh workflow run … -f name=value`).
#[derive(Clone, Debug)]
pub struct Input {
    /// Its name.
    pub name: String,
    /// Its `required:`.
    pub required: bool,
    /// Its `type:` (`string` when absent).
    pub kind: String,
    /// Its `default:`.
    pub default: Option<String>,
}

/// One job.
#[derive(Clone, Debug)]
pub struct Job {
    /// Its id.
    pub id: String,
    /// Its `if:`.
    pub condition: Option<String>,
    /// The jobs it needs.
    pub needs: Vec<String>,
    /// Its environment (`env:` of the workflow, the job and its steps, and
    /// its services' names).
    pub env: Vec<(String, String)>,
    /// Its steps.
    pub steps: Vec<Step>,
    /// Each combination of its matrix (empty without one).
    pub matrix: Vec<Vec<(String, String)>>,
    /// Its `permissions:` that grant `write`.
    pub writes: Vec<String>,
}

/// One step.
#[derive(Clone, Debug)]
pub struct Step {
    /// Its `if:`.
    pub condition: Option<String>,
    /// Its `uses:`.
    pub uses: Option<String>,
    /// Its `run:`.
    pub run: Option<String>,
    /// Its `working-directory:`.
    pub dir: Option<String>,
    /// Its `with:`.
    pub with: Vec<(String, String)>,
}

fn text(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Real(r) => Some(r.clone()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn pairs(y: &Yaml) -> Vec<(String, String)> {
    y.as_hash()
        .map(|h| {
            h.iter()
                .filter_map(|(k, v)| Some((text(k)?, text(v).unwrap_or_default())))
                .collect()
        })
        .unwrap_or_default()
}

fn list(y: &Yaml) -> Vec<String> {
    match y {
        Yaml::Array(a) => a.iter().filter_map(text).collect(),
        other => text(other).into_iter().collect(),
    }
}

fn steps(y: &Yaml) -> Vec<Step> {
    y.as_vec()
        .map(|v| {
            v.iter()
                .map(|s| Step {
                    condition: text(&s["if"]),
                    uses: text(&s["uses"]),
                    run: text(&s["run"]),
                    dir: text(&s["working-directory"]),
                    with: pairs(&s["with"]),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The combinations of a `strategy.matrix`: its `include` entries, or the
/// product of its lists.
fn matrix(y: &Yaml) -> Vec<Vec<(String, String)>> {
    let Some(h) = y.as_hash() else {
        return Vec::new();
    };
    let include: Vec<Vec<(String, String)>> = y["include"]
        .as_vec()
        .map(|v| v.iter().map(pairs).collect())
        .unwrap_or_default();
    let mut combos: Vec<Vec<(String, String)>> = vec![Vec::new()];
    for (k, v) in h {
        let Some(key) = text(k) else { continue };
        if key == "include" || key == "exclude" {
            continue;
        }
        let values = list(v);
        let mut next = Vec::new();
        for c in &combos {
            for value in &values {
                let mut c = c.clone();
                c.push((key.clone(), value.clone()));
                next.push(c);
            }
        }
        combos = next;
    }
    if combos.len() == 1 && combos[0].is_empty() {
        combos.clear();
    }
    combos.extend(include);
    combos
}

/// Reads one workflow or action file.
///
/// # Errors
///
/// When it isn't valid YAML or has no jobs or steps.
pub fn parse(path: &str, source: &str) -> Result<Workflow, String> {
    let docs = YamlLoader::load_from_str(source).map_err(|e| format!("{path}: {e}"))?;
    let doc = docs.first().ok_or_else(|| format!("{path}: empty"))?;
    let action = path.ends_with("action.yml");
    let top_env = pairs(&doc["env"]);
    let mut jobs = Vec::new();
    if action {
        if text(&doc["runs"]["using"]).as_deref() != Some("composite") {
            return Err(format!("{path}: only composite actions are used"));
        }
        jobs.push(Job {
            id: "steps".to_owned(),
            condition: None,
            needs: Vec::new(),
            env: top_env,
            steps: steps(&doc["runs"]["steps"]),
            matrix: Vec::new(),
            writes: Vec::new(),
        });
    } else {
        let h = doc["jobs"]
            .as_hash()
            .ok_or_else(|| format!("{path}: no `jobs`"))?;
        for (id, job) in h {
            let id = text(id).unwrap_or_default();
            let mut env = top_env.clone();
            env.extend(pairs(&job["env"]));
            for s in job["steps"].as_vec().into_iter().flatten() {
                env.extend(pairs(&s["env"]));
            }
            if let Some(services) = job["services"].as_hash() {
                for (name, service) in services {
                    env.push((
                        format!("service:{}", text(name).unwrap_or_default()),
                        text(&service["image"]).unwrap_or_default(),
                    ));
                }
            }
            let writes = pairs(&job["permissions"])
                .into_iter()
                .filter(|(_, v)| v == "write")
                .map(|(k, _)| k)
                .collect();
            jobs.push(Job {
                id,
                condition: text(&job["if"]),
                needs: list(&job["needs"]),
                env,
                steps: steps(&job["steps"]),
                matrix: matrix(&job["strategy"]["matrix"]),
                writes,
            });
        }
    }
    let events = match &doc["on"] {
        Yaml::Hash(h) => h.keys().filter_map(text).collect(),
        other => list(other),
    };
    let inputs = doc["on"][TRIGGER]["inputs"]
        .as_hash()
        .map(|h| {
            h.iter()
                .filter_map(|(name, i)| {
                    Some(Input {
                        name: text(name)?,
                        required: i["required"].as_bool().unwrap_or(false),
                        kind: text(&i["type"]).unwrap_or_else(|| "string".to_owned()),
                        default: text(&i["default"]),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Workflow {
        path: path.to_owned(),
        jobs,
        events,
        inputs,
        action,
    })
}

/// Every workflow in `.github/workflows/` and composite action in
/// `.github/actions/*/action.yml`, sorted by path.
///
/// # Errors
///
/// Each file that can't be read or parsed.
pub fn load(root: &Path) -> Result<Vec<Workflow>, Vec<String>> {
    let mut paths = Vec::new();
    if let Ok(entries) = fs::read_dir(root.join(".github/workflows")) {
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".yml") || name.ends_with(".yaml") {
                paths.push(format!(".github/workflows/{name}"));
            }
        }
    }
    if let Ok(entries) = fs::read_dir(root.join(".github/actions")) {
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            paths.push(format!(".github/actions/{name}/action.yml"));
        }
    }
    paths.sort();
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for path in paths {
        match fs::read_to_string(root.join(&path)) {
            Ok(source) => match parse(&path, &source) {
                Ok(w) => out.push(w),
                Err(e) => problems.push(e),
            },
            Err(e) => problems.push(format!("{path}: {e}")),
        }
    }
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(problems)
    }
}

/// Replaces each `${{ … }}` with the combination's matrix value, or with
/// `x`.
fn expand(text: &str, combo: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find("}}") else {
            break;
        };
        let expr = rest[start + 3..start + len].trim();
        let value = expr
            .strip_prefix("matrix.")
            .and_then(|k| combo.iter().find(|(key, _)| key == k))
            .map_or("x", |(_, v)| v.as_str());
        out.push_str(value);
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

/// The shell words of a command, with simple quoting.
fn words(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in command.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                started = true;
            }
            // A comment runs to the end of the line.
            None if c == '#' && word.is_empty() && !started => break,
            None if c.is_whitespace() => {
                if started || !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
                started = false;
            }
            None => word.push(c),
        }
    }
    if started || !word.is_empty() {
        out.push(word);
    }
    out
}

/// The commands of a `run:` script: its lines joined across `\`, split at
/// `&&`, `||`, `;` and `|`, comments and leading `VAR=value` dropped.
#[must_use]
pub fn commands(script: &str) -> Vec<Vec<String>> {
    let joined = script.replace("\\\n", " ");
    let mut out = Vec::new();
    for line in joined.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = vec![line.to_owned()];
        for sep in ["&&", "||", ";", "|"] {
            parts = parts
                .iter()
                .flat_map(|p| p.split(sep).map(str::to_owned).collect::<Vec<_>>())
                .collect();
        }
        for part in parts {
            let mut w = words(&part);
            while w.first().is_some_and(|x| {
                x.contains('=')
                    && !x.starts_with('-')
                    && x.split('=').next().is_some_and(|n| {
                        !n.is_empty()
                            && n.chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    })
            }) {
                w.remove(0);
            }
            if !w.is_empty() {
                out.push(w);
            }
        }
    }
    out
}

/// Every command of a job, once per matrix combination.
#[must_use]
pub fn job_commands(job: &Job) -> Vec<(Option<String>, Vec<String>)> {
    let combos = if job.matrix.is_empty() {
        vec![Vec::new()]
    } else {
        job.matrix.clone()
    };
    let mut out = Vec::new();
    for combo in &combos {
        for step in &job.steps {
            if let Some(run) = &step.run {
                for c in commands(&expand(run, combo)) {
                    out.push((step.dir.clone(), c));
                }
            }
        }
    }
    out
}

/// The `[[test]]` names and test modules of a workspace crate.
fn test_targets(root: &Path, krate: &str) -> Option<(Vec<String>, Vec<String>)> {
    let dir = root.join("crates").join(krate);
    let manifest = fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let mut names = Vec::new();
    let mut in_test = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_test = t == "[[test]]";
        } else if in_test
            && let Some(v) = t.strip_prefix("name")
            && let Some(v) = v.trim_start().strip_prefix('=')
        {
            names.push(v.trim().trim_matches('"').to_owned());
        }
    }
    let mut modules = Vec::new();
    if let Ok(entries) = fs::read_dir(dir.join("tests")) {
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".rs") {
                modules.push(stem.to_owned());
                if !names.contains(&stem.to_owned()) && !manifest.contains("autotests = false") {
                    names.push(stem.to_owned());
                }
            } else if e.path().is_dir() {
                modules.push(name);
            }
        }
    }
    Some((names, modules))
}

/// The fuzz targets in `fuzz/Cargo.toml`.
fn fuzz_targets(root: &Path) -> Vec<String> {
    let text = fs::read_to_string(root.join("fuzz/Cargo.toml")).unwrap_or_default();
    let mut out = Vec::new();
    let mut in_bin = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_bin = t == "[[bin]]";
        } else if in_bin
            && let Some(v) = t.strip_prefix("name")
            && let Some(v) = v.trim_start().strip_prefix('=')
        {
            out.push(v.trim().trim_matches('"').to_owned());
        }
    }
    out
}

/// The programs a step may run besides `cargo`, this repository's scripts
/// and the shell's built-ins: each is a tool the job installs or the
/// runner has, used for what only it does.
pub const TOOLS: &[&str] = &[
    "rustup", "git", "docker", "npm", "npx", "cosign", "gh", "tar", "unzip", "mkdir", "cp",
];

/// The script that fills the git-ignored `fuzz/corpus/<target>` from
/// committed files; a job naming such a corpus must run it first.
pub const SEED_CORPUS: &str = "fuzz/seed-corpus.sh";

/// The problems of one command run from `dir`; `seeded` when an earlier
/// step of the job ran [`SEED_CORPUS`].
fn check_command(root: &Path, dir: Option<&str>, w: &[String], seeded: bool) -> Vec<String> {
    let mut problems = Vec::new();
    let shown = w.join(" ");
    let program = w[0].as_str();
    let exists = |rel: &str| {
        let base = dir.map_or(root.to_owned(), |d| root.join(d));
        base.join(rel).exists()
    };
    match program {
        "cargo" => {
            let args: Vec<&str> = w[1..].iter().map(String::as_str).collect();
            match args.first().copied() {
                Some("xtask") => {
                    let argv = std::iter::once("cargo xtask").chain(args[1..].iter().copied());
                    if let Err(e) = Cli::try_parse_from(argv) {
                        let first = e.to_string();
                        let first = first.lines().next().unwrap_or_default().to_owned();
                        problems.push(format!("`{shown}` isn't a `cargo xtask` command: {first}"));
                    }
                }
                Some("+nightly") if args.get(1) == Some(&"fuzz") => {
                    let targets = fuzz_targets(root);
                    if let Some(t) = args.get(3)
                        && !targets.iter().any(|x| x == t)
                    {
                        problems.push(format!("`{shown}`: no fuzz target `{t}` in fuzz/Cargo.toml"));
                    }
                    if let Some(corpus) = args.get(4)
                        && !corpus.starts_with('-')
                    {
                        if corpus.starts_with("fuzz/corpus/") {
                            // Git-ignored: a fresh checkout has none.
                            if !seeded {
                                problems.push(format!(
                                    "`{shown}`: `{corpus}` is git-ignored, so a fresh checkout has none: \
                                     run `{SEED_CORPUS}` in an earlier step of the job"
                                ));
                            }
                        } else if !exists(corpus) {
                            problems.push(format!("`{shown}`: no corpus `{corpus}`"));
                        }
                    }
                }
                _ => {
                    let mut crates = Vec::new();
                    let mut binary = None;
                    let mut filters = Vec::new();
                    let mut after = false;
                    let mut i = 0;
                    while i < args.len() {
                        match args[i] {
                            "--" => after = true,
                            "-p" | "--package" if !after => {
                                crates.extend(args.get(i + 1).copied());
                                i += 1;
                            }
                            "--test" if !after => {
                                binary = args.get(i + 1).copied();
                                i += 1;
                            }
                            a if after && a.ends_with("::") => filters.push(a.trim_end_matches("::")),
                            _ => {}
                        }
                        i += 1;
                    }
                    let mut modules = Vec::new();
                    for c in &crates {
                        match test_targets(root, c) {
                            None => problems.push(format!("`{shown}`: no crate `{c}` in crates/")),
                            Some((names, mods)) => {
                                if let Some(b) = binary
                                    && !names.iter().any(|n| n == b)
                                {
                                    problems.push(format!("`{shown}`: crate `{c}` has no test binary `{b}`"));
                                }
                                modules.extend(mods);
                            }
                        }
                    }
                    for f in filters {
                        let head = f.split("::").next().unwrap_or(f);
                        if !crates.is_empty() && !modules.iter().any(|m| m == head) {
                            problems.push(format!("`{shown}`: no test module `{head}` in {}", crates.join(", ")));
                        }
                    }
                }
            }
            // A path argument must exist (`cargo xtask` checks its own).
            for a in w[1..].iter().filter(|_| w.get(1).is_none_or(|x| x != "xtask")) {
                if (a.starts_with("examples/") || a.starts_with("scripts/")) && !exists(a)
                {
                    problems.push(format!("`{shown}`: `{a}` doesn't exist"));
                }
            }
        }
        p if p.contains('/') => {
            let rel = p.trim_start_matches("./");
            if !exists(rel) {
                problems.push(format!("`{shown}`: `{rel}` doesn't exist"));
            }
        }
        p if TOOLS.contains(&p) => {
            if p == "docker" {
                for pair in w.windows(2) {
                    if (pair[0] == "-f" || pair[0] == "--file") && !exists(&pair[1]) {
                        problems.push(format!("`{shown}`: no Dockerfile `{}`", pair[1]));
                    }
                }
            }
            if (p == "npm" || p == "npx") && dir != Some("editors/vscode") {
                problems.push(format!(
                    "`{shown}`: npm runs only in editors/vscode (docs/compiler/architecture.md §15)"
                ));
            }
        }
        other => problems.push(format!(
            "`{shown}`: `{other}` is neither cargo, a script of this repository nor one of workflows::TOOLS"
        )),
    }
    problems
}

/// Every problem of the workflows in `root`: parse errors, a workflow
/// that runs on anything but [`TRIGGER`], the old `K_CI_ENABLED` switch,
/// an input pasted into a script, an action not pinned or missing, a
/// command that doesn't exist.
#[must_use]
pub fn check(root: &Path) -> Vec<String> {
    let workflows = match load(root) {
        Ok(w) => w,
        Err(p) => return p,
    };
    let mut problems = Vec::new();
    if workflows.iter().all(|w| w.action) {
        problems.push("no workflow in .github/workflows/".to_owned());
    }
    for w in &workflows {
        if !w.action && w.events != [TRIGGER] {
            problems.push(format!(
                "{}: must run only on `{TRIGGER}` (started from the Actions page or `gh workflow run`), not on `{}`",
                w.path,
                w.events.join("`, `")
            ));
        }
        for job in &w.jobs {
            let at = format!("{} job `{}`", w.path, job.id);
            let conditions = job
                .condition
                .iter()
                .chain(job.steps.iter().filter_map(|s| s.condition.as_ref()));
            for c in conditions {
                if c.contains("K_CI_ENABLED") {
                    problems.push(format!(
                        "{at}: `K_CI_ENABLED` is gone: starting the workflow is the switch (`{c}`)"
                    ));
                }
            }
            // A workflow's input is text the dispatcher typed: a script
            // reads it from the environment, never pasted into its source.
            // (A local action's inputs are written by the workflows.)
            for step in job.steps.iter().filter(|_| !w.action) {
                let run = step.run.as_deref().unwrap_or_default();
                let mut rest = run;
                while let Some(start) = rest.find("${{") {
                    let end = rest[start..].find("}}").map_or(rest.len(), |e| start + e);
                    let expr = rest[start + 3..end].trim();
                    if let Some(name) = expr.strip_prefix("inputs.") {
                        problems.push(format!(
                            "{at}: pass `inputs.{name}` through `env:` and read it as a variable, not `${{{{ {expr} }}}}` in a script"
                        ));
                    }
                    rest = &rest[end.min(rest.len())..];
                    if rest.starts_with("}}") {
                        rest = &rest[2..];
                    }
                }
            }
            for step in &job.steps {
                if let Some(uses) = &step.uses {
                    if let Some(local) = uses.strip_prefix("./") {
                        if !root.join(local).join("action.yml").is_file() {
                            problems.push(format!("{at}: no local action `{uses}`"));
                        }
                    } else if !uses
                        .split_once('@')
                        .is_some_and(|(_, r)| r.starts_with('v'))
                    {
                        problems.push(format!(
                            "{at}: `{uses}` must be pinned to a release (`@vN`)"
                        ));
                    }
                }
            }
            let mut seeded = false;
            for (dir, command) in job_commands(job) {
                problems.extend(
                    check_command(root, dir.as_deref(), &command, seeded)
                        .into_iter()
                        .map(|p| format!("{at}: {p}")),
                );
                seeded |= command[0].trim_start_matches("./") == SEED_CORPUS;
            }
        }
    }
    problems
}
