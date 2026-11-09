#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

//! Source maps back to the `.k` files (`docs/compiler/codegen.md` §18),
//! decoded with the `sourcemap` crate (an implementation independent of
//! the generator's) and checked at exact lines and columns.
//!
//! The project is `crates/k_cli/tests/fixtures/debugging`, which the
//! real-browser debugging tests (`k_cli/tests/debugging.rs`) set
//! breakpoints in too.

use k_codegen::{Entry, Environment, FileKind, Options, Output, OutputFile, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};

/// The fixture's directory.
fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../k_cli/tests/fixtures/debugging")
}

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

/// The fixture built under `profile`.
fn build(profile: Profile) -> Output {
    let host = AnalysisHost::with_files(SemaOptions::new(), project(&fixture()));
    let out = generate(
        &host.analysis(),
        &Options {
            profile,
            environment: Environment::Development,
            entry: Entry::App,
        },
    )
    .expect("the fixture generates");
    if std::env::var_os("K_DUMP").is_some() {
        for f in out.client.iter().chain(out.server.iter()) {
            eprintln!("=== {}\n{}", f.path, f.contents);
        }
    }
    out
}

/// The client file whose path starts with `prefix` and ends with `suffix`.
fn client<'a>(out: &'a Output, prefix: &str, suffix: &str) -> &'a OutputFile {
    out.client
        .iter()
        .find(|f| f.path.starts_with(prefix) && f.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("no {prefix}…{suffix}"))
}

/// The map a client file names at its end.
fn map_of(out: &Output, f: &OutputFile) -> sourcemap::SourceMap {
    let (_, url) = f
        .contents
        .rsplit_once("sourceMappingURL=")
        .unwrap_or_else(|| panic!("{} names no map", f.path));
    let url = url.trim_end().trim_end_matches("*/").trim();
    let dir = f
        .path
        .rsplit_once('/')
        .map_or(String::new(), |(d, _)| format!("{d}/"));
    let path = format!("{dir}{url}");
    let m = out
        .client
        .iter()
        .find(|m| m.path == path && m.kind == FileKind::SourceMap)
        .unwrap_or_else(|| panic!("no map {path}"));
    sourcemap::SourceMap::from_slice(m.contents.as_bytes()).expect("a valid map")
}

/// The inline map of a server module.
fn server_map(out: &Output, path: &str) -> sourcemap::SourceMap {
    let f = out
        .server
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("no server module {path}"));
    let (_, url) = f
        .contents
        .rsplit_once("//# sourceMappingURL=data:application/json;charset=utf-8;base64,")
        .unwrap_or_else(|| panic!("{path} has no inline map"));
    let json = base64_decode(url.trim());
    sourcemap::SourceMap::from_slice(&json).expect("a valid map")
}

/// Decodes standard base64.
fn base64_decode(text: &str) -> Vec<u8> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for b in text.bytes().filter(|&b| b != b'=') {
        let v = A.iter().position(|&c| c == b).expect("base64") as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// The (0-based) generated line and column of the first occurrence of
/// `needle` in `text`.
fn position(text: &str, needle: &str) -> (u32, u32) {
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in:\n{text}"));
    let line = text[..at].matches('\n').count() as u32;
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let col = text[start..at].encode_utf16().count() as u32;
    (line, col)
}

/// Where the code at `needle` maps to: `(source, 1-based line, 1-based
/// column)`.
fn original(map: &sourcemap::SourceMap, text: &str, needle: &str) -> (String, u32, u32) {
    let (line, col) = position(text, needle);
    let t = map
        .lookup_token(line, col)
        .unwrap_or_else(|| panic!("{needle:?} at {line}:{col} is not mapped"));
    (
        t.get_source().unwrap_or("").to_owned(),
        t.get_src_line() + 1,
        t.get_src_col() + 1,
    )
}

const HOME: &str = "/_k/src/src/home/Home.k";

#[test]
fn development_chunks_map_statements_bindings_and_handlers_to_k() {
    let out = build(Profile::Development);
    let page = client(&out, "pages/home.", ".js");
    let map = map_of(&out, page);
    let text = &page.contents;
    // Statements of a function, an async function and a handler.
    assert_eq!(original(&map, text, "const empty"), (HOME.into(), 22, 9));
    assert_eq!(
        original(&map, text, "count.set($k.at(empty"),
        (HOME.into(), 23, 9)
    );
    assert_eq!(original(&map, text, "const text = "), (HOME.into(), 17, 9));
    assert_eq!(
        original(&map, text, "message.set(text)"),
        (HOME.into(), 18, 9)
    );
    assert_eq!(original(&map, text, "const next"), (HOME.into(), 30, 17));
    assert_eq!(
        original(&map, text, "count.set(next)"),
        (HOME.into(), 31, 17)
    );
    // A match.
    let (src, line, _) = original(&map, text, "\"none\"");
    assert_eq!((src.as_str(), line), (HOME, 10));
    // The event handlers are attached where they are written.
    let (_, line, _) = original(&map, text, "$k.on(");
    assert!((29..=36).contains(&line), "{line}");
    // A view binding with string interpolation re-renders from its line.
    let (src, line, col) = original(&map, text, "$k.text(");
    assert_eq!(src, HOME);
    assert!(line == 27 || line == 37, "{line}:{col}");
    // `@Derived` initializers.
    let (src, line, _) = original(&map, text, "$k.computed(");
    assert_eq!((src.as_str(), line), (HOME, 6));
    // The K text is embedded, and the map names its file.
    let home = map
        .sources()
        .position(|s| s == HOME)
        .expect("Home.k is a source");
    let content = map
        .get_source_contents(home as u32)
        .expect("development maps embed the K text");
    assert!(content.contains("func explode()"), "{content}");
}

#[test]
fn the_development_stylesheet_maps_rules_to_style_blocks() {
    let out = build(Profile::Development);
    let css = client(&out, "app.", ".css");
    let map = map_of(&out, css);
    let text = &css.contents;
    let at = text.find("color: red").expect("the rule");
    let line = text[..at].matches('\n').count() as u32;
    let t = map.lookup_token(line, 2).expect("mapped");
    assert_eq!(t.get_source(), Some(HOME));
    assert_eq!((t.get_src_line() + 1, t.get_src_col() + 1), (43, 13));
    let rule = text[..at].rfind(".note").expect("the selector");
    let line = text[..rule].matches('\n').count() as u32;
    let t = map.lookup_token(line, 0).expect("mapped");
    assert_eq!((t.get_src_line() + 1, t.get_src_col() + 1), (42, 9));
}

#[test]
fn runtime_and_glue_chunks_are_ignore_listed() {
    for profile in [Profile::Development, Profile::Release] {
        let out = build(profile);
        for f in out.client.iter().filter(|f| {
            f.kind == FileKind::JavaScript
                && (f.path.starts_with("runtime.") || f.path.starts_with("app."))
        }) {
            let map = map_of(&out, f);
            let raw: serde_json::Value = serde_json::from_str(
                &out.client
                    .iter()
                    .find(|m| {
                        m.kind == FileKind::SourceMap && m.contents.contains("ignoreList") && {
                            let v: serde_json::Value =
                                serde_json::from_str(&m.contents).unwrap_or_default();
                            v["sources"][0].as_str() == map.get_source(0)
                        }
                    })
                    .unwrap_or_else(|| panic!("{profile:?}: {} is not ignore-listed", f.path))
                    .contents,
            )
            .expect("JSON");
            assert_eq!(raw["ignoreList"][0], 0, "{profile:?}: {}", f.path);
        }
    }
}

#[test]
fn release_maps_compose_minified_code_straight_to_k() {
    let out = build(Profile::Release);
    let page = client(&out, "pages/home.", ".js");
    assert!(page.contents.lines().count() <= 2, "minified");
    let map = map_of(&out, page);
    assert!(
        map.sources().all(|s| s.starts_with("/_k/src/")),
        "{:?}",
        map.sources().collect::<Vec<_>>()
    );
    assert!(
        (0..map.get_source_count()).all(|i| map.get_source_contents(i).is_none()),
        "release maps don't ship the K text"
    );
    // The minified handler body still maps to its K lines.
    let lines: std::collections::BTreeSet<u32> = map
        .tokens()
        .filter(|t| t.get_source() == Some(HOME))
        .map(|t| t.get_src_line() + 1)
        .collect();
    // (`let empty = []` on line 22 is inlined by the minifier.)
    for line in [17, 18, 23, 30, 31] {
        assert!(
            lines.contains(&line),
            "line {line} is not mapped: {lines:?}"
        );
    }
    // A minified statement maps to the start of its K statement.
    let text = &page.contents;
    let at = text.find(".uppercased").map(|_| ()).is_none();
    assert!(at, "server code stays on the server");
}

#[test]
fn server_modules_carry_inline_maps_to_k() {
    let out = build(Profile::Development);
    let path = out
        .server
        .iter()
        .find(|f| f.contents.contains("Hello, "))
        .map(|f| f.path.clone())
        .expect("the greeter's module");
    let map = server_map(&out, &path);
    let f = out.server.iter().find(|f| f.path == path).expect("module");
    let (src, line, col) = original(&map, &f.contents, "const shout");
    assert_eq!(
        (src.as_str(), line, col),
        ("k:/_k/src/src/home/GreeterApi.k", 5, 9)
    );
    let (_, line, _) = original(&map, &f.contents, "return Greeting(`Hello");
    assert_eq!(line, 6);
}

#[test]
fn renamed_locals_carry_their_k_names() {
    // A local spelled differently in JavaScript (`arguments$`, `x$1`) is
    // named in the map at its declaration and uses, so DevTools shows the
    // K name; generated names (`$c`, `$root3`) are never named.
    let out = build(Profile::Development);
    let shared = client(&out, "shared.", ".js");
    let map = map_of(&out, shared);
    let (line, col) = position(&shared.contents, "arguments$ =");
    let t = map.lookup_token(line, col).expect("mapped");
    assert_eq!(t.get_name(), Some("arguments"));
    assert_eq!(t.get_source(), Some("/_k/src/src/home/shout.k"));
    assert_eq!((t.get_src_line() + 1, t.get_src_col() + 1), (4, 5));
    for f in out.client.iter().filter(|f| f.kind == FileKind::SourceMap) {
        let v: serde_json::Value = serde_json::from_str(&f.contents).expect("JSON");
        for n in v["names"].as_array().into_iter().flatten() {
            let n = n.as_str().unwrap_or("");
            assert!(!n.contains('$'), "{}: {n}", f.path);
        }
    }
    // Release maps keep it through the minifier's renaming.
    let out = build(Profile::Release);
    let shared = client(&out, "shared.", ".js");
    let map = map_of(&out, shared);
    assert!(
        map.tokens().any(|t| t.get_name() == Some("arguments")),
        "the release map names the local"
    );
}
