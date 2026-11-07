//! Criterion benchmarks of semantic analysis over generated projects.
//!
//! The inputs come from `project/mod.rs` (shared with the budget check in
//! `tests/perf_budget.rs`, which enforces the budgets of
//! `docs/compiler/architecture.md` §12). The groups:
//!
//! * `cold_1t/realistic/{1k,10k,100k}` and `cold_mt/realistic/...`: a new
//!   host over the whole project and `diagnostics()`, on one thread and on
//!   every core. The timed region includes parsing.
//! * `warm_body/realistic/{10k,100k}` and `warm_signature/realistic/...`:
//!   one change to one file of a host that has already been checked, then
//!   `diagnostics()` again.
//! * `pathological/<family>`: a cold check, on one thread, of every
//!   pathological family at its default size.
//!
//! Run with `cargo bench -p k_sema`; see `docs/compiler/performance.md` for
//! the baselines and how to profile.

#![allow(missing_docs, clippy::unwrap_used)]

#[path = "project/mod.rs"]
mod project;

use std::hint::black_box;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use k_sema::{AnalysisHost, Change, Options, ProjectPath};
use project::{File, Pathological, SEED};

/// Converts generated files to the host's input form.
///
/// # Panics
///
/// Panics if a generated path is not a valid project path (a generator
/// bug).
fn paths(files: &[File]) -> Vec<(ProjectPath, String)> {
    files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).unwrap(), t.clone()))
        .collect()
}

/// Returns analysis options with exactly `threads` worker threads, or one
/// per core for `None`.
///
/// # Panics
///
/// Panics if `threads` is `Some(0)`.
fn options(threads: Option<usize>) -> Options {
    match threads {
        Some(n) => Options::new().with_threads(NonZeroUsize::new(n).unwrap()),
        None => Options::new(),
    }
}

/// Analyses `files` from scratch (parse and semantic pass) and returns the
/// host, so later edits measure warm re-checks.
///
/// # Panics
///
/// Panics if the analysis is cancelled, which a fresh host never is.
fn cold(options: &Options, files: Vec<(ProjectPath, String)>) -> AnalysisHost {
    let host = AnalysisHost::with_files(options.clone(), files);
    black_box(host.analysis().diagnostics().unwrap());
    host
}

/// Renders a project size as a benchmark label (`10k`).
fn label(lines: usize) -> String {
    format!("{}k", lines / 1_000)
}

/// Benchmarks cold analyses of realistic projects of 1k, 10k and 100k
/// lines, on one thread and on every core.
fn bench_cold(c: &mut Criterion) {
    for (group_name, threads) in [("cold_1t", Some(1)), ("cold_mt", None)] {
        let mut group = c.benchmark_group(group_name);
        for lines in [1_000, 10_000, 100_000] {
            let files = paths(&project::realistic(lines, SEED));
            let options = options(threads);
            // Parse the embedded stdlib once per process before measuring.
            drop(cold(&options, files[..1].to_vec()));
            group.throughput(Throughput::Elements(lines as u64));
            if lines >= 100_000 {
                group.sample_size(10);
            }
            group.bench_function(format!("realistic/{}", label(lines)), |b| {
                b.iter_batched(
                    || files.clone(),
                    |files| cold(&options, files),
                    BatchSize::PerIteration,
                );
            });
        }
        group.finish();
    }
}

/// An edit of the benchmarked file: `(text, generation) -> new text`.
type Edit = fn(&str, usize) -> String;

/// Benchmarks re-checks after a body edit and after a signature edit of
/// one service in realistic projects of 10k and 100k lines.
fn bench_warm(c: &mut Criterion) {
    let edits: [(&str, Edit); 2] = [
        ("warm_body", project::body_edit),
        ("warm_signature", project::signature_edit),
    ];
    for (group_name, edit) in edits {
        let mut group = c.benchmark_group(group_name);
        for lines in [10_000, 100_000] {
            let files = project::realistic(lines, SEED);
            let path = project::edited_service_path();
            let original = files.iter().find(|(p, _)| *p == path).unwrap().1.clone();
            let path = ProjectPath::new(&path).unwrap();
            let mut host = cold(&Options::new(), paths(&files));
            let mut generation = 0;
            group.bench_function(format!("realistic/{}", label(lines)), |b| {
                b.iter_custom(|iterations| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iterations {
                        generation += 1;
                        let mut change = Change::new();
                        change.set_file(path.clone(), edit(&original, generation % 2));
                        let start = Instant::now();
                        host.apply_change(change);
                        black_box(host.analysis().diagnostics().unwrap());
                        total += start.elapsed();
                    }
                    total
                });
            });
        }
        group.finish();
    }
}

/// Benchmarks a cold single-threaded analysis of each pathological family
/// at its default size.
fn bench_pathological(c: &mut Criterion) {
    let mut group = c.benchmark_group("pathological");
    group.sample_size(10);
    let options = options(Some(1));
    for family in Pathological::ALL {
        let files = paths(&family.generate(family.default_size()));
        drop(cold(&options, files[..1].to_vec()));
        group.throughput(Throughput::Bytes(
            files.iter().map(|(_, t)| t.len() as u64).sum(),
        ));
        group.bench_function(family.name(), |b| {
            b.iter_batched(
                || files.clone(),
                |files| cold(&options, files),
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_cold, bench_warm, bench_pathological);
criterion_main!(benches);
