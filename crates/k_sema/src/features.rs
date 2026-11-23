//! The feature table: every named feature that isn't simply stable.
//!
//! Each row has an id (`k-doc`), a [`Status`], the version it reached that
//! status in, a one-line summary and the design topic that specifies it.
//! The book's `> **Planned** (id).` and `> **Preview** (id).` notes name
//! these ids, and the book test (`crates/k_sema/tests/book.rs`) fails when
//! a note's id is missing here or its status disagrees, so the docs and the
//! compiler share one list of what is built
//! (`docs/design/48-releases-and-versioning.md`, "Stability levels").
//!
//! A `preview` row is built and opt-in: the standard library marks its
//! declarations `@Preview("id")`, and a project lists the id in
//! `[package] preview` of its `k.toml` to use them (`crate::stability`
//! checks both). A feature that ships stable is removed from the table
//! together with its notes, or kept as a `stable` row while projects may
//! still list it (listing it is then the warning `K8110`); one that ships
//! as a preview changes status here.
//!
//! ```
//! use k_sema::features::{self, Status};
//!
//! let doc = features::lookup("k-doc").unwrap();
//! assert_eq!(doc.status, Status::Planned);
//! assert_eq!(features::lookup("workers").unwrap().status, Status::Preview);
//! assert!(features::lookup("no-such-feature").is_none());
//! ```

/// How far a feature is built.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Designed, not built. The book marks it `> **Planned** (id).`
    Planned,
    /// Built, opt-in per project, and free to change in any minor. The book
    /// marks it `> **Preview** (id).`
    Preview,
    /// Built and covered by the compatibility promise. A stable feature
    /// stays in the table only while something still refers to its id.
    Stable,
}

impl Status {
    /// The word the book's notes use: `Planned`, `Preview` or `Stable`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Status::Planned => "Planned",
            Status::Preview => "Preview",
            Status::Stable => "Stable",
        }
    }
}

/// One row of the feature table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Feature {
    /// The id: lowercase words joined with `-`, such as `k-doc`.
    pub id: &'static str,
    /// How far it is built.
    pub status: Status,
    /// The toolchain version it reached [`Feature::status`] in; `None` for
    /// a planned feature.
    pub since: Option<&'static str>,
    /// What it is, in a short phrase of markdown (the design-status page's
    /// table shows it).
    pub summary: &'static str,
    /// The design topic below `docs/design/` that specifies it.
    pub design: &'static str,
}

/// Builds a planned [`Feature`]; keeps [`FEATURES`] to one line per row.
const fn planned(id: &'static str, summary: &'static str, design: &'static str) -> Feature {
    Feature {
        id,
        status: Status::Planned,
        since: None,
        summary,
        design,
    }
}

/// Builds a preview [`Feature`], built in toolchain `since`.
const fn preview(
    id: &'static str,
    since: &'static str,
    summary: &'static str,
    design: &'static str,
) -> Feature {
    Feature {
        id,
        status: Status::Preview,
        since: Some(since),
        summary,
        design,
    }
}

/// Every feature that isn't simply stable, in the order the design-status
/// page lists them.
pub const FEATURES: &[Feature] = &[
    preview(
        "workers",
        "0.1",
        "`@Worker` services: page code that runs in a Web Worker",
        "55-workers.md",
    ),
    preview(
        "view-previews",
        "0.1",
        "`preview` declarations: a view rendered in a chosen state, checked by `k check`",
        "57-previews.md",
    ),
    planned(
        "k-new-library",
        "`k new --library`, the library package skeleton",
        "24-projects.md",
    ),
    planned(
        "modifiers",
        "Modifiers (`.tooltip()`, `.animate()`, `.track()`), applied in the order written",
        "04-components.md",
    ),
    planned(
        "effect-lints",
        "Warnings for an effect that reads no reactive value, and for state read after an `await`",
        "03-reactivity.md",
    ),
    planned(
        "a11y-lints",
        "Warnings for a second landmark of a kind without a name, and a `.label` missing the visible text",
        "05-views.md",
    ),
    planned(
        "error-payload-lint",
        "A warning when a wire error carries a server-only type or a third party's response body",
        "28-rpc-wire-format.md",
    ),
    planned(
        "app-version",
        "`dist/app` reporting `[package] version` in its logs and metrics",
        "24-projects.md",
    ),
    planned(
        "safe-method-lint",
        "A warning when a `GET` or `HEAD` handler that uses the session signs in, signs out or writes",
        "26-auth-and-sessions.md",
    ),
    planned(
        "password-log-lint",
        "A warning when a field named `password`, or a `Secret`, is logged",
        "33-crypto.md",
    ),
    planned(
        "job-run-check",
        "Rejecting a call to a job's `run` outside the worker and tests",
        "30-background-jobs.md",
    ),
    planned(
        "observe-metrics",
        "Metrics and tracing in `k.observe` (OpenTelemetry), such as a dead-job count",
        "20-standard-library.md",
    ),
    planned(
        "hmr",
        "State-preserving hot module replacement in `k dev`",
        "13-tooling.md",
    ),
    planned(
        "fmt-check",
        "The formatting check of `k check` (`k fmt --check` is built)",
        "56-formatter.md",
    ),
    planned(
        "doc-blocks",
        "Type-checking the `k` blocks of `///` comments in `k check`",
        "39-doc-comments.md",
    ),
    planned(
        "k-doc",
        "`k doc`, rendering documentation from `///` comments",
        "39-doc-comments.md",
    ),
    planned(
        "doc-coverage",
        "Warnings for undocumented package API",
        "39-doc-comments.md",
    ),
    planned("k-explain", "`k explain`", "13-tooling.md"),
    planned(
        "custom-id-params",
        "Custom id types (a `ProductId` struct) as page parameters",
        "12-routing.md",
    ),
    planned(
        "iso-4217",
        "Every ISO 4217 currency (five are built)",
        "34-money.md",
    ),
    planned(
        "more-charts",
        "Bar, area, pie and scatter charts",
        "37-charts.md",
    ),
    planned(
        "ui-widgets",
        "The accessible widgets of `k.ui`",
        "20-standard-library.md",
    ),
    planned(
        "k-text",
        "Regular expressions, slugs and Markdown rendering in `k.text`",
        "20-standard-library.md",
    ),
    planned(
        "k-i18n",
        "`k.i18n`: localized strings and plurals",
        "20-standard-library.md",
    ),
    planned(
        "k-collections",
        "`k.collections`: `Deque`, `PriorityQueue`, `OrderedMap`, `LruCache`",
        "20-standard-library.md",
    ),
    planned(
        "k-animation",
        "`k.animation`: transitions and springs",
        "20-standard-library.md",
    ),
    planned(
        "k-graphics",
        "`k.graphics`: Canvas 2D and SVG drawing",
        "20-standard-library.md",
    ),
    planned(
        "protobuf",
        "The binary Protobuf encoding, `k proto export` and `import proto`",
        "28-rpc-wire-format.md",
    ),
    planned(
        "protocol-versions",
        "A protocol version bump on a deliberate break (`CartProtocol.v2`)",
        "11-protocols-and-rpc.md",
    ),
    planned(
        "packages",
        "Git packages, `k add`, `k remove` and their entries in `k.lock`",
        "18-packages.md",
    ),
    planned(
        "stream-tab-sharing",
        "One connection for identical streams opened in several tabs",
        "29-server-streaming.md",
    ),
    planned(
        "in-process-services",
        "Binding a workspace member's own implementation in-process in `.test`",
        "40-workspaces-and-services.md",
    ),
    planned(
        "service-streaming",
        "Streaming rpcs between services",
        "40-workspaces-and-services.md",
    ),
    planned(
        "toolchain-image",
        "A published toolchain container image",
        "38-deployment.md",
    ),
    planned(
        "npm-interop",
        "Typed npm interop, with bindings from `.d.ts` files",
        "17-javascript-interop.md",
    ),
];

/// Returns the row with id `id`.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Feature> {
    lookup_in(FEATURES, id)
}

/// Returns the row of `table` with id `id`: [`lookup`] for another table,
/// which the tests of a status that no real row has yet use.
#[must_use]
pub fn lookup_in<'t>(table: &'t [Feature], id: &str) -> Option<&'t Feature> {
    table.iter().find(|f| f.id == id)
}

/// The table's own invariants.
#[cfg(test)]
mod tests {
    use super::*;

    /// Ids are unique and spelt as the book's notes spell them.
    #[test]
    fn ids_are_unique_lowercase_words() {
        let mut seen = std::collections::HashSet::new();
        for feature in FEATURES {
            assert!(seen.insert(feature.id), "duplicate id {}", feature.id);
            assert!(
                !feature.id.is_empty()
                    && !feature.id.starts_with('-')
                    && !feature.id.ends_with('-')
                    && feature
                        .id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "id `{}` must be lowercase words joined with `-`",
                feature.id
            );
        }
    }

    /// `since` is set exactly on built features.
    #[test]
    fn planned_rows_have_no_version_and_others_do() {
        for feature in FEATURES {
            assert_eq!(
                feature.since.is_none(),
                feature.status == Status::Planned,
                "{}: `since` is the version a built feature reached its status in",
                feature.id
            );
        }
    }

    /// Every row's design topic exists.
    #[test]
    fn every_row_names_an_existing_design_topic() {
        let design = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design");
        for feature in FEATURES {
            assert!(
                design.join(feature.design).is_file(),
                "{}: no design topic `{}`",
                feature.id,
                feature.design
            );
        }
    }
}
