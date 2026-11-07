//! Hover: the symbol's declaration as K source, its rendered doc comment
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
            code(&mut md, &format!("class .{name} // in {component}"))
        }
        Resolution::Keyframes { component, name } => {
            code(&mut md, &format!("@keyframes {name} // in {component}"));
        }
        Resolution::Token { category, name } => {
            code(&mut md, &format!("{category}.{name}"));
            if let Some(def) = analysis.definition(&symbol.resolution)? {
                docs_of(analysis, def.file, def.range, link, &mut md);
            }
        }
        resolution @ (Resolution::Declaration { .. } | Resolution::Member { .. }) => {
            match view.resolve(resolution) {
                Some(Resolved::Decl(d)) => decl_hover(analysis, view, d, link, &mut md)?,
                Some(Resolved::Member(m)) => member_hover(analysis, view, m, link, &mut md)?,
                None => code(&mut md, &resolution.render(|_| None)),
            }
        }
    }
    Ok(Some(HoverResult {
        range: symbol.range,
        markdown: md.trim_end().to_owned(),
    }))
}

/// Writes a fenced K code block.
fn code(md: &mut String, source: &str) {
    let _ = writeln!(md, "```k\n{source}\n```");
}

/// The hover of declaration `d`.
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
    if let Some(side) = info.side {
        if side != k_sema::Side::Shared {
            facts.push(format!("Side: {side}"));
        }
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

/// The `///` lines documenting the item starting at `start`, each as the
/// offset of its body (after `///` and one space) and the body. The item's
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
        if let Ok(Some(resolution)) = analysis.resolve_at(file, range) {
            if let Ok(Some(def)) = analysis.definition(&resolution) {
                return Some(def);
            }
        }
    }
    None
}

/// The type of the innermost expression at `offset` that has one.
fn expression_type(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Option<HoverResult>, Cancelled> {
    let Some(root) = syntax(analysis, file) else {
        return Ok(None);
    };
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
