//! Persistence for K: the schema model, the query IR, SQL generation for
//! SQLite and Postgres, the drivers, the migration builder, migrations,
//! `schema.lock` and the build's database manifest.
//!
//! `k_db` is deliberately low-level and knows nothing about K syntax:
//!
//! * `k_sema` checks repository lambdas and reports what can't be
//!   translated;
//! * `k_codegen` lowers each awaited repository query into the [`ir`],
//!   renders it for both dialects with [`sql::render`], replays the
//!   migrations with [`builder::Replay`], and puts the statements (by
//!   number), the migrations and the unique indexes into a
//!   [`manifest::DbManifest`], never SQL into JavaScript;
//! * `k_server` loads the manifest, applies the pending migrations with
//!   [`migrate`] at startup, and runs statements through a
//!   [`driver::Database`] when generated code calls the database ops.
//!
//! Every statement is parameterized: values never appear in SQL text, and
//! identifiers are always quoted ([`sql::quote_identifier`]). The design is
//! `docs/compiler/db.md`; the language rules are
//! `docs/design/19-persistence.md`.
//!
//! # Example
//!
//! Render a query from the IR for the connected database's dialect and run
//! it with its parameters bound (an in-memory SQLite database here; a
//! `postgres://` URL works the same way):
//!
//! ```
//! use k_db::ir::{CmpOp, Operand, Predicate, Select, Statement};
//! use k_db::{Database, DatabaseOptions, Value};
//!
//! let runtime = tokio::runtime::Builder::new_current_thread().build()?;
//! runtime.block_on(async {
//!     let db = Database::connect("sqlite::memory:", &DatabaseOptions::default()).await?;
//!     db.execute(r#"CREATE TABLE "item" ("name" TEXT NOT NULL, "stock" INTEGER NOT NULL)"#, &[])
//!         .await?;
//!     db.execute(
//!         r#"INSERT INTO "item" ("name", "stock") VALUES (?1, ?2), (?3, ?4)"#,
//!         &[Value::Text("pen".into()), Value::Int(3), Value::Text("ink".into()), Value::Int(0)],
//!     )
//!     .await?;
//!
//!     // `items.filter { $0.stock > minimum }`, with `minimum` bound as ?1.
//!     let query = Statement::Select(Select {
//!         table: "item".into(),
//!         columns: vec!["name".into()],
//!         filter: Some(Predicate::Compare {
//!             left: Operand::Column("stock".into()),
//!             op: CmpOp::Gt,
//!             right: Operand::Param(1),
//!         }),
//!         order: Vec::new(),
//!         limit: None,
//!         offset: None,
//!     });
//!     let sql = k_db::sql::render(&query, db.dialect());
//!     assert_eq!(sql.text, r#"SELECT "name" FROM "item" WHERE "stock" > ?1"#);
//!
//!     let rows = db.query(&sql.text, &[Value::Int(0)]).await?;
//!     assert_eq!(rows.len(), 1);
//!     assert_eq!(rows[0].get("name"), Some(&Value::Text("pen".into())));
//!     Ok::<(), k_db::DbError>(())
//! })?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(clippy::missing_docs_in_private_items)]

pub mod builder;
pub mod decimal;
pub mod driver;
pub mod error;
pub mod ir;
pub mod lock;
pub mod manifest;
pub mod migrate;
pub mod schema;
pub mod sql;
pub mod stdlib;
pub mod testing;
pub mod value;

pub use driver::{Database, DatabaseOptions, PostgresListener};
pub use error::DbError;
pub use sql::{Dialect, Sql};
pub use value::{Row, Value};
