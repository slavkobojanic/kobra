//! Relations (`docs/compiler/db.md` §8, `docs/design/19-persistence.md`,
//! "Entities and tables"): to-one foreign keys, filtering by a related id,
//! and `.including { … }` as one extra query per relation (never one per
//! row), on every available database.

use crate::common;

use common::{
    eq, finish, id_list, ids, note_select, run, seeded, sorted_ids, table_count, text, uuid,
};
use k_db::ir::{Operand, Predicate, Select, Statement};
use k_db::sql::{Dialect, render};
use k_db::{DbError, Value};

fn tag_select(filter: Option<Predicate>) -> Statement {
    Statement::Select(Select {
        table: "tag".into(),
        columns: vec!["id".into(), "note_id".into(), "name".into()],
        filter,
        order: vec![],
        limit: None,
        offset: None,
    })
}

/// Tags 1–2 on note 1, tag 3 on note 3, tag 4 on note 3.
async fn seed_tags(db: &k_db::Database) {
    let st = Statement::Insert {
        table: "tag".into(),
        columns: vec!["id".into(), "note_id".into(), "name".into()],
        rows: 4,
    };
    let params: Vec<Value> = [
        (1, 1, "red"),
        (2, 1, "blue"),
        (3, 3, "red"),
        (4, 3, "green"),
    ]
    .iter()
    .flat_map(|(n, note, name)| vec![text(&uuid(*n)), text(&uuid(*note)), text(name)])
    .collect();
    assert_eq!(run(db, &st, &params).await, 4);
}

// `tags.filter { t => t.note.id == note.id }` compares the `note_id` column.
#[tokio::test]
async fn filtering_by_a_related_id() {
    let st = tag_select(Some(eq("note_id", 1)));
    assert!(common::both(&st).0.ends_with("WHERE \"note_id\" = ?1"));
    let all = seeded().await;
    for t in &all {
        seed_tags(&t.db).await;
        assert_eq!(
            sorted_ids(&t.db, &st, &[text(&uuid(3))]).await,
            [3, 4],
            "{}",
            t.name
        );
        assert!(
            sorted_ids(&t.db, &st, &[text(&uuid(2))]).await.is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { … }.including { n => n.tags }`: the main query, then ONE query for every
// main row's tags, by `note_id IN (…)`, grouped in memory.
#[tokio::test]
async fn including_a_to_many_relation_is_one_query() {
    let tags_of = tag_select(Some(Predicate::In {
        operand: Operand::Column("note_id".into()),
        list: 1,
    }));
    assert_eq!(
        render(&tags_of, Dialect::Sqlite).text,
        "SELECT \"id\", \"note_id\", \"name\" FROM \"tag\" WHERE \"note_id\" IN (SELECT value FROM json_each(?1))"
    );
    let all = seeded().await;
    for t in &all {
        seed_tags(&t.db).await;
        let main = Statement::Select(Select {
            filter: Some(eq("is_pinned", 1)),
            ..note_select()
        });
        let notes = sorted_ids(&t.db, &main, &[Value::Bool(true)]).await;
        assert_eq!(notes, [1, 3], "{}", t.name);
        let tags = sorted_ids(&t.db, &tags_of, &[id_list(&notes)]).await;
        assert_eq!(tags, [1, 2, 3, 4], "{}", t.name);
    }
    finish(all).await;
}

// A main row without related rows gets an empty list: the relation query just has no rows.
#[tokio::test]
async fn including_on_rows_without_relations() {
    let tags_of = tag_select(Some(Predicate::In {
        operand: Operand::Column("note_id".into()),
        list: 1,
    }));
    let all = seeded().await;
    for t in &all {
        seed_tags(&t.db).await;
        assert!(
            sorted_ids(&t.db, &tags_of, &[id_list(&[2, 4, 5])])
                .await
                .is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// An empty main result needs an empty relation list (the runtime may skip the query).
#[tokio::test]
async fn including_on_an_empty_result() {
    let tags_of = tag_select(Some(Predicate::In {
        operand: Operand::Column("note_id".into()),
        list: 1,
    }));
    let all = seeded().await;
    for t in &all {
        seed_tags(&t.db).await;
        assert!(
            sorted_ids(&t.db, &tags_of, &[id_list(&[])])
                .await
                .is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `tags.including { t => t.note }`: a to-one relation loads the distinct targets by id.
#[tokio::test]
async fn including_a_to_one_relation_is_one_query() {
    let notes_by_id = Statement::Select(Select {
        filter: Some(Predicate::In {
            operand: Operand::Column("id".into()),
            list: 1,
        }),
        ..note_select()
    });
    let all = seeded().await;
    for t in &all {
        seed_tags(&t.db).await;
        let tags = common::rows(&t.db, &tag_select(None), &[]).await;
        let mut targets: Vec<&str> = tags.iter().filter_map(|r| r.values[1].as_text()).collect();
        targets.sort_unstable();
        targets.dedup();
        let list = Value::Text(serde_json::to_string(&targets).expect("json"));
        assert_eq!(
            sorted_ids(&t.db, &notes_by_id, &[list]).await,
            [1, 3],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A to-one relation must point at an existing row: a dangling one is refused.
#[tokio::test]
async fn a_relation_to_a_missing_row_is_refused() {
    let all = seeded().await;
    for t in &all {
        let st = Statement::Insert {
            table: "tag".into(),
            columns: vec!["id".into(), "note_id".into(), "name".into()],
            rows: 1,
        };
        let r =
            t.db.execute(
                &render(&st, t.db.dialect()).text,
                &[text(&uuid(1)), text(&uuid(77)), text("x")],
            )
            .await;
        assert!(matches!(r, Err(DbError::Statement(_))), "{}: {r:?}", t.name);
        assert_eq!(table_count(&t.db, "tag").await, Some(0), "{}", t.name);
    }
    finish(all).await;
}

// Deleting a row that related rows still reference fails (no implicit cascade): writes never
// touch relations, so the related rows must be deleted first through their repository.
#[tokio::test]
async fn deleting_a_referenced_row_fails_without_cascading() {
    let delete_note = Statement::Delete {
        table: "note".into(),
        filter: Some(eq("id", 1)),
    };
    let all = seeded().await;
    for t in &all {
        seed_tags(&t.db).await;
        let r =
            t.db.execute(
                &render(&delete_note, t.db.dialect()).text,
                &[text(&uuid(1))],
            )
            .await;
        assert!(matches!(r, Err(DbError::Statement(_))), "{}: {r:?}", t.name);
        assert_eq!(table_count(&t.db, "tag").await, Some(4), "{}", t.name);
        // Deleting the tags first, then the note, works.
        let delete_tags = Statement::Delete {
            table: "tag".into(),
            filter: Some(eq("note_id", 1)),
        };
        assert_eq!(
            run(&t.db, &delete_tags, &[text(&uuid(1))]).await,
            2,
            "{}",
            t.name
        );
        assert_eq!(
            run(&t.db, &delete_note, &[text(&uuid(1))]).await,
            1,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Deleting a row nothing references works even when other rows have relations.
#[tokio::test]
async fn deleting_an_unreferenced_row_works() {
    let all = seeded().await;
    for t in &all {
        seed_tags(&t.db).await;
        let delete_note = Statement::Delete {
            table: "note".into(),
            filter: Some(eq("id", 1)),
        };
        assert_eq!(
            run(&t.db, &delete_note, &[text(&uuid(2))]).await,
            1,
            "{}",
            t.name
        );
        assert_eq!(
            ids(&t.db, &Statement::Select(note_select()), &[])
                .await
                .len(),
            4,
            "{}",
            t.name
        );
    }
    finish(all).await;
}
