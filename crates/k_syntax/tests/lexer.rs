//! Integration tests for the public API of the K lexer, `k_syntax::lex`.
//!
//! The golden corpus in `tests/corpus/lexer` pins down token streams and
//! diagnostic spans case by case. This file covers what the corpus format
//! cannot express:
//!
//! * table-driven checks over *every* keyword and punctuation kind, so a kind
//!   added to `kind.rs` is tested automatically;
//! * structured diagnostic details that `.diag` files deliberately leave as
//!   wildcards (severity, help lines, fix applicability, the file id);
//! * applying machine-applicable fixes and re-lexing the result;
//! * robustness: very deep nesting on a small stack, large inputs;
//! * property tests for lexer-specific invariants (beyond the general ones in
//!   `invariants.rs`): trivia shapes, `Error` tokens versus K0001, line
//!   terminators, diagnostic ordering and message conventions.
//!
//! The rules being tested are specified in `docs/compiler/grammar.md` §2–§4
//! and `docs/compiler/diagnostics.md` ("Lexer codes").

use k_diagnostics::{Applicability, Diagnostic, FileId, Severity};
use k_syntax::{Lexed, SyntaxKind, lex};
use proptest::prelude::*;

const FILE: FileId = FileId::new(0);

/// The non-whitespace tokens of `text`, as `(kind, text)` pairs.
fn tokens(text: &str) -> Vec<(SyntaxKind, String)> {
    lex(FILE, text)
        .iter()
        .filter(|(kind, _)| *kind != SyntaxKind::Whitespace)
        .map(|(kind, text)| (kind, text.to_owned()))
        .collect()
}

/// The codes of the diagnostics reported for `text`, in reported order.
fn codes(text: &str) -> Vec<String> {
    lex(FILE, text)
        .diagnostics()
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

/// The byte range of a diagnostic, as `start..end`.
fn byte_range(diagnostic: &Diagnostic) -> std::ops::Range<usize> {
    usize::from(diagnostic.span.start())..usize::from(diagnostic.span.end())
}

/// Asserts the general token-stream contract (also checked by
/// `invariants.rs`, repeated here so failures in this file are
/// self-explanatory).
fn assert_lossless(text: &str, lexed: &Lexed<'_>) {
    let mut offset = 0usize;
    for index in 0..lexed.len() {
        let range = lexed.range(index).expect("range of an existing token");
        assert_eq!(
            usize::from(range.start()),
            offset,
            "gap before token {index}"
        );
        assert!(!range.is_empty(), "empty token {index}");
        offset = usize::from(range.end());
    }
    assert_eq!(offset, text.len(), "tokens do not cover the input");
    let rebuilt: String = lexed.iter().map(|(_, t)| t).collect();
    assert_eq!(rebuilt, text);
}

/// Every punctuation and operator kind with fixed text, except the string
/// and interpolation delimiters (whose texts lex as other kinds on their own).
fn punctuation_kinds() -> Vec<SyntaxKind> {
    SyntaxKind::ALL
        .iter()
        .copied()
        .filter(|kind| kind.is_token() && !kind.is_keyword() && !kind.is_contextual_keyword())
        .filter(|kind| kind.fixed_text().is_some())
        .filter(|kind| {
            !matches!(
                kind,
                SyntaxKind::StringStart
                    | SyntaxKind::StringEnd
                    | SyntaxKind::InterpStart
                    | SyntaxKind::InterpEnd
            )
        })
        .collect()
}

// ------------------------------------------------------------ token kinds

#[test]
fn every_hard_keyword_lexes_to_its_kind() {
    let mut count = 0;
    for kind in SyntaxKind::ALL.iter().copied().filter(|k| k.is_keyword()) {
        let text = kind.fixed_text().expect("keywords have fixed text");
        // A trailing `x` after a space must not be absorbed; for `style` the
        // lookahead sees `x`, not `{`, so no style mode is entered.
        let source = format!("{text} x");
        assert_eq!(
            tokens(&source),
            vec![(kind, text.to_owned()), (SyntaxKind::Ident, "x".to_owned())],
            "{text}"
        );
        assert!(codes(&source).is_empty(), "{text}: {:?}", codes(&source));
        count += 1;
    }
    assert_eq!(count, 50, "grammar.md 3.3 lists 50 hard keywords");
}

#[test]
fn every_contextual_keyword_lexes_as_an_identifier() {
    for kind in SyntaxKind::ALL
        .iter()
        .copied()
        .filter(|k| k.is_contextual_keyword())
    {
        let text = kind
            .fixed_text()
            .expect("contextual keywords have fixed text");
        assert_eq!(
            tokens(text),
            vec![(SyntaxKind::Ident, text.to_owned())],
            "{text}"
        );
    }
}

#[test]
fn keywords_with_a_suffix_or_prefix_are_identifiers() {
    for kind in SyntaxKind::ALL.iter().copied().filter(|k| k.is_keyword()) {
        let text = kind.fixed_text().expect("keywords have fixed text");
        for word in [
            format!("{text}x"),
            format!("_{text}"),
            format!("{text}1"),
            text.to_uppercase(),
        ] {
            assert_eq!(tokens(&word), vec![(SyntaxKind::Ident, word.clone())]);
        }
    }
}

#[test]
fn every_punctuation_kind_lexes_alone() {
    let kinds = punctuation_kinds();
    assert!(
        kinds.len() >= 40,
        "expected the full table of grammar.md 3.7"
    );
    for kind in kinds {
        let text = kind.fixed_text().expect("filtered on fixed text");
        assert_eq!(tokens(text), vec![(kind, text.to_owned())], "{text:?}");
        assert!(codes(text).is_empty(), "{text:?}: {:?}", codes(text));
    }
}

#[test]
fn every_punctuation_kind_stops_before_a_following_identifier() {
    for kind in punctuation_kinds() {
        let text = kind.fixed_text().expect("filtered on fixed text");
        let source = format!("{text} q");
        assert_eq!(
            tokens(&source),
            vec![(kind, text.to_owned()), (SyntaxKind::Ident, "q".to_owned())],
            "{source:?}"
        );
    }
}

#[test]
fn the_lexer_never_produces_contextual_keyword_or_node_kinds() {
    let source = "component Counter { view { page layout key at test } }\nstyle { .a { b: c; } }\n";
    for (kind, _) in lex(FILE, source).iter() {
        assert!(kind.is_token(), "{kind:?}");
        assert!(!kind.is_contextual_keyword(), "{kind:?}");
    }
}

// ------------------------------------------------------------ accessors

#[test]
fn accessors_agree_with_iteration() {
    let source = "let é = \"日本{x}\" // c\r\n";
    let lexed = lex(FILE, source);
    assert_lossless(source, &lexed);
    for (index, (kind, text)) in lexed.iter().enumerate() {
        assert_eq!(lexed.kind(index), Some(kind));
        assert_eq!(lexed.text(index), Some(text));
        let range = lexed.range(index).expect("in bounds");
        assert_eq!(
            &source[usize::from(range.start())..usize::from(range.end())],
            text
        );
    }
    assert_eq!(lexed.kinds().len(), lexed.len());
    assert_eq!(lexed.kind(lexed.len()), None);
    assert_eq!(lexed.text(lexed.len()), None);
    assert_eq!(lexed.source(), source);
}

#[test]
fn token_ranges_are_byte_offsets() {
    let lexed = lex(FILE, "é x");
    // `é` is two bytes: Error 0..2, Whitespace 2..3, Ident 3..4.
    let ranges: Vec<_> = (0..lexed.len())
        .map(|i| {
            let r = lexed.range(i).expect("in bounds");
            (usize::from(r.start()), usize::from(r.end()))
        })
        .collect();
    assert_eq!(ranges, vec![(0, 2), (2, 3), (3, 4)]);
}

#[test]
fn empty_input_has_no_tokens_and_no_diagnostics() {
    let lexed = lex(FILE, "");
    assert!(lexed.is_empty());
    assert!(lexed.diagnostics().is_empty());
}

#[test]
fn lexing_is_deterministic() {
    let source = "component A {\n  style { .a { w: {x}; } }\n  let s = \"{a}{{\"\n}\n$";
    let first = lex(FILE, source);
    let second = lex(FILE, source);
    assert_eq!(first.kinds(), second.kinds());
    assert_eq!(first.diagnostics(), second.diagnostics());
}

// ---------------------------------------------------------- diagnostics

#[test]
fn diagnostics_use_the_given_file() {
    let file = FileId::new(7);
    let lexed = lex(file, "$ \"abc");
    assert_eq!(lexed.diagnostics().len(), 2);
    for diagnostic in lexed.diagnostics() {
        assert_eq!(diagnostic.span.file, file);
    }
}

#[test]
fn every_lexer_code_is_an_error_in_the_lexer_range() {
    // One input per lexer code K0001..=K0008.
    let inputs = [
        ("$", "K0001"),
        ("\"abc", "K0002"),
        ("/* abc", "K0003"),
        ("\"\\q\"", "K0004"),
        ("#12", "K0005"),
        ("0x", "K0006"),
        ("\"\\{\"", "K0007"),
        ("\"\\u{}\"", "K0008"),
    ];
    for (input, code) in inputs {
        let lexed = lex(FILE, input);
        let diagnostics = lexed.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{input:?}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code.to_string(), code, "{input:?}");
        assert_eq!(diagnostics[0].severity, Severity::Error, "{input:?}");
    }
}

#[test]
fn diagnostics_are_reported_in_source_order() {
    // K0004 (col 13) is detected before K0002 (col 9) is known, and the
    // nested unterminated strings are detected inner-first; the reported
    // order must still be by position.
    let source = "let s = \"abc\\\n\"{\"a{b\n$ #1\n";
    let lexed = lex(FILE, source);
    let starts: Vec<_> = lexed
        .diagnostics()
        .iter()
        .map(|d| byte_range(d).start)
        .collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted);
    assert_eq!(
        codes(source),
        ["K0002", "K0004", "K0002", "K0002", "K0001", "K0005"]
    );
}

#[test]
fn unexpected_dollar_suggests_bind() {
    let lexed = lex(FILE, "$count");
    let [diagnostic] = lexed.diagnostics() else {
        panic!("expected one diagnostic, got {:?}", lexed.diagnostics());
    };
    assert_eq!(diagnostic.code.to_string(), "K0001");
    assert!(
        diagnostic.helps.iter().any(|h| h.contains("bind count")),
        "diagnostics.md: a help line suggests `bind x` for `$x`; got {:?}",
        diagnostic.helps
    );
}

#[test]
fn brace_escape_fix_is_machine_applicable_and_produces_valid_code() {
    let source = "let s = \"a\\{b}\"\n";
    let lexed = lex(FILE, source);
    let [diagnostic] = lexed.diagnostics() else {
        panic!("expected one diagnostic, got {:?}", lexed.diagnostics());
    };
    assert_eq!(diagnostic.code.to_string(), "K0007");
    assert_eq!(byte_range(diagnostic), 10..12, "the two characters `\\{{`");
    let [fix] = diagnostic.fixes.as_slice() else {
        panic!("expected one fix, got {:?}", diagnostic.fixes);
    };
    assert_eq!(fix.applicability, Applicability::MachineApplicable);
    let fixed = apply_fixes(source, std::slice::from_ref(diagnostic));
    assert_eq!(fixed, "let s = \"a{{b}\"\n");
    assert!(lex(FILE, &fixed).diagnostics().is_empty());
}

#[test]
fn error_recovery_reports_every_problem() {
    // Each line has one problem; none may hide another.
    let source = "$\n\"open\n#zz\n0b2\n\"\\q\"\n\"\\u{D800}\"\n\"\\{\"\n/*";
    assert_eq!(
        codes(source),
        [
            "K0001", "K0002", "K0005", "K0006", "K0004", "K0008", "K0007", "K0003"
        ]
    );
}

#[test]
fn messages_follow_the_conventions() {
    // diagnostics.md rule 3: one line, lowercase start, no trailing period.
    let source = "$ é\n\"x\n/*\"\\q\"*/ #1 0x \"\\{\" \"\\u{}\" \"\\";
    let mut inputs = vec![source.to_owned()];
    inputs.extend(["\"\\q\"", "\"\\u{}\"", "#12345", "0b", "/*", "\"", "`"].map(str::to_owned));
    for input in inputs {
        for diagnostic in lex(FILE, &input).diagnostics() {
            assert_message_conventions(diagnostic);
        }
    }
}

fn assert_message_conventions(diagnostic: &Diagnostic) {
    let message = &diagnostic.message;
    assert!(!message.is_empty(), "{diagnostic:?}");
    assert!(!message.contains('\n'), "{diagnostic:?}");
    assert!(!message.ends_with('.'), "{diagnostic:?}");
    let first = message.chars().next().expect("non-empty");
    assert!(!first.is_uppercase(), "{diagnostic:?}");
}

/// Applies every fix of `diagnostics` to `source` (edits must not overlap).
fn apply_fixes(source: &str, diagnostics: &[Diagnostic]) -> String {
    let mut edits: Vec<_> = diagnostics
        .iter()
        .flat_map(|d| d.fixes.iter())
        .flat_map(|f| f.edits.iter())
        .map(|e| {
            (
                usize::from(e.span.start()),
                usize::from(e.span.end()),
                e.replacement.clone(),
            )
        })
        .collect();
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut out = source.to_owned();
    for (start, end, replacement) in edits {
        out.replace_range(start..end, &replacement);
    }
    out
}

// ----------------------------------------------------------- robustness

/// Runs `f` on a thread with a small stack, so recursion proportional to the
/// nesting depth of the input overflows and fails the test.
fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("lexing deeply nested input must not overflow the stack");
}

const DEPTH: usize = 100_000;

#[test]
fn deeply_nested_interpolations_use_constant_stack() {
    on_small_stack(|| {
        let source = format!("{}x{}", "\"{".repeat(DEPTH), "}\"".repeat(DEPTH));
        let lexed = lex(FILE, &source);
        assert_lossless(&source, &lexed);
        assert_eq!(lexed.len(), DEPTH * 4 + 1);
        assert!(lexed.diagnostics().is_empty());
    });
}

#[test]
fn deeply_nested_unterminated_interpolations_report_every_string() {
    on_small_stack(|| {
        let source = "\"{".repeat(DEPTH);
        let lexed = lex(FILE, &source);
        assert_lossless(&source, &lexed);
        assert_eq!(lexed.diagnostics().len(), DEPTH);
        assert!(
            lexed
                .diagnostics()
                .iter()
                .all(|d| d.code.to_string() == "K0002")
        );
    });
}

#[test]
fn deeply_nested_block_comments_use_constant_stack() {
    on_small_stack(|| {
        let source = format!("{}{}", "/*".repeat(DEPTH), "*/".repeat(DEPTH));
        let lexed = lex(FILE, &source);
        assert_eq!(lexed.kinds(), [SyntaxKind::BlockComment]);
        assert!(lexed.diagnostics().is_empty());
    });
}

#[test]
fn deeply_nested_braces_in_one_interpolation_use_constant_stack() {
    on_small_stack(|| {
        // The space keeps the first two braces from forming a `{{` escape.
        let source = format!("\"{{ {}{} }}\"", "{".repeat(DEPTH), "}".repeat(DEPTH));
        let lexed = lex(FILE, &source);
        assert_lossless(&source, &lexed);
        // StringStart InterpStart Whitespace ... Whitespace InterpEnd StringEnd
        assert_eq!(lexed.len(), DEPTH * 2 + 6);
        assert!(lexed.diagnostics().is_empty());
    });
}

#[test]
fn deeply_nested_style_rules_use_constant_stack() {
    on_small_stack(|| {
        let source = format!("style {{{}{}}}", ".a {".repeat(DEPTH), "}".repeat(DEPTH));
        let lexed = lex(FILE, &source);
        assert_lossless(&source, &lexed);
        assert!(lexed.diagnostics().is_empty());
        let idents = lexed
            .kinds()
            .iter()
            .filter(|k| **k == SyntaxKind::Ident)
            .count();
        assert_eq!(idents, DEPTH);
    });
}

#[test]
fn large_input_lexes_as_repeated_units() {
    // The unit ends in code mode after a newline, so the lexer state is the
    // same at the start of every repetition and the token count multiplies.
    let unit = "component Card {\n    @State var count: Int = 0\n    let label = \"{count} items {{ok}}\"\n    style { .card { padding: .md; width: calc(100% - 2px); } }\n}\n// note\n";
    let per_unit = lex(FILE, unit).len();
    let repetitions = (1 << 20) / unit.len() + 1;
    let source = unit.repeat(repetitions);
    let lexed = lex(FILE, &source);
    assert_lossless(&source, &lexed);
    assert!(lexed.diagnostics().is_empty());
    assert_eq!(lexed.len(), per_unit * repetitions);
}

// ------------------------------------------------------- property tests

/// Lexer-oriented fragments that reach every mode and error path.
const FRAGMENTS: &[&str] = &[
    "a",
    "_",
    "let",
    "style",
    "style {",
    "x.style {",
    " ",
    "\t",
    "\n",
    "\r\n",
    "\r",
    "{",
    "}",
    "(",
    ")",
    "\"",
    "\"a{b}c\"",
    "{{",
    "}}",
    "\\",
    "\\n",
    "\\{",
    "\\u{41}",
    "\\u{",
    "\\q",
    "//",
    "///",
    "/*",
    "*/",
    "#",
    "#fff",
    "#zz",
    "0x",
    "0b2",
    "1_",
    "1.5",
    "1e",
    "1e5",
    "16px",
    "50%",
    "-4px",
    ".",
    "..<",
    "...",
    "?.",
    "??",
    "-",
    "&",
    ":",
    ";",
    "@media",
    "&:hover",
    "sans-serif",
    "$",
    "`",
    "é",
    "😀",
    "\u{feff}",
    "\u{0}",
    "\u{c}",
];

fn k_shaped() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(FRAGMENTS), 0..48).prop_map(|parts| parts.concat())
}

fn any_input() -> impl Strategy<Value = String> {
    prop_oneof![any::<String>(), k_shaped()]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// grammar.md 3.2: every `Error` token has exactly one K0001 whose span is
    /// the token, and every K0001 is such a token.
    #[test]
    fn error_tokens_and_unexpected_character_diagnostics_correspond(text in any_input()) {
        let lexed = lex(FILE, &text);
        let error_ranges: Vec<_> = (0..lexed.len())
            .filter(|i| lexed.kind(*i) == Some(SyntaxKind::Error))
            .map(|i| {
                let r = lexed.range(i).expect("in bounds");
                usize::from(r.start())..usize::from(r.end())
            })
            .collect();
        let k0001: Vec<_> = lexed
            .diagnostics()
            .iter()
            .filter(|d| d.code.to_string() == "K0001")
            .map(byte_range)
            .collect();
        prop_assert_eq!(error_ranges, k0001);
    }

    /// grammar.md 3.1: Whitespace is spaces and tabs (plus a BOM at offset 0);
    /// Newline is exactly one line terminator.
    #[test]
    fn trivia_tokens_have_their_exact_shape(text in any_input()) {
        let lexed = lex(FILE, &text);
        for index in 0..lexed.len() {
            let kind = lexed.kind(index).expect("in bounds");
            let token = lexed.text(index).expect("in bounds");
            let start = usize::from(lexed.range(index).expect("in bounds").start());
            match kind {
                SyntaxKind::Whitespace => {
                    let body = if start == 0 { token.strip_prefix('\u{feff}').unwrap_or(token) } else { token };
                    prop_assert!(body.chars().all(|c| c == ' ' || c == '\t'), "{:?}", token);
                }
                SyntaxKind::Newline => {
                    prop_assert!(matches!(token, "\n" | "\r\n" | "\r"), "{:?}", token);
                }
                SyntaxKind::LineComment | SyntaxKind::DocComment => {
                    prop_assert!(token.starts_with("//"), "{:?}", token);
                }
                SyntaxKind::BlockComment => {
                    prop_assert!(token.starts_with("/*"), "{:?}", token);
                }
                _ => {}
            }
        }
    }

    /// Only Newline and BlockComment tokens contain line terminators, and a
    /// `\r\n` is never split between two tokens.
    #[test]
    fn line_terminators_stay_in_newlines_and_block_comments(text in any_input()) {
        let lexed = lex(FILE, &text);
        let mut previous_ends_with_cr = false;
        for (kind, token) in lexed.iter() {
            if !matches!(kind, SyntaxKind::Newline | SyntaxKind::BlockComment) {
                prop_assert!(!token.contains(['\n', '\r']), "{:?} {:?}", kind, token);
            }
            prop_assert!(!(previous_ends_with_cr && token.starts_with('\n')), "split CRLF at {:?}", token);
            previous_ends_with_cr = token.ends_with('\r');
        }
    }

    /// Lexer diagnostics are errors with lexer codes, sorted by position, on
    /// character boundaries, and follow the message conventions.
    #[test]
    fn diagnostics_are_well_formed(text in any_input()) {
        let lexed = lex(FILE, &text);
        let mut previous = 0usize;
        for diagnostic in lexed.diagnostics() {
            let range = byte_range(diagnostic);
            let number: u16 = diagnostic.code.to_string()[1..].parse().expect("K and four digits");
            prop_assert!((1..=99).contains(&number), "{:?}", diagnostic);
            prop_assert_eq!(diagnostic.severity, Severity::Error);
            prop_assert!(range.start >= previous, "out of order: {:?}", lexed.diagnostics());
            prop_assert!(range.end <= text.len());
            prop_assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            // Every lexer diagnostic points at existing text (none is an
            // insertion point).
            prop_assert!(!range.is_empty(), "{:?}", diagnostic);
            assert_message_conventions(diagnostic);
            previous = range.start;
        }
    }

    /// Applying every machine-applicable lexer fix removes the diagnostics it
    /// fixes and never introduces new lexer diagnostics.
    #[test]
    fn applying_lexer_fixes_removes_their_diagnostics(text in any_input()) {
        let lexed = lex(FILE, &text);
        let fixable: Vec<_> = lexed
            .diagnostics()
            .iter()
            .filter(|d| d.fixes.iter().any(|f| f.applicability == Applicability::MachineApplicable))
            .cloned()
            .collect();
        let before = lexed.diagnostics().len();
        let fixed = apply_fixes(&text, &fixable);
        let after = lex(FILE, &fixed).diagnostics().len();
        prop_assert_eq!(after, before - fixable.len(), "fixed text: {:?}", fixed);
    }

    /// Joining two inputs with a newline in between never changes how the
    /// first one's tokens before its last line are lexed when the first input
    /// is plain code (no strings, comments or braces that change mode).
    #[test]
    fn code_lines_lex_independently(
        a in "[a-z0-9 .+*=<>()\\[\\],:@?-]{0,40}",
        b in "[a-z0-9 .+*=<>()\\[\\],:@?-]{0,40}",
    ) {
        let joined = format!("{a}\n{b}");
        let mut expected = tokens(&a);
        expected.push((SyntaxKind::Newline, "\n".to_owned()));
        expected.extend(tokens(&b));
        prop_assert_eq!(tokens(&joined), expected);
    }
}
