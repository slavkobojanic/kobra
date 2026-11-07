//! `k migrate new`, `k migrate apply` and `k migrate status`
//! (`docs/design/19-persistence.md`, "Migrations").
//!
//! * `new "description"` writes the next migration from the difference
//!   between the entities and `schema.lock`, and the updated lock
//!   ([`k_codegen::plan_migration`]); `--empty` writes a migration without
//!   schema changes, for data. It changes nothing when the entities match
//!   the lock, and writes nothing when it refuses a change (exit 1, one
//!   explanation per change).
//! * `apply` applies the pending migrations to the development database,
//!   `DATABASE_URL` from the environment or the project's `.env` (the
//!   environment wins), as `k run` would at startup.
//! * `status` lists every migration of the build as `applied` or `pending`
//!   in that database, without changing it.

use std::path::Path;
use std::process::ExitCode;

use k_codegen::{MigrationError, MigrationPlan, plan_migration};

use crate::cli::{MigrateNewArgs, ProjectArgs};
use crate::{project, sources};

/// Runs `k migrate new`.
pub fn new(args: &MigrateNewArgs) -> ExitCode {
    let project = match project::load(&args.project, "migrate") {
        Ok(p) => p,
        Err(code) => return code,
    };
    let analysis = project.host.analysis();
    match plan_migration(&analysis, &args.description, args.empty) {
        Ok(MigrationPlan::UpToDate) => {
            println!("nothing to migrate: the entities match schema.lock");
            ExitCode::SUCCESS
        }
        Ok(MigrationPlan::Draft(draft)) => {
            let file = project.root.join(&draft.file);
            let lock = project.root.join("schema.lock");
            let written = std::fs::create_dir_all(project.root.join("migrations"))
                .and_then(|()| std::fs::write(&file, &draft.source))
                .and_then(|()| std::fs::write(&lock, &draft.lock));
            if let Err(e) = written {
                eprintln!("error: cannot write the migration: {e}");
                return ExitCode::from(2);
            }
            println!("wrote {}", sources::display(&file));
            println!("wrote {}", sources::display(&lock));
            ExitCode::SUCCESS
        }
        Err(MigrationError::Codegen(k_codegen::CodegenError::HasErrors { count })) => {
            project.report_errors(count)
        }
        Err(MigrationError::Refused(problems)) => {
            eprintln!("error: `k migrate new` can't write this migration:");
            for p in problems {
                eprintln!("  - {p}");
            }
            eprintln!("nothing was written");
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

/// Runs `k migrate apply`: prints `applied migration <name>` for each
/// migration applied, or `the database is up to date`.
pub fn apply(args: &ProjectArgs) -> ExitCode {
    database(&args.project, true)
}

/// Runs `k migrate status`: one line per migration of the build,
/// `applied  <name>` or `pending  <name>`, in application order.
pub fn status(args: &ProjectArgs) -> ExitCode {
    database(&args.project, false)
}

#[cfg(feature = "server")]
fn database(root: &Path, migrate: bool) -> ExitCode {
    use k_codegen::{CodegenError, Entry, Environment, Options, Profile, generate};
    use k_server::config::{ConfigField, DevEnvironment, report, resolve};
    let project = match project::load(root, "migrate") {
        Ok(p) => p,
        Err(code) => return code,
    };
    let options = Options {
        profile: Profile::Development,
        environment: Environment::Development,
        entry: Entry::App,
    };
    let analysis = project.host.analysis();
    let output = match generate(&analysis, &options) {
        Ok(o) => o,
        Err(CodegenError::HasErrors { count }) => return project.report_errors(count),
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(manifest) = output.database.clone() else {
        eprintln!(
            "error: `{}` has no database: it declares no entities or migrations",
            sources::display(root)
        );
        return ExitCode::from(1);
    };
    // Only the database's own settings: the rest of the app's
    // configuration isn't needed to migrate.
    let mut fields: Vec<ConfigField> = output
        .manifest
        .config
        .iter()
        .filter(|c| c.env == "DATABASE_URL" || c.env == "DATABASE_PASSWORD")
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
    if fields.iter().all(|f| f.env != "DATABASE_URL") {
        fields.extend(ConfigField::from_manifest(
            "DATABASE_URL",
            "k.db.DatabaseConfig.url",
            "String",
            false,
            true,
            None,
            &[],
            None,
        ));
    }
    let values = match resolve(&fields, &DevEnvironment::load(root, true)) {
        Ok(v) => v,
        Err(problems) => {
            eprint!("{}", report(&problems));
            return ExitCode::from(1);
        }
    };
    let Some(setup) = k_server::db::DatabaseSetup::from_config(Some(manifest), &values) else {
        eprintln!("error: no database is configured: set DATABASE_URL");
        return ExitCode::from(1);
    };
    // The same database `k run` and `k dev` open: a relative SQLite path is
    // the project's.
    let setup = match setup.rooted_at(root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: database: {e}");
            return ExitCode::from(1);
        }
    };
    if migrate {
        return match k_server::db::ProcessDatabase::open_with(setup, true) {
            Ok((_, applied)) => {
                if applied.is_empty() {
                    println!("the database is up to date");
                }
                for name in applied {
                    println!("applied migration {name}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: database: {e}");
                ExitCode::from(1)
            }
        };
    }
    let names: Vec<String> = setup
        .manifest
        .migrations
        .iter()
        .map(|m| m.name.clone())
        .collect();
    let applied = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| k_db::error::DbError::Connection(e.to_string()))
        .and_then(|rt| {
            rt.block_on(async {
                let options = k_db::driver::DatabaseOptions {
                    password: setup.password.clone(),
                };
                let db = k_db::driver::Database::connect(&setup.url, &options).await?;
                k_db::migrate::applied(&db).await
            })
        });
    match applied {
        Ok(applied) => {
            for name in names {
                let state = if applied.contains(&name) {
                    "applied"
                } else {
                    "pending"
                };
                println!("{state}  {name}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: database: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(feature = "server"))]
fn database(_root: &Path, _migrate: bool) -> ExitCode {
    eprintln!("error: this `k` was built without the server runtime (feature `server`)");
    ExitCode::from(2)
}
