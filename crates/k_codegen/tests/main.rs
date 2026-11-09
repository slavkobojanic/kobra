//! The integration tests of `k_codegen`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_codegen --test it -- codegen::
//! ```

// Support modules shared by the tests below.
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "../../k_sema/benches/project/mod.rs"]
mod project;

mod codegen;
mod perf_budget;
mod persistence;
mod persistence_properties;
mod release;
mod sourcemaps;
