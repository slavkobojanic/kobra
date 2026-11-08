//! The declarations of one analysed project: files, modules, top-level
//! declarations and their members (sema.md §7.1, "Item tree").
//!
//! [`Program::build`] walks the syntax trees of every stdlib and project file
//! once and records, for every declaration, its kind, name, visibility,
//! generic parameters and members, with the syntax node they come from. The
//! result is the input of name lookup and signature resolution
//! ([`crate::cx`]) and of body checking ([`crate::body`]).
//!
//! # Invariants
//!
//! * Ids are dense indices: `DeclId(n)` is `program.decls[n]`, and likewise
//!   for [`MemberId`], [`ModId`] and [`FileIx`].
//! * Stdlib files come first, in [`Stdlib::files`] order, so their
//!   declarations have the smallest ids.
//! * Nothing here reports diagnostics; the checks over declarations live in
//!   [`crate::check`].

use crate::fx::FxHashMap;

use k_diagnostics::{FileId, Span, TextRange};
use k_syntax::ast::{self, AstNode, HasMemberList, HasName, Visibility};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::project::{FileKind, ProjectPath};
use crate::stdlib::{CORE_MODULE, DeclarationKind, INTERNAL_MODULE, PRELUDE_REEXPORTS, Stdlib};

/// Index of a file in [`Program::files`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub(crate) struct FileIx(pub(crate) u32);

/// Index of a module in [`Program::modules`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub(crate) struct ModId(pub(crate) u32);

/// Index of a top-level declaration (or extension) in [`Program::decls`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub(crate) struct DeclId(pub(crate) u32);

/// Index of a member in [`Program::members`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub(crate) struct MemberId(pub(crate) u32);

/// Index of a generic parameter in [`Program::generics`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub(crate) struct GenId(pub(crate) u32);

/// One import of a file.
#[derive(Clone, Debug)]
pub(crate) struct ImportInfo {
    /// The dotted path as written (`k.time.Clock`).
    pub(crate) path: String,
    /// The range of the path, from its first segment to its last.
    pub(crate) range: TextRange,
    /// The range of the last segment.
    pub(crate) last_range: TextRange,
    /// The range of the whole `import` line, for removal fixes.
    pub(crate) node_range: TextRange,
    /// `false` when the path contains characters other than identifiers and
    /// dots (`github.com/acme/x`, a broken path): such imports are packages
    /// or syntax errors and are not resolved.
    pub(crate) plain: bool,
}

/// A source file of the analysis.
pub(crate) struct FileInfo<'a> {
    /// The host's file id.
    pub(crate) id: FileId,
    /// The file's text.
    pub(crate) text: &'a str,
    /// The syntax tree's root.
    pub(crate) root: SyntaxNode,
    /// The module of a stdlib or `src/` file; `None` for `App.k`,
    /// migrations and stray files, which form a namespace of their own.
    pub(crate) module: Option<ModId>,
    /// `true` for `*.test.k` files.
    pub(crate) is_test: bool,
    /// `true` for stdlib files.
    pub(crate) stdlib: bool,
    /// The project path (`None` for stdlib files).
    pub(crate) path: Option<&'a ProjectPath>,
    /// The file's imports, in source order.
    pub(crate) imports: Vec<ImportInfo>,
    /// The declarations of the file, in source order.
    pub(crate) decls: Vec<DeclId>,
}

/// A module: one directory of `src/` or of the stdlib.
pub(crate) struct Module {
    /// The dotted path (`shop.cart`, `k.http`).
    pub(crate) path: String,
    /// `true` for test-only modules (a `testing` segment, or `k.test`).
    pub(crate) test_only: bool,
    /// `true` for stdlib modules.
    pub(crate) stdlib: bool,
    /// The files of the module.
    pub(crate) files: Vec<FileIx>,
    /// The named declarations of the module's non-test files, by name.
    pub(crate) names: FxHashMap<String, Vec<DeclId>>,
    /// The named declarations of the module's test files, by name.
    pub(crate) test_names: FxHashMap<String, Vec<DeclId>>,
}

/// What kind of declaration a [`Decl`] is: a named declaration kind, or an
/// extension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DeclKind {
    /// A named top-level declaration.
    Named(DeclarationKind),
    /// `extend T { … }`.
    Extension,
}

/// A top-level declaration or extension.
pub(crate) struct Decl {
    /// Its kind.
    pub(crate) kind: DeclKind,
    /// The declared name (empty for extensions and unnamed items).
    pub(crate) name: String,
    /// The file that declares it.
    pub(crate) file: FileIx,
    /// The written visibility.
    pub(crate) visibility: Visibility,
    /// The item's syntax node.
    pub(crate) node: SyntaxNode,
    /// The name token's range (the `extend` keyword's for extensions).
    pub(crate) name_range: TextRange,
    /// Generic parameters, in order.
    pub(crate) generics: Vec<GenId>,
    /// The members, in source order (enum cases included).
    pub(crate) members: Vec<MemberId>,
    /// The members by name: the first non-function member wins over
    /// functions, so a property and a method may share a name.
    pub(crate) member_names: FxHashMap<String, MemberId>,
    /// Every function member by name, in order (overloads).
    pub(crate) funcs: FxHashMap<String, Vec<MemberId>>,
    /// The names of the annotations on the declaration.
    pub(crate) annotations: Vec<String>,
}

impl Decl {
    /// Returns the declaration kind, `None` for extensions.
    pub(crate) fn named_kind(&self) -> Option<DeclarationKind> {
        match self.kind {
            DeclKind::Named(kind) => Some(kind),
            DeclKind::Extension => None,
        }
    }

    /// Returns `true` if the declaration carries annotation `name`.
    pub(crate) fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a == name)
    }

    /// Returns `true` for declarations that are types (usable in type
    /// position).
    pub(crate) fn is_type(&self) -> bool {
        !matches!(
            self.named_kind(),
            None | Some(
                DeclarationKind::Function
                    | DeclarationKind::Constant
                    | DeclarationKind::TestSuite
                    | DeclarationKind::Migration
                    | DeclarationKind::App
            )
        )
    }
}

/// The kind of a member.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MemberKind {
    /// A stored field (`let`/`var` with optional default).
    Field,
    /// A computed property (`var x: T { … }`), or a field without storage
    /// in a stdlib `@Native` type.
    Computed,
    /// A function (`func`).
    Func,
    /// An initializer.
    Init,
    /// An enum case.
    Case,
    /// An rpc.
    Rpc,
    /// A theme token (`color primary: Color = …`).
    Token,
    /// A generated member: a page case, a theme variant, `copy(…)`.
    Generated,
}

/// A member of a declaration or extension.
pub(crate) struct Member {
    /// The declaration or extension that declares it.
    pub(crate) owner: DeclId,
    /// The member's name.
    pub(crate) name: String,
    /// Its kind.
    pub(crate) kind: MemberKind,
    /// `true` for members reached through the type rather than an instance:
    /// `static` members, enum cases, `init`, theme tokens and every
    /// generated member except `copy(…)`.
    pub(crate) is_static: bool,
    /// `true` for `var` fields.
    pub(crate) is_var: bool,
    /// `true` for fields written with an initializer (`let x = 1`) and for
    /// theme tokens, whose value is their default. An optional field without
    /// an initializer is `false` here; callers that treat it as omittable
    /// check its type as well.
    pub(crate) has_default: bool,
    /// The written visibility.
    pub(crate) visibility: Visibility,
    /// The member's syntax node (`None` for generated members).
    pub(crate) node: Option<SyntaxNode>,
    /// The name token's range.
    pub(crate) name_range: TextRange,
    /// The `let`/`var` keyword's range of a field.
    pub(crate) keyword_range: Option<TextRange>,
    /// Generic parameters of a function.
    pub(crate) generics: Vec<GenId>,
    /// The names of the member's annotations.
    pub(crate) annotations: Vec<String>,
    /// For theme tokens: the category word (`color`).
    pub(crate) category: Option<String>,
}

impl Member {
    /// Returns `true` if the member carries annotation `name`.
    pub(crate) fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a == name)
    }
}

/// Who declares a generic parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GenOwner {
    /// A type or extension.
    Decl(DeclId),
    /// A function member.
    Member(MemberId),
}

/// A generic parameter.
pub(crate) struct Generic {
    /// The parameter's name.
    pub(crate) name: String,
    /// Its owner.
    pub(crate) owner: GenOwner,
}

/// Every file, module, declaration and member of an analysis.
pub(crate) struct Program<'a> {
    /// The files: stdlib first, then project K sources sorted by path.
    pub(crate) files: Vec<FileInfo<'a>>,
    /// The modules, stdlib first.
    pub(crate) modules: Vec<Module>,
    /// Modules by dotted path.
    pub(crate) module_by_path: FxHashMap<String, ModId>,
    /// Every declaration and extension.
    pub(crate) decls: Vec<Decl>,
    /// Every member.
    pub(crate) members: Vec<Member>,
    /// Every generic parameter.
    pub(crate) generics: Vec<Generic>,
    /// The prelude: simple name → declaration.
    pub(crate) prelude: FxHashMap<String, DeclId>,
    /// The stdlib-internal module's names, visible to stdlib files.
    pub(crate) internal: FxHashMap<String, DeclId>,
    /// Fully qualified name → declaration, for public declarations.
    pub(crate) qualified: FxHashMap<String, DeclId>,
    /// Extensions, in declaration order.
    pub(crate) extensions: Vec<DeclId>,
    /// File index by host file id.
    pub(crate) file_by_id: FxHashMap<FileId, FileIx>,
    /// The page each generated `Page` or `Endpoint` case stands for.
    pub(crate) page_of: FxHashMap<MemberId, DeclId>,
    /// The project's lockfiles, for the lockfile checks (`crate::locks`).
    pub(crate) lockfiles: Vec<crate::locks::Lockfile>,
    /// The `k.toml` texts of the project and its mounted libraries, by
    /// project path (`k.toml`, `.deps/pay/k.toml`).
    pub(crate) manifests: Vec<(String, std::sync::Arc<str>)>,
}

/// The project files given to [`Program::build`]: id, path, text and root.
pub(crate) struct InputFile<'a> {
    /// The host's id.
    pub(crate) id: FileId,
    /// The project path.
    pub(crate) path: &'a ProjectPath,
    /// The text.
    pub(crate) text: &'a str,
    /// The same text as the host shares it: a warm re-check recognizes an
    /// unchanged file by this `Arc` (`crate::warm`).
    pub(crate) shared: &'a std::sync::Arc<str>,
    /// The parse's root node.
    pub(crate) root: SyntaxNode,
}

impl<'a> Program<'a> {
    /// Collects the declarations of the stdlib and of `inputs`.
    pub(crate) fn build(inputs: Vec<InputFile<'a>>) -> Program<'a> {
        let mut program = Program {
            files: Vec::new(),
            modules: Vec::new(),
            module_by_path: FxHashMap::default(),
            decls: Vec::new(),
            members: Vec::new(),
            generics: Vec::new(),
            prelude: FxHashMap::default(),
            internal: FxHashMap::default(),
            qualified: FxHashMap::default(),
            extensions: Vec::new(),
            file_by_id: FxHashMap::default(),
            page_of: FxHashMap::default(),
            lockfiles: Vec::new(),
            manifests: Vec::new(),
        };
        let stdlib = Stdlib::get();
        for (index, file) in stdlib.files().iter().enumerate() {
            let module = program.module(file.module.as_str(), true, false);
            let id = FileId::new(u32::try_from(index).unwrap_or(u32::MAX));
            program.add_file(
                id,
                file.text,
                file.parse.syntax(),
                Some(module),
                false,
                true,
                None,
            );
        }
        let mut inputs = inputs;
        inputs.sort_by(|a, b| a.path.cmp(b.path));
        for input in inputs {
            let (module, is_test) = match input.path.kind() {
                FileKind::Source { module, is_test } => {
                    let test_only = module.is_test_only();
                    (
                        Some(program.module(module.as_str(), false, test_only)),
                        is_test,
                    )
                }
                _ => (None, false),
            };
            program.add_file(
                input.id,
                input.text,
                input.root,
                module,
                is_test,
                false,
                Some(input.path),
            );
        }
        program.build_prelude();
        program
    }

    /// Returns the module with dotted `path`, creating it on first use.
    ///
    /// `k.test` and its submodules are always test-only; `stdlib` and
    /// `test_only` are taken from the first call for a path.
    fn module(&mut self, path: &str, stdlib: bool, test_only: bool) -> ModId {
        if let Some(&id) = self.module_by_path.get(path) {
            return id;
        }
        let id = ModId(u32::try_from(self.modules.len()).unwrap_or(u32::MAX));
        self.modules.push(Module {
            path: path.to_owned(),
            test_only: test_only || path == "k.test" || path.starts_with("k.test."),
            stdlib,
            files: Vec::new(),
            names: FxHashMap::default(),
            test_names: FxHashMap::default(),
        });
        self.module_by_path.insert(path.to_owned(), id);
        id
    }

    /// Adds a parsed file (stdlib or project) and its top-level declarations.
    ///
    /// Each named declaration goes into its module's names (test files' into
    /// the test names), public non-test ones also into the qualified-name
    /// table (the first wins); extensions are listed separately.
    #[allow(clippy::too_many_arguments)]
    fn add_file(
        &mut self,
        id: FileId,
        text: &'a str,
        root: SyntaxNode,
        module: Option<ModId>,
        is_test: bool,
        stdlib: bool,
        path: Option<&'a ProjectPath>,
    ) {
        let ix = FileIx(u32::try_from(self.files.len()).unwrap_or(u32::MAX));
        let imports = collect_imports(&root);
        self.files.push(FileInfo {
            id,
            text,
            root: root.clone(),
            module,
            is_test,
            stdlib,
            path,
            imports,
            decls: Vec::new(),
        });
        self.file_by_id.insert(id, ix);
        if let Some(module) = module {
            self.modules[module.0 as usize].files.push(ix);
        }
        let Some(source) = ast::SourceFile::cast(root) else {
            return;
        };
        for item in source.items() {
            let decl = self.add_item(ix, &item);
            self.files[ix.0 as usize].decls.push(decl);
            let info = &self.decls[decl.0 as usize];
            if matches!(info.kind, DeclKind::Extension) {
                self.extensions.push(decl);
                continue;
            }
            if info.name.is_empty() || info.named_kind() == Some(DeclarationKind::TestSuite) {
                continue;
            }
            if let Some(module) = module {
                let name = info.name.clone();
                let public = info.visibility == Visibility::Public;
                let m = &mut self.modules[module.0 as usize];
                if is_test {
                    m.test_names.entry(name.clone()).or_default().push(decl);
                } else {
                    m.names.entry(name.clone()).or_default().push(decl);
                }
                if public && !is_test {
                    let qualified = format!("{}.{}", m.path, name);
                    self.qualified.entry(qualified).or_insert(decl);
                }
            }
        }
    }

    /// Adds one top-level item of `file` as a declaration, with its
    /// annotations, generic parameters and members; returns its id.
    ///
    /// An `extend` block becomes an [`DeclKind::Extension`] and a top-level
    /// test case a nameless test suite.
    fn add_item(&mut self, file: FileIx, item: &ast::Item) -> DeclId {
        let id = DeclId(u32::try_from(self.decls.len()).unwrap_or(u32::MAX));
        let node = item.syntax().clone();
        let (kind, name_token) = match DeclarationKind::of(item) {
            Some(kind) => (DeclKind::Named(kind), item_name(item)),
            None => (
                match item {
                    ast::Item::ExtendDecl(_) => DeclKind::Extension,
                    _ => DeclKind::Named(DeclarationKind::TestSuite),
                },
                None,
            ),
        };
        let name = name_token
            .as_ref()
            .map(|t| t.text().to_owned())
            .unwrap_or_default();
        let name_range = name_token.map_or_else(
            || {
                node.children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| matches!(t.kind(), SyntaxKind::ExtendKw | SyntaxKind::TestKw))
                    .map_or_else(|| first_token_range(&node), |t| t.text_range())
            },
            |t| t.text_range(),
        );
        let visibility = visibility_of(&node);
        let annotations = annotation_names(&node);
        self.decls.push(Decl {
            kind,
            name,
            file,
            visibility,
            node: node.clone(),
            name_range,
            generics: Vec::new(),
            members: Vec::new(),
            member_names: FxHashMap::default(),
            funcs: FxHashMap::default(),
            annotations,
        });
        let generics = generic_names(&node);
        let generics = generics
            .into_iter()
            .map(|g| self.add_generic(g, GenOwner::Decl(id)))
            .collect();
        self.decls[id.0 as usize].generics = generics;
        // A top-level function's generics belong to the declaration.
        if let Some(list) = member_list(item) {
            for member in list.members() {
                self.add_member(id, &member);
            }
        }
        id
    }

    /// Adds a generic parameter called `name`, declared by `owner`.
    fn add_generic(&mut self, name: String, owner: GenOwner) -> GenId {
        let id = GenId(u32::try_from(self.generics.len()).unwrap_or(u32::MAX));
        self.generics.push(Generic { name, owner });
        id
    }

    /// Adds one member of `owner` (field, computed property, function,
    /// initializer, case, rpc, token …) from its syntax.
    ///
    /// Functions are also grouped by name in [`Decl::funcs`] (overloads). In
    /// the member-name table a function keeps the first entry of its name, any
    /// other member (not an initializer) replaces a function of the same name.
    fn add_member(&mut self, owner: DeclId, member: &ast::Member) {
        let node = member.syntax().clone();
        let (kind, name_token, is_static, is_var, has_default, keyword, category) = match member {
            ast::Member::FieldDecl(f) => {
                let computed = f.getter().is_some();
                let kind = if computed {
                    MemberKind::Computed
                } else {
                    MemberKind::Field
                };
                (
                    kind,
                    f.name(),
                    f.is_static(),
                    f.is_var(),
                    f.initializer().is_some(),
                    f.binding_keyword().map(|t| t.text_range()),
                    None,
                )
            }
            ast::Member::FuncDecl(f) => (
                MemberKind::Func,
                f.name(),
                f.is_static(),
                false,
                false,
                None,
                None,
            ),
            ast::Member::InitDecl(_) => {
                let token = node
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::InitKw);
                (MemberKind::Init, token, true, false, false, None, None)
            }
            ast::Member::EnumCase(c) => {
                (MemberKind::Case, c.name(), true, false, false, None, None)
            }
            ast::Member::RpcDecl(r) => (MemberKind::Rpc, r.name(), false, false, false, None, None),
            ast::Member::ThemeToken(_) => {
                let idents: Vec<SyntaxToken> = node
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|t| t.kind() == SyntaxKind::Ident)
                    .collect();
                let category = idents.first().map(|t| t.text().to_owned());
                (
                    MemberKind::Token,
                    idents.get(1).cloned(),
                    true,
                    false,
                    true,
                    None,
                    category,
                )
            }
            _ => return,
        };
        let Some(name_token) = name_token else {
            return;
        };
        let name = if kind == MemberKind::Init {
            "init".to_owned()
        } else {
            name_token.text().to_owned()
        };
        let id = MemberId(u32::try_from(self.members.len()).unwrap_or(u32::MAX));
        let generics = if let ast::Member::FuncDecl(_) = member {
            generic_names(&node)
        } else {
            Vec::new()
        };
        self.members.push(Member {
            owner,
            name: name.clone(),
            kind,
            is_static,
            is_var,
            has_default,
            visibility: visibility_of(&node),
            node: Some(node.clone()),
            name_range: name_token.text_range(),
            keyword_range: keyword,
            generics: Vec::new(),
            annotations: annotation_names(&node),
            category,
        });
        let generics = generics
            .into_iter()
            .map(|g| self.add_generic(g, GenOwner::Member(id)))
            .collect();
        self.members[id.0 as usize].generics = generics;
        let decl = &mut self.decls[owner.0 as usize];
        decl.members.push(id);
        if kind == MemberKind::Func {
            decl.funcs.entry(name.clone()).or_default().push(id);
            decl.member_names.entry(name).or_insert(id);
        } else if kind != MemberKind::Init {
            let previous = decl.member_names.get(&name).copied();
            let replace =
                previous.is_none_or(|p| self.members[p.0 as usize].kind == MemberKind::Func);
            if replace {
                decl.member_names.insert(name, id);
            }
        }
    }

    /// Adds a generated member to `owner` and returns its id.
    pub(crate) fn add_generated(
        &mut self,
        owner: DeclId,
        name: &str,
        kind: MemberKind,
        name_range: TextRange,
    ) -> MemberId {
        let id = MemberId(u32::try_from(self.members.len()).unwrap_or(u32::MAX));
        self.members.push(Member {
            owner,
            name: name.to_owned(),
            kind,
            is_static: kind != MemberKind::Generated || name != "copy",
            is_var: false,
            has_default: false,
            visibility: Visibility::Public,
            node: None,
            name_range,
            keyword_range: None,
            generics: Vec::new(),
            annotations: Vec::new(),
            category: None,
        });
        let decl = &mut self.decls[owner.0 as usize];
        decl.members.push(id);
        decl.member_names.entry(name.to_owned()).or_insert(id);
        id
    }

    /// Builds the prelude: the public declarations of `k.core`, then the
    /// [`PRELUDE_REEXPORTS`] from other stdlib modules.
    fn build_prelude(&mut self) {
        if let Some(&core) = self.module_by_path.get(CORE_MODULE) {
            for (name, decls) in &self.modules[core.0 as usize].names {
                if let Some(&d) = decls.first() {
                    if self.decls[d.0 as usize].visibility == Visibility::Public {
                        self.prelude.insert(name.clone(), d);
                    }
                }
            }
        }
        for qualified in PRELUDE_REEXPORTS {
            if let Some(&d) = self.qualified.get(*qualified) {
                self.prelude
                    .insert(self.decls[d.0 as usize].name.clone(), d);
            }
        }
        if let Some(&internal) = self.module_by_path.get(INTERNAL_MODULE) {
            for (name, decls) in &self.modules[internal.0 as usize].names {
                if let Some(&d) = decls.first() {
                    self.internal.insert(name.clone(), d);
                }
            }
        }
    }

    /// Returns the declaration `id`.
    pub(crate) fn decl(&self, id: DeclId) -> &Decl {
        &self.decls[id.0 as usize]
    }

    /// Returns the member `id`.
    pub(crate) fn member(&self, id: MemberId) -> &Member {
        &self.members[id.0 as usize]
    }

    /// Returns the file `ix`.
    pub(crate) fn file(&self, ix: FileIx) -> &FileInfo<'a> {
        &self.files[ix.0 as usize]
    }

    /// Returns the span of `range` in the file of `ix`.
    pub(crate) fn span(&self, ix: FileIx, range: TextRange) -> Span {
        Span::new(self.files[ix.0 as usize].id, range)
    }

    /// Returns the module of a declaration, if it has one.
    pub(crate) fn decl_module(&self, id: DeclId) -> Option<ModId> {
        self.file(self.decl(id).file).module
    }

    /// Returns `true` for stdlib declarations.
    pub(crate) fn is_stdlib(&self, id: DeclId) -> bool {
        self.file(self.decl(id).file).stdlib
    }

    /// Renders a declaration's name: prelude declarations by simple name,
    /// every other one fully qualified (sema.md §2).
    pub(crate) fn decl_name(&self, id: DeclId) -> String {
        let mut out = String::new();
        self.write_decl_name(id, &mut out);
        out
    }

    /// Appends [`decl_name`](Program::decl_name) to `out` without
    /// allocating a temporary string (type rendering calls this for every
    /// nominal type it writes).
    pub(crate) fn write_decl_name(&self, id: DeclId, out: &mut String) {
        let decl = self.decl(id);
        if self.prelude.get(&decl.name) != Some(&id) {
            if let Some(m) = self.decl_module(id) {
                out.push_str(&self.modules[m.0 as usize].path);
                out.push('.');
            }
        }
        out.push_str(&decl.name);
    }

    /// Looks up a stdlib declaration by qualified name.
    pub(crate) fn lookup_qualified(&self, name: &str) -> Option<DeclId> {
        self.qualified.get(name).copied()
    }
}

/// Returns the first non-trivia token's range of a node.
pub(crate) fn first_token_range(node: &SyntaxNode) -> TextRange {
    node.descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| !t.kind().is_trivia())
        .map_or_else(|| node.text_range(), |t| t.text_range())
}

/// Returns the written visibility of a declaration or member node.
pub(crate) fn visibility_of(node: &SyntaxNode) -> Visibility {
    // Reads the green tree: no red tokens are allocated.
    match ast::support::token_kind_where(node, |k| {
        matches!(k, SyntaxKind::PublicKw | SyntaxKind::PrivateKw)
    }) {
        Some(SyntaxKind::PublicKw) => Visibility::Public,
        Some(SyntaxKind::PrivateKw) => Visibility::Private,
        _ => Visibility::Module,
    }
}

/// Returns the `public` or `private` token of a node.
pub(crate) fn visibility_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| matches!(t.kind(), SyntaxKind::PublicKw | SyntaxKind::PrivateKw))
}

/// The names of a node's annotations (`State` for `@State`), as
/// `ast::Annotation::name` spells them, read from the green tree so no red
/// nodes are allocated (the item tree is rebuilt on every revision).
fn annotation_names(node: &SyntaxNode) -> Vec<String> {
    let raw = |k: SyntaxKind| <k_syntax::KLanguage as rowan::Language>::kind_to_raw(k);
    let (annotation, ident) = (raw(SyntaxKind::Annotation), raw(SyntaxKind::Ident));
    node.green()
        .children()
        .filter_map(rowan::NodeOrToken::into_node)
        .filter(|n| n.kind() == annotation)
        .filter_map(|a| {
            a.children()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == ident)
                .map(|t| t.text().to_owned())
        })
        .collect()
}

/// Returns the names of a declaration's generic parameters, in order.
fn generic_names(node: &SyntaxNode) -> Vec<String> {
    node.children()
        .find(|c| c.kind() == SyntaxKind::GenericParamList)
        .map(|list| {
            list.children()
                .filter_map(ast::GenericParam::cast)
                .filter_map(|g| g.name_text())
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the member list of a declaration item (extensions included);
/// `None` for top-level functions, constants and test cases, which have
/// none.
fn member_list(item: &ast::Item) -> Option<ast::MemberList> {
    match item {
        ast::Item::ComponentDecl(d) => d.member_list(),
        ast::Item::PageDecl(d) => d.member_list(),
        ast::Item::LayoutDecl(d) => d.member_list(),
        ast::Item::ServiceDecl(d) => d.member_list(),
        ast::Item::ProtocolDecl(d) => d.member_list(),
        ast::Item::EntityDecl(d) => d.member_list(),
        ast::Item::RepositoryDecl(d) => d.member_list(),
        ast::Item::StructDecl(d) => d.member_list(),
        ast::Item::EnumDecl(d) => d.member_list(),
        ast::Item::ClassDecl(d) => d.member_list(),
        ast::Item::InterfaceDecl(d) => d.member_list(),
        ast::Item::ThemeDecl(d) => d.member_list(),
        ast::Item::TestSuiteDecl(d) => d.member_list(),
        ast::Item::AppDecl(d) => d.member_list(),
        ast::Item::ConfigDecl(d) => d.member_list(),
        ast::Item::MigrationDecl(d) => d.member_list(),
        ast::Item::ExtendDecl(d) => d.member_list(),
        ast::Item::FuncDecl(_) | ast::Item::FieldDecl(_) | ast::Item::TestCaseDecl(_) => None,
    }
}

/// Returns the name token of a named item.
pub(crate) fn item_name(item: &ast::Item) -> Option<SyntaxToken> {
    match item {
        ast::Item::ComponentDecl(d) => d.name(),
        ast::Item::PageDecl(d) => d.name(),
        ast::Item::LayoutDecl(d) => d.name(),
        ast::Item::ServiceDecl(d) => d.name(),
        ast::Item::ProtocolDecl(d) => d.name(),
        ast::Item::EntityDecl(d) => d.name(),
        ast::Item::RepositoryDecl(d) => d.name(),
        ast::Item::StructDecl(d) => d.name(),
        ast::Item::EnumDecl(d) => d.name(),
        ast::Item::ClassDecl(d) => d.name(),
        ast::Item::InterfaceDecl(d) => d.name(),
        ast::Item::ThemeDecl(d) => d.name(),
        ast::Item::AppDecl(d) => d.name(),
        ast::Item::ConfigDecl(d) => d.name(),
        ast::Item::MigrationDecl(d) => d.name(),
        ast::Item::FuncDecl(d) => d.name(),
        ast::Item::FieldDecl(d) => d.name(),
        ast::Item::TestSuiteDecl(d) => d.name(),
        ast::Item::ExtendDecl(_) | ast::Item::TestCaseDecl(_) => None,
    }
}

/// Collects the imports of a file, in order, with their ranges and
/// whether each path is plain (identifiers and dots only).
fn collect_imports(root: &SyntaxNode) -> Vec<ImportInfo> {
    let mut imports = Vec::new();
    for import in root.children().filter_map(ast::Import::cast) {
        let Some(path) = import.path() else {
            continue;
        };
        let tokens: Vec<SyntaxToken> = path
            .syntax()
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| !t.kind().is_trivia())
            .collect();
        let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
            continue;
        };
        let plain = tokens.iter().enumerate().all(|(i, t)| {
            if i % 2 == 1 {
                t.kind() == SyntaxKind::Dot
            } else {
                t.text()
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && t.kind() != SyntaxKind::Dot
            }
        }) && tokens.len() % 2 == 1
            && !import
                .syntax()
                .descendants()
                .any(|n| n.kind() == SyntaxKind::ErrorNode);
        let range = TextRange::new(first.text_range().start(), last.text_range().end());
        imports.push(ImportInfo {
            path: path.text(),
            range,
            last_range: last.text_range(),
            node_range: import.syntax().text_range(),
            plain,
        });
    }
    imports
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The red-tree spelling `annotation_names` replaced.
    fn red_annotation_names(node: &SyntaxNode) -> Vec<String> {
        node.children()
            .filter_map(ast::Annotation::cast)
            .filter_map(|a| a.name().map(|n| n.text().to_owned()))
            .collect()
    }

    #[test]
    fn annotation_names_match_the_red_tree() {
        let text = "\
@Rendering(.client) /* c */ @Theme
public app A {}

service S {
    @Inject  let a: Int
    @State @Env(\"X\") var b: Int = 1
    @ // broken
    let c: Int
    func f() {}
}
";
        let parse = k_syntax::parse(FileId::new(0), text);
        let mut seen = 0;
        for node in parse.syntax().descendants() {
            let red = red_annotation_names(&node);
            seen += red.len();
            assert_eq!(annotation_names(&node), red, "{node:?}");
            assert_eq!(
                visibility_of(&node),
                match visibility_token(&node).map(|t| t.kind()) {
                    Some(SyntaxKind::PublicKw) => Visibility::Public,
                    Some(SyntaxKind::PrivateKw) => Visibility::Private,
                    _ => Visibility::Module,
                }
            );
        }
        assert!(seen >= 5, "only {seen} annotations");
    }
}
