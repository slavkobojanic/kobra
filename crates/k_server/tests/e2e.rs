//! Server end-to-end tests (`docs/compiler/testing.md`, "Server end-to-end
//! tests"): start a real server on an ephemeral port and run an HTTP
//! request script against it.
//!
//! Two kinds of case, both with a `requests.http` script
//! (`k_server::testing`):
//!
//! * **K projects** in `tests/e2e/<name>/` (a `k.toml`): analysed, compiled
//!   by `k_codegen` and served from the generated bundle. An
//!   `expected.pending` file marks a test-first case that is expected to
//!   fail until the pipeline supports it; it fails once it passes.
//! * **Bundle fixtures** in `crates/k_server/tests/http/<name>/`: a
//!   hand-written `server.js` entry (plus optional `manifest.json` and
//!   `assets/`), which test the HTTP front and the worker loop without the
//!   code generator.
//!
//! * **Database corpus cases** in `tests/corpus/db/<name>/`: K projects
//!   like the e2e ones, whose `.env` sets `DATABASE_URL=sqlite::memory:`;
//!   the server opens and migrates a fresh database for each run.
//!
//! ```text
//! cargo test -p k_server --test e2e
//! cargo test -p k_server --test e2e -- http/basic
//! cargo test -p k_server --test e2e -- db/
//! ```

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bytes::Bytes;
use k_codegen::{Entry, Environment, FileKind, Options, Profile, check_client_bundle, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::http::StaticAsset;
use k_server::testing::run_script;
use k_server::{JsModule, Output, Server, ServerBundle, ServerOptions};
use libtest_mimic::{Arguments, Failed, Trial};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("map" | "json") => "application/json",
        Some("html") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn is_hashed(name: &str) -> bool {
    name.split('.')
        .any(|part| part.len() == 8 && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn options(bundle: ServerBundle, assets: Vec<StaticAsset>, origin: String) -> ServerOptions {
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("address"), bundle);
    o.workers = 2;
    o.assets = assets;
    o.public_origin = origin;
    o.shutdown_grace = Duration::from_secs(5);
    // `K_E2E_LOG=1` prints the workers' log lines (panics with stacks).
    o.log = if std::env::var_os("K_E2E_LOG").is_some() {
        Output::JsonLines
    } else {
        Output::Capture
    };
    o
}

fn serve_and_run(o: ServerOptions, script: &str) -> Result<(), String> {
    let server = Server::start(o).map_err(|e| e.to_string())?;
    let outcome = run_script(server.local_addr(), script);
    let stopped = server
        .shutdown()
        .map_err(|e| format!("shutdown failed: {e}"));
    outcome?;
    stopped
}

fn run_fixture(dir: &Path) -> Result<(), String> {
    let source = fs::read_to_string(dir.join("server.js")).map_err(|e| e.to_string())?;
    let script = fs::read_to_string(dir.join("requests.http")).map_err(|e| e.to_string())?;
    let manifest: serde_json::Value = match fs::read_to_string(dir.join("manifest.json")) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| e.to_string())?,
        Err(_) => serde_json::json!({}),
    };
    let rpc_paths: BTreeSet<String> = manifest["rpcPaths"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let origin = manifest["publicOrigin"]
        .as_str()
        .unwrap_or("http://localhost")
        .to_owned();
    let mut assets = Vec::new();
    if let Ok(entries) = fs::read_dir(dir.join("assets")) {
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let body = fs::read(e.path()).map_err(|e| e.to_string())?;
            assets.push(StaticAsset {
                path: format!("/assets/{name}"),
                content_type: content_type(&name).to_owned(),
                body: Bytes::from(body),
                immutable: is_hashed(&name),
            });
        }
    }
    let bundle = ServerBundle {
        modules: vec![JsModule {
            specifier: "k:/app/server.js".into(),
            source,
        }],
        entry: "k:/app/server.js".into(),
        rpc_paths,
    };
    serve_and_run(options(bundle, assets, origin), &script)
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) -> Result<(), String> {
    for e in fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
    {
        let path = e.path();
        if path.is_dir() {
            collect(&path, root, out)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "requests.http" || relative.starts_with("expected.") {
            continue;
        }
        let project_path = ProjectPath::new(&relative).map_err(|e| format!("{relative}: {e}"))?;
        out.push((
            project_path,
            fs::read_to_string(&path).map_err(|e| e.to_string())?,
        ));
    }
    Ok(())
}

/// Runs `f` with a fresh Postgres schema of `K_TEST_POSTGRES_URL` and the
/// URL that selects it, dropping the schema afterwards.
fn with_postgres_schema(
    url: &str,
    f: impl FnOnce(String) -> Result<(), String>,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let k = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let schema = format!("k_e2e_{}_{n}_{k}", std::process::id());
    let admin = runtime
        .block_on(k_db::Database::connect(
            url,
            &k_db::DatabaseOptions::default(),
        ))
        .map_err(|e| e.to_string())?;
    runtime
        .block_on(admin.execute(&format!("CREATE SCHEMA \"{schema}\""), &[]))
        .map_err(|e| e.to_string())?;
    let separator = if url.contains('?') { '&' } else { '?' };
    let result = f(format!("{url}{separator}options=-csearch_path%3D{schema}"));
    let _ = runtime.block_on(admin.execute(&format!("DROP SCHEMA \"{schema}\" CASCADE"), &[]));
    result
}

fn run_project(dir: &Path) -> Result<(), String> {
    run_project_on(dir, None)
}

/// Runs a project; `database_url` replaces the `.env`'s `DATABASE_URL`.
fn run_project_on(dir: &Path, database_url: Option<String>) -> Result<(), String> {
    let script =
        fs::read_to_string(dir.join("requests.http")).map_err(|e| format!("requests.http: {e}"))?;
    let mut files = Vec::new();
    collect(dir, dir, &mut files)?;
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().map_err(|_| "cancelled".to_owned())?;
    if let Some(d) = diagnostics.iter().find(|d| d.is_error()) {
        return Err(format!(
            "an e2e project must check cleanly, pending or not: {}: {}",
            d.code, d.message
        ));
    }
    let options_ = Options {
        profile: Profile::Development,
        environment: Environment::Development,
        entry: Entry::App,
    };
    let generated = generate(&analysis, &options_).map_err(|e| e.to_string())?;
    let violations = check_client_bundle(&generated, &[]);
    if !violations.is_empty() {
        return Err(format!("server code in the client bundle: {violations:?}"));
    }
    let entry = generated.server_entry.clone().ok_or("no server entry")?;
    let bundle = ServerBundle {
        modules: generated
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect(),
        entry,
        rpc_paths: generated.manifest.rpc_paths.iter().cloned().collect(),
    };
    let assets = generated
        .client
        .iter()
        .map(|f| StaticAsset {
            path: format!("/assets/{}", f.path),
            content_type: content_type(&f.path).to_owned(),
            body: Bytes::from(f.contents.clone()),
            immutable: true,
        })
        .collect();
    let mut o = options(bundle, assets, "http://localhost".to_owned());
    // The development environment `k run` gives the app: `.env` at the
    // project root (and nothing from the test runner's environment), and
    // the endpoint pages that never see cookies.
    o.config = dev_config(dir, &generated.manifest)?;
    if let Some(url) = database_url {
        o.config.insert(
            "DATABASE_URL".to_owned(),
            k_server::config::ConfigValue::String(url),
        );
    }
    o.forgery_exempt = generated
        .manifest
        .endpoints
        .iter()
        .filter(|e| !e.uses_session)
        .map(|e| e.template.clone())
        .collect();
    // `DATABASE_URL` from `.env` (`sqlite::memory:` in the db corpus): a
    // fresh database per case, migrated before the first request.
    o.database = k_server::db::DatabaseSetup::from_config(generated.database.clone(), &o.config);
    serve_and_run(o, &script)
}

/// Resolves the manifest's configuration from the project's `.env`.
fn dev_config(
    dir: &Path,
    manifest: &k_codegen::Manifest,
) -> Result<std::collections::BTreeMap<String, k_server::config::ConfigValue>, String> {
    let fields: Vec<k_server::config::ConfigField> = manifest
        .config
        .iter()
        .filter_map(|c| {
            k_server::config::ConfigField::from_manifest(
                &c.env,
                &c.declared_at,
                &c.ty,
                c.secret,
                c.required,
                c.default.as_deref(),
                &c.cases,
                c.range,
            )
        })
        .collect();
    let env = k_server::config::DevEnvironment::load(dir, false);
    k_server::config::resolve(&fields, &env).map_err(|p| k_server::config::report(&p))
}

fn pending_wrapper(dir: &Path, result: Result<(), String>) -> Result<(), String> {
    match fs::read_to_string(dir.join("expected.pending")) {
        Err(_) => result,
        Ok(reason) if reason.trim().is_empty() => {
            Err("expected.pending is empty; say what is missing".to_owned())
        }
        Ok(reason) => match result {
            Err(_) => Ok(()),
            Ok(()) => Err(format!(
                "this pending case passes now ({}); delete expected.pending",
                reason.trim()
            )),
        },
    }
}

fn dirs(path: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(path)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn main() {
    let args = Arguments::from_args();
    let mut trials = Vec::new();
    for dir in dirs(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/http")) {
        let name = format!(
            "http/{}",
            dir.file_name().unwrap_or_default().to_string_lossy()
        );
        trials.push(Trial::test(name, move || {
            run_fixture(&dir).map_err(Failed::from)
        }));
    }
    for dir in dirs(&repo().join("tests/e2e")) {
        let name = format!(
            "e2e/{}",
            dir.file_name().unwrap_or_default().to_string_lossy()
        );
        if !dir.join("k.toml").is_file() {
            trials.push(Trial::test(name, || {
                Err(Failed::from("an e2e project needs a k.toml"))
            }));
            continue;
        }
        trials.push(Trial::test(name, move || {
            pending_wrapper(&dir, run_project(&dir)).map_err(Failed::from)
        }));
    }
    // The database corpus (`tests/corpus/db`, testing.md §3.11): apps with
    // entities, repositories and migrations, served on a fresh SQLite
    // database and driven by their `requests.http`.
    for case in k_corpus::discover::discover(&k_corpus::corpus_root())
        .cases
        .into_iter()
        .filter(|c| c.suite == k_corpus::suites::Suite::Db)
    {
        let dir = case.source.clone();
        trials.push(Trial::test(case.name.clone(), move || {
            pending_wrapper(&dir, run_project(&dir)).map_err(Failed::from)
        }));
        // And on Postgres when the run asks for it (testing.md §4.2).
        if let Ok(url) = std::env::var(k_db::testing::POSTGRES_URL_ENV) {
            let dir = case.source.clone();
            trials.push(Trial::test(format!("{}postgres", case.name), move || {
                let outcome = with_postgres_schema(&url, |u| run_project_on(&dir, Some(u)));
                pending_wrapper(&dir, outcome).map_err(Failed::from)
            }));
        }
    }
    libtest_mimic::run(&args, trials).exit();
}
