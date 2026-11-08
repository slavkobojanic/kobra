//! Completion in style blocks and of class references
//! (`docs/compiler/lsp.md`, "Completion").
//!
//! Where the cursor is in a style block is read from the block's text
//! before it, which a half-typed block still has: a small scanner tracks
//! the open rules, groups (`@media`) and `@keyframes` and the statement the
//! cursor is in. At a declaration's start it offers the CSS **property
//! names**; after `property:` the property's **keywords** (camel case, from
//! [`css::keywords`]), the CSS-wide keywords and the theme **tokens** of the
//! property's value types (`.md` for `gap`, `.muted` for `color`), each
//! with its value; at a selector's start the component's **classes**; in
//! `@media (…)` the media features and their keywords. A `.` is handled by
//! the caller: after it come tokens, or classes in a selector.
//!
//! **Class references** (`.class(.count)`, `.class([.a, .b when c])`, a
//! child's `@Stylable` prop) complete the classes the enclosing
//! declaration's style block declares.

use k_diagnostics::{FileId, TextSize};
use k_sema::stdlib::css;
use k_sema::view::{DeclId, MemberInfo, MemberKind, ProgramView, Type};
use k_sema::{Analysis, Cancelled};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use super::super::hover::doc_lines;
use super::events::{chain, component_of, link_name};
use super::{CompletionItem, ItemKind};

/// What the scanner found the cursor in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Frame {
    /// The style block itself, at the level of its rules.
    Block,
    /// A rule's body (or a keyframe's).
    Rule,
    /// An `@media`, `@container` or `@layer` group, holding rules.
    Group,
    /// A `@keyframes` block, holding keyframes.
    Keyframes,
}

/// The completions at `start` (the start of the prefix, not after a `.`)
/// in a style block: `Some` inside one (possibly empty), `None` outside,
/// and inside an interpolation (`{accent}`), where names in scope apply.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub(super) fn style_items(
    analysis: &Analysis,
    view: &ProgramView,
    root: &SyntaxNode,
    text: &str,
    start: TextSize,
) -> Result<Option<Vec<CompletionItem>>, Cancelled> {
    let Some(block) = style_block_before(root, start) else {
        return Ok(None);
    };
    let Some(open) = block
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::LBrace)
    else {
        return Ok(None);
    };
    let from = usize::from(open.text_range().end());
    let to = usize::from(start).min(text.len());
    if from > to {
        return Ok(None);
    }
    let Some((frame, statement)) = scan(&text[from..to]) else {
        return Ok(None);
    };
    let statement_trimmed = statement.trim();
    let items = match frame {
        Frame::Rule => {
            if statement_trimmed.is_empty() || after_complete_line(statement) {
                property_items()
            } else if let Some((property, _)) = declaration(statement_trimmed) {
                value_items(analysis, view, property)
            } else {
                Vec::new()
            }
        }
        Frame::Block | Frame::Group => {
            if statement_trimmed.is_empty() {
                class_items(root, start, true, true)
            } else if let Some(rest) = statement_trimmed.strip_prefix("@media") {
                media_items(rest)
            } else {
                Vec::new()
            }
        }
        Frame::Keyframes => {
            if statement_trimmed.is_empty() {
                ["from", "to"]
                    .iter()
                    .map(|k| plain((*k).to_owned(), ItemKind::Keyword, None, 0))
                    .collect()
            } else {
                Vec::new()
            }
        }
    };
    Ok(Some(items))
}

/// The style block holding the last non-trivia token before `start`.
fn style_block_before(root: &SyntaxNode, start: TextSize) -> Option<SyntaxNode> {
    let mut token = root.token_at_offset(start).left_biased();
    while let Some(t) = &token {
        if t.kind().is_trivia() || t.text_range().start() >= start {
            token = t.prev_token();
        } else {
            break;
        }
    }
    token?
        .parent_ancestors()
        .find(|n| n.kind() == SyntaxKind::StyleBlock)
}

/// Scans the text of a style block from after its `{` to the cursor:
/// the innermost open frame and the text of the statement the cursor is
/// in (since the last `{`, `}` or `;`). `None` inside an interpolation or
/// once the block is closed.
fn scan(s: &str) -> Option<(Frame, &str)> {
    let bytes = s.as_bytes();
    let mut stack = vec![Frame::Block];
    let mut statement = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                let top = *stack.last()?;
                let text = s[statement..i].trim();
                if top == Frame::Rule && declaration(text).is_some() {
                    // An interpolation in a value: skip it.
                    let mut depth = 0;
                    while i < bytes.len() {
                        match bytes[i] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    if i >= bytes.len() {
                        return None;
                    }
                } else {
                    stack.push(if text.starts_with("@keyframes") {
                        Frame::Keyframes
                    } else if text.starts_with('@') {
                        Frame::Group
                    } else {
                        Frame::Rule
                    });
                    statement = i + 1;
                }
            }
            b'}' => {
                stack.pop();
                if stack.is_empty() {
                    return None;
                }
                statement = i + 1;
            }
            b';' => statement = i + 1,
            _ => {}
        }
        i += 1;
    }
    Some((*stack.last()?, s.get(statement..)?))
}

/// `property: value…` split at its colon, when `statement` (trimmed) is a
/// declaration: a name of letters and digits, then `:`.
fn declaration(statement: &str) -> Option<(&str, &str)> {
    let (name, rest) = statement.split_once(':')?;
    let name = name.trim_end();
    (name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric()))
    .then_some((name, rest))
}

/// Whether the cursor starts a new line after a complete declaration whose
/// `;` is missing (`display: flex⏎  al`): that is a new declaration.
fn after_complete_line(statement: &str) -> bool {
    let Some((head, current)) = statement.rsplit_once('\n') else {
        return false;
    };
    current.trim().is_empty()
        && declaration(head.trim()).is_some_and(|(_, value)| !value.trim().is_empty())
}

/// An item with no import, documentation or insert text.
fn plain(label: String, kind: ItemKind, detail: Option<String>, group: u8) -> CompletionItem {
    CompletionItem {
        label,
        kind,
        detail,
        import: None,
        group,
        documentation: None,
        insert_text: None,
    }
}

/// Every CSS property, with its CSS name and grammar.
fn property_items() -> Vec<CompletionItem> {
    css::PROPERTIES
        .iter()
        .map(|p| {
            let mut item = plain(
                p.name.to_owned(),
                ItemKind::Property,
                Some(p.css.to_owned()),
                0,
            );
            item.documentation = Some(format!("```k\n{}: {}\n```", p.name, p.syntax));
            item
        })
        .collect()
}

/// The values of property `property`: its tokens, its keywords and the
/// CSS-wide keywords.
fn value_items(analysis: &Analysis, view: &ProgramView, property: &str) -> Vec<CompletionItem> {
    let Some(prop) = css::property_named(property) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for name in prop.types {
        if let Some(d) = view.lookup_display(name).or_else(|| view.lookup(name)) {
            items.extend(token_items(analysis, view, d, true));
        }
    }
    for k in css::keywords(property) {
        items.push(plain(
            (*k).to_owned(),
            ItemKind::Value,
            Some(css::keyword_css(k).into_owned()),
            1,
        ));
    }
    for k in css::WIDE_KEYWORDS {
        items.push(plain((*k).to_owned(), ItemKind::Value, None, 3));
    }
    items
}

/// `@media (feature: value)`: after `(` the features, after `feature:`
/// its keywords. `rest` is the statement after `@media`.
fn media_items(rest: &str) -> Vec<CompletionItem> {
    let Some(open) = rest.rfind('(') else {
        return Vec::new();
    };
    if rest[open..].contains(')') {
        return Vec::new();
    }
    let inside = &rest[open + 1..];
    match inside.split_once(':') {
        None => css::MEDIA_FEATURES
            .iter()
            .map(|(k, c)| {
                plain(
                    (*k).to_owned(),
                    ItemKind::Property,
                    Some((*c).to_owned()),
                    0,
                )
            })
            .collect(),
        Some((feature, _)) => css::media_feature_keywords(feature.trim())
            .iter()
            .map(|k| {
                plain(
                    (*k).to_owned(),
                    ItemKind::Value,
                    Some(css::keyword_css(k).into_owned()),
                    0,
                )
            })
            .collect(),
    }
}

/// The theme tokens of category type `d` (`Spacing`), one per name: a
/// project theme's before the stdlib's. With `dotted`, labels carry the
/// `.` (offered after `gap: `, not after `gap: .`). Each has its type and
/// value as detail and its doc comment as documentation.
pub(super) fn token_items(
    analysis: &Analysis,
    view: &ProgramView,
    d: DeclId,
    dotted: bool,
) -> Vec<CompletionItem> {
    let category = &view.decl(d).name;
    let mut tokens: Vec<&MemberInfo> = view
        .members
        .iter()
        .filter(|m| m.kind == MemberKind::Token && m.ty.as_ref().and_then(Type::decl) == Some(d))
        .collect();
    // Project themes first, so their tokens win over the stdlib's.
    tokens.sort_by_key(|m| view.decl(m.owner).stdlib);
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for member in tokens {
        if !seen.insert(member.name.as_str()) {
            continue;
        }
        let owner = view.decl(member.owner);
        let value = token_value(analysis, view, member);
        let detail = match value {
            Some(v) => format!("{category} = {v} ({})", owner.display_name),
            None => format!("{category} token of {}", owner.display_name),
        };
        let label = if dotted {
            format!(".{}", member.name)
        } else {
            member.name.clone()
        };
        let mut item = plain(
            label,
            ItemKind::Constant,
            Some(detail),
            if owner.stdlib { 2 } else { 0 },
        );
        item.documentation = member.range.and_then(|r| {
            let text = analysis.text(owner.file)?;
            let lines: Vec<&str> = doc_lines(text, r.start())
                .into_iter()
                .map(|(_, b)| b)
                .collect();
            (!lines.is_empty()).then(|| lines.join("\n"))
        });
        items.push(item);
    }
    items
}

/// The source of theme token `member`'s declaration without its doc
/// comment (`color muted: Color = #475569`).
#[must_use]
pub fn token_source(
    analysis: &Analysis,
    view: &ProgramView,
    member: &MemberInfo,
) -> Option<String> {
    let range = member.range?;
    let text = analysis
        .text(view.decl(member.owner).file)?
        .get(usize::from(range.start())..usize::from(range.end()))?;
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

/// The value of theme token `member` as written (`#475569`).
fn token_value(analysis: &Analysis, view: &ProgramView, member: &MemberInfo) -> Option<String> {
    let source = token_source(analysis, view, member)?;
    let (_, value) = source.split_once('=')?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// The classes of the declaration holding `start`: those its style block
/// declares, and with `used` also those its view references. With
/// `dotted`, labels carry the `.`. The class being typed at `start` is
/// left out.
pub(super) fn class_items(
    root: &SyntaxNode,
    start: TextSize,
    used: bool,
    dotted: bool,
) -> Vec<CompletionItem> {
    let Some(decl) = enclosing_decl(root, start) else {
        return Vec::new();
    };
    let mut declared: Vec<String> = Vec::new();
    let mut referenced: Vec<String> = Vec::new();
    let at_cursor = |t: &SyntaxToken| t.text_range().start() == start;
    for node in decl.descendants() {
        match node.kind() {
            SyntaxKind::ClassSelector => {
                if let Some(t) = first_name(&node) {
                    if !at_cursor(&t) && !declared.iter().any(|n| n == t.text()) {
                        declared.push(t.text().to_owned());
                    }
                }
            }
            SyntaxKind::ImplicitMemberExpr if used && is_class_reference(&node) => {
                if let Some(t) = first_name(&node) {
                    if !at_cursor(&t) {
                        referenced.push(t.text().to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    let dot = if dotted { "." } else { "" };
    let mut items: Vec<CompletionItem> = declared
        .iter()
        .map(|n| plain(format!("{dot}{n}"), ItemKind::StyleClass, None, 0))
        .collect();
    for n in referenced {
        if !declared.contains(&n) {
            declared.push(n.clone());
            items.push(plain(
                format!("{dot}{n}"),
                ItemKind::StyleClass,
                Some("used in the view, no rule yet".to_owned()),
                1,
            ));
        }
    }
    items
}

/// The top-level declaration whose syntax holds `start`.
fn enclosing_decl(root: &SyntaxNode, start: TextSize) -> Option<SyntaxNode> {
    let token = root.token_at_offset(start).left_biased()?;
    token.parent_ancestors().find(|n| {
        n.parent()
            .is_some_and(|p| p.kind() == SyntaxKind::SourceFile)
    })
}

/// The first name token of `node`.
fn first_name(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind().is_name_like())
}

/// Whether implicit member expression `node` is syntactically a class
/// reference: an argument of `.class(…)`, alone, in a list or before
/// `when`.
fn is_class_reference(node: &SyntaxNode) -> bool {
    class_call(node).is_some_and(|(_, name)| name == "class")
}

/// The modifier call an implicit member is the class argument of, with
/// the modifier's name: walking up through lists, `when` (as its class)
/// and the argument.
fn class_call(node: &SyntaxNode) -> Option<(SyntaxNode, String)> {
    let mut prev = node.clone();
    let mut current = node.parent()?;
    loop {
        match current.kind() {
            SyntaxKind::ListExpr | SyntaxKind::Arg => {}
            SyntaxKind::WhenExpr => {
                if current.first_child().as_ref() != Some(&prev) {
                    return None;
                }
            }
            SyntaxKind::ArgList => {
                let call = current.parent()?;
                let callee = call.first_child()?;
                if callee.kind() != SyntaxKind::MemberExpr {
                    return None;
                }
                let name = link_name(&callee)?;
                return Some((callee, name));
            }
            _ => return None,
        }
        prev = current.clone();
        current = current.parent()?;
    }
}

/// Whether implicit member `node` is where a class of the enclosing
/// declaration goes: `.class(…)` or a component's `@Stylable` prop
/// (`.container(.highlighted)`).
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub(super) fn expects_class(
    analysis: &Analysis,
    view: &ProgramView,
    file: FileId,
    node: &SyntaxNode,
) -> Result<bool, Cancelled> {
    let Some((callee, name)) = class_call(node) else {
        return Ok(false);
    };
    if name == "class" {
        return Ok(true);
    }
    let Some((base, _)) = callee.first_child().as_ref().and_then(chain) else {
        return Ok(false);
    };
    let Some(d) = component_of(analysis, view, file, &base)? else {
        return Ok(false);
    };
    Ok(view.decl(d).members.iter().any(|&m| {
        let member = view.member(m);
        member.name == name && member.has_annotation("Stylable")
    }))
}

/// Whether the `.` token `dot` starts a class selector in a style block.
pub(super) fn in_selector(dot: &SyntaxToken) -> bool {
    dot.parent_ancestors()
        .take_while(|n| n.kind() != SyntaxKind::StyleBlock)
        .any(|n| matches!(n.kind(), SyntaxKind::ClassSelector | SyntaxKind::Selector))
}
