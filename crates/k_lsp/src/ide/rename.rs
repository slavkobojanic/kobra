//! Rename, project-wide (`docs/compiler/lsp.md`, "Rename").
//!
//! A rename replaces the symbol's declaration name and every reference the
//! reverse index holds (code, imports and doc links), as plain text edits:
//! `crates/k_sema/tests/tooling.rs` checks that references are spelled like
//! their definitions, and each edit is checked again here. Beyond the
//! symbol itself a rename also covers:
//!
//! * **interface members**: the members implementing it, and the interface
//!   members an implementing member implements, with their references, so
//!   `implements` stays satisfied;
//! * **pages**: the page's case in `Page` (and `Endpoint`), `.signIn` for
//!   `SignIn`, written in lower camel case;
//! * **files**: a public declaration named like its file renames the file
//!   (`Square.k` to `Box.k`), since a file's public declaration must be named
//!   like it (K1002);
//! * **implicit labels**: a function parameter without a separate label is
//!   also its label, so renaming it keeps the label (`a: Int` becomes
//!   `a b: Int`) and callers stay valid.
//!
//! What can't be renamed: stdlib declarations, built-in elements,
//! attributes, events and CSS properties, generated members (rename their
//! source instead), and anything without a resolution.

use std::collections::BTreeSet;

use k_diagnostics::{FileId, Span, TextRange, TextSize};
use k_sema::stdlib::DeclarationKind;
use k_sema::view::{MemberId, ProgramView, Resolved, Type};
use k_sema::{Analysis, Cancelled, FileOrigin, Resolution};
use k_syntax::SyntaxKind;

use super::navigation::implementing_resolutions;
use super::{is_stdlib, member_resolution, span_text, symbol_at, syntax};

/// The edits of a rename.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenameEdits {
    /// Text replacements, sorted by span, non-overlapping.
    pub edits: Vec<(Span, String)>,
    /// Files to rename: the file and its new file name (`Box.k`).
    pub file_renames: Vec<(FileId, String)>,
}

/// Checks that the symbol at `offset` can be renamed; answers the range of
/// its name under the cursor and the name, or why it can't be renamed.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn prepare(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Result<(TextRange, String), String>, Cancelled> {
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return Ok(Err("there is no symbol to rename here".to_owned()));
    };
    if let Err(reason) = renameable(analysis, &symbol.resolution)? {
        return Ok(Err(reason));
    }
    let name = span_text(analysis, Span::new(file, symbol.range))
        .unwrap_or_default()
        .to_owned();
    Ok(Ok((symbol.range, name)))
}

/// Whether `resolution` can be renamed, and why not.
fn renameable(
    analysis: &Analysis,
    resolution: &Resolution,
) -> Result<Result<(), String>, Cancelled> {
    match resolution {
        Resolution::Element { name } => return Ok(Err(format!("`{name}` is a built-in element"))),
        Resolution::Attribute { name, .. } => {
            return Ok(Err(format!(
                "`{name}` is an attribute of a built-in element"
            )));
        }
        Resolution::Event { name, .. } => {
            return Ok(Err(format!("the event `{name}` can't be renamed")));
        }
        Resolution::CssProperty { name } => return Ok(Err(format!("`{name}` is a CSS property"))),
        _ => {}
    }
    let view = analysis.program()?;
    if let Some(Resolved::Member(m)) = view.resolve(resolution) {
        if view.member(m).range.is_none() {
            return Ok(Err(
                "a generated member can't be renamed; rename the declaration it comes from"
                    .to_owned(),
            ));
        }
    }
    let Some(definition) = analysis.definition(resolution)? else {
        return Ok(Err(
            "the symbol has no declaration in the project".to_owned()
        ));
    };
    if is_stdlib(analysis, definition.file) {
        return Ok(Err(
            "standard library declarations can't be renamed".to_owned()
        ));
    }
    Ok(Ok(()))
}

/// Checks that `name` can name a K symbol: an ASCII identifier that is not
/// a hard keyword or `_`.
pub fn valid_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("`{name}` is not a valid K identifier"));
    }
    if name == "_" {
        return Err("`_` is not a name".to_owned());
    }
    if SyntaxKind::from_keyword(name).is_some() {
        return Err(format!("`{name}` is a keyword"));
    }
    Ok(())
}

/// Renames the symbol at `offset` to `new_name`; the error says why it
/// can't be done.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn rename(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
    new_name: &str,
) -> Result<Result<RenameEdits, String>, Cancelled> {
    if let Err(reason) = valid_name(new_name) {
        return Ok(Err(reason));
    }
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return Ok(Err("there is no symbol to rename here".to_owned()));
    };
    if let Err(reason) = renameable(analysis, &symbol.resolution)? {
        return Ok(Err(reason));
    }
    let view = analysis.program()?;
    let Some(definition) = analysis.definition(&symbol.resolution)? else {
        return Ok(Err(
            "the symbol has no declaration in the project".to_owned()
        ));
    };
    let Some(old_name) = span_text(analysis, definition).map(str::to_owned) else {
        return Ok(Err("the declaration is out of date".to_owned()));
    };
    let mut targets: Vec<Resolution> = vec![symbol.resolution.clone()];
    if let Some(Resolved::Member(m)) = view.resolve(&symbol.resolution) {
        for (_, r) in member_family(analysis, view, m)? {
            if !targets.contains(&r) {
                targets.push(r);
            }
        }
    }
    let mut edits: BTreeSet<(Span, String)> = BTreeSet::new();
    for target in &targets {
        let mut spans = analysis.references(target)?;
        if let Some(def) = analysis.definition(target)? {
            if is_stdlib(analysis, def.file) {
                continue;
            }
            spans.push(def);
        }
        for span in spans {
            if span_text(analysis, span) == Some(old_name.as_str()) {
                edits.insert((span, new_name.to_owned()));
            }
        }
    }
    let mut out = RenameEdits::default();
    if let Some(Resolved::Decl(d)) = view.resolve(&symbol.resolution) {
        let info = view.decl(d);
        if info.named_kind() == Some(DeclarationKind::Page) {
            page_cases(analysis, &old_name, new_name, &mut edits)?;
        }
        if info.is_public {
            if let Some(FileOrigin::Project(path)) = analysis.origin(info.file) {
                let file_name = path.file_name();
                if file_name.strip_suffix(".k") == Some(old_name.as_str()) {
                    out.file_renames.push((info.file, format!("{new_name}.k")));
                }
            }
        }
    }
    if let Resolution::Parameter { definition, .. } = &symbol.resolution {
        if implicit_label(analysis, *definition) {
            edits.remove(&(*definition, new_name.to_owned()));
            edits.insert((*definition, format!("{old_name} {new_name}")));
        }
    }
    out.edits = edits.into_iter().collect();
    Ok(Ok(out))
}

/// The members to rename with interface member or implementing member
/// `m`: the interface members it implements (through its owner's
/// `implements`, transitively), and every member implementing any of
/// them.
fn member_family(
    analysis: &Analysis,
    view: &ProgramView,
    m: MemberId,
) -> Result<Vec<(MemberId, Resolution)>, Cancelled> {
    let member = view.member(m);
    let labels = view.labels(m);
    let mut roots = vec![m];
    // Upwards: the interfaces the owner implements, transitively.
    let mut stack: Vec<Type> = view.decl(member.owner).implements.clone();
    let mut seen = Vec::new();
    while let Some(t) = stack.pop() {
        let Some(iface) = t.decl() else { continue };
        if seen.contains(&iface) {
            continue;
        }
        seen.push(iface);
        let info = view.decl(iface);
        stack.extend(info.implements.iter().cloned());
        for &candidate in &info.members {
            let c = view.member(candidate);
            if c.name == member.name && c.kind == member.kind && view.labels(candidate) == labels {
                roots.push(candidate);
            }
        }
    }
    let mut out: Vec<(MemberId, Resolution)> = Vec::new();
    for root in roots {
        if let Some(r) = member_resolution(view, root) {
            if !out.iter().any(|(id, _)| *id == root) {
                out.push((root, r));
            }
        }
        for (id, r) in implementing_resolutions(analysis, view, root)? {
            if !out.iter().any(|(existing, _)| *existing == id) {
                out.push((id, r));
            }
        }
    }
    Ok(out)
}

/// Adds the edits renaming page `old`'s cases (`.signIn`) in `Page` and
/// `Endpoint`.
fn page_cases(
    analysis: &Analysis,
    old: &str,
    new: &str,
    edits: &mut BTreeSet<(Span, String)>,
) -> Result<(), Cancelled> {
    let (old_case, new_case) = (lower_first(old), lower_first(new));
    for owner in ["Page", "Endpoint"] {
        let case = Resolution::Member {
            owner: owner.to_owned(),
            name: old_case.clone(),
        };
        for span in analysis.references(&case)? {
            if span_text(analysis, span) == Some(old_case.as_str()) {
                edits.insert((span, new_case.clone()));
            }
        }
    }
    Ok(())
}

/// `SignIn` → `signIn`.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Whether the function parameter declared at `definition` has no separate
/// label (`a: Int`), so its name is also its label.
fn implicit_label(analysis: &Analysis, definition: Span) -> bool {
    let Some(root) = syntax(analysis, definition.file) else {
        return false;
    };
    let Some(param) = root
        .covering_element(definition.range)
        .ancestors()
        .find(|n| n.kind() == SyntaxKind::Param)
    else {
        return false;
    };
    let mut names = 0;
    for el in param.children_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Colon => break,
            rowan::NodeOrToken::Token(t)
                if super::is_name(t.kind()) || t.kind() == SyntaxKind::Underscore =>
            {
                names += 1;
            }
            rowan::NodeOrToken::Node(_) => break,
            _ => {}
        }
    }
    names == 1
}
