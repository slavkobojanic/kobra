//! Parser tests for expressions and statements that are generated rather than
//! written out one by one.
//!
//! The golden corpus (`tests/corpus/parser/expressions`) pins down individual
//! constructs by hand. This file complements it with exhaustive checks
//! derived mechanically from the precedence table in
//! `docs/compiler/grammar.md` §13.2:
//!
//! * every ordered pair of binary operators, `a OP1 b OP2 c`, groups as the
//!   table says, and non-associative pairs report K0113 at the second
//!   operator while still building a left-associative tree;
//! * prefix operators (`not`, `-`, `try`, `await`) interact with every binary
//!   operator as the table says;
//! * postfix operations bind tighter than every binary operator;
//! * every prefix of a set of realistic snippets (the end of file in the
//!   middle of every construct) parses losslessly without panicking, with
//!   in-bounds diagnostics;
//! * very deep nesting does not overflow the stack.
//!
//! Expected trees are compared structurally as S-expressions, exactly like
//! the corpus harness does.

// Helpers outside `#[test]` functions build expectations and may fail loudly.
#![allow(clippy::unwrap_used, clippy::panic)]

use k_diagnostics::FileId;
use k_syntax::sexpr::{self, Sexpr};
use k_syntax::{SyntaxKind, dump, parse};

/// Associativity of a binary precedence level.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Assoc {
    Left,
    Right,
    None,
}

/// The binary operators of grammar.md §13.2 with their level and
/// associativity (higher level binds tighter).
const BINARY: &[(&str, u8, Assoc)] = &[
    ("or", 2, Assoc::Left),
    ("and", 3, Assoc::Left),
    ("==", 5, Assoc::None),
    ("!=", 5, Assoc::None),
    ("<", 5, Assoc::None),
    ("<=", 5, Assoc::None),
    (">", 5, Assoc::None),
    (">=", 5, Assoc::None),
    ("??", 6, Assoc::Right),
    ("..<", 7, Assoc::None),
    ("...", 7, Assoc::None),
    ("+", 8, Assoc::Left),
    ("-", 8, Assoc::Left),
    ("*", 9, Assoc::Left),
    ("/", 9, Assoc::Left),
    ("%", 9, Assoc::Left),
];

/// The level of prefix `not`.
const NOT_LEVEL: u8 = 4;

/// Returns the tree dump of the name expression `n`.
fn name(n: &str) -> String {
    format!("(NameExpr {n})")
}

/// Returns the tree dump of the binary expression `lhs op rhs`, whose
/// operands are already dumps.
fn bin(lhs: &str, op: &str, rhs: &str) -> String {
    format!("(BinaryExpr {lhs} {op} {rhs})")
}

/// Wraps an expression dump in the tree of `let v = <expr>`.
fn top(expr: &str) -> String {
    format!("(SourceFile (FieldDecl let v = {expr}))")
}

/// Parses `text` as exactly one S-expression, panicking otherwise.
fn read_one(text: &str) -> Sexpr {
    let mut parsed =
        sexpr::parse(text).unwrap_or_else(|e| panic!("bad expected sexpr {text}: {e}"));
    assert_eq!(
        parsed.len(),
        1,
        "expected exactly one S-expression in {text}"
    );
    parsed.remove(0)
}

/// Parses `source` and returns its dump and its diagnostics as
/// `(code, start offset)` pairs.
fn parse_dump(source: &str) -> (Sexpr, Vec<(String, u32)>) {
    let parsed = parse(FileId::new(0), source);
    let tree = parsed.syntax();
    assert_eq!(
        tree.text().to_string(),
        source,
        "tree is not lossless for {source:?}"
    );
    let diagnostics = parsed
        .diagnostics()
        .iter()
        .map(|d| (d.code.to_string(), u32::from(d.span.start())))
        .collect();
    (dump::tree_sexpr(&tree), diagnostics)
}

/// Collects mismatches so one run reports every failing combination.
#[derive(Default)]
struct Failures(Vec<String>);

impl Failures {
    /// Parses `source` and records a failure if its tree dump differs from
    /// `expected_tree` or its diagnostics differ from `expected_diags`.
    fn check(&mut self, source: &str, expected_tree: &str, expected_diags: &[(String, u32)]) {
        let (actual, diags) = parse_dump(source);
        let expected = read_one(expected_tree);
        if actual != expected {
            self.0.push(format!(
                "{source:?}\n  expected {}\n  actual   {}",
                expected, actual
            ));
        }
        if diags != expected_diags {
            self.0.push(format!(
                "{source:?}\n  expected diagnostics {expected_diags:?}\n  actual   diagnostics {diags:?}"
            ));
        }
    }

    /// Panics with every recorded mismatch, if there are any.
    fn finish(self) {
        assert!(
            self.0.is_empty(),
            "{} mismatches:\n{}",
            self.0.len(),
            self.0.join("\n")
        );
    }
}

#[test]
fn every_pair_of_binary_operators_groups_by_the_precedence_table() {
    let mut failures = Failures::default();
    for &(op1, level1, assoc1) in BINARY {
        for &(op2, level2, _) in BINARY {
            let prefix = format!("let v = a {op1} b ");
            let source = format!("{prefix}{op2} c");
            let op2_offset = u32::try_from(prefix.len()).unwrap();
            let left = bin(&bin(&name("a"), op1, &name("b")), op2, &name("c"));
            let right = bin(&name("a"), op1, &bin(&name("b"), op2, &name("c")));
            let (tree, diags) = if level1 > level2 {
                (left, vec![])
            } else if level1 < level2 {
                (right, vec![])
            } else {
                match assoc1 {
                    Assoc::Left => (left, vec![]),
                    Assoc::Right => (right, vec![]),
                    Assoc::None => (left, vec![("K0113".to_owned(), op2_offset)]),
                }
            };
            failures.check(&source, &top(&tree), &diags);
        }
    }
    failures.finish();
}

#[test]
fn not_groups_with_each_binary_operator() {
    let mut failures = Failures::default();
    for &(op, level, _) in BINARY {
        let source = format!("let v = not a {op} b");
        let tree = if level > NOT_LEVEL {
            format!("(PrefixExpr not {})", bin(&name("a"), op, &name("b")))
        } else {
            bin("(PrefixExpr not (NameExpr a))", op, &name("b"))
        };
        failures.check(&source, &top(&tree), &[]);
    }
    for op in ["and", "or"] {
        let source = format!("let v = a {op} not b");
        failures.check(
            &source,
            &top(&bin(&name("a"), op, "(PrefixExpr not (NameExpr b))")),
            &[],
        );
    }
    failures.finish();
}

#[test]
fn unary_minus_binds_tighter_than_every_binary_operator() {
    let mut failures = Failures::default();
    for &(op, _, _) in BINARY {
        let source = format!("let v = -a {op} -b");
        let tree = bin(
            "(PrefixExpr - (NameExpr a))",
            op,
            "(PrefixExpr - (NameExpr b))",
        );
        failures.check(&source, &top(&tree), &[]);
    }
    failures.finish();
}

#[test]
fn try_and_await_cover_every_binary_operator() {
    let mut failures = Failures::default();
    for &(op, _, _) in BINARY {
        for keyword in ["try", "await"] {
            let node = if keyword == "try" {
                "TryExpr"
            } else {
                "AwaitExpr"
            };
            let source = format!("let v = {keyword} a {op} b");
            let tree = format!("({node} {keyword} {})", bin(&name("a"), op, &name("b")));
            failures.check(&source, &top(&tree), &[]);
        }
    }
    failures.finish();
}

#[test]
fn try_after_every_binary_operator_is_misplaced() {
    let mut failures = Failures::default();
    for &(op, _, _) in BINARY {
        let prefix = format!("let v = a {op} ");
        let source = format!("{prefix}try b");
        let offset = u32::try_from(prefix.len()).unwrap();
        let tree = bin(&name("a"), op, "(TryExpr try (NameExpr b))");
        failures.check(&source, &top(&tree), &[("K0114".to_owned(), offset)]);
    }
    failures.finish();
}

#[test]
fn postfix_operations_bind_tighter_than_every_binary_operator() {
    let postfixes: &[(&str, &str)] = &[
        ("b.c", "(MemberExpr (NameExpr b) c)"),
        ("b?.c", "(MemberExpr (NameExpr b) ?. c)"),
        (
            "b(1)",
            "(CallExpr (NameExpr b) (ArgList (Arg (Literal 1))))",
        ),
        ("b[0]", "(IndexExpr (NameExpr b) (Literal 0))"),
        (
            "b { x }",
            "(CallExpr (NameExpr b) (LambdaExpr (NameExpr x)))",
        ),
        (
            ".b(1)",
            "(CallExpr (ImplicitMemberExpr b) (ArgList (Arg (Literal 1))))",
        ),
        ("10.minutes", "(MemberExpr (Literal 10) minutes)"),
    ];
    let mut failures = Failures::default();
    for &(op, _, _) in BINARY {
        for &(text, dump) in postfixes {
            let source = format!("let v = {text} {op} {text}");
            failures.check(&source, &top(&bin(dump, op, dump)), &[]);
        }
    }
    failures.finish();
}

#[test]
fn symbolic_operators_recover_with_word_precedence() {
    let mut failures = Failures::default();
    // `&&` behaves like `and`, `||` like `or`, in every pairing with each other.
    for (sym1, word1) in [("&&", "and"), ("||", "or")] {
        for (sym2, word2) in [("&&", "and"), ("||", "or")] {
            let prefix1 = "let v = a ";
            let prefix2 = format!("{prefix1}{sym1} b ");
            let source = format!("{prefix2}{sym2} c");
            let level = |w: &str| if w == "and" { 3 } else { 2 };
            let tree = if level(word1) >= level(word2) {
                bin(&bin(&name("a"), sym1, &name("b")), sym2, &name("c"))
            } else {
                bin(&name("a"), sym1, &bin(&name("b"), sym2, &name("c")))
            };
            let code = |s: &str| if s == "&&" { "K0104" } else { "K0105" };
            let diags = vec![
                (code(sym1).to_owned(), u32::try_from(prefix1.len()).unwrap()),
                (code(sym2).to_owned(), u32::try_from(prefix2.len()).unwrap()),
            ];
            failures.check(&source, &top(&tree), &diags);
        }
    }
    failures.finish();
}

/// Realistic snippets covering the expression and statement grammar. Every
/// prefix of each one is parsed by [`every_prefix_parses_losslessly`].
const SNIPPETS: &[&str] = &[
    "let r = try a or not b and c == d ?? e..<f + g * -h.i(j)[k]\n",
    "let label = if done { \"Done\" } else if x { \"X\" } else { \"Pending\" }\n",
    "func f() {\n    let item = try await cart.add(product: .sample) else { return }\n}\n",
    "func f() {\n    match s {\n        case .a(let x), .b(count: 0) where x > 1 { x }\n        case 0..<10, -1...1 { }\n        default { }\n    }\n}\n",
    "func f() {\n    do {\n        try sync()\n    } catch .network(let e) where e.isTimeout {\n        retry()\n    } catch _ { }\n}\n",
    "func f() {\n    guard let user = session.user, user.isAdmin else { throw .denied }\n    for (k, v) in pairs key k { }\n    while let n = it.next() { continue }\n}\n",
    "let names = users\n    .filter { u => u.active }\n    .map { (u: User) -> String => u.name }\n",
    "let s = \"Total: {xs.filter { x => x > 0 }.count} {if a { \"y\" } else { \"n\" }} {{ }} \\u{1F600}\"\n",
    "let m = [\"a\": [1, 2,], .b: [:], f(x): [[]]]\nlet c = [.panel, .expanded when isExpanded]\n",
    "func f() {\n    Stepper(value: bind quantity)\n        .step(5)\n    x += 1; y = a ? b : c\n    z = !p && q || r\n}\n",
    "test \"t\" {\n    expect a.count == 1\n    expect throws E.a { try f() }\n}\n",
    "let w = 100px + 2rem * -4px - 50% / 10pz\nlet d = 2.hours.ago + 0.2.seconds\n",
];

#[test]
fn every_prefix_parses_losslessly() {
    for snippet in SNIPPETS {
        for (end, _) in snippet.char_indices().chain([(snippet.len(), ' ')]) {
            let source = &snippet[..end];
            let parsed = parse(FileId::new(0), source);
            let tree = parsed.syntax();
            assert_eq!(tree.kind(), SyntaxKind::SourceFile, "root for {source:?}");
            assert_eq!(tree.text().to_string(), source, "lossless for {source:?}");
            for d in parsed.diagnostics() {
                let (start, stop) = (usize::from(d.span.start()), usize::from(d.span.end()));
                assert!(
                    start <= stop && stop <= source.len(),
                    "span out of bounds for {source:?}: {d:?}"
                );
                assert!(
                    source.is_char_boundary(start) && source.is_char_boundary(stop),
                    "span not on a char boundary for {source:?}: {d:?}"
                );
            }
        }
    }
}

#[test]
fn parsing_is_deterministic() {
    for snippet in SNIPPETS {
        let (a, da) = parse_dump(snippet);
        let (b, db) = parse_dump(snippet);
        assert_eq!(a, b, "dump differs between two parses of {snippet:?}");
        assert_eq!(
            da, db,
            "diagnostics differ between two parses of {snippet:?}"
        );
    }
}

#[test]
fn valid_snippets_report_no_parse_errors() {
    // The snippets above deliberately contain errors in some places; these
    // are fully valid and must parse cleanly.
    let valid = [
        SNIPPETS[0],
        SNIPPETS[1],
        SNIPPETS[2],
        SNIPPETS[3],
        SNIPPETS[4],
        SNIPPETS[5],
        SNIPPETS[6],
        SNIPPETS[7],
        SNIPPETS[8],
        SNIPPETS[10],
    ];
    for snippet in valid {
        let parsed = parse(FileId::new(0), snippet);
        assert!(
            parsed.diagnostics().is_empty(),
            "unexpected diagnostics for {snippet:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

/// Runs `f` on a thread with a fixed, modest stack so a parser that recurses
/// without bound fails the test instead of passing by luck.
fn with_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(4 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn very_deep_nesting_does_not_overflow() {
    with_stack(|| {
        let depth = 1000;
        let cases = [
            format!("let a = {}x{}", "(".repeat(depth), ")".repeat(depth)),
            format!("let a = {}1{}", "[".repeat(depth), "]".repeat(depth)),
            format!("let a = {}x{}", "{ a => ".repeat(depth), " }".repeat(depth)),
            format!("let a = {}x", "not ".repeat(depth)),
            format!("let a = {}x", "- ".repeat(depth)),
            format!("let a = {}", vec!["x"; depth * 10].join(" + ")),
            format!("let a = x{}", ".m".repeat(depth * 10)),
            // Unclosed: the end of file arrives at maximal depth.
            format!("let a = {}", "f(".repeat(depth)),
            format!("let a = {}", "\"{".repeat(depth)),
        ];
        for source in &cases {
            let parsed = parse(FileId::new(0), source);
            assert_eq!(parsed.syntax().text().to_string(), *source);
        }
    });
}
