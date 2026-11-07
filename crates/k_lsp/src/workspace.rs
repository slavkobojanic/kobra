//! Projects, their files and the open documents that override them.
//!
//! A **project** is a directory with a `k.toml` (`docs/design/24-projects.md`);
//! each has its own [`AnalysisHost`], fed every file the analysis reads
//! ([`FileKind`] other than `Other`), as `k check` does. The workspace
//! folders the client opens are searched for projects; a document opened
//! outside every known project joins the project of its nearest ancestor
//! with a `k.toml`, or else a *loose* project rooted at its directory that
//! holds only open documents.
//!
//! **Documents override the disk.** While a document is open its text is
//! the file's text in the analysis; closing it reverts to the disk. Files
//! changed on disk (`workspace/didChangeWatchedFiles`) are reloaded unless
//! open.
//!
//! The standard library is embedded in the compiler and has no files of its
//! own; for go-to-definition into it, [`stdlib_path`] writes its sources
//! once per process to a versioned directory below the system's temporary
//! directory, read-only by convention.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use k_diagnostics::FileId;
use k_sema::stdlib::Stdlib;
use k_sema::{Analysis, AnalysisHost, Change, FileKind, FileOrigin, Options, ProjectPath};

/// Directory names never searched for sources or projects.
const SKIPPED_DIRS: &[&str] = &["target", "dist", "node_modules"];

/// How deep below a workspace folder projects are searched for.
const SEARCH_DEPTH: usize = 4;

/// The manifest marking a project root.
const MANIFEST: &str = "k.toml";

/// One project and its analysis.
#[derive(Debug)]
pub struct Project {
    /// The directory paths are relative to.
    pub root: PathBuf,
    /// `true` for a project without a `k.toml`, holding only open documents.
    pub loose: bool,
    /// The analysis inputs.
    pub host: AnalysisHost,
    /// The open documents, by path: their version and text.
    open: HashMap<ProjectPath, (i32, String)>,
    /// Counts the changes applied, so a result computed for an older state
    /// can be recognized and dropped.
    pub generation: u64,
}

impl Project {
    /// Creates a project rooted at `root`, loading its files from disk
    /// unless it is `loose`.
    fn load(root: PathBuf, loose: bool, options: &Options) -> Project {
        let mut host = AnalysisHost::new(options.clone());
        if !loose {
            let mut files = Vec::new();
            collect(&root, &root, &mut files);
            let mut change = Change::new();
            for (path, text) in files {
                change.set_file(path, text);
            }
            host.apply_change(change);
        }
        Project {
            root,
            loose,
            host,
            open: HashMap::new(),
            generation: 0,
        }
    }

    /// A snapshot of the project's current state.
    #[must_use]
    pub fn analysis(&self) -> Analysis {
        self.host.analysis()
    }

    /// The text of an open document.
    #[must_use]
    pub fn document(&self, path: &ProjectPath) -> Option<&str> {
        self.open.get(path).map(|(_, t)| t.as_str())
    }

    /// The version of an open document.
    #[must_use]
    pub fn version(&self, path: &ProjectPath) -> Option<i32> {
        self.open.get(path).map(|(v, _)| *v)
    }

    /// Sets the text of `path` in the analysis.
    fn set(&mut self, path: &ProjectPath, text: &str) {
        let mut change = Change::new();
        change.set_file(path.clone(), text);
        self.host.apply_change(change);
        self.generation += 1;
    }

    /// Opens (or reopens) a document.
    pub fn open(&mut self, path: ProjectPath, version: i32, text: String) {
        self.set(&path, &text);
        self.open.insert(path, (version, text));
    }

    /// Replaces an open document's text (opening it if needed).
    pub fn update(&mut self, path: ProjectPath, version: i32, text: String) {
        self.open(path, version, text);
    }

    /// Closes a document: the analysis reads the file from disk again, or
    /// forgets it if it doesn't exist there.
    pub fn close(&mut self, path: &ProjectPath) {
        if self.open.remove(path).is_some() {
            self.reload(path);
        }
    }

    /// Reloads `path` from disk unless it is open: a file that no longer
    /// exists, or that isn't an analysis input, is removed.
    pub fn reload(&mut self, path: &ProjectPath) {
        if self.open.contains_key(path) {
            return;
        }
        let disk = self.root.join(path.as_str());
        let mut change = Change::new();
        match fs::read(&disk) {
            Ok(bytes) if !self.loose && path.kind() != FileKind::Other => {
                change.set_file(path.clone(), String::from_utf8_lossy(&bytes).into_owned());
            }
            _ => change.remove_file(path.clone()),
        }
        self.host.apply_change(change);
        self.generation += 1;
    }

    /// The path on disk of a file of this project's analysis: the project
    /// file below the root, or the materialized stdlib file.
    #[must_use]
    pub fn file_path(root: &Path, analysis: &Analysis, file: FileId) -> Option<PathBuf> {
        match analysis.origin(file)? {
            FileOrigin::Project(path) => Some(root.join(path.as_str())),
            FileOrigin::Stdlib(path) => stdlib_path(path),
        }
    }
}

/// Every project of the session.
#[derive(Debug, Default)]
pub struct Workspace {
    /// The projects, in the order they were found.
    pub projects: Vec<Project>,
    /// The analysis options of every project.
    options: Options,
}

impl Workspace {
    /// Creates a workspace from the client's folders: each folder inside a
    /// project opens that project; any other folder is searched for
    /// projects.
    #[must_use]
    pub fn new(folders: &[PathBuf], options: Options) -> Workspace {
        let mut workspace = Workspace {
            projects: Vec::new(),
            options,
        };
        for folder in folders {
            if let Some(root) = project_root(folder) {
                workspace.add(root, false);
                continue;
            }
            let mut roots = Vec::new();
            find_projects(folder, SEARCH_DEPTH, &mut roots);
            roots.sort();
            for root in roots {
                workspace.add(root, false);
            }
        }
        workspace
    }

    /// Adds the project at `root` unless it is already there; returns its
    /// index.
    fn add(&mut self, root: PathBuf, loose: bool) -> usize {
        if let Some(i) = self.projects.iter().position(|p| p.root == root) {
            return i;
        }
        self.projects
            .push(Project::load(root, loose, &self.options));
        self.projects.len() - 1
    }

    /// The project and project path of the file at `path`: the innermost
    /// known project containing it; else the project of its nearest
    /// ancestor with a `k.toml`, loaded now; else a loose project for its
    /// directory. `None` for the materialized stdlib and for paths that
    /// can't be project paths.
    pub fn locate(&mut self, path: &Path) -> Option<(usize, ProjectPath)> {
        if is_stdlib_file(path) {
            return None;
        }
        let known = self
            .projects
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                path.starts_with(&p.root) && (!p.loose || path.parent() == Some(&p.root))
            })
            .max_by_key(|(_, p)| (!p.loose, p.root.components().count()))
            .map(|(i, _)| i);
        let index = match known {
            Some(i) => i,
            None => match path.parent().and_then(project_root) {
                Some(root) => self.add(root, false),
                None => self.add(path.parent()?.to_path_buf(), true),
            },
        };
        let rel = path.strip_prefix(&self.projects[index].root).ok()?;
        let text = rel.to_str()?.replace('\\', "/");
        let project_path = ProjectPath::new(&text).ok()?;
        Some((index, project_path))
    }

    /// The project at `index`.
    #[must_use]
    pub fn project(&self, index: usize) -> Option<&Project> {
        self.projects.get(index)
    }

    /// The project at `index`, mutably.
    pub fn project_mut(&mut self, index: usize) -> Option<&mut Project> {
        self.projects.get_mut(index)
    }
}

/// The nearest directory at or above `dir` with a `k.toml`.
#[must_use]
pub fn project_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|d| d.join(MANIFEST).is_file())
        .map(Path::to_path_buf)
}

/// Whether a directory is searched (not hidden, not build output).
fn searchable(name: &str) -> bool {
    !name.starts_with('.') && !SKIPPED_DIRS.contains(&name)
}

/// Finds project roots below `dir`, at most `depth` levels deep; a
/// project's own subdirectories are not searched.
fn find_projects(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if dir.join(MANIFEST).is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_ok_and(|t| t.is_dir()) && searchable(&name) {
            find_projects(&entry.path(), depth - 1, out);
        }
    }
}

/// Collects the analysis inputs below `dir`, as paths relative to `root`.
/// Unreadable files and paths that can't be project paths are skipped (the
/// editor shows what it can; `k check` reports them).
fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<fs::DirEntry> = entries.flatten().collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if searchable(&name) {
                collect(&path, root, out);
            }
            continue;
        }
        let Some(rel) = path.strip_prefix(root).ok().and_then(Path::to_str) else {
            continue;
        };
        let Ok(project_path) = ProjectPath::new(&rel.replace('\\', "/")) else {
            continue;
        };
        if project_path.kind() == FileKind::Other {
            continue;
        }
        if let Ok(bytes) = fs::read(&path) {
            out.push((project_path, String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
}

/// The directory the stdlib sources are materialized in, created (with
/// every file) on first use; `None` if it can't be written.
fn stdlib_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let files = Stdlib::get().files();
        // The directory name changes with the contents, so a stale copy of
        // another version is never shown.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for f in files {
            for b in f.path.bytes().chain(f.text.bytes()) {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        let dir = std::env::temp_dir().join(format!(
            "k-stdlib-{}-{hash:016x}",
            env!("CARGO_PKG_VERSION")
        ));
        for f in files {
            let path = dir.join(f.path);
            if fs::read_to_string(&path).is_ok_and(|t| t == f.text) {
                continue;
            }
            let parent = path.parent()?;
            fs::create_dir_all(parent).ok()?;
            fs::write(&path, f.text).ok()?;
        }
        Some(dir)
    })
    .as_deref()
}

/// The materialized path of stdlib file `path` (`k/core/List.k`).
#[must_use]
pub fn stdlib_path(path: &str) -> Option<PathBuf> {
    stdlib_dir().map(|d| d.join(path))
}

/// Whether `path` is inside the materialized stdlib (without creating it).
fn is_stdlib_file(path: &Path) -> bool {
    let tmp = std::env::temp_dir();
    path.strip_prefix(&tmp)
        .ok()
        .and_then(|rest| rest.components().next())
        .is_some_and(|c| c.as_os_str().to_string_lossy().starts_with("k-stdlib-"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("k-lsp-ws-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn folders_find_projects_and_documents_their_project() {
        let dir = temp("find");
        let app = dir.join("apps/shop");
        write(
            &app.join("k.toml"),
            "[package]\nname = \"shop\"\nk = \"0.1\"\n",
        );
        write(&app.join("src/cart/Cart.k"), "public struct Cart {\n}\n");
        write(&dir.join("node_modules/x/k.toml"), "");
        let mut ws = Workspace::new(std::slice::from_ref(&dir), Options::new());
        assert_eq!(ws.projects.len(), 1);
        assert_eq!(ws.projects[0].root, app);
        assert_eq!(ws.projects[0].analysis().files().len(), 2);
        let (index, path) = ws.locate(&app.join("src/cart/Cart.k")).unwrap();
        assert_eq!(index, 0);
        assert_eq!(path.as_str(), "src/cart/Cart.k");
        // A file outside every project gets a loose project.
        let (loose, _) = ws.locate(&dir.join("scratch/Note.k")).unwrap();
        assert_eq!(loose, 1);
        assert!(ws.projects[1].loose);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn documents_override_the_disk_until_closed() {
        let dir = temp("docs");
        write(
            &dir.join("k.toml"),
            "[package]\nname = \"a\"\nk = \"0.1\"\n",
        );
        write(&dir.join("src/a/A.k"), "let a = 1\n");
        let mut ws = Workspace::new(std::slice::from_ref(&dir), Options::new());
        let (i, path) = ws.locate(&dir.join("src/a/A.k")).unwrap();
        let project = ws.project_mut(i).unwrap();
        project.open(path.clone(), 1, "let a = (\n".to_owned());
        let analysis = project.analysis();
        let id = analysis.file_id(&path).unwrap();
        assert_eq!(analysis.text(id), Some("let a = (\n"));
        assert_eq!(project.version(&path), Some(1));
        project.close(&path);
        assert_eq!(project.analysis().text(id), Some("let a = 1\n"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_stdlib_is_materialized() {
        let first = &Stdlib::get().files()[0];
        let path = stdlib_path(first.path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), first.text);
        assert!(is_stdlib_file(&path));
        let mut ws = Workspace::default();
        assert!(ws.locate(&path).is_none());
    }
}
