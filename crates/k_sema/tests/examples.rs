//! The example apps in `examples/` as end-to-end inputs of the analysis.
//!
//! Every example must analyse without a single diagnostic, and every stdlib
//! declaration an example imports must exist and be public, so the embedded
//! standard library (`crates/k_sema/stdlib`) always covers what the
//! examples use. As the semantic stages land, the first test becomes the
//! strongest check the repository has.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use k_sema::stdlib::Stdlib;
use k_sema::{AnalysisHost, FileKind, Options, ProjectPath, Workspace};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn examples() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(examples_dir())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("k.toml").is_file())
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no example projects found");
    found
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') && name != "dist" && name != "target" {
                collect(&path, root, out);
            }
            continue;
        }
        let relative: Vec<String> = path
            .strip_prefix(root)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let project_path = ProjectPath::new(&relative.join("/")).unwrap();
        // A workspace's member manifests and lockfiles are classified
        // relative to their member, by `Workspace::from_files`.
        let name = project_path.file_name();
        let member_file = name == "k.toml" || name.ends_with(".lock");
        if project_path.kind() != FileKind::Other || member_file {
            out.push((project_path, fs::read_to_string(&path).unwrap()));
        }
    }
}

fn project_files(root: &Path) -> Vec<(ProjectPath, String)> {
    let mut files = Vec::new();
    collect(root, root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// The analyses of an example: one, or for a workspace (`examples/stack`)
/// one per member with the libraries it depends on.
fn analyses(example: &Path) -> Vec<k_sema::Analysis> {
    let files = project_files(example);
    let root = files.iter().find(|(p, _)| p.as_str() == "k.toml").unwrap();
    if !Workspace::is_workspace_manifest(&root.1) {
        return vec![AnalysisHost::with_files(Options::new(), files).analysis()];
    }
    let ws = Workspace::from_files(files.into_iter().map(|(p, t)| (p.as_str().to_owned(), t)));
    assert!(
        ws.problems.is_empty(),
        "{}: {:?}",
        example.display(),
        ws.problems
    );
    (0..ws.members.len())
        .map(|m| AnalysisHost::with_files(Options::new(), ws.analysis_files(m)).analysis())
        .collect()
}

#[test]
fn every_example_analyses_without_diagnostics() {
    for example in examples() {
        for analysis in analyses(&example) {
            let diagnostics = analysis.diagnostics().unwrap();
            if !diagnostics.is_empty() {
                let map = analysis.source_map(ToString::to_string);
                let rendered: Vec<String> = diagnostics
                    .iter()
                    .map(|d| {
                        k_diagnostics::render::human(
                            d,
                            &map,
                            &k_diagnostics::render::HumanOptions::plain(),
                        )
                    })
                    .collect();
                panic!("{}:\n{}", example.display(), rendered.join("\n"));
            }
        }
    }
}

#[test]
fn every_stdlib_import_of_the_examples_exists_and_is_public() {
    let stdlib = Stdlib::get();
    let mut missing = Vec::new();
    let mut checked = 0;
    for example in examples() {
        for (path, text) in project_files(&example) {
            for line in text.lines() {
                let Some(name) = line.trim().strip_prefix("import ") else {
                    continue;
                };
                let name = name.split_whitespace().next().unwrap_or_default();
                if !name.starts_with("k.") {
                    continue;
                }
                checked += 1;
                match stdlib.lookup(name) {
                    Some(declaration) if declaration.is_public => {}
                    Some(_) => missing.push(format!("{path}: `{name}` is not public")),
                    None => missing.push(format!("{path}: `{name}` is not in the stdlib")),
                }
            }
        }
    }
    assert!(checked > 0, "the examples import nothing from the stdlib");
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}
