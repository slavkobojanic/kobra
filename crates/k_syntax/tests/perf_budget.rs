//! Performance budget check for the syntax layer.
//!
//! These tests fail when lexing or parsing is slower than the budgets in
//! `docs/compiler/architecture.md`, "Performance budgets", or when the time
//! to parse an input grows faster than its size. They are `#[ignore]`d
//! because they only make sense in an optimized build on a quiet machine;
//! run them with
//!
//! ```sh
//! scripts/bench-check.sh
//! # or, equivalently:
//! cargo test --release -p k_syntax --test perf_budget -- --ignored --nocapture --test-threads=1
//! ```
//!
//! # Method
//!
//! Each measurement runs the operation repeatedly (after a warm-up) until it
//! has both a minimum number of runs and a minimum total time, and keeps the
//! **fastest** run. The minimum estimates the speed of the code itself:
//! background load and interrupts only ever make a run slower, so the best
//! run is the most stable statistic across noisy machines. The criterion
//! benchmarks in `benches/syntax.rs` report means with confidence intervals
//! for comparing changes; this check answers only "is it within budget?".
//!
//! Throughput is in decimal megabytes (10^6 bytes) per second, the unit of
//! the budget table.
//!
//! # Checks
//!
//! | Check | Requirement | Source |
//! |---|---|---|
//! | lexing realistic input (100k lines) | ≥ 150 MB/s | architecture.md §12 |
//! | lexing + parsing realistic input (100k lines) | ≥ 25 MB/s | architecture.md §12 |
//! | one typical 300-line file | ≤ 0.5 ms | architecture.md §12 |
//! | every pathological family at about 1 MB | ≥ 10 MB/s (lex + parse) | performance.md "Pathological floor" |
//! | realistic input, 10k → 100k lines | time ratio ≤ 14 (linear is 10) | architecture.md §4.2 "Linearity" |
//! | every pathological family, size s → 4s | time ratio ≤ 6 (linear is 4) | architecture.md §4.2 "Linearity" |
//!
//! # Slower machines
//!
//! The budgets are defined for a recent laptop-class CPU. On slower hardware
//! (shared CI runners), set `K_BUDGET_SCALE` to the fraction of the budget to
//! enforce, for example `K_BUDGET_SCALE=0.5` to require half the throughput
//! and allow twice the time. The linearity ratios are never scaled: they do
//! not depend on the machine.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stdout)]

#[path = "../benches/workload/mod.rs"]
mod workload;

use k_diagnostics::FileId;
use std::hint::black_box;
use std::time::{Duration, Instant};
use workload::{Pathological, SEED, realistic, typical_file};

const FILE: FileId = FileId::new(0);

/// Lexing budget, MB/s.
const LEX_MB_PER_S: f64 = 150.0;
/// Lexing + parsing budget, MB/s.
const PARSE_MB_PER_S: f64 = 25.0;
/// Budget for one typical 300-line file, milliseconds.
const TYPICAL_FILE_MS: f64 = 0.5;
/// Minimum lexing + parsing throughput on pathological input, MB/s.
const PATHOLOGICAL_MB_PER_S: f64 = 10.0;
/// Maximum time ratio for 10x more realistic input.
const REALISTIC_10X_RATIO: f64 = 14.0;
/// Maximum time ratio for 4x more pathological input.
const PATHOLOGICAL_4X_RATIO: f64 = 6.0;

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
             cargo test --release -p k_syntax --test perf_budget -- --ignored"
        );
    }
}

/// Runs `f` until it has run at least `min_runs` times and for at least
/// `min_time` in total (after two warm-up runs), and returns the fastest
/// run.
fn fastest<R>(min_runs: usize, min_time: Duration, mut f: impl FnMut() -> R) -> Duration {
    for _ in 0..2 {
        black_box(f());
    }
    let mut best = Duration::MAX;
    let mut runs = 0;
    let started = Instant::now();
    while runs < min_runs || started.elapsed() < min_time {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        // Drop outside the timed region: tree teardown is measured by the
        // benchmarks, but the budget is about producing the tree.
        drop(black_box(result));
        best = best.min(elapsed);
        runs += 1;
    }
    best
}

/// Decimal megabytes per second for `bytes` processed in `time`.
fn mb_per_s(bytes: usize, time: Duration) -> f64 {
    bytes as f64 / 1e6 / time.as_secs_f64()
}

fn lex_time(text: &str, min_runs: usize) -> Duration {
    fastest(min_runs, Duration::from_millis(300), || {
        k_syntax::lex(FILE, black_box(text))
    })
}

fn parse_time(text: &str, min_runs: usize) -> Duration {
    fastest(min_runs, Duration::from_millis(300), || {
        k_syntax::parse(FILE, black_box(text))
    })
}

#[test]
#[ignore = "performance check; run scripts/bench-check.sh"]
fn lexing_meets_the_throughput_budget() {
    require_release();
    let text = realistic(100_000, SEED);
    let time = lex_time(&text, 10);
    let rate = mb_per_s(text.len(), time);
    let budget = LEX_MB_PER_S * scale();
    println!(
        "lex   realistic 100k lines ({:.2} MB): {:.2} ms, {rate:.0} MB/s (budget {budget:.0}), {:.1} M lines/s",
        text.len() as f64 / 1e6,
        time.as_secs_f64() * 1e3,
        100_000.0 / time.as_secs_f64() / 1e6,
    );
    assert!(
        rate >= budget,
        "lexing at {rate:.1} MB/s is below the budget of {budget:.0} MB/s"
    );
}

#[test]
#[ignore = "performance check; run scripts/bench-check.sh"]
fn parsing_meets_the_throughput_budget() {
    require_release();
    let text = realistic(100_000, SEED);
    let time = parse_time(&text, 5);
    let rate = mb_per_s(text.len(), time);
    let budget = PARSE_MB_PER_S * scale();
    println!(
        "parse realistic 100k lines ({:.2} MB): {:.2} ms, {rate:.1} MB/s (budget {budget:.0}), {:.2} M lines/s",
        text.len() as f64 / 1e6,
        time.as_secs_f64() * 1e3,
        100_000.0 / time.as_secs_f64() / 1e6,
    );
    assert!(
        rate >= budget,
        "parsing at {rate:.1} MB/s is below the budget of {budget:.0} MB/s"
    );
}

#[test]
#[ignore = "performance check; run scripts/bench-check.sh"]
fn a_typical_file_parses_within_budget() {
    require_release();
    let budget = TYPICAL_FILE_MS / scale();
    let mut slowest = 0.0f64;
    for seed in 0..8 {
        let text = typical_file(SEED ^ seed);
        let ms = parse_time(&text, 200).as_secs_f64() * 1e3;
        slowest = slowest.max(ms);
    }
    println!(
        "parse typical 300-line file: {slowest:.3} ms worst of 8 files (budget {budget:.2} ms)"
    );
    assert!(
        slowest <= budget,
        "a typical file takes {slowest:.3} ms, over the budget of {budget} ms"
    );
}

#[test]
#[ignore = "performance check; run scripts/bench-check.sh"]
fn pathological_input_stays_above_the_floor() {
    require_release();
    let floor = PATHOLOGICAL_MB_PER_S * scale();
    let mut failures = Vec::new();
    for family in Pathological::ALL {
        let text = family.generate(family.megabyte_size());
        let lex = mb_per_s(text.len(), lex_time(&text, 5));
        let parse = mb_per_s(text.len(), parse_time(&text, 5));
        println!(
            "pathological {:<22} {:>6.2} MB  lex {lex:>7.0} MB/s  parse {parse:>6.1} MB/s",
            family.name(),
            text.len() as f64 / 1e6
        );
        if parse < floor {
            failures.push(format!("{}: {parse:.1} MB/s", family.name()));
        }
    }
    assert!(
        failures.is_empty(),
        "below the pathological floor of {floor:.0} MB/s: {failures:?}"
    );
}

#[test]
#[ignore = "performance check; run scripts/bench-check.sh"]
fn realistic_input_parses_in_linear_time() {
    require_release();
    let small = realistic(10_000, SEED);
    let large = realistic(100_000, SEED);
    let ratio = parse_time(&large, 5).as_secs_f64() / parse_time(&small, 20).as_secs_f64();
    let bytes_ratio = large.len() as f64 / small.len() as f64;
    println!("linearity realistic 10k -> 100k lines: time x{ratio:.2} for bytes x{bytes_ratio:.2}");
    assert!(
        ratio <= REALISTIC_10X_RATIO,
        "10x the input takes {ratio:.1}x the time"
    );
}

#[test]
#[ignore = "performance check; run scripts/bench-check.sh"]
fn pathological_input_parses_in_linear_time() {
    require_release();
    let mut failures = Vec::new();
    for family in Pathological::ALL {
        let size = family.megabyte_size() / 4;
        let small = family.generate(size);
        let large = family.generate(size * 4);
        let ratio = parse_time(&large, 5).as_secs_f64() / parse_time(&small, 10).as_secs_f64();
        println!("linearity {:<22} size x4: time x{ratio:.2}", family.name());
        if ratio > PATHOLOGICAL_4X_RATIO {
            failures.push(format!("{}: x{ratio:.2}", family.name()));
        }
    }
    assert!(failures.is_empty(), "super-linear parse time: {failures:?}");
}
