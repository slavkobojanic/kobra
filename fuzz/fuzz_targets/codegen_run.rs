//! Generated programs compiled and run in V8, one isolate at a time. The
//! first byte picks the generator and its oracle:
//!
//! * 0: a terminating typed project (`tgen::generate_runnable`): no
//!   unexpected JavaScript exception, development equals release;
//! * 1: closed expressions (`exprgen`): equal to the reference evaluation,
//!   inline (foldable) and behind opaque calls;
//! * 2: `Codable` values (`codablegen`): the JSON round trip and the
//!   reference decoder on mutated JSON;
//! * 3: views (`viewgen`): server HTML parses back to itself, equals the
//!   client render, and hydrates (claiming its nodes) to the same DOM, also
//!   after a state change.
#![no_main]

use std::time::Duration;

use arbitrary::Unstructured;
use k_fuzz::{codablegen, coracle, exprgen, tgen, viewgen};
use libfuzzer_sys::fuzz_target;

const LIMIT: Duration = Duration::from_secs(10);

fuzz_target!(|data: &[u8]| {
    let Some((kind, rest)) = data.split_first() else {
        return;
    };
    let mut u = Unstructured::new(rest);
    let outcome = match kind % 4 {
        0 => {
            let program = tgen::generate_runnable(&mut u);
            coracle::check_terminating_program(&program.files, tgen::RUNNABLE_ENTRY, LIMIT)
                .map(|_| ())
        }
        1 => exprgen::check(&exprgen::generate(&mut u), LIMIT),
        2 => codablegen::check(&codablegen::generate(&mut u), LIMIT),
        _ => viewgen::check(&viewgen::generate(&mut u), LIMIT),
    };
    if let Err(e) = outcome {
        panic!("{e}");
    }
});
