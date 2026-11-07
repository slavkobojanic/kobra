//! Applying migrations and recording them in `k_migrations`
//! (`docs/compiler/db.md`, "Migrations").
//!
//! Each migration's statements were generated at build time for both
//! dialects (from the `Schema` builder calls in `migrations/*.k`); the
//! server applies the pending ones in order at startup, each in its own
//! transaction, before it opens its port. Diffing entities against
//! `schema.lock` (`k migrate new`) is compile-time work in `k_sema` and is
//! not in this module.

use crate::driver::Database;
use crate::error::DbError;
use crate::sql::{Dialect, quote_identifier};
use crate::value::Value;

/// The table that records applied migrations. The `k_` prefix is reserved
/// for the stdlib, so no entity can collide with it.
pub const MIGRATIONS_TABLE: &str = "k_migrations";

/// One migration, ready to apply.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Migration {
    /// The file stem, `0002_add_note_pinning`; also its key in
    /// `k_migrations`.
    pub name: String,
    /// The statements for SQLite.
    pub sqlite: Vec<String>,
    /// The statements for Postgres.
    pub postgres: Vec<String>,
}

impl Migration {
    /// The statements for `dialect`.
    #[must_use]
    pub fn statements(&self, dialect: Dialect) -> &[String] {
        match dialect {
            Dialect::Sqlite => &self.sqlite,
            Dialect::Postgres => &self.postgres,
        }
    }
}

async fn ensure_table(db: &Database) -> Result<(), DbError> {
    let applied_at = match db.dialect() {
        Dialect::Sqlite => "TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        Dialect::Postgres => "timestamptz NOT NULL DEFAULT now()",
    };
    db.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS {} ({} TEXT PRIMARY KEY, {} {applied_at})",
            quote_identifier(MIGRATIONS_TABLE),
            quote_identifier("name"),
            quote_identifier("applied_at")
        ),
        &[],
    )
    .await
    .map(|_| ())
}

/// Returns the names of the applied migrations, in application order.
///
/// # Errors
///
/// Any database failure.
pub async fn applied(db: &Database) -> Result<Vec<String>, DbError> {
    ensure_table(db).await?;
    let rows = db
        .query(
            &format!(
                "SELECT {} FROM {} ORDER BY {}",
                quote_identifier("name"),
                quote_identifier(MIGRATIONS_TABLE),
                quote_identifier("name")
            ),
            &[],
        )
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            r.values.into_iter().next().and_then(|v| match v {
                Value::Text(s) => Some(s),
                _ => None,
            })
        })
        .collect())
}

/// Applies every migration of `migrations` that `k_migrations` doesn't
/// record yet, in order, each in one transaction together with its
/// `k_migrations` row. Returns the names it applied.
///
/// An applied migration that is no longer in `migrations` (a history that
/// diverged), or applied migrations that aren't the build's first ones in
/// order, are an error before anything runs. On Postgres, concurrent
/// starters serialize on the advisory lock [`ADVISORY_LOCK_KEY`].
///
/// # Errors
///
/// [`DbError::Migration`] naming the failed migration (which is rolled
/// back), or any database failure.
pub async fn apply(db: &Database, migrations: &[Migration]) -> Result<Vec<String>, DbError> {
    if db.dialect() == Dialect::Postgres {
        // Replicas starting together take turns: the first applies the
        // pending migrations, the others then find nothing left to do. The
        // lock is held by the session, so it is released when the
        // connection closes even if this process dies mid-migration.
        db.execute(
            &format!("SELECT pg_advisory_lock({ADVISORY_LOCK_KEY})"),
            &[],
        )
        .await?;
        let result = apply_unlocked(db, migrations).await;
        let unlocked = db
            .execute(
                &format!("SELECT pg_advisory_unlock({ADVISORY_LOCK_KEY})"),
                &[],
            )
            .await;
        return match (result, unlocked) {
            (Ok(ran), Ok(_)) => Ok(ran),
            (Err(e), _) | (Ok(_), Err(e)) => Err(e),
        };
    }
    apply_unlocked(db, migrations).await
}

/// The `pg_advisory_lock` key migrators serialize on: `k_migrations` as
/// ASCII digits, so it is recognizable in `pg_locks`.
pub const ADVISORY_LOCK_KEY: i64 = 0x6b5f_6d69_6772;

async fn apply_unlocked(db: &Database, migrations: &[Migration]) -> Result<Vec<String>, DbError> {
    let done = applied(db).await?;
    for name in &done {
        if !migrations.iter().any(|m| &m.name == name) {
            return Err(DbError::Migration {
                name: name.clone(),
                message: "the database records this migration but the build doesn't contain it"
                    .to_owned(),
            });
        }
    }
    // The applied migrations must be the build's first ones, in order: a
    // migration inserted before an applied one would otherwise run last,
    // against a schema its author never saw.
    for (i, name) in done.iter().enumerate() {
        if let Some(m) = migrations.get(i).filter(|m| &m.name != name) {
            return Err(DbError::Migration {
                name: m.name.clone(),
                message: format!(
                    "this migration comes before `{name}`, which the database already applied; \
                     new migrations must come after every applied one"
                ),
            });
        }
    }
    let mut ran = Vec::new();
    for m in migrations.iter().skip(done.len()) {
        let fail = |e: DbError| DbError::Migration {
            name: m.name.clone(),
            message: e.to_string(),
        };
        db.execute("BEGIN", &[]).await.map_err(fail)?;
        let mut result = Ok(());
        for statement in m.statements(db.dialect()) {
            if let Err(e) = db.execute(statement, &[]).await {
                result = Err(e);
                break;
            }
        }
        if result.is_ok() {
            let insert = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                quote_identifier(MIGRATIONS_TABLE),
                quote_identifier("name"),
                match db.dialect() {
                    Dialect::Sqlite => "?1",
                    Dialect::Postgres => "$1",
                }
            );
            result = db
                .execute(&insert, &[Value::Text(m.name.clone())])
                .await
                .map(|_| ());
        }
        match result {
            Ok(()) => db.execute("COMMIT", &[]).await.map_err(fail)?,
            Err(e) => {
                let _ = db.execute("ROLLBACK", &[]).await;
                return Err(fail(e));
            }
        };
        ran.push(m.name.clone());
    }
    Ok(ran)
}
