//! The **codegen oracle**: the invariants of code generation and execution
//! that the stage 3 property tests (`crates/k_server/tests/differential.rs`)
//! and the nightly `codegen*` and `rpc_json` fuzz targets (`fuzz/`) share,
//! so a property is never checked one way on stable and another way under
//! fuzzing (`docs/compiler/testing.md`, "Stage 3 properties").
//!
//! * [`check_codegen`]: a project without errors generates in every
//!   profile, deterministically (a second, fresh analysis generates
//!   byte-identical output), with no server code in the client files;
//! * [`run`]: executes a generated script (`Entry::Main`) in a fresh V8
//!   isolate and classifies the outcome. A K panic ([`Outcome::Panicked`])
//!   or typed error is a K result; any other JavaScript exception (a
//!   `TypeError`, a `ReferenceError`, a module that fails to load) is
//!   [`Unexpected`], a code generation or runtime bug.
//!
//! The file is included by path (`#[path]`) and has no dependencies beyond
//! `k_sema`, `k_codegen`, `k_server` and `serde_json`.

#![allow(dead_code)]

use std::time::Duration;

use k_codegen::{
    CodegenError, Entry, Environment, FileKind, Options, Output, Profile, check_client_bundle,
    generate,
};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::{Engine, EngineError, EngineOptions, JsModule};

/// A project: `(project path, text)` pairs.
pub type Files = [(String, String)];

/// Every profile, in the order [`check_codegen`] generates them.
pub const PROFILES: [Profile; 3] = [Profile::Development, Profile::Release, Profile::Test];

/// Analyses `files` as one project.
///
/// # Panics
///
/// If a path is not a valid project path (generators only write valid
/// ones).
pub fn analyse(files: &Files) -> AnalysisHost {
    let files = files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).expect("valid project path"), t.clone()))
        .collect::<Vec<_>>();
    AnalysisHost::with_files(SemaOptions::new(), files)
}

/// The error diagnostics of `files`, one line each.
pub fn errors(host: &AnalysisHost) -> Vec<String> {
    host.analysis()
        .diagnostics()
        .map(|ds| {
            ds.iter()
                .filter(|d| d.is_error())
                .map(|d| format!("{}: {}", d.code, d.message))
                .collect()
        })
        .unwrap_or_else(|_| vec!["the analysis was cancelled".to_owned()])
}

/// The generation options of a script whose entry is `entry`
/// (`module.function`), as the `run` corpus uses them.
pub fn script_options(profile: Profile, entry: &str) -> Options {
    Options {
        profile,
        environment: Environment::Test,
        entry: Entry::Main(entry.to_owned()),
    }
}

/// Formats `files` for a failure message.
pub fn dump(files: &Files) -> String {
    let mut out = String::new();
    for (path, text) in files {
        out.push_str(&format!("--- {path} ---\n{text}\n"));
    }
    out
}

/// Checks the code generation invariants of a project **without errors**
/// and returns its output for `options_of(Profile::Development)`:
///
/// 1. `generate` succeeds for every profile of [`PROFILES`]
///    (`CodegenError::Unsupported` is a failure too: the generated
///    subset is the core language, which stage 3 lowers completely);
/// 2. a fresh analysis of the same files generates exactly the same
///    output, for every profile (determinism);
/// 3. `check_client_bundle` finds no server code in the client files, and
///    no client file contains one of `forbidden`.
///
/// # Errors
///
/// A description of the first violated invariant, with the project.
pub fn check_codegen(
    files: &Files,
    options_of: &dyn Fn(Profile) -> Options,
    forbidden: &[&str],
) -> Result<Output, String> {
    let first = analyse(files);
    let errors = errors(&first);
    if !errors.is_empty() {
        return Err(format!(
            "the project has errors: {errors:#?}\n{}",
            dump(files)
        ));
    }
    let second = analyse(files);
    let mut development = None;
    for profile in PROFILES {
        let options = options_of(profile);
        let a = generate(&first.analysis(), &options);
        let b = generate(&second.analysis(), &options);
        let a = match a {
            Ok(out) => out,
            Err(CodegenError::Unsupported { what }) => {
                return Err(format!(
                    "{profile:?}: code generation does not support {what}\n{}",
                    dump(files)
                ));
            }
            Err(e) => return Err(format!("{profile:?}: {e}\n{}", dump(files))),
        };
        match b {
            Ok(b) if b == a => {}
            Ok(b) => {
                return Err(format!(
                    "{profile:?}: generation is not deterministic: {}\n{}",
                    first_difference(&a, &b),
                    dump(files)
                ));
            }
            Err(e) => {
                return Err(format!(
                    "{profile:?}: the second generation failed: {e}\n{}",
                    dump(files)
                ));
            }
        }
        let split = check_client_bundle(&a, forbidden);
        if !split.is_empty() {
            return Err(format!(
                "{profile:?}: server code in the client bundle: {split:?}\n{}",
                dump(files)
            ));
        }
        if profile == Profile::Development {
            development = Some(a);
        }
    }
    development.ok_or_else(|| "no development output".to_owned())
}

/// Names the first file that differs between two outputs.
fn first_difference(a: &Output, b: &Output) -> String {
    let files = |o: &Output| {
        o.client
            .iter()
            .chain(&o.server)
            .map(|f| (f.path.clone(), f.contents.clone()))
            .collect::<Vec<_>>()
    };
    let (fa, fb) = (files(a), files(b));
    if fa.len() != fb.len() {
        return format!("{} files vs {}", fa.len(), fb.len());
    }
    for ((pa, ca), (pb, cb)) in fa.iter().zip(&fb) {
        if pa != pb {
            return format!("file {pa} vs {pb}");
        }
        if ca != cb {
            let line = ca
                .lines()
                .zip(cb.lines())
                .position(|(x, y)| x != y)
                .unwrap_or(0);
            return format!(
                "{pa} differs at line {}: {:?} vs {:?}",
                line + 1,
                ca.lines().nth(line),
                cb.lines().nth(line)
            );
        }
    }
    "the manifests or database halves differ".to_owned()
}

/// The server modules of an output, for an [`Engine`].
pub fn server_modules(output: &Output) -> Vec<JsModule> {
    output
        .server
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript)
        .map(|f| JsModule {
            specifier: f.path.clone(),
            source: f.contents.clone(),
        })
        .collect()
}

/// How a script ended, in the `run` corpus' terms.
#[derive(Clone, PartialEq, Debug)]
pub enum Outcome {
    /// `main` returned; the JSON encoding of its value (`null` for `Void`).
    Returned(serde_json::Value),
    /// `main` threw a typed error, described as JSON.
    Threw(serde_json::Value),
    /// A K panic, with its message.
    Panicked(String),
    /// The run hit the time limit.
    Timeout,
}

/// What [`run`] observed.
#[derive(Clone, PartialEq, Debug)]
pub struct Run {
    /// The result.
    pub outcome: Outcome,
    /// The log lines, `level message`.
    pub stdout: Vec<String>,
}

/// A failure of generated code that no K program may cause: an exception
/// that is not a K panic or typed error, or a module that fails to load.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Unexpected(pub String);

/// Runs the script of `output` (generated with [`script_options`] for
/// `entry`) in a fresh isolate, at most `limit`.
///
/// `run()` of the entry module reports a panic and an unexpected
/// JavaScript exception the same way (`panics <message>`), so a panic is
/// re-run by calling the entry function directly to read the exception's
/// class: only a `KPanic` is a K outcome.
///
/// # Errors
///
/// [`Unexpected`] for anything that is not a K outcome.
pub fn run(output: &Output, entry: &str, limit: Duration) -> Result<Run, Unexpected> {
    let entry_module = output
        .server_entry
        .clone()
        .ok_or_else(|| Unexpected("the output has no server entry module".to_owned()))?;
    let (module, function) = entry
        .rsplit_once('.')
        .ok_or_else(|| Unexpected(format!("entry `{entry}` is not module.function")))?;
    let mut engine = Engine::new(EngineOptions {
        modules: server_modules(output),
        timeout: Some(limit),
        ..Default::default()
    })
    .map_err(|e| Unexpected(e.to_string()))?;
    let quoted = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let line = match engine.run_main(&format!(
        "import {{ run }} from {};\nDeno.core.ops.op_k_set_result(await run());\n",
        quoted(&entry_module)
    )) {
        Ok(Some(line)) => line,
        Ok(None) => return Err(Unexpected("run() produced no result line".to_owned())),
        Err(EngineError::Timeout(_)) => {
            return Ok(Run {
                outcome: Outcome::Timeout,
                stdout: lines(&mut engine),
            });
        }
        Err(e) => return Err(Unexpected(e.to_string())),
    };
    let stdout = lines(&mut engine);
    let parse = |json: &str| {
        serde_json::from_str::<serde_json::Value>(json)
            .map_err(|e| Unexpected(format!("the result line is not JSON ({e}): {line}")))
    };
    let outcome = if line == "returns void" {
        Outcome::Returned(serde_json::Value::Null)
    } else if let Some(json) = line.strip_prefix("returns ") {
        Outcome::Returned(parse(json)?)
    } else if let Some(json) = line.strip_prefix("throws ") {
        Outcome::Threw(parse(json)?)
    } else if let Some(message) = line.strip_prefix("panics ") {
        let specifier = quoted(&format!("k:/app/{}.js", module.replace('.', "/")));
        let class = engine.run_main(&format!(
            "import * as m from {specifier};\n\
             let name = \"none\";\n\
             try {{ await m[{}](); }} catch (e) {{ name = e === null || typeof e !== \"object\" ? typeof e : String(e.name); }}\n\
             Deno.core.ops.op_k_set_result(name);\n",
            quoted(function)
        ));
        match class {
            Ok(Some(name)) if name == "KPanic" => Outcome::Panicked(message.to_owned()),
            Ok(Some(name)) => {
                return Err(Unexpected(format!(
                    "main threw a JavaScript {name}, not a K panic: {message}"
                )));
            }
            Ok(None) => return Err(Unexpected("the re-run produced no class".to_owned())),
            Err(EngineError::Timeout(_)) => Outcome::Timeout,
            Err(e) => return Err(Unexpected(format!("re-running main failed: {e}"))),
        }
    } else {
        return Err(Unexpected(format!("not a result line: {line}")));
    };
    Ok(Run { outcome, stdout })
}

fn lines(engine: &mut Engine) -> Vec<String> {
    engine
        .take_output()
        .into_iter()
        .map(|l| format!("{} {}", l.level, l.message))
        .collect()
}

/// Compares two JSON values as K values: numbers by their `f64` value
/// (`3` and `3.0` are the same `Double`), everything else structurally.
pub fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value as V;
    match (a, b) {
        (V::Number(x), V::Number(y)) => x.as_f64() == y.as_f64(),
        (V::Array(x), V::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_eq(p, q))
        }
        (V::Object(x), V::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
}

/// Generates `files` as a script with entry `entry` in `profile`.
///
/// # Errors
///
/// The code generation error, as text.
pub fn generate_profile(files: &Files, profile: Profile, entry: &str) -> Result<Output, String> {
    let host = analyse(files);
    generate(&host.analysis(), &script_options(profile, entry)).map_err(|e| e.to_string())
}

/// The execution property of a **terminating** well-typed project (the
/// typed generator's runnable mode) whose entry is `entry`:
/// [`check_codegen`] holds, the development build runs to a K outcome
/// (never [`Unexpected`], never the time limit), and the release build
/// (renamed locals, compact printing) logs the same lines and ends the
/// same way, unless the development build panicked with an integer
/// overflow (release builds don't check `Int` `+ - *`, codegen.md §5).
///
/// # Errors
///
/// A description of the violation, with the project.
pub fn check_terminating_program(
    files: &Files,
    entry: &str,
    limit: Duration,
) -> Result<Run, String> {
    let dev = check_codegen(files, &|p| script_options(p, entry), &[])?;
    let a = run(&dev, entry, limit)
        .map_err(|Unexpected(e)| format!("development build: {e}\n{}", dump(files)))?;
    if a.outcome == Outcome::Timeout {
        return Err(format!("a terminating program timed out\n{}", dump(files)));
    }
    let release = generate_profile(files, Profile::Release, entry)?;
    if std::env::var_os("K_DIFF_DUMP").is_some() {
        for m in server_modules(&release)
            .iter()
            .filter(|m| m.specifier.starts_with("k:/app/"))
        {
            eprintln!("// ===== release {}\n{}", m.specifier, m.source);
        }
    }
    let b = run(&release, entry, limit)
        .map_err(|Unexpected(e)| format!("release build: {e}\n{}", dump(files)))?;
    // `Int` `+ - *` are checked in development and test builds only
    // (codegen.md §5): after an overflow panic the release build computes
    // on with an imprecise double, so the two builds may differ.
    let overflowed =
        matches!(&a.outcome, Outcome::Panicked(m) if m.starts_with("integer overflow"));
    if a != b && !overflowed {
        return Err(format!(
            "development and release builds disagree:\n  development: {a:?}\n  release:     {b:?}\n{}",
            dump(files)
        ));
    }
    Ok(a)
}
