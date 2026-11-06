//! Structure-aware fuzzing: the input bytes drive the K program generator
//! (`kgen::generate`, built on the `arbitrary` crate), so mutations explore
//! the grammar rather than random characters.
//!
//! Every program must keep every invariant (`oracle::check_all`). Programs
//! generated in valid mode must also parse without a single diagnostic; a
//! failure there is either a parser bug (a wrong error) or a generator bug
//! (it wrote invalid syntax), and is triaged against
//! `docs/compiler/grammar.md`.
#![no_main]

use arbitrary::Unstructured;
use k_fuzz::{kgen, oracle};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let program = kgen::generate(&mut Unstructured::new(data));
    let parse = oracle::check_all(&program.source);
    if program.valid && !parse.diagnostics().is_empty() {
        panic!(
            "valid program has diagnostics: {:#?}\n--- source ---\n{}",
            parse.diagnostics(),
            program.source
        );
    }
});
