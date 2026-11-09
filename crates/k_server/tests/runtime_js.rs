//! Runs the runtime's JavaScript unit tests (`crates/k_runtime/js/tests/*.test.js`)
//! inside V8, one cargo test per registered JS test, each in a fresh isolate
//! (`docs/compiler/testing.md`, "Runtime JS tests"). A JSON fixture next to
//! them, `name.json`, is importable as `k:/tests/name.json.js`.
//!
//! ```text
//! cargo test -p k_server --test runtime_js                 # every JS test
//! cargo test -p k_server --test runtime_js -- signals.test # one file
//! ```
//!
//! A test registered with `test.pending(name, reason, fn)` passes while it
//! fails and fails once it passes, like a `.pending` corpus case.
//!
//! Every test runs twice: against the runtime as written, and as
//! `<file>/<name> (compacted)` against the runtime after
//! `k_runtime::compact`, which is what release builds ship, so a compaction
//! that changes what the runtime means fails here.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use k_server::{Engine, EngineOptions, JsModule};
use libtest_mimic::{Arguments, Failed, Trial};
use serde::Deserialize;

/// One test as `k:testing`'s `list()` reports it.
#[derive(Deserialize)]
struct Listed {
    /// The name the test was registered with.
    name: String,
    /// The reason of a `test.pending` test; `None` for a plain `test`.
    pending: Option<String>,
}

/// What `k:testing`'s `run(name)` reports for one test.
#[derive(Deserialize)]
struct Outcome {
    /// `passed`, or how it didn't pass.
    outcome: String,
    /// The failure's message, if any.
    message: Option<String>,
}

/// `crates/k_runtime/js/tests`, where the runtime's JS test files live.
fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../k_runtime/js/tests")
}

/// A fresh isolate holding test file `file` (as `k:/tests/<file>`), with a
/// ten-second limit per run and the compacted runtime when
/// `compact_runtime` is set.
///
/// # Errors
///
/// The isolate can't be created.
fn engine(file: &str, source: &str, compact_runtime: bool) -> Result<Engine, String> {
    let mut modules = vec![JsModule {
        specifier: format!("k:/tests/{file}"),
        source: source.to_owned(),
    }];
    modules.extend(fixtures()?);
    Engine::new(EngineOptions {
        modules,
        timeout: Some(Duration::from_secs(10)),
        compact_runtime,
        ..Default::default()
    })
    .map_err(|e| e.to_string())
}

/// The JSON fixtures next to the test files (`channel-frames.json`, the
/// golden vectors shared with `k_server`'s tests), each as a module
/// `k:/tests/<name>.json.js` whose default export is the parsed value.
///
/// # Errors
///
/// A fixture can't be read.
fn fixtures() -> Result<Vec<JsModule>, String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(tests_dir()).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        out.push(JsModule {
            specifier: format!("k:/tests/{name}.js"),
            source: format!("export default {text};\n"),
        });
    }
    Ok(out)
}

/// The tests file `file` registers, in registration order.
///
/// # Errors
///
/// The file fails to load or the list isn't valid JSON.
fn list(file: &str, source: &str) -> Result<Vec<Listed>, String> {
    let mut e = engine(file, source, false)?;
    let code = format!(
        "import {{ list }} from \"k:testing\";\nimport \"k:/tests/{file}\";\nDeno.core.ops.op_k_set_result(list());\n"
    );
    let text = e
        .run_main(&code)
        .map_err(|e| e.to_string())?
        .ok_or("no test list")?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Runs one test in its own isolate, with the DOM shim installed.
///
/// # Errors
///
/// The test's failure message; for a pending test, a message saying it
/// passes now (a failing pending test is `Ok`).
fn run(file: &str, source: &str, test: &Listed, compact_runtime: bool) -> Result<(), String> {
    let mut e = engine(file, source, compact_runtime)?;
    let name = serde_json::to_string(&test.name).map_err(|e| e.to_string())?;
    let code = format!(
        "import {{ run, installDom }} from \"k:testing\";\nimport \"k:/tests/{file}\";\ninstallDom();\nDeno.core.ops.op_k_set_result(await run({name}));\n"
    );
    let result = e.run_main(&code);
    let passed = match &result {
        Ok(Some(text)) => {
            let o: Outcome = serde_json::from_str(text).map_err(|e| e.to_string())?;
            if o.outcome == "passed" {
                Ok(())
            } else {
                Err(o.message.unwrap_or_default())
            }
        }
        Ok(None) => Err("the test produced no outcome".to_owned()),
        Err(e) => Err(e.to_string()),
    };
    match (&test.pending, passed) {
        (None, outcome) => outcome,
        (Some(_), Err(_)) => Ok(()),
        (Some(reason), Ok(())) => Err(format!(
            "this pending test passes now ({reason}); change `test.pending` to `test` in {file}"
        )),
    }
}

fn main() {
    let args = Arguments::from_args();
    let mut trials = Vec::new();
    let mut files: Vec<PathBuf> = fs::read_dir(tests_dir())
        .expect("crates/k_runtime/js/tests exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".test.js"))
        .collect();
    files.sort();
    for path in files {
        let file = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let source = fs::read_to_string(&path).expect("readable test file");
        match list(&file, &source) {
            Ok(tests) => {
                let mut seen = std::collections::HashSet::new();
                for t in tests {
                    let name = format!("{file}/{}", t.name);
                    if !seen.insert(t.name.clone()) {
                        trials.push(Trial::test(format!("{name} (duplicate)"), || {
                            Err(Failed::from("duplicate test name"))
                        }));
                        continue;
                    }
                    let kind = if t.pending.is_some() { "pending" } else { "" };
                    let t = std::sync::Arc::new(t);
                    for compact in [false, true] {
                        let (file, source, t) = (file.clone(), source.clone(), t.clone());
                        let name = if compact {
                            format!("{name} (compacted)")
                        } else {
                            name.clone()
                        };
                        trials.push(
                            Trial::test(name, move || {
                                run(&file, &source, &t, compact).map_err(Failed::from)
                            })
                            .with_kind(kind),
                        );
                    }
                }
            }
            Err(e) => trials.push(Trial::test(format!("{file} (load)"), move || {
                Err(Failed::from(e))
            })),
        }
    }
    libtest_mimic::run(&args, trials).exit();
}
