//! SQL generation: the exact text of every IR construct in both dialects
//! (`docs/compiler/db.md`, "SQL generation").

use k_db::ir::{ArithOp, CmpOp, Count, Like, Operand, Order, Predicate, Select, Statement};
use k_db::sql::{Dialect, like_pattern, quote_identifier, render};

fn col(c: &str) -> Operand {
    Operand::Column(c.to_owned())
}

fn select(table: &str, columns: &[&str]) -> Select {
    Select {
        table: table.to_owned(),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        filter: None,
        order: vec![],
        limit: None,
        offset: None,
    }
}

fn both(s: &Statement) -> (String, String) {
    (
        render(s, Dialect::Sqlite).text,
        render(s, Dialect::Postgres).text,
    )
}

#[test]
fn identifiers_are_always_quoted_and_escaped() {
    assert_eq!(quote_identifier("order"), "\"order\"");
    assert_eq!(quote_identifier("we\"ird"), "\"we\"\"ird\"");
    assert_eq!(quote_identifier(""), "\"\"");
}

#[test]
fn a_plain_select_quotes_table_and_columns() {
    let (s, p) = both(&Statement::Select(select("user", &["id", "email"])));
    assert_eq!(s, "SELECT \"id\", \"email\" FROM \"user\"");
    assert_eq!(p, s);
}

#[test]
fn comparisons_use_numbered_parameters_per_dialect() {
    let mut q = select("note", &["id"]);
    q.filter = Some(Predicate::Compare {
        left: col("owner_id"),
        op: CmpOp::Eq,
        right: Operand::Param(1),
    });
    let st = Statement::Select(q);
    let (s, p) = both(&st);
    assert_eq!(s, "SELECT \"id\" FROM \"note\" WHERE \"owner_id\" = ?1");
    assert_eq!(p, "SELECT \"id\" FROM \"note\" WHERE \"owner_id\" = $1");
    assert_eq!(render(&st, Dialect::Sqlite).params, 1);
}

#[test]
fn every_operator_renders() {
    for (op, text) in [
        (CmpOp::Eq, "="),
        (CmpOp::Ne, "<>"),
        (CmpOp::Lt, "<"),
        (CmpOp::Le, "<="),
        (CmpOp::Gt, ">"),
        (CmpOp::Ge, ">="),
    ] {
        let st = Statement::Count(Select {
            filter: Some(Predicate::Compare {
                left: col("n"),
                op,
                right: Operand::Param(1),
            }),
            ..select("t", &[])
        });
        assert_eq!(
            render(&st, Dialect::Sqlite).text,
            format!("SELECT COUNT(*) FROM \"t\" WHERE \"n\" {text} ?1")
        );
    }
}

#[test]
fn nested_boolean_logic_is_parenthesized() {
    let p = Predicate::And(vec![
        Predicate::Compare {
            left: col("a"),
            op: CmpOp::Eq,
            right: Operand::Param(1),
        },
        Predicate::Or(vec![
            Predicate::IsNull(col("b")),
            Predicate::Not(Box::new(Predicate::IsNotNull(col("c")))),
        ]),
    ]);
    let st = Statement::Delete {
        table: "t".into(),
        filter: Some(p),
    };
    assert_eq!(
        render(&st, Dialect::Postgres).text,
        "DELETE FROM \"t\" WHERE \"a\" = $1 AND (\"b\" IS NULL OR NOT (\"c\" IS NOT NULL))"
    );
}

#[test]
fn empty_and_single_boolean_lists() {
    let st = |p| {
        Statement::Count(Select {
            filter: Some(p),
            ..select("t", &[])
        })
    };
    assert_eq!(
        render(&st(Predicate::And(vec![])), Dialect::Sqlite).text,
        "SELECT COUNT(*) FROM \"t\" WHERE 1 = 1"
    );
    assert_eq!(
        render(&st(Predicate::Or(vec![])), Dialect::Sqlite).text,
        "SELECT COUNT(*) FROM \"t\" WHERE 1 = 0"
    );
    assert_eq!(
        render(
            &st(Predicate::Or(vec![Predicate::IsNull(col("x"))])),
            Dialect::Sqlite
        )
        .text,
        "SELECT COUNT(*) FROM \"t\" WHERE \"x\" IS NULL"
    );
}

#[test]
fn order_limit_and_offset() {
    let mut q = select("note", &["id"]);
    q.order = vec![
        Order {
            column: "is_pinned".into(),
            descending: true,
        },
        Order {
            column: "title".into(),
            descending: false,
        },
    ];
    q.limit = Some(Count::Const(50));
    q.offset = Some(Count::Param(1));
    let (s, p) = both(&Statement::Select(q));
    assert_eq!(
        s,
        "SELECT \"id\" FROM \"note\" ORDER BY \"is_pinned\" DESC, \"title\" LIMIT 50 OFFSET ?1"
    );
    assert_eq!(
        p,
        "SELECT \"id\" FROM \"note\" ORDER BY \"is_pinned\" DESC NULLS LAST, \"title\" NULLS FIRST LIMIT 50 OFFSET $1"
    );
}

#[test]
fn offset_without_limit() {
    let q = Select {
        offset: Some(Count::Const(10)),
        ..select("t", &["a"])
    };
    let (s, p) = both(&Statement::Select(q));
    assert_eq!(s, "SELECT \"a\" FROM \"t\" LIMIT -1 OFFSET 10");
    assert_eq!(p, "SELECT \"a\" FROM \"t\" LIMIT ALL OFFSET 10");
}

#[test]
fn membership_and_like() {
    let q = Select {
        filter: Some(Predicate::And(vec![
            Predicate::In {
                operand: col("id"),
                list: 1,
            },
            Predicate::Like {
                column: "title".into(),
                kind: Like::Contains,
                param: 2,
            },
        ])),
        ..select("note", &["id"])
    };
    let (s, p) = both(&Statement::Select(q));
    assert_eq!(
        s,
        "SELECT \"id\" FROM \"note\" WHERE \"id\" IN (SELECT value FROM json_each(?1)) AND \"title\" LIKE ?2 ESCAPE '\\'"
    );
    assert_eq!(
        p,
        "SELECT \"id\" FROM \"note\" WHERE \"id\" = ANY($1) AND \"title\" LIKE $2 ESCAPE '\\'"
    );
}

#[test]
fn like_patterns_escape_wildcards() {
    assert_eq!(
        like_pattern(Like::Contains, "50%_off\\"),
        "%50\\%\\_off\\\\%"
    );
    assert_eq!(like_pattern(Like::StartsWith, "a"), "a%");
    assert_eq!(like_pattern(Like::EndsWith, ""), "%");
}

#[test]
fn multi_row_insert_numbers_parameters_row_by_row() {
    let st = Statement::Insert {
        table: "tag".into(),
        columns: vec!["id".into(), "name".into()],
        rows: 2,
    };
    let (s, p) = both(&st);
    assert_eq!(
        s,
        "INSERT INTO \"tag\" (\"id\", \"name\") VALUES (?1, ?2), (?3, ?4)"
    );
    assert_eq!(
        p,
        "INSERT INTO \"tag\" (\"id\", \"name\") VALUES ($1, $2), ($3, $4)"
    );
    assert_eq!(render(&st, Dialect::Sqlite).params, 4);
}

#[test]
fn versioned_update() {
    let st = Statement::Update {
        table: "note".into(),
        set: vec![
            ("title".into(), Operand::Param(1)),
            ("revision".into(), Operand::Param(2)),
        ],
        filter: Some(Predicate::And(vec![
            Predicate::Compare {
                left: col("id"),
                op: CmpOp::Eq,
                right: Operand::Param(3),
            },
            Predicate::Compare {
                left: col("revision"),
                op: CmpOp::Eq,
                right: Operand::Param(4),
            },
        ])),
    };
    assert_eq!(
        render(&st, Dialect::Sqlite).text,
        "UPDATE \"note\" SET \"title\" = ?1, \"revision\" = ?2 WHERE \"id\" = ?3 AND \"revision\" = ?4"
    );
}

#[test]
fn values_never_appear_in_sql_text() {
    // There is no literal operand in the IR, so any statement's text is
    // made of keywords, quoted identifiers, parameters and constant counts.
    let st = Statement::Select(Select {
        filter: Some(Predicate::Compare {
            left: col("email"),
            op: CmpOp::Eq,
            right: Operand::Param(1),
        }),
        limit: Some(Count::Const(1)),
        ..select("user", &["id"])
    });
    let text = render(&st, Dialect::Sqlite).text;
    assert!(!text.contains('\''), "no string literals: {text}");
}

// `product.stock -= quantity` in a bulk update: the database computes the new value.
#[test]
fn bulk_updates_assign_arithmetic_on_columns() {
    let st = Statement::Update {
        table: "product".into(),
        set: vec![
            (
                "stock".into(),
                Operand::Arith {
                    left: Box::new(col("stock")),
                    op: ArithOp::Sub,
                    right: Box::new(Operand::Param(1)),
                },
            ),
            (
                "sold".into(),
                Operand::Arith {
                    left: Box::new(Operand::Param(2)),
                    op: ArithOp::Add,
                    right: Box::new(col("sold")),
                },
            ),
        ],
        filter: Some(Predicate::Compare {
            left: col("id"),
            op: CmpOp::Eq,
            right: Operand::Param(3),
        }),
    };
    let (s, p) = both(&st);
    assert_eq!(
        s,
        "UPDATE \"product\" SET \"stock\" = (\"stock\" - ?1), \"sold\" = (?2 + \"sold\") WHERE \"id\" = ?3"
    );
    assert_eq!(
        p,
        "UPDATE \"product\" SET \"stock\" = (\"stock\" - $1), \"sold\" = ($2 + \"sold\") WHERE \"id\" = $3"
    );
    assert_eq!(render(&st, Dialect::Postgres).params, 3);
}
