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
//! Every builder call is rendered: `createTable`; `alterTable` with
//! `addColumn`, `renameColumn`, `dropColumn`, `retireColumn` and the enum
//! case operations `addEnumCase`, `renameEnumCase` and `removeEnumCase`;
//! `renameTable`, `dropTable`, `retireTable`, `createIndex`, `insert` and
//! `execute`. The case operations rewrite an enumeration's `CHECK`
//! constraint: Postgres swaps the constraint (found in its catalog, since
//! a constraint keeps its name across renames) around an `UPDATE` of the
//! stored values, and SQLite, which can't alter a constraint, rebuilds the
//! table with the documented create-copy-drop-rename procedure, recreating
//! every index the replay knows of. Retiring a `NOT NULL` column without a
//! default makes it nullable the same ways.
//!
//! The replay also reports each migration's **destructive** operations
//! ([`Destructive`]): drops, physical renames and the case operations that
//! rewrite values. `k_codegen` requires `@Breaking` on their migration, and
//! `@Breaking("single instance")` for a drop or rename of something no
//! earlier migration retired. [`Replay::lock_tables`] is the schema as
//! `schema.lock` records it, which `k check` compares with the lock.

use std::collections::{BTreeMap, BTreeSet};

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

    /// Converts the definition into the schema model's [`Column`], with no
    /// entity field (`field: None`): migrations know tables and columns, not
    /// the entities that map to them.
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
    /// `retireColumn(name:)`: the app no longer reads or writes the column;
    /// a later migration drops it (expand, then contract).
    RetireColumn {
        /// The column.
        name: String,
    },
    /// `addEnumCase(column:case:)`.
    AddEnumCase {
        /// The enumeration column.
        column: String,
        /// The new case, appended to the column's cases.
        case: String,
    },
    /// `renameEnumCase(column:from:to:)`: rewrites the stored values.
    RenameEnumCase {
        /// The enumeration column.
        column: String,
        /// The current case name.
        from: String,
        /// The new case name.
        to: String,
    },
    /// `removeEnumCase(column:case:replacement:)`: rewrites the rows that
    /// hold `case` to `replacement`.
    RemoveEnumCase {
        /// The enumeration column.
        column: String,
        /// The case to remove.
        case: String,
        /// The remaining case its rows (and the column's default) take.
        replacement: String,
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
    /// `retireTable(name:)`: the app no longer reads or writes the table;
    /// a later migration drops it.
    RetireTable {
        /// The table.
        name: String,
    },
    /// `createIndex(table:columns:)`.
    CreateIndex {
        /// The table to index.
        table: String,
        /// The (logical) columns, in index order.
        columns: Vec<String>,
    },
    /// `insert(into:rows:)`.
    Insert {
        /// The table to insert into.
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

/// An operation that destroys data, so its migration needs
/// `@Breaking("reason")` (`docs/design/19-persistence.md`, "The `Schema`
/// builder" and "Rolling deploys").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Destructive {
    /// The position of the [`SchemaOp`] in the migration, from 0.
    pub op: usize,
    /// For an `alterTable`, the position of the [`AlterOp`] in it.
    pub step: Option<usize>,
    /// What it does, for the diagnostic: "drops the column `note.title`".
    pub what: String,
    /// `true` when it drops or renames a table or column the previous
    /// release still reads (one that no earlier migration retired): during
    /// a rolling deploy that release would fail, so only a single-instance
    /// app may do it in one step (`@Breaking("single instance")`).
    pub live: bool,
}

/// The statements of one migration in both dialects.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Rendered {
    /// For SQLite.
    pub sqlite: Vec<String>,
    /// For Postgres.
    pub postgres: Vec<String>,
    /// Its destructive operations, in order.
    pub destructive: Vec<Destructive>,
}

impl Rendered {
    /// Appends the statements `f` renders for each dialect, SQLite's to
    /// [`Rendered::sqlite`] and Postgres' to [`Rendered::postgres`].
    fn push(&mut self, f: impl Fn(Dialect) -> Vec<String>) {
        self.sqlite.extend(f(Dialect::Sqlite));
        self.postgres.extend(f(Dialect::Postgres));
    }
}

/// Why a migration doesn't replay.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReplayError {
    /// The position of the failing [`SchemaOp`] in the migration, from 0.
    pub op: usize,
    /// For an `alterTable`, the position of the failing [`AlterOp`] in it.
    pub step: Option<usize>,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ReplayError {}

/// An index the replay created, so a table rebuilt on SQLite gets it back
/// under the same name.
#[derive(Clone, PartialEq, Eq, Debug)]
struct IndexDef {
    /// The index name, as created (renames don't change it).
    name: String,
    /// `CREATE UNIQUE INDEX`.
    unique: bool,
    /// The stored columns it covers, in order.
    columns: Vec<String>,
}

/// A table or column rename the migrations made, in order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Rename {
    /// `renameTable(from:to:)`.
    Table {
        /// The name before.
        from: String,
        /// The name after.
        to: String,
    },
    /// `renameColumn(from:to:)` on the table then called `table`.
    Column {
        /// The table's name at the time of the rename.
        table: String,
        /// The name before.
        from: String,
        /// The name after.
        to: String,
    },
}

/// The schema as the migrations so far left it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Replay {
    /// The tables by name, with their columns in order.
    pub tables: BTreeMap<String, Vec<ColumnDef>>,
    /// The renames so far, in order.
    pub renames: Vec<Rename>,
    /// The indexes of each table, in creation order.
    indexes: BTreeMap<String, Vec<IndexDef>>,
    /// The retired columns: (table, logical column).
    retired: BTreeSet<(String, String)>,
    /// The retired tables.
    retired_tables: BTreeSet<String>,
}

/// A literal as SQL text for a column of `ty`.
///
/// # Errors
///
/// A message when the literal doesn't fit `ty`: any value for a `Money`
/// column, a malformed decimal or UUID, a text that isn't one of an enum's
/// cases, or a literal of another type. `null` fits every column.
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

/// The name a rebuilt SQLite table has until it replaces the original.
/// The `k_` prefix keeps it clear of every app table.
const REBUILD_TABLE: &str = "k_rebuild";

/// A text literal in SQL: `'it''s'`.
fn sql_text(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The default of a column as `schema.lock` text: `false`, `0`,
/// `awaitingPayment`. `None` without a default or for `.null`.
fn lock_text(l: &Literal) -> Option<String> {
    match l {
        Literal::Null => None,
        Literal::Text(t) | Literal::Decimal(t) | Literal::Uuid(t) => Some(t.clone()),
        Literal::Integer(n) => Some(n.to_string()),
        Literal::Boolean(b) => Some(b.to_string()),
    }
}

/// The index a new column gets: unique for `.unique(true)`, plain for
/// `.indexed(true)` and foreign keys, as [`Table::index_sql`] creates it.
fn column_index(table: &str, c: &ColumnDef) -> Option<IndexDef> {
    let (unique, suffix) = if c.unique {
        (true, "key")
    } else if c.indexed || c.references.is_some() {
        (false, "idx")
    } else {
        return None;
    };
    Some(IndexDef {
        name: format!("{table}_{}_{suffix}", c.name),
        unique,
        columns: stored(c),
    })
}

/// `CREATE [UNIQUE] INDEX` of `index` on `table`.
fn index_statement(table: &str, index: &IndexDef) -> String {
    format!(
        "CREATE {}INDEX {} ON {} ({})",
        if index.unique { "UNIQUE " } else { "" },
        quote_identifier(&index.name),
        quote_identifier(table),
        index
            .columns
            .iter()
            .map(|n| quote_identifier(n))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Postgres: drops every single-column `CHECK` constraint of `table.column`
/// (an enumeration's case list), whatever name it was created with. A
/// constraint keeps its name across renames, so it is found in the catalog
/// rather than by name.
fn drop_check_postgres(table: &str, column: &str) -> String {
    format!(
        "DO $k$ DECLARE c record; BEGIN FOR c IN SELECT con.conname FROM pg_constraint con \
         JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = con.conkey[1] \
         WHERE con.conrelid = {}::regclass AND con.contype = 'c' AND cardinality(con.conkey) = 1 \
         AND a.attname = {} LOOP EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', {}, c.conname); \
         END LOOP; END $k$",
        sql_text(&quote_identifier(table)),
        sql_text(column),
        sql_text(table)
    )
}

/// Postgres: the named `CHECK` constraint listing `cases` for `table.column`.
fn add_check_postgres(table: &str, column: &str, cases: &[String]) -> String {
    let list: Vec<String> = cases.iter().map(|k| sql_text(k)).collect();
    format!(
        "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({} IN ({}))",
        quote_identifier(table),
        quote_identifier(&format!("{table}_{column}_check")),
        quote_identifier(column),
        list.join(", ")
    )
}

impl Replay {
    /// An empty schema.
    #[must_use]
    pub fn new() -> Replay {
        Replay::default()
    }

    /// `true` when `table.column` was retired by a migration so far.
    #[must_use]
    pub fn is_retired(&self, table: &str, column: &str) -> bool {
        self.retired
            .contains(&(table.to_owned(), column.to_owned()))
    }

    /// `true` when `table` was retired by a migration so far.
    #[must_use]
    pub fn is_retired_table(&self, table: &str) -> bool {
        self.retired_tables.contains(table)
    }

    /// The name the table called `name` has after the renames so far, or
    /// `None` if the migrations dropped it (or never created it). A table
    /// that was never renamed keeps its name.
    #[must_use]
    pub fn current_table(&self, name: &str) -> Option<String> {
        let mut current = name.to_owned();
        for r in &self.renames {
            if let Rename::Table { from, to } = r {
                if *from == current {
                    current.clone_from(to);
                }
            }
        }
        self.tables.contains_key(&current).then_some(current)
    }

    /// Where the column `table.column` is after the renames so far: its
    /// table's and its own current names, or `None` if the migrations
    /// dropped either.
    #[must_use]
    pub fn current_column(&self, table: &str, column: &str) -> Option<(String, String)> {
        let (mut t, mut c) = (table.to_owned(), column.to_owned());
        for r in &self.renames {
            match r {
                Rename::Table { from, to } if *from == t => t.clone_from(to),
                Rename::Column {
                    table: rt,
                    from,
                    to,
                } if *rt == t && *from == c => c.clone_from(to),
                _ => {}
            }
        }
        self.tables
            .get(&t)
            .is_some_and(|cols| cols.iter().any(|x| x.name == c))
            .then_some((t, c))
    }

    /// The tables as `schema.lock` records them, without the entity and
    /// field names (those come from the code, not the migrations): each
    /// column's type, cases, options and default, `indexed` for a foreign
    /// key or a single-column index, and the retired tables and columns.
    #[must_use]
    pub fn lock_tables(&self) -> BTreeMap<String, crate::lock::LockedTable> {
        let mut out = BTreeMap::new();
        for (name, columns) in &self.tables {
            let indexes = self.indexes.get(name).map_or(&[][..], Vec::as_slice);
            let locked = columns
                .iter()
                .map(|c| {
                    let single = indexes.iter().any(|i| !i.unique && i.columns == stored(c));
                    crate::lock::LockedColumn {
                        field: None,
                        name: c.name.clone(),
                        ty: c.ty.lock_name().to_owned(),
                        primary_key: c.primary_key,
                        nullable: c.nullable,
                        unique: c.unique,
                        indexed: c.indexed || c.references.is_some() || single,
                        references: c.references.as_ref().map(|t| format!("{t}.id")),
                        cases: match &c.ty {
                            ColumnType::Enumeration(cases) => cases.clone(),
                            _ => Vec::new(),
                        },
                        default: c.default.as_ref().and_then(lock_text),
                        retired: self.is_retired(name, &c.name),
                    }
                })
                .collect();
            out.insert(
                name.clone(),
                crate::lock::LockedTable {
                    entity: String::new(),
                    retired: self.is_retired_table(name),
                    columns: locked,
                },
            );
        }
        out
    }

    /// The columns of table `name` as the replay currently has them.
    ///
    /// # Errors
    ///
    /// A message saying the table doesn't exist at this point of the
    /// migrations.
    fn table(&self, name: &str) -> Result<&Vec<ColumnDef>, String> {
        self.tables
            .get(name)
            .ok_or_else(|| format!("there is no table `{name}` at this point of the migrations"))
    }

    /// Finds column `name` among `columns`, which belong to `table` (named only
    /// for the message).
    ///
    /// # Errors
    ///
    /// A message saying the table has no such column at this point of the
    /// migrations.
    fn column<'t>(
        columns: &'t [ColumnDef],
        table: &str,
        name: &str,
    ) -> Result<&'t ColumnDef, String> {
        columns.iter().find(|c| c.name == name).ok_or_else(|| {
            format!("table `{table}` has no column `{name}` at this point of the migrations")
        })
    }

    /// Checks that `c` can be added to `table`, whose columns so far are
    /// `columns`: a non-empty name not already taken, a `references` target that
    /// is `table` itself or an existing table, and a default that is a valid
    /// literal of the column's type.
    ///
    /// # Errors
    ///
    /// A message naming the first rule `c` breaks.
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
    /// The message of [`Replay::try_apply`]'s error.
    pub fn apply(&mut self, ops: &[SchemaOp]) -> Result<Rendered, String> {
        self.try_apply(ops).map_err(|e| e.message)
    }

    /// Applies one migration's operations and renders its statements,
    /// recording its destructive operations in [`Rendered::destructive`].
    ///
    /// # Errors
    ///
    /// The first operation that doesn't fit the schema (an unknown or
    /// duplicate table, column or enum case, a value of the wrong type, a
    /// non-null column added without a default, retiring a primary key or
    /// something already retired), with its position. The replay keeps the
    /// operations before it.
    pub fn try_apply(&mut self, ops: &[SchemaOp]) -> Result<Rendered, ReplayError> {
        let mut out = Rendered::default();
        for (i, op) in ops.iter().enumerate() {
            self.apply_one(i, op, &mut out)
                .map_err(|(step, message)| ReplayError {
                    op: i,
                    step,
                    message,
                })?;
        }
        Ok(out)
    }

    /// Applies one operation (the `index`th of its migration) to the
    /// replayed schema and appends its statements in both dialects to
    /// `out`.
    ///
    /// # Errors
    ///
    /// A message naming why `op` doesn't fit the schema, with the position
    /// of the failing step of an `alterTable`.
    fn apply_one(
        &mut self,
        index: usize,
        op: &SchemaOp,
        out: &mut Rendered,
    ) -> Result<(), (Option<usize>, String)> {
        if let SchemaOp::AlterTable { name, ops } = op {
            for (step, a) in ops.iter().enumerate() {
                self.alter(index, step, name, a, out)
                    .map_err(|m| (Some(step), m))?;
            }
            return Ok(());
        }
        self.apply_plain(index, op, out).map_err(|m| (None, m))
    }

    /// Applies an operation other than `alterTable`.
    ///
    /// # Errors
    ///
    /// A message naming why `op` doesn't fit the schema.
    fn apply_plain(
        &mut self,
        index: usize,
        op: &SchemaOp,
        out: &mut Rendered,
    ) -> Result<(), String> {
        match op {
            SchemaOp::CreateTable { name, columns } => self.create_table(name, columns, out),
            SchemaOp::AlterTable { .. } => Ok(()),
            SchemaOp::RenameTable { from, to } => {
                let columns = self.table(from)?.clone();
                if self.tables.contains_key(to) {
                    return Err(format!("table `{to}` already exists"));
                }
                if to.starts_with("k_") {
                    return Err(format!(
                        "table `{to}`: the `k_` prefix is reserved for the standard library"
                    ));
                }
                out.destructive.push(Destructive {
                    op: index,
                    step: None,
                    what: format!("renames the table `{from}`"),
                    live: true,
                });
                out.push(|_| {
                    vec![format!(
                        "ALTER TABLE {} RENAME TO {}",
                        quote_identifier(from),
                        quote_identifier(to)
                    )]
                });
                self.tables.remove(from);
                self.tables.insert(to.clone(), columns);
                if let Some(ix) = self.indexes.remove(from) {
                    self.indexes.insert(to.clone(), ix);
                }
                self.retired = std::mem::take(&mut self.retired)
                    .into_iter()
                    .map(|(t, c)| (if t == *from { to.clone() } else { t }, c))
                    .collect();
                if self.retired_tables.remove(from) {
                    self.retired_tables.insert(to.clone());
                }
                for cols in self.tables.values_mut() {
                    for c in cols.iter_mut() {
                        if c.references.as_deref() == Some(from.as_str()) {
                            c.references = Some(to.clone());
                        }
                    }
                }
                self.renames.push(Rename::Table {
                    from: from.clone(),
                    to: to.clone(),
                });
                Ok(())
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
                let retired = self.is_retired_table(name);
                out.destructive.push(Destructive {
                    op: index,
                    step: None,
                    what: if retired {
                        format!("drops the retired table `{name}`")
                    } else {
                        format!("drops the table `{name}`")
                    },
                    live: !retired,
                });
                out.push(|_| vec![format!("DROP TABLE {}", quote_identifier(name))]);
                self.tables.remove(name);
                self.indexes.remove(name);
                self.retired_tables.remove(name);
                self.retired.retain(|(t, _)| t != name);
                Ok(())
            }
            SchemaOp::RetireTable { name } => {
                self.table(name)?;
                if !self.retired_tables.insert(name.clone()) {
                    return Err(format!("table `{name}` is already retired"));
                }
                Ok(())
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
                let index = IndexDef {
                    name: format!("{table}_{}_idx", columns.join("_")),
                    unique: false,
                    columns: stored_names,
                };
                if self
                    .indexes
                    .get(table)
                    .is_some_and(|ix| ix.iter().any(|i| i.name == index.name))
                {
                    return Err(format!("the index `{}` already exists", index.name));
                }
                let statement = index_statement(table, &index);
                out.push(|_| vec![statement.clone()]);
                self.indexes.entry(table.clone()).or_default().push(index);
                Ok(())
            }
            SchemaOp::Insert { table, rows } => self.insert(table, rows, out),
            SchemaOp::Execute { postgres, sqlite } => {
                if postgres.trim().is_empty() || sqlite.trim().is_empty() {
                    return Err("`execute` needs SQL for both databases".to_owned());
                }
                out.sqlite.push(sqlite.clone());
                out.postgres.push(postgres.clone());
                Ok(())
            }
        }
    }

    /// `createTable`.
    ///
    /// # Errors
    ///
    /// An existing or reserved table name, or a column that breaks
    /// [`Replay::check_new_column`].
    fn create_table(
        &mut self,
        name: &str,
        columns: &[ColumnDef],
        out: &mut Rendered,
    ) -> Result<(), String> {
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
            name: name.to_owned(),
            columns: columns.iter().map(ColumnDef::column).collect(),
        };
        let defaults: Vec<Option<Literal>> = columns.iter().map(|c| c.default.clone()).collect();
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
        self.tables.insert(name.to_owned(), columns.to_vec());
        self.indexes.insert(
            name.to_owned(),
            columns
                .iter()
                .filter_map(|c| column_index(name, c))
                .collect(),
        );
        Ok(())
    }

    /// `schema.insert(into:rows:)`.
    ///
    /// # Errors
    ///
    /// An unknown table or column, a value of the wrong type, or a row
    /// without a value for a required column.
    fn insert(
        &self,
        table: &str,
        rows: &[Vec<(String, Literal)>],
        out: &mut Rendered,
    ) -> Result<(), String> {
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
                        .map(|(_, v, ty)| literal(v, ty, d).unwrap_or_else(|_| "NULL".to_owned()))
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
        Ok(())
    }

    /// One `alterTable` step on table `name`, part of the `index`th
    /// operation of the migration.
    ///
    /// # Errors
    ///
    /// A message naming why the step doesn't fit the table.
    #[allow(clippy::too_many_lines)]
    fn alter(
        &mut self,
        index: usize,
        step: usize,
        name: &str,
        a: &AlterOp,
        out: &mut Rendered,
    ) -> Result<(), String> {
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
                    name: name.to_owned(),
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
                            format!("ALTER TABLE {} ADD COLUMN {def}", quote_identifier(name))
                        })
                        .collect();
                    s.extend(table.index_sql(&column));
                    s
                });
                if let Some(t) = self.tables.get_mut(name) {
                    t.push(c.clone());
                }
                if let Some(ix) = column_index(name, c) {
                    self.indexes.entry(name.to_owned()).or_default().push(ix);
                }
            }
            AlterOp::RenameColumn { from, to } => {
                let c = Self::column(&columns, name, from)?.clone();
                if to.is_empty() {
                    return Err(format!(
                        "a column of `{name}` can't be renamed to an empty name"
                    ));
                }
                if columns.iter().any(|x| &x.name == to) {
                    return Err(format!("table `{name}` already has a column `{to}`"));
                }
                out.destructive.push(Destructive {
                    op: index,
                    step: Some(step),
                    what: format!("renames the column `{name}.{from}`"),
                    live: true,
                });
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
                if let Some(ix) = self.indexes.get_mut(name) {
                    for i in ix.iter_mut() {
                        for column in &mut i.columns {
                            if let Some((_, b)) = pairs.iter().find(|(a, _)| a == column) {
                                column.clone_from(b);
                            }
                        }
                    }
                }
                if self.retired.remove(&(name.to_owned(), from.clone())) {
                    self.retired.insert((name.to_owned(), to.clone()));
                }
                self.renames.push(Rename::Column {
                    table: name.to_owned(),
                    from: from.clone(),
                    to: to.clone(),
                });
            }
            AlterOp::DropColumn { name: column } => {
                let c = Self::column(&columns, name, column)?.clone();
                if c.primary_key {
                    return Err(format!("`{name}.{column}` is the primary key"));
                }
                let retired = self.is_retired(name, column) || self.is_retired_table(name);
                out.destructive.push(Destructive {
                    op: index,
                    step: Some(step),
                    what: if retired {
                        format!("drops the retired column `{name}.{column}`")
                    } else {
                        format!("drops the column `{name}.{column}`")
                    },
                    live: !retired,
                });
                let halves = stored(&c);
                // SQLite refuses to drop an indexed column, so its indexes
                // go first (Postgres would drop them anyway): the two the
                // column's own options create, then every other index that
                // covers it.
                let mut indexes = vec![
                    format!("{name}_{column}_key"),
                    format!("{name}_{column}_idx"),
                ];
                if let Some(ix) = self.indexes.get_mut(name) {
                    for i in ix.iter() {
                        if i.columns.iter().any(|x| halves.contains(x))
                            && !indexes.contains(&i.name)
                        {
                            indexes.push(i.name.clone());
                        }
                    }
                    ix.retain(|i| !i.columns.iter().any(|x| halves.contains(x)));
                }
                out.push(|_| {
                    let mut s: Vec<String> = indexes
                        .iter()
                        .map(|i| format!("DROP INDEX IF EXISTS {}", quote_identifier(i)))
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
                self.retired.remove(&(name.to_owned(), column.clone()));
            }
            AlterOp::RetireColumn { name: column } => {
                let c = Self::column(&columns, name, column)?.clone();
                if c.primary_key {
                    return Err(format!(
                        "`{name}.{column}` is the primary key, which can't be retired"
                    ));
                }
                if !self.retired.insert((name.to_owned(), column.clone())) {
                    return Err(format!("`{name}.{column}` is already retired"));
                }
                if !c.nullable && c.default.is_none() {
                    // The next release leaves the column out of its inserts.
                    let mut next = columns.clone();
                    for x in next.iter_mut().filter(|x| &x.name == column) {
                        x.nullable = true;
                    }
                    let halves = stored(&c);
                    out.postgres.extend(halves.iter().map(|h| {
                        format!(
                            "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL",
                            quote_identifier(name),
                            quote_identifier(h)
                        )
                    }));
                    let copy = self.copy_list(&next, &BTreeMap::new());
                    out.sqlite.extend(self.rebuild_sqlite(name, &next, &copy));
                    self.tables.insert(name.to_owned(), next);
                }
            }
            AlterOp::AddEnumCase { column, case } => {
                let cases = Self::cases_of(&columns, name, column)?;
                if case.is_empty() {
                    return Err(format!("a case of `{name}.{column}` can't be empty"));
                }
                if cases.contains(case) {
                    return Err(format!("`{name}.{column}` already has the case `{case}`"));
                }
                let mut next = cases.clone();
                next.push(case.clone());
                self.rewrite_cases(name, column, next, &[], None, out);
            }
            AlterOp::RenameEnumCase { column, from, to } => {
                let cases = Self::cases_of(&columns, name, column)?;
                if !cases.contains(from) {
                    return Err(format!("`{name}.{column}` has no case `{from}`"));
                }
                if to.is_empty() {
                    return Err(format!("a case of `{name}.{column}` can't be empty"));
                }
                if cases.contains(to) {
                    return Err(format!("`{name}.{column}` already has the case `{to}`"));
                }
                out.destructive.push(Destructive {
                    op: index,
                    step: Some(step),
                    what: format!("renames the case `{from}` of `{name}.{column}`"),
                    live: false,
                });
                let next: Vec<String> = cases
                    .iter()
                    .map(|c| if c == from { to.clone() } else { c.clone() })
                    .collect();
                self.rewrite_cases(
                    name,
                    column,
                    next,
                    &[(from.clone(), to.clone())],
                    Some((from, to)),
                    out,
                );
            }
            AlterOp::RemoveEnumCase {
                column,
                case,
                replacement,
            } => {
                let cases = Self::cases_of(&columns, name, column)?;
                if !cases.contains(case) {
                    return Err(format!("`{name}.{column}` has no case `{case}`"));
                }
                if replacement == case {
                    return Err(format!(
                        "the replacement of `{case}` in `{name}.{column}` must be another of its cases"
                    ));
                }
                if !cases.contains(replacement) {
                    return Err(format!(
                        "`{name}.{column}` has no case `{replacement}` (the replacement)"
                    ));
                }
                out.destructive.push(Destructive {
                    op: index,
                    step: Some(step),
                    what: format!("removes the case `{case}` of `{name}.{column}`"),
                    live: false,
                });
                let next: Vec<String> = cases.iter().filter(|c| *c != case).cloned().collect();
                self.rewrite_cases(
                    name,
                    column,
                    next,
                    &[(case.clone(), replacement.clone())],
                    Some((case, replacement)),
                    out,
                );
            }
        }
        Ok(())
    }

    /// The cases of the enumeration column `table.column`.
    ///
    /// # Errors
    ///
    /// An unknown column, or one that isn't an enumeration.
    fn cases_of(columns: &[ColumnDef], table: &str, column: &str) -> Result<Vec<String>, String> {
        match &Self::column(columns, table, column)?.ty {
            ColumnType::Enumeration(cases) => Ok(cases.clone()),
            other => Err(format!(
                "`{table}.{column}` isn't an enumeration column (it is `{}`)",
                other.lock_name()
            )),
        }
    }

    /// Gives the enumeration column `table.column` the case list `cases`,
    /// rewriting the stored values `map` names (old → new) and moving the
    /// column's default from `default.0` to `default.1` when it is
    /// `default.0`. Postgres swaps the `CHECK` constraint around an
    /// `UPDATE`; SQLite rebuilds the table, mapping the values while it
    /// copies the rows.
    fn rewrite_cases(
        &mut self,
        table: &str,
        column: &str,
        cases: Vec<String>,
        map: &[(String, String)],
        default: Option<(&String, &String)>,
        out: &mut Rendered,
    ) {
        let Some(mut next) = self.tables.get(table).cloned() else {
            return;
        };
        let mut new_default = None;
        for c in next.iter_mut().filter(|c| c.name == column) {
            c.ty = ColumnType::Enumeration(cases.clone());
            if let (Some((from, to)), Some(Literal::Text(d))) = (default, &c.default) {
                if d == from {
                    c.default = Some(Literal::Text(to.clone()));
                    new_default = Some(to.clone());
                }
            }
        }
        out.postgres.push(drop_check_postgres(table, column));
        for (from, to) in map {
            out.postgres.push(format!(
                "UPDATE {} SET {c} = {} WHERE {c} = {}",
                quote_identifier(table),
                sql_text(to),
                sql_text(from),
                c = quote_identifier(column)
            ));
        }
        if let Some(d) = &new_default {
            out.postgres.push(format!(
                "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}",
                quote_identifier(table),
                quote_identifier(column),
                sql_text(d)
            ));
        }
        out.postgres.push(add_check_postgres(table, column, &cases));
        let mut mapping = BTreeMap::new();
        if !map.is_empty() {
            let whens: Vec<String> = map
                .iter()
                .map(|(from, to)| format!("WHEN {} THEN {}", sql_text(from), sql_text(to)))
                .collect();
            mapping.insert(
                column.to_owned(),
                format!(
                    "CASE {c} {} ELSE {c} END",
                    whens.join(" "),
                    c = quote_identifier(column)
                ),
            );
        }
        let copy = self.copy_list(&next, &mapping);
        out.sqlite.extend(self.rebuild_sqlite(table, &next, &copy));
        self.tables.insert(table.to_owned(), next);
    }

    /// The stored columns of `columns` and the expression each is copied
    /// from in a rebuild: the column itself, or `mapping`'s expression for
    /// a logical column it names.
    fn copy_list(
        &self,
        columns: &[ColumnDef],
        mapping: &BTreeMap<String, String>,
    ) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for c in columns {
            for s in stored(c) {
                let from = mapping
                    .get(&c.name)
                    .cloned()
                    .unwrap_or_else(|| quote_identifier(&s));
                out.push((s, from));
            }
        }
        out
    }

    /// SQLite's table rebuild (its documented procedure for schema changes
    /// `ALTER TABLE` can't make): create the table as `columns` define it
    /// under [`REBUILD_TABLE`], copy every row (`copy`: each stored column
    /// and its source expression), drop the original, rename the copy, and
    /// create the original's indexes again under their own names.
    /// `migrate::apply` turns foreign key enforcement off around SQLite
    /// migrations and checks the keys before committing, so tables that
    /// reference this one keep pointing at it.
    fn rebuild_sqlite(
        &self,
        table: &str,
        columns: &[ColumnDef],
        copy: &[(String, String)],
    ) -> Vec<String> {
        let mut defs = Vec::new();
        for c in columns {
            let lit = c
                .default
                .as_ref()
                .and_then(|l| literal(l, &c.ty, Dialect::Sqlite).ok());
            defs.extend(c.column().definitions(lit.as_deref(), Dialect::Sqlite));
        }
        let mut out = vec![
            format!(
                "CREATE TABLE {} ({})",
                quote_identifier(REBUILD_TABLE),
                defs.join(", ")
            ),
            format!(
                "INSERT INTO {} ({}) SELECT {} FROM {}",
                quote_identifier(REBUILD_TABLE),
                copy.iter()
                    .map(|(s, _)| quote_identifier(s))
                    .collect::<Vec<_>>()
                    .join(", "),
                copy.iter()
                    .map(|(_, e)| e.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                quote_identifier(table)
            ),
            format!("DROP TABLE {}", quote_identifier(table)),
            format!(
                "ALTER TABLE {} RENAME TO {}",
                quote_identifier(REBUILD_TABLE),
                quote_identifier(table)
            ),
        ];
        if let Some(ix) = self.indexes.get(table) {
            out.extend(ix.iter().map(|i| index_statement(table, i)));
        }
        out
    }
}
