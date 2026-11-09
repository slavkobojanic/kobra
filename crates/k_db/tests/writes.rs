//! Writes (`docs/compiler/db.md` §7, `docs/design/19-persistence.md`,
//! "Writes" and "Bulk writes on queries"): `insert`, `insert(all:)`,
//! `update(entity)`, `delete(entity)` and the bulk `update { … }` /
//! `delete()` on filtered queries, as IR, as SQL and on every database.

use crate::common;

use common::{
    both, eq, finish, ids, insert_notes, note_values, notes_where, rows, run, seeded, sorted_ids,
    table_count, text, uuid,
};
use k_db::ir::{CmpOp, Operand, Predicate, Statement};
use k_db::sql::{Dialect, render};
use k_db::{DbError, Value};

fn insert_tags(rows: u32) -> Statement {
    Statement::Insert {
        table: "tag".into(),
        columns: vec!["id".into(), "note_id".into(), "name".into()],
        rows,
    }
}

fn tag(n: u32, note: u32, name: &str) -> Vec<Value> {
    vec![text(&uuid(n)), text(&uuid(note)), text(name)]
}

/// `update(tag)`: every column but the id, where the id matches.
fn update_tag() -> Statement {
    Statement::Update {
        table: "tag".into(),
        set: vec![
            ("note_id".into(), Operand::Param(1)),
            ("name".into(), Operand::Param(2)),
        ],
        filter: Some(eq("id", 3)),
    }
}

/// `delete(entity)` on `table`.
fn delete_by_id(table: &str) -> Statement {
    Statement::Delete {
        table: table.into(),
        filter: Some(eq("id", 1)),
    }
}

fn titles_by_id(rs: &[k_db::Row]) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = rs
        .iter()
        .map(|r| {
            (
                common::number(r.values[0].as_text().unwrap_or_default()),
                r.values[1].as_text().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    out.sort();
    out
}

// `insert(note)` inserts one row and reports one affected row.
#[tokio::test]
async fn insert_one() {
    let all = common::migrated().await;
    for t in &all {
        let n = run(
            &t.db,
            &insert_notes(1),
            &note_values(7, "New", false, 1, None),
        )
        .await;
        assert_eq!(n, 1, "{}", t.name);
        assert_eq!(table_count(&t.db, "note").await, Some(1), "{}", t.name);
    }
    finish(all).await;
}

// `insert(all: tags)` is one multi-row statement, parameters numbered row by row.
#[tokio::test]
async fn insert_all_is_one_statement() {
    let st = insert_tags(3);
    assert_eq!(
        both(&st).0,
        "INSERT INTO \"tag\" (\"id\", \"note_id\", \"name\") VALUES (?1, ?2, ?3), (?4, ?5, ?6), (?7, ?8, ?9)"
    );
    let all = seeded().await;
    for t in &all {
        let params: Vec<Value> = [tag(1, 1, "a"), tag(2, 1, "b"), tag(3, 2, "c")].concat();
        assert_eq!(run(&t.db, &st, &params).await, 3, "{}", t.name);
        assert_eq!(table_count(&t.db, "tag").await, Some(3), "{}", t.name);
    }
    finish(all).await;
}

// `insert(all:)` is atomic: one bad row (an unknown note) inserts none of them.
#[tokio::test]
async fn insert_all_is_atomic() {
    let all = seeded().await;
    for t in &all {
        let params: Vec<Value> = [tag(1, 1, "a"), tag(2, 99, "orphan"), tag(3, 2, "c")].concat();
        let sql = render(&insert_tags(3), t.db.dialect());
        assert!(
            t.db.execute(&sql.text, &params).await.is_err(),
            "{}",
            t.name
        );
        assert_eq!(table_count(&t.db, "tag").await, Some(0), "{}", t.name);
    }
    finish(all).await;
}

// A batch of 4,000 rows (28,000 parameters) fits one statement on both databases.
#[tokio::test]
async fn a_large_insert_all() {
    let all = common::migrated().await;
    for t in &all {
        let params: Vec<Value> = (1..=4000)
            .flat_map(|n| note_values(n, &format!("note {n}"), n % 2 == 0, i64::from(n), None))
            .collect();
        assert_eq!(
            run(&t.db, &insert_notes(4000), &params).await,
            4000,
            "{}",
            t.name
        );
        assert_eq!(table_count(&t.db, "note").await, Some(4000), "{}", t.name);
    }
    finish(all).await;
}

// Text is stored verbatim: quotes, SQL, newlines, emoji and the empty string.
#[tokio::test]
async fn text_is_stored_verbatim() {
    let samples = [
        "'; DROP TABLE \"note\"; --",
        "line 1\nline 2\r\n\ttab",
        "emoji 🎉 and 中文",
        "",
        "?1 $1 %s",
        "back\\slash",
    ];
    let all = common::migrated().await;
    for t in &all {
        for (i, s) in samples.iter().enumerate() {
            let n = u32::try_from(i).unwrap_or(0) + 1;
            run(&t.db, &insert_notes(1), &note_values(n, s, false, 1, None)).await;
        }
        let got = titles_by_id(&rows(&t.db, &Statement::Select(common::note_select()), &[]).await);
        let expected: Vec<(u32, String)> = samples
            .iter()
            .enumerate()
            .map(|(i, s)| (u32::try_from(i).unwrap_or(0) + 1, (*s).to_owned()))
            .collect();
        assert_eq!(got, expected, "{}", t.name);
    }
    finish(all).await;
}

// A missing required value is a statement failure (a panic in K), not a domain error.
#[tokio::test]
async fn a_null_in_a_required_column_is_a_statement_failure() {
    let all = common::migrated().await;
    for t in &all {
        let mut values = note_values(1, "x", false, 1, None);
        values[1] = Value::Null;
        let sql = render(&insert_notes(1), t.db.dialect());
        let err = t.db.execute(&sql.text, &values).await.expect_err(t.name);
        assert!(matches!(err, DbError::Statement(_)), "{}: {err:?}", t.name);
    }
    finish(all).await;
}

// `update(tag)` writes every column but the id, where the id matches.
#[tokio::test]
async fn update_entity_writes_every_column_but_the_id() {
    assert_eq!(
        both(&update_tag()).1,
        "UPDATE \"tag\" SET \"note_id\" = $1, \"name\" = $2 WHERE \"id\" = $3"
    );
    let all = seeded().await;
    for t in &all {
        run(&t.db, &insert_tags(1), &tag(1, 1, "old")).await;
        let n = run(
            &t.db,
            &update_tag(),
            &[text(&uuid(2)), text("new"), text(&uuid(1))],
        )
        .await;
        assert_eq!(n, 1, "{}", t.name);
        let r =
            t.db.query("SELECT \"note_id\", \"name\" FROM \"tag\"", &[])
                .await
                .expect(t.name);
        assert_eq!(r[0].values, vec![text(&uuid(2)), text("new")], "{}", t.name);
    }
    finish(all).await;
}

// `update(entity)` of a row that no longer exists changes nothing.
#[tokio::test]
async fn update_of_a_missing_row_affects_nothing() {
    let all = seeded().await;
    for t in &all {
        let n = run(
            &t.db,
            &update_tag(),
            &[text(&uuid(2)), text("x"), text(&uuid(42))],
        )
        .await;
        assert_eq!(n, 0, "{}", t.name);
    }
    finish(all).await;
}

// `update(note)` never writes its relations: the note's tags are untouched.
#[tokio::test]
async fn update_does_not_touch_relations() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("title".into(), Operand::Param(1))],
        filter: Some(eq("id", 2)),
    };
    let all = seeded().await;
    for t in &all {
        run(
            &t.db,
            &insert_tags(2),
            &[tag(1, 1, "a"), tag(2, 1, "b")].concat(),
        )
        .await;
        run(&t.db, &st, &[text("renamed"), text(&uuid(1))]).await;
        assert_eq!(table_count(&t.db, "tag").await, Some(2), "{}", t.name);
    }
    finish(all).await;
}

// `delete(note)` deletes the row with the entity's id; deleting it again affects nothing.
#[tokio::test]
async fn delete_entity() {
    assert_eq!(
        both(&delete_by_id("note")).0,
        "DELETE FROM \"note\" WHERE \"id\" = ?1"
    );
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            run(&t.db, &delete_by_id("note"), &[text(&uuid(3))]).await,
            1,
            "{}",
            t.name
        );
        assert_eq!(
            run(&t.db, &delete_by_id("note"), &[text(&uuid(3))]).await,
            0,
            "{}",
            t.name
        );
        assert_eq!(table_count(&t.db, "note").await, Some(4), "{}", t.name);
    }
    finish(all).await;
}

// `notes.filter { n => n.id == id }.filter { n => n.isPinned == true }.update { n => n.category = c }`
// returns the number of rows it changed.
#[tokio::test]
async fn a_bulk_update_returns_the_rows_affected() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("category".into(), Operand::Param(1))],
        filter: Some(Predicate::And(vec![eq("id", 2), eq("is_pinned", 3)])),
    };
    assert_eq!(
        both(&st).0,
        "UPDATE \"note\" SET \"category\" = ?1 WHERE \"id\" = ?2 AND \"is_pinned\" = ?3"
    );
    let all = seeded().await;
    for t in &all {
        let pinned = [text("done"), text(&uuid(1)), Value::Bool(true)];
        assert_eq!(run(&t.db, &st, &pinned).await, 1, "{}", t.name);
        let unpinned = [text("done"), text(&uuid(2)), Value::Bool(true)];
        assert_eq!(
            run(&t.db, &st, &unpinned).await,
            0,
            "{}: the second filter holds",
            t.name
        );
    }
    finish(all).await;
}

// A bulk update writes only the fields it assigns.
#[tokio::test]
async fn a_bulk_update_writes_only_assigned_fields() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("is_pinned".into(), Operand::Param(1))],
        filter: Some(eq("id", 2)),
    };
    let all = seeded().await;
    for t in &all {
        run(&t.db, &st, &[Value::Bool(true), text(&uuid(4))]).await;
        let r = rows(&t.db, &notes_where(eq("id", 1)), &[text(&uuid(4))]).await;
        assert_eq!(r[0].values[1], text("delta_x"), "{}", t.name);
        assert!(common::is_true(r[0].values.get(3)), "{}", t.name);
        assert_eq!(r[0].values[4], Value::Int(5), "{}", t.name);
        assert_eq!(
            r[0].values[6],
            Value::Int(1),
            "{}: revision untouched",
            t.name
        );
    }
    finish(all).await;
}

// A bulk update may assign nil to an optional field.
#[tokio::test]
async fn a_bulk_update_can_assign_nil() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("category".into(), Operand::Param(1))],
        filter: Some(eq("category", 2)),
    };
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            run(&t.db, &st, &[Value::Null, text("work")]).await,
            2,
            "{}",
            t.name
        );
        let nils = notes_where(Predicate::IsNull(Operand::Column("category".into())));
        assert_eq!(
            sorted_ids(&t.db, &nils, &[]).await,
            [1, 2, 4, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `update { n => n.title = n.body }`: a value may be another column of the same row.
#[tokio::test]
async fn a_bulk_update_can_copy_a_column() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("title".into(), Operand::Column("body".into()))],
        filter: None,
    };
    assert_eq!(both(&st).0, "UPDATE \"note\" SET \"title\" = \"body\"");
    let all = seeded().await;
    for t in &all {
        assert_eq!(run(&t.db, &st, &[]).await, 5, "{}", t.name);
    }
    finish(all).await;
}

// A bulk update without a filter changes every row and says how many.
#[tokio::test]
async fn a_bulk_update_without_a_filter_changes_every_row() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("is_pinned".into(), Operand::Param(1))],
        filter: None,
    };
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            run(&t.db, &st, &[Value::Bool(false)]).await,
            5,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A bulk update that matches nothing returns 0.
#[tokio::test]
async fn a_bulk_update_matching_nothing() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![("title".into(), Operand::Param(1))],
        filter: Some(common::cmp("priority", CmpOp::Gt, 2)),
    };
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            run(&t.db, &st, &[text("x"), Value::Int(100)]).await,
            0,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `tags.filter { t => t.note.id == note.id }.delete()` returns the rows deleted.
#[tokio::test]
async fn a_bulk_delete_returns_the_rows_affected() {
    let st = Statement::Delete {
        table: "tag".into(),
        filter: Some(eq("note_id", 1)),
    };
    assert_eq!(both(&st).1, "DELETE FROM \"tag\" WHERE \"note_id\" = $1");
    let all = seeded().await;
    for t in &all {
        run(
            &t.db,
            &insert_tags(3),
            &[tag(1, 1, "a"), tag(2, 1, "b"), tag(3, 2, "c")].concat(),
        )
        .await;
        assert_eq!(run(&t.db, &st, &[text(&uuid(1))]).await, 2, "{}", t.name);
        assert_eq!(table_count(&t.db, "tag").await, Some(1), "{}", t.name);
    }
    finish(all).await;
}

// `notes.delete()` without a filter deletes every row and reports how many, even though the
// statement has no parameters.
#[tokio::test]
async fn a_bulk_delete_without_a_filter_reports_its_count() {
    let st = Statement::Delete {
        table: "note".into(),
        filter: None,
    };
    assert_eq!(both(&st).0, "DELETE FROM \"note\"");
    assert_eq!(render(&st, Dialect::Postgres).params, 0);
    let all = seeded().await;
    for t in &all {
        assert_eq!(run(&t.db, &st, &[]).await, 5, "{}", t.name);
        assert_eq!(table_count(&t.db, "note").await, Some(0), "{}", t.name);
    }
    finish(all).await;
}

// A bulk delete whose filter has no parameters (`n.category == nil`) still reports its count.
#[tokio::test]
async fn a_parameterless_filtered_delete_reports_its_count() {
    let st = Statement::Delete {
        table: "note".into(),
        filter: Some(Predicate::IsNull(Operand::Column("category".into()))),
    };
    let all = seeded().await;
    for t in &all {
        assert_eq!(run(&t.db, &st, &[]).await, 2, "{}", t.name);
    }
    finish(all).await;
}

// A bulk delete matching nothing returns 0 and deletes nothing.
#[tokio::test]
async fn a_bulk_delete_matching_nothing() {
    let st = Statement::Delete {
        table: "note".into(),
        filter: Some(eq("title", 1)),
    };
    let all = seeded().await;
    for t in &all {
        assert_eq!(run(&t.db, &st, &[text("nope")]).await, 0, "{}", t.name);
        assert_eq!(
            ids(&t.db, &Statement::Select(common::note_select()), &[])
                .await
                .len(),
            5,
            "{}",
            t.name
        );
    }
    finish(all).await;
}
