//! Shared code for the K fuzz targets.
//!
//! The invariants ([`oracle`]) and the structure-aware program generator
//! ([`kgen`]) are not duplicated here: they are the very files the stable
//! property tests use (`crates/k_syntax/tests/support/`), included by path,
//! so fuzzing and `cargo test` check exactly the same properties.

#![allow(missing_docs)]

#[path = "../../crates/k_syntax/tests/support/kgen.rs"]
pub mod kgen;
#[path = "../../crates/k_syntax/tests/support/oracle.rs"]
pub mod oracle;
