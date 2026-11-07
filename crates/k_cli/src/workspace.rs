//! Workspaces for the commands (`docs/design/40-workspaces-and-services.md`):
//! finding the workspace a path belongs to, loading it, and building the
//! analysis of one member with the libraries it depends on mounted
//! ([`k_sema::Workspace`]).
//!
//! A path is **in a workspace** when it is the directory of a `k.toml`
//! with a `[workspace]` table (the root), or lies inside one of the
//! members that table lists. A project outside every workspace is a
//! workspace of one member, so every command can go through
//! [`load_member`].

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use k_sema::{AnalysisHost, Options, Workspace};

use crate::sources;

/// Where a path is relative to its workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    /// The workspace root, as a path from the working directory.
    pub root: PathBuf,
    /// The member directory (relative to the root) the path is in; `None`
    /// for the root itself (every member).
    pub member: Option<String>,
}

/// Finds the workspace `path` belongs to: the nearest ancestor (or `path`
/// itself) whose `k.toml` declares a `[workspace]` listing a member that
/// contains `path`, or that is `path`. `None` outside every workspace.
#[must_use]
pub fn locate(path: &Path) -> Option<Located> {
    let start = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    };
    let absolute = fs::canonicalize(&start).ok()?;
    let mut relative = start.clone();
    let mut below: Vec<String> = Vec::new();
    for dir in absolute.ancestors() {
        let manifest = dir.join("k.toml");
        if let Ok(text) = fs::read_to_string(&manifest) {
            if Workspace::is_workspace_manifest(&text) {
                let root = normalize(&relative);
                let rest: Vec<String> = below.iter().rev().cloned().collect();
                if rest.is_empty() {
                    return Some(Located { root, member: None });
                }
                let rel = rest.join("/");
                let (manifest, _) = k_sema::workspace::parse_manifest("k.toml", &text, true, false);
                let member = manifest
                    .members
                    .iter()
                    .map(|(m, _)| m.trim_matches('/').trim_start_matches("./").to_owned())
                    .filter(|m| rel == *m || rel.starts_with(&format!("{m}/")))
                    .max_by_key(String::len)?;
                return Some(Located {
                    root,
                    member: Some(member),
                });
            }
        }
        if let Some(name) = dir.file_name() {
            below.push(name.to_string_lossy().into_owned());
        }
        relative = relative.join("..");
    }
    None
}

/// Removes `.` components and folds `name/..` pairs, lexically.
fn normalize(path: &Path) -> PathBuf {
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

/// A loaded workspace.
pub struct Loaded {
    /// The root, as a path from the working directory.
    pub root: PathBuf,
    /// The plan.
    pub workspace: Workspace,
    /// The member the command was given, `None` for the root.
    pub focus: Option<usize>,
}

impl Loaded {
    /// The members the command applies to: the focus, or every member in
    /// dependency order.
    #[must_use]
    pub fn targets(&self) -> Vec<usize> {
        match self.focus {
            Some(m) => vec![m],
            None => self.workspace.order(),
        }
    }

    /// The directory of member `m`.
    #[must_use]
    pub fn member_dir(&self, m: usize) -> PathBuf {
        let dir = &self.workspace.members[m].dir;
        if dir.is_empty() {
            self.root.clone()
        } else {
            self.root.join(dir)
        }
    }

    /// The analysis of member `m`: its files and its libraries' sources.
    #[must_use]
    pub fn host(&self, m: usize) -> AnalysisHost {
        AnalysisHost::with_files(Options::new(), self.workspace.analysis_files(m))
    }
}

/// Loads the workspace of `path` (a project, a member or a workspace root):
/// every file below the root, the plan, and which member `path` is.
///
/// Prints the problem and returns exit status 2 when `path` is not inside a
/// project, and 1 when some file can't be read.
pub fn load(path: &Path) -> Result<Loaded, ExitCode> {
    let (root, member) = match locate(path) {
        Some(Located { root, member }) => (root, member),
        None => {
            if !path.join("k.toml").is_file() {
                eprintln!(
                    "error: `{}` is not a project: it has no k.toml",
                    sources::display(path)
                );
                return Err(ExitCode::from(2));
            }
            (path.to_path_buf(), Some(String::new()))
        }
    };
    let loaded = sources::load_tree(&root);
    if !loaded.problems.is_empty() {
        eprintln!("error: some files can't be read; run `k check` for details");
        return Err(ExitCode::from(1));
    }
    let workspace = Workspace::from_files(
        loaded
            .files
            .into_iter()
            .map(|(p, t)| (p.as_str().to_owned(), t)),
    );
    let focus = match member {
        None => None,
        Some(dir) => match workspace.member(&dir) {
            Some(m) => Some(m),
            None => {
                eprintln!(
                    "error: `{}` is not a project: it has no k.toml",
                    sources::display(path)
                );
                return Err(ExitCode::from(2));
            }
        },
    };
    Ok(Loaded {
        root,
        workspace,
        focus,
    })
}

/// [`load`] for commands that work on one project: refuses a workspace root
/// (exit status 2), naming its members.
pub fn load_member(path: &Path, command: &str) -> Result<(Loaded, usize), ExitCode> {
    let loaded = load(path)?;
    match loaded.focus {
        Some(m) => Ok((loaded, m)),
        None => {
            let members: Vec<String> = loaded
                .workspace
                .members
                .iter()
                .map(|m| format!("`{}`", sources::display(&loaded.root.join(&m.dir))))
                .collect();
            eprintln!(
                "error: `{}` is a workspace root; run `k {command}` in one of its members: {}",
                sources::display(path),
                members.join(", ")
            );
            Err(ExitCode::from(2))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_folds_parent_components() {
        assert_eq!(normalize(Path::new("a/../b/./c")), PathBuf::from("b/c"));
        assert_eq!(normalize(Path::new("./")), PathBuf::from("."));
        assert_eq!(normalize(Path::new("../x")), PathBuf::from("../x"));
    }
}
