//! Linking a release client (`docs/compiler/codegen.md` §17): after the
//! chunks are printed, every JavaScript chunk of an app's
//! [`crate::Profile::Release`] build is shaken, minified and renamed here,
//! and gets a source map.
//!
//! 1. The **module graph** is read from each chunk's imports
//!    ([`module_graph`]): static named imports, namespace imports and
//!    dynamic `import("…")`s, resolved like URLs between the chunks.
//! 2. A chunk that other chunks import only **by name** (the runtime
//!    chunk, the feature chunks, `shared`) keeps only the exports some
//!    chunk imports; a chunk imported as a namespace or dynamically (the
//!    pages) or imported by nobody (the `app` entry) keeps every export.
//!    Generated code imports the runtime by name in release builds
//!    (`bundle::named_runtime_imports`), so the runtime chunk carries only
//!    what the app reaches, across all of its chunks.
//! 3. Each chunk goes through [`crate::minify`], dependencies first, and is
//!    renamed after its minified text (`stem.<hash>.js`); the chunks that
//!    name it are updated before they are minified in turn. Each chunk
//!    ends with `//# sourceMappingURL=<stem>.<hash>.js.map` and its map
//!    ([`FileKind::SourceMap`]) maps back to the readable chunk, embedded.
//!
//! The pass is deterministic, so the same program gives the same names.

use std::collections::{BTreeMap, BTreeSet};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::module_record::ImportImportName;

use crate::{FileKind, OutputFile};

/// What one chunk imports from the others, by path relative to `/assets/`.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct Imports {
    /// Chunks imported statically (`import … from`, `export … from`).
    pub static_: BTreeSet<String>,
    /// Chunks imported with `import("…")`.
    pub dynamic: BTreeSet<String>,
    /// The names imported from each chunk by `import { … }`.
    pub names: BTreeMap<String, BTreeSet<String>>,
    /// Chunks imported whole: `import * as`, a default import or
    /// `export * from`, whose exports must all stay.
    pub whole: BTreeSet<String>,
    /// Chunks named by `new URL("…", import.meta.url)`: a `@Worker`
    /// service's chunk, which the proxy starts (docs/design/55-workers.md).
    /// Not imports, but named, so they are renamed before this chunk.
    pub urls: BTreeSet<String>,
}

/// The imports of every JavaScript file of `files`, by path. A specifier
/// that names no file of `files` (a URL, `k:runtime` in a test bundle) is
/// left out.
pub(crate) fn module_graph(files: &[OutputFile]) -> BTreeMap<String, Imports> {
    let paths: BTreeSet<&str> = files
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript)
        .map(|f| f.path.as_str())
        .collect();
    let mut graph = BTreeMap::new();
    for f in files.iter().filter(|f| f.kind == FileKind::JavaScript) {
        graph.insert(f.path.clone(), imports_of(&f.path, &f.contents, &paths));
    }
    graph
}

/// [`module_graph`] for one file.
fn imports_of(path: &str, text: &str, paths: &BTreeSet<&str>) -> Imports {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, text, SourceType::mjs()).parse();
    let record = &parsed.module_record;
    let mut out = Imports::default();
    let target = |spec: &str| resolve(path, spec).filter(|p| paths.contains(p.as_str()));
    for spec in record.requested_modules.keys() {
        if let Some(p) = target(spec.as_str()) {
            out.static_.insert(p);
        }
    }
    for entry in &record.import_entries {
        let Some(p) = target(entry.module_request.name.as_str()) else {
            continue;
        };
        match &entry.import_name {
            ImportImportName::Name(n) => {
                out.names
                    .entry(p)
                    .or_default()
                    .insert(n.name.as_str().to_owned());
            }
            ImportImportName::NamespaceObject | ImportImportName::Default(_) => {
                out.whole.insert(p);
            }
        }
    }
    for entry in record
        .indirect_export_entries
        .iter()
        .chain(record.star_export_entries.iter())
    {
        if let Some(p) = entry
            .module_request
            .as_ref()
            .and_then(|m| target(m.name.as_str()))
        {
            out.whole.insert(p);
        }
    }
    let mut rest = text;
    while let Some(i) = rest.find("new URL(") {
        rest = &rest[i + "new URL(".len()..];
        let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        if let Some(end) = rest[1..].find(quote)
            && let Some(p) = target(&rest[1..=end])
        {
            out.urls.insert(p);
        }
    }
    for d in &record.dynamic_imports {
        let raw = d.module_request.source_text(text).trim();
        let spec = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
        if spec.len() + 2 == raw.len()
            && let Some(p) = target(spec)
        {
            out.dynamic.insert(p);
        }
    }
    out
}

/// Resolves the relative specifier `spec` (`./x.js`, `../x.js`) against
/// the file `from` (`pages/home.js`), both relative to `/assets/`;
/// `None` for anything else, or a path that leaves the assets.
fn resolve(from: &str, spec: &str) -> Option<String> {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for seg in spec.split('/') {
        match seg {
            "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// The stem of a hashed chunk path: `pages/home` for
/// `pages/home.3f2a1b9c.js`.
fn stem(path: &str) -> &str {
    let without = path.strip_suffix(".js").unwrap_or(path);
    match without.rsplit_once('.') {
        Some((s, h)) if h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()) => s,
        _ => without,
    }
}

/// Links an app's release client: shakes, minifies and renames every
/// JavaScript file of `files` and adds its source map (see the module
/// documentation). CSS and other files pass through. Returns the files
/// sorted by path.
pub(crate) fn link(files: Vec<OutputFile>) -> Vec<OutputFile> {
    let graph = module_graph(&files);
    // What each chunk must keep exporting: `None` keeps everything.
    let mut keep: BTreeMap<String, Option<BTreeSet<String>>> = BTreeMap::new();
    for path in graph.keys() {
        keep.insert(path.clone(), None);
    }
    let mut imported: BTreeSet<&String> = BTreeSet::new();
    let mut whole: BTreeSet<&String> = BTreeSet::new();
    for imports in graph.values() {
        imported.extend(imports.static_.iter());
        imported.extend(imports.dynamic.iter());
        whole.extend(imports.whole.iter());
        whole.extend(imports.dynamic.iter());
    }
    for path in graph.keys() {
        if !imported.contains(path) || whole.contains(path) {
            continue;
        }
        let names: BTreeSet<String> = graph
            .values()
            .filter_map(|i| i.names.get(path))
            .flatten()
            .cloned()
            .collect();
        keep.insert(path.clone(), Some(names));
    }
    // Dependencies first (a depth-first post-order over the graph), so a
    // chunk's imports already carry their final names when it is hashed.
    let mut order: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    fn visit(
        p: &str,
        graph: &BTreeMap<String, Imports>,
        seen: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) {
        if !seen.insert(p.to_owned()) {
            return;
        }
        if let Some(i) = graph.get(p) {
            for d in i
                .static_
                .iter()
                .chain(i.dynamic.iter())
                .chain(i.urls.iter())
            {
                visit(d, graph, seen, order);
            }
        }
        order.push(p.to_owned());
    }
    for p in graph.keys() {
        visit(p, &graph, &mut seen, &mut order);
    }
    let mut texts: BTreeMap<String, String> = files
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript)
        .map(|f| (f.path.clone(), f.contents.clone()))
        .collect();
    // The K maps the bundler left next to the chunks that hold K code
    // (`<chunk>.map`): composed with the minifier's below.
    let mut kmaps: BTreeMap<String, String> = BTreeMap::new();
    let mut out: Vec<OutputFile> = Vec::new();
    for f in files.into_iter().filter(|f| f.kind != FileKind::JavaScript) {
        match f.path.strip_suffix(".map") {
            Some(js) if f.kind == FileKind::SourceMap && texts.contains_key(js) => {
                kmaps.insert(js.to_owned(), f.contents);
            }
            _ => out.push(f),
        }
    }
    for path in order {
        let Some(text) = texts.remove(&path) else {
            continue;
        };
        let stem = stem(&path).to_owned();
        let base = |p: &str| p.rsplit('/').next().unwrap_or(p).to_owned();
        let source_name = format!("{}.js", base(&stem));
        let keep = keep.get(&path).cloned().flatten();
        let (code, map) = match crate::minify::minify(&text, &source_name, keep.as_ref()) {
            Ok(m) => (m.code, Some(release_map(&m.map, kmaps.get(&path)))),
            // A chunk oxc can't read ships as printed: still correct, only
            // larger (`minify`'s errors are code generator bugs).
            Err(_) => (text, None),
        };
        // The map is named after its own text (`stem.<hash>.js.map`), so
        // the chunk can name it and still be hashed as it ships, comment
        // included: every client file's name is the hash of its bytes.
        let mut contents = code;
        if let Some(map) = map {
            let map_path = crate::hash::hashed_file_name(&stem, "js.map", map.as_bytes());
            contents.push_str(&format!("//# sourceMappingURL={}\n", base(&map_path)));
            out.push(OutputFile {
                path: map_path,
                contents: map,
                kind: FileKind::SourceMap,
            });
        }
        let new_path = crate::hash::hashed_file_name(&stem, "js", contents.as_bytes());
        // The chunks still to come name this one by its final name. File
        // names are unique (`stem.<hash>.js`), so the base name is enough.
        let (old_base, new_base) = (base(&path), base(&new_path));
        if old_base != new_base {
            for t in texts.values_mut() {
                if t.contains(&old_base) {
                    *t = t.replace(&old_base, &new_base);
                }
            }
        }
        out.push(OutputFile {
            path: new_path,
            contents,
            kind: FileKind::JavaScript,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The shipped map of a minified chunk: oxc's map (minified → readable
/// chunk) composed with the chunk's K map (readable chunk → `.k`) when it
/// holds K code, so minified code maps straight to the `.k` files; else
/// oxc's map with its one source (the runtime, a feature chunk or the
/// `app` entry, embedded) on the `ignoreList`.
fn release_map(oxc: &str, kmap: Option<&String>) -> String {
    use crate::sourcemap::{SourceMap, compose, to_json};
    match (SourceMap::parse(oxc), kmap.map(|k| SourceMap::parse(k))) {
        (Ok(outer), Some(Ok(inner))) => to_json(&compose(&outer, &inner), None),
        (Ok(mut outer), None) => {
            outer.ignore_list = (0..u32::try_from(outer.sources.len()).unwrap_or(0)).collect();
            to_json(&outer, None)
        }
        _ => oxc.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn js(path: &str, text: &str) -> OutputFile {
        OutputFile {
            path: path.into(),
            contents: text.into(),
            kind: FileKind::JavaScript,
        }
    }

    #[test]
    fn resolves_relative_specifiers_between_chunks() {
        assert_eq!(
            resolve("pages/home.js", "../runtime.js").as_deref(),
            Some("runtime.js")
        );
        assert_eq!(
            resolve("app.js", "./pages/home.js").as_deref(),
            Some("pages/home.js")
        );
        assert_eq!(resolve("app.js", "../x.js"), None);
        assert_eq!(resolve("app.js", "k:runtime"), None);
        assert_eq!(stem("pages/home.3f2a1b9c.js"), "pages/home");
        assert_eq!(stem("app.js"), "app");
    }

    #[test]
    fn the_graph_records_named_whole_and_dynamic_imports() {
        let files = vec![
            js(
                "runtime.00000000.js",
                "export function a() {}\nexport function b() {}\n",
            ),
            js(
                "shared.00000000.js",
                "import { a as x } from \"./runtime.00000000.js\";\nexport const s = x;\n",
            ),
            js(
                "pages/home.00000000.js",
                "import * as k from \"../runtime.00000000.js\";\nexport const h = k;\n",
            ),
            js(
                "app.00000000.js",
                "import { s } from \"./shared.00000000.js\";\nconst p = () => import(\"./pages/home.00000000.js\");\n",
            ),
        ];
        let g = module_graph(&files);
        let shared = &g["shared.00000000.js"];
        assert_eq!(shared.names["runtime.00000000.js"], ["a".to_owned()].into());
        assert!(
            g["pages/home.00000000.js"]
                .whole
                .contains("runtime.00000000.js")
        );
        assert!(
            g["app.00000000.js"]
                .dynamic
                .contains("pages/home.00000000.js")
        );
        assert!(g["app.00000000.js"].static_.contains("shared.00000000.js"));
    }

    #[test]
    fn shakes_named_imports_renames_and_maps_every_chunk() {
        let files = vec![
            js(
                "runtime.00000000.js",
                "export function used() { return \"kept-body\"; }\nexport function unused() { return \"dropped-body\"; }\n",
            ),
            js(
                "app.00000000.js",
                "import { used as $k$used } from \"./runtime.00000000.js\";\ndocument.title = $k$used();\n",
            ),
        ];
        let out = link(files);
        let runtime = out
            .iter()
            .find(|f| f.path.starts_with("runtime.") && f.path.ends_with(".js"))
            .expect("runtime chunk");
        assert!(
            runtime.contents.contains("kept-body"),
            "{}",
            runtime.contents
        );
        assert!(
            !runtime.contents.contains("dropped-body"),
            "{}",
            runtime.contents
        );
        assert_ne!(
            runtime.path, "runtime.00000000.js",
            "renamed after its minified text"
        );
        let app = out
            .iter()
            .find(|f| f.path.starts_with("app.") && f.path.ends_with(".js"))
            .expect("app chunk");
        assert!(app.contents.contains(&runtime.path), "{}", app.contents);
        let map = app
            .contents
            .rsplit_once("//# sourceMappingURL=")
            .map(|(_, m)| m.trim_end().to_owned())
            .expect("a map comment");
        assert!(map.starts_with("app.") && map.ends_with(".js.map"), "{map}");
        assert!(
            out.iter()
                .any(|f| f.path == map && f.kind == FileKind::SourceMap)
        );
        // Every name is the hash of the file as it ships.
        for f in &out {
            let (stem, ext) = if f.kind == FileKind::SourceMap {
                (f.path.split('.').next().unwrap_or(""), "js.map")
            } else {
                (f.path.split('.').next().unwrap_or(""), "js")
            };
            assert_eq!(
                f.path,
                crate::hash::hashed_file_name(stem, ext, f.contents.as_bytes())
            );
        }
    }

    /// A `@Worker` service's chunk is named by its proxy with `new URL(…)`
    /// (docs/design/55-workers.md): renamed before the chunk that names
    /// it, which then carries its final name, though it doesn't import it.
    #[test]
    fn a_chunk_named_by_a_url_is_renamed_before_the_chunk_naming_it() {
        let files = vec![
            js(
                "shared.00000000.js",
                "export const open = () => new Worker(new URL(\"./workers/sieve.00000000.js\", import.meta.url), { type: \"module\" });\n",
            ),
            js(
                "workers/sieve.00000000.js",
                "globalThis.onmessage = () => {};\n",
            ),
        ];
        assert!(
            module_graph(&files)["shared.00000000.js"]
                .urls
                .contains("workers/sieve.00000000.js")
        );
        let out = link(files);
        let worker = out
            .iter()
            .find(|f| f.path.starts_with("workers/sieve.") && f.path.ends_with(".js"))
            .expect("worker chunk");
        assert_ne!(worker.path, "workers/sieve.00000000.js");
        let shared = out
            .iter()
            .find(|f| f.path.starts_with("shared.") && f.path.ends_with(".js"))
            .expect("shared chunk");
        let base = worker.path.rsplit('/').next().unwrap_or_default();
        assert!(shared.contents.contains(base), "{}", shared.contents);
    }

    #[test]
    fn a_namespace_import_keeps_every_export() {
        let files = vec![
            js(
                "runtime.00000000.js",
                "export function one() { return \"one-body\"; }\nexport function two() { return \"two-body\"; }\n",
            ),
            js(
                "app.00000000.js",
                "import * as $k from \"./runtime.00000000.js\";\nwindow.k = $k;\n",
            ),
        ];
        let out = link(files);
        let runtime = out
            .iter()
            .find(|f| f.path.starts_with("runtime."))
            .expect("runtime");
        assert!(runtime.contents.contains("one-body") && runtime.contents.contains("two-body"));
    }
}
