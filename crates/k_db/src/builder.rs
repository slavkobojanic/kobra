//! The `Schema` builder of migrations, replayed into SQL for both dialects
//! (`docs/compiler/db.md` §10, `docs/design/19-persistence.md`, "The
//! `Schema` builder").
//!
//! `k_codegen` reads the builder calls of each `migrations/NNNN_*.k` file
//! into [`SchemaOp`]s; a [`Replay`] applies them in order to its model of
//! the schema, so every name a migration uses is checked against what the
//! earlier ones created, and renders each migration's statements. The
//! model also expands every operation on a `.money` column to its two
//! stored halves, so a migration never names them.
//!
//! v0 renders `createTable`, `alterTable` with `addColumn`, `renameColumn`
//! and `dropColumn`, `renameTable`, `dropTable`, `createIndex`, `insert`
//! and `execute`. The enum case operations (`addEnumCase`,
//! `renameEnumCase`, `removeEnumCase`) rewrite a `CHECK` constraint, which
//! SQLite can only do by rebuilding the table; they are refused with an
//! error until that is implemented.

use std::collections::BTreeMap;

use crate::schema::{Column, ColumnType, Table};
use crate::sql::{Dialect, quote_identifier};

/// A `ColumnValue`: a default or a row value of `schema.insert`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Literal {
    /// `.null`.
    Null,
    /// `.text("…")`.
    Text(String),
    /// `.integer(n)`.
    Integer(i64),
    /// `.boolean(b)`.
    Boolean(bool),
    /// `.decimal(…)`, as decimal text.
    Decimal(String),
    /// `.uuid(…)`, as UUID text.
    Uuid(String),
}

/// One column as a migration declares it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ColumnDef {
    /// The name (for money, the logical name).
    pub name: String,
    /// The type.
    pub ty: ColumnType,
    /// `.primaryKey(true)`.
    pub primary_key: bool,
    /// `.unique(true)`.
    pub unique: bool,
    /// `.nullable(true)`.
    pub nullable: bool,
    /// `.indexed(true)`.
    pub indexed: bool,
    /// `.references(table:)`.
    pub references: Option<String>,
    /// `.defaultValue(…)`.
    pub default: Option<Literal>,
}

impl ColumnDef {
    /// A plain `NOT NULL` column of `ty`.
    #[must_use]
    pub fn new(name: impl Into<String>, ty: ColumnType) -> ColumnDef {
        ColumnDef {
            name: name.into(),
            ty,
            primary_key: false,
            unique: false,
            nullable: false,
            indexed: false,
            references: None,
            default: None,
        }
    }

    fn column(&self) -> Column {
        Column {
            field: None,
            name: self.name.clone(),
            ty: self.ty.clone(),
            primary_key: self.primary_key,
            nullable: self.nullable,
            unique: self.unique,
            indexed: self.indexed,
            references: self.references.clone(),
        }
    }
}

/// An `alterTable` step.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AlterOp {
    /// `addColumn(name:type:)` with its options.
    AddColumn(ColumnDef),
    /// `renameColumn(from:to:)`.
    RenameColumn {
        /// The current name.
        from: String,
        /// The new name.
        to: String,
    },
    /// `dropColumn(name:)`.
    DropColumn {
        /// The column.
        name: String,
    },
    /// `addEnumCase`, `renameEnumCase`, `removeEnumCase`: refused in v0.
    EnumCase {
        /// The builder method, for the message.
        method: String,
        /// The column.
        column: String,
    },
}

/// One builder call of a migration's `up(schema:)`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SchemaOp {
    /// `createTable(name:) { … }`.
    CreateTable {
        /// The table.
        name: String,
        /// Its columns, in order.
        columns: Vec<ColumnDef>,
    },
    /// `alterTable(name:) { … }`.
    AlterTable {
        /// The table.
        name: String,
        /// The steps, in order.
        ops: Vec<AlterOp>,
    },
    /// `renameTable(from:to:)`.
    RenameTable {
        /// The current name.
        from: String,
        /// The new name.
        to: String,
    },
    /// `dropTable(name:)`.
    DropTable {
        /// The table.
        name: String,
    },
    /// `createIndex(table:columns:)`.
    CreateIndex {
        /// The table.
        table: String,
        /// The (logical) columns, in index order.
        columns: Vec<String>,
    },
    /// `insert(into:rows:)`.
    Insert {
        /// The table.
        table: String,
        /// The rows, each its column values.
        rows: Vec<Vec<(String, Literal)>>,
    },
    /// `execute(postgres:sqlite:)`.
    Execute {
        /// The Postgres SQL.
        postgres: String,
        /// The SQLite SQL.
        sqlite: String,
    },
}

/// The statements of one migration in both dialects.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Rendered {
    /// For SQLite.
    pub sqlite: Vec<String>,
    /// For Postgres.
    pub postgres: Vec<String>,
}

impl Rendered {
    fn push(&mut self, f: impl Fn(Dialect) -> Vec<String>) {
        self.sqlite.extend(f(Dialect::Sqlite));
        self.postgres.extend(f(Dialect::Postgres));
    }
}

/// The schema as the migrations so far left it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Replay {
    /// The tables by name, with their columns in order.
    pub tables: BTreeMap<String, Vec<ColumnDef>>,
}

/// A literal as SQL text for a column of `ty`.
fn literal(l: &Literal, ty: &ColumnType, dialect: Dialect) -> Result<String, String> {
    let quoted = |s: &str| format!("'{}'", s.replace('\'', "''"));
    Ok(match (l, ty) {
        (Literal::Null, _) => "NULL".to_owned(),
        (_, ColumnType::Money) => {
            return Err("a money column has no single default or value in v0".to_owned());
        }
        (Literal::Boolean(b), ColumnType::Boolean) => match (dialect, b) {
            (Dialect::Sqlite, true) => "1".to_owned(),
            (Dialect::Sqlite, false) => "0".to_owned(),
            (Dialect::Postgres, true) => "TRUE".to_owned(),
            (Dialect::Postgres, false) => "FALSE".to_owned(),
        },
        (Literal::Integer(n), ColumnType::Integer) => n.to_string(),
        (Literal::Decimal(d), ColumnType::Decimal) => {
            let digits = d.strip_prefix('-').unwrap_or(d);
            let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
            let ok = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
            if (int.is_empty() && frac.is_empty()) || !ok(int) || !ok(frac) {
                return Err(format!("`{d}` is not a decimal"));
            }
            match dialect {
                Dialect::Sqlite => quoted(d),
                Dialect::Postgres => d.clone(),
            }
        }
        (Literal::Uuid(u), ColumnType::Uuid) => {
            let hex: String = u.chars().filter(|c| *c != '-').collect();
            if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("`{u}` is not a UUID"));
            }
            quoted(&u.to_ascii_lowercase())
        }
        (Literal::Text(t), ColumnType::Text | ColumnType::Json) => quoted(t),
        (Literal::Text(t), ColumnType::Enumeration(cases)) => {
            if !cases.contains(t) {
                return Err(format!(
                    "`{t}` is not one of the column's cases ({})",
                    cases.join(", ")
                ));
            }
            quoted(t)
        }
        (l, ty) => {
            return Err(format!(
                "a {} value doesn't fit a `{}` column",
                match l {
                    Literal::Null => "null",
                    Literal::Text(_) => "text",
                    Literal::Integer(_) => "integer",
                    Literal::Boolean(_) => "boolean",
                    Literal::Decimal(_) => "decimal",
                    Literal::Uuid(_) => "uuid",
                },
                ty.lock_name()
            ));
        }
    })
}

/// The stored column names of logical column `c`.
fn stored(c: &ColumnDef) -> Vec<String> {
    c.column().stored_names()
}

impl Replay {
    /// An empty schema.
    #[must_use]
    pub fn new() -> Replay {
        Replay::default()
    }

    fn table(&self, name: &str) -> Result<&Vec<ColumnDef>, String> {
        self.tables
            .get(name)
            .ok_or_else(|| format!("there is no table `{name}` at this point of the migrations"))
    }

    fn column<'t>(
        columns: &'t [ColumnDef],
        table: &str,
        name: &str,
    ) -> Result<&'t ColumnDef, String> {
        columns.iter().find(|c| c.name == name).ok_or_else(|| {
            format!("table `{table}` has no column `{name}` at this point of the migrations")
        })
    }

    fn check_new_column(
        &self,
        table: &str,
        columns: &[ColumnDef],
        c: &ColumnDef,
    ) -> Result<(), String> {
        if c.name.is_empty() {
            return Err(format!("a column of `{table}` has an empty name"));
        }
        if columns.iter().any(|x| x.name == c.name) {
            return Err(format!("table `{table}` already has a column `{}`", c.name));
        }
        if let Some(r) = &c.references {
            if r != table && !self.tables.contains_key(r) {
                return Err(format!(
                    "`{}` references `{r}`, which doesn't exist",
                    c.name
                ));
            }
        }
        if let Some(d) = &c.default {
            literal(d, &c.ty, Dialect::Sqlite)?;
        }
        Ok(())
    }

    /// Applies one migration's operations and renders its statements.
    ///
    /// # Errors
    ///
    /// A message naming the first operation that doesn't fit the schema
    /// (an unknown or duplicate table or column, a value of the wrong type,
    /// a non-null column added without a default, an enum case operation).
    /// The replay keeps the operations before it.
    pub fn apply(&mut self, ops: &[SchemaOp]) -> Result<Rendered, String> {
        let mut out = Rendered::default();
        for op in ops {
            self.apply_one(op, &mut out)?;
        }
        Ok(out)
    }

    #[allow(clippy::too_many_lines)]
    fn apply_one(&mut self, op: &SchemaOp, out: &mut Rendered) -> Result<(), String> {
        match op {
            SchemaOp::CreateTable { name, columns } => {
                if self.tables.contains_key(name) {
                    return Err(format!("table `{name}` already exists"));
                }
                if name.starts_with("k_") {
                    return Err(format!(
                        "table `{name}`: the `k_` prefix is reserved for the standard library"
                    ));
                }
                let mut seen: Vec<ColumnDef> = Vec::new();
                for c in columns {
                    self.check_new_column(name, &seen, c)?;
                    seen.push(c.clone());
                }
                let table = Table {
                    entity: String::new(),
                    name: name.clone(),
                    columns: columns.iter().map(ColumnDef::column).collect(),
                };
                let mut defaults = Vec::new();
                for c in columns {
                    defaults.push(c.default.clone());
                }
                out.push(|d| {
                    let mut defs = Vec::new();
                    for (c, default) in table.columns.iter().zip(&defaults) {
                        let lit = default.as_ref().and_then(|l| literal(l, &c.ty, d).ok());
                        defs.extend(c.definitions(lit.as_deref(), d));
                    }
                    let mut s = vec![format!(
                        "CREATE TABLE {} ({})",
                        quote_identifier(name),
                        defs.join(", ")
                    )];
                    s.extend(table.columns.iter().filter_map(|c| table.index_sql(c)));
                    s
                });
                self.tables.insert(name.clone(), columns.clone());
            }
            SchemaOp::AlterTable { name, ops } => {
                for a in ops {
                    let columns = self.table(name)?.clone();
                    match a {
                        AlterOp::AddColumn(c) => {
                            self.check_new_column(name, &columns, c)?;
                            if !c.nullable && c.default.is_none() {
                                return Err(format!(
                                    "`{name}.{}` is added to an existing table, so it must be nullable or have a default",
                                    c.name
                                ));
                            }
                            if c.primary_key || c.unique {
                                return Err(format!(
                                    "`{name}.{}`: a column added to an existing table can't be a primary key or unique; add it, then `createIndex`",
                                    c.name
                                ));
                            }
                            let column = c.column();
                            let table = Table {
                                entity: String::new(),
                                name: name.clone(),
                                columns: vec![column.clone()],
                            };
                            let default = c.default.clone();
                            out.push(|d| {
                                let lit = default
                                    .as_ref()
                                    .and_then(|l| literal(l, &column.ty, d).ok());
                                let mut s: Vec<String> = column
                                    .definitions(lit.as_deref(), d)
                                    .into_iter()
                                    .map(|def| {
                                        format!(
                                            "ALTER TABLE {} ADD COLUMN {def}",
                                            quote_identifier(name)
                                        )
                                    })
                                    .collect();
                                s.extend(table.index_sql(&column));
                                s
                            });
                            if let Some(t) = self.tables.get_mut(name) {
                                t.push(c.clone());
                            }
                        }
                        AlterOp::RenameColumn { from, to } => {
                            let c = Self::column(&columns, name, from)?.clone();
                            if columns.iter().any(|x| &x.name == to) {
                                return Err(format!("table `{name}` already has a column `{to}`"));
                            }
                            let mut renamed = c.clone();
                            renamed.name.clone_from(to);
                            let pairs: Vec<(String, String)> =
                                stored(&c).into_iter().zip(stored(&renamed)).collect();
                            out.push(|_| {
                                pairs
                                    .iter()
                                    .map(|(a, b)| {
                                        format!(
                                            "ALTER TABLE {} RENAME COLUMN {} TO {}",
                                            quote_identifier(name),
                                            quote_identifier(a),
                                            quote_identifier(b)
                                        )
                                    })
                                    .collect()
                            });
                            if let Some(t) = self.tables.get_mut(name) {
                                for x in t.iter_mut().filter(|x| &x.name == from) {
                                    x.name.clone_from(to);
                                }
                            }
                        }
                        AlterOp::DropColumn { name: column } => {
                            let c = Self::column(&columns, name, column)?.clone();
                            if c.primary_key {
                                return Err(format!("`{name}.{column}` is the primary key"));
                            }
                            let halves = stored(&c);
                            // SQLite refuses to drop an indexed column, so its
                            // index goes first (Postgres would drop it anyway).
                            let indexes = [
                                format!("{name}_{column}_key"),
                                format!("{name}_{column}_idx"),
                            ];
                            out.push(|_| {
                                let mut s: Vec<String> = indexes
                                    .iter()
                                    .map(|i| {
                                        format!("DROP INDEX IF EXISTS {}", quote_identifier(i))
                                    })
                                    .collect();
                                s.extend(halves.iter().map(|h| {
                                    format!(
                                        "ALTER TABLE {} DROP COLUMN {}",
                                        quote_identifier(name),
                                        quote_identifier(h)
                                    )
                                }));
                                s
                            });
                            if let Some(t) = self.tables.get_mut(name) {
                                t.retain(|x| &x.name != column);
                            }
                        }
                        AlterOp::EnumCase { method, column } => {
                            return Err(format!(
                                "`{method}` on `{name}.{column}` isn't supported by v0 (it needs a table rebuild on SQLite); use `execute(postgres:sqlite:)`"
                            ));
                        }
                    }
                }
            }
            SchemaOp::RenameTable { from, to } => {
                let columns = self.table(from)?.clone();
                if self.tables.contains_key(to) {
                    return Err(format!("table `{to}` already exists"));
                }
                out.push(|_| {
                    vec![format!(
                        "ALTER TABLE {} RENAME TO {}",
                        quote_identifier(from),
                        quote_identifier(to)
                    )]
                });
                self.tables.remove(from);
                self.tables.insert(to.clone(), columns);
                for cols in self.tables.values_mut() {
                    for c in cols.iter_mut() {
                        if c.references.as_deref() == Some(from.as_str()) {
                            c.references = Some(to.clone());
                        }
                    }
                }
            }
            SchemaOp::DropTable { name } => {
                self.table(name)?;
                if let Some((other, _)) = self.tables.iter().find(|(t, cols)| {
                    *t != name
                        && cols
                            .iter()
                            .any(|c| c.references.as_deref() == Some(name.as_str()))
                }) {
                    return Err(format!("table `{name}` is still referenced by `{other}`"));
                }
                out.push(|_| vec![format!("DROP TABLE {}", quote_identifier(name))]);
                self.tables.remove(name);
            }
            SchemaOp::CreateIndex { table, columns } => {
                let cols = self.table(table)?.clone();
                if columns.is_empty() {
                    return Err(format!("an index on `{table}` needs at least one column"));
                }
                let mut stored_names = Vec::new();
                for c in columns {
                    stored_names.extend(stored(Self::column(&cols, table, c)?));
                }
                let index = format!("{table}_{}_idx", columns.join("_"));
                out.push(|_| {
                    vec![format!(
                        "CREATE INDEX {} ON {} ({})",
                        quote_identifier(&index),
                        quote_identifier(table),
                        stored_names
                            .iter()
                            .map(|n| quote_identifier(n))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )]
                });
            }
            SchemaOp::Insert { table, rows } => {
                let cols = self.table(table)?.clone();
                let mut typed: Vec<Vec<(String, Literal, ColumnType)>> = Vec::new();
                for row in rows {
                    let mut r = Vec::new();
                    for (name, value) in row {
                        let c = Self::column(&cols, table, name)?;
                        literal(value, &c.ty, Dialect::Sqlite)
                            .map_err(|e| format!("`{table}.{name}`: {e}"))?;
                        r.push((name.clone(), value.clone(), c.ty.clone()));
                    }
                    for c in cols.iter().filter(|c| !c.nullable && c.default.is_none()) {
                        if !row.iter().any(|(n, _)| n == &c.name) {
                            return Err(format!(
                                "a row inserted into `{table}` needs a value for `{}`",
                                c.name
                            ));
                        }
                    }
                    typed.push(r);
                }
                out.push(|d| {
                    typed
                        .iter()
                        .filter(|r| !r.is_empty())
                        .map(|r| {
                            let names: Vec<String> =
                                r.iter().map(|(n, _, _)| quote_identifier(n)).collect();
                            let values: Vec<String> = r
                                .iter()
                                .map(|(_, v, ty)| {
                                    literal(v, ty, d).unwrap_or_else(|_| "NULL".to_owned())
                                })
                                .collect();
                            format!(
                                "INSERT INTO {} ({}) VALUES ({})",
                                quote_identifier(table),
                                names.join(", "),
                                values.join(", ")
                            )
                        })
                        .collect()
                });
            }
            SchemaOp::Execute { postgres, sqlite } => {
                if postgres.trim().is_empty() || sqlite.trim().is_empty() {
                    return Err("`execute` needs SQL for both databases".to_owned());
                }
                out.sqlite.push(sqlite.clone());
                out.postgres.push(postgres.clone());
            }
        }
        Ok(())
    }
}
