//! The integration tests of `k_cli`, as one test binary (docs/design/13-tooling.md,
//! "One test binary per crate"). Each module is a file next to this one that used to
//! be a test binary of its own; run one with
//!
//! ```text
//! cargo test -p k_cli --test it -- book_commands::
//! ```

/// `k = "X.Y"`, this toolchain's minor, for the manifests tests write: a
/// literal `"0.1"` made `k` refuse them (K8201) once the version moved on
/// (the 0.1.0-rc.1 dry run, docs/compiler/release.md §12 step 15).
macro_rules! k_requirement {
    () => {
        concat!(
            "k = \"",
            env!("CARGO_PKG_VERSION_MAJOR"),
            ".",
            env!("CARGO_PKG_VERSION_MINOR"),
            "\""
        )
    };
}

// Support modules shared by the tests below.
#[allow(clippy::panic, clippy::unwrap_used)]
#[path = "support/apps.rs"]
mod apps;
#[allow(clippy::panic, clippy::unwrap_used, dead_code)]
#[path = "support/bidi.rs"]
mod bidi;
#[allow(clippy::panic, clippy::unwrap_used, dead_code)]
#[path = "support/cdp.rs"]
mod cdp;
#[allow(clippy::panic, clippy::unwrap_used, dead_code)]
#[path = "support/children.rs"]
mod children;
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
#[path = "support/release.rs"]
mod release;
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
mod docs_command;
mod engines;
mod example_checks;
mod examples;
mod firefox;
mod fix;
mod fmt;
mod install_script;
#[cfg(unix)]
mod kubernetes;
mod lsp;
mod new;
mod perf_budget;
mod push;
mod toolchain;
mod upgrade;
mod version;
mod workspaces;
