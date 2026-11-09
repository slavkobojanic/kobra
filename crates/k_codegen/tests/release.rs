//! Release builds of the example apps (`docs/compiler/codegen.md` §17):
//! the client is linked (named runtime imports, a runtime chunk with only
//! what the app reaches), minified with a source map per chunk, its CSS
//! names are short, its wire keys quoted, its documents preload the module
//! graph, and what each app ships stays within its budget
//! (`docs/compiler/performance.md` §11.7). The development and test
//! profiles keep their readable output.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use k_codegen::{Entry, Environment, FileKind, Options, Output, OutputFile, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};

/// Every file below `dir` as `(project path, text)`.
fn project(dir: &std::path::Path) -> Vec<(ProjectPath, String)> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(ProjectPath, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if let Ok(p) = ProjectPath::new(&rel) {
                    out.push((p, text));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

/// The apps of the examples, as `examples/<name>` or `<workspace>/<member>`.
const APPS: &[&str] = &[
    "todos",
    "kanban",
    "weather",
    "notes",
    "huddle",
    "shop/shop",
    "shop/paybox",
    "stack/shop",
    "stack/paybox",
];

/// The build of example app `name` (see [`APPS`]) under `profile`.
fn build(name: &str, profile: Profile) -> Output {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let files = match name.split_once('/') {
        Some((workspace, member)) => {
            let ws = k_sema::Workspace::from_files(
                project(&examples.join(workspace))
                    .into_iter()
                    .map(|(p, t)| (p.as_str().to_owned(), t)),
            );
            let m = ws.member(member).expect("the workspace member");
            ws.analysis_files(m)
        }
        None => project(&examples.join(name)),
    };
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let environment = if profile == Profile::Release {
        Environment::Production
    } else {
        Environment::Development
    };
    generate(
        &host.analysis(),
        &Options {
            profile,
            environment,
            entry: Entry::App,
        },
    )
    .expect("generates")
}

fn js(out: &Output) -> impl Iterator<Item = &OutputFile> {
    out.client.iter().filter(|f| f.kind == FileKind::JavaScript)
}

fn file<'a>(out: &'a Output, stem: &str, ext: &str) -> &'a OutputFile {
    let found = out
        .client
        .iter()
        .find(|f| f.path.starts_with(stem) && f.path.ends_with(ext));
    assert!(found.is_some(), "no {stem}*{ext}");
    found.expect("checked above")
}

fn gzip(bytes: &[u8]) -> usize {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    e.write_all(bytes).expect("gzip");
    e.finish().expect("gzip").len()
}

#[test]
fn release_chunks_import_the_runtime_by_name_and_carry_source_maps() {
    for name in APPS {
        let out = build(name, Profile::Release);
        let paths: BTreeSet<&str> = out.client.iter().map(|f| f.path.as_str()).collect();
        for f in js(&out) {
            assert!(
                !f.contents.contains("import*as") && !f.contents.contains("import * as"),
                "{name}: {} imports a namespace",
                f.path
            );
            let map = f
                .contents
                .rsplit_once("//# sourceMappingURL=")
                .map(|(_, m)| m.trim_end().to_owned());
            assert!(map.is_some(), "{name}: {} has no map comment", f.path);
            let dir = f
                .path
                .rsplit_once('/')
                .map_or(String::new(), |(d, _)| format!("{d}/"));
            let map = format!("{dir}{}", map.unwrap_or_default());
            assert!(paths.contains(map.as_str()), "{name}: no {map}");
            assert!(
                f.contents.lines().count() <= 2,
                "{name}: {} is not minified",
                f.path
            );
        }
        for m in out.client.iter().filter(|f| f.kind == FileKind::SourceMap) {
            let v: serde_json::Value = serde_json::from_str(&m.contents).expect("a JSON map");
            assert_eq!(v["version"], 3);
            // A chunk with K code maps straight to the `.k` files (no K
            // text shipped); the runtime and glue map to their readable
            // text, ignore-listed (codegen.md §18).
            if v["ignoreList"].is_array() {
                let content = v["sourcesContent"][0].as_str().unwrap_or("");
                assert!(
                    content.contains('\n'),
                    "{name}: {} maps to readable code",
                    m.path
                );
            } else {
                let sources = v["sources"].as_array().expect("sources");
                assert!(
                    sources
                        .iter()
                        .all(|s| s.as_str().is_some_and(|s| s.starts_with("/_k/src/"))),
                    "{name}: {} maps to K: {sources:?}",
                    m.path
                );
                assert!(v.get("sourcesContent").is_none(), "{name}: {}", m.path);
            }
        }
    }
}

#[test]
fn release_builds_are_deterministic() {
    // Content hashes name the files, so the linker and the minifier must
    // give the same bytes every time.
    for name in ["todos", "notes"] {
        assert_eq!(
            build(name, Profile::Release),
            build(name, Profile::Release),
            "{name}"
        );
    }
}

#[test]
fn every_chunk_a_release_chunk_names_is_shipped() {
    // Renaming after minification must leave no chunk pointing at an old
    // name: every relative specifier names a shipped file.
    for name in APPS {
        let out = build(name, Profile::Release);
        let paths: BTreeSet<String> = out.client.iter().map(|f| f.path.clone()).collect();
        for f in js(&out) {
            let dir = f.path.rsplit_once('/').map_or("", |(d, _)| d);
            for quote in ['"', '`', '\''] {
                for piece in f.contents.split(quote).skip(1).step_by(2) {
                    if !(piece.starts_with("./") || piece.starts_with("../"))
                        || !piece.ends_with(".js")
                    {
                        continue;
                    }
                    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
                    for seg in piece.split('/') {
                        match seg {
                            "." => {}
                            ".." => {
                                parts.pop();
                            }
                            s => parts.push(s),
                        }
                    }
                    let target = parts.join("/");
                    assert!(paths.contains(&target), "{name}: {} names {piece}", f.path);
                }
            }
        }
    }
}

#[test]
fn the_runtime_chunk_carries_only_what_the_app_reaches() {
    let todos = build("todos", Profile::Release);
    let dev = build("todos", Profile::Development);
    let shipped = file(&todos, "runtime.", ".js").contents.len();
    let full = file(&dev, "runtime.", ".js").contents.len();
    assert!(
        shipped * 3 < full,
        "todos' runtime is {shipped} of {full} bytes"
    );
    // Todos makes no rpc: the rpc client's protocol header is gone.
    let runtime = &file(&todos, "runtime.", ".js").contents;
    assert!(!runtime.contains("Connect-Protocol-Version"), "{runtime}");
    // Notes does, so its runtime keeps it.
    let notes = build("notes", Profile::Release);
    assert!(
        file(&notes, "runtime.", ".js")
            .contents
            .contains("Connect-Protocol-Version")
    );
    // The runtime differs per app.
    assert_ne!(
        file(&todos, "runtime.", ".js").path,
        file(&notes, "runtime.", ".js").path
    );
}

#[test]
fn development_and_test_builds_keep_readable_namespace_imports() {
    let dev = build("todos", Profile::Development);
    let home = file(&dev, "pages/home.", ".js");
    assert!(
        home.contents.contains("import * as $k from \"../runtime."),
        "{}",
        home.contents
    );
    // Development chunks map back to the `.k` files (codegen.md §18).
    assert!(
        home.contents.contains("//# sourceMappingURL=home."),
        "{}",
        home.contents
    );
    assert!(!home.contents.contains("$k.walk("), "{}", home.contents);
}

#[test]
fn release_templates_walk_to_their_nodes() {
    // A path of two steps or more is `walk(root, "fn…")`, not a chain of
    // `.firstChild` / `.nextSibling`.
    for name in ["todos", "kanban", "notes"] {
        let out = build(name, Profile::Release);
        for f in js(&out).filter(|f| !f.path.starts_with("runtime.")) {
            assert!(
                !f.contents.contains(".firstChild.nextSibling")
                    && !f.contents.contains(".nextSibling.nextSibling"),
                "{name}: {} has a chain",
                f.path
            );
        }
    }
}

#[test]
fn encoders_quote_their_wire_keys() {
    let dev = build("todos", Profile::Development);
    let shared = file(&dev, "shared.", ".js");
    let at = shared
        .contents
        .find("function Todo$encode(")
        .expect("todos encodes a Todo");
    let body = &shared.contents[at..at + 300];
    assert!(body.contains("\"isDone\": "), "{body}");
    assert!(body.contains("\"title\": "), "{body}");
}

/// The scoped class names (`name_<6 base-36 digits>`) in `text`.
fn hashed_classes(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let mut i = 0;
    while i < bytes.len() {
        if word(bytes[i]) && (i == 0 || !word(bytes[i - 1])) {
            let start = i;
            while i < bytes.len() && word(bytes[i]) {
                i += 1;
            }
            let w = &text[start..i];
            if let Some((head, tail)) = w.rsplit_once('_')
                && !head.is_empty()
                && tail.len() == 6
                && tail
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
                && tail.bytes().any(|b| b.is_ascii_digit())
            {
                out.insert(w.to_owned());
            }
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn release_css_names_are_short_and_agree_with_the_markup() {
    for name in ["todos", "kanban", "weather", "notes"] {
        let out = build(name, Profile::Release);
        let css = &file(&out, "app.", ".css").contents;
        assert!(
            hashed_classes(css).is_empty(),
            "{name}: {:?}",
            hashed_classes(css)
        );
        // Every class the markup names that the stylesheet styles exists
        // there under the same short name.
        let selectors: BTreeSet<String> = css
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'))
            .filter_map(|w| w.strip_prefix('.'))
            .map(str::to_owned)
            .collect();
        let mut seen = 0;
        for f in js(&out) {
            for w in f.contents.split(['"', '`', '\'', ' ', '=']) {
                if w.starts_with('_') && w.len() <= 4 && selectors.contains(w) {
                    seen += 1;
                }
            }
            assert!(
                hashed_classes(&f.contents).is_empty(),
                "{name}: {} names {:?}",
                f.path,
                hashed_classes(&f.contents)
            );
        }
        assert!(seen > 0, "{name}: no short class in the markup");
        for s in out.server.iter() {
            assert!(
                hashed_classes(&s.contents).is_empty(),
                "{name}: server {} names {:?}",
                s.path,
                hashed_classes(&s.contents)
            );
        }
        // Custom properties are short too, except those the runtime itself
        // reads (`--k-color-chart-axis`, `--k-color-drop-target`, …).
        let long: Vec<&str> = css
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .filter(|w| w.starts_with("--k-"))
            .filter(|w| {
                !k_runtime::modules().iter().any(|m| {
                    m.source.contains(&format!("{w})"))
                        || m.source.contains(&format!("{w},"))
                        || m.source.contains(&format!("{w}\""))
                })
            })
            .collect();
        assert!(long.is_empty(), "{name}: {long:?}");
    }
    // Test builds keep readable names, so goldens and class selectors work.
    let test = build("todos", Profile::Test);
    assert!(file(&test, "app.", ".css").contents.contains(".Home__page"));
}

#[test]
fn documents_preload_the_static_module_graph() {
    for name in ["todos", "notes", "weather"] {
        for profile in [Profile::Release, Profile::Development] {
            let out = build(name, profile);
            let entry = out
                .server
                .iter()
                .find(|f| Some(&f.path) == out.server_entry.as_ref())
                .expect("a server entry");
            let runtime = format!("/assets/{}", file(&out, "runtime.", ".js").path);
            let shared = format!("/assets/{}", file(&out, "shared.", ".js").path);
            // The keys' order depends on serde_json's features, which
            // other crates of the build may switch on, so each list is
            // found on its own.
            let list = |key: &str, close: char| -> &str {
                let at = entry
                    .contents
                    .find(&format!("\"{key}\":"))
                    .unwrap_or_else(|| panic!("{name}: the assets have no `{key}`"));
                let rest = &entry.contents[at..];
                &rest[..rest.find(close).map_or(rest.len(), |e| e + 1)]
            };
            let preload = list("preload", ']');
            assert!(preload.contains(&runtime), "{name}: {preload}");
            assert!(preload.contains(&shared), "{name}: {preload}");
            let pages = list("pages", '}');
            assert!(pages.contains("/assets/pages/"), "{name}: {pages}");
        }
    }
}

/// The per-app budgets of shipped JavaScript and CSS, gzip -9, from the
/// table in `docs/compiler/performance.md` §11.7 (`| app | budget B | …`).
fn budgets() -> BTreeMap<String, usize> {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/compiler/performance.md"),
    )
    .expect("performance.md");
    let at = doc.find("### 11.7").expect("§11.7");
    let section = &doc[at..];
    let section = &section[..section[4..].find("\n### ").map_or(section.len(), |e| e + 4)];
    let mut out = BTreeMap::new();
    for line in section.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 4 {
            continue;
        }
        // Budget rows name the app in code and the budget in bytes
        // (`| `todos` | 17,500 B | … |`); the other tables don't.
        let app = cells[1].trim_matches('`');
        if !cells[1].starts_with('`') || !APPS.contains(&app) || !cells[2].ends_with(" B") {
            continue;
        }
        let n: String = cells[2].chars().filter(char::is_ascii_digit).collect();
        if let Ok(n) = n.parse() {
            out.insert(app.to_owned(), n);
        }
    }
    out
}

#[test]
fn every_app_ships_within_its_gzip_budget() {
    let budgets = budgets();
    assert_eq!(
        budgets.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        APPS.iter().copied().collect::<BTreeSet<_>>(),
        "performance.md §11.7 has a budget for every example app"
    );
    let mut report = Vec::new();
    for name in APPS {
        let out = build(name, Profile::Release);
        let shipped: usize = out
            .client
            .iter()
            .filter(|f| f.kind != FileKind::SourceMap)
            .map(|f| gzip(f.contents.as_bytes()))
            .sum();
        report.push(format!("{name}: {shipped} B"));
        assert!(
            budgets[*name] < shipped * 2,
            "{name}: a budget of {} B for {shipped} B is no budget",
            budgets[*name]
        );
        assert!(
            shipped <= budgets[*name],
            "{name} ships {shipped} B gzip (budget {} B)",
            budgets[*name]
        );
    }
    eprintln!("{}", report.join("\n"));
}

/// What linking and minifying cost (`docs/compiler/performance.md`
/// §11.7): each app generated with the development and the release
/// profile, best of 5. Run in release:
/// `cargo test --release -p k_codegen --test it -- release:: --ignored --nocapture`.
#[test]
#[ignore = "timing; run in release with --ignored --nocapture"]
fn release_generation_time() {
    for name in APPS {
        let mut best = [f64::MAX; 2];
        for _ in 0..5 {
            for (i, profile) in [Profile::Development, Profile::Release]
                .into_iter()
                .enumerate()
            {
                let start = std::time::Instant::now();
                let _ = build(name, profile);
                best[i] = best[i].min(start.elapsed().as_secs_f64() * 1e3);
            }
        }
        eprintln!(
            "{name}: development {:.1} ms, release {:.1} ms (+{:.1} ms)",
            best[0],
            best[1],
            best[1] - best[0]
        );
    }
}
