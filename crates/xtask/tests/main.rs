//! The integration tests of `xtask`, as one test binary
//! (docs/design/13-tooling.md, "One test binary per crate"):
//!
//! ```text
//! cargo test -p xtask --test it -- prepare::
//! ```

#![allow(clippy::unwrap_used, clippy::panic)]

mod bump;
mod fragments;
mod gate;
mod prepare;
