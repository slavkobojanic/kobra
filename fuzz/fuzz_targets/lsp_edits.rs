//! The language server under random edits and requests: the bytes choose
//! an example document and drive incremental `didChange`s (UTF-16 ranges,
//! sometimes far outside the document, with K fragments and non-ASCII
//! text) interleaved with every kind of request, some cancelled at once
//! (`crates/k_lsp/tests/support/driver.rs`). Every request must get exactly
//! one well-formed response, and the server must keep serving.
//!
//! One server runs for the whole process; each input reopens its document
//! with the text on disk, so inputs don't depend on each other. Inputs
//! longer than `LSP_MAX_INPUT` (2 KiB) are skipped.
#![no_main]

use std::sync::{Mutex, OnceLock};

use arbitrary::Unstructured;
use k_fuzz::lspdriver::{self, Client};
use libfuzzer_sys::fuzz_target;
use serde_json::json;

/// The longest input used, in bytes.
const LSP_MAX_INPUT: usize = 2048;

/// The documents inputs edit.
const DOCUMENTS: &[(&str, &str)] = &[
    ("todos", "src/home/Home.k"),
    ("todos", "src/todo/TodoService.k"),
    ("notes", "src/notes/Notes.k"),
];

/// The process-wide server, initialized on the examples directory.
fn client() -> &'static Mutex<Client> {
    static CLIENT: OnceLock<Mutex<Client>> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let root = lspdriver::example("todos");
        let examples = root.parent().unwrap_or(&root).to_path_buf();
        Mutex::new(Client::start(&examples))
    })
}

fuzz_target!(|data: &[u8]| {
    if data.len() > LSP_MAX_INPUT {
        return;
    }
    let mut u = Unstructured::new(data);
    let (project, path) = DOCUMENTS[u.choose_index(DOCUMENTS.len()).unwrap_or(0)];
    let file = lspdriver::example(project).join(path);
    let Ok(text) = std::fs::read_to_string(&file) else {
        return;
    };
    let uri = lspdriver::url(&file);
    let mut client = match client().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": text } }),
    );
    lspdriver::drive(&mut client, &mut u, &uri, text, 64);
    client.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": uri } }),
    );
});
