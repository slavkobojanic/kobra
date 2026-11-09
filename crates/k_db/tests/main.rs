//! The integration tests of `k_db`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_db --test it -- builder::
//! ```

// Support modules shared by the tests below.
mod common;

mod builder;
mod drivers;
mod errors;
mod lock;
mod lock_edges;
mod migrations;
mod perf_budget;
mod queries;
mod relations;
mod schema_changes;
mod schema_mapping;
mod sql;
mod sql_properties;
mod transactions;
mod values;
mod versioning;
mod writes;
