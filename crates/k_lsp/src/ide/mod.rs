//! The language features, as functions of an [`Analysis`] snapshot, a file
//! and a byte offset (`docs/compiler/lsp.md`, "Features").
//!
//! Nothing here knows about the protocol: results are spans, byte ranges
//! and strings, and [`crate::server`] converts them to LSP types with the
//! documents' [`LineIndex`](crate::line_index::LineIndex)es. Every function
//! returns `Err(Cancelled)` when the snapshot went stale while it ran, and
//! an empty answer (never a panic) for offsets that are out of range, files
//! that don't exist, half-typed code or anything else it can't make sense
//! of.
//!
//! The central helper is [`symbol_at`]: the symbol under a cursor, as the
//! [`Resolution`] the reverse index is keyed by. Navigation, references,
//! rename and hover all start from it.

pub mod completion;
pub mod fixes;
pub mod hover;
pub mod inlay;
pub mod navigation;
pub mod reactive;
pub mod rename;
pub mod render;
pub mod signature_help;
pub mod symbols;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use k_diagnostics::{FileId, Span, TextRange, TextSize};
use k_sema::view::{DeclId, DeclKind, MemberId, MemberKind, ProgramView, Resolved};
use k_sema::{Analysis, Cancelled, FileOrigin, Resolution};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

/// The symbol under a cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    /// What it is, in the form the reverse index is keyed by.
    pub resolution: Resolution,
    /// The range of the name under the cursor (a reference or the
    /// declaration's own name).
    pub range: TextRange,
    /// `true` when the cursor is on the symbol's declaration rather than a
    /// reference to it.
    pub is_definition: bool,
}

/// Returns the symbol at `offset` in `file`: the innermost resolved
/// reference whose range contains the offset (a cursor just after a name
/// counts), else the declaration, member or binding whose name is there.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn symbol_at(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Option<Symbol>, Cancelled> {
    let refs = analysis.file_references(file)?;
    if let Some((range, resolution)) = innermost(&refs, offset) {
        return Ok(Some(Symbol {
            resolution: (*resolution).clone(),
            range,
            is_definition: false,
        }));
    }
    let view = analysis.program()?;
    if let Some(symbol) = declaration_at(view, file, offset) {
        return Ok(Some(symbol));
    }
    Ok(binding_at(analysis, &refs, file, offset))
}

/// The innermost reference containing `offset`, preferring one that starts
/// at or before it and ends after it over one ending exactly at it.
fn innermost(
    refs: &[(TextRange, Arc<Resolution>)],
    offset: TextSize,
) -> Option<(TextRange, Arc<Resolution>)> {
    // References are sorted by start; those starting after the offset are
    // irrelevant.
    let end = refs.partition_point(|(r, _)| r.start() <= offset);
    refs[..end]
        .iter()
        .filter(|(r, _)| r.contains_inclusive(offset))
        .min_by_key(|(r, _)| (r.end() == offset, r.len()))
        .map(|(r, res)| (*r, Arc::clone(res)))
}

/// The declaration or member whose name token contains `offset`.
fn declaration_at(view: &ProgramView, file: FileId, offset: TextSize) -> Option<Symbol> {
    for (i, decl) in view.decls.iter().enumerate() {
        if decl.file != file || decl.name.is_empty() || !decl.range.contains_inclusive(offset) {
            continue;
        }
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        for &m in &decl.members {
            let member = view.member(m);
            if member.range.is_some() && member.name_range.contains_inclusive(offset) {
                return member_resolution(view, m).map(|resolution| Symbol {
                    resolution,
                    range: member.name_range,
                    is_definition: true,
                });
            }
        }
        if decl.name_range.contains_inclusive(offset) {
            return Some(Symbol {
                resolution: decl_resolution(view, id)?,
                range: decl.name_range,
                is_definition: true,
            });
        }
    }
    // Members of extensions (whose own name is empty).
    for decl in view
        .decls
        .iter()
        .filter(|d| d.file == file && d.kind == DeclKind::Extension)
    {
        for &m in &decl.members {
            let member = view.member(m);
            if member.range.is_some() && member.name_range.contains_inclusive(offset) {
                return member_resolution(view, m).map(|resolution| Symbol {
                    resolution,
                    range: member.name_range,
                    is_definition: true,
                });
            }
        }
    }
    None
}

/// The local or parameter whose binding name is at `offset`: found among
/// the file's references when it is used, otherwise from the syntax.
fn binding_at(
    analysis: &Analysis,
    refs: &[(TextRange, Arc<Resolution>)],
    file: FileId,
    offset: TextSize,
) -> Option<Symbol> {
    for (_, resolution) in refs {
        if let Resolution::Local { definition, .. } | Resolution::Parameter { definition, .. } =
            &**resolution
        {
            if definition.file == file && definition.range.contains_inclusive(offset) {
                return Some(Symbol {
                    resolution: (**resolution).clone(),
                    range: definition.range,
                    is_definition: true,
                });
            }
        }
    }
    let root = syntax(analysis, file)?;
    let token = name_token_at(&root, offset)?;
    let parent = token.parent()?;
    let span = Span::new(file, token.text_range());
    let name = token.text().to_owned();
    let resolution = match parent.kind() {
        SyntaxKind::LetStmt
        | SyntaxKind::IdentPattern
        | SyntaxKind::BindingPattern
        | SyntaxKind::LambdaParam
        | SyntaxKind::LetCondition
        | SyntaxKind::CatchClause => Resolution::Local {
            name,
            definition: span,
        },
        SyntaxKind::Param if is_internal_param_name(&parent, &token) => Resolution::Parameter {
            name,
            definition: span,
        },
        _ => return None,
    };
    Some(Symbol {
        resolution,
        range: token.text_range(),
        is_definition: true,
    })
}

/// Whether `token` is the internal name of parameter `param`: the last
/// identifier before its `:`.
fn is_internal_param_name(param: &SyntaxNode, token: &SyntaxToken) -> bool {
    let mut last = None;
    for el in param.children_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Colon => break,
            rowan::NodeOrToken::Token(t)
                if t.kind().is_name_like() || t.kind() == SyntaxKind::Ident =>
            {
                last = Some(t);
            }
            rowan::NodeOrToken::Node(_) => break,
            _ => {}
        }
    }
    last.is_some_and(|t| t == *token)
}

/// The syntax tree of project file `file`, if it is a K source.
#[must_use]
pub fn syntax(analysis: &Analysis, file: FileId) -> Option<SyntaxNode> {
    analysis
        .parse(file)
        .ok()
        .flatten()
        .map(k_syntax::Parse::syntax)
}

/// The identifier-like token touching `offset`, preferring the one to its
/// right.
#[must_use]
pub fn name_token_at(root: &SyntaxNode, offset: TextSize) -> Option<SyntaxToken> {
    if !root.text_range().contains_inclusive(offset) {
        return None;
    }
    let tokens: Vec<SyntaxToken> = root.token_at_offset(offset).collect();
    tokens
        .iter()
        .rev()
        .find(|t| is_name(t.kind()))
        .or_else(|| tokens.first())
        .cloned()
        .filter(|t| is_name(t.kind()))
}

/// Whether tokens of `kind` spell a name: identifiers, contextual and hard
/// keywords (names after `.` and labels may be keywords).
#[must_use]
pub fn is_name(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::Ident || kind.is_contextual_keyword() || kind.is_keyword()
}

/// The resolution of declaration `id`, as references to it record it
/// (`Declaration { name }` with its display name). `None` for extensions
/// and for a declaration another one shadows in the view's display table.
#[must_use]
pub fn decl_resolution(view: &ProgramView, id: DeclId) -> Option<Resolution> {
    let decl = view.decls.get(id.0 as usize)?;
    if decl.kind == DeclKind::Extension || decl.name.is_empty() {
        return None;
    }
    let resolution = Resolution::Declaration {
        name: decl.display_name.clone(),
    };
    (view.resolve(&resolution) == Some(Resolved::Decl(id))).then_some(resolution)
}

/// The resolution of member `id`, as references to it record it: the
/// owner is the extended type for a member of an extension, and functions
/// carry their labels (`add(product:)`). Theme tokens are
/// [`Resolution::Token`]s of their category type.
#[must_use]
pub fn member_resolution(view: &ProgramView, id: MemberId) -> Option<Resolution> {
    let member = view.members.get(id.0 as usize)?;
    let owner = view.decl(member.owner);
    if member.kind == MemberKind::Token {
        let category = member.ty.as_ref()?.decl()?;
        return Some(Resolution::Token {
            category: view.decl(category).display_name.clone(),
            name: member.name.clone(),
        });
    }
    let owner_name = match owner.kind {
        DeclKind::Extension => {
            let target = owner.extends.as_ref()?.decl()?;
            view.decl(target).display_name.clone()
        }
        DeclKind::Named(_) => owner.display_name.clone(),
    };
    let mut name = member.name.clone();
    if matches!(
        member.kind,
        MemberKind::Func | MemberKind::Init | MemberKind::Rpc
    ) {
        name.push('(');
        for label in view.labels(id) {
            name.push_str(label.as_deref().unwrap_or("_"));
            name.push(':');
        }
        name.push(')');
    }
    let resolution = Resolution::Member {
        owner: owner_name,
        name,
    };
    (view.resolve(&resolution).is_some()).then_some(resolution)
}

/// Whether `file` is a stdlib file (whose declarations can't be renamed).
#[must_use]
pub fn is_stdlib(analysis: &Analysis, file: FileId) -> bool {
    matches!(analysis.origin(file), Some(FileOrigin::Stdlib(_)))
}

/// The text of `span`, or `None` when it is out of range.
#[must_use]
pub fn span_text(analysis: &Analysis, span: Span) -> Option<&str> {
    analysis
        .text(span.file)?
        .get(usize::from(span.range.start())..usize::from(span.range.end()))
}

/// The declaration whose syntax contains `offset` in `file`, innermost
/// first is irrelevant: declarations don't nest.
#[must_use]
pub fn decl_containing(view: &ProgramView, file: FileId, offset: TextSize) -> Option<DeclId> {
    view.decls
        .iter()
        .position(|d| d.file == file && d.range.contains_inclusive(offset))
        .map(|i| DeclId(u32::try_from(i).unwrap_or(u32::MAX)))
}
