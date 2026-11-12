//! The integration tests of `k_fmt`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Run one module with
//!
//! ```text
//! cargo test -p k_fmt --test it -- golden::
//! ```

// Support modules shared by the tests below.
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "support/inputs.rs"]
mod inputs;
// The structure-aware generator of `k_syntax`'s property tests.
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "../../k_syntax/tests/support/kgen.rs"]
mod kgen;
#[path = "support/oracle.rs"]
mod oracle;

mod golden;
mod properties;
mod rules;
