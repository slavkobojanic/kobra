//! The Kobra MCP server, `k mcp`.
//!
//! An AI assistant (Claude Code, or any client of the Model Context
//! Protocol) runs `k mcp` and talks to it over standard input and output to
//! read Kobra's documentation and examples and to check the Kobra it
//! writes with the real compiler. The architecture, the tools and the
//! install command are in `docs/compiler/mcp.md`; the decision is
//! recorded in `docs/design/13-tooling.md`, "AI assistants".
//!
//! # Layers
//!
//! * [`server`]: newline-delimited JSON-RPC 2.0 (the MCP stdio
//!   transport), `initialize`, `ping`, the tool and resource methods.
//! * [`tools`]: the tools' definitions and calls.
//! * [`corpus`]: the documents, embedded at build time (`build.rs`:
//!   `docs/book/src`, `docs/design`, `docs/compiler`, `README.md`,
//!   `examples/`), plus the files of the project the server runs in.
//! * [`search`]: term-based ranked search.
//! * [`explain`]: diagnostic codes, from the registry and the references.
//! * [`check`]: snippets through the same analysis as `k check`.
//!
//! The server is written for robustness: malformed lines get JSON-RPC
//! errors and the connection goes on; nothing but protocol messages is
//! written to the output (logs go to standard error).

#![warn(clippy::missing_docs_in_private_items)]

pub mod check;
pub mod corpus;
pub mod explain;
pub mod search;
pub mod server;
pub mod tools;

pub use server::{Config, PROTOCOL_VERSION, SUPPORTED_VERSIONS, handle_line, serve};

/// Runs the server on standard input and output until the input ends.
/// The project is `project`, else the nearest directory with a `k.toml` at
/// or above the working directory.
///
/// Returns the process exit code: 0 when the input ended, 1 on an I/O
/// error.
#[must_use]
pub fn run_stdio(project: Option<std::path::PathBuf>) -> i32 {
    let project = project.or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|dir| corpus::find_project(&dir))
    });
    if let Some(root) = &project {
        eprintln!(
            "k mcp: serving the Kobra docs and the project at {}",
            root.display()
        );
    } else {
        eprintln!("k mcp: serving the Kobra docs (no project here)");
    }
    let config = Config { project };
    let input = std::io::BufReader::new(std::io::stdin());
    let stdout = std::io::stdout();
    match serve(input, stdout.lock(), &config) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("k mcp: {e}");
            1
        }
    }
}
