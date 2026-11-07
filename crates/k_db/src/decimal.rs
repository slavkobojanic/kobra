//! Numeric comparison of decimal text, the `k_decimal` collation of SQLite
//! (`docs/compiler/db.md` §3).
//!
//! SQLite has no exact decimal type: a `REAL` would round
//! `1234567890123456789012345678.0123456789`, so `Decimal` columns (and the
//! amount half of `Money` columns) are `TEXT`. Text compares
//! lexicographically, though, and `"10.0" < "9.6"` as text. Every SQLite
//! connection therefore registers the collation [`COLLATION`], implemented
//! by [`compare`], and the DDL declares decimal columns with
//! `COLLATE k_decimal`: comparisons (`=`, `<`, …), `ORDER BY` and indexes on
//! those columns then follow the numbers, exactly as `numeric` does on
//! Postgres.

use std::cmp::Ordering;

/// The name of the collation SQLite connections register.
pub const COLLATION: &str = "k_decimal";

/// A decimal literal split into its normalized parts.
struct Parts<'a> {
    negative: bool,
    /// The integer digits without leading zeros (empty for zero).
    int: &'a str,
    /// The fraction digits without trailing zeros (empty for none).
    frac: &'a str,
}

/// Splits `-012.3400` into sign, `12` and `34`. Returns `None` for text
/// that isn't a plain decimal literal (an optional sign, digits, an
/// optional point and digits, at least one digit in all).
fn parts(text: &str) -> Option<Parts<'_>> {
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (int, frac) = match rest.split_once('.') {
        Some((i, f)) => (i, f),
        None => (rest, ""),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (int.is_empty() && frac.is_empty()) || !digits(int) || !digits(frac) {
        return None;
    }
    let int = int.trim_start_matches('0');
    let frac = frac.trim_end_matches('0');
    // Negative zero is zero.
    let negative = negative && !(int.is_empty() && frac.is_empty());
    Some(Parts {
        negative,
        int,
        frac,
    })
}

/// Compares two magnitudes.
fn magnitude(a: &Parts<'_>, b: &Parts<'_>) -> Ordering {
    a.int
        .len()
        .cmp(&b.int.len())
        .then_with(|| a.int.cmp(b.int))
        .then_with(|| a.frac.cmp(b.frac))
}

/// Compares two decimal texts by value: `"10.0"` > `"9.6"`, `"1.50"` =
/// `"1.5"`, `"-0"` = `"0"`.
///
/// It is a total order on all text, so it is safe as a collation: text
/// that isn't a decimal literal (which K never stores in a decimal column)
/// sorts after every decimal, and among itself byte by byte. It never
/// panics and allocates nothing.
#[must_use]
pub fn compare(a: &str, b: &str) -> Ordering {
    match (parts(a), parts(b)) {
        (Some(x), Some(y)) => match (x.negative, y.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => magnitude(&x, &y),
            (true, true) => magnitude(&y, &x),
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.as_bytes().cmp(b.as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_by_value() {
        let mut v = vec!["10.0", "9.5", "-20", "2", "0.001", "-0.5", "100", "-100.25"];
        v.sort_by(|a, b| compare(a, b));
        assert_eq!(
            v,
            ["-100.25", "-20", "-0.5", "0.001", "2", "9.5", "10.0", "100"]
        );
    }

    #[test]
    fn equal_values_with_different_spellings_are_equal() {
        assert_eq!(compare("1.50", "1.5"), Ordering::Equal);
        assert_eq!(compare("-0", "0.000"), Ordering::Equal);
        assert_eq!(compare("007", "7."), Ordering::Equal);
        assert_eq!(compare("+3", "3"), Ordering::Equal);
        assert_eq!(compare(".5", "0.5"), Ordering::Equal);
    }

    #[test]
    fn long_values_compare_exactly() {
        let a = "1234567890123456789012345678.0123456789";
        let b = "1234567890123456789012345678.0123456788";
        assert_eq!(compare(a, b), Ordering::Greater);
        assert_eq!(compare(&format!("-{a}"), &format!("-{b}")), Ordering::Less);
    }

    #[test]
    fn non_decimals_sort_last_and_never_panic() {
        for junk in [
            "", "-", ".", "+", "1e5", "abc", "1.2.3", "--1", "١٢", "\u{0}",
        ] {
            assert_eq!(compare("5", junk), Ordering::Less, "{junk:?}");
            assert_eq!(compare(junk, "-5"), Ordering::Greater, "{junk:?}");
            assert_eq!(compare(junk, junk), Ordering::Equal, "{junk:?}");
        }
        assert_eq!(compare("abc", "abd"), Ordering::Less);
    }
}
