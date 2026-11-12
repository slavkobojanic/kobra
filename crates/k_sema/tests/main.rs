//! The integration tests of `k_sema`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_sema --test it -- book::
//! ```

// Support modules shared by the tests below.
#[allow(clippy::panic, clippy::unwrap_used, missing_docs)]
#[path = "../../k_syntax/tests/support/kgen.rs"]
mod kgen;
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "support/mdbook.rs"]
mod mdbook;
#[allow(clippy::panic, clippy::print_stdout, clippy::unwrap_used)]
#[path = "../benches/project/mod.rs"]
mod project;
#[allow(clippy::panic, clippy::unwrap_used, missing_docs)]
#[path = "support/soracle.rs"]
mod soracle;
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "support/tgen.rs"]
mod tgen;

mod book;
mod book_links;
mod examples;
mod perf_budget;
mod project_workload;
mod properties;
mod robustness;
mod tooling;
mod warm;
mod web_platform;
mod workspaces;
