//! Executes the `run` corpus (`tests/corpus/run`): each case is compiled by
//! `k_codegen` and its server bundle evaluated in a fresh V8 isolate, then
//! compared on log output and result (`k_corpus::run_suite`,
//! `docs/compiler/testing.md`, "Execution tests").
//!
//! ```text
//! cargo test -p k_server --test corpus_run                 # every case
//! cargo test -p k_server --test corpus_run -- run/ints/    # a group
//! K_BLESS=1 cargo test -p k_server --test corpus_run       # rewrite expectations
//! ```

use std::time::Duration;

use k_codegen::{FileKind, Output};
use k_corpus::run_suite::{Execution, Executor, stdout_line};
use k_corpus::suites::Suite;
use k_corpus::{bless_enabled, corpus_root, discover};
use k_server::{Engine, EngineOptions, JsModule};
use libtest_mimic::{Arguments, Failed, Trial};

/// Runs a program's server bundle in a fresh isolate, at most 10 s.
struct V8Executor;

impl Executor for V8Executor {
    fn execute(&mut self, output: &Output) -> Result<Execution, String> {
        let entry = output
            .server_entry
            .clone()
            .ok_or("the output has no server entry module")?;
        let modules = output
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect();
        let mut engine = Engine::new(EngineOptions {
            modules,
            timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        let entry = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
        let result = engine
            .run_main(&format!(
                "import {{ run }} from {entry};\nDeno.core.ops.op_k_set_result(await run());\n"
            ))
            .map_err(|e| e.to_string())?
            .ok_or("the entry's run() produced no result line")?;
        let stdout = engine
            .take_output()
            .iter()
            .map(|l| stdout_line(&l.level, &l.message))
            .collect();
        Ok(Execution { stdout, result })
    }
}

fn main() {
    let args = Arguments::from_args();
    let bless = bless_enabled();
    let trials = discover::discover(&corpus_root())
        .cases
        .into_iter()
        .filter(|c| c.suite == Suite::Run)
        .map(|case| {
            let name = case.name.clone();
            Trial::test(name, move || {
                k_corpus::run_suite::run(&case, bless, &mut V8Executor).map_err(Failed::from)
            })
        })
        .collect();
    libtest_mimic::run(&args, trials).exit();
}
