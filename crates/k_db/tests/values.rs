//! Parameter encoding and row decoding for every column type
//! (`docs/compiler/db.md` §3, §4 "Parameter encoding" and §6 "Values"):
//! the K value written as its documented parameter comes back as the same
//! value on every database, and comparisons and ordering on a column follow
//! the K type's semantics, not the storage type's.

mod common;

use common::{column, create_migration, databases, finish, id_column, text, uuid};
use k_db::ir::{CmpOp, Order, Select, Statement};
use k_db::migrate;
use k_db::schema::{Column, ColumnType, Table};
use k_db::sql::render;
use k_db::{Database, Value};

/// A table `sample` with an id and one column `v` of `ty`.
fn sample(ty: ColumnType, nullable: bool) -> Table {
    Table {
        entity: "x.Sample".into(),
        name: "sample".into(),
        columns: vec![
            id_column(),
            Column {
                nullable,
                ..column("v", ty)
            },
        ],
    }
}

fn value_columns(ty: &ColumnType) -> Vec<String> {
    if *ty == ColumnType::Money {
        vec!["v_amount".into(), "v_currency".into()]
    } else {
        vec!["v".into()]
    }
}

/// Creates `sample` on `db` and inserts one row per entry of `values`
/// (each entry is the parameters of the value columns).
async fn setup(
    db: &Database,
    ty: &ColumnType,
    nullable: bool,
    values: &[Vec<Value>],
) -> Result<(), k_db::DbError> {
    migrate::apply(
        db,
        &[create_migration(
            "0001_sample",
            &[sample(ty.clone(), nullable)],
        )],
    )
    .await?;
    let mut columns = vec!["id".to_owned()];
    columns.extend(value_columns(ty));
    let insert = Statement::Insert {
        table: "sample".into(),
        columns,
        rows: 1,
    };
    let sql = render(&insert, db.dialect()).text;
    for (i, v) in values.iter().enumerate() {
        let mut params = vec![text(&uuid(u32::try_from(i).unwrap_or(0) + 1))];
        params.extend(v.iter().cloned());
        db.execute(&sql, &params).await?;
    }
    Ok(())
}

/// The value columns of every row, in insertion order.
async fn read_back(db: &Database, ty: &ColumnType) -> Result<Vec<Vec<Value>>, k_db::DbError> {
    let st = Statement::Select(Select {
        table: "sample".into(),
        columns: value_columns(ty),
        filter: None,
        order: vec![Order {
            column: "id".into(),
            descending: false,
        }],
        limit: None,
        offset: None,
    });
    let rows = db.query(&render(&st, db.dialect()).text, &[]).await?;
    Ok(rows.into_iter().map(|r| r.values).collect())
}

/// Writes `values` into a `sample` column of `ty` and returns what reads back.
async fn round_trip(
    db: &Database,
    ty: ColumnType,
    nullable: bool,
    values: &[Vec<Value>],
) -> Result<Vec<Vec<Value>>, k_db::DbError> {
    setup(db, &ty, nullable, values).await?;
    read_back(db, &ty).await
}

/// The row numbers of `sample` rows where `v op param`, ordered by `v` then id.
async fn where_v(
    db: &Database,
    op: CmpOp,
    param: Value,
    descending: bool,
) -> Result<Vec<u32>, k_db::DbError> {
    let st = Statement::Select(Select {
        table: "sample".into(),
        columns: vec!["id".into()],
        filter: Some(common::cmp("v", op, 1)),
        order: vec![
            Order {
                column: "v".into(),
                descending,
            },
            Order {
                column: "id".into(),
                descending: false,
            },
        ],
        limit: None,
        offset: None,
    });
    let rows = db.query(&render(&st, db.dialect()).text, &[param]).await?;
    Ok(rows
        .iter()
        .map(|r| common::number(r.values[0].as_text().unwrap_or_default()))
        .collect())
}

/// A decimal's text without insignificant zeros (`12.3400` → `12.34`, `5.0` → `5`).
fn normalized_decimal(v: &Value) -> Option<String> {
    let s = v.as_text()?;
    if !s.contains('.') {
        return Some(s.to_owned());
    }
    Some(s.trim_end_matches('0').trim_end_matches('.').to_owned())
}

fn one(values: &[&str]) -> Vec<Vec<Value>> {
    values.iter().map(|v| vec![text(v)]).collect()
}

// `Uuid` binds as its canonical lowercase text and reads back as the same text.
#[tokio::test]
async fn uuids_round_trip() {
    let all = databases().await;
    for t in &all {
        let ids = [
            "0b6f3f9e-1c2d-4e5f-8a9b-0c1d2e3f4a5b",
            "ffffffff-ffff-4fff-bfff-ffffffffffff",
        ];
        let got = round_trip(&t.db, ColumnType::Uuid, false, &one(&ids))
            .await
            .expect(t.name);
        assert_eq!(got, one(&ids), "{}", t.name);
    }
    finish(all).await;
}

// `String` round trips exactly, including non-ASCII and control characters.
#[tokio::test]
async fn strings_round_trip() {
    let all = databases().await;
    let samples = [
        "",
        " ",
        "naïve café",
        "日本語",
        "🎉🎊",
        "tab\there",
        "quote ' \" `",
        "\u{2028}",
    ];
    for t in &all {
        let got = round_trip(&t.db, ColumnType::Text, false, &one(&samples))
            .await
            .expect(t.name);
        assert_eq!(got, one(&samples), "{}", t.name);
    }
    finish(all).await;
}

// A long string (1 MiB) round trips.
#[tokio::test]
async fn long_strings_round_trip() {
    let all = databases().await;
    let long = "k".repeat(1 << 20);
    for t in &all {
        let got = round_trip(&t.db, ColumnType::Text, false, &one(&[&long]))
            .await
            .expect(t.name);
        assert_eq!(
            got[0][0].as_text().map(str::len),
            Some(1 << 20),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// `Int` round trips across the safe-integer range and beyond (bigint / INTEGER are 64-bit).
#[tokio::test]
async fn integers_round_trip() {
    let all = databases().await;
    let samples = [
        0,
        -1,
        1,
        9_007_199_254_740_991,
        -9_007_199_254_740_991,
        i64::MAX,
        i64::MIN,
    ];
    let values: Vec<Vec<Value>> = samples.iter().map(|n| vec![Value::Int(*n)]).collect();
    for t in &all {
        let got = round_trip(&t.db, ColumnType::Integer, false, &values)
            .await
            .expect(t.name);
        assert_eq!(got, values, "{}", t.name);
    }
    finish(all).await;
}

// `Bool` binds as a boolean and reads back as K's true/false (0/1 on SQLite).
#[tokio::test]
async fn booleans_round_trip() {
    let all = databases().await;
    let values = vec![vec![Value::Bool(true)], vec![Value::Bool(false)]];
    for t in &all {
        let got = round_trip(&t.db, ColumnType::Boolean, false, &values)
            .await
            .expect(t.name);
        assert!(common::is_true(got[0].first()), "{}: {got:?}", t.name);
        assert!(common::is_false(got[1].first()), "{}: {got:?}", t.name);
    }
    finish(all).await;
}

// `Date` binds as RFC 3339 UTC text with milliseconds and reads back as the same text.
#[tokio::test]
async fn dates_round_trip_as_rfc3339() {
    let all = databases().await;
    let samples = [
        "2026-01-02T03:04:05.678Z",
        "1970-01-01T00:00:00.000Z",
        "2099-12-31T23:59:59.999Z",
    ];
    for t in &all {
        let got = round_trip(&t.db, ColumnType::Timestamp, false, &one(&samples)).await;
        assert_eq!(got, Ok(one(&samples)), "{}", t.name);
    }
    finish(all).await;
}

// `notes.filter { n => n.createdAt > since }.sortedBy { n => n.createdAt }` compares instants.
#[tokio::test]
async fn dates_compare_and_sort_chronologically() {
    let all = databases().await;
    let samples = [
        "2026-03-01T00:00:00.000Z",
        "2025-12-31T23:59:59.999Z",
        "2026-01-15T12:00:00.000Z",
    ];
    for t in &all {
        let ready = setup(&t.db, &ColumnType::Timestamp, false, &one(&samples)).await;
        assert!(ready.is_ok(), "{}: {ready:?}", t.name);
        let got = where_v(&t.db, CmpOp::Gt, text("2026-01-01T00:00:00.000Z"), false).await;
        assert_eq!(got, Ok(vec![3, 1]), "{}", t.name);
    }
    finish(all).await;
}

// `Decimal` keeps every digit of numeric(38,10) (SQLite stores it as text so it can't round).
#[tokio::test]
async fn decimals_keep_their_precision() {
    let all = databases().await;
    let samples = [
        "12.34",
        "-0.5",
        "0",
        "1234567890123456789012345678.0123456789",
    ];
    for t in &all {
        let got = round_trip(&t.db, ColumnType::Decimal, false, &one(&samples)).await;
        let got: Result<Vec<Option<String>>, _> =
            got.map(|rows| rows.iter().map(|r| normalized_decimal(&r[0])).collect());
        let expected: Vec<Option<String>> = samples.iter().map(|s| Some((*s).to_owned())).collect();
        assert_eq!(got, Ok(expected), "{}", t.name);
    }
    finish(all).await;
}

// `products.filter { p => p.weight > 9.6 }` compares decimals as numbers, not as text
// ("10.0" > "9.6" numerically, but not lexicographically).
#[tokio::test]
async fn decimals_compare_numerically() {
    let all = databases().await;
    for t in &all {
        let ready = setup(
            &t.db,
            &ColumnType::Decimal,
            false,
            &one(&["9.5", "10.0", "100", "-20"]),
        )
        .await;
        assert!(ready.is_ok(), "{}: {ready:?}", t.name);
        let got = where_v(&t.db, CmpOp::Gt, text("9.6"), false).await;
        assert_eq!(got, Ok(vec![2, 3]), "{}", t.name);
    }
    finish(all).await;
}

// `.sortedBy { p => p.weight }` orders decimals numerically.
#[tokio::test]
async fn decimals_sort_numerically() {
    let all = databases().await;
    for t in &all {
        let ready = setup(
            &t.db,
            &ColumnType::Decimal,
            false,
            &one(&["9.5", "10.0", "-20", "2"]),
        )
        .await;
        assert!(ready.is_ok(), "{}: {ready:?}", t.name);
        let got = where_v(&t.db, CmpOp::Gt, text("-1000"), false).await;
        assert_eq!(got, Ok(vec![3, 4, 1, 2]), "{}", t.name);
    }
    finish(all).await;
}

// `Money` is two parameters, amount (decimal text) and currency (ISO code), and reads back so.
#[tokio::test]
async fn money_round_trips_as_amount_and_currency() {
    let all = databases().await;
    let values = vec![
        vec![text("19.99"), text("EUR")],
        vec![text("-5.0000"), text("USD")],
        vec![text("0"), text("JPY")],
    ];
    for t in &all {
        let got = round_trip(&t.db, ColumnType::Money, false, &values).await;
        let got: Result<Vec<(Option<String>, Value)>, _> = got.map(|rows| {
            rows.iter()
                .map(|r| (normalized_decimal(&r[0]), r[1].clone()))
                .collect()
        });
        assert_eq!(
            got,
            Ok(vec![
                (Some("19.99".to_owned()), text("EUR")),
                (Some("-5".to_owned()), text("USD")),
                (Some("0".to_owned()), text("JPY")),
            ]),
            "{}",
            t.name
        );
    }
    finish(all).await;
}

// A `Codable` value binds as JSON text and reads back as the same JSON value.
#[tokio::test]
async fn json_round_trips_as_the_same_value() {
    let all = databases().await;
    let samples = [
        r#"{"a": 1, "b": [true, null, "x"]}"#,
        "[]",
        r#""just a string""#,
        "3.5",
    ];
    for t in &all {
        let got = round_trip(&t.db, ColumnType::Json, false, &one(&samples)).await;
        let parse = |s: &str| serde_json::from_str::<serde_json::Value>(s).ok();
        let got: Result<Vec<Option<serde_json::Value>>, _> = got.map(|rows| {
            rows.iter()
                .map(|r| r[0].as_text().and_then(parse))
                .collect()
        });
        let expected: Vec<Option<serde_json::Value>> = samples.iter().map(|s| parse(s)).collect();
        assert_eq!(got, Ok(expected), "{}", t.name);
    }
    finish(all).await;
}

// An enum case binds and reads back as its name.
#[tokio::test]
async fn enum_cases_round_trip_by_name() {
    let all = databases().await;
    for t in &all {
        let ty = ColumnType::Enumeration(common::order_statuses());
        let got = round_trip(&t.db, ty, false, &one(&["paid", "awaitingPayment"]))
            .await
            .expect(t.name);
        assert_eq!(got, one(&["paid", "awaitingPayment"]), "{}", t.name);
    }
    finish(all).await;
}

// `nil` round trips as NULL in an optional column of every type.
#[tokio::test]
async fn nil_round_trips_in_every_type() {
    let types = [
        ColumnType::Uuid,
        ColumnType::Text,
        ColumnType::Integer,
        ColumnType::Decimal,
        ColumnType::Boolean,
        ColumnType::Timestamp,
        ColumnType::Json,
        ColumnType::Enumeration(vec!["a".into()]),
        ColumnType::Money,
    ];
    for ty in types {
        let all = databases().await;
        let width = value_columns(&ty).len();
        let values = vec![vec![Value::Null; width]];
        for t in &all {
            let got = round_trip(&t.db, ty.clone(), true, &values).await;
            assert_eq!(got, Ok(values.clone()), "{}: {ty:?}", t.name);
        }
        finish(all).await;
    }
}

// Raw bytes round trip through a blob / bytea column.
#[tokio::test]
async fn bytes_round_trip() {
    let all = databases().await;
    for t in &all {
        let ty = match t.db.dialect() {
            k_db::Dialect::Sqlite => "BLOB",
            k_db::Dialect::Postgres => "bytea",
        };
        t.db.execute(&format!("CREATE TABLE \"blob\" (\"b\" {ty})"), &[])
            .await
            .expect(t.name);
        let p = match t.db.dialect() {
            k_db::Dialect::Sqlite => "?1",
            k_db::Dialect::Postgres => "$1",
        };
        let bytes = vec![0u8, 1, 255, 0, 10];
        t.db.execute(
            &format!("INSERT INTO \"blob\" (\"b\") VALUES ({p})"),
            &[Value::Bytes(bytes.clone())],
        )
        .await
        .expect(t.name);
        let r =
            t.db.query("SELECT \"b\" FROM \"blob\"", &[])
                .await
                .expect(t.name);
        assert_eq!(r[0].values[0], Value::Bytes(bytes), "{}", t.name);
    }
    finish(all).await;
}
