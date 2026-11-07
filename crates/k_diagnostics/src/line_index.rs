//! Conversion between byte offsets and human-facing line/column positions.

use std::fmt;

use text_size::TextSize;

/// A 1-based line and 1-based column.
///
/// Columns count Unicode scalar values (Rust `char`s), not bytes and not
/// UTF-16 code units. This is the convention used in every human-facing
/// output of the toolchain, including the `.diag` golden files described in
/// `docs/compiler/testing.md`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineCol {
    /// The line number, starting at 1.
    pub line: u32,
    /// The column number, starting at 1, in Unicode scalar values.
    pub col: u32,
}

impl fmt::Display for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// An index of line starts for one source text.
///
/// Building the index is a single linear scan over the bytes of the text.
/// Lines longer than 1024 bytes (minified or generated sources)
/// additionally get a *checkpoint* about every 1024 bytes that records the
/// column at that offset, so a lookup is a binary search over line starts
/// (and checkpoints) followed by a scan of at most about 1024 bytes, however
/// long the line is. Many diagnostics on
/// one long line therefore cost O(N log N), not O(N × line length).
///
/// Line terminators are `\n`, `\r\n` and a lone `\r`, matching the lexer's
/// definition of a newline (see `docs/compiler/grammar.md`).
///
/// # Example
///
/// ```
/// use k_diagnostics::{LineCol, LineIndex, TextSize};
///
/// let text = "let a = 1\r\nlet é = 2\n";
/// let index = LineIndex::new(text);
/// assert_eq!(index.line_count(), 3);
///
/// // `=` on the second line is at byte 18 but column 7: `é` is two bytes.
/// let at = index.line_col(text, TextSize::from(18));
/// assert_eq!(at, LineCol { line: 2, col: 7 });
/// assert_eq!(index.offset(text, at), Some(TextSize::from(18)));
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LineIndex {
    /// Byte offset of the first character of every line. Always starts with 0.
    line_starts: Vec<u32>,
    /// Total length of the text in bytes.
    len: u32,
    /// `(byte offset, columns before it on its line)` pairs at roughly every
    /// [`CHECKPOINT_STRIDE`] bytes of each long line, in increasing offset
    /// order. Every checkpoint offset is a char boundary strictly inside its
    /// line (never a line start).
    checkpoints: Vec<(u32, u32)>,
}

/// The distance in bytes between column checkpoints on long lines, and so
/// the most bytes one [`LineIndex::line_col`] lookup scans.
const CHECKPOINT_STRIDE: usize = 1024;

impl LineIndex {
    /// Builds the line index for `text`.
    ///
    /// Texts longer than `u32::MAX` bytes are not supported by the toolchain;
    /// offsets beyond that are clamped.
    #[must_use]
    pub fn new(text: &str) -> LineIndex {
        let bytes = text.as_bytes();
        let mut line_starts = Vec::with_capacity(bytes.len() / 32 + 1);
        line_starts.push(0);
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => line_starts.push(clamp(i + 1)),
                b'\r' => {
                    if bytes.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                    line_starts.push(clamp(i + 1));
                }
                _ => {}
            }
            i += 1;
        }
        let checkpoints = checkpoints(text, &line_starts);
        LineIndex {
            line_starts,
            len: clamp(bytes.len()),
            checkpoints,
        }
    }

    /// Returns the number of lines in the text. An empty text has one line.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Returns the byte offset at which the 1-based `line` starts, or `None`
    /// if the line does not exist.
    #[must_use]
    pub fn line_start(&self, line: u32) -> Option<TextSize> {
        let index = usize::try_from(line.checked_sub(1)?).ok()?;
        self.line_starts
            .get(index)
            .map(|&start| TextSize::from(start))
    }

    /// Converts a byte offset into a [`LineCol`].
    ///
    /// `text` must be the text this index was built from. Offsets past the
    /// end of the text are clamped to the end. An offset that falls inside a
    /// multi-byte character, or between the `\r` and `\n` of a CRLF pair, is
    /// attributed to the character that contains it.
    #[must_use]
    pub fn line_col(&self, text: &str, offset: TextSize) -> LineCol {
        let offset = u32::from(offset).min(self.len);
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let line_start = self.line_starts[line];
        // Start counting at the last checkpoint of this line at or before
        // the offset, if there is one.
        let (start, before) = if (offset - line_start) as usize > CHECKPOINT_STRIDE {
            let index = self.checkpoints.partition_point(|&(at, _)| at <= offset);
            match index.checked_sub(1).map(|i| self.checkpoints[i]) {
                Some((at, cols)) if at > line_start => (at as usize, cols as usize),
                _ => (line_start as usize, 0),
            }
        } else {
            (line_start as usize, 0)
        };
        let end = offset as usize;
        let col = before
            + text
                .get(start..)
                .map(|rest| {
                    rest.char_indices()
                        .take_while(|&(i, c)| start + i + c.len_utf8() <= end)
                        .count()
                })
                .unwrap_or(0);
        LineCol {
            line: clamp(line + 1),
            col: clamp(col + 1),
        }
    }

    /// Converts a [`LineCol`] back into a byte offset.
    ///
    /// Returns `None` if the line does not exist. A column past the end of
    /// the line yields the offset of the line's end (before its terminator).
    #[must_use]
    pub fn offset(&self, text: &str, position: LineCol) -> Option<TextSize> {
        let start = u32::from(self.line_start(position.line)?) as usize;
        let line_text = text.get(start..)?;
        let line_len = line_text.find(['\n', '\r']).unwrap_or(line_text.len());
        let wanted = position.col.saturating_sub(1) as usize;
        let within = line_text[..line_len]
            .char_indices()
            .nth(wanted)
            .map_or(line_len, |(i, _)| i);
        Some(TextSize::from(clamp(start + within)))
    }
}

/// Computes the column checkpoints of every line longer than
/// [`CHECKPOINT_STRIDE`] bytes. Short lines, the common case, cost one
/// subtraction each.
fn checkpoints(text: &str, line_starts: &[u32]) -> Vec<(u32, u32)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for (index, &start) in line_starts.iter().enumerate() {
        let start = start as usize;
        let end = line_starts
            .get(index + 1)
            .map_or(bytes.len(), |&next| next as usize);
        if end - start <= CHECKPOINT_STRIDE {
            continue;
        }
        let mut at = start;
        let mut cols = 0usize;
        loop {
            let mut next = at + CHECKPOINT_STRIDE;
            if next >= end {
                break;
            }
            while !text.is_char_boundary(next) {
                next += 1;
            }
            // Count char starts (bytes that are not UTF-8 continuation bytes).
            cols += bytes[at..next]
                .iter()
                .filter(|&&b| (b & 0xC0) != 0x80)
                .count();
            out.push((clamp(next), clamp(cols)));
            at = next;
        }
    }
    out
}

/// Clamps a `usize` to `u32`, saturating at `u32::MAX`.
fn clamp(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lc(index: &LineIndex, text: &str, offset: u32) -> (u32, u32) {
        let p = index.line_col(text, offset.into());
        (p.line, p.col)
    }

    #[test]
    fn empty_text_has_one_line() {
        let index = LineIndex::new("");
        assert_eq!(index.line_count(), 1);
        assert_eq!(lc(&index, "", 0), (1, 1));
    }

    #[test]
    fn lf_crlf_and_cr_all_end_lines() {
        let text = "a\nb\r\nc\rd";
        let index = LineIndex::new(text);
        assert_eq!(index.line_count(), 4);
        assert_eq!(lc(&index, text, 0), (1, 1));
        assert_eq!(lc(&index, text, 2), (2, 1));
        assert_eq!(lc(&index, text, 5), (3, 1));
        assert_eq!(lc(&index, text, 7), (4, 1));
    }

    #[test]
    fn columns_count_chars_not_bytes() {
        let text = "é€😀x";
        let index = LineIndex::new(text);
        let x = text.find('x').unwrap() as u32;
        assert_eq!(lc(&index, text, x), (1, 4));
    }

    #[test]
    fn offsets_past_end_are_clamped() {
        let text = "ab";
        let index = LineIndex::new(text);
        assert_eq!(lc(&index, text, 99), (1, 3));
    }

    #[test]
    fn offset_inside_multibyte_char_maps_to_that_char() {
        let text = "aé";
        let index = LineIndex::new(text);
        assert_eq!(lc(&index, text, 2), (1, 2));
    }

    #[test]
    fn offset_round_trips_through_line_col() {
        let text = "let a = 1\n  é = \"x\"\r\nend";
        let index = LineIndex::new(text);
        for (offset, _) in text.char_indices() {
            let p = index.line_col(text, (offset as u32).into());
            let back = index.offset(text, p).unwrap();
            let ch = text[offset..].chars().next().unwrap();
            if ch != '\n' && ch != '\r' {
                assert_eq!(u32::from(back) as usize, offset, "at {p}");
            }
        }
    }

    #[test]
    fn checkpoints_agree_with_a_full_scan_on_long_mixed_lines() {
        // Multi-byte chars straddle the checkpoint strides, and a CRLF pair
        // and a short line separate two long lines.
        let line = "ab€😀c".repeat(250);
        let text = format!("{line}\r\nshort\n{line}é");
        let index = LineIndex::new(&text);
        assert!(!index.checkpoints.is_empty());
        let naive = |offset: usize| {
            let start = text[..offset].rfind(['\n', '\r']).map_or(0, |i| i + 1);
            let start = start.min(offset);
            let col = text[start..]
                .char_indices()
                .take_while(|&(i, c)| start + i + c.len_utf8() <= offset)
                .count();
            col as u32 + 1
        };
        for offset in (0..=text.len()).filter(|&o| text.is_char_boundary(o)) {
            let p = index.line_col(&text, (offset as u32).into());
            if text[..offset].ends_with('\r') {
                // Between `\r` and `\n`: attributed to the first line.
                continue;
            }
            assert_eq!(p.col, naive(offset), "offset {offset}");
        }
    }

    #[test]
    fn offset_of_missing_line_is_none() {
        let index = LineIndex::new("a");
        assert_eq!(index.offset("a", LineCol { line: 2, col: 1 }), None);
        assert_eq!(index.offset("a", LineCol { line: 0, col: 1 }), None);
    }
}
