//! Shared fixtures for the persistence tests: a small schema shaped like the
//! entities of `docs/design/19-persistence.md`, seed rows, and helpers that
//! render an [`ir::Statement`] for a database's dialect and run it.
//!
//! The schema, with the K entities it stands for:
//!
//! ```k
//! entity Note {
//!     @Id let id: Uuid
//!     var title: String
//!     var body: String
//!     var isPinned: Bool
//!     var priority: Int
//!     var category: String?
//!     @Version var revision: Int = 1
//!     @Relation var tags: List<Tag> = []
//! }
//! entity Tag { @Id let id: Uuid; @Relation let note: Note; var name: String }
//! entity User { @Id let id: Uuid; @Unique var email: String; @Unique var handle: String?; var name: String }
//! enum OrderStatus { case awaitingPayment, paid, shipped, cancelled }
//! entity Order { @Id let id: Uuid; var status: OrderStatus; var total: Money }
//! ```
//!
//! Every file under `crates/k_db/tests/` that needs it declares `mod common;`.
//! Not every file uses every helper, hence the `dead_code` allowance.

#![allow(dead_code)]

use k_db::ir::{CmpOp, Operand, Order, Predicate, Select, Statement};
use k_db::migrate::{self, Migration};
use k_db::schema::{Column, ColumnType, Schema, Table};
use k_db::sql::{Dialect, render};
use k_db::testing::{TestDatabase, test_databases};
use k_db::{Database, Row, Value};

/// A column of `ty` named `name`, storing the field of the same name, with
/// every flag off.
pub fn column(name: &str, ty: ColumnType) -> Column {
    Column {
        field: Some(name.to_owned()),
        name: name.to_owned(),
        ty,
        primary_key: false,
        nullable: false,
        unique: false,
        indexed: false,
        references: None,
    }
}

/// The primary key column `id`.
pub fn id_column() -> Column {
    Column {
        primary_key: true,
        ..column("id", ColumnType::Uuid)
    }
}

/// The canonical text of test UUID number `n` (`n` is recoverable with
/// [`number`]), so expectations can name rows by small numbers.
pub fn uuid(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

/// The row number encoded in a UUID made by [`uuid`].
pub fn number(id: &str) -> u32 {
    u32::from_str_radix(id.get(24..).unwrap_or("0"), 16).unwrap_or(u32::MAX)
}

/// `entity Note`.
pub fn note_table() -> Table {
    Table {
        entity: "notes.Note".into(),
        name: "note".into(),
        columns: vec![
            id_column(),
            column("title", ColumnType::Text),
            column("body", ColumnType::Text),
            Column {
                field: Some("isPinned".into()),
                ..column("is_pinned", ColumnType::Boolean)
            },
            column("priority", ColumnType::Integer),
            Column {
                nullable: true,
                ..column("category", ColumnType::Text)
            },
            column("revision", ColumnType::Integer),
        ],
    }
}

/// The columns of `note`, in field order.
pub const NOTE_COLUMNS: [&str; 7] = [
    "id",
    "title",
    "body",
    "is_pinned",
    "priority",
    "category",
    "revision",
];

/// `entity Tag`, with a to-one relation to `Note`.
pub fn tag_table() -> Table {
    Table {
        entity: "notes.Tag".into(),
        name: "tag".into(),
        columns: vec![
            id_column(),
            Column {
                field: Some("note".into()),
                references: Some("note".into()),
                indexed: true,
                ..column("note_id", ColumnType::Uuid)
            },
            column("name", ColumnType::Text),
        ],
    }
}

/// `entity User`: one required and one optional unique field.
pub fn user_table() -> Table {
    Table {
        entity: "accounts.User".into(),
        name: "user".into(),
        columns: vec![
            id_column(),
            Column {
                unique: true,
                ..column("email", ColumnType::Text)
            },
            Column {
                unique: true,
                nullable: true,
                ..column("handle", ColumnType::Text)
            },
            column("name", ColumnType::Text),
        ],
    }
}

/// The cases of `enum OrderStatus`.
pub fn order_statuses() -> Vec<String> {
    ["awaitingPayment", "paid", "shipped", "cancelled"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// `entity Order`: a reserved-word table with an enum and a money column.
pub fn order_table() -> Table {
    Table {
        entity: "store.Order".into(),
        name: "order".into(),
        columns: vec![
            id_column(),
            column("status", ColumnType::Enumeration(order_statuses())),
            column("total", ColumnType::Money),
        ],
    }
}

/// Every fixture table, in creation order (referenced tables first).
pub fn tables() -> Vec<Table> {
    vec![note_table(), tag_table(), user_table(), order_table()]
}

/// The fixture schema, tables sorted by name.
pub fn schema() -> Schema {
    let mut tables = tables();
    tables.sort_by(|a, b| a.name.cmp(&b.name));
    Schema { tables }
}

/// A migration creating `tables` in order, for both dialects.
pub fn create_migration(name: &str, tables: &[Table]) -> Migration {
    Migration {
        name: name.into(),
        sqlite: tables
            .iter()
            .flat_map(|t| t.create_sql(Dialect::Sqlite))
            .collect(),
        postgres: tables
            .iter()
            .flat_map(|t| t.create_sql(Dialect::Postgres))
            .collect(),
    }
}

/// A migration with the same raw statements on both dialects.
pub fn raw_migration(name: &str, statements: &[&str]) -> Migration {
    let statements: Vec<String> = statements.iter().map(|s| (*s).to_owned()).collect();
    Migration {
        name: name.into(),
        sqlite: statements.clone(),
        postgres: statements,
    }
}

/// `0001_init`: the whole fixture schema.
pub fn init_migration() -> Migration {
    create_migration("0001_init", &tables())
}

/// Every available database, empty.
pub async fn databases() -> Vec<TestDatabase> {
    test_databases().await.expect("test databases")
}

/// Every available database with [`init_migration`] applied.
pub async fn migrated() -> Vec<TestDatabase> {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration()])
            .await
            .expect(t.name);
    }
    all
}

/// Every available database with the schema and the [`seed_notes`] rows.
pub async fn seeded() -> Vec<TestDatabase> {
    let all = migrated().await;
    for t in &all {
        seed_notes(&t.db).await;
    }
    all
}

/// Drops what a test created in each database.
pub async fn finish(all: Vec<TestDatabase>) {
    for t in all {
        t.finish().await.expect("cleanup");
    }
}

/// A `Text` value.
pub fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

/// The parameters of one `note` row in [`NOTE_COLUMNS`] order (body empty,
/// revision 1).
pub fn note_values(
    n: u32,
    title: &str,
    pinned: bool,
    priority: i64,
    category: Option<&str>,
) -> Vec<Value> {
    vec![
        text(&uuid(n)),
        text(title),
        text(""),
        Value::Bool(pinned),
        Value::Int(priority),
        category.map_or(Value::Null, text),
        Value::Int(1),
    ]
}

/// `insert(all:)` of `rows` notes.
pub fn insert_notes(rows: u32) -> Statement {
    Statement::Insert {
        table: "note".into(),
        columns: NOTE_COLUMNS.iter().map(|c| (*c).to_owned()).collect(),
        rows,
    }
}

/// The five seed notes:
///
/// | n | title | pinned | priority | category |
/// |---|---|---|---|---|
/// | 1 | `Alpha plan` | yes | 3 | `work` |
/// | 2 | `beta notes` | no | 1 | nil |
/// | 3 | `Gamma 50% off` | yes | 2 | `home` |
/// | 4 | `delta_x` | no | 5 | `work` |
/// | 5 | `Épsilon ünïcode` | no | 4 | nil |
pub fn seed_rows() -> Vec<Vec<Value>> {
    vec![
        note_values(1, "Alpha plan", true, 3, Some("work")),
        note_values(2, "beta notes", false, 1, None),
        note_values(3, "Gamma 50% off", true, 2, Some("home")),
        note_values(4, "delta_x", false, 5, Some("work")),
        note_values(5, "Épsilon ünïcode", false, 4, None),
    ]
}

/// Inserts [`seed_rows`] with one multi-row insert.
pub async fn seed_notes(db: &Database) {
    let rows = seed_rows();
    let params: Vec<Value> = rows.into_iter().flatten().collect();
    let n = run(db, &insert_notes(5), &params).await;
    assert_eq!(n, 5, "seeding inserts five notes");
}

/// `SELECT <every note column> FROM "note"`.
pub fn note_select() -> Select {
    Select {
        table: "note".into(),
        columns: NOTE_COLUMNS.iter().map(|c| (*c).to_owned()).collect(),
        filter: None,
        order: vec![],
        limit: None,
        offset: None,
    }
}

/// `notes.filter { … }` with the given lowered predicate.
pub fn notes_where(filter: Predicate) -> Statement {
    Statement::Select(Select {
        filter: Some(filter),
        ..note_select()
    })
}

/// An ascending or descending order key.
pub fn order(column: &str, descending: bool) -> Order {
    Order {
        column: column.into(),
        descending,
    }
}

/// `Column op Param(n)`.
pub fn cmp(column: &str, op: CmpOp, param: u32) -> Predicate {
    Predicate::Compare {
        left: Operand::Column(column.into()),
        op,
        right: Operand::Param(param),
    }
}

/// `Column == Param(n)`.
pub fn eq(column: &str, param: u32) -> Predicate {
    cmp(column, CmpOp::Eq, param)
}

/// The SQL of a statement in both dialects: `(sqlite, postgres)`.
pub fn both(statement: &Statement) -> (String, String) {
    (
        render(statement, Dialect::Sqlite).text,
        render(statement, Dialect::Postgres).text,
    )
}

/// Renders `statement` for `db` and executes it, expecting success.
pub async fn run(db: &Database, statement: &Statement, params: &[Value]) -> u64 {
    let sql = render(statement, db.dialect());
    db.execute(&sql.text, params)
        .await
        .map_err(|e| format!("{}: {e}", sql.text))
        .expect("statement")
}

/// Renders `statement` for `db` and runs it as a query, expecting success.
pub async fn rows(db: &Database, statement: &Statement, params: &[Value]) -> Vec<Row> {
    let sql = render(statement, db.dialect());
    db.query(&sql.text, params)
        .await
        .map_err(|e| format!("{}: {e}", sql.text))
        .expect("query")
}

/// The row numbers of the `id` column of the query's rows, in row order.
pub async fn ids(db: &Database, statement: &Statement, params: &[Value]) -> Vec<u32> {
    rows(db, statement, params)
        .await
        .iter()
        .map(|r| match r.get("id") {
            Some(Value::Text(id)) => number(id),
            _ => u32::MAX,
        })
        .collect()
}

/// [`ids`], sorted: for queries without an order.
pub async fn sorted_ids(db: &Database, statement: &Statement, params: &[Value]) -> Vec<u32> {
    let mut out = ids(db, statement, params).await;
    out.sort_unstable();
    out
}

/// The single integer a `COUNT(*)` (or any one-cell query) returns.
pub async fn scalar(db: &Database, sql: &str, params: &[Value]) -> Option<i64> {
    db.query(sql, params)
        .await
        .ok()
        .and_then(|r| r.first().and_then(|row| row.values.first().cloned()))
        .and_then(|v| v.as_int())
}

/// The number of rows of `table`.
pub async fn table_count(db: &Database, table: &str) -> Option<i64> {
    scalar(
        db,
        &format!(
            "SELECT COUNT(*) FROM {}",
            k_db::sql::quote_identifier(table)
        ),
        &[],
    )
    .await
}

/// Whether a value is K's `true` as a driver returns it: `Bool(true)` on
/// Postgres, the integer 1 on SQLite (the generated row decoder maps both).
pub fn is_true(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true) | Value::Int(1)))
}

/// Whether a value is K's `false` as a driver returns it.
pub fn is_false(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(false) | Value::Int(0)))
}

/// A JSON array text holding `items` as strings: the list parameter of an
/// `In` predicate (`docs/compiler/db.md` §5).
pub fn json_list(items: &[&str]) -> Value {
    Value::Text(serde_json::to_string(items).expect("json"))
}

/// A JSON array of the test UUIDs of `numbers`.
pub fn id_list(numbers: &[u32]) -> Value {
    let ids: Vec<String> = numbers.iter().map(|n| uuid(*n)).collect();
    Value::Text(serde_json::to_string(&ids).expect("json"))
}

/// Whether `table` has a column called `column` on `db`, from the catalog.
///
/// Not by selecting it: SQLite reads an unknown double-quoted identifier as a
/// string literal unless the driver turns that off (see `errors.rs`).
pub async fn has_column(db: &Database, table: &str, column: &str) -> bool {
    let sql = match db.dialect() {
        Dialect::Sqlite => "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
        Dialect::Postgres => {
            "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2"
        }
    };
    scalar(db, sql, &[text(table), text(column)]).await == Some(1)
}
