//! The integration tests of `k_lsp`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_lsp --test it -- editors::
//! ```

// Support modules shared by the tests below.
#[allow(clippy::print_stdout)]
#[path = "support/driver.rs"]
mod driver;
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::unwrap_used
)]
#[path = "../../k_sema/benches/project/mod.rs"]
mod project;

mod editors;
mod perf_budget;
mod properties;
mod server;
mod textmate;
