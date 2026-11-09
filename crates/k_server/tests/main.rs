//! The integration tests of `k_server`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_server --test it -- browser::
//! ```
//!
//! `tests/corpus_run.rs`, `tests/e2e.rs`, `tests/runtime_js.rs` keep binaries of their own (`harness = false`, see Cargo.toml).

// Support modules shared by the tests below.
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#[path = "support/codablegen.rs"]
mod codablegen;
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#[path = "support/coracle.rs"]
mod coracle;
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#[path = "support/exprgen.rs"]
mod exprgen;
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#[path = "support/horacle.rs"]
mod horacle;
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#[path = "../../k_sema/tests/support/tgen.rs"]
mod tgen;
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#[path = "support/viewgen.rs"]
mod viewgen;

mod browser;
mod cancellation;
mod channels;
mod client_bundle;
mod config;
mod differential;
mod engine;
mod literals;
mod outbound;
mod perf_budget;
#[path = "regressions/main.rs"]
mod regressions;
mod server;
mod sessions;
mod shop_production;
mod stage3_server;
mod theme_snippet;
