//! `schema.lock`: the canonical layout and round trips.

use k_db::lock::{HEADER, LockedColumn, LockedTable, SchemaLock};

fn sample() -> SchemaLock {
    let mut lock = SchemaLock {
        migrations: vec!["0001_init".into(), "0002_add_review".into()],
        ..Default::default()
    };
    lock.tables.insert(
        "order".into(),
        LockedTable {
            entity: "store.Order".into(),
            columns: vec![
                LockedColumn {
                    field: Some("id".into()),
                    name: "id".into(),
                    ty: "uuid".into(),
                    primary_key: true,
                    ..Default::default()
                },
                LockedColumn {
                    field: Some("status".into()),
                    name: "status".into(),
                    ty: "enumeration".into(),
                    cases: vec!["paid".into(), "cancelled".into()],
                    default: Some("paid".into()),
                    ..Default::default()
                },
                LockedColumn {
                    name: "coupon_code".into(),
                    ty: "text".into(),
                    nullable: true,
                    retired: true,
                    ..Default::default()
                },
            ],
        },
    );
    lock
}

#[test]
fn the_canonical_layout_is_stable() {
    let expected = format!(
        "{HEADER}\nmigrations = [\"0001_init\", \"0002_add_review\"]\n\n[tables.order]\nentity = \"store.Order\"\ncolumns = [\n    {{ field = \"id\", name = \"id\", type = \"uuid\", primaryKey = true }},\n    {{ field = \"status\", name = \"status\", type = \"enumeration\", cases = [\"paid\", \"cancelled\"], default = \"paid\" }},\n    {{ name = \"coupon_code\", type = \"text\", nullable = true, retired = true }},\n]\n"
    );
    assert_eq!(sample().to_toml(), expected);
}

#[test]
fn printing_then_parsing_round_trips() {
    let lock = sample();
    assert_eq!(SchemaLock::parse(&lock.to_toml()).expect("parses"), lock);
    let empty = SchemaLock::default();
    assert_eq!(SchemaLock::parse(&empty.to_toml()).expect("parses"), empty);
}

#[test]
fn odd_names_are_quoted() {
    let mut lock = SchemaLock::default();
    lock.tables.insert(
        "we.ird \"t\"".into(),
        LockedTable {
            entity: "a.B".into(),
            columns: vec![],
        },
    );
    assert_eq!(SchemaLock::parse(&lock.to_toml()).expect("parses"), lock);
}

#[test]
fn unknown_keys_and_bad_toml_are_errors() {
    assert!(SchemaLock::parse("migrations = [1]").is_err());
    assert!(SchemaLock::parse("surprise = true").is_err());
    assert!(SchemaLock::parse("[tables.t]\nentity = \"a.B\"\ncolumns = [{ name = \"x\", type = \"text\", colour = 1 }]").is_err());
    assert!(SchemaLock::parse("migrations = [").is_err());
}
