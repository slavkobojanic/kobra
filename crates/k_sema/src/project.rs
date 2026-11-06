//! The project model: paths of project files, what kind of file each one
//! is, and the module a source file belongs to.
//!
//! A K project has a fixed layout (`docs/design/24-projects.md`):
//!
//! ```text
//! k.toml           manifest
//! App.k            the app declaration (apps only)
//! src/             source; each directory is a module (src/weather/Place.k → weather)
//! migrations/      NNNN_description.k; not a module
//! *.lock           lockfiles
//! ```
//!
//! Every file the analysis sees is identified by a [`ProjectPath`] relative
//! to the project root, and [`ProjectPath::kind`] classifies it. The rules
//! are in `docs/compiler/sema.md`, "Project model".

use std::fmt;

/// A path relative to the project root, with `/` separators, such as
/// `src/shop/cart/CartService.k`, `App.k` or `k.toml`.
///
/// # Invariants
///
/// A `ProjectPath` is never empty, never starts or ends with `/`, has no
/// empty, `.` or `..` segments, and contains no `\` and no NUL. Two paths
/// are equal exactly when their text is equal, so a path can key maps.
///
/// # Example
///
/// ```
/// use k_sema::{FileKind, ProjectPath};
///
/// let path = ProjectPath::new("src/shop/cart/CartService.test.k").unwrap();
/// let FileKind::Source { module, is_test } = path.kind() else { panic!() };
/// assert_eq!(module.as_str(), "shop.cart");
/// assert!(is_test);
/// assert!(ProjectPath::new("../x.k").is_err());
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectPath(Box<str>);

/// Why a text is not a valid [`ProjectPath`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathError {
    /// The path is empty.
    Empty,
    /// The path starts or ends with `/`, or has an empty segment (`a//b`).
    EmptySegment,
    /// The path has a `.` or `..` segment.
    RelativeSegment,
    /// The path contains `\` or a NUL character.
    InvalidCharacter,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PathError::Empty => "the path is empty",
            PathError::EmptySegment => "the path has an empty segment",
            PathError::RelativeSegment => "the path has a `.` or `..` segment",
            PathError::InvalidCharacter => "the path contains `\\` or a NUL character",
        })
    }
}

impl std::error::Error for PathError {}

impl ProjectPath {
    /// Creates a project path from `/`-separated text.
    ///
    /// # Errors
    ///
    /// Returns a [`PathError`] if the text violates the invariants above.
    pub fn new(text: &str) -> Result<ProjectPath, PathError> {
        if text.is_empty() {
            return Err(PathError::Empty);
        }
        if text.contains(['\\', '\0']) {
            return Err(PathError::InvalidCharacter);
        }
        for segment in text.split('/') {
            match segment {
                "" => return Err(PathError::EmptySegment),
                "." | ".." => return Err(PathError::RelativeSegment),
                _ => {}
            }
        }
        Ok(ProjectPath(text.into()))
    }

    /// Returns the path as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the last segment, the file name.
    #[must_use]
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// Classifies the file by its place in the fixed project layout.
    ///
    /// * `k.toml` is the [`FileKind::Manifest`];
    /// * `App.k` is [`FileKind::App`];
    /// * `protocol.lock`, `routes.lock`, `schema.lock`, `storage.lock` and
    ///   `k.lock` at the root are [`FileKind::Lockfile`]s;
    /// * `src/<dir>/…/<Name>.k` with at least one directory is a
    ///   [`FileKind::Source`] file of the module named by its directories;
    /// * `migrations/<name>.k` is a [`FileKind::Migration`];
    /// * any other `.k` file (at the root, directly in `src/`, in a nested
    ///   directory of `migrations/`, or whose directories are not valid
    ///   module names) is [`FileKind::Stray`], which semantic analysis
    ///   reports;
    /// * everything else is [`FileKind::Other`] and is ignored.
    #[must_use]
    pub fn kind(&self) -> FileKind {
        let text = self.as_str();
        match text {
            "k.toml" => return FileKind::Manifest,
            "App.k" => return FileKind::App,
            _ => {}
        }
        if let Some(kind) = LockfileKind::from_file_name(text) {
            return FileKind::Lockfile(kind);
        }
        if !text.ends_with(".k") {
            return FileKind::Other;
        }
        let segments: Vec<&str> = text.split('/').collect();
        match segments.as_slice() {
            ["migrations", _] => FileKind::Migration,
            ["src", dirs @ .., _] if !dirs.is_empty() => match ModulePath::from_segments(dirs) {
                Some(module) => FileKind::Source {
                    module,
                    is_test: text.ends_with(".test.k"),
                },
                None => FileKind::Stray,
            },
            _ => FileKind::Stray,
        }
    }
}

impl fmt::Display for ProjectPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ProjectPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &*self.0)
    }
}

/// What a project file is, by its place in the layout.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FileKind {
    /// `k.toml`, the project manifest.
    Manifest,
    /// `App.k`, the app declaration.
    App,
    /// A lockfile at the project root.
    Lockfile(LockfileKind),
    /// A source file under `src/`.
    Source {
        /// The module: the file's directories below `src/`.
        module: ModulePath,
        /// `true` for a `*.test.k` file.
        is_test: bool,
    },
    /// A migration under `migrations/`.
    Migration,
    /// A `.k` file outside the places the layout allows.
    Stray,
    /// Any other file; ignored by the analysis.
    Other,
}

impl FileKind {
    /// Returns `true` for files holding K source: [`FileKind::App`],
    /// [`FileKind::Source`], [`FileKind::Migration`] and
    /// [`FileKind::Stray`]. They are parsed and their syntax diagnostics
    /// reported.
    #[must_use]
    pub fn is_k_source(&self) -> bool {
        matches!(
            self,
            FileKind::App | FileKind::Source { .. } | FileKind::Migration | FileKind::Stray
        )
    }
}

/// The lockfiles of a project (`docs/design/13-tooling.md`, "Lockfiles").
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LockfileKind {
    /// `protocol.lock`: Protobuf names and field numbers.
    Protocol,
    /// `routes.lock`: page URLs and redirects.
    Routes,
    /// `schema.lock`: the database schema and migration order.
    Schema,
    /// `storage.lock`: storage keys and job payloads.
    Storage,
    /// `k.lock`: package versions and hashes.
    Packages,
}

impl LockfileKind {
    /// Every lockfile kind.
    pub const ALL: &'static [LockfileKind] = &[
        LockfileKind::Protocol,
        LockfileKind::Routes,
        LockfileKind::Schema,
        LockfileKind::Storage,
        LockfileKind::Packages,
    ];

    /// Returns the file name at the project root.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            LockfileKind::Protocol => "protocol.lock",
            LockfileKind::Routes => "routes.lock",
            LockfileKind::Schema => "schema.lock",
            LockfileKind::Storage => "storage.lock",
            LockfileKind::Packages => "k.lock",
        }
    }

    /// Returns the kind whose file name is `name`, if any.
    #[must_use]
    pub fn from_file_name(name: &str) -> Option<LockfileKind> {
        LockfileKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.file_name() == name)
    }
}

/// A module path such as `shop.cart` or `k.http`: dot-separated segments,
/// each an ASCII identifier (`[A-Za-z_][A-Za-z0-9_]*`, not a lone `_`).
///
/// A module is a directory (`docs/design/16-modules.md`): `src/shop/cart/`
/// is `shop.cart`. A module with a segment named `testing` is test-only.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModulePath(Box<str>);

impl ModulePath {
    /// Parses a dotted module path.
    ///
    /// Returns `None` if the text is empty or a segment is not an
    /// identifier.
    #[must_use]
    pub fn parse(text: &str) -> Option<ModulePath> {
        let segments: Vec<&str> = text.split('.').collect();
        ModulePath::from_segments(&segments)
    }

    /// Builds a module path from directory names. Returns `None` if there
    /// are none or one is not an identifier.
    #[must_use]
    pub fn from_segments(segments: &[&str]) -> Option<ModulePath> {
        if segments.is_empty() || !segments.iter().all(|s| is_identifier(s)) {
            return None;
        }
        Some(ModulePath(segments.join(".").into()))
    }

    /// Returns the dotted text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('.')
    }

    /// Returns `true` for a test-only module: one with a segment named
    /// `testing`, at any depth.
    #[must_use]
    pub fn is_test_only(&self) -> bool {
        self.segments().any(|s| s == "testing")
    }

    /// Returns `true` for a module of the standard library (`k` or below).
    #[must_use]
    pub fn is_stdlib(&self) -> bool {
        self.segments().next() == Some("k")
    }
}

impl fmt::Display for ModulePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ModulePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &*self.0)
    }
}

/// Returns `true` if `text` is an ASCII identifier as defined by the
/// grammar (`docs/compiler/grammar.md` §3.3), keywords included.
fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && text != "_"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(path: &str) -> FileKind {
        ProjectPath::new(path).unwrap().kind()
    }

    #[test]
    fn paths_reject_invalid_text() {
        assert_eq!(ProjectPath::new(""), Err(PathError::Empty));
        assert_eq!(ProjectPath::new("/a.k"), Err(PathError::EmptySegment));
        assert_eq!(ProjectPath::new("a/"), Err(PathError::EmptySegment));
        assert_eq!(ProjectPath::new("a//b.k"), Err(PathError::EmptySegment));
        assert_eq!(ProjectPath::new("./a.k"), Err(PathError::RelativeSegment));
        assert_eq!(
            ProjectPath::new("a/../b.k"),
            Err(PathError::RelativeSegment)
        );
        assert_eq!(ProjectPath::new("a\\b.k"), Err(PathError::InvalidCharacter));
        assert_eq!(ProjectPath::new("a\0.k"), Err(PathError::InvalidCharacter));
        assert_eq!(ProjectPath::new("src/é/A.k").unwrap().file_name(), "A.k");
    }

    #[test]
    fn files_are_classified_by_the_fixed_layout() {
        assert_eq!(kind("k.toml"), FileKind::Manifest);
        assert_eq!(kind("App.k"), FileKind::App);
        assert_eq!(
            kind("routes.lock"),
            FileKind::Lockfile(LockfileKind::Routes)
        );
        assert_eq!(kind("k.lock"), FileKind::Lockfile(LockfileKind::Packages));
        assert_eq!(kind("migrations/0001_init.k"), FileKind::Migration);
        assert_eq!(kind("migrations/old/0001_init.k"), FileKind::Stray);
        assert_eq!(kind("Main.k"), FileKind::Stray);
        assert_eq!(kind("src/Main.k"), FileKind::Stray);
        assert_eq!(kind("src/my-module/A.k"), FileKind::Stray);
        assert_eq!(kind("lib/x/A.k"), FileKind::Stray);
        assert_eq!(kind("README.md"), FileKind::Other);
        assert_eq!(kind("src/shop/notes.md"), FileKind::Other);
        assert_eq!(kind("src/shop/App.k.bak"), FileKind::Other);
        assert_eq!(
            kind("src/shop/cart/Cart.k"),
            FileKind::Source {
                module: ModulePath::parse("shop.cart").unwrap(),
                is_test: false
            }
        );
        assert!(matches!(
            kind("src/a/A.test.k"),
            FileKind::Source { is_test: true, .. }
        ));
        assert!(kind("App.k").is_k_source());
        assert!(kind("Main.k").is_k_source());
        assert!(!kind("k.toml").is_k_source());
    }

    #[test]
    fn module_paths_validate_segments() {
        assert!(ModulePath::parse("shop.cart").is_some());
        assert!(ModulePath::parse("_x.y1").is_some());
        assert!(ModulePath::parse("").is_none());
        assert!(ModulePath::parse("a..b").is_none());
        assert!(ModulePath::parse("1a").is_none());
        assert!(ModulePath::parse("_").is_none());
        assert!(ModulePath::parse("a-b").is_none());
        let testing = ModulePath::parse("shop.testing.fakes").unwrap();
        assert!(testing.is_test_only());
        assert!(!ModulePath::parse("shop.tests").unwrap().is_test_only());
        assert!(ModulePath::parse("k.http").unwrap().is_stdlib());
        assert!(!ModulePath::parse("kk.http").unwrap().is_stdlib());
        assert_eq!(testing.segments().count(), 3);
    }
}
