//! Shipped fonts (07-themes-and-units.md, "Shipped fonts"; sema.md §12.5).
//!
//! A theme's font token names the files of its family with
//! `FontFace("fonts/Inter.woff2", weight: 700, style: .italic)`. The path
//! is relative to the root of the package whose theme declares it, so a
//! workspace library's face names the library's file, mounted below
//! `.deps/<dir>/` in each member that depends on it.
//!
//! * A path that leaves the package (`..`, `/…`), or names no asset of
//!   the package, is **K3308**, with the closest font file of the package
//!   as a fix.
//! * A file that isn't `.woff2` or `.woff`, or whose bytes don't start with
//!   its format's signature (`wOF2`, `wOFF`), is **K3309**.
//! * Two faces of one family with the same weight and style but different
//!   files, in any themes the program has, are **K3310**.
//! * A weight that isn't an integer literal from 1 to 1000, or a style
//!   that isn't `.normal` or `.italic`, is K2403 or K2404, like any other
//!   checked literal.
//!
//! The pass reads the assets, which are not inputs of a file's body facts,
//! so it runs every revision instead of being replayed (`crate::warm`).
//! `FontFace` outside a theme's token is K3311, from the body check.

use std::collections::HashMap;

use k_diagnostics::{Diagnostic, Fix, TextRange, codes};
use k_syntax::SyntaxKind;
use k_syntax::SyntaxNode;
use k_syntax::ast::{self, AstNode};

use crate::assets::{FontFormat, bytes_of};
use crate::cx::Cx;
use crate::program::FileIx;
use crate::stdlib::DeclarationKind;

/// The upright or italic style of a face.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FaceStyle {
    /// `.normal`.
    Normal,
    /// `.italic`.
    Italic,
}

/// One `FontFace(…)` of a theme, as its syntax says.
#[derive(Clone, Debug)]
pub struct FaceSyntax {
    /// The family of the enclosing `Font(family:)`, decoded; `None` when it
    /// isn't a plain string literal.
    pub family: Option<String>,
    /// The family as written, a Kobra string literal (also the CSS string).
    pub family_literal: Option<String>,
    /// The path as written in the string literal, decoded; `None` when the
    /// argument isn't a plain string literal (K2403).
    pub path: Option<String>,
    /// The range of the path literal (the call's, without one).
    pub path_range: TextRange,
    /// The weight: the literal's value, 400 without one; `None` when it
    /// isn't an integer literal.
    pub weight: Option<i128>,
    /// The range of the weight argument's value.
    pub weight_range: Option<TextRange>,
    /// The style; `None` when it isn't `.normal` or `.italic`.
    pub style: Option<FaceStyle>,
    /// The range of the style argument's value.
    pub style_range: Option<TextRange>,
}

/// `true` when `call` calls a type named `name` (`Font`, `FontFace`).
fn calls(call: &ast::CallExpr, name: &str) -> bool {
    match call.callee() {
        Some(ast::Expr::NameExpr(n)) => n.name().is_some_and(|t| t.text() == name),
        _ => false,
    }
}

/// The argument of `call` labelled `label` (`None` for the unlabelled one).
fn arg(call: &ast::CallExpr, label: Option<&str>) -> Option<ast::Expr> {
    call.arg_list()?
        .args()
        .find(|a| a.label().map(|l| l.text().to_owned()).as_deref() == label)?
        .value()
}

/// The faces of the theme declaration `theme`, in source order: every
/// `FontFace(…)` call in its token values and overrides, variants
/// included.
#[must_use]
pub fn faces(theme: &SyntaxNode) -> Vec<FaceSyntax> {
    let mut out = Vec::new();
    for call in theme.descendants().filter_map(ast::CallExpr::cast) {
        if !calls(&call, "FontFace")
            || !call
                .syntax()
                .ancestors()
                .any(|a| matches!(a.kind(), SyntaxKind::ThemeToken | SyntaxKind::ThemeOverride))
        {
            continue;
        }
        let font = call
            .syntax()
            .ancestors()
            .skip(1)
            .filter_map(ast::CallExpr::cast)
            .find(|c| calls(c, "Font"));
        let family_expr = font.as_ref().and_then(|f| arg(f, Some("family")));
        let (family, family_literal) = match &family_expr {
            Some(ast::Expr::StringLit(s)) => (
                s.plain_value(),
                Some(s.syntax().text().to_string().trim().to_owned()),
            ),
            _ => (None, None),
        };
        let path_expr = arg(&call, None);
        let path = match &path_expr {
            Some(ast::Expr::StringLit(s)) => s.plain_value(),
            _ => None,
        };
        let path_range = path_expr
            .as_ref()
            .map_or(call.syntax().text_range(), |e| e.syntax().text_range());
        let weight_expr = arg(&call, Some("weight"));
        let weight = match &weight_expr {
            None => Some(400),
            Some(ast::Expr::Literal(l)) => l
                .token()
                .filter(|t| t.kind() == SyntaxKind::IntLit)
                .and_then(|t| t.text().replace('_', "").parse::<i128>().ok()),
            Some(_) => None,
        };
        let style_expr = arg(&call, Some("style"));
        let style = match &style_expr {
            None => Some(FaceStyle::Normal),
            Some(e) => {
                let name = match e {
                    ast::Expr::ImplicitMemberExpr(m) => m.name().map(|t| t.text().to_owned()),
                    ast::Expr::MemberExpr(m) => m.name().map(|t| t.text().to_owned()),
                    _ => None,
                };
                match name.as_deref() {
                    Some("normal") => Some(FaceStyle::Normal),
                    Some("italic") => Some(FaceStyle::Italic),
                    _ => None,
                }
            }
        };
        out.push(FaceSyntax {
            family,
            family_literal,
            path,
            path_range,
            weight,
            weight_range: weight_expr.map(|e| e.syntax().text_range()),
            style,
            style_range: style_expr.map(|e| e.syntax().text_range()),
        });
    }
    out
}

/// The directory prefix of the package that holds the project file
/// `path`: empty for the project's own files, `.deps/<dir>/` for a
/// mounted library's.
#[must_use]
pub fn package_prefix(path: &crate::ProjectPath) -> String {
    let text = path.as_str();
    if path.dependency().is_none() {
        return String::new();
    }
    // `.deps/<segment>/<inner>`: the prefix is the first two segments.
    let mut parts = text.splitn(3, '/');
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a}/{b}/"),
        _ => String::new(),
    }
}

/// Why `path` can't name a file of the package: it is empty, absolute, or
/// has a `.`, `..` or empty segment or a backslash.
fn leaves_package(path: &str) -> bool {
    path.is_empty()
        || path.contains('\\')
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
}

/// The project path of the file a face names, or why it names none, as a
/// diagnostic code and message. `prefix` is the package's
/// ([`package_prefix`]); `assets` the program's assets by project path.
///
/// # Errors
///
/// K3308 for a path outside the package or a file it doesn't have, K3309
/// for a file that isn't a WOFF2 or WOFF font.
pub fn resolve<'a>(
    path: &str,
    prefix: &str,
    assets: impl Fn(&str) -> Option<&'a str>,
) -> Result<(String, FontFormat), (k_diagnostics::Code, String)> {
    if leaves_package(path) {
        return Err((
            codes::FONT_FILE_NOT_FOUND,
            format!(
                "`{path}` is not a path inside the package: a font file's path is relative to the package root, without `.` or `..`"
            ),
        ));
    }
    let Some(format) = FontFormat::of(path) else {
        let ext = path
            .rsplit_once('.')
            .map(|(_, e)| format!("`.{e}` files"))
            .unwrap_or_else(|| "files without an extension".to_owned());
        return Err((
            codes::UNSUPPORTED_FONT_FILE,
            format!(
                "{ext} can't be shipped: a font face is a `.woff2` or `.woff` file; convert it to WOFF2"
            ),
        ));
    };
    let full = format!("{prefix}{path}");
    let Some(text) = assets(&full) else {
        return Err((
            codes::FONT_FILE_NOT_FOUND,
            format!("there is no `{path}` in this package"),
        ));
    };
    let head: Vec<u8> = bytes_of(&text.chars().take(4).collect::<String>()).unwrap_or_default();
    if head != format.magic() {
        let other = [FontFormat::Woff2, FontFormat::Woff]
            .into_iter()
            .find(|f| head == f.magic());
        let message = match other {
            Some(f) => format!(
                "`{path}` is a {} file, not {}: name it `.{}`",
                f.name(),
                format.name(),
                f.extension()
            ),
            None => format!(
                "`{path}` is not a {} file: it doesn't start with `{}`",
                format.name(),
                String::from_utf8_lossy(format.magic())
            ),
        };
        return Err((codes::UNSUPPORTED_FONT_FILE, message));
    }
    Ok((full, format))
}

/// The checks of the module documentation, over every theme of the
/// program but the stdlib's.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    // (family, weight, style) → the file and where it was first named.
    let mut seen: HashMap<(String, i128, FaceStyle), (String, FileIx, TextRange)> = HashMap::new();
    for d in &p.decls {
        if p.file(d.file).stdlib || d.named_kind() != Some(DeclarationKind::Theme) {
            continue;
        }
        let file = d.file;
        let prefix = p.file(file).path.map(package_prefix).unwrap_or_default();
        for face in faces(&d.node) {
            if let (None, Some(range)) = (face.weight, face.weight_range) {
                cx.report(
                    file,
                    Diagnostic::new(
                        codes::LITERAL_REQUIRED,
                        p.span(file, range),
                        "`FontFace(weight:)` takes an integer literal, checked when you compile",
                    ),
                );
            }
            if let (Some(w), Some(range)) = (face.weight, face.weight_range)
                && !(1..=1000).contains(&w)
            {
                cx.error(
                    file,
                    codes::INVALID_LITERAL,
                    range,
                    format!("a font weight is 1 to 1000, not {w}"),
                );
            }
            if let (None, Some(range)) = (face.style, face.style_range) {
                cx.report(
                    file,
                    Diagnostic::new(
                        codes::LITERAL_REQUIRED,
                        p.span(file, range),
                        "`FontFace(style:)` takes `.normal` or `.italic`, checked when you compile",
                    ),
                );
            }
            let Some(path) = &face.path else { continue };
            let resolved = resolve(path, &prefix, |k| p.assets.get(k).map(|t| &**t));
            let full = match resolved {
                Ok((full, _)) => full,
                Err((code, message)) => {
                    let mut diag = Diagnostic::new(code, p.span(file, face.path_range), message);
                    if code == codes::FONT_FILE_NOT_FOUND && !leaves_package(path) {
                        diag = diag.with_note(
                            "a font file's path is relative to the root of the package that declares the theme, the directory of its `k.toml`",
                        );
                        let ext = FontFormat::of(path);
                        let candidates: Vec<&str> = p
                            .assets
                            .keys()
                            .filter_map(|k| k.strip_prefix(prefix.as_str()))
                            .filter(|k| !prefix.is_empty() || !k.starts_with(".deps/"))
                            .filter(|k| FontFormat::of(k) == ext)
                            .collect();
                        if let Some(c) = crate::cx::closest(path, candidates.iter().copied()) {
                            diag = diag.with_fix(
                                Fix::replace(
                                    format!("Replace it with `\"{c}\"`"),
                                    p.span(file, face.path_range),
                                    format!("\"{c}\""),
                                )
                                .maybe_incorrect(),
                            );
                        }
                    }
                    cx.report(file, diag);
                    continue;
                }
            };
            let (Some(family), Some(weight), Some(style)) = (&face.family, face.weight, face.style)
            else {
                continue;
            };
            let key = (family.clone(), weight, style);
            match seen.get(&key) {
                Some((first, ffile, frange)) if *first != full => {
                    let s = match style {
                        FaceStyle::Normal => "an upright",
                        FaceStyle::Italic => "an italic",
                    };
                    cx.report(
                        file,
                        Diagnostic::new(
                            codes::CONFLICTING_FONT_FACES,
                            p.span(file, face.path_range),
                            format!(
                                "`{family}` already has {s} face of weight {weight}, from another file"
                            ),
                        )
                        .with_label(p.span(*ffile, *frange), "first declared here"),
                    );
                }
                Some(_) => {}
                None => {
                    seen.insert(key, (full, file, face.path_range));
                }
            }
        }
    }
}
