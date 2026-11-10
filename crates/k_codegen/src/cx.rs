//! The code generator's view of the checked program: builtin declarations,
//! the JavaScript name of every declaration and member, type queries by
//! syntax range, and syntax lookup (`docs/compiler/codegen.md` §3, step 1).
//!
//! Everything here reads `k_sema` query results ([`k_sema::Analysis`]
//! and its [`ProgramView`]); codegen has no second checker.

use std::collections::{BTreeSet, HashMap, HashSet};

use k_diagnostics::{FileId, TextRange};
use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, DeclKind, MemberId, MemberKind, ProgramView, Resolved, Type};
use k_sema::{Analysis, ProjectPath, Resolution};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::{Options, Profile};

/// Which bundle code is generated for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub(crate) enum Target {
    /// The server bundle (SSR renderers, handlers, the entry).
    Server,
    /// The client bundle (DOM factories, RPC stubs).
    Client,
}

/// Declarations the generator gives meaning to.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Builtins {
    /// `k.core.Int`, or `None` when the program's stdlib lacks it.
    pub int: Option<DeclId>,
    /// `k.core.Double`, or `None` when the program's stdlib lacks it.
    pub double: Option<DeclId>,
    /// `k.core.Decimal`, or `None` when the program's stdlib lacks it.
    pub decimal: Option<DeclId>,
    /// `k.core.Bool`, or `None` when the program's stdlib lacks it.
    pub bool_: Option<DeclId>,
    /// `k.core.String`, or `None` when the program's stdlib lacks it.
    pub string: Option<DeclId>,
    /// `k.core.Void`, or `None` when the program's stdlib lacks it.
    pub void: Option<DeclId>,
    /// `k.core.List`, or `None` when the program's stdlib lacks it.
    pub list: Option<DeclId>,
    /// `k.core.Map`, or `None` when the program's stdlib lacks it.
    pub map: Option<DeclId>,
    /// `k.core.Set`, or `None` when the program's stdlib lacks it.
    pub set: Option<DeclId>,
    /// `k.core.Money`, or `None` when the program's stdlib lacks it.
    pub money: Option<DeclId>,
    /// `k.core.Duration`, or `None` when the program's stdlib lacks it.
    pub duration: Option<DeclId>,
    /// `k.core.Date`, or `None` when the program's stdlib lacks it.
    pub date: Option<DeclId>,
    /// `k.core.Length`, or `None` when the program's stdlib lacks it.
    pub length: Option<DeclId>,
    /// `k.core.Color`, or `None` when the program's stdlib lacks it.
    pub color: Option<DeclId>,
    /// `k.core.Percent`, or `None` when the program's stdlib lacks it.
    pub percent: Option<DeclId>,
    /// `k.core.Angle`, or `None` when the program's stdlib lacks it.
    pub angle: Option<DeclId>,
    /// `k.core.HalfOpenRange`, or `None` when the program's stdlib lacks it.
    pub half_open: Option<DeclId>,
    /// `k.core.ClosedRange`, or `None` when the program's stdlib lacks it.
    pub closed: Option<DeclId>,
    /// `k.core.Result`, or `None` when the program's stdlib lacks it.
    pub result: Option<DeclId>,
    /// `k.core.Secret`, or `None` when the program's stdlib lacks it.
    pub secret: Option<DeclId>,
    /// `k.core.Currency`, or `None` when the program's stdlib lacks it.
    pub currency: Option<DeclId>,
    /// `k.crypto.Uuid`, or `None` when the program's stdlib lacks it.
    pub uuid: Option<DeclId>,
    /// `k.core.Url`, or `None` when the program's stdlib lacks it.
    pub url: Option<DeclId>,
    /// `k.text.EmailAddress`, or `None` when the program's stdlib lacks it.
    pub email: Option<DeclId>,
    /// `k.core.Bytes`, or `None` when the program's stdlib lacks it.
    pub bytes: Option<DeclId>,
    /// `k.http.DecodingError`, or `None` when the program's stdlib lacks it.
    pub decoding_error: Option<DeclId>,
    /// `k.core.Codable`, or `None` when the program's stdlib lacks it.
    pub codable: Option<DeclId>,
    /// `k.core.Spacing`, or `None` when the program's stdlib lacks it.
    pub spacing: Option<DeclId>,
    /// `k.core.Radius`, or `None` when the program's stdlib lacks it.
    pub radius: Option<DeclId>,
    /// `k.core.FontSize`, or `None` when the program's stdlib lacks it.
    pub font_size: Option<DeclId>,
    /// `k.time.TimeZone`: its value is its IANA name, with a builtin codec.
    pub time_zone: Option<DeclId>,
    /// `k.time.TimeOfDay`: its value is its `HH:MM` text, with a builtin
    /// codec.
    pub time_of_day: Option<DeclId>,
}

impl Builtins {
    /// Looks every builtin up by its qualified name in `view`.
    fn new(view: &ProgramView) -> Builtins {
        let core = |n: &str| view.lookup(&format!("k.core.{n}"));
        Builtins {
            int: core("Int"),
            double: core("Double"),
            decimal: core("Decimal"),
            bool_: core("Bool"),
            string: core("String"),
            void: core("Void"),
            list: core("List"),
            map: core("Map"),
            set: core("Set"),
            money: core("Money"),
            duration: core("Duration"),
            date: core("Date"),
            length: core("Length"),
            color: core("Color"),
            percent: core("Percent"),
            angle: core("Angle"),
            half_open: core("HalfOpenRange"),
            closed: core("ClosedRange"),
            result: core("Result"),
            secret: core("Secret"),
            currency: core("Currency"),
            uuid: view.lookup("k.crypto.Uuid"),
            url: core("Url"),
            email: view.lookup("k.text.EmailAddress"),
            bytes: core("Bytes"),
            decoding_error: view.lookup("k.http.DecodingError"),
            codable: core("Codable"),
            spacing: core("Spacing"),
            radius: core("Radius"),
            font_size: core("FontSize"),
            time_zone: view.lookup("k.time.TimeZone"),
            time_of_day: view.lookup("k.time.TimeOfDay"),
        }
    }
}

/// JavaScript reserved words and globals the output relies on; a K name
/// equal to one gets a `$` suffix (codegen.md §4).
const RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "NaN",
    "Infinity",
    "Object",
    "Array",
    "Map",
    "Set",
    "Symbol",
    "Promise",
    "JSON",
    "Math",
    "String",
    "Number",
    "Boolean",
    "Error",
    "Date",
    "globalThis",
    "$k",
    "$c",
    "self",
    "console",
    "Reflect",
    "Intl",
    "BigInt",
    "RegExp",
    "Function",
    "URL",
];

/// The `[package] name` of a `k.toml` text (the first `name = "…"` line);
/// empty when there is none.
fn manifest_name(text: &str) -> String {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name") {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix('=') {
                return v.trim().trim_matches('"').to_owned();
            }
        }
    }
    String::new()
}

/// Mangles a K identifier for use as a JavaScript binding.
#[must_use]
pub(crate) fn mangle(name: &str) -> String {
    if RESERVED.contains(&name) {
        format!("{name}$")
    } else {
        name.to_owned()
    }
}

/// The generator's context for one `generate` call.
pub(crate) struct Gen<'a> {
    /// The analysis being generated from: files, parses and type queries.
    pub analysis: &'a Analysis,
    /// The checked program: declarations, members and their types.
    pub view: &'a ProgramView,
    /// The profile, environment and entry of this `generate` call.
    pub options: &'a Options,
    /// The builtin declarations, looked up once.
    pub b: Builtins,
    /// The JavaScript name of each declaration.
    names: HashMap<DeclId, String>,
    /// The JavaScript name of members that are emitted as functions or
    /// properties (overloads carry labels).
    member_names: HashMap<MemberId, String>,
    /// Syntax roots of project files.
    roots: HashMap<FileId, SyntaxNode>,
    /// The project's own (non-stdlib) declarations, in a fixed order.
    pub own: Vec<DeclId>,
    /// The persistence model and the statements registered while
    /// lowering repositories (`crate::db`).
    pub db: crate::db::DbCell,
    /// The project name from `k.toml`, read once ([`Gen::project_name`]
    /// is called per rpc and per module; reading it scanned every file).
    project: std::cell::OnceCell<String>,
    /// The protocols called remotely ([`Gen::remote_protocols`]).
    remote: std::cell::OnceCell<BTreeSet<DeclId>>,
}

impl<'a> Gen<'a> {
    /// Builds the context for one `generate` call: looks up the builtins,
    /// collects the syntax root of every parsed file and assigns every
    /// declaration its JavaScript name ([`Gen::assign_names`]).
    ///
    /// Linear in the number of declarations, except that a name shared by
    /// several declarations costs a scan of all of them per sharer.
    pub(crate) fn new(
        analysis: &'a Analysis,
        view: &'a ProgramView,
        options: &'a Options,
    ) -> Gen<'a> {
        let b = Builtins::new(view);
        let mut roots = HashMap::new();
        for (file, _) in analysis.files() {
            if let Ok(Some(parse)) = analysis.parse(file) {
                roots.insert(file, parse.syntax());
            }
        }
        let mut this = Gen {
            analysis,
            view,
            options,
            b,
            names: HashMap::new(),
            member_names: HashMap::new(),
            roots,
            own: Vec::new(),
            db: crate::db::DbCell::default(),
            project: std::cell::OnceCell::new(),
            remote: std::cell::OnceCell::new(),
        };
        this.assign_names();
        this
    }

    /// Gives every declaration its JavaScript name and fills [`Gen::own`].
    ///
    /// A declaration keeps its simple (mangled) name unless another emitted
    /// declaration shares it: a stdlib declaration then becomes
    /// `module$Name`, and so does a project declaration when another project
    /// declaration shares the name (a project declaration wins its simple name
    /// over the stdlib). Test groups are named `$group<N>`, so they never take
    /// the name of what they test.
    fn assign_names(&mut self) {
        // Simple names, unless two declarations of different modules that
        // are both emitted share one: then `module$name`.
        let mut count: HashMap<String, usize> = HashMap::new();
        let mut ids: Vec<DeclId> = Vec::new();
        for (i, d) in self.view.decls.iter().enumerate() {
            let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
            if d.kind == DeclKind::Extension || d.name.is_empty() {
                continue;
            }
            ids.push(id);
            // A test group has a name of its own (`$group<N>`, below), so
            // `test Home { }` leaves `page Home` its simple name.
            if d.named_kind() != Some(DeclarationKind::TestSuite) {
                *count.entry(d.name.clone()).or_default() += 1;
            }
        }
        for id in ids {
            let d = self.view.decl(id);
            let name = if count.get(&d.name).copied().unwrap_or(0) > 1 && d.stdlib {
                format!(
                    "{}${}",
                    d.module.clone().unwrap_or_default().replace('.', "$"),
                    d.name
                )
            } else if count.get(&d.name).copied().unwrap_or(0) > 1 {
                // Project declarations win their simple name over stdlib
                // ones; two project declarations are qualified.
                let project_dupes = self
                    .view
                    .decls
                    .iter()
                    .filter(|o| {
                        o.name == d.name
                            && !o.stdlib
                            && o.kind != DeclKind::Extension
                            && o.named_kind() != Some(DeclarationKind::TestSuite)
                    })
                    .count();
                if project_dupes > 1 {
                    format!(
                        "{}${}",
                        d.module.clone().unwrap_or_default().replace('.', "$"),
                        d.name
                    )
                } else {
                    mangle(&d.name)
                }
            } else {
                mangle(&d.name)
            };
            // A test group is usually named after what it tests
            // (`test Panel { }` beside `component Panel`): its class and
            // templates get a name of their own (`crate::suite`).
            let name = if d.named_kind() == Some(DeclarationKind::TestSuite) {
                format!("$group{}", id.0)
            } else {
                name
            };
            self.names.insert(id, name);
        }
        let mut own: Vec<DeclId> = (0..self.view.decls.len())
            .map(|i| DeclId(u32::try_from(i).unwrap_or(u32::MAX)))
            .filter(|&id| !self.view.decl(id).stdlib)
            .collect();
        own.sort_by(|a, b| {
            let da = self.view.decl(*a);
            let db = self.view.decl(*b);
            (
                da.module.clone().unwrap_or_default(),
                da.file,
                da.range.start(),
            )
                .cmp(&(
                    db.module.clone().unwrap_or_default(),
                    db.file,
                    db.range.start(),
                ))
        });
        self.own = own;
        // Member names: overloads get their label signature.
        for d in &self.view.decls {
            let mut by_name: HashMap<&str, Vec<MemberId>> = HashMap::new();
            for &m in &d.members {
                let info = self.view.member(m);
                if matches!(info.kind, MemberKind::Func | MemberKind::Init) {
                    by_name.entry(info.name.as_str()).or_default().push(m);
                }
            }
            for (_, ms) in by_name {
                if ms.len() < 2 {
                    continue;
                }
                let mut taken: Vec<String> = Vec::new();
                for (i, m) in ms.iter().enumerate() {
                    let mut name = self.view.member(*m).name.clone();
                    for l in self.view.labels(*m).into_iter().flatten() {
                        name.push('$');
                        name.push_str(&l);
                    }
                    if taken.contains(&name) {
                        name.push_str(&format!("${i}"));
                    }
                    taken.push(name.clone());
                    self.member_names.insert(*m, name);
                }
            }
            // A static field or computed property may share its name with a
            // function (K2019 exempts functions), but in JavaScript a
            // struct's module constant and function, or a class's static
            // property and static method, would be one name: the property
            // becomes `name$static` (`static` can't be an argument label).
            // The stdlib's statics are the runtime's names.
            if d.stdlib {
                continue;
            }
            let funcs: HashSet<&str> = d
                .members
                .iter()
                .map(|&m| self.view.member(m))
                .filter(|i| matches!(i.kind, MemberKind::Func | MemberKind::Init))
                .map(|i| i.name.as_str())
                .collect();
            for &m in &d.members {
                let info = self.view.member(m);
                if info.is_static
                    && matches!(info.kind, MemberKind::Field | MemberKind::Computed)
                    && funcs.contains(info.name.as_str())
                {
                    self.member_names.insert(m, format!("{}$static", info.name));
                }
            }
        }
    }

    /// `true` when the app is `@Rendering(.client)`: pages render in the
    /// browser and have no server renderers.
    pub(crate) fn client_rendered(&self) -> bool {
        self.own.iter().any(|&d| {
            let info = self.view.decl(d);
            info.named_kind() == Some(DeclarationKind::App)
                && self.decl_node(d).is_some_and(|n| {
                    n.children()
                        .filter(|c| c.kind() == SyntaxKind::Annotation)
                        .any(|a| {
                            let t = a.text().to_string();
                            t.starts_with("@Rendering") && t.contains(".client")
                        })
                })
        })
    }

    /// `true` for a component marked `@Rendering(.client)` in an app that
    /// renders on the server: the server renders its `loading` block and
    /// the browser swaps in its `view` after hydration
    /// (47-client-rendered-components.md).
    pub(crate) fn client_component(&self, d: DeclId) -> bool {
        self.view.decl(d).named_kind() == Some(DeclarationKind::Component)
            && !self.client_rendered()
            && self.decl_node(d).is_some_and(|n| {
                n.children()
                    .filter(|c| c.kind() == SyntaxKind::Annotation)
                    .any(|a| {
                        let t = a.text().to_string();
                        t.starts_with("@Rendering") && t.contains(".client")
                    })
            })
    }

    /// The project name from `k.toml` (`name = "…"` under `[package]`).
    pub(crate) fn project_name(&self) -> String {
        self.project
            .get_or_init(|| self.read_project_name())
            .clone()
    }

    /// The namespace of the app's browser storage keys
    /// (`docs/design/36-local-storage.md`, "Keys"): `storage.lock`'s
    /// `namespace`, else the project name the lock will record.
    pub(crate) fn storage_namespace(&self) -> String {
        let recorded =
            crate::lockfiles::project_file(self, "storage.lock").and_then(|(_, text)| {
                text.lines().find_map(|line| {
                    let rest = line.trim().strip_prefix("namespace")?.trim_start();
                    let v = rest.strip_prefix('=')?.trim();
                    Some(v.trim_matches('"').to_owned())
                })
            });
        recorded
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.project_name())
    }

    /// Reads the project name from `k.toml`; empty when there is none.
    fn read_project_name(&self) -> String {
        for (file, path) in self.analysis.files() {
            if path.as_str() != "k.toml" {
                continue;
            }
            if let Some(text) = self.analysis.text(file) {
                return manifest_name(text);
            }
        }
        String::new()
    }

    /// The protocols this app calls in other services
    /// (`docs/design/40-workspaces-and-services.md`): the targets of its
    /// `use remote(…) for P` rules and the protocols `RemoteClient.at`
    /// builds clients of. The server emits a `P$remote` client class for
    /// each one it doesn't implement.
    pub(crate) fn remote_protocols(&self) -> &BTreeSet<DeclId> {
        self.remote.get_or_init(|| {
            let mut out = BTreeSet::new();
            for rule in crate::server::use_rules(self) {
                if rule.remote.is_some() {
                    out.extend(rule.target);
                }
            }
            for (&file, root) in &self.roots {
                for call in root
                    .descendants()
                    .filter_map(<k_syntax::ast::CallExpr as k_syntax::ast::AstNode>::cast)
                {
                    let syntax = k_syntax::ast::AstNode::syntax(&call);
                    let text = syntax.text().to_string();
                    if !text.trim_start().starts_with("RemoteClient") {
                        continue;
                    }
                    let range = syntax.text_range();
                    if let Ok(Some(t)) = self.analysis.type_info_at(file, range) {
                        if let Some(d) = t.decl() {
                            if self.view.decl(d).named_kind() == Some(DeclarationKind::Protocol) {
                                out.insert(d);
                            }
                        }
                    }
                }
            }
            out
        })
    }

    /// The `[package] name` of the package that declares `d`: the
    /// library's, for a declaration of a library mounted from the
    /// workspace (`k_sema::workspace`), else the project's own. Protocols
    /// take their Protobuf package from it, so the serving and the calling
    /// app agree on the rpc paths (`docs/design/40-workspaces-and-services.md`).
    pub(crate) fn package_name(&self, d: DeclId) -> String {
        let file = self.view.decl(d).file;
        let Some(k_sema::FileOrigin::Project(path)) = self.analysis.origin(file) else {
            return self.project_name();
        };
        let Some((dir, _)) = path.dependency() else {
            return self.project_name();
        };
        let Ok(manifest) = ProjectPath::new("k.toml") else {
            return self.project_name();
        };
        let mounted = ProjectPath::mounted(&dir, &manifest);
        self.analysis
            .file_id(&mounted)
            .and_then(|f| self.analysis.text(f))
            .map(manifest_name)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| dir.rsplit('/').next().unwrap_or_default().to_owned())
    }

    /// The JavaScript name of a declaration.
    pub(crate) fn name(&self, id: DeclId) -> String {
        self.names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("$decl{}", id.0))
    }

    /// The JavaScript name of a member (methods: without the owner
    /// prefix).
    pub(crate) fn member_name(&self, id: MemberId) -> String {
        self.member_names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| self.view.member(id).name.clone())
    }

    /// The prefix for a declaration's module functions: the type's name for
    /// its own members, the extended type's name for extension members.
    pub(crate) fn owner_prefix(&self, member: MemberId) -> String {
        let owner = self.view.member(member).owner;
        let d = self.view.decl(owner);
        if d.kind == DeclKind::Extension {
            if let Some(target) = d.extends.as_ref().and_then(Type::decl) {
                return self.view.decl(target).name.clone();
            }
        }
        self.name(owner)
    }

    /// The module-function name of a struct, enum or extension member:
    /// `Point$moved`, `Int$squared`.
    pub(crate) fn fn_name(&self, member: MemberId) -> String {
        format!("{}${}", self.owner_prefix(member), self.member_name(member))
    }

    // ------------------------------------------------------------- syntax

    /// The syntax node of a declaration.
    pub(crate) fn decl_node(&self, id: DeclId) -> Option<SyntaxNode> {
        let d = self.view.decl(id);
        let root = self.roots.get(&d.file)?;
        node_at(root, d.range)
    }

    /// The syntax node of a member.
    pub(crate) fn member_node(&self, id: MemberId) -> Option<SyntaxNode> {
        let m = self.view.member(id);
        let d = self.view.decl(m.owner);
        let root = self.roots.get(&d.file)?;
        node_at(root, m.range?)
    }

    /// The syntax node of a member of a stdlib declaration, from the
    /// stdlib's own parse, with the file id the analysis gives that file
    /// (stdlib files come first, in [`k_sema::stdlib::Stdlib::files`]
    /// order). `None` for a project member.
    ///
    /// [`Gen::member_node`] only knows project files, so generated code
    /// never lowers stdlib bodies by accident; this is for the few places
    /// that read a stdlib declaration's source on purpose, such as the
    /// default values of a stdlib struct's fields.
    pub(crate) fn stdlib_member_node(&self, id: MemberId) -> Option<(FileId, SyntaxNode)> {
        let m = self.view.member(id);
        let d = self.view.decl(m.owner);
        if !d.stdlib {
            return None;
        }
        let k_sema::FileOrigin::Stdlib(path) = self.analysis.origin(d.file)? else {
            return None;
        };
        let file = k_sema::stdlib::Stdlib::get()
            .files()
            .iter()
            .find(|f| f.path == path)?;
        node_at(&file.parse.syntax(), m.range?).map(|n| (d.file, n))
    }

    // -------------------------------------------------------------- types

    /// The recorded type of the node with exactly `range`.
    pub(crate) fn type_at(&self, file: FileId, range: TextRange) -> Type {
        self.analysis
            .type_info_at(file, range)
            .ok()
            .flatten()
            .unwrap_or(Type::Error)
    }

    /// The recorded resolution at exactly `range`.
    pub(crate) fn resolution(&self, file: FileId, range: TextRange) -> Option<Resolution> {
        self.analysis.resolve_at(file, range).ok().flatten()
    }

    /// The resolved declaration or member at `range`.
    pub(crate) fn resolved(&self, file: FileId, range: TextRange) -> Option<Resolved> {
        self.resolution(file, range)
            .and_then(|r| self.view.resolve(&r))
    }

    /// `true` if `t` is the nominal type `decl`.
    pub(crate) fn is(&self, t: &Type, decl: Option<DeclId>) -> bool {
        decl.is_some() && t.decl() == decl
    }

    /// The declaration kind of a nominal type.
    pub(crate) fn kind_of(&self, t: &Type) -> Option<DeclarationKind> {
        t.decl().and_then(|d| self.view.decl(d).named_kind())
    }

    /// `true` for `Int`.
    pub(crate) fn is_int(&self, t: &Type) -> bool {
        self.is(t, self.b.int)
    }

    /// `true` for `Double`.
    pub(crate) fn is_double(&self, t: &Type) -> bool {
        self.is(t, self.b.double)
    }

    /// `true` for `String`.
    pub(crate) fn is_string(&self, t: &Type) -> bool {
        self.is(t, self.b.string)
    }

    /// `true` for `Bool`.
    pub(crate) fn is_bool(&self, t: &Type) -> bool {
        self.is(t, self.b.bool_)
    }

    /// `true` for `List<T>`.
    pub(crate) fn is_list(&self, t: &Type) -> bool {
        self.is(t, self.b.list)
    }

    /// `true` for CSS-text types (`Length`, token categories, `Color`).
    pub(crate) fn is_css_text(&self, t: &Type) -> bool {
        [
            self.b.length,
            self.b.spacing,
            self.b.radius,
            self.b.font_size,
            self.b.color,
        ]
        .iter()
        .any(|d| self.is(t, *d))
    }

    /// `true` for types represented by JS numbers.
    pub(crate) fn is_number(&self, t: &Type) -> bool {
        [
            self.b.int,
            self.b.double,
            self.b.duration,
            self.b.percent,
            self.b.angle,
        ]
        .iter()
        .any(|d| self.is(t, *d))
    }

    /// `true` for an enum all of whose cases are payload-free.
    pub(crate) fn is_simple_enum(&self, t: &Type) -> bool {
        let Some(d) = t.decl() else {
            return false;
        };
        let info = self.view.decl(d);
        if info.named_kind() != Some(DeclarationKind::Enum) {
            return false;
        }
        info.members.iter().all(|&m| {
            let mi = self.view.member(m);
            mi.kind != MemberKind::Case || mi.signature.as_ref().is_none_or(|s| s.params.is_empty())
        })
    }

    /// `true` for types whose `==` is JavaScript `===`: numbers, strings,
    /// booleans, CSS text and payload-free enums, and optionals of them.
    pub(crate) fn is_identity_eq(&self, t: &Type) -> bool {
        let t = t.unwrap_optional();
        self.is_number(t)
            || self.is_string(t)
            || self.is_bool(t)
            || self.is_css_text(t)
            || self.is_simple_enum(t)
            || [self.b.uuid, self.b.url, self.b.email, self.b.time_of_day]
                .iter()
                .any(|d| self.is(t, *d))
    }

    /// `true` for types whose `Map`/`Set` keys can be JS `Map`/`Set` keys.
    pub(crate) fn is_primitive_key(&self, t: &Type) -> bool {
        self.is_identity_eq(t)
    }

    /// `true` for struct and enum types (values whose methods are module
    /// functions).
    pub(crate) fn is_value_type(&self, t: &Type) -> bool {
        matches!(
            self.kind_of(t),
            Some(DeclarationKind::Struct | DeclarationKind::Enum | DeclarationKind::Entity)
        )
    }

    /// `true` for interface types.
    pub(crate) fn is_interface(&self, t: &Type) -> bool {
        matches!(
            self.kind_of(t),
            Some(DeclarationKind::Interface | DeclarationKind::Protocol)
        )
    }

    /// `true` for development-style builds, where `Int` `+ - *` are checked.
    pub(crate) fn checked(&self) -> bool {
        self.options.profile != Profile::Release
    }

    /// The fields of a struct or class that are stored, in order.
    pub(crate) fn stored_fields(&self, decl: DeclId) -> Vec<MemberId> {
        self.view
            .decl(decl)
            .members
            .iter()
            .copied()
            .filter(|&m| {
                let mi = self.view.member(m);
                mi.kind == MemberKind::Field && !mi.is_static
            })
            .collect()
    }

    /// The enum cases of a declaration, in order.
    pub(crate) fn cases(&self, decl: DeclId) -> Vec<MemberId> {
        self.view
            .decl(decl)
            .members
            .iter()
            .copied()
            .filter(|&m| self.view.member(m).kind == MemberKind::Case)
            .collect()
    }

    /// The tag of an enum case: its index among the cases.
    pub(crate) fn case_tag(&self, case: MemberId) -> usize {
        let owner = self.view.member(case).owner;
        self.cases(owner)
            .iter()
            .position(|&c| c == case)
            .unwrap_or(0)
    }

    /// The JS field names of a case's payload: labels, or `_0`, `_1`.
    pub(crate) fn case_fields(&self, case: MemberId) -> Vec<String> {
        self.view
            .member(case)
            .signature
            .as_ref()
            .map(|s| {
                s.params
                    .iter()
                    .enumerate()
                    .map(|(i, p)| p.label.clone().unwrap_or_else(|| format!("_{i}")))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `true` if `decl` (a type) declares that it implements `iface`,
    /// directly, in one of its extensions, or through parent interfaces;
    /// a declaration counts as implementing itself.
    pub(crate) fn implements(&self, decl: DeclId, iface: DeclId) -> bool {
        let mut seen = BTreeSet::new();
        self.implements_inner(decl, iface, &mut seen)
    }

    /// The recursion of [`Gen::implements`]: follows the `implements` lists of
    /// `decl` and its extensions; `seen` cuts cycles, so a declaration is
    /// visited at most once.
    fn implements_inner(&self, decl: DeclId, iface: DeclId, seen: &mut BTreeSet<DeclId>) -> bool {
        if decl == iface {
            return true;
        }
        if !seen.insert(decl) {
            return false;
        }
        let d = self.view.decl(decl);
        let mut list: Vec<Type> = d.implements.clone();
        if let Some(exts) = self.view.extensions_of.get(&decl) {
            for &e in exts {
                list.extend(self.view.decl(e).implements.iter().cloned());
            }
        }
        list.iter()
            .filter_map(Type::decl)
            .any(|i| self.implements_inner(i, iface, seen))
    }

    /// Finds a member named `name` on `decl` or its extensions.
    pub(crate) fn find_member(&self, decl: DeclId, name: &str) -> Option<MemberId> {
        let mut owners = vec![decl];
        if let Some(exts) = self.view.extensions_of.get(&decl) {
            owners.extend(exts.iter().copied());
        }
        owners
            .into_iter()
            .flat_map(|o| self.view.decl(o).members.iter().copied())
            .find(|&m| self.view.member(m).name == name)
    }

    /// `true` when the declaration is `@Native` (its body is the runtime's).
    pub(crate) fn is_native_member(&self, m: MemberId) -> bool {
        let mi = self.view.member(m);
        mi.has_annotation("Native") || self.view.decl(mi.owner).has_annotation("Native")
    }
}

/// Finds the node with exactly `range` below `root` (the outermost one).
pub(crate) fn node_at(root: &SyntaxNode, range: TextRange) -> Option<SyntaxNode> {
    let mut node = match root.covering_element(range) {
        rowan::NodeOrToken::Node(n) => n,
        rowan::NodeOrToken::Token(t) => t.parent()?,
    };
    while node.text_range() != range {
        node = node.parent()?;
        if node.text_range().len() > range.len() {
            return None;
        }
    }
    while let Some(parent) = node.parent() {
        if parent.text_range() == range {
            node = parent;
        } else {
            break;
        }
    }
    Some(node)
}

/// The first token of `kind` among `node`'s direct children.
pub(crate) fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == kind)
}

/// `true` if `node` has a direct child token of `kind`.
pub(crate) fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    token(node, kind).is_some()
}
