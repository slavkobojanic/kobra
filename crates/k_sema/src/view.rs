//! The read-only view of a checked program that later stages (code
//! generation) consume: every declaration, member and generic parameter
//! with its resolved signature, plus conversion of recorded types into the
//! public [`Type`] form (`docs/compiler/sema.md` §2, "Program view").
//!
//! The view is built once per revision at the end of the semantic pass,
//! while the checking context is still alive, just after the rest of the
//! result is handed over (`ViewSlot`), and owned by the revision's
//! result. It holds no syntax: declarations and members point at their
//! syntax by file and range, and a consumer finds the nodes in the parses
//! of [`Analysis::parse`](crate::Analysis::parse).
//!
//! Ids ([`DeclId`], [`MemberId`], [`GenId`]) index the tables of one view
//! and are meaningless across revisions.

use std::collections::HashMap;

use k_diagnostics::{FileId, TextRange};

use crate::cx::{Cx, SigKey};
use crate::lookup::{Resolution, Side};
use crate::program::{self, DeclKind as RawDeclKind, GenOwner};
use crate::stdlib::DeclarationKind;
use crate::ty::Ty;

/// The id of a top-level declaration or extension in a [`ProgramView`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct DeclId(pub u32);

/// The id of a member in a [`ProgramView`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct MemberId(pub u32);

/// The id of a generic parameter in a [`ProgramView`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct GenId(pub u32);

/// A semantic type in public form.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    /// The type of an erroneous or not fully inferred expression.
    Error,
    /// The type of expressions that never produce a value.
    Never,
    /// A nominal type with its generic arguments.
    Named(DeclId, Vec<Type>),
    /// `T?`.
    Optional(Box<Type>),
    /// A function type.
    Func(Box<FuncType>),
    /// A tuple.
    Tuple(Vec<Type>),
    /// A generic parameter.
    Param(GenId),
    /// A type used as a value (`Light` in `Light.red`).
    Meta(Box<Type>),
}

/// A function type: `(Int) async throws E -> Bool`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FuncType {
    /// The parameter types.
    pub params: Vec<Type>,
    /// `true` for `async`.
    pub is_async: bool,
    /// The error type, `None` when nothing is thrown.
    pub throws: Option<Type>,
    /// The result type.
    pub ret: Type,
}

impl Type {
    /// The declaration of a nominal type (through one optional level).
    #[must_use]
    pub fn decl(&self) -> Option<DeclId> {
        match self {
            Type::Named(d, _) => Some(*d),
            _ => None,
        }
    }

    /// The generic arguments of a nominal type.
    #[must_use]
    pub fn args(&self) -> &[Type] {
        match self {
            Type::Named(_, a) => a,
            _ => &[],
        }
    }

    /// The type without its optional level.
    #[must_use]
    pub fn unwrap_optional(&self) -> &Type {
        match self {
            Type::Optional(t) => t,
            t => t,
        }
    }

    /// Converts a checker type to its public form; an inference variable
    /// (only unbound ones survive resolution) becomes [`Type::Error`].
    pub(crate) fn from_ty(ty: &Ty) -> Type {
        match ty {
            Ty::Error | Ty::Var(_) => Type::Error,
            Ty::Never => Type::Never,
            Ty::Named(d, args) => {
                Type::Named(DeclId(d.0), args.iter().map(Type::from_ty).collect())
            }
            Ty::Optional(t) => Type::Optional(Box::new(Type::from_ty(t))),
            Ty::Func(f) => Type::Func(Box::new(FuncType {
                params: f.params.iter().map(Type::from_ty).collect(),
                is_async: f.is_async,
                throws: f.throws.as_ref().map(Type::from_ty),
                ret: Type::from_ty(&f.ret),
            })),
            Ty::Tuple(items) => Type::Tuple(items.iter().map(Type::from_ty).collect()),
            Ty::Param(g) => Type::Param(GenId(g.0)),
            Ty::Meta(t) => Type::Meta(Box::new(Type::from_ty(t))),
        }
    }
}

/// What kind of declaration a [`DeclInfo`] is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DeclKind {
    /// A named top-level declaration.
    Named(DeclarationKind),
    /// `extend T { … }`.
    Extension,
}

/// What kind of member a [`MemberInfo`] is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MemberKind {
    /// A stored field.
    Field,
    /// A computed property, or a field without storage of a `@Native` type.
    Computed,
    /// A function.
    Func,
    /// An initializer.
    Init,
    /// An enum case.
    Case,
    /// An rpc.
    Rpc,
    /// A theme token.
    Token,
    /// A generated member: a page case, a theme variant, `copy(…)`.
    Generated,
}

/// A resolved function, initializer, rpc or enum-case signature.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Signature {
    /// The function's own generic parameters.
    pub generics: Vec<GenId>,
    /// The parameters, in declaration order.
    pub params: Vec<Param>,
    /// `true` for `async`.
    pub is_async: bool,
    /// The declared error type.
    pub throws: Option<Type>,
    /// The result type.
    pub ret: Type,
}

/// One parameter of a [`Signature`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Param {
    /// The external label; `None` for `_`.
    pub label: Option<String>,
    /// The internal name.
    pub name: String,
    /// The type.
    pub ty: Type,
    /// `true` when the parameter has a default value.
    pub has_default: bool,
    /// The internal name's range.
    pub name_range: TextRange,
}

/// A top-level declaration or extension.
#[derive(Clone, Debug)]
pub struct DeclInfo {
    /// Its kind.
    pub kind: DeclKind,
    /// The simple name (empty for extensions).
    pub name: String,
    /// The module's dotted path; `None` for `App.k` and stray files.
    pub module: Option<String>,
    /// The name as diagnostics render it: prelude declarations by simple
    /// name, others fully qualified.
    pub display_name: String,
    /// The declaring file.
    pub file: FileId,
    /// `true` for stdlib declarations.
    pub stdlib: bool,
    /// `true` when the declaring file is a `*.test.k` file.
    pub is_test: bool,
    /// `true` when the declaring module is a test-only module (a directory
    /// named `testing`, 16-modules.md): only test code can use it, so
    /// production bundles and bindings leave it out.
    pub in_test_module: bool,
    /// `true` for `public` declarations.
    pub is_public: bool,
    /// The item's syntax range.
    pub range: TextRange,
    /// The name token's range.
    pub name_range: TextRange,
    /// The generic parameters.
    pub generics: Vec<GenId>,
    /// The members, in source order.
    pub members: Vec<MemberId>,
    /// The annotation names (`Native`, `ServerOnly`).
    pub annotations: Vec<String>,
    /// The resolved `implements` list.
    pub implements: Vec<Type>,
    /// The extended type of an extension.
    pub extends: Option<Type>,
    /// The signature of a top-level function, or a preview's parameters.
    pub signature: Option<Signature>,
    /// The type of a top-level constant.
    pub constant_type: Option<Type>,
    /// The side of the server/client boundary.
    pub side: Option<Side>,
}

impl DeclInfo {
    /// Returns `true` if the declaration carries annotation `name`.
    #[must_use]
    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a == name)
    }

    /// The named declaration kind, `None` for extensions.
    #[must_use]
    pub fn named_kind(&self) -> Option<DeclarationKind> {
        match self.kind {
            DeclKind::Named(k) => Some(k),
            DeclKind::Extension => None,
        }
    }
}

/// A member of a declaration or extension.
#[derive(Clone, Debug)]
pub struct MemberInfo {
    /// The declaring declaration or extension.
    pub owner: DeclId,
    /// The name.
    pub name: String,
    /// The kind.
    pub kind: MemberKind,
    /// `true` for static members and enum cases.
    pub is_static: bool,
    /// `true` for `var` fields.
    pub is_var: bool,
    /// `true` for fields with a default.
    pub has_default: bool,
    /// `true` for `private` members.
    pub is_private: bool,
    /// The member's syntax range (`None` for generated members).
    pub range: Option<TextRange>,
    /// The name token's range.
    pub name_range: TextRange,
    /// The member's own generic parameters.
    pub generics: Vec<GenId>,
    /// The annotation names.
    pub annotations: Vec<String>,
    /// A theme token's category word (`color`).
    pub category: Option<String>,
    /// The type of a field or computed property, or the category type of a
    /// theme token (`Spacing` for `space md`).
    pub ty: Option<Type>,
    /// The signature of a function, initializer, rpc or payload case.
    pub signature: Option<Signature>,
}

impl MemberInfo {
    /// Returns `true` if the member carries annotation `name`.
    #[must_use]
    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a == name)
    }
}

/// A generic parameter.
#[derive(Clone, Debug)]
pub struct GenericInfo {
    /// The name.
    pub name: String,
    /// The constraints from `where` clauses.
    pub constraints: Vec<Type>,
}

/// The checked program's declarations, members and generics.
#[derive(Debug, Default)]
pub struct ProgramView {
    /// Every declaration and extension; [`DeclId`] indexes it.
    pub decls: Vec<DeclInfo>,
    /// Every member; [`MemberId`] indexes it.
    pub members: Vec<MemberInfo>,
    /// Every generic parameter; [`GenId`] indexes it.
    pub generics: Vec<GenericInfo>,
    /// Extensions by the declaration they extend.
    pub extensions_of: HashMap<DeclId, Vec<DeclId>>,
    /// Named declarations by qualified name (`k.core.Int`; bare for
    /// `App.k`), the first of a name winning, for [`ProgramView::lookup`].
    qualified: HashMap<String, DeclId>,
    /// Declarations by the name a resolution displays (bare for the
    /// prelude, qualified otherwise), for [`ProgramView::lookup_display`].
    display: HashMap<String, DeclId>,
}

impl ProgramView {
    /// The declaration `id`.
    ///
    /// ```
    /// use k_sema::{AnalysisHost, Options, ProjectPath};
    ///
    /// let host = AnalysisHost::with_files(
    ///     Options::new(),
    ///     [(
    ///         ProjectPath::new("src/geo/Point.k").unwrap(),
    ///         "/// A point.\npublic struct Point {\n    /// The x.\n    let x: Int\n}\n".to_owned(),
    ///     )],
    /// );
    /// let analysis = host.analysis();
    /// let program = analysis.program().unwrap();
    /// let point = program.lookup("geo.Point").unwrap();
    /// let decl = program.decl(point);
    /// assert_eq!(decl.name, "Point");
    /// let x = program.member(decl.members[0]);
    /// assert_eq!(x.name, "x");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `id` is not an id of this view (one from another revision).
    #[must_use]
    pub fn decl(&self, id: DeclId) -> &DeclInfo {
        &self.decls[id.0 as usize]
    }

    /// The member `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` is not an id of this view.
    #[must_use]
    pub fn member(&self, id: MemberId) -> &MemberInfo {
        &self.members[id.0 as usize]
    }

    /// The declaration with the fully qualified name `name`
    /// (`k.core.List`, `corpus.Point`).
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<DeclId> {
        self.qualified.get(name).copied()
    }

    /// The declaration rendered as `name` in a [`Resolution`] or type
    /// display (`Int`, `shop.cart.Cart`).
    #[must_use]
    pub fn lookup_display(&self, name: &str) -> Option<DeclId> {
        self.display.get(name).copied()
    }

    /// The fully qualified name of a declaration (`k.core.List`); the
    /// simple name when it has no module.
    #[must_use]
    pub fn qualified_name(&self, id: DeclId) -> String {
        let d = self.decl(id);
        match &d.module {
            Some(m) => format!("{m}.{}", d.name),
            None => d.name.clone(),
        }
    }

    /// The labels of a member's signature, Swift-style (`add(product:)`
    /// gives `["product"]`, `_` as `None`).
    #[must_use]
    pub fn labels(&self, id: MemberId) -> Vec<Option<String>> {
        self.member(id)
            .signature
            .as_ref()
            .map(|s| s.params.iter().map(|p| p.label.clone()).collect())
            .unwrap_or_default()
    }

    /// Maps a [`Resolution::Declaration`] or [`Resolution::Member`] back to
    /// its id. Members of extensions are found through the extended type.
    #[must_use]
    pub fn resolve(&self, resolution: &Resolution) -> Option<Resolved> {
        match resolution {
            Resolution::Declaration { name } => self.lookup_display(name).map(Resolved::Decl),
            Resolution::Member { owner, name } => {
                let display = owner;
                let owner = self.lookup_display(owner)?;
                let (base, labels) = match name.find('(') {
                    Some(i) => (&name[..i], Some(&name[i..])),
                    None => (name.as_str(), None),
                };
                let mut owners = vec![owner];
                if let Some(exts) = self.extensions_of.get(&owner) {
                    owners.extend(exts.iter().copied());
                }
                let mut fallback = None;
                let mut other_kind = None;
                for o in owners {
                    for &m in &self.decl(o).members {
                        let info = self.member(m);
                        if info.name != base {
                            continue;
                        }
                        // A function's resolution has its labels, a
                        // property's none: a `static let label` and a
                        // `func label()` share the name, not the resolution.
                        let callable = matches!(
                            info.kind,
                            MemberKind::Func | MemberKind::Init | MemberKind::Rpc
                        ) || (info.kind == MemberKind::Generated
                            && info.name == "copy");
                        if callable != labels.is_some() {
                            other_kind.get_or_insert(m);
                            continue;
                        }
                        match labels {
                            None => return Some(Resolved::Member(m)),
                            Some(l) => {
                                if self.label_text(m) == l {
                                    return Some(Resolved::Member(m));
                                }
                                fallback.get_or_insert(m);
                            }
                        }
                    }
                }
                // A test group may share its qualified name with the
                // declaration it tests (`test CartService { }` next to
                // `service CartService`); its members are found here: one
                // whose labels match exactly wins over the declaration's
                // member of the same name with other labels.
                for (i, d) in self.decls.iter().enumerate() {
                    if d.named_kind() != Some(DeclarationKind::TestSuite) {
                        continue;
                    }
                    let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
                    if self.qualified_name(id) != *display {
                        continue;
                    }
                    for &m in &d.members {
                        if self.member(m).name != base {
                            continue;
                        }
                        let exact = labels.is_none_or(|l| self.label_text(m) == l);
                        if exact || fallback.is_none() {
                            return Some(Resolved::Member(m));
                        }
                    }
                }
                fallback.or(other_kind).map(Resolved::Member)
            }
            _ => None,
        }
    }

    /// Renders the argument labels of `m` as a doc link writes them:
    /// `(label:_:)`, with `_` for an unlabelled parameter.
    fn label_text(&self, m: MemberId) -> String {
        let mut s = String::from("(");
        for l in self.labels(m) {
            s.push_str(l.as_deref().unwrap_or("_"));
            s.push(':');
        }
        s.push(')');
        s
    }
}

/// What a resolution refers to, by id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Resolved {
    /// A top-level declaration.
    Decl(DeclId),
    /// A member.
    Member(MemberId),
}

/// A revision's [`ProgramView`], filled after the rest of its result.
///
/// Only code generation and the editor queries read the view; the
/// diagnostics don't. So the semantic pass hands its result over first and
/// builds the view afterwards, on its own thread, while the caller reports
/// the diagnostics (about a tenth of a warm re-check at 100k lines,
/// `docs/compiler/performance.md` §10.4). A reader that comes before the
/// view is filled waits for it ([`ViewSlot::get`]).
#[derive(Debug)]
pub(crate) struct ViewSlot {
    /// The view, once built.
    view: std::sync::OnceLock<ProgramView>,
    /// Guards the wait for [`ViewSlot::view`].
    lock: std::sync::Mutex<()>,
    /// Signalled when [`ViewSlot::view`] is filled.
    ready: std::sync::Condvar,
}

impl Default for ViewSlot {
    /// A slot holding an empty view (the result of a pass that never ran).
    fn default() -> ViewSlot {
        let slot = ViewSlot::pending();
        slot.fill(ProgramView::default());
        slot
    }
}

impl ViewSlot {
    /// An empty slot, to be filled with [`ViewSlot::fill`].
    pub(crate) fn pending() -> ViewSlot {
        ViewSlot {
            view: std::sync::OnceLock::new(),
            lock: std::sync::Mutex::new(()),
            ready: std::sync::Condvar::new(),
        }
    }

    /// Fills the slot and wakes every reader waiting for it; a second fill
    /// is ignored.
    pub(crate) fn fill(&self, view: ProgramView) {
        let _ = self.view.set(view);
        // Taking the lock orders the fill before the wake-up: a reader that
        // saw the slot empty holds the lock until it waits.
        drop(
            self.lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        self.ready.notify_all();
    }

    /// The view, waiting until it is filled.
    pub(crate) fn get(&self) -> &ProgramView {
        if let Some(view) = self.view.get() {
            return view;
        }
        let mut guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(view) = self.view.get() {
                return view;
            }
            guard = self
                .ready
                .wait(guard)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}

/// Fills a [`ViewSlot`] with an empty view if it is dropped before the
/// real one is built (the pass panicked), so no reader waits forever.
pub(crate) struct FillOnDrop<'s>(pub(crate) &'s ViewSlot);

impl Drop for FillOnDrop<'_> {
    fn drop(&mut self) {
        if self.0.view.get().is_none() {
            self.0.fill(ProgramView::default());
        }
    }
}

/// Converts a checker signature to its public [`Signature`] form.
fn sig(s: &crate::cx::Sig) -> Signature {
    Signature {
        generics: s.generics.iter().map(|g| GenId(g.0)).collect(),
        params: s
            .params
            .iter()
            .map(|p| Param {
                label: p.label.clone(),
                name: p.name.clone(),
                ty: Type::from_ty(&p.ty),
                has_default: p.has_default,
                name_range: p.name_range,
            })
            .collect(),
        is_async: s.is_async,
        throws: s.throws.as_ref().map(Type::from_ty),
        ret: Type::from_ty(&s.ret),
    }
}

/// Builds the view of `cx`'s program. Diagnostics reported while resolving
/// signatures that no check needed are discarded by the caller.
pub(crate) fn build(cx: &Cx<'_, '_>, sides: &HashMap<(FileId, TextRange), Side>) -> ProgramView {
    let p = cx.p;
    let mut view = ProgramView::default();
    for g in &p.generics {
        let _ = g.owner;
        view.generics.push(GenericInfo {
            name: g.name.clone(),
            constraints: Vec::new(),
        });
    }
    for (i, g) in p.generics.iter().enumerate() {
        let id = program::GenId(u32::try_from(i).unwrap_or(u32::MAX));
        let skip = match g.owner {
            GenOwner::Decl(d) => skip_decl(p, d),
            GenOwner::Member(m) => skip_decl(p, p.member(m).owner),
        };
        if !skip {
            view.generics[i].constraints = cx.constraints(id).iter().map(Type::from_ty).collect();
        }
    }
    let mut suites = Vec::new();
    for (i, d) in p.decls.iter().enumerate() {
        let id = program::DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let file = p.file(d.file);
        let module = file.module.map(|m| p.modules[m.0 as usize].path.clone());
        let skip = skip_decl(p, id);
        let kind = match d.kind {
            RawDeclKind::Named(k) => DeclKind::Named(k),
            RawDeclKind::Extension => DeclKind::Extension,
        };
        let (signature, constant_type) = match d.named_kind() {
            Some(DeclarationKind::Function | DeclarationKind::Preview) if !skip => {
                (Some(sig(&cx.sig(SigKey::Decl(id)))), None)
            }
            Some(DeclarationKind::Constant) if !skip => {
                (None, Some(Type::from_ty(&cx.constant_ty(id))))
            }
            _ => (None, None),
        };
        let implements = if skip || d.kind == RawDeclKind::Extension {
            Vec::new()
        } else {
            cx.implements_list(id)
                .iter()
                .map(|(t, _)| Type::from_ty(t))
                .collect()
        };
        let extends = (d.kind == RawDeclKind::Extension).then(|| Type::from_ty(&cx.ext_target(id)));
        if let Some(Type::Named(target, _)) = &extends {
            view.extensions_of
                .entry(*target)
                .or_default()
                .push(DeclId(id.0));
        }
        let info = DeclInfo {
            kind,
            name: d.name.clone(),
            module: module.clone(),
            display_name: p.decl_name(id),
            file: file.id,
            stdlib: file.stdlib,
            is_test: file.is_test,
            in_test_module: file
                .module
                .is_some_and(|m| p.modules[m.0 as usize].test_only),
            // An `@Internal` stdlib declaration is public to the stdlib only.
            is_public: d.visibility == k_syntax::ast::Visibility::Public
                && !(file.stdlib && d.has_annotation("Internal")),
            range: d.node.text_range(),
            name_range: d.name_range,
            generics: d.generics.iter().map(|g| GenId(g.0)).collect(),
            members: d.members.iter().map(|m| MemberId(m.0)).collect(),
            annotations: d.annotations.clone(),
            implements,
            extends,
            signature,
            constant_type,
            side: sides.get(&(file.id, d.name_range)).copied(),
        };
        if d.kind != RawDeclKind::Extension && !d.name.is_empty() {
            let q = match &module {
                Some(m) => format!("{m}.{}", d.name),
                None => d.name.clone(),
            };
            // A test group shares its name with what it tests (`test
            // Panel { }` beside `component Panel`), and its file may come
            // first: the name is the declaration's, and the group's only
            // when nothing else has it (below).
            if d.named_kind() == Some(DeclarationKind::TestSuite) {
                suites.push((q, info.display_name.clone(), DeclId(id.0)));
            } else {
                view.qualified.entry(q).or_insert(DeclId(id.0));
                view.display
                    .entry(info.display_name.clone())
                    .or_insert(DeclId(id.0));
            }
        }
        view.decls.push(info);
    }
    for (q, display, id) in suites {
        view.qualified.entry(q).or_insert(id);
        view.display.entry(display).or_insert(id);
    }
    for (i, m) in p.members.iter().enumerate() {
        let id = program::MemberId(u32::try_from(i).unwrap_or(u32::MAX));
        let skip = skip_decl(p, m.owner);
        let kind = match m.kind {
            program::MemberKind::Field => MemberKind::Field,
            program::MemberKind::Computed => MemberKind::Computed,
            program::MemberKind::Func => MemberKind::Func,
            program::MemberKind::Init => MemberKind::Init,
            program::MemberKind::Case => MemberKind::Case,
            program::MemberKind::Rpc => MemberKind::Rpc,
            program::MemberKind::Token => MemberKind::Token,
            program::MemberKind::Generated => MemberKind::Generated,
        };
        let (ty, signature) = if skip {
            (None, None)
        } else {
            match m.kind {
                program::MemberKind::Field | program::MemberKind::Computed => {
                    (Some(Type::from_ty(&cx.field_ty(id))), None)
                }
                program::MemberKind::Func
                | program::MemberKind::Init
                | program::MemberKind::Rpc => (None, Some(sig(&cx.sig(SigKey::Member(id))))),
                program::MemberKind::Case => (None, Some(sig(&cx.case_sig(id)))),
                // A theme token's type is its category type (`space md`
                // is a `Spacing`), for tooling.
                program::MemberKind::Token => (
                    m.category
                        .as_deref()
                        .and_then(|c| cx.token_category(c))
                        .map(|d| Type::Named(DeclId(d.0), Vec::new())),
                    None,
                ),
                _ => (None, None),
            }
        };
        view.members.push(MemberInfo {
            owner: DeclId(m.owner.0),
            name: m.name.clone(),
            kind,
            is_static: m.is_static,
            is_var: m.is_var,
            has_default: m.has_default,
            is_private: m.visibility == k_syntax::ast::Visibility::Private,
            range: m.node.as_ref().map(rowan::SyntaxNode::text_range),
            name_range: m.name_range,
            generics: m.generics.iter().map(|g| GenId(g.0)).collect(),
            annotations: m.annotations.clone(),
            category: m.category.clone(),
            ty,
            signature,
        });
    }
    view
}

/// Declarations whose signatures only the framework layer resolves
/// (`app`, migrations): the view leaves them unresolved.
fn skip_decl(p: &program::Program<'_>, d: program::DeclId) -> bool {
    matches!(
        p.decl(d).named_kind(),
        Some(DeclarationKind::App | DeclarationKind::Migration)
    )
}
