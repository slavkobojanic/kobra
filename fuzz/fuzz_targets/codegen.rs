//! Code generation of well-typed generated projects (`tgen`, in both
//! modes): generation succeeds in every profile, is deterministic (a fresh
//! analysis generates byte-identical output), and puts no server code in
//! the client bundle (`coracle::check_codegen`). Nothing runs in V8, so
//! this target is fast; `codegen_run` executes what is generated.
#![no_main]

use arbitrary::Unstructured;
use k_codegen::Options;
use k_fuzz::{coracle, tgen};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let (runnable, rest) = match data.split_first() {
        Some((b, rest)) => (b % 2 == 1, rest),
        None => (false, data),
    };
    let mut u = Unstructured::new(rest);
    let program = if runnable {
        tgen::generate_runnable(&mut u)
    } else {
        tgen::generate(&mut u)
    };
    let options = |profile| Options {
        profile,
        ..Options::default()
    };
    if let Err(e) = coracle::check_codegen(&program.files, &options, &[]) {
        panic!("{e}");
    }
});
