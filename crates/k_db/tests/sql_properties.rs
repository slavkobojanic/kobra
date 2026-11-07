//! Property tests for SQL generation (`docs/compiler/db.md` §5,
//! `docs/compiler/testing.md`, "Stage 3 properties"): over generated query
//! IR with hostile identifiers and values,
//!
//! 1. **no value is ever interpolated**: rendered SQL, with its quoted
//!    identifiers and numbered parameters taken out, is made only of SQL
//!    keywords, punctuation and the constants the renderer itself writes
//!    (`1 = 1`, `LIMIT -1`, `ESCAPE '\'`, a source constant `LIMIT 50`), in
//!    both dialects; every quoted identifier is one of the statement's
//!    locked names, unquoted back exactly; every parameter number is at most
//!    [`Sql::params`];
//! 2. **SQLite agrees with a reference evaluation** in Rust: generated
//!    tables (hostile table and column names) and rows (hostile text: quotes,
//!    `%`, `_`, `\`, `--`, `;`, NUL-free Unicode, case variants) are inserted
//!    with rendered `INSERT`s, and every rendered `SELECT`, `COUNT`,
//!    `UPDATE` and `DELETE` returns or changes exactly the rows a
//!    three-valued-logic evaluator says (SQL `NULL` semantics, `LIKE`
//!    case-sensitive with its escapes, `IN` over a JSON list, byte-order
//!    text comparison, `ORDER BY` with `NULL` first, `LIMIT`/`OFFSET`).
//!
//! `PROPTEST_CASES=N` sets the number of cases (default 256).

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use k_db::ir::{ArithOp, CmpOp, Count, Like, Operand, Order, Predicate, Select, Statement};
use k_db::sql::{Dialect, Sql, like_pattern, quote_identifier, render};
use k_db::{Database, DatabaseOptions, Value};
use proptest::prelude::*;

/// Table and column names a lock could hold: reserved words, quotes,
/// spaces, comment and statement separators, Unicode.
const NAMES: &[&str] = &[
    "order",
    "user",
    "select",
    "a\"b",
    "\"",
    "x y",
    ";--",
    "Robert'); DROP TABLE t;--",
    "\u{e9}t\u{e9}",
    "group by",
    "$1",
    "?1",
    "plain",
    "\\",
    "%_",
];

/// Text values: everything that would break a statement if it were
/// interpolated, and `LIKE` metacharacters.
const TEXTS: &[&str] = &[
    "",
    "a",
    "A",
    "ab",
    "'",
    "''",
    "\"",
    "%",
    "_",
    "\\",
    "a%b",
    "a_b",
    "x\\%",
    "--",
    "; DROP TABLE t; --",
    "' OR '1'='1",
    "\u{e9}",
    "\u{1F642}",
    "?1",
    "$1",
    "Ab",
];

/// A generated table: name, the names of its id, int and text columns.
#[derive(Clone, Debug)]
struct Table {
    name: String,
    id: String,
    int: String,
    text: String,
}

/// One row: id, nullable int, nullable text.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    id: i64,
    int: Option<i64>,
    text: Option<String>,
}

fn table() -> impl Strategy<Value = Table> {
    prop::sample::subsequence(NAMES.to_vec(), 4)
        .prop_shuffle()
        .prop_map(|n| Table {
            name: n[0].to_owned(),
            id: n[1].to_owned(),
            int: n[2].to_owned(),
            text: n[3].to_owned(),
        })
}

fn rows() -> impl Strategy<Value = Vec<Row>> {
    prop::collection::vec(
        (
            prop::option::weighted(0.8, -3i64..4),
            prop::option::weighted(0.8, prop::sample::select(TEXTS.to_vec())),
        ),
        0..8,
    )
    .prop_map(|v| {
        v.into_iter()
            .enumerate()
            .map(|(i, (int, text))| Row {
                id: i64::try_from(i).unwrap() + 1,
                int,
                text: text.map(str::to_owned),
            })
            .collect()
    })
}

/// The shape of a predicate before parameters are numbered.
#[derive(Clone, Debug)]
enum P {
    /// `int op param`, or `param op int` when `flipped`.
    CmpInt(CmpOp, i64, bool),
    /// `text op param`.
    CmpText(CmpOp, String),
    /// `id op int` (two columns).
    CmpColumns(CmpOp),
    IsNull(bool),
    IsNotNull(bool),
    And(Vec<P>),
    Or(Vec<P>),
    Not(Box<P>),
    /// `int IN (list)`.
    In(Vec<i64>),
    Like(Like, String),
}

fn cmp_op() -> impl Strategy<Value = CmpOp> {
    prop::sample::select(vec![
        CmpOp::Eq,
        CmpOp::Ne,
        CmpOp::Lt,
        CmpOp::Le,
        CmpOp::Gt,
        CmpOp::Ge,
    ])
}

fn shape() -> impl Strategy<Value = P> {
    let leaf = prop_oneof![
        (cmp_op(), -3i64..4, any::<bool>()).prop_map(|(o, v, f)| P::CmpInt(o, v, f)),
        (cmp_op(), prop::sample::select(TEXTS.to_vec()))
            .prop_map(|(o, t)| P::CmpText(o, t.to_owned())),
        cmp_op().prop_map(P::CmpColumns),
        any::<bool>().prop_map(P::IsNull),
        any::<bool>().prop_map(P::IsNotNull),
        prop::collection::vec(-3i64..4, 0..4).prop_map(P::In),
        (
            prop::sample::select(vec![Like::Contains, Like::StartsWith, Like::EndsWith]),
            prop::sample::select(TEXTS.to_vec())
        )
            .prop_map(|(k, t)| P::Like(k, t.to_owned())),
    ];
    leaf.prop_recursive(3, 16, 3, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..3).prop_map(P::And),
            prop::collection::vec(inner.clone(), 0..3).prop_map(P::Or),
            inner.prop_map(|p| P::Not(Box::new(p))),
        ]
    })
}

/// Numbers the parameters of `p` and builds the IR.
fn lower(p: &P, t: &Table, params: &mut Vec<Value>) -> Predicate {
    let mut param = |v: Value| {
        params.push(v);
        Operand::Param(u32::try_from(params.len()).unwrap())
    };
    let col = |c: &str| Operand::Column(c.to_owned());
    match p {
        P::CmpInt(op, v, false) => Predicate::Compare {
            left: col(&t.int),
            op: *op,
            right: param(Value::Int(*v)),
        },
        P::CmpInt(op, v, true) => Predicate::Compare {
            left: param(Value::Int(*v)),
            op: *op,
            right: col(&t.int),
        },
        P::CmpText(op, s) => Predicate::Compare {
            left: col(&t.text),
            op: *op,
            right: param(Value::Text(s.clone())),
        },
        P::CmpColumns(op) => Predicate::Compare {
            left: col(&t.id),
            op: *op,
            right: col(&t.int),
        },
        P::IsNull(int) => Predicate::IsNull(col(if *int { &t.int } else { &t.text })),
        P::IsNotNull(int) => Predicate::IsNotNull(col(if *int { &t.int } else { &t.text })),
        P::And(ps) => Predicate::And(ps.iter().map(|q| lower(q, t, params)).collect()),
        P::Or(ps) => Predicate::Or(ps.iter().map(|q| lower(q, t, params)).collect()),
        P::Not(q) => Predicate::Not(Box::new(lower(q, t, params))),
        P::In(list) => {
            let json = serde_json::to_string(list).unwrap();
            let Operand::Param(n) = param(Value::Text(json)) else {
                unreachable!()
            };
            Predicate::In {
                operand: col(&t.int),
                list: n,
            }
        }
        P::Like(kind, s) => {
            let Operand::Param(n) = param(Value::Text(like_pattern(*kind, s))) else {
                unreachable!()
            };
            Predicate::Like {
                column: t.text.clone(),
                kind: *kind,
                param: n,
            }
        }
    }
}

/// SQL's three-valued logic: `None` is unknown.
fn holds(p: &P, r: &Row) -> Option<bool> {
    let ord = |op: CmpOp, o: std::cmp::Ordering| {
        use std::cmp::Ordering::{Equal, Greater, Less};
        match op {
            CmpOp::Eq => o == Equal,
            CmpOp::Ne => o != Equal,
            CmpOp::Lt => o == Less,
            CmpOp::Le => o != Greater,
            CmpOp::Gt => o == Greater,
            CmpOp::Ge => o != Less,
        }
    };
    match p {
        P::CmpInt(op, v, false) => r.int.map(|i| ord(*op, i.cmp(v))),
        P::CmpInt(op, v, true) => r.int.map(|i| ord(*op, v.cmp(&i))),
        // SQLite's BINARY collation compares the UTF-8 bytes.
        P::CmpText(op, s) => r
            .text
            .as_ref()
            .map(|x| ord(*op, x.as_bytes().cmp(s.as_bytes()))),
        P::CmpColumns(op) => r.int.map(|i| ord(*op, r.id.cmp(&i))),
        P::IsNull(int) => Some(if *int {
            r.int.is_none()
        } else {
            r.text.is_none()
        }),
        P::IsNotNull(int) => Some(if *int {
            r.int.is_some()
        } else {
            r.text.is_some()
        }),
        P::And(ps) => ps
            .iter()
            .try_fold(Some(true), |acc, q| match (acc, holds(q, r)) {
                (_, Some(false)) | (Some(false), _) => Err(()),
                (Some(true), Some(true)) => Ok(Some(true)),
                _ => Ok(None),
            })
            .unwrap_or(Some(false)),
        P::Or(ps) => ps
            .iter()
            .try_fold(Some(false), |acc, q| match (acc, holds(q, r)) {
                (_, Some(true)) | (Some(true), _) => Err(()),
                (Some(false), Some(false)) => Ok(Some(false)),
                _ => Ok(None),
            })
            .unwrap_or(Some(true)),
        P::Not(q) => holds(q, r).map(|b| !b),
        // `x IN ()` is false even when `x` is NULL.
        P::In(list) if list.is_empty() => Some(false),
        P::In(list) => r.int.map(|i| list.contains(&i)),
        P::Like(kind, s) => r.text.as_ref().map(|x| match kind {
            Like::Contains => x.contains(s.as_str()),
            Like::StartsWith => x.starts_with(s.as_str()),
            Like::EndsWith => x.ends_with(s.as_str()),
        }),
    }
}

/// Checks that `sql` interpolates nothing: identifiers are quoted names of
/// `names`, parameters are numbered within `sql.params`, and what is left
/// is keywords, punctuation and renderer constants.
fn check_text(sql: &Sql, names: &[&str], dialect: Dialect) -> Result<(), String> {
    let text = &sql.text;
    let mut rest = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some((_, '"')) if chars.peek().is_some_and(|&(_, d)| d == '"') => {
                            chars.next();
                            name.push('"');
                        }
                        Some((_, '"')) => break,
                        Some((_, d)) => name.push(d),
                        None => return Err(format!("unterminated identifier at {i}: {text}")),
                    }
                }
                if !names.contains(&name.as_str()) {
                    return Err(format!("identifier {name:?} is not a locked name: {text}"));
                }
                rest.push('I');
            }
            '?' | '$' => {
                let mut n = String::new();
                while let Some(&(_, d)) = chars.peek() {
                    if !d.is_ascii_digit() {
                        break;
                    }
                    n.push(d);
                    chars.next();
                }
                let expected = if dialect == Dialect::Sqlite { '?' } else { '$' };
                let number: u32 = n.parse().map_err(|_| format!("bare {c} at {i}: {text}"))?;
                if c != expected || number == 0 || number > sql.params {
                    return Err(format!("parameter {c}{n} out of range: {text}"));
                }
                rest.push('P');
            }
            c => rest.push(c),
        }
    }
    // The only string literal the renderer writes is LIKE's escape.
    let rest = rest
        .replace(" ESCAPE '\\'", " ESCAPE")
        .replace("(SELECT value FROM json_each(P))", "(P)");
    if let Some(bad) = rest.chars().find(|c| {
        !(c.is_ascii_uppercase()
            || c.is_ascii_digit()
            || " ,()=<>*+-".contains(*c)
            || *c == 'I'
            || *c == 'P')
    }) {
        return Err(format!(
            "unexpected {bad:?} outside identifiers: {text} (rest: {rest})"
        ));
    }
    // Numbers only where the renderer writes them.
    for word in rest.split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
        if word.chars().any(|c| c.is_ascii_digit()) && !matches!(word, "1" | "0" | "-1" | "50") {
            return Err(format!("unexpected number {word:?}: {text}"));
        }
    }
    Ok(())
}

fn select(t: &Table, filter: Option<Predicate>) -> Select {
    Select {
        table: t.name.clone(),
        columns: vec![t.id.clone(), t.int.clone(), t.text.clone()],
        filter,
        order: vec![Order {
            column: t.id.clone(),
            descending: false,
        }],
        limit: None,
        offset: None,
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

async fn setup(t: &Table, rows: &[Row]) -> Database {
    let db = Database::connect("sqlite::memory:", &DatabaseOptions::default())
        .await
        .unwrap();
    let ddl = format!(
        "CREATE TABLE {} ({} INTEGER PRIMARY KEY, {} INTEGER, {} TEXT)",
        quote_identifier(&t.name),
        quote_identifier(&t.id),
        quote_identifier(&t.int),
        quote_identifier(&t.text)
    );
    db.execute(&ddl, &[]).await.unwrap();
    if !rows.is_empty() {
        let insert = Statement::Insert {
            table: t.name.clone(),
            columns: vec![t.id.clone(), t.int.clone(), t.text.clone()],
            rows: u32::try_from(rows.len()).unwrap(),
        };
        let sql = render(&insert, Dialect::Sqlite);
        check_text(&sql, &[&t.name, &t.id, &t.int, &t.text], Dialect::Sqlite).unwrap();
        let params: Vec<Value> = rows
            .iter()
            .flat_map(|r| {
                [
                    Value::Int(r.id),
                    r.int.map_or(Value::Null, Value::Int),
                    r.text.clone().map_or(Value::Null, Value::Text),
                ]
            })
            .collect();
        assert_eq!(
            db.execute(&sql.text, &params).await.unwrap(),
            rows.len() as u64
        );
    }
    db
}

async fn table_rows(db: &Database, t: &Table) -> Vec<Row> {
    let sql = render(&Statement::Select(select(t, None)), Dialect::Sqlite);
    db.query(&sql.text, &[])
        .await
        .unwrap()
        .into_iter()
        .map(|r| Row {
            id: r.values[0].as_int().unwrap(),
            int: r.values[1].as_int(),
            text: r.values[2].as_text().map(str::to_owned),
        })
        .collect()
}

fn config() -> ProptestConfig {
    if std::env::var_os("PROPTEST_CASES").is_some() {
        ProptestConfig::default()
    } else {
        ProptestConfig::with_cases(256)
    }
}

proptest! {
    #![proptest_config(config())]

    /// Rendered SQL never holds a value, in either dialect, for every kind
    /// of statement.
    #[test]
    fn rendered_sql_interpolates_nothing(t in table(), p in shape(), n in 0u32..4, limit in prop::option::of(0u64..3)) {
        let mut params = Vec::new();
        let filter = lower(&p, &t, &mut params);
        let next = u32::try_from(params.len()).unwrap() + 1;
        let names = [t.name.as_str(), t.id.as_str(), t.int.as_str(), t.text.as_str()];
        let mut s = select(&t, Some(filter.clone()));
        s.limit = limit.map(|l| if l == 0 { Count::Const(50) } else { Count::Param(next) });
        s.offset = (n % 2 == 1).then_some(Count::Const(1));
        let statements = [
            Statement::Select(s.clone()),
            Statement::Count(s),
            Statement::Update {
                table: t.name.clone(),
                set: vec![(t.int.clone(), Operand::Arith {
                    left: Box::new(Operand::Column(t.int.clone())),
                    op: if n % 2 == 0 { ArithOp::Add } else { ArithOp::Sub },
                    right: Box::new(Operand::Param(next)),
                })],
                filter: Some(filter.clone()),
            },
            Statement::Delete { table: t.name.clone(), filter: Some(filter) },
            Statement::Insert { table: t.name.clone(), columns: vec![t.id.clone(), t.text.clone()], rows: n },
        ];
        for st in &statements {
            for dialect in [Dialect::Sqlite, Dialect::Postgres] {
                let sql = render(st, dialect);
                if let Err(e) = check_text(&sql, &names, dialect) {
                    panic!("{e}\n{st:?}");
                }
            }
        }
    }

    /// SQLite returns and changes exactly the rows the reference says.
    #[test]
    fn sqlite_agrees_with_the_reference(t in table(), rows in rows(), p in shape(), limit in prop::option::of(0u64..4), offset in prop::option::of(0u64..3), delta in -2i64..3) {
        let rt = runtime();
        rt.block_on(async {
            let db = setup(&t, &rows).await;
            prop_assert_eq!(table_rows(&db, &t).await, rows.clone(), "inserted rows read back");
            let mut params = Vec::new();
            let filter = lower(&p, &t, &mut params);
            let matching: Vec<Row> = rows.iter().filter(|r| holds(&p, r) == Some(true)).cloned().collect();

            // SELECT … WHERE … ORDER BY id LIMIT ? OFFSET ?
            let mut s = select(&t, Some(filter.clone()));
            let mut select_params = params.clone();
            let count_param = |v: u64, ps: &mut Vec<Value>| {
                ps.push(Value::Int(i64::try_from(v).unwrap()));
                Count::Param(u32::try_from(ps.len()).unwrap())
            };
            s.limit = limit.map(|l| count_param(l, &mut select_params));
            s.offset = offset.map(|o| count_param(o, &mut select_params));
            let sql = render(&Statement::Select(s.clone()), Dialect::Sqlite);
            let got: Vec<i64> = db.query(&sql.text, &select_params).await.unwrap()
                .iter().map(|r| r.values[0].as_int().unwrap()).collect();
            let want: Vec<i64> = matching.iter().map(|r| r.id)
                .skip(usize::try_from(offset.unwrap_or(0)).unwrap())
                .take(usize::try_from(limit.unwrap_or(u64::MAX)).unwrap_or(usize::MAX))
                .collect();
            prop_assert_eq!(&got, &want, "SELECT {}\nparams {:?}", sql.text, select_params);

            // ORDER BY the nullable text column: NULL first ascending, last
            // descending, byte order otherwise; ties broken by id.
            for descending in [false, true] {
                let mut ordered = select(&t, Some(filter.clone()));
                ordered.order = vec![
                    Order { column: t.text.clone(), descending },
                    Order { column: t.id.clone(), descending: false },
                ];
                let sql = render(&Statement::Select(ordered), Dialect::Sqlite);
                let got: Vec<i64> = db.query(&sql.text, &params).await.unwrap()
                    .iter().map(|r| r.values[0].as_int().unwrap()).collect();
                let mut want = matching.clone();
                want.sort_by(|a, b| {
                    let key = |r: &Row| r.text.as_ref().map(|s| s.as_bytes().to_vec());
                    let o = key(a).cmp(&key(b));
                    (if descending { o.reverse() } else { o }).then(a.id.cmp(&b.id))
                });
                let want: Vec<i64> = want.iter().map(|r| r.id).collect();
                prop_assert_eq!(&got, &want, "ORDER BY {}", sql.text);
            }

            // COUNT(*)
            let sql = render(&Statement::Count(select(&t, Some(filter.clone()))), Dialect::Sqlite);
            let n = db.query(&sql.text, &params).await.unwrap()[0].values[0].as_int().unwrap();
            prop_assert_eq!(n, i64::try_from(matching.len()).unwrap(), "COUNT {}", sql.text);

            // UPDATE … SET int = (int + ?) WHERE …
            let mut update_params = params.clone();
            update_params.push(Value::Int(delta));
            let update = Statement::Update {
                table: t.name.clone(),
                set: vec![(t.int.clone(), Operand::Arith {
                    left: Box::new(Operand::Column(t.int.clone())),
                    op: ArithOp::Add,
                    right: Box::new(Operand::Param(u32::try_from(update_params.len()).unwrap())),
                })],
                filter: Some(filter.clone()),
            };
            let sql = render(&update, Dialect::Sqlite);
            let changed = db.execute(&sql.text, &update_params).await.unwrap();
            prop_assert_eq!(changed, matching.len() as u64, "UPDATE {}", sql.text);
            let updated: Vec<Row> = rows.iter().map(|r| {
                if holds(&p, r) == Some(true) {
                    Row { int: r.int.map(|i| i + delta), ..r.clone() }
                } else {
                    r.clone()
                }
            }).collect();
            prop_assert_eq!(table_rows(&db, &t).await, updated.clone(), "after UPDATE {}", sql.text);

            // DELETE … WHERE … (the filter sees the updated rows)
            let sql = render(&Statement::Delete { table: t.name.clone(), filter: Some(filter) }, Dialect::Sqlite);
            let deleted = db.execute(&sql.text, &params).await.unwrap();
            let kept: Vec<Row> = updated.iter().filter(|r| holds(&p, r) != Some(true)).cloned().collect();
            prop_assert_eq!(deleted, (updated.len() - kept.len()) as u64, "DELETE {}", sql.text);
            prop_assert_eq!(table_rows(&db, &t).await, kept, "after DELETE {}", sql.text);
            Ok(())
        })?;
    }
}

/// The text checker rejects what it must: a value written into the SQL.
#[test]
fn the_text_checker_finds_interpolated_values() {
    let names = ["t", "c"];
    let ok = Sql {
        text: "SELECT \"c\" FROM \"t\" WHERE \"c\" = ?1".into(),
        params: 1,
    };
    assert!(check_text(&ok, &names, Dialect::Sqlite).is_ok());
    for bad in [
        "SELECT \"c\" FROM \"t\" WHERE \"c\" = 'x'",
        "SELECT \"c\" FROM \"t\" WHERE \"c\" = 42",
        "SELECT \"c\" FROM \"t\" WHERE \"c\" = ?2",
        "SELECT \"c\" FROM \"u\"",
        "SELECT \"c\" FROM \"t\" WHERE \"c\" = $1",
    ] {
        let sql = Sql {
            text: bad.into(),
            params: 1,
        };
        assert!(check_text(&sql, &names, Dialect::Sqlite).is_err(), "{bad}");
    }
}
