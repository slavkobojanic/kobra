//! Finding corpus cases and misplaced files.

use std::fs;
use std::path::{Path, PathBuf};

use crate::suites::Suite;

/// The name of the file that marks a directory of the `sema` suite as one
/// multi-file project case.
pub const PROJECT_MARKER: &str = "k.toml";

/// The stem of a project case's expectation files, which live inside the
/// project directory: `expected.diag`, `expected.pending`.
pub const PROJECT_EXPECTATION_STEM: &str = "expected";

/// One corpus case: a `.k` source file in a suite, or (in the `sema`
/// suite) a project directory containing a [`PROJECT_MARKER`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Case {
    /// The suite the case belongs to.
    pub suite: Suite,
    /// The path of the `.k` file, or of the project directory.
    pub source: PathBuf,
    /// The test name: the path relative to the corpus root, with `/`
    /// separators, for example `lexer/strings/escapes.k`. A project's name
    /// ends with `/` (`sema/modules/cycle/`).
    pub name: String,
    /// `true` for a multi-file project case.
    pub project: bool,
}

impl Case {
    /// Returns the path of the expectation file with extension `extension`
    /// (without the dot): next to a `.k` case with the same stem, or
    /// `expected.<extension>` inside a project directory.
    #[must_use]
    pub fn expectation(&self, extension: &str) -> PathBuf {
        if self.project {
            self.source
                .join(format!("{PROJECT_EXPECTATION_STEM}.{extension}"))
        } else {
            self.source.with_extension(extension)
        }
    }
}

/// A problem with the corpus layout, reported as a failing test so that it
/// cannot go unnoticed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Problem {
    /// The test name under which the problem is reported.
    pub name: String,
    /// What is wrong and how to fix it.
    pub message: String,
}

/// Everything found under the corpus root.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Discovery {
    /// The cases, sorted by name.
    pub cases: Vec<Case>,
    /// Layout problems, sorted by name.
    pub problems: Vec<Problem>,
}

/// Scans `root` (normally [`crate::corpus_root`]).
///
/// Each direct subdirectory of `root` must be a known [`Suite`]. In the
/// `sema` suite, a directory that contains a [`PROJECT_MARKER`] is one
/// project case, and every file inside it belongs to the project (except
/// its `expected.*` expectation files, which must use the suite's
/// extensions). Elsewhere below a suite, every file must be one of:
///
/// * a `.k` case;
/// * an expectation file whose extension the suite uses
///   ([`Suite::expectation_extensions`]) and whose `.k` sibling exists;
/// * a Markdown file (`.md`), for notes about a group of cases.
///
/// Anything else, including a file directly in `root` other than Markdown,
/// is reported as a [`Problem`]. A missing `root` yields an empty discovery.
#[must_use]
pub fn discover(root: &Path) -> Discovery {
    let mut discovery = Discovery::default();
    let Ok(entries) = fs::read_dir(root) else {
        return discovery;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = relative_name(root, &path);
        if path.is_dir() {
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            match Suite::from_dir_name(dir_name) {
                Some(suite) => scan_suite(root, &path, suite, &mut discovery),
                None => discovery.problems.push(Problem {
                    name,
                    message: format!(
                        "`{dir_name}` is not a known corpus suite; known suites: {}",
                        Suite::ALL
                            .iter()
                            .map(|s| s.dir_name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }),
            }
        } else if !is_markdown(&path) {
            discovery.problems.push(Problem {
                name,
                message: "files directly in tests/corpus must be inside a suite directory".into(),
            });
        }
    }
    discovery.cases.sort_by(|a, b| a.name.cmp(&b.name));
    discovery.problems.sort_by(|a, b| a.name.cmp(&b.name));
    discovery
}

fn scan_suite(root: &Path, dir: &Path, suite: Suite, discovery: &mut Discovery) {
    let mut files = Vec::new();
    let mut projects = Vec::new();
    collect_files(dir, suite.has_projects(), &mut files, &mut projects);
    files.sort();
    for project in projects {
        scan_project(root, &project, suite, discovery);
    }
    for path in files {
        let name = relative_name(root, &path);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if extension == "k" {
            discovery.cases.push(Case {
                suite,
                source: path,
                name,
                project: false,
            });
        } else if suite.expectation_extensions().contains(&extension) {
            if !path.with_extension("k").is_file() {
                discovery.problems.push(Problem {
                    name,
                    message: format!(
                        "orphaned expectation file: `{}` does not exist",
                        relative_name(root, &path.with_extension("k"))
                    ),
                });
            }
        } else if !is_markdown(&path) {
            discovery.problems.push(Problem {
                name,
                message: format!(
                    "unexpected file in the `{}` suite; allowed: .k cases, {} expectations and .md notes",
                    suite.dir_name(),
                    suite
                        .expectation_extensions()
                        .iter()
                        .map(|e| format!(".{e}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    }
}

/// Registers a project case and checks its `expected.*` files.
fn scan_project(root: &Path, dir: &Path, suite: Suite, discovery: &mut Discovery) {
    let name = format!("{}/", relative_name(root, dir));
    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let is_expectation = path
                .file_stem()
                .is_some_and(|s| s == PROJECT_EXPECTATION_STEM)
                && path.is_file();
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            if is_expectation && !suite.expectation_extensions().contains(&extension) {
                discovery.problems.push(Problem {
                    name: relative_name(root, &path),
                    message: format!(
                        "unexpected expectation file in a project; allowed: {}",
                        suite
                            .expectation_extensions()
                            .iter()
                            .map(|e| format!("{PROJECT_EXPECTATION_STEM}.{e}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
        }
    }
    discovery.cases.push(Case {
        suite,
        source: dir.to_path_buf(),
        name,
        project: true,
    });
}

/// Collects the files below `dir`. With `projects`, a directory holding a
/// [`PROJECT_MARKER`] is collected as a project instead of descended into.
fn collect_files(dir: &Path, projects: bool, out: &mut Vec<PathBuf>, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if projects && path.join(PROJECT_MARKER).is_file() {
                found.push(path);
            } else {
                collect_files(&path, projects, out, found);
            }
        } else {
            out.push(path);
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "md")
}

fn relative_name(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let path = std::env::temp_dir().join(format!("k_corpus_{tag}_{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn write(&self, relative: &str, text: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_cases_and_reports_problems() {
        let dir = TempDir::new("discover");
        dir.write("README.md", "notes");
        dir.write("stray.txt", "");
        dir.write("lexer/strings/escapes.k", "");
        dir.write("lexer/strings/escapes.tokens", "");
        dir.write("lexer/strings/escapes.diag", "");
        dir.write("lexer/strings/notes.md", "");
        dir.write("lexer/orphan.tokens", "");
        dir.write("lexer/wrong.ast", "");
        dir.write("parser/a.k", "");
        dir.write("bogus/x.k", "");
        let found = discover(&dir.0);
        let names: Vec<_> = found.cases.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["lexer/strings/escapes.k", "parser/a.k"]);
        assert_eq!(found.cases[0].suite, Suite::Lexer);
        assert!(
            found.cases[0]
                .expectation("tokens")
                .ends_with("escapes.tokens")
        );
        let problems: Vec<_> = found.problems.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            problems,
            [
                "bogus",
                "lexer/orphan.tokens",
                "lexer/wrong.ast",
                "stray.txt"
            ]
        );
    }

    #[test]
    fn sema_projects_are_one_case_each() {
        let dir = TempDir::new("discover_projects");
        dir.write("sema/single/rule.k", "");
        dir.write("sema/single/rule.diag", "");
        dir.write("sema/single/rule.pending", "not yet");
        dir.write("sema/modules/cycle/k.toml", "");
        dir.write("sema/modules/cycle/App.k", "");
        dir.write("sema/modules/cycle/src/a/A.k", "");
        dir.write("sema/modules/cycle/src/a/notes.txt", "");
        dir.write("sema/modules/cycle/expected.diag", "");
        dir.write("sema/modules/cycle/expected.pending", "later");
        dir.write("sema/modules/bad/k.toml", "");
        dir.write("sema/modules/bad/expected.ast", "");
        // A k.toml outside the sema suite is just an unexpected file.
        dir.write("parser/x/k.toml", "");
        let found = discover(&dir.0);
        let names: Vec<_> = found.cases.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "sema/modules/bad/",
                "sema/modules/cycle/",
                "sema/single/rule.k"
            ]
        );
        let cycle = &found.cases[1];
        assert!(cycle.project);
        assert!(cycle.expectation("diag").ends_with("cycle/expected.diag"));
        assert!(!found.cases[2].project);
        assert!(
            found.cases[2]
                .expectation("pending")
                .ends_with("rule.pending")
        );
        let problems: Vec<_> = found.problems.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            problems,
            ["parser/x/k.toml", "sema/modules/bad/expected.ast"]
        );
    }

    #[test]
    fn missing_root_is_empty() {
        assert_eq!(
            discover(Path::new("/definitely/not/here")),
            Discovery::default()
        );
    }
}
