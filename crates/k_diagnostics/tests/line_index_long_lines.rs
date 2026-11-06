//! `LineIndex::line_col` on very long lines (minified or generated sources, or one bad line).
//!
//! Regression: each lookup scanned its line from the start, so N diagnostics on one long line
//! cost O(N × line length). Parsing a 160 KB line of `x < x < x ...` took 11 ms, but
//! rendering its 40,000 K0113 diagnostics took over 20 s in `k check`, and 402 diagnostics
//! on a 3 MB line took 3.6 s. Lookups must not rescan the line.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::time::{Duration, Instant};

use k_diagnostics::{LineCol, LineIndex, TextSize};

fn lookups_are_fast(text: &str, offsets: &[(u32, u32)]) {
    let index = LineIndex::new(text);
    let start = Instant::now();
    for &(offset, col) in offsets {
        assert_eq!(
            index.line_col(text, TextSize::from(offset)),
            LineCol { line: 1, col },
            "offset {offset}"
        );
    }
    let elapsed = start.elapsed();
    // A lookup should cost microseconds; scanning the line costs over ten milliseconds each
    // on these 1 MB lines in a debug build. The bound is generous so that a slow debug build
    // on a busy machine still passes once lookups no longer rescan the line.
    assert!(
        elapsed < Duration::from_secs(2),
        "{} lookups on a {}-byte line took {elapsed:?}",
        offsets.len(),
        text.len()
    );
}

#[test]
fn many_lookups_on_one_long_ascii_line_are_fast() {
    let text = "ab".repeat(500_000);
    let offsets: Vec<(u32, u32)> = (0..300u32).map(|i| (i * 3_000, i * 3_000 + 1)).collect();
    lookups_are_fast(&text, &offsets);
}

#[test]
fn many_lookups_on_one_long_non_ascii_line_are_fast() {
    // Every 10-character group is 9 ASCII bytes and one two-byte `é`: 11 bytes, 10 columns.
    let text = "abcdefghié".repeat(100_000);
    let offsets: Vec<(u32, u32)> = (0..300u32).map(|i| (i * 3_300, i * 3_000 + 1)).collect();
    lookups_are_fast(&text, &offsets);
}

#[test]
fn lookups_in_descending_order_are_fast_too() {
    let text = "ab".repeat(500_000);
    let offsets: Vec<(u32, u32)> = (0..300u32)
        .rev()
        .map(|i| (i * 3_000, i * 3_000 + 1))
        .collect();
    lookups_are_fast(&text, &offsets);
}
