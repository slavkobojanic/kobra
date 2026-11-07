//! `k test`: runs the project's tests (`docs/design/21-testing.md`;
//! `docs/compiler/testing.md` §10, "`k test`").
//!
//! The project is analysed; a program with errors is reported like
//! `k check` and nothing runs. Otherwise the test bundle
//! ([`k_codegen::Entry::Tests`]) is generated for the `.test` environment
//! and evaluated in one V8 isolate over the fake DOM: its browser modules
//! (`k:/client/…`) and its server modules (`k:/app/…`, whose entry exports
//! `$test`) together. The test kit (`k:testing/kit`) runs the selected
//! tests, each with a fresh document, fresh containers and a fresh server
//! process (`op_k_test_reset`; for an app with persistence, a fresh
//! in-memory database with the migrations applied, from
//! [`k_server::Engine::set_test_database`]), and reports each outcome.
//! `K_TEST_BUNDLE_DIR=dir` writes the bundle's modules there, for
//! debugging the compiler.
//!
//! # Output
//!
//! On standard output: `running N tests`, one `PASS` or `FAIL` line per
//! test (`Group › description`), then, if any failed, `failures:` and one
//! `K7510` diagnostic per failure, rendered like `k check`'s with the
//! source of the failed expectation and a note per operand value, and
//! finally `test result: ok.` or `test result: FAILED.` with the counts.
//!
//! # Selection
//!
//! `--filter TEXT` (repeatable) runs the tests whose full name contains any
//! of the texts. `--watch` runs, then re-runs the affected tests whenever a
//! file of the project changes: the test files that changed, or that refer
//! (transitively) to a file that changed, by
//! [`k_sema::Analysis::file_dependencies`] over the incrementally updated
//! analysis.
//!
//! # Exit status
//!
//! 0 when every selected test passed, 1 when one failed, the program has
//! errors or a filter matched nothing, 2 when the directory is not a
//! project or the bundle couldn't be built or run.

use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;

use k_codegen::{CodegenError, Entry, Environment, Options, Profile, generate};
use k_diagnostics::{Diagnostic, FileId, Span, TextRange, TextSize, codes, render};
use k_sema::{Analysis, ProjectPath};

use crate::cli::{ColorChoice, TestArgs};
use crate::project;

/// One test's outcome, as the kit reports it.
#[derive(Debug)]
struct Outcome {
    /// `Group › description`.
    full: String,
    /// The project path of the test file.
    file: String,
    /// The failure, if the test failed.
    failure: Option<Failure>,
}

/// Why a test failed.
#[derive(Debug)]
struct Failure {
    message: String,
    start: u32,
    end: u32,
    values: Vec<(String, String)>,
}

/// The outcome of one run.
struct Run {
    /// Every test of the bundle, selected or not.
    total: usize,
    outcomes: Vec<Outcome>,
}

/// Runs `k test`.
pub fn run(args: &TestArgs) -> ExitCode {
    let color = match args.color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::stdout().is_terminal(),
    };
    // At a workspace root: every member's tests, members in dependency
    // order (40-workspaces-and-services.md).
    if crate::workspace::locate(&args.project).is_some_and(|l| l.member.is_none()) {
        if args.watch {
            eprintln!(
                "error: `k test --watch` runs in one project; run it in a member of the workspace"
            );
            return ExitCode::from(2);
        }
        return run_workspace(args, color);
    }
    let mut project = match project::load(&args.project, "test") {
        Ok(p) => p,
        Err(code) => return code,
    };
    let first = run_once(&project, &args.filter, None, color);
    if !args.watch {
        return first;
    }
    watch(&mut project, &args.filter, color)
}

/// Runs the tests of every member of the workspace at `args.project`, each
/// after a `== <name>` line; fails when any member fails.
fn run_workspace(args: &TestArgs, color: bool) -> ExitCode {
    let loaded = match crate::workspace::load(&args.project) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let mut worst = ExitCode::SUCCESS;
    for m in loaded.targets() {
        let dir = loaded.member_dir(m);
        let project = project::project_of(&dir, &loaded, m);
        println!("== {}", project.name());
        let code = run_once(&project, &args.filter, None, color);
        if code != ExitCode::SUCCESS {
            worst = code;
        }
        println!();
    }
    worst
}

/// Analyses, builds and runs once: every test, or those of `files` (test
/// file paths), narrowed by `filters`. Prints the report.
fn run_once(
    project: &project::Project,
    filters: &[String],
    files: Option<&BTreeSet<String>>,
    color: bool,
) -> ExitCode {
    let analysis = project.host.analysis();
    let Ok(diagnostics) = analysis.diagnostics() else {
        eprintln!("error: the analysis was cancelled");
        return ExitCode::from(2);
    };
    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
    if errors > 0 {
        let map = analysis.source_map(|p| project.display(p));
        let options = if std::io::stderr().is_terminal() {
            render::HumanOptions::colored()
        } else {
            render::HumanOptions::plain()
        };
        for d in &diagnostics {
            eprintln!("{}", render::human(d, &map, &options));
        }
        eprintln!("error: the project has {errors} error(s), so its tests can't run");
        return ExitCode::from(1);
    }
    let run = match execute(&analysis, filters, files) {
        Ok(r) => r,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(2);
        }
    };
    if !filters.is_empty() && run.outcomes.is_empty() && run.total > 0 && files.is_none() {
        let list: Vec<String> = filters.iter().map(|f| format!("`{f}`")).collect();
        eprintln!("error: no test matches {}", list.join(" or "));
        return ExitCode::from(1);
    }
    print!("{}", report(&analysis, project, &run, color));
    if run.outcomes.iter().any(|o| o.failure.is_some()) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// The report of a run, as printed.
fn report(analysis: &Analysis, project: &project::Project, run: &Run, color: bool) -> String {
    let paint = |code: &str, text: &str| {
        if color {
            format!("\u{1b}[{code}m{text}\u{1b}[0m")
        } else {
            text.to_owned()
        }
    };
    let noun = if run.outcomes.len() == 1 {
        "test"
    } else {
        "tests"
    };
    let mut out = format!("running {} {noun}\n", run.outcomes.len());
    for o in &run.outcomes {
        let label = if o.failure.is_some() {
            paint("1;31", "FAIL")
        } else {
            paint("1;32", "PASS")
        };
        out.push_str(&format!("{label}  {}\n", o.full));
    }
    out.push('\n');
    let failed: Vec<&Outcome> = run
        .outcomes
        .iter()
        .filter(|o| o.failure.is_some())
        .collect();
    if !failed.is_empty() {
        out.push_str("failures:\n\n");
        let map = analysis.source_map(|p| project.display(p));
        let options = if color {
            render::HumanOptions::colored()
        } else {
            render::HumanOptions::plain()
        };
        for o in &failed {
            let Some(f) = &o.failure else { continue };
            let file = ProjectPath::new(&o.file)
                .ok()
                .and_then(|p| analysis.file_id(&p))
                .unwrap_or_else(|| FileId::new(0));
            let range = TextRange::new(TextSize::from(f.start), TextSize::from(f.end.max(f.start)));
            let mut d = Diagnostic::new(codes::TEST_FAILED, Span::new(file, range), &f.message);
            for (text, value) in &f.values {
                d = d.with_note(format!("`{text}` was {value}"));
            }
            d = d.with_note(format!("in test `{}`", o.full));
            out.push_str(&render::human(&d, &map, &options));
            out.push('\n');
        }
    }
    let passed = run.outcomes.len() - failed.len();
    let verdict = if failed.is_empty() {
        paint("32", "ok")
    } else {
        paint("31", "FAILED")
    };
    out.push_str(&format!(
        "test result: {verdict}. {passed} passed; {} failed; {} filtered out\n",
        failed.len(),
        run.total - run.outcomes.len()
    ));
    out
}

/// Builds the test bundle of `analysis` and runs the selected tests.
#[cfg(feature = "server")]
fn execute(
    analysis: &Analysis,
    filters: &[String],
    files: Option<&BTreeSet<String>>,
) -> Result<Run, String> {
    use k_server::{Engine, EngineOptions, JsModule};
    let options = Options {
        profile: Profile::Development,
        environment: Environment::Test,
        entry: Entry::Tests,
    };
    let output = match generate(analysis, &options) {
        Ok(o) => o,
        Err(CodegenError::HasErrors { count }) => {
            return Err(format!("the project has {count} error(s)"));
        }
        Err(e) => return Err(format!("the tests can't be built: {e}")),
    };
    let Some(entry) = output
        .client
        .iter()
        .find(|f| f.path.starts_with("app.") && f.path.ends_with(".js"))
    else {
        return Ok(Run {
            total: 0,
            outcomes: Vec::new(),
        });
    };
    // One isolate holds both halves: the browser's modules under
    // `k:/client/`, the server's under their own specifiers (`k:/app/…`).
    let mut modules: Vec<JsModule> = output
        .client
        .iter()
        .filter(|f| f.path.ends_with(".js"))
        .map(|f| JsModule {
            specifier: format!("k:/client/{}", f.path),
            source: f.contents.clone(),
        })
        .collect();
    modules.extend(output.server.iter().map(|f| JsModule {
        specifier: f.path.clone(),
        source: f.contents.clone(),
    }));
    // `K_TEST_BUNDLE_DIR=dir` writes the bundle there, for debugging the
    // compiler (docs/compiler/testing.md §8.4).
    if let Some(dir) = std::env::var_os("K_TEST_BUNDLE_DIR") {
        write_bundle(Path::new(&dir), &modules);
    }
    let mut engine = Engine::new(EngineOptions {
        modules,
        timeout: Some(std::time::Duration::from_secs(300)),
        output: if std::env::var_os("K_TEST_LOG").is_some() {
            k_server::Output::JsonLines
        } else {
            k_server::Output::Capture
        },
        ..EngineOptions::default()
    })
    .map_err(|e| e.to_string())?;
    let json = |v: &serde_json::Value| v.to_string();
    // Every test starts with a fresh in-memory database, migrated.
    engine.set_test_database(output.database.clone());
    let server = output.server_entry.as_deref().map_or_else(
        || "null".to_owned(),
        |spec| format!("(await import({})).$test", json(&serde_json::json!(spec))),
    );
    let main = format!(
        "import {{ installDom }} from \"k:testing\";\n\
         import * as kit from \"k:testing/kit\";\n\
         installDom();\n\
         const server = {server};\n\
         const {{ $groups, $pages }} = await import(\"k:/client/{entry}\");\n\
         const filters = {filters};\n\
         const files = {files};\n\
         const all = [];\n\
         for (const g of $groups) for (const t of g.tests) all.push({{ full: `${{g.name}} › ${{t.name}}`, file: g.file }});\n\
         const selected = all.filter((t) => (files === null || files.includes(t.file)) && (filters.length === 0 || filters.some((f) => t.full.includes(f)))).map((t) => t.full);\n\
         const results = await kit.run($groups, selected, server, $pages ?? []);\n\
         Deno.core.ops.op_k_set_result(JSON.stringify({{ total: all.length, results }}));\n",
        entry = entry.path,
        server = server,
        filters = json(&serde_json::json!(filters)),
        files = files.map_or_else(
            || "null".to_owned(),
            |f| json(&serde_json::json!(f.iter().collect::<Vec<_>>()))
        ),
    );
    let result = engine
        .run_main(&main)
        .map_err(|e| format!("the tests couldn't run: {e}"))?
        .ok_or("the tests ended without reporting")?;
    let value: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| format!("bad report: {e}"))?;
    let total = value["total"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let mut outcomes = Vec::new();
    for r in value["results"].as_array().into_iter().flatten() {
        let text = |k: &str| r[k].as_str().unwrap_or_default().to_owned();
        let failure = r.get("failure").filter(|f| f.is_object()).map(|f| {
            let offset = |k: &str| {
                f[k].as_f64()
                    .filter(|n| *n >= 0.0)
                    .map_or(0, |n| u32::try_from(n as i64).unwrap_or(0))
            };
            Failure {
                message: f["message"]
                    .as_str()
                    .unwrap_or("the test failed")
                    .to_owned(),
                start: offset("start"),
                end: offset("end"),
                values: f["values"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| Some((v[0].as_str()?.to_owned(), v[1].as_str()?.to_owned())))
                    .collect(),
            }
        });
        outcomes.push(Outcome {
            full: format!("{} › {}", text("group"), text("name")),
            file: text("file"),
            failure,
        });
    }
    Ok(Run { total, outcomes })
}

/// Writes each module of a test bundle to `dir`, named after its
/// specifier (`k__client_app.1a2b3c4d.js`, `k__app_$server.js`); a failure
/// to write is reported and otherwise ignored.
#[cfg(feature = "server")]
fn write_bundle(dir: &Path, modules: &[k_server::JsModule]) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("warning: K_TEST_BUNDLE_DIR: {e}");
        return;
    }
    for m in modules {
        let name = m.specifier.replace(['/', ':'], "_");
        if let Err(e) = std::fs::write(dir.join(name), &m.source) {
            eprintln!("warning: K_TEST_BUNDLE_DIR: {e}");
        }
    }
}

#[cfg(not(feature = "server"))]
fn execute(
    _analysis: &Analysis,
    _filters: &[String],
    _files: Option<&BTreeSet<String>>,
) -> Result<Run, String> {
    let _ = (
        Options::default(),
        Profile::Development,
        Environment::Test,
        Entry::Tests,
    );
    let _: fn(&Analysis, &Options) -> Result<k_codegen::Output, CodegenError> = generate;
    Err("this `k` was built without the server runtime (feature `server`)".to_owned())
}

/// The test files affected by a change of `changed`: those that changed
/// or that refer, directly or through other files, to one that did.
fn affected(analysis: &Analysis, changed: &BTreeSet<FileId>) -> BTreeSet<String> {
    let files = analysis.files();
    let mut dependents: BTreeMap<FileId, Vec<FileId>> = BTreeMap::new();
    for (id, _) in &files {
        for dep in analysis.file_dependencies(*id).unwrap_or_default() {
            dependents.entry(dep).or_default().push(*id);
        }
    }
    let mut seen: BTreeSet<FileId> = changed.clone();
    let mut queue: Vec<FileId> = changed.iter().copied().collect();
    while let Some(f) = queue.pop() {
        for &d in dependents.get(&f).into_iter().flatten() {
            if seen.insert(d) {
                queue.push(d);
            }
        }
    }
    files
        .into_iter()
        .filter(|(id, p)| seen.contains(id) && p.as_str().ends_with(".test.k"))
        .map(|(_, p)| p.as_str().to_owned())
        .collect()
}

/// `--watch`: re-runs the affected tests on every change until stopped.
#[cfg(feature = "server")]
fn watch(project: &mut project::Project, filters: &[String], color: bool) -> ExitCode {
    let root = project.root.clone();
    let mut seen = crate::build::fingerprint(&root);
    let mut texts = current_texts(&root);
    println!("watching for changes; press Ctrl-C to stop");
    loop {
        std::thread::sleep(crate::build::WATCH_INTERVAL);
        let now = crate::build::fingerprint(&root);
        if now == seen {
            continue;
        }
        // Let a burst of writes settle.
        std::thread::sleep(crate::build::WATCH_INTERVAL);
        seen = crate::build::fingerprint(&root);
        let next = current_texts(&root);
        let mut change = k_sema::Change::new();
        let mut paths: Vec<ProjectPath> = Vec::new();
        for (path, text) in &next {
            if texts.get(path) != Some(text) {
                change.set_file(path.clone(), text.as_str());
                paths.push(path.clone());
            }
        }
        for path in texts.keys() {
            if !next.contains_key(path) {
                change.remove_file(path.clone());
            }
        }
        texts = next;
        if change.is_empty() {
            continue;
        }
        project.host.apply_change(change);
        let analysis = project.host.analysis();
        let changed: BTreeSet<FileId> = paths.iter().filter_map(|p| analysis.file_id(p)).collect();
        let files = affected(&analysis, &changed);
        drop(analysis);
        println!();
        if files.is_empty() {
            println!("change detected: no test is affected");
        } else {
            println!(
                "change detected: running the tests of {}",
                files.iter().cloned().collect::<Vec<_>>().join(", ")
            );
            let _ = run_once(project, filters, Some(&files), color);
        }
        println!("watching for changes; press Ctrl-C to stop");
    }
}

#[cfg(not(feature = "server"))]
fn watch(_project: &mut project::Project, _filters: &[String], _color: bool) -> ExitCode {
    eprintln!("error: this `k` was built without the server runtime (feature `server`)");
    ExitCode::from(2)
}

/// The project's files and their texts, keyed by project path.
fn current_texts(root: &Path) -> BTreeMap<ProjectPath, String> {
    project::read(root)
        .map(|files| files.into_iter().collect())
        .unwrap_or_default()
}
