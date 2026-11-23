//! The preview bundle (`k_codegen::Entry::Previews`,
//! docs/design/57-previews.md) in one isolate over the fake DOM, as
//! `k check` and `k preview` run it: `tests/projects/previews`.
//!
//! * `$previews` lists each preview's label, file, keyword range and
//!   parameter names, in the order of `$groups` and of
//!   `Analysis::previews`;
//! * `$mount(c, values)` takes parameter values by name, and a `bind` of a
//!   parameter writes back, re-rendering what reads it;
//! * a page renders inside its layouts, as the router shows it;
//! * no other bundle has the previews.

use std::path::{Path, PathBuf};
use std::time::Duration;

use k_codegen::{Entry, Environment, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::{Engine, EngineOptions, JsModule};
use serde_json::{Value, json};

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/projects/previews")
}

fn files(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("reads")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, root, out);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("inside")
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&path).expect("reads");
        out.push((ProjectPath::new(&relative).expect("a project path"), text));
    }
}

fn build(entry: Entry) -> Output {
    let dir = project();
    let mut all = Vec::new();
    files(&dir, &dir, &mut all);
    let host = AnalysisHost::with_files(SemaOptions::new(), all);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().expect("not cancelled");
    assert!(
        diagnostics.is_empty(),
        "the project checks cleanly: {diagnostics:?}"
    );
    generate(
        &analysis,
        &Options {
            profile: Profile::Development,
            environment: Environment::Test,
            entry,
        },
    )
    .expect("generates")
}

/// Runs `body` (JavaScript, with `$groups`, `$pages`, `$previews`, `kit`
/// and `server` in scope) after loading the preview bundle; its value,
/// JSON-encoded, is the result.
fn run(body: &str) -> Value {
    let output = build(Entry::Previews(None));
    let entry = output
        .client
        .iter()
        .find(|f| f.path.starts_with("app.") && f.path.ends_with(".js"))
        .expect("an entry")
        .path
        .clone();
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
    let mut engine = Engine::new(EngineOptions {
        modules,
        timeout: Some(Duration::from_secs(120)),
        ..EngineOptions::default()
    })
    .expect("an isolate");
    engine.set_test_database(output.database.clone());
    let server = output.server_entry.as_deref().map_or_else(
        || "null".to_owned(),
        |spec| format!("(await import({})).$test", json!(spec)),
    );
    let main = format!(
        "import {{ installDom }} from \"k:testing\";\n\
         import * as kit from \"k:testing/kit\";\n\
         installDom();\n\
         const server = {server};\n\
         const {{ $groups, $pages, $previews }} = await import(\"k:/client/{entry}\");\n\
         const value = await (async () => {{ {body} }})();\n\
         Deno.core.ops.op_k_set_result(JSON.stringify(value));\n"
    );
    let result = engine.run_main(&main).expect("runs").expect("reports");
    serde_json::from_str(&result).expect("JSON")
}

/// Runs a preview's group with one custom test, `$custom`, added to its
/// class: `script` is its body, with `this` the group instance.
fn custom(label: &str, script: &str) -> Value {
    run(&format!(
        "const i = $groups.findIndex((g) => g.name === {label});\n\
         const g = $groups[i];\n\
         g.group.prototype.$custom = async function ($c) {{ {script} }};\n\
         const groups = $groups.map((x, j) => (j === i ? {{ ...x, tests: [{{ name: \"custom\", method: \"$custom\", start: 0, end: 0 }}] }} : {{ ...x, tests: [] }}));\n\
         return await kit.run(groups, [`${{g.name}} › custom`], server, $pages ?? []);",
        label = json!(label),
    ))
}

fn passed(results: &Value) {
    assert_eq!(results.as_array().map(Vec::len), Some(1), "{results}");
    assert!(
        results[0].get("failure").is_none_or(Value::is_null),
        "{results}"
    );
}

#[test]
fn previews_are_listed_in_order_with_their_parameters() {
    let list = run(
        "return { previews: $previews, groups: $groups.map((g) => [g.name, g.file, g.tests.map((t) => t.name)]) };",
    );
    assert_eq!(
        list["groups"],
        json!([
            ["Counter", "src/shop/Counter.k", ["mounts"]],
            ["home", "src/shop/Home.k", ["mounts"]],
        ])
    );
    let first = &list["previews"][0];
    assert_eq!(first["label"], json!("Counter"));
    assert_eq!(first["params"], json!(["value", "label"]));
    let text = std::fs::read_to_string(project().join("src/shop/Counter.k")).expect("reads");
    let start = usize::try_from(first["start"].as_u64().expect("a start")).expect("fits");
    assert_eq!(&text[start..start + "preview".len()], "preview");
}

#[test]
fn every_preview_mounts_with_its_defaults() {
    let results = run(
        "return await kit.run($groups, $groups.map((g) => `${g.name} › mounts`), server, $pages ?? []);",
    );
    assert_eq!(results.as_array().map(Vec::len), Some(2), "{results}");
    for r in results.as_array().into_iter().flatten() {
        assert!(r.get("failure").is_none_or(Value::is_null), "{r}");
    }
}

#[test]
fn mount_takes_values_and_a_binding_writes_back() {
    passed(&custom(
        "Counter",
        "const s = await this.$mount($c, { value: 5 });\n\
         await s.settle();\n\
         if (!s.text(\"Items: 5\").exists) throw new Error(`given: ${document.body.textContent}`);\n\
         s.button(\"Add\").click();\n\
         await s.settle();\n\
         if (!s.text(\"Items: 6\").exists) throw new Error(`written back: ${document.body.textContent}`);",
    ));
}

#[test]
fn a_page_renders_inside_its_layouts() {
    passed(&custom(
        "home",
        "const s = await this.$mount($c);\n\
         await s.settle();\n\
         const text = document.body.textContent;\n\
         if (!text.includes(\"Shop\") || !text.includes(\"Welcome\")) throw new Error(`shown: ${text}`);",
    ));
}

#[test]
fn no_other_bundle_has_the_previews() {
    for entry in [Entry::App, Entry::Tests] {
        let output = build(entry.clone());
        for f in output.client.iter().chain(output.server.iter()) {
            assert!(
                !f.contents.contains("$mount") && !f.contents.contains("$previews"),
                "{entry:?}: {} has preview code",
                f.path
            );
        }
    }
    let previews = build(Entry::Previews(None));
    assert!(
        previews
            .client
            .iter()
            .any(|f| f.contents.contains("$mount")),
        "the preview bundle has them"
    );
}
