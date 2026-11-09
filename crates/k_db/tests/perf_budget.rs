//! The performance budgets of `docs/compiler/db.md` §13 that `k_db` owns:
//! rendering a typical query, and the driver's overhead per statement over
//! the raw `rusqlite` call. Ignored by default; run in release:
//!
//! ```text
//! cargo test --release -p k_db --test it -- perf_budget:: --ignored --nocapture --test-threads 1
//! ```

use std::time::{Duration, Instant};

use k_db::ir::{CmpOp, Count, Operand, Order, Predicate, Select, Statement};
use k_db::sql::{Dialect, render};
use k_db::{Database, DatabaseOptions, Value};

fn typical() -> Statement {
    Statement::Select(Select {
        table: "note".into(),
        columns: [
            "id",
            "owner_id",
            "title",
            "body",
            "is_pinned",
            "revision",
            "created_at",
        ]
        .iter()
        .map(|c| (*c).to_owned())
        .collect(),
        filter: Some(Predicate::And(vec![
            Predicate::Compare {
                left: Operand::Column("owner_id".into()),
                op: CmpOp::Eq,
                right: Operand::Param(1),
            },
            Predicate::IsNull(Operand::Column("deleted_at".into())),
        ])),
        order: vec![
            Order {
                column: "is_pinned".into(),
                descending: true,
            },
            Order {
                column: "updated_at".into(),
                descending: true,
            },
        ],
        limit: Some(Count::Const(50)),
        offset: Some(Count::Param(2)),
    })
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn rendering_a_typical_query() {
    let st = typical();
    let n = 200_000u32;
    let start = Instant::now();
    let mut bytes = 0usize;
    for i in 0..n {
        let d = if i % 2 == 0 {
            Dialect::Sqlite
        } else {
            Dialect::Postgres
        };
        bytes += render(&st, d).text.len();
    }
    let per = start.elapsed() / n;
    eprintln!("render of a typical query: {per:?} per statement ({bytes} bytes; budget 2 µs)");
    assert!(per < Duration::from_micros(2));
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn driver_overhead_over_rusqlite() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let n = 20_000u32;
    let sql = "SELECT \"id\", \"title\" FROM \"t\" WHERE \"id\" = ?1";
    // The raw library call on a prepared, cached statement.
    let raw = rusqlite::Connection::open_in_memory().expect("opens");
    raw.execute_batch(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, title TEXT); WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1000) INSERT INTO t SELECT x, 'title ' || x FROM c;",
    )
    .expect("seeds");
    let start = Instant::now();
    for i in 0..n {
        let mut s = raw.prepare_cached(sql).expect("prepares");
        let title: String = s
            .query_row([i64::from(i % 1000 + 1)], |r| r.get(1))
            .expect("row");
        assert!(!title.is_empty());
    }
    let raw_per = start.elapsed() / n;
    // The same through `Database` (blocking-pool hop, value conversion).
    let db = runtime
        .block_on(Database::connect(
            "sqlite::memory:",
            &DatabaseOptions::default(),
        ))
        .expect("connects");
    runtime
        .block_on(db.execute(
            "CREATE TABLE \"t\" (\"id\" INTEGER PRIMARY KEY, \"title\" TEXT)",
            &[],
        ))
        .expect("creates");
    runtime
        .block_on(db.execute(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1000) INSERT INTO \"t\" SELECT x, 'title ' || x FROM c",
            &[],
        ))
        .expect("seeds");
    let per = runtime.block_on(async {
        let start = Instant::now();
        for i in 0..n {
            let rows = db
                .query(sql, &[Value::Int(i64::from(i % 1000 + 1))])
                .await
                .expect("queries");
            assert_eq!(rows.len(), 1);
        }
        start.elapsed() / n
    });
    let overhead = per.saturating_sub(raw_per);
    eprintln!(
        "driver: {per:?} per point query, raw rusqlite {raw_per:?}: overhead {overhead:?} (budget 10 µs)"
    );
    assert!(overhead < Duration::from_micros(10));
}
