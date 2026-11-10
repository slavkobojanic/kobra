//! Hover: what the editor shows for the symbol or expression under the
//! cursor.
//!
//! On a symbol: its declaration as K source, its rendered doc comment
//! with intra-doc links resolved, the reactive dependencies of a
//! `@Derived` field or `@Effect`, a page's URL and a declaration's side of
//! the server/client boundary (`docs/design/13-tooling.md`, "Language
//! server"). Off a symbol, hover shows the type of the innermost typed
//! expression.

use std::fmt::Write as _;

use k_diagnostics::{FileId, Span, TextRange, TextSize};
use k_sema::stdlib::{css, elements};
use k_sema::view::{DeclId, MemberId, ProgramView, Resolved};
use k_sema::{Analysis, Cancelled, Resolution};

use super::reactive::{self, Dependencies};
use super::{name_token_at, render, symbol_at, syntax};

/// A hover's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverResult {
    /// The range the hover is for.
    pub range: TextRange,
    /// The content, as Markdown.
    pub markdown: String,
}

/// The hover at `offset`. `link` turns a span into a URL for intra-doc
/// links (`file:///…#L12`); a link it can't place is rendered as code.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn hover(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
    link: &dyn Fn(Span) -> Option<String>,
) -> Result<Option<HoverResult>, Cancelled> {
    let view = analysis.program()?;
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return expression_type(analysis, file, offset);
    };
    let mut md = String::new();
    match &symbol.resolution {
        Resolution::Local { name, definition } => {
            let ty = analysis.type_at(definition.file, definition.range)?;
            code(
                &mut md,
                &format!(
                    "let {name}{}",
                    ty.map(|t| format!(": {t}")).unwrap_or_default()
                ),
            );
        }
        Resolution::Parameter { name, definition } => {
            let ty = analysis.type_at(definition.file, definition.range)?;
            code(
                &mut md,
                &format!("{name}{}", ty.map(|t| format!(": {t}")).unwrap_or_default()),
            );
            md.push_str("\n_parameter_\n");
        }
        Resolution::Element { name } => {
            code(&mut md, &format!("element {name}"));
            if let Some(def) = elements::element(name) {
                let _ = writeln!(md, "\nRenders `<{}>`.", def.html);
            }
        }
        Resolution::Attribute { element, name } => {
            let param = elements::element(element).and_then(|e| e.attribute(name));
            match param {
                Some(p) => {
                    code(&mut md, &format!("{name}: {}", p.ty));
                    let _ = writeln!(md, "\nAn attribute of `{element}`. {}", p.doc);
                }
                None => code(&mut md, &format!("{element}.{name}")),
            }
        }
        Resolution::Event { owner, name } => {
            code(&mut md, &format!("event {owner}.{name}"));
        }
        Resolution::CssProperty { name } => match css::property_named(name) {
            Some(p) => {
                code(&mut md, &format!("{name}: {}", p.syntax));
                let _ = writeln!(md, "\nThe CSS property `{}`.", p.css);
            }
            None => code(&mut md, name),
        },
        Resolution::Class { component, name } => {
            let rule = analysis
                .definition(&symbol.resolution)?
                .and_then(|def| style_rule(analysis, def));
            match rule {
                Some(rule) => code(&mut md, &format!("// class .{name} in {component}\n{rule}")),
                None => code(&mut md, &format!("class .{name} // in {component}")),
            }
        }
        Resolution::Keyframes { component, name } => {
            code(&mut md, &format!("@keyframes {name} // in {component}"));
        }
        Resolution::Token { category, name } => {
            let def = analysis.definition(&symbol.resolution)?;
            // The token's declaration, with its value.
            let member = def.and_then(|def| {
                view.members.iter().find(|m| {
                    m.kind == k_sema::view::MemberKind::Token
                        && m.name_range == def.range
                        && view.decl(m.owner).file == def.file
                })
            });
            let source = member.and_then(|m| {
                let source = super::completion::token_source(analysis, view, m)?;
                Some(format!(
                    "{source} // in {}",
                    view.decl(m.owner).display_name
                ))
            });
            code(
                &mut md,
                &source.unwrap_or_else(|| format!("{category}.{name}")),
            );
            let variants = def
                .map(|d| variant_values(analysis, d, name))
                .unwrap_or_default();
            if !variants.is_empty() {
                let list: Vec<String> = variants
                    .iter()
                    .map(|(variant, value)| format!("`{variant}`: `{value}`"))
                    .collect();
                let _ = writeln!(md, "\nVariants: {}", list.join(" · "));
            }
            if let Some(def) = def {
                docs_of(analysis, def.file, def.range, link, &mut md);
            }
        }
        resolution @ (Resolution::Declaration { .. } | Resolution::Member { .. }) => {
            match view.resolve(resolution) {
                Some(Resolved::Decl(d)) => decl_hover(analysis, view, d, link, &mut md)?,
                Some(Resolved::Member(m)) => member_hover(analysis, view, m, link, &mut md)?,
                None => code(&mut md, &resolution.render(|_| None)),
            }
            if let Some(line) = instantiation(analysis, view, file, symbol.range, resolution) {
                let _ = writeln!(md, "\nCalled as `{line}`.");
            }
        }
    }
    Ok(Some(HoverResult {
        range: symbol.range,
        markdown: md.trim_end().to_owned(),
    }))
}

/// The signature of a generic function as a call with explicit type
/// arguments instantiates it (`decode<User>(_ json: String) -> User` for
/// the name of `decode<User>(json)`), or `None` without them.
fn instantiation(
    analysis: &Analysis,
    view: &ProgramView,
    file: FileId,
    name: TextRange,
    resolution: &Resolution,
) -> Option<String> {
    let root = syntax(analysis, file)?;
    let token = root.token_at_offset(name.start()).right_biased()?;
    let list = token
        .parent()?
        .children()
        .find(|c| c.kind() == k_syntax::SyntaxKind::GenericArgList)?;
    let written: Vec<String> = list
        .children()
        .map(|t| t.text().to_string().trim().to_owned())
        .collect();
    let (label, sig) = match view.resolve(resolution)? {
        Resolved::Decl(d) => (view.decl(d).name.clone(), view.decl(d).signature.as_ref()?),
        Resolved::Member(m) => (
            view.member(m).name.clone(),
            view.member(m).signature.as_ref()?,
        ),
    };
    if sig.generics.len() != written.len() {
        return None;
    }
    let subst: Vec<_> = sig.generics.iter().copied().zip(written).collect();
    Some(render::instantiated(view, &label, sig, &subst))
}

/// The values the variants of the theme declaring token `name` (at
/// `def`) give it: `("dark", "#4ADE80")` for `variant dark … { name = #4ADE80 }`.
fn variant_values(analysis: &Analysis, def: Span, name: &str) -> Vec<(String, String)> {
    use k_syntax::SyntaxKind;
    let Some(root) = syntax(analysis, def.file) else {
        return Vec::new();
    };
    let Some(theme) = root
        .token_at_offset(def.range.start())
        .right_biased()
        .and_then(|t| {
            t.parent_ancestors()
                .find(|n| n.kind() == SyntaxKind::ThemeDecl)
        })
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for variant in theme
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::ThemeVariant)
    {
        let Some(variant_name) = variant
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::Ident)
        else {
            continue;
        };
        for o in variant
            .children()
            .filter(|n| n.kind() == SyntaxKind::ThemeOverride)
        {
            let text = o.text().to_string();
            if let Some((target, value)) = text.split_once('=')
                && target.trim() == name
            {
                out.push((variant_name.text().to_owned(), value.trim().to_owned()));
            }
        }
    }
    out
}

/// The longest rule hover shows, in lines.
const RULE_LINES: usize = 24;

/// The source of the style rule whose selector holds `name` (a class
/// selector's name), dedented and cut at [`RULE_LINES`] lines.
fn style_rule(analysis: &Analysis, name: Span) -> Option<String> {
    let root = syntax(analysis, name.file)?;
    let rule = root
        .token_at_offset(name.range.start())
        .right_biased()?
        .parent_ancestors()
        .find(|n| n.kind() == k_syntax::SyntaxKind::StyleRule)?;
    let text = analysis.text(name.file)?;
    let start = usize::from(rule.text_range().start());
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let indent = text[line_start..start].len();
    let source = text.get(start..usize::from(rule.text_range().end()))?;
    let mut lines: Vec<&str> = source
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                l
            } else {
                let strip = l.len() - l.trim_start().len();
                &l[strip.min(indent)..]
            }
        })
        .collect();
    if lines.len() > RULE_LINES {
        lines.truncate(RULE_LINES);
        lines.push("…");
    }
    Some(lines.join("\n").trim_end().to_owned())
}

/// Writes a fenced K code block.
fn code(md: &mut String, source: &str) {
    let _ = writeln!(md, "```k\n{source}\n```");
}

/// The hover of declaration `d`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn decl_hover(
    analysis: &Analysis,
    view: &ProgramView,
    d: DeclId,
    link: &dyn Fn(Span) -> Option<String>,
    md: &mut String,
) -> Result<(), Cancelled> {
    let info = view.decl(d);
    code(md, &render::decl(view, d));
    let mut facts = Vec::new();
    if let Some(url) = analysis.page_url(d)? {
        facts.push(format!("URL: `{url}`"));
    }
    if let Some(side) = info.side
        && side != k_sema::Side::Shared
    {
        facts.push(format!("Side: {side}"));
    }
    // `@Rendering(.client)` on a component (`.server` there is K4027).
    if info.named_kind() == Some(k_sema::stdlib::DeclarationKind::Component)
        && info.has_annotation("Rendering")
    {
        facts.push("Rendered in the browser only; the server renders `loading`".to_owned());
    }
    if !facts.is_empty() {
        md.push('\n');
        md.push_str(&facts.join(" · "));
        md.push('\n');
    }
    docs_of(analysis, info.file, info.range, link, md);
    Ok(())
}

/// The hover of member `m`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn member_hover(
    analysis: &Analysis,
    view: &ProgramView,
    m: MemberId,
    link: &dyn Fn(Span) -> Option<String>,
    md: &mut String,
) -> Result<(), Cancelled> {
    let info = view.member(m);
    let owner = render::owner_name(view, m);
    code(md, &format!("// in {owner}\n{}", render::member(view, m)));
    if let Some(note) = user_action_note(info) {
        let _ = writeln!(md, "\n{note}");
    }
    if let Some(deps) = reactive::dependencies(analysis, view, m)? {
        md.push('\n');
        match deps {
            Dependencies::Tracked(names) if names.is_empty() => {
                md.push_str("Reactive dependencies: none (it never re-runs)\n");
            }
            Dependencies::Tracked(names) => {
                let list: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                let _ = writeln!(md, "Reactive dependencies: {}", list.join(", "));
            }
            Dependencies::Explicit(names) => {
                let list: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                let _ = writeln!(
                    md,
                    "Reactive dependencies (explicit, `on:`): {}",
                    list.join(", ")
                );
            }
        }
    }
    if let Some(range) = info.range {
        docs_of(analysis, view.decl(info.owner).file, range, link, md);
    }
    Ok(())
}

/// What hover and signature help say about a member marked `@UserAction`
/// (`docs/design/45-web-platform.md`, "User activation"): browsers allow
/// it only during a user's action, which the compiler checks (K4030).
#[must_use]
pub fn user_action_note(info: &k_sema::view::MemberInfo) -> Option<&'static str> {
    info.has_annotation("UserAction").then_some(
        "**Needs a user's action:** call it from a click, submit or key handler \
         (or a component event declared `@UserAction`), before the handler's first `await`.",
    )
}

/// Appends the rendered doc comment of the item at `range` in `file`.
fn docs_of(
    analysis: &Analysis,
    file: FileId,
    range: TextRange,
    link: &dyn Fn(Span) -> Option<String>,
    md: &mut String,
) {
    let Some(text) = analysis.text(file) else {
        return;
    };
    let lines = doc_lines(text, range.start());
    if lines.is_empty() {
        return;
    }
    md.push_str("\n---\n\n");
    for (offset, body) in lines {
        let rendered = render_links(analysis, file, offset, body, link);
        md.push_str(&rendered);
        md.push('\n');
    }
}

/// The `///` lines documenting the item starting at `start`.
///
/// Each is returned as the offset of its body (after `///` and one space)
/// and the body. The item's
/// syntax may or may not include its leading doc comment; both are handled.
#[must_use]
pub fn doc_lines(text: &str, start: TextSize) -> Vec<(usize, &str)> {
    let start = usize::from(start).min(text.len());
    if !text.is_char_boundary(start) {
        return Vec::new();
    }
    let line_start = text[..start].rfind(['\n', '\r']).map_or(0, |i| i + 1);
    let mut out = Vec::new();
    if text[start..].trim_start().starts_with("///") && text[line_start..start].trim().is_empty() {
        // Forwards from the item's start.
        let mut at = line_start;
        for line in text[line_start..].split_inclusive('\n') {
            match doc_body(line) {
                Some((skip, body)) => out.push((at + skip, body)),
                None => break,
            }
            at += line.len();
        }
        return out;
    }
    // Backwards from the line before the item.
    let mut end = line_start;
    while end > 0 {
        let prev_end = end - 1;
        let prev_start = text[..prev_end].rfind('\n').map_or(0, |i| i + 1);
        let line = &text[prev_start..end];
        match doc_body(line) {
            Some((skip, body)) => out.push((prev_start + skip, body)),
            None => break,
        }
        end = prev_start;
    }
    out.reverse();
    out
}

/// The body of a `///` comment line (without the line break), and its
/// byte offset in the line.
fn doc_body(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("///")?;
    if rest.starts_with('/') {
        return None;
    }
    let skip = usize::from(rest.starts_with(' '));
    let body = rest[skip..].trim_end_matches(['\n', '\r']);
    Some((indent + 3 + skip, body))
}

/// Rewrites the ``[`name`]`` links of one doc line (at `offset` in `file`)
/// as Markdown links to their declarations, using the resolutions the
/// checker recorded for each segment (the last one that resolves wins).
fn render_links(
    analysis: &Analysis,
    file: FileId,
    offset: usize,
    body: &str,
    link: &dyn Fn(Span) -> Option<String>,
) -> String {
    let mut out = String::new();
    let mut rest = body;
    let mut at = offset;
    while let Some(i) = rest.find("[`") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("`]") else { break };
        let name = &after[..j];
        let name_start = at + i + 2;
        out.push_str(&rest[..i]);
        let target = link_target(analysis, file, name_start, name).and_then(link);
        match target {
            Some(url) if !name.contains(' ') => {
                let _ = write!(out, "[`{name}`]({url})");
            }
            _ => {
                let _ = write!(out, "`{name}`");
            }
        }
        let consumed = i + 2 + j + 2;
        rest = &rest[consumed..];
        at += consumed;
    }
    out.push_str(rest);
    out
}

/// The declaration a doc link at `start` names: its last resolved segment.
fn link_target(analysis: &Analysis, file: FileId, start: usize, name: &str) -> Option<Span> {
    let base = name.split('(').next().unwrap_or(name);
    let mut segments = Vec::new();
    let mut at = start;
    for segment in base.split('.') {
        segments.push((at, segment.len()));
        at += segment.len() + 1;
    }
    for (offset, len) in segments.into_iter().rev() {
        let range = TextRange::at(
            TextSize::from(u32::try_from(offset).ok()?),
            TextSize::from(u32::try_from(len).ok()?),
        );
        if let Ok(Some(resolution)) = analysis.resolve_at(file, range)
            && let Ok(Some(def)) = analysis.definition(&resolution)
        {
            return Some(def);
        }
    }
    None
}

/// The type of the innermost expression at `offset` that has one.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn expression_type(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Option<HoverResult>, Cancelled> {
    let Some(root) = syntax(analysis, file) else {
        return Ok(None);
    };
    if !root.text_range().contains_inclusive(offset) {
        return Ok(None);
    }
    let token = match name_token_at(&root, offset) {
        Some(t) => t,
        None => match root.token_at_offset(offset).right_biased() {
            Some(t) if !t.kind().is_trivia() => t,
            _ => return Ok(None),
        },
    };
    for node in token.parent_ancestors().take(4) {
        if let Some(ty) = analysis.type_at(file, node.text_range())? {
            let mut md = String::new();
            code(&mut md, &ty);
            return Ok(Some(HoverResult {
                range: node.text_range(),
                markdown: md.trim_end().to_owned(),
            }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_lines_before_an_item() {
        let text = "/// One.\n///\n/// Two [`X`].\n@A\nstruct S {}\n";
        let start = TextSize::from(u32::try_from(text.find("@A").unwrap()).unwrap());
        let lines = doc_lines(text, start);
        let bodies: Vec<&str> = lines.iter().map(|(_, b)| *b).collect();
        assert_eq!(bodies, ["One.", "", "Two [`X`]."]);
        let (offset, body) = lines[2];
        assert_eq!(&text[offset..offset + body.len()], "Two [`X`].");
    }

    #[test]
    fn doc_lines_inside_an_item() {
        let text = "    /// A.\n    /// B.\n    let x: Int\n";
        let lines = doc_lines(text, TextSize::from(4));
        let bodies: Vec<&str> = lines.iter().map(|(_, b)| *b).collect();
        assert_eq!(bodies, ["A.", "B."]);
    }

    #[test]
    fn no_doc_lines() {
        assert!(doc_lines("// plain\nlet x = 1\n", TextSize::from(9)).is_empty());
        assert!(doc_lines("", TextSize::from(0)).is_empty());
        assert!(doc_lines("//// four\nx", TextSize::from(10)).is_empty());
    }
}
