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
pub mod testing;
pub mod value;

pub use driver::{Database, DatabaseOptions};
pub use error::DbError;
pub use sql::{Dialect, Sql};
pub use value::{Row, Value};
