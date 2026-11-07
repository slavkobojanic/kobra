//! Database errors.

use std::fmt;

/// Why a database operation failed.
///
/// Only [`DbError::UniqueViolation`] is a domain outcome that K code sees
/// (as the entity's generated `UniqueViolation` enum, design 19); every
/// other variant is an infrastructure failure, which the server runtime
/// turns into a panic (`docs/design/09-errors.md`, "Panics").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DbError {
    /// A `DATABASE_URL` that is neither `sqlite:…` nor `postgres://…`, or
    /// is malformed. The message never contains a password.
    InvalidUrl(String),
    /// Connecting failed or the connection was lost.
    Connection(String),
    /// An insert or update hit a unique index. `constraint` is the index
    /// name when the database reports it (Postgres); `columns` are the
    /// columns when it reports them (SQLite). [`crate::schema::Schema::unique_field`]
    /// maps either to the entity field.
    UniqueViolation {
        /// The table, when known.
        table: Option<String>,
        /// The columns of the violated index, when known.
        columns: Vec<String>,
        /// The index or constraint name, when known.
        constraint: Option<String>,
    },
    /// Any other failure the database reported for a statement.
    Statement(String),
    /// A value of a type the driver can't convert.
    UnsupportedType(String),
    /// A migration failed; `name` is the migration.
    Migration {
        /// The migration's name, `0003_add_tags`.
        name: String,
        /// What failed.
        message: String,
    },
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::InvalidUrl(m) => write!(f, "invalid database URL: {m}"),
            DbError::Connection(m) => write!(f, "database connection failed: {m}"),
            DbError::UniqueViolation {
                table,
                columns,
                constraint,
            } => {
                write!(f, "unique violation")?;
                if let Some(t) = table {
                    write!(f, " on {t}")?;
                }
                if !columns.is_empty() {
                    write!(f, " ({})", columns.join(", "))?;
                }
                if let Some(c) = constraint {
                    write!(f, " [{c}]")?;
                }
                Ok(())
            }
            DbError::Statement(m) => write!(f, "statement failed: {m}"),
            DbError::UnsupportedType(m) => write!(f, "unsupported value type: {m}"),
            DbError::Migration { name, message } => write!(f, "migration {name} failed: {message}"),
        }
    }
}

impl std::error::Error for DbError {}
