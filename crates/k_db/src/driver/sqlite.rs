//! The SQLite driver.

use std::sync::{Arc, Mutex};

use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::{Connection, ErrorCode, ToSql};

use crate::error::DbError;
use crate::value::{Row, Value};

/// SQLite's extended result code for a unique index violation.
const CONSTRAINT_UNIQUE: i32 = 2067;
/// SQLite's extended result code for a primary key violation.
const CONSTRAINT_PRIMARYKEY: i32 = 1555;

/// A SQLite connection, used from the blocking thread pool.
pub struct SqliteDatabase {
    /// The one connection, shared with the blocking tasks that use it; the
    /// mutex serializes statements, as SQLite connections require.
    connection: Arc<Mutex<Connection>>,
}

impl ToSql for Value {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Value::Null => ToSqlOutput::Borrowed(ValueRef::Null),
            Value::Bool(b) => ToSqlOutput::Owned(rusqlite::types::Value::Integer(i64::from(*b))),
            Value::Int(i) => ToSqlOutput::Borrowed(ValueRef::Integer(*i)),
            Value::Double(d) => ToSqlOutput::Borrowed(ValueRef::Real(*d)),
            Value::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            Value::Bytes(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        })
    }
}

/// Maps a driver error to a [`DbError`]: a unique or primary key violation
/// becomes [`DbError::UniqueViolation`] with the table and columns parsed
/// from SQLite's message (SQLite reports no constraint name), anything else
/// [`DbError::Statement`].
fn map_error(error: rusqlite::Error) -> DbError {
    if let rusqlite::Error::SqliteFailure(e, message) = &error {
        if e.code == ErrorCode::ConstraintViolation
            && (e.extended_code == CONSTRAINT_UNIQUE || e.extended_code == CONSTRAINT_PRIMARYKEY)
        {
            // "UNIQUE constraint failed: user.email[, user.other]"
            let mut table = None;
            let mut columns = Vec::new();
            if let Some(list) = message.as_deref().and_then(|m| m.split(": ").nth(1)) {
                for qualified in list.split(", ") {
                    if let Some((t, c)) = qualified.split_once('.') {
                        table = Some(t.to_owned());
                        columns.push(c.to_owned());
                    }
                }
            }
            return DbError::UniqueViolation {
                table,
                columns,
                constraint: None,
            };
        }
    }
    DbError::Statement(error.to_string())
}

/// Maps a blocking task that panicked or was cancelled to
/// [`DbError::Connection`].
fn join_error(error: tokio::task::JoinError) -> DbError {
    DbError::Connection(format!("the SQLite worker failed: {error}"))
}

/// Prepares a fresh connection: foreign keys enforced, case-sensitive
/// `LIKE` (as on Postgres), double-quoted text always an identifier (SQLite
/// otherwise reads an unknown `"name"` as a string literal, so a wrong
/// column name would silently compare against its own spelling), and the
/// `k_decimal` collation for decimal columns ([`crate::decimal`]).
///
/// # Errors
///
/// SQLite refused one of the settings, mapped by [`map_error`].
fn configure(c: &Connection) -> Result<(), DbError> {
    use rusqlite::config::DbConfig;
    c.execute_batch("PRAGMA foreign_keys = ON; PRAGMA case_sensitive_like = ON;")
        .map_err(map_error)?;
    c.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)
        .map_err(map_error)?;
    c.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DDL, false)
        .map_err(map_error)?;
    c.create_collation(crate::decimal::COLLATION, crate::decimal::compare)
        .map_err(map_error)?;
    Ok(())
}

impl SqliteDatabase {
    /// Opens (creating if needed) the database file at `path`, or a private
    /// in-memory database for `:memory:`, and [`configure`]s it. The open runs
    /// on the blocking thread pool.
    ///
    /// # Errors
    ///
    /// [`DbError::Connection`] when the file can't be opened, or the failure
    /// of a configuration step.
    pub(super) async fn open(path: &str) -> Result<SqliteDatabase, DbError> {
        let path = path.to_owned();
        let connection = tokio::task::spawn_blocking(move || {
            let c = if path == ":memory:" {
                Connection::open_in_memory()
            } else {
                Connection::open(&path)
            }
            .map_err(|e| DbError::Connection(e.to_string()))?;
            configure(&c)?;
            Ok::<_, DbError>(c)
        })
        .await
        .map_err(join_error)??;
        Ok(SqliteDatabase {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Runs one statement (prepared through the connection's statement cache)
    /// on the blocking thread pool and returns the number of rows it changed.
    ///
    /// # Errors
    ///
    /// The statement's failure, mapped by [`map_error`], or
    /// [`DbError::Connection`] when a previous statement panicked while holding
    /// the connection.
    pub(super) async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64, DbError> {
        let connection = Arc::clone(&self.connection);
        let sql = sql.to_owned();
        let params = params.to_vec();
        tokio::task::spawn_blocking(move || {
            let c = connection
                .lock()
                .map_err(|_| DbError::Connection("the connection is poisoned".to_owned()))?;
            let mut statement = c.prepare_cached(&sql).map_err(map_error)?;
            let n = statement
                .execute(rusqlite::params_from_iter(params.iter()))
                .map_err(map_error)?;
            Ok(u64::try_from(n).unwrap_or(u64::MAX))
        })
        .await
        .map_err(join_error)?
    }

    /// Runs one query on the blocking thread pool and returns its rows, each
    /// with the column names in result order. Text that isn't valid UTF-8 is
    /// read lossily.
    ///
    /// # Errors
    ///
    /// As [`SqliteDatabase::execute`].
    pub(super) async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DbError> {
        let connection = Arc::clone(&self.connection);
        let sql = sql.to_owned();
        let params = params.to_vec();
        tokio::task::spawn_blocking(move || {
            let c = connection
                .lock()
                .map_err(|_| DbError::Connection("the connection is poisoned".to_owned()))?;
            let mut statement = c.prepare_cached(&sql).map_err(map_error)?;
            let columns: Vec<String> = statement.column_names().map(str::to_owned).collect();
            let mut rows = statement
                .query(rusqlite::params_from_iter(params.iter()))
                .map_err(map_error)?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(map_error)? {
                let mut values = Vec::with_capacity(columns.len());
                for i in 0..columns.len() {
                    values.push(match row.get_ref(i).map_err(map_error)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => Value::Int(n),
                        ValueRef::Real(d) => Value::Double(d),
                        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
                        ValueRef::Blob(b) => Value::Bytes(b.to_vec()),
                    });
                }
                out.push(Row {
                    columns: columns.clone(),
                    values,
                });
            }
            Ok(out)
        })
        .await
        .map_err(join_error)?
    }
}
