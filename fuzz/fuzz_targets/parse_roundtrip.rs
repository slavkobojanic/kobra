//! Fuzzes the parser with arbitrary UTF-8 and checks every invariant
//! (`oracle::check_all`): the tree text equals the input exactly, the tree's
//! tokens are the lexer's tokens, re-lexing reproduces the stream, the
//! golden dumps read back, and parsing is deterministic. Inputs that are not
//! UTF-8 are skipped.
#![no_main]

use k_fuzz::oracle;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let parse = oracle::check_all(text);
    // The headline property, stated once more on its own.
    assert_eq!(
        parse.syntax().text().to_string(),
        text,
        "tree is not lossless"
    );
});
