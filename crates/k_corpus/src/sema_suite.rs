//! The `sema` suite: whole-project analysis cases.
//!
//! A case is either
//!
//! * a **single file** `name.k` anywhere below `tests/corpus/sema`, with
//!   optional siblings `name.diag` and `name.pending`; or
//! * a **project**: a directory containing a `k.toml`
//!   ([`PROJECT_MARKER`](crate::discover::PROJECT_MARKER)). Every file below
//!   it is a project file at its relative path (`App.k`, `src/shop/Cart.k`,
//!   `routes.lock`, …), except `expected.diag` and `expected.pending` in
//!   the directory itself, which are the case's expectations.
//!
//! A project whose `k.toml` declares a `[workspace]` is a **workspace
//! case** (`docs/design/40-workspaces-and-services.md`): every member is
//! analysed with the libraries it depends on mounted
//! ([`k_sema::Workspace`]), and the case's diagnostics are the merged
//! [`k_sema::Workspace::report`], with every path relative to the case
//! directory (`shop/App.k:3:5-3:9`, `shop/k.toml:6:1-6:33`). Caret
//! annotations are checked in the analysis of the member that owns the file.
//!
//! A single file is analysed as the only source file of a library project
//! whose `k.toml` is [`IMPLICIT_MANIFEST`]. Its project path is
//! `src/corpus/<file name>` ([`SINGLE_FILE_MODULE`]), unless its first line
//! is a path directive such as `// path: src/shop/Cart.k` or `// path: App.k`
//! ([`PATH_DIRECTIVE`]).
//!
//! # What a case checks
//!
//! 1. **Diagnostics.** Every diagnostic of the analysis (syntax and
//!    semantic) must match the `.diag` file, in the format of
//!    [`diag_format`]; no `.diag` file means none. In a
//!    single-file case spans have no path (except places in other files,
//!    such as `<stdlib>/k/core/List.k:3:1-3:5`); in a project every header
//!    names its file: `error[K1101] src/shop/Cart.k:3:8-3:20`. Every code in
//!    an expectation must be registered in `k_diagnostics::codes`.
//! 2. **Annotations.** Every caret annotation
//!    ([`annotations`]) in a K source file of the case
//!    must hold.
//!
//! # Pending cases
//!
//! Tests are written before the implementation. A case whose `.pending`
//! file exists (`name.pending`, or `expected.pending` in a project) is
//! **expected to fail** until the stage that makes it pass lands: it passes
//! while its expectations are unmet and *fails* once they all hold, telling
//! you to delete the `.pending` file. The file holds a one-line reason
//! (what is missing, such as `name resolution (sema.md §5)`); an empty file
//! is an error. Malformed expectations and unregistered codes fail even in a
//! pending case, and so do syntax diagnostics (`K0001`–`K0999`) that don't
//! match, because the parser exists: a pending source must parse exactly as
//! its author intended. Pending cases are never blessed.

use std::collections::HashMap;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use k_diagnostics::{FileId, LineIndex, TextRange, codes};
use k_sema::stdlib::Stdlib;
use k_sema::{Analysis, AnalysisHost, Options, ProjectPath, Workspace};

use crate::annotations::{self, Annotation, Query};
use crate::diag_format::{self, DiagEntry};
use crate::discover::{Case, PROJECT_EXPECTATION_STEM};
use crate::suites::{read_expectation, write};

/// The `k.toml` of a single-file case's implicit project.
pub const IMPLICIT_MANIFEST: &str = "[package]\nname = \"corpus\"\nk = \"0.1\"\n";

/// The module a single-file case belongs to unless it has a path
/// directive: the file is `src/corpus/<file name>`.
pub const SINGLE_FILE_MODULE: &str = "corpus";

/// The prefix of a path directive on the first line of a single-file case.
pub const PATH_DIRECTIVE: &str = "// path:";

/// The files of a case, ready to analyse.
#[derive(Clone, Debug)]
pub struct LoadedCase {
    /// Every project file and its text, sorted by path.
    pub files: Vec<(ProjectPath, String)>,
    /// The case file of a single-file case, whose diagnostics are written
    /// without a path; `None` for a project.
    pub home: Option<ProjectPath>,
}

/// Loads the files of `case`.
///
/// # Errors
///
/// Returns a message for unreadable or non-UTF-8 files, an invalid path
/// directive, or a project file whose relative path is not a valid
/// [`ProjectPath`].
pub fn load(case: &Case) -> Result<LoadedCase, String> {
    if case.project {
        let mut paths = Vec::new();
        collect(&case.source, &mut paths);
        paths.sort();
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let relative = relative_path(&case.source, &path);
            // `expected.diag`, `expected.client.js`, … in the project
            // directory itself are expectations, not project files.
            let expectation = relative
                .strip_prefix(PROJECT_EXPECTATION_STEM)
                .is_some_and(|rest| rest.starts_with('.'))
                && !relative.contains('/');
            if expectation {
                continue;
            }
            let project_path = ProjectPath::new(&relative)
                .map_err(|e| format!("`{relative}` is not a valid project path: {e}"))?;
            files.push((project_path, read_text(&path)?));
        }
        return Ok(LoadedCase { files, home: None });
    }
    let text = read_text(&case.source)?;
    let path = single_file_path(case, &text)?;
    let manifest = ProjectPath::new("k.toml").map_err(|e| e.to_string())?;
    let mut files = vec![(path.clone(), text)];
    if path != manifest {
        files.push((manifest, IMPLICIT_MANIFEST.to_owned()));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(LoadedCase {
        files,
        home: Some(path),
    })
}

/// Returns the project path of a single-file case: the path directive on
/// its first line, or `src/corpus/<file name>`.
fn single_file_path(case: &Case, text: &str) -> Result<ProjectPath, String> {
    let first = text.lines().next().unwrap_or_default();
    if let Some(rest) = first.trim_start().strip_prefix(PATH_DIRECTIVE) {
        let rest = rest.trim();
        return ProjectPath::new(rest)
            .map_err(|e| format!("the path directive `{rest}` is not a valid project path: {e}"));
    }
    let name = case
        .source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    ProjectPath::new(&format!("src/{SINGLE_FILE_MODULE}/{name}"))
        .map_err(|e| format!("`{name}` can't be a project file name: {e}"))
}

/// Returns the diagnostics of `case` rendered as its `.diag` file would be
/// written, for `k debug sema`.
///
/// # Errors
///
/// Returns a message if the case's files can't be loaded.
pub fn render_diagnostics(case: &Case) -> Result<String, String> {
    let loaded = load(case)?;
    if let Some(ws) = workspace_of(&loaded) {
        let (entries, _) = workspace_entries(&ws, NonZeroUsize::MIN)?;
        return Ok(diag_format::render(&entries));
    }
    let host = AnalysisHost::with_files(Options::new(), loaded.files);
    let analysis = host.analysis();
    Ok(diag_format::render(&actual_entries(
        &analysis,
        loaded.home.as_ref(),
    )?))
}

/// Runs one `sema` case. With `bless`, a mismatching `.diag` file of a case
/// that is not pending is rewritten (annotation mismatches still fail).
///
/// # Errors
///
/// Returns a failure report.
pub fn run(case: &Case, bless: bool) -> Result<(), String> {
    let loaded = load(case)?;
    let diag_path = case.expectation("diag");
    let expected = match read_expectation(&diag_path)? {
        Some(text) => diag_format::parse_diag_file(&text)
            .map_err(|e| format!("{}: {e}", diag_path.display()))?,
        None => Vec::new(),
    };
    check_codes_registered(&diag_path, &expected)?;
    let pending_path = case.expectation("pending");
    let pending = read_expectation(&pending_path)?;
    if pending
        .as_deref()
        .is_some_and(|reason| reason.trim().is_empty())
    {
        return Err(format!(
            "{} is empty; write why the case is pending (the missing stage)",
            pending_path.display()
        ));
    }
    let mut annotations = Vec::new();
    for (path, text) in &loaded.files {
        if path.kind().is_k_source() {
            let found = annotations::parse(text).map_err(|e| format!("{path}: {e}"))?;
            annotations.extend(found.into_iter().map(|a| (path.clone(), a)));
        }
    }

    let mut failures = Vec::new();
    let actual = if let Some(ws) = workspace_of(&loaded) {
        let (actual, analyses) = workspace_entries(&ws, NonZeroUsize::MIN)?;
        for (path, annotation) in &annotations {
            let Some(member) = ws.owner_of(path.as_str()) else {
                failures.push(format!("{path}: the file belongs to no workspace member"));
                continue;
            };
            let Some(inner) = ws.view_path(member, path.as_str()) else {
                continue;
            };
            let Some((_, analysis)) = analyses.iter().find(|(m, _)| *m == member) else {
                continue;
            };
            if let Some(failure) = check_annotation(analysis, &inner, annotation)? {
                failures.push(format!("{path}: {failure}"));
            }
        }
        actual
    } else {
        let host = AnalysisHost::with_files(
            Options::new().with_threads(NonZeroUsize::MIN),
            loaded.files.clone(),
        );
        let analysis = host.analysis();
        for (path, annotation) in &annotations {
            if let Some(failure) = check_annotation(&analysis, path, annotation)? {
                failures.push(failure);
            }
        }
        actual_entries(&analysis, loaded.home.as_ref())?
    };
    let diag_mismatch = diag_format::compare(&expected, &actual).err();

    if pending.is_some() {
        // Stage 1 is done, so a pending case's syntax diagnostics must hold
        // now: a typo in a test-first source can't hide behind the marker.
        let syntax = |entries: &[DiagEntry]| -> Vec<DiagEntry> {
            entries
                .iter()
                .filter(|e| is_syntax_code(&e.code))
                .cloned()
                .collect()
        };
        if let Err(report) = diag_format::compare(&syntax(&expected), &syntax(&actual)) {
            return Err(format!(
                "the syntax diagnostics (K0001–K0999) of a pending case must already match {}:\n{report}",
                diag_path.display()
            ));
        }
        if diag_mismatch.is_none() && failures.is_empty() {
            return Err(format!(
                "this pending case passes now; delete {} to make it a regular case",
                pending_path.display()
            ));
        }
        return Ok(());
    }
    if let Some(report) = diag_mismatch {
        if bless {
            if actual.is_empty() {
                if diag_path.exists() {
                    fs::remove_file(&diag_path)
                        .map_err(|e| format!("cannot remove {}: {e}", diag_path.display()))?;
                }
            } else {
                write(&diag_path, &diag_format::render(&actual))?;
            }
        } else {
            failures.insert(
                0,
                format!("diagnostics differ from {}:\n{report}", diag_path.display()),
            );
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

/// Each member's analysis, by member index.
type MemberAnalyses = Vec<(usize, Analysis)>;

/// The workspace of a project case whose `k.toml` declares one.
fn workspace_of(loaded: &LoadedCase) -> Option<Workspace> {
    let root = loaded.files.iter().find(|(p, _)| p.as_str() == "k.toml")?;
    if loaded.home.is_some() || !Workspace::is_workspace_manifest(&root.1) {
        return None;
    }
    Some(Workspace::from_files(
        loaded
            .files
            .iter()
            .map(|(p, t)| (p.as_str().to_owned(), t.clone())),
    ))
}

/// Analyses every member of `ws` and returns the merged diagnostics as
/// `.diag` entries (paths relative to the case directory), with the
/// member analyses.
fn workspace_entries(
    ws: &Workspace,
    threads: NonZeroUsize,
) -> Result<(Vec<DiagEntry>, MemberAnalyses), String> {
    let analyses: Vec<(usize, Analysis)> = (0..ws.members.len())
        .map(|i| {
            let host = AnalysisHost::with_files(
                Options::new().with_threads(threads),
                ws.analysis_files(i),
            );
            (i, host.analysis())
        })
        .collect();
    let (map, diagnostics) = ws
        .report(&analyses, str::to_owned)
        .map_err(|e| e.to_string())?;
    let lookup = |file: FileId| map.get(file).map(|f| (f.path(), f.text()));
    Ok((
        diag_format::entries_for_files(&diagnostics, lookup, None),
        analyses,
    ))
}

/// Returns `true` for a lexer or parser code (`K0001`–`K0999`).
fn is_syntax_code(code: &str) -> bool {
    k_diagnostics::Code::parse(code).is_some_and(|c| c.number() < 1000)
}

/// Every code an expectation names must be registered, so `.diag` files
/// and `k_diagnostics::codes` can't drift apart.
fn check_codes_registered(path: &Path, expected: &[DiagEntry]) -> Result<(), String> {
    for entry in expected {
        let registered = k_diagnostics::Code::parse(&entry.code).and_then(codes::lookup);
        if registered.is_none() {
            return Err(format!(
                "{}: `{}` is not a registered code; add it to crates/k_diagnostics/src/codes.rs and \
                 docs/compiler/diagnostics.md first",
                path.display(),
                entry.code
            ));
        }
    }
    Ok(())
}

/// The analysis's diagnostics as `.diag` entries, with project paths and
/// `<stdlib>/…` paths as file names.
fn actual_entries(
    analysis: &Analysis,
    home: Option<&ProjectPath>,
) -> Result<Vec<DiagEntry>, String> {
    let diagnostics = analysis.diagnostics().map_err(|e| e.to_string())?;
    let mut names: HashMap<FileId, String> = HashMap::new();
    for (id, path) in analysis.files() {
        names.insert(id, path.to_string());
    }
    for (index, file) in Stdlib::get().files().iter().enumerate() {
        let id = FileId::new(u32::try_from(index).unwrap_or(u32::MAX));
        names.insert(id, format!("<stdlib>/{}", file.path));
    }
    let home = home.and_then(|path| analysis.file_id(path));
    let lookup = |file: FileId| Some((names.get(&file)?.as_str(), analysis.text(file)?));
    Ok(diag_format::entries_for_files(&diagnostics, lookup, home))
}

/// Checks one annotation; `Ok(Some(report))` when it doesn't hold.
fn check_annotation(
    analysis: &Analysis,
    path: &ProjectPath,
    annotation: &Annotation,
) -> Result<Option<String>, String> {
    let file = analysis
        .file_id(path)
        .ok_or_else(|| format!("{path} is not in the analysis"))?;
    let text = analysis.text(file).unwrap_or_default();
    let index = LineIndex::new(text);
    let offset = |position| {
        index.offset(text, position).ok_or_else(|| {
            format!(
                "{path}:{}: the annotation's range {} is outside the file",
                annotation.line, annotation.target
            )
        })
    };
    let range = TextRange::new(
        offset(annotation.target.start)?,
        offset(annotation.target.end)?,
    );
    let cancelled = |e: k_sema::Cancelled| e.to_string();
    let actual = match annotation.query {
        Query::Type => analysis.type_at(file, range).map_err(cancelled)?,
        Query::Resolves => analysis
            .resolve_at(file, range)
            .map_err(cancelled)?
            .map(|resolution| {
                resolution.render(|span| {
                    let text = analysis.text(span.file)?;
                    Some(LineIndex::new(text).line_col(text, span.start()))
                })
            }),
        Query::Side => analysis
            .side_of(file, range)
            .map_err(cancelled)?
            .map(|side| side.to_string()),
    };
    if actual == annotation.expected {
        return Ok(None);
    }
    Ok(Some(format!(
        "{path}:{}: expected `{annotation}`, got `{}`",
        annotation.line,
        actual.as_deref().unwrap_or("none")
    )))
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| {
        format!(
            "{} is not valid UTF-8; corpus sources must be UTF-8",
            path.display()
        )
    })
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suites::Suite;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let path =
                std::env::temp_dir().join(format!("k_corpus_sema_{tag}_{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }

        fn file_case(&self, relative: &str) -> Case {
            Case {
                suite: Suite::Sema,
                source: self.0.join(relative),
                name: relative.into(),
                project: false,
            }
        }

        fn project_case(&self, relative: &str) -> Case {
            Case {
                suite: Suite::Sema,
                source: self.0.join(relative),
                name: format!("{relative}/"),
                project: true,
            }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn single_files_get_an_implicit_project() {
        let dir = TempDir::new("single");
        dir.write("plain.k", "let a = 1\n");
        dir.write("app.k", "// path: App.k\napp A {}\n");
        let plain = load(&dir.file_case("plain.k")).unwrap();
        let paths: Vec<_> = plain.files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["k.toml", "src/corpus/plain.k"]);
        assert_eq!(plain.files[0].1, IMPLICIT_MANIFEST);
        assert_eq!(plain.home.unwrap().as_str(), "src/corpus/plain.k");
        let app = load(&dir.file_case("app.k")).unwrap();
        assert_eq!(app.home.unwrap().as_str(), "App.k");
        dir.write("bad.k", "// path: ../x.k\n");
        assert!(
            load(&dir.file_case("bad.k"))
                .unwrap_err()
                .contains("path directive")
        );
    }

    #[test]
    fn projects_load_every_file_but_their_expectations() {
        let dir = TempDir::new("project");
        dir.write("p/k.toml", IMPLICIT_MANIFEST);
        dir.write("p/App.k", "app A {}\n");
        dir.write("p/src/shop/Cart.k", "let c = 1\n");
        dir.write("p/src/shop/expected.diag", "not an expectation here\n");
        dir.write("p/expected.diag", "");
        let loaded = load(&dir.project_case("p")).unwrap();
        let paths: Vec<_> = loaded.files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            [
                "App.k",
                "k.toml",
                "src/shop/Cart.k",
                "src/shop/expected.diag"
            ]
        );
        assert_eq!(loaded.home, None);
        run(&dir.project_case("p"), false).unwrap();
    }

    #[test]
    fn diagnostics_are_compared_with_paths_in_projects() {
        let dir = TempDir::new("project_diag");
        dir.write("p/k.toml", IMPLICIT_MANIFEST);
        dir.write("p/src/a/A.k", "let a = (\n");
        let case = dir.project_case("p");
        let error = run(&case, false).unwrap_err();
        assert!(error.contains("error[K0101] src/a/A.k:"), "{error}");
        run(&case, true).unwrap();
        let blessed = fs::read_to_string(case.expectation("diag")).unwrap();
        assert!(blessed.starts_with("error[K0101] src/a/A.k:"), "{blessed}");
        run(&case, false).unwrap();
    }

    #[test]
    fn single_file_diagnostics_have_no_path() {
        let dir = TempDir::new("single_diag");
        dir.write("e.k", "let a = (\n");
        let case = dir.file_case("e.k");
        run(&case, true).unwrap();
        let blessed = fs::read_to_string(case.expectation("diag")).unwrap();
        assert!(blessed.starts_with("error[K0101] 1:"), "{blessed}");
        run(&case, false).unwrap();
        // Fixing the source makes the expectation stale; blessing removes it.
        dir.write("e.k", "let a = (1)\n");
        assert!(run(&case, false).is_err());
        run(&case, true).unwrap();
        assert!(!case.expectation("diag").exists());
    }

    #[test]
    fn pending_cases_must_fail_until_they_pass() {
        let dir = TempDir::new("pending");
        // Not produced by the skeleton: the pending case passes.
        dir.write("p.k", "let a = 1\n");
        dir.write("p.diag", "error[K1201] 1:5-1:6\n");
        dir.write("p.pending", "name resolution\n");
        run(&dir.file_case("p.k"), false).unwrap();
        // Blessing never touches a pending case.
        run(&dir.file_case("p.k"), true).unwrap();
        assert!(dir.0.join("p.diag").exists());
        // Once everything holds, the marker must go.
        dir.write("p.diag", "");
        let error = run(&dir.file_case("p.k"), false).unwrap_err();
        assert!(error.contains("passes now"), "{error}");
        dir.write("p.pending", "  \n");
        assert!(
            run(&dir.file_case("p.k"), false)
                .unwrap_err()
                .contains("is empty")
        );
    }

    #[test]
    fn pending_cases_must_match_syntax_diagnostics() {
        let dir = TempDir::new("pending_syntax");
        dir.write("s.k", "let a = (\n");
        dir.write("s.diag", "error[K2001] 1:5-1:6\n");
        dir.write("s.pending", "types\n");
        let error = run(&dir.file_case("s.k"), false).unwrap_err();
        assert!(error.contains("syntax diagnostics"), "{error}");
        dir.write(
            "s.diag",
            "error[K0101] 1:9-1:10\nerror[K0122] 1:9-1:10\nerror[K2001] 1:5-1:6\n",
        );
        run(&dir.file_case("s.k"), false).unwrap();
    }

    #[test]
    fn unregistered_codes_fail_even_when_pending() {
        let dir = TempDir::new("codes");
        dir.write("c.k", "let a = 1\n");
        dir.write("c.diag", "error[K0999] 1:1-1:2\n");
        dir.write("c.pending", "later\n");
        let error = run(&dir.file_case("c.k"), false).unwrap_err();
        assert!(error.contains("not a registered code"), "{error}");
    }

    #[test]
    fn annotations_are_checked() {
        let dir = TempDir::new("annotations");
        dir.write(
            "none.k",
            "let a = 1\n//^^^ type: none\n//  ^ resolves: none\n",
        );
        run(&dir.file_case("none.k"), false).unwrap();
        dir.write("int.k", "let a = 1\n//      ^ type: Int\n");
        run(&dir.file_case("int.k"), false).unwrap();
        dir.write("typed.k", "let a = 1\n//      ^ type: String\n");
        let error = run(&dir.file_case("typed.k"), false).unwrap_err();
        assert!(
            error.contains("src/corpus/typed.k:2: expected `type: String at 1:9-1:10`, got `Int`"),
            "{error}"
        );
        // Blessing doesn't hide annotation failures.
        assert!(run(&dir.file_case("typed.k"), true).is_err());
        dir.write("typed.pending", "type inference\n");
        run(&dir.file_case("typed.k"), false).unwrap();
        dir.write("bad.k", "let a = 1\n// ^ kind: x\n");
        assert!(
            run(&dir.file_case("bad.k"), false)
                .unwrap_err()
                .contains("unknown annotation")
        );
        dir.write("far.k", "let a = 1\n//@ 9:1-9:2 type: Int\n");
        assert!(
            run(&dir.file_case("far.k"), false)
                .unwrap_err()
                .contains("outside the file")
        );
    }
}
