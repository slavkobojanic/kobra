//! Latency budgets of the language server (`docs/compiler/performance.md`
//! §12, budgets in `docs/compiler/architecture.md` §12).
//!
//! `#[ignore]`d: they only make sense in an optimized build on a quiet
//! machine. Run them with
//!
//! ```sh
//! scripts/bench-check.sh --lsp
//! # or, equivalently:
//! cargo test --release -p k_lsp --test perf_budget -- --ignored --nocapture --test-threads=1
//! ```
//!
//! # Method
//!
//! The input is the realistic 100k-line project of the semantic budgets
//! (`crates/k_sema/benches/project`, 1,369 files). **Warm** means the
//! project has been checked and the request's index built by an earlier
//! request of the same revision, the state an editor is in while the user
//! moves the cursor. The positions are the start of every resolved
//! reference of two files (feature 0's service and page), so each feature
//! runs on a few hundred real names; the p50 and p95 of those calls are
//! compared with the budgets.
//!
//! **After a keystroke** is measured end to end through the server in
//! process (the JSON-RPC framing, the main loop, the default 30 ms
//! debounce and a worker): the time from writing a `didChange` (a body edit
//! that alternately adds and removes a type error) to reading the
//! `publishDiagnostics` of the edited file, over 20 edits.
//!
//! | Check | Budget | Source |
//! |---|---|---|
//! | hover, warm, 100k lines | p50 ≤ 2 ms, p95 ≤ 10 ms | performance.md §12 |
//! | go to definition, warm, 100k lines | p50 ≤ 2 ms, p95 ≤ 10 ms | performance.md §12 |
//! | find references (project-wide), warm, 100k lines | p50 ≤ 5 ms, p95 ≤ 20 ms | performance.md §12 |
//! | completion, warm, 100k lines | p95 ≤ 50 ms | architecture.md §12 |
//! | completion after every `:`, `(` and `.` of style blocks and views, warm, 100k lines | p95 ≤ 50 ms | architecture.md §12 |
//! | hover on the first request after a body edit (re-check and re-index), 100k lines | ≤ 60 ms | performance.md §12 |
//! | diagnostics after a keystroke, end to end, 100k lines | p95 ≤ 100 ms | architecture.md §12 |
//!
//! `K_BUDGET_SCALE` (in `(0, 1]`, default 1) divides every budget, for
//! slower machines.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::expect_used
)]

#[path = "support/driver.rs"]
mod driver;
#[path = "../../k_sema/benches/project/mod.rs"]
mod project;

use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use k_diagnostics::{FileId, TextSize};
use k_lsp::ide;
use k_sema::{Analysis, AnalysisHost, Change, Options, ProjectPath};
use serde_json::json;

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

/// The realistic 100k-line project.
fn project_files() -> Vec<(ProjectPath, String)> {
    project::realistic(100_000, project::SEED)
        .into_iter()
        .map(|(p, t)| (ProjectPath::new(&p).unwrap(), t))
        .collect()
}

/// A warm host of the 100k-line project.
fn warm_host() -> AnalysisHost {
    let host = AnalysisHost::with_files(Options::new(), project_files());
    let analysis = host.analysis();
    assert!(
        analysis.diagnostics().unwrap().is_empty(),
        "the generated project is clean"
    );
    host
}

/// The measured files and the start of every resolved reference in them.
fn positions(analysis: &Analysis) -> Vec<(FileId, TextSize)> {
    let mut out = Vec::new();
    let page = format!("src/{}/Page0.k", project::module_name(0));
    for path in [project::edited_service_path(), page] {
        let Some(file) = analysis.file_id(&ProjectPath::new(&path).unwrap()) else {
            continue;
        };
        for (range, _) in analysis.file_references(file).unwrap() {
            out.push((file, range.start()));
        }
    }
    assert!(out.len() > 100, "only {} positions", out.len());
    out
}

/// The p50 and p95 of `run` over `positions`, in milliseconds.
fn percentiles(
    positions: &[(FileId, TextSize)],
    mut run: impl FnMut(FileId, TextSize),
) -> (f64, f64) {
    // Warm up: the first call of a revision builds the reverse index.
    for &(f, o) in positions.iter().take(5) {
        run(f, o);
    }
    let mut times: Vec<f64> = positions
        .iter()
        .map(|&(f, o)| {
            let start = Instant::now();
            run(f, o);
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    times.sort_by(f64::total_cmp);
    let at = |q: f64| times[((times.len() as f64 - 1.0) * q).round() as usize];
    (at(0.5), at(0.95))
}

/// Checks a p50/p95 pair against budgets (milliseconds).
fn check(name: &str, (p50, p95): (f64, f64), budget50: Option<f64>, budget95: f64) {
    let s = scale();
    println!(
        "{name}: p50 {p50:.3} ms, p95 {p95:.3} ms (budget p50 {budget50:?} ms, p95 {budget95} ms, scale {s})"
    );
    if let Some(b) = budget50 {
        assert!(
            p50 <= b / s,
            "{name}: p50 {p50:.3} ms over the {b} ms budget"
        );
    }
    assert!(
        p95 <= budget95 / s,
        "{name}: p95 {p95:.3} ms over the {budget95} ms budget"
    );
}

#[test]
#[ignore = "performance budget; run with --release --ignored"]
fn warm_hover_definition_and_references() {
    let host = warm_host();
    let analysis = host.analysis();
    let positions = positions(&analysis);
    println!("{} positions", positions.len());
    let link = |_: k_diagnostics::Span| None;
    // The measured calls do answer: every position is a reference, so
    // nearly all hovers have content and most definitions a location (not
    // built-in elements and CSS properties).
    let answered = positions
        .iter()
        .filter(|&&(f, o)| ide::hover::hover(&analysis, f, o, &link).unwrap().is_some())
        .count();
    assert!(
        answered * 10 >= positions.len() * 9,
        "{answered} of {} hovers",
        positions.len()
    );
    let located = positions
        .iter()
        .filter(|&&(f, o)| {
            !ide::navigation::definition(&analysis, f, o)
                .unwrap()
                .is_empty()
        })
        .count();
    assert!(
        located * 2 >= positions.len(),
        "{located} of {} definitions",
        positions.len()
    );
    let hover = percentiles(&positions, |f, o| {
        black_box(ide::hover::hover(&analysis, f, o, &link).unwrap());
    });
    check("hover", hover, Some(2.0), 10.0);
    let definition = percentiles(&positions, |f, o| {
        black_box(ide::navigation::definition(&analysis, f, o).unwrap());
    });
    check("definition", definition, Some(2.0), 10.0);
    let references = percentiles(&positions, |f, o| {
        black_box(ide::navigation::references(&analysis, f, o, true).unwrap());
    });
    check("references", references, Some(5.0), 20.0);
}

#[test]
#[ignore = "performance budget; run with --release --ignored"]
fn warm_completion() {
    let host = warm_host();
    let analysis = host.analysis();
    let positions = positions(&analysis);
    let completion = percentiles(&positions, |f, o| {
        black_box(ide::completion::completions(&analysis, f, o).unwrap());
    });
    check("completion", completion, None, 50.0);
}

/// Positions in style blocks and modifier calls: after every `:`, `(` and
/// `.` of the first component files' style blocks and views.
fn style_and_event_positions(analysis: &Analysis) -> Vec<(FileId, TextSize)> {
    let mut out = Vec::new();
    let files = project_files();
    for (path, text) in files
        .iter()
        .filter(|(_, t)| t.contains("style {") && t.contains(".on("))
        .take(4)
    {
        let file = analysis.file_id(path).unwrap();
        for (i, c) in text.char_indices() {
            if matches!(c, ':' | '(' | '.') {
                out.push((file, TextSize::from(u32::try_from(i + 1).unwrap())));
            }
        }
    }
    assert!(out.len() > 100, "only {} positions", out.len());
    out
}

#[test]
#[ignore = "performance budget; run with --release --ignored"]
fn warm_completion_in_styles_and_events() {
    let host = warm_host();
    let analysis = host.analysis();
    let positions = style_and_event_positions(&analysis);
    let completion = percentiles(&positions, |f, o| {
        black_box(ide::completion::completions(&analysis, f, o).unwrap());
    });
    check("completion in styles and events", completion, None, 50.0);
}

#[test]
#[ignore = "performance budget; run with --release --ignored"]
fn hover_after_a_body_edit() {
    let mut host = warm_host();
    let path = ProjectPath::new(&project::edited_service_path()).unwrap();
    let original = {
        let a = host.analysis();
        a.text(a.file_id(&path).unwrap()).unwrap().to_owned()
    };
    let positions = positions(&host.analysis());
    let (file, offset) = positions[positions.len() / 3];
    // Warm the index of the first revision.
    let _ = ide::hover::hover(&host.analysis(), file, offset, &|_| None).unwrap();
    let mut best = Duration::MAX;
    for generation in 0..20 {
        let mut change = Change::new();
        change.set_file(path.clone(), project::body_edit(&original, generation % 2));
        let start = Instant::now();
        host.apply_change(change);
        let analysis = host.analysis();
        black_box(ide::hover::hover(&analysis, file, offset, &|_| None).unwrap());
        best = best.min(start.elapsed());
    }
    let ms = best.as_secs_f64() * 1e3;
    let budget = 60.0 / scale();
    println!("hover after a body edit: {ms:.2} ms (budget {budget} ms)");
    assert!(
        ms <= budget,
        "hover after a body edit: {ms:.2} ms over {budget} ms"
    );
}

#[test]
#[ignore = "performance budget; run with --release --ignored"]
fn diagnostics_after_a_keystroke() {
    // The project on disk, for the server to load.
    let dir = std::env::temp_dir().join(format!("k-lsp-perf-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (path, text) in project::realistic(100_000, project::SEED) {
        let p = dir.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    let root: PathBuf = dir.canonicalize().unwrap();
    let mut client = driver::Client::spawn_with(k_lsp::Config::default());
    let _ = client.request(
        "initialize",
        json!({ "processId": null, "rootUri": driver::url(&root), "capabilities": {} }),
    );
    client.notify("initialized", json!({}));
    let file = root.join(project::edited_service_path());
    let uri = driver::url(&file);
    let original = fs::read_to_string(&file).unwrap();
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": original } }),
    );
    client.diagnostics(&uri, <[serde_json::Value]>::is_empty);
    let marker = "return value * 3 + ";
    let at = original.find(marker).unwrap() + marker.len();
    let line = original[..at].matches('\n').count();
    let character = at - original[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = original[at..].find('\n').unwrap() + character;
    let mut times = Vec::new();
    for i in 0..20 {
        let broken = i % 2 == 0;
        let text = if broken { "\"x\"" } else { "2" };
        // Replace the rest of the line: an error, or no error.
        let current_end = if i == 0 {
            end
        } else if broken {
            character + 1
        } else {
            character + 3
        };
        let start = Instant::now();
        client.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": i + 2 },
                "contentChanges": [{ "range": {
                    "start": { "line": line, "character": character },
                    "end": { "line": line, "character": current_end },
                }, "text": text }],
            }),
        );
        client.diagnostics(&uri, |d| d.is_empty() != broken);
        times.push(start.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    let p95 = times[((times.len() as f64 - 1.0) * 0.95).round() as usize];
    let p50 = times[times.len() / 2];
    let budget = 100.0 / scale();
    println!(
        "diagnostics after a keystroke: p50 {p50:.2} ms, p95 {p95:.2} ms (budget p95 {budget} ms, 30 ms of debounce included)"
    );
    assert_eq!(client.shutdown(), 0);
    let _ = fs::remove_dir_all(&dir);
    assert!(
        p95 <= budget,
        "diagnostics after a keystroke: p95 {p95:.2} ms over {budget} ms"
    );
}
