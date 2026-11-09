//! Typed database errors (`docs/compiler/db.md` §2 and §7): unique
//! violations, their mapping to the entity's `UniqueViolation` case, every
//! other failure as a non-domain error (a panic in K), messages that never
//! leak passwords or bound values, and no driver panics on bad input.

use crate::common;

use common::{finish, migrated, run, schema, text, uuid};
use k_db::ir::{Operand, Statement};
use k_db::schema::{Column, ColumnType, Schema, Table};
use k_db::sql::render;
use k_db::{Database, DatabaseOptions, DbError, Dialect, Value};

fn insert_user() -> Statement {
    Statement::Insert {
        table: "user".into(),
        columns: vec!["id".into(), "email".into(), "handle".into(), "name".into()],
        rows: 1,
    }
}

fn user(n: u32, email: &str, handle: Option<&str>) -> Vec<Value> {
    vec![
        text(&uuid(n)),
        text(email),
        handle.map_or(Value::Null, text),
        text("Name"),
    ]
}

/// Inserts a user, returning the error if it failed.
async fn try_insert(db: &Database, values: &[Value]) -> Result<u64, DbError> {
    db.execute(&render(&insert_user(), db.dialect()).text, values)
        .await
}

fn violation(table: Option<&str>, columns: &[&str], constraint: Option<&str>) -> DbError {
    DbError::UniqueViolation {
        table: table.map(str::to_owned),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        constraint: constraint.map(str::to_owned),
    }
}

// --- unique violations on the databases -----------------------------------

// A duplicate `@Unique` email on insert is a UniqueViolation that maps to `User.email`.
#[tokio::test]
async fn a_duplicate_on_insert_maps_to_its_field() {
    let all = migrated().await;
    for t in &all {
        try_insert(&t.db, &user(1, "ana@example.com", None))
            .await
            .expect(t.name);
        let err = try_insert(&t.db, &user(2, "ana@example.com", None))
            .await
            .expect_err(t.name);
        assert_eq!(
            schema().unique_field(&err),
            Some(("accounts.User", "email")),
            "{}: {err:?}",
            t.name
        );
    }
    finish(all).await;
}

// SQLite reports the table and column; Postgres the constraint name. Both are enough.
#[tokio::test]
async fn each_database_reports_what_identifies_the_index() {
    let all = migrated().await;
    for t in &all {
        try_insert(&t.db, &user(1, "ana@example.com", None))
            .await
            .expect(t.name);
        let err = try_insert(&t.db, &user(2, "ana@example.com", None))
            .await
            .expect_err(t.name);
        let DbError::UniqueViolation {
            table,
            columns,
            constraint,
        } = &err
        else {
            panic!("{}: {err:?}", t.name);
        };
        match t.db.dialect() {
            Dialect::Sqlite => {
                assert_eq!(table.as_deref(), Some("user"), "{err:?}");
                assert_eq!(columns, &["email".to_owned()], "{err:?}");
            }
            Dialect::Postgres => {
                assert_eq!(constraint.as_deref(), Some("user_email_key"), "{err:?}");
                assert_eq!(table.as_deref(), Some("user"), "{err:?}");
            }
        }
    }
    finish(all).await;
}

// The second `@Unique` field maps to its own case, `User.handle`.
#[tokio::test]
async fn a_second_unique_field_maps_to_its_own_case() {
    let all = migrated().await;
    for t in &all {
        try_insert(&t.db, &user(1, "a@example.com", Some("ana")))
            .await
            .expect(t.name);
        let err = try_insert(&t.db, &user(2, "b@example.com", Some("ana")))
            .await
            .expect_err(t.name);
        assert_eq!(
            schema().unique_field(&err),
            Some(("accounts.User", "handle")),
            "{}: {err:?}",
            t.name
        );
    }
    finish(all).await;
}

// `update(entity)` into a taken value throws the same UniqueViolation case.
#[tokio::test]
async fn a_duplicate_on_update_maps_to_its_field() {
    let update = Statement::Update {
        table: "user".into(),
        set: vec![("email".into(), Operand::Param(1))],
        filter: Some(common::eq("id", 2)),
    };
    let all = migrated().await;
    for t in &all {
        try_insert(&t.db, &user(1, "a@example.com", None))
            .await
            .expect(t.name);
        try_insert(&t.db, &user(2, "b@example.com", None))
            .await
            .expect(t.name);
        let sql = render(&update, t.db.dialect());
        let err =
            t.db.execute(&sql.text, &[text("a@example.com"), text(&uuid(2))])
                .await
                .expect_err(t.name);
        assert_eq!(
            schema().unique_field(&err),
            Some(("accounts.User", "email")),
            "{}: {err:?}",
            t.name
        );
    }
    finish(all).await;
}

// Several nil values in an optional unique field don't collide (NULLs are distinct).
#[tokio::test]
async fn nil_values_of_a_unique_field_do_not_collide() {
    let all = migrated().await;
    for t in &all {
        for n in 1..=3 {
            try_insert(&t.db, &user(n, &format!("u{n}@example.com"), None))
                .await
                .expect(t.name);
        }
        assert_eq!(
            common::table_count(&t.db, "user").await,
            Some(3),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Uniqueness is exact: values differing only in case are different (normalizing is K's job).
#[tokio::test]
async fn uniqueness_is_case_sensitive() {
    let all = migrated().await;
    for t in &all {
        try_insert(&t.db, &user(1, "Ana@example.com", None))
            .await
            .expect(t.name);
        try_insert(&t.db, &user(2, "ana@example.com", None))
            .await
            .expect(t.name);
    }
    finish(all).await;
}

// A duplicate within one `insert(all:)` fails the whole statement; nothing is inserted.
#[tokio::test]
async fn a_duplicate_inside_one_insert_all_inserts_nothing() {
    let st = Statement::Insert {
        table: "user".into(),
        columns: vec!["id".into(), "email".into(), "handle".into(), "name".into()],
        rows: 3,
    };
    let all = migrated().await;
    for t in &all {
        let params = [
            user(1, "a@example.com", None),
            user(2, "b@example.com", None),
            user(3, "a@example.com", None),
        ]
        .concat();
        let err =
            t.db.execute(&render(&st, t.db.dialect()).text, &params)
                .await
                .expect_err(t.name);
        assert_eq!(
            schema().unique_field(&err),
            Some(("accounts.User", "email")),
            "{}: {err:?}",
            t.name
        );
        assert_eq!(
            common::table_count(&t.db, "user").await,
            Some(0),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A duplicate primary key is a UniqueViolation but of no `@Unique` field: a panic, since ids
// are generated.
#[tokio::test]
async fn a_duplicate_primary_key_maps_to_no_field() {
    let all = migrated().await;
    for t in &all {
        try_insert(&t.db, &user(1, "a@example.com", None))
            .await
            .expect(t.name);
        let err = try_insert(&t.db, &user(1, "b@example.com", None))
            .await
            .expect_err(t.name);
        assert!(
            matches!(err, DbError::UniqueViolation { .. }),
            "{}: {err:?}",
            t.name
        );
        assert_eq!(schema().unique_field(&err), None, "{}", t.name);
    }
    finish(all).await;
}

// A violation of a multi-column index written in a migration maps to no field.
#[tokio::test]
async fn a_composite_index_violation_maps_to_no_field() {
    let table = Table {
        entity: "x.Pair".into(),
        name: "pair".into(),
        columns: vec![
            common::id_column(),
            common::column("a", ColumnType::Text),
            common::column("b", ColumnType::Text),
        ],
    };
    let pair_schema = Schema {
        tables: vec![table.clone()],
    };
    for t in common::databases().await {
        k_db::migrate::apply(
            &t.db,
            &[common::create_migration(
                "0001_pair",
                std::slice::from_ref(&table),
            )],
        )
        .await
        .expect(t.name);
        t.db.execute(
            "CREATE UNIQUE INDEX \"pair_a_b_key\" ON \"pair\" (\"a\", \"b\")",
            &[],
        )
        .await
        .expect(t.name);
        let insert = |n: u32| {
            format!(
                "INSERT INTO \"pair\" (\"id\", \"a\", \"b\") VALUES ('{}', 'x', 'y')",
                uuid(n)
            )
        };
        t.db.execute(&insert(1), &[]).await.expect(t.name);
        let err = t.db.execute(&insert(2), &[]).await.expect_err(t.name);
        assert!(
            matches!(err, DbError::UniqueViolation { .. }),
            "{}: {err:?}",
            t.name
        );
        assert_eq!(pair_schema.unique_field(&err), None, "{}: {err:?}", t.name);
        t.finish().await.expect("cleanup");
    }
}

// A unique violation on the reserved-word table "user" still parses (SQLite's message).
#[tokio::test]
async fn unique_violations_on_reserved_word_tables_map() {
    let table = Table {
        entity: "store.Order".into(),
        name: "order".into(),
        columns: vec![
            common::id_column(),
            Column {
                unique: true,
                field: Some("reference".into()),
                ..common::column("reference", ColumnType::Text)
            },
        ],
    };
    let s = Schema {
        tables: vec![table.clone()],
    };
    for t in common::databases().await {
        k_db::migrate::apply(
            &t.db,
            &[common::create_migration(
                "0001",
                std::slice::from_ref(&table),
            )],
        )
        .await
        .expect(t.name);
        let st = Statement::Insert {
            table: "order".into(),
            columns: vec!["id".into(), "reference".into()],
            rows: 1,
        };
        run(&t.db, &st, &[text(&uuid(1)), text("R-1")]).await;
        let err =
            t.db.execute(
                &render(&st, t.db.dialect()).text,
                &[text(&uuid(2)), text("R-1")],
            )
            .await
            .expect_err(t.name);
        assert_eq!(
            s.unique_field(&err),
            Some(("store.Order", "reference")),
            "{}: {err:?}",
            t.name
        );
        t.finish().await.expect("cleanup");
    }
}

// --- unique_field without a database --------------------------------------

// Postgres-style: the constraint name alone identifies the field.
#[test]
fn unique_field_by_constraint_name() {
    let err = violation(None, &[], Some("user_handle_key"));
    assert_eq!(
        schema().unique_field(&err),
        Some(("accounts.User", "handle"))
    );
}

// SQLite-style: table plus single column identify the field.
#[test]
fn unique_field_by_table_and_column() {
    let err = violation(Some("user"), &["email"], None);
    assert_eq!(
        schema().unique_field(&err),
        Some(("accounts.User", "email"))
    );
}

// The right column on the wrong table is not a match.
#[test]
fn unique_field_needs_the_right_table() {
    let err = violation(Some("tag"), &["email"], None);
    assert_eq!(schema().unique_field(&err), None);
}

// Two reported columns (a composite index) never map to one field.
#[test]
fn unique_field_ignores_composite_columns() {
    let err = violation(Some("user"), &["email", "handle"], None);
    assert_eq!(schema().unique_field(&err), None);
}

// An unknown constraint name maps to nothing.
#[test]
fn unique_field_ignores_unknown_constraints() {
    let err = violation(Some("user"), &[], Some("user_pkey"));
    assert_eq!(schema().unique_field(&err), None);
}

// A non-unique column with the right name is not a match.
#[test]
fn unique_field_ignores_columns_without_unique() {
    let err = violation(Some("user"), &["name"], None);
    assert_eq!(schema().unique_field(&err), None);
}

// A retired column has no field, so its index maps to nothing.
#[test]
fn unique_field_ignores_retired_columns() {
    let mut s = schema();
    for t in &mut s.tables {
        for c in &mut t.columns {
            if c.name == "email" {
                c.field = None;
            }
        }
    }
    assert_eq!(
        s.unique_field(&violation(Some("user"), &["email"], None)),
        None
    );
}

// The field name, not the column name, is what K sees (`isPinned`, not `is_pinned`).
#[test]
fn unique_field_returns_the_field_name() {
    let table = Table {
        entity: "accounts.User".into(),
        name: "user".into(),
        columns: vec![Column {
            unique: true,
            field: Some("emailAddress".into()),
            ..common::column("email_address", ColumnType::Text)
        }],
    };
    let s = Schema {
        tables: vec![table],
    };
    let err = violation(None, &[], Some("user_email_address_key"));
    assert_eq!(
        s.unique_field(&err),
        Some(("accounts.User", "emailAddress"))
    );
}

// Any other error maps to nothing.
#[test]
fn unique_field_of_other_errors_is_none() {
    for e in [
        DbError::Statement("UNIQUE constraint failed: user.email".into()),
        DbError::Connection("x".into()),
        DbError::InvalidUrl("x".into()),
        DbError::UnsupportedType("x".into()),
        DbError::Migration {
            name: "0001".into(),
            message: "user_email_key".into(),
        },
    ] {
        assert_eq!(schema().unique_field(&e), None, "{e:?}");
    }
}

// --- every other failure ---------------------------------------------------

// A foreign key to a missing row is a statement failure, not a unique violation.
#[tokio::test]
async fn a_dangling_foreign_key_is_a_statement_failure() {
    let all = migrated().await;
    for t in &all {
        let st = Statement::Insert {
            table: "tag".into(),
            columns: vec!["id".into(), "note_id".into(), "name".into()],
            rows: 1,
        };
        let err =
            t.db.execute(
                &render(&st, t.db.dialect()).text,
                &[text(&uuid(1)), text(&uuid(9)), text("x")],
            )
            .await
            .expect_err(t.name);
        assert!(matches!(err, DbError::Statement(_)), "{}: {err:?}", t.name);
    }
    finish(all).await;
}

// Invalid SQL is a statement failure, never a panic.
#[tokio::test]
async fn invalid_sql_is_a_statement_failure() {
    let all = common::databases().await;
    for t in &all {
        for sql in ["SELEC 1", "SELECT * FROM \"missing\"", "INSERT INTO", ")"] {
            let q = t.db.query(sql, &[]).await;
            assert!(
                matches!(q, Err(DbError::Statement(_))),
                "{}: {sql}: {q:?}",
                t.name
            );
        }
    }
    finish(all).await;
}

// Binding fewer parameters than the statement uses is an error, never a panic.
#[tokio::test]
async fn too_few_parameters_is_an_error() {
    let all = migrated().await;
    for t in &all {
        let sql = render(&insert_user(), t.db.dialect());
        let r = t.db.execute(&sql.text, &[text(&uuid(1))]).await;
        assert!(r.is_err(), "{}: {r:?}", t.name);
    }
    finish(all).await;
}

// Binding more parameters than the statement uses is an error, never a panic.
#[tokio::test]
async fn too_many_parameters_is_an_error() {
    let all = migrated().await;
    for t in &all {
        let sql = render(&insert_user(), t.db.dialect());
        let mut values = user(1, "a@example.com", None);
        values.push(text("extra"));
        let r = t.db.execute(&sql.text, &values).await;
        assert!(r.is_err(), "{}: {r:?}", t.name);
    }
    finish(all).await;
}

// The empty statement doesn't panic either.
#[tokio::test]
async fn an_empty_statement_does_not_panic() {
    let all = common::databases().await;
    for t in &all {
        let _ = t.db.execute("", &[]).await;
        let _ = t.db.query("", &[]).await;
        assert!(
            t.db.ping(std::time::Duration::from_secs(2)).await,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A text value containing NUL never panics (Postgres refuses it; SQLite stores it).
#[tokio::test]
async fn a_nul_character_never_panics() {
    let all = migrated().await;
    for t in &all {
        let r = try_insert(&t.db, &user(1, "a\0b@example.com", None)).await;
        if t.db.dialect() == Dialect::Sqlite {
            assert_eq!(r, Ok(1), "{}", t.name);
        }
    }
    finish(all).await;
}

// An unknown double-quoted identifier is an error on SQLite too, not a string literal, so a
// wrong locked name can never silently compare against its own spelling.
#[tokio::test]
async fn unknown_quoted_identifiers_are_errors() {
    let all = migrated().await;
    for t in &all {
        let q =
            t.db.query("SELECT \"no_such_column\" FROM \"note\"", &[])
                .await;
        assert!(q.is_err(), "{}: {q:?}", t.name);
        let n =
            t.db.query(
                "SELECT COUNT(*) FROM \"note\" WHERE \"titel\" = 'titel'",
                &[],
            )
            .await;
        assert!(n.is_err(), "{}: {n:?}", t.name);
    }
    finish(all).await;
}

// Error messages never contain the values that were bound, which may be secrets or PII.
#[tokio::test]
async fn error_messages_do_not_echo_bound_values() {
    let all = migrated().await;
    for t in &all {
        let secret = "sk_live_SECRET_VALUE";
        try_insert(&t.db, &user(1, secret, None))
            .await
            .expect(t.name);
        let dup = try_insert(&t.db, &user(2, secret, None))
            .await
            .expect_err(t.name);
        assert!(!dup.to_string().contains(secret), "{}: {dup}", t.name);
        assert!(!format!("{dup:?}").contains(secret), "{}: {dup:?}", t.name);
        let mut missing = user(3, "c@example.com", Some(secret));
        missing[3] = Value::Null;
        let null = try_insert(&t.db, &missing).await.expect_err(t.name);
        assert!(!null.to_string().contains(secret), "{}: {null}", t.name);
    }
    finish(all).await;
}

// --- connections -----------------------------------------------------------

// An unreachable Postgres is a Connection error whose message hides the password.
#[tokio::test]
async fn an_unreachable_server_is_a_connection_error() {
    let err = Database::connect(
        "postgres://k:hunter2@127.0.0.1:1/k?connect_timeout=2",
        &DatabaseOptions::default(),
    )
    .await
    .expect_err("nothing listens on port 1");
    assert!(matches!(err, DbError::Connection(_)), "{err:?}");
    assert!(!err.to_string().contains("hunter2"), "{err}");
}

// A password given separately (`DATABASE_PASSWORD`) is not echoed when connecting fails.
#[tokio::test]
async fn a_separate_password_is_not_echoed() {
    let options = DatabaseOptions {
        password: Some("s3cr3t-from-file".into()),
    };
    let err = Database::connect("postgres://k@127.0.0.1:1/k?connect_timeout=2", &options)
        .await
        .expect_err("refused");
    assert!(!err.to_string().contains("s3cr3t-from-file"), "{err}");
    assert!(!format!("{err:?}").contains("s3cr3t-from-file"), "{err:?}");
}

// Unknown or malformed URLs are InvalidUrl, whatever their shape.
#[tokio::test]
async fn malformed_urls_are_invalid() {
    for url in [
        "",
        "sqlite",
        "SQLITE::memory:",
        "postgres:/x",
        "mysql://h/db",
        "  sqlite::memory:",
    ] {
        let r = Database::connect(url, &DatabaseOptions::default()).await;
        assert!(matches!(r, Err(DbError::InvalidUrl(_))), "{url:?}: {r:?}");
    }
}

// `DATABASE_PASSWORD` overrides a wrong password in the URL (Postgres only).
#[tokio::test]
async fn the_password_option_overrides_the_url() {
    let Ok(url) = std::env::var(k_db::testing::POSTGRES_URL_ENV) else {
        return;
    };
    let Some((scheme_user, rest)) = url.split_once('@') else {
        return;
    };
    let Some((prefix, password)) = scheme_user.rsplit_once(':') else {
        return;
    };
    if prefix.ends_with('/') {
        return; // no password in the URL to replace
    }
    let wrong = format!("{prefix}:wrong@{rest}");
    let options = DatabaseOptions {
        password: Some(password.to_owned()),
    };
    let db = Database::connect(&wrong, &options)
        .await
        .expect("the option wins");
    assert!(db.ping(std::time::Duration::from_secs(2)).await);
}

// Each error variant displays a stable, human-readable message.
#[test]
fn error_display() {
    assert_eq!(
        DbError::InvalidUrl("x".into()).to_string(),
        "invalid database URL: x"
    );
    assert_eq!(
        DbError::Connection("x".into()).to_string(),
        "database connection failed: x"
    );
    assert_eq!(
        DbError::Statement("x".into()).to_string(),
        "statement failed: x"
    );
    assert_eq!(
        DbError::UnsupportedType("money".into()).to_string(),
        "unsupported value type: money"
    );
    assert_eq!(
        DbError::Migration {
            name: "0002_x".into(),
            message: "boom".into()
        }
        .to_string(),
        "migration 0002_x failed: boom"
    );
    assert_eq!(
        violation(Some("user"), &["email"], Some("user_email_key")).to_string(),
        "unique violation on user (email) [user_email_key]"
    );
    assert_eq!(violation(None, &[], None).to_string(), "unique violation");
}

// A Postgres int4 column refuses an Int that doesn't fit with an error, not a wraparound.
#[tokio::test]
async fn narrowing_overflow_is_an_error() {
    let all = common::databases().await;
    for t in all.iter().filter(|t| t.db.dialect() == Dialect::Postgres) {
        t.db.execute("CREATE TABLE \"small\" (\"n\" integer)", &[])
            .await
            .expect(t.name);
        let r =
            t.db.execute(
                "INSERT INTO \"small\" (\"n\") VALUES ($1)",
                &[Value::Int(1 << 40)],
            )
            .await;
        assert!(r.is_err(), "{r:?}");
        assert_eq!(common::table_count(&t.db, "small").await, Some(0));
    }
    finish(all).await;
}
