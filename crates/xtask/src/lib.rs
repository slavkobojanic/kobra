//! The K repository's release automation, run as `cargo xtask …`
//! (`docs/compiler/release.md`; the decisions are
//! `docs/design/48-releases-and-versioning.md`, "The release process").
//!
//! * [`fragment`]: the changelog fragments in `changes/`, parsed and
//!   checked.
//! * [`changelog`]: compiling fragments into a `CHANGELOG.md` section, the
//!   book's changelog page and a section of the upgrade guide.
//! * [`bump`]: setting the version everywhere it is written.
//! * [`release`]: `release prepare`, `release start-next` and the local
//!   gate of `release check`.
//! * [`gate`]: `docs-gate`, each user-visible fragment's book pages in the
//!   diff.
//!
//! Nothing here publishes anything or reaches the network.

pub mod bump;
pub mod changelog;
pub mod fragment;
pub mod gate;
pub mod release;
pub mod version;

pub use version::Version;
