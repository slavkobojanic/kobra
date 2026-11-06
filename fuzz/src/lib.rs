//! Shared code for the K fuzz targets.
//!
//! The invariants ([`oracle`] for syntax, [`soracle`] for semantic
//! analysis) and the generators ([`kgen`] for the grammar, [`tgen`] for
//! well-typed projects) are not duplicated here: they are the very files the
//! stable property tests use (`crates/k_syntax/tests/support/` and
//! `crates/k_sema/tests/support/`), included by path, so fuzzing and
//! `cargo test` check exactly the same properties.

#![allow(missing_docs)]

#[path = "../../crates/k_syntax/tests/support/kgen.rs"]
pub mod kgen;
#[path = "../../crates/k_syntax/tests/support/oracle.rs"]
pub mod oracle;
#[path = "../../crates/k_sema/tests/support/soracle.rs"]
pub mod soracle;
#[path = "../../crates/k_sema/tests/support/tgen.rs"]
pub mod tgen;
