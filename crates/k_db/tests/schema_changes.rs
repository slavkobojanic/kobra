//! The schema changes that need more than one `ALTER TABLE`
//! (`docs/compiler/db.md` §10): the enum case operations, retiring columns
//! and tables, which operations are destructive, and the lock a replay
//! produces. Every rendered migration is applied to each test database
//! (SQLite always, Postgres when `K_TEST_POSTGRES_URL` is set), with rows,
//! indexes and a referencing table in place, so a table rebuild on SQLite
//! is shown to keep all three.

use crate::common;

use common::{databases, finish, scalar, text};
use k_db::builder::{AlterOp, ColumnDef, Literal, Replay, SchemaOp};
use k_db::migrate::{self, Migration};
use k_db::schema::ColumnType;
use k_db::sql::Dialect;
use k_db::value::Value;

fn col(name: &str, ty: ColumnType) -> ColumnDef {
    ColumnDef::new(name, ty)
}

fn statuses(cases: &[&str]) -> ColumnType {
    ColumnType::Enumeration(cases.iter().map(|c| (*c).to_owned()).collect())
}

/// An `order` table with an enumeration column (defaulting to `paid`), a
/// unique and a plain index, a two-column index, and a `line` table that
/// references it.
fn base_ops() -> Vec<SchemaOp> {
    vec![
        SchemaOp::CreateTable {
            name: "order".into(),
            columns: vec![
                ColumnDef {
                    primary_key: true,
                    ..col("id", ColumnType::Uuid)
                },
                ColumnDef {
                    unique: true,
                    ..col("number", ColumnType::Integer)
                },
                ColumnDef {
                    default: Some(Literal::Text("paid".into())),
                    indexed: true,
                    ..col("status", statuses(&["paid", "shipped", "lost"]))
                },
                col("note", ColumnType::Text),
            ],
        },
        SchemaOp::CreateIndex {
            table: "order".into(),
            columns: vec!["status".into(), "number".into()],
        },
        SchemaOp::CreateTable {
            name: "line".into(),
            columns: vec![
                ColumnDef {
                    primary_key: true,
                    ..col("id", ColumnType::Uuid)
                },
                ColumnDef {
                    references: Some("order".into()),
                    ..col("order_id", ColumnType::Uuid)
                },
            ],
        },
    ]
}

fn alter(table: &str, ops: Vec<AlterOp>) -> SchemaOp {
    SchemaOp::AlterTable {
        name: table.into(),
        ops,
    }
}

fn uuid(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn migration(name: &str, r: k_db::builder::Rendered) -> Migration {
    Migration {
        name: name.into(),
        sqlite: r.sqlite,
        postgres: r.postgres,
    }
}

/// Inserts order `n` with `status` (and a line referencing it).
async fn insert_order(db: &k_db::driver::Database, n: u32, status: &str) -> bool {
    let (a, b, c, d) = match db.dialect() {
        Dialect::Sqlite => ("?1", "?2", "?3", "?4"),
        Dialect::Postgres => ("$1", "$2", "$3", "$4"),
    };
    let order = db
        .execute(
            &format!(
                "INSERT INTO \"order\" (\"id\", \"number\", \"status\", \"note\") VALUES ({a}, {b}, {c}, {d})"
            ),
            &[
                text(&uuid(n)),
                Value::Int(i64::from(n)),
                text(status),
                text("n"),
            ],
        )
        .await;
    if order.is_err() {
        return false;
    }
    let line = match db.dialect() {
        Dialect::Sqlite => "INSERT INTO \"line\" (\"id\", \"order_id\") VALUES (?1, ?2)",
        Dialect::Postgres => "INSERT INTO \"line\" (\"id\", \"order_id\") VALUES ($1, $2)",
    };
    db.execute(line, &[text(&uuid(n + 1000)), text(&uuid(n))])
        .await
        .is_ok()
}

/// The number of orders with `status`.
async fn count_status(db: &k_db::driver::Database, status: &str) -> Option<i64> {
    let p = match db.dialect() {
        Dialect::Sqlite => "?1",
        Dialect::Postgres => "$1",
    };
    scalar(
        db,
        &format!("SELECT COUNT(*) FROM \"order\" WHERE \"status\" = {p}"),
        &[text(status)],
    )
    .await
}

/// The number of indexes on `table`, from the catalog.
async fn index_count(db: &k_db::driver::Database, table: &str) -> Option<i64> {
    let sql = match db.dialect() {
        Dialect::Sqlite => {
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1 AND name NOT LIKE 'sqlite_autoindex%'"
        }
        Dialect::Postgres => {
            "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = current_schema() AND tablename = $1 AND indexname NOT LIKE '%_pkey'"
        }
    };
    scalar(db, sql, &[text(table)]).await
}

// Adding a case renders a `CHECK` rewrite: a constraint swap on Postgres, a
// rebuild on SQLite that keeps the rows, the indexes and the table name.
#[test]
fn add_enum_case_rewrites_the_check() {
    let mut replay = Replay::new();
    replay.apply(&base_ops()).expect("replays");
    let r = replay
        .apply(&[alter(
            "order",
            vec![AlterOp::AddEnumCase {
                column: "status".into(),
                case: "returned".into(),
            }],
        )])
        .expect("replays");
    let pg = r.postgres.join("\n");
    assert!(pg.contains("DROP CONSTRAINT"), "{pg}");
    assert!(
        pg.contains(
            "ALTER TABLE \"order\" ADD CONSTRAINT \"order_status_check\" CHECK (\"status\" IN ('paid', 'shipped', 'lost', 'returned'))"
        ),
        "{pg}"
    );
    let lite = &r.sqlite;
    assert!(
        lite[0].starts_with("CREATE TABLE \"k_rebuild\""),
        "{lite:?}"
    );
    assert!(
        lite[0].contains("CHECK (\"status\" IN ('paid', 'shipped', 'lost', 'returned'))"),
        "{lite:?}"
    );
    assert!(
        lite.contains(&"DROP TABLE \"order\"".to_owned()),
        "{lite:?}"
    );
    assert!(
        lite.contains(&"ALTER TABLE \"k_rebuild\" RENAME TO \"order\"".to_owned()),
        "{lite:?}"
    );
    // Every index the replay knows of is created again, by its own name.
    for index in [
        "order_number_key",
        "order_status_idx",
        "order_status_number_idx",
    ] {
        assert!(
            lite.iter()
                .any(|s| s.contains(&format!("INDEX \"{index}\" ON \"order\""))),
            "{index}: {lite:?}"
        );
    }
    let ColumnType::Enumeration(cases) = &replay.tables["order"][2].ty else {
        panic!("still an enumeration");
    };
    assert_eq!(cases, &["paid", "shipped", "lost", "returned"]);
}

// The case operations on a real database: added cases are accepted,
// renamed and removed ones rewrite the rows, and the old values are refused
// afterwards. Rows, indexes and the referencing table survive.
#[tokio::test]
async fn enum_case_operations_apply_on_every_database() {
    let mut replay = Replay::new();
    let init = replay.apply(&base_ops()).expect("replays");
    let add = replay
        .apply(&[alter(
            "order",
            vec![AlterOp::AddEnumCase {
                column: "status".into(),
                case: "returned".into(),
            }],
        )])
        .expect("replays");
    let rename = replay
        .apply(&[alter(
            "order",
            vec![AlterOp::RenameEnumCase {
                column: "status".into(),
                from: "paid".into(),
                to: "settled".into(),
            }],
        )])
        .expect("replays");
    let remove = replay
        .apply(&[alter(
            "order",
            vec![AlterOp::RemoveEnumCase {
                column: "status".into(),
                case: "lost".into(),
                replacement: "returned".into(),
            }],
        )])
        .expect("replays");
    let all = databases().await;
    for t in &all {
        let db = &t.db;
        migrate::apply(db, &[migration("0001_init", init.clone())])
            .await
            .expect(t.name);
        assert!(insert_order(db, 1, "paid").await, "{}", t.name);
        assert!(insert_order(db, 2, "lost").await, "{}", t.name);
        assert!(insert_order(db, 3, "shipped").await, "{}", t.name);
        assert!(
            !insert_order(db, 4, "returned").await,
            "{}: not a case yet",
            t.name
        );
        let before = index_count(db, "order").await;
        assert_eq!(before, Some(3), "{}", t.name);

        let steps = [
            migration("0001_init", init.clone()),
            migration("0002_add", add.clone()),
            migration("0003_rename", rename.clone()),
            migration("0004_remove", remove.clone()),
        ];
        let ran = migrate::apply(db, &steps).await.expect(t.name);
        assert_eq!(
            ran,
            ["0002_add", "0003_rename", "0004_remove"],
            "{}",
            t.name
        );

        assert_eq!(count_status(db, "settled").await, Some(1), "{}", t.name);
        assert_eq!(count_status(db, "paid").await, Some(0), "{}", t.name);
        assert_eq!(count_status(db, "returned").await, Some(1), "{}", t.name);
        assert_eq!(count_status(db, "shipped").await, Some(1), "{}", t.name);
        assert!(insert_order(db, 5, "returned").await, "{}", t.name);
        assert!(!insert_order(db, 6, "paid").await, "{}: renamed", t.name);
        assert!(!insert_order(db, 7, "lost").await, "{}: removed", t.name);
        assert_eq!(index_count(db, "order").await, before, "{}", t.name);
        assert_eq!(
            scalar(db, "SELECT COUNT(*) FROM \"line\"", &[]).await,
            Some(4),
            "{}: the referencing rows are kept",
            t.name
        );
        // The default followed the rename.
        let p = match db.dialect() {
            Dialect::Sqlite => "?1",
            Dialect::Postgres => "$1",
        };
        db.execute(
            &format!("INSERT INTO \"order\" (\"id\", \"number\", \"note\") VALUES ({p}, 9, 'x')"),
            &[text(&uuid(9))],
        )
        .await
        .expect(t.name);
        assert_eq!(count_status(db, "settled").await, Some(2), "{}", t.name);
        // A unique index rebuilt on SQLite still refuses duplicates.
        assert!(!insert_order(db, 3, "shipped").await, "{}", t.name);
    }
    finish(all).await;
}

// A retired column stays in the table; one that was `NOT NULL` without a
// default becomes nullable, so the next release's inserts, which leave it
// out, succeed.
#[tokio::test]
async fn retiring_a_column_relaxes_it_on_every_database() {
    let mut replay = Replay::new();
    let init = replay.apply(&base_ops()).expect("replays");
    let retire = replay
        .apply(&[alter(
            "order",
            vec![AlterOp::RetireColumn {
                name: "note".into(),
            }],
        )])
        .expect("replays");
    assert!(replay.is_retired("order", "note"));
    assert!(!replay.is_retired("order", "status"));
    assert!(
        replay.tables["order"].iter().any(|c| c.name == "note"),
        "a retired column is kept"
    );
    assert!(
        retire
            .postgres
            .contains(&"ALTER TABLE \"order\" ALTER COLUMN \"note\" DROP NOT NULL".to_owned()),
        "{:?}",
        retire.postgres
    );
    let all = databases().await;
    for t in &all {
        let db = &t.db;
        let ran = migrate::apply(
            db,
            &[
                migration("0001_init", init.clone()),
                migration("0002_retire", retire.clone()),
            ],
        )
        .await
        .expect(t.name);
        assert_eq!(ran.len(), 2, "{}", t.name);
        let p = match db.dialect() {
            Dialect::Sqlite => "?1",
            Dialect::Postgres => "$1",
        };
        db.execute(
            &format!("INSERT INTO \"order\" (\"id\", \"number\") VALUES ({p}, 1)"),
            &[text(&uuid(1))],
        )
        .await
        .expect(t.name);
        assert_eq!(index_count(db, "order").await, Some(3), "{}", t.name);
    }
    finish(all).await;
}

// A column with a default stays `NOT NULL` when retired: inserts that leave
// it out get the default. Retiring renders no SQL then.
#[test]
fn retiring_a_defaulted_column_changes_nothing_in_the_database() {
    let mut replay = Replay::new();
    replay.apply(&base_ops()).expect("replays");
    let r = replay
        .apply(&[alter(
            "order",
            vec![AlterOp::RetireColumn {
                name: "status".into(),
            }],
        )])
        .expect("replays");
    assert!(r.sqlite.is_empty() && r.postgres.is_empty(), "{r:?}");
    let r = replay
        .apply(&[SchemaOp::RetireTable {
            name: "line".into(),
        }])
        .expect("replays");
    assert!(r.sqlite.is_empty() && r.postgres.is_empty(), "{r:?}");
    assert!(replay.is_retired_table("line"));
}

// Which operations destroy data, and which of those still touch something
// the previous release reads (live), so `k check` can ask for `@Breaking`.
#[test]
fn destructive_operations_are_reported() {
    let mut replay = Replay::new();
    replay.apply(&base_ops()).expect("replays");
    replay
        .apply(&[
            alter(
                "order",
                vec![AlterOp::RetireColumn {
                    name: "note".into(),
                }],
            ),
            SchemaOp::RetireTable {
                name: "line".into(),
            },
        ])
        .expect("replays");
    let r = replay
        .apply(&[
            alter(
                "order",
                vec![
                    AlterOp::AddEnumCase {
                        column: "status".into(),
                        case: "returned".into(),
                    },
                    AlterOp::DropColumn {
                        name: "note".into(),
                    },
                ],
            ),
            SchemaOp::DropTable {
                name: "line".into(),
            },
            alter(
                "order",
                vec![
                    AlterOp::RenameEnumCase {
                        column: "status".into(),
                        from: "lost".into(),
                        to: "missing".into(),
                    },
                    AlterOp::RemoveEnumCase {
                        column: "status".into(),
                        case: "missing".into(),
                        replacement: "shipped".into(),
                    },
                    AlterOp::RenameColumn {
                        from: "number".into(),
                        to: "reference".into(),
                    },
                    AlterOp::DropColumn {
                        name: "reference".into(),
                    },
                ],
            ),
            SchemaOp::RenameTable {
                from: "order".into(),
                to: "purchase".into(),
            },
        ])
        .expect("replays");
    let found: Vec<(usize, bool, &str)> = r
        .destructive
        .iter()
        .map(|d| (d.op, d.live, d.what.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            (0, false, "drops the retired column `order.note`"),
            (1, false, "drops the retired table `line`"),
            (2, false, "renames the case `lost` of `order.status`"),
            (2, false, "removes the case `missing` of `order.status`"),
            (2, true, "renames the column `order.number`"),
            (2, true, "drops the column `order.reference`"),
            (3, true, "renames the table `order`"),
        ]
    );
}

// Mistakes in the new operations are errors naming the problem and the
// operation's position in the migration.
#[test]
fn mistaken_schema_changes_are_errors() {
    let base = || {
        let mut r = Replay::new();
        r.apply(&base_ops()).expect("replays");
        r
    };
    let cases: Vec<(AlterOp, &str)> = vec![
        (
            AlterOp::AddEnumCase {
                column: "note".into(),
                case: "x".into(),
            },
            "isn't an enumeration column",
        ),
        (
            AlterOp::AddEnumCase {
                column: "status".into(),
                case: "paid".into(),
            },
            "already has the case `paid`",
        ),
        (
            AlterOp::AddEnumCase {
                column: "status".into(),
                case: String::new(),
            },
            "empty",
        ),
        (
            AlterOp::RenameEnumCase {
                column: "status".into(),
                from: "nope".into(),
                to: "x".into(),
            },
            "has no case `nope`",
        ),
        (
            AlterOp::RenameEnumCase {
                column: "status".into(),
                from: "paid".into(),
                to: "lost".into(),
            },
            "already has the case `lost`",
        ),
        (
            AlterOp::RemoveEnumCase {
                column: "status".into(),
                case: "lost".into(),
                replacement: "lost".into(),
            },
            "replacement",
        ),
        (
            AlterOp::RemoveEnumCase {
                column: "status".into(),
                case: "lost".into(),
                replacement: "gone".into(),
            },
            "has no case `gone`",
        ),
        (AlterOp::RetireColumn { name: "id".into() }, "primary key"),
        (
            AlterOp::RetireColumn {
                name: "nope".into(),
            },
            "no column `nope`",
        ),
    ];
    for (op, message) in cases {
        let err = base()
            .try_apply(&[
                SchemaOp::Execute {
                    postgres: "SELECT 1".into(),
                    sqlite: "SELECT 1".into(),
                },
                alter("order", vec![op.clone()]),
            ])
            .expect_err(message);
        assert_eq!(err.op, 1, "{op:?}");
        assert!(err.message.contains(message), "{op:?}: {}", err.message);
    }
    let mut r = base();
    r.apply(&[alter(
        "order",
        vec![AlterOp::RetireColumn {
            name: "note".into(),
        }],
    )])
    .expect("retires");
    let err = r
        .apply(&[alter(
            "order",
            vec![AlterOp::RetireColumn {
                name: "note".into(),
            }],
        )])
        .expect_err("twice");
    assert!(err.contains("already retired"), "{err}");
    let err = r
        .apply(&[SchemaOp::RetireTable {
            name: "nope".into(),
        }])
        .expect_err("unknown");
    assert!(err.contains("no table `nope`"), "{err}");
}

// The lock a replay produces records every table and column with the
// options `schema.lock` writes, the retired ones marked, and no entity or
// field (those come from the code).
#[test]
fn a_replay_produces_the_physical_lock() {
    let mut replay = Replay::new();
    replay.apply(&base_ops()).expect("replays");
    replay
        .apply(&[
            alter(
                "order",
                vec![
                    AlterOp::AddColumn(ColumnDef {
                        default: Some(Literal::Boolean(false)),
                        ..col("is_gift", ColumnType::Boolean)
                    }),
                    AlterOp::RetireColumn {
                        name: "note".into(),
                    },
                ],
            ),
            SchemaOp::CreateIndex {
                table: "order".into(),
                columns: vec!["is_gift".into()],
            },
            SchemaOp::RetireTable {
                name: "line".into(),
            },
        ])
        .expect("replays");
    let lock = replay.lock_tables();
    let order = &lock["order"];
    assert!(order.entity.is_empty() && !order.retired);
    let rows: Vec<String> = order
        .columns
        .iter()
        .map(|c| {
            format!(
                "{} {} pk={} null={} unique={} indexed={} ref={:?} cases={:?} default={:?} retired={}",
                c.name,
                c.ty,
                c.primary_key,
                c.nullable,
                c.unique,
                c.indexed,
                c.references,
                c.cases,
                c.default,
                c.retired
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            "id uuid pk=true null=false unique=false indexed=false ref=None cases=[] default=None retired=false",
            "number integer pk=false null=false unique=true indexed=false ref=None cases=[] default=None retired=false",
            "status enumeration pk=false null=false unique=false indexed=true ref=None cases=[\"paid\", \"shipped\", \"lost\"] default=Some(\"paid\") retired=false",
            "note text pk=false null=true unique=false indexed=false ref=None cases=[] default=None retired=true",
            "is_gift boolean pk=false null=false unique=false indexed=true ref=None cases=[] default=Some(\"false\") retired=false",
        ]
    );
    assert!(lock["line"].retired);
    let fk = &lock["line"].columns[1];
    assert_eq!(
        (fk.references.as_deref(), fk.indexed),
        (Some("order.id"), true)
    );
    assert!(order.columns.iter().all(|c| c.field.is_none()));
}

// Renames are logged, so the lock's entity and field names can follow a
// physical rename.
#[test]
fn renames_are_logged() {
    let mut replay = Replay::new();
    replay.apply(&base_ops()).expect("replays");
    replay
        .apply(&[
            alter(
                "order",
                vec![AlterOp::RenameColumn {
                    from: "note".into(),
                    to: "remark".into(),
                }],
            ),
            SchemaOp::RenameTable {
                from: "order".into(),
                to: "purchase".into(),
            },
        ])
        .expect("replays");
    assert_eq!(replay.current_table("order").as_deref(), Some("purchase"));
    assert_eq!(
        replay.current_column("order", "note"),
        Some(("purchase".to_owned(), "remark".to_owned()))
    );
    assert_eq!(
        replay.current_column("order", "status"),
        Some(("purchase".to_owned(), "status".to_owned()))
    );
    assert_eq!(replay.current_column("order", "gone"), None);
}

// Dropping a column that a multi-column index covers drops that index
// first, which SQLite requires.
#[tokio::test]
async fn dropping_an_indexed_column_drops_its_indexes() {
    let mut replay = Replay::new();
    let init = replay.apply(&base_ops()).expect("replays");
    let drop = replay
        .apply(&[alter(
            "order",
            vec![AlterOp::DropColumn {
                name: "number".into(),
            }],
        )])
        .expect("replays");
    assert!(
        drop.sqlite
            .contains(&"DROP INDEX IF EXISTS \"order_status_number_idx\"".to_owned()),
        "{:?}",
        drop.sqlite
    );
    let all = databases().await;
    for t in &all {
        migrate::apply(
            &t.db,
            &[
                migration("0001_init", init.clone()),
                migration("0002_drop", drop.clone()),
            ],
        )
        .await
        .expect(t.name);
        assert_eq!(index_count(&t.db, "order").await, Some(1), "{}", t.name);
    }
    finish(all).await;
}

// SQLite runs migrations with foreign key enforcement off (a rebuild drops
// a referenced table), so the keys are checked before each commit: a
// migration that leaves a broken reference fails and is rolled back on
// both databases.
#[tokio::test]
async fn a_migration_that_breaks_a_foreign_key_is_rolled_back() {
    let mut replay = Replay::new();
    let init = replay.apply(&base_ops()).expect("replays");
    let orphan = replay
        .apply(&[SchemaOp::Insert {
            table: "line".into(),
            rows: vec![vec![
                ("id".into(), Literal::Uuid(uuid(1))),
                ("order_id".into(), Literal::Uuid(uuid(2))),
            ]],
        }])
        .expect("replays");
    let all = databases().await;
    for t in &all {
        let err = migrate::apply(
            &t.db,
            &[
                migration("0001_init", init.clone()),
                migration("0002_orphan", orphan.clone()),
            ],
        )
        .await
        .expect_err(t.name);
        assert!(err.to_string().contains("0002_orphan"), "{}: {err}", t.name);
        assert_eq!(
            scalar(&t.db, "SELECT COUNT(*) FROM \"line\"", &[]).await,
            Some(0),
            "{}",
            t.name
        );
        assert_eq!(
            migrate::applied(&t.db).await.expect(t.name),
            ["0001_init"],
            "{}",
            t.name
        );
        // Enforcement is back on after the migration.
        assert!(!insert_order_line_only(&t.db).await, "{}", t.name);
    }
    finish(all).await;
}

/// Inserts a line referencing a missing order outside any migration.
async fn insert_order_line_only(db: &k_db::driver::Database) -> bool {
    let sql = match db.dialect() {
        Dialect::Sqlite => "INSERT INTO \"line\" (\"id\", \"order_id\") VALUES (?1, ?2)",
        Dialect::Postgres => "INSERT INTO \"line\" (\"id\", \"order_id\") VALUES ($1, $2)",
    };
    db.execute(sql, &[text(&uuid(7)), text(&uuid(8))])
        .await
        .is_ok()
}
