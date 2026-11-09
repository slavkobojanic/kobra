//! `schema.lock` edge cases beyond `lock.rs` (`docs/compiler/db.md` §11,
//! `docs/design/19-persistence.md`, "The `schema.lock` file"): the empty
//! lock, ordering, quoting of every odd character, every key in its fixed
//! order, what parsing rejects, and the lock form of the fixture entities.

use crate::common;

use k_db::lock::{HEADER, LockedColumn, LockedTable, SchemaLock};
use k_db::schema::{ColumnType, Schema};

fn table(entity: &str, columns: Vec<LockedColumn>) -> LockedTable {
    LockedTable {
        entity: entity.into(),
        retired: false,
        columns,
    }
}

fn col(field: &str, name: &str, ty: &str) -> LockedColumn {
    LockedColumn {
        field: Some(field.into()),
        name: name.into(),
        ty: ty.into(),
        ..Default::default()
    }
}

/// The lock entry for a schema, as `k migrate new` records it: one locked
/// column per logical column (money stays one column), references as
/// `table.id`, enumeration cases listed.
fn lock_of(schema: &Schema, migrations: &[&str]) -> SchemaLock {
    let mut lock = SchemaLock {
        migrations: migrations.iter().map(|m| (*m).to_owned()).collect(),
        ..Default::default()
    };
    for t in &schema.tables {
        let columns = t
            .columns
            .iter()
            .map(|c| LockedColumn {
                field: c.field.clone(),
                name: c.name.clone(),
                ty: c.ty.lock_name().into(),
                primary_key: c.primary_key,
                nullable: c.nullable,
                unique: c.unique,
                indexed: c.indexed,
                references: c.references.as_ref().map(|r| format!("{r}.id")),
                cases: match &c.ty {
                    ColumnType::Enumeration(cases) => cases.clone(),
                    _ => vec![],
                },
                default: None,
                retired: c.field.is_none(),
            })
            .collect();
        lock.tables
            .insert(t.name.clone(), table(&t.entity, columns));
    }
    lock
}

// The empty lock is the header and an empty migration list.
#[test]
fn the_empty_lock() {
    assert_eq!(
        SchemaLock::default().to_toml(),
        format!("{HEADER}\nmigrations = []\n")
    );
}

// An empty file parses as the empty lock (a project before its first migration).
#[test]
fn an_empty_file_is_the_empty_lock() {
    assert_eq!(
        SchemaLock::parse("").expect("parses"),
        SchemaLock::default()
    );
    assert_eq!(
        SchemaLock::parse(HEADER).expect("parses"),
        SchemaLock::default()
    );
}

// The migration list keeps its order: it *is* the application order, never sorted.
#[test]
fn migrations_keep_their_order() {
    let lock = SchemaLock {
        migrations: vec!["0002_b".into(), "0001_a".into()],
        ..Default::default()
    };
    let text = lock.to_toml();
    assert!(
        text.contains("migrations = [\"0002_b\", \"0001_a\"]"),
        "{text}"
    );
    assert_eq!(
        SchemaLock::parse(&text).expect("parses").migrations,
        lock.migrations
    );
}

// Tables are written sorted by name whatever order they were added in.
#[test]
fn tables_are_sorted_by_name() {
    let mut lock = SchemaLock::default();
    lock.tables.insert("zebra".into(), table("z.Zebra", vec![]));
    lock.tables.insert("apple".into(), table("a.Apple", vec![]));
    let text = lock.to_toml();
    let a = text.find("[tables.apple]").unwrap_or(usize::MAX);
    let z = text.find("[tables.zebra]").unwrap_or(0);
    assert!(a < z, "{text}");
}

// Columns keep their field order.
#[test]
fn columns_keep_their_order() {
    let mut lock = SchemaLock::default();
    lock.tables.insert(
        "t".into(),
        table("x.T", vec![col("z", "z", "text"), col("a", "a", "text")]),
    );
    let parsed = SchemaLock::parse(&lock.to_toml()).expect("parses");
    let names: Vec<&str> = parsed.tables["t"]
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["z", "a"]);
}

// Every key set at once appears in the fixed order.
#[test]
fn every_key_in_its_fixed_order() {
    let mut lock = SchemaLock::default();
    lock.tables.insert(
        "t".into(),
        table(
            "x.T",
            vec![LockedColumn {
                field: Some("f".into()),
                name: "n".into(),
                ty: "enumeration".into(),
                primary_key: true,
                nullable: true,
                unique: true,
                indexed: true,
                references: Some("u.id".into()),
                cases: vec!["a".into(), "b".into()],
                default: Some("a".into()),
                retired: true,
            }],
        ),
    );
    assert!(lock.to_toml().contains(
        "    { field = \"f\", name = \"n\", type = \"enumeration\", primaryKey = true, nullable = true, unique = true, indexed = true, references = \"u.id\", cases = [\"a\", \"b\"], default = \"a\", retired = true },\n"
    ));
}

// False flags and empty lists are left out, so diffs stay small.
#[test]
fn false_and_empty_keys_are_omitted() {
    let mut lock = SchemaLock::default();
    lock.tables
        .insert("t".into(), table("x.T", vec![col("f", "f", "text")]));
    assert!(
        lock.to_toml()
            .contains("    { field = \"f\", name = \"f\", type = \"text\" },\n")
    );
}

// A table with no columns still round trips.
#[test]
fn a_table_without_columns() {
    let mut lock = SchemaLock::default();
    lock.tables.insert("t".into(), table("x.T", vec![]));
    let text = lock.to_toml();
    assert!(
        text.contains("[tables.t]\nentity = \"x.T\"\ncolumns = [\n]\n"),
        "{text}"
    );
    assert_eq!(SchemaLock::parse(&text).expect("parses"), lock);
}

// Quotes, backslashes, newlines, tabs, control characters and non-ASCII survive a round trip.
#[test]
fn every_odd_character_round_trips() {
    let odd = "q\"uote b\\ack\nnew\ttab \u{1} \u{7f} é 日本 🎉";
    let mut lock = SchemaLock {
        migrations: vec![odd.into()],
        ..Default::default()
    };
    lock.tables.insert(
        odd.into(),
        table(
            odd,
            vec![LockedColumn {
                default: Some(odd.into()),
                cases: vec![odd.into()],
                ..col(odd, odd, "enumeration")
            }],
        ),
    );
    assert_eq!(SchemaLock::parse(&lock.to_toml()).expect("parses"), lock);
}

// Plain table names are bare keys; anything else is quoted.
#[test]
fn table_keys_are_bare_only_when_safe() {
    let mut lock = SchemaLock::default();
    lock.tables
        .insert("order_line".into(), table("x.OrderLine", vec![]));
    lock.tables
        .insert("order line".into(), table("x.Odd", vec![]));
    let text = lock.to_toml();
    assert!(text.contains("[tables.order_line]"), "{text}");
    assert!(text.contains("[tables.\"order line\"]"), "{text}");
    assert_eq!(SchemaLock::parse(&text).expect("parses"), lock);
}

// A lock written by hand in another valid layout (keys reordered, one line) parses the same.
#[test]
fn a_non_canonical_layout_parses() {
    let text = r#"
        [tables.note]
        columns = [{ type = "uuid", primaryKey = true, name = "id", field = "id" }]
        entity = "notes.Note"
        [tables.tag]
        entity = "notes.Tag"
        columns = []
    "#;
    let lock = SchemaLock::parse(&format!("migrations = [\"0001_init\"]\n{text}")).expect("parses");
    assert_eq!(lock.migrations, ["0001_init"]);
    assert_eq!(lock.tables["note"].columns[0].ty, "uuid");
    assert!(lock.tables["note"].columns[0].primary_key);
    assert!(lock.tables["tag"].columns.is_empty());
}

// A retired column has no field and the `retired` flag.
#[test]
fn retired_columns_have_no_field() {
    let mut lock = SchemaLock::default();
    lock.tables.insert(
        "t".into(),
        table(
            "x.T",
            vec![LockedColumn {
                name: "old".into(),
                ty: "text".into(),
                nullable: true,
                retired: true,
                ..Default::default()
            }],
        ),
    );
    let text = lock.to_toml();
    assert!(
        text.contains(
            "    { name = \"old\", type = \"text\", nullable = true, retired = true },\n"
        ),
        "{text}"
    );
    assert_eq!(
        SchemaLock::parse(&text).expect("parses").tables["t"].columns[0].field,
        None
    );
}

// An unknown key on a table is rejected.
#[test]
fn unknown_table_keys_are_rejected() {
    assert!(SchemaLock::parse("[tables.t]\nentity = \"x.T\"\nprimary = \"id\"").is_err());
}

// A wrongly typed flag is rejected, not coerced.
#[test]
fn wrongly_typed_values_are_rejected() {
    let bad = "[tables.t]\nentity = \"x.T\"\ncolumns = [{ name = \"id\", type = \"uuid\", primaryKey = \"yes\" }]";
    assert!(SchemaLock::parse(bad).is_err());
}

// A column without a name or a type is rejected.
#[test]
fn a_column_needs_a_name_and_a_type() {
    assert!(
        SchemaLock::parse("[tables.t]\nentity = \"x.T\"\ncolumns = [{ type = \"uuid\" }]").is_err()
    );
    assert!(
        SchemaLock::parse("[tables.t]\nentity = \"x.T\"\ncolumns = [{ name = \"id\" }]").is_err()
    );
}

// A table without its entity is rejected.
#[test]
fn a_table_needs_its_entity() {
    assert!(SchemaLock::parse("[tables.t]\ncolumns = []").is_err());
}

// Parse errors say they are about schema.lock and where the problem is.
#[test]
fn parse_errors_name_the_file_and_the_line() {
    let err = SchemaLock::parse("migrations = []\n\n[tables.t]\nentity = 3\n").expect_err("bad");
    let text = err.to_string();
    assert!(text.starts_with("schema.lock: "), "{text}");
    assert!(text.contains('4'), "the message points at line 4: {text}");
}

// Arbitrary bytes never panic the parser.
#[test]
fn garbage_never_panics() {
    for text in [
        "\u{0}",
        "[[[",
        "tables = 1",
        "[tables]\nx = 1",
        "migrations = [\"a\", 2]",
        "=\n=",
    ] {
        assert!(SchemaLock::parse(text).is_err(), "{text:?}");
    }
}

// The fixture entities' lock: money stays one logical column, foreign keys are `table.id`
// and indexed, enum cases are listed, and `user`/`order` need no quoting as keys.
#[test]
fn the_fixture_schema_as_a_lock() {
    let lock = lock_of(&common::schema(), &["0001_init"]);
    let expected = format!(
        "{HEADER}
migrations = [\"0001_init\"]

[tables.note]
entity = \"notes.Note\"
columns = [
    {{ field = \"id\", name = \"id\", type = \"uuid\", primaryKey = true }},
    {{ field = \"title\", name = \"title\", type = \"text\" }},
    {{ field = \"body\", name = \"body\", type = \"text\" }},
    {{ field = \"isPinned\", name = \"is_pinned\", type = \"boolean\" }},
    {{ field = \"priority\", name = \"priority\", type = \"integer\" }},
    {{ field = \"category\", name = \"category\", type = \"text\", nullable = true }},
    {{ field = \"revision\", name = \"revision\", type = \"integer\" }},
]

[tables.order]
entity = \"store.Order\"
columns = [
    {{ field = \"id\", name = \"id\", type = \"uuid\", primaryKey = true }},
    {{ field = \"status\", name = \"status\", type = \"enumeration\", cases = [\"awaitingPayment\", \"paid\", \"shipped\", \"cancelled\"] }},
    {{ field = \"total\", name = \"total\", type = \"money\" }},
]

[tables.tag]
entity = \"notes.Tag\"
columns = [
    {{ field = \"id\", name = \"id\", type = \"uuid\", primaryKey = true }},
    {{ field = \"note\", name = \"note_id\", type = \"uuid\", indexed = true, references = \"note.id\" }},
    {{ field = \"name\", name = \"name\", type = \"text\" }},
]

[tables.user]
entity = \"accounts.User\"
columns = [
    {{ field = \"id\", name = \"id\", type = \"uuid\", primaryKey = true }},
    {{ field = \"email\", name = \"email\", type = \"text\", unique = true }},
    {{ field = \"handle\", name = \"handle\", type = \"text\", nullable = true, unique = true }},
    {{ field = \"name\", name = \"name\", type = \"text\" }},
]
"
    );
    assert_eq!(lock.to_toml(), expected);
    assert_eq!(SchemaLock::parse(&expected).expect("parses"), lock);
}

// Locked names are what the schema uses, so a renamed field keeps its column.
#[test]
fn a_renamed_field_keeps_its_locked_column() {
    let mut lock = lock_of(&common::schema(), &["0001_init"]);
    // `isPinned` renamed to `pinned` in K: the field changes, the column doesn't.
    if let Some(t) = lock.tables.get_mut("note") {
        t.columns[3].field = Some("pinned".into());
    }
    let parsed = SchemaLock::parse(&lock.to_toml()).expect("parses");
    assert_eq!(parsed.tables["note"].columns[3].name, "is_pinned");
    assert_eq!(
        parsed.tables["note"].columns[3].field.as_deref(),
        Some("pinned")
    );
}

// A default written as a TOML boolean or number (`default = false`, as the
// examples' locks do) reads as the same text as its quoted form.
#[test]
fn scalar_defaults_read_as_text() {
    let lock = SchemaLock::parse(
        "[tables.note]\nentity = \"notes.Note\"\ncolumns = [\n    { field = \"isPinned\", name = \"is_pinned\", type = \"boolean\", default = false },\n    { field = \"stock\", name = \"stock\", type = \"integer\", default = -3 },\n    { field = \"weight\", name = \"weight\", type = \"decimal\", default = 1.5 },\n    { field = \"status\", name = \"status\", type = \"text\", default = \"open\" },\n]\n",
    )
    .expect("parses");
    let defaults: Vec<Option<String>> = lock.tables["note"]
        .columns
        .iter()
        .map(|c| c.default.clone())
        .collect();
    assert_eq!(
        defaults,
        [
            Some("false".to_owned()),
            Some("-3".to_owned()),
            Some("1.5".to_owned()),
            Some("open".to_owned())
        ]
    );
    // Arrays and tables are not defaults.
    assert!(
        SchemaLock::parse(
            "[tables.t]\nentity = \"x.T\"\ncolumns = [{ name = \"a\", type = \"text\", default = [1] }]\n"
        )
        .is_err()
    );
}
