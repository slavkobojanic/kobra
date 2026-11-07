//! Transactions (`docs/compiler/db.md` §9, `docs/design/19-persistence.md`,
//! "`Database` and transactions"): the statement protocol the server
//! runtime uses for `database.transaction { … }` (`BEGIN`, `COMMIT`,
//! `ROLLBACK`), nesting through savepoints (`SAVEPOINT k_sp_<n>`,
//! `RELEASE`, `ROLLBACK TO`), recovering from a caught unique violation, the
//! `k test` isolation scheme, and SQLite file databases for `k dev`.

mod common;

use std::sync::atomic::{AtomicU32, Ordering};

use common::{finish, insert_notes, note_values, run, seeded, table_count, text, uuid};
use k_db::ir::{Operand, Statement};
use k_db::sql::render;
use k_db::{Database, DatabaseOptions, DbError, Dialect, Value};

async fn exec(db: &Database, sql: &str) {
    db.execute(sql, &[])
        .await
        .map_err(|e| format!("{sql}: {e}"))
        .expect("statement");
}

async fn add_note(db: &Database, n: u32) {
    run(
        db,
        &insert_notes(1),
        &note_values(n, &format!("note {n}"), false, 1, None),
    )
    .await;
}

async fn notes(db: &Database) -> i64 {
    table_count(db, "note").await.unwrap_or(-1)
}

static NEXT_FILE: AtomicU32 = AtomicU32::new(0);

/// A fresh SQLite file path under the system temp directory, and a guard
/// that removes it (and its journal) when dropped.
struct TempFile(std::path::PathBuf);

impl TempFile {
    fn new() -> TempFile {
        let n = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        TempFile(std::env::temp_dir().join(format!("k_db_test_{}_{n}.sqlite", std::process::id())))
    }

    fn url(&self) -> String {
        format!("sqlite:{}", self.0.display())
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let mut p = self.0.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}

// `database.transaction { … }` that returns normally commits its writes.
#[tokio::test]
async fn a_committed_transaction_keeps_its_writes() {
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        add_note(&t.db, 6).await;
        add_note(&t.db, 7).await;
        exec(&t.db, "COMMIT").await;
        assert_eq!(notes(&t.db).await, 7, "{}", t.name);
    }
    finish(all).await;
}

// A block that throws rolls back every write: inserts, updates and deletes.
#[tokio::test]
async fn a_rolled_back_transaction_discards_its_writes() {
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        add_note(&t.db, 6).await;
        let rename = Statement::Update {
            table: "note".into(),
            set: vec![("title".into(), Operand::Param(1))],
            filter: None,
        };
        run(&t.db, &rename, &[text("gone")]).await;
        let delete = Statement::Delete {
            table: "note".into(),
            filter: Some(common::eq("id", 1)),
        };
        run(&t.db, &delete, &[text(&uuid(1))]).await;
        exec(&t.db, "ROLLBACK").await;
        assert_eq!(notes(&t.db).await, 5, "{}", t.name);
        let first = common::rows(
            &t.db,
            &common::notes_where(common::eq("id", 1)),
            &[text(&uuid(1))],
        )
        .await;
        assert_eq!(first[0].values[1], text("Alpha plan"), "{}", t.name);
    }
    finish(all).await;
}

// Reads inside a transaction see its own uncommitted writes.
#[tokio::test]
async fn a_transaction_reads_its_own_writes() {
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        add_note(&t.db, 6).await;
        assert_eq!(notes(&t.db).await, 6, "{}", t.name);
        exec(&t.db, "ROLLBACK").await;
    }
    finish(all).await;
}

// A nested `transaction` is a savepoint: an inner throw rolls back to it, and if the outer
// code catches and carries on, the outer writes remain.
#[tokio::test]
async fn an_inner_rollback_keeps_the_outer_writes() {
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        add_note(&t.db, 6).await;
        exec(&t.db, "SAVEPOINT \"k_sp_1\"").await;
        add_note(&t.db, 7).await;
        exec(&t.db, "ROLLBACK TO SAVEPOINT \"k_sp_1\"").await;
        exec(&t.db, "RELEASE SAVEPOINT \"k_sp_1\"").await;
        exec(&t.db, "COMMIT").await;
        assert_eq!(
            notes(&t.db).await,
            6,
            "{}: note 6 stays, note 7 is gone",
            t.name
        );
    }
    finish(all).await;
}

// An inner block that completes releases its savepoint; an outer throw still discards it.
#[tokio::test]
async fn an_outer_rollback_discards_released_inner_writes() {
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        exec(&t.db, "SAVEPOINT \"k_sp_1\"").await;
        add_note(&t.db, 6).await;
        exec(&t.db, "RELEASE SAVEPOINT \"k_sp_1\"").await;
        exec(&t.db, "ROLLBACK").await;
        assert_eq!(notes(&t.db).await, 5, "{}", t.name);
    }
    finish(all).await;
}

// Three levels: rolling back the middle level discards it and the innermost, not the outer.
#[tokio::test]
async fn deep_nesting_rolls_back_to_the_right_level() {
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        add_note(&t.db, 6).await;
        exec(&t.db, "SAVEPOINT \"k_sp_1\"").await;
        add_note(&t.db, 7).await;
        exec(&t.db, "SAVEPOINT \"k_sp_2\"").await;
        add_note(&t.db, 8).await;
        exec(&t.db, "RELEASE SAVEPOINT \"k_sp_2\"").await;
        exec(&t.db, "ROLLBACK TO SAVEPOINT \"k_sp_1\"").await;
        exec(&t.db, "RELEASE SAVEPOINT \"k_sp_1\"").await;
        add_note(&t.db, 9).await;
        exec(&t.db, "COMMIT").await;
        let ids = common::sorted_ids(&t.db, &Statement::Select(common::note_select()), &[]).await;
        assert_eq!(ids, [1, 2, 3, 4, 5, 6, 9], "{}", t.name);
    }
    finish(all).await;
}

// `do { try await users.insert(u) } catch .email { … }` inside a transaction: the throwing
// write runs under a savepoint, so after the caught violation the transaction carries on
// and commits (Postgres would otherwise abort the whole transaction).
#[tokio::test]
async fn a_caught_unique_violation_does_not_poison_the_transaction() {
    let insert_user = Statement::Insert {
        table: "user".into(),
        columns: vec!["id".into(), "email".into(), "handle".into(), "name".into()],
        rows: 1,
    };
    let user = |n: u32| {
        vec![
            text(&uuid(n)),
            text("same@example.com"),
            Value::Null,
            text("N"),
        ]
    };
    let all = common::migrated().await;
    for t in &all {
        let sql = render(&insert_user, t.db.dialect()).text;
        exec(&t.db, "BEGIN").await;
        t.db.execute(&sql, &user(1)).await.expect(t.name);
        exec(&t.db, "SAVEPOINT \"k_sp_1\"").await;
        let err = t.db.execute(&sql, &user(2)).await.expect_err(t.name);
        assert!(matches!(err, DbError::UniqueViolation { .. }), "{}", t.name);
        exec(&t.db, "ROLLBACK TO SAVEPOINT \"k_sp_1\"").await;
        exec(&t.db, "RELEASE SAVEPOINT \"k_sp_1\"").await;
        add_note(&t.db, 1).await;
        exec(&t.db, "COMMIT").await;
        assert_eq!(table_count(&t.db, "user").await, Some(1), "{}", t.name);
        assert_eq!(notes(&t.db).await, 1, "{}", t.name);
    }
    finish(all).await;
}

// Why the savepoint is needed: after a failed statement, Postgres refuses everything else in
// the transaction, while SQLite only undoes the failed statement.
#[tokio::test]
async fn a_failed_statement_aborts_a_postgres_transaction() {
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        assert!(
            t.db.execute("INSERT INTO \"missing\" VALUES (1)", &[])
                .await
                .is_err()
        );
        let next = t.db.query("SELECT 1", &[]).await;
        match t.db.dialect() {
            Dialect::Postgres => assert!(next.is_err(), "the transaction is aborted"),
            Dialect::Sqlite => assert!(next.is_ok(), "{next:?}"),
        }
        exec(&t.db, "ROLLBACK").await;
        assert!(
            t.db.query("SELECT 1", &[]).await.is_ok(),
            "{}: usable again",
            t.name
        );
    }
    finish(all).await;
}

// SQLite write transactions start with `BEGIN IMMEDIATE` (one writer at a time).
#[tokio::test]
async fn sqlite_write_transactions_begin_immediate() {
    let all = seeded().await;
    for t in all.iter().filter(|t| t.db.dialect() == Dialect::Sqlite) {
        exec(&t.db, "BEGIN IMMEDIATE").await;
        add_note(&t.db, 6).await;
        exec(&t.db, "COMMIT").await;
        assert_eq!(notes(&t.db).await, 6);
    }
    finish(all).await;
}

// Postgres transactions run at READ COMMITTED.
#[tokio::test]
async fn postgres_transactions_are_read_committed() {
    let all = common::databases().await;
    for t in all.iter().filter(|t| t.db.dialect() == Dialect::Postgres) {
        exec(&t.db, "BEGIN ISOLATION LEVEL READ COMMITTED").await;
        let r =
            t.db.query("SHOW transaction_isolation", &[])
                .await
                .expect(t.name);
        assert_eq!(r[0].values[0], text("read committed"));
        exec(&t.db, "COMMIT").await;
    }
    finish(all).await;
}

// Schema changes are transactional on both databases.
#[tokio::test]
async fn ddl_rolls_back_with_the_transaction() {
    let all = common::databases().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        exec(&t.db, "CREATE TABLE \"scratch\" (\"x\" TEXT)").await;
        exec(&t.db, "ROLLBACK").await;
        assert_eq!(table_count(&t.db, "scratch").await, None, "{}", t.name);
    }
    finish(all).await;
}

// A versioned update rolled back with its transaction leaves the old revision.
#[tokio::test]
async fn a_rolled_back_versioned_update_keeps_the_revision() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("revision".into(), Operand::Param(1))],
        filter: Some(k_db::ir::Predicate::And(vec![
            common::eq("id", 2),
            common::eq("revision", 3),
        ])),
    };
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await;
        assert_eq!(
            run(&t.db, &st, &[Value::Int(2), text(&uuid(1)), Value::Int(1)]).await,
            1
        );
        exec(&t.db, "ROLLBACK").await;
        assert_eq!(
            run(&t.db, &st, &[Value::Int(2), text(&uuid(1)), Value::Int(1)]).await,
            1,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Ending a transaction that isn't open is an error or a warning, never a panic.
#[tokio::test]
async fn ending_a_transaction_that_is_not_open_does_not_panic() {
    let all = common::databases().await;
    for t in &all {
        let _ = t.db.execute("ROLLBACK", &[]).await;
        let _ = t.db.execute("COMMIT", &[]).await;
        let _ = t.db.execute("RELEASE SAVEPOINT \"k_sp_9\"", &[]).await;
        assert!(
            t.db.ping(std::time::Duration::from_secs(2)).await,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `k test`: each test runs in an outer transaction the app can't see; the app's outermost
// transaction is a savepoint inside it, its "commit" releases the savepoint, and the final
// rollback discards everything.
#[tokio::test]
async fn the_k_test_isolation_scheme() {
    let all = seeded().await;
    for t in &all {
        exec(&t.db, "BEGIN").await; // the test's transaction
        exec(&t.db, "SAVEPOINT \"k_sp_1\"").await; // app: database.transaction {
        add_note(&t.db, 6).await;
        exec(&t.db, "RELEASE SAVEPOINT \"k_sp_1\"").await; // app: } commits
        assert_eq!(notes(&t.db).await, 6, "{}: visible to the test", t.name);
        exec(&t.db, "ROLLBACK").await; // the test ends
        assert_eq!(notes(&t.db).await, 5, "{}: gone for the next test", t.name);
    }
    finish(all).await;
}

// SQLite file databases (`k dev`): writes outlive the connection.
#[tokio::test]
async fn a_sqlite_file_persists_across_connections() {
    let file = TempFile::new();
    {
        let db = Database::connect(&file.url(), &DatabaseOptions::default())
            .await
            .expect("open");
        k_db::migrate::apply(&db, &[common::init_migration()])
            .await
            .expect("migrate");
        add_note(&db, 1).await;
    }
    let db = Database::connect(&file.url(), &DatabaseOptions::default())
        .await
        .expect("reopen");
    assert_eq!(notes(&db).await, 1);
    assert_eq!(
        k_db::migrate::applied(&db).await.expect("applied"),
        ["0001_init"]
    );
}

// Another connection to the same SQLite file doesn't see uncommitted writes, then does.
#[tokio::test]
async fn uncommitted_writes_are_invisible_to_other_connections() {
    let file = TempFile::new();
    let writer = Database::connect(&file.url(), &DatabaseOptions::default())
        .await
        .expect("open");
    k_db::migrate::apply(&writer, &[common::init_migration()])
        .await
        .expect("migrate");
    let reader = Database::connect(&file.url(), &DatabaseOptions::default())
        .await
        .expect("open");
    exec(&writer, "BEGIN IMMEDIATE").await;
    add_note(&writer, 1).await;
    assert_eq!(notes(&reader).await, 0);
    exec(&writer, "COMMIT").await;
    assert_eq!(notes(&reader).await, 1);
}

// A SQLite path in a directory that doesn't exist is a Connection error, not a panic.
#[tokio::test]
async fn a_sqlite_path_in_a_missing_directory_is_an_error() {
    let url = format!(
        "sqlite:{}",
        std::env::temp_dir()
            .join("k_db_no_such_dir_x9")
            .join("db.sqlite")
            .display()
    );
    let r = Database::connect(&url, &DatabaseOptions::default()).await;
    assert!(matches!(r, Err(DbError::Connection(_))), "{r:?}");
}

// Foreign keys are enforced on SQLite file connections too (the pragma is per connection).
#[tokio::test]
async fn sqlite_file_connections_enforce_foreign_keys() {
    let file = TempFile::new();
    let db = Database::connect(&file.url(), &DatabaseOptions::default())
        .await
        .expect("open");
    k_db::migrate::apply(&db, &[common::init_migration()])
        .await
        .expect("migrate");
    let r = db
        .execute(
            "INSERT INTO \"tag\" (\"id\", \"note_id\", \"name\") VALUES (?1, ?2, ?3)",
            &[text(&uuid(1)), text(&uuid(99)), text("orphan")],
        )
        .await;
    assert!(matches!(r, Err(DbError::Statement(_))), "{r:?}");
}
