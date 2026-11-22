//! The schema model: tables, columns and indexes derived from entities
//! with their locked names (`docs/design/19-persistence.md`, "Entities and
//! tables"), and the DDL for both dialects.

use crate::error::DbError;
use crate::sql::{Dialect, quote_identifier};

/// A logical column type (the closed `ColumnType` enum of `k.db`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ColumnType {
    /// `Uuid`: `uuid` on Postgres, `TEXT` on SQLite.
    Uuid,
    /// `String`, `EmailAddress`, `Url`: `text` / `TEXT`.
    Text,
    /// `Int`: `bigint` / `INTEGER`.
    Integer,
    /// `Decimal`: `numeric(38,10)` / `TEXT` (SQLite would round it).
    Decimal,
    /// `Bool`: `boolean` / `INTEGER` 0 or 1.
    Boolean,
    /// `Date`: `timestamptz` / `TEXT` in RFC 3339 UTC.
    Timestamp,
    /// `Money`: one logical column stored as `<name>_amount`
    /// (`numeric(19,4)` / `TEXT`) and `<name>_currency` (`char(3)` /
    /// `TEXT`) (`docs/design/34-money.md`).
    Money,
    /// A `Codable` value as JSON: `jsonb` / `TEXT`.
    Json,
    /// An enum without payloads, by case name: `text` with a `CHECK` on
    /// both dialects.
    Enumeration(Vec<String>),
}

impl ColumnType {
    /// Returns the name used in `schema.lock` (`uuid`, `enumeration`, …).
    #[must_use]
    pub const fn lock_name(&self) -> &'static str {
        match self {
            ColumnType::Uuid => "uuid",
            ColumnType::Text => "text",
            ColumnType::Integer => "integer",
            ColumnType::Decimal => "decimal",
            ColumnType::Boolean => "boolean",
            ColumnType::Timestamp => "timestamp",
            ColumnType::Money => "money",
            ColumnType::Json => "json",
            ColumnType::Enumeration(_) => "enumeration",
        }
    }

    /// The column type in `dialect`'s DDL, with SQLite's `k_decimal`
    /// collation on decimals. For [`ColumnType::Money`] it is the amount's
    /// type; the DDL writes two columns.
    #[must_use]
    pub fn column_sql(&self, dialect: Dialect) -> &'static str {
        match (self, dialect) {
            (ColumnType::Decimal | ColumnType::Money, Dialect::Sqlite) => SQLITE_DECIMAL,
            (ColumnType::Money, Dialect::Postgres) => "numeric(19,4)",
            // Code-point order, as Kobra's own `sortedBy` and SQLite's BINARY:
            // without it Postgres sorts and compares by the database's locale.
            (ColumnType::Text | ColumnType::Enumeration(_), Dialect::Postgres) => {
                "text COLLATE \"C\""
            }
            _ => self.sql(dialect),
        }
    }

    /// The plain column type in `dialect`, without the decimal collation that
    /// [`ColumnType::column_sql`] adds on SQLite.
    fn sql(&self, dialect: Dialect) -> &'static str {
        match (self, dialect) {
            (ColumnType::Uuid, Dialect::Postgres) => "uuid",
            (ColumnType::Integer, Dialect::Postgres) => "bigint",
            (ColumnType::Decimal, Dialect::Postgres) => "numeric(38,10)",
            (ColumnType::Boolean, Dialect::Postgres) => "boolean",
            (ColumnType::Timestamp, Dialect::Postgres) => "timestamptz",
            (ColumnType::Json, Dialect::Postgres) => "jsonb",
            (
                ColumnType::Text | ColumnType::Enumeration(_) | ColumnType::Money,
                Dialect::Postgres,
            ) => "text",
            (ColumnType::Integer | ColumnType::Boolean, Dialect::Sqlite) => "INTEGER",
            (_, Dialect::Sqlite) => "TEXT",
        }
    }
}

/// One logical column.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Column {
    /// The entity field it stores; `None` for a retired column.
    pub field: Option<String>,
    /// The locked column name. For [`ColumnType::Money`] the stored
    /// columns are `<name>_amount` and `<name>_currency`.
    pub name: String,
    /// The type.
    pub ty: ColumnType,
    /// `@Id`.
    pub primary_key: bool,
    /// `T?` fields and retired columns.
    pub nullable: bool,
    /// `@Unique`: a unique index named `<table>_<column>_key`.
    pub unique: bool,
    /// `@Index` or a foreign key: an index named `<table>_<column>_idx`.
    pub indexed: bool,
    /// A to-one `@Relation`: the referenced table (its `id` column).
    pub references: Option<String>,
}

/// One table.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Table {
    /// The qualified entity name, `notes.Note`.
    pub entity: String,
    /// The locked table name.
    pub name: String,
    /// The columns, in field order.
    pub columns: Vec<Column>,
}

impl Table {
    /// The name of the unique index of `column`.
    #[must_use]
    pub fn unique_index_name(&self, column: &str) -> String {
        format!("{}_{column}_key", self.name)
    }

    /// The `CREATE TABLE` statement followed by one `CREATE INDEX` per
    /// unique or indexed column, for `dialect`.
    #[must_use]
    pub fn create_sql(&self, dialect: Dialect) -> Vec<String> {
        let mut defs = Vec::new();
        for c in &self.columns {
            defs.extend(c.definitions(None, dialect));
        }
        let mut out = vec![format!(
            "CREATE TABLE {} ({})",
            quote_identifier(&self.name),
            defs.join(", ")
        )];
        for c in &self.columns {
            out.extend(self.index_sql(c));
        }
        out
    }

    /// The `CREATE [UNIQUE] INDEX` statement of `column`, if it is unique,
    /// indexed or a foreign key. A money column's index covers its stored
    /// `<name>_amount` and `<name>_currency` columns.
    #[must_use]
    pub fn index_sql(&self, column: &Column) -> Option<String> {
        let (kind, name) = if column.unique {
            ("UNIQUE INDEX", self.unique_index_name(&column.name))
        } else if column.indexed || column.references.is_some() {
            ("INDEX", format!("{}_{}_idx", self.name, column.name))
        } else {
            return None;
        };
        Some(format!(
            "CREATE {kind} {} ON {} ({})",
            quote_identifier(&name),
            quote_identifier(&self.name),
            column
                .stored_names()
                .iter()
                .map(|n| quote_identifier(n))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// The SQLite type of decimal text: `TEXT` compared by value
/// ([`crate::decimal`]).
const SQLITE_DECIMAL: &str = "TEXT COLLATE k_decimal";

impl Column {
    /// The column definitions of `CREATE TABLE` or `ADD COLUMN` for this
    /// logical column: two for money (`<name>_amount`, `<name>_currency`),
    /// otherwise one with its `NOT NULL`, `DEFAULT default` (SQL literal
    /// text), `PRIMARY KEY`, `REFERENCES` and enum `CHECK`.
    #[must_use]
    pub fn definitions(&self, default: Option<&str>, dialect: Dialect) -> Vec<String> {
        let not_null = if self.nullable { "" } else { " NOT NULL" };
        let default = default.map_or_else(String::new, |d| format!(" DEFAULT {d}"));
        if self.ty == ColumnType::Money {
            let (amount, currency) = match dialect {
                Dialect::Postgres => ("numeric(19,4)", "char(3)"),
                Dialect::Sqlite => (SQLITE_DECIMAL, "TEXT"),
            };
            return vec![
                format!(
                    "{} {amount}{not_null}",
                    quote_identifier(&format!("{}_amount", self.name))
                ),
                format!(
                    "{} {currency}{not_null}",
                    quote_identifier(&format!("{}_currency", self.name))
                ),
            ];
        }
        let mut d = format!(
            "{} {}{not_null}{default}",
            quote_identifier(&self.name),
            self.ty.column_sql(dialect)
        );
        if self.primary_key {
            d.push_str(" PRIMARY KEY");
        }
        if let Some(t) = &self.references {
            d.push_str(&format!(
                " REFERENCES {} ({})",
                quote_identifier(t),
                quote_identifier("id")
            ));
        }
        if let ColumnType::Enumeration(cases) = &self.ty {
            let list: Vec<String> = cases
                .iter()
                .map(|k| format!("'{}'", k.replace('\'', "''")))
                .collect();
            d.push_str(&format!(
                " CHECK ({} IN ({}))",
                quote_identifier(&self.name),
                list.join(", ")
            ));
        }
        vec![d]
    }

    /// The physical columns this logical column is stored in, in select
    /// order: `<name>_amount` and `<name>_currency` for money, the name
    /// otherwise.
    #[must_use]
    pub fn stored_names(&self) -> Vec<String> {
        if self.ty == ColumnType::Money {
            vec![
                format!("{}_amount", self.name),
                format!("{}_currency", self.name),
            ]
        } else {
            vec![self.name.clone()]
        }
    }
}

/// The whole schema of an app.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Schema {
    /// The tables, sorted by name.
    pub tables: Vec<Table>,
}

impl Schema {
    /// Returns the table called `name`.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// Maps a [`DbError::UniqueViolation`] to the entity and field whose
    /// `@Unique` index it hit, which is the case of the entity's generated
    /// `UniqueViolation` enum. Uses the constraint name when the database
    /// reported one (Postgres), otherwise the table and single column
    /// (SQLite). Returns `None` for any other error or an unknown index.
    #[must_use]
    pub fn unique_field(&self, error: &DbError) -> Option<(&str, &str)> {
        let DbError::UniqueViolation {
            table,
            columns,
            constraint,
        } = error
        else {
            return None;
        };
        for t in &self.tables {
            for c in t.columns.iter().filter(|c| c.unique) {
                let by_constraint =
                    constraint.as_deref() == Some(t.unique_index_name(&c.name).as_str());
                let by_columns =
                    table.as_deref() == Some(t.name.as_str()) && *columns == c.stored_names();
                if by_constraint || by_columns {
                    return Some((t.entity.as_str(), c.field.as_deref()?));
                }
            }
        }
        None
    }
}
