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
