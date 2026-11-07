//! Generated client bundles run: the runtime chunk, the shared chunk and
//! the components in it load as ES modules in V8 (with the fake DOM), a
//! factory builds its DOM, and a click updates it; the server renderer of
//! the same component produces the HTML the client would claim
//! (`docs/compiler/codegen.md` §10, §11, §17).

use k_codegen::{FileKind, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::{Engine, EngineOptions, JsModule};

const TOML: &str = "[package]\nname = \"demo\"\nk = \"0.1\"\n";

const COUNTER: &str = r#"public component Counter {
    @Prop let label: String
    @State var count: Int = 0
    @State var items: List<String> = ["a"]

    func increment() {
        count += 1
        items.append("n{count}")
    }

    view {
        box {
            text { "{label}: {count}" }
            button { "Add" }
                .on(click: increment)
            list {
                for entry in items key entry {
                    item { entry }
                }
            }
            if count > 1 {
                paragraph { "many" }
            }
        }
    }
}
"#;

fn generate_counter(profile: Profile) -> Output {
    let host = AnalysisHost::with_files(
        SemaOptions::new(),
        [
            (ProjectPath::new("k.toml").expect("path"), TOML.to_owned()),
            (
                ProjectPath::new("src/demo/Counter.k").expect("path"),
                COUNTER.to_owned(),
            ),
        ],
    );
    let analysis = host.analysis();
    generate(
        &analysis,
        &Options {
            profile,
            ..Options::default()
        },
    )
    .expect("generates")
}

fn run(modules: Vec<JsModule>, main: &str) -> String {
    let mut engine = Engine::new(EngineOptions {
        modules,
        timeout: Some(std::time::Duration::from_secs(10)),
        ..Default::default()
    })
    .expect("engine");
    engine.run_main(main).expect("runs").expect("a result")
}

fn client_modules(output: &Output) -> (Vec<JsModule>, String) {
    let mut shared = String::new();
    let modules = output
        .client
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript)
        .map(|f| {
            if f.path.starts_with("shared.") {
                shared = format!("k:/client/{}", f.path);
            }
            JsModule {
                specifier: format!("k:/client/{}", f.path),
                source: f.contents.clone(),
            }
        })
        .collect();
    (modules, shared)
}

fn clicks_update_the_dom(profile: Profile) {
    let output = generate_counter(profile);
    let (modules, shared) = client_modules(&output);
    let main = format!(
        r#"import {{ installDom }} from "k:testing";
installDom();
const m = await import({shared:?});
const root = m.Counter({{ label: () => "Count" }}, {{ c: {{}} }});
document.body.appendChild(root);
const before = root.outerHTML;
const button = root.childNodes[1];
button.click();
button.click();
Deno.core.ops.op_k_set_result(JSON.stringify([before, root.outerHTML]));
"#
    );
    let result = run(modules, &main);
    let [before, after]: [String; 2] = serde_json::from_str(&result).expect("json");
    assert_eq!(
        before,
        "<div><span>Count: 0</span><button>Add</button><ul><!--[--><li>a</li><!--]--></ul><!--[--><!--]--></div>"
    );
    assert_eq!(
        after,
        "<div><span>Count: 2</span><button>Add</button><ul><!--[--><li>a</li><li>n1</li><li>n2</li><!--]--></ul><!--[--><p>many</p><!--]--></div>"
    );
}

#[test]
fn a_generated_component_renders_and_reacts_in_development_builds() {
    clicks_update_the_dom(Profile::Development);
}

#[test]
fn a_generated_component_renders_and_reacts_in_release_builds() {
    clicks_update_the_dom(Profile::Release);
}

#[test]
fn the_server_renderer_writes_the_markup_the_client_claims() {
    let output = generate_counter(Profile::Development);
    let modules = output
        .server
        .iter()
        .map(|f| JsModule {
            specifier: f.path.clone(),
            source: f.contents.clone(),
        })
        .collect();
    let main = r#"import { Out } from "k:runtime/server";
import { Counter$ssr } from "k:/app/demo.js";
const out = new Out();
Counter$ssr({ label: () => "<b>" }, { c: {} }, out);
Deno.core.ops.op_k_set_result(out.html());
"#;
    assert_eq!(
        run(modules, main),
        "<div><span>&lt;b&gt;: 0</span><button>Add</button><ul><!--[--><li>a</li><!--]--></ul><!--[--><!--]--></div>"
    );
}
