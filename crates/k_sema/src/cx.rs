//! The checking context: name lookup in files (sema.md §7.2), type
//! resolution, signatures (§7.1, layer 5), conformance (§9.2–9.3) and member
//! lookup, shared by every body check.
//!
//! A [`Cx`] is built once per analysis over a [`Program`]. Results that are
//! computed on demand (field types, function signatures, `implements`
//! lists) are memoized in `RefCell` tables, so each signature is resolved
//! once and its diagnostics are reported once. Diagnostics and the answers
//! of the position queries are collected in the context and handed to the
//! host at the end.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use k_diagnostics::{Diagnostic, FileId, Fix, Span, TextEdit, TextRange, TextSize, codes};
use k_syntax::ast::{self, AstNode, Visibility};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::fx::{FxHashMap, FxHashSet};
use crate::lookup::Resolution;
use crate::program::{DeclId, DeclKind, FileIx, GenId, GenOwner, MemberId, MemberKind, Program};
use crate::stdlib::DeclarationKind;
use crate::ty::Ty;

/// Declarations the checker gives meaning to (operators, literals,
/// conformance).
pub(crate) struct Builtins {
    // Each field is `None` if the standard library lacks the declaration;
    // [`builtin`] then gives `{error}`, which silences the checks that
    // need it.
    /// `k.core.Int`.
    pub(crate) int: Option<DeclId>,
    /// `k.core.Double`.
    pub(crate) double: Option<DeclId>,
    /// `k.core.Decimal`.
    pub(crate) decimal: Option<DeclId>,
    /// `k.core.Bool`.
    pub(crate) bool_: Option<DeclId>,
    /// `k.core.String`.
    pub(crate) string: Option<DeclId>,
    /// `k.core.Void`.
    pub(crate) void: Option<DeclId>,
    /// `k.core.Optional`.
    pub(crate) optional: Option<DeclId>,
    /// `k.core.List`.
    pub(crate) list: Option<DeclId>,
    /// `k.core.Map`.
    pub(crate) map: Option<DeclId>,
    /// `k.core.Set`.
    pub(crate) set: Option<DeclId>,
    /// `k.core.Stream`.
    pub(crate) stream: Option<DeclId>,
    /// `k.core.HalfOpenRange`.
    pub(crate) half_open: Option<DeclId>,
    /// `k.core.ClosedRange`.
    pub(crate) closed: Option<DeclId>,
    /// `k.core.Length`.
    pub(crate) length: Option<DeclId>,
    /// `k.core.Duration`.
    pub(crate) duration: Option<DeclId>,
    /// `k.core.Angle`.
    pub(crate) angle: Option<DeclId>,
    /// `k.core.Percent`.
    pub(crate) percent: Option<DeclId>,
    /// `k.core.Color`.
    pub(crate) color: Option<DeclId>,
    /// `k.core.Spacing`.
    pub(crate) spacing: Option<DeclId>,
    /// `k.core.Radius`.
    pub(crate) radius: Option<DeclId>,
    /// `k.core.FontSize`.
    pub(crate) font_size: Option<DeclId>,
    /// `k.core.Money`.
    pub(crate) money: Option<DeclId>,
    /// `k.core.Error`.
    pub(crate) error: Option<DeclId>,
    /// `k.core.Equatable`.
    pub(crate) equatable: Option<DeclId>,
    /// `k.core.Hashable`.
    pub(crate) hashable: Option<DeclId>,
    /// `k.core.Comparable`.
    pub(crate) comparable: Option<DeclId>,
    /// `k.core.Codable`.
    pub(crate) codable: Option<DeclId>,
    /// `k.core.Copyable`.
    pub(crate) copyable: Option<DeclId>,
    /// `k.core.Page`.
    pub(crate) page: Option<DeclId>,
    /// `k.core.ThemeVariant`.
    pub(crate) theme_variant: Option<DeclId>,
    /// `k.core.View`.
    pub(crate) view: Option<DeclId>,
    /// `k.http.HttpStatus` (K2401's named status cases).
    pub(crate) http_status: Option<DeclId>,
}

impl Builtins {
    /// Looks every builtin up in `p` by its qualified name.
    fn new(p: &Program<'_>) -> Builtins {
        let core = |n: &str| p.lookup_qualified(&format!("k.core.{n}"));
        Builtins {
            int: core("Int"),
            double: core("Double"),
            decimal: core("Decimal"),
            bool_: core("Bool"),
            string: core("String"),
            void: core("Void"),
            optional: core("Optional"),
            list: core("List"),
            map: core("Map"),
            set: core("Set"),
            stream: core("Stream"),
            half_open: core("HalfOpenRange"),
            closed: core("ClosedRange"),
            length: core("Length"),
            duration: core("Duration"),
            angle: core("Angle"),
            percent: core("Percent"),
            color: core("Color"),
            spacing: core("Spacing"),
            radius: core("Radius"),
            font_size: core("FontSize"),
            money: core("Money"),
            error: core("Error"),
            equatable: core("Equatable"),
            hashable: core("Hashable"),
            comparable: core("Comparable"),
            codable: core("Codable"),
            copyable: core("Copyable"),
            page: core("Page"),
            theme_variant: core("ThemeVariant"),
            view: core("View"),
            http_status: p.lookup_qualified("k.http.HttpStatus"),
        }
    }
}

/// Builds the type of a builtin declaration, `{error}` if it is missing.
pub(crate) fn builtin(decl: Option<DeclId>) -> Ty {
    decl.map_or(Ty::Error, Ty::named)
}

/// How an import resolved.
#[derive(Clone, Debug)]
pub(crate) enum ImportRes {
    /// The import binds `name` to `decl`.
    Bound(String, DeclId),
    /// The import failed (and was reported); it binds `name` to nothing.
    Failed(String),
    /// Not resolved at all (package path or syntax error).
    Ignored,
}

/// A resolved function or initializer signature.
#[derive(Clone, Debug)]
pub(crate) struct Sig {
    /// The function's own generic parameters.
    pub(crate) generics: Vec<GenId>,
    /// The parameters.
    pub(crate) params: Vec<SigParam>,
    /// `true` for `async`.
    pub(crate) is_async: bool,
    /// The declared error type.
    pub(crate) throws: Option<Ty>,
    /// The result type.
    pub(crate) ret: Ty,
}

/// One parameter of a [`Sig`].
#[derive(Clone, Debug)]
pub(crate) struct SigParam {
    /// The external label; `None` for `_` (positional).
    pub(crate) label: Option<String>,
    /// The internal name.
    pub(crate) name: String,
    /// The type.
    pub(crate) ty: Ty,
    /// `true` when a call may omit the argument: the parameter has a
    /// default value, or it is a page case's optional `@Param`.
    pub(crate) has_default: bool,
    /// The internal name's range.
    pub(crate) name_range: TextRange,
}

/// The key of a memoized signature.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum SigKey {
    /// A top-level function.
    Decl(DeclId),
    /// A member function, initializer or rpc.
    Member(MemberId),
}

/// The result of looking up a name in a file's scope.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Found {
    /// A visible declaration.
    Decl(DeclId),
    /// A declaration that exists but is out of reach (K1301).
    NotVisible(DeclId),
    /// The name belongs to an import that failed: report nothing more.
    Silenced,
    /// Nothing.
    Missing,
}

/// The result of a member lookup.
#[derive(Clone, Debug)]
pub(crate) enum MemberFound {
    /// A member, with the substitution of its owner's generic parameters.
    Member(MemberId, Vec<(GenId, Ty)>),
    /// A theme token of a category type.
    Token(MemberId),
    /// The member exists but is private to another body (K1301).
    NotVisible(MemberId),
    /// The member exists but is static where an instance member is needed,
    /// or the reverse (K1203).
    WrongKind,
    /// An extension member whose `where` constraints the receiver doesn't
    /// meet (K2009 at the member name).
    Unsatisfied(MemberId),
    /// Nothing.
    Missing,
}

/// Infers the type of an unannotated field from its initializer.
pub(crate) type FieldHook<'p, 'a> = Box<dyn Fn(&Cx<'p, 'a>, MemberId) -> Ty + 'p>;

/// Infers the type of an unannotated top-level constant.
pub(crate) type ConstantHook<'p, 'a> = Box<dyn Fn(&Cx<'p, 'a>, DeclId) -> Ty + 'p>;

/// A signature to call with the substitution of its owner's generics.
pub(crate) type SigUse = (Rc<Sig>, Vec<(GenId, Ty)>);

/// The resolved `implements` entries of a declaration, with their ranges.
pub(crate) type Implements = Rc<Vec<(Ty, TextRange)>>;

/// Mutating methods of the stdlib collections (K2011 on a `let` receiver).
pub(crate) const MUTATING_METHODS: &[&str] = &[
    "append",
    "insert",
    "remove",
    "removeAll",
    "removeWhere",
    "set",
];

/// The interfaces whose conformance is derived from fields and payloads.
pub(crate) const DERIVED: &[&str] = &["Equatable", "Hashable", "Codable"];

/// The checking context.
pub(crate) struct Cx<'p, 'a> {
    /// The program.
    pub(crate) p: &'p Program<'a>,
    /// Builtin declarations.
    pub(crate) b: Builtins,
    /// Import resolutions per file and import.
    pub(crate) imports: Vec<Vec<ImportRes>>,
    /// Imports that were used: (file, index).
    pub(crate) used_imports: RefCell<FxHashSet<(FileIx, usize)>>,
    /// Every diagnostic reported so far.
    pub(crate) diags: RefCell<Vec<Diagnostic>>,
    /// Types for `type_at`.
    pub(crate) types: RefCell<FxHashMap<(FileId, TextRange), Ty>>,
    /// Resolutions for `resolve_at`.
    pub(crate) resolutions: RefCell<FxHashMap<(FileId, TextRange), Resolution>>,
    /// The resolutions of each file's body check, one chunk per file (its
    /// own, or replayed from the previous revision; `crate::warm`). The
    /// project passes that read resolutions see these and `resolutions`.
    pub(crate) body_resolutions: RefCell<Vec<std::sync::Arc<crate::warm::RangeMap<Resolution>>>>,
    /// Memoized field types ([`Cx::field_ty`]).
    field_tys: RefCell<FxHashMap<MemberId, Ty>>,
    /// Fields whose type is being inferred: a field met again while its
    /// own initializer is inferred is `{error}` (no infinite recursion).
    field_busy: RefCell<FxHashSet<MemberId>>,
    /// Memoized signatures ([`Cx::sig`]).
    sigs: RefCell<FxHashMap<SigKey, Rc<Sig>>>,
    /// Memoized `implements` lists ([`Cx::implements_list`]).
    implements: RefCell<FxHashMap<DeclId, Implements>>,
    /// Memoized constraints of generic parameters ([`Cx::constraints`]).
    constraints: RefCell<FxHashMap<GenId, Rc<Vec<Ty>>>>,
    /// Memoized extension targets ([`Cx::ext_target`]).
    ext_targets: RefCell<FxHashMap<DeclId, Ty>>,
    /// Extensions by the declaration they extend.
    pub(crate) extensions_of: RefCell<FxHashMap<DeclId, Vec<DeclId>>>,
    /// Theme tokens by category type.
    pub(crate) tokens: HashMap<DeclId, Vec<MemberId>>,
    /// The hook that infers the type of a field without annotation from
    /// its initializer, installed by the semantic pass (`check.rs`).
    pub(crate) infer_field: RefCell<Option<FieldHook<'p, 'a>>>,
    /// Infers the type of a top-level constant without annotation.
    pub(crate) infer_constant: RefCell<Option<ConstantHook<'p, 'a>>>,
    /// The framework layer's state (styles, references, table types).
    pub(crate) fw: crate::framework::FwState,
    /// Sides and `use` rules of the DI layer.
    pub(crate) di: crate::di::DiState,
    /// Memoized [`Cx::session_alias`] answers.
    aliases: RefCell<FxHashMap<DeclId, Option<DeclId>>>,
}

impl<'p, 'a> Cx<'p, 'a> {
    /// Creates the context and resolves every import.
    pub(crate) fn new(p: &'p Program<'a>) -> Cx<'p, 'a> {
        let b = Builtins::new(p);
        let mut tokens: HashMap<DeclId, Vec<MemberId>> = HashMap::new();
        let mut cx = Cx {
            p,
            b,
            imports: Vec::new(),
            used_imports: RefCell::default(),
            diags: RefCell::new(Vec::new()),
            types: RefCell::default(),
            resolutions: RefCell::default(),
            body_resolutions: RefCell::default(),
            field_tys: RefCell::default(),
            field_busy: RefCell::default(),
            sigs: RefCell::default(),
            implements: RefCell::default(),
            constraints: RefCell::default(),
            ext_targets: RefCell::default(),
            extensions_of: RefCell::default(),
            tokens: HashMap::new(),
            infer_field: RefCell::new(None),
            infer_constant: RefCell::new(None),
            fw: crate::framework::FwState::default(),
            di: crate::di::DiState::default(),
            aliases: RefCell::default(),
        };
        cx.imports = (0..p.files.len())
            .map(|i| cx.resolve_imports(FileIx(u32::try_from(i).unwrap_or(u32::MAX))))
            .collect();
        // The last segment of a bound import refers to its declaration, for
        // the reverse index and rename (sema.md §17).
        for (i, imports) in cx.imports.iter().enumerate() {
            let file = FileIx(u32::try_from(i).unwrap_or(u32::MAX));
            for (import, res) in p.file(file).imports.iter().zip(imports) {
                if let ImportRes::Bound(_, decl) = res {
                    cx.record_resolution(file, import.last_range, cx.decl_resolution(*decl));
                }
            }
        }
        for (i, member) in p.members.iter().enumerate() {
            if member.kind != MemberKind::Token {
                continue;
            }
            let Some(category) = &member.category else {
                continue;
            };
            if let Some(decl) = cx.token_category(category) {
                tokens
                    .entry(decl)
                    .or_default()
                    .push(MemberId(u32::try_from(i).unwrap_or(u32::MAX)));
            }
        }
        cx.tokens = tokens;
        cx
    }

    /// Maps a token category word to its type.
    pub(crate) fn token_category(&self, word: &str) -> Option<DeclId> {
        let name = match word {
            "color" => "Color",
            "space" | "spacing" => "Spacing",
            "radius" => "Radius",
            "font" => "Font",
            "fontSize" | "size" => "FontSize",
            "shadow" => "Shadow",
            "duration" => "Duration",
            _ => return None,
        };
        self.p.prelude.get(name).copied()
    }

    // ------------------------------------------------------------ reporting

    /// Reports a diagnostic, unless it is in a stdlib file.
    pub(crate) fn report(&self, file: FileIx, diagnostic: Diagnostic) {
        if self.p.file(file).stdlib {
            return;
        }
        self.diags.borrow_mut().push(diagnostic);
    }

    /// Reports `code` at `range` of `file` with `message`.
    pub(crate) fn error(
        &self,
        file: FileIx,
        code: k_diagnostics::Code,
        range: TextRange,
        message: impl Into<String>,
    ) {
        let span = self.p.span(file, range);
        self.report(file, Diagnostic::new(code, span, message));
    }

    /// Records the type of the node at `range` (the first one recorded
    /// wins). Types are rendered only when `type_at` asks for one.
    pub(crate) fn record_type(&self, file: FileIx, range: TextRange, ty: Ty) {
        if self.p.file(file).stdlib {
            return;
        }
        self.types
            .borrow_mut()
            .entry((self.p.file(file).id, range))
            .or_insert(ty);
    }

    /// Records what the reference at `range` resolves to.
    pub(crate) fn record_resolution(&self, file: FileIx, range: TextRange, resolution: Resolution) {
        if self.p.file(file).stdlib {
            return;
        }
        self.resolutions
            .borrow_mut()
            .entry((self.p.file(file).id, range))
            .or_insert(resolution);
    }

    /// The resolution of a declaration reference.
    pub(crate) fn decl_resolution(&self, decl: DeclId) -> Resolution {
        Resolution::Declaration {
            name: self.p.decl_name(decl),
        }
    }

    /// The resolution of a member reference.
    pub(crate) fn member_resolution(&self, member: MemberId) -> Resolution {
        let m = self.p.member(member);
        let owner = self.member_owner_type(member);
        let mut name = m.name.clone();
        if matches!(
            m.kind,
            MemberKind::Func | MemberKind::Init | MemberKind::Rpc
        ) || (m.kind == MemberKind::Generated && m.name == "copy")
        {
            name.push('(');
            for label in self.member_labels(member) {
                name.push_str(label.as_deref().unwrap_or("_"));
                name.push(':');
            }
            name.push(')');
        }
        if m.kind == MemberKind::Token {
            if let Some(category) = m.category.as_deref().and_then(|c| self.token_category(c)) {
                return Resolution::Token {
                    category: self.p.decl_name(category),
                    name: m.name.clone(),
                };
            }
        }
        Resolution::Member {
            owner: self.p.decl_name(owner),
            name,
        }
    }

    /// The declaration a member belongs to for display: the extended type
    /// for extension members.
    pub(crate) fn member_owner_type(&self, member: MemberId) -> DeclId {
        let owner = self.p.member(member).owner;
        if self.p.decl(owner).kind == DeclKind::Extension {
            if let Some(d) = self.ext_target(owner).decl() {
                return d;
            }
        }
        owner
    }

    /// Returns the argument labels of a function-like member, `None` for an
    /// unlabelled (`_`) parameter; a generated `copy` takes one label per
    /// stored field of its type.
    fn member_labels(&self, member: MemberId) -> Vec<Option<String>> {
        let m = self.p.member(member);
        if m.kind == MemberKind::Generated && m.name == "copy" {
            return self
                .stored_fields(m.owner)
                .iter()
                .map(|f| Some(self.p.member(*f).name.clone()))
                .collect();
        }
        let Some(node) = &m.node else {
            return Vec::new();
        };
        param_list_labels(node)
    }

    // --------------------------------------------------------------- imports

    /// Resolves the imports of `file`, one [`ImportRes`] per import in order.
    ///
    /// Reports unknown imports (K1101, with a "did you mean" fix), imports of
    /// non-public declarations (K1102), test-only modules imported outside tests (K1104) and
    /// duplicate or self imports (K1106). `k.internal` is unknown outside
    /// the stdlib, and package (non-plain) imports are [`ImportRes::Ignored`].
    fn resolve_imports(&self, file: FileIx) -> Vec<ImportRes> {
        let info = self.p.file(file);
        let mut out = Vec::new();
        let mut bound: HashMap<String, (usize, DeclId)> = HashMap::new();
        for (index, import) in info.imports.iter().enumerate() {
            if !import.plain {
                out.push(ImportRes::Ignored);
                continue;
            }
            let segments: Vec<&str> = import.path.split('.').collect();
            let name = segments.last().copied().unwrap_or_default().to_owned();
            let module_path = segments[..segments.len().saturating_sub(1)].join(".");
            let module = self.p.module_by_path.get(&module_path).copied();
            let decl = module.and_then(|m| {
                self.p.modules[m.0 as usize]
                    .names
                    .get(&name)
                    .and_then(|d| d.first().copied())
            });
            let internal = module_path == crate::stdlib::INTERNAL_MODULE;
            let Some(decl) = decl.filter(|_| !internal || info.stdlib) else {
                let mut diagnostic = Diagnostic::new(
                    codes::UNRESOLVED_IMPORT,
                    self.p.span(file, import.range),
                    format!("`{}` doesn't exist", import.path),
                );
                if let Some(closest) = self.closest_import(&import.path, info.is_test) {
                    diagnostic = diagnostic.with_fix(
                        Fix::replace(
                            format!("Replace with `{closest}`"),
                            self.p.span(file, import.range),
                            closest,
                        )
                        .maybe_incorrect(),
                    );
                }
                self.report(file, diagnostic);
                out.push(ImportRes::Failed(name));
                continue;
            };
            let target = self.p.decl(decl);
            if target.visibility != Visibility::Public {
                self.report(
                    file,
                    Diagnostic::new(
                        codes::IMPORT_NOT_VISIBLE,
                        self.p.span(file, import.last_range),
                        format!("`{}` is not public", import.path),
                    ),
                );
                out.push(ImportRes::Failed(name));
                continue;
            }
            let target_module = module.map(|m| &self.p.modules[m.0 as usize]);
            let importer_test_only = info
                .module
                .is_some_and(|m| self.p.modules[m.0 as usize].test_only);
            if target_module.is_some_and(|m| m.test_only)
                && !info.is_test
                && !importer_test_only
                && !info.stdlib
            {
                self.error(
                    file,
                    codes::TEST_ONLY_IMPORT,
                    import.range,
                    format!(
                        "`{}` is test-only and can be imported only from tests",
                        import.path
                    ),
                );
            }
            if info.module.is_some() && info.module == module {
                // The import names the module's own declaration: it is
                // already in scope, so removing the import changes nothing.
                let target = self.p.decl(decl);
                self.report(
                    file,
                    Diagnostic::new(
                        codes::DUPLICATE_IMPORT,
                        self.p.span(file, import.range),
                        format!("`{name}` is already declared in this module"),
                    )
                    .with_label(
                        self.p.span(target.file, target.name_range),
                        "declared in this module here",
                    )
                    .with_fix(Fix::replace(
                        "Remove the duplicate import",
                        self.p.span(file, self.line_range(file, import.node_range)),
                        "",
                    )),
                );
                out.push(ImportRes::Failed(name));
                continue;
            }
            if let Some(m) = info.module {
                let own = &self.p.modules[m.0 as usize];
                let local = own
                    .names
                    .get(&name)
                    .or_else(|| info.is_test.then(|| own.test_names.get(&name)).flatten())
                    .and_then(|list| list.first().copied());
                if let Some(local) = local {
                    let local = self.p.decl(local);
                    self.report(
                        file,
                        Diagnostic::new(
                            codes::DUPLICATE_IMPORT,
                            self.p.span(file, import.range),
                            format!("`{name}` is already declared in this module"),
                        )
                        .with_label(
                            self.p.span(local.file, local.name_range),
                            "declared in this module here",
                        ),
                    );
                    out.push(ImportRes::Failed(name));
                    continue;
                }
            }
            if let Some(&(first, first_decl)) = bound.get(&name) {
                let mut diagnostic = Diagnostic::new(
                    codes::DUPLICATE_IMPORT,
                    self.p.span(file, import.range),
                    format!("`{name}` is imported twice"),
                );
                if first_decl == decl {
                    diagnostic = diagnostic
                        .with_label(
                            self.p.span(file, info.imports[first].range),
                            "first imported here",
                        )
                        .with_fix(Fix::replace(
                            "Remove the duplicate import",
                            self.p.span(file, self.line_range(file, import.node_range)),
                            "",
                        ));
                }
                self.report(file, diagnostic);
                out.push(ImportRes::Failed(name));
                continue;
            }
            bound.insert(name.clone(), (index, decl));
            out.push(ImportRes::Bound(name, decl));
        }
        out
    }

    /// The range of whole lines covering `range`, newline included.
    pub(crate) fn line_range(&self, file: FileIx, range: TextRange) -> TextRange {
        let text = self.p.file(file).text;
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
        TextRange::new(to_size(line_start), to_size(line_end))
    }

    /// Returns the qualified name closest to the unknown import `path` (edit
    /// distance at most 2, ties broken alphabetically), skipping `k.internal`
    /// and, outside test files, test-only modules.
    ///
    /// Linear in the number of qualified names.
    fn closest_import(&self, path: &str, test: bool) -> Option<String> {
        let mut best: Option<(usize, &String)> = None;
        for (name, decl) in &self.p.qualified {
            let module = self.p.decl_module(*decl);
            if module.is_some_and(|m| self.p.modules[m.0 as usize].test_only) && !test {
                continue;
            }
            if name.starts_with("k.internal.") {
                continue;
            }
            let distance = edit_distance(path, name);
            if distance <= 2
                && best.is_none_or(|(d, n)| distance < d || (distance == d && name < n))
            {
                best = Some((distance, name));
            }
        }
        best.map(|(_, n)| n.clone())
    }

    // ---------------------------------------------------------------- lookup

    /// Looks up a simple name in the scope of `file` (sema.md §7.2):
    /// test-file declarations, the module, the file's imports, the
    /// stdlib-internal module (stdlib files only) and the prelude.
    pub(crate) fn lookup(&self, file: FileIx, name: &str) -> Found {
        let info = self.p.file(file);
        let mut hidden = None;
        let mut candidates: Vec<DeclId> = Vec::new();
        if let Some(m) = info.module {
            let module = &self.p.modules[m.0 as usize];
            if info.is_test {
                if let Some(list) = module.test_names.get(name) {
                    candidates.extend(list);
                }
            }
            if let Some(list) = module.names.get(name) {
                candidates.extend(list);
            }
        } else {
            for &d in &info.decls {
                let decl = self.p.decl(d);
                if decl.name == name && decl.kind != DeclKind::Extension {
                    candidates.push(d);
                }
            }
        }
        for d in candidates {
            let decl = self.p.decl(d);
            if decl.named_kind() == Some(DeclarationKind::TestSuite) {
                continue;
            }
            if decl.visibility == Visibility::Private && decl.file != file {
                hidden.get_or_insert(d);
                continue;
            }
            return Found::Decl(d);
        }
        for (index, import) in self.imports[file.0 as usize].iter().enumerate() {
            match import {
                ImportRes::Bound(n, d) if n == name => {
                    self.used_imports.borrow_mut().insert((file, index));
                    return Found::Decl(*d);
                }
                ImportRes::Failed(n) if n == name => return Found::Silenced,
                _ => {}
            }
        }
        if let Some(d) = hidden {
            return Found::NotVisible(d);
        }
        if info.is_test {
            if let Some(&d) = self.p.qualified.get(&format!("k.test.{name}")) {
                return Found::Decl(d);
            }
        }
        if info.stdlib {
            if let Some(&d) = self.p.internal.get(name) {
                return Found::Decl(d);
            }
        }
        if let Some(&d) = self.p.prelude.get(name) {
            return Found::Decl(d);
        }
        Found::Missing
    }

    /// Returns the closest name to an unresolved `name` among the
    /// declarations in `file`'s scope (its module, its bound imports and
    /// the prelude) and the `extra` names of the use site (locals,
    /// members), for the maybe-incorrect closest-name fix of K1201/K1202.
    ///
    /// With `types_only`, only type declarations are candidates. Candidates
    /// are visited in a deterministic order, and ties go to the
    /// alphabetically first name (see [`closest`]).
    pub(crate) fn closest_visible(
        &self,
        file: FileIx,
        name: &str,
        types_only: bool,
        extra: &[&str],
    ) -> Option<String> {
        let info = self.p.file(file);
        let keep = |d: DeclId| {
            let decl = self.p.decl(d);
            decl.kind != DeclKind::Extension
                && decl.named_kind() != Some(DeclarationKind::TestSuite)
                && (decl.visibility != Visibility::Private || decl.file == file)
                && (!types_only || decl.is_type())
        };
        let mut names: Vec<&str> = extra.to_vec();
        if let Some(m) = info.module {
            let module = &self.p.modules[m.0 as usize];
            let tables = [
                Some(&module.names),
                info.is_test.then_some(&module.test_names),
            ];
            for table in tables.into_iter().flatten() {
                for (n, list) in table {
                    if list.iter().any(|d| keep(*d)) {
                        names.push(n);
                    }
                }
            }
        } else {
            for &d in &info.decls {
                if keep(d) {
                    names.push(&self.p.decl(d).name);
                }
            }
        }
        for import in &self.imports[file.0 as usize] {
            if let ImportRes::Bound(n, d) = import {
                if keep(*d) {
                    names.push(n);
                }
            }
        }
        for (n, d) in &self.p.prelude {
            if keep(*d) {
                names.push(n);
            }
        }
        closest(name, names.into_iter()).map(str::to_owned)
    }

    /// Returns the import fix for an unresolved `name` in `file`, when
    /// exactly one public declaration outside the file's scope has it.
    pub(crate) fn import_fix(&self, file: FileIx, name: &str) -> Option<Fix> {
        let info = self.p.file(file);
        let mut found: Vec<&String> = Vec::new();
        for (q, d) in &self.p.qualified {
            let decl = self.p.decl(*d);
            if decl.name != name || q.starts_with("k.internal.") {
                continue;
            }
            let module = self.p.decl_module(*d);
            if module.is_some_and(|m| self.p.modules[m.0 as usize].test_only) && !info.is_test {
                continue;
            }
            found.push(q);
        }
        if found.len() != 1 {
            return None;
        }
        let path = found[0].clone();
        let (offset, text) = self.import_insertion(file, &path);
        Some(Fix::replace(
            format!("Import `{path}`"),
            Span::empty(info.id, offset),
            text,
        ))
    }

    /// The edits removing the imports of `file` that only text inside
    /// `removed` uses, for a fix that deletes that text, so applying the fix
    /// leaves no unused import (K1105). Each edit deletes the import's whole
    /// line.
    ///
    /// An import is only the removed text's when every identifier token of
    /// the file with its name (outside the imports) lies in `removed`, and
    /// at least one does. Names in `kept` are never removed: a fix that also
    /// writes them elsewhere in the file still needs their import.
    pub(crate) fn imports_only_in(
        &self,
        file: FileIx,
        removed: TextRange,
        kept: &HashSet<String>,
    ) -> Vec<TextEdit> {
        let info = self.p.file(file);
        let mut edits = Vec::new();
        for (i, import) in info.imports.iter().enumerate() {
            let Some(ImportRes::Bound(name, _)) = self.imports[file.0 as usize].get(i) else {
                continue;
            };
            if kept.contains(name) {
                continue;
            }
            let mut inside = false;
            let mut elsewhere = false;
            for t in info
                .root
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::Ident && t.text() == name)
            {
                let r = t.text_range();
                if info
                    .imports
                    .iter()
                    .any(|im| im.node_range.contains_range(r))
                {
                    continue;
                }
                if removed.contains_range(r) {
                    inside = true;
                } else {
                    elsewhere = true;
                    break;
                }
            }
            if inside && !elsewhere {
                edits.push(TextEdit {
                    span: self.p.span(file, self.line_range(file, import.node_range)),
                    replacement: String::new(),
                });
            }
        }
        edits
    }

    /// The edit importing the declaration with the qualified name `path`
    /// into `file`, for a fix that writes its simple name; `None` when the
    /// file already sees it (imported, in the file's module, or in the
    /// prelude) or no such declaration exists.
    ///
    /// Unlike [`Cx::lookup`], this doesn't mark an import as used.
    pub(crate) fn import_edit(&self, file: FileIx, path: &str) -> Option<TextEdit> {
        let target = self.p.lookup_qualified(path)?;
        let info = self.p.file(file);
        let visible = info.module.is_some() && self.p.decl_module(target) == info.module
            || self.p.prelude.get(&self.p.decl(target).name) == Some(&target)
            || self.imports[file.0 as usize]
                .iter()
                .any(|i| matches!(i, ImportRes::Bound(_, d) if *d == target));
        if visible {
            return None;
        }
        let (offset, text) = self.import_insertion(file, path);
        Some(TextEdit {
            span: Span::empty(info.id, offset),
            replacement: text,
        })
    }

    /// Returns where and what to insert to import `path` into `file`: in
    /// alphabetical order among the existing imports, or before the first item
    /// followed by a blank line when there are none.
    fn import_insertion(&self, file: FileIx, path: &str) -> (TextSize, String) {
        let info = self.p.file(file);
        if info.imports.is_empty() {
            let offset = info
                .root
                .children()
                .find(|c| ast::Item::can_cast(c.kind()))
                .map_or(TextSize::from(0), |c| {
                    crate::program::first_token_range(&c).start()
                });
            let offset = item_start(&info.root, offset);
            return (offset, format!("import {path}\n\n"));
        }
        for import in &info.imports {
            if import.path.as_str() > path {
                return (
                    self.line_range(file, import.node_range).start(),
                    format!("import {path}\n"),
                );
            }
        }
        let last = info
            .imports
            .last()
            .map_or(TextRange::empty(0.into()), |i| i.node_range);
        (
            self.line_range(file, last).end(),
            format!("import {path}\n"),
        )
    }

    // ------------------------------------------------------- type resolution

    /// Resolves type syntax in `file`, with `generics` in scope.
    pub(crate) fn resolve_type(&self, file: FileIx, generics: &[GenId], ty: &ast::Type) -> Ty {
        match ty {
            ast::Type::PathType(path) => self.resolve_path_type(file, generics, path),
            ast::Type::OptionalType(o) => o.inner().map_or(Ty::Error, |inner| {
                Ty::optional(self.resolve_type(file, generics, &inner))
            }),
            ast::Type::ParenType(p) => p
                .syntax()
                .children()
                .find_map(ast::Type::cast)
                .map_or(Ty::Error, |t| self.resolve_type(file, generics, &t)),
            ast::Type::TupleType(t) => Ty::Tuple(
                t.syntax()
                    .children()
                    .filter_map(ast::Type::cast)
                    .map(|t| self.resolve_type(file, generics, &t))
                    .collect(),
            ),
            ast::Type::FuncType(f) => {
                let node = f.syntax();
                let params = node
                    .children()
                    .filter_map(ast::Type::cast)
                    .map(|t| self.resolve_type(file, generics, &t))
                    .collect();
                let is_async = has_token(node, SyntaxKind::AsyncKw);
                let throws = node.children().find_map(ast::ThrowsClause::cast).map(|c| {
                    c.error_type()
                        .map_or(Ty::Error, |t| self.resolve_type(file, generics, &t))
                });
                let ret = node
                    .children()
                    .find_map(ast::ReturnType::cast)
                    .and_then(|r| r.ty())
                    .map_or_else(
                        || builtin(self.b.void),
                        |t| self.resolve_type(file, generics, &t),
                    );
                Ty::func(params, is_async, throws, ret)
            }
        }
    }

    /// Resolves a named type (`Name`, `Name<Args>` or a qualified
    /// `Outer.Inner`) in `file` with `generics` in scope: generic parameters
    /// first, then the file's names; reports unknown (K1202) and invisible
    /// (K1301) types and returns `{error}` for them.
    fn resolve_path_type(&self, file: FileIx, generics: &[GenId], path: &ast::PathType) -> Ty {
        let segments = path.segments();
        let (Some(first), Some(last)) = (segments.first(), segments.last()) else {
            return Ty::Error;
        };
        let range = TextRange::new(first.text_range().start(), last.text_range().end());
        let args: Vec<(Ty, TextRange)> = path
            .generic_arg_list()
            .map(|list| {
                list.types()
                    .map(|t| {
                        (
                            self.resolve_type(file, generics, &t),
                            t.syntax().text_range(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        if segments.len() == 1 {
            let name = first.text();
            if let Some(&g) = generics
                .iter()
                .find(|g| self.p.generics[g.0 as usize].name == name)
            {
                return Ty::Param(g);
            }
        }
        let decl = if segments.len() == 1 {
            match self.lookup(file, first.text()) {
                Found::Decl(d) => Some(d),
                Found::NotVisible(d) => {
                    self.report(
                        file,
                        Diagnostic::new(
                            codes::NOT_VISIBLE,
                            self.p.span(file, range),
                            format!("`{}` is private to its file", first.text()),
                        ),
                    );
                    let _ = d;
                    return Ty::Error;
                }
                Found::Silenced => return Ty::Error,
                Found::Missing => None,
            }
        } else {
            let text: Vec<&str> = segments.iter().map(SyntaxToken::text).collect();
            let found = self.p.lookup_qualified(&text.join("."));
            if found.is_none() {
                // `Product.UniqueViolation`: types generated for an entity.
                if let Found::Decl(d) = self.lookup(file, first.text()) {
                    if self.p.decl(d).named_kind() == Some(DeclarationKind::Entity) {
                        return Ty::Error;
                    }
                }
            }
            found
        };
        let Some(decl) = decl.filter(|d| self.p.decl(*d).is_type()) else {
            if segments.len() == 1 && first.text() == "Never" {
                return Ty::Never;
            }
            let text: Vec<&str> = segments.iter().map(SyntaxToken::text).collect();
            let name = text.join(".");
            let mut diagnostic = Diagnostic::new(
                codes::UNRESOLVED_TYPE,
                self.p.span(file, range),
                format!("cannot find type `{name}` in this scope"),
            );
            if segments.len() == 1 {
                if let Some(fix) = self.import_fix(file, &name) {
                    diagnostic = diagnostic.with_fix(fix);
                } else if let Some(c) = self.closest_visible(file, &name, true, &[]) {
                    diagnostic = diagnostic.with_fix(
                        Fix::replace(format!("Replace with `{c}`"), self.p.span(file, range), c)
                            .maybe_incorrect(),
                    );
                }
            }
            self.report(file, diagnostic);
            return Ty::Error;
        };
        self.record_resolution(file, last.text_range(), self.decl_resolution(decl));
        if let Some(alias) = self.session_alias(decl) {
            return Ty::named(alias);
        }
        let params = &self.p.decl(decl).generics;
        let mut tys: Vec<Ty> = args.iter().map(|(t, _)| t.clone()).collect();
        tys.resize(params.len(), Ty::Error);
        tys.truncate(params.len());
        // Constraints on the arguments (K2009 at the type argument).
        for (i, g) in params.iter().enumerate() {
            let Some((arg, arg_range)) = args.get(i) else {
                continue;
            };
            for constraint in self.constraints(*g).iter() {
                if let Some(iface) = constraint.decl() {
                    if !self.implements(arg, iface) {
                        self.error(
                            file,
                            codes::UNSATISFIED_CONSTRAINT,
                            *arg_range,
                            format!(
                                "`{}` doesn't implement `{}`",
                                arg.render(self.p),
                                self.p.decl_name(iface)
                            ),
                        );
                    }
                }
            }
        }
        Ty::generic(decl, tys)
    }

    // ------------------------------------------------------------ generics

    /// The generic parameters in scope inside a declaration's body.
    pub(crate) fn decl_generics(&self, decl: DeclId) -> Vec<GenId> {
        self.p.decl(decl).generics.clone()
    }

    /// The `where` constraints of a generic parameter.
    pub(crate) fn constraints(&self, g: GenId) -> Rc<Vec<Ty>> {
        if let Some(c) = self.constraints.borrow().get(&g) {
            return Rc::clone(c);
        }
        self.constraints.borrow_mut().insert(g, Rc::new(Vec::new()));
        let generic = &self.p.generics[g.0 as usize];
        let (node, file, scope) = match generic.owner {
            GenOwner::Decl(d) => (
                Some(self.p.decl(d).node.clone()),
                self.p.decl(d).file,
                self.decl_generics(d),
            ),
            GenOwner::Member(m) => {
                let member = self.p.member(m);
                let decl = self.p.decl(member.owner);
                let mut scope = self.decl_generics(member.owner);
                scope.extend(member.generics.iter().copied());
                (member.node.clone(), decl.file, scope)
            }
        };
        let mut out = Vec::new();
        if let Some(node) = node {
            if let Some(clause) = node
                .children()
                .find(|c| c.kind() == SyntaxKind::WhereClause)
            {
                for predicate in clause
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::WherePredicate)
                {
                    let name = predicate
                        .children_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .find(|t| t.kind() == SyntaxKind::Ident);
                    if name.is_some_and(|n| n.text() == generic.name) {
                        if let Some(t) = predicate.children().find_map(ast::Type::cast) {
                            out.push(self.resolve_type_quiet(file, &scope, &t));
                        }
                    }
                }
            }
        }
        let out = Rc::new(out);
        self.constraints.borrow_mut().insert(g, Rc::clone(&out));
        out
    }

    /// Resolves a type without reporting anything (the signature pass
    /// reports its errors once).
    pub(crate) fn resolve_type_quiet(
        &self,
        file: FileIx,
        generics: &[GenId],
        ty: &ast::Type,
    ) -> Ty {
        let before = self.diags.borrow().len();
        let t = self.resolve_type(file, generics, ty);
        self.diags.borrow_mut().truncate(before);
        t
    }

    // ---------------------------------------------------------- extensions

    /// The type an extension extends (`{error}` when it doesn't resolve).
    pub(crate) fn ext_target(&self, ext: DeclId) -> Ty {
        if let Some(t) = self.ext_targets.borrow().get(&ext) {
            return t.clone();
        }
        let decl = self.p.decl(ext);
        let ty = decl
            .node
            .children()
            .find_map(ast::Type::cast)
            .map_or(Ty::Error, |t| {
                self.resolve_type(decl.file, &decl.generics, &t)
            });
        self.ext_targets.borrow_mut().insert(ext, ty.clone());
        ty
    }

    /// Indexes extensions by the declaration they extend; reports K1202
    /// for unknown extended types.
    pub(crate) fn index_extensions(&self) {
        let mut map: FxHashMap<DeclId, Vec<DeclId>> = FxHashMap::default();
        for &ext in &self.p.extensions {
            if let Some(d) = self.ext_target(ext).decl() {
                map.entry(d).or_default().push(ext);
            }
        }
        *self.extensions_of.borrow_mut() = map;
    }

    /// Returns `true` if extension `ext`'s members are visible in `file`.
    pub(crate) fn extension_visible(&self, ext: DeclId, file: FileIx) -> bool {
        let decl = self.p.decl(ext);
        match decl.visibility {
            Visibility::Public => true,
            _ => {
                let a = self.p.file(decl.file).module;
                let b = self.p.file(file).module;
                if a.is_none() || b.is_none() {
                    decl.file == file
                } else {
                    a == b
                }
            }
        }
    }

    // --------------------------------------------------------- signatures

    /// The type of a field or computed property, resolved on first use. A
    /// field without annotation takes its initializer's type.
    pub(crate) fn field_ty(&self, member: MemberId) -> Ty {
        if let Some(t) = self.field_tys.borrow().get(&member) {
            return t.clone();
        }
        let m = self.p.member(member);
        let decl = self.p.decl(m.owner);
        let ty = match m.kind {
            MemberKind::Token => {
                let annotated = m
                    .node
                    .as_ref()
                    .and_then(|n| n.children().find_map(ast::Type::cast))
                    .map(|t| self.resolve_type(decl.file, &[], &t));
                annotated.unwrap_or_else(|| {
                    m.category
                        .as_deref()
                        .and_then(|c| self.token_category(c))
                        .map_or(Ty::Error, Ty::named)
                })
            }
            MemberKind::Generated => {
                if m.name == "copy" {
                    Ty::Error
                } else {
                    Ty::named(m.owner)
                }
            }
            _ => {
                let annotated = m
                    .node
                    .as_ref()
                    .and_then(|n| ast::FieldDecl::cast(n.clone()))
                    .and_then(|f| f.ty());
                if let Some(t) = annotated {
                    self.resolve_type(decl.file, &decl.generics, &t)
                } else {
                    if !self.field_busy.borrow_mut().insert(member) {
                        return Ty::Error;
                    }
                    let infer = self.infer_field.borrow();
                    let t = infer.as_ref().map_or(Ty::Error, |f| f(self, member));
                    drop(infer);
                    self.field_busy.borrow_mut().remove(&member);
                    t
                }
            }
        };
        self.field_tys.borrow_mut().insert(member, ty.clone());
        ty
    }

    /// The type of a top-level constant.
    pub(crate) fn constant_ty(&self, decl: DeclId) -> Ty {
        // Top-level constants are recorded as pseudo-members: resolve
        // through the same path by inferring the initializer.
        let key = SigKey::Decl(decl);
        if let Some(sig) = self.sigs.borrow().get(&key) {
            return sig.ret.clone();
        }
        let d = self.p.decl(decl);
        let field = ast::FieldDecl::cast(d.node.clone());
        let ty = match field.as_ref().and_then(ast::FieldDecl::ty) {
            Some(t) => self.resolve_type(d.file, &[], &t),
            None => {
                // Guard recursion with a placeholder.
                self.sigs.borrow_mut().insert(
                    key,
                    Rc::new(Sig {
                        generics: Vec::new(),
                        params: Vec::new(),
                        is_async: false,
                        throws: None,
                        ret: Ty::Error,
                    }),
                );
                let infer = self.infer_constant.borrow();
                let t = infer.as_ref().map_or(Ty::Error, |f| f(self, decl));
                drop(infer);
                t
            }
        };
        self.sigs.borrow_mut().insert(
            key,
            Rc::new(Sig {
                generics: Vec::new(),
                params: Vec::new(),
                is_async: false,
                throws: None,
                ret: ty.clone(),
            }),
        );
        ty
    }

    /// The signature of a function, initializer or rpc.
    pub(crate) fn sig(&self, key: SigKey) -> Rc<Sig> {
        if let Some(s) = self.sigs.borrow().get(&key) {
            return Rc::clone(s);
        }
        let (node, file, mut scope, own) = match key {
            SigKey::Decl(d) => {
                let decl = self.p.decl(d);
                (
                    Some(decl.node.clone()),
                    decl.file,
                    Vec::new(),
                    decl.generics.clone(),
                )
            }
            SigKey::Member(m) => {
                let member = self.p.member(m);
                let decl = self.p.decl(member.owner);
                (
                    member.node.clone(),
                    decl.file,
                    decl.generics.clone(),
                    member.generics.clone(),
                )
            }
        };
        scope.extend(own.iter().copied());
        let sig = match node {
            Some(node) => self.build_sig(file, &scope, own, &node, key),
            None => Sig {
                generics: own,
                params: Vec::new(),
                is_async: false,
                throws: None,
                ret: Ty::Error,
            },
        };
        let sig = Rc::new(sig);
        self.sigs.borrow_mut().insert(key, Rc::clone(&sig));
        sig
    }

    /// Builds the signature of a function-like `node` under `key`: parameter
    /// labels (the name when no label is written, `None` for `_`), types,
    /// defaults, `async`, `throws` and the return type (`Void` when omitted).
    fn build_sig(
        &self,
        file: FileIx,
        scope: &[GenId],
        own: Vec<GenId>,
        node: &SyntaxNode,
        key: SigKey,
    ) -> Sig {
        let mut params = Vec::new();
        if let Some(list) = node.children().find_map(ast::ParamList::cast) {
            for param in list.params() {
                let Some(name) = param.name() else { continue };
                let label = param.label();
                let label = match label {
                    Some(l) if l.kind() == SyntaxKind::Underscore => None,
                    Some(l) => Some(l.text().to_owned()),
                    None => Some(name.text().to_owned()),
                };
                let ty = param
                    .ty()
                    .map_or(Ty::Error, |t| self.resolve_type(file, scope, &t));
                params.push(SigParam {
                    label,
                    name: name.text().to_owned(),
                    ty,
                    has_default: param.default_value().is_some(),
                    name_range: name.text_range(),
                });
            }
        }
        // An rpc is a remote call: calling it is async whether or not the
        // declaration says so (11-protocols-and-rpc.md, `try await
        // api.add(…)`).
        let is_rpc = matches!(key, SigKey::Member(m) if self.p.member(m).kind == MemberKind::Rpc);
        let is_async = is_rpc || has_token(node, SyntaxKind::AsyncKw);
        let throws = node.children().find_map(ast::ThrowsClause::cast).map(|c| {
            c.error_type()
                .map_or(Ty::Error, |t| self.resolve_type(file, scope, &t))
        });
        let ret = node
            .children()
            .find_map(ast::ReturnType::cast)
            .and_then(|r| r.ty())
            .map(|t| self.resolve_type(file, scope, &t));
        let ret = match key {
            SigKey::Member(m) if self.p.member(m).kind == MemberKind::Init => {
                self.self_type(self.p.member(m).owner)
            }
            _ => ret.unwrap_or_else(|| builtin(self.b.void)),
        };
        Sig {
            generics: own,
            params,
            is_async,
            throws,
            ret,
        }
    }

    /// The type of `self` inside a declaration or extension.
    pub(crate) fn self_type(&self, decl: DeclId) -> Ty {
        let d = self.p.decl(decl);
        if d.kind == DeclKind::Extension {
            return self.ext_target(decl);
        }
        Ty::generic(decl, d.generics.iter().map(|g| Ty::Param(*g)).collect())
    }

    /// The stored fields of a struct-like declaration, in order.
    pub(crate) fn stored_fields(&self, decl: DeclId) -> Vec<MemberId> {
        let d = self.p.decl(decl);
        // A duplicated name (K2019) keeps its first field, as lookups do.
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        d.members
            .iter()
            .copied()
            .filter(|m| {
                let m = self.p.member(*m);
                m.kind == MemberKind::Field && !m.is_static && seen.insert(m.name.as_str())
            })
            .collect()
    }

    /// The declared initializers of a declaration.
    pub(crate) fn inits(&self, decl: DeclId) -> Vec<MemberId> {
        self.p
            .decl(decl)
            .members
            .iter()
            .copied()
            .filter(|m| self.p.member(*m).kind == MemberKind::Init)
            .collect()
    }

    // ---------------------------------------------------------- conformance

    /// The resolved `implements` entries of a declaration, with ranges.
    pub(crate) fn implements_list(&self, decl: DeclId) -> Implements {
        if let Some(list) = self.implements.borrow().get(&decl) {
            return Rc::clone(list);
        }
        self.implements
            .borrow_mut()
            .insert(decl, Rc::new(Vec::new()));
        let d = self.p.decl(decl);
        let mut out = Vec::new();
        if let Some(clause) = d.node.children().find_map(ast::ImplementsClause::cast) {
            for t in clause.types() {
                let ty = self.resolve_type(d.file, &d.generics, &ast::Type::PathType(t.clone()));
                out.push((ty, t.syntax().text_range()));
            }
        }
        let out = Rc::new(out);
        self.implements.borrow_mut().insert(decl, Rc::clone(&out));
        out
    }

    /// Returns `true` if declaration `decl` declares (directly or through
    /// interfaces it implements) interface `iface`.
    pub(crate) fn declares(&self, decl: DeclId, iface: DeclId) -> bool {
        let mut seen = HashSet::new();
        self.declares_inner(decl, iface, &mut seen)
    }

    /// The recursion of [`Cx::declares`]; `seen` stops cycles of `implements`
    /// clauses. Payload-less, non-generic enums declare `Equatable` and
    /// `Hashable` implicitly (sema.md §9.2).
    fn declares_inner(&self, decl: DeclId, iface: DeclId, seen: &mut HashSet<DeclId>) -> bool {
        if decl == iface {
            return true;
        }
        if (Some(iface) == self.b.equatable || Some(iface) == self.b.hashable)
            && self.is_enum(decl)
            && self.p.decl(decl).generics.is_empty()
            && self.cases(decl).iter().all(|c| !self.case_has_payload(*c))
        {
            // Enums without payloads compare by case (sema.md §9.2).
            return true;
        }
        if !seen.insert(decl) {
            return false;
        }
        let list = self.implements_list(decl);
        list.iter().any(|(t, _)| {
            t.decl()
                .is_some_and(|d| d == iface || self.declares_inner(d, iface, seen))
        }) || (Some(iface) == self.b.equatable
            && self
                .b
                .hashable
                .is_some_and(|h| h != decl && self.declares_inner(decl, h, seen)))
    }

    /// Returns `true` if values of `ty` implement interface `iface`.
    pub(crate) fn implements(&self, ty: &Ty, iface: DeclId) -> bool {
        let derived = [self.b.equatable, self.b.hashable, self.b.codable].contains(&Some(iface));
        match ty {
            Ty::Error | Ty::Never | Ty::Var(_) => true,
            Ty::Meta(_) | Ty::Func(_) => false,
            Ty::Tuple(items) => derived && items.iter().all(|t| self.implements(t, iface)),
            Ty::Optional(inner) => derived && self.implements(inner, iface),
            Ty::Param(g) => self
                .constraints(*g)
                .iter()
                .any(|c| c.decl().is_some_and(|d| self.declares(d, iface))),
            Ty::Named(d, args) => {
                if Some(iface) == self.b.view
                    && matches!(
                        self.p.decl(*d).named_kind(),
                        Some(
                            DeclarationKind::Component
                                | DeclarationKind::Page
                                | DeclarationKind::Layout
                        )
                    )
                {
                    // Components are values of `View` (sema.md §9.1).
                    return true;
                }
                let containers = [self.b.list, self.b.map, self.b.set];
                if derived && containers.contains(&Some(*d)) {
                    return args.iter().all(|a| self.implements(a, iface));
                }
                self.declares(*d, iface)
            }
        }
    }

    /// Returns `true` for declarations whose members the checker doesn't
    /// fully know yet (generated by the framework layer, sema.md §12):
    /// `@Generated` stdlib shells other than `Page` and `ThemeVariant`,
    /// repositories (generated queries and writes), and the static side of
    /// services, components and pages (`Job.kind`, generated factories).
    pub(crate) fn is_open(&self, decl: DeclId, statics: bool) -> bool {
        let d = self.p.decl(decl);
        if d.has_annotation("Generated") {
            return !self.is_closed_generated(decl) && (Some(decl) != self.b.page || !statics);
        }
        match d.named_kind() {
            Some(DeclarationKind::Repository) => true,
            Some(DeclarationKind::Component | DeclarationKind::Page | DeclarationKind::Layout) => {
                true
            }
            Some(
                DeclarationKind::Service | DeclarationKind::Protocol | DeclarationKind::Entity,
            ) => statics,
            _ => false,
        }
    }

    /// Returns `true` for `@Generated` declarations that stand for an app
    /// type (`SessionUserId`, `SessionRole`): any value converts to them.
    pub(crate) fn is_wildcard(&self, decl: DeclId) -> bool {
        let d = self.p.decl(decl);
        d.has_annotation("Generated")
            && Some(decl) != self.b.page
            && !self.is_closed_generated(decl)
    }

    /// Generated shells whose members the checker knows completely:
    /// `ThemeVariant`, `Endpoint`, and the session types (aliased to the
    /// app's own `AuthUserId`/`AuthRole` when it declares one).
    pub(crate) fn is_closed_generated(&self, decl: DeclId) -> bool {
        Some(decl) == self.b.theme_variant
            || [
                "k.core.Endpoint",
                "k.auth.SessionUserId",
                "k.auth.SessionRole",
            ]
            .iter()
            .any(|q| self.p.lookup_qualified(q) == Some(decl))
    }

    /// The app's own type a generated session type stands for:
    /// `SessionUserId` is the one struct implementing `AuthUserId`,
    /// `SessionRole` the one enum implementing `AuthRole` (sema.md §16).
    pub(crate) fn session_alias(&self, decl: DeclId) -> Option<DeclId> {
        let iface = if self.p.lookup_qualified("k.auth.SessionUserId") == Some(decl) {
            self.p.lookup_qualified("k.auth.AuthUserId")?
        } else if self.p.lookup_qualified("k.auth.SessionRole") == Some(decl) {
            self.p.lookup_qualified("k.auth.AuthRole")?
        } else {
            return None;
        };
        if let Some(found) = self.aliases.borrow().get(&decl) {
            return *found;
        }
        let mut found = None;
        let mut count = 0;
        for (i, d) in self.p.decls.iter().enumerate() {
            if self.p.file(d.file).stdlib || !d.is_type() {
                continue;
            }
            let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
            let declares = self.p.decl(id).node.children().any(|c| {
                c.kind() == SyntaxKind::ImplementsClause
                    && c.descendants_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .any(|t| t.text() == self.p.decl(iface).name)
            });
            if declares {
                count += 1;
                found = Some(id);
            }
        }
        let result = if count == 1 { found } else { None };
        self.aliases.borrow_mut().insert(decl, result);
        result
    }

    /// Returns `true` for interface declarations.
    pub(crate) fn is_interface(&self, decl: DeclId) -> bool {
        self.p.decl(decl).named_kind() == Some(DeclarationKind::Interface)
    }

    // ------------------------------------------------------- member lookup

    /// Looks up member `name` on a value of type `recv` (`statics: false`)
    /// or on the type itself (`statics: true`), as seen from `file` inside
    /// `body_owner` (for private members).
    pub(crate) fn lookup_member(
        &self,
        file: FileIx,
        body_owner: Option<DeclId>,
        recv: &Ty,
        name: &str,
        statics: bool,
    ) -> MemberFound {
        match recv {
            Ty::Named(d, args) => self.lookup_in_decl(file, body_owner, *d, args, name, statics),
            Ty::Optional(inner) if statics => {
                // `Optional` cases on `T?`.
                let _ = inner;
                match self.b.optional {
                    Some(o) => {
                        self.lookup_in_decl(file, body_owner, o, &[(**inner).clone()], name, true)
                    }
                    None => MemberFound::Missing,
                }
            }
            Ty::Param(g) => {
                for c in self.constraints(*g).iter() {
                    if let Ty::Named(d, args) = c {
                        let found = self.lookup_in_decl(file, body_owner, *d, args, name, statics);
                        if !matches!(found, MemberFound::Missing) {
                            return found;
                        }
                    }
                }
                MemberFound::Missing
            }
            _ => MemberFound::Missing,
        }
    }

    /// Looks the member `name` up in `decl` (instantiated with `args`) as seen
    /// from `body_owner`: its own members, then (for statics) its theme
    /// tokens, the interfaces it implements and its extensions whose `where`
    /// constraints the receiver meets.
    fn lookup_in_decl(
        &self,
        file: FileIx,
        body_owner: Option<DeclId>,
        decl: DeclId,
        args: &[Ty],
        name: &str,
        statics: bool,
    ) -> MemberFound {
        let d = self.p.decl(decl);
        let subst: Vec<(GenId, Ty)> = d
            .generics
            .iter()
            .copied()
            .zip(args.iter().cloned())
            .collect();
        if let Some(&m) = d.member_names.get(name) {
            return self.check_member(m, body_owner, statics, subst);
        }
        if statics {
            if let Some(tokens) = self.tokens.get(&decl) {
                if let Some(&t) = tokens.iter().find(|t| self.p.member(**t).name == name) {
                    return MemberFound::Token(t);
                }
            }
        }
        // Interfaces it implements (default methods and requirements).
        let mut seen = HashSet::new();
        if let Some(found) = self.lookup_in_interfaces(body_owner, decl, name, statics, &mut seen) {
            return found;
        }
        // Extensions.
        let exts = self
            .extensions_of
            .borrow()
            .get(&decl)
            .cloned()
            .unwrap_or_default();
        let mut invisible = None;
        for ext in exts {
            let e = self.p.decl(ext);
            let Some(&m) = e.member_names.get(name) else {
                continue;
            };
            if !self.extension_visible(ext, file) {
                invisible.get_or_insert(m);
                continue;
            }
            // Match the extension's target against the receiver.
            let target = self.ext_target(ext);
            let mut ext_subst = Vec::new();
            for (t, a) in target.args().iter().zip(args) {
                if let Ty::Param(g) = t {
                    ext_subst.push((*g, a.clone()));
                }
            }
            for (g, a) in &ext_subst {
                for c in self.constraints(*g).iter() {
                    if let Some(i) = c.decl() {
                        if !self.implements(a, i) {
                            return MemberFound::Unsatisfied(m);
                        }
                    }
                }
            }
            return self.check_member(m, body_owner, statics, ext_subst);
        }
        if let Some(m) = invisible {
            return MemberFound::NotVisible(m);
        }
        MemberFound::Missing
    }

    /// Looks the member `name` up in the interfaces `decl` implements,
    /// depth-first in declaration order; `seen` stops cycles.
    fn lookup_in_interfaces(
        &self,
        body_owner: Option<DeclId>,
        decl: DeclId,
        name: &str,
        statics: bool,
        seen: &mut HashSet<DeclId>,
    ) -> Option<MemberFound> {
        if !seen.insert(decl) {
            return None;
        }
        for (t, _) in self.implements_list(decl).iter() {
            let Some(i) = t.decl() else { continue };
            if !self.is_interface(i) {
                continue;
            }
            if let Some(&m) = self.p.decl(i).member_names.get(name) {
                return Some(self.check_member(m, body_owner, statics, Vec::new()));
            }
            if let Some(found) = self.lookup_in_interfaces(body_owner, i, name, statics, seen) {
                return Some(found);
            }
        }
        None
    }

    /// Classifies a found member for a lookup: initializers are not members,
    /// the static/instance kind must match ([`MemberFound::WrongKind`]) and a
    /// private or `@Inject` member is visible only from its own declaration.
    fn check_member(
        &self,
        m: MemberId,
        body_owner: Option<DeclId>,
        statics: bool,
        subst: Vec<(GenId, Ty)>,
    ) -> MemberFound {
        let member = self.p.member(m);
        if member.kind == MemberKind::Init {
            return MemberFound::Missing;
        }
        if member.is_static != statics {
            return MemberFound::WrongKind;
        }
        let private = member.visibility == Visibility::Private || member.has_annotation("Inject");
        if private && body_owner != Some(member.owner) {
            return MemberFound::NotVisible(m);
        }
        MemberFound::Member(m, subst)
    }

    /// Returns the type of a value member (field, computed property, case
    /// without payload, token) after substitution.
    pub(crate) fn member_value_ty(&self, m: MemberId, subst: &[(GenId, Ty)]) -> Ty {
        let member = self.p.member(m);
        let raw = match member.kind {
            MemberKind::Field if member.has_annotation("Form") => {
                let inner = self.field_ty(m);
                match self.p.lookup_qualified("k.validation.Form") {
                    Some(form) => Ty::generic(form, vec![inner]),
                    None => inner,
                }
            }
            MemberKind::Field
            | MemberKind::Computed
            | MemberKind::Token
            | MemberKind::Generated => self.field_ty(m),
            MemberKind::Case => self.self_type(member.owner),
            MemberKind::Func | MemberKind::Rpc => {
                let sig = self.sig(SigKey::Member(m));
                Ty::func(
                    sig.params.iter().map(|p| p.ty.clone()).collect(),
                    sig.is_async,
                    sig.throws.clone(),
                    sig.ret.clone(),
                )
            }
            MemberKind::Init => Ty::Error,
        };
        apply(&raw, subst)
    }

    /// The payload parameters of an enum case, as a signature.
    pub(crate) fn case_sig(&self, m: MemberId) -> Rc<Sig> {
        let key = SigKey::Member(m);
        if let Some(s) = self.sigs.borrow().get(&key) {
            return Rc::clone(s);
        }
        let member = self.p.member(m);
        let decl = self.p.decl(member.owner);
        let mut params = Vec::new();
        if let Some(&page) = self.p.page_of.get(&m) {
            // A page case's payload: the page's `@Param`s; those with a
            // default (query parameters) may be omitted or chained.
            for &f in &self.p.decl(page).members {
                let field = self.p.member(f);
                if field.kind != MemberKind::Field || !field.has_annotation("Param") {
                    continue;
                }
                let ty = self.field_ty(f);
                params.push(SigParam {
                    label: Some(field.name.clone()),
                    name: field.name.clone(),
                    has_default: field.has_default || matches!(ty, Ty::Optional(_)),
                    ty,
                    name_range: field.name_range,
                });
            }
        }
        if let Some(node) = &member.node {
            for field in node
                .children()
                .filter(|c| c.kind() == SyntaxKind::CaseField)
            {
                let label = field
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident)
                    .filter(|_| has_token(&field, SyntaxKind::Colon));
                let ty = field
                    .children()
                    .find_map(ast::Type::cast)
                    .map_or(Ty::Error, |t| {
                        self.resolve_type(decl.file, &decl.generics, &t)
                    });
                params.push(SigParam {
                    label: label.as_ref().map(|l| l.text().to_owned()),
                    name: label
                        .as_ref()
                        .map(|l| l.text().to_owned())
                        .unwrap_or_default(),
                    ty,
                    has_default: false,
                    name_range: label.map_or_else(|| field.text_range(), |l| l.text_range()),
                });
            }
        }
        let sig = Rc::new(Sig {
            generics: Vec::new(),
            params,
            is_async: false,
            throws: None,
            ret: self.self_type(member.owner),
        });
        self.sigs.borrow_mut().insert(key, Rc::clone(&sig));
        sig
    }

    /// Returns `true` if the enum case has a payload.
    pub(crate) fn case_has_payload(&self, m: MemberId) -> bool {
        if self.p.page_of.contains_key(&m) {
            return self.case_sig(m).params.iter().any(|p| !p.has_default);
        }
        self.p
            .member(m)
            .node
            .as_ref()
            .is_some_and(|n| has_token(n, SyntaxKind::LParen))
    }

    /// The cases of an enum, in order.
    pub(crate) fn cases(&self, decl: DeclId) -> Vec<MemberId> {
        // A duplicated case name (K2019) keeps its first case.
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        self.p
            .decl(decl)
            .members
            .iter()
            .copied()
            .filter(|m| {
                let m = self.p.member(*m);
                m.kind == MemberKind::Case && seen.insert(m.name.as_str())
            })
            .collect()
    }

    /// Returns `true` for enum declarations.
    pub(crate) fn is_enum(&self, decl: DeclId) -> bool {
        self.p.decl(decl).named_kind() == Some(DeclarationKind::Enum)
    }

    /// Returns `true` for class declarations.
    pub(crate) fn is_class(&self, decl: DeclId) -> bool {
        matches!(
            self.p.decl(decl).named_kind(),
            Some(
                DeclarationKind::Class
                    | DeclarationKind::Service
                    | DeclarationKind::Component
                    | DeclarationKind::Page
                    | DeclarationKind::Layout
                    | DeclarationKind::Repository
                    | DeclarationKind::Protocol
            )
        )
    }

    /// The infer hook for top-level constants.
    pub(crate) fn set_hooks(&self, field: FieldHook<'p, 'a>, constant: ConstantHook<'p, 'a>) {
        *self.infer_field.borrow_mut() = Some(field);
        *self.infer_constant.borrow_mut() = Some(constant);
    }
}

/// Applies a substitution of generic parameters.
pub(crate) fn apply(ty: &Ty, subst: &[(GenId, Ty)]) -> Ty {
    if subst.is_empty() {
        return ty.clone();
    }
    ty.subst(&|g| subst.iter().find(|(p, _)| *p == g).map(|(_, t)| t.clone()))
}

/// Returns `true` if `node` has a direct child token of `kind`.
pub(crate) fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    ast::support::has_token(node, kind)
}

/// Returns the direct child token of `kind`.
pub(crate) fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == kind)
}

/// The labels of a function node's parameters (`None` for `_`).
pub(crate) fn param_list_labels(node: &SyntaxNode) -> Vec<Option<String>> {
    let Some(list) = node.children().find_map(ast::ParamList::cast) else {
        return Vec::new();
    };
    list.params()
        .map(|p| match p.label() {
            Some(l) if l.kind() == SyntaxKind::Underscore => None,
            Some(l) => Some(l.text().to_owned()),
            None => p.name().map(|n| n.text().to_owned()),
        })
        .collect()
}

/// Returns where an import inserted before the first item (which starts
/// at `offset`) goes; currently `offset` itself.
fn item_start(_root: &SyntaxNode, offset: TextSize) -> TextSize {
    offset
}

/// Converts a byte offset to a [`TextSize`], saturating at `u32::MAX`.
fn to_size(n: usize) -> TextSize {
    TextSize::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The optimal-string-alignment distance of two strings, in characters:
/// Levenshtein plus adjacent transpositions.
///
/// Strings whose lengths differ by more than 3 give `usize::MAX` without
/// being compared (callers only accept small distances). O(|a|·|b|) time,
/// O(|b|) space.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > 3 {
        return usize::MAX;
    }
    // Optimal string alignment: insertions, deletions, substitutions and
    // transpositions of adjacent characters (`titel` → `title`) cost 1.
    let mut before: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            let mut best = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
            if i > 0 && j > 0 && *ca == b[j - 1] && a[i - 1] == *cb {
                best = best.min(before[j - 1] + 1);
            }
            cur[j + 1] = best;
        }
        std::mem::swap(&mut before, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// The closest of `candidates` to `name`, if close enough to be a typo.
///
/// A candidate equal to `name` is skipped: replacing a name with itself
/// fixes nothing, and when the name exists but doesn't fit (a static
/// member of the wrong type, an alternative label already passed) the
/// diagnostic explains that instead.
pub(crate) fn closest<'s>(
    name: &str,
    candidates: impl Iterator<Item = &'s str>,
) -> Option<&'s str> {
    let limit = (name.chars().count() / 3).max(1);
    let mut best: Option<(usize, &str)> = None;
    for c in candidates {
        if c == name {
            continue;
        }
        let d = edit_distance(name, c);
        if d <= limit && best.is_none_or(|(bd, bn)| d < bd || (d == bd && c < bn)) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}
