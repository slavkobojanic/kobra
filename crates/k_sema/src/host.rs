//! The analysis host: the inputs of one project and immutable snapshots
//! that answer queries about them.
//!
//! # Model
//!
//! An [`AnalysisHost`] owns the current inputs: the text of every project
//! file, keyed by [`ProjectPath`]. [`AnalysisHost::apply_change`] creates a
//! new *revision*; files whose text didn't change keep their memoized
//! results (today: the parse), which is what makes a warm re-check cheap.
//! [`AnalysisHost::analysis`] returns an [`Analysis`], an immutable,
//! cheaply cloned snapshot that can be queried from any thread. A snapshot
//! taken before a change answers [`Cancelled`] from then on, so stale work
//! (an editor keystroke made it obsolete) stops early.
//!
//! The full query design, including the layers that the semantic stages
//! add, is in `docs/compiler/sema.md`, "Query architecture".
//!
//! # File ids
//!
//! Ids `0..n` are the `n` stdlib files, in [`Stdlib::files`] order, so the
//! stdlib's spans are valid in every host. Project files get the following
//! ids in the order they are first added; an id is never reused, and a path
//! keeps its id when it is removed and added again.
//!
//! # Status
//!
//! The query surface is final. The analysis parses every K source file (in
//! parallel), then runs the semantic pass once per revision
//! (`crate::check::analyze`); its diagnostics, types and resolutions are
//! memoized in the revision's state, as are the sides of every top-level
//! declaration that `side_of` answers (sema.md §12.6). Each revision hands
//! its body facts to the next, whose semantic pass replays those of the
//! files that didn't change (`crate::warm`).

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use k_diagnostics::{Diagnostic, FileId, SourceMap, Span, TextRange};
use k_syntax::Parse;

use crate::check::SemaResult;
use crate::lookup::{Cancelled, Resolution, Side};
use crate::program::InputFile;
use crate::project::{FileKind, ProjectPath};
use crate::refs::RefIndex;
use crate::stdlib::Stdlib;
use crate::warm::BodyCache;

/// Stack size of the worker threads that parse and check files. The parser
/// needs about 2 MiB on the deepest input in debug builds
/// (`docs/compiler/architecture.md`, "Nesting limit").
const WORKER_STACK: usize = 4 << 20;

/// Stack size of the thread that runs the semantic pass. The checker stops
/// following expressions past its depth limit; this leaves ample room for
/// that depth in debug builds. The memory is reserved, not committed.
const SEMA_STACK: usize = 256 << 20;

/// Settings of an analysis.
#[derive(Clone, Debug, Default)]
pub struct Options {
    threads: Option<NonZeroUsize>,
}

impl Options {
    /// The default options: as many worker threads as the machine has
    /// cores.
    #[must_use]
    pub fn new() -> Options {
        Options::default()
    }

    /// Uses exactly `threads` worker threads (1 checks on the calling
    /// thread only).
    #[must_use]
    pub fn with_threads(mut self, threads: NonZeroUsize) -> Options {
        self.threads = Some(threads);
        self
    }

    /// Returns the number of worker threads to use.
    #[must_use]
    pub fn threads(&self) -> NonZeroUsize {
        self.threads
            .unwrap_or_else(|| std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN))
    }
}

/// A batch of input changes, applied atomically by
/// [`AnalysisHost::apply_change`].
///
/// # Example
///
/// ```
/// use k_sema::{AnalysisHost, Change, Options, ProjectPath};
///
/// let mut host = AnalysisHost::new(Options::new());
/// let mut change = Change::new();
/// change.set_file(ProjectPath::new("src/a/A.k").unwrap(), "let x = (\n");
/// host.apply_change(change);
/// let diagnostics = host.analysis().diagnostics().unwrap();
/// assert!(!diagnostics.is_empty()); // the unclosed `(`
/// ```
#[derive(Clone, Debug, Default)]
pub struct Change {
    entries: Vec<(ProjectPath, Option<Arc<str>>)>,
}

impl Change {
    /// Creates an empty change.
    #[must_use]
    pub fn new() -> Change {
        Change::default()
    }

    /// Sets the text of the file at `path`, adding the file if it is new.
    /// Every file of the project is given, including `k.toml` and
    /// lockfiles; files of kind [`FileKind::Other`] are kept but ignored.
    pub fn set_file(&mut self, path: ProjectPath, text: impl Into<Arc<str>>) {
        self.entries.push((path, Some(text.into())));
    }

    /// Removes the file at `path`. Removing a file that doesn't exist does
    /// nothing.
    pub fn remove_file(&mut self, path: ProjectPath) {
        self.entries.push((path, None));
    }

    /// Returns `true` if the change holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One project file and its memoized results.
#[derive(Debug)]
struct FileData {
    path: ProjectPath,
    kind: FileKind,
    text: Arc<str>,
    /// The parse, computed on first use. Kept across revisions while the
    /// text is unchanged.
    parse: OnceLock<Parse>,
}

/// The inputs of one revision.
#[derive(Debug)]
struct State {
    revision: u64,
    options: Options,
    /// The project files by `id - stdlib_count`; `None` for removed files.
    files: Vec<Option<Arc<FileData>>>,
    by_path: HashMap<ProjectPath, FileId>,
    stdlib_count: u32,
    /// The semantic pass's result, computed on first use.
    sema: OnceLock<Arc<SemaResult>>,
    /// The body facts of the latest earlier revision whose semantic pass
    /// ran: unchanged files replay them (`crate::warm`).
    prev_cache: Option<Arc<BodyCache>>,
    /// The reverse index of the resolutions, built by the first tooling
    /// query of the revision (`crate::refs`).
    refs: OnceLock<Arc<RefIndex>>,
    /// The latest earlier revision's reverse index: the chunks this
    /// revision shares with it reuse their index.
    prev_refs: Option<Arc<RefIndex>>,
}

impl State {
    fn project_file(&self, file: FileId) -> Option<&Arc<FileData>> {
        let index = file.raw().checked_sub(self.stdlib_count)?;
        self.files.get(index as usize)?.as_ref()
    }
}

/// The inputs of one project and the memoized results derived from them.
///
/// Create one per project, feed it every project file with
/// [`apply_change`](AnalysisHost::apply_change), and query
/// [`analysis`](AnalysisHost::analysis) snapshots.
#[derive(Debug)]
pub struct AnalysisHost {
    state: Arc<State>,
    /// The current revision, shared with every snapshot so stale snapshots
    /// can tell they are cancelled.
    current: Arc<AtomicU64>,
}

impl AnalysisHost {
    /// Creates a host with no project files.
    #[must_use]
    pub fn new(options: Options) -> AnalysisHost {
        let stdlib_count = u32::try_from(Stdlib::get().files().len()).unwrap_or(u32::MAX);
        AnalysisHost {
            state: Arc::new(State {
                revision: 0,
                options,
                files: Vec::new(),
                by_path: HashMap::new(),
                stdlib_count,
                sema: OnceLock::new(),
                prev_cache: None,
                refs: OnceLock::new(),
                prev_refs: None,
            }),
            current: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Creates a host holding `files`, a convenience for batch drivers.
    #[must_use]
    pub fn with_files(
        options: Options,
        files: impl IntoIterator<Item = (ProjectPath, String)>,
    ) -> AnalysisHost {
        let mut host = AnalysisHost::new(options);
        let mut change = Change::new();
        for (path, text) in files {
            change.set_file(path, text);
        }
        host.apply_change(change);
        host
    }

    /// Applies `change` as a new revision. Snapshots taken earlier answer
    /// [`Cancelled`] from now on. Files whose text is unchanged keep their
    /// memoized results.
    pub fn apply_change(&mut self, change: Change) {
        if change.is_empty() {
            return;
        }
        let old = &self.state;
        let mut files = old.files.clone();
        let mut by_path = old.by_path.clone();
        for (path, text) in change.entries {
            let id = match by_path.get(&path) {
                Some(&id) => id,
                None => {
                    let raw = old.stdlib_count + u32::try_from(files.len()).unwrap_or(u32::MAX);
                    let id = FileId::new(raw);
                    files.push(None);
                    by_path.insert(path.clone(), id);
                    id
                }
            };
            let slot = &mut files[(id.raw() - old.stdlib_count) as usize];
            match text {
                Some(text) => {
                    let unchanged = slot.as_ref().is_some_and(|f| f.text == text);
                    if !unchanged {
                        *slot = Some(Arc::new(FileData {
                            kind: path.kind(),
                            path,
                            text,
                            parse: OnceLock::new(),
                        }));
                    }
                }
                None => *slot = None,
            }
        }
        let revision = old.revision + 1;
        let new = Arc::new(State {
            revision,
            options: old.options.clone(),
            files,
            by_path,
            stdlib_count: old.stdlib_count,
            sema: OnceLock::new(),
            prev_cache: old
                .sema
                .get()
                .map(|s| Arc::clone(&s.cache))
                .or_else(|| old.prev_cache.clone()),
            refs: OnceLock::new(),
            prev_refs: old.refs.get().cloned().or_else(|| old.prev_refs.clone()),
        });
        // The replaced revision usually holds the only reference to the
        // previous semantic result; freeing it is left to a background
        // thread.
        dispose(std::mem::replace(&mut self.state, new));
        self.current.store(revision, Ordering::Release);
    }

    /// Returns a snapshot of the current revision.
    #[must_use]
    pub fn analysis(&self) -> Analysis {
        Analysis {
            state: Arc::clone(&self.state),
            current: Arc::clone(&self.current),
        }
    }
}

/// Where a file comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileOrigin<'a> {
    /// A project file.
    Project(&'a ProjectPath),
    /// A stdlib file, by its path below `stdlib/` (`k/http/HttpClient.k`).
    Stdlib(&'static str),
}

/// An immutable snapshot of one revision of a project, answering queries.
///
/// Cloning is cheap, and a snapshot can be sent to other threads. Every
/// query returns `Err(Cancelled)` once the host has moved to a newer
/// revision.
#[derive(Clone, Debug)]
pub struct Analysis {
    state: Arc<State>,
    current: Arc<AtomicU64>,
}

impl Analysis {
    /// Returns `true` once the host has applied a newer change.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.current.load(Ordering::Acquire) != self.state.revision
    }

    fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }

    /// Returns the project files, sorted by path.
    #[must_use]
    pub fn files(&self) -> Vec<(FileId, &ProjectPath)> {
        let mut files: Vec<(FileId, &ProjectPath)> = self
            .state
            .files
            .iter()
            .flatten()
            .map(|f| (self.state.by_path[&f.path], &f.path))
            .collect();
        files.sort_by(|a, b| a.1.cmp(b.1));
        files
    }

    /// Returns the id of the project file at `path`, if it exists in this
    /// revision.
    #[must_use]
    pub fn file_id(&self, path: &ProjectPath) -> Option<FileId> {
        let id = *self.state.by_path.get(path)?;
        self.state.project_file(id).map(|_| id)
    }

    /// Returns where `file` comes from, or `None` for an unknown or removed
    /// file.
    #[must_use]
    pub fn origin(&self, file: FileId) -> Option<FileOrigin<'_>> {
        if file.raw() < self.state.stdlib_count {
            return Stdlib::get()
                .files()
                .get(file.raw() as usize)
                .map(|f| FileOrigin::Stdlib(f.path));
        }
        self.state
            .project_file(file)
            .map(|f| FileOrigin::Project(&f.path))
    }

    /// Returns the text of a project or stdlib file.
    #[must_use]
    pub fn text(&self, file: FileId) -> Option<&str> {
        if file.raw() < self.state.stdlib_count {
            return Stdlib::get()
                .files()
                .get(file.raw() as usize)
                .map(|f| f.text);
        }
        self.state.project_file(file).map(|f| &*f.text)
    }

    /// Builds a [`SourceMap`] for rendering diagnostics, in which every id
    /// of this analysis is valid. Project files are shown as
    /// `display(path)` (a driver prefixes its working directory), stdlib
    /// files as `<stdlib>/k/…`, and removed files as their old path with no
    /// text.
    pub fn source_map(&self, display: impl Fn(&ProjectPath) -> String) -> SourceMap {
        let mut map = SourceMap::new();
        for file in Stdlib::get().files() {
            map.add(format!("<stdlib>/{}", file.path), file.text);
        }
        let mut paths: Vec<Option<&ProjectPath>> = vec![None; self.state.files.len()];
        for (path, id) in &self.state.by_path {
            if let Some(slot) = paths.get_mut((id.raw() - self.state.stdlib_count) as usize) {
                *slot = Some(path);
            }
        }
        for (index, data) in self.state.files.iter().enumerate() {
            match data {
                Some(data) => map.add(display(&data.path), &*data.text),
                None => map.add(paths[index].map(&display).unwrap_or_default(), ""),
            };
        }
        map
    }

    /// Returns the parse of a K source file of the project, parsing it on
    /// first use. `None` for unknown files and files that aren't K source.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn parse(&self, file: FileId) -> Result<Option<&Parse>, Cancelled> {
        self.check()?;
        Ok(self
            .state
            .project_file(file)
            .filter(|f| f.kind.is_k_source())
            .map(|f| f.parse.get_or_init(|| k_syntax::parse(file, &f.text))))
    }

    /// Returns every diagnostic of the project, sorted by file, span start,
    /// span end and code (`docs/compiler/diagnostics.md`, rule 7):
    /// syntax diagnostics of every K source file, then the semantic stages'
    /// diagnostics (`docs/compiler/sema.md`).
    ///
    /// Files are parsed in parallel on [`Options::threads`] threads.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed while the query ran.
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>, Cancelled> {
        self.parse_all()?;
        let mut diagnostics = Vec::new();
        for data in self.state.files.iter().flatten() {
            if let Some(parse) = data.parse.get() {
                diagnostics.extend_from_slice(parse.diagnostics());
            }
        }
        self.check()?;
        diagnostics.extend(self.semantic_diagnostics()?);
        diagnostics.sort_by_key(|d| (d.span, d.code));
        Ok(diagnostics)
    }

    /// Returns the other project files that `file`'s code refers to: the
    /// files declaring the declarations and members its names, types and
    /// members resolve to, sorted and without duplicates. A change to any
    /// of them can change what `file` means, so `k test --watch` re-runs
    /// the tests of a test file when it or one of these (transitively)
    /// changes. Stdlib files are never listed.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed while the query ran.
    pub fn file_dependencies(&self, file: FileId) -> Result<Vec<FileId>, Cancelled> {
        let sema = self.sema()?;
        let view = &sema.view;
        let mut out: Vec<FileId> = sema
            .resolutions
            .values_in(file)
            .into_iter()
            .filter_map(|r| match view.resolve(r)? {
                crate::view::Resolved::Decl(d) => Some(view.decl(d)),
                crate::view::Resolved::Member(m) => Some(view.decl(view.member(m).owner)),
            })
            .filter(|d| !d.stdlib)
            .map(|d| d.file)
            .filter(|&f| f != file)
            .collect();
        out.sort_unstable();
        out.dedup();
        self.check()?;
        Ok(out)
    }

    /// Returns the diagnostics whose primary span is in `file`, sorted.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed while the query ran.
    pub fn file_diagnostics(&self, file: FileId) -> Result<Vec<Diagnostic>, Cancelled> {
        Ok(self
            .diagnostics()?
            .into_iter()
            .filter(|d| d.span.file == file)
            .collect())
    }

    /// Returns the type of the expression, pattern binding or declaration
    /// name whose syntax range (trivia excluded) is exactly `range` in
    /// `file`, rendered as K type syntax (`List<String>`, `shop.Cart?`,
    /// `(Int) async throws k.http.HttpError -> String`, `{error}`).
    /// `None` when no such node exists or it has no type.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn type_at(&self, file: FileId, range: TextRange) -> Result<Option<String>, Cancelled> {
        let sema = self.sema()?;
        Ok(sema
            .types
            .get(&(file, range))
            .map(|ty| ty.render(&sema.names)))
    }

    /// Returns what the name, member, element, attribute, CSS property,
    /// class or token reference whose syntax range is exactly `range`
    /// refers to. `None` when nothing at that range resolves.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn resolve_at(
        &self,
        file: FileId,
        range: TextRange,
    ) -> Result<Option<Resolution>, Cancelled> {
        let sema = self.sema()?;
        Ok(sema.resolutions.get(&(file, range)).cloned())
    }

    /// Returns the side of the server/client boundary of the top-level
    /// declaration whose name token is exactly `range`.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn side_of(&self, file: FileId, range: TextRange) -> Result<Option<Side>, Cancelled> {
        let sema = self.sema()?;
        Ok(sema.sides.get(&(file, range)).copied())
    }

    /// Returns the read-only view of the checked program: every
    /// declaration, member and generic parameter with resolved signatures
    /// ([`crate::view`]). Code generation reads the program through it and
    /// through [`type_info_at`](Analysis::type_info_at) and
    /// [`resolve_at`](Analysis::resolve_at).
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn program(&self) -> Result<&crate::view::ProgramView, Cancelled> {
        Ok(&self.sema()?.view)
    }

    /// Returns the type of the node whose range is exactly `range` in
    /// `file`, like [`type_at`](Analysis::type_at) but in the structured
    /// form whose ids index [`program`](Analysis::program).
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn type_info_at(
        &self,
        file: FileId,
        range: TextRange,
    ) -> Result<Option<crate::view::Type>, Cancelled> {
        let sema = self.sema()?;
        Ok(sema
            .types
            .get(&(file, range))
            .map(crate::view::Type::from_ty))
    }

    /// Returns every span in the project that refers to `target`, sorted
    /// and without duplicates: the project-wide reverse index of
    /// [`resolve_at`](Analysis::resolve_at), doc links included. The
    /// definition itself is not a reference; see
    /// [`definition`](Analysis::definition).
    ///
    /// The index is built on the first tooling query of a revision and
    /// reuses the previous revision's index for every file whose body facts
    /// were replayed (`crate::refs`), so after a body edit only the edited
    /// file is indexed again.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn references(&self, target: &Resolution) -> Result<Vec<Span>, Cancelled> {
        let index = self.ref_index()?;
        let refs = index.references(target);
        self.check()?;
        Ok(refs)
    }

    /// Returns every resolved reference in `file`, sorted by range start
    /// then end: what [`resolve_at`](Analysis::resolve_at) answers for each
    /// range of the file that resolves. Tooling finds the reference under a
    /// cursor here, and highlights a file's occurrences of a symbol.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn file_references(
        &self,
        file: FileId,
    ) -> Result<Vec<(TextRange, Arc<Resolution>)>, Cancelled> {
        let index = self.ref_index()?;
        let refs = index.file_references(file);
        self.check()?;
        Ok(refs)
    }

    /// Returns where `target` is declared: the name token of a declaration
    /// or member (stdlib ones included, whose files have ids below the
    /// project's), the binding name of a local or parameter, the token
    /// name of a theme token, or the selector of a style class or
    /// keyframes rule. `None` for built-in elements, attributes, events and
    /// CSS properties, which have no K source, and for names that no
    /// longer exist.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn definition(&self, target: &Resolution) -> Result<Option<Span>, Cancelled> {
        let sema = self.sema()?;
        Ok(crate::tooling::definition(self, &sema.view, target))
    }

    /// Returns every declaration that implements interface or protocol
    /// `decl`, directly or through another interface, sorted by id. Empty
    /// for a declaration nothing implements.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn implementations(
        &self,
        decl: crate::view::DeclId,
    ) -> Result<Vec<crate::view::DeclId>, Cancelled> {
        let sema = self.sema()?;
        Ok(crate::tooling::implementations(&sema.view, decl))
    }

    /// Returns the URL template of page `decl`: its `at` template, or the
    /// default derived from its name and its required `@Param`s
    /// (`/sign-in`, `/product/{id}`; `docs/design/12-routing.md`). `None`
    /// when `decl` is not a page.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the inputs changed.
    pub fn page_url(&self, decl: crate::view::DeclId) -> Result<Option<String>, Cancelled> {
        let sema = self.sema()?;
        Ok(crate::tooling::page_url(self, &sema.view, decl))
    }

    /// The revision's reverse index, built on first use.
    fn ref_index(&self) -> Result<&RefIndex, Cancelled> {
        let sema = self.sema()?;
        let index = self.state.refs.get_or_init(|| {
            Arc::new(RefIndex::build(
                &sema.resolutions,
                self.state.prev_refs.as_deref(),
            ))
        });
        self.check()?;
        Ok(index)
    }

    /// Parses every K source file that has no memoized parse yet, on
    /// [`Options::threads`] worker threads.
    fn parse_all(&self) -> Result<(), Cancelled> {
        let pending: Vec<(FileId, &FileData)> = self
            .state
            .files
            .iter()
            .flatten()
            .filter(|f| f.kind.is_k_source() && f.parse.get().is_none())
            .map(|f| (self.state.by_path[&f.path], &**f))
            .collect();
        let threads = self.state.options.threads().get().min(pending.len());
        if threads <= 1 {
            for (id, data) in &pending {
                self.check()?;
                data.parse.get_or_init(|| k_syntax::parse(*id, &data.text));
            }
            return self.check();
        }
        let next = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            for _ in 0..threads {
                let worker = || {
                    loop {
                        if stop.load(Ordering::Relaxed) || self.is_cancelled() {
                            stop.store(true, Ordering::Relaxed);
                            return;
                        }
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(id, data)) = pending.get(index) else {
                            return;
                        };
                        data.parse.get_or_init(|| k_syntax::parse(id, &data.text));
                    }
                };
                let spawned = std::thread::Builder::new()
                    .stack_size(WORKER_STACK)
                    .spawn_scoped(scope, worker);
                if spawned.is_err() {
                    // Out of threads: the remaining work runs on the threads
                    // already started, or below on this one.
                    break;
                }
            }
        });
        // Whatever a failed spawn left undone.
        for (id, data) in &pending {
            self.check()?;
            data.parse.get_or_init(|| k_syntax::parse(*id, &data.text));
        }
        self.check()
    }

    /// The diagnostics of the semantic stages (`docs/compiler/sema.md`).
    fn semantic_diagnostics(&self) -> Result<Vec<Diagnostic>, Cancelled> {
        Ok(self.sema()?.diagnostics.clone())
    }

    /// Runs the semantic pass once per revision (`crate::check`).
    ///
    /// Once the result exists this is O(1): the point queries
    /// ([`type_info_at`](Analysis::type_info_at),
    /// [`resolve_at`](Analysis::resolve_at)) call it once per syntax node
    /// during code generation, so it must not rescan the files
    /// (`docs/compiler/performance.md` §11.4).
    fn sema(&self) -> Result<&SemaResult, Cancelled> {
        if let Some(result) = self.state.sema.get() {
            self.check()?;
            return Ok(result);
        }
        self.parse_all()?;
        let result = self.state.sema.get_or_init(|| {
            // The checker recurses over expressions; give it a stack well
            // beyond its depth limit, as the parser's workers have. The
            // thread owns a handle on the state, so it hands the result over
            // as soon as it exists and frees the pass's working state (the
            // item tree, the checker's caches: about a sixth of a warm
            // re-check, `docs/compiler/performance.md` §10.4) after the
            // caller has moved on. If the thread can't start or panics
            // before delivering, the pass runs here, as before.
            let state = Arc::clone(&self.state);
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let spawned = std::thread::Builder::new()
                .stack_size(SEMA_STACK)
                .spawn(move || {
                    run_sema(&state, |result| {
                        let _ = tx.send(Arc::new(result));
                    });
                });
            spawned
                .ok()
                .and_then(|_| rx.recv().ok())
                .unwrap_or_else(|| {
                    let mut out = None;
                    run_sema(&self.state, |result| out = Some(Arc::new(result)));
                    out.unwrap_or_else(|| Arc::new(SemaResult::default()))
                })
        });
        self.check()?;
        Ok(result)
    }
}

/// Runs the semantic pass over `state`'s parsed sources and passes the
/// result to `deliver` before the pass's working state is dropped
/// (`check::analyze`).
fn run_sema(state: &State, deliver: impl FnOnce(SemaResult)) {
    let inputs: Vec<InputFile<'_>> = state
        .files
        .iter()
        .flatten()
        .filter(|f| f.kind.is_k_source())
        .filter_map(|f| {
            let parse = f.parse.get()?;
            Some(InputFile {
                id: state.by_path[&f.path],
                path: &f.path,
                text: &f.text,
                shared: &f.text,
                root: parse.syntax(),
            })
        })
        .collect();
    let lockfiles: Vec<crate::locks::Lockfile> = state
        .files
        .iter()
        .flatten()
        .filter_map(|f| match f.kind {
            FileKind::Lockfile(kind) => Some(crate::locks::Lockfile {
                id: state.by_path[&f.path],
                kind,
                text: Arc::clone(&f.text),
            }),
            _ => None,
        })
        .collect();
    crate::check::analyze(inputs, lockfiles, state.prev_cache.as_deref(), deliver);
}

/// Drops `value` on a background thread, so freeing a large structure
/// (a replaced revision and its semantic result) isn't paid by the caller.
/// Drops it in place if the thread can't be started.
fn dispose<T: Send + 'static>(value: T) {
    type Garbage = Box<dyn Send>;
    static DISPOSER: OnceLock<Option<std::sync::mpsc::Sender<Garbage>>> = OnceLock::new();
    let sender = DISPOSER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Garbage>();
        std::thread::Builder::new()
            .name("k-sema-dispose".to_owned())
            .spawn(move || {
                for garbage in rx {
                    drop(garbage);
                }
            })
            .ok()
            .map(|_| tx)
    });
    let garbage: Garbage = Box::new(value);
    if let Some(Err(std::sync::mpsc::SendError(garbage))) = sender.as_ref().map(|s| s.send(garbage))
    {
        drop(garbage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> ProjectPath {
        ProjectPath::new(text).unwrap()
    }

    #[test]
    fn file_dependencies_are_the_files_a_file_refers_to() {
        let host = AnalysisHost::with_files(
            Options::new(),
            vec![
                (path("k.toml"), "[package]\nname = \"p\"\nk = \"0.1\"\n".to_owned()),
                (path("src/a/Cart.k"), "import b.Price\n\npublic func total(p: Price) -> Int {\n    return p.cents + Tax.rate\n}\n".to_owned()),
                (path("src/a/Tax.k"), "public enum Tax {\n    case none\n\n    static let rate: Int = 0\n}\n".to_owned()),
                (path("src/b/Price.k"), "public struct Price {\n    let cents: Int\n}\n".to_owned()),
                (path("src/b/Unrelated.k"), "public let unrelated = 1\n".to_owned()),
            ],
        );
        let analysis = host.analysis();
        let id = |p: &str| analysis.file_id(&path(p)).unwrap();
        let deps = analysis.file_dependencies(id("src/a/Cart.k")).unwrap();
        let mut expected = vec![id("src/a/Tax.k"), id("src/b/Price.k")];
        expected.sort_unstable();
        assert_eq!(deps, expected);
        assert!(
            analysis
                .file_dependencies(id("src/b/Price.k"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn stdlib_files_take_the_first_ids() {
        let host = AnalysisHost::with_files(
            Options::new(),
            [(path("src/a/A.k"), "let a = 1\n".to_owned())],
        );
        let analysis = host.analysis();
        let stdlib = Stdlib::get().files().len();
        let id = analysis.file_id(&path("src/a/A.k")).unwrap();
        assert_eq!(id.raw() as usize, stdlib);
        assert_eq!(
            analysis.origin(FileId::new(0)),
            Some(FileOrigin::Stdlib(Stdlib::get().files()[0].path))
        );
        assert_eq!(
            analysis.origin(id),
            Some(FileOrigin::Project(&path("src/a/A.k")))
        );
        assert_eq!(analysis.text(id), Some("let a = 1\n"));
        assert!(analysis.text(FileId::new(0)).is_some());
        assert_eq!(analysis.origin(FileId::new(u32::MAX)), None);
        let map = analysis.source_map(|p| format!("root/{p}"));
        assert_eq!(map.len(), stdlib + 1);
        assert_eq!(map.get(id).unwrap().path(), "root/src/a/A.k");
        assert!(
            map.get(FileId::new(0))
                .unwrap()
                .path()
                .starts_with("<stdlib>/k/")
        );
    }

    #[test]
    fn syntax_diagnostics_are_reported_for_k_sources_only() {
        let host = AnalysisHost::with_files(
            Options::new().with_threads(NonZeroUsize::MIN),
            [
                (path("src/a/A.k"), "let a = (\n".to_owned()),
                (path("App.k"), "app A {\n".to_owned()),
                (path("k.toml"), "let (((\n".to_owned()),
                (path("README.md"), "let (((\n".to_owned()),
            ],
        );
        let analysis = host.analysis();
        let diagnostics = analysis.diagnostics().unwrap();
        let files: Vec<FileId> = diagnostics.iter().map(|d| d.span.file).collect();
        let a = analysis.file_id(&path("src/a/A.k")).unwrap();
        let app = analysis.file_id(&path("App.k")).unwrap();
        assert!(
            files.contains(&a) && files.contains(&app),
            "{diagnostics:?}"
        );
        let expected = k_syntax::parse(a, "let a = (\n").diagnostics().len()
            + k_syntax::parse(app, "app A {\n").diagnostics().len();
        assert_eq!(files.len(), expected);
        assert!(
            analysis
                .parse(analysis.file_id(&path("k.toml")).unwrap())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            analysis.file_diagnostics(a).unwrap(),
            k_syntax::parse(a, "let a = (\n").diagnostics()
        );
    }

    #[test]
    fn parallel_and_serial_parsing_agree() {
        let files: Vec<(ProjectPath, String)> = (0..64)
            .map(|i| {
                (
                    path(&format!("src/m{i}/F.k")),
                    format!("let x{i} = (\nfunc f() {{ $ }}\n"),
                )
            })
            .collect();
        let serial = AnalysisHost::with_files(
            Options::new().with_threads(NonZeroUsize::MIN),
            files.clone(),
        );
        let parallel = AnalysisHost::with_files(
            Options::new().with_threads(NonZeroUsize::new(8).unwrap()),
            files,
        );
        let a = serial.analysis().diagnostics().unwrap();
        let b = parallel.analysis().diagnostics().unwrap();
        assert_eq!(a, b);
        let per_file = k_syntax::parse(FileId::new(0), "let x0 = (\nfunc f() { $ }\n")
            .diagnostics()
            .len();
        assert!(per_file > 0);
        assert_eq!(a.len(), 64 * per_file);
        let mut sorted = a.clone();
        sorted.sort_by_key(|d| (d.span, d.code));
        assert_eq!(a, sorted, "diagnostics are sorted");
    }

    #[test]
    fn warm_re_checks_replay_unchanged_files_only() {
        let mut host = AnalysisHost::new(Options::new());
        let mut change = Change::new();
        change.set_file(path("k.toml"), "[package]\nname = \"w\"\nk = \"0.1\"\n");
        change.set_file(
            path("src/a/f.k"),
            "public func f() -> Int {\n    return 1\n}\n",
        );
        change.set_file(
            path("src/b/g.k"),
            "public func g() -> Int {\n    return 2\n}\n",
        );
        host.apply_change(change);
        assert!(host.analysis().diagnostics().unwrap().is_empty());
        let facts = |host: &AnalysisHost, p: &str| {
            let id = host.analysis().file_id(&path(p)).unwrap();
            let sema = host.state.sema.get().unwrap();
            Arc::clone(&sema.cache.files[&id])
        };
        let (a0, b0) = (facts(&host, "src/a/f.k"), facts(&host, "src/b/g.k"));

        // A body edit: `b` is replayed (the same facts), `a` is checked.
        let mut body = Change::new();
        body.set_file(
            path("src/a/f.k"),
            "public func f() -> Int {\n    return 3\n}\n",
        );
        host.apply_change(body);
        assert!(host.analysis().diagnostics().unwrap().is_empty());
        let (a1, b1) = (facts(&host, "src/a/f.k"), facts(&host, "src/b/g.k"));
        assert!(!Arc::ptr_eq(&a0, &a1), "the edited file is checked again");
        assert!(Arc::ptr_eq(&b0, &b1), "an unchanged file is replayed");

        // A signature edit re-checks every file.
        let mut sig = Change::new();
        sig.set_file(
            path("src/a/f.k"),
            "public func f() -> Double {\n    return 3.0\n}\n",
        );
        host.apply_change(sig);
        assert!(host.analysis().diagnostics().unwrap().is_empty());
        let b2 = facts(&host, "src/b/g.k");
        assert!(
            !Arc::ptr_eq(&b1, &b2),
            "a signature edit checks everything again"
        );
    }

    #[test]
    fn the_reverse_index_reuses_the_chunks_of_replayed_files() {
        let files = (0..8).map(|i| {
            (
                path(&format!("src/m{i}/F{i}.k")),
                format!("public func f{i}() -> Int {{\n    let x = 1\n    return x\n}}\n"),
            )
        });
        let mut host = AnalysisHost::with_files(Options::new(), files);
        let x = Resolution::Local {
            name: "x".to_owned(),
            definition: Span::new(
                host.analysis().file_id(&path("src/m0/F0.k")).unwrap(),
                TextRange::new(34.into(), 35.into()),
            ),
        };
        assert_eq!(host.analysis().references(&x).unwrap().len(), 1);
        let before = Arc::clone(host.state.refs.get().unwrap());
        let mut body = Change::new();
        body.set_file(
            path("src/m3/F3.k"),
            "public func f3() -> Int {\n    let x = 2\n    return x + x\n}\n",
        );
        host.apply_change(body);
        assert_eq!(host.analysis().references(&x).unwrap().len(), 1);
        let after = Arc::clone(host.state.refs.get().unwrap());
        // The edited file's chunk and the live chunk are indexed again; the
        // seven other files reuse theirs.
        assert!(
            after.rebuilt_since(&before) <= 2,
            "{}",
            after.rebuilt_since(&before)
        );
    }

    #[test]
    fn changes_create_revisions_and_cancel_old_snapshots() {
        let mut host = AnalysisHost::new(Options::new());
        let mut change = Change::new();
        change.set_file(path("src/a/A.k"), "let a = (\n");
        change.set_file(path("src/b/B.k"), "let b = 1\n");
        host.apply_change(change);
        let before = host.analysis();
        assert!(!before.diagnostics().unwrap().is_empty());
        let b = before.file_id(&path("src/b/B.k")).unwrap();
        let b_parse: *const Parse = before.parse(b).unwrap().unwrap();

        // An empty change is not a revision.
        host.apply_change(Change::new());
        assert!(!before.is_cancelled());

        let mut fix = Change::new();
        fix.set_file(path("src/a/A.k"), "let a = (1)\n");
        fix.set_file(path("src/b/B.k"), "let b = 1\n");
        host.apply_change(fix);
        assert!(before.is_cancelled());
        assert_eq!(before.diagnostics(), Err(Cancelled));
        assert_eq!(
            before.type_at(b, TextRange::empty(0.into())),
            Err(Cancelled)
        );
        let after = host.analysis();
        assert!(after.diagnostics().unwrap().is_empty());
        // Unchanged text keeps its memoized parse.
        assert!(std::ptr::eq(b_parse, after.parse(b).unwrap().unwrap()));

        let mut remove = Change::new();
        remove.remove_file(path("src/a/A.k"));
        remove.remove_file(path("src/missing/M.k"));
        host.apply_change(remove);
        let removed = host.analysis();
        assert_eq!(removed.file_id(&path("src/a/A.k")), None);
        assert_eq!(removed.files().len(), 1);
        let mut again = Change::new();
        again.set_file(path("src/a/A.k"), "let a = 2\n");
        host.apply_change(again);
        assert_eq!(
            host.analysis().file_id(&path("src/a/A.k")),
            before.file_id(&path("src/a/A.k")),
            "a path keeps its id"
        );
        let map = removed.source_map(ToString::to_string);
        assert!(
            map.get(before.file_id(&path("src/a/A.k")).unwrap())
                .is_some()
        );
    }

    #[test]
    fn queries_answer_types_and_resolutions() {
        let host = AnalysisHost::with_files(
            Options::new(),
            [(path("src/a/A.k"), "let a = 1\nlet b = a\n".to_owned())],
        );
        let analysis = host.analysis();
        let id = analysis.file_id(&path("src/a/A.k")).unwrap();
        let range = TextRange::new(4.into(), 5.into());
        assert_eq!(analysis.type_at(id, range), Ok(Some("Int".to_owned())));
        assert_eq!(analysis.resolve_at(id, range), Ok(None));
        assert_eq!(
            analysis.resolve_at(id, TextRange::new(18.into(), 19.into())),
            Ok(Some(Resolution::Declaration {
                name: "a.a".to_owned()
            }))
        );
        assert_eq!(
            analysis.type_at(id, TextRange::new(0.into(), 1.into())),
            Ok(None)
        );
        // `a` is a top-level declaration's name: a plain constant is shared.
        assert_eq!(analysis.side_of(id, range), Ok(Some(Side::Shared)));
        // Any other range answers nothing.
        assert_eq!(
            analysis.side_of(id, TextRange::new(0.into(), 3.into())),
            Ok(None)
        );
        assert_eq!(analysis.files().len(), 1);
    }
}
