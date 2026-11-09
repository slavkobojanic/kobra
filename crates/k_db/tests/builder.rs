//! The `Schema` builder replay (`docs/compiler/db.md` §10): the exact SQL
//! of each operation in both dialects, the checks that make a mistaken
//! migration a build error, and the rendered migrations applied to every
//! test database.

use crate::common;

use common::{databases, finish, has_column, scalar, table_count};
use k_db::builder::{AlterOp, ColumnDef, Literal, Replay, SchemaOp};
use k_db::migrate::{self, Migration};
use k_db::schema::ColumnType;

fn col(name: &str, ty: ColumnType) -> ColumnDef {
    ColumnDef::new(name, ty)
}

fn pk(name: &str) -> ColumnDef {
    ColumnDef {
        primary_key: true,
        ..col(name, ColumnType::Uuid)
    }
}

/// The notes example's two migrations.
fn notes_ops() -> (Vec<SchemaOp>, Vec<SchemaOp>) {
    let init = vec![
        SchemaOp::CreateTable {
            name: "account".into(),
            columns: vec![
                pk("id"),
                ColumnDef {
                    unique: true,
                    ..col("email", ColumnType::Text)
                },
                col("created_at", ColumnType::Timestamp),
            ],
        },
        SchemaOp::CreateTable {
            name: "note".into(),
            columns: vec![
                pk("id"),
                ColumnDef {
                    indexed: true,
                    ..col("owner_id", ColumnType::Uuid)
                },
                col("title", ColumnType::Text),
            ],
        },
        SchemaOp::CreateTable {
            name: "tag".into(),
            columns: vec![
                pk("id"),
                ColumnDef {
                    references: Some("note".into()),
                    ..col("note_id", ColumnType::Uuid)
                },
                col("label", ColumnType::Text),
            ],
        },
    ];
    let pinning = vec![SchemaOp::AlterTable {
        name: "note".into(),
        ops: vec![AlterOp::AddColumn(ColumnDef {
            default: Some(Literal::Boolean(false)),
            ..col("is_pinned", ColumnType::Boolean)
        })],
    }];
    (init, pinning)
}

#[test]
fn create_table_renders_columns_and_indexes_in_both_dialects() {
    let (init, _) = notes_ops();
    let r = Replay::new().apply(&init).expect("replays");
    assert_eq!(
        r.sqlite,
        [
            "CREATE TABLE \"account\" (\"id\" TEXT NOT NULL PRIMARY KEY, \"email\" TEXT NOT NULL, \"created_at\" TEXT NOT NULL)",
            "CREATE UNIQUE INDEX \"account_email_key\" ON \"account\" (\"email\")",
            "CREATE TABLE \"note\" (\"id\" TEXT NOT NULL PRIMARY KEY, \"owner_id\" TEXT NOT NULL, \"title\" TEXT NOT NULL)",
            "CREATE INDEX \"note_owner_id_idx\" ON \"note\" (\"owner_id\")",
            "CREATE TABLE \"tag\" (\"id\" TEXT NOT NULL PRIMARY KEY, \"note_id\" TEXT NOT NULL REFERENCES \"note\" (\"id\"), \"label\" TEXT NOT NULL)",
            "CREATE INDEX \"tag_note_id_idx\" ON \"tag\" (\"note_id\")",
        ]
    );
    assert_eq!(
        r.postgres[0],
        "CREATE TABLE \"account\" (\"id\" uuid NOT NULL PRIMARY KEY, \"email\" text NOT NULL, \"created_at\" timestamptz NOT NULL)"
    );
}

#[test]
fn added_columns_carry_their_default_per_dialect() {
    let (init, pinning) = notes_ops();
    let mut replay = Replay::new();
    replay.apply(&init).expect("replays");
    let r = replay.apply(&pinning).expect("replays");
    assert_eq!(
        r.sqlite,
        ["ALTER TABLE \"note\" ADD COLUMN \"is_pinned\" INTEGER NOT NULL DEFAULT 0"]
    );
    assert_eq!(
        r.postgres,
        ["ALTER TABLE \"note\" ADD COLUMN \"is_pinned\" boolean NOT NULL DEFAULT FALSE"]
    );
    assert_eq!(
        replay.tables["note"].len(),
        4,
        "the replay knows the new column"
    );
}

#[test]
fn defaults_of_every_type_are_literals_never_parameters() {
    let ops = vec![SchemaOp::CreateTable {
        name: "t".into(),
        columns: vec![
            ColumnDef {
                default: Some(Literal::Text("it's".into())),
                ..col("a", ColumnType::Text)
            },
            ColumnDef {
                default: Some(Literal::Integer(-3)),
                ..col("b", ColumnType::Integer)
            },
            ColumnDef {
                default: Some(Literal::Decimal("1.50".into())),
                ..col("c", ColumnType::Decimal)
            },
            ColumnDef {
                default: Some(Literal::Text("paid".into())),
                ..col(
                    "d",
                    ColumnType::Enumeration(vec!["paid".into(), "open".into()]),
                )
            },
            ColumnDef {
                nullable: true,
                default: Some(Literal::Null),
                ..col("e", ColumnType::Uuid)
            },
        ],
    }];
    let r = Replay::new().apply(&ops).expect("replays");
    assert_eq!(
        r.sqlite[0],
        "CREATE TABLE \"t\" (\"a\" TEXT NOT NULL DEFAULT 'it''s', \"b\" INTEGER NOT NULL DEFAULT -3, \"c\" TEXT COLLATE k_decimal NOT NULL DEFAULT '1.50', \"d\" TEXT NOT NULL DEFAULT 'paid' CHECK (\"d\" IN ('paid', 'open')), \"e\" TEXT DEFAULT NULL)"
    );
    assert!(r.postgres[0].contains("\"c\" numeric(38,10) NOT NULL DEFAULT 1.50"));
}

#[test]
fn money_operations_expand_to_both_halves() {
    let mut replay = Replay::new();
    replay
        .apply(&[SchemaOp::CreateTable {
            name: "product".into(),
            columns: vec![pk("id"), col("price", ColumnType::Money)],
        }])
        .expect("creates");
    let r = replay
        .apply(&[
            SchemaOp::CreateIndex {
                table: "product".into(),
                columns: vec!["price".into()],
            },
            SchemaOp::AlterTable {
                name: "product".into(),
                ops: vec![AlterOp::RenameColumn {
                    from: "price".into(),
                    to: "list_price".into(),
                }],
            },
        ])
        .expect("replays");
    assert_eq!(
        r.sqlite,
        [
            "CREATE INDEX \"product_price_idx\" ON \"product\" (\"price_amount\", \"price_currency\")",
            "ALTER TABLE \"product\" RENAME COLUMN \"price_amount\" TO \"list_price_amount\"",
            "ALTER TABLE \"product\" RENAME COLUMN \"price_currency\" TO \"list_price_currency\"",
        ]
    );
    let r = replay
        .apply(&[SchemaOp::AlterTable {
            name: "product".into(),
            ops: vec![AlterOp::DropColumn {
                name: "list_price".into(),
            }],
        }])
        .expect("drops");
    // The index created before the rename keeps its name, and SQLite
    // refuses to drop a column an index covers, so it is dropped first.
    assert_eq!(
        &r.sqlite[2..],
        [
            "DROP INDEX IF EXISTS \"product_price_idx\"",
            "ALTER TABLE \"product\" DROP COLUMN \"list_price_amount\"",
            "ALTER TABLE \"product\" DROP COLUMN \"list_price_currency\"",
        ]
    );
}

#[test]
fn tables_rename_drop_and_take_data_rows() {
    let mut replay = Replay::new();
    replay
        .apply(&[SchemaOp::CreateTable {
            name: "category".into(),
            columns: vec![pk("id"), col("name", ColumnType::Text)],
        }])
        .expect("creates");
    let r = replay
        .apply(&[
            SchemaOp::Insert {
                table: "category".into(),
                rows: vec![vec![
                    (
                        "id".into(),
                        Literal::Uuid("6F9619FF-8B86-D011-B42D-00C04FC964FF".into()),
                    ),
                    ("name".into(), Literal::Text("Mugs".into())),
                ]],
            },
            SchemaOp::RenameTable {
                from: "category".into(),
                to: "kind".into(),
            },
            SchemaOp::DropTable {
                name: "kind".into(),
            },
        ])
        .expect("replays");
    assert_eq!(
        r.sqlite,
        [
            "INSERT INTO \"category\" (\"id\", \"name\") VALUES ('6f9619ff-8b86-d011-b42d-00c04fc964ff', 'Mugs')",
            "ALTER TABLE \"category\" RENAME TO \"kind\"",
            "DROP TABLE \"kind\"",
        ]
    );
    assert!(replay.tables.is_empty());
}

#[test]
fn raw_sql_needs_both_dialects() {
    let ok = Replay::new()
        .apply(&[SchemaOp::Execute {
            postgres: "SELECT 1".into(),
            sqlite: "SELECT 2".into(),
        }])
        .expect("replays");
    assert_eq!(
        (ok.sqlite, ok.postgres),
        (vec!["SELECT 2".into()], vec!["SELECT 1".into()])
    );
    assert!(
        Replay::new()
            .apply(&[SchemaOp::Execute {
                postgres: "SELECT 1".into(),
                sqlite: "  ".into(),
            }])
            .is_err()
    );
}

// Every mistake a replay catches, so a typo in a migration is a build error.
#[test]
fn mistakes_are_errors_naming_the_problem() {
    let (init, _) = notes_ops();
    let base = || {
        let mut r = Replay::new();
        r.apply(&init).expect("replays");
        r
    };
    let cases: Vec<(SchemaOp, &str)> = vec![
        (
            SchemaOp::CreateTable {
                name: "note".into(),
                columns: vec![pk("id")],
            },
            "already exists",
        ),
        (
            SchemaOp::CreateTable {
                name: "k_job".into(),
                columns: vec![pk("id")],
            },
            "reserved",
        ),
        (
            SchemaOp::CreateTable {
                name: "x".into(),
                columns: vec![pk("id"), pk("id")],
            },
            "already has a column",
        ),
        (
            SchemaOp::CreateTable {
                name: "x".into(),
                columns: vec![ColumnDef {
                    references: Some("nope".into()),
                    ..col("y_id", ColumnType::Uuid)
                }],
            },
            "doesn't exist",
        ),
        (
            SchemaOp::AlterTable {
                name: "notes".into(),
                ops: vec![AlterOp::DropColumn {
                    name: "title".into(),
                }],
            },
            "no table `notes`",
        ),
        (
            SchemaOp::AlterTable {
                name: "note".into(),
                ops: vec![AlterOp::DropColumn {
                    name: "titel".into(),
                }],
            },
            "no column `titel`",
        ),
        (
            SchemaOp::AlterTable {
                name: "note".into(),
                ops: vec![AlterOp::AddColumn(col("body", ColumnType::Text))],
            },
            "nullable or have a default",
        ),
        (
            SchemaOp::AlterTable {
                name: "note".into(),
                ops: vec![AlterOp::AddColumn(ColumnDef {
                    default: Some(Literal::Integer(1)),
                    ..col("flag", ColumnType::Boolean)
                })],
            },
            "doesn't fit",
        ),
        (
            SchemaOp::AlterTable {
                name: "note".into(),
                ops: vec![AlterOp::AddEnumCase {
                    column: "status".into(),
                    case: "archived".into(),
                }],
            },
            "no column `status`",
        ),
        (
            SchemaOp::DropTable {
                name: "note".into(),
            },
            "still referenced by `tag`",
        ),
        (
            SchemaOp::CreateIndex {
                table: "note".into(),
                columns: vec![],
            },
            "at least one column",
        ),
        (
            SchemaOp::Insert {
                table: "note".into(),
                rows: vec![vec![("id".into(), Literal::Uuid("not-a-uuid".into()))]],
            },
            "not a UUID",
        ),
        (
            SchemaOp::Insert {
                table: "note".into(),
                rows: vec![vec![(
                    "id".into(),
                    Literal::Uuid("6f9619ff-8b86-d011-b42d-00c04fc964ff".into()),
                )]],
            },
            "needs a value for `owner_id`",
        ),
    ];
    for (op, message) in cases {
        let err = base().apply(std::slice::from_ref(&op)).expect_err(message);
        assert!(err.contains(message), "{op:?}: {err}");
    }
}

// The rendered migrations run on every database, and run again do nothing.
#[tokio::test]
async fn rendered_migrations_apply_on_every_database() {
    let (init, pinning) = notes_ops();
    let mut replay = Replay::new();
    let a = replay.apply(&init).expect("replays");
    let b = replay.apply(&pinning).expect("replays");
    let seed = replay
        .apply(&[SchemaOp::Insert {
            table: "account".into(),
            rows: vec![vec![
                (
                    "id".into(),
                    Literal::Uuid("6f9619ff-8b86-d011-b42d-00c04fc964ff".into()),
                ),
                ("email".into(), Literal::Text("o'neil@example.com".into())),
                ("created_at".into(), Literal::Text("ignored".into())),
            ]],
        }])
        .expect_err("text doesn't fit a timestamp");
    assert!(seed.contains("timestamp"), "{seed}");
    let migrations = vec![
        Migration {
            name: "0001_create".into(),
            sqlite: a.sqlite,
            postgres: a.postgres,
        },
        Migration {
            name: "0002_pinning".into(),
            sqlite: b.sqlite,
            postgres: b.postgres,
        },
    ];
    let all = databases().await;
    for t in &all {
        let ran = migrate::apply(&t.db, &migrations).await.expect(t.name);
        assert_eq!(ran.len(), 2, "{}", t.name);
        assert!(has_column(&t.db, "note", "is_pinned").await, "{}", t.name);
        assert_eq!(table_count(&t.db, "tag").await, Some(0), "{}", t.name);
        assert_eq!(
            scalar(&t.db, "SELECT COUNT(*) FROM \"k_migrations\"", &[]).await,
            Some(2),
            "{}",
            t.name
        );
        assert!(
            migrate::apply(&t.db, &migrations)
                .await
                .expect(t.name)
                .is_empty()
        );
    }
    finish(all).await;
}
