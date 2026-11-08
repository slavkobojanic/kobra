//! Performance budget check for code generation (`docs/compiler/codegen.md`
//! §20, `docs/compiler/architecture.md` §12).
//!
//! The inputs are the generated realistic projects of
//! `crates/k_sema/benches/project/` (shared with the semantic-analysis
//! benchmarks), so the numbers of `k check` and of code generation are
//! measured over the same text. The tests are `#[ignore]`d because they
//! only make sense in an optimized build on a quiet machine; run them with
//!
//! ```sh
//! scripts/bench-check.sh --codegen
//! # or, equivalently:
//! cargo test --release -p k_codegen --test perf_budget -- --ignored --nocapture --test-threads=1
//! ```
//!
//! # Method
//!
//! As in `k_sema`'s budget check, each measurement repeats the operation
//! (after a warm-up) until it has a minimum number of runs and a minimum
//! total time, and keeps the **fastest** run.
//!
//! * **Lowering + printing:** [`generate`] on an [`Analysis`] whose
//!   diagnostics and program view are already computed (the first call
//!   computes them), so only lowering, printing and bundling are timed.
//!   The budget is single-threaded and code generation is single-threaded
//!   today, so the measured number is the single-threaded one.
//! * **Lockfile checks:** [`project_diagnostics`] on an [`Analysis`] whose
//!   diagnostics and program view are already computed: what `k check`
//!   adds to the semantic pass (the route table against the project's
//!   `routes.lock`, the persistence model and the migrations' replay).
//! * **Cold build:** a new [`AnalysisHost`] over the whole project, then
//!   [`generate`]: parsing, every semantic stage, lowering and printing
//!   (`k build` without file I/O and without writing `dist/`).
//!
//! # Checks
//!
//! | Check | Requirement | Source |
//! |---|---|---|
//! | lowering + printing, realistic 100k lines | ≥ 200,000 lines/s | codegen.md §20 |
//! | cold build (check + codegen), realistic 100k lines, every core up to 8 | ≤ 2.5 s, codegen ≤ 0.6 s of it | architecture.md §12, codegen.md §20 |
//! | lowering + printing, 10k → 100k lines | time ratio ≤ 14 (linear is 10) | codegen.md §20 |
//! | lockfile checks (`project_diagnostics`), realistic 100k lines | ≤ 100 ms (a tenth of the cold check) | performance.md §10 |
//!
//! `K_BUDGET_SCALE` (in `(0, 1]`, default 1) is the fraction of the budget
//! to enforce: time budgets are divided by it and throughput budgets
//! multiplied by it. The linearity ratio is never scaled.

#![allow(clippy::unwrap_used, clippy::panic)]

#[path = "../../k_sema/benches/project/mod.rs"]
mod project;

use std::hint::black_box;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use k_codegen::{
    Entry, Environment, Options, Output, Profile, generate, project_diagnostics, routes_lock,
};
use k_sema::{Analysis, AnalysisHost, Options as SemaOptions, ProjectPath};
use project::{File, SEED};

/// Lowering + printing, single-threaded, lines of K per second.
const LINES_PER_SECOND: f64 = 200_000.0;
/// Cold build of 100k lines, seconds.
const COLD_BUILD_S: f64 = 2.5;
/// The code generation part of a cold build of 100k lines, seconds.
const COLD_CODEGEN_S: f64 = 0.6;
/// The lockfile checks of `k check` on 100k lines, milliseconds.
const LOCKFILE_CHECKS_MS: f64 = 100.0;
/// Maximum time ratio for a 10x larger project.
const LINEAR_10X_RATIO: f64 = 14.0;
/// The thread count of the cold-build budget.
const BUDGET_THREADS: usize = 8;

/// The fraction of the budget to enforce (`K_BUDGET_SCALE`, default 1).
fn scale() -> f64 {
    match std::env::var("K_BUDGET_SCALE") {
        Ok(value) => {
            let scale: f64 = value.parse().unwrap_or_else(|_| {
                panic!("K_BUDGET_SCALE must be a number in (0, 1], got {value:?}")
            });
            assert!(
                scale > 0.0 && scale <= 1.0,
                "K_BUDGET_SCALE must be in (0, 1]"
            );
            scale
        }
        Err(_) => 1.0,
    }
}

/// Fails unless this is an optimized build: debug timings are meaningless.
fn require_release() {
    if cfg!(debug_assertions) {
        panic!(
            "the performance budget check must run in an optimized build: \
             cargo test --release -p k_codegen --test perf_budget -- --ignored"
        );
    }
}

/// Runs `setup` then `f` until `f` has run at least `min_runs` times and
/// for at least `min_time` in total (after one warm-up run), and returns
/// the fastest run of `f`. Only `f` is timed.
fn fastest<S, R>(
    min_runs: usize,
    min_time: Duration,
    mut setup: impl FnMut() -> S,
    mut f: impl FnMut(S) -> R,
) -> Duration {
    black_box(f(setup()));
    let mut best = Duration::MAX;
    let mut runs = 0;
    let mut total = Duration::ZERO;
    while runs < min_runs || total < min_time {
        let input = setup();
        let start = Instant::now();
        let result = f(input);
        let elapsed = start.elapsed();
        drop(black_box(result));
        best = best.min(elapsed);
        total += elapsed;
        runs += 1;
    }
    best
}

/// The generated project's files as the input of an `AnalysisHost`.
///
/// # Panics
///
/// If a generated path is not a valid project path.
fn paths(files: &[File]) -> Vec<(ProjectPath, String)> {
    files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).unwrap(), t.clone()))
        .collect()
}

/// The options of `k build` (a release build of an app).
fn release() -> Options {
    Options {
        profile: Profile::Release,
        environment: Environment::Production,
        entry: Entry::App,
    }
}

/// Generates `analysis`, failing the test on any error.
fn emit(analysis: &Analysis) -> Output {
    generate(analysis, &release()).unwrap_or_else(|e| panic!("generate: {e}"))
}

/// A checked host of the realistic project of `lines` lines.
fn checked(lines: usize, options: SemaOptions) -> (AnalysisHost, usize) {
    let files = project::realistic(lines, SEED);
    let counted = project::project_lines(&files);
    let host = AnalysisHost::with_files(options, paths(&files));
    let diagnostics = host.analysis().diagnostics().unwrap();
    assert!(diagnostics.is_empty(), "{} diagnostics", diagnostics.len());
    (host, counted)
}

/// The fastest lowering + printing of the checked realistic project of
/// `lines` lines, and its line count.
fn codegen_time(lines: usize) -> (Duration, usize) {
    let (host, counted) = checked(lines, SemaOptions::new());
    let analysis = host.analysis();
    let best = fastest(3, Duration::from_millis(800), || (), |()| emit(&analysis));
    (best, counted)
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn lowering_and_printing_a_100k_line_project() {
    require_release();
    let (best, lines) = codegen_time(100_000);
    let rate = lines as f64 / best.as_secs_f64();
    let budget = LINES_PER_SECOND * scale();
    let output = emit(&checked(100_000, SemaOptions::new()).0.analysis());
    let bytes: usize = output
        .client
        .iter()
        .chain(&output.server)
        .map(|f| f.contents.len())
        .sum();
    eprintln!(
        "codegen (lowering + printing), {lines} lines: {:.1} ms, {rate:.0} lines/s, \
         {:.1} MB of JS/CSS in {} files (budget {budget:.0} lines/s)",
        best.as_secs_f64() * 1e3,
        bytes as f64 / 1e6,
        output.client.len() + output.server.len(),
    );
    assert!(
        rate >= budget,
        "codegen ran at {rate:.0} lines/s, under the {budget:.0} lines/s budget"
    );
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn codegen_time_is_linear_in_project_size() {
    require_release();
    let (small, small_lines) = codegen_time(10_000);
    let (large, large_lines) = codegen_time(100_000);
    let ratio = large.as_secs_f64() / small.as_secs_f64();
    eprintln!(
        "codegen {small_lines} lines: {:.1} ms; {large_lines} lines: {:.1} ms; ratio {ratio:.1} \
         (limit {LINEAR_10X_RATIO})",
        small.as_secs_f64() * 1e3,
        large.as_secs_f64() * 1e3,
    );
    assert!(
        ratio <= LINEAR_10X_RATIO,
        "codegen time grew {ratio:.1}x for 10x the lines"
    );
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn cold_build_of_a_100k_line_project() {
    require_release();
    let threads = std::thread::available_parallelism()
        .map_or(1, NonZeroUsize::get)
        .min(BUDGET_THREADS);
    let options = SemaOptions::new().with_threads(NonZeroUsize::new(threads).unwrap());
    let files = paths(&project::realistic(100_000, SEED));
    let mut codegen = Duration::MAX;
    let best = fastest(
        3,
        Duration::from_millis(500),
        || files.clone(),
        |files| {
            let host = AnalysisHost::with_files(options.clone(), files);
            let analysis = host.analysis();
            assert!(analysis.diagnostics().unwrap().is_empty());
            let t = Instant::now();
            let out = emit(&analysis);
            codegen = codegen.min(t.elapsed());
            (host, out)
        },
    );
    let budget = COLD_BUILD_S / scale();
    let codegen_budget = COLD_CODEGEN_S / scale();
    eprintln!(
        "cold build (check + codegen), 100k lines, {threads} threads: {:.0} ms, codegen {:.0} ms \
         (budget {:.0} ms, codegen {:.0} ms)",
        best.as_secs_f64() * 1e3,
        codegen.as_secs_f64() * 1e3,
        budget * 1e3,
        codegen_budget * 1e3,
    );
    assert!(best.as_secs_f64() <= budget, "cold build over budget");
    assert!(
        codegen.as_secs_f64() <= codegen_budget,
        "codegen part of the cold build over budget"
    );
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn lockfile_checks_of_a_100k_line_project() {
    require_release();
    let (first, lines) = checked(100_000, SemaOptions::new().with_threads(NonZeroUsize::MIN));
    // With the `routes.lock` `k build` writes, so every recorded page is
    // compared.
    let lock = routes_lock(&first.analysis())
        .unwrap()
        .expect("the project has pages");
    let mut files = paths(&project::realistic(100_000, SEED));
    files.push((ProjectPath::new("routes.lock").unwrap(), lock));
    let host = AnalysisHost::with_files(SemaOptions::new().with_threads(NonZeroUsize::MIN), files);
    let analysis = host.analysis();
    assert!(analysis.diagnostics().unwrap().is_empty());
    let best = fastest(
        5,
        Duration::from_millis(500),
        || (),
        |()| project_diagnostics(&analysis),
    );
    let found = project_diagnostics(&analysis);
    assert!(found.is_empty(), "{} lockfile diagnostics", found.len());
    let ms = best.as_secs_f64() * 1000.0;
    let budget = LOCKFILE_CHECKS_MS / scale();
    println!("lockfile checks, {lines} lines: {ms:.1} ms (budget {budget:.0} ms)");
    assert!(ms <= budget, "lockfile checks over budget");
}
