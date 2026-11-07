//! Repository queries as lowered to the IR (`docs/compiler/db.md` §4) and
//! rendered to SQL (§5), run against the seed notes on every available
//! database.
//!
//! Each test starts with the K query it stands for; the IR is what `k_sema`
//! lowers that query to according to the table in db.md §4, so these tests
//! pin the contract between lowering, rendering and the drivers. The seed
//! rows are listed in `common::seed_rows`.

mod common;

use common::{
    both, cmp, eq, finish, id_list, ids, json_list, note_select, notes_where, order, rows, seeded,
    sorted_ids, text,
};
use k_db::Value;
use k_db::ir::{CmpOp, Count, Like, Operand, Predicate, Select, Statement};
use k_db::sql::{Dialect, like_pattern, render};

fn like(column: &str, kind: Like, param: u32) -> Predicate {
    Predicate::Like {
        column: column.into(),
        kind,
        param,
    }
}

fn pattern(kind: Like, s: &str) -> Value {
    Value::Text(like_pattern(kind, s))
}

fn sorted_select(order_by: Vec<k_db::ir::Order>) -> Statement {
    Statement::Select(Select {
        order: order_by,
        ..note_select()
    })
}

fn count_where(filter: Option<Predicate>) -> Statement {
    Statement::Count(Select {
        filter,
        ..note_select()
    })
}

async fn count(db: &k_db::Database, st: &Statement, params: &[Value]) -> Option<i64> {
    rows(db, st, params)
        .await
        .first()
        .and_then(|r| r.values[0].as_int())
}

// --- the collection -------------------------------------------------------

// `await notes` selects every column in field order.
#[tokio::test]
async fn the_collection_selects_every_row() {
    let st = Statement::Select(note_select());
    assert_eq!(
        both(&st).0,
        "SELECT \"id\", \"title\", \"body\", \"is_pinned\", \"priority\", \"category\", \"revision\" FROM \"note\""
    );
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[]).await,
            [1, 2, 3, 4, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Rows come back with the selected columns in select order, for the generated decoder.
#[tokio::test]
async fn rows_carry_columns_in_select_order() {
    let all = seeded().await;
    for t in &all {
        let r = rows(&t.db, &notes_where(eq("id", 1)), &[text(&common::uuid(1))]).await;
        assert_eq!(r.len(), 1, "{}", t.name);
        assert_eq!(r[0].columns, common::NOTE_COLUMNS, "{}", t.name);
        assert_eq!(r[0].values[1], text("Alpha plan"), "{}", t.name);
        assert!(common::is_true(r[0].values.get(3)), "{}", t.name);
        assert_eq!(r[0].values[4], Value::Int(3), "{}", t.name);
        assert_eq!(r[0].values[5], text("work"), "{}", t.name);
    }
    finish(all).await;
}

// --- comparisons ----------------------------------------------------------

// `notes.filter { n => n.isPinned == true }`: the constant is a parameter too.
#[tokio::test]
async fn equality_on_a_boolean() {
    let st = notes_where(eq("is_pinned", 1));
    assert!(both(&st).0.ends_with("WHERE \"is_pinned\" = ?1"));
    assert!(both(&st).1.ends_with("WHERE \"is_pinned\" = $1"));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Bool(true)]).await,
            [1, 3],
            "{}",
            t.name
        );
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Bool(false)]).await,
            [2, 4, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.title == title }` is exact and case-sensitive on both databases.
#[tokio::test]
async fn equality_on_text_is_case_sensitive() {
    let st = notes_where(eq("title", 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[text("Alpha plan")]).await,
            [1],
            "{}",
            t.name
        );
        assert!(
            sorted_ids(&t.db, &st, &[text("alpha plan")])
                .await
                .is_empty(),
            "{}",
            t.name
        );
        assert!(
            sorted_ids(&t.db, &st, &[text("Alpha plan ")])
                .await
                .is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.priority != 3 }` on a non-optional column renders `<>`.
#[tokio::test]
async fn inequality_on_a_required_column() {
    let st = notes_where(cmp("priority", CmpOp::Ne, 1));
    assert!(both(&st).0.ends_with("WHERE \"priority\" <> ?1"));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Int(3)]).await,
            [2, 3, 4, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.priority < 3 }`
#[tokio::test]
async fn less_than() {
    let st = notes_where(cmp("priority", CmpOp::Lt, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Int(3)]).await,
            [2, 3],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.priority <= 3 }`
#[tokio::test]
async fn less_than_or_equal() {
    let st = notes_where(cmp("priority", CmpOp::Le, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Int(3)]).await,
            [1, 2, 3],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.priority > 3 }`
#[tokio::test]
async fn greater_than() {
    let st = notes_where(cmp("priority", CmpOp::Gt, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Int(3)]).await,
            [4, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.priority >= 3 }`
#[tokio::test]
async fn greater_than_or_equal() {
    let st = notes_where(cmp("priority", CmpOp::Ge, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Int(3)]).await,
            [1, 4, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => minimum < n.priority }`: the column may be on the right.
#[tokio::test]
async fn the_column_may_be_on_the_right() {
    let st = notes_where(Predicate::Compare {
        left: Operand::Param(1),
        op: CmpOp::Lt,
        right: Operand::Column("priority".into()),
    });
    assert!(both(&st).1.ends_with("WHERE $1 < \"priority\""));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Int(3)]).await,
            [4, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.priority == n.revision }`: two columns compare without a parameter.
#[tokio::test]
async fn two_columns_compare() {
    let st = notes_where(Predicate::Compare {
        left: Operand::Column("priority".into()),
        op: CmpOp::Eq,
        right: Operand::Column("revision".into()),
    });
    assert_eq!(render(&st, Dialect::Sqlite).params, 0);
    let all = seeded().await;
    for t in &all {
        assert_eq!(sorted_ids(&t.db, &st, &[]).await, [2], "{}", t.name);
    }
    finish(all).await;
}

// A comparison with a value no row has returns nothing (and is not an error).
#[tokio::test]
async fn a_comparison_matching_nothing() {
    let st = notes_where(eq("priority", 1));
    let all = seeded().await;
    for t in &all {
        assert!(
            ids(&t.db, &st, &[Value::Int(99)]).await.is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Safe-integer extremes bind without overflow on either database.
#[tokio::test]
async fn integer_parameters_at_the_safe_range_limits() {
    let st = notes_where(cmp("priority", CmpOp::Lt, 1));
    let all = seeded().await;
    for t in &all {
        let max = Value::Int(9_007_199_254_740_991);
        let min = Value::Int(-9_007_199_254_740_991);
        assert_eq!(sorted_ids(&t.db, &st, &[max]).await.len(), 5, "{}", t.name);
        assert!(
            sorted_ids(&t.db, &st, &[min]).await.is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// --- nil ------------------------------------------------------------------

// `notes.filter { n => n.category == nil }` is `IS NULL`, never `= NULL`.
#[tokio::test]
async fn equality_with_nil_is_is_null() {
    let st = notes_where(Predicate::IsNull(Operand::Column("category".into())));
    assert!(both(&st).0.ends_with("WHERE \"category\" IS NULL"));
    let all = seeded().await;
    for t in &all {
        assert_eq!(sorted_ids(&t.db, &st, &[]).await, [2, 5], "{}", t.name);
    }
    finish(all).await;
}

// `notes.filter { n => n.category != nil }` is `IS NOT NULL`.
#[tokio::test]
async fn inequality_with_nil_is_is_not_null() {
    let st = notes_where(Predicate::IsNotNull(Operand::Column("category".into())));
    let all = seeded().await;
    for t in &all {
        assert_eq!(sorted_ids(&t.db, &st, &[]).await, [1, 3, 4], "{}", t.name);
    }
    finish(all).await;
}

// Why lowering must special-case optionals: SQL `=` with a NULL parameter matches no row.
#[tokio::test]
async fn sql_equality_with_a_null_parameter_matches_nothing() {
    let st = notes_where(eq("category", 1));
    let all = seeded().await;
    for t in &all {
        assert!(
            ids(&t.db, &st, &[Value::Null]).await.is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.category == wanted }` with `wanted: String?`: K equality, where
// `nil == nil` holds, lowers null-aware: `c = ?1 OR (c IS NULL AND ?1 IS NULL)`.
#[tokio::test]
async fn equality_with_an_optional_parameter_is_null_aware() {
    let st = notes_where(Predicate::Or(vec![
        eq("category", 1),
        Predicate::And(vec![
            Predicate::IsNull(Operand::Column("category".into())),
            Predicate::IsNull(Operand::Param(1)),
        ]),
    ]));
    assert!(
        both(&st)
            .0
            .ends_with("WHERE \"category\" = ?1 OR (\"category\" IS NULL AND ?1 IS NULL)")
    );
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Null]).await,
            [2, 5],
            "{}",
            t.name
        );
        assert_eq!(
            sorted_ids(&t.db, &st, &[text("work")]).await,
            [1, 4],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Why lowering must special-case optionals: SQL `<>` drops rows whose column is NULL.
#[tokio::test]
async fn sql_inequality_drops_null_rows() {
    let st = notes_where(cmp("category", CmpOp::Ne, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[text("work")]).await,
            [3],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => n.category != "work" }`: in K a nil category is != "work", so the
// lowering on an optional column is `c <> ?1 OR c IS NULL`.
#[tokio::test]
async fn inequality_on_an_optional_column_keeps_nil_rows() {
    let st = notes_where(Predicate::Or(vec![
        cmp("category", CmpOp::Ne, 1),
        Predicate::IsNull(Operand::Column("category".into())),
    ]));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[text("work")]).await,
            [2, 3, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// --- boolean logic --------------------------------------------------------

// `notes.filter { n => n.isPinned == true and n.priority > 2 }`
#[tokio::test]
async fn and_of_two_conditions() {
    let st = notes_where(Predicate::And(vec![
        eq("is_pinned", 1),
        cmp("priority", CmpOp::Gt, 2),
    ]));
    assert!(
        both(&st)
            .0
            .ends_with("WHERE \"is_pinned\" = ?1 AND \"priority\" > ?2")
    );
    let all = seeded().await;
    for t in &all {
        let got = sorted_ids(&t.db, &st, &[Value::Bool(true), Value::Int(2)]).await;
        assert_eq!(got, [1], "{}", t.name);
    }
    finish(all).await;
}

// `notes.filter { n => n.priority == 1 or n.priority == 5 }`
#[tokio::test]
async fn or_of_two_conditions() {
    let st = notes_where(Predicate::Or(vec![eq("priority", 1), eq("priority", 2)]));
    let all = seeded().await;
    for t in &all {
        let got = sorted_ids(&t.db, &st, &[Value::Int(1), Value::Int(5)]).await;
        assert_eq!(got, [2, 4], "{}", t.name);
    }
    finish(all).await;
}

// `notes.filter { n => not (n.isPinned == true) }`
#[tokio::test]
async fn not_of_a_condition() {
    let st = notes_where(Predicate::Not(Box::new(eq("is_pinned", 1))));
    assert!(both(&st).0.ends_with("WHERE NOT (\"is_pinned\" = ?1)"));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Bool(true)]).await,
            [2, 4, 5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.filter { n => (n.priority == 1 or n.priority == 2) and not (n.isPinned == true) }`
// keeps K's grouping through parentheses.
#[tokio::test]
async fn nested_logic_keeps_its_grouping() {
    let st = notes_where(Predicate::And(vec![
        Predicate::Or(vec![eq("priority", 1), eq("priority", 2)]),
        Predicate::Not(Box::new(eq("is_pinned", 3))),
    ]));
    assert!(
        both(&st).0.ends_with(
            "WHERE (\"priority\" = ?1 OR \"priority\" = ?2) AND NOT (\"is_pinned\" = ?3)"
        )
    );
    let all = seeded().await;
    for t in &all {
        let got = sorted_ids(
            &t.db,
            &st,
            &[Value::Int(1), Value::Int(2), Value::Bool(true)],
        )
        .await;
        assert_eq!(got, [2], "{}", t.name);
    }
    finish(all).await;
}

// `a or (b and c)`: an `and` inside an `or` is parenthesized too.
#[tokio::test]
async fn and_inside_or_is_parenthesized() {
    let st = notes_where(Predicate::Or(vec![
        eq("priority", 1),
        Predicate::And(vec![eq("is_pinned", 2), cmp("priority", CmpOp::Gt, 3)]),
    ]));
    assert!(
        both(&st)
            .0
            .ends_with("WHERE \"priority\" = ?1 OR (\"is_pinned\" = ?2 AND \"priority\" > ?3)")
    );
    let all = seeded().await;
    for t in &all {
        let got = sorted_ids(
            &t.db,
            &st,
            &[Value::Int(5), Value::Bool(true), Value::Int(2)],
        )
        .await;
        assert_eq!(got, [1, 4], "{}", t.name);
    }
    finish(all).await;
}

// `.filter { a }.filter { b }`: repeated filters are and-ed.
#[tokio::test]
async fn repeated_filters_are_and_ed() {
    let st = notes_where(Predicate::And(vec![
        Predicate::IsNotNull(Operand::Column("category".into())),
        eq("is_pinned", 1),
    ]));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Bool(false)]).await,
            [4],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// An empty `and` is true: every row.
#[tokio::test]
async fn an_empty_and_is_true() {
    let st = notes_where(Predicate::And(vec![]));
    let all = seeded().await;
    for t in &all {
        assert_eq!(sorted_ids(&t.db, &st, &[]).await.len(), 5, "{}", t.name);
    }
    finish(all).await;
}

// An empty `or` is false: no rows.
#[tokio::test]
async fn an_empty_or_is_false() {
    let st = notes_where(Predicate::Or(vec![]));
    let all = seeded().await;
    for t in &all {
        assert!(sorted_ids(&t.db, &st, &[]).await.is_empty(), "{}", t.name);
    }
    finish(all).await;
}

// A captured value used twice is one parameter (numbered by first use) bound once.
#[tokio::test]
async fn a_reused_capture_is_one_parameter() {
    let st = notes_where(Predicate::Or(vec![
        cmp("priority", CmpOp::Gt, 1),
        eq("revision", 1),
    ]));
    assert_eq!(render(&st, Dialect::Postgres).params, 1);
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[Value::Int(4)]).await,
            [4],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// --- strings --------------------------------------------------------------

// `notes.filter { n => n.title.contains(text) }`
#[tokio::test]
async fn contains() {
    let st = notes_where(like("title", Like::Contains, 1));
    assert!(both(&st).0.ends_with("WHERE \"title\" LIKE ?1 ESCAPE '\\'"));
    let all = seeded().await;
    for t in &all {
        let got = sorted_ids(&t.db, &st, &[pattern(Like::Contains, "ta")]).await;
        assert_eq!(got, [2, 4], "{}", t.name);
    }
    finish(all).await;
}

// `contains("50%")`: a `%` in the text is literal, not a wildcard.
#[tokio::test]
async fn contains_treats_percent_literally() {
    let st = notes_where(like("title", Like::Contains, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "50%")]).await,
            [3],
            "{}",
            t.name
        );
        assert_eq!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "%")]).await,
            [3],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `contains("_")`: an underscore is literal, not "any character".
#[tokio::test]
async fn contains_treats_underscore_literally() {
    let st = notes_where(like("title", Like::Contains, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "_")]).await,
            [4],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `contains("\\")`: a backslash is escaped too, so it is neither an error nor an escape.
#[tokio::test]
async fn contains_treats_backslash_literally() {
    let st = notes_where(like("title", Like::Contains, 1));
    let all = seeded().await;
    for t in &all {
        assert!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "\\")])
                .await
                .is_empty(),
            "{}",
            t.name
        );
        assert!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "a\\")])
                .await
                .is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `contains("")` is true for every string, as in K.
#[tokio::test]
async fn contains_the_empty_string() {
    let st = notes_where(like("title", Like::Contains, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "")])
                .await
                .len(),
            5,
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `hasPrefix` is case-sensitive on both databases (SQLite sets `case_sensitive_like`).
#[tokio::test]
async fn has_prefix_is_case_sensitive() {
    let st = notes_where(like("title", Like::StartsWith, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[pattern(Like::StartsWith, "Alpha")]).await,
            [1],
            "{}",
            t.name
        );
        assert!(
            sorted_ids(&t.db, &st, &[pattern(Like::StartsWith, "alpha")])
                .await
                .is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `hasSuffix`
#[tokio::test]
async fn has_suffix() {
    let st = notes_where(like("title", Like::EndsWith, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[pattern(Like::EndsWith, "notes")]).await,
            [2],
            "{}",
            t.name
        );
        assert!(
            sorted_ids(&t.db, &st, &[pattern(Like::EndsWith, "note")])
                .await
                .is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// Non-ASCII text matches by its characters, case-sensitively.
#[tokio::test]
async fn contains_non_ascii_text() {
    let st = notes_where(like("title", Like::Contains, 1));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "ünï")]).await,
            [5],
            "{}",
            t.name
        );
        assert!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "épsilon")])
                .await
                .is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `not n.title.contains(t)` on a required column.
#[tokio::test]
async fn not_contains() {
    let st = notes_where(Predicate::Not(Box::new(like("title", Like::Contains, 1))));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[pattern(Like::Contains, "a")]).await,
            [5],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// --- membership -----------------------------------------------------------

// `notes.filter { n => ids.contains(n.id) }` binds the list as one parameter.
#[tokio::test]
async fn membership_in_a_list_of_ids() {
    let st = notes_where(Predicate::In {
        operand: Operand::Column("id".into()),
        list: 1,
    });
    assert_eq!(render(&st, Dialect::Sqlite).params, 1);
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[id_list(&[1, 3, 9])]).await,
            [1, 3],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `[].contains(n.id)` is false for every row.
#[tokio::test]
async fn membership_in_an_empty_list() {
    let st = notes_where(Predicate::In {
        operand: Operand::Column("id".into()),
        list: 1,
    });
    let all = seeded().await;
    for t in &all {
        assert!(
            sorted_ids(&t.db, &st, &[id_list(&[])]).await.is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A list with duplicates still returns each row once.
#[tokio::test]
async fn membership_with_duplicates_returns_rows_once() {
    let st = notes_where(Predicate::In {
        operand: Operand::Column("id".into()),
        list: 1,
    });
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            sorted_ids(&t.db, &st, &[id_list(&[2, 2, 2])]).await,
            [2],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `categories.contains(n.category)` on text values, including hostile ones.
#[tokio::test]
async fn membership_in_a_list_of_strings_is_injection_safe() {
    let st = notes_where(Predicate::In {
        operand: Operand::Column("title".into()),
        list: 1,
    });
    let all = seeded().await;
    for t in &all {
        let list = json_list(&["delta_x", "') OR 1=1 --", "\" OR \"\"=\"", "Alpha plan"]);
        assert_eq!(sorted_ids(&t.db, &st, &[list]).await, [1, 4], "{}", t.name);
    }
    finish(all).await;
}

// `priorities.contains(n.priority)` on integers.
#[tokio::test]
async fn membership_in_a_list_of_integers() {
    let st = notes_where(Predicate::In {
        operand: Operand::Column("priority".into()),
        list: 1,
    });
    let all = seeded().await;
    for t in &all {
        let list = Value::Text("[1, 4]".into());
        assert_eq!(sorted_ids(&t.db, &st, &[list]).await, [2, 5], "{}", t.name);
    }
    finish(all).await;
}

// --- ordering -------------------------------------------------------------

// `.sortedBy { n => n.priority }`
#[tokio::test]
async fn sorted_by_ascending() {
    let st = sorted_select(vec![order("priority", false)]);
    assert!(both(&st).0.ends_with(" ORDER BY \"priority\""));
    let all = seeded().await;
    for t in &all {
        assert_eq!(ids(&t.db, &st, &[]).await, [2, 3, 1, 5, 4], "{}", t.name);
    }
    finish(all).await;
}

// `.sortedByDescending { n => n.priority }`
#[tokio::test]
async fn sorted_by_descending() {
    let st = sorted_select(vec![order("priority", true)]);
    assert!(both(&st).0.ends_with(" ORDER BY \"priority\" DESC"));
    let all = seeded().await;
    for t in &all {
        assert_eq!(ids(&t.db, &st, &[]).await, [4, 5, 1, 3, 2], "{}", t.name);
    }
    finish(all).await;
}

// `.sortedByDescending { n => n.isPinned }.thenBy { n => n.priority }`
#[tokio::test]
async fn then_by_breaks_ties() {
    let st = sorted_select(vec![order("is_pinned", true), order("priority", false)]);
    assert!(
        both(&st)
            .1
            .ends_with(" ORDER BY \"is_pinned\" DESC NULLS LAST, \"priority\" NULLS FIRST")
    );
    assert!(
        both(&st)
            .0
            .ends_with(" ORDER BY \"is_pinned\" DESC, \"priority\"")
    );
    let all = seeded().await;
    for t in &all {
        assert_eq!(ids(&t.db, &st, &[]).await, [3, 1, 2, 5, 4], "{}", t.name);
    }
    finish(all).await;
}

// `.filter { … }.sortedBy { … }`: the filter comes before the order.
#[tokio::test]
async fn filter_then_order() {
    let st = Statement::Select(Select {
        filter: Some(eq("is_pinned", 1)),
        order: vec![order("priority", true)],
        ..note_select()
    });
    assert!(
        both(&st)
            .0
            .ends_with("WHERE \"is_pinned\" = ?1 ORDER BY \"priority\" DESC")
    );
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            ids(&t.db, &st, &[Value::Bool(false)]).await,
            [4, 5, 2],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

/// The order of `values` that `sortedBy` produces on each database.
async fn text_order(values: &[&str], descending: bool) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for t in common::migrated().await {
        for (i, v) in values.iter().enumerate() {
            let n = u32::try_from(i).unwrap_or(0) + 1;
            common::run(
                &t.db,
                &common::insert_notes(1),
                &common::note_values(n, v, false, 1, None),
            )
            .await;
        }
        let st = sorted_select(vec![order("title", descending), order("id", false)]);
        let titles = rows(&t.db, &st, &[])
            .await
            .iter()
            .map(|r| r.values[1].as_text().unwrap_or_default().to_owned())
            .collect();
        out.push((t.name.to_owned(), titles));
        t.finish().await.expect("cleanup");
    }
    out
}

// `.sortedBy { n => n.title }` orders strings the same way on SQLite (dev) and Postgres
// (production): by code point, as K's own `sortedBy` does.
#[tokio::test]
async fn text_order_is_the_same_on_every_database() {
    let values = ["b", "B", "a", "A", "_", "é", "Z", "aa", ""];
    let orders = text_order(&values, false).await;
    let expected: Vec<String> = {
        let mut v: Vec<String> = values.iter().map(|s| (*s).to_owned()).collect();
        v.sort();
        v
    };
    for (name, got) in &orders {
        assert_eq!(got, &expected, "{name}");
    }
}

// `.sortedBy { n => n.category }` on an optional field puts nil rows in the same place on
// both databases (SQLite sorts NULL first, Postgres last, unless the SQL says which).
#[tokio::test]
async fn nil_order_is_the_same_on_every_database() {
    let mut orders = Vec::new();
    for t in seeded().await {
        let st = sorted_select(vec![order("category", false), order("id", false)]);
        orders.push((t.name, ids(&t.db, &st, &[]).await));
        t.finish().await.expect("cleanup");
    }
    for pair in orders.windows(2) {
        assert_eq!(pair[0].1, pair[1].1, "{} vs {}", pair[0].0, pair[1].0);
    }
}

// --- limit, offset, first, single -----------------------------------------

// `.sortedBy { … }.limit(2)`: a literal limit is inlined.
#[tokio::test]
async fn a_literal_limit_is_inlined() {
    let st = Statement::Select(Select {
        order: vec![order("priority", false)],
        limit: Some(Count::Const(2)),
        ..note_select()
    });
    assert!(both(&st).0.ends_with(" ORDER BY \"priority\" LIMIT 2"));
    assert_eq!(render(&st, Dialect::Sqlite).params, 0);
    let all = seeded().await;
    for t in &all {
        assert_eq!(ids(&t.db, &st, &[]).await, [2, 3], "{}", t.name);
    }
    finish(all).await;
}

// `.limit(pageSize)`: a captured limit is a parameter.
#[tokio::test]
async fn a_captured_limit_is_a_parameter() {
    let st = Statement::Select(Select {
        order: vec![order("priority", false)],
        limit: Some(Count::Param(1)),
        ..note_select()
    });
    assert!(both(&st).1.ends_with(" LIMIT $1"));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            ids(&t.db, &st, &[Value::Int(3)]).await,
            [2, 3, 1],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `.limit(0)` returns no rows.
#[tokio::test]
async fn a_zero_limit_returns_nothing() {
    let st = Statement::Select(Select {
        limit: Some(Count::Const(0)),
        ..note_select()
    });
    let all = seeded().await;
    for t in &all {
        assert!(ids(&t.db, &st, &[]).await.is_empty(), "{}", t.name);
    }
    finish(all).await;
}

// A limit larger than the table returns every row.
#[tokio::test]
async fn a_large_limit_returns_every_row() {
    let st = Statement::Select(Select {
        limit: Some(Count::Const(1_000_000)),
        ..note_select()
    });
    let all = seeded().await;
    for t in &all {
        assert_eq!(ids(&t.db, &st, &[]).await.len(), 5, "{}", t.name);
    }
    finish(all).await;
}

// `.sortedBy { … }.offset(page * size).limit(size)`: paging with parameters.
#[tokio::test]
async fn limit_with_offset_pages() {
    let st = Statement::Select(Select {
        order: vec![order("priority", false)],
        limit: Some(Count::Param(1)),
        offset: Some(Count::Param(2)),
        ..note_select()
    });
    assert!(both(&st).0.ends_with(" LIMIT ?1 OFFSET ?2"));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            ids(&t.db, &st, &[Value::Int(2), Value::Int(0)]).await,
            [2, 3],
            "{}",
            t.name
        );
        assert_eq!(
            ids(&t.db, &st, &[Value::Int(2), Value::Int(2)]).await,
            [1, 5],
            "{}",
            t.name
        );
        assert_eq!(
            ids(&t.db, &st, &[Value::Int(2), Value::Int(4)]).await,
            [4],
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `.offset(n)` alone skips rows without limiting the rest.
#[tokio::test]
async fn an_offset_without_a_limit() {
    let st = Statement::Select(Select {
        order: vec![order("priority", false)],
        offset: Some(Count::Const(3)),
        ..note_select()
    });
    let all = seeded().await;
    for t in &all {
        assert_eq!(ids(&t.db, &st, &[]).await, [5, 4], "{}", t.name);
    }
    finish(all).await;
}

// An offset past the end is an empty page, not an error.
#[tokio::test]
async fn an_offset_past_the_end() {
    let st = Statement::Select(Select {
        order: vec![order("priority", false)],
        offset: Some(Count::Param(1)),
        ..note_select()
    });
    let all = seeded().await;
    for t in &all {
        assert!(
            ids(&t.db, &st, &[Value::Int(50)]).await.is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `notes.first { n => n.category == c }` is `LIMIT 1`: at most one row (`T?`).
#[tokio::test]
async fn first_is_limit_one() {
    let st = Statement::Select(Select {
        filter: Some(eq("category", 1)),
        order: vec![order("priority", false)],
        limit: Some(Count::Const(1)),
        ..note_select()
    });
    let all = seeded().await;
    for t in &all {
        assert_eq!(ids(&t.db, &st, &[text("work")]).await, [1], "{}", t.name);
        assert!(
            ids(&t.db, &st, &[text("none")]).await.is_empty(),
            "{}: nil",
            t.name
        );
    }
    finish(all).await;
}

// `notes.single { … }` is `LIMIT 2`, so the runtime can tell one row from several.
#[tokio::test]
async fn single_fetches_at_most_two_rows() {
    let st = Statement::Select(Select {
        filter: Some(eq("category", 1)),
        limit: Some(Count::Const(2)),
        ..note_select()
    });
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            ids(&t.db, &st, &[text("home")]).await.len(),
            1,
            "{}: one row",
            t.name
        );
        assert_eq!(
            ids(&t.db, &st, &[text("work")]).await.len(),
            2,
            "{}: several (a panic)",
            t.name
        );
        assert!(
            ids(&t.db, &st, &[text("none")]).await.is_empty(),
            "{}: none (a panic)",
            t.name
        );
    }
    finish(all).await;
}

// --- count ----------------------------------------------------------------

// `await notes.count()`
#[tokio::test]
async fn count_of_every_row() {
    let st = count_where(None);
    assert_eq!(both(&st).0, "SELECT COUNT(*) FROM \"note\"");
    let all = seeded().await;
    for t in &all {
        assert_eq!(count(&t.db, &st, &[]).await, Some(5), "{}", t.name);
    }
    finish(all).await;
}

// `await notes.filter { n => n.isPinned == true }.count()`
#[tokio::test]
async fn count_of_a_filter() {
    let st = count_where(Some(eq("is_pinned", 1)));
    let all = seeded().await;
    for t in &all {
        assert_eq!(
            count(&t.db, &st, &[Value::Bool(true)]).await,
            Some(2),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A count matching nothing is 0, an `Int`.
#[tokio::test]
async fn count_of_nothing_is_zero() {
    let st = count_where(Some(Predicate::Or(vec![])));
    let all = seeded().await;
    for t in &all {
        assert_eq!(count(&t.db, &st, &[]).await, Some(0), "{}", t.name);
    }
    finish(all).await;
}

// --- reserved names, enums, injection -------------------------------------

// `orders.filter { o => o.status == .paid }` on the reserved-word table: the case is text.
#[tokio::test]
async fn enum_cases_filter_on_a_reserved_word_table() {
    let table = k_db::schema::Table {
        entity: "store.Order".into(),
        name: "order".into(),
        columns: vec![
            common::id_column(),
            common::column(
                "status",
                k_db::schema::ColumnType::Enumeration(common::order_statuses()),
            ),
        ],
    };
    let insert = Statement::Insert {
        table: "order".into(),
        columns: vec!["id".into(), "status".into()],
        rows: 3,
    };
    let st = Statement::Select(Select {
        table: "order".into(),
        columns: vec!["id".into(), "status".into()],
        filter: Some(eq("status", 1)),
        order: vec![],
        limit: None,
        offset: None,
    });
    assert_eq!(
        both(&st).0,
        "SELECT \"id\", \"status\" FROM \"order\" WHERE \"status\" = ?1"
    );
    let all = common::databases().await;
    for t in &all {
        k_db::migrate::apply(
            &t.db,
            &[common::create_migration(
                "0001_orders",
                std::slice::from_ref(&table),
            )],
        )
        .await
        .expect(t.name);
        let params = vec![
            text(&common::uuid(1)),
            text("paid"),
            text(&common::uuid(2)),
            text("shipped"),
            text(&common::uuid(3)),
            text("paid"),
        ];
        common::run(&t.db, &insert, &params).await;
        assert_eq!(
            sorted_ids(&t.db, &st, &[text("paid")]).await,
            [1, 3],
            "{}",
            t.name
        );
        assert!(
            sorted_ids(&t.db, &st, &[text("Paid")]).await.is_empty(),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A hostile string compared with a column is only a value: no row matches, nothing breaks.
#[tokio::test]
async fn injection_through_a_comparison_is_a_value() {
    let st = notes_where(eq("title", 1));
    let all = seeded().await;
    for t in &all {
        for evil in [
            "' OR '1'='1",
            "x'; DROP TABLE \"note\"; --",
            "\\'; --",
            "?1",
            "$1",
        ] {
            assert!(
                sorted_ids(&t.db, &st, &[text(evil)]).await.is_empty(),
                "{}: {evil}",
                t.name
            );
        }
        assert_eq!(
            common::table_count(&t.db, "note").await,
            Some(5),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A hostile string inside a LIKE pattern is only a value too.
#[tokio::test]
async fn injection_through_like_is_a_value() {
    let st = notes_where(like("title", Like::Contains, 1));
    let all = seeded().await;
    for t in &all {
        let got = sorted_ids(&t.db, &st, &[pattern(Like::Contains, "%' OR 1=1 --")]).await;
        assert!(got.is_empty(), "{}", t.name);
    }
    finish(all).await;
}

// No rendered query contains a quote character other than the LIKE escape clause.
#[test]
fn rendered_queries_contain_no_string_literals() {
    let statements = [
        notes_where(Predicate::And(vec![
            eq("title", 1),
            like("body", Like::Contains, 2),
        ])),
        notes_where(Predicate::In {
            operand: Operand::Column("id".into()),
            list: 1,
        }),
        count_where(Some(Predicate::Not(Box::new(eq("category", 1))))),
    ];
    for st in &statements {
        for dialect in [Dialect::Sqlite, Dialect::Postgres] {
            let text = render(st, dialect).text.replace("ESCAPE '\\'", "");
            assert!(!text.contains('\''), "{dialect:?}: {text}");
        }
    }
}
