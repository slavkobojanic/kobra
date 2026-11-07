//! Migrations (`docs/compiler/db.md` §10, `docs/design/19-persistence.md`,
//! "Migrations"): order of application, idempotence, recording in
//! `k_migrations`, rollback of a failing migration, diverged histories and
//! concurrent starters, on every available database.

mod common;

use common::{
    create_migration, databases, finish, init_migration, note_table, raw_migration, table_count,
    text,
};
use k_db::migrate::{self, MIGRATIONS_TABLE, Migration};
use k_db::{DbError, Dialect, Value};

/// `0002_add_archived`: adds a nullable column to `note`.
fn add_archived() -> Migration {
    raw_migration(
        "0002_add_archived",
        &["ALTER TABLE \"note\" ADD COLUMN \"archived\" TEXT"],
    )
}

/// `0003_seed_categories`: a data migration with reference rows.
fn seed_categories() -> Migration {
    raw_migration(
        "0003_seed_categories",
        &[
            "CREATE TABLE \"category\" (\"name\" TEXT PRIMARY KEY)",
            "INSERT INTO \"category\" (\"name\") VALUES ('home'), ('work')",
        ],
    )
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// Whether `table` exists on `db` (a count succeeds).
async fn exists(db: &k_db::Database, table: &str) -> bool {
    table_count(db, table).await.is_some()
}

// A fresh database has no applied migrations, and asking creates the bookkeeping table.
#[tokio::test]
async fn a_fresh_database_has_applied_nothing() {
    let all = databases().await;
    for t in &all {
        assert!(
            migrate::applied(&t.db).await.expect(t.name).is_empty(),
            "{}",
            t.name
        );
        assert_eq!(
            table_count(&t.db, MIGRATIONS_TABLE).await,
            Some(0),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Applying an empty build is a no-op that still succeeds.
#[tokio::test]
async fn applying_no_migrations_is_a_no_op() {
    let all = databases().await;
    for t in &all {
        assert!(
            migrate::apply(&t.db, &[]).await.expect(t.name).is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// The bookkeeping table is the reserved `k_migrations`.
#[test]
fn the_bookkeeping_table_is_reserved() {
    assert_eq!(MIGRATIONS_TABLE, "k_migrations");
    assert!(MIGRATIONS_TABLE.starts_with("k_"));
}

// A migration picks the statements of the connected dialect.
#[test]
fn statements_are_chosen_by_dialect() {
    let m = Migration {
        name: "0001_x".into(),
        sqlite: vec!["S".into()],
        postgres: vec!["P".into(), "Q".into()],
    };
    assert_eq!(m.statements(Dialect::Sqlite), ["S"]);
    assert_eq!(m.statements(Dialect::Postgres), ["P", "Q"]);
}

// Migrations run in build order; a later one may depend on an earlier one's table.
#[tokio::test]
async fn migrations_apply_in_order() {
    let all = databases().await;
    let build = [init_migration(), add_archived(), seed_categories()];
    for t in &all {
        let ran = migrate::apply(&t.db, &build).await.expect(t.name);
        assert_eq!(
            ran,
            names(&["0001_init", "0002_add_archived", "0003_seed_categories"]),
            "{}",
            t.name
        );
        assert_eq!(
            migrate::applied(&t.db).await.expect(t.name),
            ran,
            "{}",
            t.name
        );
        assert_eq!(table_count(&t.db, "category").await, Some(2), "{}", t.name);
    }
    finish(all).await;
}

// Applying the same build twice runs nothing the second time.
#[tokio::test]
async fn applying_twice_is_idempotent() {
    let all = databases().await;
    let build = [init_migration(), add_archived(), seed_categories()];
    for t in &all {
        migrate::apply(&t.db, &build).await.expect(t.name);
        assert!(
            migrate::apply(&t.db, &build)
                .await
                .expect(t.name)
                .is_empty(),
            "{}",
            t.name
        );
        // The data migration's rows were not inserted twice.
        assert_eq!(table_count(&t.db, "category").await, Some(2), "{}", t.name);
        assert_eq!(
            table_count(&t.db, MIGRATIONS_TABLE).await,
            Some(3),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A later release applies only the migrations added since the last start.
#[tokio::test]
async fn a_new_release_applies_only_pending_migrations() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration()])
            .await
            .expect(t.name);
        let ran = migrate::apply(&t.db, &[init_migration(), add_archived()])
            .await
            .expect(t.name);
        assert_eq!(ran, names(&["0002_add_archived"]), "{}", t.name);
    }
    finish(all).await;
}

// A migration with no statements (an empty `up`) is still recorded.
#[tokio::test]
async fn an_empty_migration_is_recorded() {
    let all = databases().await;
    for t in &all {
        let ran = migrate::apply(&t.db, &[raw_migration("0001_empty", &[])])
            .await
            .expect(t.name);
        assert_eq!(ran, names(&["0001_empty"]), "{}", t.name);
        assert_eq!(
            migrate::applied(&t.db).await.expect(t.name),
            ran,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Each recorded migration has an application time.
#[tokio::test]
async fn recorded_migrations_have_an_applied_at() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration()])
            .await
            .expect(t.name);
        let n = common::scalar(
            &t.db,
            "SELECT COUNT(*) FROM \"k_migrations\" WHERE \"applied_at\" IS NOT NULL",
            &[],
        )
        .await;
        assert_eq!(n, Some(1), "{}", t.name);
    }
    finish(all).await;
}

// A migration name is bound as a parameter, so quotes in it are stored verbatim.
#[tokio::test]
async fn migration_names_are_parameters() {
    let all = databases().await;
    let name = "0001_it's \"quoted\"; DROP TABLE k_migrations; --";
    for t in &all {
        migrate::apply(&t.db, &[raw_migration(name, &[])])
            .await
            .expect(t.name);
        assert_eq!(
            migrate::applied(&t.db).await.expect(t.name),
            names(&[name]),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A failing statement rolls back the whole migration, DDL included, and names it.
#[tokio::test]
async fn a_failing_migration_leaves_nothing_behind() {
    let all = databases().await;
    let bad = raw_migration(
        "0001_bad",
        &[
            "CREATE TABLE \"half\" (\"x\" TEXT)",
            "INSERT INTO \"half\" (\"x\") VALUES ('row')",
            "INSERT INTO \"missing_table\" (\"x\") VALUES ('row')",
        ],
    );
    for t in &all {
        let err = migrate::apply(&t.db, std::slice::from_ref(&bad))
            .await
            .expect_err(t.name);
        assert!(
            matches!(&err, DbError::Migration { name, .. } if name == "0001_bad"),
            "{}: {err}",
            t.name
        );
        assert!(
            !exists(&t.db, "half").await,
            "{}: the table was rolled back",
            t.name
        );
        assert!(
            migrate::applied(&t.db).await.expect(t.name).is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// The failure's message names the migration, so `dist/app` can report it before opening its port.
#[tokio::test]
async fn a_failure_message_names_the_migration() {
    let all = databases().await;
    for t in &all {
        let err = migrate::apply(&t.db, &[raw_migration("0007_broken", &["NOT SQL"])])
            .await
            .expect_err(t.name);
        assert!(
            err.to_string()
                .starts_with("migration 0007_broken failed: "),
            "{}: {err}",
            t.name
        );
    }
    finish(all).await;
}

// When the second of three fails, the first stays applied and the third never runs.
#[tokio::test]
async fn a_failure_stops_at_the_failing_migration() {
    let all = databases().await;
    let build = [
        init_migration(),
        raw_migration("0002_bad", &["ALTER TABLE \"nope\" ADD COLUMN \"x\" TEXT"]),
        seed_categories(),
    ];
    for t in &all {
        let err = migrate::apply(&t.db, &build).await.expect_err(t.name);
        assert!(
            matches!(&err, DbError::Migration { name, .. } if name == "0002_bad"),
            "{}",
            t.name
        );
        assert_eq!(
            migrate::applied(&t.db).await.expect(t.name),
            names(&["0001_init"]),
            "{}",
            t.name
        );
        assert!(exists(&t.db, "note").await, "{}", t.name);
        assert!(
            !exists(&t.db, "category").await,
            "{}: 0003 never ran",
            t.name
        );
    }
    finish(all).await;
}

// After fixing a failed migration, the next start applies it and the ones after it.
#[tokio::test]
async fn a_fixed_migration_applies_on_the_next_start() {
    let all = databases().await;
    for t in &all {
        let broken = [
            init_migration(),
            raw_migration("0002_add_archived", &["NOT SQL"]),
        ];
        migrate::apply(&t.db, &broken).await.expect_err(t.name);
        let fixed = [init_migration(), add_archived(), seed_categories()];
        let ran = migrate::apply(&t.db, &fixed).await.expect(t.name);
        assert_eq!(
            ran,
            names(&["0002_add_archived", "0003_seed_categories"]),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A failing data step rolls back the schema change made earlier in the same migration.
#[tokio::test]
async fn a_failing_data_step_rolls_back_its_schema_change() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration()])
            .await
            .expect(t.name);
        let bad = raw_migration(
            "0002_bad_backfill",
            &[
                "ALTER TABLE \"note\" ADD COLUMN \"archived\" TEXT",
                // NOT NULL title: the backfill violates it.
                "INSERT INTO \"note\" (\"id\", \"title\") VALUES ('00000000-0000-4000-8000-000000000001', NULL)",
            ],
        );
        migrate::apply(&t.db, &[init_migration(), bad])
            .await
            .expect_err(t.name);
        assert!(
            !common::has_column(&t.db, "note", "archived").await,
            "{}: the added column was rolled back",
            t.name
        );
    }
    finish(all).await;
}

// A database that records a migration the build lacks is refused before anything runs.
#[tokio::test]
async fn a_diverged_history_is_refused_before_running_anything() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration(), add_archived()])
            .await
            .expect(t.name);
        // A build from another branch: 0002 differs, and it has a new 0003.
        let other = [
            init_migration(),
            raw_migration("0002_other_branch", &[]),
            seed_categories(),
        ];
        let err = migrate::apply(&t.db, &other).await.expect_err(t.name);
        assert!(
            matches!(&err, DbError::Migration { name, .. } if name == "0002_add_archived"),
            "{}: {err}",
            t.name
        );
        assert!(!exists(&t.db, "category").await, "{}: nothing ran", t.name);
    }
    finish(all).await;
}

// A migration added *before* an applied one (an out-of-order history) is refused, not applied last.
#[tokio::test]
async fn a_migration_inserted_before_applied_ones_is_refused() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration(), seed_categories()])
            .await
            .expect(t.name);
        let reordered = [init_migration(), add_archived(), seed_categories()];
        let result = migrate::apply(&t.db, &reordered).await;
        assert!(
            matches!(&result, Err(DbError::Migration { .. })),
            "{}: applied migrations must be a prefix of the build, got {result:?}",
            t.name
        );
        assert!(
            !common::has_column(&t.db, "note", "archived").await,
            "{}: 0002 was not applied after 0003",
            t.name
        );
    }
    finish(all).await;
}

// The applied list comes back in application order.
#[tokio::test]
async fn applied_lists_in_application_order() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration()])
            .await
            .expect(t.name);
        migrate::apply(&t.db, &[init_migration(), add_archived()])
            .await
            .expect(t.name);
        assert_eq!(
            migrate::applied(&t.db).await.expect(t.name),
            names(&["0001_init", "0002_add_archived"]),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A migration uses its own dialect's statements: the other dialect's would fail.
#[tokio::test]
async fn each_database_runs_its_own_dialect() {
    let m = Migration {
        name: "0001_dialects".into(),
        sqlite: vec!["CREATE TABLE \"d\" (\"x\" INTEGER) STRICT".into()],
        postgres: vec!["CREATE TABLE \"d\" (\"x\" bigint GENERATED ALWAYS AS IDENTITY)".into()],
    };
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, std::slice::from_ref(&m))
            .await
            .expect(t.name);
        assert!(exists(&t.db, "d").await, "{}", t.name);
    }
    finish(all).await;
}

// A column added with a default fills existing rows with it (expand without rewriting code).
#[tokio::test]
async fn an_added_column_with_a_default_fills_existing_rows() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration()])
            .await
            .expect(t.name);
        common::seed_notes(&t.db).await;
        let (ty, default) = match t.db.dialect() {
            Dialect::Sqlite => ("INTEGER", "0"),
            Dialect::Postgres => ("boolean", "false"),
        };
        let add = format!(
            "ALTER TABLE \"note\" ADD COLUMN \"is_archived\" {ty} NOT NULL DEFAULT {default}"
        );
        migrate::apply(
            &t.db,
            &[init_migration(), raw_migration("0002_archive", &[&add])],
        )
        .await
        .expect(t.name);
        let rows =
            t.db.query("SELECT \"is_archived\" FROM \"note\"", &[])
                .await
                .expect(t.name);
        assert_eq!(rows.len(), 5, "{}", t.name);
        assert!(
            rows.iter().all(|r| common::is_false(r.values.first())),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Migrations generated from the schema model create every fixture table and index.
#[tokio::test]
async fn generated_create_migrations_apply() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[create_migration("0001_notes", &[note_table()])])
            .await
            .expect(t.name);
        assert_eq!(table_count(&t.db, "note").await, Some(0), "{}", t.name);
    }
    finish(all).await;
}

// The migration row and the migration's own writes commit together.
#[tokio::test]
async fn a_migration_and_its_record_commit_together() {
    let all = databases().await;
    for t in &all {
        // The migration itself inserts the row the runner is about to insert,
        // so recording fails and the migration's table must roll back too.
        let sneaky = Migration {
            name: "0001_sneaky".into(),
            sqlite: vec![
                "CREATE TABLE \"s\" (\"x\" TEXT)".into(),
                "INSERT INTO \"k_migrations\" (\"name\") VALUES ('0001_sneaky')".into(),
            ],
            postgres: vec![
                "CREATE TABLE \"s\" (\"x\" text)".into(),
                "INSERT INTO \"k_migrations\" (\"name\") VALUES ('0001_sneaky')".into(),
            ],
        };
        migrate::apply(&t.db, &[sneaky]).await.expect_err(t.name);
        assert!(!exists(&t.db, "s").await, "{}", t.name);
        assert!(
            migrate::applied(&t.db).await.expect(t.name).is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Two replicas starting together against one Postgres apply each migration exactly once
// (`pg_advisory_lock`, db.md §10). Postgres only: needs two connections to one schema.
#[tokio::test]
async fn concurrent_starters_apply_each_migration_once() {
    let Ok(url) = std::env::var(k_db::testing::POSTGRES_URL_ENV) else {
        return;
    };
    if url.is_empty() {
        return;
    }
    let options = k_db::DatabaseOptions::default();
    let admin = k_db::Database::connect(&url, &options)
        .await
        .expect("postgres");
    let schema = format!("k_test_concurrent_{}", std::process::id());
    admin
        .execute(&format!("CREATE SCHEMA \"{schema}\""), &[])
        .await
        .expect("schema");
    let mut replicas = Vec::new();
    for _ in 0..4 {
        let db = k_db::Database::connect(&url, &options)
            .await
            .expect("replica");
        db.execute(&format!("SET search_path TO \"{schema}\""), &[])
            .await
            .expect("search path");
        replicas.push(db);
    }
    let slow = raw_migration(
        "0001_slow",
        &["CREATE TABLE \"slow\" (\"x\" text)", "SELECT pg_sleep(0.2)"],
    );
    let build = vec![
        slow,
        raw_migration("0002_next", &["CREATE TABLE \"next\" (\"x\" text)"]),
    ];
    let mut tasks = Vec::new();
    for db in replicas {
        let build = build.clone();
        tasks.push(tokio::spawn(
            async move { migrate::apply(&db, &build).await },
        ));
    }
    let mut applied = Vec::new();
    for task in tasks {
        let ran = task.await.expect("join");
        assert!(ran.is_ok(), "every replica starts: {ran:?}");
        applied.extend(ran.unwrap_or_default());
    }
    applied.sort();
    assert_eq!(applied, names(&["0001_slow", "0002_next"]));
    admin
        .execute(&format!("DROP SCHEMA \"{schema}\" CASCADE"), &[])
        .await
        .expect("cleanup");
}

// A Postgres migration row stores the name exactly as text.
#[tokio::test]
async fn the_recorded_name_is_text() {
    let all = databases().await;
    for t in &all {
        migrate::apply(&t.db, &[init_migration()])
            .await
            .expect(t.name);
        let rows =
            t.db.query("SELECT \"name\" FROM \"k_migrations\"", &[])
                .await
                .expect(t.name);
        assert_eq!(rows[0].values, vec![text("0001_init")], "{}", t.name);
        assert_ne!(rows[0].values[0], Value::Null, "{}", t.name);
    }
    finish(all).await;
}
