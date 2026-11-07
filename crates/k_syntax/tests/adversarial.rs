//! Regression tests from adversarial testing of the lexer and parser that the golden corpus
//! cannot express: recursion the nesting limit (grammar.md RA-34) did not bound, and dumps
//! of very deep trees. The corpus cases from the same hunt are under
//! `tests/corpus/{lexer,parser}/regressions/`.
//!
//! Every test runs on a bounded stack. Overflowing it aborts the whole test binary, which is
//! the failure being guarded against, so these tests live apart from `robustness.rs`: one
//! overflow here must not hide the results of the other robustness tests.

#![allow(clippy::unwrap_used, clippy::panic)]

use k_diagnostics::{Code, FileId};
use k_syntax::{Parse, dump, parse};

/// The file id every test parses under.
const FILE: FileId = FileId::new(0);

/// Runs `f` on a thread with `mib` MiB of stack.
fn on_stack(mib: usize, f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(mib * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

/// Returns the number of `NESTING_TOO_DEEP` (K0134) diagnostics of `parse`.
fn too_deep_count(parse: &Parse) -> usize {
    parse
        .diagnostics()
        .iter()
        .filter(|d| d.code == Code::new(134))
        .count()
}

/// Asserts that `text` parses losslessly and reports K0134 once or twice (once per
/// over-deep construct; RA-34), on a stack the nesting limit is meant to fit in.
fn assert_bounded(name: &'static str, text: String) {
    on_stack(4, move || {
        let parse = parse(FILE, &text);
        assert_eq!(
            parse.syntax().text().len(),
            u32::try_from(text.len()).unwrap().into(),
            "{name}: tree is not lossless"
        );
        let n = too_deep_count(&parse);
        assert!(
            (1..=2).contains(&n),
            "{name}: expected K0134 once or twice, got {n}"
        );
    });
}

/// `() -> () -> ... -> Int`: every return type is a parenthesized type whose own return type
/// is parsed by recursion, but the `()` lists close before the recursion, so no level was
/// counted. At 5,000 levels the unwinding exhausted the parser's fuel (debug builds panicked
/// with "the parser is stuck"; release builds then saw a premature end of file), and at
/// 50,000 levels `k check` overflowed its stack.
#[test]
fn function_type_return_chains_are_bounded_by_the_nesting_limit() {
    for depth in [1_000, 10_000] {
        assert_bounded(
            "func-type returns",
            format!("let x: {}Int\n", "() -> ".repeat(depth)),
        );
    }
}

/// `() throws () throws ... E -> Int`: the thrown type recurses the same way.
#[test]
fn function_type_throws_chains_are_bounded_by_the_nesting_limit() {
    assert_bounded(
        "func-type throws",
        format!("let x: {}E -> Int\n", "() throws ".repeat(10_000)),
    );
}

/// `if if if ... x { } { } { }`: each condition is an `IfExpr` whose condition is parsed by
/// recursion before any block opens. 20,000 levels overflowed the stack of `k check` in a
/// release build, and no K0134 was reported at 1,000 levels.
#[test]
fn if_expressions_nested_in_conditions_are_bounded_by_the_nesting_limit() {
    for depth in [1_000, 20_000] {
        assert_bounded(
            "if conditions",
            format!(
                "func f() {{ let a = {}x{} }}\n",
                "if ".repeat(depth),
                " { }".repeat(depth)
            ),
        );
    }
}

/// `match match ... x { } { }`: the scrutinee recurses like an `if` condition.
#[test]
fn match_expressions_nested_in_scrutinees_are_bounded_by_the_nesting_limit() {
    for depth in [1_000, 20_000] {
        assert_bounded(
            "match scrutinees",
            format!(
                "func f() {{ let a = {}x{} }}\n",
                "match ".repeat(depth),
                " { }".repeat(depth)
            ),
        );
    }
}

/// `while if if ... x { } { }`: the same recursion through a `while` condition.
#[test]
fn if_expressions_nested_in_while_conditions_are_bounded_by_the_nesting_limit() {
    assert_bounded(
        "while conditions",
        format!(
            "func f() {{ while {}x{} {{ }} }}\n",
            "if ".repeat(5_000),
            " { }".repeat(5_000)
        ),
    );
}

/// The parser builds a 200,000-term sum and a 200,000-member chain with loops (no K0134;
/// they are valid code), but `dump::tree_sexpr`, which the golden corpus, `k debug ast` and
/// the `parse` fuzz target use, recursed once per tree level and overflowed an 8 MiB stack
/// (the main thread's default). The dump and its single-line rendering must handle any tree
/// the parser accepts.
#[test]
fn dumps_of_very_deep_trees_built_without_recursion_do_not_overflow() {
    on_stack(8, || {
        let sum = format!("let a = {}\n", vec!["x"; 200_000].join(" + "));
        let chain = format!("let a = x{}\n", ".m".repeat(200_000));
        for text in [sum, chain] {
            let parse = parse(FILE, &text);
            assert!(parse.diagnostics().is_empty());
            let sexpr = dump::tree_sexpr(&parse.syntax());
            let flat = sexpr.to_string();
            assert!(
                flat.starts_with("(SourceFile (FieldDecl let a ="),
                "{flat:.60}"
            );
            drop(sexpr);
        }
    });
}

/// Lambda parameter lists of any length are recognized. The lookahead that decides whether a
/// lambda's `{` is followed by parameters was capped at 512 tokens (a typed list longer than
/// that parsed as a block full of errors), and every lookahead burned the parser's fuel, so an
/// untyped list of a few thousand names exhausted it (a debug-build panic). The scan stops at
/// the first brace, so it stays linear without a cap.
#[test]
fn long_lambda_parameter_lists_are_recognized() {
    on_stack(8, || {
        let names: Vec<String> = (0..3_000).map(|i| format!("a{i}")).collect();
        let untyped = format!("let f = {{ {} => 1 }}\n", names.join(", "));
        let typed = format!(
            "let f = {{ ({}) -> Int => 1 }}\n",
            names
                .iter()
                .map(|n| format!("{n}: Int"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for text in [untyped, typed] {
            let parse = parse(FILE, &text);
            assert!(
                parse.diagnostics().is_empty(),
                "{:?}",
                parse.diagnostics().first()
            );
            let flat = dump::tree_sexpr(&parse.syntax()).to_string();
            assert!(flat.contains("(LambdaExpr (LambdaParamList"), "{flat:.80}");
        }
    });
}
