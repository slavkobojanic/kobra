//! Line diffs for golden-test failure messages.

/// Renders a line diff from `expected` to `actual`.
///
/// Every line is prefixed with `  ` (unchanged), `- ` (only in expected) or
/// `+ ` (only in actual). The diff is a longest-common-subsequence diff; for
/// inputs whose product of line counts exceeds a few million it falls back
/// to printing both texts in full, which keeps failure reporting fast.
#[must_use]
pub fn line_diff(expected: &str, actual: &str) -> String {
    let a: Vec<&str> = expected.lines().collect();
    let b: Vec<&str> = actual.lines().collect();
    if a.len().saturating_mul(b.len()) > 4_000_000 {
        return format!("--- expected\n{expected}\n+++ actual\n{actual}\n");
    }
    // lcs[i][j] = length of the LCS of a[i..] and b[j..].
    let width = b.len() + 1;
    let mut lcs = vec![0u32; (a.len() + 1) * width];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i * width + j] = if a[i] == b[j] {
                lcs[(i + 1) * width + j + 1] + 1
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let mut out = String::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push_str("  ");
            out.push_str(a[i]);
            i += 1;
            j += 1;
        } else if j < b.len()
            && (i == a.len() || lcs[i * width + j + 1] >= lcs[(i + 1) * width + j])
        {
            out.push_str("+ ");
            out.push_str(b[j]);
            j += 1;
        } else {
            out.push_str("- ");
            out.push_str(a[i]);
            i += 1;
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_have_no_markers() {
        assert_eq!(line_diff("a\nb\n", "a\nb\n"), "  a\n  b\n");
    }

    #[test]
    fn changes_are_marked() {
        assert_eq!(
            line_diff("a\nb\nc\n", "a\nx\nc\nd\n"),
            "  a\n+ x\n- b\n  c\n+ d\n"
        );
    }

    #[test]
    fn empty_sides() {
        assert_eq!(line_diff("", "a\n"), "+ a\n");
        assert_eq!(line_diff("a\n", ""), "- a\n");
        assert_eq!(line_diff("", ""), "");
    }
}
