//! Support for database tests that run on every available database
//! (`docs/compiler/testing.md`, "Database tests").
//!
//! SQLite is always available (an in-memory database per test). Postgres
//! joins when the environment variable [`POSTGRES_URL_ENV`] holds a
//! connection URL: each test then gets its own fresh schema on that
//! server, removed by [`TestDatabase::finish`], so tests can run in
//! parallel and leave nothing behind. Without the variable, Postgres is
//! skipped silently; with it set but unreachable, the test fails, because
//! the run asked for Postgres.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::driver::{Database, DatabaseOptions};
use crate::error::DbError;
use crate::sql::quote_identifier;

/// The environment variable that enables Postgres in database tests, for
/// example `postgres://postgres@localhost:5432/k_test`.
pub const POSTGRES_URL_ENV: &str = "K_TEST_POSTGRES_URL";

/// One database a test runs against.
#[derive(Debug)]
pub struct TestDatabase {
    /// `"sqlite"` or `"postgres"`, for failure messages.
    pub name: &'static str,
    /// The connected database, empty.
    pub db: Database,
    /// The Postgres schema to drop afterwards.
    schema: Option<String>,
}

impl TestDatabase {
    /// Removes what the test created (the Postgres schema). SQLite memory
    /// databases vanish on drop.
    ///
    /// # Errors
    ///
    /// Any database failure while dropping the schema.
    pub async fn finish(self) -> Result<(), DbError> {
        if let Some(schema) = &self.schema {
            self.db
                .execute(
                    &format!("DROP SCHEMA {} CASCADE", quote_identifier(schema)),
                    &[],
                )
                .await?;
        }
        Ok(())
    }
}

static NEXT: AtomicU32 = AtomicU32::new(0);

/// Connects to every database the test run has: SQLite always, Postgres
/// when [`POSTGRES_URL_ENV`] is set.
///
/// # Errors
///
/// Failing to open SQLite, or to connect to a configured Postgres server.
pub async fn test_databases() -> Result<Vec<TestDatabase>, DbError> {
    let mut out = vec![TestDatabase {
        name: "sqlite",
        db: Database::connect("sqlite::memory:", &DatabaseOptions::default()).await?,
        schema: None,
    }];
    if let Ok(url) = std::env::var(POSTGRES_URL_ENV) {
        if !url.is_empty() {
            let db = Database::connect(&url, &DatabaseOptions::default()).await?;
            let schema = format!(
                "k_test_{}_{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            db.execute(&format!("CREATE SCHEMA {}", quote_identifier(&schema)), &[])
                .await?;
            db.execute(
                &format!("SET search_path TO {}", quote_identifier(&schema)),
                &[],
            )
            .await?;
            out.push(TestDatabase {
                name: "postgres",
                db,
                schema: Some(schema),
            });
        }
    }
    Ok(out)
}
