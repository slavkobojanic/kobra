//! Bundling (`docs/compiler/codegen.md` §4, §17): lowered items become
//! files. The server gets one module per K module (`k:/app/shop/cart.js`)
//! with named imports between them; the client gets the runtime chunk, a
//! `shared` chunk with every non-page declaration (names are unique across
//! the program, so no renaming is needed), one chunk per page and the
//! `app` entry chunk. Client chunk names are content-hashed, and a chunk's
//! hash covers the names of the chunks it imports.

use std::collections::{BTreeMap, BTreeSet};

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, DeclKind};

use crate::cx::{Gen, Target};
use crate::decl::Item;
use crate::js::{self, PrintOptions, Stmt};
use crate::{FileKind, OutputFile, Profile};

/// The JS module path of a declaration's module: `shop/cart`, or `$app`
/// for declarations outside `src/` (`App.k`).
pub(crate) fn module_path(g: &Gen<'_>, d: DeclId) -> String {
    match &g.view.decl(d).module {
        Some(m) => m.replace('.', "/"),
        None => "$app".to_owned(),
    }
}

/// The server specifier of a module path.
pub(crate) fn server_specifier(path: &str) -> String {
    format!("k:/app/{path}.js")
}

/// Sort key that puts declarations other code reads at module evaluation
/// first (enums, structs, classes), then constants, then the rest.
fn order_key(g: &Gen<'_>, d: DeclId) -> (u8, bool, String, u32) {
    let info = g.view.decl(d);
    let rank = match info.kind {
        DeclKind::Extension => 3,
        DeclKind::Named(k) => match k {
            DeclarationKind::Enum => 0,
            DeclarationKind::Struct | DeclarationKind::Entity | DeclarationKind::Interface => 1,
            DeclarationKind::Class => 1,
            DeclarationKind::Constant => 2,
            _ => 3,
        },
    };
    (
        rank,
        !info.stdlib,
        info.module.clone().unwrap_or_default(),
        u32::from(info.range.start()),
    )
}

/// Selects the on-demand pieces that are referenced, transitively, from
/// the items' own code and `roots`, and folds them into the items: their
/// statements are appended and their uses merged.
pub(crate) fn settle_on_demand(
    items: &mut BTreeMap<DeclId, Item>,
    roots: &BTreeSet<(DeclId, String)>,
) {
    let mut used: BTreeSet<(DeclId, String)> = roots.clone();
    for item in items.values() {
        used.extend(item.uses.iter().cloned());
    }
    let mut taken: BTreeSet<(DeclId, usize)> = BTreeSet::new();
    loop {
        let mut grew = false;
        for (d, item) in items.iter() {
            for (i, piece) in item.on_demand.iter().enumerate() {
                if taken.contains(&(*d, i)) {
                    continue;
                }
                if piece
                    .symbols
                    .iter()
                    .any(|s| used.contains(&(*d, s.clone())))
                {
                    taken.insert((*d, i));
                    used.extend(piece.uses.iter().cloned());
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    for (d, item) in items.iter_mut() {
        let pieces = std::mem::take(&mut item.on_demand);
        for (i, piece) in pieces.into_iter().enumerate() {
            if taken.contains(&(*d, i)) {
                item.stmts.extend(piece.stmts);
                item.uses.extend(piece.uses);
            }
        }
    }
}

/// Drops the items not reachable from `roots` through the items' uses and
/// the `extra` edges (`@Inject` dependencies).
pub(crate) fn keep_reachable(
    items: &mut BTreeMap<DeclId, Item>,
    roots: &BTreeSet<DeclId>,
    extra: &BTreeMap<DeclId, Vec<DeclId>>,
) {
    let mut reached: BTreeSet<DeclId> = BTreeSet::new();
    let mut work: Vec<DeclId> = roots.iter().copied().collect();
    while let Some(d) = work.pop() {
        if !reached.insert(d) {
            continue;
        }
        if let Some(item) = items.get(&d) {
            work.extend(
                item.uses
                    .iter()
                    .map(|(u, _)| *u)
                    .filter(|u| !reached.contains(u)),
            );
        }
        if let Some(more) = extra.get(&d) {
            work.extend(more.iter().copied().filter(|u| !reached.contains(u)));
        }
    }
    items.retain(|d, _| reached.contains(d));
}

/// Prints a server module, importing the server runtime as `$s` only
/// when the module uses it.
fn print_server(mut stmts: Vec<Stmt>, profile: Profile) -> String {
    let text = print(&stmts, profile);
    if !text.contains("$s.") {
        return text;
    }
    stmts.insert(1, Stmt::ImportAll("$s".into(), "k:runtime/server".into()));
    print(&stmts, profile)
}

/// Prints statements with the profile's options.
pub(crate) fn print(stmts: &[Stmt], profile: Profile) -> String {
    js::print_module(
        stmts,
        PrintOptions {
            compact: profile == Profile::Release,
        },
    )
}

/// An entry module: its specifier, statements and the symbols it uses.
pub(crate) type EntryModule = (String, Vec<Stmt>, BTreeSet<(DeclId, String)>);

/// Builds the server modules from items, plus extra statements per module
/// (entry code). Returns the files sorted by specifier.
pub(crate) fn server_files(
    g: &Gen<'_>,
    items: &BTreeMap<DeclId, Item>,
    extra: Vec<EntryModule>,
) -> Vec<OutputFile> {
    let mut modules: BTreeMap<String, Vec<DeclId>> = BTreeMap::new();
    for d in items.keys() {
        modules.entry(module_path(g, *d)).or_default().push(*d);
    }
    let mut out = Vec::new();
    for (path, mut decls) in modules {
        decls.sort_by_key(|d| order_key(g, *d));
        let mut uses = BTreeSet::new();
        let mut body = Vec::new();
        let mut prelude = Vec::new();
        for d in &decls {
            let item = &items[d];
            uses.extend(item.uses.iter().cloned());
            prelude.extend(item.prelude.iter().cloned());
            body.extend(item.stmts.iter().cloned());
        }
        let mut stmts = vec![Stmt::ImportAll("$k".into(), "k:runtime".into())];
        stmts.extend(imports(g, &path, &uses, false));
        stmts.extend(prelude);
        stmts.extend(body);
        out.push(OutputFile {
            path: server_specifier(&path),
            contents: print_server(stmts, g.options.profile),
            kind: FileKind::JavaScript,
        });
    }
    let mut wanted: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for item in items.values() {
        for (d, sym) in &item.uses {
            wanted
                .entry(module_path(g, *d))
                .or_default()
                .insert(sym.clone());
        }
    }
    for (spec, body, uses) in extra {
        for (d, sym) in &uses {
            wanted
                .entry(module_path(g, *d))
                .or_default()
                .insert(sym.clone());
        }
        let mut stmts = vec![Stmt::ImportAll("$k".into(), "k:runtime".into())];
        stmts.extend(imports(g, "$entry", &uses, false));
        stmts.extend(body);
        out.push(OutputFile {
            path: spec,
            contents: print_server(stmts, g.options.profile),
            kind: FileKind::JavaScript,
        });
    }
    provide_natives(&mut out, &wanted, g.options.profile);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// `true` if module text `text` exports `sym`.
fn exports(text: &str, sym: &str) -> bool {
    [
        format!("export function {sym}("),
        format!("export async function {sym}("),
        format!("export const {sym} ="),
        format!("export const {sym}="),
        format!("export class {sym} "),
        format!("export class {sym}{{"),
    ]
    .iter()
    .any(|p| text.contains(p.as_str()))
}

/// Defines every imported symbol a stdlib module of the server bundle
/// lacks: a `@Native` member without a generated body. Each is the
/// server runtime's implementation when it has one (`$s.natives`), or a
/// stand-in that panics when used, naming the member, so a bundle always
/// loads and only the unimplemented path fails (with a 500).
fn provide_natives(
    out: &mut Vec<OutputFile>,
    wanted: &BTreeMap<String, BTreeSet<String>>,
    profile: Profile,
) {
    for (path, syms) in wanted {
        if !path.starts_with("k/") {
            continue;
        }
        let spec = server_specifier(path);
        let existing = out.iter().position(|f| f.path == spec);
        let text = existing.map_or("", |i| out[i].contents.as_str());
        let missing: Vec<&String> = syms.iter().filter(|s| !exports(text, s)).collect();
        if missing.is_empty() {
            continue;
        }
        let module = path.trim_end_matches(".js").replace('/', ".");
        let stmts: Vec<Stmt> = missing
            .into_iter()
            .map(|sym| {
                let name = format!("{module}.{sym}");
                Stmt::ExportConst(
                    sym.clone(),
                    js::bin(
                        "??",
                        js::Expr::Index(
                            Box::new(js::member(js::id("$s"), "natives")),
                            Box::new(js::str(name.clone())),
                        ),
                        js::call(
                            js::member(js::id("$s"), "missingNative"),
                            vec![js::str(name)],
                        ),
                    ),
                )
            })
            .collect();
        let mut extra = print(&stmts, profile);
        match existing {
            Some(i) => {
                let f = &mut out[i];
                if !f.contents.contains("k:runtime/server") {
                    f.contents =
                        format!("import * as $s from \"k:runtime/server\";\n{}", f.contents);
                }
                if !f.contents.ends_with('\n') {
                    f.contents.push('\n');
                }
                f.contents.push_str(&extra);
            }
            None => {
                extra = format!("import * as $s from \"k:runtime/server\";\n{extra}");
                out.push(OutputFile {
                    path: spec,
                    contents: extra,
                    kind: FileKind::JavaScript,
                });
            }
        }
    }
}

/// Named imports of the symbols `uses` names that are defined outside
/// module `path`.
fn imports(g: &Gen<'_>, path: &str, uses: &BTreeSet<(DeclId, String)>, _client: bool) -> Vec<Stmt> {
    let mut by_module: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (d, sym) in uses {
        let m = module_path(g, *d);
        if m == path {
            continue;
        }
        by_module.entry(m).or_default().insert(sym.clone());
    }
    by_module
        .into_iter()
        .map(|(m, syms)| {
            Stmt::Import(
                syms.into_iter().map(|s| (s.clone(), s)).collect(),
                server_specifier(&m),
            )
        })
        .collect()
}

/// The client entry's statements and the symbols it uses.
pub(crate) type ClientEntry = (Vec<Stmt>, BTreeSet<(DeclId, String)>);

/// The client files: `shared` (every non-page item), page chunks, the
/// `app` entry and the runtime chunk, with hashed names.
pub(crate) fn client_files(
    g: &Gen<'_>,
    items: &BTreeMap<DeclId, Item>,
    pages: &[DeclId],
    entry: Option<ClientEntry>,
) -> Vec<OutputFile> {
    let profile = g.options.profile;
    let mut out = Vec::new();
    let runtime = crate::runtime_chunk(profile);
    let runtime_name = hashed("runtime", "js", &runtime, profile);
    out.push(OutputFile {
        path: runtime_name.clone(),
        contents: runtime,
        kind: FileKind::JavaScript,
    });
    let mut shared_decls: Vec<DeclId> = items
        .keys()
        .copied()
        .filter(|d| !pages.contains(d))
        .collect();
    shared_decls.sort_by_key(|d| order_key(g, *d));
    let mut shared_name = None;
    if !shared_decls.is_empty() {
        let mut stmts = vec![Stmt::ImportAll("$k".into(), format!("./{runtime_name}"))];
        stmts.extend(client_natives(
            g,
            items,
            shared_decls.iter().flat_map(|d| items[d].uses.iter()),
        ));
        for d in &shared_decls {
            stmts.extend(items[d].prelude.iter().cloned());
        }
        for d in &shared_decls {
            stmts.extend(items[d].stmts.iter().cloned());
        }
        let text = print(&stmts, profile);
        let name = hashed("shared", "js", &text, profile);
        shared_name = Some(name.clone());
        out.push(OutputFile {
            path: name,
            contents: text,
            kind: FileKind::JavaScript,
        });
    }
    let mut page_names = BTreeMap::new();
    for &p in pages {
        let Some(item) = items.get(&p) else { continue };
        let mut stmts = vec![Stmt::ImportAll("$k".into(), format!("../{runtime_name}"))];
        let syms: BTreeSet<String> = item
            .uses
            .iter()
            .filter(|(d, _)| !pages.contains(d) || *d != p)
            .filter(|(d, _)| items.contains_key(d) && !pages.contains(d))
            .map(|(_, s)| s.clone())
            .collect();
        if let (Some(shared), false) = (&shared_name, syms.is_empty()) {
            stmts.push(Stmt::Import(
                syms.into_iter().map(|s| (s.clone(), s)).collect(),
                format!("../{shared}"),
            ));
        }
        stmts.extend(client_natives(g, items, item.uses.iter()));
        stmts.extend(item.prelude.iter().cloned());
        stmts.extend(item.stmts.iter().cloned());
        let text = print(&stmts, profile);
        let stem = format!("pages/{}", crate::css::kebab(&g.view.decl(p).name));
        let name = hashed(&stem, "js", &text, profile);
        page_names.insert(p, name.clone());
        out.push(OutputFile {
            path: name,
            contents: text,
            kind: FileKind::JavaScript,
        });
    }
    if let Some((entry, uses)) = entry {
        let mut stmts = vec![Stmt::ImportAll("$k".into(), format!("./{runtime_name}"))];
        // The containers construct services and RPC clients of the shared
        // chunk (`$shared` is replaced by its name below).
        let syms: BTreeSet<String> = uses
            .iter()
            .filter(|(d, _)| items.contains_key(d) && !pages.contains(d))
            .map(|(_, s)| s.clone())
            .collect();
        if shared_name.is_some() && !syms.is_empty() {
            stmts.push(Stmt::Import(
                syms.into_iter().map(|s| (s.clone(), s)).collect(),
                "$shared".into(),
            ));
        }
        stmts.extend(entry);
        let mut text = print(&stmts, profile);
        for (p, name) in &page_names {
            text = text.replace(&format!("$page:{}", p.0), &format!("./{name}"));
        }
        if let Some(shared) = &shared_name {
            text = text.replace("$shared", &format!("./{shared}"));
        }
        let name = hashed("app", "js", &text, profile);
        out.push(OutputFile {
            path: name,
            contents: text,
            kind: FileKind::JavaScript,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The stdlib symbols `uses` names that no item of the client bundle
/// defines: `@Native` members without a generated body. Each becomes
/// `const Sym = $k.native("module.Sym")`, the browser's implementation
/// (`services.js`) or a stand-in that panics with the name when used, so
/// a chunk always loads and only the unimplemented path fails.
fn client_natives<'a>(
    g: &Gen<'_>,
    items: &BTreeMap<DeclId, Item>,
    uses: impl Iterator<Item = &'a (DeclId, String)>,
) -> Vec<Stmt> {
    let missing: BTreeSet<(String, String)> = uses
        .filter(|(d, _)| !items.contains_key(d) && g.view.decl(*d).stdlib)
        .map(|(d, sym)| (sym.clone(), module_path(g, *d).replace('/', ".")))
        .collect();
    missing
        .into_iter()
        .map(|(sym, module)| {
            let name = format!("{module}.{sym}");
            Stmt::Const(sym, js::rt("native", vec![js::str(name)]))
        })
        .collect()
}

/// `stem.<hash>.ext`, or `stem.[hash].ext` under the test profile.
pub(crate) fn hashed(stem: &str, ext: &str, contents: &str, profile: Profile) -> String {
    if profile == Profile::Test {
        format!("{stem}.[hash].{ext}")
    } else {
        crate::hash::hashed_file_name(stem, ext, contents.as_bytes())
    }
}

/// `true` for declarations emitted on `target` at all.
pub(crate) fn allowed(g: &Gen<'_>, d: DeclId, target: Target) -> bool {
    use k_sema::Side;
    let info = g.view.decl(d);
    // Test code, and test-only modules outside a test build (fakes such as
    // a `testing/FakeHttpClient` never ship).
    if info.is_test || (info.in_test_module && g.options.environment != crate::Environment::Test) {
        return false;
    }
    match (info.side, target) {
        (Some(Side::ServerOnly), Target::Client) => false,
        (Some(Side::ClientOnly), Target::Server) => {
            // Client-only components still get a server renderer.
            matches!(
                info.named_kind(),
                Some(
                    DeclarationKind::Component
                        | DeclarationKind::Page
                        | DeclarationKind::Layout
                        | DeclarationKind::Service
                )
            )
        }
        _ => true,
    }
}
