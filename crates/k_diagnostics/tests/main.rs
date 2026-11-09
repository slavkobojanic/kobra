//! The integration tests of `k_diagnostics`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_diagnostics --test it -- line_index_long_lines::
//! ```

mod line_index_long_lines;
