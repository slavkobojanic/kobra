//! `k migrate new`, `k migrate rename`, `k migrate apply` and `k migrate
//! status` (`docs/design/19-persistence.md`, "Migrations" and "Renames").
//!
//! * `new "description"` writes the next migration from the difference
//!   between the entities and `schema.lock`, and the updated lock
//!   ([`k_codegen::plan_migration_with`]); `--empty` writes a migration
//!   without schema changes, for data; `--drop-retired` drops the tables
//!   and columns earlier migrations retired; `--rename NAME` renames the
//!   table of an entity or the column of a field to follow its current
//!   name. It changes nothing when there is nothing to write, and writes
//!   nothing when it refuses a change (exit 1, one explanation per
//!   change).
//! * `rename OLD NEW` records that an entity or field was renamed in the
//!   code, keeping its table or column: only `schema.lock` changes
//!   ([`k_codegen::record_rename`]).
//! * `apply` applies the pending migrations to the development database,
//!   `DATABASE_URL` from the environment, the project's `.env` or
//!   `[dev.env]` (the environment wins), else `sqlite:.k/<package>.db`, as
//!   `k run` would at startup.
//! * `status` lists every migration of the build as `applied` or `pending`
//!   in that database, without changing it.

use std::path::Path;
use std::process::ExitCode;

use k_codegen::{MigrationError, MigrationPlan, MigrationRequest, plan_migration_with};

use crate::cli::{MigrateNewArgs, MigrateRenameArgs, ProjectArgs};
use crate::{project, sources};

/// Runs `k migrate new`.
pub fn new(args: &MigrateNewArgs) -> ExitCode {
    let project = match project::load(&args.project, "migrate") {
        Ok(p) => p,
        Err(code) => return code,
    };
    let analysis = project.host.analysis();
    let request = if args.empty {
        MigrationRequest::Empty
    } else if args.drop_retired {
        MigrationRequest::DropRetired
    } else if !args.rename.is_empty() {
        MigrationRequest::Rename(&args.rename)
    } else {
        MigrationRequest::Diff
    };
    match plan_migration_with(&analysis, &args.description, request) {
        Ok(MigrationPlan::UpToDate) => {
            if args.drop_retired {
                println!("nothing to drop: schema.lock records no retired table or column");
            } else {
                println!("nothing to migrate: the entities match schema.lock");
            }
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
        Err(e) => refused(&project, e, "k migrate new", "this migration"),
    }
}

/// Reports why a `k migrate` command wrote nothing: the compiler's code
/// for a program with errors, 1 with one line per problem for a refusal,
/// 2 otherwise.
fn refused(project: &project::Project, e: MigrationError, command: &str, what: &str) -> ExitCode {
    match e {
        MigrationError::Codegen(k_codegen::CodegenError::HasErrors { count }) => {
            project.report_errors(count)
        }
        MigrationError::Refused(problems) => {
            eprintln!("error: `{command}` can't write {what}:");
            for p in problems {
                eprintln!("  - {p}");
            }
            eprintln!("nothing was written");
            ExitCode::from(1)
        }
        e => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

/// Runs `k migrate rename`: updates `schema.lock` so the entity or field
/// `old` is recorded under its new name, keeping its table or column.
pub fn rename(args: &MigrateRenameArgs) -> ExitCode {
    let project = match project::load(&args.project, "migrate") {
        Ok(p) => p,
        Err(code) => return code,
    };
    let analysis = project.host.analysis();
    match k_codegen::record_rename(&analysis, &args.old, &args.new) {
        Ok(lock) => {
            let path = project.root.join("schema.lock");
            if let Err(e) = std::fs::write(&path, lock) {
                eprintln!("error: cannot write `{}`: {e}", sources::display(&path));
                return ExitCode::from(2);
            }
            println!(
                "recorded `{}` as `{}`; its storage keeps its name, so no migration is needed",
                args.old, args.new
            );
            println!("wrote {}", sources::display(&path));
            ExitCode::SUCCESS
        }
        Err(e) => refused(&project, e, "k migrate rename", "this rename"),
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

/// The body of `k migrate apply` (`migrate`) and `k migrate status`
/// (`!migrate`): builds the project at `root` for development, resolves
/// `DATABASE_URL` from the development environment, connects, and either
/// applies the pending migrations or lists each one as applied or pending.
/// Exits 1 for a project without a database, a configuration problem or a
/// database failure, and with the compiler's code when the project has
/// errors.
#[cfg(feature = "server")]
fn database(root: &Path, migrate: bool) -> ExitCode {
    use k_codegen::{CodegenError, Entry, Environment, Options, Profile, generate};
    use k_server::config::{ConfigField, report, resolve};
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
    // The development environment `k dev` gives the app: `.env`, then
    // `[dev.env]`, then the default `sqlite:.k/<package>.db`.
    let wiring = std::collections::BTreeMap::new();
    let inputs = crate::devenv::DevInputs {
        project: root,
        package: project.name(),
        port: 8080,
        wiring: &wiring,
    };
    let env = match crate::devenv::prepare(&inputs, &fields, &mut || None) {
        Ok(p) => p.env,
        Err(problem) => {
            eprintln!("{problem}");
            return ExitCode::from(1);
        }
    };
    let values = match resolve(&fields, &env) {
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

/// Without the server runtime: reports that the database commands are
/// unavailable and exits 2.
#[cfg(not(feature = "server"))]
fn database(_root: &Path, _migrate: bool) -> ExitCode {
    eprintln!("error: this `k` was built without the server runtime (feature `server`)");
    ExitCode::from(2)
}
