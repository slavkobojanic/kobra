//! Workspaces: several projects in one directory tree, and the libraries
//! they share (`docs/design/40-workspaces-and-services.md`).
//!
//! # Model
//!
//! A **workspace** is a directory whose `k.toml` has a `[workspace]` table
//! listing its member projects:
//!
//! ```toml
//! [workspace]
//! members = ["pay-contracts", "paybox", "shop"]
//! ```
//!
//! Each member is an ordinary project (an app with `App.k`, or a library)
//! and depends on libraries of the same workspace with
//! `name = { workspace = true }` under `[dependencies]`, where `name` is the
//! library's `[package] name`. A project outside any workspace is a
//! workspace of one member whose directory is the root.
//!
//! [`Workspace::from_files`] takes every file below the root, keyed by its
//! path relative to the root, and plans the build: the members, their
//! dependency graph and the manifest problems (`K8101`–`K8106`). The graph
//! must be a DAG; a cycle (`K8104`) and a dependency on an app (`K8105`)
//! are reported at the offending `[dependencies]` entries and leave the
//! edge out, so every member's analysis still runs.
//!
//! **A member's analysis holds its own files and, mounted below
//! [`DEPENDENCY_DIR`](crate::DEPENDENCY_DIR), the sources of every library it depends on,
//! transitively** ([`Workspace::analysis_files`], [`ProjectPath::mounted`]).
//! A library's modules keep their own paths (`pay.Payments`), so its code
//! is imported exactly as a module of the member; a module declared by two
//! packages of one build is `K8106`. Only a library's non-test sources are
//! mounted: its tests, lockfiles and migrations stay its own.
//!
//! [`Workspace::report`] merges the diagnostics of every member's analysis
//! into one [`SourceMap`] of real paths: a library's diagnostics appear
//! once, however many members mount it.
//!
//! # Guarantees
//!
//! Never panics on any input; the result depends only on the file texts and
//! paths, sorted, never on the order they are given in.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use k_diagnostics::{Code, Diagnostic, FileId, SourceMap, Span, TextRange, TextSize, codes};

use crate::host::{Analysis, FileOrigin};
use crate::lookup::Cancelled;
use crate::project::{FileKind, ProjectPath};
use crate::stdlib::Stdlib;

mod requirement;

pub use requirement::{Requirement, TOOLCHAIN_LOCK, Version, lock_toolchain, with_lock_toolchain};

/// The manifest file name.
pub const MANIFEST: &str = "k.toml";

// ------------------------------------------------------------- manifests

/// What a `k.toml` says, as far as the workspace is concerned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    /// `[package] name`, when it is a string.
    pub name: Option<String>,
    /// `true` when the manifest has a `[workspace]` table.
    pub is_workspace: bool,
    /// `[workspace] members`, each with the range of its string.
    pub members: Vec<(String, TextRange)>,
    /// `[dependencies]`, in file order.
    pub dependencies: Vec<Dependency>,
    /// `[dev]`: what `k dev` and `k run` add to the development
    /// environment (`docs/design/24-projects.md`, "Development settings").
    pub dev: DevSettings,
    /// `[package] k` or `[workspace] k`, the toolchain requirement, with
    /// the range of its line, when it is a valid one.
    pub k: Option<(Requirement, TextRange)>,
    /// `true` when the manifest has a `k` key, valid or not.
    pub has_k: bool,
}

/// A toolchain requirement of a workspace's manifests
/// ([`Workspace::requirements`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainRequirement {
    /// The manifest's path relative to the workspace root.
    pub path: String,
    /// The range of its `k` line.
    pub range: TextRange,
    /// The requirement.
    pub requirement: Requirement,
}

/// The `[dev]` section of a package's `k.toml`: development-only values,
/// read by `k dev`, `k run` and `k migrate`, never by a build's `dist/app`
/// or by `k test`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DevSettings {
    /// `[dev.env]`: values for config variables, below the process
    /// environment and `.env`. `{origin}` and `{port}` in a value stand
    /// for the app's development origin and port.
    pub env: BTreeMap<String, String>,
    /// `[dev.secrets]`: secret variables generated on first use and kept
    /// in `.k/secrets/`, never committed.
    pub secrets: BTreeMap<String, DevSecret>,
    /// `[dev] seed`: a SQL file run once, when `k dev` or `k run` creates
    /// the development database.
    pub seed: Option<String>,
}

/// What `[dev.secrets]` generates for a variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevSecret {
    /// `"token"`: 32 random bytes, base64url.
    Token,
    /// `"password-hash"`: a random password, kept next to its Argon2id
    /// hash (the variable's value) and printed at startup.
    PasswordHash,
}

/// One `[dependencies]` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// The key: a package name or module path.
    pub name: String,
    /// What the value asks for.
    pub source: DependencySource,
    /// The range of the entry's line.
    pub range: TextRange,
}

/// Where a dependency comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencySource {
    /// `{ workspace = true }`: the workspace member with that package name.
    Workspace,
    /// `"v1.2.0"`: a git tag (`docs/design/18-packages.md`), not resolved yet.
    Git(String),
    /// Anything else: reported as `K8101`.
    Invalid,
}

/// A problem found in a manifest, reported as a diagnostic at `range` of
/// the file at `path` (relative to the workspace root).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The manifest's path relative to the workspace root (`shop/k.toml`).
    pub path: String,
    /// The code (`K8101`–`K8106`).
    pub code: Code,
    /// The range in that file.
    pub range: TextRange,
    /// The message.
    pub message: String,
    /// Notes, in order.
    pub notes: Vec<String>,
}

/// Parses a `k.toml`. Problems are reported with `path` as their file.
/// `is_root` is `true` for the manifest at the workspace root, `in_workspace`
/// for a member's manifest.
#[must_use]
pub fn parse_manifest(
    path: &str,
    text: &str,
    is_root: bool,
    in_workspace: bool,
) -> (Manifest, Vec<Problem>) {
    let mut problems = Vec::new();
    let mut out = Manifest::default();
    let problem = |problems: &mut Vec<Problem>, range: TextRange, message: String| {
        problems.push(Problem {
            path: path.to_owned(),
            code: codes::INVALID_MANIFEST,
            range,
            message,
            notes: Vec::new(),
        });
    };
    let table: toml::Table = match toml::from_str(text) {
        Ok(t) => t,
        Err(e) => {
            let offset = e.span().map_or(0, |s| s.start);
            problem(
                &mut problems,
                line_at(text, offset),
                format!("k.toml can't be read: {}", e.message().trim()),
            );
            return (out, problems);
        }
    };
    if let Some(ws) = table.get("workspace") {
        out.is_workspace = true;
        let header = section_line(text, "workspace");
        if !is_root || in_workspace {
            problem(
                &mut problems,
                header,
                "a workspace can't be nested in another workspace's member".to_owned(),
            );
        }
        match ws.as_table() {
            None => problem(
                &mut problems,
                header,
                "`workspace` must be a table".to_owned(),
            ),
            Some(ws) => match ws.get("members") {
                None => problem(
                    &mut problems,
                    header,
                    "`[workspace]` needs `members`, the list of member directories".to_owned(),
                ),
                Some(members) => match members.as_array() {
                    None => problem(
                        &mut problems,
                        key_line(text, "workspace", "members"),
                        "`members` must be a list of directory names".to_owned(),
                    ),
                    Some(list) => {
                        let mut from = text.find("members").unwrap_or(0);
                        for m in list {
                            let Some(name) = m.as_str() else {
                                problem(
                                    &mut problems,
                                    key_line(text, "workspace", "members"),
                                    "`members` must be a list of directory names".to_owned(),
                                );
                                continue;
                            };
                            let range = string_range(text, name, &mut from)
                                .unwrap_or_else(|| key_line(text, "workspace", "members"));
                            out.members.push((name.to_owned(), range));
                        }
                    }
                },
            },
        }
        if let Some(k) = ws.as_table().and_then(|t| t.get("k")) {
            out.has_k = true;
            let range = key_line(text, "workspace", "k");
            match k.as_str().and_then(Requirement::parse) {
                Some(req) => out.k = Some((req, range)),
                None => problem(&mut problems, range, requirement_message("workspace")),
            }
        }
        if table.contains_key("package") {
            problem(
                &mut problems,
                section_line(text, "package"),
                "a workspace root is not a package: move `[package]` into a member".to_owned(),
            );
        }
    }
    if !out.is_workspace {
        match table.get("package").and_then(toml::Value::as_table) {
            None => problem(
                &mut problems,
                line_at(text, 0),
                "k.toml needs a `[package]` table with `name` and `k`".to_owned(),
            ),
            Some(package) => {
                match package.get("name") {
                    Some(toml::Value::String(name)) if !name.is_empty() => {
                        out.name = Some(name.clone());
                    }
                    _ => problem(
                        &mut problems,
                        key_or_section(text, "package", "name"),
                        "`[package] name` must be a non-empty string".to_owned(),
                    ),
                }
                match package.get("k") {
                    // A member may leave it to `[workspace] k`, which
                    // `Workspace::from_files` checks.
                    None if in_workspace => {}
                    None => problem(
                        &mut problems,
                        key_or_section(text, "package", "k"),
                        "`[package] k`, the toolchain requirement, must be a string such as \"0.1\""
                            .to_owned(),
                    ),
                    Some(k) => {
                        out.has_k = true;
                        let range = key_line(text, "package", "k");
                        match k.as_str().and_then(Requirement::parse) {
                            Some(req) => out.k = Some((req, range)),
                            None => problem(&mut problems, range, requirement_message("package")),
                        }
                    }
                }
            }
        }
    }
    if let Some(deps) = table.get("dependencies") {
        match deps.as_table() {
            None => problem(
                &mut problems,
                key_or_section(text, "", "dependencies"),
                "`dependencies` must be a table".to_owned(),
            ),
            Some(deps) => {
                for (name, value) in deps {
                    let source = match value {
                        toml::Value::String(tag) => DependencySource::Git(tag.clone()),
                        toml::Value::Table(t)
                            if t.len() == 1
                                && t.get("workspace") == Some(&toml::Value::Boolean(true)) =>
                        {
                            DependencySource::Workspace
                        }
                        _ => DependencySource::Invalid,
                    };
                    let range = key_line(text, "dependencies", name);
                    if source == DependencySource::Invalid {
                        problem(
                            &mut problems,
                            range,
                            format!(
                                "dependency `{name}` must be a git tag (\"v1.2.0\") or `{{ workspace = true }}`"
                            ),
                        );
                    }
                    out.dependencies.push(Dependency {
                        name: name.clone(),
                        source,
                        range,
                    });
                }
                out.dependencies.sort_by_key(|d| d.range.start());
            }
        }
    }
    if let Some(dev) = table.get("dev") {
        if out.is_workspace {
            problem(
                &mut problems,
                section_line(text, "dev"),
                "a workspace root has no `[dev]` section: put it in each member's k.toml"
                    .to_owned(),
            );
        } else {
            for (range, message) in parse_dev(text, dev, &mut out.dev) {
                problem(&mut problems, range, message);
            }
        }
    }
    (out, problems)
}

/// The `K8101` message for a `k` in `[section]` that isn't a requirement.
fn requirement_message(section: &str) -> String {
    format!(
        "`[{section}] k` must be a toolchain requirement such as \"0.1\" (any 0.1.x) or \"=0.1.2\" (exactly that version)"
    )
}

/// Reads the `[dev]` table into `out`, returning its problems.
fn parse_dev(text: &str, dev: &toml::Value, out: &mut DevSettings) -> Vec<(TextRange, String)> {
    let mut problems = Vec::new();
    let Some(dev) = dev.as_table() else {
        return vec![(
            key_or_section(text, "", "dev"),
            "`dev` must be a table".to_owned(),
        )];
    };
    let mut unknown = Vec::new();
    for (key, value) in dev {
        match key.as_str() {
            "seed" => match value {
                toml::Value::String(path) if !path.is_empty() => out.seed = Some(path.clone()),
                _ => problems.push((
                    key_or_section(text, "dev", "seed"),
                    "`[dev] seed` must be the path of a SQL file, such as \"scripts/seed.sql\""
                        .to_owned(),
                )),
            },
            "env" => match value.as_table() {
                None => problems.push((
                    key_or_section(text, "dev", "env"),
                    "`[dev] env` must be a table".to_owned(),
                )),
                Some(env) => {
                    for (name, v) in env {
                        match v {
                            toml::Value::String(s) => {
                                out.env.insert(name.clone(), s.clone());
                            }
                            _ => problems.push((
                                key_line(text, "dev.env", name),
                                format!("`[dev.env] {name}` must be a string"),
                            )),
                        }
                    }
                }
            },
            "secrets" => match value.as_table() {
                None => problems.push((
                    key_or_section(text, "dev", "secrets"),
                    "`[dev] secrets` must be a table".to_owned(),
                )),
                Some(secrets) => {
                    for (name, v) in secrets {
                        let kind = match v.as_str() {
                            Some("token") => Some(DevSecret::Token),
                            Some("password-hash") => Some(DevSecret::PasswordHash),
                            _ => None,
                        };
                        match kind {
                            Some(k) => {
                                out.secrets.insert(name.clone(), k);
                            }
                            None => problems.push((
                                key_line(text, "dev.secrets", name),
                                format!(
                                    "`[dev.secrets] {name}` must be \"token\" or \"password-hash\""
                                ),
                            )),
                        }
                    }
                }
            },
            other => unknown.push(other.to_owned()),
        }
    }
    for key in unknown {
        problems.push((
            key_line(text, "dev", &key),
            format!("unknown key `{key}` in `[dev]`; expected `env`, `secrets` or `seed`"),
        ));
    }
    problems.sort_by_key(|(r, _)| r.start());
    problems
}

/// The range of the line containing `offset`, without its line break.
fn line_at(text: &str, offset: usize) -> TextRange {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let start = text[..offset].rfind('\n').map_or(0, |n| n + 1);
    let end = text[start..].find('\n').map_or(text.len(), |n| start + n);
    let end = if text[start..end].ends_with('\r') {
        end - 1
    } else {
        end
    };
    TextRange::new(size(start), size(end))
}

/// A byte offset as a [`TextSize`].
fn size(n: usize) -> TextSize {
    TextSize::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The line of the `[section]` header, else offset 0's line.
fn section_line(text: &str, section: &str) -> TextRange {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[')
            && t.trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .trim_matches('"')
                == section
        {
            return line_at(text, offset);
        }
        offset += line.len();
    }
    line_at(text, 0)
}

/// The line of `key` inside `[section]` (`""` for the top level), else the
/// section's header, else offset 0's line.
fn key_line(text: &str, section: &str, key: &str) -> TextRange {
    let mut offset = 0;
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') {
            current = t
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .trim_matches('"')
                .to_owned();
        } else if current == section {
            let k = t.split('=').next().unwrap_or("").trim().trim_matches('"');
            if k == key && t.contains('=') {
                return line_at(text, offset);
            }
        }
        // `[dependencies.name]` as a table of its own.
        if t.starts_with('[') && current == format!("{section}.{key}") {
            return line_at(text, offset);
        }
        offset += line.len();
    }
    section_line(text, section)
}

/// [`key_line`], which already falls back to the section.
fn key_or_section(text: &str, section: &str, key: &str) -> TextRange {
    key_line(text, section, key)
}

/// The range of the string literal `"value"` at or after `*from`, moving
/// `*from` past it.
fn string_range(text: &str, value: &str, from: &mut usize) -> Option<TextRange> {
    for quote in ['"', '\''] {
        let needle = format!("{quote}{value}{quote}");
        if let Some(i) = text.get(*from..).and_then(|t| t.find(&needle)) {
            let start = *from + i;
            *from = start + needle.len();
            return Some(TextRange::new(size(start), size(start + needle.len())));
        }
    }
    None
}

// ------------------------------------------------------------- workspaces

/// One member project of a workspace.
#[derive(Clone, Debug)]
pub struct Member {
    /// The member's directory relative to the workspace root, with `/`
    /// separators; empty for a project that is its own workspace.
    pub dir: String,
    /// `[package] name`, or the directory name when the manifest has none.
    pub name: String,
    /// `true` for an app (it has `App.k`), `false` for a library.
    pub is_app: bool,
    /// The members it depends on directly, by index, without the edges
    /// reported as problems (cycles, apps).
    pub dependencies: Vec<usize>,
    /// Every library it depends on, transitively, dependencies first.
    pub libraries: Vec<usize>,
    /// Its own files, by path relative to its directory, sorted.
    pub files: Vec<(ProjectPath, String)>,
}

impl Member {
    /// Returns the path of the member's file `path` relative to the
    /// workspace root.
    #[must_use]
    pub fn real_path(&self, path: &str) -> String {
        join(&self.dir, path)
    }
}

/// `dir/path`, or `path` for the root.
fn join(dir: &str, path: &str) -> String {
    if dir.is_empty() {
        path.to_owned()
    } else {
        format!("{dir}/{path}")
    }
}

/// A planned workspace: its members, their dependency graph and the
/// problems of its manifests.
#[derive(Clone, Debug, Default)]
pub struct Workspace {
    /// `true` when the root's `k.toml` has a `[workspace]` table.
    pub is_workspace: bool,
    /// The members, sorted by directory.
    pub members: Vec<Member>,
    /// The manifest problems, sorted by path and range.
    pub problems: Vec<Problem>,
    /// Files that belong to no member (the root's own `k.toml` included),
    /// by path relative to the root.
    pub loose: Vec<(String, String)>,
    /// The valid toolchain requirements of the manifests: the root's
    /// `[workspace] k` first, then each member's `[package] k`.
    requirements: Vec<ToolchainRequirement>,
}

impl Workspace {
    /// Returns `true` when `text`, a root `k.toml`, declares a workspace.
    /// Malformed TOML is not a workspace.
    #[must_use]
    pub fn is_workspace_manifest(text: &str) -> bool {
        toml::from_str::<toml::Table>(text).is_ok_and(|t| t.contains_key("workspace"))
    }

    /// Plans the workspace whose files are `files`, keyed by their path
    /// relative to the root (`k.toml`, `shop/App.k`, …).
    ///
    /// When the root `k.toml` declares no workspace, the whole tree is one
    /// member with an empty directory. Files below no member are kept in
    /// [`Workspace::loose`] and analysed by nobody. Problems in the
    /// manifests are collected in [`Workspace::problems`], never raised.
    ///
    /// ```
    /// use k_sema::Workspace;
    ///
    /// let manifest = |name: &str, deps: &str| {
    ///     format!("[package]\nname = \"{name}\"\nk = \"0.1\"\n{deps}")
    /// };
    /// let ws = Workspace::from_files([
    ///     ("k.toml".to_owned(), "[workspace]\nmembers = [\"lib\", \"app\"]\n".to_owned()),
    ///     ("lib/k.toml".to_owned(), manifest("lib", "")),
    ///     (
    ///         "app/k.toml".to_owned(),
    ///         manifest("app", "[dependencies]\nlib = { workspace = true }\n"),
    ///     ),
    ///     ("app/App.k".to_owned(), "public app Shop {}\n".to_owned()),
    /// ]);
    /// assert!(ws.is_workspace);
    /// assert!(ws.problems.is_empty(), "{:?}", ws.problems);
    /// let app = ws.member("app").unwrap();
    /// let lib = ws.member("lib").unwrap();
    /// assert!(ws.members[app].is_app);
    /// // Libraries come before the members that depend on them.
    /// assert_eq!(ws.order(), vec![lib, app]);
    /// assert_eq!(ws.owner_of("app/App.k"), Some(app));
    /// ```
    #[must_use]
    pub fn from_files(files: impl IntoIterator<Item = (String, String)>) -> Workspace {
        let mut files: Vec<(String, String)> = files.into_iter().collect();
        files.sort();
        files.dedup_by(|a, b| a.0 == b.0);
        let root_text = files
            .iter()
            .find(|(p, _)| p == MANIFEST)
            .map(|(_, t)| t.clone());
        let mut ws = Workspace::default();
        let root = root_text
            .as_deref()
            .map(|t| parse_manifest(MANIFEST, t, true, false));
        let is_workspace = root.as_ref().is_some_and(|(m, _)| m.is_workspace);
        ws.is_workspace = is_workspace;
        // The member directories and their manifests.
        let mut dirs: Vec<(String, Option<TextRange>)> = Vec::new();
        if let Some((manifest, problems)) = &root {
            if is_workspace {
                ws.problems.extend(problems.iter().cloned());
                let mut seen = BTreeSet::new();
                for (name, range) in &manifest.members {
                    match normalize_dir(name) {
                        Some(dir) if seen.insert(dir.clone()) => dirs.push((dir, Some(*range))),
                        Some(_) => ws.problems.push(manifest_problem(
                            MANIFEST,
                            *range,
                            format!("`{name}` is listed twice in `members`"),
                        )),
                        None => ws.problems.push(manifest_problem(
                            MANIFEST,
                            *range,
                            format!(
                                "`{name}` is not a member directory: use a relative path below the workspace root, without `.` or `..`"
                            ),
                        )),
                    }
                }
                // A member inside another member would own its files twice.
                let all: Vec<String> = dirs.iter().map(|(d, _)| d.clone()).collect();
                dirs.retain(|(d, range)| {
                    let nested = all
                        .iter()
                        .any(|o| o != d && d.starts_with(&format!("{o}/")));
                    if nested {
                        ws.problems.push(manifest_problem(
                            MANIFEST,
                            range.unwrap_or_default(),
                            format!("member `{d}` is inside another member"),
                        ));
                    }
                    !nested
                });
            } else {
                dirs.push((String::new(), None));
            }
        } else {
            dirs.push((String::new(), None));
        }
        dirs.sort_by(|a, b| a.0.cmp(&b.0));
        // Assign the files to members.
        let mut owned: Vec<Vec<(ProjectPath, String)>> = vec![Vec::new(); dirs.len()];
        for (path, text) in files {
            let owner = dirs
                .iter()
                .enumerate()
                .filter(|(_, (d, _))| d.is_empty() || path.starts_with(&format!("{d}/")))
                .max_by_key(|(_, (d, _))| d.len())
                .map(|(i, _)| i);
            let inner = owner.map(|i| {
                let d = &dirs[i].0;
                if d.is_empty() {
                    path.clone()
                } else {
                    path[d.len() + 1..].to_owned()
                }
            });
            match (owner, inner.and_then(|p| ProjectPath::new(&p).ok())) {
                (Some(i), Some(p)) if !(is_workspace && path == MANIFEST) => {
                    owned[i].push((p, text));
                }
                _ => ws.loose.push((path, text)),
            }
        }
        // Parse each member's manifest.
        let mut manifests: Vec<Manifest> = Vec::new();
        for (i, (dir, range)) in dirs.iter().enumerate() {
            let manifest_path = join(dir, MANIFEST);
            let text = owned[i]
                .iter()
                .find(|(p, _)| p.as_str() == MANIFEST)
                .map(|(_, t)| t.as_str());
            let manifest = match text {
                Some(text) => {
                    let (m, problems) =
                        parse_manifest(&manifest_path, text, !is_workspace, is_workspace);
                    // A project outside a workspace already reported its
                    // own manifest as the root.
                    ws.problems.extend(problems);
                    m
                }
                None => {
                    if let Some(range) = range {
                        ws.problems.push(Problem {
                            path: MANIFEST.to_owned(),
                            code: codes::MISSING_WORKSPACE_MEMBER,
                            range: *range,
                            message: format!("workspace member `{dir}` has no k.toml"),
                            notes: vec![format!(
                                "create `{dir}/k.toml` with a `[package]` table, or remove `{dir}` from `members`"
                            )],
                        });
                    }
                    Manifest::default()
                }
            };
            manifests.push(manifest);
        }
        // One toolchain: every member's requirement must allow a version
        // the root's does, and a member without one relies on the root's.
        let root_k = root
            .as_ref()
            .filter(|_| is_workspace)
            .map(|(m, _)| (m.k.clone(), m.has_k));
        if let Some((Some((req, range)), _)) = &root_k {
            ws.requirements.push(ToolchainRequirement {
                path: MANIFEST.to_owned(),
                range: *range,
                requirement: req.clone(),
            });
        }
        for (i, manifest) in manifests.iter().enumerate() {
            let path = join(&dirs[i].0, MANIFEST);
            let present = owned[i].iter().any(|(p, _)| p.as_str() == MANIFEST);
            if let Some((req, range)) = &manifest.k {
                if let Some((Some((root_req, _)), _)) = &root_k
                    && !req.intersects(root_req)
                {
                    ws.problems.push(Problem {
                        path: path.clone(),
                        code: codes::CONFLICTING_TOOLCHAIN,
                        range: *range,
                        message: format!(
                            "`k = \"{req}\"` allows no toolchain that the workspace's `k = \"{root_req}\"` allows"
                        ),
                        notes: vec![
                            "a workspace builds with one toolchain: require a version the root's `[workspace] k` allows, or remove `k` here".to_owned(),
                        ],
                    });
                }
                ws.requirements.push(ToolchainRequirement {
                    path: path.clone(),
                    range: *range,
                    requirement: req.clone(),
                });
            } else if is_workspace
                && present
                && !manifest.has_k
                && !manifest.is_workspace
                && root_k.as_ref().is_some_and(|(_, has)| !has)
            {
                let text = owned[i]
                    .iter()
                    .find(|(p, _)| p.as_str() == MANIFEST)
                    .map_or("", |(_, t)| t.as_str());
                ws.problems.push(Problem {
                    path: path.clone(),
                    code: codes::INVALID_MANIFEST,
                    range: key_or_section(text, "package", "k"),
                    message:
                        "`[package] k`, the toolchain requirement, must be a string such as \"0.1\""
                            .to_owned(),
                    notes: vec![
                        "or give the whole workspace one requirement in the root's `[workspace] k`"
                            .to_owned(),
                    ],
                });
            }
        }
        let mut members: Vec<Member> = dirs
            .iter()
            .zip(owned)
            .zip(&manifests)
            .map(|(((dir, _), files), manifest)| Member {
                name: manifest
                    .name
                    .clone()
                    .unwrap_or_else(|| dir.rsplit('/').next().unwrap_or_default().to_owned()),
                is_app: files.iter().any(|(p, _)| p.as_str() == "App.k"),
                dir: dir.clone(),
                dependencies: Vec::new(),
                libraries: Vec::new(),
                files,
            })
            .collect();
        // Package names are unique.
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for (i, m) in members.iter().enumerate() {
            if let Some(&first) = by_name.get(&m.name) {
                let path = join(&m.dir, MANIFEST);
                let range = members[i]
                    .files
                    .iter()
                    .find(|(p, _)| p.as_str() == MANIFEST)
                    .map_or_else(TextRange::default, |(_, t)| key_line(t, "package", "name"));
                ws.problems.push(manifest_problem(
                    &path,
                    range,
                    format!(
                        "`{}` is also the name of member `{}`; package names are unique in a workspace",
                        m.name, members[first].dir
                    ),
                ));
            } else {
                by_name.insert(m.name.clone(), i);
            }
        }
        // Resolve the dependencies.
        let mut edges: Vec<Vec<(usize, TextRange)>> = vec![Vec::new(); members.len()];
        for (i, manifest) in manifests.iter().enumerate() {
            let path = join(&members[i].dir, MANIFEST);
            for dep in &manifest.dependencies {
                match &dep.source {
                    DependencySource::Invalid => {}
                    DependencySource::Git(tag) => ws.problems.push(Problem {
                        path: path.clone(),
                        code: codes::UNKNOWN_DEPENDENCY,
                        range: dep.range,
                        message: format!(
                            "package `{}` at `{tag}` can't be resolved: git packages aren't supported yet",
                            dep.name
                        ),
                        notes: vec![
                            "depend on a library of the same workspace with `{ workspace = true }`"
                                .to_owned(),
                        ],
                    }),
                    DependencySource::Workspace => {
                        let target = by_name.get(&dep.name).copied();
                        match target {
                            _ if !is_workspace => ws.problems.push(Problem {
                                path: path.clone(),
                                code: codes::UNKNOWN_DEPENDENCY,
                                range: dep.range,
                                message: format!(
                                    "`{}` is a workspace dependency, but this project is not in a workspace",
                                    dep.name
                                ),
                                notes: vec!["list the projects in a root k.toml's `[workspace] members`".to_owned()],
                            }),
                            None => ws.problems.push(Problem {
                                path: path.clone(),
                                code: codes::UNKNOWN_DEPENDENCY,
                                range: dep.range,
                                message: format!(
                                    "no workspace member is named `{}`",
                                    dep.name
                                ),
                                notes: member_names_note(&members),
                            }),
                            Some(t) if t == i => ws.problems.push(Problem {
                                path: path.clone(),
                                code: codes::PACKAGE_CYCLE,
                                range: dep.range,
                                message: format!("`{}` depends on itself", dep.name),
                                notes: Vec::new(),
                            }),
                            Some(t) if members[t].is_app => ws.problems.push(Problem {
                                path: path.clone(),
                                code: codes::APP_DEPENDENCY,
                                range: dep.range,
                                message: format!(
                                    "`{}` is an app; only libraries can be depended on",
                                    dep.name
                                ),
                                notes: vec![
                                    "move the protocols and types both sides need into a library, and depend on it from each app".to_owned(),
                                ],
                            }),
                            Some(t) => {
                                if !edges[i].iter().any(|(x, _)| *x == t) {
                                    edges[i].push((t, dep.range));
                                }
                            }
                        }
                    }
                }
            }
        }
        // Cycles: every edge inside a strongly connected component.
        let sccs = tarjan(&edges);
        let mut component = vec![usize::MAX; members.len()];
        for (c, scc) in sccs.iter().enumerate() {
            for &v in scc {
                component[v] = c;
            }
        }
        let snapshot = edges.clone();
        for (i, out) in edges.iter_mut().enumerate() {
            out.retain(|&(t, range)| {
                let cyclic = component[i] == component[t] && sccs[component[i]].len() > 1;
                if cyclic {
                    let names = cycle_path(&snapshot, &sccs[component[i]], i, t, &members);
                    ws.problems.push(Problem {
                        path: join(&members[i].dir, MANIFEST),
                        code: codes::PACKAGE_CYCLE,
                        range,
                        message: format!(
                            "`{}` and `{}` depend on each other",
                            members[i].name, members[t].name
                        ),
                        notes: vec![
                            format!("the cycle: {}", names.join(" → ")),
                            "dependencies must form a DAG: move what both need into a library both depend on".to_owned(),
                        ],
                    });
                }
                !cyclic
            });
        }
        for (i, m) in members.iter_mut().enumerate() {
            m.dependencies = edges[i].iter().map(|(t, _)| *t).collect();
        }
        for i in 0..members.len() {
            let mut order = Vec::new();
            let mut seen = BTreeSet::new();
            for &d in &members[i].dependencies.clone() {
                post_order(&members, d, &mut seen, &mut order);
            }
            members[i].libraries = order;
        }
        // A module belongs to one package of a build.
        for i in 0..members.len() {
            let mut owner: BTreeMap<String, usize> = BTreeMap::new();
            for m in modules_of(&members[i]) {
                owner.insert(m, i);
            }
            for &lib in &members[i].libraries.clone() {
                for module in modules_of(&members[lib]) {
                    match owner.get(&module) {
                        Some(&other) if other != lib => {
                            let range = direct_entry(&members, &edges, i, lib);
                            ws.problems.push(Problem {
                                path: join(&members[i].dir, MANIFEST),
                                code: codes::DUPLICATE_MODULE,
                                range,
                                message: format!(
                                    "module `{module}` is declared by both `{}` and `{}`",
                                    members[lib].name, members[other].name
                                ),
                                notes: vec![
                                    "a library's modules are imported by their own paths, so rename one of them".to_owned(),
                                ],
                            });
                        }
                        Some(_) => {}
                        None => {
                            owner.insert(module, lib);
                        }
                    }
                }
            }
        }
        ws.members = members;
        ws.problems.sort_by(|a, b| {
            (&a.path, a.range.start(), a.range.end(), a.code).cmp(&(
                &b.path,
                b.range.start(),
                b.range.end(),
                b.code,
            ))
        });
        ws.problems.dedup();
        ws
    }

    /// The valid toolchain requirements of the workspace's manifests: the
    /// root's `[workspace] k` first, then each member's `[package] k`, in
    /// member order. The toolchain that builds the workspace must satisfy
    /// every one (`docs/design/48-releases-and-versioning.md`, "The
    /// requirement and the lock").
    #[must_use]
    pub fn requirements(&self) -> &[ToolchainRequirement] {
        &self.requirements
    }

    /// Returns the index of the member whose directory is `dir`.
    #[must_use]
    pub fn member(&self, dir: &str) -> Option<usize> {
        self.members.iter().position(|m| m.dir == dir)
    }

    /// Returns the index of the member that owns the file at `path`
    /// (relative to the root): the member with the longest directory that
    /// contains it.
    #[must_use]
    pub fn owner_of(&self, path: &str) -> Option<usize> {
        self.members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.dir.is_empty() || path.starts_with(&format!("{}/", m.dir)))
            .max_by_key(|(_, m)| m.dir.len())
            .map(|(i, _)| i)
    }

    /// The members in dependency order: every member after the libraries
    /// it depends on, ties by directory.
    #[must_use]
    pub fn order(&self) -> Vec<usize> {
        let mut order = Vec::new();
        let mut seen = BTreeSet::new();
        for i in 0..self.members.len() {
            post_order(&self.members, i, &mut seen, &mut order);
        }
        order
    }

    /// The files of member `member`'s analysis: its own, then the sources
    /// of every library it depends on (transitively), mounted below
    /// [`DEPENDENCY_DIR`](crate::DEPENDENCY_DIR) with [`ProjectPath::mounted`]. Each library's
    /// `k.toml` is mounted too (an inert file the analysis ignores), so
    /// code generation can read the library's name.
    #[must_use]
    pub fn analysis_files(&self, member: usize) -> Vec<(ProjectPath, String)> {
        let Some(m) = self.members.get(member) else {
            return Vec::new();
        };
        let mut out = m.files.clone();
        for &lib in &m.libraries {
            let l = &self.members[lib];
            for (path, text) in &l.files {
                if mounts(path) {
                    out.push((ProjectPath::mounted(&l.dir, path), text.clone()));
                }
            }
        }
        out
    }

    /// The path, relative to the workspace root, of `path` in member
    /// `member`'s analysis: a mounted library file is the library's own
    /// file. `None` for an unknown member.
    #[must_use]
    pub fn real_path(&self, member: usize, path: &ProjectPath) -> Option<String> {
        let m = self.members.get(member)?;
        Some(match path.dependency() {
            Some((dir, inner)) => join(&dir, inner.as_str()),
            None => m.real_path(path.as_str()),
        })
    }

    /// The member and member-analysis path of the file at `real` (relative
    /// to the root) as seen from member `viewer`: the viewer's own file, or
    /// the mounted copy of a library file the viewer depends on. `None`
    /// when the viewer doesn't see the file.
    #[must_use]
    pub fn view_path(&self, viewer: usize, real: &str) -> Option<ProjectPath> {
        let owner = self.owner_of(real)?;
        let o = &self.members[owner];
        let inner = if o.dir.is_empty() {
            real.to_owned()
        } else {
            real.strip_prefix(&format!("{}/", o.dir))?.to_owned()
        };
        let inner = ProjectPath::new(&inner).ok()?;
        if owner == viewer {
            return Some(inner);
        }
        let v = self.members.get(viewer)?;
        (v.libraries.contains(&owner) && mounts(&inner))
            .then(|| ProjectPath::mounted(&o.dir, &inner))
    }

    /// Merges the diagnostics of the member analyses in `analyses` (member
    /// index and its analysis) with the manifest problems into one source
    /// map whose files are named `display(path)` (`path` relative to the
    /// root; stdlib files keep their ids and `<stdlib>/…` names). A
    /// diagnostic found by several members (one in a library they all
    /// mount) is reported once. Sorted like [`Analysis::diagnostics`].
    ///
    /// Only problems of manifests that belong to `analyses`' members, or to
    /// no member (the root's), are included, so checking one member of a
    /// workspace doesn't report another member's manifest.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if an analysis is stale.
    pub fn report(
        &self,
        analyses: &[(usize, Analysis)],
        display: impl Fn(&str) -> String,
    ) -> Result<(SourceMap, Vec<Diagnostic>), Cancelled> {
        self.report_with(analyses, |_| Vec::new(), display)
    }

    /// [`Workspace::report`], with the diagnostics `extra` adds for each
    /// member analysis (in that analysis's file ids) merged in: the
    /// lockfile checks the driver runs beside the semantic pass
    /// (`k_codegen::project_diagnostics`).
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if an analysis is stale.
    pub fn report_with(
        &self,
        analyses: &[(usize, Analysis)],
        extra: impl Fn(&Analysis) -> Vec<Diagnostic>,
        display: impl Fn(&str) -> String,
    ) -> Result<(SourceMap, Vec<Diagnostic>), Cancelled> {
        let mut map = SourceMap::new();
        for file in Stdlib::get().files() {
            map.add(format!("<stdlib>/{}", file.path), file.text);
        }
        let stdlib_count = u32::try_from(map.len()).unwrap_or(u32::MAX);
        // Every real file, by path.
        let mut texts: BTreeMap<String, String> = BTreeMap::new();
        for (path, text) in &self.loose {
            texts.insert(path.clone(), text.clone());
        }
        for m in &self.members {
            for (path, text) in &m.files {
                texts.insert(m.real_path(path.as_str()), text.clone());
            }
        }
        // Files an analysis knows that aren't on disk (an editor's
        // unsaved documents differ only in text, which the analysis has).
        let mut ids: HashMap<String, FileId> = HashMap::new();
        let mut collected: Vec<Diagnostic> = Vec::new();
        let mut pending: Vec<(usize, &Analysis, Vec<Diagnostic>)> = Vec::new();
        for (member, analysis) in analyses {
            for (_, path) in analysis.files() {
                if let Some(real) = self.real_path(*member, path)
                    && let Some(text) = analysis.file_id(path).and_then(|id| analysis.text(id))
                {
                    texts.insert(real, text.to_owned());
                }
            }
            let mut diagnostics = analysis.diagnostics()?;
            diagnostics.extend(extra(analysis));
            pending.push((*member, analysis, diagnostics));
        }
        for (path, text) in &texts {
            ids.insert(path.clone(), map.add(display(path), text.clone()));
        }
        let placeholder = |map: &mut SourceMap, ids: &mut HashMap<String, FileId>, path: &str| {
            *ids.entry(path.to_owned())
                .or_insert_with(|| map.add(display(path), ""))
        };
        for (member, analysis, diagnostics) in pending {
            let mut remap = |file: FileId| -> FileId {
                if file.raw() < stdlib_count {
                    return file;
                }
                match analysis.origin(file) {
                    Some(FileOrigin::Project(path)) => {
                        let real = self.real_path(member, path).unwrap_or_default();
                        placeholder(&mut map, &mut ids, &real)
                    }
                    _ => file,
                }
            };
            for mut d in diagnostics {
                d.span = Span::new(remap(d.span.file), d.span.range);
                for label in &mut d.labels {
                    label.span = Span::new(remap(label.span.file), label.span.range);
                }
                for fix in &mut d.fixes {
                    for edit in &mut fix.edits {
                        edit.span = Span::new(remap(edit.span.file), edit.span.range);
                    }
                }
                collected.push(d);
            }
        }
        let wanted: BTreeSet<usize> = analyses.iter().map(|(m, _)| *m).collect();
        for p in &self.problems {
            let owner = self
                .owner_of(&p.path)
                .filter(|_| p.path != MANIFEST || !self.is_workspace);
            if owner.is_some_and(|o| !wanted.contains(&o)) {
                continue;
            }
            let file = placeholder(&mut map, &mut ids, &p.path);
            let mut d = Diagnostic::new(p.code, Span::new(file, p.range), p.message.clone());
            for n in &p.notes {
                d = d.with_note(n.clone());
            }
            collected.push(d);
        }
        collected.sort_by(|a, b| (a.span, a.code, &a.message).cmp(&(b.span, b.code, &b.message)));
        collected.dedup();
        Ok((map, collected))
    }
}

/// `true` for the library files a dependent member's analysis mounts: its
/// manifest, its non-test sources and its assets (the fonts its theme
/// ships, which every app using it serves).
fn mounts(path: &ProjectPath) -> bool {
    match path.kind() {
        FileKind::Manifest | FileKind::Asset => true,
        FileKind::Source { is_test, .. } => !is_test,
        _ => false,
    }
}

/// The modules of a member's own non-test sources.
fn modules_of(m: &Member) -> BTreeSet<String> {
    m.files
        .iter()
        .filter_map(|(p, _)| match p.kind() {
            FileKind::Source { module, is_test } if !is_test => Some(module.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

/// The range of member `i`'s direct dependency entry through which it
/// reaches library `lib`; the start of its manifest when none is found.
fn direct_entry(
    members: &[Member],
    edges: &[Vec<(usize, TextRange)>],
    i: usize,
    lib: usize,
) -> TextRange {
    for &(d, range) in &edges[i] {
        if d == lib || members[d].libraries.contains(&lib) {
            return range;
        }
    }
    TextRange::default()
}

/// A problem of code `K8101`.
fn manifest_problem(path: &str, range: TextRange, message: String) -> Problem {
    Problem {
        path: path.to_owned(),
        code: codes::INVALID_MANIFEST,
        range,
        message,
        notes: Vec::new(),
    }
}

/// The note listing the workspace's package names.
fn member_names_note(members: &[Member]) -> Vec<String> {
    let names: Vec<String> = members.iter().map(|m| format!("`{}`", m.name)).collect();
    if names.is_empty() {
        Vec::new()
    } else {
        vec![format!("the members are {}", names.join(", "))]
    }
}

/// A member directory as written in `members`, normalized: `./a/` → `a`.
/// `None` for an absolute path, `..`, or an empty one.
fn normalize_dir(text: &str) -> Option<String> {
    if text.starts_with('/') || text.contains('\\') || text.contains('\0') {
        return None;
    }
    let parts: Vec<&str> = text
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    if parts.is_empty() || parts.contains(&"..") || parts.iter().any(|p| p.starts_with('.')) {
        return None;
    }
    Some(parts.join("/"))
}

/// Appends `v` and the members it depends on (dependencies first).
fn post_order(members: &[Member], v: usize, seen: &mut BTreeSet<usize>, out: &mut Vec<usize>) {
    // An explicit stack: a long chain of members can't overflow.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    if !seen.insert(v) {
        return;
    }
    stack.push((v, 0));
    while let Some((node, next)) = stack.pop() {
        let deps = &members[node].dependencies;
        if let Some(&d) = deps.get(next) {
            stack.push((node, next + 1));
            if seen.insert(d) {
                stack.push((d, 0));
            }
        } else {
            out.push(node);
        }
    }
}

/// The strongly connected components of the graph, by Tarjan's algorithm
/// (iterative).
fn tarjan(edges: &[Vec<(usize, TextRange)>]) -> Vec<Vec<usize>> {
    let n = edges.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut out = Vec::new();
    let mut counter = 0;
    for start in 0..n {
        if index[start] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(start, 0)];
        index[start] = counter;
        low[start] = counter;
        counter += 1;
        stack.push(start);
        on_stack[start] = true;
        while let Some(&mut (v, ref mut next)) = work.last_mut() {
            if let Some(&(w, _)) = edges[v].get(*next) {
                *next += 1;
                if index[w] == usize::MAX {
                    index[w] = counter;
                    low[w] = counter;
                    counter += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                work.pop();
                if let Some(&(parent, _)) = work.last() {
                    low[parent] = low[parent].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut scc = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        scc.push(w);
                        if w == v {
                            break;
                        }
                    }
                    scc.sort_unstable();
                    out.push(scc);
                }
            }
        }
    }
    out
}

/// A path around the cycle through edge `from → to`, by package name:
/// `a → b → a`; the shortest one, found breadth-first inside the cycle's
/// component.
fn cycle_path(
    edges: &[Vec<(usize, TextRange)>],
    scc: &[usize],
    from: usize,
    to: usize,
    members: &[Member],
) -> Vec<String> {
    let mut prev: HashMap<usize, usize> = HashMap::new();
    let mut queue = std::collections::VecDeque::from([to]);
    let mut seen = BTreeSet::from([to]);
    while let Some(v) = queue.pop_front() {
        if v == from {
            break;
        }
        for &(d, _) in &edges[v] {
            if scc.contains(&d) && seen.insert(d) {
                prev.insert(d, v);
                queue.push_back(d);
            }
        }
    }
    // Walk back from `from` to `to`.
    let mut back = vec![from];
    let mut v = from;
    while v != to {
        match prev.get(&v) {
            Some(&p) => {
                back.push(p);
                v = p;
            }
            None => break,
        }
    }
    back.reverse();
    let mut names = vec![members[from].name.clone()];
    names.extend(back.into_iter().map(|i| members[i].name.clone()));
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(files: &[(&str, &str)]) -> Workspace {
        Workspace::from_files(
            files
                .iter()
                .map(|(p, t)| ((*p).to_owned(), (*t).to_owned())),
        )
    }

    fn package(name: &str, deps: &[&str]) -> String {
        let mut s = format!("[package]\nname = \"{name}\"\nk = \"0.1\"\n");
        if !deps.is_empty() {
            s.push_str("\n[dependencies]\n");
            for d in deps {
                s.push_str(&format!("{d} = {{ workspace = true }}\n"));
            }
        }
        s
    }

    fn codes_of(ws: &Workspace) -> Vec<(String, String)> {
        ws.problems
            .iter()
            .map(|p| (p.path.clone(), p.code.to_string()))
            .collect()
    }

    const ROOT: &str = "[workspace]\nmembers = [\"contracts\", \"pay\", \"shop\"]\n";

    #[test]
    fn a_project_outside_a_workspace_is_one_member() {
        let w = ws(&[
            ("k.toml", &package("hello", &[])),
            ("App.k", "public app Hello {}\n"),
            ("src/home/Home.k", ""),
        ]);
        assert!(!w.is_workspace);
        assert_eq!(w.members.len(), 1);
        assert_eq!(w.members[0].dir, "");
        assert_eq!(w.members[0].name, "hello");
        assert!(w.members[0].is_app);
        assert!(w.problems.is_empty(), "{:?}", w.problems);
        assert_eq!(w.analysis_files(0).len(), 3);
    }

    #[test]
    fn members_mount_the_libraries_they_depend_on_transitively() {
        let w = ws(&[
            (
                "k.toml",
                "[workspace]\nmembers = [\"base\", \"contracts\", \"shop\"]\n",
            ),
            ("base/k.toml", &package("base", &[])),
            ("base/src/base/Id.k", "public struct Id {}\n"),
            ("base/src/base/Id.test.k", ""),
            ("contracts/k.toml", &package("contracts", &["base"])),
            ("contracts/src/pay/Payments.k", ""),
            ("contracts/rpc.lock", ""),
            ("shop/k.toml", &package("shop", &["contracts"])),
            ("shop/App.k", "public app Shop {}\n"),
            ("shop/src/cart/Cart.k", ""),
            ("README.md", "loose"),
        ]);
        assert!(w.problems.is_empty(), "{:?}", w.problems);
        let shop = w.member("shop").unwrap();
        let base = w.member("base").unwrap();
        let contracts = w.member("contracts").unwrap();
        assert_eq!(w.members[shop].libraries, vec![base, contracts]);
        let paths: Vec<String> = w
            .analysis_files(shop)
            .into_iter()
            .map(|(p, _)| p.to_string())
            .collect();
        assert_eq!(
            paths,
            [
                "App.k",
                "k.toml",
                "src/cart/Cart.k",
                ".deps/base/k.toml",
                ".deps/base/src/base/Id.k",
                ".deps/contracts/k.toml",
                ".deps/contracts/src/pay/Payments.k",
            ]
        );
        let mounted = ProjectPath::new(".deps/contracts/src/pay/Payments.k").unwrap();
        assert_eq!(
            w.real_path(shop, &mounted).as_deref(),
            Some("contracts/src/pay/Payments.k")
        );
        assert_eq!(
            w.view_path(shop, "contracts/src/pay/Payments.k"),
            Some(mounted)
        );
        assert_eq!(w.view_path(contracts, "shop/App.k"), None);
        assert_eq!(w.order(), vec![base, contracts, shop]);
        // The root manifest and the README.
        assert_eq!(w.loose.len(), 2);
    }

    #[test]
    fn cycles_between_members_are_reported_at_each_edge() {
        let w = ws(&[
            ("k.toml", "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\n"),
            ("a/k.toml", &package("a", &["b"])),
            ("b/k.toml", &package("b", &["c"])),
            ("c/k.toml", &package("c", &["a"])),
        ]);
        assert_eq!(
            codes_of(&w),
            [
                ("a/k.toml".to_owned(), "K8104".to_owned()),
                ("b/k.toml".to_owned(), "K8104".to_owned()),
                ("c/k.toml".to_owned(), "K8104".to_owned()),
            ]
        );
        assert_eq!(w.problems[0].notes[0], "the cycle: a → b → c → a");
        // The cyclic edges are left out, so the graph stays a DAG.
        assert!(w.members.iter().all(|m| m.dependencies.is_empty()));
    }

    #[test]
    fn apps_unknown_names_and_git_tags_are_not_dependencies() {
        let w = ws(&[
            ("k.toml", ROOT),
            ("contracts/k.toml", &package("contracts", &[])),
            ("pay/k.toml", &package("pay", &[])),
            ("pay/App.k", "public app Pay {}\n"),
            (
                "shop/k.toml",
                &(package("shop", &["pay", "nope"]) + "\"github.com/acme/charts\" = \"v1.0.0\"\n"),
            ),
            ("shop/App.k", ""),
        ]);
        assert_eq!(
            codes_of(&w),
            [
                ("shop/k.toml".to_owned(), "K8105".to_owned()),
                ("shop/k.toml".to_owned(), "K8102".to_owned()),
                ("shop/k.toml".to_owned(), "K8102".to_owned()),
            ]
        );
    }

    #[test]
    fn missing_and_invalid_members_are_reported_in_the_root_manifest() {
        let w = ws(&[
            (
                "k.toml",
                "[workspace]\nmembers = [\"a\", \"gone\", \"../x\", \"a\"]\n",
            ),
            ("a/k.toml", &package("a", &[])),
        ]);
        let found: Vec<String> = w.problems.iter().map(|p| p.code.to_string()).collect();
        assert_eq!(found, ["K8103", "K8101", "K8101"]);
        assert_eq!(w.problems[0].range, TextRange::new(28.into(), 34.into()));
    }

    #[test]
    fn malformed_manifests_are_k8101_and_never_panic() {
        for text in [
            "let (((",
            "[package]\nk = \"0.1\"\n",
            "[package]\nname = 3\nk = \"0.1\"\n",
            "[package]\nname = \"a\"\n",
            "[package]\nname = \"a\"\nk = \"0.1\"\n[dependencies]\nx = 3\n",
            "dependencies = 4\n[package]\nname = \"a\"\nk = \"0.1\"\n",
            "[workspace]\nmembers = 3\n",
            "[workspace]\nmembers = [3]\n",
            "[workspace]\n",
            "workspace = 1\n",
            "[workspace]\nmembers = []\n[package]\nname = \"a\"\nk = \"0.1\"\n",
            "",
            "\u{feff}\r\n[package]\r\nname=\"é\"",
        ] {
            let w = ws(&[("k.toml", text)]);
            assert!(
                w.problems.iter().all(|p| p.code == codes::INVALID_MANIFEST),
                "{text:?}: {:?}",
                w.problems
            );
            assert!(!w.problems.is_empty(), "{text:?}");
            for p in &w.problems {
                assert!(u32::from(p.range.end()) as usize <= text.len());
            }
        }
    }

    #[test]
    fn a_module_belongs_to_one_package() {
        let w = ws(&[
            ("k.toml", "[workspace]\nmembers = [\"lib\", \"shop\"]\n"),
            ("lib/k.toml", &package("lib", &[])),
            ("lib/src/pay/A.k", ""),
            ("shop/k.toml", &package("shop", &["lib"])),
            ("shop/App.k", ""),
            ("shop/src/pay/B.k", ""),
        ]);
        assert_eq!(
            codes_of(&w),
            [("shop/k.toml".to_owned(), "K8106".to_owned())]
        );
        assert_eq!(
            w.problems[0].message,
            "module `pay` is declared by both `lib` and `shop`"
        );
    }

    #[test]
    fn the_dev_section_holds_development_values_secrets_and_a_seed() {
        let text = "[package]\nname = \"shop\"\nk = \"0.1\"\n\n[dev]\nseed = \"scripts/demo.sql\"\n\n[dev.env]\nSHOP_ADMIN_EMAIL = \"admin@shop.example\"\nPAYBOX_CALLBACK_URL = \"{origin}\"\nPAYBOX_TIMEOUT = \"10s\"\n\n[dev.secrets]\nSHOP_ADMIN_PASSWORD_HASH = \"password-hash\"\nK_SERVICE_TOKEN = \"token\"\n";
        let (m, problems) = parse_manifest("k.toml", text, true, false);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(m.dev.seed.as_deref(), Some("scripts/demo.sql"));
        assert_eq!(
            m.dev.env.get("PAYBOX_CALLBACK_URL").map(String::as_str),
            Some("{origin}")
        );
        assert_eq!(m.dev.env.len(), 3);
        assert_eq!(
            m.dev.secrets.get("SHOP_ADMIN_PASSWORD_HASH"),
            Some(&DevSecret::PasswordHash)
        );
        assert_eq!(
            m.dev.secrets.get("K_SERVICE_TOKEN"),
            Some(&DevSecret::Token)
        );
    }

    #[test]
    fn a_malformed_dev_section_is_reported() {
        let text = "[package]\nname = \"shop\"\nk = \"0.1\"\n\n[dev]\nseed = 3\nport = 1\n\n[dev.env]\nA = [1]\n\n[dev.secrets]\nB = \"rsa-key\"\n";
        let (_, problems) = parse_manifest("k.toml", text, true, false);
        let messages: Vec<&str> = problems.iter().map(|p| p.message.as_str()).collect();
        assert_eq!(
            messages,
            vec![
                "`[dev] seed` must be the path of a SQL file, such as \"scripts/seed.sql\"",
                "unknown key `port` in `[dev]`; expected `env`, `secrets` or `seed`",
                "`[dev.env] A` must be a string",
                "`[dev.secrets] B` must be \"token\" or \"password-hash\"",
            ]
        );
        let root = "[workspace]\nmembers = [\"a\"]\n\n[dev.env]\nA = \"1\"\n";
        let (_, problems) = parse_manifest("k.toml", root, true, false);
        assert_eq!(
            problems
                .iter()
                .map(|p| p.message.as_str())
                .collect::<Vec<_>>(),
            vec!["a workspace root has no `[dev]` section: put it in each member's k.toml"]
        );
    }

    #[test]
    fn dependency_entries_have_the_range_of_their_line() {
        let text = package("shop", &["contracts"]);
        let (m, problems) = parse_manifest("k.toml", &text, false, true);
        assert!(problems.is_empty());
        let d = &m.dependencies[0];
        let start = u32::from(d.range.start()) as usize;
        let end = u32::from(d.range.end()) as usize;
        assert_eq!(&text[start..end], "contracts = { workspace = true }");
    }
}
