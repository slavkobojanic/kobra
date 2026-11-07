//! The database drivers: SQLite through `rusqlite` (bundled SQLite, on the
//! blocking thread pool) and Postgres through `tokio-postgres`.
//!
//! Both are behind one [`Database`] type with the same three operations
//! (`execute`, `query`, `dialect`), so the server's database ops don't
//! care which one `DATABASE_URL` selected. Statements arrive already
//! rendered for the right dialect ([`crate::sql::render`]) with their
//! parameters as [`Value`]s.
//!
//! **v0 limits** (`docs/compiler/db.md`, "Drivers"): one connection per
//! [`Database`] (the pool, and binding a transaction to one pooled
//! connection, come with the server's database ops); Postgres without TLS.

mod pgcodec;
mod postgres;
mod sqlite;

use crate::error::DbError;
use crate::sql::Dialect;
use crate::value::{Row, Value};

/// How to connect, besides the URL.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DatabaseOptions {
    /// `DATABASE_PASSWORD`: overrides any password in the URL, so the
    /// password can come from a secret file (`docs/design/25-config-and-secrets.md`).
    pub password: Option<String>,
}

/// A connected database.
pub enum Database {
    /// SQLite, in memory or in a file.
    Sqlite(sqlite::SqliteDatabase),
    /// A Postgres server, over one connection.
    Postgres(postgres::PostgresDatabase),
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print connection details: they may contain a password.
        write!(f, "Database({:?})", self.dialect())
    }
}

impl Database {
    /// Connects to `url`:
    ///
    /// * `sqlite::memory:` — a private in-memory database;
    /// * `sqlite:<path>` — a file, created if missing (relative paths are
    ///   relative to the working directory);
    /// * `postgres://…` or `postgresql://…` — a Postgres server.
    ///
    /// SQLite connections enable foreign keys and case-sensitive `LIKE`, so
    /// both dialects compare strings the same way.
    ///
    /// # Errors
    ///
    /// [`DbError::InvalidUrl`] for an unknown scheme or malformed URL (the
    /// message never repeats the URL, which may hold a password), and
    /// [`DbError::Connection`] when connecting fails.
    pub async fn connect(url: &str, options: &DatabaseOptions) -> Result<Database, DbError> {
        if let Some(path) = url.strip_prefix("sqlite:") {
            return sqlite::SqliteDatabase::open(path)
                .await
                .map(Database::Sqlite);
        }
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            return postgres::PostgresDatabase::connect(url, options)
                .await
                .map(Database::Postgres);
        }
        Err(DbError::InvalidUrl(
            "expected `sqlite:<path>`, `sqlite::memory:` or `postgres://…`".to_owned(),
        ))
    }

    /// The SQL dialect of this database.
    #[must_use]
    pub fn dialect(&self) -> Dialect {
        match self {
            Database::Sqlite(_) => Dialect::Sqlite,
            Database::Postgres(_) => Dialect::Postgres,
        }
    }

    /// Runs a statement that returns no rows and returns the number of rows
    /// it changed.
    ///
    /// # Errors
    ///
    /// [`DbError::UniqueViolation`] for a duplicate in a unique index,
    /// [`DbError::Statement`] or [`DbError::Connection`] otherwise.
    pub async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64, DbError> {
        match self {
            Database::Sqlite(db) => db.execute(sql, params).await,
            Database::Postgres(db) => db.execute(sql, params).await,
        }
    }

    /// Runs a query and returns its rows.
    ///
    /// # Errors
    ///
    /// As [`Database::execute`], plus [`DbError::UnsupportedType`] for a
    /// column of a type the driver doesn't convert.
    pub async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DbError> {
        match self {
            Database::Sqlite(db) => db.query(sql, params).await,
            Database::Postgres(db) => db.query(sql, params).await,
        }
    }

    /// Runs `SELECT 1` and returns `true` if it answered within `timeout`
    /// (`database.ping(timeout:)`). Never fails.
    pub async fn ping(&self, timeout: std::time::Duration) -> bool {
        matches!(
            tokio::time::timeout(timeout, self.query("SELECT 1", &[])).await,
            Ok(Ok(_))
        )
    }
}
