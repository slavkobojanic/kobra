//! Property tests for semantic analysis, run on stable with proptest.
//!
//! Every property is checked by the shared semantic oracle in
//! `support/soracle.rs`, the same code the nightly `check*` fuzz targets in
//! `fuzz/` run, over three input families:
//!
//! * **arbitrary text** analysed as the only file of a project, at a module
//!   path, a test file path, `App.k` or a stray path;
//! * **syntax-generated projects**: one to three programs of the grammar
//!   generator (`crates/k_syntax/tests/support/kgen.rs`), which are
//!   well-formed (or deliberately noisy) but rarely well-typed;
//! * **well-typed projects** from the typed generator
//!   (`support/tgen.rs`): a `model` module of public types and a `logic`
//!   module of private types, functions and constants.
//!
//! The properties:
//!
//! 1. checking never panics, and every diagnostic is well-formed
//!    (`soracle::check_diagnostics`);
//! 2. diagnostics are deterministic: a fresh analysis and a single-threaded
//!    one report exactly the same list;
//! 3. checking is independent of the order files are added;
//! 4. an incremental re-check after a sequence of edits equals a fresh
//!    check of the edited project;
//! 5. well-typed generated projects produce zero errors;
//! 6. applying every machine-applicable fix never introduces a new error,
//!    on generated mistakes and on every case of the `sema` corpus.
//!
//! `PROPTEST_CASES=N` sets the number of cases per property (defaults
//! below; each case runs several whole-project analyses). Failures are
//! shrunk and persisted in `properties.proptest-regressions`; pin each
//! finding as a corpus case under `tests/corpus/sema/regressions/`
//! (docs/compiler/testing.md, "Regression tests").

#![allow(clippy::unwrap_used, clippy::panic)]

use crate::kgen;
use crate::soracle;
use crate::tgen;

use std::fs;
use std::path::{Path, PathBuf};

use arbitrary::Unstructured;
use proptest::prelude::*;
use soracle::{MANIFEST, Norm};

/// The project paths a loose file is analysed at.
const PATHS: &[&str] = &[
    "src/fz/Case.k",
    "src/fz/Case.test.k",
    "App.k",
    "notes/Stray.k",
];

/// The config for a property with `default` cases, unless `PROPTEST_CASES`
/// is set.
fn config(default: u32) -> ProptestConfig {
    if std::env::var_os("PROPTEST_CASES").is_some() {
        ProptestConfig::default()
    } else {
        ProptestConfig::with_cases(default)
    }
}

fn bytes(max: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..max)
}

/// A project holding `text` at `PATHS[at]`.
fn loose(text: &str, at: usize) -> Vec<(String, String)> {
    vec![
        ("k.toml".to_owned(), MANIFEST.to_owned()),
        (PATHS[at % PATHS.len()].to_owned(), text.to_owned()),
    ]
}

/// One to three grammar-generated programs in two modules, read from `u`.
fn syntax_project(u: &mut Unstructured<'_>) -> Vec<(String, String)> {
    let n = 1 + u.choose_index(3).unwrap_or(0);
    let mut files = vec![("k.toml".to_owned(), MANIFEST.to_owned())];
    for i in 0..n {
        let program = kgen::generate(u);
        let path = ["src/a/A.k", "src/a/B.k", "src/b/C.k"][i];
        files.push((path.to_owned(), program.source));
    }
    files
}

/// Asserts that a typed program has no errors.
fn assert_no_errors(program: &tgen::Program, diagnostics: &[Norm]) {
    let errors = soracle::errors(diagnostics);
    assert!(
        errors.is_empty(),
        "a well-typed generated project has errors: {errors:#?}\n{}",
        program.dump()
    );
}

proptest! {
    #![proptest_config(config(256))]

    #[test]
    fn arbitrary_text_is_checked_without_panicking(text in any::<String>(), at in 0usize..4) {
        let files = loose(&text, at);
        let diagnostics = soracle::check(&files);
        soracle::check_deterministic(&files, &diagnostics);
    }

    #[test]
    fn lossy_bytes_are_checked_without_panicking(raw in bytes(512), at in 0usize..4) {
        let files = loose(&String::from_utf8_lossy(&raw), at);
        soracle::check(&files);
    }
}

proptest! {
    #![proptest_config(config(128))]

    #[test]
    fn syntax_generated_projects_keep_every_invariant(raw in bytes(4096), order in bytes(16)) {
        let files = syntax_project(&mut Unstructured::new(&raw));
        let diagnostics = soracle::check(&files);
        soracle::check_deterministic(&files, &diagnostics);
        let perm = soracle::permutation(&mut Unstructured::new(&order), files.len());
        soracle::check_order_independent(&files, &perm, &diagnostics);
        let host = soracle::host(&files, k_sema::Options::new());
        soracle::check_queries(&host.analysis(), 300);
    }

    #[test]
    fn well_typed_projects_have_no_errors(raw in bytes(4096)) {
        let program = tgen::generate(&mut Unstructured::new(&raw));
        let diagnostics = soracle::check(&program.files);
        assert_no_errors(&program, &diagnostics);
    }

    #[test]
    fn well_typed_projects_are_deterministic_and_order_independent(raw in bytes(4096), order in bytes(16)) {
        let program = tgen::generate(&mut Unstructured::new(&raw));
        let diagnostics = soracle::check(&program.files);
        soracle::check_deterministic(&program.files, &diagnostics);
        let perm = soracle::permutation(&mut Unstructured::new(&order), program.files.len());
        soracle::check_order_independent(&program.files, &perm, &diagnostics);
    }

    #[test]
    fn incremental_recheck_equals_a_fresh_check(raw in bytes(4096), edits in bytes(256)) {
        let program = tgen::generate(&mut Unstructured::new(&raw));
        let mut u = Unstructured::new(&edits);
        let n = 1 + u.choose_index(4).unwrap_or(0);
        let mut files = program.files.clone();
        let mut sequence = Vec::new();
        for _ in 0..n {
            let edit = soracle::random_edit(&files, &mut u);
            soracle::apply_edit(&mut files, &edit);
            sequence.push(edit);
        }
        soracle::check_incremental(&program.files, &sequence);
    }

    #[test]
    fn incremental_recheck_of_syntax_projects_equals_a_fresh_check(raw in bytes(4096), edits in bytes(256)) {
        let files = syntax_project(&mut Unstructured::new(&raw));
        let mut u = Unstructured::new(&edits);
        let n = 1 + u.choose_index(3).unwrap_or(0);
        let mut current = files.clone();
        let mut sequence = Vec::new();
        for _ in 0..n {
            let edit = soracle::random_edit(&current, &mut u);
            soracle::apply_edit(&mut current, &edit);
            sequence.push(edit);
        }
        soracle::check_incremental(&files, &sequence);
    }

    #[test]
    fn machine_fixes_of_generated_mistakes_add_no_errors(raw in bytes(4096), mistakes in bytes(32)) {
        let program = tgen::generate(&mut Unstructured::new(&raw));
        let files = tgen::inject_fixable_mistakes(&program, &mut Unstructured::new(&mistakes));
        let before = soracle::check(&files);
        soracle::check_fixes_add_no_errors(&files, &before);
    }
}

// ---------------------------------------------------------------- the corpus

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/sema")
}

/// Collects the `sema` corpus cases: single `.k` files outside projects,
/// and project directories (those with a `k.toml`).
fn collect_cases(dir: &Path, singles: &mut Vec<PathBuf>, projects: &mut Vec<PathBuf>) {
    if dir.join("k.toml").is_file() {
        projects.push(dir.to_path_buf());
        return;
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_cases(&path, singles, projects);
        } else if path.extension().is_some_and(|e| e == "k") {
            singles.push(path);
        }
    }
}

fn project_files(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            project_files(root, &path, out);
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.contains('/') && rel.starts_with("expected.") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            out.push((rel, text));
        }
    }
}

/// A `sema` corpus case loaded as a project.
struct CorpusCase {
    /// The case's path, for messages.
    name: String,
    files: Vec<(String, String)>,
    /// `true` if the case has a `.pending` file: an open finding whose
    /// expectations don't hold yet.
    pending: bool,
}

/// Loads every case of the `sema` corpus as a project, as the corpus suite
/// does (`crates/k_corpus/src/sema_suite.rs`): a single file is
/// `src/corpus/<name>` unless its first line is `// path: …`.
fn corpus_projects() -> Vec<CorpusCase> {
    let (mut singles, mut projects) = (Vec::new(), Vec::new());
    collect_cases(&corpus_dir(), &mut singles, &mut projects);
    let mut cases = Vec::new();
    for file in singles {
        let text = fs::read_to_string(&file).unwrap();
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let path = text
            .lines()
            .next()
            .and_then(|l| l.trim_start().strip_prefix("// path:"))
            .map_or_else(|| format!("src/corpus/{name}"), |p| p.trim().to_owned());
        let mut files = vec![(path.clone(), text)];
        if path != "k.toml" {
            files.push((
                "k.toml".to_owned(),
                "[package]\nname = \"corpus\"\nk = \"0.1\"\n".to_owned(),
            ));
        }
        cases.push(CorpusCase {
            name: file.display().to_string(),
            files,
            pending: file.with_extension("pending").is_file(),
        });
    }
    for dir in projects {
        let mut files = Vec::new();
        project_files(&dir, &dir, &mut files);
        cases.push(CorpusCase {
            name: dir.display().to_string(),
            files,
            pending: dir.join("expected.pending").is_file(),
        });
    }
    cases
}

/// Corpus cases whose machine-applicable fixes are known to add errors:
/// checker findings of this property, reported and not yet fixed. Like a
/// `.pending` file, an entry must keep failing; once the fix is repaired the
/// test fails and asks for the entry to be removed. Each names the case
/// (a suffix of its path) and the finding.
///
/// Empty since round 2: the `try`/`await` fixes insert before the covering
/// expression, the `try` fix is maybe-incorrect where the error can't
/// propagate, and K3001's `var`-to-`let` is maybe-incorrect for an assigned
/// field.
const KNOWN_BAD_FIXES: &[(&str, &str)] = &[];

/// Every corpus case's machine-applicable fixes add no error. Pending cases
/// are skipped: they pin open findings (often a wrong fix) whose
/// expectations don't hold yet.
#[test]
fn machine_fixes_in_the_corpus_add_no_errors() {
    let cases = corpus_projects();
    assert!(cases.len() > 300, "the sema corpus is missing");
    let mut fixed = 0;
    let mut failures = Vec::new();
    for CorpusCase {
        name,
        files,
        pending,
    } in &cases
    {
        if *pending
            || files
                .iter()
                .any(|(p, _)| k_sema::ProjectPath::new(p).is_err())
        {
            continue;
        }
        let before = soracle::check(files);
        let (_, applied) = soracle::apply_machine_fixes(files, &before);
        if applied == 0 {
            continue;
        }
        fixed += 1;
        let result =
            std::panic::catch_unwind(|| soracle::check_fixes_add_no_errors(files, &before));
        let known = KNOWN_BAD_FIXES
            .iter()
            .find(|(case, _)| name.ends_with(case));
        match (result, known) {
            (Ok(_), None) => {}
            (Ok(_), Some((case, _))) => failures.push(format!(
                "{case}: its fixes no longer add errors; remove it from KNOWN_BAD_FIXES"
            )),
            (Err(_), Some(_)) => {}
            (Err(e), None) => {
                let message = e.downcast_ref::<String>().cloned().unwrap_or_default();
                let head: String = message.lines().take(3).collect::<Vec<_>>().join(" | ");
                failures.push(format!("{name}: {head}"));
            }
        }
    }
    assert!(
        fixed > 10,
        "only {fixed} corpus cases have machine-applicable fixes"
    );
    assert!(
        failures.is_empty(),
        "{} finding(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ------------------------------------------------------- generator sanity

#[test]
fn typed_generator_on_exhausted_input_yields_a_clean_project() {
    let program = tgen::generate(&mut Unstructured::new(&[]));
    let diagnostics = soracle::check(&program.files);
    assert_no_errors(&program, &diagnostics);
}

/// A cheap check that the typed generator is not degenerate: across
/// deterministic pseudo-random inputs it writes every construct it knows,
/// and every project is error-free.
#[test]
fn typed_generator_covers_its_constructs() {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut seen = std::collections::BTreeSet::new();
    let constructs = [
        "public struct",
        "public enum",
        "\nstruct ",
        "\nenum ",
        "import model.",
        "match ",
        "default {",
        "case nil",
        "case .some(",
        "if let ",
        "guard let ",
        "while ",
        "for ",
        " ?? ",
        ".count",
        "(if ",
        " += ",
        " and ",
        "(not ",
        "{l",
        "\"",
        "func m0(",
        "let g0",
        ": let ",
        "= .",
        "nil",
        "[]",
    ];
    for _ in 0..160 {
        let raw: Vec<u8> = (0..3000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        let program = tgen::generate(&mut Unstructured::new(&raw));
        let all: String = program.files.iter().map(|(_, t)| t.as_str()).collect();
        for c in constructs {
            if all.contains(c) {
                seen.insert(c);
            }
        }
        let diagnostics = soracle::check(&program.files);
        assert_no_errors(&program, &diagnostics);
    }
    let missing: Vec<_> = constructs.iter().filter(|c| !seen.contains(*c)).collect();
    assert!(
        missing.is_empty(),
        "the typed generator never writes {missing:?}"
    );
}

#[test]
fn edge_case_projects_keep_every_invariant() {
    let cases: Vec<Vec<(String, String)>> = vec![
        vec![],
        vec![("k.toml".to_owned(), String::new())],
        vec![("k.toml".to_owned(), "[[[".to_owned())],
        loose("", 0),
        loose("\u{feff}", 0),
        loose("public struct Case {\n    let a: Case\n}\n", 0),
        loose("import fz.Case\n", 0),
        loose("let a = a\n", 0),
        loose("func f() -> Int {\n    return f()\n}\n", 0),
        loose("enum E {\n    case a(E)\n}\n", 0),
        loose("struct S {\n    let s: S?\n}\n", 0),
        loose("let x = 1 +\n", 1),
        loose("public app A {}\n", 2),
        loose("let s = \"{\"{\"{1}\"}\"}\"\n", 0),
    ];
    for files in &cases {
        let diagnostics = soracle::check(files);
        soracle::check_deterministic(files, &diagnostics);
        let perm: Vec<usize> = (0..files.len()).rev().collect();
        soracle::check_order_independent(files, &perm, &diagnostics);
    }
}
