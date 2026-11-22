//! `k docs` (`docs/design/48-releases-and-versioning.md`, "Public
//! distribution"): the book of the running toolchain, installed with it in
//! `$K_HOME/docs/<version>/` by the install scripts and `k toolchain
//! install`, so it reads offline. `--path` prints the page; without it the
//! page is opened with `$BROWSER` (a fake here) or the system's opener; a
//! toolchain without its book names the docs site and how to read one
//! from a checkout.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// The running toolchain's version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root =
            std::env::temp_dir().join(format!("k_docs_cmd_{tag}_{}_{n}", std::process::id()));
        fs::create_dir_all(root.join("home")).unwrap();
        Fixture { root }
    }

    fn book(&self) -> PathBuf {
        self.root.join("home/docs").join(VERSION)
    }

    fn install_book(&self) {
        fs::create_dir_all(self.book()).unwrap();
        fs::write(self.book().join("index.html"), "<title>the book</title>\n").unwrap();
    }

    fn k(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_k"));
        command
            .args(args)
            .current_dir(&self.root)
            .env("K_HOME", self.root.join("home"))
            .env_remove("K_TOOLCHAIN")
            .env_remove("K_TOOLCHAIN_DELEGATED")
            .env_remove("BROWSER");
        for (k, v) in env {
            command.env(k, v);
        }
        command.output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn describe(out: &Output) -> String {
    format!(
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        text(&out.stdout),
        text(&out.stderr)
    )
}

#[test]
fn path_prints_the_installed_books_page() {
    let fx = Fixture::new("path");
    fx.install_book();
    let out = fx.k(&["docs", "--path"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        text(&out.stdout).trim(),
        fx.book().join("index.html").display().to_string(),
        "{}",
        describe(&out)
    );
}

#[cfg(unix)]
#[test]
fn docs_opens_the_page_with_browser() {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fixture::new("open");
    fx.install_book();
    let opened = fx.root.join("opened");
    let browser = fx.root.join("browser");
    fs::write(
        &browser,
        format!("#!/bin/sh\necho \"$1\" >'{}'\n", opened.display()),
    )
    .unwrap();
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o755)).unwrap();
    let out = fx.k(&["docs"], &[("BROWSER", &browser.display().to_string())]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(&opened).unwrap().trim(),
        fx.book().join("index.html").display().to_string()
    );
    assert!(text(&out.stderr).contains("opening"), "{}", describe(&out));
}

#[test]
fn without_the_book_docs_names_the_site_and_the_source() {
    let fx = Fixture::new("missing");
    for args in [&["docs"][..], &["docs", "--path"][..]] {
        let out = fx.k(args, &[("BROWSER", "/nonexistent/browser")]);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", describe(&out));
        let err = text(&out.stderr);
        assert!(
            err.contains(&format!("the book of k {VERSION} isn't installed")),
            "{err}"
        );
        assert!(err.contains("mdbook serve docs/book"), "{err}");
        // The docs site, at this release's minor (or /next/ for a nightly).
        let minor: Vec<&str> = VERSION.split(['.', '-']).take(2).collect();
        let page = if VERSION.contains("-nightly.") {
            "https://slavkobojanic.github.io/kobra/next/".to_owned()
        } else {
            format!("https://slavkobojanic.github.io/kobra/{}/", minor.join("."))
        };
        assert!(err.contains(&page), "{err}");
        assert!(text(&out.stdout).is_empty(), "{}", describe(&out));
    }
}
