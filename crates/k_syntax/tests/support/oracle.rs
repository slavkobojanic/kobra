//! The syntax oracle: every invariant that lexing and parsing one source text
//! must satisfy, checked in one place.
//!
//! This file is shared, verbatim, by the stable property tests
//! (`crates/k_syntax/tests/properties.rs`) and by the nightly fuzz targets
//! (`fuzz/src/lib.rs` includes it with `#[path]`), so a property violated
//! under fuzzing is exactly the property the stable suite checks. Every
//! check panics with a message naming the broken invariant; fuzzers treat
//! the panic as a crash and proptest shrinks the input.
//!
//! The invariants are specified in `docs/compiler/testing.md`, "Property
//! tests", and in the "Guarantees" section of `crates/k_syntax/src/lib.rs`:
//!
//! | Check | Invariant |
//! |---|---|
//! | [`check_lexed`] | tokens are non-empty, contiguous, cover the input and are token kinds |
//! | [`check_diagnostics`] | spans and fix edits lie in the file, on char boundaries; codes are registered; sorted |
//! | [`check_tree`] | the tree text equals the input; its tokens are the lexer's tokens |
//! | [`check_relex`] | re-lexing the tree's token texts reproduces the token stream |
//! | [`check_dumps`] | the `.tokens` and `.ast` dumps read back to the same values |
//! | [`check_deterministic`] | lexing and parsing twice give identical results |
//! | [`check_error_bound`] | the number of diagnostics is linear in the number of tokens |
//! | [`check_no_duplicates`] | no diagnostic (code and span) is reported twice; not part of [`check_all`] yet, see `docs/compiler/testing.md` |

#![allow(dead_code, clippy::panic, clippy::unwrap_used)]

use k_diagnostics::{Diagnostic, FileId};
use k_syntax::{Lexed, Parse, SyntaxElement, SyntaxKind, dump, lex, parse, sexpr};

/// The file id every oracle check lexes and parses under.
pub const FILE: FileId = FileId::new(0);

/// The most diagnostics a source with `tokens` non-trivia tokens may
/// produce. Each token can be the site of at most a couple of problems (a
/// lexer error, an unexpected-token error, an unclosed delimiter), so a
/// cascade of errors from one mistake shows up as a violation.
#[must_use]
pub const fn max_diagnostics(tokens: usize) -> usize {
    2 * tokens + 2
}

/// Runs every check on `text` and returns the parse for further checks.
///
/// This is what the fuzz targets and the property tests call; it parses the
/// text twice (for [`check_deterministic`]).
pub fn check_all(text: &str) -> Parse {
    let lexed = lex(FILE, text);
    check_lexed(text, &lexed);
    check_diagnostics(text, lexed.diagnostics(), "lexer");
    let parsed = parse(FILE, text);
    check_tree(text, &lexed, &parsed);
    check_diagnostics(text, parsed.diagnostics(), "parser");
    check_no_duplicates(&parsed);
    check_lexer_diagnostics_are_reported(&lexed, &parsed);
    check_relex(&lexed, &parsed);
    check_dumps(&lexed, &parsed);
    check_error_bound(&lexed, &parsed);
    check_deterministic(text, &lexed, &parsed);
    parsed
}

/// Checks the cheap invariants only (no dumps, no second parse); used by
/// the throughput-oriented fuzz targets.
pub fn check_fast(text: &str) -> Parse {
    let lexed = lex(FILE, text);
    check_lexed(text, &lexed);
    check_diagnostics(text, lexed.diagnostics(), "lexer");
    let parsed = parse(FILE, text);
    if parsed.syntax().kind() != SyntaxKind::SourceFile {
        panic!("root is {:?}, not SourceFile", parsed.syntax().kind());
    }
    check_diagnostics(text, parsed.diagnostics(), "parser");
    check_error_bound(&lexed, &parsed);
    parsed
}

/// Checks the token stream of `text`: every token is a non-empty token
/// kind, tokens are contiguous from offset 0, and their texts concatenate
/// to the input.
pub fn check_lexed(text: &str, lexed: &Lexed<'_>) {
    assert_eq!(lexed.source(), text, "Lexed::source is not the input");
    assert_eq!(
        lexed.starts().len(),
        lexed.len() + 1,
        "starts() must have one sentinel entry"
    );
    assert_eq!(
        lexed.is_empty(),
        text.is_empty(),
        "a stream is empty exactly when the input is"
    );
    let mut offset = 0usize;
    for index in 0..lexed.len() {
        let kind = lexed.kind(index).unwrap();
        assert!(kind.is_token(), "token {index} has node kind {kind:?}");
        let range = lexed.range(index).unwrap();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        assert_eq!(start, offset, "gap or overlap before token {index}");
        assert!(end > start, "token {index} ({kind:?}) is empty");
        assert!(
            text.is_char_boundary(start) && text.is_char_boundary(end),
            "token {index} ({kind:?}) splits a character"
        );
        let token_text = lexed.text(index).unwrap();
        assert_eq!(token_text, &text[start..end], "text of token {index}");
        if let Some(fixed) = kind.fixed_text() {
            // Keywords and punctuation always have their spelling; contextual
            // keywords are `Ident`s in the stream, so never reach this.
            assert_eq!(token_text, fixed, "token {index} has kind {kind:?}");
        }
        offset = end;
    }
    assert_eq!(offset, text.len(), "tokens do not cover the input");
    assert_eq!(lexed.kind(lexed.len()), None, "kind() past the end");
    assert_eq!(lexed.range(lexed.len()), None, "range() past the end");
    let rebuilt: String = lexed.iter().map(|(_, t)| t).collect();
    assert_eq!(rebuilt, text, "token stream is not lossless");
}

/// Checks a list of diagnostics produced for `text` by `stage`.
pub fn check_diagnostics(text: &str, diagnostics: &[Diagnostic], stage: &str) {
    let in_file = |what: &str, d: &Diagnostic, span: k_diagnostics::Span| {
        let (start, end) = (usize::from(span.start()), usize::from(span.end()));
        assert_eq!(span.file, FILE, "{stage}: {what} in another file: {d:?}");
        assert!(
            start <= end && end <= text.len(),
            "{stage}: {what} out of bounds ({start}..{end} in {} bytes): {d:?}",
            text.len()
        );
        assert!(
            text.is_char_boundary(start) && text.is_char_boundary(end),
            "{stage}: {what} not on char boundaries: {d:?}"
        );
    };
    for d in diagnostics {
        in_file("primary span", d, d.span);
        assert!(d.code.info().is_some(), "{stage}: unregistered code: {d:?}");
        assert!(
            !d.message.is_empty() && !d.message.contains('\n'),
            "{stage}: message must be one non-empty line: {d:?}"
        );
        for label in &d.labels {
            in_file("label", d, label.span);
        }
        for fix in &d.fixes {
            assert!(
                !fix.message.is_empty(),
                "{stage}: fix without message: {d:?}"
            );
            assert!(!fix.edits.is_empty(), "{stage}: fix without edits: {d:?}");
            for edit in &fix.edits {
                in_file("fix edit", d, edit.span);
            }
            let mut edits: Vec<_> = fix.edits.iter().map(|e| e.span.range).collect();
            edits.sort_by_key(|r| (r.start(), r.end()));
            assert!(
                edits.windows(2).all(|w| w[0].end() <= w[1].start()),
                "{stage}: overlapping edits in one fix: {d:?}"
            );
        }
    }
    assert!(
        diagnostics
            .windows(2)
            .all(|w| (w[0].span.start(), w[0].span.end()) <= (w[1].span.start(), w[1].span.end())),
        "{stage}: diagnostics are not sorted by position"
    );
}

/// Returns `true` if the tree may give token kind `tree` to a token the
/// lexer produced as `lexed` with text `text`: either the kinds are equal,
/// or the parser remapped a contextual keyword (an `Ident` in the stream) to
/// its keyword kind (`docs/compiler/grammar.md` §6).
#[must_use]
pub fn compatible_kinds(lexed: SyntaxKind, tree: SyntaxKind, text: &str) -> bool {
    lexed == tree
        || (lexed == SyntaxKind::Ident
            && tree.is_contextual_keyword()
            && SyntaxKind::from_contextual_keyword(text) == Some(tree))
}

/// Checks the tree of `text`: the root is a `SourceFile` whose text is the
/// input, nodes have node kinds, and the tree's tokens are exactly the
/// lexer's tokens (same ranges, same texts, compatible kinds).
pub fn check_tree(text: &str, lexed: &Lexed<'_>, parsed: &Parse) {
    let root = parsed.syntax();
    assert_eq!(root.kind(), SyntaxKind::SourceFile, "root kind");
    assert!(parsed.tree().is_some(), "typed root is missing");
    assert_eq!(
        usize::from(root.text_range().start()),
        0,
        "root does not start at 0"
    );
    assert_eq!(
        usize::from(root.text_range().end()),
        text.len(),
        "root does not end at the end of the input"
    );
    assert_eq!(root.text().to_string(), text, "syntax tree is not lossless");

    let mut index = 0usize;
    for element in root.descendants_with_tokens() {
        match element {
            SyntaxElement::Node(node) => {
                assert!(
                    node.kind().is_node(),
                    "node at {:?} has token kind {:?}",
                    node.text_range(),
                    node.kind()
                );
            }
            SyntaxElement::Token(token) => {
                let kind = lexed
                    .kind(index)
                    .unwrap_or_else(|| panic!("tree has more tokens than the lexer ({index}+)"));
                let range = lexed.range(index).unwrap();
                assert_eq!(
                    token.text_range(),
                    range,
                    "tree token {index} ({:?}) is not lexer token {index} ({kind:?})",
                    token.kind()
                );
                assert!(
                    compatible_kinds(kind, token.kind(), token.text()),
                    "tree token {index} {:?} {:?} was lexed as {kind:?}",
                    token.kind(),
                    token.text()
                );
                index += 1;
            }
        }
    }
    assert_eq!(index, lexed.len(), "tree has fewer tokens than the lexer");
}

/// Checks that every lexer diagnostic is also reported by the parse.
pub fn check_lexer_diagnostics_are_reported(lexed: &Lexed<'_>, parsed: &Parse) {
    for d in lexed.diagnostics() {
        assert!(
            parsed.diagnostics().contains(d),
            "lexer diagnostic missing from the parse: {d:?}"
        );
    }
}

/// Checks that re-lexing the concatenated texts of the tree's tokens
/// reproduces the original token stream, kind for kind, and that every
/// tree token's kind is compatible with the re-lexed one.
pub fn check_relex(lexed: &Lexed<'_>, parsed: &Parse) {
    let root = parsed.syntax();
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .collect();
    let text: String = tokens.iter().map(|t| t.text()).collect();
    let relexed = lex(FILE, &text);
    assert_eq!(relexed.kinds(), lexed.kinds(), "re-lexed kinds differ");
    assert_eq!(relexed.starts(), lexed.starts(), "re-lexed offsets differ");
    assert_eq!(
        relexed.diagnostics(),
        lexed.diagnostics(),
        "re-lexed diagnostics differ"
    );
    for (index, token) in tokens.iter().enumerate() {
        let kind = relexed.kind(index).unwrap();
        assert!(
            compatible_kinds(kind, token.kind(), token.text()),
            "re-lexed token {index} is {kind:?}, tree has {:?}",
            token.kind()
        );
    }
}

/// Checks the golden-test text formats: the `.tokens` dump parses back to
/// the token lines it was written from, and the `.ast` dump parses back to
/// the S-expression it was printed from.
pub fn check_dumps(lexed: &Lexed<'_>, parsed: &Parse) {
    let tokens = dump::dump_tokens(lexed);
    let read = dump::parse_tokens_dump(&tokens)
        .unwrap_or_else(|e| panic!("token dump does not read back: {e:?}\n{tokens}"));
    assert_eq!(read, dump::token_lines(lexed), "token dump round trip");

    let tree = dump::tree_sexpr(&parsed.syntax());
    let printed = dump::dump_tree(&parsed.syntax());
    let read = sexpr::parse(&printed)
        .unwrap_or_else(|e| panic!("tree dump does not read back: {e:?}\n{printed}"));
    assert_eq!(read, vec![tree], "tree dump round trip");
}

/// Checks that the number of diagnostics is bounded by
/// [`max_diagnostics`] of the number of non-trivia tokens.
pub fn check_error_bound(lexed: &Lexed<'_>, parsed: &Parse) {
    let tokens = lexed.kinds().iter().filter(|k| !k.is_trivia()).count();
    let count = parsed.diagnostics().len();
    assert!(
        count <= max_diagnostics(tokens),
        "{count} diagnostics for {tokens} non-trivia tokens (bound {})",
        max_diagnostics(tokens)
    );
}

/// Returns the first diagnostic that is reported twice (same code, same
/// span), if any.
#[must_use]
pub fn duplicate_diagnostic(parsed: &Parse) -> Option<&Diagnostic> {
    parsed
        .diagnostics()
        .windows(2)
        .find(|pair| pair[0].code == pair[1].code && pair[0].span == pair[1].span)
        .map(|pair| &pair[0])
}

/// Checks that no diagnostic is reported twice.
pub fn check_no_duplicates(parsed: &Parse) {
    if let Some(d) = duplicate_diagnostic(parsed) {
        panic!("diagnostic reported twice: {d:?}");
    }
}

/// Checks that lexing and parsing `text` again yields an identical token
/// stream, an identical green tree and identical diagnostics.
pub fn check_deterministic(text: &str, lexed: &Lexed<'_>, parsed: &Parse) {
    let again = lex(FILE, text);
    assert_eq!(again.kinds(), lexed.kinds(), "lexing is not deterministic");
    assert_eq!(
        again.starts(),
        lexed.starts(),
        "lexing is not deterministic"
    );
    assert_eq!(
        again.diagnostics(),
        lexed.diagnostics(),
        "lexer diagnostics are not deterministic"
    );
    let reparsed = parse(FILE, text);
    assert_eq!(
        reparsed.green(),
        parsed.green(),
        "parsing is not deterministic"
    );
    assert_eq!(
        reparsed.diagnostics(),
        parsed.diagnostics(),
        "parser diagnostics are not deterministic"
    );
}
