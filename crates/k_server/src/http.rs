//! The HTTP server: a hyper front on a multi-threaded tokio runtime and N
//! worker threads, each running one [`Engine`] with the app's server bundle
//! (`docs/compiler/server.md`, "Process model").
//!
//! ```text
//! TCP ─▶ hyper (tokio) ─▶ static asset?  ─▶ bytes from memory
//!                       ├▶ front::check  ─▶ 405/413/415/403/404 without JS
//!                       └▶ round-robin  ─▶ worker isolate: serve(handler) ─▶ response
//! ```
//!
//! A worker isolate runs the bundle's entry module, which calls `serve()`
//! from `k:runtime/server`; requests interleave on the isolate's event
//! loop. Shutdown stops accepting, lets in-flight connections finish
//! within the grace period, then closes the worker queues so each
//! `serve()` returns and its thread exits.

use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::sync::{mpsc, oneshot};

use crate::config::ConfigValue;
use crate::db::DatabaseSetup;
use crate::engine::{Engine, EngineOptions, JsModule, Output};
use crate::front::{self, FrontPolicy};
use crate::ops::{PendingRequest, RequestQueue, WorkerResponse};
use crate::process::ProcessState;

/// The body of every response: in memory, or streamed from a worker.
type Body = BoxBody<Bytes, std::convert::Infallible>;

/// A streamed response body: the chunks a worker sends with
/// `op_k_respond_chunk`, ending when it calls `op_k_respond_end` or drops
/// the stream.
struct ChunkBody(mpsc::UnboundedReceiver<Vec<u8>>);

impl hyper::body::Body for ChunkBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        self.0
            .poll_recv(cx)
            .map(|chunk| chunk.map(|c| Ok(hyper::body::Frame::data(Bytes::from(c)))))
    }
}

/// The server half of a build: the JavaScript modules and what the front
/// needs to know about the app.
#[derive(Clone, Debug, Default)]
pub struct ServerBundle {
    /// The modules (generated server code), by specifier.
    pub modules: Vec<JsModule>,
    /// The specifier of the entry module, whose evaluation calls `serve()`.
    pub entry: String,
    /// Every rpc path, `/shop.cart.CartProtocol/Add`.
    pub rpc_paths: BTreeSet<String>,
}

/// A static file served from memory.
#[derive(Clone, Debug)]
pub struct StaticAsset {
    /// The URL path, `/assets/app.3f2a1b9c.js`.
    pub path: String,
    /// The `Content-Type`.
    pub content_type: String,
    /// The bytes.
    pub body: Bytes,
    /// `true` for content-hashed names: served with
    /// `Cache-Control: public, max-age=31536000, immutable`.
    pub immutable: bool,
}

/// How to run a [`Server`].
#[derive(Clone, Debug)]
pub struct ServerOptions {
    /// The address to listen on; port 0 picks a free port
    /// ([`Server::local_addr`] tells which).
    pub addr: SocketAddr,
    /// The number of worker isolates (at least 1).
    pub workers: usize,
    /// The app's server bundle.
    pub bundle: ServerBundle,
    /// The static assets.
    pub assets: Vec<StaticAsset>,
    /// `K_PUBLIC_ORIGIN`, for the request-forgery check.
    pub public_origin: String,
    /// The request body limit.
    pub max_body_bytes: usize,
    /// How long shutdown waits for in-flight requests
    /// (`K_SHUTDOWN_GRACE`, default 25 s).
    pub shutdown_grace: Duration,
    /// Where worker log lines go.
    pub log: Output,
    /// The validated configuration (`config::resolve`), by variable name.
    /// The generated app reads it once per isolate through
    /// `op_k_server_info`; secrets stay inside the server.
    pub config: std::collections::BTreeMap<String, ConfigValue>,
    /// URL templates (`/webhooks/{tenant}`) of endpoint pages that don't
    /// inject `Session` (the build manifest's endpoints with
    /// `uses_session: false`). Their requests reach the handler without
    /// `Cookie` headers, so a session cookie neither triggers the
    /// request-forgery check nor reaches a webhook
    /// (`docs/design/27-endpoint-pages.md`).
    pub forgery_exempt: Vec<String>,
    /// The app's database (`DATABASE_URL` and the build's statements and
    /// migrations), opened and migrated before any worker starts.
    pub database: Option<DatabaseSetup>,
    /// The build's background jobs (the manifest's `jobs`). With a
    /// database, the jobs worker ([`crate::jobs`]) runs next to the
    /// listener, `K_JOB_CONCURRENCY` jobs at a time.
    pub jobs: Vec<k_db::stdlib::JobSpec>,
    /// How often the jobs worker looks for due jobs (250 ms).
    pub job_poll: Duration,
    /// Replaces origins of outgoing `HttpClient` requests: a request to
    /// the first origin of a pair (or below it) goes to the second
    /// instead ([`crate::outbound::replace_origin`]). Tests use it to
    /// stand a [`crate::fakes::FakeHttp`] in for a third party whose URL
    /// the app hard-codes; `k run` and `dist/app` never set it, so it is
    /// empty in production.
    pub fetch_origins: Vec<(String, String)>,
}

impl ServerOptions {
    /// Options for `bundle` on `addr` with the documented defaults: one
    /// worker per core, no assets, origin `http://localhost`, 1 MB bodies,
    /// 25 s grace, JSON-lines logs.
    #[must_use]
    pub fn new(addr: SocketAddr, bundle: ServerBundle) -> ServerOptions {
        ServerOptions {
            addr,
            workers: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            bundle,
            assets: Vec::new(),
            public_origin: "http://localhost".to_owned(),
            max_body_bytes: front::DEFAULT_MAX_BODY_BYTES,
            shutdown_grace: Duration::from_secs(25),
            log: Output::JsonLines,
            config: std::collections::BTreeMap::new(),
            forgery_exempt: Vec::new(),
            database: None,
            jobs: Vec::new(),
            job_poll: Duration::from_millis(250),
            fetch_origins: Vec::new(),
        }
    }
}

/// Why the server couldn't start or stopped with an error.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ServerError {
    /// Binding the address failed.
    Bind(String),
    /// A worker isolate couldn't start or its entry module failed.
    Worker(String),
    /// The server's threads failed.
    Internal(String),
    /// The database couldn't be opened or a migration failed.
    Database(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Bind(m) => write!(f, "cannot listen: {m}"),
            ServerError::Worker(m) => write!(f, "worker failed: {m}"),
            ServerError::Internal(m) => write!(f, "server failure: {m}"),
            ServerError::Database(m) => write!(f, "database: {m}"),
        }
    }
}

impl std::error::Error for ServerError {}

/// A running server.
pub struct Server {
    /// The bound address (with the actual port when `0` was asked for).
    addr: SocketAddr,
    /// Fired (or dropped) to stop accepting; `None` once stopped.
    stop: Option<oneshot::Sender<()>>,
    /// The HTTP thread, joined by [`Server::stop_and_join`]; `None` once joined.
    thread: Option<std::thread::JoinHandle<Result<(), ServerError>>>,
}

/// What every connection task of a [`Server`] shares.
struct Shared {
    /// The front's checks ([`front::check`]).
    policy: FrontPolicy,
    /// [`ServerOptions::forgery_exempt`], split into segments.
    exempt: Vec<Vec<String>>,
    /// The static assets, by request path.
    assets: HashMap<String, StaticAsset>,
    /// Cleared at shutdown, which closes every worker's queue.
    workers: std::sync::RwLock<Vec<mpsc::UnboundedSender<PendingRequest>>>,
    /// Round-robin counter picking the worker for the next request.
    next: AtomicUsize,
}

/// The body of one worker thread: creates its isolate, reports on `started`
/// whether that worked, then runs the bundle's entry module, which serves
/// `receiver`'s requests until the queue closes.
///
/// # Errors
///
/// [`ServerError::Worker`] when the isolate can't be created or the entry
/// module fails; [`ServerError::Internal`] if the entry specifier can't be
/// encoded.
fn worker_thread(
    bundle: ServerBundle,
    log: Output,
    process: Arc<ProcessState>,
    receiver: mpsc::UnboundedReceiver<PendingRequest>,
    started: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), ServerError> {
    let mut engine = match Engine::new(EngineOptions {
        modules: bundle.modules,
        output: log,
        timeout: None,
        compact_runtime: false,
    }) {
        Ok(e) => e,
        Err(e) => {
            let _ = started.send(Err(e.to_string()));
            return Err(ServerError::Worker(e.to_string()));
        }
    };
    engine.install_requests(RequestQueue::new(receiver));
    engine.install_process(process);
    let _ = started.send(Ok(()));
    let entry =
        serde_json::to_string(&bundle.entry).map_err(|e| ServerError::Internal(e.to_string()))?;
    let outcome = engine
        .run_main(&format!("import {entry};\n"))
        .map(|_| ())
        .map_err(|e| ServerError::Worker(e.to_string()));
    // A worker whose entry module failed answers 503 from now on; say why
    // at once rather than only at shutdown.
    if let (Err(e), Output::JsonLines) = (&outcome, log) {
        let line = serde_json::json!({ "level": "error", "message": e.to_string() });
        println!("{line}");
    }
    outcome
}

/// Writes a server log line the way workers do.
fn log_line(log: Output, level: &str, message: &str) {
    if log == Output::JsonLines {
        let line = serde_json::json!({ "level": level, "message": message });
        println!("{line}");
    }
}

/// A complete response with `status` (`500` if it isn't a valid status),
/// `content_type` (left out if it isn't a valid header value) and `body`.
fn simple(status: u16, content_type: &str, body: impl Into<Bytes>) -> Response<Body> {
    let mut r = Response::new(Full::new(body.into()).boxed());
    *r.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if let Ok(v) = content_type.parse() {
        r.headers_mut().insert(hyper::header::CONTENT_TYPE, v);
    }
    r
}

/// `true` if `path` matches one of the templates (`{name}` matches one
/// non-empty segment).
fn matches_template(templates: &[Vec<String>], path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').skip(1).collect();
    templates.iter().any(|t| {
        t.len() == parts.len()
            && t.iter().zip(&parts).all(|(s, p)| {
                if s.starts_with('{') && s.ends_with('}') {
                    !p.is_empty()
                } else {
                    s == p
                }
            })
    })
}

/// Adds the headers of every response: `x-content-type-options: nosniff`,
/// and `cache-control: private, no-store` on rpc-shaped answers.
fn finish(mut r: Response<Body>, rpc: bool) -> Response<Body> {
    let h = r.headers_mut();
    h.insert(
        "x-content-type-options",
        hyper::header::HeaderValue::from_static("nosniff"),
    );
    if rpc {
        h.insert(
            hyper::header::CACHE_CONTROL,
            hyper::header::HeaderValue::from_static("private, no-store"),
        );
    }
    r
}

/// Answers one request: a static asset for `GET`/`HEAD` when one matches;
/// otherwise the front's checks ([`front::check`]), the body read within
/// [`front::BODY_READ_TIMEOUT`] and the body limit (`408` and `413`), and
/// then a worker chosen round robin. `503` when no worker takes it (at
/// shutdown). Never fails: every outcome is a response.
async fn handle(
    shared: Arc<Shared>,
    remote: SocketAddr,
    request: Request<Incoming>,
) -> Result<Response<Body>, std::convert::Infallible> {
    let method = request.method().as_str().to_owned();
    let path = request
        .uri()
        .path_and_query()
        .map_or_else(|| "/".to_owned(), |p| p.as_str().to_owned());
    let bare_path = request.uri().path().to_owned();
    let rpc = shared.policy.rpc_paths.contains(&bare_path);
    // Every rpc-shaped answer is private, the 404 of an unknown rpc too.
    let rpc_shaped = rpc || front::looks_like_rpc(&bare_path);
    if (method == "GET" || method == "HEAD") && !rpc {
        if let Some(asset) = shared.assets.get(&bare_path) {
            let body = if method == "HEAD" {
                Bytes::new()
            } else {
                asset.body.clone()
            };
            let mut r = simple(200, &asset.content_type, body);
            let cache = if asset.immutable {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            r.headers_mut().insert(
                hyper::header::CACHE_CONTROL,
                hyper::header::HeaderValue::from_static(cache),
            );
            return Ok(finish(r, false));
        }
    }
    // A session-free endpoint never sees cookies (so it needs no forgery
    // check); rpcs and pages always do.
    let exempt = !rpc_shaped && matches_template(&shared.exempt, &bare_path);
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .filter(|(n, _)| !(exempt && *n == hyper::header::COOKIE))
        .map(|(n, v)| {
            (
                n.as_str().to_ascii_lowercase(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    if let Some(refused) = front::check(&shared.policy, &method, &path, &headers) {
        // Read (and drop) a body of moderate size first, so a client that
        // writes its whole request before reading sees the refusal instead
        // of a reset connection.
        let cap = shared.policy.max_body_bytes.saturating_mul(2);
        let _ = tokio::time::timeout(
            front::BODY_READ_TIMEOUT,
            Limited::new(request.into_body(), cap).collect(),
        )
        .await;
        let mut r = simple(refused.status, refused.content_type, refused.body);
        for (n, v) in refused.headers {
            if let (Ok(n), Ok(v)) = (
                hyper::header::HeaderName::try_from(n),
                hyper::header::HeaderValue::try_from(v),
            ) {
                r.headers_mut().insert(n, v);
            }
        }
        return Ok(finish(r, rpc_shaped));
    }
    let mut incoming = request.into_body();
    let read = tokio::time::timeout(
        front::BODY_READ_TIMEOUT,
        Limited::new(&mut incoming, shared.policy.max_body_bytes).collect(),
    )
    .await;
    let body = match read {
        Ok(Ok(b)) => b.to_bytes().to_vec(),
        Err(_) => {
            // The body stalled: answer and close, never wait forever.
            let mut r = simple(
                408,
                "text/plain; charset=utf-8",
                "the request body timed out",
            );
            r.headers_mut().insert(
                hyper::header::CONNECTION,
                hyper::header::HeaderValue::from_static("close"),
            );
            return Ok(finish(r, rpc));
        }
        Ok(Err(_)) => {
            let cap = shared.policy.max_body_bytes;
            let _ = tokio::time::timeout(
                front::BODY_READ_TIMEOUT,
                Limited::new(&mut incoming, cap).collect(),
            )
            .await;
            let r = if rpc {
                simple(
                    413,
                    "application/json",
                    r#"{"code":"resource_exhausted","message":"the request body is larger than 1 MB"}"#,
                )
            } else {
                simple(413, "text/plain; charset=utf-8", "request body too large")
            };
            return Ok(finish(r, rpc));
        }
    };
    let (reply, response) = oneshot::channel::<WorkerResponse>();
    let sent = match shared.workers.read() {
        Ok(workers) if !workers.is_empty() => {
            let n = shared.next.fetch_add(1, Ordering::Relaxed) % workers.len();
            workers[n]
                .send(PendingRequest {
                    method,
                    path,
                    headers,
                    body,
                    remote: remote.ip().to_string(),
                    job: None,
                    reply,
                })
                .is_ok()
        }
        _ => false,
    };
    let response = if sent { response.await.ok() } else { None };
    let Some(response) = response else {
        return Ok(finish(
            simple(
                503,
                "text/plain; charset=utf-8",
                "the server is shutting down",
            ),
            rpc,
        ));
    };
    let mut r = match response.stream {
        Some(chunks) => {
            let mut r = Response::new(ChunkBody(chunks).boxed());
            *r.status_mut() =
                StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            r
        }
        None => {
            let mut r = simple(response.status, "application/octet-stream", response.body);
            r.headers_mut().remove(hyper::header::CONTENT_TYPE);
            r
        }
    };
    for (n, v) in response.headers {
        if let (Ok(n), Ok(v)) = (
            hyper::header::HeaderName::try_from(n),
            hyper::header::HeaderValue::try_from(v),
        ) {
            r.headers_mut().append(n, v);
        }
    }
    Ok(finish(r, rpc))
}

impl Server {
    /// Starts the workers, binds the address and starts serving. Returns
    /// once the listener is bound and every worker isolate has started.
    ///
    /// # Errors
    ///
    /// [`ServerError::Bind`] if the address can't be bound,
    /// [`ServerError::Worker`] if an isolate can't be created.
    pub fn start(options: ServerOptions) -> Result<Server, ServerError> {
        let workers = options.workers.max(1);
        let mut process = ProcessState::new(options.public_origin.clone(), options.config.clone())
            .with_fetch_origins(options.fetch_origins.clone());
        if let Some(setup) = options.database.clone() {
            // Migrations run before any worker starts or the port opens:
            // a failure leaves the previous release serving elsewhere.
            let (db, applied) = crate::db::ProcessDatabase::open(setup)
                .map_err(|e| ServerError::Database(e.to_string()))?;
            for name in applied {
                log_line(options.log, "info", &format!("applied migration {name}"));
            }
            process = process.with_database(db);
        }
        let process = Arc::new(process);
        let mut senders = Vec::with_capacity(workers);
        let mut worker_threads = Vec::with_capacity(workers);
        for i in 0..workers {
            let (tx, rx) = mpsc::unbounded_channel();
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let bundle = options.bundle.clone();
            let log = options.log;
            let process = Arc::clone(&process);
            let handle = std::thread::Builder::new()
                .name(format!("k-worker-{i}"))
                .stack_size(8 * 1024 * 1024)
                .spawn(move || worker_thread(bundle, log, process, rx, started_tx))
                .map_err(|e| ServerError::Internal(e.to_string()))?;
            match started_rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(ServerError::Worker(e)),
                Err(_) => {
                    return Err(ServerError::Worker(
                        "a worker exited while starting".to_owned(),
                    ));
                }
            }
            senders.push(tx);
            worker_threads.push(handle);
        }
        let shared = Arc::new(Shared {
            policy: FrontPolicy {
                rpc_paths: options.bundle.rpc_paths.clone(),
                public_origin: options.public_origin.clone(),
                max_body_bytes: options.max_body_bytes,
            },
            exempt: options
                .forgery_exempt
                .iter()
                .map(|t| t.split('/').skip(1).map(str::to_owned).collect())
                .collect(),
            assets: options
                .assets
                .iter()
                .map(|a| (a.path.clone(), a.clone()))
                .collect(),
            workers: std::sync::RwLock::new(senders),
            next: AtomicUsize::new(0),
        });
        // The jobs worker: with jobs and a database, after every isolate
        // started (it hands runs to them like requests).
        let job_worker = match (options.jobs.is_empty(), process.database()) {
            (false, Some(db)) => {
                let mut o = crate::jobs::JobWorkerOptions::new(options.jobs.clone());
                if let Some(ConfigValue::Int(n)) = options.config.get("K_JOB_CONCURRENCY") {
                    o.concurrency = usize::try_from(*n).unwrap_or(4);
                }
                o.poll = options.job_poll;
                o.grace = options.shutdown_grace;
                o.log = options.log;
                let to = Arc::clone(&shared);
                let dispatch: crate::jobs::Dispatch =
                    Arc::new(move |request| match to.workers.read() {
                        Ok(workers) if !workers.is_empty() => {
                            let n = to.next.fetch_add(1, Ordering::Relaxed) % workers.len();
                            workers[n].send(request).is_ok()
                        }
                        _ => false,
                    });
                Some(
                    crate::jobs::JobWorker::start(db, o, dispatch)
                        .map_err(|e| ServerError::Internal(e.to_string()))?,
                )
            }
            (false, None) => {
                log_line(
                    options.log,
                    "warning",
                    "the app has background jobs but no database (DATABASE_URL): no jobs worker runs",
                );
                None
            }
            _ => None,
        };
        let std_listener = std::net::TcpListener::bind(options.addr)
            .map_err(|e| ServerError::Bind(e.to_string()))?;
        std_listener
            .set_nonblocking(true)
            .map_err(|e| ServerError::Bind(e.to_string()))?;
        let addr = std_listener
            .local_addr()
            .map_err(|e| ServerError::Bind(e.to_string()))?;
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let grace = options.shutdown_grace;
        let thread = std::thread::Builder::new()
            .name("k-http".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| ServerError::Internal(e.to_string()))?;
                runtime.block_on(accept_loop(
                    std_listener,
                    Arc::clone(&shared),
                    stop_rx,
                    grace,
                ))?;
                // The jobs worker stops claiming and finishes (or gives
                // back) its runs while the isolates still answer them.
                if let Some(w) = job_worker {
                    w.stop();
                }
                // Closing the queues ends every worker's `serve()` once the
                // requests already queued are answered.
                if let Ok(mut w) = shared.workers.write() {
                    w.clear();
                }
                for w in worker_threads {
                    w.join()
                        .map_err(|_| ServerError::Internal("a worker panicked".to_owned()))??;
                }
                Ok(())
            })
            .map_err(|e| ServerError::Internal(e.to_string()))?;
        Ok(Server {
            addr,
            stop: Some(stop_tx),
            thread: Some(thread),
        })
    }

    /// The bound address.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops accepting, waits up to the grace period for in-flight
    /// requests, stops the workers and returns.
    ///
    /// # Errors
    ///
    /// A worker's failure (its entry module threw), or a thread panic.
    pub fn shutdown(mut self) -> Result<(), ServerError> {
        self.stop_and_join()
    }

    /// Serves until the process receives `SIGTERM` or `SIGINT` (Ctrl-C),
    /// then shuts down gracefully (`docs/design/38-deployment.md`,
    /// "Startup and shutdown").
    ///
    /// # Errors
    ///
    /// As [`Server::shutdown`], or a failure to install the signal handlers.
    pub fn run_until_signal(self) -> Result<(), ServerError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ServerError::Internal(e.to_string()))?;
        runtime.block_on(async {
            #[cfg(unix)]
            {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .map_err(|e| ServerError::Internal(e.to_string()))?;
                tokio::select! {
                    _ = term.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
            Ok::<(), ServerError>(())
        })?;
        self.shutdown()
    }

    /// Stops accepting and joins the HTTP thread; a no-op the second time.
    ///
    /// # Errors
    ///
    /// The HTTP thread's own error, or [`ServerError::Internal`] if it
    /// panicked.
    fn stop_and_join(&mut self) -> Result<(), ServerError> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        match self.thread.take() {
            Some(t) => t
                .join()
                .map_err(|_| ServerError::Internal("the HTTP thread panicked".to_owned()))?,
            None => Ok(()),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

/// Accepts connections until `stop` fires, serving each over HTTP/1.1 with
/// the header read timeout ([`front::HEADER_READ_TIMEOUT`]), then waits up
/// to `grace` for open connections to finish.
///
/// # Errors
///
/// [`ServerError::Bind`] if the listener can't join the runtime. Failed
/// accepts are skipped.
async fn accept_loop(
    listener: std::net::TcpListener,
    shared: Arc<Shared>,
    mut stop: oneshot::Receiver<()>,
    grace: Duration,
) -> Result<(), ServerError> {
    let listener = tokio::net::TcpListener::from_std(listener)
        .map_err(|e| ServerError::Bind(e.to_string()))?;
    let graceful = hyper_util::server::graceful::GracefulShutdown::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { continue };
                let shared = Arc::clone(&shared);
                let service = service_fn(move |req| handle(Arc::clone(&shared), peer, req));
                // A timer gives the connection a header read timeout, so a
                // client that never finishes its headers is dropped.
                let connection = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .header_read_timeout(front::HEADER_READ_TIMEOUT)
                    .serve_connection(TokioIo::new(stream), service);
                let watched = graceful.watch(connection);
                tokio::spawn(async move {
                    let _ = watched.await;
                });
            }
            _ = &mut stop => break,
        }
    }
    drop(listener);
    tokio::select! {
        () = graceful.shutdown() => {}
        () = tokio::time::sleep(grace) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::matches_template;

    fn t(templates: &[&str]) -> Vec<Vec<String>> {
        templates
            .iter()
            .map(|t| t.split('/').skip(1).map(str::to_owned).collect())
            .collect()
    }

    #[test]
    fn templates_match_whole_paths_with_one_segment_per_parameter() {
        let ts = t(&["/webhooks/echo", "/tenants/{id}/ping"]);
        assert!(matches_template(&ts, "/webhooks/echo"));
        assert!(matches_template(&ts, "/tenants/7/ping"));
        assert!(!matches_template(&ts, "/tenants//ping"));
        assert!(!matches_template(&ts, "/webhooks/echo/x"));
        assert!(!matches_template(&ts, "/webhooks"));
        assert!(!matches_template(&ts, "/"));
        assert!(!matches_template(&[], "/webhooks/echo"));
    }
}
