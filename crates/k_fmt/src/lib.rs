//! The canonical formatter of the K language, the core of `k fmt`.
//!
//! [`format`] turns the text of one `.k` file into its canonical form. It
//! works on the lossless syntax tree of `k_syntax`, so it sees every comment
//! and never drops one, and it has no options: two K codebases formatted
//! with it look alike (`docs/design/01-principles.md`, "One obvious way").
//! The rules are specified in `docs/design/56-formatter.md`.
//!
//! # Stages
//!
//! ```text
//! text ──parse──▶ tree ──rewrite──▶ text' ──parse──▶ tree' ──layout──▶ text'' ──check──▶ Ok(text'')
//! ```
//!
//! 1. **Canonical rewrites** ([`rewrite`]) change tokens where the language
//!    has a redundant second spelling: `public` on a member, `private` on an
//!    `@Inject` field, a top-level `case _` arm, trailing commas, a missing
//!    `;` after the last style declaration, the order of the imports and the
//!    position of a service's `init()`.
//! 2. **Layout** changes only whitespace: indentation, spaces between
//!    tokens, blank lines and a few line breaks that the newline rules of
//!    `docs/compiler/grammar.md` §5 make safe to add or remove.
//! 3. **Check.** The result is parsed again and must have the same syntax
//!    tree as the rewritten text, whitespace aside and comments included. A
//!    formatter bug is therefore reported as [`Error::Internal`] instead of
//!    changing the program.
//!
//! A file with syntax errors is not formatted ([`Error::Syntax`]): without a
//! correct tree the formatter couldn't know which newlines are significant.
//!
//! # Example
//!
//! ```
//! let source = "component Counter {\n  @State var count: Int=0\n\n\n  view { button { \"+\" }.on(click: { count+=1 }) }\n}\n";
//! let formatted = k_fmt::format(source).unwrap();
//! assert_eq!(
//!     formatted,
//!     "component Counter {\n    @State var count: Int = 0\n\n    view { button { \"+\" }.on(click: { count += 1 }) }\n}\n"
//! );
//! // Formatting is idempotent.
//! assert_eq!(k_fmt::format(&formatted).unwrap(), formatted);
//! ```

#![warn(clippy::missing_docs_in_private_items)]

mod layout;
mod rewrite;
mod tree;

use std::fmt;

use k_diagnostics::{Diagnostic, FileId};
use k_syntax::Parse;

/// Why a file was not formatted.
#[derive(Clone, Debug)]
pub enum Error {
    /// The file has lexer or parser errors, which are returned. Nothing is
    /// formatted until they are fixed.
    Syntax(Vec<Diagnostic>),
    /// A formatter bug: a stage produced text whose syntax tree differs
    /// from its input's. The message says where; the input is left alone.
    Internal(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Syntax(diagnostics) => {
                let n = diagnostics.len();
                write!(
                    f,
                    "the file has {n} syntax error{} and can't be formatted",
                    if n == 1 { "" } else { "s" }
                )
            }
            Error::Internal(message) => write!(f, "internal formatter error: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Formats the text of one `.k` file into its canonical form.
///
/// The result always ends with exactly one newline (an empty file stays
/// empty), uses `\n` line endings and four-space indentation, and keeps
/// every comment. `format(&format(x)?)? == format(x)?` always holds.
///
/// # Errors
///
/// [`Error::Syntax`] if `text` has lexer or parser errors;
/// [`Error::Internal`] if a stage would change the syntax tree, which is a
/// formatter bug.
pub fn format(text: &str) -> Result<String, Error> {
    let (_, parse) = rewritten(text)?;
    let laid_out = layout::layout(&parse.syntax());
    let output = parse_clean(&laid_out, "the laid-out text")?;
    tree::same_tree(&parse.syntax(), &output.syntax())
        .map_err(|e| Error::Internal(format!("layout changed the syntax tree: {e}")))?;
    Ok(laid_out)
}

/// Returns `true` if `text` is already in canonical form, as the
/// formatting check of `k check` asks.
///
/// # Errors
///
/// As [`format`].
pub fn is_formatted(text: &str) -> Result<bool, Error> {
    Ok(format(text)? == text)
}

/// Applies only the canonical rewrites, the first stage of [`format`],
/// leaving the layout alone (apart from the lines a moved or sorted
/// declaration occupies). `format(x) == format(rewrite(x))`.
///
/// The rewrites are listed in `docs/design/56-formatter.md`, "Canonical
/// rewrites". Each is the machine-applicable fix of a redundant spelling
/// that the formatter can apply from the syntax tree alone.
///
/// # Errors
///
/// As [`format`].
pub fn rewrite(text: &str) -> Result<String, Error> {
    Ok(rewritten(text)?.0)
}

/// [`rewrite`], also returning the parse of the result.
fn rewritten(text: &str) -> Result<(String, Parse), Error> {
    let parse = k_syntax::parse(FileId::new(0), text);
    if !parse.diagnostics().is_empty() {
        return Err(Error::Syntax(parse.diagnostics().to_vec()));
    }
    let edits = rewrite::tokens(&parse.syntax());
    let (current, parse) = if edits.is_empty() {
        (text.to_owned(), parse)
    } else {
        let next = rewrite::apply(text, edits);
        let parse = parse_clean(&next, "the token rewrites")?;
        (next, parse)
    };
    let edits = rewrite::moves(&parse.syntax());
    if edits.is_empty() {
        return Ok((current, parse));
    }
    let moved = rewrite::apply(&current, edits);
    let after = parse_clean(&moved, "the moved declarations")?;
    tree::same_tokens_sorted(&parse.syntax(), &after.syntax())
        .map_err(|e| Error::Internal(format!("a move lost or added tokens: {e}")))?;
    Ok((moved, after))
}

/// Parses `text`, which a stage produced, and requires it to have no
/// syntax diagnostics.
fn parse_clean(text: &str, what: &str) -> Result<Parse, Error> {
    let parse = k_syntax::parse(FileId::new(0), text);
    match parse.diagnostics().first() {
        None => Ok(parse),
        Some(d) => Err(Error::Internal(format!(
            "{what} doesn't parse: {} at {}..{}: {}",
            d.code,
            u32::from(d.span.start()),
            u32::from(d.span.end()),
            d.message
        ))),
    }
}
