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
fn compile(project: &Path, options: &Options, command: &str) -> Result<Output, ExitCode> {
    let (loaded, member) = workspace::load_member(project, command)?;
    compile_member(&loaded, member, project, options).map(|(o, _)| o)
}

/// Generates member `m`'s bundles, shown as `project`; also returns the
/// `rpc.lock` its program implies (`k_sema::Analysis::rpc_lock`).
fn compile_member(
    loaded: &workspace::Loaded,
    m: usize,
    project: &Path,
    options: &Options,
) -> Result<(Output, Option<String>), ExitCode> {
    let host = loaded.host(m);
    let analysis = host.analysis();
    match generate(&analysis, options) {
        Ok(output) if output.server_entry.is_none() && !output.manifest.static_only => {
            eprintln!(
                "error: `{}` is a library: it has no `app` to build or run",
                sources::display(project)
            );
            Err(ExitCode::from(2))
        }
        Ok(output) => Ok((output, analysis.rpc_lock().ok().flatten())),
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

/// Writes `lock` to `dir/rpc.lock` when it differs from what is there
/// (`docs/design/40-workspaces-and-services.md`, "Compatibility between
/// deploys"): `k build` records the shape of every protocol the app
/// serves, after `k check` accepted the change.
fn write_rpc_lock(dir: &Path, lock: Option<&str>) -> Result<(), ExitCode> {
    let Some(lock) = lock else {
        return Ok(());
    };
    let path = dir.join("rpc.lock");
    if std::fs::read_to_string(&path).is_ok_and(|old| old == lock) {
        return Ok(());
    }
    match std::fs::write(&path, lock) {
        Ok(()) => {
            eprintln!("wrote {}", sources::display(&path));
            Ok(())
        }
        Err(e) => {
            eprintln!("error: cannot write `{}`: {e}", sources::display(&path));
            Err(ExitCode::from(2))
        }
    }
}

/// Runs `k build`: one project into its `dist/` (or `--out`), or, at a
/// workspace root, every app member into `dist/<name>/` below the root (or
/// `--out`). Each app's `rpc.lock` is updated.
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
            let (output, lock) = match compile_member(&loaded, m, &dir, &options) {
                Ok(o) => o,
                Err(code) => return code,
            };
            if let Err(code) = write_rpc_lock(&dir, lock.as_deref()) {
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
    let (output, lock) = match compile_member(&loaded, member, &args.project, &options) {
        Ok(o) => o,
        Err(code) => return code,
    };
    if let Err(code) = write_rpc_lock(&args.project, lock.as_deref()) {
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
    let _ = server.shutdown();
    let page = page.map_err(std::io::Error::other)?;
    if page.status != 200 {
        return Err(std::io::Error::other(format!(
            "the app's document answered {} instead of 200",
            page.status
        )));
    }
    std::fs::write(out.join("public").join("index.html"), &page.body)
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

/// Runs `k run`.
pub fn run(args: &RunArgs) -> ExitCode {
    let options = Options {
        profile: Profile::Development,
        environment: Environment::Development,
        entry: Entry::App,
    };
    let output = match compile(&args.project, &options, "run") {
        Ok(o) => o,
        Err(code) => return code,
    };
    serve(&output, &args.project, args.port)
}

/// Serves a development build until `SIGTERM` or Ctrl-C (see [`start`]).
#[cfg(feature = "server")]
fn serve(output: &Output, project: &Path, port: u16) -> ExitCode {
    let server = match start(output, project, port, &Wiring::default()) {
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

/// What `k dev` at a workspace root adds to one app's development
/// configuration: the variables wiring its remotes to their sibling
/// members, which win over the member's `.env`, and the app's name for the
/// `<name> serving on …` line.
#[derive(Clone, Debug, Default)]
pub(crate) struct Wiring {
    /// Variables and values.
    vars: std::collections::BTreeMap<String, String>,
    /// The app's name, printed before `serving on`.
    label: Option<String>,
}

/// Starts serving a development build on `port` (0 picks a free one) and
/// prints `serving on http://localhost:<port>`. The configuration comes
/// from the process environment and the project's `.env` (the process
/// wins; 25-config-and-secrets.md, "Per environment"), with `wiring`'s
/// variables over the `.env`, and is validated first: every problem is
/// printed and the error is exit status 78.
#[cfg(feature = "server")]
fn start(
    output: &Output,
    project: &Path,
    port: u16,
    wiring: &Wiring,
) -> Result<k_server::Server, ExitCode> {
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
    let mut env = DevEnvironment::load(project, true);
    env.dotenv.extend(wiring.vars.clone());
    let values = match resolve(&fields, &env) {
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
    // A relative SQLite path is the project's (each member's, at a
    // workspace root), and its directory is created.
    options.database =
        match k_server::db::DatabaseSetup::from_config(output.database.clone(), &options.config)
            .map(|d| d.rooted_at(project))
            .transpose()
        {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: database: {e}");
                return Err(ExitCode::from(1));
            }
        };
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
    match Server::start(options) {
        Ok(server) => {
            let prefix = wiring
                .label
                .as_ref()
                .map(|l| format!("{l} "))
                .unwrap_or_default();
            eprintln!(
                "{prefix}serving on http://localhost:{}",
                server.local_addr().port()
            );
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
pub(crate) const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

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
    if workspace::locate(&args.project).is_some_and(|l| l.member.is_none()) {
        return dev_workspace(&args.project, args.port, &options);
    }
    let output = match compile(&args.project, &options, "dev") {
        Ok(o) => o,
        Err(code) => return code,
    };
    watch(&output, &args.project, args.port, &options)
}

/// One app of `k dev` at a workspace root.
#[cfg(feature = "server")]
struct DevApp {
    member: usize,
    dir: std::path::PathBuf,
    port: u16,
    output: Output,
    server: Option<k_server::Server>,
}

/// `k dev` at a workspace root (`docs/design/40-workspaces-and-services.md`,
/// "Tooling"): every app member is built and served, each on its own port
/// (`port`, `port + 1`, … in dependency order; free ports for 0). Each
/// `remote` rule whose protocol a sibling member serves is wired to it:
/// the config's `url` variable is that member's address and its `token`
/// one development secret, generated per run, which every member also gets
/// as `K_SERVICE_TOKEN`. A change anywhere in the workspace rebuilds and
/// restarts every app; a build with errors keeps the last good one
/// serving.
#[cfg(feature = "server")]
fn dev_workspace(root: &Path, port: u16, options: &Options) -> ExitCode {
    let loaded = match workspace::load(root) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let apps: Vec<usize> = loaded
        .targets()
        .into_iter()
        .filter(|&m| loaded.workspace.members[m].is_app)
        .collect();
    if apps.is_empty() {
        eprintln!(
            "error: the workspace `{}` has no app to run",
            sources::display(root)
        );
        return ExitCode::from(2);
    }
    let mut running: Vec<DevApp> = Vec::new();
    for (i, &m) in apps.iter().enumerate() {
        let dir = loaded.member_dir(m);
        let output = match compile_member(&loaded, m, &dir, options) {
            Ok((o, _)) => o,
            Err(code) => return code,
        };
        let port = if port == 0 {
            match std::net::TcpListener::bind(("127.0.0.1", 0)).and_then(|l| l.local_addr()) {
                Ok(a) => a.port(),
                Err(e) => {
                    eprintln!("error: no free port: {e}");
                    return ExitCode::from(2);
                }
            }
        } else {
            match port.checked_add(u16::try_from(i).unwrap_or(u16::MAX)) {
                Some(p) => p,
                None => {
                    eprintln!("error: port {port} leaves no room for {} apps", apps.len());
                    return ExitCode::from(2);
                }
            }
        };
        running.push(DevApp {
            member: m,
            dir,
            port,
            output,
            server: None,
        });
    }
    let secret = match k_server::process::random_token() {
        Ok(t) => format!("dev-{t}"),
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let start_all = |running: &mut Vec<DevApp>| -> bool {
        let serves: Vec<(Vec<String>, u16)> = running
            .iter()
            .map(|a| (a.output.manifest.serves.clone(), a.port))
            .collect();
        let mut ok = true;
        for app in running.iter_mut() {
            let mut wiring = Wiring {
                vars: std::collections::BTreeMap::new(),
                label: Some(loaded.workspace.members[app.member].name.clone()),
            };
            wiring
                .vars
                .insert("K_SERVICE_TOKEN".to_owned(), secret.clone());
            wiring.vars.insert(
                "K_PUBLIC_ORIGIN".to_owned(),
                format!("http://localhost:{}", app.port),
            );
            for remote in &app.output.manifest.remotes {
                if let Some((_, p)) = serves.iter().find(|(s, _)| s.contains(&remote.protocol)) {
                    wiring
                        .vars
                        .insert(remote.url_env.clone(), format!("http://127.0.0.1:{p}"));
                    wiring.vars.insert(remote.token_env.clone(), secret.clone());
                }
            }
            if let Some(old) = app.server.take()
                && let Err(e) = old.shutdown()
            {
                eprintln!("error: {e}");
            }
            app.server = start(&app.output, &app.dir, app.port, &wiring).ok();
            ok &= app.server.is_some();
        }
        ok
    };
    if !start_all(&mut running) {
        for app in &mut running {
            if let Some(s) = app.server.take() {
                let _ = s.shutdown();
            }
        }
        return ExitCode::from(2);
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
    let mut seen = fingerprint(root);
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
            let now = fingerprint(root);
            if now == seen {
                continue;
            }
            tokio::time::sleep(WATCH_INTERVAL).await;
            seen = fingerprint(root);
            eprintln!("change detected; rebuilding every app");
            let Ok(next) = workspace::load(root) else {
                eprintln!("the last good builds keep serving");
                continue;
            };
            let mut outputs = Vec::new();
            for app in &running {
                match compile_member(&next, app.member, &app.dir, options) {
                    Ok((o, _)) => outputs.push(o),
                    Err(_) => break,
                }
            }
            if outputs.len() != running.len() {
                eprintln!("the last good builds keep serving");
                continue;
            }
            for (app, output) in running.iter_mut().zip(outputs) {
                app.output = output;
            }
            if !start_all(&mut running) {
                eprintln!("some servers are stopped until the next change");
            }
        }
    });
    let mut code = ExitCode::SUCCESS;
    if let Err(c) = stopped {
        code = c;
    }
    for app in &mut running {
        if let Some(Err(e)) = app.server.take().map(k_server::Server::shutdown) {
            eprintln!("error: {e}");
            code = ExitCode::from(1);
        }
    }
    code
}

#[cfg(not(feature = "server"))]
fn dev_workspace(_root: &Path, _port: u16, _options: &Options) -> ExitCode {
    eprintln!("error: this `k` was built without the server runtime (feature `server`)");
    ExitCode::from(2)
}

#[cfg(feature = "server")]
fn watch(output: &Output, project: &Path, port: u16, options: &Options) -> ExitCode {
    let mut server = match start(output, project, port, &Wiring::default()) {
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
            let Ok(next) = compile(project, options, "dev") else {
                eprintln!("the last good build keeps serving");
                continue;
            };
            if let Some(old) = server.take()
                && let Err(e) = old.shutdown()
            {
                eprintln!("error: {e}");
            }
            server = start(&next, project, port, &Wiring::default()).ok();
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
pub(crate) fn fingerprint(
    project: &Path,
) -> Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)> {
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
