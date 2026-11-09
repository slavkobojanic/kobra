//! Short CSS names in release builds (`docs/compiler/codegen.md` §17.5).
//!
//! A release build renames, in every client and server file at once:
//!
//! * each **scoped class and keyframes name** the style blocks define
//!   (`panel_k3x9a1`, [`crate::css::scoped_names`]) to `_` and a base-36
//!   number (`_0`, `_1`, … `_z`, `_10`);
//! * each **`--k-*` custom property** of the stylesheet (theme tokens,
//!   `{value}` interpolations, icons) to `--k` and a base-36 number
//!   (`--k0`), except those an embedded runtime file names itself
//!   (`--k-color-chart-axis`, `--k-color-drop-target`): the server renders
//!   charts with the embedded runtime, which no build rewrites.
//!
//! Names are given in sorted order of the long names, so a build is
//! deterministic. A replacement only matches a whole name (bounded by
//! characters that can't continue a CSS identifier), and the long names
//! are distinctive (a 6-digit hash, a `--k-` prefix), so text that merely
//! contains one is not touched. The stylesheet, the templates' markup, the
//! SSR renderers and the style bindings all name the same short name.
//! Development and test builds keep the long names (`Card__panel` under
//! the test profile, for goldens and class selectors).

use std::collections::{BTreeSet, HashMap};

use crate::{FileKind, OutputFile};

/// `true` for a byte that can continue a CSS identifier.
fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b >= 0x80
}

/// `n` in base 36, lower case, shortest form.
fn base36(mut n: usize) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[n % 36]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The `--k-*` names in `text`.
fn custom_properties(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(at) = text[i..].find("--k-") {
        let start = i + at;
        let mut end = start + 4;
        while end < bytes.len() && word(bytes[end]) {
            end += 1;
        }
        if start == 0 || !word(bytes[start - 1]) {
            out.insert(text[start..end].to_owned());
        }
        i = end;
    }
    out
}

/// The `--k-*` names the embedded runtime files name themselves, which
/// keep their spelling.
fn runtime_properties() -> &'static BTreeSet<String> {
    static NAMES: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        k_runtime::modules()
            .iter()
            .flat_map(|m| custom_properties(m.source))
            .collect()
    })
}

/// The renaming of a release build: long name → short name, for the
/// scoped names `classes` and the custom properties of the stylesheets
/// among `client`.
pub(crate) fn renaming(
    classes: &BTreeSet<String>,
    client: &[OutputFile],
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (i, c) in classes.iter().enumerate() {
        map.insert(c.clone(), format!("_{}", base36(i)));
    }
    let keep = runtime_properties();
    let props: BTreeSet<String> = client
        .iter()
        .filter(|f| f.kind == FileKind::Css)
        .flat_map(|f| custom_properties(&f.contents))
        .filter(|p| !keep.contains(p))
        .collect();
    for (i, p) in props.iter().enumerate() {
        map.insert(p.clone(), format!("--k{}", base36(i)));
    }
    map
}

/// `text` with every whole name of `map` replaced.
pub(crate) fn apply(text: &str, map: &HashMap<String, String>) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut i = 0;
    while i < bytes.len() {
        if word(bytes[i]) && (i == 0 || !word(bytes[i - 1])) {
            let start = i;
            while i < bytes.len() && word(bytes[i]) {
                i += 1;
            }
            if let Some(short) = map.get(&text[start..i]) {
                out.push_str(&text[last..start]);
                out.push_str(short);
                last = i;
            }
        } else {
            i += 1;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Renames in place: every JavaScript and CSS file of `client` and
/// `server`.
pub(crate) fn shorten(
    classes: &BTreeSet<String>,
    client: &mut [OutputFile],
    server: &mut [OutputFile],
) {
    let map = renaming(classes, client);
    if map.is_empty() {
        return;
    }
    for f in client.iter_mut().chain(server.iter_mut()) {
        if matches!(f.kind, FileKind::JavaScript | FileKind::Css) {
            f.contents = apply(&f.contents, &map);
        }
    }
    // The stylesheet's name carries the hash of what ships. (JavaScript
    // chunks are renamed by `crate::link`, which comes next.)
    for f in client.iter_mut().filter(|f| f.kind == FileKind::Css) {
        let stem = f.path.split('.').next().unwrap_or("app").to_owned();
        f.path = crate::hash::hashed_file_name(&stem, "css", f.contents.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// The renaming as a sorted table.
    fn sorted(map: &HashMap<String, String>) -> BTreeMap<&str, &str> {
        map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    fn css(text: &str) -> OutputFile {
        OutputFile {
            path: "app.css".into(),
            contents: text.into(),
            kind: FileKind::Css,
        }
    }

    #[test]
    fn short_names_are_base36_in_sorted_order() {
        assert_eq!(base36(0), "0");
        assert_eq!(base36(35), "z");
        assert_eq!(base36(36), "10");
        let classes: BTreeSet<String> = ["panel_k3x9a1".into(), "card_0abc12".into()].into();
        let files = [css(
            ":root{--k-space-md:1rem;--k-color-chart-axis:#000}.card_0abc12{padding:var(--k-space-md)}",
        )];
        let map = renaming(&classes, &files);
        assert_eq!(
            sorted(&map),
            BTreeMap::from([
                ("card_0abc12", "_0"),
                ("panel_k3x9a1", "_1"),
                ("--k-space-md", "--k0"),
            ]),
            "the runtime's own `--k-color-chart-axis` keeps its name"
        );
    }

    #[test]
    fn only_whole_names_are_replaced() {
        let map = HashMap::from([
            ("panel_k3x9a1".to_owned(), "_1".to_owned()),
            ("--k-space-md".to_owned(), "--k0".to_owned()),
        ]);
        assert_eq!(
            apply(".panel_k3x9a1:hover{gap:var(--k-space-md)}", &map),
            "._1:hover{gap:var(--k0)}"
        );
        assert_eq!(
            apply(
                "<div class=\"a panel_k3x9a1\">xpanel_k3x9a1 --k-space-mdx</div>",
                &map
            ),
            "<div class=\"a _1\">xpanel_k3x9a1 --k-space-mdx</div>"
        );
        assert_eq!(
            apply("style.setProperty(\"--k-space-md\",v)", &map),
            "style.setProperty(\"--k0\",v)"
        );
    }
}
