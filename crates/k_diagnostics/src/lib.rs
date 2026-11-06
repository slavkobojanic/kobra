//! Structured diagnostics for the K toolchain.
//!
//! Every problem the toolchain reports about user input, from an unexpected
//! character in the lexer to a layering violation in semantic analysis, is a
//! [`Diagnostic`]. The compiler never panics on user input; instead it records
//! diagnostics and keeps going, so that one run reports as many problems as
//! possible.
//!
//! # Anatomy of a diagnostic
//!
//! A diagnostic has:
//!
//! * a stable [`Code`] such as `K0002`, registered in [`codes::REGISTRY`];
//! * a [`Severity`] ([`Severity::Error`] or [`Severity::Warning`]);
//! * a one-line, human-readable message;
//! * a primary [`Span`] pointing at the offending source text;
//! * zero or more secondary [`Label`]s, notes and help lines;
//! * zero or more [`Fix`]es. A fix is a set of [`TextEdit`]s plus an
//!   [`Applicability`]; a [`Applicability::MachineApplicable`] fix can be
//!   applied by an agent or an editor without human review.
//!
//! # Rendering
//!
//! Diagnostics are rendered for humans by [`render::human`] (in the style of
//! `rustc`, with source snippets and carets) and for machines by
//! [`render::json`] (one JSON object per diagnostic, suitable for JSON Lines
//! output). Both renderers read source text through a [`SourceMap`].
//!
//! # Positions
//!
//! Offsets are UTF-8 byte offsets ([`TextSize`]). Human-facing positions
//! ([`LineCol`]) are 1-based lines and 1-based columns counted in Unicode
//! scalar values, computed by [`LineIndex`].
//!
//! # Example
//!
//! ```
//! use k_diagnostics::{codes, Diagnostic, FileId, SourceMap, Span, TextRange, render};
//!
//! let mut sources = SourceMap::new();
//! let file = sources.add("src/Counter.k", "let a = \"abc\n");
//! let span = Span::new(file, TextRange::new(8.into(), 9.into()));
//!
//! let diagnostic = Diagnostic::new(codes::UNTERMINATED_STRING, span, "unterminated string literal")
//!     .with_help("close the string with `\"` before the end of the line");
//!
//! let text = render::human(&diagnostic, &sources, &render::HumanOptions::plain());
//! assert!(text.starts_with("error[K0002]: unterminated string literal"));
//! # let _ = FileId::new(0);
//! ```

pub mod codes;
mod diagnostic;
mod line_index;
pub mod render;
mod source_map;
mod span;

pub use codes::{Code, CodeInfo};
pub use diagnostic::{Applicability, Diagnostic, Fix, Label, Severity, TextEdit};
pub use line_index::{LineCol, LineIndex};
pub use source_map::{SourceFile, SourceMap};
pub use span::{FileId, Span};
pub use text_size::{TextRange, TextSize};
