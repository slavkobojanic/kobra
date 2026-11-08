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

// ---------------------------------------------------------- the container

/// An app whose services cover the browser's scopes (codegen.md §13):
/// `Serials` is app-scoped, `Visit` lives while its page is mounted, and
/// each `Draft` belongs to the component that injects it and its subtree.
const SCOPES: &[(&str, &str)] = &[
    ("App.k", "@Rendering(.client)\npublic app Demo {\n}\n"),
    (
        "src/demo/Serials.k",
        r#"public service Serials {
    var last: Int = 0

    func take() -> Int {
        last += 1
        return last
    }
}

@Scope(.route)
service Visit {
    @Inject let serials: Serials
    var id: Int = 0

    init() {
        id = serials.take()
    }
}

@Scope(.component)
service Draft {
    @Inject let serials: Serials
    var id: Int = 0

    init() {
        id = serials.take()
    }
}
"#,
    ),
    (
        "src/demo/Editor.k",
        r#"component Field {
    @Inject let draft: Draft

    view {
        paragraph { "field {draft.id}" }
    }
}

public component Editor {
    @Inject let draft: Draft
    @Inject let visit: Visit

    view {
        box {
            paragraph { "editor {draft.id} in visit {visit.id}" }
            Field()
        }
    }
}

"#,
    ),
    (
        "src/demo/Home.k",
        r#"public page Home at "/" {
    @Inject let visit: Visit

    view {
        paragraph { "home visit {visit.id}" }
        Editor()
        Editor()
        link(to: .other) { "other" }
    }
}

"#,
    ),
    (
        "src/demo/Other.k",
        r#"public page Other {
    @Inject let visit: Visit

    view {
        paragraph { "other visit {visit.id}" }
        link(to: .home) { "home" }
    }
}
"#,
    ),
];

#[test]
fn the_browser_container_scopes_services_per_app_route_and_component() {
    let mut files = vec![(ProjectPath::new("k.toml").expect("path"), TOML.to_owned())];
    for (p, t) in SCOPES {
        files.push((ProjectPath::new(p).expect("path"), (*t).to_owned()));
    }
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().expect("not cancelled");
    let errors: Vec<_> = diagnostics.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let output = generate(
        &analysis,
        &Options {
            profile: Profile::Development,
            entry: k_codegen::Entry::App,
            ..Options::default()
        },
    )
    .expect("generates");
    let (modules, _) = client_modules(&output);
    let entry = output
        .client
        .iter()
        .find(|f| f.path.starts_with("app.") && f.path.ends_with(".js"))
        .map(|f| format!("k:/client/{}", f.path))
        .expect("a client entry");
    // Each step is its own module, so the router's page chunks load (an
    // event-loop turn) between them.
    let boot = format!(
        r#"import {{ installDom }} from "k:testing";
installDom();
document.body.innerHTML = '<div id="k-app"></div>';
const loc = {{ origin: "http://localhost", pathname: "/", search: "" }};
globalThis.location = loc;
globalThis.history = {{ pushState(_s, _t, p) {{ loc.pathname = p; }} }};
globalThis.seen = [];
globalThis.click = (label) => {{
  const visit = (n) => {{
    if (n.nodeType === 1 && n.localName === "a" && n.textContent === label) return n;
    for (const c of n.childNodes) {{ const f = visit(c); if (f) return f; }}
    return null;
  }};
  const e = new Event("click", {{ bubbles: true }});
  e.button = 0;
  visit(document.body).dispatchEvent(e);
}};
await import({entry:?});
"#
    );
    let mut engine = Engine::new(EngineOptions {
        modules,
        timeout: Some(std::time::Duration::from_secs(10)),
        ..Default::default()
    })
    .expect("engine");
    let read = "seen.push(document.getElementById(\"k-app\").textContent);";
    for step in [
        boot.as_str(),
        read,
        "click(\"other\");",
        read,
        "click(\"home\");",
        read,
    ] {
        engine.run_main(step).expect("runs");
    }
    let result = engine
        .run_main("Deno.core.ops.op_k_set_result(JSON.stringify(seen));")
        .expect("runs")
        .expect("a result");
    let seen: Vec<String> = serde_json::from_str(&result).expect("json");
    assert_eq!(
        seen,
        [
            "home visit 1editor 2 in visit 1field 2editor 3 in visit 1field 3other",
            "other visit 4home",
            "home visit 5editor 6 in visit 5field 6editor 7 in visit 5field 7other",
        ]
    );
}

// ------------------------------------------------------- time of day

/// A component that reads a `TimeOfDay` outside a job schedule: the literal,
/// its text, its hour and minute, a parsed one and equality (35-time.md,
/// "TimeOfDay").
const OPENING_HOURS: &str = r#"import k.time.TimeOfDay

public component OpeningHours {
    @Prop let opens: TimeOfDay = TimeOfDay("09:30")
    @State var closes: TimeOfDay? = TimeOfDay.parse("17:05")

    view {
        paragraph { "opens {opens} ({opens.hour}h {opens.minute}m)" }
        if let c = closes {
            paragraph { "closes {c.hour}:{c.minute} {c == TimeOfDay("17:05")}" }
        }
    }
}
"#;

#[test]
fn a_time_of_day_renders_alike_in_the_browser_and_on_the_server() {
    let host = AnalysisHost::with_files(
        SemaOptions::new(),
        [
            (ProjectPath::new("k.toml").expect("path"), TOML.to_owned()),
            (
                ProjectPath::new("src/demo/OpeningHours.k").expect("path"),
                OPENING_HOURS.to_owned(),
            ),
        ],
    );
    let analysis = host.analysis();
    let output = generate(&analysis, &Options::default()).expect("generates");
    let (modules, shared) = client_modules(&output);
    let client = run(
        modules,
        &format!(
            r#"import {{ installDom }} from "k:testing";
installDom();
const m = await import({shared:?});
const root = document.createElement("div");
root.appendChild(m.OpeningHours({{}}, {{ c: {{}} }}));
Deno.core.ops.op_k_set_result(root.textContent);
"#
        ),
    );
    let modules = output
        .server
        .iter()
        .map(|f| JsModule {
            specifier: f.path.clone(),
            source: f.contents.clone(),
        })
        .collect();
    let server = run(
        modules,
        r#"import { Out } from "k:runtime/server";
import { OpeningHours$ssr } from "k:/app/demo.js";
const out = new Out();
OpeningHours$ssr({}, { c: {} }, out);
Deno.core.ops.op_k_set_result(out.html());
"#,
    );
    assert_eq!(client, "opens 09:30 (9h 30m)closes 17:5 true");
    assert!(
        server.contains("opens 09:30 (9h 30m)") && server.contains("closes 17:5 true"),
        "{server}"
    );
}
