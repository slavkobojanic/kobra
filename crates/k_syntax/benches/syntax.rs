//! Throughput benchmarks for the lexer and the parser.
//!
//! All input comes from the deterministic generators in
//! [`workload`](workload/mod.rs), so the numbers are reproducible and compare
//! directly with the budgets in `docs/compiler/architecture.md`,
//! "Performance budgets":
//!
//! * lexing: at least 150 MB/s;
//! * lexing plus parsing into a green tree: at least 25 MB/s, and a typical
//!   300-line file in at most 0.5 ms.
//!
//! Every benchmark reports both lines per second and bytes per second.
//!
//! # Groups
//!
//! | Group | Measures |
//! |---|---|
//! | `lex` | [`k_syntax::lex`] on realistic input of 1k, 10k and 100k lines |
//! | `parse` | [`k_syntax::parse`] (lexing included) on the same input |
//! | `parse_only` | [`k_syntax::parser::parse_lexed`] on pre-lexed 10k lines, isolating the parser and the tree sink |
//! | `typical_file` | lexing and parsing one realistic 300-line file |
//! | `pathological_lex` / `pathological_parse` | each [`Pathological`] family at about 1 MB |
//!
//! The pass/fail check against the budgets is `tests/perf_budget.rs` (run
//! by `scripts/bench-check.sh`); these benchmarks are for measuring and
//! comparing changes. Baselines are recorded in
//! `docs/compiler/performance.md`.
//!
//! # Running
//!
//! ```sh
//! cargo bench -p k_syntax                              # everything
//! cargo bench -p k_syntax -- parse/                    # one group (a regex filter)
//! cargo bench -p k_syntax -- --save-baseline main      # record a baseline
//! cargo bench -p k_syntax -- --baseline main           # compare a change against it
//! ```

#![allow(missing_docs)]

mod workload;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use k_diagnostics::FileId;
use std::hint::black_box;
use std::time::Duration;
use workload::{Pathological, SEED, line_count, realistic, typical_file};

const FILE: FileId = FileId::new(0);

/// Realistic input sizes, in lines.
const SIZES: [(usize, &str); 3] = [(1_000, "1k"), (10_000, "10k"), (100_000, "100k")];

/// Both throughputs criterion can report for `text`: lines and bytes.
fn throughput(text: &str) -> Throughput {
    Throughput::ElementsAndBytes {
        elements: line_count(text) as u64,
        bytes: text.len() as u64,
    }
}

fn bench_realistic(c: &mut Criterion) {
    let inputs: Vec<(&str, String)> = SIZES
        .iter()
        .map(|&(lines, name)| (name, realistic(lines, SEED)))
        .collect();

    let mut lex = c.benchmark_group("lex");
    for (name, text) in &inputs {
        lex.throughput(throughput(text));
        if text.len() > 1 << 20 {
            lex.sample_size(20);
        }
        lex.bench_with_input(BenchmarkId::new("realistic", name), text, |b, text| {
            b.iter(|| black_box(k_syntax::lex(FILE, black_box(text)).len()));
        });
    }
    lex.finish();

    let mut parse = c.benchmark_group("parse");
    for (name, text) in &inputs {
        parse.throughput(throughput(text));
        if text.len() > 1 << 20 {
            parse
                .sample_size(10)
                .measurement_time(Duration::from_secs(8));
        }
        parse.bench_with_input(BenchmarkId::new("realistic", name), text, |b, text| {
            b.iter(|| black_box(k_syntax::parse(FILE, black_box(text))));
        });
    }
    parse.finish();

    let mut parse_only = c.benchmark_group("parse_only");
    let text = &inputs[1].1;
    let lexed = k_syntax::lex(FILE, text);
    parse_only.throughput(throughput(text));
    parse_only.bench_function("realistic/10k", |b| {
        b.iter(|| black_box(k_syntax::parser::parse_lexed(black_box(&lexed), FILE)));
    });
    parse_only.finish();
}

fn bench_typical_file(c: &mut Criterion) {
    let text = typical_file(SEED);
    let mut group = c.benchmark_group("typical_file");
    group.throughput(throughput(&text));
    group.bench_function("lex", |b| {
        b.iter(|| black_box(k_syntax::lex(FILE, black_box(&text)).len()));
    });
    group.bench_function("parse", |b| {
        b.iter(|| black_box(k_syntax::parse(FILE, black_box(&text))));
    });
    group.finish();
}

fn bench_pathological(c: &mut Criterion) {
    let inputs: Vec<(Pathological, String)> = Pathological::ALL
        .iter()
        .map(|&f| (f, f.generate(f.megabyte_size())))
        .collect();

    let mut lex = c.benchmark_group("pathological_lex");
    lex.sample_size(20);
    for (family, text) in &inputs {
        lex.throughput(throughput(text));
        lex.bench_with_input(
            BenchmarkId::from_parameter(family.name()),
            text,
            |b, text| {
                b.iter(|| black_box(k_syntax::lex(FILE, black_box(text)).len()));
            },
        );
    }
    lex.finish();

    let mut parse = c.benchmark_group("pathological_parse");
    parse
        .sample_size(10)
        .measurement_time(Duration::from_secs(4));
    for (family, text) in &inputs {
        parse.throughput(throughput(text));
        parse.bench_with_input(
            BenchmarkId::from_parameter(family.name()),
            text,
            |b, text| {
                b.iter(|| black_box(k_syntax::parse(FILE, black_box(text))));
            },
        );
    }
    parse.finish();
}

criterion_group!(
    benches,
    bench_realistic,
    bench_typical_file,
    bench_pathological
);
criterion_main!(benches);
