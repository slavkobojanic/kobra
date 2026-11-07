//! Runs the runtime's JavaScript unit tests (`crates/k_runtime/js/tests/*.test.js`)
//! inside V8, one cargo test per registered JS test, each in a fresh isolate
//! (`docs/compiler/testing.md`, "Runtime JS tests").
//!
//! ```text
//! cargo test -p k_server --test runtime_js                 # every JS test
//! cargo test -p k_server --test runtime_js -- signals.test # one file
//! ```
//!
//! A test registered with `test.pending(name, reason, fn)` passes while it
//! fails and fails once it passes, like a `.pending` corpus case.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use k_server::{Engine, EngineOptions, JsModule};
use libtest_mimic::{Arguments, Failed, Trial};
use serde::Deserialize;

#[derive(Deserialize)]
struct Listed {
    name: String,
    pending: Option<String>,
}

#[derive(Deserialize)]
struct Outcome {
    outcome: String,
    message: Option<String>,
}

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../k_runtime/js/tests")
}

fn engine(file: &str, source: &str) -> Result<Engine, String> {
    Engine::new(EngineOptions {
        modules: vec![JsModule {
            specifier: format!("k:/tests/{file}"),
            source: source.to_owned(),
        }],
        timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    })
    .map_err(|e| e.to_string())
}

fn list(file: &str, source: &str) -> Result<Vec<Listed>, String> {
    let mut e = engine(file, source)?;
    let code = format!(
        "import {{ list }} from \"k:testing\";\nimport \"k:/tests/{file}\";\nDeno.core.ops.op_k_set_result(list());\n"
    );
    let text = e
        .run_main(&code)
        .map_err(|e| e.to_string())?
        .ok_or("no test list")?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn run(file: &str, source: &str, test: &Listed) -> Result<(), String> {
    let mut e = engine(file, source)?;
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
                    let (file, source) = (file.clone(), source.clone());
                    let ignored = t.pending.is_some();
                    trials.push(
                        Trial::test(name, move || run(&file, &source, &t).map_err(Failed::from))
                            .with_kind(if ignored { "pending" } else { "" }),
                    );
                }
            }
            Err(e) => trials.push(Trial::test(format!("{file} (load)"), move || {
                Err(Failed::from(e))
            })),
        }
    }
    libtest_mimic::run(&args, trials).exit();
}
