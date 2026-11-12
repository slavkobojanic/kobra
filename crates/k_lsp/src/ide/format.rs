//! Formatting (`textDocument/formatting` and `rangeFormatting`): the
//! canonical text of [`k_fmt::format`], sent as the line edits that turn
//! the document into it, so the cursor and the undo history keep their
//! place outside the changed lines (`docs/design/56-formatter.md`, "In
//! editors").
//!
//! A range is formatted by keeping only the edits on its lines. That is
//! only done when the result is still the same program up to layout
//! (formatting it again gives the canonical text): otherwise, such as when
//! a moved `init()` would be half moved, the range gets no edits.

use k_diagnostics::{TextRange, TextSize};

/// One edit: the byte range of the old text to replace, and its
/// replacement.
pub type Edit = (TextRange, String);

/// The edits that format `text`; `None` when it has syntax errors (or the
/// formatter fails), so nothing should change. With `lines` (the first and
/// last line, zero-based and inclusive), only the edits touching those
/// lines.
#[must_use]
pub fn format(text: &str, lines: Option<(usize, usize)>) -> Option<Vec<Edit>> {
    let formatted = k_fmt::format(text).ok()?;
    let old: Vec<&str> = text.split_inclusive('\n').collect();
    let new: Vec<&str> = formatted.split_inclusive('\n').collect();
    let mut hunks = hunks(&old, &new);
    if let Some((first, last)) = lines {
        hunks.retain(|h| {
            if h.old.start == h.old.end {
                // An insertion before line `h.old.start`.
                (first..=last + 1).contains(&h.old.start)
            } else {
                h.old.start <= last && h.old.end > first
            }
        });
        let partial = apply(&old, &hunks);
        if k_fmt::format(&partial).ok()? != formatted {
            return Some(Vec::new());
        }
    }
    let starts = line_starts(&old);
    Some(
        hunks
            .into_iter()
            .map(|h| {
                let range = TextRange::new(starts[h.old.start], starts[h.old.end]);
                (range, h.new.concat())
            })
            .collect(),
    )
}

/// The byte offset of each line's start, and the text's length last.
fn line_starts(lines: &[&str]) -> Vec<TextSize> {
    let mut out = Vec::with_capacity(lines.len() + 1);
    let mut at = 0usize;
    out.push(TextSize::new(0));
    for line in lines {
        at += line.len();
        out.push(TextSize::new(u32::try_from(at).unwrap_or(u32::MAX)));
    }
    out
}

/// Old lines `old` replaced by `new`.
struct Hunk<'a> {
    /// The old lines replaced, by index.
    old: std::ops::Range<usize>,
    /// Their replacement.
    new: Vec<&'a str>,
}

/// `old` with `hunks` (in order, not overlapping) applied.
fn apply(old: &[&str], hunks: &[Hunk<'_>]) -> String {
    let mut out = String::new();
    let mut at = 0;
    for h in hunks {
        out.extend(old[at..h.old.start].iter().copied());
        out.extend(h.new.iter().copied());
        at = h.old.end;
    }
    out.extend(old[at..].iter().copied());
    out
}

/// The changed lines between `old` and `new`, in order. Lines are aligned
/// by their text without whitespace, so a line the formatter only
/// re-indented or re-spaced is a hunk of its own, which a range can take
/// alone; the lines between two aligned ones (joined, split, added or
/// removed lines) are one hunk.
fn hunks<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Hunk<'a>> {
    let squeeze = |s: &str| s.split_whitespace().collect::<String>();
    let a: Vec<String> = old.iter().map(|l| squeeze(l)).collect();
    let b: Vec<String> = new.iter().map(|l| squeeze(l)).collect();
    // Aligned pairs: a longest common subsequence (none when the texts are
    // too large to align, so the whole text is one hunk).
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    if a.len().saturating_mul(b.len()) <= 4_000_000 {
        let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                lcs[i][j] = if a[i] == b[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            if a[i] == b[j] {
                pairs.push((i, j));
                i += 1;
                j += 1;
            } else if lcs[i + 1][j] >= lcs[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    pairs.push((old.len(), new.len()));
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (pi, pj) in pairs {
        if pi > i || pj > j {
            out.push(Hunk {
                old: i..pi,
                new: new[j..pj].to_vec(),
            });
        }
        if pi < old.len() && old[pi] != new[pj] {
            out.push(Hunk {
                old: pi..pi + 1,
                new: vec![new[pj]],
            });
        }
        i = pi + 1;
        j = pj + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(text: &str, edits: &[Edit]) -> String {
        let mut out = text.to_owned();
        for (range, new) in edits.iter().rev() {
            out.replace_range(usize::from(range.start())..usize::from(range.end()), new);
        }
        out
    }

    #[test]
    fn edits_give_the_formatted_text() {
        let text = "func a() -> Int {\nreturn 1+2\n}\n\n\n\nfunc b() -> Int {\n    return 3\n}";
        let edits = format(text, None).unwrap();
        assert_eq!(applied(text, &edits), k_fmt::format(text).unwrap());
        // The unchanged lines aren't edited.
        assert!(
            edits.iter().all(|(r, _)| usize::from(r.start()) >= 18),
            "{edits:?}"
        );
        assert!(format("func (\n", None).is_none());
        assert!(
            format(&k_fmt::format(text).unwrap(), None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_range_keeps_the_edits_on_its_lines() {
        let text = "func a() -> Int {\nreturn 1+2\n}\n\nfunc b() -> Int {\nreturn 3\n}\n";
        let edits = format(text, Some((1, 1))).unwrap();
        assert_eq!(
            applied(text, &edits),
            text.replace("\nreturn 1+2", "\n    return 1 + 2")
        );
    }
}
