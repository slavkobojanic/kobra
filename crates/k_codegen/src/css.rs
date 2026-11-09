//! CSS generation: style blocks, theme tokens and variants
//! (`docs/compiler/codegen.md` §12).
//!
//! * Class `panel` of component `shop.CartSummary` is `panel_<suffix>`
//!   (`hash::scope_suffix`), or `CartSummary__panel` under
//!   `Profile::Test`; keyframes are scoped the same way.
//! * Rules print in source order; nesting is flattened and `@media`,
//!   `@container` and `@layer` wrap the flattened rules; property names and
//!   keywords are kebab case (`justifyContent: spaceBetween` is
//!   `justify-content: space-between`, `k_sema::stdlib::css::keyword_css`).
//! * Implicit members are theme tokens, printed as custom properties
//!   (`var(--k-space-md)`); `{value}` interpolations become
//!   `var(--k-<scope>-<n>)`, set on the element by a style binding.
//! * Theme tokens are custom properties on `:root`; variants are classes on
//!   `<html>` (`:root.k-theme-dark`), and a variant that `matches
//!   .prefersDark` also applies through `prefers-color-scheme` until a
//!   choice is made (`:root:not(.k-theme-set)`).

use k_sema::stdlib::DeclarationKind;
use k_sema::view::DeclId;
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::Gen;
use crate::lower::Body;
use crate::view::ClassPart;
use crate::{FileKind, OutputFile, Profile};

/// Converts a K name to kebab case: `surfaceHover` → `surface-hover`,
/// `backgroundColor` → `background-color`.
#[must_use]
pub(crate) fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The CSS text of a token of a value: a keyword in its CSS spelling
/// (`spaceBetween` → `space-between`), anything else as written.
fn token_text(t: &k_syntax::SyntaxToken) -> String {
    if t.kind().is_name_like() {
        k_sema::stdlib::css::keyword_css(t.text()).into_owned()
    } else {
        t.text().to_owned()
    }
}

/// The name of a CSS function (`repeat`), as written, and its
/// comma-separated arguments.
fn function_parts(n: &SyntaxNode) -> (String, Vec<SyntaxNode>) {
    let name = n
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind().is_name_like())
        .map(|t| t.text().to_owned())
        .unwrap_or_default();
    let args = n
        .children()
        .filter(|c| c.kind() == SyntaxKind::CssValue)
        .collect();
    (name, args)
}

/// `true` when scoped names are readable (`Card__panel`) rather than
/// hashed: under [`Profile::Test`], so golden files stay stable.
fn readable(g: &Gen<'_>) -> bool {
    g.options.profile == Profile::Test
}

/// The scoped name of class `class` of component `owner`.
pub(crate) fn class_name(g: &Gen<'_>, owner: DeclId, class: &str) -> String {
    crate::hash::scoped_name(&g.view.qualified_name(owner), class, readable(g))
}

/// Every scoped name the project's style blocks define, as the hashed
/// profiles print them (`panel_k3x9a1`): each class selector and
/// keyframes name of each component, page and layout. A release build
/// renames exactly these (`crate::shorten`).
pub(crate) fn scoped_names(g: &Gen<'_>) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for &d in &g.own {
        let Some(block) = style_block(g, d) else {
            continue;
        };
        for n in block.descendants() {
            let name = match n.kind() {
                SyntaxKind::ClassSelector => ident(&n),
                SyntaxKind::KeyframesRule => n
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|t| t.kind().is_name_like())
                    .nth(1)
                    .map(|t| t.text().to_owned()),
                _ => None,
            };
            if let Some(name) = name {
                out.insert(crate::hash::scoped_name(
                    &g.view.qualified_name(d),
                    &name,
                    false,
                ));
            }
        }
    }
    out
}

/// The custom property of the `n`th interpolation of `owner`'s style block.
fn reactive_property(g: &Gen<'_>, owner: DeclId, n: usize) -> String {
    let scope = if readable(g) {
        g.view.decl(owner).name.clone()
    } else {
        crate::hash::scope_suffix(&g.view.qualified_name(owner))
    };
    format!("--k-{scope}-{n}")
}

/// The style block of a component.
fn style_block(g: &Gen<'_>, owner: DeclId) -> Option<SyntaxNode> {
    let node = g.decl_node(owner)?;
    node.children()
        .find(|c| c.kind() == SyntaxKind::MemberList)?
        .children()
        .find(|c| c.kind() == SyntaxKind::StyleBlock)
}

/// The reactive style values an element with `classes` must set: every
/// interpolation in the rules of those classes, as `(property, value)`.
pub(crate) fn reactive_values(
    b: &mut Body<'_, '_>,
    owner: DeclId,
    classes: &[ClassPart],
) -> Vec<(String, ast::Expr)> {
    let Some(block) = style_block(b.g, owner) else {
        return Vec::new();
    };
    let names: Vec<String> = classes
        .iter()
        .filter_map(|c| match c {
            ClassPart::Static(n) | ClassPart::When(n, _) => Some(n.clone()),
            ClassPart::Dyn(_) => None,
        })
        .collect();
    let mut out = Vec::new();
    let mut index = 0usize;
    for rule in block
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::StyleRule)
    {
        let classes_here: Vec<String> = rule
            .children()
            .filter(|c| c.kind() == SyntaxKind::Selector)
            .flat_map(|s| {
                s.descendants()
                    .filter(|d| d.kind() == SyntaxKind::ClassSelector)
                    .collect::<Vec<_>>()
            })
            .filter_map(|c| ident(&c))
            .map(|n| class_name(b.g, owner, &n))
            .collect();
        let decls: Vec<SyntaxNode> = rule
            .children()
            .filter(|c| c.kind() == SyntaxKind::Declaration)
            .collect();
        for d in decls {
            for i in d
                .descendants()
                .filter(|n| n.kind() == SyntaxKind::Interpolation)
            {
                if classes_here.iter().any(|c| names.contains(c)) {
                    if let Some(e) = i.children().find_map(ast::Expr::cast) {
                        out.push((reactive_property(b.g, owner, index), e));
                    }
                }
                index += 1;
            }
        }
    }
    out
}

/// The text of the first name-like token directly inside `n` (a class
/// selector's name, an at-rule's keyword, a declaration's property), or
/// `None` when it has none.
fn ident(n: &SyntaxNode) -> Option<String> {
    n.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind().is_name_like())
        .map(|t| t.text().to_owned())
}

/// The project's own theme declarations, with their syntax.
fn own_themes(g: &Gen<'_>) -> Vec<SyntaxNode> {
    g.own
        .iter()
        .copied()
        .filter(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::Theme))
        .filter_map(|d| g.decl_node(d))
        .collect()
}

/// The syntax of a stdlib declaration, from the stdlib's own parse
/// ([`Gen::decl_node`] only knows project files).
fn stdlib_decl_node(g: &Gen<'_>, d: DeclId) -> Option<SyntaxNode> {
    let info = g.view.decl(d);
    let k_sema::FileOrigin::Stdlib(path) = g.analysis.origin(info.file)? else {
        return None;
    };
    let file = k_sema::stdlib::Stdlib::get()
        .files()
        .iter()
        .find(|f| f.path == path)?;
    crate::cx::node_at(&file.parse.syntax(), info.range)
}

/// The paths the project's files import (`k.charts.LineChart`).
pub(crate) fn imports(g: &Gen<'_>) -> std::collections::BTreeSet<String> {
    let mut files = std::collections::HashSet::new();
    let mut out = std::collections::BTreeSet::new();
    for &d in &g.own {
        if !files.insert(g.view.decl(d).file) {
            continue;
        }
        let Some(root) = g.decl_node(d).and_then(|n| n.ancestors().last()) else {
            continue;
        };
        if root.kind() != SyntaxKind::SourceFile {
            continue;
        }
        for i in root
            .children()
            .filter_map(ast::Import::cast)
            .filter_map(|i| i.path())
        {
            out.insert(i.text());
        }
    }
    out
}

/// `true` when the project imports a declaration of stdlib module
/// `module` (`k.charts`).
fn uses_module(imports: &std::collections::BTreeSet<String>, module: &str) -> bool {
    imports.iter().any(|i| {
        i.strip_prefix(module)
            .is_some_and(|rest| rest.starts_with('.') && !rest[1..].contains('.'))
    })
}

/// The token names a theme assigns with the assignment form, at its top
/// level or in a variant.
fn assigned_names(theme: &SyntaxNode) -> Vec<String> {
    theme
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::ThemeOverride)
        .filter_map(|o| ident(&o))
        .collect()
}

/// The token declarations of the libraries the project uses
/// (07-themes-and-units.md, "Library tokens"): every stdlib `theme` (such
/// as `k.charts`' `ChartTokens`) of a module the project imports from, or
/// whose tokens its theme assigns.
fn library_themes(g: &Gen<'_>) -> Vec<SyntaxNode> {
    let imported = imports(g);
    let assigned: Vec<String> = own_themes(g).iter().flat_map(assigned_names).collect();
    let mut out = Vec::new();
    for (i, info) in g.view.decls.iter().enumerate() {
        if !info.stdlib || info.named_kind() != Some(DeclarationKind::Theme) {
            continue;
        }
        let Some(node) = stdlib_decl_node(g, DeclId(i as u32)) else {
            continue;
        };
        let module = info.module.clone().unwrap_or_default();
        let declares_assigned = theme_tokens(&node)
            .iter()
            .any(|t| assigned.contains(&t.name));
        if uses_module(&imported, &module) || declares_assigned {
            out.push(node);
        }
    }
    out
}

/// The theme declarations whose tokens the app has: the libraries' first,
/// then the project's own.
fn themes(g: &Gen<'_>) -> Vec<SyntaxNode> {
    let mut out = library_themes(g);
    out.extend(own_themes(g));
    out
}

/// One token of a project theme, `color primary = #2563EB`.
struct Token {
    /// The category word, `color` or `space`, as written in the theme.
    category: String,
    /// The token's name, `primary`.
    name: String,
    /// The value expression, or `None` when the token declares none.
    value: Option<SyntaxNode>,
}

/// The tokens one theme declares, in declaration order; a token without
/// both a category and a name is skipped.
fn theme_tokens(theme: &SyntaxNode) -> Vec<Token> {
    let mut out = Vec::new();
    for tok in theme
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::ThemeToken)
    {
        let words: Vec<String> = tok
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind().is_name_like())
            .map(|t| t.text().to_owned())
            .collect();
        if words.len() < 2 {
            continue;
        }
        let value = tok
            .children()
            .filter(|c| c.kind() != SyntaxKind::PathType)
            .find(|c| ast::Expr::can_cast(c.kind()));
        out.push(Token {
            category: words[0].clone(),
            name: words[1].clone(),
            value,
        });
    }
    out
}

/// The value of an assignment (`chartGrid = #E2E8F0`), as `(name, value)`.
fn assignment(o: &SyntaxNode) -> Option<(String, SyntaxNode)> {
    let name = ident(o)?;
    let value = o.children().find(|c| ast::Expr::can_cast(c.kind()))?;
    Some((name, value))
}

/// Every token of the app, in declaration order (the libraries' tokens
/// first), each with its base value: the declared one, or the one the
/// project's theme assigns at its top level (`chartGrid = #CBD5E1`, the
/// override of a library default).
fn tokens(g: &Gen<'_>) -> Vec<Token> {
    let mut out: Vec<Token> = themes(g).iter().flat_map(theme_tokens).collect();
    for theme in own_themes(g) {
        for o in theme
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::ThemeOverride)
            .filter(|n| !n.ancestors().any(|a| a.kind() == SyntaxKind::ThemeVariant))
        {
            let Some((name, value)) = assignment(&o) else {
                continue;
            };
            if let Some(t) = out.iter_mut().find(|t| t.name == name) {
                t.value = Some(value);
            }
        }
    }
    out
}

/// The literal value of the project's color token `name` (`#2563EB`),
/// for places that need a color rather than a CSS variable (the
/// `theme-color` meta element). `None` for a token that isn't a literal.
pub(crate) fn color_literal(g: &Gen<'_>, name: &str) -> Option<String> {
    tokens(g)
        .into_iter()
        .find(|t| t.category == "color" && t.name == name)
        .and_then(|t| t.value)
        .map(|v| v.text().to_string().trim().to_owned())
        .filter(|v| {
            v.starts_with('#') && v.len() > 1 && v[1..].bytes().all(|b| b.is_ascii_hexdigit())
        })
}

/// The names of the theme's variants.
pub(crate) fn variants(g: &Gen<'_>) -> Vec<String> {
    let mut out = Vec::new();
    for node in own_themes(g) {
        for v in node
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::ThemeVariant)
        {
            if let Some(name) = v
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind().is_name_like())
                .nth(1)
            {
                out.push(name.text().to_owned());
            }
        }
    }
    out
}

/// The custom property of a theme token, `--k-<category>-<token>`, with
/// `spacing` and `size` normalized to `space` and `font-size`.
fn token_property(category: &str, name: &str) -> String {
    let c = match category {
        "space" | "spacing" => "space".to_owned(),
        "fontSize" | "size" => "font-size".to_owned(),
        other => kebab(other),
    };
    format!("--k-{c}-{}", kebab(name))
}

/// The theme category whose tokens can fill a CSS property of the K type
/// `t` (`Spacing` → `space`); empty for a type with no token category.
fn type_category(t: &str) -> &'static str {
    match t {
        "Spacing" => "space",
        "Color" => "color",
        "Radius" => "radius",
        "FontSize" => "fontSize",
        "Font" => "font",
        "Shadow" => "shadow",
        _ => "",
    }
}

/// The CSS of a theme token reference `.name` in property `property`.
fn token_ref(all: &[Token], property: &str, name: &str) -> Option<String> {
    let types = k_sema::stdlib::css::property_named(property)
        .map(|p| p.types)
        .unwrap_or(&[]);
    for t in types {
        let cat = type_category(t);
        if let Some(tok) = all.iter().find(|x| {
            x.name == name && (x.category == cat || (cat == "space" && x.category == "spacing"))
        }) {
            return Some(format!("var({})", token_property(&tok.category, &tok.name)));
        }
    }
    all.iter()
        .find(|x| x.name == name)
        .map(|tok| format!("var({})", token_property(&tok.category, &tok.name)))
}

/// Prints the CSS text of a value expression in a theme (a literal).
fn literal_text(n: &SyntaxNode) -> String {
    n.text().to_string().trim().to_owned()
}

/// The generic stack after a family that doesn't name a kind: the
/// platform's UI font, then the common sans-serif faces
/// (07-themes-and-units.md, "Fonts"). It is also the body's font in an app
/// whose theme has no `font body`.
pub(crate) const SANS_STACK: &str =
    "system-ui, -apple-system, \"Segoe UI\", Roboto, \"Helvetica Neue\", Arial, sans-serif";

/// The fallback stack after a serif family (one whose name says `Serif`
/// and not `Sans`).
const SERIF_STACK: &str = "ui-serif, Georgia, Cambria, \"Times New Roman\", serif";

/// The fallback stack after a monospaced family (`Mono` or `Code` in its
/// name).
const MONO_STACK: &str = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";

/// The fallback stack for the family `family` (its CSS string): serif or
/// monospace when its name says so, sans-serif otherwise.
fn fallback_stack(family: &str) -> &'static str {
    let f = family.to_ascii_lowercase();
    if f.contains("mono") || f.contains("code") {
        MONO_STACK
    } else if f.contains("serif") && !f.contains("sans") {
        SERIF_STACK
    } else {
        SANS_STACK
    }
}

/// Prints the CSS text of a token value of `category`: a `font` token's
/// `Font(family: "Inter", size: 16px)` is the value of the `font`
/// shorthand, size then family then a generic fallback stack
/// (`16px "Inter", system-ui, …, sans-serif`, [`fallback_stack`]), with
/// the family's K string literal kept as the CSS string (both escape `"`
/// and `\\` with a backslash), so text never falls back to the browser's
/// default serif when the family isn't installed; every other value is its
/// literal text.
fn token_value(category: &str, n: &SyntaxNode) -> String {
    if category == "font" {
        if let Some(text) = font_shorthand(n) {
            return text;
        }
    }
    literal_text(n)
}

/// `size family, fallback` of a `Font(family:size:)` call, or `None` when
/// the value isn't one with both arguments.
fn font_shorthand(n: &SyntaxNode) -> Option<String> {
    let call = ast::CallExpr::cast(n.clone())?;
    let mut family = None;
    let mut size = None;
    for a in call.arg_list()?.args() {
        let value = a.value().map(|v| literal_text(v.syntax()))?;
        match a.label()?.text() {
            "family" => family = Some(value),
            "size" => size = Some(value),
            _ => {}
        }
    }
    let family = family?;
    Some(format!("{} {family}, {}", size?, fallback_stack(&family)))
}

/// The first of `names` that is a color token of the app: the token the
/// base stylesheet takes a page colour from.
fn first_color<'t>(all: &'t [Token], names: &[&str]) -> Option<&'t Token> {
    names
        .iter()
        .find_map(|n| all.iter().find(|t| t.category == "color" && t.name == *n))
}

/// The page's text colour token: `text`, else `foreground`.
const TEXT_TOKENS: &[&str] = &["text", "foreground", "onBackground", "onSurface"];

/// The page's background colour token: `background`, else `surface`.
const BACKGROUND_TOKENS: &[&str] = &["background", "surface"];

/// `Some(true)` when the hex color `text` (`#0F172A`) is dark (its
/// relative luminance below one half), `None` for anything but a hex
/// literal.
fn is_dark(text: &str) -> Option<bool> {
    let hex = text.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |s: &str| u8::from_str_radix(s, 16).ok().map(f64::from);
    let (r, g, b) = match hex.len() {
        3 | 4 => {
            let d = |i: usize| channel(&hex[i..=i].repeat(2));
            (d(0)?, d(1)?, d(2)?)
        }
        6 | 8 => (
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
        ),
        _ => return None,
    };
    let lin = |c: f64| {
        let c = c / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
    Some(luminance < 0.5)
}

/// The `color-scheme` of the base theme: dark when its page background is
/// a dark color literal, light otherwise.
fn base_scheme(all: &[Token]) -> &'static str {
    let dark = first_color(all, BACKGROUND_TOKENS)
        .and_then(|t| t.value.as_ref())
        .and_then(|v| is_dark(&literal_text(v)));
    if dark == Some(true) { "dark" } else { "light" }
}

/// The base stylesheet of an app (07-themes-and-units.md, "Base styles"):
/// `hidden` stays hidden whatever a style sets `display` to, the body has
/// no margin and takes the theme's `font body` (else [`SANS_STACK`]) and
/// page colours, and form controls inherit the font instead of the
/// platform's.
fn base_rules(all: &[Token]) -> String {
    let mut css = String::from("[hidden] {\n  display: none !important;\n}\n");
    css.push_str("body {\n  margin: 0;\n");
    if all.iter().any(|t| t.category == "font" && t.name == "body") {
        css.push_str("  font: var(--k-font-body);\n");
    } else {
        css.push_str(&format!("  font-family: {SANS_STACK};\n"));
    }
    if let Some(t) = first_color(all, TEXT_TOKENS) {
        css.push_str(&format!(
            "  color: var({});\n",
            token_property("color", &t.name)
        ));
    }
    if let Some(t) = first_color(all, BACKGROUND_TOKENS) {
        css.push_str(&format!(
            "  background-color: var({});\n",
            token_property("color", &t.name)
        ));
    }
    css.push_str("}\n");
    css.push_str("button, input, select, textarea {\n  font: inherit;\n}\n");
    css
}

/// The base rules of the libraries the project uses: a `ReorderableList`
/// (`k.drag`, a `ul` marked `data-k-list`) has no bullets, margin or
/// padding, at zero specificity so the app's own classes still win.
fn library_rules(g: &Gen<'_>) -> String {
    if uses_module(&imports(g), "k.drag") {
        ":where(ul[data-k-list]) {\n  list-style: none;\n  margin: 0;\n  padding: 0;\n}\n"
            .to_owned()
    } else {
        String::new()
    }
}

/// The stylesheet files: `app.[hash].css` with the theme and every
/// component's rules.
pub(crate) fn files(g: &Gen<'_>) -> Vec<OutputFile> {
    let mut css = String::new();
    // Where each style rule and declaration came from (development source
    // maps, codegen.md §18).
    let mut marks: Vec<crate::js::Mark> = Vec::new();
    let all = tokens(g);
    let is_app = g
        .own
        .iter()
        .any(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::App));
    // `color-scheme` follows the variants in an app with a theme, so native
    // controls, scrollbars and default link colours match the page.
    let themed_app = is_app && !own_themes(g).is_empty();
    let scheme = base_scheme(&all);
    if !all.is_empty() {
        css.push_str(":root {\n");
        if themed_app {
            css.push_str(&format!("  color-scheme: {scheme};\n"));
        }
        for t in &all {
            let v = t
                .value
                .as_ref()
                .map(|n| token_value(&t.category, n))
                .unwrap_or_default();
            css.push_str(&format!(
                "  {}: {v};\n",
                token_property(&t.category, &t.name)
            ));
        }
        css.push_str("}\n");
    }
    let background = first_color(&all, BACKGROUND_TOKENS).map(|t| t.name.clone());
    // Variants.
    for node in own_themes(g) {
        for v in node
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::ThemeVariant)
        {
            let words: Vec<String> = v
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind().is_name_like())
                .map(|t| t.text().to_owned())
                .collect();
            let Some(name) = words.get(1) else { continue };
            let matches_dark = v.descendants().any(|n| {
                n.kind() == SyntaxKind::ImplicitMemberExpr
                    && ident(&n).as_deref() == Some("prefersDark")
            });
            let matches_light = v.descendants().any(|n| {
                n.kind() == SyntaxKind::ImplicitMemberExpr
                    && ident(&n).as_deref() == Some("prefersLight")
            });
            let mut body = String::new();
            let mut variant_dark = None;
            for o in v
                .children()
                .filter(|c| c.kind() == SyntaxKind::ThemeOverride)
            {
                let Some(tname) = ident(&o) else { continue };
                let cat = all
                    .iter()
                    .find(|x| x.name == tname)
                    .map_or("color", |x| x.category.as_str());
                let value = o.children().find(|c| ast::Expr::can_cast(c.kind()));
                if background.as_deref() == Some(tname.as_str()) {
                    variant_dark = value.as_ref().and_then(|n| is_dark(&literal_text(n)));
                }
                let value = value.map(|n| token_value(cat, &n)).unwrap_or_default();
                body.push_str(&format!("{}: {value};\n", token_property(cat, &tname)));
            }
            if themed_app {
                // A variant's scheme: from its page background when it
                // sets one, else from what it matches or is named.
                let dark = variant_dark.unwrap_or_else(|| {
                    matches_dark
                        || (!matches_light && name.to_ascii_lowercase().contains("dark"))
                        || (!matches_light && scheme == "dark")
                });
                let s = if dark { "dark" } else { "light" };
                body.insert_str(0, &format!("color-scheme: {s};\n"));
            }
            let class = format!("k-theme-{}", kebab(name));
            css.push_str(&format!(":root.{class} {{\n"));
            for line in body.lines() {
                css.push_str(&format!("  {line}\n"));
            }
            css.push_str("}\n");
            if matches_dark || matches_light {
                let scheme = if matches_dark { "dark" } else { "light" };
                css.push_str(&format!(
                    "@media (prefers-color-scheme: {scheme}) {{\n  :root:not(.k-theme-set) {{\n"
                ));
                for line in body.lines() {
                    css.push_str(&format!("    {line}\n"));
                }
                css.push_str("  }\n}\n");
            }
        }
    }
    css.push_str(&library_rules(g));
    css.push_str(&crate::icons::rules(g));
    for &d in &g.own {
        if !matches!(
            g.view.decl(d).named_kind(),
            Some(DeclarationKind::Component | DeclarationKind::Page | DeclarationKind::Layout)
        ) {
            continue;
        }
        let Some(block) = style_block(g, d) else {
            continue;
        };
        let mut p = Printer {
            g,
            owner: d,
            tokens: &all,
            out: String::new(),
            interpolation: 0,
            marks: Vec::new(),
            file: g.view.decl(d).file.raw(),
        };
        for item in block.children() {
            p.item(&item, &[], 0, &[]);
        }
        let base = css.len();
        marks.extend(p.marks.into_iter().map(|mut m| {
            m.pos += base;
            m
        }));
        css.push_str(&p.out);
    }
    if is_app {
        // Every app links one stylesheet, so the document's shape doesn't
        // depend on whether the project has styles.
        let base = base_rules(&all);
        for m in &mut marks {
            m.pos += base.len();
        }
        css.insert_str(0, &base);
    }
    if css.is_empty() {
        return Vec::new();
    }
    if g.options.profile == Profile::Development {
        // A development stylesheet maps its rules back to the style blocks,
        // and is named after its text and its map, so an edit that moves a
        // rule without changing the CSS still loads the new map.
        let sources =
            crate::sourcemap::Sources::new(g.analysis, crate::sourcemap::CLIENT_ROOT, true);
        let map = crate::sourcemap::from_marks(&css, &marks, &sources);
        let json = crate::sourcemap::to_json(&map, None);
        let name = crate::hash::hashed_file_name("app", "css", format!("{css}{json}").as_bytes());
        let base = name.rsplit('/').next().unwrap_or(&name).to_owned();
        let mut contents = css;
        contents.push_str(&format!("/*# sourceMappingURL={base}.map */\n"));
        return vec![
            OutputFile {
                path: format!("{name}.map"),
                contents: json,
                kind: FileKind::SourceMap,
            },
            OutputFile {
                path: name,
                contents,
                kind: FileKind::Css,
            },
        ];
    }
    let contents = if g.options.profile == Profile::Release {
        minify(&css)
    } else {
        css
    };
    // The name carries the hash of the file as shipped.
    let name = crate::bundle::hashed("app", "css", &contents, g.options.profile);
    vec![OutputFile {
        path: name,
        contents,
        kind: FileKind::Css,
    }]
}

/// Release builds strip indentation and line breaks.
fn minify(css: &str) -> String {
    css.lines().map(str::trim).collect::<Vec<_>>().join("")
}

/// Prints the style block of one component as scoped CSS.
///
/// Nested rules are flattened against their parents' selectors, at-rules
/// wrap the rules inside them, and interpolations become custom properties
/// numbered in source order.
struct Printer<'p, 'g> {
    /// The generator context.
    g: &'p Gen<'g>,
    /// The component whose style block is printed, which scopes its class and
    /// keyframes names.
    owner: DeclId,
    /// The project's theme tokens, for `.name` references.
    tokens: &'p [Token],
    /// The CSS printed so far.
    out: String,
    /// The number of interpolations printed so far: the index of the next
    /// one's custom property, matching [`reactive_values`].
    interpolation: usize,
    /// Source-map marks: where each printed rule and declaration came from.
    marks: Vec<crate::js::Mark>,
    /// The raw id of the file the style block is in.
    file: u32,
}

impl Printer<'_, '_> {
    /// Prints a rule, at-rule or keyframes; `parents` are the enclosing
    /// selectors (nesting), `wrappers` the enclosing at-rule preludes.
    fn item(&mut self, item: &SyntaxNode, parents: &[String], _depth: usize, wrappers: &[String]) {
        match item.kind() {
            SyntaxKind::StyleRule => {
                let selectors: Vec<String> = item
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::Selector)
                    .flat_map(|s| {
                        let mine = self.selector(&s, parents);
                        mine.into_iter()
                    })
                    .collect();
                let decl_nodes: Vec<SyntaxNode> = item
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::Declaration)
                    .collect();
                let decls: Vec<String> = decl_nodes.iter().map(|d| self.declaration(d)).collect();
                if !decls.is_empty() {
                    let locs = std::iter::once(item)
                        .chain(decl_nodes.iter())
                        .map(|n| self.loc(n))
                        .collect::<Vec<_>>();
                    self.emit_mapped(&selectors.join(", "), &decls, wrappers, &locs);
                }
                for child in item
                    .children()
                    .filter(|c| matches!(c.kind(), SyntaxKind::StyleRule | SyntaxKind::AtRule))
                {
                    self.item(&child, &selectors, 0, wrappers);
                }
            }
            SyntaxKind::AtRule => {
                let name = ident(item).unwrap_or_default();
                let prelude = item
                    .children()
                    .find(|c| c.kind() == SyntaxKind::AtPrelude)
                    .map(|p| self.prelude(&p))
                    .unwrap_or_default();
                let mut w = wrappers.to_vec();
                w.push(format!("@{name} {prelude}"));
                let decl_nodes: Vec<SyntaxNode> = item
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::Declaration)
                    .collect();
                let decls: Vec<String> = decl_nodes.iter().map(|d| self.declaration(d)).collect();
                if !decls.is_empty() && !parents.is_empty() {
                    let locs = std::iter::once(item)
                        .chain(decl_nodes.iter())
                        .map(|n| self.loc(n))
                        .collect::<Vec<_>>();
                    self.emit_mapped(&parents.join(", "), &decls, &w, &locs);
                }
                for child in item
                    .children()
                    .filter(|c| matches!(c.kind(), SyntaxKind::StyleRule | SyntaxKind::AtRule))
                {
                    self.item(&child, parents, 0, &w);
                }
            }
            SyntaxKind::KeyframesRule => {
                let name = item
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|t| t.kind().is_name_like())
                    .nth(1)
                    .map(|t| t.text().to_owned())
                    .unwrap_or_default();
                let scoped = class_name(self.g, self.owner, &name);
                let mut body = format!("@keyframes {scoped} {{\n");
                for kb in item
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::KeyframeBlock)
                {
                    let sels: Vec<String> = kb
                        .children()
                        .filter(|c| c.kind() == SyntaxKind::KeyframeSelector)
                        .map(|s| s.text().to_string().trim().to_owned())
                        .collect();
                    body.push_str(&format!("  {} {{\n", sels.join(", ")));
                    for d in kb
                        .children()
                        .filter(|c| c.kind() == SyntaxKind::Declaration)
                    {
                        let text = self.declaration(&d);
                        body.push_str(&format!("    {text}\n"));
                    }
                    body.push_str("  }\n");
                }
                body.push_str("}\n");
                self.out.push_str(&body);
            }
            _ => {}
        }
    }

    /// The source-map location of `node`: its first non-trivia token.
    fn loc(&self, node: &SyntaxNode) -> crate::js::Loc {
        let start = node
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| !t.kind().is_trivia())
            .map_or_else(|| node.text_range().start(), |t| t.text_range().start());
        crate::js::Loc {
            file: self.file,
            offset: u32::from(start),
        }
    }

    /// Records that what is printed next came from `loc`.
    fn mark(&mut self, loc: crate::js::Loc) {
        self.marks.push(crate::js::Mark {
            pos: self.out.len(),
            loc,
            name: None,
        });
    }

    /// Appends one rule, `selector { decls }`, nested inside each at-rule
    /// prelude of `wrappers` (outermost first); `locs` are the rule's
    /// location and then each declaration's.
    fn emit_mapped(
        &mut self,
        selector: &str,
        decls: &[String],
        wrappers: &[String],
        locs: &[crate::js::Loc],
    ) {
        let mut indent = String::new();
        for w in wrappers {
            self.out.push_str(&format!("{indent}{w} {{\n"));
            indent.push_str("  ");
        }
        self.out.push_str(&indent);
        if let Some(l) = locs.first() {
            self.mark(*l);
        }
        self.out.push_str(&format!("{selector} {{\n"));
        for (i, d) in decls.iter().enumerate() {
            self.out.push_str(&format!("{indent}  "));
            if let Some(l) = locs.get(i + 1) {
                self.mark(*l);
            }
            self.out.push_str(&format!("{d}\n"));
        }
        self.out.push_str(&format!("{indent}}}\n"));
        for _ in wrappers {
            indent.truncate(indent.len().saturating_sub(2));
            self.out.push_str(&format!("{indent}}}\n"));
        }
    }

    /// One selector, combined with each parent selector.
    fn selector(&self, s: &SyntaxNode, parents: &[String]) -> Vec<String> {
        let mut text = String::new();
        let mut uses_parent = false;
        for el in s.children_with_tokens() {
            match el {
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::CompoundSelector => {
                    for part in n.children() {
                        match part.kind() {
                            SyntaxKind::ClassSelector => {
                                if let Some(name) = ident(&part) {
                                    text.push('.');
                                    text.push_str(&class_name(self.g, self.owner, &name));
                                }
                            }
                            SyntaxKind::NestingSelector => {
                                uses_parent = true;
                                text.push('&');
                            }
                            SyntaxKind::PseudoClass => {
                                let inner = part.text().to_string();
                                let inner = inner.trim().trim_start_matches(':');
                                text.push(':');
                                text.push_str(&kebab(inner));
                            }
                            SyntaxKind::PseudoElement => {
                                let inner = part.text().to_string();
                                let inner = inner.trim().trim_start_matches(':');
                                text.push_str("::");
                                text.push_str(&kebab(inner));
                            }
                            _ => text.push_str(part.text().to_string().trim()),
                        }
                    }
                }
                rowan::NodeOrToken::Token(t) if !t.kind().is_trivia() => {
                    text.push(' ');
                    text.push_str(t.text());
                    text.push(' ');
                }
                rowan::NodeOrToken::Token(_) => {
                    if !text.is_empty() && !text.ends_with(' ') {
                        text.push(' ');
                    }
                }
                rowan::NodeOrToken::Node(n) => text.push_str(n.text().to_string().trim()),
            }
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if parents.is_empty() {
            return vec![text.replace('&', "")];
        }
        parents
            .iter()
            .map(|p| {
                if uses_parent {
                    text.replace('&', p)
                } else {
                    format!("{p} {text}")
                }
            })
            .collect()
    }

    /// The CSS text of an at-rule prelude: keywords as written and each media
    /// feature as `(kebab-name: value)`.
    fn prelude(&self, p: &SyntaxNode) -> String {
        let mut out = Vec::new();
        for c in p.children_with_tokens() {
            match c {
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::MediaFeature => {
                    let name = ident(&n).unwrap_or_default();
                    let value = n
                        .children()
                        .find(|x| x.kind() == SyntaxKind::CssValue)
                        .map(|v| self.value_text("", &v))
                        .unwrap_or_default();
                    out.push(format!("({}: {value})", kebab(&name)));
                }
                rowan::NodeOrToken::Token(t) if t.kind().is_name_like() => {
                    out.push(t.text().to_owned())
                }
                _ => {}
            }
        }
        out.join(" ")
    }

    /// The CSS text of one declaration, `css-property: value;`, with the K
    /// property name mapped to its CSS name.
    fn declaration(&mut self, d: &SyntaxNode) -> String {
        let prop = d
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind().is_name_like())
            .map(|t| t.text().to_owned())
            .unwrap_or_default();
        // Comma-separated groups (`fontFamily: "Inter", sansSerif`) are
        // sibling values.
        let values: Vec<SyntaxNode> = d
            .children()
            .filter(|c| c.kind() == SyntaxKind::CssValue)
            .collect();
        let v = values
            .iter()
            .map(|v| self.value_mut(&prop, v))
            .collect::<Vec<_>>()
            .join(", ");
        let css_prop = k_sema::stdlib::css::property_named(&prop)
            .map_or_else(|| kebab(&prop), |p| p.css.to_owned());
        format!("{css_prop}: {v};")
    }

    /// The CSS text of the value of property `prop`, turning each
    /// interpolation into `var(--k-…)` and advancing
    /// [`Printer::interpolation`].
    fn value_mut(&mut self, prop: &str, v: &SyntaxNode) -> String {
        let mut parts = Vec::new();
        for el in v.children_with_tokens() {
            match el {
                rowan::NodeOrToken::Node(n) => {
                    if n.kind() == SyntaxKind::Interpolation {
                        parts.push(format!(
                            "var({})",
                            reactive_property(self.g, self.owner, self.interpolation)
                        ));
                        self.interpolation += 1;
                    } else if n.kind() == SyntaxKind::CssFunction {
                        let (name, args) = function_parts(&n);
                        let args: Vec<String> =
                            args.iter().map(|a| self.value_mut(prop, a)).collect();
                        parts.push(format!("{name}({})", args.join(", ")));
                    } else {
                        parts.push(self.expr_text(prop, &n));
                    }
                }
                rowan::NodeOrToken::Token(t) => {
                    if t.kind() == SyntaxKind::Comma {
                        if let Some(last) = parts.last_mut() {
                            last.push(',');
                        }
                    } else if !t.kind().is_trivia() {
                        parts.push(token_text(&t));
                    }
                }
            }
        }
        parts.join(" ")
    }

    /// The CSS text of a value that can't hold interpolations (a media
    /// feature's value).
    fn value_text(&self, prop: &str, v: &SyntaxNode) -> String {
        let mut parts = Vec::new();
        for el in v.children_with_tokens() {
            match el {
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::CssFunction => {
                    let (name, args) = function_parts(&n);
                    let args: Vec<String> = args.iter().map(|a| self.value_text(prop, a)).collect();
                    parts.push(format!("{name}({})", args.join(", ")));
                }
                rowan::NodeOrToken::Node(n) => parts.push(self.expr_text(prop, &n)),
                rowan::NodeOrToken::Token(t) if !t.kind().is_trivia() => parts.push(token_text(&t)),
                rowan::NodeOrToken::Token(_) => {}
            }
        }
        parts.join(" ")
    }

    /// The CSS text of one value expression of property `prop`: a theme token
    /// reference, a scoped keyframes name (`animation`, `animationName`),
    /// `.token.opacity(x)` and `.token.mix(other, amount)` as
    /// `color-mix(…)`, or a literal.
    fn expr_text(&self, prop: &str, n: &SyntaxNode) -> String {
        match n.kind() {
            SyntaxKind::ImplicitMemberExpr => {
                let name = ident(n).unwrap_or_default();
                if prop == "animationName" || prop == "animation" {
                    return class_name(self.g, self.owner, &name);
                }
                token_ref(self.tokens, prop, &name).unwrap_or_else(|| kebab(&name))
            }
            SyntaxKind::CallExpr => {
                // `.primary.opacity(0.8)` → `color-mix(…)`.
                let c = ast::CallExpr::cast(n.clone());
                if let Some(ast::Expr::MemberExpr(m)) = c.as_ref().and_then(ast::CallExpr::callee) {
                    let method = m.name().map(|t| t.text().to_owned()).unwrap_or_default();
                    let recv = m
                        .receiver()
                        .map(|r| self.expr_text(prop, r.syntax()))
                        .unwrap_or_default();
                    let args: Vec<SyntaxNode> = c
                        .as_ref()
                        .and_then(ast::CallExpr::arg_list)
                        .map(|l| {
                            l.args()
                                .filter_map(|a| a.value())
                                .map(|v| v.syntax().clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    let arg = args
                        .first()
                        .map(|v| v.text().to_string())
                        .unwrap_or_default();
                    // `.primary.mix(.white, 20%)`: 20% white mixed into
                    // the color (07-themes-and-units.md, "Colors").
                    if method == "mix" {
                        if let [other, amount] = args.as_slice() {
                            let other = self.expr_text(prop, other);
                            let amount = self.expr_text(prop, amount);
                            return format!("color-mix(in srgb, {recv}, {other} {amount})");
                        }
                    }
                    if method == "opacity" {
                        let pct = arg
                            .trim()
                            .parse::<f64>()
                            .map_or_else(|_| arg.clone(), |a| crate::lower::fmt_num(a * 100.0));
                        return format!("color-mix(in srgb, {recv} {pct}%, transparent)");
                    }
                }
                n.text().to_string()
            }
            SyntaxKind::Literal => literal_text(n),
            _ => n.text().to_string().trim().to_owned(),
        }
    }
}
