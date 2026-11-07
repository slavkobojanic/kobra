//! The embedded runtime as a whole: the module table matches the files on
//! disk, every file compacts, public JavaScript is documented, resolution
//! follows the documented table, and the client runtime meets its size
//! budgets (`docs/compiler/runtime.md`, "Size budget").

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use k_runtime::{
    CORE_SIZE_BUDGET, FULL_SIZE_BUDGET, Part, STDLIB_SIZE_BUDGET, compact, compacted_sources,
    modules, resolve,
};

fn js_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("js")
}

fn collect(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
    for entry in fs::read_dir(dir).expect("readable js directory") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            collect(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "js") {
            let rel = path.strip_prefix(root).expect("under root");
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn the_module_table_lists_exactly_the_runtime_files() {
    let mut on_disk = BTreeSet::new();
    collect(&js_root(), &js_root(), &mut on_disk);
    let listed: BTreeSet<String> = modules().iter().map(|m| m.path.to_owned()).collect();
    assert_eq!(
        listed, on_disk,
        "update MODULES in crates/k_runtime/src/lib.rs"
    );
    for m in modules() {
        assert_eq!(m.specifier, format!("k:/{}", m.path));
    }
}

#[test]
fn every_runtime_file_compacts() {
    for m in modules() {
        if let Err(e) = compact(m.source) {
            panic!("{}: {e}", m.path);
        }
    }
}

#[test]
fn every_export_has_a_jsdoc_comment() {
    for m in modules() {
        let lines: Vec<&str> = m.source.lines().collect();
        assert!(
            m.source.starts_with("/**"),
            "{} starts with a module JSDoc comment",
            m.path
        );
        for (i, line) in lines.iter().enumerate() {
            let declares = [
                "export function ",
                "export async function ",
                "export class ",
                "export const ",
            ]
            .iter()
            .any(|p| line.starts_with(p));
            if !declares {
                continue;
            }
            let previous = lines[..i]
                .iter()
                .rev()
                .find(|l| !l.trim().is_empty())
                .copied()
                .unwrap_or("");
            assert!(
                previous.trim_end().ends_with("*/"),
                "{}:{}: `{}` needs a JSDoc comment",
                m.path,
                i + 1,
                line.trim()
            );
        }
    }
}

#[test]
fn short_names_and_relative_imports_resolve() {
    assert_eq!(resolve("k:runtime", None), Some("k:/runtime/index.js"));
    assert_eq!(
        resolve("k:runtime/server", None),
        Some("k:/runtime/server.js")
    );
    assert_eq!(resolve("k:testing", None), Some("k:/testing/harness.js"));
    assert_eq!(
        resolve("./signals.js", Some("k:/runtime/dom.js")),
        Some("k:/runtime/signals.js")
    );
    assert_eq!(
        resolve("../runtime/index.js", Some("k:/testing/harness.js")),
        Some("k:/runtime/index.js")
    );
    assert_eq!(
        resolve("k:/runtime/dom.js", None),
        Some("k:/runtime/dom.js")
    );
}

#[test]
fn unknown_and_escaping_imports_do_not_resolve() {
    assert_eq!(resolve("k:nothing", None), None);
    assert_eq!(resolve("./missing.js", Some("k:/runtime/dom.js")), None);
    assert_eq!(
        resolve("../../../etc/passwd", Some("k:/runtime/dom.js")),
        None
    );
    assert_eq!(resolve("./signals.js", None), None);
    assert_eq!(resolve("./signals.js", Some("file:///x.js")), None);
    assert_eq!(resolve("", None), None);
}

#[test]
fn every_relative_import_in_the_runtime_resolves() {
    for m in modules() {
        for line in m.source.lines() {
            let Some(from) = line.split(" from \"").nth(1) else {
                continue;
            };
            let Some(spec) = from.split('"').next() else {
                continue;
            };
            assert!(
                resolve(spec, Some(m.specifier)).is_some(),
                "{}: import {spec:?} doesn't resolve",
                m.path
            );
        }
    }
}

#[test]
fn testing_and_server_modules_are_not_client_parts() {
    for m in modules() {
        if m.path.starts_with("testing/") {
            assert_eq!(m.part, Part::Testing, "{}", m.path);
        }
        if matches!(
            m.part,
            Part::Core | Part::Extended | Part::ClientEntry | Part::Stdlib
        ) {
            assert!(
                !m.source.contains("Deno.core"),
                "{} is client code but uses Deno.core",
                m.path
            );
            assert!(
                !m.source.contains("k:testing"),
                "{} is client code but imports the test harness",
                m.path
            );
        }
    }
}

/// The bytes of the uncompacted sources of `parts`, as written.
fn raw_len(parts: &[Part]) -> usize {
    modules()
        .iter()
        .filter(|m| parts.contains(&m.part))
        .map(|m| m.source.len())
        .sum()
}

fn gzip_len(s: &str) -> usize {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(s.as_bytes()).expect("gzip");
    encoder.finish().expect("gzip").len()
}

#[test]
fn the_core_runtime_meets_its_size_budget() {
    let core = compacted_sources(&[Part::Core]).expect("compacts");
    let size = gzip_len(&core);
    eprintln!(
        "core runtime: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {CORE_SIZE_BUDGET})",
        raw_len(&[Part::Core]),
        core.len()
    );
    assert!(
        size <= CORE_SIZE_BUDGET,
        "core runtime is {size} bytes gzip, over the {CORE_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_full_client_runtime_meets_its_size_budget() {
    let all =
        compacted_sources(&[Part::Core, Part::Extended, Part::ClientEntry]).expect("compacts");
    let size = gzip_len(&all);
    eprintln!(
        "client runtime: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {FULL_SIZE_BUDGET})",
        raw_len(&[Part::Core, Part::Extended, Part::ClientEntry]),
        all.len()
    );
    assert!(
        size <= FULL_SIZE_BUDGET,
        "client runtime is {size} bytes gzip, over the {FULL_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_stdlib_bodies_meet_their_size_budget() {
    let std = compacted_sources(&[Part::Stdlib]).expect("compacts");
    let size = gzip_len(&std);
    eprintln!(
        "stdlib bodies: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {STDLIB_SIZE_BUDGET})",
        raw_len(&[Part::Stdlib]),
        std.len()
    );
    assert!(
        size <= STDLIB_SIZE_BUDGET,
        "the stdlib bodies are {size} bytes gzip, over the {STDLIB_SIZE_BUDGET}-byte budget"
    );
}

/// The client runtime ships as one module (the runtime chunk), so the
/// top-level names of its files must not collide.
#[test]
fn client_runtime_files_share_no_top_level_names() {
    let mut seen: std::collections::BTreeMap<String, &str> = std::collections::BTreeMap::new();
    for m in modules() {
        if !matches!(m.part, Part::Core | Part::Extended | Part::Stdlib) {
            continue;
        }
        for line in m.source.lines() {
            let rest = line.strip_prefix("export ").unwrap_or(line);
            let rest = rest.strip_prefix("async ").unwrap_or(rest);
            let name = ["function ", "class ", "const ", "let "]
                .iter()
                .find_map(|k| rest.strip_prefix(k))
                .map(|r| {
                    r.trim_start_matches('*')
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                        .collect::<String>()
                });
            let Some(name) = name.filter(|n| !n.is_empty()) else {
                continue;
            };
            if let Some(other) = seen.insert(name.clone(), m.path) {
                assert_eq!(
                    other, m.path,
                    "`{name}` is declared in {other} and {}",
                    m.path
                );
            }
        }
    }
}
