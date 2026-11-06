//! Rendering diagnostics for humans and machines.
//!
//! * [`human()`] produces `rustc`-style text with a source snippet, carets
//!   under the primary span and secondary labels, followed by notes, help
//!   lines and fixes.
//! * [`json()`] produces a single-line JSON object. `k check --format json`
//!   prints one such object per line (JSON Lines), which agents and editors
//!   consume directly.
//!
//! Both renderers are total: a span that points outside its file, or a file
//! id the [`SourceMap`](crate::SourceMap) does not know, degrades to a
//! location-less rendering instead of panicking.

mod human;
mod json;

pub use human::{HumanOptions, human};
pub use json::{json, json_string};
