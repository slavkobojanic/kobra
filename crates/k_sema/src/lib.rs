//! Semantic analysis of K programs.
//!
//! `k_sema` turns the syntax trees of a project into a checked program: the
//! module graph, name resolution, the high-level IR (HIR), types, effects,
//! control flow and the framework rules (components, views, styles,
//! dependency injection, routing, protocols, forms, persistence, tests).
//! The design is specified in `docs/compiler/sema.md`; the language in
//! `docs/design`.
//!
//! # Entry points
//!
//! * [`AnalysisHost`] holds a project's inputs (every file, by
//!   [`ProjectPath`]) and applies [`Change`]s as revisions.
//! * [`Analysis`] is an immutable snapshot of one revision. It reports
//!   [`diagnostics`](Analysis::diagnostics) and answers position queries
//!   for tests and tooling: [`type_at`](Analysis::type_at),
//!   [`resolve_at`](Analysis::resolve_at) and [`side_of`](Analysis::side_of).
//! * [`stdlib`] is the standard library as the checker sees it: embedded K
//!   sources plus the element, CSS and annotation tables.
//!
//! ```
//! use k_sema::{AnalysisHost, Options, ProjectPath};
//!
//! let host = AnalysisHost::with_files(
//!     Options::new(),
//!     [(
//!         ProjectPath::new("src/home/Home.k").unwrap(),
//!         "public page Home {\n    view {\n        text { \"Hello\" }\n    }\n}\n".to_owned(),
//!     )],
//! );
//! let analysis = host.analysis();
//! assert!(analysis.diagnostics().unwrap().is_empty());
//! ```
//!
//! # Status
//!
//! The public API is final. The analysis parses every K source file in
//! parallel and memoizes parses across revisions; the semantic pass runs
//! once per revision: modules, imports, visibility, names, doc links,
//! types, inference, effects and control flow (round 1a), and the framework
//! layer (round 1b: components, views, styles, themes, reactivity, DI,
//! layering, the server/client boundary, routing, protocols, `@Load`,
//! forms, configs, persistence and tests). It answers
//! [`type_at`](Analysis::type_at), [`resolve_at`](Analysis::resolve_at) and
//! [`side_of`](Analysis::side_of). What each stage checks, and which checks
//! are v0 and which deferred, is in `docs/compiler/sema.md`.
//!
//! # Guarantees
//!
//! * **Never panics** on any input: user errors are diagnostics in the
//!   K1000–K8999 ranges of `docs/compiler/diagnostics.md`.
//! * **Deterministic:** the same inputs give the same diagnostics, sorted
//!   by file, span and code, whatever the number of threads.
//! * **Incremental:** results are memoized per revision and kept for
//!   unchanged inputs.

#![warn(clippy::missing_docs_in_private_items)]

mod app;
mod body;
mod check;
mod cx;
mod di;
mod doclinks;
mod framework;
mod fx;
mod host;
mod locks;
mod lookup;
mod pattern;
mod program;
mod project;
mod refs;
mod regex;
mod routing;
mod rpc;
mod rpclock;
pub mod stdlib;
mod tooling;
mod ty;
pub mod view;
mod warm;
pub mod workspace;

pub use host::{Analysis, AnalysisHost, Change, FileOrigin, Options};
pub use lookup::{Cancelled, Resolution, Side};
pub use project::{DEPENDENCY_DIR, FileKind, LockfileKind, ModulePath, PathError, ProjectPath};
pub use workspace::Workspace;
