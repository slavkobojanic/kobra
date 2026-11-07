//! The drivers and migrations against every available database: SQLite
//! always, Postgres when `K_TEST_POSTGRES_URL` is set
//! (`docs/compiler/testing.md`, "Database tests").

use k_db::ir::{CmpOp, Operand, Predicate, Select, Statement};
use k_db::migrate::{self, Migration};
use k_db::schema::{Column, ColumnType, Schema, Table};
use k_db::sql::render;
use k_db::testing::test_databases;
use k_db::{Database, DatabaseOptions, DbError, Value};

fn user_table() -> Table {
    let column = |name: &str, ty: ColumnType| Column {
        field: Some(name.to_owned()),
        name: name.to_owned(),
        ty,
        primary_key: false,
        nullable: false,
        unique: false,
        indexed: false,
        references: None,
    };
    Table {
        entity: "accounts.User".into(),
        name: "user".into(),
        columns: vec![
            Column {
                primary_key: true,
                ..column("id", ColumnType::Uuid)
            },
            Column {
                unique: true,
                ..column("email", ColumnType::Text)
            },
            column("age", ColumnType::Integer),
            column("is_admin", ColumnType::Boolean),
            Column {
                nullable: true,
                ..column("nickname", ColumnType::Text)
            },
            column(
                "status",
                ColumnType::Enumeration(vec!["active".into(), "it's".into()]),
            ),
        ],
    }
}

fn migration(table: &Table) -> Migration {
    Migration {
        name: "0001_init".into(),
        sqlite: table.create_sql(k_db::Dialect::Sqlite),
        postgres: table.create_sql(k_db::Dialect::Postgres),
    }
}

const ID_1: &str = "0b6f3f9e-1c2d-4e5f-8a9b-0c1d2e3f4a5b";
const ID_2: &str = "1b6f3f9e-1c2d-4e5f-8a9b-0c1d2e3f4a5b";

fn insert() -> Statement {
    Statement::Insert {
        table: "user".into(),
        columns: vec![
            "id".into(),
            "email".into(),
            "age".into(),
            "is_admin".into(),
            "nickname".into(),
            "status".into(),
        ],
        rows: 1,
    }
}

fn row(id: &str, email: &str) -> Vec<Value> {
    vec![
        Value::Text(id.into()),
        Value::Text(email.into()),
        Value::Int(42),
        Value::Bool(true),
        Value::Null,
        Value::Text("active".into()),
    ]
}

#[tokio::test]
async fn migrations_apply_once_and_are_recorded() {
    for t in test_databases().await.expect("databases") {
        let m = migration(&user_table());
        let first = migrate::apply(&t.db, std::slice::from_ref(&m))
            .await
            .expect(t.name);
        assert_eq!(first, vec!["0001_init".to_owned()], "{}", t.name);
        let second = migrate::apply(&t.db, std::slice::from_ref(&m))
            .await
            .expect(t.name);
        assert!(second.is_empty(), "{}", t.name);
        assert_eq!(
            migrate::applied(&t.db).await.expect(t.name),
            vec!["0001_init".to_owned()]
        );
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn a_failing_migration_rolls_back_and_names_itself() {
    for t in test_databases().await.expect("databases") {
        let bad = Migration {
            name: "0001_bad".into(),
            sqlite: vec![
                "CREATE TABLE \"a\" (\"x\" TEXT)".into(),
                "THIS IS NOT SQL".into(),
            ],
            postgres: vec![
                "CREATE TABLE \"a\" (\"x\" text)".into(),
                "THIS IS NOT SQL".into(),
            ],
        };
        let err = migrate::apply(&t.db, &[bad]).await.expect_err(t.name);
        assert!(
            matches!(&err, DbError::Migration { name, .. } if name == "0001_bad"),
            "{}: {err}",
            t.name
        );
        assert!(
            migrate::applied(&t.db).await.expect(t.name).is_empty(),
            "{}",
            t.name
        );
        let count = t.db.query("SELECT COUNT(*) FROM \"a\"", &[]).await;
        assert!(count.is_err(), "{}: table a was rolled back", t.name);
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn a_history_the_build_lacks_is_refused() {
    for t in test_databases().await.expect("databases") {
        migrate::apply(&t.db, &[migration(&user_table())])
            .await
            .expect(t.name);
        let err = migrate::apply(&t.db, &[]).await.expect_err(t.name);
        assert!(matches!(err, DbError::Migration { .. }), "{}", t.name);
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn rows_round_trip_through_parameters() {
    for t in test_databases().await.expect("databases") {
        migrate::apply(&t.db, &[migration(&user_table())])
            .await
            .expect(t.name);
        let ins = render(&insert(), t.db.dialect());
        assert_eq!(
            t.db.execute(&ins.text, &row(ID_1, "ana@example.com"))
                .await
                .expect(t.name),
            1,
            "{}",
            t.name
        );
        let q = Statement::Select(Select {
            table: "user".into(),
            columns: vec!["id".into(), "email".into(), "age".into(), "nickname".into()],
            filter: Some(Predicate::Compare {
                left: Operand::Column("email".into()),
                op: CmpOp::Eq,
                right: Operand::Param(1),
            }),
            order: vec![],
            limit: None,
            offset: None,
        });
        let sql = render(&q, t.db.dialect());
        let rows =
            t.db.query(&sql.text, &[Value::Text("ana@example.com".into())])
                .await
                .expect(t.name);
        assert_eq!(rows.len(), 1, "{}", t.name);
        assert_eq!(
            rows[0].get("id"),
            Some(&Value::Text(ID_1.into())),
            "{}",
            t.name
        );
        assert_eq!(rows[0].get("age"), Some(&Value::Int(42)), "{}", t.name);
        assert_eq!(rows[0].get("nickname"), Some(&Value::Null), "{}", t.name);
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn injection_attempts_are_just_values() {
    for t in test_databases().await.expect("databases") {
        migrate::apply(&t.db, &[migration(&user_table())])
            .await
            .expect(t.name);
        let ins = render(&insert(), t.db.dialect());
        let evil = "x'); DROP TABLE \"user\"; --";
        t.db.execute(&ins.text, &row(ID_1, evil))
            .await
            .expect(t.name);
        let rows =
            t.db.query("SELECT \"email\" FROM \"user\"", &[])
                .await
                .expect(t.name);
        assert_eq!(
            rows[0].get("email"),
            Some(&Value::Text(evil.into())),
            "{}",
            t.name
        );
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn duplicates_map_to_the_unique_field() {
    for t in test_databases().await.expect("databases") {
        let table = user_table();
        migrate::apply(&t.db, &[migration(&table)])
            .await
            .expect(t.name);
        let ins = render(&insert(), t.db.dialect());
        t.db.execute(&ins.text, &row(ID_1, "ana@example.com"))
            .await
            .expect(t.name);
        let err =
            t.db.execute(&ins.text, &row(ID_2, "ana@example.com"))
                .await
                .expect_err(t.name);
        assert!(
            matches!(err, DbError::UniqueViolation { .. }),
            "{}: {err}",
            t.name
        );
        let schema = Schema {
            tables: vec![table],
        };
        assert_eq!(
            schema.unique_field(&err),
            Some(("accounts.User", "email")),
            "{}: {err:?}",
            t.name
        );
        let pk =
            t.db.execute(&ins.text, &row(ID_1, "ben@example.com"))
                .await
                .expect_err(t.name);
        assert!(
            matches!(pk, DbError::UniqueViolation { .. }),
            "{}: a duplicate primary key is a unique violation too",
            t.name
        );
        assert_eq!(
            schema.unique_field(&pk),
            None,
            "{}: but not of a @Unique field",
            t.name
        );
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn enumeration_columns_reject_unknown_cases() {
    for t in test_databases().await.expect("databases") {
        migrate::apply(&t.db, &[migration(&user_table())])
            .await
            .expect(t.name);
        let ins = render(&insert(), t.db.dialect());
        let mut values = row(ID_1, "a@example.com");
        values[5] = Value::Text("it's".into());
        t.db.execute(&ins.text, &values).await.expect(t.name);
        let mut bad = row(ID_2, "b@example.com");
        bad[5] = Value::Text("nope".into());
        assert!(
            matches!(
                t.db.execute(&ins.text, &bad).await,
                Err(DbError::Statement(_))
            ),
            "{}",
            t.name
        );
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn like_is_case_sensitive_on_both_databases() {
    for t in test_databases().await.expect("databases") {
        migrate::apply(&t.db, &[migration(&user_table())])
            .await
            .expect(t.name);
        let ins = render(&insert(), t.db.dialect());
        t.db.execute(&ins.text, &row(ID_1, "Ana@example.com"))
            .await
            .expect(t.name);
        let q = Statement::Count(Select {
            table: "user".into(),
            columns: vec![],
            filter: Some(Predicate::Like {
                column: "email".into(),
                kind: k_db::ir::Like::StartsWith,
                param: 1,
            }),
            order: vec![],
            limit: None,
            offset: None,
        });
        let sql = render(&q, t.db.dialect());
        let pattern = |s: &str| {
            vec![Value::Text(k_db::sql::like_pattern(
                k_db::ir::Like::StartsWith,
                s,
            ))]
        };
        let count = |rows: Vec<k_db::Row>| rows[0].values[0].as_int();
        assert_eq!(
            count(t.db.query(&sql.text, &pattern("Ana")).await.expect(t.name)),
            Some(1),
            "{}",
            t.name
        );
        assert_eq!(
            count(t.db.query(&sql.text, &pattern("ana")).await.expect(t.name)),
            Some(0),
            "{}",
            t.name
        );
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn ping_answers_true_on_a_live_database() {
    for t in test_databases().await.expect("databases") {
        assert!(
            t.db.ping(std::time::Duration::from_secs(2)).await,
            "{}",
            t.name
        );
        t.finish().await.expect("cleanup");
    }
}

#[tokio::test]
async fn bad_urls_are_refused_without_echoing_them() {
    let err = Database::connect("mysql://root:hunter2@db/x", &DatabaseOptions::default())
        .await
        .expect_err("refused");
    assert!(
        matches!(&err, DbError::InvalidUrl(m) if !m.contains("hunter2")),
        "{err}"
    );
    let err = Database::connect("postgres://u:hunter2@[bad", &DatabaseOptions::default())
        .await
        .expect_err("refused");
    assert!(!err.to_string().contains("hunter2"), "{err}");
}

#[tokio::test]
async fn database_debug_output_hides_connection_details() {
    let db = Database::connect("sqlite::memory:", &DatabaseOptions::default())
        .await
        .expect("sqlite");
    assert_eq!(format!("{db:?}"), "Database(Sqlite)");
}
