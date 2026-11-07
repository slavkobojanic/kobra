//! The message loop (`docs/compiler/lsp.md`, "Server").
//!
//! # Threads
//!
//! * A **reader** thread reads frames from the input and sends them to the
//!   main loop.
//! * The **main loop** owns the [`Workspace`]: it applies document changes
//!   to the projects' hosts at once and in order, answers `initialize`,
//!   `shutdown` and the notifications itself, and hands every other request
//!   to the worker pool with a snapshot ([`handlers::Snap`]) taken when the
//!   request arrived. It is the only writer of the output.
//! * **Workers** run the handlers and send results back to the main loop.
//!   A worker that panics (a bug) is contained; its request gets an
//!   `InternalError` response.
//!
//! # Staleness and cancellation
//!
//! A snapshot answers `Cancelled` as soon as a later change reaches its
//! project; the request then gets `ContentModified`, and the client asks
//! again. `$/cancelRequest` marks a request: when its result arrives it is
//! replaced by `RequestCancelled`.
//!
//! # Diagnostics
//!
//! Every change marks its project dirty and (re)starts a debounce timer
//! ([`Config::debounce`]). When it fires, a worker computes the project's
//! diagnostics on a snapshot; the main loop publishes them for every file
//! whose list changed (clearing files that no longer have any) unless the
//! project changed again in the meantime.

pub mod handlers;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use k_diagnostics::FileId;
use k_sema::{Cancelled, Options, ProjectPath};
use lsp_types as lsp;
use lsp_types::Url;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::line_index::LineIndex;
use crate::transport::{self, Frame, Incoming, codes};
use crate::workspace::Workspace;
use handlers::{ClientCaps, Goto, Snap};

/// Settings of a server.
#[derive(Clone, Debug)]
pub struct Config {
    /// How long the input must be quiet after a change before diagnostics
    /// are computed.
    pub debounce: Duration,
    /// The number of worker threads.
    pub workers: usize,
    /// The number of threads each analysis parses with; `None` for every
    /// core.
    pub analysis_threads: Option<NonZeroUsize>,
}

impl Default for Config {
    /// 30 ms of debounce, four workers, every core for parsing.
    fn default() -> Config {
        Config {
            debounce: Duration::from_millis(30),
            workers: 4,
            analysis_threads: None,
        }
    }
}

/// The stack size of worker threads: the parser may run on them (a
/// snapshot parses a file on first use), and it needs about 2 MiB on the
/// deepest input in debug builds.
const WORKER_STACK: usize = 16 << 20;

/// A job for the worker pool.
type Job = Box<dyn FnOnce() + Send>;

/// What the main loop reacts to.
enum Event {
    /// A frame from the reader.
    Frame(Frame),
    /// The input failed.
    ReadFailed,
    /// A request's result, from a worker.
    Done {
        /// The request id.
        id: Value,
        /// The result or the error.
        outcome: Outcome,
    },
    /// A project's diagnostics, from a worker.
    Diagnostics {
        /// The project's index in the workspace.
        project: usize,
        /// The project's generation they were computed for.
        generation: u64,
        /// The diagnostics of every file that has some.
        files: Vec<(Url, Vec<lsp::Diagnostic>)>,
    },
}

/// A request's result.
enum Outcome {
    /// The result.
    Ok(Value),
    /// An error code and message.
    Err(i64, String),
}

impl From<Cancelled> for Outcome {
    /// The snapshot went stale: the client may retry.
    fn from(_: Cancelled) -> Outcome {
        Outcome::Err(codes::CONTENT_MODIFIED, "the document changed".to_owned())
    }
}

/// A fixed pool of worker threads.
struct Pool {
    /// The queue the workers take jobs from; dropping it stops them.
    jobs: mpsc::Sender<Job>,
}

impl Pool {
    /// Starts `n` workers (at least one; if no thread can be started, jobs
    /// run on the caller's thread).
    fn new(n: usize) -> Pool {
        let (jobs, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..n.max(1) {
            let rx = Arc::clone(&rx);
            let _ = std::thread::Builder::new()
                .name(format!("k-lsp-worker-{i}"))
                .stack_size(WORKER_STACK)
                .spawn(move || {
                    loop {
                        let job = match rx.lock() {
                            Ok(guard) => guard.recv(),
                            Err(poisoned) => poisoned.into_inner().recv(),
                        };
                        match job {
                            Ok(job) => job(),
                            Err(_) => return,
                        }
                    }
                });
        }
        Pool { jobs }
    }

    /// Runs `job` on a worker.
    fn run(&self, job: Job) {
        if let Err(mpsc::SendError(job)) = self.jobs.send(job) {
            job();
        }
    }
}

/// Runs a server reading `input` and writing `output` until `exit` or the
/// end of the input; returns the exit code (0 after `shutdown`, else 1).
///
/// Input is read on a thread of its own and requests are answered by
/// [`Config::workers`] worker threads, so `input` must be `Send`; `output`
/// is only written from the calling thread.
///
/// # Examples
///
/// A whole session over in-memory streams:
///
/// ```
/// use std::io::Cursor;
/// use k_lsp::transport::{notification, request, write_message};
/// use serde_json::json;
///
/// let mut input = Vec::new();
/// let initialize = json!({ "processId": null, "rootUri": null, "capabilities": {} });
/// write_message(&mut input, &request(&json!(1), "initialize", initialize))?;
/// write_message(&mut input, &notification("initialized", json!({})))?;
/// write_message(&mut input, &request(&json!(2), "shutdown", json!(null)))?;
/// write_message(&mut input, &notification("exit", json!(null)))?;
///
/// let mut output = Vec::new();
/// let code = k_lsp::serve(Cursor::new(input), &mut output, k_lsp::Config::default());
/// assert_eq!(code, 0);
///
/// // Split the output into its messages and find the `initialize` response.
/// let text = String::from_utf8(output).expect("UTF-8");
/// let mut messages = Vec::new();
/// let mut rest = text.as_str();
/// while let Some((header, body)) = rest.split_once("\r\n\r\n") {
///     let length: usize = header["Content-Length: ".len()..].parse().expect("length");
///     messages.push(serde_json::from_str::<serde_json::Value>(&body[..length])?);
///     rest = &body[length..];
/// }
/// let initialized = messages.iter().find(|m| m["id"] == json!(1)).expect("a response");
/// assert!(initialized["result"]["capabilities"]["hoverProvider"].is_boolean());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn serve<R, W>(input: R, mut output: W, config: Config) -> i32
where
    R: BufRead + Send + 'static,
    W: Write,
{
    let (tx, rx) = mpsc::channel::<Event>();
    let reader_tx = tx.clone();
    let spawned = std::thread::Builder::new()
        .name("k-lsp-reader".to_owned())
        .spawn(move || {
            let mut input = input;
            loop {
                match transport::read_frame(&mut input) {
                    Ok(Frame::Eof) => {
                        let _ = reader_tx.send(Event::Frame(Frame::Eof));
                        return;
                    }
                    Ok(frame) => {
                        if reader_tx.send(Event::Frame(frame)).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = reader_tx.send(Event::ReadFailed);
                        return;
                    }
                }
            }
        });
    if spawned.is_err() {
        return 1;
    }
    let mut server = Server {
        pool: Pool::new(config.workers),
        config,
        tx,
        workspace: None,
        caps: ClientCaps::default(),
        shutdown: false,
        cancelled: HashSet::new(),
        pending: HashSet::new(),
        dirty: HashSet::new(),
        due: None,
        published: HashMap::new(),
        next_id: 0,
    };
    loop {
        let event = match server.due {
            Some(due) => match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(e) => Some(e),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return 1,
            },
            None => match rx.recv() {
                Ok(e) => Some(e),
                Err(_) => return 1,
            },
        };
        let step = match event {
            Some(event) => server.on_event(event, &mut output),
            None => Step::Continue,
        };
        match step {
            Step::Continue => {}
            Step::Exit(code) => return code,
        }
        if server.due.is_some_and(|d| d <= Instant::now()) {
            server.flush_diagnostics();
        }
    }
}

/// What the loop does after an event.
enum Step {
    /// Go on.
    Continue,
    /// Stop with this exit code.
    Exit(i32),
}

/// The main loop's state.
struct Server {
    /// The server's configuration.
    config: Config,
    /// The workers answering requests on snapshots.
    pool: Pool,
    /// For handing results back to the loop.
    tx: mpsc::Sender<Event>,
    /// `None` until `initialize`.
    workspace: Option<Workspace>,
    /// What the client said it supports at `initialize`.
    caps: ClientCaps,
    /// Whether `shutdown` was received; `exit` then exits 0 instead of 1.
    shutdown: bool,
    /// Requests in flight, by id key.
    pending: HashSet<String>,
    /// Requests the client cancelled, by id key.
    cancelled: HashSet<String>,
    /// Projects whose diagnostics are out of date.
    dirty: HashSet<usize>,
    /// When the debounce timer fires.
    due: Option<Instant>,
    /// What was last published per URL, and for which project.
    published: HashMap<Url, (usize, Vec<lsp::Diagnostic>)>,
    /// The id of the next request the server sends.
    next_id: u64,
}

/// The key of a request id in the pending and cancelled sets.
fn key(id: &Value) -> String {
    id.to_string()
}

/// Writes a message; a write error ends the session.
fn send(output: &mut impl Write, message: &Value) -> bool {
    transport::write_message(output, message).is_ok()
}

impl Server {
    /// Reacts to one event.
    fn on_event(&mut self, event: Event, out: &mut impl Write) -> Step {
        match event {
            Event::Frame(Frame::Eof) | Event::ReadFailed => {
                Step::Exit(if self.shutdown { 0 } else { 1 })
            }
            Event::Frame(Frame::Malformed { code, message, id }) => {
                if send(out, &transport::error_response(id.as_ref(), code, &message)) {
                    Step::Continue
                } else {
                    Step::Exit(1)
                }
            }
            Event::Frame(Frame::Message(message)) => self.on_message(message, out),
            Event::Done { id, outcome } => {
                let k = key(&id);
                self.pending.remove(&k);
                let message = if self.cancelled.remove(&k) {
                    transport::error_response(
                        Some(&id),
                        codes::REQUEST_CANCELLED,
                        "the request was cancelled",
                    )
                } else {
                    match outcome {
                        Outcome::Ok(v) => transport::response(&id, v),
                        Outcome::Err(code, message) => {
                            transport::error_response(Some(&id), code, &message)
                        }
                    }
                };
                if send(out, &message) {
                    Step::Continue
                } else {
                    Step::Exit(1)
                }
            }
            Event::Diagnostics {
                project,
                generation,
                files,
            } => {
                if self.publish(project, generation, files, out) {
                    Step::Continue
                } else {
                    Step::Exit(1)
                }
            }
        }
    }

    /// Reacts to a message from the client.
    fn on_message(&mut self, message: Incoming, out: &mut impl Write) -> Step {
        match message {
            Incoming::Response { .. } => Step::Continue,
            Incoming::Notification { method, params } => self.on_notification(&method, params, out),
            Incoming::Request { id, method, params } => {
                let reply = self.on_request(id.clone(), &method, params);
                match reply {
                    Some(Outcome::Ok(v)) => {
                        if send(out, &transport::response(&id, v)) {
                            Step::Continue
                        } else {
                            Step::Exit(1)
                        }
                    }
                    Some(Outcome::Err(code, message)) => {
                        if send(out, &transport::error_response(Some(&id), code, &message)) {
                            Step::Continue
                        } else {
                            Step::Exit(1)
                        }
                    }
                    None => Step::Continue,
                }
            }
        }
    }

    /// Handles a request: answers at once (`Some`) or hands it to a
    /// worker (`None`).
    fn on_request(&mut self, id: Value, method: &str, params: Value) -> Option<Outcome> {
        if method == "initialize" {
            if self.workspace.is_some() {
                return Some(Outcome::Err(
                    codes::INVALID_REQUEST,
                    "already initialized".to_owned(),
                ));
            }
            return Some(self.initialize(params));
        }
        if self.workspace.is_none() {
            return Some(Outcome::Err(
                codes::SERVER_NOT_INITIALIZED,
                "the server is not initialized".to_owned(),
            ));
        }
        if self.shutdown {
            return Some(Outcome::Err(
                codes::INVALID_REQUEST,
                "the server is shutting down".to_owned(),
            ));
        }
        match method {
            "shutdown" => {
                self.shutdown = true;
                Some(Outcome::Ok(Value::Null))
            }
            "textDocument/hover" => self.position_request::<lsp::HoverParams, _>(
                id,
                params,
                |p| p.text_document_position_params,
                |snap, file, pos, _| handlers::hover(snap, file, pos),
            ),
            "textDocument/definition" => self.position_request::<lsp::GotoDefinitionParams, _>(
                id,
                params,
                |p| p.text_document_position_params,
                |snap, file, pos, _| handlers::goto(snap, file, pos, Goto::Definition),
            ),
            "textDocument/typeDefinition" => self.position_request::<lsp::GotoDefinitionParams, _>(
                id,
                params,
                |p| p.text_document_position_params,
                |snap, file, pos, _| handlers::goto(snap, file, pos, Goto::TypeDefinition),
            ),
            "textDocument/implementation" => self.implementation(id, params),
            "textDocument/documentHighlight" => self
                .position_request::<lsp::DocumentHighlightParams, _>(
                    id,
                    params,
                    |p| p.text_document_position_params,
                    |snap, file, pos, _| handlers::highlights(snap, file, pos),
                ),
            "textDocument/completion" => self.position_request::<lsp::CompletionParams, _>(
                id,
                params,
                |p| p.text_document_position,
                |snap, file, pos, _| handlers::completion(snap, file, pos),
            ),
            "textDocument/signatureHelp" => self.position_request::<lsp::SignatureHelpParams, _>(
                id,
                params,
                |p| p.text_document_position_params,
                |snap, file, pos, _| handlers::signature_help(snap, file, pos),
            ),
            "textDocument/references" => {
                let include = params
                    .pointer("/context/includeDeclaration")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                self.position_request::<lsp::ReferenceParams, _>(
                    id,
                    params,
                    |p| p.text_document_position,
                    move |snap, file, pos, _| handlers::references(snap, file, pos, include),
                )
            }
            "textDocument/prepareRename" => self
                .position_request_fallible::<lsp::TextDocumentPositionParams, _>(
                    id,
                    params,
                    |p| p,
                    |snap, file, pos, _| handlers::prepare_rename(snap, file, pos),
                ),
            "textDocument/rename" => {
                let new_name = params
                    .get("newName")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                self.position_request_fallible::<lsp::RenameParams, _>(
                    id,
                    params,
                    |p| p.text_document_position,
                    move |snap, file, pos, _| handlers::rename(snap, file, pos, &new_name),
                )
            }
            "textDocument/documentSymbol" => self
                .document_request::<lsp::DocumentSymbolParams, _, _>(
                    id,
                    params,
                    |p| (p.text_document.uri.clone(), ()),
                    |snap, file, ()| handlers::document_symbols(snap, file),
                ),
            "textDocument/codeAction" => self.document_request::<lsp::CodeActionParams, _, _>(
                id,
                params,
                |p| (p.text_document.uri.clone(), p.range),
                handlers::code_actions,
            ),
            "textDocument/inlayHint" => self.document_request::<lsp::InlayHintParams, _, _>(
                id,
                params,
                |p| (p.text_document.uri.clone(), p.range),
                handlers::inlay_hints,
            ),
            "workspace/symbol" => {
                let query = match serde_json::from_value::<lsp::WorkspaceSymbolParams>(params) {
                    Ok(p) => p.query,
                    Err(e) => return Some(Outcome::Err(codes::INVALID_PARAMS, e.to_string())),
                };
                let snaps: Vec<Snap> = self
                    .workspace
                    .iter()
                    .flat_map(|w| w.projects.iter())
                    .map(|p| Snap::new(p, self.caps))
                    .collect();
                self.spawn(id, move || {
                    match handlers::workspace_symbols(&snaps, &query) {
                        Ok(found) => to_value(&found),
                        Err(c) => c.into(),
                    }
                });
                None
            }
            _ => Some(Outcome::Err(
                codes::METHOD_NOT_FOUND,
                format!("unknown method `{method}`"),
            )),
        }
    }

    /// Runs `job` for request `id` on a worker, containing panics.
    fn spawn(&mut self, id: Value, job: impl FnOnce() -> Outcome + Send + 'static) {
        self.pending.insert(key(&id));
        let tx = self.tx.clone();
        self.pool.run(Box::new(move || {
            let outcome = catch_unwind(AssertUnwindSafe(job)).unwrap_or_else(|_| {
                Outcome::Err(
                    codes::INTERNAL_ERROR,
                    "the request failed in the server".to_owned(),
                )
            });
            let _ = tx.send(Event::Done { id, outcome });
        }));
    }

    /// The snapshot and file of the document at `uri`; `None` when it is in
    /// no project or not an analysis input.
    fn snap_for(&mut self, uri: &Url) -> Option<(Snap, FileId)> {
        let path = uri.to_file_path().ok()?;
        let workspace = self.workspace.as_mut()?;
        let (index, project_path) = workspace.locate(&path)?;
        let project = workspace.project(index)?;
        let snap = Snap::new(project, self.caps);
        let file = snap.analysis.file_id(&project_path)?;
        Some((snap, file))
    }

    /// A request on a document position whose handler answers `Option<R>`
    /// (`null` when absent).
    fn position_request<P, R>(
        &mut self,
        id: Value,
        params: Value,
        position: fn(P) -> lsp::TextDocumentPositionParams,
        handler: impl FnOnce(&Snap, FileId, lsp::Position, ()) -> Result<Option<R>, Cancelled>
        + Send
        + 'static,
    ) -> Option<Outcome>
    where
        P: DeserializeOwned,
        R: Serialize,
    {
        let p: P = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return Some(Outcome::Err(codes::INVALID_PARAMS, e.to_string())),
        };
        let at = position(p);
        let Some((snap, file)) = self.snap_for(&at.text_document.uri) else {
            return Some(Outcome::Ok(Value::Null));
        };
        self.spawn(id, move || match handler(&snap, file, at.position, ()) {
            Ok(Some(r)) => to_value(&r),
            Ok(None) => Outcome::Ok(Value::Null),
            Err(c) => c.into(),
        });
        None
    }

    /// `textDocument/implementation`: in the document's project and the
    /// other members of its workspace.
    fn implementation(&mut self, id: Value, params: Value) -> Option<Outcome> {
        let p: lsp::GotoDefinitionParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return Some(Outcome::Err(codes::INVALID_PARAMS, e.to_string())),
        };
        let at = p.text_document_position_params;
        let Ok(path) = at.text_document.uri.to_file_path() else {
            return Some(Outcome::Ok(Value::Null));
        };
        let Some(workspace) = self.workspace.as_mut() else {
            return Some(Outcome::Ok(Value::Null));
        };
        let Some((index, project_path)) = workspace.locate(&path) else {
            return Some(Outcome::Ok(Value::Null));
        };
        let caps = self.caps;
        let Some(project) = workspace.project(index) else {
            return Some(Outcome::Ok(Value::Null));
        };
        let snap = Snap::new(project, caps);
        let Some(file) = snap.analysis.file_id(&project_path) else {
            return Some(Outcome::Ok(Value::Null));
        };
        let siblings: Vec<Snap> = workspace
            .siblings(index)
            .into_iter()
            .filter_map(|i| workspace.project(i))
            .map(|p| Snap::new(p, caps))
            .collect();
        self.spawn(id, move || {
            match handlers::implementation_across(&snap, &siblings, file, at.position) {
                Ok(Some(r)) => to_value(&r),
                Ok(None) => Outcome::Ok(Value::Null),
                Err(c) => c.into(),
            }
        });
        None
    }

    /// A request on a document position whose handler may refuse with a
    /// message (prepare rename, rename).
    fn position_request_fallible<P, R>(
        &mut self,
        id: Value,
        params: Value,
        position: fn(P) -> lsp::TextDocumentPositionParams,
        handler: impl FnOnce(&Snap, FileId, lsp::Position, ()) -> Result<Result<R, String>, Cancelled>
        + Send
        + 'static,
    ) -> Option<Outcome>
    where
        P: DeserializeOwned,
        R: Serialize,
    {
        let p: P = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return Some(Outcome::Err(codes::INVALID_PARAMS, e.to_string())),
        };
        let at = position(p);
        let Some((snap, file)) = self.snap_for(&at.text_document.uri) else {
            return Some(Outcome::Err(
                codes::REQUEST_FAILED,
                "the document is not part of a K project".to_owned(),
            ));
        };
        self.spawn(id, move || match handler(&snap, file, at.position, ()) {
            Ok(Ok(r)) => to_value(&r),
            Ok(Err(reason)) => Outcome::Err(codes::REQUEST_FAILED, reason),
            Err(c) => c.into(),
        });
        None
    }

    /// A request on a whole document (or a range of it).
    fn document_request<P, A, R>(
        &mut self,
        id: Value,
        params: Value,
        target: fn(&P) -> (Url, A),
        handler: impl FnOnce(&Snap, FileId, A) -> Result<Option<R>, Cancelled> + Send + 'static,
    ) -> Option<Outcome>
    where
        P: DeserializeOwned,
        A: Send + 'static,
        R: Serialize,
    {
        let p: P = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return Some(Outcome::Err(codes::INVALID_PARAMS, e.to_string())),
        };
        let (uri, arg) = target(&p);
        let Some((snap, file)) = self.snap_for(&uri) else {
            return Some(Outcome::Ok(Value::Null));
        };
        self.spawn(id, move || match handler(&snap, file, arg) {
            Ok(Some(r)) => to_value(&r),
            Ok(None) => Outcome::Ok(Value::Null),
            Err(c) => c.into(),
        });
        None
    }

    /// `initialize`: reads the folders and capabilities, loads the projects
    /// and answers the server's capabilities.
    fn initialize(&mut self, params: Value) -> Outcome {
        let params: lsp::InitializeParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return Outcome::Err(codes::INVALID_PARAMS, e.to_string()),
        };
        let mut folders: Vec<PathBuf> = params
            .workspace_folders
            .iter()
            .flatten()
            .filter_map(|f| f.uri.to_file_path().ok())
            .collect();
        #[allow(deprecated, reason = "older clients send only rootUri or rootPath")]
        if folders.is_empty() {
            if let Some(path) = params.root_uri.as_ref().and_then(|u| u.to_file_path().ok()) {
                folders.push(path);
            } else if let Some(path) = &params.root_path {
                folders.push(PathBuf::from(path));
            }
        }
        let caps = &params.capabilities;
        let edit = caps
            .workspace
            .as_ref()
            .and_then(|w| w.workspace_edit.as_ref());
        self.caps = ClientCaps {
            rename_files: edit.is_some_and(|e| {
                e.document_changes == Some(true)
                    && e.resource_operations
                        .as_ref()
                        .is_some_and(|ops| ops.contains(&lsp::ResourceOperationKind::Rename))
            }),
            hierarchical_symbols: caps
                .text_document
                .as_ref()
                .and_then(|t| t.document_symbol.as_ref())
                .and_then(|d| d.hierarchical_document_symbol_support)
                .unwrap_or(false),
            watch_files: caps
                .workspace
                .as_ref()
                .and_then(|w| w.did_change_watched_files.as_ref())
                .and_then(|d| d.dynamic_registration)
                .unwrap_or(false),
        };
        let mut options = Options::new();
        if let Some(n) = self.config.analysis_threads {
            options = options.with_threads(n);
        }
        let workspace = Workspace::new(&folders, options);
        for i in 0..workspace.projects.len() {
            self.dirty.insert(i);
        }
        self.workspace = Some(workspace);
        to_value(&lsp::InitializeResult {
            capabilities: capabilities(),
            server_info: Some(lsp::ServerInfo {
                name: "k".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
        })
    }

    /// Handles a notification.
    fn on_notification(&mut self, method: &str, params: Value, out: &mut impl Write) -> Step {
        if method == "exit" {
            return Step::Exit(if self.shutdown { 0 } else { 1 });
        }
        if self.workspace.is_none() {
            return Step::Continue;
        }
        match method {
            "initialized" => {
                // Diagnostics of every project, as soon as possible.
                self.due = Some(Instant::now());
                if self.caps.watch_files {
                    let id = json!(format!("k-watch-{}", self.next_id));
                    self.next_id += 1;
                    let watchers: Vec<Value> = ["**/*.k", "**/k.toml", "**/*.lock"]
                        .iter()
                        .map(|g| json!({ "globPattern": g }))
                        .collect();
                    let params = json!({ "registrations": [{
                        "id": "k-watch-files",
                        "method": "workspace/didChangeWatchedFiles",
                        "registerOptions": { "watchers": watchers },
                    }]});
                    if !send(
                        out,
                        &transport::request(&id, "client/registerCapability", params),
                    ) {
                        return Step::Exit(1);
                    }
                }
            }
            "textDocument/didOpen" => {
                if let Ok(p) = serde_json::from_value::<lsp::DidOpenTextDocumentParams>(params) {
                    let doc = p.text_document;
                    self.with_document(&doc.uri, |project, path| {
                        project.open(path, doc.version, doc.text.clone());
                    });
                }
            }
            "textDocument/didChange" => {
                if let Ok(p) = serde_json::from_value::<lsp::DidChangeTextDocumentParams>(params) {
                    let version = p.text_document.version;
                    let changes = p.content_changes;
                    self.with_document(&p.text_document.uri, |project, path| {
                        let mut text = project
                            .document(&path)
                            .map(str::to_owned)
                            .or_else(|| {
                                let a = project.analysis();
                                a.file_id(&path).and_then(|f| a.text(f)).map(str::to_owned)
                            })
                            .unwrap_or_default();
                        for change in changes.clone() {
                            apply_change(&mut text, change);
                        }
                        project.update(path, version, text);
                    });
                }
            }
            "textDocument/didClose" => {
                if let Ok(p) = serde_json::from_value::<lsp::DidCloseTextDocumentParams>(params) {
                    self.with_document(&p.text_document.uri, |project, path| project.close(&path));
                }
            }
            "workspace/didChangeWatchedFiles" => {
                if let Ok(p) = serde_json::from_value::<lsp::DidChangeWatchedFilesParams>(params) {
                    for change in p.changes {
                        self.with_document(&change.uri, |project, path| project.reload(&path));
                    }
                }
            }
            "$/cancelRequest" => {
                if let Some(id) = params.get("id") {
                    let k = key(id);
                    if self.pending.contains(&k) {
                        self.cancelled.insert(k);
                    }
                }
            }
            // `textDocument/didSave` (the text is already known), traces,
            // configuration and anything unknown: nothing to do.
            _ => {}
        }
        Step::Continue
    }

    /// Runs `f` on the project of the document at `uri` and marks the
    /// project dirty.
    fn with_document(
        &mut self,
        uri: &Url,
        mut f: impl FnMut(&mut crate::workspace::Project, ProjectPath),
    ) {
        let Ok(path) = uri.to_file_path() else { return };
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        // A library file is in its own project and mounted in every member
        // that depends on it (40-workspaces-and-services.md).
        for (index, project_path) in workspace.locate_all(&path) {
            let Some(project) = workspace.project_mut(index) else {
                continue;
            };
            f(project, project_path);
            self.dirty.insert(index);
        }
        self.due = Some(Instant::now() + self.config.debounce);
    }

    /// Starts the diagnostics of every dirty project on workers.
    fn flush_diagnostics(&mut self) {
        self.due = None;
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        for project in std::mem::take(&mut self.dirty) {
            let Some(p) = workspace.project(project) else {
                continue;
            };
            let snap = Snap::new(p, self.caps);
            let generation = p.generation;
            let tx = self.tx.clone();
            self.pool.run(Box::new(move || {
                let result =
                    catch_unwind(AssertUnwindSafe(|| handlers::project_diagnostics(&snap)));
                if let Ok(Ok(files)) = result {
                    let _ = tx.send(Event::Diagnostics {
                        project,
                        generation,
                        files,
                    });
                }
            }));
        }
    }

    /// Publishes a project's diagnostics: every file whose list changed,
    /// and an empty list for files that had some. Results for an older
    /// generation are dropped (newer ones are on their way).
    fn publish(
        &mut self,
        project: usize,
        generation: u64,
        files: Vec<(Url, Vec<lsp::Diagnostic>)>,
        out: &mut impl Write,
    ) -> bool {
        let current = self
            .workspace
            .as_ref()
            .and_then(|w| w.project(project))
            .map(|p| p.generation);
        if current != Some(generation) {
            return true;
        }
        let fresh: HashSet<Url> = files.iter().map(|(u, _)| u.clone()).collect();
        let stale: Vec<Url> = self
            .published
            .iter()
            .filter(|(u, (p, list))| *p == project && !list.is_empty() && !fresh.contains(*u))
            .map(|(u, _)| u.clone())
            .collect();
        let mut messages = Vec::new();
        for url in stale {
            self.published.insert(url.clone(), (project, Vec::new()));
            messages.push((url, Vec::new()));
        }
        for (url, list) in files {
            if self
                .published
                .get(&url)
                .is_some_and(|(_, old)| *old == list)
            {
                continue;
            }
            self.published.insert(url.clone(), (project, list.clone()));
            messages.push((url, list));
        }
        for (uri, diagnostics) in messages {
            let params = lsp::PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: None,
            };
            let Ok(params) = serde_json::to_value(params) else {
                continue;
            };
            if !send(
                out,
                &transport::notification("textDocument/publishDiagnostics", params),
            ) {
                return false;
            }
        }
        true
    }
}

/// Serializes a result.
fn to_value(value: &impl Serialize) -> Outcome {
    match serde_json::to_value(value) {
        Ok(v) => Outcome::Ok(v),
        Err(e) => Outcome::Err(codes::INTERNAL_ERROR, e.to_string()),
    }
}

/// Applies one content change: a range replacement, or the whole text.
fn apply_change(text: &mut String, change: lsp::TextDocumentContentChangeEvent) {
    match change.range {
        Some(range) => {
            let index = LineIndex::new(text);
            let r = index.text_range(text, range);
            let (start, end) = (usize::from(r.start()), usize::from(r.end()));
            if start <= end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end)
            {
                text.replace_range(start..end, &change.text);
            }
        }
        None => *text = change.text,
    }
}

/// The server's capabilities.
fn capabilities() -> lsp::ServerCapabilities {
    lsp::ServerCapabilities {
        position_encoding: Some(lsp::PositionEncodingKind::UTF16),
        text_document_sync: Some(lsp::TextDocumentSyncCapability::Options(
            lsp::TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(lsp::TextDocumentSyncKind::INCREMENTAL),
                will_save: None,
                will_save_wait_until: None,
                save: Some(lsp::TextDocumentSyncSaveOptions::Supported(true)),
            },
        )),
        hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
        completion_provider: Some(lsp::CompletionOptions {
            trigger_characters: Some(vec![".".to_owned(), "@".to_owned()]),
            ..lsp::CompletionOptions::default()
        }),
        signature_help_provider: Some(lsp::SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_owned(), ",".to_owned()]),
            retrigger_characters: Some(vec![":".to_owned()]),
            work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
        }),
        definition_provider: Some(lsp::OneOf::Left(true)),
        type_definition_provider: Some(lsp::TypeDefinitionProviderCapability::Simple(true)),
        implementation_provider: Some(lsp::ImplementationProviderCapability::Simple(true)),
        references_provider: Some(lsp::OneOf::Left(true)),
        document_highlight_provider: Some(lsp::OneOf::Left(true)),
        document_symbol_provider: Some(lsp::OneOf::Left(true)),
        workspace_symbol_provider: Some(lsp::OneOf::Left(true)),
        code_action_provider: Some(lsp::CodeActionProviderCapability::Options(
            lsp::CodeActionOptions {
                code_action_kinds: Some(vec![lsp::CodeActionKind::QUICKFIX]),
                ..lsp::CodeActionOptions::default()
            },
        )),
        rename_provider: Some(lsp::OneOf::Right(lsp::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
        })),
        inlay_hint_provider: Some(lsp::OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_changes_splice_utf16_ranges() {
        let mut text = "let é = 1\nlet b = 2\n".to_owned();
        apply_change(
            &mut text,
            lsp::TextDocumentContentChangeEvent {
                range: Some(lsp::Range::new(
                    lsp::Position::new(0, 4),
                    lsp::Position::new(0, 5),
                )),
                range_length: None,
                text: "a".to_owned(),
            },
        );
        assert_eq!(text, "let a = 1\nlet b = 2\n");
        apply_change(
            &mut text,
            lsp::TextDocumentContentChangeEvent {
                range: Some(lsp::Range::new(
                    lsp::Position::new(1, 8),
                    lsp::Position::new(9, 0),
                )),
                range_length: None,
                text: "3\n".to_owned(),
            },
        );
        assert_eq!(text, "let a = 1\nlet b = 3\n");
        apply_change(
            &mut text,
            lsp::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "x".to_owned(),
            },
        );
        assert_eq!(text, "x");
    }
}
