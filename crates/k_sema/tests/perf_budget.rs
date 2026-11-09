//! Performance budget check for semantic analysis (`k check` without the
//! driver's file I/O).
//!
//! These tests fail when analysing a generated project is slower, or takes
//! more memory, than the budgets in `docs/compiler/architecture.md`,
//! "Performance budgets", or when the time to check a project grows faster
//! than its size. They are `#[ignore]`d because they only make sense in an
//! optimized build on a quiet machine; run them with
//!
//! ```sh
//! scripts/bench-check.sh
//! # or, equivalently:
//! cargo test --release -p k_sema --test it -- perf_budget:: --ignored --nocapture --test-threads=1
//! ```
//!
//! # Method
//!
//! As in `k_syntax`'s budget check: each measurement runs the operation
//! repeatedly (after a warm-up) until it has both a minimum number of runs
//! and a minimum total time, and keeps the **fastest** run. Background load
//! only ever makes a run slower, so the best run is the most stable
//! statistic. Results are dropped outside the timed region.
//!
//! * **Cold check:** a new [`AnalysisHost`] over every file of the project
//!   and [`Analysis::diagnostics`](k_sema::Analysis::diagnostics): parsing
//!   and every semantic stage. The embedded standard library is parsed once
//!   per process (it never changes, sema.md §3.1), so it is warmed up first.
//! * **Warm re-check:** a host that has already answered `diagnostics`, then
//!   one [`Change`] to one file and `diagnostics` again; the change and the
//!   re-check are timed together. Consecutive runs alternate between two
//!   versions of the edit so that every run is a real change.
//! * **Memory:** the resident set of this process while a warm host of the
//!   100k-line project is alive, measured in a test of its own
//!   (`memory_of_a_warm_100k_line_project`), which `bench-check.sh` runs in
//!   a separate process so that no other test's freed memory is counted.
//!
//! # Checks
//!
//! | Check | Requirement | Source |
//! |---|---|---|
//! | cold check, realistic 100k lines, 8 threads (or every core, if fewer) | ≤ 1.0 s | architecture.md §12 |
//! | cold check, realistic 100k lines, 1 thread | ≤ 4.0 s | architecture.md §12 |
//! | warm re-check after a function body edit, 100k lines | ≤ 30 ms | architecture.md §12 |
//! | warm re-check after a public signature edit, 100k lines | ≤ 150 ms | architecture.md §12 |
//! | resident memory with a warm 100k-line project | ≤ 1.5 GB | architecture.md §12 |
//! | realistic project, 10k → 100k lines | time ratio ≤ 14 (linear is 10) | sema.md §15 |
//! | every pathological family but `deep-generics`, size s → 4s | time ratio ≤ 6 (linear is 4) | sema.md §15 "no quadratic algorithms" |
//! | `deep-generics`, depth d → 4d | time ratio ≤ 20 (quadratic is 16: every expression's type is recorded, and its size is its depth) | sema.md §15 |
//!
//! # Slower machines
//!
//! `K_BUDGET_SCALE` (in `(0, 1]`, default 1) is the fraction of the budget
//! to enforce: time budgets are divided by it. The memory budget and the
//! linearity ratios are never scaled; they do not depend on the speed of
//! the machine.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stdout)]

use crate::project;

use std::hint::black_box;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use k_sema::{AnalysisHost, Change, Options, ProjectPath};
use project::{File, Pathological, SEED};

/// Cold check of 100k lines on 8 cores, seconds.
const COLD_PARALLEL_S: f64 = 1.0;
/// Cold check of 100k lines on 1 core, seconds.
const COLD_SERIAL_S: f64 = 4.0;
/// Warm re-check after a body edit, milliseconds.
const WARM_BODY_MS: f64 = 30.0;
/// Warm re-check after a public signature edit, milliseconds.
const WARM_SIGNATURE_MS: f64 = 150.0;
/// Resident memory of a warm 100k-line project, bytes (1.5 GB).
const WARM_MEMORY_BYTES: f64 = 1.5e9;
/// Maximum time ratio for a 10x larger realistic project.
const REALISTIC_10X_RATIO: f64 = 14.0;
/// Maximum time ratio for 4x larger pathological input.
const PATHOLOGICAL_4X_RATIO: f64 = 6.0;
/// Maximum time ratio for 4x deeper generic nesting. The checker records
/// the type of every expression (for `type_at`), and in a chain nested `d`
/// deep the type at level `k` has size `k`, so resolving and storing them
/// is Θ(d²) however the checker works: quadratic is 16. Before the
/// optimizations recorded in performance.md it was cubic (x44).
const DEEP_GENERICS_4X_RATIO: f64 = 20.0;
/// The thread count of the parallel cold-check budget.
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
             cargo test --release -p k_sema --test it -- perf_budget:: --ignored"
        );
    }
}

/// Runs `setup` then `f` until `f` has run at least `min_runs` times and
/// for at least `min_time` in total (after one warm-up run), and returns
/// the fastest run of `f`. Only `f` is timed; its result is dropped
/// outside the timed region.
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

fn paths(files: &[File]) -> Vec<(ProjectPath, String)> {
    files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).unwrap(), t.clone()))
        .collect()
}

fn threads(n: usize) -> Options {
    Options::new().with_threads(NonZeroUsize::new(n).unwrap())
}

/// The thread count for the parallel budget: 8, or every core if fewer.
fn budget_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, NonZeroUsize::get)
        .min(BUDGET_THREADS)
}

/// A cold check: builds a host over `files` and asks for every diagnostic.
/// Returns the host so its teardown is not timed.
fn cold(options: &Options, files: &[(ProjectPath, String)]) -> AnalysisHost {
    let host = AnalysisHost::with_files(options.clone(), files.iter().cloned());
    let diagnostics = host.analysis().diagnostics().unwrap();
    assert!(diagnostics.is_empty(), "{} diagnostics", diagnostics.len());
    host
}

/// The fastest cold check of `files` on `threads` threads, in seconds.
fn cold_seconds(files: &[File], threads_: usize, min_runs: usize) -> f64 {
    let files = paths(files);
    let options = threads(threads_);
    // Parses the embedded stdlib once per process.
    drop(cold(&options, &files[..1]));
    fastest(
        min_runs,
        Duration::from_millis(300),
        || files.clone(),
        |files| cold(&options, &files),
    )
    .as_secs_f64()
}

/// The fastest warm re-check after `edit` of the edited service of a
/// realistic project of `lines` lines, in milliseconds.
fn warm_ms(lines: usize, edit: fn(&str, usize) -> String) -> f64 {
    let files = project::realistic(lines, SEED);
    let path = project::edited_service_path();
    let original = files.iter().find(|(p, _)| *p == path).unwrap().1.clone();
    let path = ProjectPath::new(&path).unwrap();
    let mut host = cold(&Options::new(), &paths(&files));
    let mut generation = 0;
    let best = fastest(
        10,
        Duration::from_millis(500),
        || {
            generation += 1;
            let mut change = Change::new();
            change.set_file(path.clone(), edit(&original, generation % 2));
            change
        },
        |change| {
            host.apply_change(change);
            let diagnostics = host.analysis().diagnostics().unwrap();
            assert!(diagnostics.is_empty());
            diagnostics
        },
    );
    best.as_secs_f64() * 1e3
}

fn report(name: &str, measured: f64, budget: f64, unit: &str, lower_is_better: bool) -> bool {
    let ok = if lower_is_better {
        measured <= budget
    } else {
        measured >= budget
    };
    let rel = if lower_is_better { "<=" } else { ">=" };
    println!(
        "{} {name}: {measured:.3} {unit} (budget {rel} {budget:.3} {unit})",
        if ok { "ok  " } else { "MISS" }
    );
    ok
}

#[test]
#[ignore = "performance budget: run with --release --ignored (scripts/bench-check.sh)"]
fn cold_check_of_a_100k_line_project_on_8_cores() {
    require_release();
    let files = project::realistic(100_000, SEED);
    let threads = budget_threads();
    let seconds = cold_seconds(&files, threads, 3);
    let budget = COLD_PARALLEL_S / scale();
    let ok = report(
        &format!(
            "cold check, 100k lines ({:.2} MB, {} files), {threads} threads",
            project::project_bytes(&files) as f64 / 1e6,
            files.len()
        ),
        seconds,
        budget,
        "s",
        true,
    );
    assert!(
        ok,
        "cold check on {threads} threads: {seconds:.3} s > {budget:.3} s"
    );
}

#[test]
#[ignore = "performance budget: run with --release --ignored (scripts/bench-check.sh)"]
fn cold_check_of_a_100k_line_project_on_1_core() {
    require_release();
    let files = project::realistic(100_000, SEED);
    let seconds = cold_seconds(&files, 1, 2);
    let budget = COLD_SERIAL_S / scale();
    let ok = report(
        "cold check, 100k lines, 1 thread",
        seconds,
        budget,
        "s",
        true,
    );
    assert!(ok, "cold check on 1 thread: {seconds:.3} s > {budget:.3} s");
}

#[test]
#[ignore = "performance budget: run with --release --ignored (scripts/bench-check.sh)"]
fn warm_recheck_after_a_body_edit() {
    require_release();
    let ms = warm_ms(100_000, project::body_edit);
    let budget = WARM_BODY_MS / scale();
    let ok = report(
        "warm re-check after a body edit, 100k lines",
        ms,
        budget,
        "ms",
        true,
    );
    assert!(ok, "warm body edit: {ms:.1} ms > {budget:.1} ms");
}

#[test]
#[ignore = "performance budget: run with --release --ignored (scripts/bench-check.sh)"]
fn warm_recheck_after_a_signature_edit() {
    require_release();
    let ms = warm_ms(100_000, project::signature_edit);
    let budget = WARM_SIGNATURE_MS / scale();
    let ok = report(
        "warm re-check after a signature edit, 100k lines",
        ms,
        budget,
        "ms",
        true,
    );
    assert!(ok, "warm signature edit: {ms:.1} ms > {budget:.1} ms");
}

#[test]
#[ignore = "performance budget: run with --release --ignored (scripts/bench-check.sh)"]
fn realistic_check_time_is_linear() {
    require_release();
    let small = project::realistic(10_000, SEED);
    let large = project::realistic(100_000, SEED);
    let t_small = cold_seconds(&small, 1, 5);
    let t_large = cold_seconds(&large, 1, 2);
    let ratio = t_large / t_small;
    let size_ratio = project::project_bytes(&large) as f64 / project::project_bytes(&small) as f64;
    let ok = report(
        &format!("linearity, realistic 10k -> 100k (x{size_ratio:.2} bytes), time ratio"),
        ratio,
        REALISTIC_10X_RATIO,
        "x",
        true,
    );
    assert!(ok, "10x project took x{ratio:.2} the time");
}

#[test]
#[ignore = "performance budget: run with --release --ignored (scripts/bench-check.sh)"]
fn pathological_check_time_is_linear() {
    require_release();
    let mut failures = Vec::new();
    for family in Pathological::ALL {
        let size = family.default_size();
        let small = family.generate(size);
        let large = family.generate(size * 4);
        let t_small = cold_seconds(&small, 1, 5);
        let t_large = cold_seconds(&large, 1, 3);
        let ratio = t_large / t_small;
        println!(
            "     {}: size {size} {:.2} ms ({:.0} KB), size {} {:.2} ms",
            family.name(),
            t_small * 1e3,
            project::project_bytes(&small) as f64 / 1e3,
            size * 4,
            t_large * 1e3
        );
        let limit = if family == Pathological::DeepGenerics {
            DEEP_GENERICS_4X_RATIO
        } else {
            PATHOLOGICAL_4X_RATIO
        };
        if !report(
            &format!("linearity, {} s -> 4s, time ratio", family.name()),
            ratio,
            limit,
            "x",
            true,
        ) {
            failures.push(family.name());
        }
    }
    assert!(failures.is_empty(), "superlinear: {failures:?}");
}

/// The resident set size of this process, in bytes, or `None` where it
/// can't be read without `unsafe` (the crate forbids it).
fn resident_bytes() -> Option<f64> {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
        let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
        return Some(kb * 1024.0);
    }
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    let kb: f64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    Some(kb * 1024.0)
}

#[test]
#[ignore = "performance budget: run alone with --release --ignored (scripts/bench-check.sh)"]
fn memory_of_a_warm_100k_line_project() {
    require_release();
    let files = project::realistic(100_000, SEED);
    let path = project::edited_service_path();
    let original = files.iter().find(|(p, _)| *p == path).unwrap().1.clone();
    let before = resident_bytes().expect("cannot read the resident set size");
    let mut host = cold(&Options::new(), &paths(&files));
    drop(files);
    // A warm host: a few revisions after edits, with a live snapshot.
    for generation in 0..4 {
        let mut change = Change::new();
        change.set_file(
            ProjectPath::new(&path).unwrap(),
            project::body_edit(&original, generation),
        );
        host.apply_change(change);
        assert!(host.analysis().diagnostics().unwrap().is_empty());
    }
    let analysis = host.analysis();
    assert!(analysis.diagnostics().unwrap().is_empty());
    let after = resident_bytes().expect("cannot read the resident set size");
    black_box(&analysis);
    let ok = report(
        &format!(
            "resident memory, warm 100k-line project (process before: {:.0} MB)",
            before / 1e6
        ),
        after / 1e6,
        WARM_MEMORY_BYTES / 1e6,
        "MB",
        true,
    );
    assert!(ok, "warm memory {after:.0} B > {WARM_MEMORY_BYTES:.0} B");
}
