//! The standard library as the checker sees it.
//!
//! # Layout
//!
//! The stdlib's declarations are **K source files embedded in the
//! binary**: `crates/k_sema/stdlib/k/<module>/<Name>.k`. The directory is
//! the module, exactly as for an app (`stdlib/k/http/HttpClient.k` is
//! `k.http.HttpClient`), and every file follows the language's rules: at
//! most one public declaration, named like the file. Implementations that
//! the runtime provides are marked `@Native` and have no body (an `init`
//! keeps an empty `{}`, which the grammar requires). The files are parsed
//! by the same parser as user code, so the standard library is documented,
//! reviewable K.
//!
//! Three tables complete the K sources, because what they describe has no
//! K declaration syntax:
//!
//! * [`elements`]: the built-in view elements, their required named
//!   arguments, chained attributes and events;
//! * [`css`]: the CSS properties of style blocks, with their value grammars
//!   and the K types their implicit members resolve against;
//! * [`annotations`]: what each annotation means: where it may appear and
//!   whether it requires `let` or `var`.
//!
//! # Visibility rules for stdlib files
//!
//! Every stdlib file implicitly sees the prelude (below) and the internal
//! module `k.internal` (`@Native`, `@Annotation`, `@Platform`, …). User code
//! can't import `k.internal`.
//!
//! # The prelude
//!
//! The prelude is what every file sees without an import: every public
//! declaration of `k.core` (core types, framework annotations, the
//! generated `Page`, `Endpoint` and `ThemeVariant`) plus the re-exports in
//! [`PRELUDE_REEXPORTS`]. An import may shadow a prelude name.
//!
//! The full design is in `docs/compiler/sema.md`, "The standard library".

pub mod annotations;
pub mod css;
pub mod elements;
mod files;

use std::collections::HashMap;
use std::sync::OnceLock;

use k_diagnostics::{FileId, TextRange};
use k_syntax::ast::{self, AstNode, HasName, HasVisibility, Visibility};
use k_syntax::{Parse, SyntaxKind};

use crate::project::ModulePath;

/// The module whose public declarations are all in the prelude.
pub const CORE_MODULE: &str = "k.core";

/// The stdlib-only module of compiler-facing annotations (`@Native`,
/// `@Annotation`, …). Every stdlib file sees it; user code can't import it.
pub const INTERNAL_MODULE: &str = "k.internal";

/// Declarations outside `k.core` that are in the prelude anyway: the
/// annotations the design uses without an import.
pub const PRELUDE_REEXPORTS: &[&str] = &[
    "k.db.Id",
    "k.db.Index",
    "k.db.Relation",
    "k.db.Unique",
    "k.db.Version",
    "k.encoding.JsonKey",
    "k.server.JobName",
    "k.server.Retry",
    "k.server.Schedule",
    "k.ui.Router",
];

/// The kind of a top-level declaration.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DeclarationKind {
    /// `component`
    Component,
    /// `page`
    Page,
    /// `layout`
    Layout,
    /// `service`
    Service,
    /// `protocol`
    Protocol,
    /// `entity`
    Entity,
    /// `repository`
    Repository,
    /// `struct`
    Struct,
    /// `enum`
    Enum,
    /// `class`
    Class,
    /// `interface`
    Interface,
    /// `theme`
    Theme,
    /// `app`
    App,
    /// `config`
    Config,
    /// `migration`
    Migration,
    /// A top-level `func`.
    Function,
    /// A top-level `let`.
    Constant,
    /// `test Name { … }`
    TestSuite,
}

impl DeclarationKind {
    /// Returns the kind of a named top-level item, or `None` for items
    /// without a name (`extend`, `test "…"`).
    #[must_use]
    pub fn of(item: &ast::Item) -> Option<DeclarationKind> {
        Some(match item {
            ast::Item::ComponentDecl(_) => DeclarationKind::Component,
            ast::Item::PageDecl(_) => DeclarationKind::Page,
            ast::Item::LayoutDecl(_) => DeclarationKind::Layout,
            ast::Item::ServiceDecl(_) => DeclarationKind::Service,
            ast::Item::ProtocolDecl(_) => DeclarationKind::Protocol,
            ast::Item::EntityDecl(_) => DeclarationKind::Entity,
            ast::Item::RepositoryDecl(_) => DeclarationKind::Repository,
            ast::Item::StructDecl(_) => DeclarationKind::Struct,
            ast::Item::EnumDecl(_) => DeclarationKind::Enum,
            ast::Item::ClassDecl(_) => DeclarationKind::Class,
            ast::Item::InterfaceDecl(_) => DeclarationKind::Interface,
            ast::Item::ThemeDecl(_) => DeclarationKind::Theme,
            ast::Item::AppDecl(_) => DeclarationKind::App,
            ast::Item::ConfigDecl(_) => DeclarationKind::Config,
            ast::Item::MigrationDecl(_) => DeclarationKind::Migration,
            ast::Item::FuncDecl(_) => DeclarationKind::Function,
            ast::Item::FieldDecl(_) => DeclarationKind::Constant,
            ast::Item::TestSuiteDecl(_) => DeclarationKind::TestSuite,
            ast::Item::ExtendDecl(_) | ast::Item::TestCaseDecl(_) => return None,
        })
    }
}

/// One embedded stdlib file.
pub struct StdlibFile {
    /// The path below `stdlib/`, such as `k/http/HttpClient.k`.
    pub path: &'static str,
    /// The module: the path's directories (`k.http`).
    pub module: ModulePath,
    /// The file's text.
    pub text: &'static str,
    /// The file parsed with [`Stdlib::FILE_ID_BASE`] plus its index in
    /// [`Stdlib::files`] as file id.
    pub parse: Parse,
}

/// One named top-level declaration of the stdlib.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StdlibDeclaration {
    /// The module that declares it.
    pub module: ModulePath,
    /// The declared name.
    pub name: String,
    /// What kind of declaration it is.
    pub kind: DeclarationKind,
    /// `true` for a `public` declaration.
    pub is_public: bool,
    /// The names of the annotations written on it (`Native`, `Annotation`).
    pub annotations: Vec<String>,
    /// The index of its file in [`Stdlib::files`].
    pub file: usize,
    /// The range of its name token in that file.
    pub name_range: TextRange,
}

impl StdlibDeclaration {
    /// Returns the fully qualified name, such as `k.http.HttpClient`.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.module, self.name)
    }

    /// Returns `true` if the declaration carries the annotation `name`.
    #[must_use]
    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a == name)
    }
}

/// The parsed standard library and an index of its declarations.
///
/// It is built once per process ([`Stdlib::get`]) and shared by every
/// analysis. Building it parses about two hundred small files, well under a
/// millisecond each.
pub struct Stdlib {
    /// The parsed files, in [`files::FILES`] order.
    files: Vec<StdlibFile>,
    /// The named top-level declarations of every file, in file order.
    declarations: Vec<StdlibDeclaration>,
    /// Indices into `declarations` by qualified name (`k.core.Int`).
    by_name: HashMap<String, usize>,
}

impl Stdlib {
    /// The file id of the first stdlib file when parsed for the index. An
    /// [`AnalysisHost`](crate::AnalysisHost) assigns its own ids.
    pub const FILE_ID_BASE: u32 = 0;

    /// Returns the process-wide standard library, parsing it on first use.
    ///
    /// Thread-safe: concurrent first calls block until one of them has
    /// built it, and every later call is a load.
    ///
    /// ```
    /// use k_sema::stdlib::{DeclarationKind, Stdlib};
    ///
    /// let stdlib = Stdlib::get();
    /// let client = stdlib.lookup("k.http.HttpClient").unwrap();
    /// assert_eq!(client.kind, DeclarationKind::Interface);
    /// assert_eq!(client.qualified_name(), "k.http.HttpClient");
    /// // `Int` needs no import: it is in the prelude.
    /// assert!(stdlib.prelude().iter().any(|(name, _)| *name == "Int"));
    /// ```
    pub fn get() -> &'static Stdlib {
        static STDLIB: OnceLock<Stdlib> = OnceLock::new();
        STDLIB.get_or_init(Stdlib::build)
    }

    /// Parses every embedded stdlib file (ids from [`Stdlib::FILE_ID_BASE`],
    /// in [`files::FILES`] order) and indexes its declarations by qualified
    /// name.
    fn build() -> Stdlib {
        let mut files = Vec::with_capacity(files::FILES.len());
        for (index, &(path, text)) in files::FILES.iter().enumerate() {
            let raw = Stdlib::FILE_ID_BASE + u32::try_from(index).unwrap_or(u32::MAX);
            let parse = k_syntax::parse(FileId::new(raw), text);
            let dirs: Vec<&str> = path.split('/').collect();
            // A file outside a module directory is skipped; the tests
            // require every listed file to be in one.
            let Some(module) = dirs
                .split_last()
                .and_then(|(_, dirs)| ModulePath::from_segments(dirs))
            else {
                continue;
            };
            files.push(StdlibFile {
                path,
                module,
                text,
                parse,
            });
        }
        let mut declarations = Vec::new();
        for (index, file) in files.iter().enumerate() {
            collect_declarations(index, file, &mut declarations);
        }
        let by_name = declarations
            .iter()
            .enumerate()
            .map(|(i, d)| (d.qualified_name(), i))
            .collect();
        Stdlib {
            files,
            declarations,
            by_name,
        }
    }

    /// Returns every file, sorted by path.
    #[must_use]
    pub fn files(&self) -> &[StdlibFile] {
        &self.files
    }

    /// Returns every named top-level declaration, in file order.
    #[must_use]
    pub fn declarations(&self) -> &[StdlibDeclaration] {
        &self.declarations
    }

    /// Returns the declaration with a fully qualified name such as
    /// `k.http.HttpClient`.
    #[must_use]
    pub fn lookup(&self, qualified: &str) -> Option<&StdlibDeclaration> {
        self.by_name.get(qualified).map(|&i| &self.declarations[i])
    }

    /// Returns the names visible without an import, as `(name, declaration)`
    /// pairs sorted by name: every public declaration of [`CORE_MODULE`]
    /// and every [`PRELUDE_REEXPORTS`] entry.
    #[must_use]
    pub fn prelude(&self) -> Vec<(&str, &StdlibDeclaration)> {
        let mut names: Vec<(&str, &StdlibDeclaration)> = self
            .declarations
            .iter()
            .filter(|d| d.is_public && d.module.as_str() == CORE_MODULE)
            .chain(PRELUDE_REEXPORTS.iter().filter_map(|q| self.lookup(q)))
            .map(|d| (d.name.as_str(), d))
            .collect();
        names.sort_by(|a, b| a.0.cmp(b.0));
        names
    }

    /// Returns the modules of the stdlib, sorted.
    #[must_use]
    pub fn modules(&self) -> Vec<&ModulePath> {
        let mut modules: Vec<&ModulePath> = self.files.iter().map(|f| &f.module).collect();
        modules.sort();
        modules.dedup();
        modules
    }
}

/// Appends the named top-level declarations of stdlib file number `index`
/// to `out`, in source order.
fn collect_declarations(index: usize, file: &StdlibFile, out: &mut Vec<StdlibDeclaration>) {
    let Some(root) = ast::SourceFile::cast(file.parse.syntax()) else {
        return;
    };
    for item in root.items() {
        let Some(kind) = DeclarationKind::of(&item) else {
            continue;
        };
        let Some(name) = item_name(&item) else {
            continue;
        };
        out.push(StdlibDeclaration {
            module: file.module.clone(),
            name: name.text().to_owned(),
            kind,
            is_public: item_is_public(&item),
            annotations: item
                .syntax()
                .children()
                .filter_map(ast::Annotation::cast)
                .filter_map(|a| a.name().map(|n| n.text().to_owned()))
                .collect(),
            file: index,
            name_range: name.text_range(),
        });
    }
}

/// Returns the name token of a named item.
fn item_name(item: &ast::Item) -> Option<k_syntax::SyntaxToken> {
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

/// Returns `true` if a named item is declared `public`.
fn item_is_public(item: &ast::Item) -> bool {
    let visibility = match item {
        ast::Item::ComponentDecl(d) => d.visibility(),
        ast::Item::PageDecl(d) => d.visibility(),
        ast::Item::LayoutDecl(d) => d.visibility(),
        ast::Item::ServiceDecl(d) => d.visibility(),
        ast::Item::ProtocolDecl(d) => d.visibility(),
        ast::Item::EntityDecl(d) => d.visibility(),
        ast::Item::RepositoryDecl(d) => d.visibility(),
        ast::Item::StructDecl(d) => d.visibility(),
        ast::Item::EnumDecl(d) => d.visibility(),
        ast::Item::ClassDecl(d) => d.visibility(),
        ast::Item::InterfaceDecl(d) => d.visibility(),
        ast::Item::ThemeDecl(d) => d.visibility(),
        ast::Item::AppDecl(d) => d.visibility(),
        ast::Item::ConfigDecl(d) => d.visibility(),
        ast::Item::MigrationDecl(d) => d.visibility(),
        ast::Item::FuncDecl(d) => d.visibility(),
        ast::Item::FieldDecl(d) => d.visibility(),
        ast::Item::ExtendDecl(d) => {
            if d.syntax()
                .children_with_tokens()
                .any(|e| e.kind() == SyntaxKind::PublicKw)
            {
                Visibility::Public
            } else {
                Visibility::Module
            }
        }
        ast::Item::TestSuiteDecl(_) | ast::Item::TestCaseDecl(_) => Visibility::Module,
    };
    visibility == Visibility::Public
}

#[cfg(test)]
mod tests;
