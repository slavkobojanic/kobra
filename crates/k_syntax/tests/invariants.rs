//! Property tests for the invariants every lexer and parser version must
//! keep, whatever its stage of completion:
//!
//! 1. Lexing and parsing never panic, on any input.
//! 2. The token stream is lossless: tokens are non-empty, contiguous, and
//!    their texts concatenate to the input.
//! 3. The syntax tree is lossless: its text equals the input, and its root
//!    is a `SourceFile`.
//! 4. Every diagnostic span lies within the input and on char boundaries.
//!
//! Inputs are drawn both from arbitrary Unicode strings and from a
//! "K-shaped" generator that glues together fragments of real K syntax, so
//! the tests exercise deep lexer modes (strings, interpolations, style
//! blocks) rather than only noise.

use k_diagnostics::FileId;
use k_syntax::{SyntaxKind, lex, parse};
use proptest::prelude::*;

/// Fragments of K syntax, including deliberately broken ones.
const FRAGMENTS: &[&str] = &[
    "component",
    "page",
    "service",
    "style",
    "view",
    "func",
    "let",
    "var",
    "@State",
    "@Prop",
    " ",
    "  ",
    "\n",
    "\r\n",
    "\r",
    "\t",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    ",",
    ":",
    ";",
    ".",
    "..<",
    "...",
    "?.",
    "??",
    "->",
    "=>",
    "==",
    "!=",
    "&&",
    "||",
    "!",
    "+",
    "-",
    "*",
    "/",
    "%",
    "=",
    "\"",
    "\"text\"",
    "\"{name}\"",
    "\"{{\"",
    "\\n",
    "\\u{1F600}",
    "\\{",
    "{expr}",
    "16px",
    "1.5rem",
    "50%",
    "200ms",
    "45deg",
    "#3B82F6",
    "#zz",
    "0x",
    "1e",
    "1_000",
    "10.minutes",
    "0.2.seconds",
    "// comment",
    "/* block */",
    "/* open",
    "/// doc",
    "count",
    "Counter",
    ".class(.muted)",
    "bind",
    "when",
    "if let",
    "match",
    "case .idle",
    "for item in items key item.id",
    "padding: .md;",
    "&:hover",
    "@media (maxWidth: 600px)",
    "é",
    "😀",
    "$",
    "`",
    "\u{0}",
    "\u{feff}",
];

fn k_shaped() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(FRAGMENTS), 0..64).prop_map(|parts| parts.concat())
}

fn check(text: &str) {
    let file = FileId::new(0);
    let lexed = lex(file, text);
    let mut rebuilt = String::with_capacity(text.len());
    let mut offset = 0u32;
    for index in 0..lexed.len() {
        let range = lexed.range(index).expect("range of an existing token");
        assert!(!range.is_empty(), "empty token {index} in {text:?}");
        assert_eq!(
            u32::from(range.start()),
            offset,
            "gap before token {index} in {text:?}"
        );
        offset = range.end().into();
        rebuilt.push_str(lexed.text(index).expect("text of an existing token"));
        assert!(lexed.kind(index).is_some_and(SyntaxKind::is_token));
    }
    assert_eq!(rebuilt, text, "token stream is not lossless");
    check_spans(text, lexed.diagnostics());

    let parse = parse(file, text);
    let root = parse.syntax();
    assert_eq!(root.kind(), SyntaxKind::SourceFile);
    assert_eq!(root.text().to_string(), text, "syntax tree is not lossless");
    assert!(parse.tree().is_some());
    check_spans(text, parse.diagnostics());
}

fn check_spans(text: &str, diagnostics: &[k_diagnostics::Diagnostic]) {
    for diagnostic in diagnostics {
        let start = usize::from(diagnostic.span.start());
        let end = usize::from(diagnostic.span.end());
        assert!(
            start <= end && end <= text.len(),
            "span out of bounds: {diagnostic:?}"
        );
        assert!(
            text.is_char_boundary(start) && text.is_char_boundary(end),
            "{diagnostic:?}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn arbitrary_text_is_lossless(text in any::<String>()) {
        check(&text);
    }

    #[test]
    fn k_shaped_text_is_lossless(text in k_shaped()) {
        check(&text);
    }
}

#[test]
fn edge_case_inputs_are_lossless() {
    for text in [
        "", "\n", "\r", "\r\n", "\u{feff}", "\"", "{", "}", "style {", "\"{", "/*", "\0", "é",
    ] {
        check(text);
    }
}
