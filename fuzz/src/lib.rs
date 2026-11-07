//! Shared code for the K fuzz targets.
//!
//! The invariants ([`oracle`] for syntax, [`soracle`] for semantic
//! analysis, [`coracle`] for code generation and execution, [`horacle`]
//! for the HTTP server) and the generators ([`kgen`] for the grammar,
//! [`tgen`] for well-typed projects, [`exprgen`], [`codablegen`] and
//! [`viewgen`] for the stage 3 differential properties) are not duplicated
//! here: they are the very files the stable property tests use
//! (`crates/k_syntax/tests/support/`, `crates/k_sema/tests/support/` and
//! `crates/k_server/tests/support/`), included by path, so fuzzing and
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
