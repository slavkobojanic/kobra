//! The codegen API contract: it refuses erroneous programs, generates
//! deterministic output for clean ones (never panicking, the example apps
//! included), keeps server code out of client files, and the split
//! checker finds server code in client files.

use k_codegen::{
    CodegenError, FileKind, Options, Output, OutputFile, check_client_bundle, generate,
};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};

/// An analysis host over `files`, given as `(project path, text)` pairs.
///
/// # Panics
///
/// If a path is not a valid project path.
fn analyse(files: &[(&str, &str)]) -> AnalysisHost {
    let files = files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).expect("path"), (*t).to_owned()))
        .collect::<Vec<_>>();
    AnalysisHost::with_files(SemaOptions::new(), files)
}

/// The `k.toml` of the test projects: a package named `demo`.
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

/// The shop member of the `examples/shop` workspace, with the
/// `pay-contracts` library it depends on mounted.
fn shop_files() -> Vec<(ProjectPath, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/shop");
    let ws = k_sema::Workspace::from_files(
        project(&dir)
            .into_iter()
            .map(|(p, t)| (p.as_str().to_owned(), t)),
    );
    let shop = ws.member("shop").expect("the shop member");
    ws.analysis_files(shop)
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
        let files = project(&dir);
        // A workspace example (`examples/stack`): every app member, with
        // the libraries it depends on.
        let manifest = std::fs::read_to_string(dir.join("k.toml")).unwrap_or_default();
        let hosts: Vec<AnalysisHost> = if k_sema::Workspace::is_workspace_manifest(&manifest) {
            let ws = k_sema::Workspace::from_files(
                files.into_iter().map(|(p, t)| (p.as_str().to_owned(), t)),
            );
            assert!(
                ws.problems.is_empty(),
                "{}: {:?}",
                dir.display(),
                ws.problems
            );
            (0..ws.members.len())
                .filter(|&m| ws.members[m].is_app)
                .map(|m| AnalysisHost::with_files(SemaOptions::new(), ws.analysis_files(m)))
                .collect()
        } else {
            vec![AnalysisHost::with_files(SemaOptions::new(), files)]
        };
        for host in hosts {
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

/// A config field of type `Secret<T>?` is a secret like `Secret<T>`: it may
/// come from `NAME_FILE`, and `dist/app config` marks it as one.
#[test]
fn an_optional_secret_config_field_is_listed_as_a_secret() {
    let host = analyse(&[
        ("k.toml", TOML),
        ("App.k", "public app Demo {}\n"),
        (
            "src/hooks/HookConfig.k",
            "public config HookConfig {\n    @Env(\"HOOK_TOKEN\") let token: Secret<String>? = nil\n    @Env(\"HOOK_KEY\") let key: Secret<String>\n    @Env(\"HOOK_NAME\") let name: String? = nil\n}\n",
        ),
        (
            "src/hooks/HookProtocol.k",
            "import k.rpc.RpcOnlyError\n\npublic protocol HookProtocol {\n    rpc ping() throws RpcOnlyError -> String\n}\n",
        ),
        (
            "src/hooks/HookApi.k",
            "import k.rpc.RpcOnlyError\n\npublic service HookApi implements HookProtocol {\n    @Inject let config: HookConfig\n\n    rpc ping() async throws RpcOnlyError -> String {\n        return config.name ?? \"none\"\n    }\n}\n",
        ),
    ]);
    let out = generate(&host.analysis(), &Options::default()).expect("generates");
    let entry = |env: &str| {
        out.manifest
            .config
            .iter()
            .find(|c| c.env == env)
            .unwrap_or_else(|| panic!("{env} in {:?}", out.manifest.config))
    };
    assert!(entry("HOOK_TOKEN").secret, "{:?}", entry("HOOK_TOKEN"));
    assert!(!entry("HOOK_TOKEN").required);
    assert!(entry("HOOK_KEY").secret);
    assert!(!entry("HOOK_NAME").secret);
    // The server sends a secret's value wrapped (`{"$secret": …}`), so the
    // config loader unwraps it into a `Secret`, or `nil` when it is unset.
    let loader = out
        .server
        .iter()
        .find(|f| f.contents.contains("function HookConfig$load"))
        .expect("the config loader");
    for needle in [
        "token: $s.configSecret(\"HOOK_TOKEN\")",
        "key: $s.configSecret(\"HOOK_KEY\")",
        "name: $s.config(\"HOOK_NAME\")",
    ] {
        assert!(
            loader.contents.contains(needle),
            "{needle} in\n{}",
            loader.contents
        );
    }
}

/// A config field of a key type (33-crypto.md, "Keys in configuration")
/// is read by `k.crypto` from the text startup checked: a private key in a
/// `Secret`, a public key as itself; and its manifest entry is a secret
/// exactly when the field is one.
#[test]
fn key_config_fields_are_parsed_as_keys() {
    let host = analyse(&[
        ("k.toml", TOML),
        ("App.k", "public app Demo {}\n"),
        (
            "src/sign/SignConfig.k",
            "import k.crypto.Ed25519PrivateKey\nimport k.crypto.Ed25519PublicKey\n\npublic config SignConfig {\n    @Env(\"SIGNING_KEY\") let signing: Secret<Ed25519PrivateKey>\n    @Env(\"PEER_KEY\") let peer: Ed25519PublicKey\n}\n",
        ),
        (
            "src/sign/SignProtocol.k",
            "import k.rpc.RpcOnlyError\n\npublic protocol SignProtocol {\n    rpc fingerprint() throws RpcOnlyError -> String\n}\n",
        ),
        (
            "src/sign/SignApi.k",
            "import k.rpc.RpcOnlyError\n\npublic service SignApi implements SignProtocol {\n    @Inject let config: SignConfig\n\n    rpc fingerprint() async throws RpcOnlyError -> String {\n        return await config.signing.publicKey.fingerprint() + (await config.peer.fingerprint())\n    }\n}\n",
        ),
    ]);
    let out = generate(&host.analysis(), &Options::default()).expect("generates");
    let entry = |env: &str| {
        out.manifest
            .config
            .iter()
            .find(|c| c.env == env)
            .unwrap_or_else(|| panic!("{env} in {:?}", out.manifest.config))
    };
    assert!(entry("SIGNING_KEY").secret);
    assert_eq!(entry("SIGNING_KEY").ty, "Secret<Ed25519PrivateKey>");
    assert!(!entry("PEER_KEY").secret);
    let loader = out
        .server
        .iter()
        .find(|f| f.contents.contains("function SignConfig$load"))
        .expect("the config loader");
    for needle in [
        "signing: $s.configKey(\"SIGNING_KEY\", \"Ed25519PrivateKey\", true)",
        "peer: $s.configKey(\"PEER_KEY\", \"Ed25519PublicKey\", false)",
    ] {
        assert!(
            loader.contents.contains(needle),
            "{needle} in\n{}",
            loader.contents
        );
    }
}

/// A stdlib `config` that app code injects (`k.rpc.ServiceConfig`) is
/// built from the environment like the app's own configs, and its
/// variables are listed, even when nothing else would require them.
#[test]
fn an_injected_stdlib_config_is_read_from_the_environment() {
    let host = analyse(&[
        ("k.toml", TOML),
        ("App.k", "public app Demo {}\n"),
        (
            "src/hooks/Signer.k",
            "import k.rpc.ServiceConfig\n\npublic service Signer {\n    @Inject let service: ServiceConfig\n\n    func secret() -> Secret<String> {\n        return service.token\n    }\n}\n",
        ),
        (
            "src/hooks/HookProtocol.k",
            "import k.rpc.RpcOnlyError\n\npublic protocol HookProtocol {\n    rpc ping() throws RpcOnlyError -> Bool\n}\n",
        ),
        (
            "src/hooks/HookApi.k",
            "import k.rpc.RpcOnlyError\n\npublic service HookApi implements HookProtocol {\n    @Inject let signer: Signer\n\n    rpc ping() async throws RpcOnlyError -> Bool {\n        return signer.secret().matches(\"x\")\n    }\n}\n",
        ),
    ]);
    let out = generate(&host.analysis(), &Options::default()).expect("generates");
    let token = out
        .manifest
        .config
        .iter()
        .find(|c| c.env == "K_SERVICE_TOKEN")
        .unwrap_or_else(|| panic!("K_SERVICE_TOKEN in {:?}", out.manifest.config));
    assert!(token.secret && token.required, "{token:?}");
    let entry = out
        .server
        .iter()
        .find(|f| Some(&f.path) == out.server_entry.as_ref())
        .expect("the entry");
    assert!(
        entry
            .contents
            .contains("token: $s.configSecret(\"K_SERVICE_TOKEN\")"),
        "{}",
        entry.contents
    );
    assert!(
        !entry
            .contents
            .contains("missingNative(\"k.rpc.ServiceConfig\")"),
        "{}",
        entry.contents
    );
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

/// The contents of the client entry chunk (`app.<hash>.js`) among `files`.
///
/// # Panics
///
/// If there is no client entry.
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
    // An app service is built with no reactive owner, so a page that first
    // reads it never disposes its computeds. A client-rendered shell's
    // `k-data` only starts the session mirror: the router gets it as
    // `session`, never as `data`, so nothing hydrates.
    for needle in [
        "class $App {",
        // `test TodoService { }` beside the service leaves it its simple name.
        "get TodoService() {",
        "return this.$a.$TodoService ??= $k.runWithOwner(null, () => new TodoService({ LocalStorage: this.$a.LocalStorage }));",
        "get LocalStorage() {\n    return this.$a.BrowserStorage;",
        // Keys are namespaced with storage.lock's namespace (36-local-storage.md).
        "$k.browserStorage(StorageReadError, StorageWriteError, \"todos\")",
        "get Theme() {\n    return this.$a.$Theme ??= $k.theme();",
        "const data = $k.readData(document);",
        "$k.startRouter(routes, container, { c: () => app, session: data });",
    ] {
        assert!(entry.contains(needle), "missing {needle:?} in\n{entry}");
    }
    assert!(!entry.contains("class $Route"), "no route-scoped services");
}

/// `PublicUrls` in the browser (12-routing.md, "Absolute URLs"): the
/// page's typed URL against `location.origin` with the runtime's
/// `pageUrl`, and no endpoint URLs, which are server-only.
#[test]
fn the_browser_container_builds_absolute_page_urls() {
    let files = example_client("kanban");
    let entry = entry_of(&files);
    for needle in [
        "get PublicUrls() {",
        "absolute: (p) => $k.pageUrl(Page$href(p))",
        "absolute$endpoint: $k.missingNative(\"k.server.PublicUrls.absolute(endpoint:)\")",
    ] {
        assert!(entry.contains(needle), "missing {needle:?} in\n{entry}");
    }
    assert!(
        files.iter().all(|f| !f.contents.contains("Endpoint$href")),
        "no endpoint URLs in the browser"
    );
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

/// The client route table's entry for the page whose case name is `name`
/// in `entry`: the text from its `name:` to the next route's.
///
/// # Panics
///
/// If the table has no such route.
fn client_route<'a>(entry: &'a str, name: &str) -> &'a str {
    let start = entry.find(&format!("name: \"{name}\""));
    assert!(start.is_some(), "no route {name} in\n{entry}");
    let start = start.unwrap_or_default();
    let rest = &entry[start..];
    let end = rest[1..].find("name: \"").map_or(rest.len(), |i| i + 1);
    &rest[..end]
}

#[test]
fn the_client_route_table_carries_the_guards_the_browser_can_check() {
    // Notes: every notes page requires `Authenticated`; the account pages
    // are open (12-routing.md, "Guards: `requires`").
    let files = example_client("notes");
    let entry = entry_of(&files);
    for page in ["notes", "newNote", "editNote"] {
        let r = client_route(entry, page);
        assert!(r.contains("guards: [[\"auth\"]]"), "{page}: {r}");
    }
    for page in ["signIn", "signUp"] {
        let r = client_route(entry, page);
        assert!(!r.contains("guards"), "{page} is open: {r}");
    }
    // Shop: `Role(.admin)` is the role's case name, as the `Session`
    // mirror lists roles; `RateLimit` is the server's alone, so no client
    // table has one.
    let files = shop_client();
    let entry = entry_of(&files);
    let admin = client_route(entry, "admin");
    assert!(
        admin.contains("guards: [[\"auth\"], [\"role\", \"admin\"]]"),
        "{admin}"
    );
    assert!(!client_route(entry, "home").contains("guards"));
    for f in &files {
        assert!(
            !f.contents.contains("[\"rate\""),
            "{} has a rate guard",
            f.path
        );
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

/// The generated client files of the shop member for development.
fn shop_client() -> Vec<OutputFile> {
    let host = AnalysisHost::with_files(SemaOptions::new(), shop_files());
    generate(
        &host.analysis(),
        &Options {
            profile: k_codegen::Profile::Development,
            entry: k_codegen::Entry::App,
            ..Options::default()
        },
    )
    .expect("generates")
    .client
}

/// An endpoint page has no view and runs on the server only, so what it
/// injects is not in the browser's containers: the shop's
/// `StripeWebhook` injects `StripeEvents`, bound to `StripeDisabled`, and
/// neither reaches a client file (codegen.md §16).
#[test]
fn what_only_an_endpoint_page_injects_never_ships_to_the_browser() {
    for f in &shop_client() {
        for needle in ["StripeDisabled", "StripeWebhookVerifier", "StripeEvents"] {
            assert!(
                !f.contents.contains(needle),
                "{} ships {needle}, which only an endpoint page injects",
                f.path
            );
        }
    }
}

/// The client file of `files` whose name starts with `stem`, if any.
fn chunk<'a>(files: &'a [OutputFile], stem: &str) -> Option<&'a OutputFile> {
    files.iter().find(|f| f.path.starts_with(stem))
}

#[test]
fn forms_and_money_ship_only_to_clients_that_use_them() {
    // `@Form` support and `Money`/`Decimal` are feature chunks
    // (runtime.md §2): the runtime chunk has neither, and a client that
    // uses one reaches it through a namespace import of its chunk.
    let weather = example_client("weather");
    for stem in ["forms.", "money."] {
        assert!(chunk(&weather, stem).is_none(), "weather ships {stem}");
    }
    let runtime = chunk(&weather, "runtime.").expect("a runtime chunk");
    for gone in ["function createForm(", "class Money", "class Decimal"] {
        assert!(
            !runtime.contents.contains(gone),
            "the runtime chunk has {gone}"
        );
    }

    let kanban = example_client("kanban");
    let forms = chunk(&kanban, "forms.").expect("kanban ships the forms chunk");
    assert!(forms.contents.contains("export function createForm("));
    assert!(chunk(&kanban, "money.").is_none(), "kanban has no money");
    let mut reached = false;
    for f in &kanban {
        assert!(
            !f.contents.contains("$k.createForm"),
            "{} calls $k.createForm",
            f.path
        );
        if f.contents.contains("$k_forms.createForm(") {
            reached = true;
            let prefix = if f.path.starts_with("pages/") {
                "../"
            } else {
                "./"
            };
            let import = format!("import * as $k_forms from \"{prefix}{}\";", forms.path);
            assert!(
                f.contents.contains(&import),
                "missing {import:?} in {}",
                f.path
            );
        }
    }
    assert!(reached, "no kanban file creates a form");

    let shop = shop_client();
    let money = chunk(&shop, "money.").expect("shop ships the money chunk");
    assert!(money.contents.contains("export class Money"));
    for f in &shop {
        for gone in ["$k.money", "$k.decimal", "$k.parseDecimal"] {
            assert!(!f.contents.contains(gone), "{} uses {gone}", f.path);
        }
    }
    assert!(
        shop.iter()
            .any(|f| f.contents.contains("$k_money.moneyCodecs.money")),
        "shop decodes its prices with the money chunk's codec"
    );
}

#[test]
fn hydrated_load_values_are_decoded_and_server_loads_are_framed() {
    let host = AnalysisHost::with_files(SemaOptions::new(), shop_files());
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
    let host = AnalysisHost::with_files(SemaOptions::new(), shop_files());
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
        entry.contains(r#""order-confirmation": { run: (c, $c, j) => c.OrderConfirmationJob.run($c, OrderConfirmation$codec.decode(j, "payload")) }"#),
        "{entry}"
    );
    let store = out
        .server
        .iter()
        .find(|f| f.path == "k:/app/store.js")
        .expect("store");
    assert!(
        store.contents.contains(
            r#"OrderConfirmationJob.kind = Object.freeze({ name: "order-confirmation" });"#
        )
    );
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

/// The production build of `examples/<name>`, or of the member `<member>`
/// of the workspace `examples/<name>` when `name` is `<name>/<member>` (the
/// libraries it depends on mounted).
fn example_build(name: &str) -> Output {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let files = match name.split_once('/') {
        Some((workspace, member)) => {
            let ws = k_sema::Workspace::from_files(
                project(&examples.join(workspace))
                    .into_iter()
                    .map(|(p, t)| (p.as_str().to_owned(), t)),
            );
            let m = ws.member(member).expect("the workspace member");
            ws.analysis_files(m)
        }
        None => project(&examples.join(name)),
    };
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    generate(
        &host.analysis(),
        &Options {
            profile: k_codegen::Profile::Release,
            environment: k_codegen::Environment::Production,
            entry: k_codegen::Entry::App,
        },
    )
    .expect("generates")
}

/// docs/design/38-deployment.md, "Static-only apps": an app that renders
/// on the client and has no server-only code (no rpc, endpoint page,
/// configuration, job, entity or guarded page) is static-only, so `k build` emits only
/// `dist/public`; every other app keeps `dist/app`.
#[test]
fn client_rendered_apps_without_server_code_are_static_only() {
    for name in ["todos", "kanban"] {
        assert!(
            example_build(name).manifest.static_only,
            "{name} renders on the client and has no server-only code"
        );
    }
    for name in ["weather", "notes", "shop/shop", "shop/paybox"] {
        assert!(
            !example_build(name).manifest.static_only,
            "{name} renders on the server or has server-only code"
        );
    }
}

/// A page guard is checked by the server before it answers the page's
/// shell (26-auth-and-sessions.md, "In the browser"), so a client-rendered
/// app with a guarded page keeps `dist/app` even when nothing else needs a
/// server: from a static host the guard would never run.
#[test]
fn a_client_rendered_app_with_a_guarded_page_is_not_static_only() {
    let files = [
        ("k.toml", "[package]\nname = \"guarded\"\nk = \"0.1\"\n"),
        (
            "App.k",
            "@Rendering(.client)\n@SignInPage(.home)\npublic app Guarded {}\n",
        ),
        (
            "src/guarded/Home.k",
            "public page Home at \"/\" {\n    view {\n        text { \"home\" }\n    }\n}\n",
        ),
        (
            "src/guarded/Account.k",
            "import k.auth.Authenticated\n\npublic page Account requires Authenticated {\n    view {\n        text { \"account\" }\n    }\n}\n",
        ),
    ];
    let host = AnalysisHost::with_files(
        SemaOptions::new(),
        files.map(|(p, t)| (ProjectPath::new(p).expect("path"), t.to_owned())),
    );
    let analysis = host.analysis();
    let errors: Vec<String> = analysis
        .diagnostics()
        .expect("not cancelled")
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let output = generate(&analysis, &Options::default()).expect("generates");
    assert!(
        !output.manifest.static_only,
        "the server checks the guard before it sends the shell"
    );
}

/// A service's `static let` may share its name with a method (K2019
/// exempts functions), so its JavaScript property is renamed
/// (`label$static`) rather than replacing a static method or being read
/// as one; a `static func` is a static method of the class.
#[test]
fn a_service_static_let_named_like_a_method_keeps_both() {
    let host = analyse(&[
        ("k.toml", TOML),
        ("App.k", "public app Demo {}\n"),
        (
            "src/demo/Labels.k",
            "public service Labels {\n    static let label: String = \"static\"\n\n    func label() -> String {\n        return Labels.label\n    }\n\n    static let make: Int = 2\n\n    static func make() -> Int {\n        return Labels.make\n    }\n\n    func both() -> Int {\n        return Labels.make() + Labels.make\n    }\n}\n",
        ),
    ]);
    let out = generate(&host.analysis(), &Options::default()).expect("generates");
    let module = out
        .server
        .iter()
        .find(|f| f.path == "k:/app/demo.js")
        .expect("the module's server file");
    let js = &module.contents;
    for needle in [
        "static make() {",
        "return Labels.make$static;",
        "return Labels.label$static;",
        "Labels.make() + Labels.make$static",
        "Labels.label$static = \"static\";",
        "Labels.make$static = 2;",
    ] {
        assert!(js.contains(needle), "{needle} in\n{js}");
    }
}
