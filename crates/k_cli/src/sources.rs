//! Finding the projects to check and loading their files.
//!
//! A **project** is a directory with a `k.toml` (`docs/design/24-projects.md`).
//! `k check <paths>` maps every path to a project:
//!
//! * a path inside a project (the directory with `k.toml` itself, or any
//!   file or directory below it) checks **the whole project**, because a
//!   file can't be analysed without its module graph;
//! * a directory that is not inside a project is searched for projects
//!   (each subdirectory with a `k.toml` is one), and its remaining `.k`
//!   files form one ad-hoc group rooted at that directory;
//! * a `.k` file that is not inside a project is checked alone, rooted at
//!   its directory.
//!
//! A path in a **workspace** (`docs/design/40-workspaces-and-services.md`,
//! [`crate::workspace::locate`]) checks that workspace instead: its root
//! checks every member, a path inside a member checks that member, each
//! with the libraries it depends on. Such a group is rooted at the
//! workspace root and holds every file below it ([`Scope::Workspace`]).
//!
//! Files of a project are the ones the analysis reads
//! ([`k_sema::FileKind`] other than `Other`): `k.toml`, `App.k`, lockfiles
//! and `.k` files. Searches skip hidden directories and [`SKIPPED_DIRS`].
//! Everything is sorted, so output is deterministic.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use k_sema::{FileKind, ProjectPath};

/// Directory names that are never searched for sources.
const SKIPPED_DIRS: &[&str] = &["target", "dist", "node_modules"];

/// The manifest that marks a project root.
const MANIFEST: &str = "k.toml";

/// Whether a directory named `name` is build output or vendored code that
/// is never searched for sources ([`SKIPPED_DIRS`]).
pub fn is_skipped_dir(name: &str) -> bool {
    SKIPPED_DIRS.contains(&name)
}

/// A problem loading a file, reported by the driver as a diagnostic.
#[derive(Clone)]
pub enum Problem {
    /// The file could not be read (K9002).
    Unreadable(String),
    /// The file is not valid UTF-8 (K9001): its lossy text and the offset of
    /// the first invalid byte.
    InvalidUtf8 {
        /// The text with invalid sequences replaced.
        text: String,
        /// The byte offset of the first invalid byte.
        offset: usize,
    },
    /// The file's path can't be a project path (K9002).
    InvalidPath(String),
}

/// What a group is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// One project, or loose files.
    Project,
    /// A workspace, rooted at its root: the members to check, by
    /// directory, or `None` for every member.
    Workspace(Option<BTreeSet<String>>),
}

/// One group of files analysed together.
pub struct Group {
    /// The directory the project paths are relative to.
    pub root: PathBuf,
    /// A project or a workspace.
    pub scope: Scope,
    /// The loaded files, sorted by path.
    pub files: Vec<(ProjectPath, String)>,
    /// Files that could not be loaded, by display path.
    pub problems: Vec<(String, Problem)>,
}

/// Everything `k check` analyses.
pub struct Sources {
    /// The groups, sorted by root.
    pub groups: Vec<Group>,
    /// Paths that don't exist, by display path.
    pub missing: Vec<String>,
}

/// What to load for one root.
#[derive(Default)]
struct Plan {
    /// Load every file below the root.
    whole: bool,
    /// Individual files to load.
    files: BTreeSet<PathBuf>,
    /// For a workspace root: `Some(None)` checks every member,
    /// `Some(Some(dirs))` the members named.
    workspace: Option<Option<BTreeSet<String>>>,
}

/// Plans the project at `root`: its workspace when it is in one.
fn plan_project(root: PathBuf, plans: &mut BTreeMap<PathBuf, Plan>) {
    match crate::workspace::locate(&root) {
        Some(found) => {
            let plan = plans.entry(found.root).or_default();
            plan.whole = true;
            match (found.member, &mut plan.workspace) {
                (None, slot) => *slot = Some(None),
                (Some(_), Some(None)) => {}
                (Some(m), Some(Some(set))) => {
                    set.insert(m);
                }
                (Some(m), slot @ None) => *slot = Some(Some(BTreeSet::from([m]))),
            }
        }
        None => plans.entry(root).or_default().whole = true,
    }
}

/// Finds and loads the groups for `paths`.
pub fn load(paths: &[PathBuf]) -> Sources {
    let mut plans: BTreeMap<PathBuf, Plan> = BTreeMap::new();
    let mut missing = Vec::new();
    for path in paths {
        if !path.exists() {
            missing.push(display(path));
            continue;
        }
        let start = if path.is_dir() {
            path.clone()
        } else {
            parent(path)
        };
        if let Some(found) = crate::workspace::locate(&start) {
            plan_project(
                found.root.join(found.member.unwrap_or_default()),
                &mut plans,
            );
        } else if let Some(root) = project_root(&start) {
            plan_project(root, &mut plans);
        } else if path.is_dir() {
            search(path, &mut plans);
        } else {
            plans.entry(start).or_default().files.insert(path.clone());
        }
    }
    missing.sort();
    missing.dedup();
    let groups = plans
        .into_iter()
        .map(|(root, plan)| load_group(root, plan))
        .collect();
    Sources { groups, missing }
}

/// Returns the nearest directory at or above `start` that has a `k.toml`.
pub fn project_root(start: &Path) -> Option<PathBuf> {
    let absolute = fs::canonicalize(start).ok()?;
    let mut relative = start.to_path_buf();
    for dir in absolute.ancestors() {
        if dir.join(MANIFEST).is_file() {
            return Some(normalize(&relative));
        }
        relative = relative.join("..");
    }
    None
}

/// Removes `.` components and folds `name/..` pairs, lexically, so roots
/// display as `../app`, not `src/../../app`.
fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if matches!(parts.last(), Some(Component::Normal(_))) => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        PathBuf::from(".")
    } else {
        parts.iter().collect()
    }
}

/// Plans a directory outside any project: its projects, and an ad-hoc group
/// for its other `.k` files.
fn search(dir: &Path, plans: &mut BTreeMap<PathBuf, Plan>) {
    let mut loose = Vec::new();
    let mut projects = Vec::new();
    walk(dir, &mut loose, &mut projects, true);
    for project in projects {
        plan_project(project, plans);
    }
    let plan = plans.entry(dir.to_path_buf()).or_default();
    plan.files.extend(
        loose
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "k")),
    );
}

/// Collects the files below `dir`, skipping hidden and build directories.
/// With `split`, subdirectories holding a `k.toml` are collected as projects
/// instead of descended into.
fn walk(dir: &Path, out: &mut Vec<PathBuf>, projects: &mut Vec<PathBuf>, split: bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name.starts_with('.') || SKIPPED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            if split && path.join(MANIFEST).is_file() {
                projects.push(path);
            } else {
                walk(&path, out, projects, split);
            }
        } else {
            out.push(path);
        }
    }
}

/// Every file below `root` that the analysis reads, nested projects
/// included: the files of a workspace.
pub fn load_tree(root: &Path) -> Group {
    load_group(
        root.to_path_buf(),
        Plan {
            whole: true,
            files: BTreeSet::new(),
            workspace: Some(None),
        },
    )
}

/// Reads the files `plan` names below `root` into a [`Group`]: for a
/// project, the files a project path classifies as manifest, lockfile or
/// source; for a workspace, every manifest, lockfile and `.k` file (its
/// members classify them later). Unreadable files, files that aren't
/// UTF-8 and invalid project paths become [`Group::problems`].
fn load_group(root: PathBuf, plan: Plan) -> Group {
    let mut paths: BTreeSet<PathBuf> = plan.files;
    if plan.whole {
        let mut all = Vec::new();
        // A project's files stop at nested projects, which are checked on
        // their own when named; a workspace holds its members' files.
        let mut nested = Vec::new();
        walk(&root, &mut all, &mut nested, plan.workspace.is_none());
        paths.extend(all);
    }
    let mut group = Group {
        root,
        scope: match plan.workspace {
            Some(members) => Scope::Workspace(members),
            None => Scope::Project,
        },
        files: Vec::new(),
        problems: Vec::new(),
    };
    for path in paths {
        let relative = relative(&group.root, &path);
        let Ok(project_path) = ProjectPath::new(&relative) else {
            group.problems.push((
                display(&path),
                Problem::InvalidPath(format!("`{relative}` is not a valid project path")),
            ));
            continue;
        };
        // A workspace's files are classified relative to their member,
        // later; here, keep every manifest, lockfile and K source.
        let wanted = if group.scope == Scope::Project {
            project_path.kind() != FileKind::Other
        } else {
            let name = project_path.file_name();
            name == MANIFEST
                || k_sema::LockfileKind::from_file_name(name).is_some()
                || name.ends_with(".k")
        };
        if !wanted {
            continue;
        }
        match fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => group.files.push((project_path, text)),
                Err(error) => {
                    let offset = error.utf8_error().valid_up_to();
                    let text = String::from_utf8_lossy(error.as_bytes()).into_owned();
                    group
                        .problems
                        .push((display(&path), Problem::InvalidUtf8 { text, offset }));
                }
            },
            Err(error) => group
                .problems
                .push((display(&path), Problem::Unreadable(error.to_string()))),
        }
    }
    group
}

/// The directory holding `path`, `.` for a bare file name.
fn parent(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// The path of `path` relative to `root`, with `/` separators.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The path as shown in diagnostics: without a leading `./`.
pub fn display(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix("./").unwrap_or(&text).to_owned()
}
