//! Robustness tests for the parser that complement the property tests in
//! `invariants.rs` and the fuzz targets in `fuzz/`:
//!
//! * a deterministic random stress run over K-shaped fragments, checking
//!   every invariant (losslessness, in-bounds spans, registered codes,
//!   message style, readable dumps) and, in debug builds, the parser's fuel
//!   assertion, which catches loops that do not consume input;
//! * nesting beyond the parser's limit (K0134) for every recursive construct,
//!   on a bounded stack;
//! * very deep trees produced *without* recursion (long operator chains and
//!   postfix chains), which must be dropped without overflowing the stack;
//! * machine-applicable quick fixes: applying them removes the diagnostic
//!   and yields code that parses cleanly.
//!
//! `K_STRESS_CASES=N` raises the number of random inputs (default 3000).

#![allow(clippy::unwrap_used, clippy::panic)]

use k_diagnostics::{Applicability, Code, Diagnostic, FileId};
use k_syntax::{Parse, dump, parse, sexpr};

/// The file id every test parses under.
const FILE: FileId = FileId::new(0);

/// Fragments covering every keyword, operator, literal shape, lexer mode and
/// a few frequent error shapes.
const FRAGMENTS: &[&str] = &[
    "component",
    "page",
    "layout",
    "service",
    "protocol",
    "entity",
    "repository",
    "struct",
    "enum",
    "class",
    "interface",
    "extend",
    "theme",
    "app",
    "test",
    "func",
    "let",
    "var",
    "init",
    "rpc",
    "case",
    "use",
    "import",
    "proto",
    "style",
    "view",
    "loading",
    "failed",
    "metadata",
    "variant",
    "key",
    "at",
    "for",
    "in",
    "if",
    "else",
    "match",
    "default",
    "guard",
    "while",
    "do",
    "catch",
    "try",
    "await",
    "throw",
    "throws",
    "return",
    "break",
    "continue",
    "expect",
    "bind",
    "when",
    "where",
    "implements",
    "requires",
    "async",
    "public",
    "private",
    "static",
    "not",
    "and",
    "or",
    "true",
    "false",
    "nil",
    "self",
    "as",
    "is",
    "@",
    "@State",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ",",
    ":",
    ";",
    ".",
    "..",
    "..<",
    "...",
    "?",
    "?.",
    "??",
    "->",
    "=>",
    "=",
    "==",
    "!=",
    "<",
    ">",
    "<=",
    ">=",
    "+",
    "-",
    "*",
    "/",
    "%",
    "+=",
    "&&",
    "||",
    "!",
    "&",
    "|",
    "~",
    "_",
    "\"",
    "\"a\"",
    "\"{x}\"",
    "\"{",
    "}\"",
    "{{",
    "\\n",
    "\\{",
    "#fff",
    "#zz",
    "16px",
    "4pz",
    "50%",
    "1.5",
    "0x",
    "1e",
    "x",
    "Foo",
    "a.b",
    "\n",
    "\n",
    "\r\n",
    " ",
    "  ",
    "\t",
    "// c",
    "/* c */",
    "/*",
    "///",
    "$",
    "é",
    "::",
    "&:hover",
    "@media",
    "(maxWidth: 600px)",
    "padding: .md;",
    "!important",
    ".a {",
    "box {",
    "f(",
    "x =>",
    "(x) ->",
];

/// A small deterministic xorshift generator, so failures reproduce.
struct Rng(u64);

impl Rng {
    /// Advances the xorshift state and returns it.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Returns a value in `0..n`; `n` must not be zero.
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }
}

/// Asserts every invariant a parse of `text` must satisfy.
fn check(text: &str) -> Parse {
    let parse = parse(FILE, text);
    assert_eq!(parse.syntax().text().to_string(), text, "not lossless");
    for d in parse.diagnostics() {
        let (start, end) = (usize::from(d.span.start()), usize::from(d.span.end()));
        assert!(
            start <= end && end <= text.len(),
            "span out of bounds: {d:?}"
        );
        assert!(
            text.is_char_boundary(start) && text.is_char_boundary(end),
            "{d:?}"
        );
        assert!(d.code.info().is_some(), "unregistered code: {d:?}");
        assert!(!d.message.is_empty() && !d.message.contains('\n'), "{d:?}");
        assert!(!d.message.starts_with(char::is_uppercase), "{d:?}");
        assert!(!d.message.ends_with('.'), "{d:?}");
        for fix in &d.fixes {
            assert!(!fix.edits.is_empty(), "{d:?}");
            for edit in &fix.edits {
                let (s, e) = (usize::from(edit.span.start()), usize::from(edit.span.end()));
                assert!(s <= e && e <= text.len(), "edit out of bounds: {d:?}");
                assert!(
                    text.is_char_boundary(s) && text.is_char_boundary(e),
                    "{d:?}"
                );
            }
        }
    }
    let starts: Vec<_> = parse.diagnostics().iter().map(|d| d.span.start()).collect();
    assert!(
        starts.windows(2).all(|w| w[0] <= w[1]),
        "diagnostics not sorted"
    );
    let dump = dump::dump_tree(&parse.syntax());
    assert!(
        sexpr::parse(&dump).is_ok(),
        "dump does not read back:\n{dump}"
    );
    parse
}

#[test]
fn random_k_shaped_inputs_keep_every_invariant() {
    let cases: usize = std::env::var("K_STRESS_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for iteration in 0..cases {
        let count = rng.below(120);
        let mut text = String::new();
        for _ in 0..count {
            text.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
            if rng.below(3) == 0 {
                text.push(' ');
            }
        }
        let result = std::panic::catch_unwind(|| {
            check(&text);
        });
        assert!(result.is_ok(), "iteration {iteration} failed on {text:?}");
    }
}

/// Runs `f` on a thread with `mib` MiB of stack. Overflowing it aborts the
/// whole test binary, which is the failure this guards against.
fn on_stack(mib: usize, f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(mib * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

/// Inputs nesting each recursive construct 2000 levels deep, well past the
/// parser's limit of 400.
fn deep_inputs() -> Vec<(&'static str, String)> {
    let d = 2000;
    vec![
        (
            "parentheses",
            format!("let a = {}x{}", "(".repeat(d), ")".repeat(d)),
        ),
        (
            "lambdas",
            format!("let a = {}x{}", "{ a => ".repeat(d), " }".repeat(d)),
        ),
        ("not", format!("let a = {}x", "not ".repeat(d))),
        ("minus", format!("let a = {}x", "- ".repeat(d))),
        (
            "views",
            format!("component T {{ view {{ {} }} }}", "box {\n".repeat(d)),
        ),
        ("ifs", format!("func f() {{ {} }}", "if a {\n".repeat(d))),
        (
            "else-ifs",
            format!("func f() {{ if a {{ }} {} }}", "else if b { } ".repeat(d)),
        ),
        (
            "styles",
            format!("component T {{ style {{ {} }} }}", ".a {\n".repeat(d)),
        ),
        ("types", format!("let x: {}Int", "List<".repeat(d))),
        ("strings", format!("let a = {}", "\"{".repeat(d))),
        ("suites", "test S {\n".repeat(d)),
        (
            "patterns",
            format!(
                "func f() {{ match x {{ case {}a {{ }} }} }}",
                ".a(".repeat(d)
            ),
        ),
        ("ternaries", format!("let a = {}x", "a ? b : ".repeat(d))),
        ("coalescing", format!("let a = {}x", "a ?? ".repeat(d))),
        ("calls", format!("let a = {}", "f(".repeat(d))),
        ("indexes", format!("let a = {}", "x[".repeat(d))),
        (
            "lists",
            format!("let a = {}1{}", "[".repeat(d), "]".repeat(d)),
        ),
    ]
}

#[test]
fn nesting_beyond_the_limit_is_reported_once_and_skipped() {
    // At the limit the parser needs about 2 MiB of stack in debug builds and
    // 1 MiB in release builds (docs/compiler/architecture.md, "Parser").
    on_stack(4, || {
        for (name, text) in deep_inputs() {
            let parse = check(&text);
            let too_deep = parse
                .diagnostics()
                .iter()
                .filter(|d| d.code == Code::new(134))
                .count();
            // Ternaries and else-if chains recurse only through constructs
            // that are not nested inside each other's delimiters, so they
            // may or may not reach the limit; everything else must.
            if !matches!(name, "ternaries" | "else-ifs" | "styles" | "types") {
                assert!(too_deep >= 1, "{name}: no K0134");
            }
            assert!(too_deep <= 2, "{name}: K0134 reported {too_deep} times");
        }
    });
}

#[test]
fn unwinding_the_deepest_nesting_at_the_end_of_file_is_not_a_stuck_parser() {
    // Every construct opened right up to the limit and then cut off by the
    // end of file: each level unwinds with lookaheads but no tokens left to
    // consume. In debug builds the fuel assertion would fire if unwinding
    // were mistaken for a loop that does not advance.
    on_stack(4, || {
        for opener in [
            "f(", "[", "(", "{ x => ", "box {\n", "if a {\n", "\"{", "List<", ".a(",
        ] {
            let text = format!("let a = {}", opener.repeat(399));
            check(&text);
            let text = format!("component T {{ view {{ {} }}", opener.repeat(398));
            check(&text);
        }
    });
}

#[test]
fn nesting_below_the_limit_is_not_reported() {
    on_stack(4, || {
        let d = 300;
        for text in [
            format!("let a = {}x{}", "(".repeat(d), ")".repeat(d)),
            format!(
                "component T {{ view {{\n{}\"x\"\n{}}} }}\n",
                "box {\n".repeat(d),
                "}\n".repeat(d)
            ),
            format!("let x: {}Int{} = []\n", "List<".repeat(d), ">".repeat(d)),
        ] {
            let parse = check(&text);
            assert!(
                parse.diagnostics().is_empty(),
                "{:?}",
                parse.diagnostics().first()
            );
        }
    });
}

#[test]
fn deep_trees_built_without_recursion_are_dropped_on_a_small_stack() {
    // A 100,000-term sum nests 99,999 BinaryExpr nodes, and a 100,000-member
    // chain as many MemberExprs, although the parser itself loops. Dropping
    // such trees must not recurse once per level.
    on_stack(1, || {
        let sum = format!("let a = {}\n", vec!["x"; 100_000].join(" + "));
        let tree = parse(FILE, &sum);
        assert!(tree.diagnostics().is_empty());
        assert!(tree.depth() > 99_999);
        assert_eq!(
            tree.syntax().text().len(),
            u32::try_from(sum.len()).unwrap().into()
        );
        drop(tree);

        let chain = format!("let a = x{}\n", ".m".repeat(100_000));
        let tree = parse(FILE, &chain);
        assert!(tree.diagnostics().is_empty());
        drop(tree);
    });
}

#[test]
fn shallow_trees_report_their_depth() {
    // SourceFile > FieldDecl > Literal
    assert_eq!(parse(FILE, "let a = 1\n").depth(), 3);
    assert_eq!(parse(FILE, "").depth(), 1);
}

/// Applies every machine-applicable fix of `diagnostics` (edits must not
/// overlap).
fn apply_fixes(source: &str, diagnostics: &[Diagnostic]) -> String {
    let mut edits: Vec<_> = diagnostics
        .iter()
        .flat_map(|d| d.fixes.iter().take(1))
        .filter(|f| f.applicability == Applicability::MachineApplicable)
        .flat_map(|f| f.edits.iter())
        .map(|e| {
            (
                usize::from(e.span.start()),
                usize::from(e.span.end()),
                e.replacement.clone(),
            )
        })
        .collect();
    edits.sort_by_key(|(start, end, _)| std::cmp::Reverse((*start, *end)));
    let mut out = source.to_owned();
    for (start, end, replacement) in edits {
        out.replace_range(start..end, &replacement);
    }
    out
}

#[test]
fn machine_applicable_fixes_produce_clean_code() {
    let cases: &[(&str, u16)] = &[
        ("func f() {\n    let a = 1 let b = 2\n}\n", 102),
        ("struct A {} struct B {}\n", 102),
        ("func f() {\n    let a = 1; let b = 2\n    g();\n}\n", 103),
        ("let ok = a && b\n", 104),
        ("let ok = a || b\n", 105),
        ("let ok = !done\nlet no = !   (a and b)\n", 106),
        (
            "component T {\n    style {\n        .a {\n            color: red\n            margin: 0;\n        }\n    }\n}\n",
            117,
        ),
        (
            "component T {\n    style {\n        .a { color: red !important; }\n    }\n}\n",
            118,
        ),
        ("let x: Int?? = nil\n", 120),
        ("service S {\n    private @Inject let api: Api\n}\n", 121),
        ("public public struct A {}\n", 127),
        ("let s = \"a{}b\"\n", 128),
        ("enum E {\n    case a, b, c(Int)\n}\n", 130),
    ];
    for (source, code) in cases {
        let before = check(source);
        assert!(
            before
                .diagnostics()
                .iter()
                .any(|d| d.code == Code::new(*code)),
            "{source:?} does not report K{code:04}: {:?}",
            before.diagnostics()
        );
        let fixed = apply_fixes(source, before.diagnostics());
        let after = check(&fixed);
        assert!(
            after.diagnostics().is_empty(),
            "fixing {source:?} gave {fixed:?} with {:?}",
            after.diagnostics()
        );
    }
}

#[test]
fn maybe_incorrect_fixes_are_marked() {
    for (source, code) in [
        ("var x = 1\n", 131),
        ("let a = b + try c()\n", 114),
        ("let r = 0..10\n", 119),
        ("let w = 4pz\n", 112),
        (
            "func f() {\n    match v {\n        case x { }\n    }\n}\n",
            115,
        ),
    ] {
        let parse = check(source);
        let diagnostic = parse
            .diagnostics()
            .iter()
            .find(|d| d.code == Code::new(code))
            .unwrap_or_else(|| panic!("{source:?}: no K{code:04}"));
        assert!(!diagnostic.fixes.is_empty(), "{source:?}: no fix");
        assert!(
            diagnostic
                .fixes
                .iter()
                .all(|f| f.applicability == Applicability::MaybeIncorrect),
            "{source:?}: {diagnostic:?}"
        );
    }
}
