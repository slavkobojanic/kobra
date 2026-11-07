//! The golden corpus test harness of the K toolchain.
//!
//! The corpus lives in `tests/corpus/` at the repository root. Each direct
//! subdirectory is a *suite* (`lexer`, `parser`, ...); every `*.k` file
//! anywhere below a suite directory is one *case*. A case's expectations are
//! sibling files with the same stem and a suite-specific extension:
//!
//! | Suite | Expectation files | Stage under test |
//! |---|---|---|
//! | `lexer` | `name.tokens` (required), `name.diag` (optional) | [`k_syntax::lex`] |
//! | `parser` | `name.ast` (required), `name.diag` (optional) | [`k_syntax::parse`] |
//! | `sema` | `name.diag`, `name.pending` (both optional); projects: `expected.diag`, `expected.pending` | [`k_sema::Analysis::diagnostics`] and the position queries |
//! | `codegen` | `name.client.js`, `name.server.js`, `name.css`, `name.expect` (at least one), `name.pending` | [`k_codegen::generate`]; see [`codegen_suite`] |
//! | `run` | `name.stdout`, `name.result`, `name.pending` (all optional) | compile and execute in V8; see [`run_suite`] (executed by `k_server`'s `corpus_run` test) |
//!
//! A missing `.diag` file means "no diagnostics expected". The exact file
//! formats are specified in `docs/compiler/testing.md`; this crate implements
//! them:
//!
//! * [`diag_format`] reads and writes `.diag` files and matches them against
//!   actual diagnostics;
//! * [`annotations`] reads the caret annotations of `sema` cases;
//! * [`sema_suite`] runs `sema` cases, single files and projects;
//! * `.tokens` and `.ast` files are read with [`k_syntax::dump`] and
//!   [`k_syntax::sexpr`];
//! * [`discover`] finds cases and reports misplaced files;
//! * [`suites`] runs a case;
//! * [`diff`] renders readable line diffs for failures.
//!
//! The test binary `tests/corpus.rs` turns every case into one test, so
//! `cargo test -p k_corpus -- lexer/strings` runs a subset by name.
//!
//! # Blessing
//!
//! With the environment variable `K_BLESS=1`, failing cases overwrite their
//! expectation files with the actual output instead of failing (a `.diag`
//! file is deleted when no diagnostics remain). Corpus expectations are
//! normally written by hand *before* the implementation exists; blessing is
//! for bulk updates after a deliberate, reviewed change.

#![warn(clippy::missing_docs_in_private_items)]

pub mod annotations;
pub mod codegen_suite;
pub mod diag_format;
pub mod diff;
pub mod discover;
pub mod run_suite;
pub mod sema_suite;
pub mod suites;

use std::path::PathBuf;

/// The environment variable that turns on blessing.
pub const BLESS_ENV: &str = "K_BLESS";

/// Returns the corpus root, `tests/corpus` at the repository root, as an
/// absolute path without `..` components when it exists.
#[must_use]
pub fn corpus_root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    root.canonicalize().unwrap_or(root)
}

/// Returns `true` if `K_BLESS` is set to a non-empty value other than `0`.
#[must_use]
pub fn bless_enabled() -> bool {
    std::env::var_os(BLESS_ENV).is_some_and(|value| !value.is_empty() && value != "0")
}
