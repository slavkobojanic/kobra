//! Running as `dist/app`: the `k` executable with an app appended by
//! `k build` (`docs/compiler/server.md`, "Packaging" and "Startup and
//! graceful shutdown"; `docs/design/38-deployment.md`, "Commands").
//!
//! | Command | Does |
//! |---|---|
//! | `app` / `app serve` | validates the configuration (all problems at once, exit 78, before any port opens), then serves on `PORT` until `SIGTERM` or `SIGINT` and exits 0 |
//! | `app probe <url>` | exits 0 on a 2xx answer within 3 s, 1 otherwise |
//! | `app config` | lists the configuration variables the build reads, never their values |
//!
//! `serve` opens `DATABASE_URL` and applies the build's pending
//! migrations before the port opens (a failure exits 1); v0 runs no jobs
//! yet (server.md).

use std::process::ExitCode;
use std::time::Duration;

use k_server::app::{AppPayload, ConfigSpec, probe, read_file};
use k_server::config::{self, ConfigValue, EXIT_CONFIG, ProcessEnvironment};
use k_server::http::StaticAsset;
use k_server::{JsModule, Server, ServerBundle, ServerOptions};

/// Returns `Some(exit code)` if this executable carries an app (and runs
/// it), `None` for the plain `k` toolchain.
pub fn run_if_app() -> Option<ExitCode> {
    let exe = std::env::current_exe().ok()?;
    let payload = match read_file(&exe) {
        Ok(Some(p)) => p,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("error: {e}");
            return Some(ExitCode::from(2));
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    Some(match args.first().map(String::as_str) {
        None | Some("serve") => serve(payload),
        Some("probe") => match args.get(1) {
            Some(url) if probe(url, Duration::from_secs(3)) => ExitCode::SUCCESS,
            Some(_) => ExitCode::from(1),
            None => {
                eprintln!("usage: app probe <url>");
                ExitCode::from(2)
            }
        },
        Some("config") => {
            print!("{}", config_table(&specs(&payload)));
            ExitCode::SUCCESS
        }
        Some("migrate") => migrate(payload),
        Some("jobs") => match (args.get(1).map(String::as_str), args.get(2)) {
            (Some("list"), flag) if flag.is_none_or(|f| f == "--dead") => {
                jobs_list(payload, flag.is_some())
            }
            (Some("retry"), Some(id)) => jobs_retry(payload, id),
            _ => {
                eprintln!("usage: app jobs list [--dead] | app jobs retry <id>");
                ExitCode::from(2)
            }
        },
        Some(other) => {
            eprintln!(
                "error: unknown command `{other}`; expected serve, migrate, jobs, probe or config"
            );
            ExitCode::from(2)
        }
    })
}

/// The archive's configuration table, with the server's own variables
/// added when the archive doesn't list them.
fn specs(payload: &AppPayload) -> Vec<ConfigSpec> {
    let mut out = payload.config.clone();
    for d in ConfigSpec::server_defaults() {
        if !out.iter().any(|c| c.env == d.env) {
            out.push(d);
        }
    }
    out.sort_by(|a, b| a.env.cmp(&b.env));
    out
}

/// The table `app config` prints: variable, type, whether it is required,
/// its default and whether it is a secret. Values are never printed.
pub fn config_table(specs: &[ConfigSpec]) -> String {
    let width = specs.iter().map(|c| c.env.len()).max().unwrap_or(0);
    let ty_width = specs.iter().map(|c| c.ty.len()).max().unwrap_or(0);
    let mut out = String::new();
    for c in specs {
        let need = if c.required { "required" } else { "optional" };
        let default = c
            .default
            .as_ref()
            .map_or_else(String::new, |d| format!("  default {d:?}"));
        let secret = if c.secret { "  secret" } else { "" };
        out.push_str(&format!(
            "{:width$}  {:ty_width$}  {need}{default}{secret}\n",
            c.env, c.ty
        ));
    }
    out
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}

fn serve(payload: AppPayload) -> ExitCode {
    let fields: Vec<config::ConfigField> = specs(&payload)
        .iter()
        .filter_map(ConfigSpec::field)
        .collect();
    // Configuration first: every problem at once, before a port opens.
    let values = match config::resolve(&fields, &ProcessEnvironment) {
        Ok(v) => v,
        Err(problems) => {
            eprint!("{}", config::report(&problems));
            return ExitCode::from(u8::try_from(EXIT_CONFIG).unwrap_or(78));
        }
    };
    let port = match values.get("PORT") {
        Some(ConfigValue::Int(n)) => u16::try_from(*n).ok(),
        _ => Some(8080),
    };
    let Some(port) = port else {
        eprint!(
            "{}",
            config::report(&[config::Problem {
                env: "PORT".into(),
                message: "is not a port (0 to 65535)".into(),
            }])
        );
        return ExitCode::from(78);
    };
    let origin = match values.get("K_PUBLIC_ORIGIN") {
        Some(ConfigValue::String(o)) => o.trim_end_matches('/').to_owned(),
        _ => format!("http://localhost:{port}"),
    };
    let grace = match values.get("K_SHUTDOWN_GRACE") {
        Some(ConfigValue::Duration(ms)) => Duration::from_secs_f64((ms / 1000.0).max(0.0)),
        _ => Duration::from_secs(25),
    };
    let bundle = ServerBundle {
        modules: payload
            .modules
            .into_iter()
            .map(|f| JsModule {
                specifier: f.path,
                source: f.text,
            })
            .collect(),
        entry: payload.entry,
        rpc_paths: payload.rpc_paths.into_iter().collect(),
    };
    let mut options = ServerOptions::new(std::net::SocketAddr::from(([0, 0, 0, 0], port)), bundle);
    options.public_origin = origin;
    options.shutdown_grace = grace;
    options.config = values;
    options.forgery_exempt = payload.forgery_exempt;
    // The database opens and migrates before any worker starts or the port
    // opens; a failure exits 1 (server.md, "Startup").
    options.database = k_server::db::DatabaseSetup::from_config(payload.database, &options.config);
    // The jobs worker runs next to the listener (30-background-jobs.md).
    options.jobs = payload.jobs;
    options.assets = payload
        .assets
        .into_iter()
        .map(|f| StaticAsset {
            content_type: content_type(&f.path).to_owned(),
            immutable: is_hashed(&f.path),
            path: f.path,
            body: f.text.into_bytes().into(),
        })
        .collect();
    match Server::start(options) {
        Ok(server) => {
            eprintln!(
                "{} serving on port {}",
                payload.name,
                server.local_addr().port()
            );
            match server.run_until_signal() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

/// The archive's database from the process environment, opened (and
/// migrated when `migrate`), or the exit code: 78 for invalid
/// configuration, 1 for a database that can't be opened or a build
/// without one.
fn open_database(
    payload: AppPayload,
    migrate: bool,
) -> Result<(std::sync::Arc<k_server::db::ProcessDatabase>, Vec<String>), ExitCode> {
    let fields: Vec<config::ConfigField> = specs(&payload)
        .iter()
        .filter_map(ConfigSpec::field)
        .collect();
    let values = match config::resolve(&fields, &ProcessEnvironment) {
        Ok(v) => v,
        Err(problems) => {
            eprint!("{}", config::report(&problems));
            return Err(ExitCode::from(u8::try_from(EXIT_CONFIG).unwrap_or(78)));
        }
    };
    let manifest = payload.database.unwrap_or_default();
    let Some(setup) = k_server::db::DatabaseSetup::from_config(Some(manifest), &values) else {
        eprintln!("error: {} has no database (DATABASE_URL)", payload.name);
        return Err(ExitCode::from(1));
    };
    k_server::db::ProcessDatabase::open_with(setup, migrate).map_err(|e| {
        eprintln!("error: database: {e}");
        ExitCode::from(1)
    })
}

/// `app migrate`: applies the pending migrations and exits
/// (38-deployment.md, "Commands"): 0 when the schema is current, 1 when a
/// migration failed (it is rolled back).
fn migrate(payload: AppPayload) -> ExitCode {
    match open_database(payload, true) {
        Ok((_, applied)) => {
            if applied.is_empty() {
                println!("the database is up to date");
            }
            for name in applied {
                println!("applied migration {name}");
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn block_on<T>(f: impl Future<Output = T>) -> Option<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .map(|r| r.block_on(f))
}

/// `app jobs list [--dead]`: one line per job, tab-separated: id, name,
/// status, attempts, time (due, or when it died, RFC 3339) and the last
/// error. Never runs migrations.
fn jobs_list(payload: AppPayload, dead_only: bool) -> ExitCode {
    let (db, _) = match open_database(payload, false) {
        Ok(d) => d,
        Err(code) => return code,
    };
    match block_on(db.list_jobs(dead_only)) {
        Some(Ok(rows)) => {
            print!("{}", jobs_table(&rows));
            ExitCode::SUCCESS
        }
        Some(Err(e)) => {
            eprintln!("error: database: {e}");
            ExitCode::from(1)
        }
        None => ExitCode::from(2),
    }
}

/// The text `jobs list` prints.
pub fn jobs_table(rows: &[k_server::db::JobRow]) -> String {
    let mut out = String::from("id\tname\tstatus\tattempts\tat\tlast error\n");
    for r in rows {
        let error = r
            .last_error
            .as_deref()
            .unwrap_or("")
            .replace(['\n', '\t'], " ");
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{error}\n",
            r.id,
            r.name,
            r.status,
            r.attempts,
            rfc3339(r.at)
        ));
    }
    out
}

/// Epoch milliseconds as `2026-11-07T14:40:00Z`.
fn rfc3339(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// `app jobs retry <id>`: puts a dead job back in the queue with fresh
/// attempts; exit 1 when there is no dead job with that id.
fn jobs_retry(payload: AppPayload, id: &str) -> ExitCode {
    let (db, _) = match open_database(payload, false) {
        Ok(d) => d,
        Err(code) => return code,
    };
    match block_on(db.requeue_job(id, k_server::jobs::now_ms())) {
        Some(Ok(true)) => {
            println!("queued {id}");
            ExitCode::SUCCESS
        }
        Some(Ok(false)) => {
            eprintln!("error: no dead job has id {id}");
            ExitCode::from(1)
        }
        Some(Err(e)) => {
            eprintln!("error: database: {e}");
            ExitCode::from(1)
        }
        None => ExitCode::from(2),
    }
}

/// `true` for a content-hashed file name (`app.3f2a1b9c.js`).
pub fn is_hashed(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .split('.')
        .any(|p| p.len() == 8 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}
