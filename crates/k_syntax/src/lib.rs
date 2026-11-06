//! Syntax of the K language: tokens, lexer, parser and the lossless syntax
//! tree.
//!
//! This crate is the front end of the K toolchain. It turns source text into
//! a *lossless* concrete syntax tree: every byte of the input, including
//! whitespace and comments, is represented in the tree, so tools such as the
//! formatter and the language server can work on it directly and
//! `tree.text() == source` always holds.
//!
//! # Pipeline
//!
//! ```text
//! &str ──lexer::lex──▶ Lexed (flat tokens) ──parser──▶ Parse (green tree + diagnostics)
//!                                                         │
//!                                                         ▼
//!                                      SyntaxNode (red tree) ──ast──▶ typed accessors
//! ```
//!
//! * [`lexer`] produces a flat token stream with lexer diagnostics.
//! * [`parser`] builds the tree. It never fails: invalid input produces a
//!   complete tree with [`SyntaxKind::ErrorNode`]s and diagnostics.
//! * [`ast`] offers typed, zero-cost views of tree nodes.
//! * [`dump`] and [`sexpr`] implement the text formats used by the golden
//!   test corpus.
//!
//! The grammar is specified in `docs/compiler/grammar.md`; this crate's
//! architecture is described in `docs/compiler/architecture.md`, "Syntax".
//!
//! # Example
//!
//! ```
//! use k_diagnostics::FileId;
//! use k_syntax::parse;
//!
//! let source = "component Counter {}\n";
//! let parse = parse(FileId::new(0), source);
//! assert_eq!(parse.syntax().text().to_string(), source); // lossless
//! ```
//!
//! # Guarantees
//!
//! * **Lossless:** the token texts and the tree text always equal the input.
//! * **Total:** every input text, however malformed, produces a
//!   `SourceFile` tree and diagnostics; nothing panics. Nesting deeper
//!   than [`parser`]'s limit is reported (K0134) and skipped iteratively, so
//!   stack use is bounded.
//! * **Linear:** lexing and parsing run in time linear in the input,
//!   including error recovery; see `crates/k_syntax/benches/syntax.rs`.
//! * **Deterministic:** the same input always yields the same tokens, tree
//!   and diagnostics, sorted by position.

pub mod ast;
pub mod dump;
pub mod kind;
pub mod lexer;
pub mod parser;
pub mod sexpr;
mod syntax;

pub use kind::SyntaxKind;
pub use lexer::{Lexed, lex};
pub use parser::parse;
pub use syntax::{KLanguage, Parse, SyntaxElement, SyntaxNode, SyntaxNodeChildren, SyntaxToken};
