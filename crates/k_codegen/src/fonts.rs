//! Shipped fonts (07-themes-and-units.md, "Shipped fonts"; codegen.md §12).
//!
//! Every `FontFace("fonts/Inter.woff2", weight:, style:)` of a theme the
//! program has (the project's and its mounted workspace libraries') is
//! one [`Face`]: its file ships below `/assets/fonts/` under a
//! content-hashed name ([`crate::BinaryFile`]), and the stylesheet gets an
//! `@font-face` rule with `font-display: swap`. Faces of one family,
//! weight and style dedupe into one rule (sema reports two different
//! files for one as K3310). The document preloads one face: the body
//! font's upright face closest to weight 400 ([`preload`]).

use k_sema::FileOrigin;
use k_sema::ProjectPath;
use k_sema::assets::FontFormat;
use k_sema::fonts::FaceStyle;
use k_sema::stdlib::DeclarationKind;

use crate::Profile;
use crate::cx::Gen;

/// One face the app ships.
#[derive(Clone, Debug)]
pub(crate) struct Face {
    /// The family, decoded (`PP Neue Gstaad`).
    pub(crate) family: String,
    /// The family as a CSS string, the Kobra literal as written.
    pub(crate) family_css: String,
    /// The CSS weight.
    pub(crate) weight: i128,
    /// Upright or italic.
    pub(crate) style: FaceStyle,
    /// The file's format.
    pub(crate) format: FontFormat,
    /// The path below `/assets/`, `fonts/Inter.3f2a1b9c.woff2`.
    pub(crate) path: String,
    /// The file's bytes.
    pub(crate) bytes: Vec<u8>,
}

impl Face {
    /// The URL the stylesheet and the preload link name.
    pub(crate) fn url(&self) -> String {
        format!("/assets/{}", self.path)
    }
}

/// The faces of every non-stdlib theme, in declaration order, one per
/// family, weight and style. A face whose file sema refused, or whose
/// family isn't a string literal, is left out.
pub(crate) fn faces(g: &Gen<'_>) -> Vec<Face> {
    let mut out: Vec<Face> = Vec::new();
    let mut themes: Vec<_> = g
        .own
        .iter()
        .copied()
        .filter(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::Theme))
        .collect();
    // Libraries' themes (mounted below `.deps/`) first, like their tokens.
    themes.sort_by_key(|&d| !is_library(g, d));
    for d in themes {
        let Some(node) = g.decl_node(d) else { continue };
        let Some(FileOrigin::Project(path)) = g.analysis.origin(g.view.decl(d).file) else {
            continue;
        };
        let prefix = k_sema::fonts::package_prefix(path);
        for face in k_sema::fonts::faces(&node) {
            let (Some(family), Some(family_css), Some(file), Some(weight), Some(style)) = (
                face.family,
                face.family_literal,
                face.path,
                face.weight,
                face.style,
            ) else {
                continue;
            };
            if out
                .iter()
                .any(|f| f.family == family && f.weight == weight && f.style == style)
            {
                continue;
            }
            let text = |p: &str| {
                let id = g.analysis.file_id(&ProjectPath::new(p).ok()?)?;
                g.analysis.text(id)
            };
            let Ok((full, format)) = k_sema::fonts::resolve(&file, &prefix, text) else {
                continue;
            };
            let Some(bytes) = text(&full).and_then(k_sema::assets::bytes_of) else {
                continue;
            };
            let name = file.rsplit('/').next().unwrap_or(&file);
            let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
            let hashed = if g.options.profile == Profile::Test {
                format!("{stem}.[hash].{}", format.extension())
            } else {
                crate::hash::hashed_file_name(stem, format.extension(), &bytes)
            };
            out.push(Face {
                family,
                family_css,
                weight,
                style,
                format,
                path: format!("fonts/{hashed}"),
                bytes,
            });
        }
    }
    out
}

/// `true` for a declaration of a workspace library mounted into the
/// project.
fn is_library(g: &Gen<'_>, d: k_sema::view::DeclId) -> bool {
    matches!(
        g.analysis.origin(g.view.decl(d).file),
        Some(FileOrigin::Project(p)) if p.dependency().is_some()
    )
}

/// The `@font-face` rules of `faces`, one per face, for the stylesheet
/// (before `:root`).
pub(crate) fn rules(faces: &[Face]) -> String {
    let mut css = String::new();
    for f in faces {
        let style = match f.style {
            FaceStyle::Normal => "normal",
            FaceStyle::Italic => "italic",
        };
        css.push_str(&format!(
            "@font-face {{\n  font-family: {};\n  src: url(\"{}\") format(\"{}\");\n  font-weight: {};\n  font-style: {style};\n  font-display: swap;\n}}\n",
            f.family_css,
            f.url(),
            f.format.css(),
            f.weight,
        ));
    }
    css
}

/// The face every document preloads, if any: of the family of the `font
/// body` token (the base stylesheet's font, used by every page), the
/// upright face whose weight is closest to 400 (the lighter on a tie).
/// Only that one: a preload the first paint doesn't use costs bandwidth
/// and a console warning, and every other face still loads when text uses
/// it, drawn in the fallback until then (`font-display: swap`).
pub(crate) fn preload<'f>(faces: &'f [Face], body_family: Option<&str>) -> Option<&'f Face> {
    let family = body_family?;
    faces
        .iter()
        .filter(|f| f.family == family && f.style == FaceStyle::Normal)
        .min_by_key(|f| ((f.weight - 400).abs(), f.weight))
}
