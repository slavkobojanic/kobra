//! The command line of `cargo xtask`, in the library so the workflow test
//! (`tests/workflows.rs`) can parse every `cargo xtask …` line of
//! `.github/` with it.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// The K repository's release automation.
#[derive(Debug, Parser)]
#[command(name = "cargo xtask")]
pub struct Cli {
    /// The task.
    #[command(subcommand)]
    pub task: Task,
}

/// The tasks.
#[derive(Debug, Subcommand)]
pub enum Task {
    /// Cutting a release.
    Release {
        /// The release step.
        #[command(subcommand)]
        step: ReleaseStep,
    },
    /// Check that each user-visible fragment added since BASE lists book pages the diff changes.
    DocsGate {
        /// The commit the change is based on (`origin/main`).
        base: String,
    },
    /// Check every changelog fragment and list them.
    Fragments {
        /// Also require that none remains, as on a release tag.
        #[arg(long)]
        none_left: bool,
    },
    /// The docs site.
    Docs {
        /// The docs step.
        #[command(subcommand)]
        step: DocsStep,
    },
    /// The release artifacts: archives, checksums, the index, the smoke test.
    Dist {
        /// The artifact step.
        #[command(subcommand)]
        step: DistStep,
    },
    /// Helpers for the CI workflows in `.github/`.
    Ci {
        /// The helper.
        #[command(subcommand)]
        step: CiStep,
    },
}

/// `release` steps.
#[derive(Debug, Subcommand)]
pub enum ReleaseStep {
    /// Set VERSION everywhere and compile the fragments into CHANGELOG.md, the changelog page and the upgrade guide.
    Prepare {
        /// The version: `0.2.0-rc.1` (fragments kept) or `0.2.0` (fragments deleted).
        version: String,
        /// The release date, `YYYY-MM-DD`; today (UTC) by default.
        #[arg(long)]
        date: Option<String>,
        /// Don't run the version consistency test afterwards.
        #[arg(long)]
        no_verify: bool,
    },
    /// Run the local gate: formatting, lints, the tests, real Chrome, the book.
    Check {
        /// Print the commands instead of running them.
        #[arg(long)]
        list: bool,
        /// Also require that no fragment remains, as on a release tag.
        #[arg(long)]
        release: bool,
    },
    /// Set main's version to the next minor, leaving the fragments as they are.
    StartNext {
        /// The next minor's version, `0.3.0`.
        version: String,
    },
    /// Set the version to this one's nightly, `X.Y.Z-nightly.YYYYMMDD` (in a CI checkout, never committed).
    Nightly {
        /// The date, `YYYYMMDD`; today (UTC) by default.
        #[arg(long)]
        date: Option<String>,
    },
    /// Build the macOS archive on this machine (CI runs no macOS) and attach it to the existing GitHub release TAG, merged into its SHA256SUMS and index.
    AttachLocal {
        /// The release's tag (`v0.6.0`); the checkout must be at it.
        tag: String,
        /// A target to build and attach (repeatable); `aarch64-apple-darwin` by default.
        #[arg(long = "target")]
        targets: Vec<String>,
        /// Attach this already built archive instead of building (repeatable).
        #[arg(long = "archive")]
        archives: Vec<PathBuf>,
        /// The repository (`<owner>/<repo>`); `[workspace.metadata.project]`'s by default.
        #[arg(long)]
        repo: Option<String>,
        /// Skip the smoke test of each built `k`.
        #[arg(long)]
        no_smoke: bool,
    },
}

/// `docs` steps.
#[derive(Debug, Subcommand)]
pub enum DocsStep {
    /// Build one version of the site into OUT: `/0.N/` or `/next/`, plus versions.json, the redirects and the install scripts.
    Build {
        /// `next`, `latest` (the newest release, from its tag), a minor (`0.6`), a version or a tag (`v0.6.1`).
        #[arg(long)]
        version: String,
        /// The site directory.
        #[arg(long, default_value = "target/site")]
        out: PathBuf,
        /// Build this commit's book (a tag, `origin/main`) through a temporary worktree.
        #[arg(long)]
        from: Option<String>,
        /// The release tags, comma-separated; `git tag --list 'v*'` by default.
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
    },
}

/// `dist` steps.
#[derive(Debug, Subcommand)]
pub enum DistStep {
    /// Pack a built `k` into `k-<version>-<target>.tar.gz` (`.zip` for Windows) with the README and the licences (`LICENSE-MIT`, `LICENSE-APACHE`).
    Archive {
        /// The target triple.
        #[arg(long)]
        target: String,
        /// The built executable; `target/<target>/release/k` by default.
        #[arg(long)]
        binary: Option<PathBuf>,
        /// The directory the archive is written to.
        #[arg(long, default_value = "target/dist")]
        out: PathBuf,
    },
    /// Pack the built book in BOOK (the site's version directory, `target/site/0.6`) into `docs-<version>.tar.gz`.
    Docs {
        /// The book's directory, with its `index.html`.
        book: PathBuf,
        /// The version the archive is named for (a nightly's); Cargo.toml's by default.
        #[arg(long)]
        version: Option<String>,
        /// The directory the archive is written to.
        #[arg(long, default_value = "target/dist")]
        out: PathBuf,
    },
    /// Write SHA256SUMS over every archive (and book) in DIR.
    Checksums {
        /// The directory of archives.
        dir: PathBuf,
    },
    /// Add this version's release, read from DIR's archives and SHA256SUMS, to an index.json.
    Index {
        /// The directory of archives and SHA256SUMS.
        dir: PathBuf,
        /// The index to add the release to (created when missing).
        #[arg(long)]
        index: PathBuf,
        /// Where the archives are downloaded from: `<url>/v<version>/<archive>`.
        #[arg(long)]
        download: String,
        /// The release date, `YYYY-MM-DD`; today (UTC) by default.
        #[arg(long)]
        date: Option<String>,
    },
    /// Merge one-release indexes (each release's `index-entry.json`) into one index.json.
    MergeIndex {
        /// The directory of `*.json` indexes.
        dir: PathBuf,
        /// The merged index.
        #[arg(long)]
        out: PathBuf,
    },
    /// Smoke-test a `k`: `k new`, `k check`, `k test`, `k build`, then `dist/app serve` and `dist/app probe`.
    Smoke {
        /// The `k` to test.
        #[arg(long)]
        k: PathBuf,
        /// The port the app serves on; a free one by default.
        #[arg(long)]
        port: Option<u16>,
    },
}

/// `ci` steps.
#[derive(Debug, Subcommand)]
pub enum CiStep {
    /// Print `version=`, `minor=` and `prerelease=` from Cargo.toml (also written to `$GITHUB_OUTPUT`).
    Version,
    /// Check the version a release was started with: prepared on this checkout and not tagged yet (or, with `--tagged`, tagged); print `version=`, `tag=`, `minor=` and `prerelease=` (also written to `$GITHUB_OUTPUT`).
    CheckVersion {
        /// The version (`0.6.0`, `0.6.0-rc.1`; a leading `v` is accepted).
        version: String,
        /// Require its tag to exist instead (publishing a release made already).
        #[arg(long)]
        tagged: bool,
    },
    /// Write VERSION's section of CHANGELOG.md, the release notes, to OUT.
    Notes {
        /// The version (`0.6.0`) or its tag (`v0.6.0`).
        version: String,
        /// The notes file.
        #[arg(long)]
        out: PathBuf,
        /// Make relative links absolute below this URL: the repository at the tag
        /// (`https://github.com/<owner>/<repo>/blob/v0.6.0`), since a release page doesn't resolve them.
        #[arg(long)]
        links: Option<String>,
    },
    /// Print `changed=true` when a file below one of PATHS changed since BASE (also written to `$GITHUB_OUTPUT`).
    Changed {
        /// The commit the change is based on; empty or unknown counts as changed.
        #[arg(long)]
        base: String,
        /// Path prefixes (`docs/book/`, `scripts/install.`).
        #[arg(required = true)]
        paths: Vec<String>,
    },
}
