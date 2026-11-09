//! Property tests of the server, on stable with proptest: random edit
//! sequences and requests on documents of the example apps, the same
//! driver the nightly `lsp_edits` fuzz target runs
//! (`fuzz/fuzz_targets/lsp_edits.rs`, `support/driver.rs`).
//!
//! The properties: the server never dies, every request gets exactly one
//! well-formed response (a result, or `RequestCancelled`,
//! `ContentModified`, `RequestFailed` or `InvalidParams`), and after the
//! edits the server's view of the document equals the client's: the
//! diagnostics it publishes are those of a fresh `k check` of the edited
//! text.
//!
//! `PROPTEST_CASES=N` sets the number of cases.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use crate::driver;

use std::fs;

use arbitrary::Unstructured;
use driver::{Client, example, url};
use k_sema::{AnalysisHost, Options, ProjectPath};
use proptest::prelude::*;
use serde_json::json;

/// The config for `default` cases unless `PROPTEST_CASES` is set.
fn config(default: u32) -> ProptestConfig {
    if std::env::var_os("PROPTEST_CASES").is_some() {
        ProptestConfig::default()
    } else {
        ProptestConfig::with_cases(default)
    }
}

/// The documents edited: one per example, of different shapes.
const DOCUMENTS: &[(&str, &str)] = &[
    ("todos", "src/home/Home.k"),
    ("todos", "src/todo/TodoService.k"),
    ("weather", "App.k"),
    ("notes", "src/notes/Notes.k"),
    ("shop", "shop/src/payments/StripeCheckout.k"),
    ("shop", "paybox/src/payments/TestCards.k"),
];

/// The diagnostic codes of a fresh check of the project with `path` set to
/// `text`, for `path`'s file, sorted.
fn fresh_codes(project: &str, path: &str, text: &str) -> Vec<String> {
    let root = example(project);
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    let target = ProjectPath::new(path).unwrap();
    files.retain(|(p, _)| *p != target);
    files.push((target.clone(), text.to_owned()));
    // In a workspace (`examples/shop`), the file's member is analysed with
    // the libraries it depends on, as the server does.
    let manifest = files
        .iter()
        .find(|(p, _)| p.as_str() == "k.toml")
        .map(|(_, t)| t.clone())
        .unwrap_or_default();
    let (files, target) = if k_sema::Workspace::is_workspace_manifest(&manifest) {
        let ws = k_sema::Workspace::from_files(
            files.into_iter().map(|(p, t)| (p.as_str().to_owned(), t)),
        );
        let member = ws.owner_of(path).unwrap();
        let inner = path
            .strip_prefix(&format!("{}/", ws.members[member].dir))
            .unwrap();
        (ws.analysis_files(member), ProjectPath::new(inner).unwrap())
    } else {
        (files, target)
    };
    let host = AnalysisHost::with_files(Options::new(), files);
    let analysis = host.analysis();
    let id = analysis.file_id(&target).unwrap();
    let mut codes: Vec<String> = analysis
        .file_diagnostics(id)
        .unwrap()
        .iter()
        .map(|d| d.code.to_string())
        .collect();
    codes.sort();
    codes
}

fn collect(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(ProjectPath, String)>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !name.starts_with('.') && name != "dist" && name != "target" {
                collect(&p, root, out);
            }
            continue;
        }
        let rel = p
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(pp) = ProjectPath::new(&rel) else {
            continue;
        };
        // A workspace member's manifest and lockfiles are classified
        // relative to the member, by `Workspace::from_files`.
        let member_file = name == "k.toml" || name.ends_with(".lock");
        if pp.kind() != k_sema::FileKind::Other || member_file {
            out.push((pp, fs::read_to_string(&p).unwrap()));
        }
    }
}

proptest! {
    #![proptest_config(config(24))]

    #[test]
    fn random_edits_and_requests_keep_the_server_consistent(
        doc in 0usize..DOCUMENTS.len(),
        raw in prop::collection::vec(any::<u8>(), 0..2048),
    ) {
        let (project, path) = DOCUMENTS[doc];
        let root = example(project);
        let file = root.join(path);
        let text = fs::read_to_string(&file).unwrap();
        let uri = url(&file);
        let mut client = Client::start(&root);
        client.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": text } }),
        );
        let mut u = Unstructured::new(&raw);
        let edited = driver::drive(&mut client, &mut u, &uri, text, 64);
        // The published diagnostics converge on a fresh check's.
        let expected = fresh_codes(project, path, &edited);
        let published = client.diagnostics(&uri, |d| {
            let mut codes: Vec<String> = d.iter().map(|x| x["code"].as_str().unwrap_or("").to_owned()).collect();
            codes.sort();
            codes == expected
        });
        prop_assert!(published.len() == expected.len());
        prop_assert_eq!(client.shutdown(), 0);
    }
}
