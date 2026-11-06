//! Fuzzes the parser with arbitrary UTF-8 for throughput: no panic, a
//! `SourceFile` root, valid diagnostics and a bounded number of them
//! (`oracle::check_fast`). Inputs that are not UTF-8 are skipped.
#![no_main]

use k_fuzz::oracle;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    oracle::check_fast(text);
});
