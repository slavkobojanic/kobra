//! `@Version` optimistic concurrency (`docs/compiler/db.md` §7,
//! `docs/design/19-persistence.md`, "Writes"): `update(entity)` on a
//! versioned entity adds `AND "revision" = ?` to its filter, sets the
//! column to the read value plus one, and returns `Bool` from the
//! affected-row count. `Note.revision` is the fixture's version field.

mod common;

use common::{both, eq, finish, rows, run, seeded, text, uuid};
use k_db::Value;
use k_db::ir::{Operand, Predicate, Statement};
use k_db::sql::{Dialect, render};

/// `update(note)` for the versioned `Note`: every column but the id, the
/// revision set to `?6`, where the id is `?7` and the revision `?8`.
fn versioned_update() -> Statement {
    Statement::Update {
        table: "note".into(),
        set: vec![
            ("title".into(), Operand::Param(1)),
            ("body".into(), Operand::Param(2)),
            ("is_pinned".into(), Operand::Param(3)),
            ("priority".into(), Operand::Param(4)),
            ("category".into(), Operand::Param(5)),
            ("revision".into(), Operand::Param(6)),
        ],
        filter: Some(Predicate::And(vec![eq("id", 7), eq("revision", 8)])),
    }
}

/// The parameters of `versioned_update` for a note read at `read_revision`.
fn params(id: u32, title: &str, read_revision: i64) -> Vec<Value> {
    vec![
        text(title),
        text(""),
        Value::Bool(false),
        Value::Int(1),
        Value::Null,
        Value::Int(read_revision + 1),
        text(&uuid(id)),
        Value::Int(read_revision),
    ]
}

/// `(title, revision)` of note `id`.
async fn read(db: &k_db::Database, id: u32) -> Option<(String, i64)> {
    let r = rows(db, &common::notes_where(eq("id", 1)), &[text(&uuid(id))]).await;
    let row = r.first()?;
    Some((row.values[1].as_text()?.to_owned(), row.values[6].as_int()?))
}

// The versioned update's SQL in both dialects.
#[test]
fn the_versioned_update_sql() {
    let (s, p) = both(&versioned_update());
    assert_eq!(
        s,
        "UPDATE \"note\" SET \"title\" = ?1, \"body\" = ?2, \"is_pinned\" = ?3, \"priority\" = ?4, \"category\" = ?5, \"revision\" = ?6 WHERE \"id\" = ?7 AND \"revision\" = ?8"
    );
    assert_eq!(
        p,
        "UPDATE \"note\" SET \"title\" = $1, \"body\" = $2, \"is_pinned\" = $3, \"priority\" = $4, \"category\" = $5, \"revision\" = $6 WHERE \"id\" = $7 AND \"revision\" = $8"
    );
    assert_eq!(render(&versioned_update(), Dialect::Sqlite).params, 8);
}

// An update from the current revision succeeds (`true`) and increments the revision.
#[tokio::test]
async fn an_update_from_the_current_revision_succeeds() {
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            run(&t.db, &versioned_update(), &params(1, "edited", 1)).await,
            1,
            "{}",
            t.name
        );
        assert_eq!(
            read(&t.db, 1).await,
            Some(("edited".into(), 2)),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// An update from a stale revision changes nothing (`false`), so no update is lost silently.
#[tokio::test]
async fn an_update_from_a_stale_revision_fails() {
    let all = seeded().await;
    for t in &all {
        run(&t.db, &versioned_update(), &params(1, "first", 1)).await;
        assert_eq!(
            run(&t.db, &versioned_update(), &params(1, "second", 1)).await,
            0,
            "{}",
            t.name
        );
        assert_eq!(
            read(&t.db, 1).await,
            Some(("first".into(), 2)),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Two copies read at the same revision: the first write wins, the second learns it lost.
#[tokio::test]
async fn concurrent_editors_cannot_both_win() {
    let all = seeded().await;
    for t in &all {
        let alice = params(2, "alice", 1);
        let bob = params(2, "bob", 1);
        let (a, b) = (
            run(&t.db, &versioned_update(), &alice).await,
            run(&t.db, &versioned_update(), &bob).await,
        );
        assert_eq!((a, b), (1, 0), "{}", t.name);
        assert_eq!(
            read(&t.db, 2).await,
            Some(("alice".into(), 2)),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// After reloading (revision 2), the loser's retry succeeds.
#[tokio::test]
async fn a_retry_after_reloading_succeeds() {
    let all = seeded().await;
    for t in &all {
        run(&t.db, &versioned_update(), &params(2, "alice", 1)).await;
        let (_, revision) = read(&t.db, 2).await.unwrap_or_default();
        assert_eq!(
            run(&t.db, &versioned_update(), &params(2, "bob", revision)).await,
            1,
            "{}",
            t.name
        );
        assert_eq!(read(&t.db, 2).await, Some(("bob".into(), 3)), "{}", t.name);
    }
    finish(all).await;
}

// An update of a row that was deleted is `false` too.
#[tokio::test]
async fn an_update_of_a_deleted_row_fails() {
    let all = seeded().await;
    for t in &all {
        let delete = Statement::Delete {
            table: "note".into(),
            filter: Some(eq("id", 1)),
        };
        run(&t.db, &delete, &[text(&uuid(3))]).await;
        assert_eq!(
            run(&t.db, &versioned_update(), &params(3, "gone", 1)).await,
            0,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A revision from the future (never read) fails rather than jumping ahead.
#[tokio::test]
async fn a_future_revision_fails() {
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            run(&t.db, &versioned_update(), &params(1, "x", 5)).await,
            0,
            "{}",
            t.name
        );
        assert_eq!(
            read(&t.db, 1).await,
            Some(("Alpha plan".into(), 1)),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Many sequential edits each bump the revision by exactly one.
#[tokio::test]
async fn sequential_edits_count_up() {
    let all = seeded().await;
    for t in &all {
        for revision in 1..=10 {
            let n = run(
                &t.db,
                &versioned_update(),
                &params(4, &format!("v{revision}"), revision),
            )
            .await;
            assert_eq!(n, 1, "{}: edit {revision}", t.name);
        }
        assert_eq!(read(&t.db, 4).await, Some(("v10".into(), 11)), "{}", t.name);
    }
    finish(all).await;
}

// A versioned update only touches its own row, even when others share the revision.
#[tokio::test]
async fn a_versioned_update_touches_one_row() {
    let all = seeded().await;
    for t in &all {
        run(&t.db, &versioned_update(), &params(1, "one", 1)).await;
        for other in 2..=5 {
            assert_eq!(
                read(&t.db, other).await.map(|r| r.1),
                Some(1),
                "{}: note {other}",
                t.name
            );
        }
    }
    finish(all).await;
}

// A bulk update changes a `@Version` field only when it assigns it.
#[tokio::test]
async fn a_bulk_update_leaves_the_version_alone() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("is_pinned".into(), Operand::Param(1))],
        filter: Some(eq("id", 2)),
    };
    let all = seeded().await;
    for t in &all {
        run(&t.db, &st, &[Value::Bool(true), text(&uuid(1))]).await;
        assert_eq!(read(&t.db, 1).await.map(|r| r.1), Some(1), "{}", t.name);
        // So a versioned update from the copy read before still succeeds.
        assert_eq!(
            run(&t.db, &versioned_update(), &params(1, "after", 1)).await,
            1,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A bulk update that assigns the version field changes it like any other.
#[tokio::test]
async fn a_bulk_update_may_assign_the_version() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("revision".into(), Operand::Param(1))],
        filter: Some(eq("id", 2)),
    };
    let all = seeded().await;
    for t in &all {
        run(&t.db, &st, &[Value::Int(9), text(&uuid(1))]).await;
        assert_eq!(read(&t.db, 1).await.map(|r| r.1), Some(9), "{}", t.name);
        assert_eq!(
            run(&t.db, &versioned_update(), &params(1, "stale", 1)).await,
            0,
            "{}",
            t.name
        );
    }
    finish(all).await;
}
