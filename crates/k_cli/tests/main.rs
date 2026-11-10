//! The integration tests of `k_cli`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_cli --test it -- book_commands::
//! ```

// Support modules shared by the tests below.
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "support/apps.rs"]
mod apps;
#[allow(clippy::panic, clippy::unwrap_used, dead_code)]
#[path = "support/cdp.rs"]
mod cdp;
#[allow(
    clippy::panic,
    clippy::unwrap_used,
    dead_code,
    missing_docs,
    unused_qualifications
)]
#[path = "../src/cli.rs"]
mod cli_support;
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "../../k_sema/tests/support/mdbook.rs"]
mod mdbook;
#[allow(clippy::panic, clippy::unwrap_used, dead_code)]
#[path = "support/page.rs"]
mod page;
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "support/ports.rs"]
mod ports;
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used, dead_code)]
#[path = "../../k_server/tests/support/postgres.rs"]
mod postgres;
#[allow(clippy::panic, clippy::unwrap_used, dead_code)]
#[path = "support/static_host.rs"]
mod static_host;

mod book_commands;
mod book_projects;
mod call_deploy;
mod chrome;
mod cli;
mod commands;
mod compose;
mod debugging;
mod deploy;
mod dist_browser;
mod example_checks;
mod examples;
mod lsp;
mod new;
mod perf_budget;
mod push;
mod version;
mod workspaces;
