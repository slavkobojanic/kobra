//! Structure-aware fuzzing of semantic analysis with the grammar generator
//! (`kgen`): the bytes drive one to three programs, analysed as one project
//! in two modules. Diagnostics must be well-formed, deterministic (a fresh
//! and a single-threaded analysis agree) and independent of the order the
//! files are added; after edits read from the remaining bytes, the
//! incremental re-check must equal a fresh check.
#![no_main]

use arbitrary::Unstructured;
use k_fuzz::{kgen, soracle};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let n = 1 + u.choose_index(3).unwrap_or(0);
    let mut files = vec![("k.toml".to_owned(), soracle::MANIFEST.to_owned())];
    for path in ["src/a/A.k", "src/a/B.k", "src/b/C.k"].iter().take(n) {
        files.push(((*path).to_owned(), kgen::generate(&mut u).source));
    }
    let diagnostics = soracle::check(&files);
    soracle::check_deterministic(&files, &diagnostics);
    let perm = soracle::permutation(&mut u, files.len());
    soracle::check_order_independent(&files, &perm, &diagnostics);
    let mut current = files.clone();
    let mut edits = Vec::new();
    for _ in 0..1 + u.choose_index(3).unwrap_or(0) {
        let edit = soracle::random_edit(&current, &mut u);
        soracle::apply_edit(&mut current, &edit);
        edits.push(edit);
    }
    soracle::check_incremental(&files, &edits);
});
