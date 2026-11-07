//! Code generation for K: JavaScript and CSS from the checked program.
//!
//! [`generate`] turns a checked [`k_sema::Analysis`] into an [`Output`]:
//!
//! * the **client bundle**: ES2022 modules for the browser (components as
//!   fine-grained DOM code over `k:runtime`, the generated `Page` enum and
//!   route table, RPC client stubs, hydration entry points), one chunk per
//!   page, with content-hashed names;
//! * the **server bundle**: the same components as SSR string renderers,
//!   services with request-scoped DI, RPC dispatch, endpoint pages,
//!   repositories with their SQL, and the entry module the server's worker
//!   isolates evaluate;
//! * **CSS**: scoped rules with hashed class names, theme tokens as custom
//!   properties, variants as classes;
//! * the [`Manifest`]: what the server front and `dist/app` need to know
//!   (rpc paths, routes, endpoints, config fields, assets);
//! * source maps for both bundles.
//!
//! Codegen never reports semantic errors: it refuses to run on a program
//! with errors ([`CodegenError::HasErrors`]) and never panics on one. Anything
//! reachable only from server-only declarations is absent from the client
//! bundle by construction (`docs/compiler/codegen.md`, "Client/server
//! split"), and [`check_client_bundle`] verifies the invariant on the output.
//!
//! # Status
//!
//! The API is final. The core language, components, views, styles, DI
//! and the client/server split are lowered; what is not yet lowered is
//! reported as [`CodegenError::Unsupported`]. The design is
//! `docs/compiler/codegen.md`.

mod app;
mod bundle;
mod codec;
mod css;
mod cx;
mod db;
mod decl;
pub mod dist;
pub mod hash;
mod jobs;
mod js;
mod lower;
mod migration;
mod native;
mod routes;
mod server;
mod suite;
mod ui;
mod view;

pub use migration::{MigrationDraft, MigrationError, MigrationPlan, plan_migration};
pub use routes::{Route, RouteKind, RouteParam, route_table};

use serde::{Deserialize, Serialize};

/// How generated code is printed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Profile {
    /// Readable output with source maps: `k dev`, `k run`.
    #[default]
    Development,
    /// Compact output (no whitespace, short local names) with source maps:
    /// `k build`.
    Release,
    /// Readable output with **placeholder hashes** (`[hash]` in file names,
    /// `Component__class` class names) and no source-map comments, so golden
    /// files don't change when an unrelated byte does: the `codegen` corpus.
    Test,
}

/// The `use … in` environment the DI wiring is resolved for
/// (`docs/design/23-app-structure.md`, "Environments").
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Environment {
    /// `k build`.
    #[default]
    Production,
    /// `k dev`, `k run`.
    Development,
    /// `k test` and the execution corpus.
    Test,
}

/// What the bundle starts from.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Entry {
    /// The project's `app`: a server entry that serves pages, rpcs and
    /// endpoints, and a client entry that hydrates.
    #[default]
    App,
    /// A script: the server bundle's entry module exports `main()` of the
    /// named top-level function (`corpus.main`), encoding its result as JSON.
    /// Used by the execution corpus (`tests/corpus/run`).
    Main(String),
    /// The project's tests (`k test`): the client bundle, test code
    /// included, whose entry exports `$groups`, the test groups with their
    /// containers and tests; and the server bundle, whose entry exports
    /// `$test` (the app's request handler, each group's server container
    /// and a per-test reset) instead of serving (`suite`, codegen.md §21).
    Tests,
}

/// How to generate.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Options {
    /// Printing and hashing.
    pub profile: Profile,
    /// The environment for DI.
    pub environment: Environment,
    /// The entry point.
    pub entry: Entry,
}

/// What a generated file holds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum FileKind {
    /// An ES2022 module.
    JavaScript,
    /// A stylesheet.
    Css,
    /// A source map (v3, JSON).
    SourceMap,
}

/// One generated file.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct OutputFile {
    /// For client files, the URL path below `/assets/`
    /// (`pages/product.3f2a1b9c.js`); for server files, the module
    /// specifier (`k:/app/shop/cart.js`).
    pub path: String,
    /// The contents.
    pub contents: String,
    /// The kind.
    pub kind: FileKind,
}

/// A page route, for the front and `k routes`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RouteEntry {
    /// The `Page` case name, `product`.
    pub page: String,
    /// The URL template from `routes.lock`, `/product/{id}`.
    pub template: String,
}

/// An endpoint page, for the front.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EndpointEntry {
    /// The URL template, `/webhooks/{tenant}`.
    pub template: String,
    /// The methods it handles, upper case.
    pub methods: Vec<String>,
    /// `true` if it injects `Session` (cookie and forgery checks apply).
    pub uses_session: bool,
}

/// One `@Env` field the build reads at startup.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// The case names when the type is a payload-free enum (or a list of
    /// one), in declaration order; empty otherwise.
    #[serde(default)]
    pub cases: Vec<String>,
    /// The bounds of an `@Range(lo...hi)` constraint on an `Int` field.
    #[serde(default)]
    pub range: Option<(i64, i64)>,
    /// The variable.
    pub env: String,
    /// `module.Config.field`.
    pub declared_at: String,
    /// The K type, `Duration`, `Secret<String>`.
    pub ty: String,
    /// `true` for `Secret<T>`.
    pub secret: bool,
    /// `true` when the field has no default and isn't optional.
    pub required: bool,
    /// The default, as environment text.
    pub default: Option<String>,
}

/// Everything about the build that isn't code.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// The project name from `k.toml`.
    pub name: String,
    /// Every rpc path, sorted.
    pub rpc_paths: Vec<String>,
    /// The page routes, sorted by template.
    pub routes: Vec<RouteEntry>,
    /// The endpoint pages, sorted by template.
    pub endpoints: Vec<EndpointEntry>,
    /// The configuration the production graph reads.
    pub config: Vec<ConfigEntry>,
    /// `true` when the app renders on the client and has no server-only
    /// code: `k build` then emits only `dist/public`.
    pub static_only: bool,
    /// The background jobs (`docs/design/30-background-jobs.md`): each
    /// job's name, `@Retry` policy and `@Schedule`, sorted by name. The
    /// server's jobs worker runs them.
    #[serde(default)]
    pub jobs: Vec<k_db::stdlib::JobSpec>,
}

/// The result of [`generate`].
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Output {
    /// The client files: JavaScript chunks, CSS, source maps. Never empty
    /// for an app (the runtime chunk is always there).
    pub client: Vec<OutputFile>,
    /// The server modules; empty for a static-only app.
    pub server: Vec<OutputFile>,
    /// The specifier of the server entry module.
    pub server_entry: Option<String>,
    /// The manifest.
    pub manifest: Manifest,
    /// The database half of the build (`docs/compiler/db.md` §6): every
    /// statement the app runs, by number, in both dialects; the migrations;
    /// the unique indexes. `None` for a program without persistence.
    pub database: Option<k_db::manifest::DbManifest>,
}

/// Why [`generate`] produced nothing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CodegenError {
    /// The program has `count` error diagnostics; `k check` reports them.
    HasErrors {
        /// The number of errors.
        count: usize,
    },
    /// The program uses something the code generator doesn't lower yet.
    /// `what` names it (`components`, `the whole program`).
    Unsupported {
        /// What is not supported.
        what: String,
    },
    /// The analysis was cancelled by a newer revision.
    Cancelled,
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::HasErrors { count } => {
                write!(f, "the program has {count} error(s); run `k check`")
            }
            CodegenError::Unsupported { what } => {
                write!(f, "code generation does not support {what} yet")
            }
            CodegenError::Cancelled => write!(f, "the analysis was cancelled"),
        }
    }
}

impl std::error::Error for CodegenError {}

/// Generates the bundles of the analysed project.
///
/// # Errors
///
/// [`CodegenError::HasErrors`] if any diagnostic is an error,
/// [`CodegenError::Cancelled`] if the analysis was cancelled, and
/// [`CodegenError::Unsupported`] for programs the generator can't lower
/// yet (currently every program).
pub fn generate(analysis: &k_sema::Analysis, options: &Options) -> Result<Output, CodegenError> {
    let diagnostics = analysis
        .diagnostics()
        .map_err(|_| CodegenError::Cancelled)?;
    let count = diagnostics.iter().filter(|d| d.is_error()).count();
    if count > 0 {
        return Err(CodegenError::HasErrors { count });
    }
    let view = analysis.program().map_err(|_| CodegenError::Cancelled)?;
    let g = cx::Gen::new(analysis, view, options);
    let out = generate_program(&g)?;
    if analysis.is_cancelled() {
        return Err(CodegenError::Cancelled);
    }
    Ok(out)
}

/// Lowers every declaration for both sides and bundles the result.
fn generate_program(g: &cx::Gen<'_>) -> Result<Output, CodegenError> {
    use cx::Target;
    use std::collections::{BTreeMap, BTreeSet};
    let mut server: BTreeMap<k_sema::view::DeclId, decl::Item> = BTreeMap::new();
    let mut client: BTreeMap<k_sema::view::DeclId, decl::Item> = BTreeMap::new();
    let app = g
        .own
        .iter()
        .copied()
        .find(|&d| g.view.decl(d).named_kind() == Some(k_sema::stdlib::DeclarationKind::App));
    let tests = g.options.entry == Entry::Tests;
    for &d in &g.own {
        for target in [Target::Server, Target::Client] {
            // A test group's class runs in the browser half of a test.
            if tests
                && target == Target::Server
                && g.view.decl(d).named_kind() == Some(k_sema::stdlib::DeclarationKind::TestSuite)
            {
                continue;
            }
            if !bundle::allowed(g, d, target) {
                continue;
            }
            if let Some(item) = decl::lower(g, d, target) {
                match target {
                    Target::Server => server.insert(d, item),
                    Target::Client => client.insert(d, item),
                };
            }
        }
    }
    // The `Page` enum is the routing table of a project with pages.
    if !app::pages(g).is_empty() {
        if let Some(p) = g.view.lookup("k.core.Page") {
            for target in [Target::Server, Target::Client] {
                if let Some(item) = decl::lower(g, p, target) {
                    match target {
                        Target::Server => server.insert(p, item),
                        Target::Client => client.insert(p, item),
                    };
                }
            }
        }
    }
    // The client entry: its uses are roots of the client bundle.
    let client_entry = if tests {
        Some(suite::entry(g))
    } else {
        app.map(|_| app::client_entry(g, &app::client_reach(g, &client)))
    };
    let entry_uses: BTreeSet<(k_sema::view::DeclId, String)> = client_entry
        .as_ref()
        .map(|(_, u)| u.clone())
        .unwrap_or_default();
    let mut output = Output {
        manifest: Manifest {
            name: g.project_name(),
            ..Manifest::default()
        },
        ..Output::default()
    };
    let mut extra = Vec::new();
    match &g.options.entry {
        Entry::Main(name) => {
            let main = find_main(g, name).ok_or_else(|| CodegenError::Unsupported {
                what: format!("an entry `{name}`: no such top-level function"),
            })?;
            let (body, uses) = main_entry(g, main);
            let spec = "k:/app/$main.js".to_owned();
            extra.push((spec.clone(), body, uses));
            output.server_entry = Some(spec);
        }
        Entry::Tests => {
            // The server half of a test: the app driven in-process.
            let (body, uses) = server::server_entry(g);
            let spec = "k:/app/$server.js".to_owned();
            extra.push((spec.clone(), body, uses));
            output.server_entry = Some(spec);
        }
        Entry::App => {
            if app.is_some() {
                let (body, uses) = server::server_entry(g);
                let spec = "k:/app/$server.js".to_owned();
                extra.push((spec.clone(), body, uses));
                output.server_entry = Some(spec);
            }
        }
    }
    let server_entry_uses: BTreeSet<(k_sema::view::DeclId, String)> = extra
        .iter()
        .flat_map(|(_, _, u): &bundle::EntryModule| u.iter().cloned())
        .collect();
    // Stdlib declarations the program references (non-native enums).
    for (items, target) in [(&mut server, Target::Server), (&mut client, Target::Client)] {
        let mut seen: BTreeSet<k_sema::view::DeclId> = items.keys().copied().collect();
        let mut first = true;
        loop {
            // The entries' uses are roots too: the client entry's for the
            // client, the server entry's (the request container's stdlib
            // enums, `HttpError` and `EmailError`) for the server.
            let from_entry: Vec<k_sema::view::DeclId> = match (first, target) {
                (true, Target::Client) => entry_uses.iter().map(|(d, _)| *d).collect(),
                (true, Target::Server) => server_entry_uses.iter().map(|(d, _)| *d).collect(),
                _ => Vec::new(),
            };
            first = false;
            let wanted: Vec<k_sema::view::DeclId> = items
                .values()
                .flat_map(|i| i.uses.iter().map(|(d, _)| *d))
                .chain(from_entry)
                .filter(|d| !seen.contains(d) && native::emits_stdlib_decl(g, *d))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if wanted.is_empty() {
                break;
            }
            for d in wanted {
                seen.insert(d);
                if let Some(item) = decl::lower(g, d, target) {
                    items.insert(d, item);
                }
            }
        }
    }
    let mut roots = BTreeSet::new();
    for (_, _, uses) in &extra {
        roots.extend(uses.iter().cloned());
    }
    bundle::settle_on_demand(&mut server, &roots);
    bundle::settle_on_demand(&mut client, &BTreeSet::new());
    output.server = bundle::server_files(g, &server, extra);
    let (pages, entry) = if let Some((stmts, uses)) = client_entry {
        // The entry's uses are roots of the client too.
        bundle::settle_on_demand(&mut client, &uses);
        // Step 2 of the pipeline (codegen.md §3, §16): an app's client
        // bundle holds only what its pages, layouts and entry reach; a
        // helper used only by server code stays out, literals and all.
        let mut roots: BTreeSet<k_sema::view::DeclId> = uses.iter().map(|(d, _)| *d).collect();
        for &d in &g.own {
            if matches!(
                g.view.decl(d).named_kind(),
                Some(
                    k_sema::stdlib::DeclarationKind::Page | k_sema::stdlib::DeclarationKind::Layout
                )
            ) {
                roots.insert(d);
            }
        }
        // `@Inject` is reached through the container, not a symbol.
        let injects: BTreeMap<k_sema::view::DeclId, Vec<k_sema::view::DeclId>> =
            g.own.iter().map(|&d| (d, server::injected(g, d))).collect();
        bundle::keep_reachable(&mut client, &roots, &injects);
        // A test bundle is loaded at once: its pages are in the shared
        // chunk, which the entry imports.
        let pages = if tests { Vec::new() } else { app::pages(g) };
        (pages, Some((stmts, uses)))
    } else {
        (Vec::new(), None)
    };
    // In a test bundle both halves share one isolate: test code (lowered
    // for the browser) that names a server-only declaration (an entity, a
    // repository's row type) imports the server module's.
    let mut from_server: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if tests {
        let entry_uses = entry.iter().flat_map(|(_, u)| u.iter());
        for (d, sym) in client
            .values()
            .flat_map(|i| i.uses.iter())
            .chain(entry_uses)
        {
            if !client.contains_key(d) && server.contains_key(d) && !g.view.decl(*d).stdlib {
                from_server
                    .entry(bundle::server_specifier(&bundle::module_path(g, *d)))
                    .or_default()
                    .insert(sym.clone());
            }
        }
    }
    output.client = bundle::client_files(g, &client, &pages, entry, &from_server);
    output.client.extend(css::files(g));
    // The server entry names the hashed client files it links.
    if let Some(entry) = &output.server_entry {
        let assets = server::assets_json(&output.client);
        for f in output.server.iter_mut().filter(|f| &f.path == entry) {
            if let Some(i) = f.contents.rfind(server::ASSETS_PLACEHOLDER) {
                f.contents
                    .replace_range(i..i + server::ASSETS_PLACEHOLDER.len(), &assets);
            }
        }
    }
    output.manifest.endpoints = app::endpoints(g)
        .into_iter()
        .map(|p| EndpointEntry {
            template: app::template(g, p),
            methods: g
                .view
                .decl(p)
                .members
                .iter()
                .filter_map(|&m| {
                    ["Get", "Post", "Put", "Patch", "Delete"]
                        .iter()
                        .find(|v| g.view.member(m).has_annotation(v))
                        .map(|v| v.to_uppercase())
                })
                .collect(),
            uses_session: server::uses_session(g, p),
        })
        .collect();
    output
        .manifest
        .endpoints
        .sort_by(|a, b| a.template.cmp(&b.template));
    if app.is_some() {
        output.manifest.config = server::config_entries(g);
        output.manifest.jobs = jobs::specs(g);
    }
    output.manifest.routes = app::pages(g)
        .into_iter()
        .map(|p| RouteEntry {
            page: app::case_name(&g.view.decl(p).name),
            template: app::template(g, p),
        })
        .collect();
    output
        .manifest
        .routes
        .sort_by(|a, b| a.template.cmp(&b.template));
    output.database = db::manifest(g).map_err(|errors| CodegenError::Unsupported {
        what: format!(
            "persistence code that can't be translated:\n  {}",
            errors.join("\n  ")
        ),
    })?;
    output.manifest.rpc_paths = if app.is_some() {
        server::rpc_paths(g)
    } else {
        output
            .server
            .iter()
            .flat_map(|f| rpc_paths_in(&f.contents))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    Ok(output)
}

/// The rpc paths in the server entry's dispatch table.
fn rpc_paths_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("\"/") {
            if let Some(end) = rest.find('"') {
                let path = &rest[..end];
                if path.contains('/') && path.contains('.') {
                    out.push(format!("/{path}"));
                }
            }
        }
    }
    out
}

/// The top-level function an [`Entry::Main`] names: `corpus.main`, or a
/// bare name for the project's one function of that name.
fn find_main(g: &cx::Gen<'_>, name: &str) -> Option<k_sema::view::DeclId> {
    let is_fn = |d: k_sema::view::DeclId| {
        g.view.decl(d).named_kind() == Some(k_sema::stdlib::DeclarationKind::Function)
    };
    if let Some(d) = g.view.lookup(name).filter(|&d| is_fn(d)) {
        return Some(d);
    }
    g.own
        .iter()
        .copied()
        .find(|&d| is_fn(d) && g.view.decl(d).name == name)
}

/// The entry module of [`Entry::Main`]: `run()` calls `main` and returns
/// the result line (testing.md §3.10).
fn main_entry(
    g: &cx::Gen<'_>,
    main: k_sema::view::DeclId,
) -> (
    Vec<js::Stmt>,
    std::collections::BTreeSet<(k_sema::view::DeclId, String)>,
) {
    use js::{Expr, Prop, Stmt, call, id, member, rt};
    let info = g.view.decl(main);
    let mut b = lower::Body::new(g, info.file, cx::Target::Server);
    let f = b.decl_ref(main);
    let sig = info.signature.clone();
    let is_async = sig.as_ref().is_some_and(|s| s.is_async);
    let args = if is_async {
        vec![Expr::Null]
    } else {
        Vec::new()
    };
    let invoke = js::arrow(vec![], call(f, args));
    let ret = sig
        .as_ref()
        .map(|s| s.ret.clone())
        .unwrap_or(k_sema::view::Type::Error);
    let encode = if ret.decl() == g.b.void || matches!(ret, k_sema::view::Type::Error) {
        Expr::Null
    } else {
        let c = codec::codec_expr(&mut b, &ret);
        js::arrow(
            vec![js::Param::new("v")],
            call(member(c, "encode"), vec![id("v")]),
        )
    };
    let describe = match sig.as_ref().and_then(|s| s.throws.clone()) {
        Some(t) if t.decl().is_some() => {
            let d = t.decl().unwrap_or(main);
            let text = b.text_fn(&t);
            js::arrow(
                vec![js::Param::new("e")],
                Expr::Object(
                    vec![
                        Prop::KeyValue("error".into(), js::str(g.view.qualified_name(d))),
                        Prop::KeyValue("case".into(), call(text, vec![id("e")])),
                        Prop::KeyValue("fields".into(), rt("payloadFields", vec![id("e")])),
                    ],
                    false,
                ),
            )
        }
        _ => Expr::Null,
    };
    let run = js::Function {
        name: "run".into(),
        params: Vec::new(),
        body: js::Body::Block(vec![Stmt::Return(Some(Expr::Await(Box::new(rt(
            "runMain",
            vec![invoke, encode, describe],
        )))))]),
        is_async: true,
        kind: js::MethodKind::Plain,
    };
    (vec![Stmt::Function(run, true)], b.uses)
}

/// The client runtime as one ES module: the client parts of the embedded
/// runtime concatenated, with the links between their files removed
/// (top-level names are unique across the runtime's client files), and
/// compacted in release builds.
pub(crate) fn runtime_chunk(profile: Profile, entry: &Entry) -> String {
    use k_runtime::Part;
    if *entry == Entry::Tests {
        return suite::RUNTIME_CHUNK.to_owned();
    }
    let mut out = String::new();
    for m in k_runtime::modules() {
        if !matches!(
            m.part,
            Part::Core | Part::Extended | Part::Stdlib | Part::Services
        ) {
            continue;
        }
        let text = k_runtime::strip_links(m.source);
        let text = if profile == Profile::Release {
            k_runtime::compact(&text).unwrap_or(text)
        } else {
            text
        };
        out.push_str(&text);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// A feature chunk (`runtime/charts.js`, `runtime/streams.js`): the
/// runtime file `specifier` with its imports of other runtime files
/// pointing at the runtime chunk `runtime_name`, which exports every name
/// they import, compacted in release builds. Only apps whose client uses
/// the feature ship it (runtime.md §2).
///
/// In a test bundle ([`Entry::Tests`]) the chunk re-exports the embedded
/// module itself, which shares the embedded runtime with everything else.
pub(crate) fn feature_chunk(
    specifier: &str,
    profile: Profile,
    runtime_name: &str,
    entry: &Entry,
) -> String {
    if *entry == Entry::Tests {
        return format!("export * from \"{specifier}\";\n");
    }
    let source = k_runtime::module(specifier).map_or("", |m| m.source);
    let mut text = String::with_capacity(source.len());
    for line in source.lines() {
        match line
            .strip_prefix("import ")
            .and_then(|l| l.split_once(" from \"./"))
        {
            Some((names, _)) => {
                text.push_str(&format!("import {names} from \"./{runtime_name}\";\n"));
            }
            None => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    if profile == Profile::Release {
        k_runtime::compact(&text).unwrap_or(text)
    } else {
        text
    }
}

/// A violation of the client/server split found in generated output.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SplitViolation {
    /// The client file.
    pub file: String,
    /// What it contains that it must not.
    pub found: String,
}

/// Checks the client bundle of `output` for anything that must never reach
/// a browser: the server runtime (`k:runtime/server`, `Deno.core`), any
/// server module specifier (`k:/app/…`, which only server modules use),
/// and each string of `forbidden` (names of server-only declarations,
/// secret values in tests).
///
/// Codegen guarantees the split by construction; this is the independent
/// check that tests and `k build` run on the result.
#[must_use]
pub fn check_client_bundle(output: &Output, forbidden: &[&str]) -> Vec<SplitViolation> {
    let mut out = Vec::new();
    for file in &output.client {
        for needle in ["k:runtime/server", "Deno.core", "k:/app/", "op_k_"]
            .iter()
            .chain(forbidden)
        {
            if file.contents.contains(needle) {
                out.push(SplitViolation {
                    file: file.path.clone(),
                    found: (*needle).to_owned(),
                });
            }
        }
    }
    out
}
