//! Stage 3 differential and property tests: generated K programs are
//! compiled by `k_codegen` and run in V8 (`docs/compiler/testing.md`,
//! "Stage 3 properties").
//!
//! 1. **Typed programs** (`tgen::generate_runnable`, the stage 2 typed
//!    generator in its terminating mode) check without errors, generate
//!    deterministically in every profile with no server code in the client
//!    bundle, and run without an unexpected JavaScript exception; the
//!    development and release builds log the same lines and end the same
//!    way.
//! 2. **Closed expressions** (`exprgen`) evaluate in V8 to exactly what a
//!    reference evaluator in Rust computes, written inline (where the code
//!    generator may fold constants) and behind opaque calls (where it
//!    can't): panics included.
//! 3. **Codable values** (`codablegen`) round-trip through `Json.encode` and
//!    `Json.decode`, encode to the reference JSON of the wire format, and
//!    mutated JSON decodes exactly as a reference decoder says: a value, or
//!    `malformed` / `mismatch`, never an exception.
//! 4. **Views** (`viewgen`): the server renderer's HTML parses back to
//!    itself, equals the client render, and hydrating it claims the
//!    server's nodes and reacts to state changes exactly like a client
//!    render.
//!
//! 5. **The HTTP front and RPC dispatch** (`horacle`) of a generated app
//!    answer every seed request of the fuzz targets, and arbitrary rpc
//!    bodies, without a `500`; an rpc's `200` answer is a fixed point.
//!
//! `PROPTEST_CASES=N` sets the number of cases per property. Each case
//! compiles a project and starts one or more isolates, so the defaults are
//! small; the nightly fuzz targets (`fuzz/`) run the same oracles
//! (`support/coracle.rs`) and generators for as long as one likes.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use crate::codablegen;
use crate::coracle;
use crate::exprgen;
use crate::horacle;
use crate::tgen;
use crate::viewgen;

use std::time::Duration;

use arbitrary::Unstructured;
use k_codegen::Profile;
use proptest::prelude::*;

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

const LIMIT: Duration = Duration::from_secs(10);

proptest! {
    #![proptest_config(config(64))]

    #[test]
    fn typed_programs_generate_deterministically_and_run(data in bytes(4096)) {
        let program = tgen::generate_runnable(&mut Unstructured::new(&data));
        if let Err(e) = coracle::check_terminating_program(&program.files, tgen::RUNNABLE_ENTRY, LIMIT).map(|_| ()) {
            panic!("{e}");
        }
    }

    #[test]
    fn closed_expressions_agree_with_the_reference(data in bytes(2048)) {
        let case = exprgen::generate(&mut Unstructured::new(&data));
        if let Err(e) = exprgen::check(&case, LIMIT) {
            panic!("{e}");
        }
    }

    #[test]
    fn codable_values_round_trip_and_decode_like_the_reference(data in bytes(2048)) {
        let case = codablegen::generate(&mut Unstructured::new(&data));
        if let Err(e) = codablegen::check(&case, LIMIT) {
            panic!("{e}");
        }
    }

    #[test]
    fn server_html_hydrates_to_the_client_render(data in bytes(2048)) {
        let case = viewgen::generate(&mut Unstructured::new(&data));
        if let Err(e) = viewgen::check(&case, LIMIT) {
            panic!("{e}");
        }
    }
}

/// The empty input is the smallest program of each generator, and every
/// oracle accepts it (so a failure above is never about the harness).
#[test]
fn the_smallest_programs_pass() {
    let empty: &[u8] = &[];
    let program = tgen::generate_runnable(&mut Unstructured::new(empty));
    coracle::check_terminating_program(&program.files, tgen::RUNNABLE_ENTRY, LIMIT)
        .map(|_| ())
        .unwrap();
    exprgen::check(&exprgen::generate(&mut Unstructured::new(empty)), LIMIT).unwrap();
    codablegen::check(&codablegen::generate(&mut Unstructured::new(empty)), LIMIT).unwrap();
    viewgen::check(&viewgen::generate(&mut Unstructured::new(empty)), LIMIT).unwrap();
}

/// The oracle tells K panics from JavaScript exceptions: an overflow is a
/// `Panicked` outcome, not an unexpected error.
#[test]
fn an_overflow_is_a_panic_outcome() {
    let files = vec![
        ("k.toml".to_owned(), tgen::MANIFEST.to_owned()),
        (
            "src/logic/F0.k".to_owned(),
            "func main() -> Int {\n    let big = 9007199254740991\n    return big + 1\n}\n"
                .to_owned(),
        ),
    ];
    let out =
        coracle::check_codegen(&files, &|p| coracle::script_options(p, "logic.main"), &[]).unwrap();
    let run = coracle::run(&out, "logic.main", LIMIT).unwrap();
    assert!(
        matches!(&run.outcome, coracle::Outcome::Panicked(m) if m.starts_with("integer overflow")),
        "{run:?}"
    );
}

/// A release build is a different printing of the same program.
#[test]
fn development_and_release_runs_agree_on_a_fixed_program() {
    let files = vec![
        ("k.toml".to_owned(), tgen::MANIFEST.to_owned()),
        (
            "src/logic/F0.k".to_owned(),
            "import k.observe.Log\n\nfunc main() {\n    var total = 0\n    for x in [1, 2, 3] {\n        total += x\n    }\n    Log.info(\"{total}\")\n}\n"
                .to_owned(),
        ),
    ];
    let dev =
        coracle::check_codegen(&files, &|p| coracle::script_options(p, "logic.main"), &[]).unwrap();
    let release = coracle::generate_profile(&files, Profile::Release, "logic.main").unwrap();
    let a = coracle::run(&dev, "logic.main", LIMIT).unwrap();
    let b = coracle::run(&release, "logic.main", LIMIT).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.stdout, vec!["info 6".to_owned()]);
}

/// Replays a view case by hand while minimizing a finding:
/// `K_REPLAY_VIEW=path/to/Generated.k K_REPLAY_PROPS='[{…}, {…}]' cargo test
/// -p k_server --test differential -- --ignored replay_view`. The props are
/// the two `Props` as JSON objects (`title`, `count`, `isOn`, `items`).
#[test]
#[ignore = "a tool for minimizing findings"]
fn replay_view() {
    let path = std::env::var("K_REPLAY_VIEW").expect("K_REPLAY_VIEW");
    let source = std::fs::read_to_string(path).expect("readable");
    let props: Vec<serde_json::Value> =
        serde_json::from_str(&std::env::var("K_REPLAY_PROPS").expect("K_REPLAY_PROPS"))
            .expect("JSON");
    let parse = |v: &serde_json::Value| viewgen::Props {
        title: v["title"].as_str().unwrap_or("").to_owned(),
        count: v["count"].as_i64().unwrap_or(0),
        is_on: v["isOn"].as_bool().unwrap_or(false),
        items: v["items"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut case = viewgen::generate(&mut Unstructured::new(&[]));
    case.files[1].1 = source;
    case.first = parse(&props[0]);
    case.second = parse(&props[1]);
    if let Err(e) = viewgen::check(&case, LIMIT) {
        panic!("{e}");
    }
}

/// Replays a script project by hand while minimizing a finding:
/// `K_REPLAY_DIR=path/to/project K_REPLAY_ENTRY=logic.main cargo test -p
/// k_server --test differential -- --ignored replay_program` (with
/// `K_DIFF_DUMP=1` to print the release build's modules).
#[test]
#[ignore = "a tool for minimizing findings"]
fn replay_program() {
    let dir = std::path::PathBuf::from(std::env::var("K_REPLAY_DIR").expect("K_REPLAY_DIR"));
    let entry = std::env::var("K_REPLAY_ENTRY").unwrap_or_else(|_| tgen::RUNNABLE_ENTRY.to_owned());
    let mut files = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("dir").flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(text) = std::fs::read_to_string(&p) {
                let rel = p
                    .strip_prefix(&dir)
                    .expect("under")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((rel, text));
            }
        }
    }
    files.sort();
    match coracle::check_terminating_program(&files, &entry, LIMIT) {
        Ok(run) => eprintln!("{run:?}"),
        Err(e) => panic!("{e}"),
    }
}

/// A view case with the standard props, for the pins below.
fn view_case(view: &str) -> viewgen::Case {
    let mut case = viewgen::generate(&mut Unstructured::new(&[]));
    case.files[1].1 = format!(
        "public component Generated {{\n    @Prop let title: String\n    @Prop let count: Int\n    @Prop let isOn: Bool\n    @Prop let items: List<String>\n\n    view {{\n{view}\n    }}\n}}\n"
    );
    case.first = viewgen::Props {
        title: "t".to_owned(),
        count: 3,
        is_on: true,
        items: vec!["a".to_owned()],
    };
    case.second = case.first.clone();
    case
}

/// Findings of these properties that the `run` corpus can't pin (they need
/// a release build or the client DOM), each still reproducing: like a
/// `.pending` corpus case, this test fails once a finding is fixed, and
/// then the entry moves to [`fixed_findings_stay_fixed`], and the
/// generator workaround that cites it goes (`KNOWN_FINDINGS` in `tgen.rs`,
/// `exprgen.rs` and `viewgen.rs`). The findings the `run` corpus can pin
/// are under `tests/corpus/run/regressions/fuzz/`. None is open.
#[test]
fn pinned_findings_still_reproduce() {
    // (finding, symptom the failure must mention, the reproduction)
    let pins: Vec<(&str, &str, Result<(), String>)> = Vec::new();
    let mut fixed = Vec::new();
    for (finding, symptom, outcome) in pins {
        match outcome {
            Err(e) if e.contains(symptom) => {}
            Err(e) => fixed.push(format!("{finding}: fails differently now:\n{e}")),
            Ok(()) => fixed.push(format!(
                "{finding}: passes now; remove the pin and the generator workaround that cites it"
            )),
        }
    }
    assert!(fixed.is_empty(), "{}", fixed.join("\n\n"));
}

/// The reproductions of findings that were pinned by
/// [`pinned_findings_still_reproduce`] and are fixed: each must run and
/// agree with the reference now.
#[test]
fn fixed_findings_stay_fixed() {
    let script = |source: &str| {
        let files = vec![
            ("k.toml".to_owned(), tgen::MANIFEST.to_owned()),
            ("src/calc/Main.k".to_owned(), source.to_owned()),
        ];
        coracle::check_terminating_program(&files, "calc.main", LIMIT).map(|_| ())
    };
    let view = |body: &str| viewgen::check(&view_case(body), LIMIT);
    // `isOn` false and an empty `title` (found by the hydration property).
    let off_and_empty = |body: &str| {
        let mut case = view_case(body);
        case.first.is_on = false;
        case.first.title = String::new();
        case.second = case.first.clone();
        viewgen::check(&case, LIMIT)
    };
    let fixed: Vec<(&str, Result<(), String>)> = vec![
        (
            "release-minus-minus: `a - (-b)` in a release build",
            script("func main() -> List<Int> {\n    let a = 5\n    return [a - (-a)]\n}\n"),
        ),
        (
            "release-minus-minus: `a - (-3)` in a release build",
            script("func main() -> List<Int> {\n    let a = 5\n    return [a - (-3)]\n}\n"),
        ),
        (
            "release-minus-minus: `x -= (-x)` in a release build",
            script(
                "func main() -> Double {\n    var x: Double = 10.0\n    x -= (-x)\n    return x\n}\n",
            ),
        ),
        (
            "release-minus-minus: `a + (+b)`-like and `a - (-(-b))` in a release build",
            script(
                "func main() -> List<Int> {\n    let a = 5\n    return [a - (-(-a)), a + (-a), a - -3]\n}\n",
            ),
        ),
        (
            "root-block-hydration: hydrating a root `if` keeps one pair of markers",
            view(
                "        if isOn {\n            box {\n                \"a\"\n            }\n        }",
            ),
        ),
        (
            "root-block-hydration: a root `if` with a text and an `else`",
            view(
                "        if isOn {\n            \"a\"\n        } else {\n            title\n        }",
            ),
        ),
        (
            "block-content-is-claimed: a text at the top of an `if` branch",
            view(
                "        box {\n            if isOn {\n                \"a\"\n            }\n        }",
            ),
        ),
        (
            "block-content-is-claimed: a dynamic text at the top of an `if` branch",
            view(
                "        box {\n            if isOn {\n                title\n\n                \"b\"\n            }\n        }",
            ),
        ),
        (
            "block-content-is-claimed: a nested block at the top of an `if` branch",
            view(
                "        box {\n            if isOn {\n                if isOn {\n                    box {\n                        \"a\"\n                    }\n                }\n            }\n        }",
            ),
        ),
        (
            "template-text-nodes-merge: a static text before a dynamic text",
            view("        paragraph {\n            \"plain\"\n\n            title\n        }"),
        ),
        (
            "template-text-nodes-merge: an empty static text before a dynamic text",
            view("        paragraph {\n            \"\"\n\n            title\n        }"),
        ),
        (
            "template-text-nodes-merge: two static texts before an element with a binding",
            view(
                "        paragraph {\n            \"a\"\n\n            \"b\"\n\n            text {\n                title\n            }\n        }",
            ),
        ),
        (
            "block-content-is-claimed: a text before an element at the top of an `else` branch, with empty texts",
            off_and_empty(
                "        box {\n            if isOn {\n                \"x\"\n            } else {\n                \"plain\"\n\n                heading(level: 2) {\n                    \"x {count}\"\n\n                    title\n\n                    title\n                }\n            }\n        }",
            ),
        ),
        (
            "root-text-separators: two dynamic texts at the top of an `if` branch",
            view(
                "        box {\n            if isOn {\n                title\n\n                \"{count}\"\n            }\n        }",
            ),
        ),
        (
            "block-content-is-claimed: an empty text at the top of a branch before a keyed list",
            off_and_empty(
                "        box {\n            if isOn {\n                title\n            } else {\n                title\n            }\n\n            list {\n                for entry in items key entry {\n                    item {\n                        entry\n\n                        title\n                    }\n                }\n            }\n        }",
            ),
        ),
        (
            "template-text-nodes-merge: dynamic, static, dynamic",
            view(
                "        paragraph {\n            title\n\n            \" and \"\n\n            title\n        }",
            ),
        ),
    ];
    let broken: Vec<String> = fixed
        .into_iter()
        .filter_map(|(finding, outcome)| outcome.err().map(|e| format!("{finding}:\n{e}")))
        .collect();
    assert!(broken.is_empty(), "{}", broken.join("\n\n"));
}

/// The HTTP oracle holds on the seeds of the `http_request` and `rpc_json`
/// fuzz targets.
#[test]
fn the_server_answers_the_fuzz_seeds() {
    let server = horacle::start_server().unwrap();
    let addr = server.local_addr();
    horacle::check_alive(addr).unwrap();
    for raw in horacle::RAW_SEEDS {
        if let Err(e) = horacle::check_raw_request(addr, raw) {
            panic!("{e}");
        }
    }
    for (index, body) in horacle::RPC_SEEDS {
        if let Err(e) = horacle::check_rpc_body(addr, *index, body.as_bytes()) {
            panic!("{e}");
        }
    }
    server.shutdown().unwrap();
}

proptest! {
    #![proptest_config(config(128))]

    /// Arbitrary rpc bodies (bytes, and seeds with a byte changed) are a
    /// `200` fixed point or a `400 invalid_argument`.
    #[test]
    fn rpc_bodies_never_fail_the_server(index in 0usize..8, seed in 0usize..13, at in any::<usize>(), byte in any::<u8>(), raw in bytes(64), use_raw in any::<bool>()) {
        thread_local! {
            static SERVER: std::cell::OnceCell<k_server::Server> = const { std::cell::OnceCell::new() };
        }
        let addr = SERVER.with(|s| s.get_or_init(|| horacle::start_server().unwrap()).local_addr());
        let body = if use_raw {
            raw
        } else {
            let mut b = horacle::RPC_SEEDS[seed % horacle::RPC_SEEDS.len()].1.as_bytes().to_vec();
            if !b.is_empty() {
                let i = at % b.len();
                b[i] = byte;
            }
            b
        };
        if let Err(e) = horacle::check_rpc_body(addr, index, &body) {
            panic!("{e}");
        }
    }
}
