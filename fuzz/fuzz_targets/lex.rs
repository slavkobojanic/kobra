//! Fuzzes the lexer with arbitrary bytes.
//!
//! Bytes that are not UTF-8 are decoded lossily (invalid sequences become
//! U+FFFD, which the lexer must treat as an unexpected character), so every
//! mutation reaches the lexer. Asserts the token-stream invariants of
//! `oracle::check_lexed` and valid diagnostics.
#![no_main]

use k_fuzz::oracle;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let lexed = k_syntax::lex(oracle::FILE, &text);
    oracle::check_lexed(&text, &lexed);
    oracle::check_diagnostics(&text, lexed.diagnostics(), "lexer");
});
