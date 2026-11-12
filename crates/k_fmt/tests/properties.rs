//! The formatter's properties (`docs/design/56-formatter.md`,
//! "Guarantees"), checked by the oracle (`support/oracle.rs`):
//!
//! | Test | Inputs |
//! |---|---|
//! | `repository_files` | every `.k` file of `tests/corpus`, `examples` and `tests/fmt`, and every checked ` ```k ` block of the book |
//! | `generated_programs` | programs from the structure-aware generator of `k_syntax` (`kgen`), valid and noisy; `K_FMT_CASES=N` sets how many (default 1500) |
//! | `sema_diagnostics` | every `sema` corpus case reports the same diagnostics after formatting, apart from the redundant spellings the rewrites fix |

use std::collections::BTreeMap;

use arbitrary::Unstructured;
use k_sema::{AnalysisHost, Options};

use crate::oracle::{self, Outcome};
use crate::{inputs, kgen};

/// Fails with the first failures, if any.
#[allow(clippy::panic)]
fn report(what: &str, failures: &[String]) {
    if failures.is_empty() {
        return;
    }
    let shown: Vec<&str> = failures.iter().take(12).map(String::as_str).collect();
    panic!(
        "{what} for {} input(s); the first:\n\n{}",
        failures.len(),
        shown.join("\n\n")
    );
}

#[test]
fn repository_files() {
    let mut failures = Vec::new();
    let (mut refused, mut formatted) = (0, 0);
    for (n, input) in inputs::all().into_iter().enumerate() {
        match oracle::check(&input.text, n as u64) {
            Ok(Outcome::Refused) => refused += 1,
            Ok(Outcome::Formatted) => formatted += 1,
            Err(e) => failures.push(format!("{}: {e}", input.name)),
        }
    }
    report("a formatter property failed", &failures);
    assert!(formatted > 3000, "only {formatted} inputs were formatted");
    assert!(refused > 100, "only {refused} inputs had syntax errors");
}

#[test]
fn generated_programs() {
    let cases: u64 = std::env::var("K_FMT_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500);
    let mut failures = Vec::new();
    let mut formatted = 0;
    for seed in 0..cases {
        // Deterministic bytes for the generator: an xorshift stream.
        let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let bytes: Vec<u8> = (0..4096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        let generated = kgen::generate(&mut Unstructured::new(&bytes));
        match oracle::check(&generated.source, seed) {
            Ok(Outcome::Formatted) => formatted += 1,
            Ok(Outcome::Refused) if generated.valid => failures.push(format!(
                "seed {seed}: a valid program was refused:\n{}",
                generated.source
            )),
            Ok(Outcome::Refused) => {}
            Err(e) => failures.push(format!(
                "seed {seed}: {e}\n--- input:\n{}",
                generated.source
            )),
        }
    }
    report(
        "a formatter property failed on a generated program",
        &failures,
    );
    assert!(
        formatted * 3 > cases,
        "only {formatted} of {cases} generated programs were formatted"
    );
}

/// The codes whose diagnostics the rewrites fix: `public` on a member,
/// `public` on an `@Inject` field, and a top-level `case _`.
const FIXED: [&str; 3] = ["K1302", "K1303", "K2303"];

/// `(path, code, message)` of each diagnostic of a project, counted.
fn diagnostics_of(
    files: &[(k_sema::ProjectPath, String)],
) -> BTreeMap<(String, String, String), usize> {
    let host = AnalysisHost::with_files(Options::new(), files.to_vec());
    let analysis = host.analysis();
    let mut out = BTreeMap::new();
    for d in analysis
        .diagnostics()
        .expect("a current snapshot is never cancelled")
    {
        let path = match analysis.origin(d.span.file) {
            Some(k_sema::FileOrigin::Project(p)) => p.as_str().to_owned(),
            Some(k_sema::FileOrigin::Stdlib(p)) => format!("<stdlib>/{p}"),
            None => String::new(),
        };
        *out.entry((path, d.code.to_string(), d.message.clone()))
            .or_insert(0) += 1;
    }
    out
}

#[test]
fn sema_diagnostics() {
    let discovery = k_corpus::discover::discover(&k_corpus::corpus_root());
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in discovery
        .cases
        .iter()
        .filter(|c| c.suite == k_corpus::suites::Suite::Sema)
    {
        let Ok(loaded) = k_corpus::sema_suite::load(case) else {
            continue;
        };
        let mut formatted = Vec::new();
        let mut skip = false;
        for (path, text) in &loaded.files {
            if !path.as_str().ends_with(".k") {
                formatted.push((path.clone(), text.clone()));
                continue;
            }
            match k_fmt::format(text) {
                Ok(f) => formatted.push((path.clone(), f)),
                Err(k_fmt::Error::Syntax(_)) => skip = true,
                Err(e) => {
                    failures.push(format!("{}: {e}", case.source.display()));
                    skip = true;
                }
            }
        }
        if skip {
            continue;
        }
        checked += 1;
        let mut before = diagnostics_of(&loaded.files);
        let mut after = diagnostics_of(&formatted);
        for key in after.keys().filter(|k| FIXED.contains(&k.1.as_str())) {
            let was = before.get(key).copied().unwrap_or(0);
            if after[key] > was {
                failures.push(format!(
                    "{}: formatting added {key:?}",
                    case.source.display()
                ));
            }
        }
        before.retain(|k, _| !FIXED.contains(&k.1.as_str()));
        after.retain(|k, _| !FIXED.contains(&k.1.as_str()));
        if before != after {
            let lost: Vec<_> = before
                .iter()
                .filter(|(k, n)| after.get(*k) != Some(n))
                .collect();
            let added: Vec<_> = after
                .iter()
                .filter(|(k, n)| before.get(*k) != Some(n))
                .collect();
            failures.push(format!(
                "{}: lost {lost:?}, added {added:?}",
                case.source.display()
            ));
        }
    }
    report("formatting changed the semantic diagnostics", &failures);
    assert!(checked > 500, "only {checked} sema cases were checked");
}
