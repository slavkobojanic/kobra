//! Document symbols (the outline of a file) and workspace symbols (a
//! fuzzy search over every declaration and member of the project).

use k_diagnostics::{FileId, TextRange};
use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclKind, MemberKind, ProgramView};
use k_sema::{Analysis, Cancelled};

use super::render;

/// What kind of symbol an outline entry is, mapped to the protocol's
/// symbol kinds by the server.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SymbolKind {
    /// A struct, entity, config or extension.
    Struct,
    /// A class, component, page, layout, service, repository, theme or app.
    Class,
    /// An enum.
    Enum,
    /// An interface or protocol.
    Interface,
    /// A top-level function or a method.
    Function,
    /// A top-level constant.
    Constant,
    /// A field or computed property.
    Field,
    /// An enum case.
    EnumMember,
    /// An initializer.
    Constructor,
    /// A test suite or migration.
    Module,
    /// A theme token.
    Property,
}

/// One outline entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSymbol {
    /// The declaration's or member's name as written.
    pub name: String,
    /// A short detail (a member's type or signature).
    pub detail: Option<String>,
    /// What kind of declaration or member it is (the outline's icon).
    pub kind: SymbolKind,
    /// The whole item.
    pub range: TextRange,
    /// The name, within `range`.
    pub selection: TextRange,
    /// Members.
    pub children: Vec<DocumentSymbol>,
}

/// One workspace symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSymbol {
    /// The declaration's or member's simple name.
    pub name: String,
    /// What kind of declaration or member it is.
    pub kind: SymbolKind,
    /// The declaring file.
    pub file: FileId,
    /// The name's range.
    pub range: TextRange,
    /// The enclosing declaration's qualified name, for members; the module
    /// for declarations.
    pub container: Option<String>,
}

/// The kind of a declaration.
fn decl_kind(kind: DeclKind) -> SymbolKind {
    match kind {
        DeclKind::Extension => SymbolKind::Struct,
        DeclKind::Named(k) => match k {
            DeclarationKind::Struct | DeclarationKind::Entity | DeclarationKind::Config => {
                SymbolKind::Struct
            }
            DeclarationKind::Enum => SymbolKind::Enum,
            DeclarationKind::Interface | DeclarationKind::Protocol => SymbolKind::Interface,
            DeclarationKind::Function => SymbolKind::Function,
            DeclarationKind::Constant => SymbolKind::Constant,
            DeclarationKind::TestSuite | DeclarationKind::Migration | DeclarationKind::Preview => {
                SymbolKind::Module
            }
            DeclarationKind::Component
            | DeclarationKind::Page
            | DeclarationKind::Layout
            | DeclarationKind::Service
            | DeclarationKind::Repository
            | DeclarationKind::Class
            | DeclarationKind::Theme
            | DeclarationKind::App => SymbolKind::Class,
        },
    }
}

/// The kind of a member.
fn member_kind(kind: MemberKind) -> SymbolKind {
    match kind {
        MemberKind::Field | MemberKind::Computed => SymbolKind::Field,
        MemberKind::Func | MemberKind::Rpc => SymbolKind::Function,
        MemberKind::Init => SymbolKind::Constructor,
        MemberKind::Case | MemberKind::Generated => SymbolKind::EnumMember,
        MemberKind::Token => SymbolKind::Property,
    }
}

/// The outline of `file`: its declarations in source order, each with its
/// members.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn document_symbols(
    analysis: &Analysis,
    file: FileId,
) -> Result<Vec<DocumentSymbol>, Cancelled> {
    let view = analysis.program()?;
    let previews = analysis.previews(file)?;
    let mut out = Vec::new();
    for decl in &view.decls {
        if decl.file != file {
            continue;
        }
        // A preview's declaration name is internal: the outline shows its
        // label.
        let preview = previews.iter().find(|p| p.range == decl.range);
        let name = match decl.kind {
            DeclKind::Named(_) if preview.is_some() => {
                format!("preview {}", preview.map_or("", |p| p.label.as_str()))
            }
            DeclKind::Extension => format!(
                "extend {}",
                decl.extends
                    .as_ref()
                    .map(|t| render::ty(view, t))
                    .unwrap_or_default()
            ),
            DeclKind::Named(_) => decl.name.clone(),
        };
        let selection = if decl.range.contains_range(decl.name_range) && !decl.name_range.is_empty()
        {
            decl.name_range
        } else {
            TextRange::empty(decl.range.start())
        };
        let mut children = Vec::new();
        for &m in &decl.members {
            let member = view.member(m);
            let Some(range) = member.range else { continue };
            let selection = if range.contains_range(member.name_range) {
                member.name_range
            } else {
                TextRange::empty(range.start())
            };
            children.push(DocumentSymbol {
                name: member.name.clone(),
                detail: Some(member_detail(view, m)),
                kind: member_kind(member.kind),
                range,
                selection,
                children: Vec::new(),
            });
        }
        let detail = match (&decl.signature, decl.named_kind()) {
            (Some(sig), _) => Some(render::signature_tail(view, sig)),
            (None, Some(k)) => Some(render::kind_keyword(k).to_owned()),
            _ => None,
        };
        out.push(DocumentSymbol {
            name,
            detail,
            kind: decl_kind(decl.kind),
            range: decl.range,
            selection,
            children,
        });
    }
    out.sort_by_key(|s| s.range.start());
    Ok(out)
}

/// A member's outline detail: a field's type or a function's signature.
fn member_detail(view: &ProgramView, m: k_sema::view::MemberId) -> String {
    let info = view.member(m);
    match (&info.ty, &info.signature) {
        (Some(t), _) => render::ty(view, t),
        (None, Some(sig)) => render::signature_tail(view, sig),
        _ => String::new(),
    }
}

/// The project's declarations and members whose names match `query`
/// fuzzily, best matches first.
///
/// A name matches when it has the query's characters in order, ignoring
/// case; at most `limit` are returned. Stdlib declarations are left out. An empty query
/// matches every declaration, but no member.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn workspace_symbols(
    analysis: &Analysis,
    query: &str,
    limit: usize,
) -> Result<Vec<WorkspaceSymbol>, Cancelled> {
    let view = analysis.program()?;
    let mut scored: Vec<(u32, WorkspaceSymbol)> = Vec::new();
    for (i, decl) in view.decls.iter().enumerate() {
        if decl.stdlib {
            continue;
        }
        let id = k_sema::view::DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        // A preview's name is internal, and its fields are its own.
        let preview = decl.named_kind() == Some(DeclarationKind::Preview);
        if !decl.name.is_empty()
            && !preview
            && let Some(score) = fuzzy(query, &decl.name)
        {
            scored.push((
                score,
                WorkspaceSymbol {
                    name: decl.name.clone(),
                    kind: decl_kind(decl.kind),
                    file: decl.file,
                    range: decl.name_range,
                    container: decl.module.clone(),
                },
            ));
        }
        if query.is_empty() || preview {
            continue;
        }
        for &m in &decl.members {
            let member = view.member(m);
            if member.range.is_none() {
                continue;
            }
            if let Some(score) = fuzzy(query, &member.name) {
                scored.push((
                    score + 1,
                    WorkspaceSymbol {
                        name: member.name.clone(),
                        kind: member_kind(member.kind),
                        file: decl.file,
                        range: member.name_range,
                        container: Some(match decl.kind {
                            DeclKind::Named(_) => view.qualified_name(id),
                            DeclKind::Extension => render::owner_name(view, m),
                        }),
                    },
                ));
            }
        }
    }
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    Ok(scored.into_iter().take(limit).map(|(_, s)| s).collect())
}

/// A fuzzy match score of `name` for `query` (lower is better): `None`
/// unless the query's characters appear in order, ignoring case. Exact,
/// prefix and contiguous matches score best.
#[must_use]
pub fn fuzzy(query: &str, name: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(10);
    }
    let q = query.to_ascii_lowercase();
    let n = name.to_ascii_lowercase();
    if n == q {
        return Some(0);
    }
    if n.starts_with(&q) {
        return Some(1);
    }
    if n.contains(&q) {
        return Some(2);
    }
    let mut chars = n.chars();
    for c in q.chars() {
        chars.find(|&x| x == c)?;
    }
    Some(3)
}
