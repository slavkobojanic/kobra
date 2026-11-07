//! `k build` and `k run`: analyse a project, generate its bundles and
//! either write `dist/` or serve them (`docs/compiler/codegen.md`, "Output
//! layout"; `docs/compiler/server.md`).
//!
//! Both refuse a program with errors (printing them like `k check`, exit
//! 1), and a library (a project without an `app`, which has nothing to
//! serve) with exit 2.

use std::path::Path;
use std::process::ExitCode;

use k_codegen::{CodegenError, Entry, Environment, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions};

use crate::cli::{BuildArgs, RunArgs};
use crate::sources;

/// Analyses the project at `project` and generates its bundles.
fn compile(project: &Path, options: &Options) -> Result<Output, ExitCode> {
    if !project.join("k.toml").is_file() {
        eprintln!(
            "error: `{}` is not a project: it has no k.toml",
            sources::display(project)
        );
        return Err(ExitCode::from(2));
    }
    let loaded = sources::load(&[project.to_path_buf()]);
    let Some(group) = loaded.groups.into_iter().next() else {
        eprintln!("error: no sources found in `{}`", sources::display(project));
        return Err(ExitCode::from(2));
    };
    if !group.problems.is_empty() {
        eprintln!("error: some files can't be read; run `k check` for details");
        return Err(ExitCode::from(1));
    }
    let host = AnalysisHost::with_files(SemaOptions::new(), group.files);
    let analysis = host.analysis();
    match generate(&analysis, options) {
        Ok(output) if output.server_entry.is_none() && !output.manifest.static_only => {
            eprintln!(
                "error: `{}` is a library: it has no `app` to build or run",
                sources::display(project)
            );
            Err(ExitCode::from(2))
        }
        Ok(output) => Ok(output),
        Err(CodegenError::HasErrors { count }) => {
            eprintln!(
                "error: the project has {count} error(s); run `k check {}` to see them",
                sources::display(project)
            );
            Err(ExitCode::from(1))
        }
        Err(e) => {
            eprintln!("error: {e}");
            Err(ExitCode::from(2))
        }
    }
}

/// Runs `k build`.
pub fn build(args: &BuildArgs) -> ExitCode {
    let options = Options {
        profile: Profile::Release,
        environment: Environment::Production,
        entry: Entry::App,
    };
    let output = match compile(&args.project, &options) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| args.project.join("dist"));
    if let Err(e) = write(&output, &out) {
        eprintln!("error: cannot write `{}`: {e}", sources::display(&out));
        return ExitCode::from(2);
    }
    eprintln!("built {}", sources::display(&out));
    ExitCode::SUCCESS
}

#[cfg(feature = "server")]
fn write(output: &Output, out: &Path) -> std::io::Result<()> {
    use k_server::app::{AppPayload, ConfigSpec, EmbeddedFile, append};
    k_codegen::dist::write_public(output, out)?;
    if output.manifest.static_only {
        return Ok(());
    }
    // v0: `dist/app` is a copy of this `k` executable with the app appended;
    // it detects the trailer at startup and serves (server.md, "Packaging").
    let exe = std::fs::read(std::env::current_exe()?)?;
    let payload = AppPayload {
        name: output.manifest.name.clone(),
        modules: output
            .server
            .iter()
            .map(|f| EmbeddedFile {
                path: f.path.clone(),
                text: f.contents.clone(),
            })
            .collect(),
        entry: output.server_entry.clone().unwrap_or_default(),
        rpc_paths: output.manifest.rpc_paths.clone(),
        assets: output
            .client
            .iter()
            .map(|f| EmbeddedFile {
                path: format!("/assets/{}", f.path),
                text: f.contents.clone(),
            })
            .collect(),
        config: output
            .manifest
            .config
            .iter()
            .map(|c| ConfigSpec {
                env: c.env.clone(),
                declared_at: c.declared_at.clone(),
                ty: c.ty.clone(),
                secret: c.secret,
                required: c.required,
                default: c.default.clone(),
                cases: c.cases.clone(),
                range: c.range,
            })
            .collect(),
        forgery_exempt: forgery_exempt(output),
        database: output.database.clone(),
    };
    let bytes = append(&exe, &payload).map_err(std::io::Error::other)?;
    let app = out.join("app");
    std::fs::write(&app, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(not(feature = "server"))]
fn write(output: &Output, out: &Path) -> std::io::Result<()> {
    // Without the server runtime only the client assets can be written.
    k_codegen::dist::write_public(output, out)
}

/// Runs `k debug codegen`: prints the development build's files, server
/// modules first, each after a `=== path` line.
pub fn dump(project: &Path) -> ExitCode {
    let options = Options {
        profile: Profile::Development,
        environment: Environment::Development,
        entry: Entry::App,
    };
    let output = match compile(project, &options) {
        Ok(o) => o,
        Err(code) => return code,
    };
    for f in &output.server {
        println!("=== server {}\n{}", f.path, f.contents);
    }
    for f in &output.client {
        println!("=== client {}\n{}", f.path, f.contents);
    }
    println!(
        "=== entry {}",
        output.server_entry.as_deref().unwrap_or("(none)")
    );
    ExitCode::SUCCESS
}

/// The endpoint templates that never see cookies: the endpoint pages that
/// don't inject `Session`.
#[cfg(feature = "server")]
fn forgery_exempt(output: &Output) -> Vec<String> {
    output
        .manifest
        .endpoints
        .iter()
        .filter(|e| !e.uses_session)
        .map(|e| e.template.clone())
        .collect()
}

/// Runs `k run`.
pub fn run(args: &RunArgs) -> ExitCode {
    let options = Options {
        profile: Profile::Development,
        environment: Environment::Development,
        entry: Entry::App,
    };
    let output = match compile(&args.project, &options) {
        Ok(o) => o,
        Err(code) => return code,
    };
    serve(&output, &args.project, args.port)
}

/// Serves a development build until `SIGTERM` or Ctrl-C (see [`start`]).
#[cfg(feature = "server")]
fn serve(output: &Output, project: &Path, port: u16) -> ExitCode {
    let server = match start(output, project, port) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match server.run_until_signal() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

/// Starts serving a development build on `port` (0 picks a free one) and
/// prints `serving on http://localhost:<port>`. The configuration comes
/// from the process environment and the project's `.env` (the process
/// wins; 25-config-and-secrets.md, "Per environment"), and is validated
/// first: every problem is printed and the error is exit status 78.
#[cfg(feature = "server")]
fn start(output: &Output, project: &Path, port: u16) -> Result<k_server::Server, ExitCode> {
    use k_server::config::{ConfigField, DevEnvironment, EXIT_CONFIG, report, resolve};
    use k_server::http::StaticAsset;
    use k_server::{JsModule, Server, ServerBundle, ServerOptions};
    let fields: Vec<ConfigField> = output
        .manifest
        .config
        .iter()
        .filter_map(|c| {
            ConfigField::from_manifest(
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
    let values = match resolve(&fields, &DevEnvironment::load(project, true)) {
        Ok(v) => v,
        Err(problems) => {
            eprint!("{}", report(&problems));
            return Err(ExitCode::from(u8::try_from(EXIT_CONFIG).unwrap_or(78)));
        }
    };
    let bundle = ServerBundle {
        modules: output
            .server
            .iter()
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect(),
        entry: output.server_entry.clone().unwrap_or_default(),
        rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
    };
    let mut options =
        ServerOptions::new(std::net::SocketAddr::from(([127, 0, 0, 1], port)), bundle);
    options.public_origin = format!("http://localhost:{port}");
    options.config = values;
    options.forgery_exempt = forgery_exempt(output);
    options.database =
        k_server::db::DatabaseSetup::from_config(output.database.clone(), &options.config);
    options.assets = output
        .client
        .iter()
        .map(|f| StaticAsset {
            path: format!("/assets/{}", f.path),
            content_type: content_type(&f.path).to_owned(),
            body: f.contents.clone().into_bytes().into(),
            immutable: true,
        })
        .collect();
    match Server::start(options) {
        Ok(server) => {
            eprintln!("serving on http://localhost:{}", server.local_addr().port());
            Ok(server)
        }
        Err(e) => {
            eprintln!("error: {e}");
            Err(ExitCode::from(2))
        }
    }
}

/// How often `k dev` looks for changed sources.
#[cfg(feature = "server")]
const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// Runs `k dev`: serves a development build like `k run`, and rebuilds
/// and restarts the server on the same port whenever a file of the project
/// changes (`docs/compiler/server.md`, "Packaging"). There is no hot module
/// replacement in v0: a restart drops in-memory state such as sessions.
///
/// The first build must succeed (it fails like `k run`). After that, a
/// build with errors is reported and the last good build keeps serving; a
/// restart that fails (a configuration problem in `.env`) is reported and
/// retried on the next change. `SIGTERM` or Ctrl-C stops the server
/// gracefully and exits 0.
pub fn dev(args: &RunArgs) -> ExitCode {
    let options = Options {
        profile: Profile::Development,
        environment: Environment::Development,
        entry: Entry::App,
    };
    let output = match compile(&args.project, &options) {
        Ok(o) => o,
        Err(code) => return code,
    };
    watch(&output, &args.project, args.port, &options)
}

#[cfg(feature = "server")]
fn watch(output: &Output, project: &Path, port: u16, options: &Options) -> ExitCode {
    let mut server = match start(output, project, port) {
        Ok(s) => Some(s),
        Err(code) => return code,
    };
    // Port 0 picked a free port: keep it across restarts.
    let port = server.as_ref().map_or(port, |s| s.local_addr().port());
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let mut seen = fingerprint(project);
    let stopped = runtime.block_on(async {
        #[cfg(unix)]
        let mut term =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {e}");
                    return Err(ExitCode::from(2));
                }
            };
        loop {
            #[cfg(unix)]
            let signal = async {
                tokio::select! {
                    _ = term.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            };
            #[cfg(not(unix))]
            let signal = async {
                let _ = tokio::signal::ctrl_c().await;
            };
            tokio::select! {
                () = signal => return Ok(()),
                () = tokio::time::sleep(WATCH_INTERVAL) => {}
            }
            let now = fingerprint(project);
            if now == seen {
                continue;
            }
            // Let a burst of writes (an editor saving several files) settle.
            tokio::time::sleep(WATCH_INTERVAL).await;
            seen = fingerprint(project);
            eprintln!("change detected; rebuilding");
            let Ok(next) = compile(project, options) else {
                eprintln!("the last good build keeps serving");
                continue;
            };
            if let Some(old) = server.take()
                && let Err(e) = old.shutdown()
            {
                eprintln!("error: {e}");
            }
            server = start(&next, project, port).ok();
            if server.is_none() {
                eprintln!("the server is stopped until the next change");
            }
        }
    });
    if let Err(code) = stopped {
        return code;
    }
    match server.map(k_server::Server::shutdown) {
        Some(Err(e)) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
        _ => ExitCode::SUCCESS,
    }
}

#[cfg(not(feature = "server"))]
fn watch(_output: &Output, _project: &Path, _port: u16, _options: &Options) -> ExitCode {
    eprintln!("error: this `k` was built without the server runtime (feature `server`)");
    ExitCode::from(2)
}

/// A snapshot of the project's files (path, size and modification time),
/// skipping hidden directories and build output like the source loader,
/// but keeping `.env`. Two equal snapshots mean nothing changed.
#[cfg(feature = "server")]
fn fingerprint(project: &Path) -> Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)> {
    fn walk(
        dir: &Path,
        top: bool,
        out: &mut Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                if !name.starts_with('.') && !sources::is_skipped_dir(&name) {
                    walk(&path, false, out);
                }
            } else if !name.starts_with('.') || (top && name == ".env") {
                out.push((path, meta.len(), meta.modified().ok()));
            }
        }
    }
    let mut out = Vec::new();
    walk(project, true, &mut out);
    out.sort();
    out
}

#[cfg(feature = "server")]
fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(not(feature = "server"))]
fn serve(_output: &Output, _project: &Path, _port: u16) -> ExitCode {
    eprintln!("error: this `k` was built without the server runtime (feature `server`)");
    ExitCode::from(2)
}
