//! Shared code for the K fuzz targets.
//!
//! The invariants ([`oracle`] for syntax, [`soracle`] for semantic
//! analysis, [`coracle`] for code generation and execution, [`horacle`]
//! for the HTTP server) and the generators ([`kgen`] for the grammar,
//! [`tgen`] for well-typed projects, [`exprgen`], [`codablegen`] and
//! [`viewgen`] for the stage 3 differential properties, [`lspdriver`] for
//! the language server) are not duplicated here: they are the very files
//! the stable property tests use (`crates/k_syntax/tests/support/`,
//! `crates/k_sema/tests/support/`, `crates/k_server/tests/support/` and
//! `crates/k_lsp/tests/support/`), included by path, so fuzzing and
//! `cargo test` check exactly the same properties.

#![allow(missing_docs)]

#[path = "../../crates/k_server/tests/support/codablegen.rs"]
pub mod codablegen;
#[path = "../../crates/k_server/tests/support/coracle.rs"]
pub mod coracle;
#[path = "../../crates/k_server/tests/support/exprgen.rs"]
pub mod exprgen;
#[path = "../../crates/k_server/tests/support/horacle.rs"]
pub mod horacle;
#[path = "../../crates/k_syntax/tests/support/kgen.rs"]
pub mod kgen;
#[path = "../../crates/k_lsp/tests/support/driver.rs"]
pub mod lspdriver;
#[path = "../../crates/k_syntax/tests/support/oracle.rs"]
pub mod oracle;
#[path = "../../crates/k_sema/tests/support/soracle.rs"]
pub mod soracle;
#[path = "../../crates/k_sema/tests/support/tgen.rs"]
pub mod tgen;
#[path = "../../crates/k_server/tests/support/viewgen.rs"]
pub mod viewgen;

use std::net::SocketAddr;
use std::sync::OnceLock;

/// The longest input the `check` target analyses, in bytes; longer inputs
/// return at once. Every input is a whole-project analysis (the standard
/// library included), so its cost grows with the text, and 2 KiB holds
/// every construct the golden corpus has. Pass the same value as
/// `-max_len` (docs/compiler/testing.md §6.3).
pub const CHECK_MAX_INPUT: usize = 2048;

/// The longest input the generator-driven semantic targets
/// (`check_structured`, `check_typed`) use, in bytes; longer inputs return
/// at once. The bytes size the generated project, and each input runs
/// several analyses, so a cap keeps every input well under a second while
/// 1 KiB still reaches every generator branch. Pass the same value as
/// `-max_len`.
pub const GENERATOR_MAX_INPUT: usize = 1024;

/// The address of the fuzz app's server, started on first use and kept for
/// the life of the process (each fuzz target is one process).
///
/// # Panics
///
/// If the app doesn't compile or the server doesn't start: the harness is
/// broken, not the input.
pub fn server() -> SocketAddr {
    static ADDR: OnceLock<SocketAddr> = OnceLock::new();
    *ADDR.get_or_init(|| {
        let server = horacle::start_server().expect("the fuzz app starts");
        let addr = server.local_addr();
        // Never shut down: the process ends with the fuzzer.
        std::mem::forget(server);
        addr
    })
}
