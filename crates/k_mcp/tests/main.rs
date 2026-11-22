//! The integration tests of `k_mcp`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"); run one module with
//!
//! ```text
//! cargo test -p k_mcp --test it -- server::
//! ```

mod server;
