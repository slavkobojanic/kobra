//! The codegen API contract: it refuses erroneous programs, generates
//! deterministic output for clean ones (never panicking, the example apps
//! included), keeps server code out of client files, and the split
//! checker finds server code in client files.

use k_codegen::{
    CodegenError, FileKind, Options, Output, OutputFile, check_client_bundle, generate,
};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};

fn analyse(files: &[(&str, &str)]) -> AnalysisHost {
    let files = files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).expect("path"), (*t).to_owned()))
        .collect::<Vec<_>>();
    AnalysisHost::with_files(SemaOptions::new(), files)
}

const TOML: &str = "[package]\nname = \"demo\"\nk = \"0.1\"\n";

#[test]
fn a_program_with_errors_is_refused() {
    let host = analyse(&[
        ("k.toml", TOML),
        ("src/demo/a.k", "func f() -> Int {\n    return \"x\"\n}\n"),
    ]);
    assert!(matches!(
        generate(&host.analysis(), &Options::default()),
        Err(CodegenError::HasErrors { count: 1 })
    ));
}

#[test]
fn a_clean_program_generates_both_bundles() {
    let host = analyse(&[
        ("k.toml", TOML),
        ("src/demo/a.k", "func f() -> Int {\n    return 1\n}\n"),
    ]);
    let out = generate(&host.analysis(), &Options::default()).expect("generates");
    let server = out
        .server
        .iter()
        .find(|f| f.path == "k:/app/demo.js")
        .expect("the module's server file");
    assert!(
        server.contents.contains("export function f()"),
        "{}",
        server.contents
    );
    assert!(
        out.client.iter().any(|f| f.path.starts_with("runtime.")),
        "the runtime chunk"
    );
    assert!(
        out.client
            .iter()
            .any(|f| f.contents.contains("export function f()"))
    );
    assert_eq!(out.manifest.name, "demo");
}

#[test]
fn generation_is_deterministic() {
    let files = [
        ("k.toml", TOML),
        (
            "src/demo/a.k",
            "struct P implements Codable {\n    let x: Int\n}\n\nfunc f(p: P) -> String {\n    return \"{p.x}\"\n}\n",
        ),
    ];
    let a = generate(&analyse(&files).analysis(), &Options::default()).expect("generates");
    let b = generate(&analyse(&files).analysis(), &Options::default()).expect("generates");
    assert_eq!(a, b);
}

/// Loads every file of a project directory.
fn project(dir: &std::path::Path) -> Vec<(ProjectPath, String)> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(ProjectPath, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if let Ok(p) = ProjectPath::new(&rel) {
                    out.push((p, text));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn the_example_apps_generate_without_server_code_in_the_client() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut seen = 0;
    for entry in std::fs::read_dir(&root).expect("examples").flatten() {
        let dir = entry.path();
        if !dir.join("k.toml").exists() {
            continue;
        }
        seen += 1;
        let host = AnalysisHost::with_files(SemaOptions::new(), project(&dir));
        for profile in [
            k_codegen::Profile::Development,
            k_codegen::Profile::Release,
            k_codegen::Profile::Test,
        ] {
            let options = Options {
                profile,
                ..Options::default()
            };
            match generate(&host.analysis(), &options) {
                Ok(out) => {
                    let found = check_client_bundle(&out, &[]);
                    assert!(found.is_empty(), "{}: {found:?}", dir.display());
                }
                Err(CodegenError::Unsupported { what }) => {
                    panic!("{}: unsupported: {what}", dir.display());
                }
                Err(e) => panic!("{}: {e}", dir.display()),
            }
        }
    }
    assert!(seen >= 4, "the four example apps");
}

#[test]
fn the_split_checker_finds_server_code_in_client_files() {
    let output = Output {
        client: vec![
            OutputFile {
                path: "app.js".into(),
                contents: "import { signal } from \"k:runtime\";".into(),
                kind: FileKind::JavaScript,
            },
            OutputFile {
                path: "bad.js".into(),
                contents: "import \"k:runtime/server\"; const k = \"sk_live\";".into(),
                kind: FileKind::JavaScript,
            },
        ],
        ..Output::default()
    };
    let found = check_client_bundle(&output, &["sk_live"]);
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|v| v.file == "bad.js"));
}

/// The manifest lists the configuration the selected environment's
/// dependency graph reads (docs/design/25-config-and-secrets.md, "Startup
/// validation"): a config injected only by an implementation that the
/// environment's `use` rule doesn't choose is not required to start.
#[test]
fn the_manifest_lists_only_configs_the_environment_reaches() {
    use k_codegen::Environment;
    let host = analyse(&[
        ("k.toml", TOML),
        (
            "App.k",
            "import pay.Gateway\nimport pay.LiveGateway\nimport pay.FakeGateway\n\npublic app Demo {\n    use LiveGateway for Gateway in .production\n    use FakeGateway for Gateway in .development, .test\n}\n",
        ),
        (
            "src/pay/Gateway.k",
            "public interface Gateway {\n    func charge() -> String\n}\n",
        ),
        (
            "src/pay/LiveConfig.k",
            "public config LiveConfig {\n    @Env(\"LIVE_API_KEY\") let apiKey: Secret<String>\n}\n",
        ),
        (
            "src/pay/LiveGateway.k",
            "public service LiveGateway implements Gateway {\n    @Inject let config: LiveConfig\n\n    func charge() -> String {\n        return \"live\"\n    }\n}\n",
        ),
        (
            "src/pay/FakeGateway.k",
            "public service FakeGateway implements Gateway {\n    func charge() -> String {\n        return \"fake\"\n    }\n}\n",
        ),
        (
            "src/pay/ChargeProtocol.k",
            "import k.rpc.RpcOnlyError\n\npublic protocol ChargeProtocol {\n    rpc charge() throws RpcOnlyError -> String\n}\n",
        ),
        (
            "src/pay/ChargeApi.k",
            "import k.rpc.RpcOnlyError\n\npublic service ChargeApi implements ChargeProtocol {\n    @Inject let gateway: Gateway\n\n    rpc charge() async throws RpcOnlyError -> String {\n        return gateway.charge()\n    }\n}\n",
        ),
        (
            "src/pay/Home.k",
            "public page Home {\n    view {\n        heading(level: 1) { \"Pay\" }\n    }\n}\n",
        ),
    ]);
    let analysis = host.analysis();
    let envs = |environment| {
        let options = Options {
            environment,
            ..Options::default()
        };
        generate(&analysis, &options)
            .expect("generates")
            .manifest
            .config
            .iter()
            .map(|c| c.env.clone())
            .collect::<Vec<_>>()
    };
    let production = envs(Environment::Production);
    let development = envs(Environment::Development);
    assert!(
        production.iter().any(|e| e == "LIVE_API_KEY"),
        "{production:?}"
    );
    assert!(
        !development.iter().any(|e| e == "LIVE_API_KEY"),
        "{development:?}"
    );
    // The stdlib's own configuration is still listed.
    assert!(development.iter().any(|e| e == "PORT"), "{development:?}");
}

/// The generated client files of `examples/<name>` for development.
fn example_client(name: &str) -> Vec<OutputFile> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    let host = AnalysisHost::with_files(SemaOptions::new(), project(&dir));
    let out = generate(
        &host.analysis(),
        &Options {
            profile: k_codegen::Profile::Development,
            entry: k_codegen::Entry::App,
            ..Options::default()
        },
    )
    .expect("generates");
    out.client
}

fn entry_of(files: &[OutputFile]) -> &str {
    &files
        .iter()
        .find(|f| f.path.starts_with("app.") && f.path.ends_with(".js"))
        .expect("the client entry")
        .contents
}

#[test]
fn the_client_entry_builds_the_browser_container() {
    let files = example_client("todos");
    let entry = entry_of(&files);
    // App-scoped services are memoized on the app container and get their
    // dependencies from it; an interface with no project implementation is
    // the stdlib's default; client-only stdlib services are runtime bodies.
    for needle in [
        "class $App {",
        "get todo$TodoService() {",
        "return this.$a.$todo$TodoService ??= new todo$TodoService({ LocalStorage: this.$a.LocalStorage });",
        "get LocalStorage() {\n    return this.$a.BrowserStorage;",
        "$k.browserStorage(StorageReadError, StorageWriteError)",
        "get Theme() {\n    return this.$a.$Theme ??= $k.theme();",
        "const data = null;",
        "$k.startRouter(routes, container, { c: () => app });",
    ] {
        assert!(entry.contains(needle), "missing {needle:?} in\n{entry}");
    }
    assert!(!entry.contains("class $Route"), "no route-scoped services");
}

#[test]
fn a_server_rendered_entry_starts_clock_session_and_rpc_clients_from_k_data() {
    let files = example_client("notes");
    let entry = entry_of(&files);
    for needle in [
        "const data = $k.readData(document);",
        "const app = new $App(data);",
        "{ c: () => app, data }",
        "$k.clock(this.$a.$d)",
        "$k.session(this.$a.$d)",
        "new AuthProtocol$client()",
    ] {
        assert!(entry.contains(needle), "missing {needle:?} in\n{entry}");
    }
}

#[test]
fn test_only_modules_never_ship_or_bind_outside_tests() {
    let files = example_client("weather");
    let entry = entry_of(&files);
    assert!(
        entry.contains("$k.fetchHttpClient(HttpError, HttpStatus)"),
        "{entry}"
    );
    for f in &files {
        assert!(
            !f.contents.contains("FakeHttpClient") && !f.contents.contains("FakeWeatherService"),
            "{} ships a fake from `testing`",
            f.path
        );
    }
}

#[test]
fn hydrated_load_values_are_decoded_and_server_loads_are_framed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/shop");
    let host = AnalysisHost::with_files(SemaOptions::new(), project(&dir));
    let out = generate(
        &host.analysis(),
        &Options {
            profile: k_codegen::Profile::Development,
            entry: k_codegen::Entry::App,
            ..Options::default()
        },
    )
    .expect("generates");
    let home = out
        .client
        .iter()
        .find(|f| f.path.starts_with("pages/home."))
        .expect("the home chunk");
    assert!(
        home.contents.contains(
            "(($v) => $v && { products: $k.codecs.list(ProductSummary$codec).decode($v.products, \"products\") })($k.loadsOf(ctx))"
        ),
        "{}",
        home.contents
    );
    // The client's load branch is a block range, so the server frames the
    // view it renders with block markers.
    let server = out
        .server
        .iter()
        .find(|f| f.contents.contains("export async function Home$ssr"))
        .expect("the home renderer");
    let start = server
        .contents
        .find("export async function Home$ssr")
        .expect("found");
    let body = &server.contents[start..];
    let view = body
        .find("out.raw(\"<h1>New this season</h1>\")")
        .expect("the view");
    assert!(
        body[..view].trim_end().ends_with("out.raw($s.OPEN);"),
        "{}",
        &body[..view]
    );
}

/// The shop's server entry for `environment`.
fn shop_server(environment: k_codegen::Environment) -> (String, Output) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/shop");
    let host = AnalysisHost::with_files(SemaOptions::new(), project(&dir));
    let out = generate(
        &host.analysis(),
        &Options {
            profile: k_codegen::Profile::Development,
            entry: k_codegen::Entry::App,
            environment,
        },
    )
    .expect("generates");
    let entry = out
        .server
        .iter()
        .find(|f| Some(&f.path) == out.server_entry.as_ref())
        .expect("a server entry")
        .contents
        .clone();
    (entry, out)
}

#[test]
fn server_platform_interfaces_bind_to_the_stdlib_implementations() {
    let (entry, out) = shop_server(k_codegen::Environment::Production);
    for needle in [
        "get HttpClient() {\n    return this.FetchHttpClient;",
        "$s.fetchHttpClient(HttpError, HttpStatus, [200, 201,",
        "get EmailProvider() {\n    return this.SmtpEmailProvider;",
        "$s.smtpEmailProvider(EmailError, $s.info().config)",
        "get JobQueue() {\n    return this.DatabaseJobQueue;",
        "$s.jobQueue(this.Database)",
    ] {
        assert!(entry.contains(needle), "missing {needle:?} in\n{entry}");
    }
    // The enums the container passes are generated, not missing natives.
    let http = out
        .server
        .iter()
        .find(|f| f.path == "k:/app/k/http.js")
        .expect("k.http");
    assert!(
        http.contents.contains("export const HttpError = {"),
        "{}",
        http.contents
    );
    // SMTP is configured because the reachable graph sends email.
    for env in ["SMTP_HOST", "MAIL_FROM"] {
        assert!(
            out.manifest
                .config
                .iter()
                .any(|c| c.env == env && c.required),
            "{env} is required"
        );
    }
    let security = out
        .manifest
        .config
        .iter()
        .find(|c| c.env == "SMTP_SECURITY")
        .expect("SMTP_SECURITY");
    assert_eq!(security.cases, ["startTls", "tls", "none"]);
}

#[test]
fn a_test_build_binds_the_k_test_doubles() {
    let (entry, _) = shop_server(k_codegen::Environment::Test);
    for needle in [
        "return this.TestHttpClient;",
        "$s.testHttpClient(HttpError, HttpStatus,",
        "return this.RecordingEmailProvider;",
        "return this.RecordingJobQueue;",
    ] {
        assert!(entry.contains(needle), "missing {needle:?} in\n{entry}");
    }
}

#[test]
fn jobs_get_a_kind_a_spec_entry_and_a_manifest_entry() {
    let (entry, out) = shop_server(k_codegen::Environment::Production);
    assert!(
        entry.contains(r#""order-confirmation": { run: (c, $c, j) => c.store$OrderConfirmationJob.run($c, OrderConfirmation$codec.decode(j, "payload")) }"#),
        "{entry}"
    );
    let store = out
        .server
        .iter()
        .find(|f| f.path == "k:/app/store.js")
        .expect("store");
    assert!(store.contents.contains(
        r#"store$OrderConfirmationJob.kind = Object.freeze({ name: "order-confirmation" });"#
    ));
    assert_eq!(
        out.manifest.jobs,
        [k_db::stdlib::JobSpec {
            name: "order-confirmation".into(),
            declaration: "store.OrderConfirmationJob".into(),
            attempts: 8,
            backoff_ms: 30_000,
            schedule: None,
        }]
    );
}

#[test]
fn in_process_protocol_clients_go_through_the_rpc_table() {
    let (entry, _) = shop_server(k_codegen::Environment::Production);
    assert!(
        entry.contains(r#"return $s.callLocal(this.c, $spec.rpcs["/shop.store.StoreProtocol/PlaceOrder"], $c, a);"#),
        "{entry}"
    );
    assert!(
        entry.contains("failure: (f) => CheckoutError.rpcFailure(f),"),
        "{entry}"
    );
}

#[test]
fn the_runtime_status_table_follows_the_stdlib_enum() {
    // `HTTP_CODES` in std.js maps `HttpStatus` case indexes to codes; it
    // must list the named cases in declaration order.
    let (entry, _) = shop_server(k_codegen::Environment::Production);
    let start = entry.find("httpStatus: [").expect("the spec's table") + "httpStatus: [".len();
    let table = &entry[start..start + entry[start..].find(']').expect("end")];
    let generated: Vec<&str> = table.split(", ").filter(|c| *c != "null").collect();
    let std = k_runtime::module("k:/runtime/std.js")
        .expect("std.js")
        .source;
    let line = std
        .lines()
        .find(|l| l.starts_with("export const HTTP_CODES = ["))
        .expect("HTTP_CODES");
    let runtime: Vec<&str> = line
        .trim_start_matches("export const HTTP_CODES = [")
        .trim_end_matches("];")
        .split(", ")
        .collect();
    assert_eq!(generated, runtime);
}
