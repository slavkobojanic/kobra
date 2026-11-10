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

use crate::cli::{BuildArgs, RunArgs};
use crate::{sources, workspace};

/// Analyses the project at `project` (a member of a workspace with the
/// libraries it depends on) and generates its bundles.
///
/// # Errors
///
/// The exit status, after the problem was printed: as
/// [`workspace::load_member`] and [`compile_member`].
fn compile(project: &Path, options: &Options, command: &str) -> Result<Output, ExitCode> {
    let (loaded, member) = workspace::load_member(project, command)?;
    compile_member(&loaded, member, project, options, false).map(|(o, _)| o)
}

/// The lockfiles a build writes, by file name, with the text each
/// program implies; a lock that doesn't change is left out.
type Locks = Vec<(&'static str, String)>;

/// The locks `k build` writes for `analysis`: `rpc.lock` and
/// `storage.lock` (`k_sema::Analysis::rpc_lock`,
/// `k_sema::Analysis::storage_lock`), `schema.lock` and `routes.lock`
/// (`k_codegen::schema_lock`, `k_codegen::routes_lock`).
fn locks(analysis: &k_sema::Analysis) -> Locks {
    let mut out = Vec::new();
    if let Ok(Some(rpc)) = analysis.rpc_lock() {
        out.push(("rpc.lock", rpc));
    }
    if let Ok(Some(storage)) = analysis.storage_lock() {
        out.push(("storage.lock", storage));
    }
    if let Ok(Some(schema)) = k_codegen::schema_lock(analysis) {
        out.push(("schema.lock", schema));
    }
    if let Ok(Some(routes)) = k_codegen::routes_lock(analysis) {
        out.push(("routes.lock", routes));
    }
    out
}

/// Generates member `m`'s bundles, shown as `project`; also returns the
/// lockfiles its program implies ([`locks`]). With `serve` (`k dev` and
/// `k run`), the `schema.lock` errors `k check` reports
/// ([`k_codegen::schema_diagnostics`], such as an entity with no
/// migration) count as the project's errors too, so a build whose tables
/// don't exist is never served.
///
/// # Errors
///
/// The exit status, after the problem was printed: 1 when the project has
/// errors, 2 when it is a library (no `app`) or code generation fails.
fn compile_member(
    loaded: &workspace::Loaded,
    m: usize,
    project: &Path,
    options: &Options,
    serve: bool,
) -> Result<(Output, Locks), ExitCode> {
    let host = loaded.host(m);
    let analysis = host.analysis();
    // Checked before generating, whose own error for a column the
    // migrations don't create would otherwise come first.
    let unmigrated = if serve {
        k_codegen::schema_diagnostics(&analysis)
            .iter()
            .filter(|d| d.is_error())
            .count()
    } else {
        0
    };
    let generated = if unmigrated > 0 {
        Err(CodegenError::HasErrors { count: unmigrated })
    } else {
        generate(&analysis, options)
    };
    match generated {
        Ok(output) if output.server_entry.is_none() && !output.manifest.static_only => {
            eprintln!(
                "error: `{}` is a library: it has no `app` to build or run",
                sources::display(project)
            );
            Err(ExitCode::from(2))
        }
        Ok(output) => Ok((output, locks(&analysis))),
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

/// Writes each of `locks` to `dir` when it differs from what is there
/// (`docs/design/13-tooling.md`, "Lockfiles"): `k build` records the shape
/// of every protocol the app serves (`rpc.lock`), its stored formats
/// (`storage.lock`: storage keys, collections and jobs), the schema its migrations
/// produce (`schema.lock`) and its URLs and redirects (`routes.lock`), after
/// `k check` accepted the change. `k check` only reads them.
///
/// # Errors
///
/// Exit status 2, after printing the problem, when a file can't be
/// written.
fn write_locks(dir: &Path, locks: &[(&str, String)]) -> Result<(), ExitCode> {
    for (name, lock) in locks {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).is_ok_and(|old| old == *lock) {
            continue;
        }
        if let Err(e) = std::fs::write(&path, lock) {
            eprintln!("error: cannot write `{}`: {e}", sources::display(&path));
            return Err(ExitCode::from(2));
        }
        eprintln!("wrote {}", sources::display(&path));
    }
    Ok(())
}

/// Runs `k build`: one project into its `dist/` (or `--out`), or, at a
/// workspace root, every app member into `dist/<name>/` below the root (or
/// `--out`). Each app's `rpc.lock`, `schema.lock` and `routes.lock` are
/// updated.
pub fn build(args: &BuildArgs) -> ExitCode {
    let options = Options {
        profile: Profile::Release,
        environment: Environment::Production,
        entry: Entry::App,
    };
    let loaded = match workspace::load(&args.project) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let Some(member) = loaded.focus else {
        let out = args.out.clone().unwrap_or_else(|| loaded.root.join("dist"));
        let mut built = 0;
        for m in loaded.targets() {
            if !loaded.workspace.members[m].is_app {
                continue;
            }
            let dir = loaded.member_dir(m);
            let (output, lock) = match compile_member(&loaded, m, &dir, &options, false) {
                Ok(o) => o,
                Err(code) => return code,
            };
            if let Err(code) = write_locks(&dir, &lock) {
                return code;
            }
            let target = out.join(&loaded.workspace.members[m].name);
            if let Err(e) = write(&output, &target) {
                eprintln!("error: cannot write `{}`: {e}", sources::display(&target));
                return ExitCode::from(2);
            }
            eprintln!("built {}", sources::display(&target));
            built += 1;
        }
        if built == 0 {
            eprintln!(
                "error: the workspace `{}` has no app to build",
                sources::display(&args.project)
            );
            return ExitCode::from(2);
        }
        return ExitCode::SUCCESS;
    };
    let (output, lock) = match compile_member(&loaded, member, &args.project, &options, false) {
        Ok(o) => o,
        Err(code) => return code,
    };
    if let Err(code) = write_locks(&args.project, &lock) {
        return code;
    }
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

/// Writes a build into `out`: the public files, then either the static-only
/// document ([`write_static`]) or `out/app`, a copy of this executable with
/// the app's [`AppPayload`](k_server::app::AppPayload) appended (made
/// executable on Unix).
///
/// # Errors
///
/// Any failure to read this executable or to write the files.
#[cfg(feature = "server")]
fn write(output: &Output, out: &Path) -> std::io::Result<()> {
    use k_server::app::{AppPayload, ConfigSpec, EmbeddedFile, append};
    k_codegen::dist::write_public(output, out)?;
    if output.manifest.static_only {
        return write_static(output, out);
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
            // The service worker at the site's root, unhashed, so served
            // with `no-cache` (codegen.md §19).
            .chain(output.root.iter().map(|f| EmbeddedFile {
                path: format!("/{}", f.path),
                text: f.contents.clone(),
            }))
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
                pattern: c.pattern.clone(),
                pattern_message: c.pattern_message.clone(),
            })
            .collect(),
        forgery_exempt: forgery_exempt(output),
        database: output.database.clone(),
        jobs: output.manifest.jobs.clone(),
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

/// Finishes a static-only build (`docs/design/38-deployment.md`,
/// "Static-only apps"): writes `dist/public/index.html`, the document
/// every URL of the app answers, and removes a `dist/app` an earlier
/// build left, which would serve an old version.
///
/// The document is the one the app's server answers for a page, rendered
/// once by the server runtime on a loopback port, so it can't differ from
/// what `k run` serves: the app's metadata, its stylesheet and its entry
/// script around an empty `#k-app`.
///
/// # Errors
///
/// A file can't be written or removed, the server runtime can't start, or
/// rendering the page fails.
#[cfg(feature = "server")]
fn write_static(output: &Output, out: &Path) -> std::io::Result<()> {
    use k_server::{JsModule, Server, ServerBundle, ServerOptions};
    let app = out.join("app");
    if app.is_file() {
        std::fs::remove_file(&app)?;
    }
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
    let mut options = ServerOptions::new(std::net::SocketAddr::from(([127, 0, 0, 1], 0)), bundle);
    options.workers = 1;
    let server = Server::start(options).map_err(std::io::Error::other)?;
    let page = k_server::testing::send(
        server.local_addr(),
        "GET",
        "/",
        &[("accept".to_owned(), "text/html".to_owned())],
        b"",
    );
    // The app's web app manifest, when it declares one, next to the
    // document that links it (22-metadata.md).
    let manifest = k_server::testing::send(
        server.local_addr(),
        "GET",
        "/manifest.webmanifest",
        &[],
        b"",
    );
    let _ = server.shutdown();
    let page = page.map_err(std::io::Error::other)?;
    if page.status != 200 {
        return Err(std::io::Error::other(format!(
            "the app's document answered {} instead of 200",
            page.status
        )));
    }
    let manifest_file = out.join("public").join("manifest.webmanifest");
    match manifest {
        Ok(m) if m.status == 200 => std::fs::write(&manifest_file, &m.body)?,
        _ if manifest_file.is_file() => std::fs::remove_file(&manifest_file)?,
        _ => {}
    }
    std::fs::write(out.join("public").join("index.html"), &page.body)
}

/// Without the server runtime: writes only the public files; there is no
/// `dist/app` to produce.
///
/// # Errors
///
/// Any failure to write the files.
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
    let output = match compile(project, &options, "debug codegen") {
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

/// Runs `k run`: builds the project for development and serves it until
/// `SIGTERM` or Ctrl-C, without watching for changes. At a workspace root
/// it serves every app member, wired together like `k dev` ([`serve_dev`]).
pub fn run(args: &RunArgs) -> ExitCode {
    serve_dev(args, false)
}

/// What `k dev` and `k run` at a workspace root add to one app's
/// development configuration: the variables wiring its remotes to their
/// sibling members, which win over the member's `.env`, and the app's name,
/// printed before each of its lines (`shop   | serving on …`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Wiring {
    /// Variables and values.
    vars: std::collections::BTreeMap<String, String>,
    /// The app's name, padded to the longest of the run.
    label: Option<&'static str>,
    /// `--inspect`: the port of the server's V8 inspector.
    inspect: Option<u16>,
}

/// Starts serving a development build on `port` (0 picks a free one) and
/// prints `serving on http://localhost:<port>`. The configuration comes
/// from the process environment and the project's `.env` (the process
/// wins; 25-config-and-secrets.md, "Per environment"), with `wiring`'s
/// variables over the `.env`, and is validated first: every problem is
/// printed and the error is exit status 78. The `.env` keys the process
/// environment shadows are named first, without their values.
///
/// # Errors
///
/// The exit status, after the problem was printed: 78 for invalid
/// configuration, 1 when the database settings are unusable, 2 when the
/// server can't start.
#[cfg(feature = "server")]
fn start(
    output: &Output,
    project: &Path,
    port: u16,
    wiring: &Wiring,
    mail: &mut Option<crate::devenv::MailSink>,
) -> Result<k_server::Server, ExitCode> {
    use k_server::config::{ConfigField, EXIT_CONFIG, report, resolve};
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
            .map(|f| f.with_pattern(c.pattern.as_deref(), c.pattern_message.as_deref()))
        })
        .collect();
    let label = wiring.label;
    let config_error = || ExitCode::from(u8::try_from(EXIT_CONFIG).unwrap_or(78));
    let inputs = crate::devenv::DevInputs {
        project,
        package: &output.manifest.name,
        port,
        wiring: &wiring.vars,
    };
    let mut start_mail = || {
        if mail.is_none() {
            match crate::devenv::MailSink::start(project) {
                Ok(sink) => *mail = Some(sink),
                Err(e) => crate::devenv::say(label, &format!("warning: no mail catcher: {e}")),
            }
        }
        mail.as_ref().map(crate::devenv::MailSink::port)
    };
    let prepared = match crate::devenv::prepare(&inputs, &fields, &mut start_mail) {
        Ok(p) => p,
        Err(problem) => {
            crate::devenv::say(label, &problem);
            return Err(config_error());
        }
    };
    for note in &prepared.notes {
        crate::devenv::say(label, note);
    }
    let values = match resolve(&fields, &prepared.env) {
        Ok(v) => v,
        Err(problems) => {
            crate::devenv::say(label, &report(&problems));
            return Err(config_error());
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
    // A restart in `k dev` shouldn't wait out production's 25 s: open
    // streams end at once anyway, and the browser re-subscribes.
    options.shutdown_grace = DEV_SHUTDOWN_GRACE;
    if let Some(l) = label {
        options.log = k_server::Output::Prefixed(l);
    }
    options.fetch_origins = dev_fetch_origins();
    // A panic's 500 page shows its stack, mapped to `.k` lines, and with
    // `--inspect` a debugger can attach (13-tooling.md, "Debugging").
    options.dev_errors = true;
    // Channels' open and close lines and the inspector at
    // `/_k/dev/channels` (44-channels.md); `dist/app` never has them.
    options.dev_tools = true;
    options.inspect = wiring
        .inspect
        .map(|p| std::net::SocketAddr::from(([127, 0, 0, 1], p)));
    // A relative SQLite path is the project's (each member's, at a
    // workspace root), and its directory is created.
    options.database =
        match k_server::db::DatabaseSetup::from_config(output.database.clone(), &options.config)
            .map(|d| d.rooted_at(project))
            .transpose()
        {
            Ok(d) => d,
            Err(e) => {
                crate::devenv::say(label, &format!("error: database: {e}"));
                return Err(ExitCode::from(1));
            }
        };
    // `[dev] seed` fills a database this start creates.
    if let Some(setup) = &options.database {
        match crate::devenv::seed(project, setup) {
            Ok(lines) => {
                for line in lines {
                    crate::devenv::say(label, &line);
                }
            }
            Err(e) => {
                crate::devenv::say(label, &format!("error: {e}"));
                return Err(ExitCode::from(1));
            }
        }
    }
    // `k run` and `k dev` run the jobs worker too, against the development
    // database (30-background-jobs.md).
    options.jobs = output.manifest.jobs.clone();
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
    // The service worker at the site's root (`/k-sw.js`), revalidated on
    // every fetch so a rebuild reaches the browser's next update check
    // (codegen.md §19).
    options
        .assets
        .extend(output.root.iter().map(|f| StaticAsset {
            path: format!("/{}", f.path),
            content_type: content_type(&f.path).to_owned(),
            body: f.contents.clone().into_bytes().into(),
            immutable: false,
        }));
    // The `.k` files the source maps name (`/_k/src/<path>`), for the
    // debuggers that fetch sources rather than read the embedded text
    // (codegen.md §18).
    options.assets.extend(k_sources(project));
    let inspect = options.inspect;
    match Server::start(options) {
        Ok(server) => {
            crate::devenv::say(
                label,
                &format!("serving on http://localhost:{}", server.local_addr().port()),
            );
            if let Some(addr) = inspect {
                crate::devenv::say(
                    label,
                    &format!(
                        "inspector on ws://{addr}/ws/k (chrome://inspect, or attach VS Code to port {})",
                        addr.port()
                    ),
                );
            }
            Ok(server)
        }
        Err(k_server::http::ServerError::Bind(e)) => {
            crate::devenv::say(
                label,
                &format!(
                    "error: cannot listen on port {port}: {e}; another program is using it, \
                     so pick other ports with `--port`"
                ),
            );
            Err(ExitCode::from(2))
        }
        Err(e) => {
            crate::devenv::say(label, &format!("error: {e}"));
            Err(ExitCode::from(2))
        }
    }
}

/// Every `.k` file below `project` as a development asset at
/// `/_k/src/<project-relative path>`, the URLs client source maps name.
#[cfg(feature = "server")]
fn k_sources(project: &Path) -> Vec<k_server::http::StaticAsset> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<k_server::http::StaticAsset>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !name.starts_with('.') && name != "dist" && name != "target" {
                    walk(&path, root, out);
                }
            } else if name.ends_with(".k")
                && let (Ok(rel), Ok(text)) = (path.strip_prefix(root), std::fs::read(&path))
            {
                let rel = rel.to_string_lossy().replace('\\', "/");
                out.push(k_server::http::StaticAsset {
                    path: format!("/_k/src/{rel}"),
                    content_type: "text/plain; charset=utf-8".to_owned(),
                    body: text.into(),
                    immutable: false,
                });
            }
        }
    }
    let mut out = Vec::new();
    walk(project, project, &mut out);
    out
}

/// How long a development server waits for in-flight requests when it
/// stops or restarts (production's default is 25 s).
#[cfg(feature = "server")]
const DEV_SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// `K_DEV_FETCH_ORIGINS` from the process environment: comma-separated
/// `from=to` origin pairs. Under `k run` and `k dev` only, a server-side
/// `HttpClient` request to `from` (or below it) goes to `to` instead
/// ([`k_server::ServerOptions::fetch_origins`]), so an end-to-end test can
/// stand a local fake in for a third-party API whose URL the app
/// hard-codes (`docs/compiler/testing.md`, "Real-browser tests"). Builds
/// and `dist/app` never read it. A pair without `=` is ignored.
#[cfg(feature = "server")]
fn dev_fetch_origins() -> Vec<(String, String)> {
    std::env::var("K_DEV_FETCH_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(from, to)| {
            (
                from.trim().trim_end_matches('/').to_owned(),
                to.trim().trim_end_matches('/').to_owned(),
            )
        })
        .filter(|(from, to)| !from.is_empty() && !to.is_empty())
        .collect()
}

/// How often `k dev` looks for changed sources.
#[cfg(feature = "server")]
pub(crate) const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// Runs `k dev`: serves a development build like `k run`, and rebuilds
/// and restarts the server on the same port whenever a file of the project
/// changes (`docs/compiler/server.md`, "Packaging"). There is no hot module
/// replacement in v0: a restart drops in-memory state, but sessions and
/// data are in the development database and survive it.
///
/// The first build must succeed (it fails like `k run`). After that, a
/// build with errors is reported and the last good build keeps serving; a
/// restart that fails (a configuration problem in `.env`) is reported and
/// retried on the next change. `SIGTERM` or Ctrl-C stops the server
/// gracefully and exits 0.
pub fn dev(args: &RunArgs) -> ExitCode {
    serve_dev(args, true)
}

/// One app of `k dev` or `k run`.
#[cfg(feature = "server")]
struct DevApp {
    /// The member's index in the workspace.
    member: usize,
    /// The member's directory.
    dir: std::path::PathBuf,
    /// The port it is served on, fixed for the whole run.
    port: u16,
    /// Its name before each of its lines, at a workspace root.
    label: Option<&'static str>,
    /// Its last good build.
    output: Output,
    /// Its running server; `None` while stopped (before the first start, or
    /// after a restart failed).
    server: Option<k_server::Server>,
    /// Its development mail catcher, once it sends email.
    mail: Option<crate::devenv::MailSink>,
}

/// A free local port, for `--port 0`.
///
/// # Errors
///
/// Exit status 2 after printing the problem.
#[cfg(feature = "server")]
fn free_port() -> Result<u16, ExitCode> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| {
            eprintln!("error: no free port: {e}");
            ExitCode::from(2)
        })
}

/// `k run` (`watch` false) and `k dev` (`watch` true) for a project, or
/// for every app member of a workspace root
/// (`docs/design/40-workspaces-and-services.md`, "Tooling").
///
/// At a root, each app is served on its own port (`port`, `port + 1`, …
/// in dependency order; free ports for 0), its lines are prefixed with its
/// name, and each `remote` rule whose protocol a sibling member serves is
/// wired to it: the config's `url` variable is that member's address and
/// its `token` one development secret, generated per run, which every
/// member also gets as `K_SERVICE_TOKEN`. With `watch`, a change anywhere
/// (each member's `.env` included) rebuilds and restarts every app; a build
/// with errors keeps the last good one serving. Ctrl-C or `SIGTERM` stops
/// them all.
#[cfg(feature = "server")]
fn serve_dev(args: &RunArgs, watch: bool) -> ExitCode {
    let options = Options {
        // `--release`: the client `k build` ships, with development wiring
        // (13-tooling.md, "Release builds").
        profile: if args.release {
            Profile::Release
        } else {
            Profile::Development
        },
        environment: Environment::Development,
        entry: Entry::App,
    };
    let loaded = match workspace::load(&args.project) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let at_root = loaded.focus.is_none();
    let apps: Vec<usize> = match loaded.focus {
        Some(m) => vec![m],
        None => loaded
            .targets()
            .into_iter()
            .filter(|&m| loaded.workspace.members[m].is_app)
            .collect(),
    };
    if apps.is_empty() {
        eprintln!(
            "error: the workspace `{}` has no app to run",
            sources::display(&args.project)
        );
        return ExitCode::from(2);
    }
    let width = apps
        .iter()
        .map(|&m| loaded.workspace.members[m].name.len())
        .max()
        .unwrap_or(0);
    let mut running: Vec<DevApp> = Vec::new();
    for (i, &m) in apps.iter().enumerate() {
        let dir = if at_root {
            loaded.member_dir(m)
        } else {
            args.project.clone()
        };
        let output = match compile_member(&loaded, m, &dir, &options, true) {
            Ok((o, _)) => o,
            Err(code) => return code,
        };
        let port = if args.port == 0 {
            match free_port() {
                Ok(p) => p,
                Err(code) => return code,
            }
        } else {
            match args.port.checked_add(u16::try_from(i).unwrap_or(u16::MAX)) {
                Some(p) => p,
                None => {
                    eprintln!(
                        "error: port {} leaves no room for {} apps",
                        args.port,
                        apps.len()
                    );
                    return ExitCode::from(2);
                }
            }
        };
        // One small leak per app and run: the server's log output keeps it.
        let label: Option<&'static str> = at_root.then(|| {
            &*Box::leak(format!("{:width$}", loaded.workspace.members[m].name).into_boxed_str())
        });
        running.push(DevApp {
            member: m,
            dir,
            port,
            label,
            output,
            server: None,
            mail: None,
        });
    }
    let secret = if at_root {
        match k_server::process::random_token() {
            Ok(t) => Some(format!("dev-{t}")),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    // Starts (or restarts) every app; the first failure's exit status.
    let start_all = |running: &mut Vec<DevApp>| -> Result<(), ExitCode> {
        let serves: Vec<(Vec<String>, u16)> = running
            .iter()
            .map(|a| (a.output.manifest.serves.clone(), a.port))
            .collect();
        let mut failed = None;
        for (i, app) in running.iter_mut().enumerate() {
            let mut wiring = Wiring {
                vars: std::collections::BTreeMap::new(),
                label: app.label,
                inspect: if i == 0 { args.inspect } else { None },
            };
            if let Some(secret) = &secret {
                wiring
                    .vars
                    .insert("K_SERVICE_TOKEN".to_owned(), secret.clone());
                wiring.vars.insert(
                    "K_PUBLIC_ORIGIN".to_owned(),
                    format!("http://localhost:{}", app.port),
                );
                for remote in &app.output.manifest.remotes {
                    if let Some((_, p)) = serves.iter().find(|(s, _)| s.contains(&remote.protocol))
                    {
                        wiring
                            .vars
                            .insert(remote.url_env.clone(), format!("http://127.0.0.1:{p}"));
                        wiring.vars.insert(remote.token_env.clone(), secret.clone());
                    }
                }
            }
            if let Some(old) = app.server.take()
                && let Err(e) = old.shutdown()
            {
                crate::devenv::say(app.label, &format!("error: {e}"));
            }
            match start(&app.output, &app.dir, app.port, &wiring, &mut app.mail) {
                Ok(s) => app.server = Some(s),
                Err(code) => {
                    failed.get_or_insert(code);
                }
            }
        }
        failed.map_or(Ok(()), Err)
    };
    if let Err(code) = start_all(&mut running) {
        for app in &mut running {
            if let Some(s) = app.server.take() {
                let _ = s.shutdown();
            }
        }
        return code;
    }
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
    let watched = if at_root {
        loaded.root.clone()
    } else {
        args.project.clone()
    };
    let mut seen = fingerprint(&watched);
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
            for app in &mut running {
                if let Some(mail) = &mut app.mail {
                    mail.poll(app.label);
                }
            }
            if !watch {
                continue;
            }
            let now = fingerprint(&watched);
            if now == seen {
                continue;
            }
            // Let a burst of writes (an editor saving several files) settle.
            tokio::time::sleep(WATCH_INTERVAL).await;
            seen = fingerprint(&watched);
            eprintln!(
                "change detected; rebuilding{}",
                if at_root { " every app" } else { "" }
            );
            let keep = if at_root {
                "the last good builds keep serving"
            } else {
                "the last good build keeps serving"
            };
            let Ok(next) = workspace::load(&args.project) else {
                eprintln!("{keep}");
                continue;
            };
            let mut outputs = Vec::new();
            for app in &running {
                match compile_member(&next, app.member, &app.dir, &options, true) {
                    Ok((o, _)) => outputs.push(o),
                    Err(_) => break,
                }
            }
            if outputs.len() != running.len() {
                eprintln!("{keep}");
                continue;
            }
            for (app, output) in running.iter_mut().zip(outputs) {
                app.output = output;
            }
            if start_all(&mut running).is_err() {
                eprintln!(
                    "{} stopped until the next change",
                    if at_root {
                        "some servers are"
                    } else {
                        "the server is"
                    }
                );
            }
        }
    });
    let mut code = ExitCode::SUCCESS;
    if let Err(c) = stopped {
        code = c;
    }
    for app in &mut running {
        if let Some(Err(e)) = app.server.take().map(k_server::Server::shutdown) {
            crate::devenv::say(app.label, &format!("error: {e}"));
            code = ExitCode::from(1);
        }
        if let Some(mail) = &mut app.mail {
            mail.poll(app.label);
        }
    }
    code
}

/// Without the server runtime: reports that `k run` and `k dev` are
/// unavailable and exits 2.
#[cfg(not(feature = "server"))]
fn serve_dev(_args: &RunArgs, _watch: bool) -> ExitCode {
    eprintln!("error: this `k` was built without the server runtime (feature `server`)");
    ExitCode::from(2)
}

/// A snapshot of the project's files (path, size and modification time),
/// skipping hidden directories and build output like the source loader,
/// but keeping every `.env` (a workspace member's too). Two equal
/// snapshots mean nothing changed.
#[cfg(feature = "server")]
pub(crate) fn fingerprint(
    project: &Path,
) -> Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)> {
    fn walk(dir: &Path, out: &mut Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)>) {
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
                    walk(&path, out);
                }
            } else if !name.starts_with('.') || name == ".env" {
                out.push((path, meta.len(), meta.modified().ok()));
            }
        }
    }
    let mut out = Vec::new();
    walk(project, &mut out);
    out.sort();
    out
}

/// The `content-type` of a development build's asset, by extension (as
/// `dist/app` serves them).
#[cfg(feature = "server")]
fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}
