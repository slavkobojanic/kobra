//! Semantic types (sema.md §9.1).
//!
//! A [`Ty`] is an immutable value: nominal types carry their declaration and
//! arguments, functions their parameters, effects and result. Arguments are
//! shared through [`Arc`]s, so cloning a type is cheap and types can be
//! compared structurally; type equality is `==`.
//!
//! Two special types keep checking going after an error without cascades:
//! [`Ty::Error`] (`{error}`) unifies with everything and suppresses every
//! diagnostic that depends on it, and an unbound inference variable
//! ([`Ty::Var`]) renders as `{unknown}`.

use std::fmt::Write as _;
use std::sync::Arc;

use crate::program::{DeclId, GenId, Program};

/// What rendering a type needs: the names of declarations and generic
/// parameters. Implemented by the [`Program`] during the semantic pass and
/// by a [`NameTable`] afterwards, so recorded types can be rendered lazily,
/// when a query asks for one.
pub(crate) trait Names {
    /// Appends the rendered name of `decl` (sema.md §2).
    fn write_decl_name(&self, decl: DeclId, out: &mut String);
    /// The name of a generic parameter.
    fn generic_name(&self, generic: GenId) -> &str;
}

impl Names for Program<'_> {
    fn write_decl_name(&self, decl: DeclId, out: &mut String) {
        Program::write_decl_name(self, decl, out);
    }

    fn generic_name(&self, generic: GenId) -> &str {
        self.generics
            .get(generic.0 as usize)
            .map_or("{error}", |g| g.name.as_str())
    }
}

/// Renders types as source text for quick fixes: every declaration by its
/// simple name (`E`, not `corpus.E`), as the user would write it in a file
/// that imports it.
///
/// Diagnostics *messages* use [`Ty::render`] with the program (qualified
/// names); edits use this, so an inserted `throws E` or `<#Item#>`
/// placeholder reads like hand-written code.
pub(crate) struct SourceNames<'a, 'p>(pub(crate) &'a Program<'p>);

impl Names for SourceNames<'_, '_> {
    fn write_decl_name(&self, decl: DeclId, out: &mut String) {
        out.push_str(&self.0.decl(decl).name);
    }

    fn generic_name(&self, generic: GenId) -> &str {
        Names::generic_name(self.0, generic)
    }
}

/// The rendered name of every declaration and generic parameter of one
/// revision's program, owned by its `SemaResult` so that `type_at` can
/// render recorded [`Ty`]s after the program (which borrows the syntax
/// trees) is gone. Built in one pass over the declarations; rendering
/// every recorded type eagerly instead cost about 15% of a check.
#[derive(Debug, Default)]
pub(crate) struct NameTable {
    decls: Vec<Box<str>>,
    generics: Vec<Box<str>>,
}

impl NameTable {
    /// Takes the names of `program`'s declarations and generics.
    pub(crate) fn new(program: &Program<'_>) -> NameTable {
        let mut name = String::new();
        let decls = (0..program.decls.len())
            .map(|i| {
                name.clear();
                program.write_decl_name(DeclId(u32::try_from(i).unwrap_or(u32::MAX)), &mut name);
                Box::from(name.as_str())
            })
            .collect();
        let generics = program
            .generics
            .iter()
            .map(|g| Box::from(g.name.as_str()))
            .collect();
        NameTable { decls, generics }
    }
}

impl Names for NameTable {
    fn write_decl_name(&self, decl: DeclId, out: &mut String) {
        out.push_str(self.decls.get(decl.0 as usize).map_or("{error}", |n| n));
    }

    fn generic_name(&self, generic: GenId) -> &str {
        self.generics
            .get(generic.0 as usize)
            .map_or("{error}", |n| n)
    }
}

/// A function type: `(Int, String) async throws E -> Bool`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FnTy {
    /// The parameter types, in order.
    pub(crate) params: Vec<Ty>,
    /// `true` for `async` functions.
    pub(crate) is_async: bool,
    /// The error type of `throws E`; `None` when the function throws
    /// nothing.
    pub(crate) throws: Option<Ty>,
    /// The result type (`Void` when absent).
    pub(crate) ret: Ty,
}

/// A semantic type.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Ty {
    /// An erroneous expression's type; unifies with everything.
    Error,
    /// The type of expressions that never produce a value (`return`,
    /// `throw`), and the empty error type.
    Never,
    /// A nominal type with its generic arguments.
    Named(DeclId, Arc<[Ty]>),
    /// `T?`.
    Optional(Arc<Ty>),
    /// A function type.
    Func(Arc<FnTy>),
    /// A tuple (only produced by stdlib signatures such as `enumerated()`).
    Tuple(Arc<[Ty]>),
    /// A generic parameter, inside the body that declares it.
    Param(GenId),
    /// An inference variable of the body being checked.
    Var(u32),
    /// A type used as a value (`Light` in `Light.red`).
    Meta(Arc<Ty>),
}

impl Ty {
    /// A nominal type without arguments.
    pub(crate) fn named(decl: DeclId) -> Ty {
        Ty::Named(decl, Arc::from([]))
    }

    /// A nominal type with arguments.
    pub(crate) fn generic(decl: DeclId, args: Vec<Ty>) -> Ty {
        Ty::Named(decl, Arc::from(args))
    }

    /// `T?`; an optional stays one level deep (`T??` is `T?`).
    pub(crate) fn optional(inner: Ty) -> Ty {
        match inner {
            Ty::Optional(_) | Ty::Error => inner,
            other => Ty::Optional(Arc::new(other)),
        }
    }

    /// A function type.
    pub(crate) fn func(params: Vec<Ty>, is_async: bool, throws: Option<Ty>, ret: Ty) -> Ty {
        Ty::Func(Arc::new(FnTy {
            params,
            is_async,
            throws,
            ret,
        }))
    }

    /// Returns `true` for `{error}`.
    pub(crate) fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Returns `true` if `{error}` occurs anywhere in the type.
    pub(crate) fn contains_error(&self) -> bool {
        match self {
            Ty::Error => true,
            Ty::Never | Ty::Param(_) | Ty::Var(_) => false,
            Ty::Named(_, args) | Ty::Tuple(args) => args.iter().any(Ty::contains_error),
            Ty::Optional(inner) | Ty::Meta(inner) => inner.contains_error(),
            Ty::Func(f) => {
                f.params.iter().any(Ty::contains_error)
                    || f.ret.contains_error()
                    || f.throws.as_ref().is_some_and(Ty::contains_error)
            }
        }
    }

    /// Returns the declaration of a nominal type.
    pub(crate) fn decl(&self) -> Option<DeclId> {
        match self {
            Ty::Named(d, _) => Some(*d),
            _ => None,
        }
    }

    /// Returns the generic arguments of a nominal type (empty otherwise).
    pub(crate) fn args(&self) -> &[Ty] {
        match self {
            Ty::Named(_, args) => args,
            _ => &[],
        }
    }

    /// Replaces generic parameters by `subst(gen)` where it answers `Some`.
    pub(crate) fn subst(&self, subst: &dyn Fn(GenId) -> Option<Ty>) -> Ty {
        match self {
            Ty::Param(g) => subst(*g).unwrap_or_else(|| self.clone()),
            Ty::Error | Ty::Never | Ty::Var(_) => self.clone(),
            Ty::Named(d, args) => {
                if args.is_empty() {
                    self.clone()
                } else {
                    Ty::Named(*d, args.iter().map(|a| a.subst(subst)).collect())
                }
            }
            Ty::Tuple(args) => Ty::Tuple(args.iter().map(|a| a.subst(subst)).collect()),
            Ty::Optional(inner) => Ty::optional(inner.subst(subst)),
            Ty::Meta(inner) => Ty::Meta(Arc::new(inner.subst(subst))),
            Ty::Func(f) => Ty::Func(Arc::new(FnTy {
                params: f.params.iter().map(|p| p.subst(subst)).collect(),
                is_async: f.is_async,
                throws: f.throws.as_ref().map(|t| t.subst(subst)),
                ret: f.ret.subst(subst),
            })),
        }
    }

    /// Returns `true` if the type mentions a generic parameter.
    pub(crate) fn has_params(&self) -> bool {
        match self {
            Ty::Param(_) => true,
            Ty::Error | Ty::Never | Ty::Var(_) => false,
            Ty::Named(_, args) | Ty::Tuple(args) => args.iter().any(Ty::has_params),
            Ty::Optional(inner) | Ty::Meta(inner) => inner.has_params(),
            Ty::Func(f) => {
                f.params.iter().any(Ty::has_params)
                    || f.ret.has_params()
                    || f.throws.as_ref().is_some_and(Ty::has_params)
            }
        }
    }

    /// Renders the type as K type syntax (sema.md §2): prelude declarations
    /// by simple name, others fully qualified.
    pub(crate) fn render(&self, program: &dyn Names) -> String {
        let mut out = String::new();
        self.write(program, &mut out);
        out
    }

    fn write(&self, program: &dyn Names, out: &mut String) {
        match self {
            Ty::Error => out.push_str("{error}"),
            Ty::Never => out.push_str("Never"),
            Ty::Var(_) => out.push_str("{unknown}"),
            Ty::Param(g) => out.push_str(program.generic_name(*g)),
            Ty::Named(d, args) => {
                program.write_decl_name(*d, out);
                if !args.is_empty() {
                    out.push('<');
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        a.write(program, out);
                    }
                    out.push('>');
                }
            }
            Ty::Tuple(args) => {
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    a.write(program, out);
                }
                out.push(')');
            }
            Ty::Optional(inner) => {
                if matches!(**inner, Ty::Func(_)) {
                    out.push('(');
                    inner.write(program, out);
                    out.push(')');
                } else {
                    inner.write(program, out);
                }
                out.push('?');
            }
            Ty::Meta(inner) => inner.write(program, out),
            Ty::Func(f) => {
                out.push('(');
                for (i, p) in f.params.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    p.write(program, out);
                }
                out.push(')');
                if f.is_async {
                    out.push_str(" async");
                }
                if let Some(e) = &f.throws {
                    out.push_str(" throws ");
                    e.write(program, out);
                }
                let _ = write!(out, " -> ");
                f.ret.write(program, out);
            }
        }
    }
}
