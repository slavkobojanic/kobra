//! Fuzzes semantic analysis with arbitrary UTF-8: the text is the only
//! source file of a project, at a path chosen by the first byte (a module
//! file, a test file, `App.k` or a stray file). Checking must not panic,
//! every diagnostic must be well-formed (`soracle::check_diagnostics`) and
//! the position queries must answer on every token. Inputs that are not
//! UTF-8 after the first byte are skipped.
#![no_main]

use k_fuzz::soracle;
use libfuzzer_sys::fuzz_target;

const PATHS: &[&str] = &["src/fz/Case.k", "src/fz/Case.test.k", "App.k", "notes/Stray.k"];

fuzz_target!(|data: &[u8]| {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    let files = [
        ("k.toml".to_owned(), soracle::MANIFEST.to_owned()),
        (PATHS[usize::from(first) % PATHS.len()].to_owned(), text.to_owned()),
    ];
    let host = soracle::host(&files, k_sema::Options::new());
    let analysis = host.analysis();
    soracle::check_analysis(&analysis, &files);
    soracle::check_queries(&analysis, 400);
});
