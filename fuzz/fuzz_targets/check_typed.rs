//! Structure-aware fuzzing with the typed generator (`tgen`): the bytes
//! drive a well-typed project, which must check with **zero errors** (a
//! report is a checker bug, or a generator bug triaged against
//! docs/design/08-types.md), independently of the order files are added.
//! The remaining bytes then drive edits (the incremental re-check must equal
//! a fresh check) and fixable mistakes (applying every machine-applicable
//! fix must not add an error).
#![no_main]

use arbitrary::Unstructured;
use k_fuzz::{soracle, tgen};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let program = tgen::generate(&mut u);
    let diagnostics = soracle::check(&program.files);
    let errors = soracle::errors(&diagnostics);
    assert!(
        errors.is_empty(),
        "a well-typed generated project has errors: {errors:#?}\n{}",
        program.dump()
    );
    let perm = soracle::permutation(&mut u, program.files.len());
    soracle::check_order_independent(&program.files, &perm, &diagnostics);
    let mut current = program.files.clone();
    let mut edits = Vec::new();
    for _ in 0..1 + u.choose_index(3).unwrap_or(0) {
        let edit = soracle::random_edit(&current, &mut u);
        soracle::apply_edit(&mut current, &edit);
        edits.push(edit);
    }
    soracle::check_incremental(&program.files, &edits);
    let mistaken = tgen::inject_fixable_mistakes(&program, &mut u);
    let before = soracle::check(&mistaken);
    soracle::check_fixes_add_no_errors(&mistaken, &before);
});
