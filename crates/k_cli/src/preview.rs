//! `k preview`: the preview server (docs/design/57-previews.md, "`k
//! preview`"; docs/compiler/preview.md).
//!
//! One process compiles the project's previews ([`k_codegen::Entry::Previews`])
//! and serves the panel on `http://127.0.0.1:<port>/`: an index of every
//! preview, a page per preview with its switcher, controls, environment
//! bar and grid, and the JSON API and event stream the page runs on. Each
//! preview the page shows is a *mount*: a server of its own with the
//! preview's hermetic server container (`mounts`), which serves the frame
//! the page shows in an iframe.
//!
//! The project is watched on disk, as `k dev` watches it; with `--editor`
//! an editor also sends its unsaved buffers on standard input (`editor`),
//! which then carries the editor protocol both ways. A change rebuilds
//! after a short quiet time, and only a program without errors replaces
//! the last good build; otherwise the page shows the first error over the
//! last good render.

mod controls;
mod editor;
mod http;
mod mounts;

use std::collections::{BTreeMap, BTreeSet};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use k_codegen::{Entry, Environment, Options, Profile};
use k_sema::ProjectPath;
use k_sema::preview::PreviewParam;
use serde_json::{Value, json};

use crate::cli::PreviewArgs;
use controls::Kept;

/// How long the sources must be quiet before a rebuild.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// One preview of a build, as the page lists it.
struct PreviewEntry {
    /// Its group in the preview bundle (`$groups`), which a mount names.
    group: usize,
    /// The switcher's label.
    label: String,
    /// The `preview` keyword's byte range in its file.
    keyword: (u32, u32),
    /// The whole declaration's byte range.
    range: (u32, u32),
    /// The keyword's 1-based line.
    line: usize,
    /// The parameters.
    params: Vec<PreviewParam>,
}

/// A file with previews.
struct FileEntry {
    /// The project path.
    path: String,
    /// Its previews, in source order.
    previews: Vec<PreviewEntry>,
}

/// A good build.
struct Build {
    /// Its number, counting from 1.
    id: u64,
    /// The files with previews, by path.
    files: Vec<FileEntry>,
    /// What each mount serves.
    bundle: Arc<mounts::Bundle>,
}

impl Build {
    /// The preview of group `group`, with its file.
    fn entry(&self, group: usize) -> Option<(&FileEntry, &PreviewEntry)> {
        self.files
            .iter()
            .find_map(|f| f.previews.iter().find(|p| p.group == group).map(|p| (f, p)))
    }
}

/// What the page and the editor see, shared by the threads.
#[derive(Default)]
struct State {
    /// The last good build.
    build: Option<Arc<Build>>,
    /// The first error of the latest sources, while they have one:
    /// `{file, message, line, column}`.
    error: Option<Value>,
    /// The control values set in the panel, by file and preview label.
    values: BTreeMap<(String, String), BTreeMap<String, Kept>>,
    /// The preview the editor's cursor is in: file and index in the file.
    selected: Option<(String, usize)>,
}

/// The state and channels the threads share.
struct Shared {
    /// The state.
    state: Mutex<State>,
    /// The open event streams of the pages.
    streams: Mutex<Vec<TcpStream>>,
    /// The running mounts.
    mounts: Mutex<mounts::Mounts>,
    /// The project path prefix the index lists (`src/shop`), or empty.
    under: String,
    /// The project's name.
    name: String,
}

impl Shared {
    /// Sends `event` to every page, dropping the streams that closed.
    fn broadcast(&self, event: &Value) {
        let text = event.to_string();
        let mut streams = self
            .streams
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        streams.retain(|s| http::send_event(s, &text).is_ok());
    }
}

/// Runs `k preview`.
pub fn run(args: &PreviewArgs) -> ExitCode {
    let given = std::path::absolute(&args.path).unwrap_or_else(|_| args.path.clone());
    let Some(root) = project_root(&given) else {
        eprintln!(
            "error: `{}` is not in a Kobra project (no `k.toml` at or above it)",
            args.path.display()
        );
        return ExitCode::from(2);
    };
    let under = given
        .strip_prefix(&root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let mut project = match crate::project::load(&root, "preview") {
        Ok(p) => p,
        Err(code) => return code,
    };
    let listener = match TcpListener::bind(("127.0.0.1", args.port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "error: cannot listen on port {}: {e}; pick another with `--port`",
                args.port
            );
            return ExitCode::from(2);
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(args.port);
    let url = format!("http://127.0.0.1:{port}/");
    let shared = Arc::new(Shared {
        state: Mutex::new(State::default()),
        streams: Mutex::new(Vec::new()),
        mounts: Mutex::new(mounts::Mounts::default()),
        under: under.trim_matches('/').to_owned(),
        name: project.name().to_owned(),
    });
    let (tx, rx) = mpsc::channel::<editor::Message>();
    let out = args.editor.then(editor::Output::stdout);
    let mut session = Session {
        root: std::fs::canonicalize(&root).unwrap_or(root.clone()),
        shared: Arc::clone(&shared),
        overlays: BTreeMap::new(),
        disk: crate::test::current_texts(&root),
        seen: crate::build::fingerprint(&root),
        next_build: 1,
        editor: out.clone(),
        mentioned: BTreeSet::new(),
        changed: None,
    };
    let fs_root = root.clone();
    {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || serve(&listener, &shared));
    }
    if args.editor {
        let tx = tx.clone();
        std::thread::spawn(move || editor::read_stdin(&tx));
        if let Some(o) = &out {
            o.send(&json!({ "type": "ready", "url": url }));
        }
        eprintln!("previews on {url}");
    } else {
        println!("previews on {url}");
    }
    if !args.no_open
        && let Err(e) = crate::docs::open(Path::new(&url))
    {
        eprintln!("warning: couldn't open the browser: {e}");
    }
    drop(tx);
    session.rebuild(&mut project);
    let code = session.run(&mut project, &rx, &fs_root, args.editor);
    shared
        .mounts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .stop_all();
    code
}

/// The directory with `k.toml` at or above `path`.
fn project_root(path: &Path) -> Option<PathBuf> {
    let mut dir = if path.is_file() { path.parent()? } else { path };
    loop {
        if dir.join("k.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// The main loop's state: the sources, the overlays and the builds.
struct Session {
    /// The project directory, canonical.
    root: PathBuf,
    /// What the threads share.
    shared: Arc<Shared>,
    /// The editor's unsaved buffers, by project path.
    overlays: BTreeMap<ProjectPath, String>,
    /// The texts on disk, by project path.
    disk: BTreeMap<ProjectPath, String>,
    /// The disk's last fingerprint.
    seen: Vec<(PathBuf, u64, Option<std::time::SystemTime>)>,
    /// The number of the next good build.
    next_build: u64,
    /// The editor's end of the protocol, with `--editor`.
    editor: Option<editor::Output>,
    /// The files the editor sent or selected in, by project path: each
    /// gets a `previews` message after a build.
    mentioned: BTreeSet<String>,
    /// The file changed last, whose error the badge prefers.
    changed: Option<String>,
}

impl Session {
    /// Runs until the editor's input ends (`--editor`) or the process is
    /// stopped: watches the disk, applies the editor's messages and
    /// rebuilds after [`DEBOUNCE`] of quiet.
    fn run(
        &mut self,
        project: &mut crate::project::Project,
        rx: &mpsc::Receiver<editor::Message>,
        fs_root: &Path,
        editor_mode: bool,
    ) -> ExitCode {
        let mut dirty: Option<Instant> = None;
        let mut next_poll = Instant::now() + crate::build::WATCH_INTERVAL;
        loop {
            let now = Instant::now();
            let deadline = dirty.map_or(next_poll, |d| (d + DEBOUNCE).min(next_poll));
            match rx.recv_timeout(deadline.saturating_duration_since(now)) {
                Ok(message) => match self.handle(project, message) {
                    Handled::Changed => dirty = Some(Instant::now()),
                    Handled::Nothing => {}
                    Handled::Stop => return ExitCode::SUCCESS,
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if editor_mode {
                        return ExitCode::SUCCESS;
                    }
                    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                }
            }
            if Instant::now() >= next_poll {
                next_poll = Instant::now() + crate::build::WATCH_INTERVAL;
                if self.poll_disk(project, fs_root) {
                    dirty = Some(Instant::now());
                }
            }
            if dirty.is_some_and(|d| d.elapsed() >= DEBOUNCE) {
                dirty = None;
                self.rebuild(project);
            }
        }
    }

    /// Applies the changes on disk (the files without an overlay); `true`
    /// when something changed.
    fn poll_disk(&mut self, project: &mut crate::project::Project, fs_root: &Path) -> bool {
        let now = crate::build::fingerprint(fs_root);
        if now == self.seen {
            return false;
        }
        self.seen = now;
        let next = crate::test::current_texts(fs_root);
        let mut change = k_sema::Change::new();
        for (path, text) in &next {
            if self.disk.get(path) != Some(text) && !self.overlays.contains_key(path) {
                change.set_file(path.clone(), text.as_str());
                self.changed = Some(path.as_str().to_owned());
            }
        }
        for path in self.disk.keys() {
            if !next.contains_key(path) && !self.overlays.contains_key(path) {
                change.remove_file(path.clone());
            }
        }
        self.disk = next;
        if change.is_empty() {
            return false;
        }
        project.host.apply_change(change);
        true
    }

    /// Applies one editor message.
    fn handle(
        &mut self,
        project: &mut crate::project::Project,
        message: editor::Message,
    ) -> Handled {
        match message {
            editor::Message::Change { uri, text } => {
                let Some(path) = self.path_of(&uri) else {
                    self.reply_error(&uri, &format!("`{uri}` is not a file of this project"));
                    return Handled::Nothing;
                };
                self.mentioned.insert(path.as_str().to_owned());
                if self.overlays.get(&path) == Some(&text) {
                    return Handled::Nothing;
                }
                let mut change = k_sema::Change::new();
                change.set_file(path.clone(), text.as_str());
                project.host.apply_change(change);
                self.changed = Some(path.as_str().to_owned());
                self.overlays.insert(path, text);
                Handled::Changed
            }
            editor::Message::Close { uri } => {
                let Some(path) = self.path_of(&uri) else {
                    return Handled::Nothing;
                };
                if self.overlays.remove(&path).is_none() {
                    return Handled::Nothing;
                }
                let mut change = k_sema::Change::new();
                match self.disk.get(&path) {
                    Some(text) => change.set_file(path.clone(), text.as_str()),
                    None => change.remove_file(path.clone()),
                }
                project.host.apply_change(change);
                Handled::Changed
            }
            editor::Message::Select { uri, offset } => {
                let Some(path) = self.path_of(&uri) else {
                    return Handled::Nothing;
                };
                self.mentioned.insert(path.as_str().to_owned());
                let text = self
                    .overlays
                    .get(&path)
                    .or_else(|| self.disk.get(&path))
                    .cloned()
                    .unwrap_or_default();
                let byte = editor::utf16_to_byte(&text, offset);
                let state = self
                    .shared
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let found = state.build.as_ref().and_then(|b| {
                    let f = b.files.iter().find(|f| f.path == path.as_str())?;
                    f.previews
                        .iter()
                        .position(|p| p.range.0 <= byte && byte <= p.range.1)
                        .map(|i| (f.path.clone(), i))
                });
                drop(state);
                if let Some((file, index)) = found {
                    let mut state = self
                        .shared
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if state.selected.as_ref() != Some(&(file.clone(), index)) {
                        state.selected = Some((file.clone(), index));
                        drop(state);
                        self.shared
                            .broadcast(&json!({ "type": "select", "file": file, "index": index }));
                    }
                }
                Handled::Nothing
            }
            editor::Message::Shutdown => Handled::Stop,
            editor::Message::Invalid(problem) => {
                if let Some(o) = &self.editor {
                    o.send(&json!({ "type": "error", "uri": null, "message": problem, "line": null, "column": null }));
                }
                Handled::Nothing
            }
        }
    }

    /// The project path of a `file:` URI.
    fn path_of(&self, uri: &str) -> Option<ProjectPath> {
        let path = PathBuf::from(http::percent_decode(uri.strip_prefix("file://")?));
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let rel = path.strip_prefix(&self.root).ok()?;
        ProjectPath::new(&rel.to_string_lossy().replace('\\', "/")).ok()
    }

    /// The `file:` URI of a project path.
    fn uri_of(&self, path: &str) -> String {
        let full = self.root.join(path);
        format!(
            "file://{}",
            full.to_string_lossy()
                .replace('\\', "/")
                .replace(' ', "%20")
        )
    }

    /// Sends the editor an `error` about `uri` that isn't the program's.
    fn reply_error(&self, uri: &str, message: &str) {
        if let Some(o) = &self.editor {
            o.send(&json!({ "type": "error", "uri": uri, "message": message, "line": null, "column": null }));
        }
    }

    /// Rebuilds the previews from the current sources: a program without
    /// errors becomes the new build (control values kept by parameter name
    /// and type) and every page remounts; otherwise the last good build
    /// stays, under the first error.
    fn rebuild(&mut self, project: &mut crate::project::Project) {
        let analysis = project.host.analysis();
        let Ok(diagnostics) = analysis.diagnostics() else {
            return;
        };
        let files = analysis.files();
        let path_of = |id| {
            files
                .iter()
                .find(|(f, _)| *f == id)
                .map(|(_, p)| p.as_str().to_owned())
        };
        let mut errors: Vec<_> = diagnostics.iter().filter(|d| d.is_error()).collect();
        // The badge names the first error of the file changed last, else
        // the project's first.
        errors.sort_by_key(|d| path_of(d.span.file) != self.changed);
        if let Some(d) = errors.first() {
            let file = path_of(d.span.file);
            let (line, column) = analysis
                .text(d.span.file)
                .map_or((1, 1), |t| line_column(t, u32::from(d.span.range.start())));
            let badge = json!({
                "file": file,
                "message": format!("{}: {}", d.code, d.message),
                "line": line,
                "column": column,
                "count": errors.len(),
            });
            self.publish_error(badge, file.as_deref());
            return;
        }
        let options = Options {
            profile: Profile::Development,
            environment: Environment::Test,
            entry: Entry::Previews(None),
        };
        let output = match k_codegen::generate(&analysis, &options) {
            Ok(o) => o,
            Err(e) => {
                let badge = json!({ "file": null, "message": format!("the previews can't be built: {e}"), "line": null, "column": null, "count": 1 });
                self.publish_error(badge, None);
                return;
            }
        };
        // The groups' order: file path, then source order, as the bundle's.
        let mut with: Vec<(String, k_diagnostics::FileId)> = files
            .iter()
            .map(|(id, p)| (p.as_str().to_owned(), *id))
            .filter(|(p, _)| p.ends_with(".k"))
            .collect();
        with.sort();
        let mut out_files = Vec::new();
        let mut group = 0usize;
        for (path, id) in with {
            let Ok(list) = analysis.previews(id) else {
                return;
            };
            if list.is_empty() {
                continue;
            }
            let text = analysis.text(id).unwrap_or_default();
            let mut entries = Vec::new();
            for p in list {
                entries.push(PreviewEntry {
                    group,
                    label: p.label.clone(),
                    keyword: (u32::from(p.keyword.start()), u32::from(p.keyword.end())),
                    range: (u32::from(p.range.start()), u32::from(p.range.end())),
                    line: line_column(text, u32::from(p.keyword.start())).0,
                    params: p.params.clone(),
                });
                group += 1;
            }
            out_files.push(FileEntry {
                path,
                previews: entries,
            });
        }
        let id = self.next_build;
        self.next_build += 1;
        let build = Arc::new(Build {
            id,
            files: out_files,
            bundle: Arc::new(mounts::Bundle::new(&output)),
        });
        {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut values = BTreeMap::new();
            for f in &build.files {
                for p in &f.previews {
                    let key = (f.path.clone(), p.label.clone());
                    if let Some(kept) = state.values.get(&key) {
                        let next = controls::retain(kept, &p.params);
                        if !next.is_empty() {
                            values.insert(key, next);
                        }
                    }
                }
            }
            state.values = values;
            state.build = Some(Arc::clone(&build));
            state.error = None;
        }
        self.shared
            .broadcast(&json!({ "type": "build", "build": id }));
        if let Some(o) = &self.editor {
            o.send(&json!({ "type": "built", "build": id }));
            for path in &self.mentioned {
                o.send(&self.previews_message(&build, path));
            }
            for f in &build.files {
                if !self.mentioned.contains(&f.path) {
                    o.send(&self.previews_message(&build, &f.path));
                }
            }
        }
    }

    /// The editor's `previews` message for the file at `path`.
    fn previews_message(&self, build: &Build, path: &str) -> Value {
        let text = ProjectPath::new(path)
            .ok()
            .and_then(|p| self.overlays.get(&p).or_else(|| self.disk.get(&p)).cloned())
            .unwrap_or_default();
        let list: Vec<Value> = build
            .files
            .iter()
            .find(|f| f.path == path)
            .map(|f| {
                f.previews
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        json!({
                            "index": i,
                            "label": p.label,
                            "line": p.line,
                            "start": editor::byte_to_utf16(&text, p.keyword.0),
                            "end": editor::byte_to_utf16(&text, p.keyword.1),
                            "path": format!("/p/{}/{i}", f.path),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        json!({ "type": "previews", "uri": self.uri_of(path), "list": list })
    }

    /// Shows `badge` on every page and tells the editor; the last good
    /// build keeps serving.
    fn publish_error(&self, badge: Value, file: Option<&str>) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.error = Some(badge.clone());
        }
        self.shared
            .broadcast(&json!({ "type": "error", "error": badge }));
        if let Some(o) = &self.editor {
            o.send(&json!({
                "type": "error",
                "uri": file.map(|f| self.uri_of(f)),
                "message": badge["message"],
                "line": badge["line"],
                "column": badge["column"],
            }));
        }
    }
}

/// What a message did to the sources.
enum Handled {
    /// They changed: rebuild after the debounce.
    Changed,
    /// Nothing to rebuild.
    Nothing,
    /// Stop the server.
    Stop,
}

/// The 1-based line and column (in characters) of byte `offset` of `text`.
fn line_column(text: &str, offset: u32) -> (usize, usize) {
    let offset = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(text.len());
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

// ------------------------------------------------------------------ HTTP

/// The panel's page, script and styles.
const PANEL_HTML: &str = include_str!("preview/panel.html");
/// The panel's script.
const PANEL_JS: &str = include_str!("preview/panel.js");
/// The panel's styles.
const PANEL_CSS: &str = include_str!("preview/panel.css");

/// Accepts connections until the process ends, each on a thread.
fn serve(listener: &TcpListener, shared: &Arc<Shared>) {
    for stream in listener.incoming().flatten() {
        let shared = Arc::clone(shared);
        std::thread::spawn(move || exchange(stream, &shared));
    }
}

/// Answers one request.
fn exchange(stream: TcpStream, shared: &Arc<Shared>) {
    let Some(request) = http::read(&stream) else {
        return;
    };
    if request.method == "GET" && request.path == "/api/events" {
        if http::start_events(&stream).is_ok() {
            shared
                .streams
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(stream);
        }
        return;
    }
    let response = route(&request, shared);
    http::write(&stream, &response);
}

/// The response to a request other than the event stream.
fn route(r: &http::Request, shared: &Shared) -> http::Response {
    match (r.method.as_str(), r.path.as_str()) {
        ("GET", "/") => http::Response::ok("text/html; charset=utf-8", index_page(shared)),
        ("GET", "/_panel/panel.js") => {
            http::Response::ok("text/javascript; charset=utf-8", PANEL_JS)
        }
        ("GET", "/_panel/panel.css") => http::Response::ok("text/css; charset=utf-8", PANEL_CSS),
        ("GET", "/api/state") => http::Response::json(200, &state_json(shared)),
        ("POST", "/api/values") => set_values(shared, &r.body),
        ("POST", "/api/mount") => mount(shared, &r.body),
        ("GET", p) if p.starts_with("/p/") => {
            http::Response::ok("text/html; charset=utf-8", PANEL_HTML)
        }
        ("GET", _) => http::Response::text(404, "not found"),
        _ => http::Response::text(405, "method not allowed"),
    }
}

/// `/`: every preview of the project (or under the path given), grouped
/// by file, linking to their pages.
fn index_page(shared: &Shared) -> String {
    let state = shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut body = String::new();
    if let Some(e) = &state.error {
        body.push_str(&format!(
            "<p class=\"badge\" role=\"alert\">{}</p>\n",
            escape(&error_text(e))
        ));
    }
    let listed: Vec<&FileEntry> = state
        .build
        .iter()
        .flat_map(|b| b.files.iter())
        .filter(|f| shared.under.is_empty() || f.path.starts_with(&shared.under))
        .collect();
    if listed.is_empty() {
        body.push_str("<p>No previews here yet. Write one with <code>preview { … }</code> in the file of a view.</p>\n");
    }
    for f in listed {
        body.push_str(&format!(
            "<section><h2><a href=\"/p/{0}/0?mode=grid\">{1}</a></h2>\n<ul>\n",
            escape(&f.path),
            escape(&f.path)
        ));
        for (i, p) in f.previews.iter().enumerate() {
            body.push_str(&format!(
                "<li><a href=\"/p/{}/{i}\">{}</a> <span class=\"line\">line {}</span></li>\n",
                escape(&f.path),
                escape(&p.label),
                p.line
            ));
        }
        body.push_str("</ul></section>\n");
    }
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Previews · {name}</title>\n<link rel=\"stylesheet\" href=\"/_panel/panel.css\">\n</head>\n\
         <body class=\"index\">\n<h1>Previews of {name}</h1>\n{body}\
         <script>new EventSource('/api/events').onmessage = () => location.reload();</script>\n\
         </body>\n</html>\n",
        name = escape(&shared.name)
    )
}

/// The badge's text: `file:line: message`.
fn error_text(e: &Value) -> String {
    let file = e["file"].as_str().unwrap_or("");
    match e["line"].as_u64() {
        Some(l) => format!("{file}:{l}: {}", e["message"].as_str().unwrap_or("")),
        None => e["message"].as_str().unwrap_or("").to_owned(),
    }
}

/// `text` escaped for HTML.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `GET /api/state`: the build, its previews with their controls and
/// values, the error badge and the editor's selection.
fn state_json(shared: &Shared) -> Value {
    let state = shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let files: Vec<Value> = state
        .build
        .iter()
        .flat_map(|b| b.files.iter())
        .map(|f| {
            let previews: Vec<Value> = f
                .previews
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let kept = state
                        .values
                        .get(&(f.path.clone(), p.label.clone()))
                        .cloned()
                        .unwrap_or_default();
                    preview_json(i, p, &kept)
                })
                .collect();
            json!({ "path": f.path, "previews": previews })
        })
        .collect();
    json!({
        "name": shared.name,
        "under": shared.under,
        "build": state.build.as_ref().map(|b| b.id),
        "error": state.error,
        "selected": state.selected.as_ref().map(|(f, i)| json!({ "file": f, "index": i })),
        "files": files,
    })
}

/// One preview in `/api/state`.
fn preview_json(index: usize, p: &PreviewEntry, kept: &BTreeMap<String, Kept>) -> Value {
    let params: Vec<Value> = p
        .params
        .iter()
        .map(|q| {
            json!({
                "name": q.name,
                "type": q.type_name,
                "control": controls::control_json(&q.control),
                "default": controls::default_json(q),
                "defaultSource": q.default_source,
            })
        })
        .collect();
    json!({
        "index": index,
        "group": p.group,
        "label": p.label,
        "line": p.line,
        "params": params,
        "values": controls::shown(kept, &p.params),
    })
}

/// `POST /api/values {group, values: {name: value|null}, reset?}`: sets
/// the control values of a preview (`null` is the default; `reset`
/// restores every default first). Answers its values, or `400` with
/// `errors` by parameter name.
fn set_values(shared: &Shared, body: &[u8]) -> http::Response {
    let Ok(req) = serde_json::from_slice::<Value>(body) else {
        return http::Response::json(400, &json!({ "error": "the body isn't JSON" }));
    };
    let Some(group) = req["group"].as_u64().and_then(|g| usize::try_from(g).ok()) else {
        return http::Response::json(400, &json!({ "error": "`group` is missing" }));
    };
    let mut state = shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(build) = state.build.clone() else {
        return http::Response::json(409, &json!({ "error": "there is no build yet" }));
    };
    let Some((file, p)) = build.entry(group) else {
        return http::Response::json(404, &json!({ "error": "no such preview" }));
    };
    let key = (file.path.clone(), p.label.clone());
    let mut kept = if req["reset"].as_bool() == Some(true) {
        BTreeMap::new()
    } else {
        state.values.get(&key).cloned().unwrap_or_default()
    };
    let mut errors = serde_json::Map::new();
    if let Some(values) = req["values"].as_object() {
        for (name, v) in values {
            let Some(param) = p.params.iter().find(|q| &q.name == name) else {
                errors.insert(name.clone(), json!("the preview has no such parameter"));
                continue;
            };
            match controls::decode(&param.control, v) {
                Ok(Value::Null) => {
                    kept.remove(name);
                }
                Ok(v) => {
                    kept.insert(
                        name.clone(),
                        Kept {
                            type_name: param.type_name.clone(),
                            value: v,
                        },
                    );
                }
                Err(e) => {
                    errors.insert(name.clone(), json!(e));
                }
            }
        }
    }
    if !errors.is_empty() {
        return http::Response::json(400, &json!({ "errors": errors }));
    }
    let shown = controls::shown(&kept, &p.params);
    state.values.insert(key, kept);
    http::Response::json(200, &json!({ "values": shown }))
}

/// `POST /api/mount {group, slot}`: a fresh mount of a preview of the
/// current build for a slot of the page (the previous one's stops).
/// Answers `{url, build}`: the frame's URL, to which the page appends the
/// frame's configuration as its fragment.
fn mount(shared: &Shared, body: &[u8]) -> http::Response {
    let Ok(req) = serde_json::from_slice::<Value>(body) else {
        return http::Response::json(400, &json!({ "error": "the body isn't JSON" }));
    };
    let group = req["group"].as_u64().and_then(|g| usize::try_from(g).ok());
    let slot = req["slot"].as_str().unwrap_or("main").to_owned();
    let build = shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .build
        .clone();
    let (Some(build), Some(group)) = (build, group) else {
        return http::Response::json(409, &json!({ "error": "there is no build yet" }));
    };
    if build.entry(group).is_none() {
        return http::Response::json(404, &json!({ "error": "no such preview" }));
    }
    let started = shared
        .mounts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .mount(&build.bundle, group, &slot);
    match started {
        Ok(url) => http::Response::json(200, &json!({ "url": url, "build": build.id })),
        Err(e) => http::Response::json(
            500,
            &json!({ "error": format!("the preview couldn't start: {e}") }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_columns_count_from_one() {
        assert_eq!(line_column("ab\ncd", 0), (1, 1));
        assert_eq!(line_column("ab\ncd", 4), (2, 2));
        assert_eq!(line_column("é\nx", 3), (2, 1));
        assert_eq!(line_column("ab", 99), (1, 3));
    }
}
