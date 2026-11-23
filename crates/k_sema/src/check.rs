//! The semantic pass over a whole project: builds the [`Program`], runs the
//! declaration checks (sema.md §4, §6, §9.2–9.3), checks every body
//! ([`crate::body`]) and collects diagnostics and the answers of the
//! position queries into a [`SemaResult`].
//!
//! The pass is a pure function of the project's parses; the host memoizes
//! its result per revision, and hands each revision's body facts to the
//! next one, which replays those of unchanged files ([`crate::warm`]).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use k_diagnostics::{Diagnostic, FileId, Fix, Span, TextRange, TextSize, codes};
use k_syntax::ast::{self, AstNode, HasName, Visibility};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::body::{BodyOwner, Checker};
use crate::cx::{Cx, DERIVED, ImportRes, SigKey, builtin, has_token, token};
use crate::fx::{FxHashMap, FxHashSet};
use crate::lookup::Resolution;
use crate::program::{
    DeclId, DeclKind, FileIx, InputFile, MemberId, MemberKind, Program, visibility_token,
};
use crate::stdlib::DeclarationKind;
use crate::ty::Ty;
use crate::warm::{BodyCache, Chunked, FileFacts, RangeMap};

/// Everything the semantic pass computed for one revision.
#[derive(Debug, Default)]
pub(crate) struct SemaResult {
    /// The semantic diagnostics (not sorted).
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Types by node range (rendered on demand with `names`).
    pub(crate) types: Chunked<Ty>,
    /// The names that render `types`.
    pub(crate) names: crate::ty::NameTable,
    /// Resolutions by reference range.
    pub(crate) resolutions: Chunked<Resolution>,
    /// The side of every top-level declaration, by its name's range.
    pub(crate) sides: HashMap<(FileId, TextRange), crate::lookup::Side>,
    /// The body facts of every file, for the next revision's warm re-check
    /// ([`crate::warm`]).
    pub(crate) cache: Arc<BodyCache>,
    /// The read-only program view for code generation ([`crate::view`]),
    /// filled after the rest of the result ([`crate::view::ViewSlot`]).
    pub(crate) view: Arc<crate::view::ViewSlot>,
    /// The `rpc.lock` the program implies ([`crate::rpclock`]); `None` for
    /// a project that serves no protocol and has no lock.
    pub(crate) rpc_lock: Option<String>,
    /// The `storage.lock` the program implies ([`crate::locks`]); `None`
    /// when it would be unchanged or there is nothing to record.
    pub(crate) storage_lock: Option<String>,
    /// The program's indexed collections ([`crate::collections`]).
    pub(crate) collections: Arc<Vec<crate::collections::CollectionSchema>>,
}

/// Runs the semantic pass over `inputs` and the project's `lockfiles`.
///
/// `prev` is the previous revision's [`BodyCache`]: the files whose text is
/// unchanged replay their body facts from it instead of being checked
/// again, when the project's file list and signatures are the same
/// ([`crate::warm`]). The result is the same as without `prev`.
///
/// The result goes to `deliver` as soon as it is complete; the item tree
/// and the checker's caches are dropped afterwards, so a caller on another
/// thread doesn't wait for them to be freed (`host::Analysis::sema`).
pub(crate) fn analyze(
    inputs: Vec<InputFile<'_>>,
    lockfiles: Vec<crate::locks::Lockfile>,
    manifests: Vec<(String, FileId, Arc<str>)>,
    prev: Option<&BodyCache>,
    deliver: impl FnOnce(SemaResult),
) {
    let manifest_ids: FxHashMap<String, FileId> = manifests
        .iter()
        .map(|(p, id, _)| (p.clone(), *id))
        .collect();
    let manifests: Vec<(String, Arc<str>)> =
        manifests.into_iter().map(|(p, _, t)| (p, t)).collect();
    let shared: Vec<(FileId, Arc<str>, String)> = inputs
        .iter()
        .map(|i| (i.id, Arc::clone(i.shared), i.path.as_str().to_owned()))
        .collect();
    let side_inputs: crate::warm::SideInputs = (
        lockfiles
            .iter()
            .map(|l| (l.id, Arc::clone(&l.text)))
            .collect(),
        manifests.clone(),
    );
    let mut program = Program::build(inputs);
    program.lockfiles = lockfiles;
    program.manifests = manifests;
    program.manifest_ids = manifest_ids;
    generate(&mut program);
    let cx = Cx::new(&program);
    cx.set_hooks(
        Box::new(|cx: &Cx<'_, '_>, member: MemberId| infer_field(cx, member)),
        Box::new(|cx: &Cx<'_, '_>, decl: DeclId| infer_constant(cx, decl)),
    );
    cx.index_extensions();
    project_checks(&cx);
    resolve_signatures(&cx);
    // Which files may replay their facts?
    let file_list: Vec<(FileId, String)> =
        shared.iter().map(|(id, _, p)| (*id, p.clone())).collect();
    let prev = prev.filter(|c| c.file_list == file_list);
    let roots: FxHashMap<FileId, &SyntaxNode> = program
        .files
        .iter()
        .filter(|f| !f.stdlib)
        .map(|f| (f.id, &f.root))
        .collect();
    let mut unchanged: FxHashSet<FileId> = FxHashSet::default();
    let mut signatures: FxHashMap<FileId, Arc<str>> = FxHashMap::default();
    let mut texts: FxHashMap<FileId, Arc<str>> = FxHashMap::default();
    let mut same_signatures = prev.is_some();
    for (id, text, _) in &shared {
        texts.insert(*id, Arc::clone(text));
        let before = prev.and_then(|c| c.files.get(id));
        match before {
            Some(f) if Arc::ptr_eq(&f.text, text) => {
                unchanged.insert(*id);
                signatures.insert(*id, Arc::clone(&f.signature));
            }
            _ => {
                let signature: Arc<str> = roots
                    .get(id)
                    .map_or_else(String::new, |r| crate::warm::signature_text(r))
                    .into();
                if before.is_none_or(|f| f.signature != signature) {
                    same_signatures = false;
                }
                signatures.insert(*id, signature);
            }
        }
    }
    let prev = prev.filter(|_| same_signatures);
    let mut facts_by_file: FxHashMap<FileId, Arc<FileFacts>> = FxHashMap::default();
    let mut type_chunks: Vec<Arc<RangeMap<Ty>>> = Vec::new();
    for ix in 0..program.files.len() {
        let file = FileIx(u32::try_from(ix).unwrap_or(u32::MAX));
        let info = program.file(file);
        if info.stdlib || info.scope_of.is_some() {
            continue;
        }
        let id = info.id;
        let replay = prev.and_then(|c| c.files.get(&id)).filter(|f| {
            unchanged.contains(&id)
                && f.deps
                    .iter()
                    .all(|d| unchanged.contains(d) || !texts.contains_key(d))
        });
        let facts = match replay {
            Some(f) => {
                replay_facts(&cx, f);
                Arc::clone(f)
            }
            None => {
                let text = texts.get(&id).cloned().unwrap_or_else(|| Arc::from(""));
                let signature = signatures
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| Arc::from(""));
                Arc::new(check_file_facts(&cx, file, text, signature))
            }
        };
        cx.body_resolutions
            .borrow_mut()
            .push(Arc::clone(&facts.resolutions));
        type_chunks.push(Arc::clone(&facts.types));
        facts_by_file.insert(id, facts);
    }
    crate::routing::check(&cx);
    crate::metadata::check(&cx);
    crate::rpc::check(&cx);
    crate::channels::check(&cx);
    crate::workers::check(&cx);
    crate::app::check(&cx);
    crate::di::check(&cx);
    crate::di::remote_services(&cx);
    let storage = crate::locks::check(&cx);
    crate::activation::check(&cx);
    crate::element_refs::check(&cx);
    crate::channels::opened_during_ssr(&cx);
    // The `rpc.lock` check reads no body: replayed while its inputs are
    // unchanged (`warm::PassFacts`).
    let same_side_inputs = prev.is_some_and(|c| c.side_inputs == side_inputs);
    let lock_files: FxHashSet<FileId> = side_inputs.0.iter().map(|(id, _)| *id).collect();
    let rpc_lock = match prev
        .filter(|_| same_side_inputs)
        .and_then(|c| c.rpc_lock.as_ref())
        .filter(|f| f.replayable(|id| unchanged.contains(&id) || lock_files.contains(&id)))
    {
        Some(facts) => {
            cx.diags.borrow_mut().extend(facts.diags.iter().cloned());
            Arc::clone(facts)
        }
        None => {
            let start = cx.diags.borrow().len();
            let value = crate::rpclock::check(&cx);
            let diags = cx.diags.borrow()[start..].to_vec();
            Arc::new(crate::warm::PassFacts { diags, value })
        }
    };
    let names = crate::ty::NameTable::new(&program);
    crate::di::client_references(&cx, &names);
    crate::di::server_references(&cx, &names);
    let sides = crate::di::sides(&cx);
    unused_imports(&cx);
    preview_imports(&cx);
    cx.fw.styles_checked.set(true);
    crate::framework::report_unused(&cx);
    let mut types = Chunked::default();
    types.push(Arc::new(std::mem::take(&mut *cx.types.borrow_mut())));
    for chunk in type_chunks {
        types.push(chunk);
    }
    let mut resolutions = Chunked::default();
    resolutions.push(Arc::new(std::mem::take(&mut *cx.resolutions.borrow_mut())));
    for chunk in cx.body_resolutions.borrow().iter() {
        resolutions.push(Arc::clone(chunk));
    }
    // Deprecated and preview uses, from every resolution of the revision.
    crate::stability::check(&cx, &resolutions);
    let diagnostics = std::mem::take(&mut *cx.diags.borrow_mut());
    // The view is built after the result is delivered (`view::ViewSlot`):
    // the caller reports the diagnostics meanwhile.
    let view = Arc::new(crate::view::ViewSlot::pending());
    let fill = crate::view::FillOnDrop(&view);
    let view_sides = sides.clone();
    deliver(SemaResult {
        diagnostics,
        names,
        types,
        resolutions,
        sides,
        cache: Arc::new(BodyCache {
            file_list,
            files: facts_by_file,
            side_inputs,
            rpc_lock: Some(Arc::clone(&rpc_lock)),
        }),
        view: Arc::clone(&view),
        rpc_lock: rpc_lock.value.clone(),
        storage_lock: storage.lock,
        collections: Arc::new(storage.collections),
    });
    // Every signature is resolved by now (`resolve_signatures`), so the
    // view records no type or resolution the delivered result would lack.
    // Diagnostics it causes (signatures no check needed) are dropped.
    view.fill(crate::view::build(&cx, &view_sides));
    debug_assert!(
        cx.types.borrow().is_empty() && cx.resolutions.borrow().is_empty(),
        "the program view resolved a signature `resolve_signatures` missed"
    );
    drop(fill);
    // `cx` and `program` are dropped here, after the result is delivered.
}

/// Resolves every project signature before any body is checked: field
/// types (inferring unannotated ones), function, initializer, rpc and case
/// signatures, `implements` lists, generic constraints, extension targets
/// and the types of top-level constants.
///
/// Signatures are memoized and their diagnostics reported once, by
/// whichever check first needs them. Resolving them all here, in
/// declaration order, makes those facts independent of the bodies, which
/// is what lets a warm re-check replay a file's body facts
/// ([`crate::warm`]). Linear in the members.
fn resolve_signatures(cx: &Cx<'_, '_>) {
    let p = cx.p;
    for (i, d) in p.decls.iter().enumerate() {
        if p.file(d.file).stdlib {
            continue;
        }
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        match d.named_kind() {
            // Checked by the framework layer alone, member signatures
            // included (`check_decl`), except `@Inject` fields: the
            // dependency-injection pass reads their types in every revision
            // (`crate::di`), so they are signature facts. Resolving them in
            // the body check instead would cache their diagnostics with the
            // file's body facts and report them twice when a warm re-check
            // replays the file.
            Some(DeclarationKind::App | DeclarationKind::Migration) => {
                for (m, _) in crate::di::inject_fields(cx, id) {
                    let _ = cx.field_ty(m);
                }
                continue;
            }
            Some(DeclarationKind::Function) => {
                let _ = cx.sig(SigKey::Decl(id));
                // The `where` clause too: the view reads it, and a body
                // calling the function must not be the first to resolve it.
                for &g in &d.generics {
                    let _ = cx.constraints(g);
                }
                continue;
            }
            Some(DeclarationKind::Constant) => {
                let _ = cx.constant_ty(id);
                continue;
            }
            // A preview's parameters are its signature, read by the view
            // (`crate::preview`); its fields follow below.
            Some(DeclarationKind::Preview) => {
                let _ = cx.sig(SigKey::Decl(id));
            }
            _ => {}
        }
        if d.kind == DeclKind::Extension {
            let _ = cx.ext_target(id);
        } else {
            let _ = cx.implements_list(id);
        }
        for &g in &d.generics {
            let _ = cx.constraints(g);
        }
        for &m in &d.members {
            let member = p.member(m);
            for &g in &member.generics {
                let _ = cx.constraints(g);
            }
            match member.kind {
                MemberKind::Field | MemberKind::Computed => {
                    let _ = cx.field_ty(m);
                }
                MemberKind::Case => {
                    let _ = cx.case_sig(m);
                }
                MemberKind::Func | MemberKind::Rpc | MemberKind::Init => {
                    let _ = cx.sig(SigKey::Member(m));
                }
                _ => {}
            }
        }
    }
}

/// Checks the bodies of one file (its declarations and doc links) and
/// returns what that produced ([`FileFacts`]).
///
/// The live types, resolutions and side-fact sets are swapped out while the
/// file is checked, so what remains in them afterwards is exactly the
/// file's; the side facts are then merged back (the project passes read
/// them), while the types and resolutions stay in the file's own chunk.
/// The diagnostics stay in the live list too (client references skip spans
/// already reported) and are copied into the facts.
fn check_file_facts(
    cx: &Cx<'_, '_>,
    file: FileIx,
    text: Arc<str>,
    signature: Arc<str>,
) -> FileFacts {
    let diag_start = cx.diags.borrow().len();
    let saved_types = std::mem::take(&mut *cx.types.borrow_mut());
    let saved_resolutions = std::mem::take(&mut *cx.resolutions.borrow_mut());
    let saved_imports = std::mem::take(&mut *cx.used_imports.borrow_mut());
    let saved_classes = std::mem::take(&mut *cx.fw.used_classes.borrow_mut());
    let saved_keyframes = std::mem::take(&mut *cx.fw.used_keyframes.borrow_mut());
    let saved_stylables = std::mem::take(&mut *cx.fw.applied_stylables.borrow_mut());
    let saved_loads = std::mem::take(&mut *cx.fw.loads.borrow_mut());
    let mut decls = cx.p.file(file).decls.clone();
    // A file's previews are checked with it, in its preview scope.
    if let Some(scope) = cx.p.file(file).previews {
        decls.extend(cx.p.file(scope).decls.iter().copied());
    }
    for &decl in &decls {
        check_decl(cx, decl);
    }
    crate::preview::check_names(cx, &decls);
    crate::doclinks::check_file(cx, file);
    let diags: Vec<Diagnostic> = cx.diags.borrow()[diag_start..].to_vec();
    let types = std::mem::replace(&mut *cx.types.borrow_mut(), saved_types);
    let resolutions = std::mem::replace(&mut *cx.resolutions.borrow_mut(), saved_resolutions);
    let imports = std::mem::replace(&mut *cx.used_imports.borrow_mut(), saved_imports);
    let classes = std::mem::replace(&mut *cx.fw.used_classes.borrow_mut(), saved_classes);
    let keyframes = std::mem::replace(&mut *cx.fw.used_keyframes.borrow_mut(), saved_keyframes);
    let stylables = std::mem::replace(&mut *cx.fw.applied_stylables.borrow_mut(), saved_stylables);
    let loads = std::mem::replace(&mut *cx.fw.loads.borrow_mut(), saved_loads);
    let mut used_imports: Vec<(FileIx, usize)> = imports.into_iter().collect();
    used_imports.sort_unstable();
    let mut used_classes: Vec<(DeclId, String)> = classes.into_iter().collect();
    used_classes.sort();
    let mut used_keyframes: Vec<(DeclId, String)> = keyframes.into_iter().collect();
    used_keyframes.sort();
    let mut applied_stylables: Vec<MemberId> = stylables.into_iter().collect();
    applied_stylables.sort_unstable();
    let mut loads: Vec<(MemberId, (Option<Ty>, TextRange))> = loads.into_iter().collect();
    loads.sort_by_key(|(m, _)| *m);
    let own = cx.p.file(file).id;
    let facts = FileFacts {
        text,
        signature,
        deps: crate::warm::foreign_files(own, &diags),
        diags,
        types: Arc::new(types),
        resolutions: Arc::new(resolutions),
        used_imports,
        used_classes,
        used_keyframes,
        applied_stylables,
        loads,
    };
    replay_side_facts(cx, &facts);
    facts
}

/// Replays a file's facts from the previous revision: its diagnostics and
/// side facts join the live ones; its types and resolutions are shared as
/// chunks by the caller.
fn replay_facts(cx: &Cx<'_, '_>, facts: &FileFacts) {
    cx.diags.borrow_mut().extend(facts.diags.iter().cloned());
    replay_side_facts(cx, facts);
}

/// Merges a file's side facts into the live sets the project passes read.
fn replay_side_facts(cx: &Cx<'_, '_>, facts: &FileFacts) {
    cx.used_imports
        .borrow_mut()
        .extend(facts.used_imports.iter().copied());
    cx.fw
        .used_classes
        .borrow_mut()
        .extend(facts.used_classes.iter().cloned());
    cx.fw
        .used_keyframes
        .borrow_mut()
        .extend(facts.used_keyframes.iter().cloned());
    cx.fw
        .applied_stylables
        .borrow_mut()
        .extend(facts.applied_stylables.iter().copied());
    cx.fw.loads.borrow_mut().extend(facts.loads.iter().cloned());
}

/// Adds generated members (sema.md §5.1, §12.7): a `Page` case per page with
/// a view, an `Endpoint` case per endpoint page, and a `ThemeVariant` case
/// per theme variant. Each page case remembers its page
/// ([`Program::page_of`]), whose `@Param`s are its payload.
fn generate(program: &mut Program<'_>) {
    let page = program.lookup_qualified("k.core.Page");
    let endpoint = program.lookup_qualified("k.core.Endpoint");
    let variant = program.lookup_qualified("k.core.ThemeVariant");
    let mut pages = Vec::new();
    let mut variants = Vec::new();
    for (i, decl) in program.decls.iter().enumerate() {
        if program.files[decl.file.0 as usize].stdlib {
            continue;
        }
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        match decl.named_kind() {
            Some(DeclarationKind::Page) => {
                let mut name = decl.name.clone();
                if let Some(first) = name.get(..1) {
                    name = first.to_lowercase() + &name[1..];
                }
                let is_endpoint = decl.members.iter().any(|m| {
                    let m = &program.members[m.0 as usize];
                    m.kind == MemberKind::Func
                        && crate::framework::HANDLER_METHODS
                            .iter()
                            .any(|h| m.has_annotation(h))
                });
                pages.push((name, decl.name_range, id, is_endpoint));
            }
            Some(DeclarationKind::Theme) => {
                for v in decl
                    .node
                    .descendants()
                    .filter(|n| n.kind() == SyntaxKind::ThemeVariant)
                {
                    if let Some(name) = ast::ThemeVariant::cast(v).and_then(|v| v.name()) {
                        variants.push((name.text().to_owned(), name.text_range()));
                    }
                }
            }
            _ => {}
        }
    }
    for (name, range, decl, is_endpoint) in pages {
        let owner = if is_endpoint { endpoint } else { page };
        if let Some(owner) = owner
            && !program.decl(owner).member_names.contains_key(&name)
        {
            let m = program.add_generated(owner, &name, MemberKind::Case, range);
            program.page_of.insert(m, decl);
        }
    }
    if let Some(variant) = variant {
        for (name, range) in variants {
            if !program.decl(variant).member_names.contains_key(&name) {
                program.add_generated(variant, &name, MemberKind::Case, range);
            }
        }
    }
}

/// Returns where the bodies of `decl` are checked: its file and generics,
/// and, for types, extensions and named test suites, the declaration that
/// owns `self` (an extension's target type) and the type of `self`.
fn owner_of(cx: &Cx<'_, '_>, decl: DeclId) -> BodyOwner {
    let d = cx.p.decl(decl);
    let typed = d.is_type()
        || d.kind == DeclKind::Extension
        || (d.named_kind() == Some(DeclarationKind::TestSuite) && !d.name.is_empty())
        || d.named_kind() == Some(DeclarationKind::Preview);
    BodyOwner {
        file: d.file,
        decl: typed.then_some(if d.kind == DeclKind::Extension {
            cx.ext_target(decl).decl().unwrap_or(decl)
        } else {
            decl
        }),
        self_ty: typed.then(|| cx.self_type(decl)),
        generics: d.generics.clone(),
    }
}

/// Returns [`owner_of`] for member bodies: inside an extension the private
/// members visible are the extension's own (sema.md §6), not the extended
/// type's.
fn owner_with_ext(cx: &Cx<'_, '_>, decl: DeclId) -> BodyOwner {
    let mut owner = owner_of(cx, decl);
    // Inside an extension, private members of the extended type are not
    // visible (sema.md §6): the body owner is the extension itself.
    if cx.p.decl(decl).kind == DeclKind::Extension {
        owner.decl = Some(decl);
    }
    owner
}

/// Infers the type of an unannotated field from its initializer, silently
/// (the body pass reports later); `{error}` without an initializer.
///
/// Installed as the context's field hook, so field types can be inferred
/// on demand in any order.
fn infer_field(cx: &Cx<'_, '_>, member: MemberId) -> Ty {
    let m = cx.p.member(member);
    let Some(field) = m
        .node
        .as_ref()
        .and_then(|n| ast::FieldDecl::cast(n.clone()))
    else {
        return Ty::Error;
    };
    let Some(init) = field.initializer() else {
        return Ty::Error;
    };
    let mut checker = Checker::new(cx, owner_with_ext(cx, m.owner)).silent();
    checker.check_initializer(&init, None)
}

/// Infers the type of an unannotated top-level constant from its
/// initializer, silently; `{error}` without an initializer. Installed as
/// the context's constant hook, like [`infer_field`].
fn infer_constant(cx: &Cx<'_, '_>, decl: DeclId) -> Ty {
    let d = cx.p.decl(decl);
    let Some(field) = ast::FieldDecl::cast(d.node.clone()) else {
        return Ty::Error;
    };
    let Some(init) = field.initializer() else {
        return Ty::Error;
    };
    let mut checker = Checker::new(cx, owner_of(cx, decl)).silent();
    checker.check_initializer(&init, None)
}

// ------------------------------------------------------------ project checks

/// Runs the project-wide declaration checks: stray files (K1001), the
/// file rules (K1002, K1003, K1005), duplicate declarations per module
/// (K1004) and members (K2019), import cycles (K1103) and the extension
/// rules.
fn project_checks(cx: &Cx<'_, '_>) {
    let p = cx.p;
    for (ix, file) in p.files.iter().enumerate() {
        let fix = FileIx(u32::try_from(ix).unwrap_or(u32::MAX));
        // A preview scope's file rules are its production file's.
        if file.stdlib || file.scope_of.is_some() {
            continue;
        }
        if let Some(path) = file.path
            && path.kind() == crate::project::FileKind::Stray
        {
            cx.report(
                fix,
                Diagnostic::new(
                    codes::STRAY_SOURCE_FILE,
                    Span::empty(file.id, TextSize::from(0)),
                    "this `.k` file is outside the project layout",
                ),
            );
        }
        file_rule(cx, fix);
        misplaced(cx, fix);
    }
    // Duplicate declarations per module (K1004).
    for module in &p.modules {
        if module.stdlib {
            continue;
        }
        for names in [&module.names, &module.test_names] {
            let mut keys: Vec<&String> = names.keys().collect();
            keys.sort();
            for name in keys {
                let list = &names[name];
                let mut sorted = list.clone();
                sorted.sort_by_key(|d| {
                    (
                        p.file(p.decl(*d).file).path.map(|x| x.as_str().to_owned()),
                        p.decl(*d).name_range.start(),
                    )
                });
                let Some(&first) = sorted.first() else {
                    continue;
                };
                let first = p.decl(first);
                for d in sorted.iter().skip(1) {
                    let decl = p.decl(*d);
                    cx.report(
                        decl.file,
                        Diagnostic::new(
                            codes::DUPLICATE_DECLARATION,
                            p.span(decl.file, decl.name_range),
                            format!("`{name}` is declared more than once in this module"),
                        )
                        .with_label(p.span(first.file, first.name_range), "first declared here"),
                    );
                }
            }
        }
    }
    duplicate_members(cx);
    import_cycles(cx);
    // Extensions: unknown targets, `private extend`, stored fields, and
    // conflicting members.
    let mut seen: HashMap<(DeclId, String), Vec<DeclId>> = HashMap::new();
    for &ext in &p.extensions {
        let decl = p.decl(ext);
        let target = cx.ext_target(ext);
        if p.file(decl.file).stdlib {
            if let Some(t) = target.decl() {
                for &m in &decl.members {
                    seen.entry((t, p.member(m).name.clone()))
                        .or_default()
                        .push(ext);
                }
            }
            continue;
        }
        if decl.visibility == Visibility::Private
            && let Some(tok) = visibility_token(&decl.node)
        {
            let end = next_non_space(p.file(decl.file).text, tok.text_range().end());
            cx.report(
                decl.file,
                Diagnostic::new(
                    codes::PRIVATE_EXTEND,
                    p.span(decl.file, tok.text_range()),
                    "an extension can't be `private`",
                )
                .with_fix(
                    Fix::replace(
                        "Remove `private`",
                        p.span(decl.file, TextRange::new(tok.text_range().start(), end)),
                        "",
                    )
                    .maybe_incorrect(),
                ),
            );
        }
        for &m in &decl.members {
            let member = p.member(m);
            if member.kind == MemberKind::Field && !member.is_static {
                cx.error(
                    decl.file,
                    codes::STORED_FIELD_IN_EXTENSION,
                    member.name_range,
                    "an extension can't add stored fields",
                );
            }
        }
        let Some(t) = target.decl() else { continue };
        for &m in &decl.members {
            let member = p.member(m);
            let key = (t, member.name.clone());
            let own = p.decl(t).member_names.contains_key(&member.name);
            let earlier = seen.get(&key).cloned().unwrap_or_default();
            let conflict = own
                || earlier.iter().any(|e| {
                    let ed = p.decl(*e);
                    ed.visibility == Visibility::Public
                        || decl.visibility == Visibility::Public
                        || p.file(ed.file).module == p.file(decl.file).module
                });
            if conflict {
                cx.error(
                    decl.file,
                    codes::CONFLICTING_EXTENSION_MEMBER,
                    member.name_range,
                    format!(
                        "`{}` already has a member `{}`",
                        p.decl_name(t),
                        member.name
                    ),
                );
            }
            seen.entry(key).or_default().push(ext);
        }
    }
}

/// K1005: `app` only in `App.k` (and once), `migration` only in
/// `migrations/` (one per file, alone), `test` only in `*.test.k` files.
fn misplaced(cx: &Cx<'_, '_>, file: FileIx) {
    use crate::project::FileKind;
    let p = cx.p;
    let info = p.file(file);
    let Some(path) = info.path else { return };
    let kind = path.kind();
    let keyword = |d: DeclId, k: SyntaxKind| {
        p.decl(d)
            .node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == k)
            .map_or(p.decl(d).name_range, |t| t.text_range())
    };
    let mut apps = 0;
    let mut migrations = 0;
    for &d in &info.decls {
        let decl = p.decl(d);
        match decl.named_kind() {
            Some(DeclarationKind::App) => {
                apps += 1;
                if kind != FileKind::App || apps > 1 {
                    cx.error(
                        file,
                        codes::MISPLACED_DECLARATION,
                        keyword(d, SyntaxKind::AppKw),
                        if kind == FileKind::App {
                            "a project has exactly one `app`"
                        } else {
                            "`app` is declared only in `App.k`"
                        },
                    );
                }
            }
            Some(DeclarationKind::Migration) => {
                migrations += 1;
                if kind != FileKind::Migration || migrations > 1 {
                    cx.error(
                        file,
                        codes::MISPLACED_DECLARATION,
                        keyword(d, SyntaxKind::MigrationKw),
                        if kind == FileKind::Migration {
                            "a migration file holds one `migration`"
                        } else {
                            "a `migration` lives in `migrations/`"
                        },
                    );
                }
            }
            Some(DeclarationKind::TestSuite) if !info.is_test => {
                cx.error(
                    file,
                    codes::MISPLACED_DECLARATION,
                    keyword(d, SyntaxKind::TestKw),
                    "tests are declared in `*.test.k` files",
                );
            }
            _ if kind == FileKind::Migration && decl.kind != DeclKind::Extension => {
                cx.error(
                    file,
                    codes::MISPLACED_DECLARATION,
                    decl.name_range,
                    "a migration file holds only its `migration`",
                );
            }
            _ => {}
        }
    }
}

/// Returns the offset of the first character at or after `offset` that is
/// neither a space nor a tab; fixes that remove a keyword also remove the
/// space after it.
///
/// # Panics
///
/// Panics if `offset` is past the end of `text` or not on a character
/// boundary.
fn next_non_space(text: &str, offset: TextSize) -> TextSize {
    let start = usize::from(offset);
    let rest = &text[start..];
    let n = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    offset + TextSize::from(u32::try_from(n).unwrap_or(0))
}

/// Checks the one-public-declaration rule of a source file (sema.md §4):
/// at most one public declaration (K1003), named after the file (K1002,
/// with a fix when only the case differs); a test file has none.
fn file_rule(cx: &Cx<'_, '_>, file: FileIx) {
    let p = cx.p;
    let info = p.file(file);
    let Some(path) = info.path else { return };
    if info.is_test {
        // A test file has no public declaration (sema.md §4).
        for &d in &info.decls {
            let decl = p.decl(d);
            if decl.kind != DeclKind::Extension && decl.visibility == Visibility::Public {
                cx.error(
                    file,
                    codes::MULTIPLE_PUBLIC_DECLARATIONS,
                    decl.name_range,
                    "a test file has no public declaration",
                );
            }
        }
        return;
    }
    if !matches!(path.kind(), crate::project::FileKind::Source { .. }) {
        return;
    }
    let stem = path.file_name().trim_end_matches(".k");
    let mut first: Option<DeclId> = None;
    for &d in &info.decls {
        let decl = p.decl(d);
        if decl.kind == DeclKind::Extension
            || decl.visibility != Visibility::Public
            || decl.name.is_empty()
        {
            continue;
        }
        if first.is_none() {
            first = Some(d);
            if decl.name != stem {
                let mut diag = Diagnostic::new(
                    codes::PUBLIC_NAME_MISMATCH,
                    p.span(file, decl.name_range),
                    format!(
                        "the public declaration of `{}` must be named `{stem}`",
                        path.file_name()
                    ),
                );
                if decl.name.eq_ignore_ascii_case(stem) {
                    diag = diag.with_fix(
                        Fix::replace(
                            format!("Rename `{}` to `{stem}`", decl.name),
                            p.span(file, decl.name_range),
                            stem,
                        )
                        .maybe_incorrect(),
                    );
                }
                cx.report(file, diag);
            }
            continue;
        }
        cx.error(
            file,
            codes::MULTIPLE_PUBLIC_DECLARATIONS,
            decl.name_range,
            "a file holds at most one public declaration",
        );
    }
}

/// K2019: two fields, computed properties or cases of one declaration with
/// the same name in the same namespace (instance members; static members
/// and cases). Functions are exempt: they may be overloaded and may share a
/// name with a property. Linear in the members.
fn duplicate_members(cx: &Cx<'_, '_>) {
    let p = cx.p;
    for decl in &p.decls {
        if p.file(decl.file).stdlib || decl.kind == DeclKind::Extension {
            continue;
        }
        let mut first: FxHashMap<(&str, bool), MemberId> = FxHashMap::default();
        for &m in &decl.members {
            let member = p.member(m);
            let is_static = match member.kind {
                MemberKind::Case => true,
                MemberKind::Field | MemberKind::Computed => member.is_static,
                _ => continue,
            };
            if member.node.is_none() {
                continue;
            }
            let earlier = *first.entry((member.name.as_str(), is_static)).or_insert(m);
            if earlier == m {
                continue;
            }
            let earlier = p.member(earlier);
            cx.report(
                decl.file,
                Diagnostic::new(
                    codes::DUPLICATE_MEMBER,
                    p.span(decl.file, member.name_range),
                    format!(
                        "`{}` is declared more than once in `{}`",
                        member.name, decl.name
                    ),
                )
                .with_label(p.span(decl.file, earlier.name_range), "first declared here"),
            );
        }
    }
}

/// Reports K1103 on every import between two project modules of the same
/// strongly connected component (test files' imports excluded), labelled
/// with the imports that lead back along the shortest path.
///
/// Tarjan's algorithm over the module graph: linear in modules and
/// imports, plus one breadth-first search per distinct cyclic edge.
fn import_cycles(cx: &Cx<'_, '_>) {
    let p = cx.p;
    // Edges between project modules from non-test files' imports.
    let n = p.modules.len();
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut import_edges: Vec<(FileIx, usize, usize, usize)> = Vec::new();
    for (ix, file) in p.files.iter().enumerate() {
        if file.stdlib || file.is_test {
            continue;
        }
        let Some(from) = file.module else { continue };
        for (i, res) in cx.imports[ix].iter().enumerate() {
            let ImportRes::Bound(_, d) = res else {
                continue;
            };
            let Some(to) = p.decl_module(*d) else {
                continue;
            };
            if to == from || p.modules[to.0 as usize].stdlib {
                continue;
            }
            // A file's import of a test-only module for its previews is
            // test code's, like a test file's (`preview_imports`).
            if file.previews.is_some() && p.modules[to.0 as usize].test_only {
                continue;
            }
            edges[from.0 as usize].push(to.0 as usize);
            import_edges.push((
                FileIx(u32::try_from(ix).unwrap_or(u32::MAX)),
                i,
                from.0 as usize,
                to.0 as usize,
            ));
        }
    }
    let scc = tarjan(&edges);
    // The first import (in file order) of each module edge inside a cycle:
    // the labels of a cycle name these.
    let mut first_import: FxHashMap<(usize, usize), (FileIx, usize)> = FxHashMap::default();
    for &(file, i, from, to) in &import_edges {
        if scc[from] == scc[to] {
            first_import.entry((from, to)).or_insert((file, i));
        }
    }
    let mut path_cache: FxHashMap<(usize, usize), Vec<usize>> = FxHashMap::default();
    for (file, i, from, to) in import_edges {
        if scc[from] != scc[to] {
            continue;
        }
        let range = p.file(file).imports[i].range;
        let mut diag = Diagnostic::new(
            codes::IMPORT_CYCLE,
            p.span(file, range),
            "this import is part of an import cycle",
        );
        // The rest of the cycle: the shortest path back from `to` to `from`.
        let path = path_cache
            .entry((to, from))
            .or_insert_with(|| shortest_path_in_component(&edges, &scc, to, from));
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let Some(&(at_file, at)) = first_import.get(&(a, b)) else {
                continue;
            };
            diag = diag.with_label(
                p.span(at_file, p.file(at_file).imports[at].range),
                format!(
                    "then `{}` imports `{}`",
                    p.modules[a].path, p.modules[b].path
                ),
            );
        }
        cx.report(file, diag);
    }
}

/// The K2010 fix: inserts a stub for each missing requirement before the
/// closing `}` of `decl`'s member list (on its own line when the `}` starts
/// one), copying the requirement's signature and giving functions an empty
/// body. Maybe-incorrect: the bodies are the user's to write.
///
/// `None` when the declaration has no member list to insert into.
fn interface_stubs_fix(cx: &Cx<'_, '_>, decl: DeclId, missing: &[MemberId]) -> Option<Fix> {
    let p = cx.p;
    let d = p.decl(decl);
    let close = d
        .node
        .children()
        .find(|c| c.kind() == SyntaxKind::MemberList)
        .and_then(|list| token(&list, SyntaxKind::RBrace))?
        .text_range()
        .start();
    let text = p.file(d.file).text;
    let line_start = text[..usize::from(close)].rfind('\n').map_or(0, |i| i + 1);
    let at = if text[line_start..usize::from(close)].trim().is_empty() {
        TextSize::try_from(line_start).unwrap_or(close)
    } else {
        close
    };
    let mut out = String::new();
    for &m in missing {
        let member = p.member(m);
        let Some(node) = &member.node else { continue };
        // The signature without leading doc comments and trivia.
        let start = node
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| !t.kind().is_trivia())
            .map_or(node.text_range().start(), |t| t.text_range().start());
        let end = node.text_range().end();
        let rtext = p.file(p.decl(member.owner).file).text;
        let signature = rtext[usize::from(start)..usize::from(end)].trim_end();
        out.push_str("\n    ");
        out.push_str(signature);
        if member.kind == MemberKind::Func {
            out.push_str(" {\n    }");
        }
        out.push('\n');
    }
    if out.is_empty() {
        return None;
    }
    Some(
        Fix::replace(
            "Insert stubs for the missing members",
            Span::empty(p.file(d.file).id, at),
            out,
        )
        .maybe_incorrect(),
    )
}

/// Returns the nodes of a shortest path from `start` to `goal` that stays
/// inside their strongly connected component, both ends included (just
/// `[start]` when they are equal, empty when no path exists).
///
/// A breadth-first search: O(V + E) of the component per call.
pub(crate) fn shortest_path_in_component(
    edges: &[Vec<usize>],
    scc: &[usize],
    start: usize,
    goal: usize,
) -> Vec<usize> {
    if start == goal {
        return vec![start];
    }
    let comp = scc[start];
    let mut parent: FxHashMap<usize, usize> = FxHashMap::default();
    parent.insert(start, start);
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for &next in &edges[node] {
            if scc[next] != comp || parent.contains_key(&next) {
                continue;
            }
            parent.insert(next, node);
            if next == goal {
                let mut path = vec![goal];
                let mut at = goal;
                while at != start {
                    at = parent[&at];
                    path.push(at);
                }
                path.reverse();
                return path;
            }
            queue.push_back(next);
        }
    }
    Vec::new()
}

/// Tarjan's strongly connected components; returns the component of each
/// node (iterative, so deep graphs don't overflow the stack).
pub(crate) fn tarjan(edges: &[Vec<usize>]) -> Vec<usize> {
    let n = edges.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut comp = vec![usize::MAX; n];
    let mut stack = Vec::new();
    let mut next = 0;
    let mut comps = 0;
    for start in 0..n {
        if index[start] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&mut (v, ref mut ei)) = work.last_mut() {
            if *ei == 0 && index[v] == usize::MAX {
                index[v] = next;
                low[v] = next;
                next += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            if *ei < edges[v].len() {
                let w = edges[v][*ei];
                *ei += 1;
                if index[w] == usize::MAX {
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == index[v] {
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    comp[w] = comps;
                    if w == v {
                        break;
                    }
                }
                comps += 1;
            }
        }
    }
    // Singleton components without a self-edge are not cycles: give each
    // node of a singleton a unique id so `scc[a] == scc[b]` means a cycle.
    comp
}

/// Reports K1105 for bound imports that no checked body used and that
/// neither a doc comment nor an identifier elsewhere in the file mentions, with a fix removing the import.
fn unused_imports(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let used = cx.used_imports.borrow();
    for (ix, file) in p.files.iter().enumerate() {
        // A preview scope's imports are its production file's.
        if file.stdlib || file.imports.is_empty() || file.scope_of.is_some() {
            continue;
        }
        // Collected only when some import isn't used by a checked body:
        // most files never need the scan.
        let mut idents: Option<HashSet<String>> = None;
        let fix = FileIx(u32::try_from(ix).unwrap_or(u32::MAX));
        for (i, res) in cx.imports[ix].iter().enumerate() {
            let in_previews = file.previews.is_some_and(|s| used.contains(&(s, i)));
            if !matches!(res, ImportRes::Bound(..)) || used.contains(&(fix, i)) || in_previews {
                continue;
            }
            let import = &file.imports[i];
            if mentioned_in_docs(file.text, &import.path)
                || mentioned_in_code(
                    idents.get_or_insert_with(|| identifiers(&file.root)),
                    &import.path,
                )
            {
                continue;
            }
            cx.report(
                fix,
                Diagnostic::new(
                    codes::UNUSED_IMPORT,
                    p.span(fix, import.range),
                    format!("`{}` is imported but never used", import.path),
                )
                .with_fix(Fix::replace(
                    "Remove the import",
                    p.span(fix, cx.line_range(fix, import.node_range)),
                    "",
                )),
            );
        }
    }
}

/// K1104 for a production file with previews: it may import a test-only
/// module for them (docs/design/57-previews.md, "What a preview can
/// reach"), so the import itself isn't reported (`Cx::resolve_imports`);
/// each use of the imported name outside the file's previews is, at the
/// use.
fn preview_imports(cx: &Cx<'_, '_>) {
    let p = cx.p;
    for (ix, file) in p.files.iter().enumerate() {
        if file.previews.is_none() || file.stdlib || file.is_test {
            continue;
        }
        let own_test_only = file
            .module
            .is_some_and(|m| p.modules[m.0 as usize].test_only);
        if own_test_only {
            continue;
        }
        let fix = FileIx(u32::try_from(ix).unwrap_or(u32::MAX));
        for (i, res) in cx.imports[ix].iter().enumerate() {
            let ImportRes::Bound(name, d) = res else {
                continue;
            };
            let test_only = p.decl_module(*d).is_some_and(|m| {
                p.modules[m.0 as usize].test_only && !p.modules[m.0 as usize].stdlib
            });
            if !test_only {
                continue;
            }
            let path = &file.imports[i].path;
            let uses = file
                .root
                .children()
                .filter(|c| !matches!(c.kind(), SyntaxKind::Import | SyntaxKind::PreviewDecl))
                .flat_map(|c| c.descendants_with_tokens())
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::Ident && t.text() == name);
            for token in uses {
                cx.error(
                    fix,
                    codes::TEST_ONLY_IMPORT,
                    token.text_range(),
                    format!(
                        "`{path}` is test-only: this file imports it for its previews, and only they can use it"
                    ),
                );
            }
        }
    }
}

/// Returns `true` if the imported name appears as an identifier outside
/// the imports: annotations, `use` rules, guards, views and styles use
/// declarations without going through body checking.
fn mentioned_in_code(idents: &HashSet<String>, path: &str) -> bool {
    let name = path.rsplit('.').next().unwrap_or(path);
    idents.contains(name)
}

/// The identifiers of a file outside its imports.
fn identifiers(root: &SyntaxNode) -> HashSet<String> {
    root.children()
        .filter(|c| c.kind() != SyntaxKind::Import)
        .flat_map(|c| c.descendants_with_tokens())
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::Ident)
        .map(|t| t.text().to_owned())
        .collect()
}

/// Returns `true` if a doc comment links the imported name (a doc link is a
/// use, sema.md §7.5).
fn mentioned_in_docs(text: &str, path: &str) -> bool {
    let name = path.rsplit('.').next().unwrap_or(path);
    let link = format!("[`{name}");
    text.lines()
        .filter(|l| l.trim_start().starts_with("///"))
        .any(|l| l.contains(&link))
}

// ------------------------------------------------------------ declarations

/// Checks one top-level declaration: a function's or constant's body, the
/// `app`/`migration` blocks, a top-level test case, then every member and
/// the nested blocks (view, style, metadata, `@Load` states, test suites),
/// and the member modifiers.
fn check_decl(cx: &Cx<'_, '_>, decl: DeclId) {
    let p = cx.p;
    let d = p.decl(decl);
    match d.named_kind() {
        Some(DeclarationKind::Function) => {
            let sig = cx.sig(SigKey::Decl(decl));
            let mut checker = Checker::new(cx, owner_of(cx, decl));
            let func = ast::FuncDecl::cast(d.node.clone());
            let name = func.as_ref().and_then(HasName::name);
            if let Some(name) = name {
                let t = Ty::func(
                    sig.params.iter().map(|p| p.ty.clone()).collect(),
                    sig.is_async,
                    sig.throws.clone(),
                    sig.ret.clone(),
                );
                cx.record_type(d.file, name.text_range(), t);
            }
            let body = func.as_ref().and_then(ast::FuncDecl::body);
            checker.check_function(&d.node, &sig, body, false, false);
            checker.finish();
            return;
        }
        Some(DeclarationKind::Constant) => {
            let Some(field) = ast::FieldDecl::cast(d.node.clone()) else {
                return;
            };
            let annotated = field.ty().map(|_| cx.constant_ty(decl));
            if let Some(init) = field.initializer() {
                let mut checker = Checker::new(cx, owner_of(cx, decl));
                let ty = checker.check_initializer(&init, annotated.as_ref());
                if let Some(name) = field.name() {
                    cx.record_type(d.file, name.text_range(), ty);
                }
                checker.finish();
            }
            return;
        }
        Some(DeclarationKind::App | DeclarationKind::Migration) => {
            crate::framework::check_declaration(cx, decl, &owner_of(cx, decl));
            return;
        }
        // A preview: its `@Inject` fields as a test group's, then its
        // parameters and body (`crate::preview`).
        Some(DeclarationKind::Preview) => {
            crate::framework::check_declaration(cx, decl, &owner_of(cx, decl));
            for m in d.members.clone() {
                check_member(cx, decl, m);
            }
            crate::preview::check(cx, decl, owner_of(cx, decl));
            return;
        }
        _ => {}
    }
    if d.kind == DeclKind::Named(DeclarationKind::TestSuite) && d.name.is_empty() {
        // A top-level `test "…" { }` case.
        if let Some(block) = d.node.children().find_map(ast::Block::cast) {
            let mut checker = Checker::new(cx, owner_of(cx, decl));
            let sig = test_sig(cx);
            checker.check_function(&d.node, &sig, Some(block), false, true);
            checker.finish();
        }
        return;
    }
    member_modifiers(cx, decl);
    crate::framework::check_declaration(cx, decl, &owner_with_ext(cx, decl));
    conformance(cx, decl);
    let members = d.members.clone();
    for m in members {
        check_member(cx, decl, m);
    }
    crate::framework::load_rules(cx, decl);
    // Blocks and nested test cases that aren't members with names.
    if let Some(list) = d
        .node
        .children()
        .find(|c| c.kind() == SyntaxKind::MemberList)
    {
        for child in list.children() {
            match child.kind() {
                SyntaxKind::ViewBlock
                | SyntaxKind::LoadingBlock
                | SyntaxKind::FailedBlock
                | SyntaxKind::MetadataBlock => {
                    if let Some(block) = child.children().find_map(ast::Block::cast) {
                        let mut checker = Checker::new(cx, owner_with_ext(cx, decl));
                        // `failed(let error)` binds the loads' error.
                        if let Some(pattern) = child.children().find_map(ast::Pattern::cast) {
                            let error = crate::framework::load_error(cx, decl);
                            checker.bind_names_typed(&pattern, &error);
                        }
                        if child.kind() == SyntaxKind::LoadingBlock
                            && crate::di::client_component(cx, decl)
                        {
                            checker.props_only();
                        }
                        if child.kind() == SyntaxKind::MetadataBlock {
                            checker.check_metadata_block(&block);
                        } else {
                            checker.check_view_block(&block);
                        }
                        checker.finish();
                    }
                }
                SyntaxKind::StyleBlock => {
                    let mut checker = Checker::new(cx, owner_with_ext(cx, decl));
                    checker.check_style_block(&child);
                    checker.finish();
                }
                SyntaxKind::TestCaseDecl => check_test_case(cx, decl, &child),
                SyntaxKind::TestSuiteDecl => {
                    for case in child
                        .descendants()
                        .filter(|c| c.kind() == SyntaxKind::TestCaseDecl)
                    {
                        check_test_case(cx, decl, &case);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Returns the signature a test case body is checked with: no
/// parameters, `async`, not throwing, returning `Void`.
fn test_sig(cx: &Cx<'_, '_>) -> crate::cx::Sig {
    crate::cx::Sig {
        generics: Vec::new(),
        params: Vec::new(),
        is_async: true,
        throws: None,
        ret: builtin(cx.b.void),
    }
}

/// Checks the body of a `test "…" { … }` case of `decl` (a suite or the
/// file) with [`test_sig`].
fn check_test_case(cx: &Cx<'_, '_>, decl: DeclId, node: &SyntaxNode) {
    let Some(block) = node.children().find_map(ast::Block::cast) else {
        return;
    };
    let owner = owner_of(cx, decl);
    let mut checker = Checker::new(cx, owner);
    let sig = test_sig(cx);
    checker.check_function(node, &sig, Some(block), false, true);
    checker.finish();
}

/// Checks one member of `decl`: a field's or computed property's type and
/// initializer, a function's or initializer's body, a case's payload.
fn check_member(cx: &Cx<'_, '_>, decl: DeclId, m: MemberId) {
    let p = cx.p;
    let member = p.member(m);
    let d = p.decl(decl);
    if d.named_kind() == Some(DeclarationKind::TestSuite) {
        // Test suites: injected fields and helpers; cases are handled above.
    }
    let Some(node) = member.node.clone() else {
        return;
    };
    let owner = owner_with_ext(cx, decl);
    match member.kind {
        MemberKind::Field | MemberKind::Computed => {
            let field = ast::FieldDecl::cast(node.clone());
            let annotated = field.as_ref().and_then(ast::FieldDecl::ty).is_some();
            let ty = cx.field_ty(m);
            cx.record_type(d.file, member.name_range, ty.clone());
            if let Some(getter) = field.as_ref().and_then(ast::FieldDecl::getter) {
                let mut checker = Checker::new(cx, owner);
                checker.check_value_block(&getter, &ty);
                checker.finish();
            } else if let Some(init) = field.as_ref().and_then(ast::FieldDecl::initializer) {
                let mut checker = Checker::new(cx, owner);
                if member.has_annotation("Load") {
                    checker.allow_throws();
                }
                if member.has_annotation("Derived") {
                    checker.derived();
                }
                checker.server_rendered();
                if member.has_annotation("Form") {
                    checker.form_initializer();
                    // `@Form` of a type that isn't `Validatable` is K2009 at
                    // the type; its initializer reports nothing more.
                    let validatable = cx.p.lookup_qualified("k.validation.Validatable");
                    if validatable.is_some_and(|v| !cx.implements(&ty, v)) {
                        checker = checker.silent();
                    }
                }
                if member.is_var {
                    checker.var_initializer();
                }
                checker.check_initializer(&init, annotated.then_some(&ty));
                if member.has_annotation("Load") {
                    let escaped = checker.take_escaped();
                    crate::framework::record_load(cx, m, escaped, init.syntax().text_range());
                }
                checker.finish();
            }
        }
        MemberKind::Func | MemberKind::Rpc => {
            let sig = cx.sig(SigKey::Member(m));
            let t = Ty::func(
                sig.params.iter().map(|p| p.ty.clone()).collect(),
                sig.is_async,
                sig.throws.clone(),
                sig.ret.clone(),
            );
            cx.record_type(d.file, member.name_range, t);
            let body = node.children().find_map(ast::Block::cast);
            let mut owner = owner;
            owner.generics.extend(member.generics.iter().copied());
            let mut checker = Checker::new(cx, owner);
            checker.in_function(m);
            checker.check_function(&node, &sig, body, false, false);
            checker.finish();
        }
        MemberKind::Init => {
            let sig = cx.sig(SigKey::Member(m));
            let body = node.children().find_map(ast::Block::cast);
            let mut checker = Checker::new(cx, owner);
            checker.check_function(&node, &sig, body, true, false);
            checker.finish();
        }
        MemberKind::Case => {
            let sig = cx.case_sig(m);
            // A payload field's default is checked against its type.
            let fields = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::CaseField);
            for (field, sp) in fields.zip(sig.params.iter()) {
                if let Some(default) = field.children().find_map(ast::Expr::cast) {
                    let mut checker = Checker::new(cx, owner.clone());
                    checker.check_initializer(&default, Some(&sp.ty));
                    checker.finish();
                }
            }
        }
        _ => {}
    }
}

/// Checks the visibility modifiers on `decl`'s members: `public` on an
/// `@Inject` field is K1303 and elsewhere redundant (K1302), both with a
/// fix; `private` on an enum case or rpc is K1301.
fn member_modifiers(cx: &Cx<'_, '_>, decl: DeclId) {
    let p = cx.p;
    let d = p.decl(decl);
    for &m in &d.members {
        let member = p.member(m);
        let Some(node) = &member.node else { continue };
        let Some(tok) = visibility_token(node) else {
            continue;
        };
        let range = tok.text_range();
        let end = next_non_space(p.file(d.file).text, range.end());
        let removal = TextRange::new(range.start(), end);
        match tok.kind() {
            SyntaxKind::PublicKw => {
                if member.has_annotation("Inject") {
                    cx.report(
                        d.file,
                        Diagnostic::new(
                            codes::PUBLIC_INJECT,
                            p.span(d.file, range),
                            "`@Inject` fields are always private",
                        )
                        .with_fix(Fix::replace(
                            "Remove `public`",
                            p.span(d.file, removal),
                            "",
                        )),
                    );
                } else {
                    cx.report(
                        d.file,
                        Diagnostic::new(
                            codes::REDUNDANT_PUBLIC_MEMBER,
                            p.span(d.file, range),
                            "members are as visible as their declaration; `public` is redundant",
                        )
                        .with_fix(Fix::replace(
                            "Remove `public`",
                            p.span(d.file, removal),
                            "",
                        )),
                    );
                }
            }
            SyntaxKind::PrivateKw if matches!(member.kind, MemberKind::Case | MemberKind::Rpc) => {
                cx.error(
                    d.file,
                    codes::NOT_VISIBLE,
                    range,
                    "enum cases and rpcs are visible wherever their declaration is",
                );
            }
            _ => {}
        }
    }
}

/// Checks `implements` entries: interfaces only (K2014), derived
/// conformances (K2009), required members (K2010, K2015).
fn conformance(cx: &Cx<'_, '_>, decl: DeclId) {
    let p = cx.p;
    let d = p.decl(decl);
    let list = cx.implements_list(decl);
    for (ty, range) in list.iter() {
        let Some(iface) = ty.decl() else { continue };
        let kind = p.decl(iface).named_kind();
        let non_interface = matches!(
            kind,
            Some(
                DeclarationKind::Struct
                    | DeclarationKind::Class
                    | DeclarationKind::Enum
                    | DeclarationKind::Entity
            )
        );
        if !cx.is_interface(iface) && non_interface {
            cx.error(
                d.file,
                codes::NOT_AN_INTERFACE,
                *range,
                format!(
                    "`{}` is not an interface: Kobra has no inheritance",
                    p.decl_name(iface)
                ),
            );
            continue;
        }
        if !cx.is_interface(iface) {
            continue;
        }
        let iname = p.decl(iface).name.clone();
        if DERIVED.contains(&iname.as_str()) && p.file(p.decl(iface).file).stdlib {
            // Every field (or case payload) with its name, for the message.
            let field_tys: Vec<(String, Ty)> = if cx.is_enum(decl) {
                cx.cases(decl)
                    .iter()
                    .flat_map(|c| {
                        let case = p.member(*c).name.clone();
                        cx.case_sig(*c)
                            .params
                            .iter()
                            .map(|p| (format!("the payload of `{case}`"), p.ty.clone()))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            } else {
                cx.stored_fields(decl)
                    .iter()
                    .map(|f| (format!("field `{}`", p.member(*f).name), cx.field_ty(*f)))
                    .collect()
            };
            if let Some((what, t)) = field_tys.iter().find(|(_, t)| !cx.implements(t, iface)) {
                cx.error(
                    d.file,
                    codes::UNSATISFIED_CONSTRAINT,
                    *range,
                    format!(
                        "`{}` can't derive `{iname}`: {what} (`{}`) doesn't implement it",
                        d.name,
                        t.render(p)
                    ),
                );
            }
            continue;
        }
        // Required members of the interface and its parents.
        let mut required: Vec<MemberId> = Vec::new();
        let mut stack = vec![iface];
        let mut seen = HashSet::new();
        while let Some(i) = stack.pop() {
            if !seen.insert(i) {
                continue;
            }
            for &m in &p.decl(i).members {
                let member = p.member(m);
                let has_body = member.node.as_ref().is_some_and(|n| {
                    n.children().any(|c| c.kind() == SyntaxKind::Block)
                        || ast::FieldDecl::cast(n.clone())
                            .is_some_and(|f| f.initializer().is_some())
                });
                if !has_body
                    && matches!(
                        member.kind,
                        MemberKind::Func | MemberKind::Field | MemberKind::Computed
                    )
                {
                    required.push(m);
                }
            }
            for (t, _) in cx.implements_list(i).iter() {
                if let Some(parent) = t.decl() {
                    stack.push(parent);
                }
            }
        }
        if cx.is_interface(decl) {
            continue;
        }
        let mut missing = Vec::new();
        for r in required {
            let req = p.member(r);
            let found = d.member_names.get(&req.name).copied().or_else(|| {
                cx.extensions_of.borrow().get(&decl).and_then(|exts| {
                    exts.iter()
                        .find_map(|e| p.decl(*e).member_names.get(&req.name).copied())
                })
            });
            let Some(found) = found else {
                missing.push(r);
                continue;
            };
            let f = p.member(found);
            let iface_subst: Vec<(crate::program::GenId, Ty)> = p
                .decl(req.owner)
                .generics
                .iter()
                .copied()
                .zip(ty.args().iter().cloned())
                .collect();
            let same = |x: &Ty, y: &Ty| {
                let x = crate::cx::apply(x, &iface_subst);
                x == *y || x.contains_error() || y.contains_error() || x.has_params()
            };
            let matches = match req.kind {
                MemberKind::Func => {
                    if f.kind != MemberKind::Func {
                        false
                    } else {
                        let a = cx.sig(SigKey::Member(r));
                        let b = cx.sig(SigKey::Member(found));
                        a.params.len() == b.params.len()
                            && a.params
                                .iter()
                                .zip(b.params.iter())
                                .all(|(x, y)| x.label == y.label && same(&x.ty, &y.ty))
                            && same(&a.ret, &b.ret)
                    }
                }
                _ => {
                    let a = crate::cx::apply(&cx.field_ty(r), &iface_subst);
                    let b = cx.field_ty(found);
                    matches!(f.kind, MemberKind::Field | MemberKind::Computed)
                        && (a == b || a.contains_error() || b.contains_error())
                }
            };
            if !matches {
                let file = p.decl(f.owner).file;
                let req_file = p.decl(req.owner).file;
                cx.report(
                    file,
                    Diagnostic::new(
                        codes::INTERFACE_MEMBER_MISMATCH,
                        p.span(file, f.name_range),
                        format!(
                            "`{}` doesn't match the requirement of `{}`",
                            f.name,
                            p.decl_name(iface)
                        ),
                    )
                    .with_label(p.span(req_file, req.name_range), "required here"),
                );
            }
        }
        if !missing.is_empty() {
            let names: Vec<String> = missing
                .iter()
                .map(|m| format!("`{}`", p.member(*m).name))
                .collect();
            let mut diag = Diagnostic::new(
                codes::MISSING_INTERFACE_MEMBER,
                p.span(d.file, *range),
                format!("`{}` is missing {}", d.name, names.join(", ")),
            );
            if let Some(fix) = interface_stubs_fix(cx, decl, &missing) {
                diag = diag.with_fix(fix);
            }
            cx.report(d.file, diag);
        }
    }
    let _ = has_token;
    let _ = token;
}
