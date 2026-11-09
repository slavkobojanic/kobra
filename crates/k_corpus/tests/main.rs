//! The integration tests of `k_corpus`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_corpus --test it -- book_reference::
//! ```
//!
//! `tests/corpus.rs` keep binaries of their own (`harness = false`, see Cargo.toml).

mod book_reference;
