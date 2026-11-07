//! Go to definition, type definition and implementation; find references;
//! document highlights.

use k_diagnostics::{FileId, Span, TextRange, TextSize};
use k_sema::view::{DeclId, MemberId, ProgramView, Resolved, Type};
use k_sema::{Analysis, Cancelled, Resolution};

use super::{Symbol, member_resolution, symbol_at};

/// Where the symbol at `offset` is declared; empty when nothing there
/// resolves or the symbol has no K source (a built-in element).
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn definition(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Vec<Span>, Cancelled> {
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return Ok(Vec::new());
    };
    Ok(analysis
        .definition(&symbol.resolution)?
        .into_iter()
        .collect())
}

/// Where the type of the symbol at `offset` is declared.
///
/// That is the nominal type of an expression, binding, field, parameter or function result, through
/// one optional level and through the element type of a generic container
/// when the container itself is from the stdlib (`List<Item>` goes to
/// `Item`).
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn type_definition(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Vec<Span>, Cancelled> {
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return Ok(Vec::new());
    };
    let view = analysis.program()?;
    let mut ty = analysis.type_info_at(file, symbol.range)?;
    if ty.is_none() {
        ty = symbol_type(analysis, view, &symbol)?;
    }
    let Some(ty) = ty else {
        return Ok(Vec::new());
    };
    Ok(nominal(view, &ty)
        .map(|d| {
            let info = view.decl(d);
            Span::new(info.file, info.name_range)
        })
        .into_iter()
        .collect())
}

/// The declared type of a symbol: a binding's recorded type, a field's
/// type, a function's result.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn symbol_type(
    analysis: &Analysis,
    view: &ProgramView,
    symbol: &Symbol,
) -> Result<Option<Type>, Cancelled> {
    match &symbol.resolution {
        Resolution::Local { definition, .. } | Resolution::Parameter { definition, .. } => {
            analysis.type_info_at(definition.file, definition.range)
        }
        resolution @ (Resolution::Declaration { .. } | Resolution::Member { .. }) => {
            Ok(match view.resolve(resolution) {
                Some(Resolved::Decl(d)) => {
                    let info = view.decl(d);
                    match (&info.signature, &info.constant_type) {
                        (Some(sig), _) => Some(sig.ret.clone()),
                        (None, Some(t)) => Some(t.clone()),
                        (None, None) => Some(Type::Named(d, Vec::new())),
                    }
                }
                Some(Resolved::Member(m)) => {
                    let info = view.member(m);
                    info.ty
                        .clone()
                        .or_else(|| info.signature.as_ref().map(|s| s.ret.clone()))
                }
                None => None,
            })
        }
        _ => Ok(None),
    }
}

/// The declaration a type names: through optionals and metatypes, and into
/// the first argument of a stdlib generic (`List<Item>`, `Loadable<Item>`).
fn nominal(view: &ProgramView, ty: &Type) -> Option<DeclId> {
    match ty {
        Type::Optional(inner) | Type::Meta(inner) => nominal(view, inner),
        Type::Named(d, args) => {
            let info = view.decls.get(d.0 as usize)?;
            if info.stdlib {
                if let Some(arg) = args.iter().find_map(|a| nominal(view, a)) {
                    if !view.decl(arg).stdlib {
                        return Some(arg);
                    }
                }
            }
            Some(*d)
        }
        Type::Func(f) => nominal(view, &f.ret),
        _ => None,
    }
}

/// The implementations of the symbol at `offset`.
///
/// For an interface or protocol, every declaration implementing it (transitively); for a member
/// of one, the members of those declarations (and of their extensions)
/// with the same name and labels.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn implementation(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Vec<Span>, Cancelled> {
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return Ok(Vec::new());
    };
    let view = analysis.program()?;
    let spans = match view.resolve(&symbol.resolution) {
        Some(Resolved::Decl(d)) => analysis
            .implementations(d)?
            .into_iter()
            .map(|i| {
                let info = view.decl(i);
                Span::new(info.file, info.name_range)
            })
            .collect(),
        Some(Resolved::Member(m)) => implementing_members(analysis, view, m)?
            .into_iter()
            .map(|i| {
                Span::new(
                    view.decl(view.member(i).owner).file,
                    view.member(i).name_range,
                )
            })
            .collect(),
        None => Vec::new(),
    };
    Ok(spans)
}

/// The declaration (or member of one) a cursor is on, by name, so another
/// analysis of the same files can find it: the qualified name of the
/// declaration and, for a member, the member's name and labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The declaration in this analysis.
    pub decl: DeclId,
    /// Its qualified name (`pay.Payments`).
    pub qualified: String,
    /// For a member: its name and labels (`charge`, `[Some("request")]`).
    pub member: Option<(String, Vec<Option<String>>)>,
}

/// The [`Target`] at `offset`, when it is a declaration or a member.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn target_at(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Option<Target>, Cancelled> {
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return Ok(None);
    };
    let view = analysis.program()?;
    Ok(match view.resolve(&symbol.resolution) {
        Some(Resolved::Decl(d)) => Some(Target {
            decl: d,
            qualified: view.qualified_name(d),
            member: None,
        }),
        Some(Resolved::Member(m)) => {
            let owner = view.member(m).owner;
            Some(Target {
                decl: owner,
                qualified: view.qualified_name(owner),
                member: Some((view.member(m).name.clone(), view.labels(m))),
            })
        }
        None => None,
    })
}

/// The implementations of declaration `d`, or of its member named and
/// labelled like `member`: their name spans.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn implementations_of(
    analysis: &Analysis,
    d: DeclId,
    member: Option<&(String, Vec<Option<String>>)>,
) -> Result<Vec<Span>, Cancelled> {
    let view = analysis.program()?;
    let Some((name, labels)) = member else {
        return Ok(analysis
            .implementations(d)?
            .into_iter()
            .map(|i| Span::new(view.decl(i).file, view.decl(i).name_range))
            .collect());
    };
    let Some(&m) = view
        .decl(d)
        .members
        .iter()
        .find(|&&m| view.member(m).name == *name && view.labels(m) == *labels)
    else {
        return Ok(Vec::new());
    };
    Ok(implementing_members(analysis, view, m)?
        .into_iter()
        .map(|i| {
            Span::new(
                view.decl(view.member(i).owner).file,
                view.member(i).name_range,
            )
        })
        .collect())
}

/// The members implementing interface member `m`: in each implementor of
/// its owner, and in their extensions, the members named like `m` with the
/// same labels.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn implementing_members(
    analysis: &Analysis,
    view: &ProgramView,
    m: MemberId,
) -> Result<Vec<MemberId>, Cancelled> {
    let member = view.member(m);
    let labels = view.labels(m);
    let mut out = Vec::new();
    for implementor in analysis.implementations(member.owner)? {
        let mut owners = vec![implementor];
        if let Some(exts) = view.extensions_of.get(&implementor) {
            owners.extend(exts.iter().copied());
        }
        for owner in owners {
            for &candidate in &view.decl(owner).members {
                let info = view.member(candidate);
                if info.name == member.name
                    && info.kind == member.kind
                    && view.labels(candidate) == labels
                {
                    out.push(candidate);
                }
            }
        }
    }
    Ok(out)
}

/// Every reference to the symbol at `offset`, project-wide; with its
/// declaration first when `include_declaration` is set.
///
/// A member of an interface and the members implementing it are one
/// symbol, as for rename ([`member_family`]): the references of an
/// interface method include the methods implementing it (their
/// declarations) and the calls through concrete types, and those of an
/// implementing method include the interface method and its other
/// implementations.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn references(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
    include_declaration: bool,
) -> Result<Vec<Span>, Cancelled> {
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return Ok(Vec::new());
    };
    let own = analysis.definition(&symbol.resolution)?;
    let mut out = Vec::new();
    if include_declaration {
        out.extend(own);
    }
    let mut targets = vec![symbol.resolution.clone()];
    let view = analysis.program()?;
    if let Some(Resolved::Member(m)) = view.resolve(&symbol.resolution) {
        for (_, r) in member_family(analysis, view, m)? {
            if !targets.contains(&r) {
                targets.push(r);
            }
        }
    }
    let mut rest = Vec::new();
    for (i, target) in targets.iter().enumerate() {
        if i > 0 {
            // The family's other members are references by their
            // declarations.
            if let Some(def) = analysis.definition(target)? {
                if !super::is_stdlib(analysis, def.file) {
                    rest.push(def);
                }
            }
        }
        rest.extend(analysis.references(target)?);
    }
    rest.sort();
    for span in rest {
        if !out.contains(&span) && Some(span) != own {
            out.push(span);
        }
    }
    Ok(out)
}

/// How a highlighted occurrence uses its symbol.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// The declaration.
    Write,
    /// A use.
    Read,
}

/// The occurrences of the symbol at `offset` in `file`: its declaration
/// when it is in this file, and every reference in this file.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn highlights(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Vec<(TextRange, Access)>, Cancelled> {
    let Some(symbol) = symbol_at(analysis, file, offset)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    if let Some(def) = analysis.definition(&symbol.resolution)? {
        if def.file == file {
            out.push((def.range, Access::Write));
        }
    }
    for (range, resolution) in analysis.file_references(file)? {
        if *resolution == symbol.resolution && !out.iter().any(|(r, _)| *r == range) {
            out.push((range, Access::Read));
        }
    }
    out.sort_by_key(|(r, _)| r.start());
    Ok(out)
}

/// The resolutions of every implementing member of interface member `m`,
/// for rename.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn implementing_resolutions(
    analysis: &Analysis,
    view: &ProgramView,
    m: MemberId,
) -> Result<Vec<(MemberId, Resolution)>, Cancelled> {
    Ok(implementing_members(analysis, view, m)?
        .into_iter()
        .filter_map(|i| member_resolution(view, i).map(|r| (i, r)))
        .collect())
}

/// The family of interface member or implementing member `m`, which
/// rename and find-references treat as one symbol.
///
/// The family is the interface members `m` implements (through its
/// owner's `implements`, transitively), and every member implementing any
/// of them.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn member_family(
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
