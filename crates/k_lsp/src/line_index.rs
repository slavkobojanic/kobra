//! Conversion between byte offsets and LSP positions.
//!
//! The protocol counts columns in **UTF-16 code units** unless the client
//! negotiates otherwise; this server always uses UTF-16 (`docs/compiler/lsp.md`,
//! "Positions"). The analysis counts **UTF-8 bytes**. A [`LineIndex`]
//! converts between the two for one text.
//!
//! Lines end at `\n`, `\r\n` or a lone `\r`, as the K lexer and the LSP
//! specification agree. Conversions never fail: a position past the end of
//! a line clamps to the line's end, a line past the end of the text clamps
//! to the end of the text, and an offset inside a multi-byte character (or
//! between the `\r` and `\n` of a line break) snaps back to the start of
//! the character.

use k_diagnostics::{TextRange, TextSize};
use lsp_types::{Position, Range};

/// The line starts of one text, for offset/position conversion.
#[derive(Clone, Debug)]
pub struct LineIndex {
    /// The byte offset at which each line starts; `starts[0] == 0`.
    starts: Vec<u32>,
    /// The length of the text in bytes.
    len: u32,
}

impl LineIndex {
    /// Indexes `text`. O(n) in its length.
    #[must_use]
    pub fn new(text: &str) -> LineIndex {
        let bytes = text.as_bytes();
        let mut starts = vec![0u32];
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => starts.push(to_u32(i + 1)),
                b'\r' => {
                    if bytes.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                    starts.push(to_u32(i + 1));
                }
                _ => {}
            }
            i += 1;
        }
        LineIndex {
            starts,
            len: to_u32(text.len()),
        }
    }

    /// The number of lines (a text ending in a line break has an empty last
    /// line).
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The LSP position of byte `offset` in `text` (the text this index was
    /// built from). An offset past the end maps to the end.
    #[must_use]
    pub fn position(&self, text: &str, offset: TextSize) -> Position {
        let offset = u32::from(offset).min(self.len);
        let line = match self.starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next.saturating_sub(1),
        };
        let start = self.starts[line] as usize;
        let end = (offset as usize).min(text.len());
        let mut character = 0u32;
        let mut at = start;
        if let Some(slice) = text.get(start..) {
            for c in slice.chars() {
                if at + c.len_utf8() > end || c == '\n' || c == '\r' {
                    break;
                }
                at += c.len_utf8();
                character += to_u32(c.len_utf16());
            }
        }
        Position {
            line: to_u32(line),
            character,
        }
    }

    /// The LSP range of byte range `range` in `text`.
    #[must_use]
    pub fn range(&self, text: &str, range: TextRange) -> Range {
        Range {
            start: self.position(text, range.start()),
            end: self.position(text, range.end()),
        }
    }

    /// The byte offset of LSP position `position` in `text`, clamped to the
    /// line's end and to the end of the text. A column inside a surrogate
    /// pair snaps to the start of the character.
    #[must_use]
    pub fn offset(&self, text: &str, position: Position) -> TextSize {
        let Some(&start) = self.starts.get(position.line as usize) else {
            return TextSize::from(self.len);
        };
        let start = start as usize;
        let mut units = 0u32;
        let mut at = start;
        if let Some(slice) = text.get(start..) {
            for c in slice.chars() {
                if c == '\n' || c == '\r' {
                    break;
                }
                let width = to_u32(c.len_utf16());
                if units + width > position.character {
                    break;
                }
                units += width;
                at += c.len_utf8();
            }
        }
        TextSize::from(to_u32(at))
    }

    /// The byte range of LSP range `range` in `text`; a reversed range is
    /// put in order.
    #[must_use]
    pub fn text_range(&self, text: &str, range: Range) -> TextRange {
        let a = self.offset(text, range.start);
        let b = self.offset(text, range.end);
        TextRange::new(a.min(b), a.max(b))
    }
}

/// Converts a length that fits a [`TextSize`] (the analysis caps file sizes
/// well below 4 GiB) to `u32`, saturating.
fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn ascii_round_trips() {
        let text = "ab\ncd\n";
        let index = LineIndex::new(text);
        assert_eq!(index.line_count(), 3);
        for offset in 0..=u32::try_from(text.len()).unwrap() {
            let p = index.position(text, offset.into());
            assert_eq!(index.offset(text, p), TextSize::from(offset));
        }
        assert_eq!(index.position(text, 4.into()), pos(1, 1));
    }

    #[test]
    fn columns_count_utf16_code_units() {
        // `é` is 2 bytes and 1 unit; `😀` is 4 bytes and 2 units.
        let text = "é😀x\n";
        let index = LineIndex::new(text);
        assert_eq!(index.position(text, 2.into()), pos(0, 1));
        assert_eq!(index.position(text, 6.into()), pos(0, 3));
        assert_eq!(index.offset(text, pos(0, 3)), TextSize::from(6));
        assert_eq!(index.offset(text, pos(0, 4)), TextSize::from(7));
        // Inside the surrogate pair: the start of the emoji.
        assert_eq!(index.offset(text, pos(0, 2)), TextSize::from(2));
        // Inside a multi-byte character: snapped back.
        assert_eq!(index.position(text, 3.into()), pos(0, 1));
    }

    #[test]
    fn line_breaks_of_every_kind() {
        let text = "a\r\nb\rc\nd";
        let index = LineIndex::new(text);
        assert_eq!(index.line_count(), 4);
        assert_eq!(index.offset(text, pos(1, 0)), TextSize::from(3));
        assert_eq!(index.offset(text, pos(2, 0)), TextSize::from(5));
        assert_eq!(index.offset(text, pos(3, 0)), TextSize::from(7));
        // Between `\r` and `\n`: the end of the first line.
        assert_eq!(index.position(text, 2.into()), pos(0, 1));
    }

    #[test]
    fn out_of_range_positions_clamp() {
        let text = "abc\nde";
        let index = LineIndex::new(text);
        assert_eq!(index.offset(text, pos(0, 99)), TextSize::from(3));
        assert_eq!(index.offset(text, pos(9, 0)), TextSize::from(6));
        assert_eq!(index.position(text, 99.into()), pos(1, 2));
        let r = index.text_range(text, Range::new(pos(1, 1), pos(0, 1)));
        assert_eq!(r, TextRange::new(1.into(), 5.into()));
    }

    #[test]
    fn empty_text() {
        let index = LineIndex::new("");
        assert_eq!(index.position("", 0.into()), pos(0, 0));
        assert_eq!(index.offset("", pos(3, 3)), TextSize::from(0));
    }
}
