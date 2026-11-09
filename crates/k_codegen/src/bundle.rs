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
fn print_server(mut stmts: Vec<Stmt>, profile: Profile) -> (String, Vec<js::Mark>) {
    let printed = print_mapped(&stmts, profile);
    if !printed.0.contains("$s.") {
        return printed;
    }
    stmts.insert(1, Stmt::ImportAll("$s".into(), "k:runtime/server".into()));
    print_mapped(&stmts, profile)
}

/// [`print`], with the printer's source-map marks.
pub(crate) fn print_mapped(stmts: &[Stmt], profile: Profile) -> (String, Vec<js::Mark>) {
    js::print_module_mapped(
        stmts,
        PrintOptions {
            compact: profile == Profile::Release,
        },
    )
}

/// The base name of a path: `pages/home.3f2a1b9c.js` → `home.3f2a1b9c.js`.
fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Appends the inline source map of each server module that holds K code
/// (`marks`, by specifier): `//# sourceMappingURL=data:…`, which the
/// isolate's stack traces and the inspector read (codegen.md §18). The
/// K text is embedded in development builds.
fn add_server_maps(g: &Gen<'_>, out: &mut [OutputFile], marks: &BTreeMap<String, Vec<js::Mark>>) {
    if g.options.profile == Profile::Test {
        return;
    }
    let sources = crate::sourcemap::Sources::new(
        g.analysis,
        crate::sourcemap::SERVER_ROOT,
        g.options.profile == Profile::Development,
    );
    for f in out.iter_mut() {
        let Some(m) = marks.get(&f.path).filter(|m| !m.is_empty()) else {
            continue;
        };
        let map = crate::sourcemap::from_marks(&f.contents, m, &sources);
        let json = crate::sourcemap::to_json(&map, Some(&f.path));
        if !f.contents.ends_with('\n') {
            f.contents.push('\n');
        }
        f.contents
            .push_str(&crate::sourcemap::js_comment(&crate::sourcemap::data_url(
                &json,
            )));
    }
}

/// Prints a client chunk and adds it to `out` (with its source map, which
/// maps it back to the `.k` files outside the test profile) under the name
/// `stem.<hash>.js`, which it returns.
///
/// A development chunk is named after its text and its map, and ends with
/// `//# sourceMappingURL=`; a release chunk (`linked`) is named after its
/// text and its K map is left next to it (`<name>.map`, no K text) for
/// [`crate::link`] to compose with the minifier's.
fn client_chunk(
    g: &Gen<'_>,
    stem: &str,
    text: String,
    marks: &[js::Mark],
    out: &mut Vec<OutputFile>,
) -> String {
    let profile = g.options.profile;
    if profile == Profile::Test {
        let name = hashed(stem, "js", &text, profile);
        out.push(OutputFile {
            path: name.clone(),
            contents: text,
            kind: FileKind::JavaScript,
        });
        return name;
    }
    let release = linked(g);
    let sources =
        crate::sourcemap::Sources::new(g.analysis, crate::sourcemap::CLIENT_ROOT, !release);
    let map = crate::sourcemap::from_marks(&text, marks, &sources);
    let json = crate::sourcemap::to_json(&map, None);
    let (name, contents) = if release {
        (hashed(stem, "js", &text, profile), text)
    } else {
        let name = crate::hash::hashed_file_name(stem, "js", format!("{text}{json}").as_bytes());
        let mut contents = text;
        contents.push_str(&crate::sourcemap::js_comment(&format!(
            "{}.map",
            base(&name)
        )));
        (name, contents)
    };
    out.push(OutputFile {
        path: format!("{name}.map"),
        contents: json,
        kind: FileKind::SourceMap,
    });
    out.push(OutputFile {
        path: name.clone(),
        contents,
        kind: FileKind::JavaScript,
    });
    name
}

/// Adds the development map of a chunk that holds no K code (the runtime,
/// a feature chunk, the `app` entry): the chunk itself as one source below
/// `/_k/runtime/`, on the `ignoreList`, so DevTools steps over it and
/// leaves it out of stack traces. Release builds get theirs from
/// [`crate::link`]; test builds none.
fn glue_map(profile: Profile, release: bool, file: &mut OutputFile, out: &mut Vec<OutputFile>) {
    if profile == Profile::Test || release {
        return;
    }
    let stem = base(&file.path).split('.').next().unwrap_or("glue");
    let url = format!("{}{stem}.js", crate::sourcemap::RUNTIME_ROOT);
    let map = crate::sourcemap::identity(&file.contents, &url, true);
    let json = crate::sourcemap::to_json(&map, None);
    file.contents
        .push_str(&crate::sourcemap::js_comment(&format!(
            "{}.map",
            base(&file.path)
        )));
    out.push(OutputFile {
        path: format!("{}.map", file.path),
        contents: json,
        kind: FileKind::SourceMap,
    });
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
    let mut marks: BTreeMap<String, Vec<js::Mark>> = BTreeMap::new();
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
        let (contents, m) = print_server(stmts, g.options.profile);
        marks.insert(server_specifier(&path), m);
        out.push(OutputFile {
            path: server_specifier(&path),
            contents,
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
        let (contents, m) = print_server(stmts, g.options.profile);
        // The entry gets its asset table spliced in later (`ASSETS_PLACEHOLDER`),
        // which would move its columns: it is glue, and keeps no map.
        let _ = m;
        out.push(OutputFile {
            path: spec,
            contents,
            kind: FileKind::JavaScript,
        });
    }
    provide_natives(&mut out, &wanted, g.options.profile);
    add_server_maps(g, &mut out, &marks);
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
/// `app` entry and the runtime chunk, with hashed names. `from_server`
/// (a test bundle's) are the server modules' names the shared chunk
/// imports, by specifier.
pub(crate) fn client_files(
    g: &Gen<'_>,
    items: &BTreeMap<DeclId, Item>,
    pages: &[DeclId],
    entry: Option<ClientEntry>,
    from_server: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<OutputFile> {
    let profile = g.options.profile;
    // A release app's client is linked and minified after printing
    // (`crate::link`): its chunks are printed readable, for the source
    // maps, and reach the runtime through named imports, which is what
    // lets the runtime chunk keep only what the app uses.
    let release = linked(g);
    let printed = if release {
        Profile::Development
    } else {
        profile
    };
    let print = |stmts: &[Stmt], _: Profile| print(stmts, printed);
    let print_mapped = |stmts: &[Stmt]| print_mapped(stmts, printed);
    let mut out = Vec::new();
    let runtime = crate::runtime_chunk(profile, &g.options.entry);
    let runtime_name = hashed("runtime", "js", &runtime, profile);
    let mut runtime_file = OutputFile {
        path: runtime_name.clone(),
        contents: runtime,
        kind: FileKind::JavaScript,
    };
    glue_map(profile, release, &mut runtime_file, &mut out);
    out.push(runtime_file);
    // Feature chunks ship only when the client uses one of their symbols:
    // its items, or the entry's containers (a platform service's browser
    // body, `BrowserFiles`).
    let entry_uses: Vec<(DeclId, String)> = entry
        .as_ref()
        .map(|(_, uses)| uses.iter().cloned().collect())
        .unwrap_or_default();
    let mut features: Vec<(usize, String)> = Vec::new();
    for (i, (stem, specifier, _)) in FEATURE_CHUNKS.iter().enumerate() {
        let used = items
            .values()
            .flat_map(|it| it.uses.iter())
            .chain(entry_uses.iter())
            .any(|(d, sym)| {
                g.view.decl(*d).stdlib
                    && !items.contains_key(d)
                    && feature_of(&module_path(g, *d), sym) == Some(i)
            });
        if used {
            let text = crate::feature_chunk(specifier, profile, &runtime_name, &g.options.entry);
            let name = hashed(stem, "js", &text, profile);
            let mut file = OutputFile {
                path: name.clone(),
                contents: text,
                kind: FileKind::JavaScript,
            };
            glue_map(profile, release, &mut file, &mut out);
            out.push(file);
            features.push((i, name));
        }
    }
    // Runtime features generated code reaches through `$k` but that ship
    // as chunks of their own (`forms`, `money`): their names are known up
    // front, and each ships when a client file uses one of its exports.
    let runtime_features: Vec<(String, String)> = RUNTIME_FEATURES
        .iter()
        .map(|(stem, specifier)| {
            let text = crate::feature_chunk(specifier, profile, &runtime_name, &g.options.entry);
            let name = hashed(stem, "js", &text, profile);
            (text, name)
        })
        .collect();
    let feature_names: Vec<&str> = runtime_features.iter().map(|(_, n)| n.as_str()).collect();
    let mut used_features = BTreeSet::new();
    let mut shared_decls: Vec<DeclId> = items
        .keys()
        .copied()
        .filter(|d| !pages.contains(d))
        .collect();
    shared_decls.sort_by_key(|d| order_key(g, *d));
    let mut shared_name = None;
    if !shared_decls.is_empty() {
        let mut stmts = vec![Stmt::ImportAll("$k".into(), format!("./{runtime_name}"))];
        // A test bundle's server-only names (`from_server`, empty otherwise).
        for (spec, syms) in from_server {
            stmts.push(Stmt::Import(
                syms.iter().map(|s| (s.clone(), s.clone())).collect(),
                spec.clone(),
            ));
        }
        stmts.extend(client_natives(
            g,
            items,
            shared_decls.iter().flat_map(|d| items[d].uses.iter()),
            &features,
            "./",
        ));
        for d in &shared_decls {
            stmts.extend(items[d].prelude.iter().cloned());
        }
        for d in &shared_decls {
            stmts.extend(items[d].stmts.iter().cloned());
        }
        route_runtime_features(&mut stmts, &feature_names, "./", &mut used_features);
        if release {
            named_runtime_imports(&mut stmts);
        }
        let (text, marks) = print_mapped(&stmts);
        let name = client_chunk(g, "shared", text, &marks, &mut out);
        shared_name = Some(name);
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
        stmts.extend(client_natives(g, items, item.uses.iter(), &features, "../"));
        stmts.extend(item.prelude.iter().cloned());
        stmts.extend(item.stmts.iter().cloned());
        route_runtime_features(&mut stmts, &feature_names, "../", &mut used_features);
        if release {
            named_runtime_imports(&mut stmts);
        }
        let (text, marks) = print_mapped(&stmts);
        let stem = format!("pages/{}", crate::css::kebab(&g.view.decl(p).name));
        let name = client_chunk(g, &stem, text, &marks, &mut out);
        page_names.insert(p, name);
    }
    if let Some((entry, uses)) = entry {
        let mut stmts = vec![Stmt::ImportAll("$k".into(), format!("./{runtime_name}"))];
        // Platform services whose browser body is a feature chunk's.
        stmts.extend(feature_imports(g, items, &uses, &features));
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
        route_runtime_features(&mut stmts, &feature_names, "./", &mut used_features);
        if release {
            named_runtime_imports(&mut stmts);
        }
        let mut text = print(&stmts, profile);
        for (p, name) in &page_names {
            text = text.replace(&format!("$page:{}", p.0), &format!("./{name}"));
        }
        if let Some(shared) = &shared_name {
            text = text.replace("$shared", &format!("./{shared}"));
        }
        let name = hashed("app", "js", &text, profile);
        let mut file = OutputFile {
            path: name,
            contents: text,
            kind: FileKind::JavaScript,
        };
        glue_map(profile, release, &mut file, &mut out);
        out.push(file);
    }
    for (i, (text, name)) in runtime_features.into_iter().enumerate() {
        if used_features.contains(&i) {
            let mut file = OutputFile {
                path: name,
                contents: text,
                kind: FileKind::JavaScript,
            };
            glue_map(profile, release, &mut file, &mut out);
            out.push(file);
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// `true` when the client of this build is linked and minified after
/// printing (`crate::link`): an app's release build. Tests and the
/// development profile keep readable output.
pub(crate) fn linked(g: &Gen<'_>) -> bool {
    g.options.profile == Profile::Release && g.options.entry == crate::Entry::App
}

/// The namespaces generated client code reaches the runtime through: the
/// runtime chunk (`$k`) and the runtime features with chunks of their own
/// ([`route_runtime_features`]).
fn runtime_namespace(name: &str) -> bool {
    name == "$k"
        || RUNTIME_FEATURES
            .iter()
            .any(|(stem, _)| name == format!("$k_{stem}"))
}

/// Replaces each namespace import of the runtime (`import * as $k`, and
/// `$k_forms`, `$k_money`) with named imports of exactly the exports the
/// chunk uses: `$k.template(…)` becomes `$k$template(…)` with
/// `import { template as $k$template }`. Names starting `$k$` are never
/// generated otherwise. A chunk that still uses a namespace as a value
/// keeps its namespace import too, which keeps every export of that file
/// alive (`crate::link` shakes only files imported by name).
fn named_runtime_imports(stmts: &mut Vec<Stmt>) {
    let mut used: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    js::walk_exprs_mut(stmts, &mut |e| {
        if let js::Expr::Member(base, name) = e
            && let js::Expr::Ident(ns) = &**base
            && runtime_namespace(ns)
        {
            let alias = format!("{ns}${name}");
            used.entry(ns.clone()).or_default().insert(name.clone());
            *e = js::id(alias);
        }
    });
    let mut bare: BTreeSet<String> = BTreeSet::new();
    js::walk_exprs_mut(stmts, &mut |e| {
        if let js::Expr::Ident(ns) = e
            && runtime_namespace(ns)
        {
            bare.insert(ns.clone());
        }
    });
    let mut i = 0;
    while i < stmts.len() {
        if let Stmt::ImportAll(ns, spec) = &stmts[i]
            && runtime_namespace(ns)
        {
            let (ns, spec) = (ns.clone(), spec.clone());
            let names: Vec<(String, String)> = used
                .remove(&ns)
                .unwrap_or_default()
                .into_iter()
                .map(|n| (n.clone(), format!("{ns}${n}")))
                .collect();
            let keep_namespace = bare.contains(&ns);
            if !keep_namespace {
                stmts.remove(i);
            } else {
                i += 1;
            }
            if !names.is_empty() {
                stmts.insert(i, Stmt::Import(names, spec));
                i += 1;
            }
            continue;
        }
        i += 1;
    }
}

/// Runtime files that generated code calls through `$k` but that ship as
/// client chunks of their own, only to clients with a file that uses one
/// of their exports: `(chunk stem, runtime specifier)` (runtime.md §2).
/// A client file reaches the chunk as `$k_<stem>`
/// ([`route_runtime_features`]); the server keeps reaching them through
/// `k:runtime`, which re-exports them.
const RUNTIME_FEATURES: &[(&str, &str)] = &[
    ("forms", "k:/runtime/forms.js"),
    ("money", "k:/runtime/money.js"),
];

/// The index in [`RUNTIME_FEATURES`] of the file exporting each name,
/// built once from the embedded sources.
fn runtime_feature_exports() -> &'static BTreeMap<String, usize> {
    static EXPORTS: std::sync::OnceLock<BTreeMap<String, usize>> = std::sync::OnceLock::new();
    EXPORTS.get_or_init(|| {
        let mut names = BTreeMap::new();
        for (i, (_, specifier)) in RUNTIME_FEATURES.iter().enumerate() {
            let source = k_runtime::module(specifier).map_or("", |m| m.source);
            for line in source.lines() {
                let Some(rest) = line.strip_prefix("export ") else {
                    continue;
                };
                let rest = rest.strip_prefix("async ").unwrap_or(rest);
                let name: String = ["function ", "class ", "const ", "let "]
                    .iter()
                    .find_map(|k| rest.strip_prefix(k))
                    .unwrap_or("")
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    names.insert(name, i);
                }
            }
        }
        names
    })
}

/// Points `stmts`' uses of a runtime feature's exports at the feature's
/// chunk: `$k.createForm` becomes `$k_forms.createForm`, and the file
/// imports `* as $k_forms` from `prefix` followed by the chunk's name
/// (`names`, by index of [`RUNTIME_FEATURES`]) right after its import of
/// the runtime chunk. Adds the index of every feature used to `used`.
fn route_runtime_features(
    stmts: &mut Vec<Stmt>,
    names: &[&str],
    prefix: &str,
    used: &mut BTreeSet<usize>,
) {
    let exports = runtime_feature_exports();
    let mut here = BTreeSet::new();
    js::walk_exprs_mut(stmts, &mut |e| {
        if let js::Expr::Member(base, name) = e
            && matches!(&**base, js::Expr::Ident(k) if k == "$k")
            && let Some(&i) = exports.get(name.as_str())
        {
            **base = js::id(format!("$k_{}", RUNTIME_FEATURES[i].0));
            here.insert(i);
        }
    });
    let at = stmts
        .iter()
        .position(|s| !matches!(s, Stmt::ImportAll(..) | Stmt::Import(..)))
        .unwrap_or(stmts.len());
    for &i in here.iter().rev() {
        let Some(name) = names.get(i) else { continue };
        stmts.insert(
            at,
            Stmt::ImportAll(
                format!("$k_{}", RUNTIME_FEATURES[i].0),
                format!("{prefix}{name}"),
            ),
        );
    }
    used.extend(here);
}

/// Runtime files shipped as client chunks of their own, only to clients
/// that use them: `(chunk stem, runtime specifier, the stdlib module whose
/// symbols they implement)` (runtime.md §2).
const FEATURE_CHUNKS: &[(&str, &str, &str)] = &[
    ("charts", "k:/runtime/charts.js", "k/charts"),
    ("drag", "k:/runtime/drag.js", "k/drag"),
    ("files", "k:/runtime/files.js", "k/device"),
    ("clipboard", "k:/runtime/clipboard.js", "k/device"),
    ("share", "k:/runtime/share.js", "k/device"),
    ("keys", "k:/runtime/keys.js", "k/ui"),
    ("streams", "k:/runtime/streams.js", "k/core"),
    ("http", "k:/runtime/http.js", "k/http"),
];

/// The feature chunk (an index of [`FEATURE_CHUNKS`]) that implements
/// symbol `sym` of stdlib module `module` (`k/charts`), if any.
fn feature_of(module: &str, sym: &str) -> Option<usize> {
    FEATURE_CHUNKS.iter().position(|(_, specifier, m)| {
        *m == module && k_runtime::module(specifier).is_some_and(|f| exports(f.source, sym))
    })
}

/// The stdlib symbols `uses` names that no item of the client bundle
/// defines: `@Native` members without a generated body. One a feature
/// chunk implements is imported from it (`features`: the chunks this
/// bundle ships, by index of [`FEATURE_CHUNKS`]; `prefix` the way from the
/// chunk being written to them, `./` or `../`); every other becomes
/// `const Sym = $k.native("module.Sym")`, the browser's implementation
/// (`services.js`) or a stand-in that panics with the name when used, so
/// a chunk always loads and only the unimplemented path fails. A linked
/// release build binds `$k.native_<name>` or `$k.missingNative("…")`
/// instead, so the lookup table doesn't keep every native alive.
fn client_natives<'a>(
    g: &Gen<'_>,
    items: &BTreeMap<DeclId, Item>,
    uses: impl Iterator<Item = &'a (DeclId, String)>,
    features: &[(usize, String)],
    prefix: &str,
) -> Vec<Stmt> {
    let missing: BTreeSet<(String, String)> = uses
        .filter(|(d, _)| !items.contains_key(d) && g.view.decl(*d).stdlib)
        .map(|(d, sym)| (sym.clone(), module_path(g, *d)))
        .collect();
    let mut from: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut out = Vec::new();
    for (sym, module) in missing {
        let chunk = feature_of(&module, &sym)
            .and_then(|i| features.iter().find(|(f, _)| *f == i))
            .map(|(_, name)| name.clone());
        if let Some(name) = chunk {
            from.entry(name).or_default().push((sym.clone(), sym));
            continue;
        }
        let name = format!("{}.{sym}", module.replace('/', "."));
        // A release build binds the implementation itself (or the
        // stand-in), so the runtime's table of natives, and every native
        // the app doesn't use, can go (codegen.md §17.1).
        let value = if linked(g) {
            let export = format!("native_{}", name.replace('.', "_"));
            let implemented = k_runtime::module("k:/runtime/services.js")
                .is_some_and(|m| m.source.contains(&format!("export const {export} =")));
            if implemented {
                js::member(js::id("$k"), export)
            } else {
                js::rt("missingNative", vec![js::str(name)])
            }
        } else {
            js::rt("native", vec![js::str(name)])
        };
        out.push(Stmt::Const(sym, value));
    }
    let imports = from
        .into_iter()
        .map(|(name, syms)| Stmt::Import(syms, format!("{prefix}{name}")));
    imports.chain(out).collect()
}

/// The imports of the stdlib symbols in `uses` that a shipped feature
/// chunk implements (`features`, as in [`client_natives`]), from the
/// client entry, which sits next to the chunks. Unlike
/// [`client_natives`] it binds nothing else: the entry's other stdlib
/// uses are the shared chunk's.
fn feature_imports(
    g: &Gen<'_>,
    items: &BTreeMap<DeclId, Item>,
    uses: &BTreeSet<(DeclId, String)>,
    features: &[(usize, String)],
) -> Vec<Stmt> {
    let mut from: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (d, sym) in uses {
        if items.contains_key(d) || !g.view.decl(*d).stdlib {
            continue;
        }
        let chunk = feature_of(&module_path(g, *d), sym)
            .and_then(|i| features.iter().find(|(f, _)| *f == i));
        if let Some((_, name)) = chunk {
            from.entry(name.clone())
                .or_default()
                .push((sym.clone(), sym.clone()));
        }
    }
    from.into_iter()
        .map(|(name, syms)| Stmt::Import(syms, format!("./{name}")))
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
    let tests = g.options.entry == crate::Entry::Tests;
    if (info.is_test && !tests)
        || (info.in_test_module && g.options.environment != crate::Environment::Test)
    {
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
