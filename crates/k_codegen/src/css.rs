//! CSS generation: style blocks, theme tokens and variants
//! (`docs/compiler/codegen.md` §12).
//!
//! * Class `panel` of component `shop.CartSummary` is `panel_<suffix>`
//!   (`hash::scope_suffix`), or `CartSummary__panel` under
//!   `Profile::Test`; keyframes are scoped the same way.
//! * Rules print in source order; nesting is flattened and `@media`,
//!   `@container` and `@layer` wrap the flattened rules; property names are
//!   kebab case.
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

/// `true` when scoped names are readable (`Card__panel`) rather than
/// hashed: under [`Profile::Test`], so golden files stay stable.
fn readable(g: &Gen<'_>) -> bool {
    g.options.profile == Profile::Test
}

/// The scoped name of class `class` of component `owner`.
pub(crate) fn class_name(g: &Gen<'_>, owner: DeclId, class: &str) -> String {
    crate::hash::scoped_name(&g.view.qualified_name(owner), class, readable(g))
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

/// The theme declarations of the project.
fn themes(g: &Gen<'_>) -> Vec<DeclId> {
    g.own
        .iter()
        .copied()
        .filter(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::Theme))
        .collect()
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

/// Every token of every theme of the project, in declaration order; a
/// token without both a category and a name is skipped.
fn tokens(g: &Gen<'_>) -> Vec<Token> {
    let mut out = Vec::new();
    for t in themes(g) {
        let Some(node) = g.decl_node(t) else { continue };
        for tok in node
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
    for t in themes(g) {
        let Some(node) = g.decl_node(t) else { continue };
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

/// Prints the CSS text of a token value of `category`: a `font` token's
/// `Font(family: "Inter", size: 16px)` is the value of the `font`
/// shorthand, size then family (`16px "Inter"`), with the family's K
/// string literal kept as the CSS string (both escape `"` and `\\` with a
/// backslash); every other value is its literal text.
fn token_value(category: &str, n: &SyntaxNode) -> String {
    if category == "font" {
        if let Some(text) = font_shorthand(n) {
            return text;
        }
    }
    literal_text(n)
}

/// `size family` of a `Font(family:size:)` call, or `None` when the value
/// isn't one with both arguments.
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
    Some(format!("{} {}", size?, family?))
}

/// The stylesheet files: `app.[hash].css` with the theme and every
/// component's rules.
pub(crate) fn files(g: &Gen<'_>) -> Vec<OutputFile> {
    let mut css = String::new();
    let all = tokens(g);
    if !all.is_empty() {
        css.push_str(":root {\n");
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
    // Variants.
    for t in themes(g) {
        let Some(node) = g.decl_node(t) else { continue };
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
            let mut body = String::new();
            for o in v
                .children()
                .filter(|c| c.kind() == SyntaxKind::ThemeOverride)
            {
                let Some(tname) = ident(&o) else { continue };
                let cat = all
                    .iter()
                    .find(|x| x.name == tname)
                    .map_or("color", |x| x.category.as_str());
                let value = o
                    .children()
                    .find(|c| ast::Expr::can_cast(c.kind()))
                    .map(|n| token_value(cat, &n))
                    .unwrap_or_default();
                body.push_str(&format!("{}: {value};\n", token_property(cat, &tname)));
            }
            let class = format!("k-theme-{}", kebab(name));
            css.push_str(&format!(":root.{class} {{\n"));
            for line in body.lines() {
                css.push_str(&format!("  {line}\n"));
            }
            css.push_str("}\n");
            let matches_dark = v.descendants().any(|n| {
                n.kind() == SyntaxKind::ImplicitMemberExpr
                    && ident(&n).as_deref() == Some("prefersDark")
            });
            let matches_light = v.descendants().any(|n| {
                n.kind() == SyntaxKind::ImplicitMemberExpr
                    && ident(&n).as_deref() == Some("prefersLight")
            });
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
        };
        for item in block.children() {
            p.item(&item, &[], 0, &[]);
        }
        css.push_str(&p.out);
    }
    let is_app = g
        .own
        .iter()
        .any(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::App));
    if is_app {
        // Every app links one stylesheet, so the document's shape doesn't
        // depend on whether the project has styles: the base rule keeps
        // `hidden` elements hidden whatever a style sets `display` to.
        css.insert_str(0, "[hidden] {\n  display: none !important;\n}\n");
    }
    if css.is_empty() {
        return Vec::new();
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
                let decls: Vec<String> = item
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::Declaration)
                    .map(|d| self.declaration(&d))
                    .collect();
                if !decls.is_empty() {
                    self.emit(&selectors.join(", "), &decls, wrappers);
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
                let decls: Vec<String> = item
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::Declaration)
                    .map(|d| self.declaration(&d))
                    .collect();
                if !decls.is_empty() && !parents.is_empty() {
                    self.emit(&parents.join(", "), &decls, &w);
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

    /// Appends one rule, `selector { decls }`, nested inside each at-rule
    /// prelude of `wrappers` (outermost first).
    fn emit(&mut self, selector: &str, decls: &[String], wrappers: &[String]) {
        let mut indent = String::new();
        for w in wrappers {
            self.out.push_str(&format!("{indent}{w} {{\n"));
            indent.push_str("  ");
        }
        self.out.push_str(&format!("{indent}{selector} {{\n"));
        for d in decls {
            self.out.push_str(&format!("{indent}  {d}\n"));
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
        let value = d.children().find(|c| c.kind() == SyntaxKind::CssValue);
        let v = match value {
            Some(v) => self.value_mut(&prop, &v),
            None => String::new(),
        };
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
                        parts.push(t.text().to_owned());
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
                rowan::NodeOrToken::Node(n) => parts.push(self.expr_text(prop, &n)),
                rowan::NodeOrToken::Token(t) if !t.kind().is_trivia() => {
                    parts.push(t.text().to_owned())
                }
                rowan::NodeOrToken::Token(_) => {}
            }
        }
        parts.join(" ")
    }

    /// The CSS text of one value expression of property `prop`: a theme token
    /// reference, a scoped keyframes name (`animation`, `animationName`),
    /// `.token.opacity(x)` as `color-mix(…)`, or a literal.
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
                    let arg = c
                        .as_ref()
                        .and_then(ast::CallExpr::arg_list)
                        .and_then(|l| l.args().next())
                        .and_then(|a| a.value())
                        .map(|v| v.syntax().text().to_string())
                        .unwrap_or_default();
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
