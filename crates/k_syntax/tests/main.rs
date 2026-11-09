//! The integration tests of `k_syntax`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_syntax --test it -- adversarial::
//! ```

// Support modules shared by the tests below.
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "support/kgen.rs"]
mod kgen;
#[path = "support/oracle.rs"]
mod oracle;
#[allow(clippy::panic, clippy::print_stdout, clippy::unwrap_used)]
#[path = "../benches/workload/mod.rs"]
mod workload_support;

mod adversarial;
mod examples;
mod green_support;
mod invariants;
mod lexer;
mod parser_declarations;
mod parser_expressions;
mod parser_views_styles_recovery;
mod perf_budget;
mod properties;
mod robustness;
mod workload;
