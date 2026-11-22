//! `k docs`: the running toolchain's book
//! (`docs/design/48-releases-and-versioning.md`, "Public distribution").
//!
//! Each release attaches its built book (`docs-<version>.tar.gz`), and the
//! install scripts and `k toolchain install` unpack it into
//! `$K_HOME/docs/<version>/`, so it reads offline and matches this `k`.
//! `k docs` opens that book's `index.html` with `$BROWSER` when set, else
//! the system's opener (`open`, `xdg-open`, `start`); `k docs --path`
//! prints the page's path. Nothing is fetched. Without the book it names
//! the docs site's copy (`<site><minor>/`, or `<site>next/` for a nightly).

use std::path::Path;
use std::process::ExitCode;

use crate::cli::DocsArgs;
use crate::{index, toolchain};

/// The docs site (`[workspace.metadata.project] site`;
/// `crates/k_cli/tests/version.rs` checks they agree).
pub(crate) const SITE: &str = "https://slavkobojanic.github.io/kobra/";

/// Runs `k docs`.
pub fn run(args: &DocsArgs) -> ExitCode {
    let version = toolchain::running();
    let Some(dir) = toolchain::k_home().map(|h| h.join("docs").join(version.to_string())) else {
        eprintln!("error: no home directory: set K_HOME to where toolchains are installed");
        return ExitCode::from(2);
    };
    let page = dir.join("index.html");
    if !page.is_file() {
        let online = if index::is_nightly(&version) {
            format!("{SITE}next/")
        } else {
            format!("{SITE}{}.{}/", version.major, version.minor)
        };
        eprintln!(
            "error: the book of k {version} isn't installed (in {}): read it online at {online}; \
             the install script installs it with the toolchain, as does `k toolchain install \
             {version}` from a release that has it; in a checkout of Kobra, read it with \
             `mdbook serve docs/book`",
            dir.display()
        );
        return ExitCode::from(1);
    }
    if args.path {
        println!("{}", page.display());
        return ExitCode::SUCCESS;
    }
    eprintln!("opening {}", page.display());
    match open(&page) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: cannot open {}: {e}", page.display());
            ExitCode::from(1)
        }
    }
}

/// Opens `page` with `$BROWSER` or the system's opener.
fn open(page: &Path) -> Result<(), String> {
    let mut command = if let Some(browser) = std::env::var_os("BROWSER").filter(|b| !b.is_empty()) {
        std::process::Command::new(browser)
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    let status = command
        .arg(page)
        .status()
        .map_err(|e| format!("cannot run the browser: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("the browser exited with {status}"))
    }
}
