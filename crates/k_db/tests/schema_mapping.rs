//! Entity mapping: the DDL `Table::create_sql` produces for every column
//! type and option in both dialects (`docs/compiler/db.md` §3,
//! `docs/design/19-persistence.md`, "Entities and tables"), and that the
//! DDL actually runs on every available database.

use crate::common;

use common::{
    column, create_migration, finish, id_column, migrated, note_table, order_table, tag_table,
    user_table,
};
use k_db::Dialect;
use k_db::migrate;
use k_db::schema::{Column, ColumnType, Table};
use k_db::testing::test_databases;

/// A one-column table `"t"` with column `"c"` of `ty`.
fn single(ty: ColumnType) -> Table {
    Table {
        entity: "t.T".into(),
        name: "t".into(),
        columns: vec![column("c", ty)],
    }
}

fn ddl(table: &Table, dialect: Dialect) -> Vec<String> {
    table.create_sql(dialect)
}

/// The index names of `table` on `db`.
async fn index_names(db: &k_db::Database, table: &str) -> Vec<String> {
    let (sql, param) = match db.dialect() {
        Dialect::Sqlite => (
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            table,
        ),
        Dialect::Postgres => (
            "SELECT indexname::text FROM pg_indexes WHERE schemaname = current_schema() AND tablename = $1 AND indexname NOT LIKE '%_pkey' ORDER BY indexname",
            table,
        ),
    };
    db.query(sql, &[common::text(param)])
        .await
        .expect("index listing")
        .into_iter()
        .filter_map(|r| {
            r.values
                .first()
                .and_then(|v| v.as_text().map(str::to_owned))
        })
        .collect()
}

// `Uuid` is `uuid` on Postgres and `TEXT` on SQLite.
#[test]
fn uuid_columns() {
    let t = single(ColumnType::Uuid);
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" TEXT NOT NULL)"]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        ["CREATE TABLE \"t\" (\"c\" uuid NOT NULL)"]
    );
}

// `String`, `EmailAddress` and `Url` are `text` / `TEXT`.
#[test]
fn text_columns() {
    let t = single(ColumnType::Text);
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" TEXT NOT NULL)"]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        ["CREATE TABLE \"t\" (\"c\" text COLLATE \"C\" NOT NULL)"]
    );
}

// `Int` is `bigint` / `INTEGER`, so every safe integer fits on both.
#[test]
fn integer_columns() {
    let t = single(ColumnType::Integer);
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" INTEGER NOT NULL)"]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        ["CREATE TABLE \"t\" (\"c\" bigint NOT NULL)"]
    );
}

// `Decimal` is `numeric(38,10)` on Postgres and `TEXT` on SQLite (a REAL would round),
// with the `k_decimal` collation so comparisons and ordering follow the numbers.
#[test]
fn decimal_columns() {
    let t = single(ColumnType::Decimal);
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" TEXT COLLATE k_decimal NOT NULL)"]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        ["CREATE TABLE \"t\" (\"c\" numeric(38,10) NOT NULL)"]
    );
}

// `Bool` is `boolean` on Postgres and `INTEGER` (0/1) on SQLite.
#[test]
fn boolean_columns() {
    let t = single(ColumnType::Boolean);
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" INTEGER NOT NULL)"]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        ["CREATE TABLE \"t\" (\"c\" boolean NOT NULL)"]
    );
}

// `Date` is `timestamptz` on Postgres and RFC 3339 `TEXT` on SQLite.
#[test]
fn timestamp_columns() {
    let t = single(ColumnType::Timestamp);
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" TEXT NOT NULL)"]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        ["CREATE TABLE \"t\" (\"c\" timestamptz NOT NULL)"]
    );
}

// A `Codable` value is `jsonb` on Postgres and `TEXT` on SQLite.
#[test]
fn json_columns() {
    let t = single(ColumnType::Json);
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" TEXT NOT NULL)"]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        ["CREATE TABLE \"t\" (\"c\" jsonb NOT NULL)"]
    );
}

// A payload-free enum is text with a CHECK listing its cases, on both dialects.
#[test]
fn enumeration_columns_check_their_cases() {
    let t = single(ColumnType::Enumeration(vec![
        "draft".into(),
        "published".into(),
    ]));
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" TEXT NOT NULL CHECK (\"c\" IN ('draft', 'published')))"]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        [
            "CREATE TABLE \"t\" (\"c\" text COLLATE \"C\" NOT NULL CHECK (\"c\" IN ('draft', 'published')))"
        ]
    );
}

// A quote inside an enum case name is doubled in the CHECK's string literal.
#[test]
fn enumeration_case_quotes_are_escaped() {
    let t = single(ColumnType::Enumeration(vec!["it's".into()]));
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c\" TEXT NOT NULL CHECK (\"c\" IN ('it''s')))"]
    );
}

// `Money` is one logical column stored as `<name>_amount` and `<name>_currency`.
#[test]
fn money_expands_to_amount_and_currency() {
    let t = single(ColumnType::Money);
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        [
            "CREATE TABLE \"t\" (\"c_amount\" TEXT COLLATE k_decimal NOT NULL, \"c_currency\" TEXT NOT NULL)"
        ]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        [
            "CREATE TABLE \"t\" (\"c_amount\" numeric(19,4) NOT NULL, \"c_currency\" char(3) NOT NULL)"
        ]
    );
}

// An optional `Money?` field makes both stored halves nullable.
#[test]
fn optional_money_makes_both_halves_nullable() {
    let mut t = single(ColumnType::Money);
    t.columns[0].nullable = true;
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        ["CREATE TABLE \"t\" (\"c_amount\" TEXT COLLATE k_decimal, \"c_currency\" TEXT)"]
    );
}

// `T?` fields are nullable; every other column is NOT NULL.
#[test]
fn optional_fields_are_nullable() {
    let mut t = single(ColumnType::Text);
    t.columns[0].nullable = true;
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        ["CREATE TABLE \"t\" (\"c\" text COLLATE \"C\")"]
    );
}

// The whole `note` entity, in field order, in both dialects.
#[test]
fn the_note_entity_in_both_dialects() {
    let t = note_table();
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        [
            "CREATE TABLE \"note\" (\"id\" TEXT NOT NULL PRIMARY KEY, \"title\" TEXT NOT NULL, \"body\" TEXT NOT NULL, \"is_pinned\" INTEGER NOT NULL, \"priority\" INTEGER NOT NULL, \"category\" TEXT, \"revision\" INTEGER NOT NULL)"
        ]
    );
    assert_eq!(
        ddl(&t, Dialect::Postgres),
        [
            "CREATE TABLE \"note\" (\"id\" uuid NOT NULL PRIMARY KEY, \"title\" text COLLATE \"C\" NOT NULL, \"body\" text COLLATE \"C\" NOT NULL, \"is_pinned\" boolean NOT NULL, \"priority\" bigint NOT NULL, \"category\" text COLLATE \"C\", \"revision\" bigint NOT NULL)"
        ]
    );
}

// A to-one relation is a `<field>_id` column referencing the target's id, always indexed.
#[test]
fn to_one_relations_are_indexed_foreign_keys() {
    assert_eq!(
        ddl(&tag_table(), Dialect::Postgres),
        [
            "CREATE TABLE \"tag\" (\"id\" uuid NOT NULL PRIMARY KEY, \"note_id\" uuid NOT NULL REFERENCES \"note\" (\"id\"), \"name\" text COLLATE \"C\" NOT NULL)",
            "CREATE INDEX \"tag_note_id_idx\" ON \"tag\" (\"note_id\")",
        ]
    );
}

// A foreign key is indexed even when the column doesn't say `indexed`.
#[test]
fn foreign_keys_are_indexed_without_the_flag() {
    let mut t = tag_table();
    t.columns[1].indexed = false;
    assert_eq!(
        ddl(&t, Dialect::Sqlite)[1],
        "CREATE INDEX \"tag_note_id_idx\" ON \"tag\" (\"note_id\")"
    );
}

// `@Unique` fields get a unique index named `<table>_<column>_key`, Postgres' constraint name.
#[test]
fn unique_fields_get_key_indexes() {
    let sql = ddl(&user_table(), Dialect::Sqlite);
    assert_eq!(
        &sql[1..],
        [
            "CREATE UNIQUE INDEX \"user_email_key\" ON \"user\" (\"email\")",
            "CREATE UNIQUE INDEX \"user_handle_key\" ON \"user\" (\"handle\")",
        ]
    );
    assert_eq!(user_table().unique_index_name("email"), "user_email_key");
}

// A unique column that is also `@Index`ed gets only the unique index.
#[test]
fn unique_and_indexed_gets_one_index() {
    let mut t = single(ColumnType::Text);
    t.columns[0].unique = true;
    t.columns[0].indexed = true;
    assert_eq!(
        ddl(&t, Dialect::Sqlite),
        [
            "CREATE TABLE \"t\" (\"c\" TEXT NOT NULL)",
            "CREATE UNIQUE INDEX \"t_c_key\" ON \"t\" (\"c\")",
        ]
    );
}

// `@Index` adds a plain index named `<table>_<column>_idx`.
#[test]
fn indexed_fields_get_plain_indexes() {
    let mut t = single(ColumnType::Integer);
    t.columns[0].indexed = true;
    assert_eq!(
        ddl(&t, Dialect::Postgres)[1],
        "CREATE INDEX \"t_c_idx\" ON \"t\" (\"c\")"
    );
}

// The reserved-word entity `Order` maps to the quoted table "order" with an enum and money.
#[test]
fn the_order_entity_in_both_dialects() {
    assert_eq!(
        ddl(&order_table(), Dialect::Sqlite),
        [
            "CREATE TABLE \"order\" (\"id\" TEXT NOT NULL PRIMARY KEY, \"status\" TEXT NOT NULL CHECK (\"status\" IN ('awaitingPayment', 'paid', 'shipped', 'cancelled')), \"total_amount\" TEXT COLLATE k_decimal NOT NULL, \"total_currency\" TEXT NOT NULL)"
        ]
    );
    assert_eq!(
        ddl(&order_table(), Dialect::Postgres),
        [
            "CREATE TABLE \"order\" (\"id\" uuid NOT NULL PRIMARY KEY, \"status\" text COLLATE \"C\" NOT NULL CHECK (\"status\" IN ('awaitingPayment', 'paid', 'shipped', 'cancelled')), \"total_amount\" numeric(19,4) NOT NULL, \"total_currency\" char(3) NOT NULL)"
        ]
    );
}

// The column types' `schema.lock` names are the closed `ColumnType` enum's case names.
#[test]
fn lock_names_of_every_column_type() {
    let names: Vec<&str> = [
        ColumnType::Uuid,
        ColumnType::Text,
        ColumnType::Integer,
        ColumnType::Decimal,
        ColumnType::Boolean,
        ColumnType::Timestamp,
        ColumnType::Money,
        ColumnType::Json,
        ColumnType::Enumeration(vec![]),
    ]
    .iter()
    .map(ColumnType::lock_name)
    .collect();
    assert_eq!(
        names,
        [
            "uuid",
            "text",
            "integer",
            "decimal",
            "boolean",
            "timestamp",
            "money",
            "json",
            "enumeration"
        ]
    );
}

// The fixture schema's DDL runs on every database, with the expected indexes.
#[tokio::test]
async fn the_whole_schema_creates_on_every_database() {
    let all = migrated().await;
    for t in &all {
        assert_eq!(
            index_names(&t.db, "user").await,
            ["user_email_key", "user_handle_key"],
            "{}",
            t.name
        );
        assert_eq!(
            index_names(&t.db, "tag").await,
            ["tag_note_id_idx"],
            "{}",
            t.name
        );
        assert_eq!(
            common::table_count(&t.db, "order").await,
            Some(0),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Tables named after reserved words work once quoted, on every database.
#[tokio::test]
async fn reserved_word_tables_and_columns_work() {
    let tables: Vec<Table> = ["select", "group", "where"]
        .iter()
        .map(|name| Table {
            entity: format!("x.{name}"),
            name: (*name).to_owned(),
            columns: vec![id_column(), column("from", ColumnType::Text)],
        })
        .collect();
    for t in test_databases().await.expect("databases") {
        migrate::apply(&t.db, &[create_migration("0001_reserved", &tables)])
            .await
            .expect(t.name);
        for table in &tables {
            assert_eq!(
                common::table_count(&t.db, &table.name).await,
                Some(0),
                "{}: {}",
                t.name,
                table.name
            );
        }
        t.finish().await.expect("cleanup");
    }
}

// An identifier containing a double quote is escaped, not a way out of the identifier.
#[tokio::test]
async fn identifiers_with_quotes_stay_identifiers() {
    let table = Table {
        entity: "x.Weird".into(),
        name: "we\"ird".into(),
        columns: vec![
            id_column(),
            column("a\"; DROP TABLE x; --", ColumnType::Text),
        ],
    };
    for t in test_databases().await.expect("databases") {
        migrate::apply(
            &t.db,
            &[create_migration("0001_weird", std::slice::from_ref(&table))],
        )
        .await
        .expect(t.name);
        assert_eq!(
            common::table_count(&t.db, "we\"ird").await,
            Some(0),
            "{}",
            t.name
        );
        t.finish().await.expect("cleanup");
    }
}

// An `@Index`ed money field indexes its stored halves, so the migration runs (one logical column).
#[tokio::test]
async fn an_indexed_money_column_creates() {
    let table = Table {
        entity: "store.Product".into(),
        name: "product".into(),
        columns: vec![
            id_column(),
            Column {
                indexed: true,
                ..column("price", ColumnType::Money)
            },
        ],
    };
    for t in test_databases().await.expect("databases") {
        let result = migrate::apply(
            &t.db,
            &[create_migration(
                "0001_product",
                std::slice::from_ref(&table),
            )],
        )
        .await;
        assert!(result.is_ok(), "{}: {result:?}", t.name);
        // The index really covers the stored amount, not a phantom `price` column
        // (or, on SQLite, the string literal 'price').
        let sql = match t.db.dialect() {
            Dialect::Sqlite => {
                "SELECT COUNT(*) FROM pragma_index_list('product') AS il, pragma_index_info(il.name) AS ii WHERE ii.name = 'price_amount'"
            }
            Dialect::Postgres => {
                "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = current_schema() AND tablename = 'product' AND indexdef LIKE '%price_amount%'"
            }
        };
        assert_eq!(common::scalar(&t.db, sql, &[]).await, Some(1), "{}", t.name);
        t.finish().await.expect("cleanup");
    }
}

// On Postgres the stored types are the native ones (uuid, boolean, bigint), not text.
#[tokio::test]
async fn postgres_uses_native_column_types() {
    let all = migrated().await;
    for t in all.iter().filter(|t| t.db.dialect() == Dialect::Postgres) {
        let rows = t
            .db
            .query(
                "SELECT column_name::text, data_type::text FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'note' ORDER BY ordinal_position",
                &[],
            )
            .await
            .expect(t.name);
        let types: Vec<(String, String)> = rows
            .iter()
            .map(|r| {
                (
                    r.values[0].as_text().unwrap_or_default().to_owned(),
                    r.values[1].as_text().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            types,
            [
                ("id".to_owned(), "uuid".to_owned()),
                ("title".to_owned(), "text".to_owned()),
                ("body".to_owned(), "text".to_owned()),
                ("is_pinned".to_owned(), "boolean".to_owned()),
                ("priority".to_owned(), "bigint".to_owned()),
                ("category".to_owned(), "text".to_owned()),
                ("revision".to_owned(), "bigint".to_owned()),
            ]
        );
    }
    finish(all).await;
}
