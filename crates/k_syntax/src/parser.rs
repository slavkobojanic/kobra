//! The K parser: token stream to lossless syntax tree.
//!
//! [`parse`] lexes and parses a source file into a [`Parse`]: a complete,
//! lossless rowan tree plus diagnostics. The parser is a hand-written
//! recursive-descent parser with a Pratt loop for expressions, following the
//! grammar in `docs/compiler/grammar.md` exactly. It never fails and never
//! panics: on unexpected input it reports a diagnostic, wraps skipped tokens
//! in a [`SyntaxKind::ErrorNode`] and resynchronizes at the next statement,
//! member or declaration boundary.
//!
//! # Design
//!
//! The design is described in `docs/compiler/architecture.md`, "Syntax":
//!
//! 1. The parser runs over a *trivia-free view* of the lexer's tokens. Each
//!    token carries two flags: whether a line terminator precedes it (which
//!    implements the newline rules of grammar.md §5) and whether any trivia
//!    precedes it (used where the grammar forbids trivia, such as import
//!    paths and compound selectors).
//! 2. Grammar functions record *events* (`Start`, `Token`, `Finish`); see the
//!    engine module. Left-recursive constructs wrap completed nodes with
//!    `precede`.
//! 3. The tree sink replays the events against the full token stream,
//!    re-inserting trivia and attaching doc comments to the following item
//!    or member.
//!
//! Contextual keywords are remapped while parsing (`page` becomes
//! [`SyntaxKind::PageKw`] in a declaration header), so the tree carries the
//! keyword kinds.
//!
//! # Recovery
//!
//! Every list parser (items, members, statements, arguments, parameters,
//! style items) knows which tokens start its next element and which close
//! it. Unexpected tokens are wrapped in an `ErrorNode` up to the next such
//! token, skipping balanced bracket groups as a whole. Missing pieces are
//! simply absent from the tree; the diagnostic records them. A parse always
//! consumes every token and always yields a `SourceFile`.
//!
//! # Limits
//!
//! Recursion is bounded: constructs nested more than 400 levels deep are
//! reported as K0134 and skipped iteratively, so the parser's stack use is
//! bounded for every input.

mod engine;
mod expressions;
mod items;
mod patterns;
mod sink;
mod statements;
mod styles;
mod types;

use k_diagnostics::FileId;

use crate::lexer::{self, Lexed};
use crate::syntax::Parse;

#[cfg(doc)]
use crate::kind::SyntaxKind;

/// Lexes and parses `text`, which belongs to `file`.
///
/// The diagnostics of the returned [`Parse`] include the lexer's and are
/// sorted by position (start offset, end offset, then code). No two of them
/// have the same code and span.
///
/// ```
/// use k_diagnostics::FileId;
/// use k_syntax::{dump, parse};
///
/// let parse = parse(FileId::new(0), "let x = 1 + 2\n");
/// assert!(parse.diagnostics().is_empty());
/// assert_eq!(
///     dump::dump_tree(&parse.syntax()),
///     "(SourceFile (FieldDecl let x = (BinaryExpr (Literal 1) + (Literal 2))))\n"
/// );
/// ```
#[must_use]
pub fn parse(file: FileId, text: &str) -> Parse {
    let lexed = lexer::lex(file, text);
    parse_lexed(&lexed, file)
}

/// Parses an already lexed token stream.
///
/// Parser diagnostics are reported in `FileId(0)`; use [`parse`] (or
/// [`parse_lexed`]) to choose the file.
#[must_use]
pub fn parse_tokens(lexed: &Lexed<'_>) -> Parse {
    parse_lexed(lexed, FileId::new(0))
}

/// Parses an already lexed token stream whose diagnostics belong to `file`.
///
/// The lexer diagnostics of `lexed` are included in the result.
#[must_use]
pub fn parse_lexed(lexed: &Lexed<'_>, file: FileId) -> Parse {
    let mut parser = engine::Parser::new(lexed, file);
    items::source_file(&mut parser);
    let (events, parser_diagnostics) = parser.finish();
    let (green, depth) = sink::build(lexed, events);
    let mut diagnostics = Vec::with_capacity(lexed.diagnostics().len() + parser_diagnostics.len());
    diagnostics.extend_from_slice(lexed.diagnostics());
    diagnostics.extend(parser_diagnostics);
    diagnostics.sort_by_key(|d| (d.span.start(), d.span.end(), d.code));
    // One mistake gets one diagnostic: of several with the same code and
    // span (an inner and an enclosing construct describing the same gap,
    // possibly with other diagnostics reported in between), the first
    // reported is kept. The sort is stable, so that is the first of each run.
    diagnostics.dedup_by(|later, earlier| later.code == earlier.code && later.span == earlier.span);
    Parse::with_depth(green, diagnostics, depth)
}
