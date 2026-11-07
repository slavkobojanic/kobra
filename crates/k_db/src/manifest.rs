//! The database half of a build: every statement the app can run, its
//! migrations and its unique indexes (`docs/compiler/db.md` §6).
//!
//! `k_codegen` renders each repository statement for both dialects at
//! build time and numbers them; the generated JavaScript refers to a
//! statement only by its number. `k_server` loads the table for the dialect
//! of `DATABASE_URL` at startup, so SQL text never comes from JavaScript.

use serde::{Deserialize, Serialize};

use crate::error::DbError;
use crate::migrate::Migration;
use crate::sql::Dialect;

/// The SQL of one statement in both dialects. Its position in
/// [`DbManifest::statements`] is its id.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct StatementSql {
    /// The SQLite text.
    pub sqlite: String,
    /// The Postgres text.
    pub postgres: String,
}

impl StatementSql {
    /// The text for `dialect`.
    #[must_use]
    pub fn text(&self, dialect: Dialect) -> &str {
        match dialect {
            Dialect::Sqlite => &self.sqlite,
            Dialect::Postgres => &self.postgres,
        }
    }
}

/// A `@Unique` field's index, for mapping a unique violation to the case
/// of the entity's generated `UniqueViolation` enum.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct UniqueIndex {
    /// The table the index is on.
    pub table: String,
    /// The stored columns, in index order (two for money).
    pub columns: Vec<String>,
    /// The index name, `<table>_<column>_key`.
    pub index: String,
    /// The entity field, which names the `UniqueViolation` case.
    pub field: String,
}

/// Everything the server needs to run an app's database code.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct DbManifest {
    /// The statements, by id.
    pub statements: Vec<StatementSql>,
    /// The migrations, in application order.
    pub migrations: Vec<Migration>,
    /// The unique indexes of `@Unique` fields.
    pub unique: Vec<UniqueIndex>,
}

impl DbManifest {
    /// Maps a [`DbError::UniqueViolation`] to the field whose `@Unique`
    /// index it hit: by constraint name when the database reports one
    /// (Postgres), otherwise by table and columns (SQLite). `None` for any
    /// other error, a primary key or an index that isn't a `@Unique`
    /// field's.
    #[must_use]
    pub fn unique_field(&self, error: &DbError) -> Option<&str> {
        let DbError::UniqueViolation {
            table,
            columns,
            constraint,
        } = error
        else {
            return None;
        };
        self.unique
            .iter()
            .find(|u| match constraint {
                Some(c) => *c == u.index,
                None => table.as_deref() == Some(u.table.as_str()) && *columns == u.columns,
            })
            .map(|u| u.field.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> DbManifest {
        DbManifest {
            unique: vec![
                UniqueIndex {
                    table: "account".into(),
                    columns: vec!["email".into()],
                    index: "account_email_key".into(),
                    field: "email".into(),
                },
                UniqueIndex {
                    table: "plan".into(),
                    columns: vec!["price_amount".into(), "price_currency".into()],
                    index: "plan_price_key".into(),
                    field: "price".into(),
                },
            ],
            ..DbManifest::default()
        }
    }

    fn violation(table: Option<&str>, columns: &[&str], constraint: Option<&str>) -> DbError {
        DbError::UniqueViolation {
            table: table.map(str::to_owned),
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
            constraint: constraint.map(str::to_owned),
        }
    }

    #[test]
    fn maps_by_constraint_name_or_by_table_and_columns() {
        let m = manifest();
        assert_eq!(
            m.unique_field(&violation(None, &[], Some("account_email_key"))),
            Some("email")
        );
        assert_eq!(
            m.unique_field(&violation(Some("account"), &["email"], None)),
            Some("email")
        );
        assert_eq!(
            m.unique_field(&violation(
                Some("plan"),
                &["price_amount", "price_currency"],
                None
            )),
            Some("price")
        );
    }

    #[test]
    fn other_indexes_and_errors_map_to_nothing() {
        let m = manifest();
        assert_eq!(
            m.unique_field(&violation(Some("account"), &["id"], None)),
            None
        );
        assert_eq!(
            m.unique_field(&violation(None, &[], Some("account_pkey"))),
            None
        );
        assert_eq!(
            m.unique_field(&violation(Some("other"), &["email"], None)),
            None
        );
        assert_eq!(m.unique_field(&DbError::Statement("x".into())), None);
    }

    #[test]
    fn round_trips_as_json() {
        let mut m = manifest();
        m.statements.push(StatementSql {
            sqlite: "SELECT 1".into(),
            postgres: "SELECT 1".into(),
        });
        m.migrations.push(Migration {
            name: "0001_init".into(),
            sqlite: vec!["CREATE TABLE \"t\" (\"id\" TEXT)".into()],
            postgres: vec!["CREATE TABLE \"t\" (\"id\" uuid)".into()],
        });
        let json = serde_json::to_string(&m).expect("serializes");
        assert_eq!(
            serde_json::from_str::<DbManifest>(&json).expect("parses"),
            m
        );
    }
}
