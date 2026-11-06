//! Tests of the benchmark input generators in `benches/workload/mod.rs`.
//!
//! The benchmarks and the budget check (`tests/perf_budget.rs`) are only
//! meaningful if the text they measure is what it claims to be: realistic
//! input must be *valid* K (otherwise the numbers would measure error
//! recovery), must cover every kind of declaration and the constructs that
//! dominate real code, must be deterministic (otherwise baselines would not
//! be comparable) and must have the requested size. Pathological input must
//! stress what its name says and must be valid or invalid as documented.
//!
//! These tests run in the ordinary `cargo test` suite, in debug builds, on
//! small sizes; the large sizes are exercised by the benchmarks.

#![allow(clippy::unwrap_used, clippy::panic)]

#[path = "../benches/workload/mod.rs"]
mod workload;

use k_diagnostics::FileId;
use k_syntax::{SyntaxKind, SyntaxNode, lex, parse};
use std::collections::BTreeSet;
use workload::{Pathological, Rng, SEED, line_count, realistic, typical_file};

const FILE: FileId = FileId::new(0);

/// Parses `text` and fails with the first few diagnostics, quoting the lines
/// they point at, if there are any.
fn assert_valid(text: &str, what: &str) {
    let parsed = parse(FILE, text);
    if parsed.diagnostics().is_empty() {
        assert_eq!(
            parsed.syntax().text().to_string(),
            text,
            "{what}: not lossless"
        );
        return;
    }
    let mut report = String::new();
    for d in parsed.diagnostics().iter().take(8) {
        let start = usize::from(d.span.start());
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        let line_number = text[..start].matches('\n').count() + 1;
        report.push_str(&format!(
            "  {} at line {line_number}: {}\n    | {}\n",
            d.code,
            d.message,
            &text[line_start..line_end]
        ));
    }
    panic!(
        "{what}: expected valid K, got {} diagnostics:\n{report}",
        parsed.diagnostics().len()
    );
}

/// The set of node kinds that occur in `node`'s subtree.
fn node_kinds(node: &SyntaxNode) -> BTreeSet<SyntaxKind> {
    node.descendants().map(|n| n.kind()).collect()
}

// ---------------------------------------------------------------- the PRNG

#[test]
fn rng_matches_the_splitmix64_reference_sequence() {
    // Known-answer values of SplitMix64 for seed 0 (Vigna's reference).
    let mut rng = Rng::new(0);
    assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
    assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
    assert_eq!(rng.next_u64(), 0x06c4_5d18_8009_454f);
}

#[test]
fn rng_helpers_stay_in_range() {
    let mut rng = Rng::new(7);
    for _ in 0..10_000 {
        assert!(rng.below(3) < 3);
        let r = rng.range(5, 9);
        assert!((5..=9).contains(&r));
        assert_eq!(rng.range(4, 4), 4);
        assert!(["a", "b"].contains(&rng.pick(&["a", "b"])));
    }
    assert!(!rng.chance(0));
    assert!(rng.chance(100));
}

// ---------------------------------------------------------- realistic input

#[test]
fn realistic_input_is_valid_for_many_seeds() {
    for seed in 0..64 {
        let text = realistic(400, seed);
        assert_valid(&text, &format!("realistic(400, {seed})"));
    }
}

#[test]
fn realistic_input_is_valid_at_benchmark_sizes() {
    for lines in [1_000, 10_000] {
        let text = realistic(lines, SEED);
        assert_valid(&text, &format!("realistic({lines}, SEED)"));
    }
}

#[test]
fn realistic_input_is_deterministic() {
    assert_eq!(realistic(2_000, SEED), realistic(2_000, SEED));
    assert_eq!(typical_file(3), typical_file(3));
    assert_ne!(realistic(2_000, 1), realistic(2_000, 2));
}

#[test]
fn realistic_input_has_the_requested_number_of_lines() {
    for lines in [0, 1, 50, 300, 1_000, 10_000] {
        let text = realistic(lines, SEED);
        let actual = line_count(&text);
        assert!(
            actual >= lines,
            "realistic({lines}) has only {actual} lines"
        );
        // At most one declaration (a few dozen lines) more than requested,
        // plus the import block.
        assert!(
            actual <= lines + 120,
            "realistic({lines}) has {actual} lines"
        );
    }
}

#[test]
fn realistic_lines_have_a_realistic_length() {
    let text = realistic(10_000, SEED);
    let bytes_per_line = text.len() as f64 / line_count(&text) as f64;
    // The project-size budgets assume about 35 bytes per line.
    assert!(
        (22.0..=48.0).contains(&bytes_per_line),
        "{bytes_per_line:.1} bytes per line"
    );
    // No line is absurdly long: realistic input must not hide a
    // pathological case.
    let longest = text.lines().map(str::len).max().unwrap();
    assert!(longest <= 600, "longest line has {longest} bytes");
}

#[test]
fn realistic_input_covers_every_declaration_and_major_construct() {
    use SyntaxKind as K;
    let text = realistic(5_000, SEED);
    let parsed = parse(FILE, &text);
    let kinds = node_kinds(&parsed.syntax());
    let expected = [
        // Files and declarations.
        K::Import,
        K::ComponentDecl,
        K::PageDecl,
        K::LayoutDecl,
        K::ServiceDecl,
        K::ProtocolDecl,
        K::EntityDecl,
        K::RepositoryDecl,
        K::StructDecl,
        K::EnumDecl,
        K::ClassDecl,
        K::InterfaceDecl,
        K::ThemeDecl,
        K::AppDecl,
        K::ExtendDecl,
        K::TestSuiteDecl,
        K::TestCaseDecl,
        K::FuncDecl,
        K::FieldDecl,
        // Members.
        K::InitDecl,
        K::RpcDecl,
        K::EnumCase,
        K::UseDecl,
        K::ThemeToken,
        K::ThemeVariant,
        K::ViewBlock,
        K::StyleBlock,
        K::LoadingBlock,
        K::FailedBlock,
        K::MetadataBlock,
        K::Annotation,
        // Statements.
        K::LetStmt,
        K::AssignStmt,
        K::ReturnStmt,
        K::ThrowStmt,
        K::BreakStmt,
        K::ContinueStmt,
        K::GuardStmt,
        K::WhileStmt,
        K::ForStmt,
        K::DoStmt,
        K::ExpectStmt,
        K::ExpectThrowsStmt,
        // Expressions.
        K::NameExpr,
        K::ImplicitMemberExpr,
        K::ParenExpr,
        K::TupleExpr,
        K::ListExpr,
        K::MapExpr,
        K::LambdaExpr,
        K::IfExpr,
        K::MatchExpr,
        K::TryExpr,
        K::AwaitExpr,
        K::PrefixExpr,
        K::BinaryExpr,
        K::CallExpr,
        K::MemberExpr,
        K::IndexExpr,
        K::BindExpr,
        K::WhenExpr,
        K::StringLit,
        K::Interpolation,
        // Styles.
        K::StyleRule,
        K::Selector,
        K::CompoundSelector,
        K::ClassSelector,
        K::NestingSelector,
        K::PseudoClass,
        K::PseudoElement,
        K::AttributeSelector,
        K::UniversalSelector,
        K::AtRule,
        K::Declaration,
        K::CssFunction,
    ];
    let missing: Vec<_> = expected.iter().filter(|k| !kinds.contains(k)).collect();
    assert!(
        missing.is_empty(),
        "realistic input never produces {missing:?}"
    );
}

#[test]
fn realistic_input_covers_every_literal_and_trivia_kind() {
    use SyntaxKind as K;
    let text = realistic(5_000, SEED);
    let lexed = lex(FILE, &text);
    let kinds: BTreeSet<SyntaxKind> = lexed.kinds().iter().copied().collect();
    for kind in [
        K::IntLit,
        K::FloatLit,
        K::UnitLit,
        K::ColorLit,
        K::StringText,
        K::StringEscape,
        K::InterpStart,
        K::DocComment,
        K::LineComment,
        K::BlockComment,
        K::Whitespace,
        K::Newline,
    ] {
        assert!(
            kinds.contains(&kind),
            "realistic input has no {kind:?} token"
        );
    }
    // Non-ASCII text appears inside strings.
    assert!(!text.is_ascii());
}

#[test]
fn typical_file_is_about_300_valid_lines() {
    for seed in 0..16 {
        let text = typical_file(seed);
        let lines = line_count(&text);
        assert!(
            (300..=420).contains(&lines),
            "typical_file({seed}) has {lines} lines"
        );
        assert_valid(&text, &format!("typical_file({seed})"));
    }
}

#[test]
fn line_count_counts_like_an_editor() {
    assert_eq!(line_count(""), 0);
    assert_eq!(line_count("a"), 1);
    assert_eq!(line_count("a\n"), 1);
    assert_eq!(line_count("a\nb"), 2);
    assert_eq!(line_count("\n\n"), 2);
}

// -------------------------------------------------------- pathological input

/// A small size for each family that still shows its shape.
fn small_size(family: Pathological) -> usize {
    (family.megabyte_size() / 100).max(2)
}

#[test]
fn valid_pathological_inputs_parse_without_diagnostics() {
    for family in Pathological::ALL.into_iter().filter(|f| f.is_valid()) {
        let text = family.generate(small_size(family));
        assert_valid(&text, family.name());
    }
}

#[test]
fn every_pathological_input_is_lossless_and_never_panics() {
    for family in Pathological::ALL {
        let text = family.generate(small_size(family));
        let lexed = lex(FILE, &text);
        let joined: String = lexed.iter().map(|(_, t)| t).collect();
        assert_eq!(joined, text, "{}: lexer is not lossless", family.name());
        let parsed = parse(FILE, &text);
        assert_eq!(
            parsed.syntax().text().to_string(),
            text,
            "{}",
            family.name()
        );
    }
}

#[test]
fn too_deep_input_reports_k0134_exactly_once() {
    let text = Pathological::TooDeep.generate(5_000);
    let parsed = parse(FILE, &text);
    let k0134: Vec<_> = parsed
        .diagnostics()
        .iter()
        .filter(|d| d.code.to_string() == "K0134")
        .collect();
    assert_eq!(k0134.len(), 1, "{:?}", parsed.diagnostics());
}

#[test]
fn many_diagnostics_input_has_errors_on_every_line() {
    let lines = 240;
    let text = Pathological::ManyDiagnostics.generate(lines);
    let parsed = parse(FILE, &text);
    let mut lines_with_errors = BTreeSet::new();
    for d in parsed.diagnostics() {
        let start = usize::from(d.span.start());
        lines_with_errors.insert(text[..start].matches('\n').count());
    }
    assert_eq!(
        lines_with_errors.len(),
        lines,
        "every line must report a diagnostic"
    );
    assert!(
        parsed.diagnostics().len() >= lines * 3 / 2,
        "{}",
        parsed.diagnostics().len()
    );
    // Many of them carry quick fixes, which exercise the fix helpers.
    let with_fixes = parsed
        .diagnostics()
        .iter()
        .filter(|d| !d.fixes.is_empty())
        .count();
    assert!(
        with_fixes >= lines / 2,
        "only {with_fixes} diagnostics have fixes"
    );
}

#[test]
fn deep_families_reach_their_documented_depth() {
    // The deepest valid families must actually produce deep trees, or they
    // would not test what they claim.
    for (family, minimum) in [
        (Pathological::DeepParens, 300),
        (Pathological::DeepBlocks, 150),
        (Pathological::DeepView, 150),
        (Pathological::DeepStyle, 150),
        (Pathological::NestedInterpolation, 100),
        (Pathological::MemberChain, 1_000),
        (Pathological::LongSum, 1_000),
    ] {
        let size = if matches!(family, Pathological::MemberChain | Pathological::LongSum) {
            2_000
        } else {
            2
        };
        let parsed = parse(FILE, &family.generate(size));
        assert!(
            parsed.depth() >= minimum,
            "{}: depth {} < {minimum}",
            family.name(),
            parsed.depth()
        );
    }
}

#[test]
fn pathological_megabyte_sizes_yield_about_a_megabyte() {
    for family in Pathological::ALL {
        let len = family.generate(family.megabyte_size()).len();
        assert!(
            (500_000..=2_000_000).contains(&len),
            "{}: {len} bytes at megabyte_size",
            family.name()
        );
    }
}

#[test]
fn pathological_sizes_scale_linearly() {
    for family in Pathological::ALL {
        let small = family.generate(small_size(family)).len() as f64;
        let large = family.generate(small_size(family) * 4).len() as f64;
        let ratio = large / small;
        assert!(
            (3.5..=4.5).contains(&ratio),
            "{}: ratio {ratio:.2}",
            family.name()
        );
    }
}

#[test]
fn pathological_names_are_unique_and_kebab_case() {
    let names: BTreeSet<_> = Pathological::ALL.iter().map(|f| f.name()).collect();
    assert_eq!(names.len(), Pathological::ALL.len());
    for name in names {
        assert!(
            name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'),
            "{name}"
        );
    }
}
