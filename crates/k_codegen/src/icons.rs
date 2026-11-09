//! The bundled icon set of `k.ui.Icon` (`docs/compiler/codegen.md` §10,
//! `docs/design/20-standard-library.md`).
//!
//! An `Icon` is a `span` with the case name of its `IconName` in
//! `data-icon`. The stylesheet draws it: `[data-icon]` is a 1em square
//! filled with the current text colour and masked by the icon's glyph, and
//! each `[data-icon="<name>"]` sets the glyph, an inline SVG data URL, in
//! `--k-icon`. So an icon takes the size and colour of the text around it,
//! and needs no request, no script and no font.
//!
//! Only the glyphs the project can show are emitted: the names of every
//! `Icon(name: .case)`, or the whole set once any `Icon` names its icon
//! with a computed value.
//!
//! The glyphs are drawn for K on a 24 × 24 grid with 2px round strokes, in
//! the style of the Lucide set whose names `IconName` follows.

use std::collections::BTreeSet;

use k_syntax::ast::{self, AstNode};

use crate::cx::Gen;

/// The SVG body of each glyph, by `IconName` case.
const GLYPHS: &[(&str, &str)] = &[
    (
        "sun",
        "<circle cx='12' cy='12' r='4'/><path d='M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41'/>",
    ),
    (
        "moon",
        "<path d='M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z'/>",
    ),
    (
        "cloud",
        "<path d='M7 19h10a4 4 0 0 0 .5-7.97A6 6 0 0 0 6.1 11.2 4 4 0 0 0 7 19z'/>",
    ),
    (
        "cloudSun",
        "<circle cx='8' cy='8' r='3'/><path d='M8 2v1.5M2 8h1.5M3.76 3.76l1.06 1.06M12.24 3.76l-1.06 1.06M9 20h9a3.5 3.5 0 0 0 .4-6.98A5 5 0 0 0 8.6 12.5 3.75 3.75 0 0 0 9 20z'/>",
    ),
    (
        "cloudMoon",
        "<path d='M10.5 3.5A4.5 4.5 0 1 0 3.5 10.5 3.5 3.5 0 0 1 10.5 3.5zM9 20h9a3.5 3.5 0 0 0 .4-6.98A5 5 0 0 0 8.6 12.5 3.75 3.75 0 0 0 9 20z'/>",
    ),
    ("rain", CLOUD_RAIN),
    ("cloudRain", CLOUD_RAIN),
    (
        "cloudDrizzle",
        "<path d='M7 15h10a3.5 3.5 0 0 0 .4-6.98A5.5 5.5 0 0 0 6.6 7.5 3.75 3.75 0 0 0 7 15zM8 18v1M12 18v1M16 18v1M10 21v1M14 21v1'/>",
    ),
    (
        "snow",
        "<path d='M7 15h10a3.5 3.5 0 0 0 .4-6.98A5.5 5.5 0 0 0 6.6 7.5 3.75 3.75 0 0 0 7 15zM8 18.5h.01M12 18.5h.01M16 18.5h.01M10 21.5h.01M14 21.5h.01'/>",
    ),
    (
        "snowflake",
        "<path d='M12 2v20M3.34 7l17.32 10M3.34 17l17.32-10M9.5 3.5 12 6l2.5-2.5M9.5 20.5 12 18l2.5 2.5'/>",
    ),
    ("thunderstorm", CLOUD_LIGHTNING),
    ("cloudLightning", CLOUD_LIGHTNING),
    ("fog", "<path d='M6 6h12M4 10h16M4 14h16M6 18h12'/>"),
    (
        "cloudFog",
        "<path d='M7 15h10a3.5 3.5 0 0 0 .4-6.98A5.5 5.5 0 0 0 6.6 7.5 3.75 3.75 0 0 0 7 15zM5 19h14M7 22h10'/>",
    ),
    (
        "pin",
        "<path d='M12 22s7-6.2 7-12a7 7 0 0 0-14 0c0 5.8 7 12 7 12z'/><circle cx='12' cy='10' r='2.5'/>",
    ),
    (
        "trash",
        "<path d='M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14M10 10v6M14 10v6'/>",
    ),
    ("check", "<path d='M20 6 9 17l-5-5'/>"),
    ("close", "<path d='M18 6 6 18M6 6l12 12'/>"),
    (
        "circleHelp",
        "<circle cx='12' cy='12' r='10'/><path d='M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01'/>",
    ),
    ("arrowUp", "<path d='M12 19V5M5 12l7-7 7 7'/>"),
];

/// Rain falling from a cloud.
const CLOUD_RAIN: &str = "<path d='M7 15h10a3.5 3.5 0 0 0 .4-6.98A5.5 5.5 0 0 0 6.6 7.5 3.75 3.75 0 0 0 7 15zM8 18l-1 3M12 18l-1 3M16 18l-1 3'/>";

/// A lightning bolt below a cloud.
const CLOUD_LIGHTNING: &str = "<path d='M7 15h10a3.5 3.5 0 0 0 .4-6.98A5.5 5.5 0 0 0 6.6 7.5 3.75 3.75 0 0 0 7 15zM13 14l-3 4h4l-3 4'/>";

/// The glyph of a case the set has no drawing for: a circle, so the icon
/// still takes its place.
const MISSING: &str = "<circle cx='12' cy='12' r='9'/>";

/// The data URL of glyph `body`: a stroked 24 × 24 SVG, escaped for a
/// double-quoted `url("…")`.
fn data_url(body: &str) -> String {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'>{body}</svg>"
    );
    let escaped = svg
        .replace('%', "%25")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('#', "%23")
        .replace('"', "%22");
    format!("url(\"data:image/svg+xml,{escaped}\")")
}

/// The icon names the project's views can show: `Some` of the static
/// `Icon(name: .case)` names, or `None` when some `Icon` computes its name
/// (then every glyph ships).
fn used(g: &Gen<'_>) -> Option<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for &d in &g.own {
        let Some(node) = g.decl_node(d) else { continue };
        for call in node.descendants().filter_map(ast::CallExpr::cast) {
            let is_icon = matches!(call.callee(), Some(ast::Expr::NameExpr(n)) if n.name().is_some_and(|t| t.text() == "Icon"));
            if !is_icon {
                continue;
            }
            let Some(list) = call.arg_list() else {
                continue;
            };
            for a in list.args() {
                if a.label().is_some_and(|l| l.text() == "name") {
                    match a.value() {
                        Some(ast::Expr::ImplicitMemberExpr(m)) => {
                            if let Some(n) = m.name() {
                                names.insert(n.text().to_owned());
                            }
                        }
                        _ => return None,
                    }
                }
            }
        }
    }
    Some(names)
}

/// The icon rules of the stylesheet: empty unless the project imports
/// `k.ui.Icon`.
pub(crate) fn rules(g: &Gen<'_>) -> String {
    if !crate::css::imports(g).contains("k.ui.Icon") {
        return String::new();
    }
    let all: Vec<String> = g
        .view
        .lookup("k.ui.IconName")
        .map(|d| {
            g.cases(d)
                .into_iter()
                .map(|c| g.view.member(c).name.clone())
                .collect()
        })
        .unwrap_or_default();
    let names: Vec<String> = match used(g) {
        Some(set) => all.into_iter().filter(|n| set.contains(n)).collect(),
        None => all,
    };
    let mut css = String::from(
        "[data-icon] {\n  display: inline-block;\n  flex: none;\n  width: 1em;\n  height: 1em;\n  vertical-align: -0.125em;\n  background-color: currentColor;\n  -webkit-mask: var(--k-icon) center / contain no-repeat;\n  mask: var(--k-icon) center / contain no-repeat;\n}\n",
    );
    for n in names {
        let body = GLYPHS
            .iter()
            .find(|(k, _)| *k == n)
            .map_or(MISSING, |(_, b)| b);
        css.push_str(&format!(
            "[data-icon=\"{n}\"] {{\n  --k-icon: {};\n}}\n",
            data_url(body)
        ));
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_is_an_escaped_svg_data_url() {
        for (name, body) in GLYPHS {
            let url = data_url(body);
            assert!(
                url.starts_with("url(\"data:image/svg+xml,%3Csvg "),
                "{name}"
            );
            assert!(
                !url[5..url.len() - 2].contains(['<', '>', '"', '#']),
                "{name}: {url}"
            );
        }
    }

    #[test]
    fn every_icon_name_has_a_glyph() {
        let source = include_str!("../../k_sema/stdlib/k/ui/IconName.k");
        let cases: Vec<&str> = source
            .lines()
            .filter_map(|l| l.trim().strip_prefix("case "))
            .collect();
        assert!(cases.len() >= 20, "{cases:?}");
        for c in cases {
            assert!(GLYPHS.iter().any(|(n, _)| *n == c), "no glyph for `{c}`");
        }
    }
}
