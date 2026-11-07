//! Content hashes for file names and scoped CSS names.
//!
//! The hash is 64-bit FNV-1a. It is not cryptographic: file-name hashes
//! only need to change when contents change (cache busting), and class
//! name hashes only need to differ between components, which the generator
//! verifies for each build (`docs/compiler/codegen.md`, "Styles").

/// Returns the 64-bit FNV-1a hash of `bytes`.
///
/// Linear in the length of `bytes`; the same input always gives the same
/// value on every platform.
///
/// # Examples
///
/// ```
/// assert_eq!(k_codegen::hash::fnv1a(b""), 0xcbf2_9ce4_8422_2325);
/// assert_eq!(k_codegen::hash::fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
/// ```
#[must_use]
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The digits of base 36, in value order: `0`–`9`, then `a`–`z`.
const BASE36: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Formats the low `digits` base-36 digits of `n`, most significant first,
/// zero-padded on the left; higher digits are dropped, so the result is
/// always exactly `digits` characters of [`BASE36`].
fn base36(mut n: u64, digits: usize) -> String {
    let mut out = vec![b'0'; digits];
    for slot in out.iter_mut().rev() {
        *slot = BASE36[(n % 36) as usize];
        n /= 36;
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Returns the content hash used in file names: the high 32 bits of
/// [`fnv1a`] as 8 lower-case hex digits.
///
/// # Examples
///
/// ```
/// let h = k_codegen::hash::content_hash(b"export {};");
/// assert_eq!(h.len(), 8);
/// assert!(h.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
/// ```
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    format!("{:08x}", fnv1a(bytes) >> 32)
}

/// Returns a content-hashed file name, `stem.<hash>.ext`
/// (`app.3f2a1b9c.js`), with the hash of `contents` from
/// [`content_hash`].
///
/// # Examples
///
/// ```
/// use k_codegen::hash::{content_hash, hashed_file_name};
///
/// let name = hashed_file_name("app", "js", b"x");
/// assert_eq!(name, format!("app.{}.js", content_hash(b"x")));
/// ```
#[must_use]
pub fn hashed_file_name(stem: &str, extension: &str, contents: &[u8]) -> String {
    format!("{stem}.{}.{extension}", content_hash(contents))
}

/// Returns the scope suffix of a component's CSS names: 6 base-36 digits
/// of the hash of its qualified name (`shop.CartSummary`), so it doesn't
/// change when the component's styles do and class names stay stable
/// across deploys.
#[must_use]
pub fn scope_suffix(component: &str) -> String {
    base36(fnv1a(component.as_bytes()), 6)
}

/// Returns the scoped CSS name of class (or keyframes) `name` of
/// `component`: `panel_k3x9a1` ([`scope_suffix`] of the qualified name),
/// or `CartSummary__panel` (the component's simple name) when `readable`
/// is set, as under [`crate::Profile::Test`].
///
/// # Examples
///
/// ```
/// use k_codegen::hash::{scope_suffix, scoped_name};
///
/// assert_eq!(scoped_name("shop.CartSummary", "panel", true), "CartSummary__panel");
/// assert_eq!(
///     scoped_name("shop.CartSummary", "panel", false),
///     format!("panel_{}", scope_suffix("shop.CartSummary")),
/// );
/// ```
#[must_use]
pub fn scoped_name(component: &str, name: &str, readable: bool) -> String {
    if readable {
        let simple = component.rsplit('.').next().unwrap_or(component);
        format!("{simple}__{name}")
    } else {
        format!("{name}_{}", scope_suffix(component))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_reference_values() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn hashes_are_fixed_width_and_deterministic() {
        assert_eq!(content_hash(b"x").len(), 8);
        assert_eq!(content_hash(b"x"), content_hash(b"x"));
        assert_ne!(content_hash(b"x"), content_hash(b"y"));
        assert_eq!(scope_suffix("shop.Cart").len(), 6);
        assert!(
            scope_suffix("shop.Cart")
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
        );
    }

    #[test]
    fn names() {
        assert_eq!(hashed_file_name("app", "js", b"x").split('.').count(), 3);
        assert_eq!(
            scoped_name("shop.CartSummary", "panel", true),
            "CartSummary__panel"
        );
        let hashed = scoped_name("shop.CartSummary", "panel", false);
        assert!(
            hashed.starts_with("panel_") && hashed.len() == "panel_".len() + 6,
            "{hashed}"
        );
        assert_ne!(
            scoped_name("a.X", "panel", false),
            scoped_name("b.X", "panel", false)
        );
    }
}
