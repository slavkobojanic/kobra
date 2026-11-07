//! Runs the golden corpus in `tests/corpus`, one test per case.
//!
//! ```text
//! cargo test -p k_corpus --test corpus                    # every case
//! cargo test -p k_corpus --test corpus -- lexer/strings   # cases whose name contains the filter
//! K_BLESS=1 cargo test -p k_corpus --test corpus          # rewrite expectations from actual output
//! ```
//!
//! Layout problems (unknown suites, orphaned or unexpected files) are
//! reported as failing tests named after the offending path. See
//! `docs/compiler/testing.md`.

use k_corpus::{bless_enabled, corpus_root, discover, suites};
use libtest_mimic::{Arguments, Failed, Trial};

/// Discovers the corpus and runs every `lexer`, `parser`, `sema` and
/// `codegen` case, plus one failing trial per layout problem, through
/// `libtest-mimic`; `run` and `db` cases are skipped here (see the
/// module documentation).
fn main() {
    let args = Arguments::from_args();
    let bless = bless_enabled();
    let discovery = discover::discover(&corpus_root());
    let mut trials = Vec::with_capacity(discovery.cases.len() + discovery.problems.len());
    for case in discovery.cases {
        // Run cases need V8; `k_server`'s `corpus_run` test executes them.
        // Db cases need the server; its `e2e` test executes them.
        if matches!(case.suite, suites::Suite::Run | suites::Suite::Db) {
            continue;
        }
        let name = case.name.clone();
        trials.push(Trial::test(name, move || {
            suites::run_case(&case, bless).map_err(Failed::from)
        }));
    }
    for problem in discovery.problems {
        let message = problem.message;
        trials.push(Trial::test(format!("layout/{}", problem.name), move || {
            Err(Failed::from(message))
        }));
    }
    libtest_mimic::run(&args, trials).exit();
}
