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

/// Where the type of the symbol at `offset` is declared: the nominal type
/// of an expression, binding, field, parameter or function result, through
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

/// The implementations of the symbol at `offset`: for an interface or
/// protocol, every declaration implementing it (transitively); for a member
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

/// Every reference to the symbol at `offset`, project-wide, sorted; with
/// its declaration first when `include_declaration` is set.
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
    let mut out = Vec::new();
    if include_declaration {
        if let Some(def) = analysis.definition(&symbol.resolution)? {
            out.push(def);
        }
    }
    for span in analysis.references(&symbol.resolution)? {
        if !out.contains(&span) {
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
