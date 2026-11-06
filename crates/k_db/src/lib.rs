//! Persistence for K: schema model, query IR, SQL generation, migrations
//! and database drivers.
//!
//! This crate is deliberately low-level: it does not depend on the compiler,
//! so the server runtime can use its drivers without linking `k_sema`.
//!
//! `k_db` owns everything between K's `entity` and `repository`
//! declarations and a real database:
//!
//! * the **schema model** derived from entities, and `schema.lock`;
//! * the **query IR** that `k_sema` lowers repository lambdas (`filter`,
//!   `sortedBy`, `limit`, `first`, `including`, ...) into, and **SQL
//!   generation** from it for both dialects, with bind parameters;
//! * **migrations**: `k migrate new` diffs entities against `schema.lock` and
//!   writes a reviewable migration; destructive changes require
//!   `@Breaking("reason")`;
//! * the **drivers**: SQLite (development and tests, with per-test
//!   transactions rolled back) and Postgres (production), exposed to the
//!   server runtime as Rust ops.
//!
//! The design is in `docs/compiler/architecture.md`, "Persistence".
//!
//! # Status
//!
//! Not implemented yet.
