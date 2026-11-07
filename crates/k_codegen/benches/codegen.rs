//! Criterion benchmarks of code generation over the generated projects of
//! `crates/k_sema/benches/project/` (shared with the semantic-analysis
//! benchmarks and with the budget check in `tests/perf_budget.rs`, which
//! enforces the budgets of `docs/compiler/codegen.md` §20).
//!
//! The groups:
//!
//! * `emit/realistic/{1k,10k,100k}`: [`generate`] on an analysis whose
//!   diagnostics and program view are already computed, so only lowering,
//!   printing and bundling are timed. Throughput is in lines of K.
//! * `build/realistic/{10k,100k}`: a cold build on every core: a new host
//!   over the whole project, checking and [`generate`] (`k build` without
//!   file I/O).
//!
//! Run with `cargo bench -p k_codegen`; the baselines are in
//! `docs/compiler/performance.md` §11.

#![allow(missing_docs, clippy::unwrap_used)]

#[path = "../../k_sema/benches/project/mod.rs"]
mod project;

use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use k_codegen::{Entry, Environment, Options, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use project::{File, SEED};

fn paths(files: &[File]) -> Vec<(ProjectPath, String)> {
    files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).unwrap(), t.clone()))
        .collect()
}

/// The options of `k build`.
fn release() -> Options {
    Options {
        profile: Profile::Release,
        environment: Environment::Production,
        entry: Entry::App,
    }
}

fn label(lines: usize) -> String {
    format!("{}k", lines / 1_000)
}

fn bench_emit(c: &mut Criterion) {
    let mut group = c.benchmark_group("emit");
    for lines in [1_000, 10_000, 100_000] {
        let files = project::realistic(lines, SEED);
        let counted = project::project_lines(&files);
        let host = AnalysisHost::with_files(SemaOptions::new(), paths(&files));
        let analysis = host.analysis();
        assert!(analysis.diagnostics().unwrap().is_empty());
        // Computes the program view and the point-query index.
        black_box(generate(&analysis, &release()).unwrap());
        group.throughput(Throughput::Elements(counted as u64));
        if lines >= 100_000 {
            group.sample_size(10);
        }
        group.bench_function(format!("realistic/{}", label(lines)), |b| {
            b.iter(|| generate(&analysis, &release()).unwrap());
        });
    }
    group.finish();
}

fn bench_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("build");
    group.sample_size(10);
    for lines in [10_000, 100_000] {
        let files = paths(&project::realistic(lines, SEED));
        group.throughput(Throughput::Elements(lines as u64));
        group.bench_function(format!("realistic/{}", label(lines)), |b| {
            b.iter_batched(
                || files.clone(),
                |files| {
                    let host = AnalysisHost::with_files(SemaOptions::new(), files);
                    let out = generate(&host.analysis(), &release()).unwrap();
                    (host, out)
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_emit, bench_build);
criterion_main!(benches);
