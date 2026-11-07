//! The K language server.
//!
//! The compiler and the language server are the same engine
//! (`docs/design/13-tooling.md`): every request is answered from an
//! [`Analysis`](k_sema::Analysis) snapshot of the project's
//! [`AnalysisHost`](k_sema::AnalysisHost), the same one `k check` queries,
//! so the editor never disagrees with the compiler. The architecture,
//! protocol coverage and budgets are in `docs/compiler/lsp.md`.
//!
//! # Layers
//!
//! * [`transport`]: JSON-RPC framing over any byte stream, robust against
//!   malformed input.
//! * [`server`]: the message loop: document synchronization feeding the
//!   host's [`Change`](k_sema::Change)s, a worker pool answering requests on
//!   snapshots, debounced diagnostics, cancellation.
//! * [`workspace`]: projects (directories with `k.toml`), their files on
//!   disk and the open documents that override them.
//! * [`ide`]: the features, as pure functions of a snapshot, a file and an
//!   offset.
//! * [`line_index`]: UTF-8 offsets to UTF-16 LSP positions and back.
//!
//! # Entry point
//!
//! `k lsp` calls [`run_stdio`]. Tests and fuzz targets drive [`serve`]
//! over in-memory streams.
//!
//! # Guarantees
//!
//! * **Never panics** on any input: malformed messages get JSON-RPC error
//!   responses, unknown methods `MethodNotFound`, bad parameters
//!   `InvalidParams`, and positions outside a document clamp to it. A
//!   worker that panics anyway (a bug) is contained, and its request gets
//!   an `InternalError` response.
//! * **Stale work is abandoned:** a request whose snapshot an edit made
//!   obsolete answers `ContentModified`; `$/cancelRequest` answers
//!   `RequestCancelled`.

pub mod ide;
pub mod line_index;
pub mod server;
pub mod transport;
pub mod workspace;

pub use server::{Config, serve};

/// Runs the language server on standard input and output until the client
/// sends `exit` or closes the input. Returns the process exit code: 0 after
/// a `shutdown` request, 1 otherwise (as the protocol specifies).
#[must_use]
pub fn run_stdio() -> i32 {
    let input = std::io::BufReader::new(std::io::stdin());
    let stdout = std::io::stdout();
    serve(input, stdout.lock(), Config::default())
}
