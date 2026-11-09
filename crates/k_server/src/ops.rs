//! K's ops: the functions JavaScript calls into Rust through
//! `Deno.core.ops` (`docs/compiler/server.md`, "Ops").
//!
//! | Op | Signature (JS) | Used by |
//! |---|---|---|
//! | `op_k_log` | `(level: string, message: string) => void` | `console.*`, `Log.*` |
//! | `op_k_set_result` | `(text: string) => void` | test and corpus drivers |
//! | `op_k_next_request` | `() => Promise<Request \| null>` | `serve()` in `k:runtime/server` |
//! | `op_k_respond` | `(id: number, response: Response) => void` | `serve()` |
//! | `op_k_respond_start` / `op_k_respond_chunk` / `op_k_respond_end` | a streamed response | server streaming |
//! | `op_k_server_info` | `() => {publicOrigin, config}` | the app's startup |
//! | `op_k_now` | `() => number` (epoch ms) | `Clock`, `k-data` |
//! | `op_k_random_token` | `() => string` (43 chars base64url) | session ids, OAuth state |
//! | `op_k_password_hash` | `(password) => Promise<string>` (an Argon2id PHC string) | `Password.hash` |
//! | `op_k_password_verify` | `(password, hash) => Promise<boolean>` (`false` for a malformed hash) | `Password.verify` |
//! | `op_k_utf8_valid` | `(bytes: Uint8Array) => boolean` | `TextDecoder` with `fatal: true` (rpc bodies, `request.text()`) |
//! | `op_k_session_get` / `_create` / `_delete` / `_end_all` | the session store, async (`k_session` with a database) | `Session`, `SessionManager` |
//! | `op_k_session_issue_state` / `_consume_state` | `(id) => Promise<state \| null>`, `(id, state) => Promise<boolean>` | OAuth state |
//! | `op_k_rate_limit` | `(key, limit, windowMs, nowMs) => Promise<retryAfterSeconds>` (0: allowed; `nowMs` a test's virtual time, else negative) | `RateLimit` |
//! | `op_k_shared_get` / `op_k_shared_set` | `(key) => json \| null`, `(key, json)` | `@Scope(.server)` state |
//! | `op_k_broadcast_subscribe` / `_publish` / `_next` / `_close` | keyed broadcasts | `Broadcast` |
//! | `op_k_presence_join` / `_update` / `_leave` / `_members` | presence leases (`_members` async) | `Presence` |
//! | `op_k_channel_send` / `op_k_channel_close` | `(id, json) => boolean`, `(id, reason) => void` | channels (`k:runtime/app`) |
//! | `op_k_db_query` | `(statement, params, tx) => Promise<row[]>`, a row a value list in select order | repositories |
//! | `op_k_db_execute` | `(statement, params, tx) => Promise<{count} \| {unique: field}>` | repository writes |
//! | `op_k_db_begin` / `op_k_db_finish` | `() => Promise<tx>`, `(tx, commit) => Promise<void>` | `Database.transaction` |
//! | `op_k_db_savepoint` | `(tx, depth, op: 0 begin, 1 release, 2 roll back) => Promise<void>` | nested transactions |
//! | `op_k_db_ping` | `(timeoutMs) => Promise<boolean>` | `Database.ping` |
//! | `op_k_hmac` | `(bits, key, message) => Uint8Array` (bits 256 or 512) | `Hmac.sha256`, `Hmac.sha512` |
//! | `op_k_jobs_enqueue` | `(name, payloadJson, delayMs, tx) => Promise<id>` | `DatabaseJobQueue` |
//! | `op_k_fetch` | `(id, {method, url, headers, timeoutMs}, body, hasBody) => Promise<outcome>` | `FetchHttpClient` |
//! | `op_k_fetch_abort` | `(id) => void` | a cancelled `FetchHttpClient` call |
//! | `op_k_smtp_send` | `(settings, message) => Promise<outcome>` | `SmtpEmailProvider` |
//! | `op_k_test_reset` | `() => void` | `k test`: a fresh process (and database) per test |
//! | `op_k_test_tick` | `() => Promise<void>` | `k test`: one event-loop turn between idle rounds |
//!
//! The process-wide ops read and write the [`ProcessState`] every worker
//! of a server shares; an engine outside a server (tests, the execution
//! corpus) has a private one. The database ops refer to statements by
//! their number in the build's statement table (`docs/compiler/db.md` §6);
//! `tx` 0 means "no transaction". Their errors are database failures,
//! which the runtime turns into panics; their messages never contain
//! bound values or the connection URL.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use deno_core::{OpState, ToJsBuffer, op2};
use deno_error::JsErrorBox;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::engine::{LogLine, Output};
use crate::process::{ProcessState, SessionView};

/// A request handed to JavaScript.
#[derive(Serialize)]
pub struct JsRequest {
    /// The id to respond with.
    pub id: u32,
    /// The method, upper case.
    pub method: String,
    /// The path and query.
    pub path: String,
    /// The headers, names lower case, in order.
    pub headers: Vec<(String, String)>,
    /// The request body bytes (a `Uint8Array` in JavaScript).
    pub body: ToJsBuffer,
    /// The client's IP address: the socket peer's, or behind a trusted
    /// proxy the client `X-Forwarded-For` names
    /// ([`crate::proxy::TrustedProxies::client_address`]). The HTTP front
    /// decides it; the runtime uses it as `clientAddress`.
    pub remote: String,
    /// For a background job's run (never an HTTP request): which job and
    /// attempt. The body is then the payload JSON.
    pub job: Option<JobRun>,
    /// For a channel event (never an HTTP request): what happened to which
    /// channel ([`crate::sockets`]). The path is then the rpc's, and the
    /// body the open's request or the message, as JSON.
    pub channel: Option<crate::sockets::ChannelRun>,
}

/// A background job's run, delivered to a worker isolate like a request
/// (`docs/compiler/server.md` §11). Only the jobs worker creates one; no
/// HTTP request can, so a client can't run a job.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct JobRun {
    /// The job's name.
    pub name: String,
    /// The attempt, from 1.
    pub attempt: u32,
}

/// The body of a response from JavaScript: a string or bytes.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum JsBody {
    /// Text, sent as UTF-8.
    Text(String),
    /// Bytes (a `Uint8Array`).
    Bytes(deno_core::JsBuffer),
}

/// A response from JavaScript.
#[derive(Deserialize)]
pub struct JsResponse {
    /// The status code as JavaScript gave it; anything that is not an
    /// integer in `100..=999` becomes a `500` with a generic body
    /// ([`WorkerResponse::from_js`]).
    pub status: f64,
    /// The response headers, in order (repeats allowed).
    pub headers: Vec<(String, String)>,
    /// The response body, text or bytes.
    pub body: JsBody,
}

/// A response converted to plain Rust values, sent back to the HTTP task.
#[derive(Debug)]
pub struct WorkerResponse {
    /// The status code.
    pub status: u16,
    /// The response headers, in order (repeats allowed).
    pub headers: Vec<(String, String)>,
    /// The body bytes.
    pub body: Vec<u8>,
    /// For a streamed response, the rest of the body: chunks sent by
    /// `op_k_respond_chunk` until `op_k_respond_end` (or the worker drops
    /// the stream). `body` is then empty.
    pub stream: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
}

/// A request waiting in a worker's queue.
pub struct PendingRequest {
    /// The request data (without an id; the worker assigns it).
    pub method: String,
    /// The path and query.
    pub path: String,
    /// The request headers, names lower case, in order.
    pub headers: Vec<(String, String)>,
    /// The request body bytes, read whole by the HTTP front.
    pub body: Vec<u8>,
    /// The client's IP address, as the HTTP front decided it from the
    /// socket peer and `K_TRUSTED_PROXIES` ([`crate::proxy`]).
    pub remote: String,
    /// A job run instead of an HTTP request ([`JobRun`]).
    pub job: Option<JobRun>,
    /// A channel event instead of an HTTP request
    /// ([`crate::sockets::ChannelRun`]).
    pub channel: Option<crate::sockets::ChannelRun>,
    /// Where the response goes.
    pub reply: oneshot::Sender<WorkerResponse>,
}

/// The receiving end of a worker's request queue.
pub struct RequestQueue {
    /// The worker's end of the HTTP front's queue.
    receiver: Rc<tokio::sync::Mutex<mpsc::UnboundedReceiver<PendingRequest>>>,
    /// Requests handed to JavaScript and not answered yet, by request id.
    waiting: HashMap<u32, oneshot::Sender<WorkerResponse>>,
    /// Streamed responses in progress, by request id: the body's chunk sender.
    streaming: HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>,
    /// The id of the next request handed to JavaScript (wraps around).
    next_id: u32,
}

impl RequestQueue {
    /// Wraps a channel receiver.
    pub fn new(receiver: mpsc::UnboundedReceiver<PendingRequest>) -> RequestQueue {
        RequestQueue {
            receiver: Rc::new(tokio::sync::Mutex::new(receiver)),
            waiting: HashMap::new(),
            streaming: HashMap::new(),
            next_id: 0,
        }
    }
}

/// Per-isolate state the ops share.
pub struct EngineState {
    /// Captured log lines.
    pub output: Vec<LogLine>,
    /// Where log lines go.
    pub mode: Output,
    /// The last `op_k_set_result` value.
    pub result: Option<String>,
    /// The request queue of a server worker.
    pub requests: Option<RequestQueue>,
    /// The state every worker of the process shares.
    pub process: Arc<ProcessState>,
    /// The abort handles of `op_k_fetch` requests in flight, by id.
    pub fetches: HashMap<u32, Rc<tokio::sync::Notify>>,
    /// The database half of a test build (`k test`): `op_k_test_reset`
    /// opens a fresh in-memory SQLite database with its migrations for
    /// every test. `None` outside tests, or for a build without
    /// persistence.
    pub test_database: Option<k_db::manifest::DbManifest>,
    /// Maps the generated modules' stack frames to `.k` lines
    /// (`crate::stack`), for logged traces and the development 500 page.
    pub symbols: Rc<crate::stack::Symbolicator>,
}

impl EngineState {
    /// Fresh state with a private [`ProcessState`].
    pub fn new(mode: Output) -> EngineState {
        EngineState {
            output: Vec::new(),
            mode,
            result: None,
            requests: None,
            process: Arc::new(ProcessState::default()),
            fetches: HashMap::new(),
            test_database: None,
            symbols: Rc::new(crate::stack::Symbolicator::default()),
        }
    }
}

/// How the ops reach the [`EngineState`]: stored in each isolate's
/// [`OpState`].
type Shared = Rc<RefCell<EngineState>>;

/// The [`EngineState`] of the isolate an op runs in.
///
/// # Panics
///
/// If the engine didn't put the state into `state`; [`crate::Engine::new`]
/// always does.
fn shared(state: &OpState) -> Shared {
    Rc::clone(state.borrow::<Shared>())
}

/// `console.*` and `Log.*`: records a log line at `level`, captured in
/// [`EngineState::output`] or printed as a JSON line on standard output,
/// per [`EngineState::mode`].
#[op2(fast)]
fn op_k_log(state: &mut OpState, #[string] level: &str, #[string] message: &str) {
    let s = shared(state);
    let mut s = s.borrow_mut();
    // Stack traces name `.k` lines (server.md §15).
    let mapped;
    let message = if message.contains("k:/") {
        mapped = s.symbols.symbolicate(message);
        mapped.as_str()
    } else {
        message
    };
    match s.mode {
        Output::Capture => s.output.push(LogLine {
            level: level.to_owned(),
            message: message.to_owned(),
        }),
        other => other.write(level, message),
    }
}

/// `text` with its stack-frame locations mapped to `.k` lines
/// (`crate::stack`): the development 500 page shows it.
#[op2]
#[string]
fn op_k_symbolicate(state: &mut OpState, #[string] text: &str) -> String {
    shared(state).borrow().symbols.symbolicate(text)
}

/// `true` when a failed request's 500 shows the mapped stack trace
/// (development servers: `k run`, `k dev`).
#[op2(fast)]
fn op_k_dev_errors(state: &mut OpState) -> bool {
    process(state).dev_errors
}

/// Stores `text` as the result of the current [`crate::Engine::run_main`]
/// (the test and corpus drivers report through it).
#[op2(fast)]
fn op_k_set_result(state: &mut OpState, #[string] text: &str) {
    shared(state).borrow_mut().result = Some(text.to_owned());
}

/// Starts a test of `k test` (docs/design/21-testing.md): replaces the
/// process state with a fresh one (no sessions, rate-limit counts, shared
/// service state or subscribers) and, for a build with persistence
/// ([`EngineState::test_database`]), a fresh in-memory SQLite database
/// with every migration applied. Each test therefore sees only what it
/// wrote itself, and commits behave exactly as in production.
///
/// # Errors
///
/// The database couldn't be opened or a migration failed; the message
/// names the cause (never a URL or password: the database is in memory).
#[op2(fast)]
fn op_k_test_reset(state: &mut OpState) -> Result<(), JsErrorBox> {
    let s = shared(state);
    let manifest = s.borrow().test_database.clone();
    let mut process = ProcessState::default();
    if let Some(manifest) = manifest {
        let (db, _) = crate::db::ProcessDatabase::open(crate::db::DatabaseSetup {
            url: "sqlite::memory:".to_owned(),
            password: None,
            manifest,
        })
        .map_err(|e| JsErrorBox::generic(format!("the test database couldn't be opened: {e}")))?;
        process = process.with_database(db);
    }
    s.borrow_mut().process = Arc::new(process);
    Ok(())
}

/// Yields to the event loop once, so the ops that have completed deliver
/// their results: the test kit's idle rounds use it, since a run of
/// microtasks alone never lets an op's result in.
#[op2]
async fn op_k_test_tick() {
    tokio::task::yield_now().await;
}

/// `serve()`'s loop: waits for the worker's next request and hands it to
/// JavaScript under a fresh id, keeping its reply channel until
/// `op_k_respond` (or `op_k_respond_start`) answers it. Resolves to `null`
/// once the queue is closed (shutdown) or outside a server worker.
///
/// # Errors
///
/// Never in practice; the signature is the op's.
#[op2]
#[serde]
async fn op_k_next_request(state: Rc<RefCell<OpState>>) -> Result<Option<JsRequest>, JsErrorBox> {
    let s = shared(&state.borrow());
    let receiver = match s.borrow().requests.as_ref() {
        Some(q) => Rc::clone(&q.receiver),
        None => return Ok(None),
    };
    let next = receiver.lock().await.recv().await;
    let Some(request) = next else { return Ok(None) };
    let mut s = s.borrow_mut();
    let Some(queue) = s.requests.as_mut() else {
        return Ok(None);
    };
    let id = queue.next_id;
    queue.next_id = queue.next_id.wrapping_add(1);
    queue.waiting.insert(id, request.reply);
    Ok(Some(JsRequest {
        id,
        method: request.method,
        path: request.path,
        headers: request.headers,
        body: request.body.into(),
        job: request.job,
        channel: request.channel,
        remote: request.remote,
    }))
}

/// Answers request `id` with a complete response. A client that went away
/// is not an error.
///
/// # Errors
///
/// No request `id` is waiting (already answered, or never handed out).
#[op2]
fn op_k_respond(
    state: &mut OpState,
    id: u32,
    #[serde] response: JsResponse,
) -> Result<(), JsErrorBox> {
    let s = shared(state);
    let mut s = s.borrow_mut();
    let Some(reply) = s.requests.as_mut().and_then(|q| q.waiting.remove(&id)) else {
        return Err(JsErrorBox::generic(format!(
            "no request {id} is waiting for a response"
        )));
    };
    // The client may have gone away; that is not the handler's problem.
    let _ = reply.send(WorkerResponse::from_js(response));
    Ok(())
}

impl WorkerResponse {
    /// The generic answer for a request whose handler failed or broke the
    /// response protocol: a `500` that reveals nothing about the failure.
    #[must_use]
    pub fn internal_error() -> WorkerResponse {
        WorkerResponse {
            status: 500,
            headers: vec![(
                "content-type".to_owned(),
                "text/plain; charset=utf-8".to_owned(),
            )],
            body: b"internal error".to_vec(),
            stream: None,
        }
    }

    /// Converts a response from JavaScript. A status that is not an integer
    /// in `100..=999` is a broken handler, answered with
    /// [`WorkerResponse::internal_error`] so its body never reaches the
    /// client (`docs/compiler/server.md`, "Logging, panics and errors").
    #[must_use]
    pub fn from_js(response: JsResponse) -> WorkerResponse {
        let s = response.status;
        if !(s.fract() == 0.0 && (100.0..=999.0).contains(&s)) {
            return WorkerResponse::internal_error();
        }
        let body = match response.body {
            JsBody::Text(t) => t.into_bytes(),
            JsBody::Bytes(b) => b.to_vec(),
        };
        WorkerResponse {
            // In range, checked above.
            status: s as u16,
            headers: response.headers,
            body,
            stream: None,
        }
    }
}

/// The [`ProcessState`] of the isolate an op runs in.
fn process(state: &OpState) -> Arc<ProcessState> {
    Arc::clone(&shared(state).borrow().process)
}

/// What the generated app reads once at startup.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfo {
    /// [`ProcessState::public_origin`].
    public_origin: String,
    /// The validated configuration as JSON ([`crate::config::ConfigValue::to_json`]).
    config: serde_json::Map<String, serde_json::Value>,
}

/// The public origin and the validated configuration, read once by the
/// generated app at startup.
#[op2]
#[serde]
fn op_k_server_info(state: &mut OpState) -> ServerInfo {
    let p = process(state);
    ServerInfo {
        public_origin: p.public_origin.clone(),
        config: p
            .config
            .iter()
            .map(|(k, v)| (k.clone(), v.to_json()))
            .collect(),
    }
}

/// The wall clock in whole epoch milliseconds (0 before 1970).
#[op2(fast)]
fn op_k_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
        .floor()
}

/// `true` when `bytes` are valid UTF-8: the check behind the bootstrap's
/// `TextDecoder` with `fatal: true`, so a body that isn't UTF-8 is refused
/// instead of being decoded lossily.
#[op2(fast)]
fn op_k_utf8_valid(#[buffer] bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// A fresh session id or OAuth state ([`crate::process::random_token`]).
///
/// # Errors
///
/// The operating system refused randomness.
#[op2]
#[string]
fn op_k_random_token() -> Result<String, JsErrorBox> {
    crate::process::random_token().map_err(JsErrorBox::generic)
}

/// Argon2id with the OWASP minimum parameters (m = 19 MiB, t = 2, p = 1),
/// the `argon2` crate's defaults (`docs/design/33-crypto.md`, "Passwords").
fn argon2id() -> argon2::Argon2<'static> {
    argon2::Argon2::default()
}

/// Hashes `password` with a fresh 16-byte salt from the OS generator.
///
/// # Errors
///
/// The random generator failed, or Argon2 refused the input.
pub fn password_hash(password: &str) -> Result<String, String> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| format!("the random generator failed: {e}"))?;
    let salt = SaltString::encode_b64(&salt).map_err(|e| e.to_string())?;
    argon2id()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("password hashing failed: {e}"))
}

/// Whether `password` matches the PHC string `hash`, in constant time; a
/// malformed hash or one of another algorithm matches nothing.
fn password_verify(password: &str, hash: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    PasswordHash::new(hash).is_ok_and(|parsed| {
        argon2id()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Runs CPU-heavy work on Tokio's blocking pool when a runtime is
/// available, so one sign-in doesn't stall the worker's other requests;
/// inline otherwise. `None` if the work panicked.
async fn off_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.spawn_blocking(f).await.ok(),
        Err(_) => Some(f()),
    }
}

/// `Password.hash`: the Argon2id PHC string of `password`, computed off the
/// worker's thread ([`password_hash`]).
///
/// # Errors
///
/// Randomness or hashing failed (or the hashing task panicked).
#[op2]
#[string]
async fn op_k_password_hash(#[string] password: String) -> Result<String, JsErrorBox> {
    off_thread(move || password_hash(&password))
        .await
        .unwrap_or_else(|| Err("password hashing failed".to_owned()))
        .map_err(JsErrorBox::generic)
}

/// `Password.verify`: whether `password` matches `hash`, computed off the
/// worker's thread; `false` for a malformed hash or if the task panicked.
#[op2]
async fn op_k_password_verify(#[string] password: String, #[string] hash: String) -> bool {
    off_thread(move || password_verify(&password, &hash))
        .await
        .unwrap_or(false)
}

/// The process state of an async op.
fn process_of(state: &Rc<RefCell<OpState>>) -> Arc<ProcessState> {
    let s = shared(&state.borrow());
    Arc::clone(&s.borrow().process)
}

/// The session of a cookie token, `null` if there is none (expired,
/// revoked, never issued). A guest session's `user` is `null`.
///
/// # Errors
///
/// The session store (the database, when the app has one) failed.
#[op2]
#[serde]
async fn op_k_session_get(
    state: Rc<RefCell<OpState>>,
    #[string] id: String,
) -> Result<Option<SessionView>, JsErrorBox> {
    process_of(&state)
        .session_load(&id)
        .await
        .map_err(|e| JsErrorBox::generic(format!("sessions: {e}")))
}

/// Starts a session (`user` `null`: a guest's) and returns its token.
///
/// # Errors
///
/// The session store failed, or the OS refused randomness for the token.
#[op2]
#[string]
async fn op_k_session_create(
    state: Rc<RefCell<OpState>>,
    #[serde] user: serde_json::Value,
    #[serde] roles: Vec<String>,
) -> Result<String, JsErrorBox> {
    process_of(&state)
        .session_create(user, roles)
        .await
        .map_err(|e| JsErrorBox::generic(format!("sessions: {e}")))
}

/// Deletes a session.
///
/// # Errors
///
/// The session store failed.
#[op2]
async fn op_k_session_delete(
    state: Rc<RefCell<OpState>>,
    #[string] id: String,
) -> Result<(), JsErrorBox> {
    process_of(&state)
        .session_delete(&id)
        .await
        .map_err(|e| JsErrorBox::generic(format!("sessions: {e}")))
}

/// Deletes every session of a user; returns how many.
///
/// # Errors
///
/// The session store failed.
#[op2]
async fn op_k_session_end_all(
    state: Rc<RefCell<OpState>>,
    #[serde] user: serde_json::Value,
) -> Result<u32, JsErrorBox> {
    let n = process_of(&state)
        .session_end_all(&user)
        .await
        .map_err(|e| JsErrorBox::generic(format!("sessions: {e}")))?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Binds a fresh OAuth state to a session; `null` without the session.
///
/// # Errors
///
/// The session store failed.
#[op2]
#[serde]
async fn op_k_session_issue_state(
    state: Rc<RefCell<OpState>>,
    #[string] id: String,
) -> Result<serde_json::Value, JsErrorBox> {
    Ok(process_of(&state)
        .session_issue_state(&id)
        .await
        .map_err(|e| JsErrorBox::generic(format!("sessions: {e}")))?
        .map_or(serde_json::Value::Null, serde_json::Value::String))
}

/// Takes a session's OAuth state: `true` once for the state it holds.
///
/// # Errors
///
/// The session store failed.
#[op2]
async fn op_k_session_consume_state(
    state: Rc<RefCell<OpState>>,
    #[string] id: String,
    #[string] oauth_state: String,
) -> Result<bool, JsErrorBox> {
    process_of(&state)
        .session_consume_state(&id, &oauth_state)
        .await
        .map_err(|e| JsErrorBox::generic(format!("sessions: {e}")))
}

/// Counts one call against a rate-limit key: `0` if allowed, else the
/// seconds to wait.
///
/// # Errors
///
/// The rate-limit store (the database, when the app has one) failed.
#[op2]
async fn op_k_rate_limit(
    state: Rc<RefCell<OpState>>,
    #[string] key: String,
    limit: u32,
    window_ms: f64,
    now_ms: f64,
) -> Result<u32, JsErrorBox> {
    let window = if window_ms.is_finite() && window_ms > 0.0 {
        std::time::Duration::from_secs_f64(window_ms / 1000.0)
    } else {
        std::time::Duration::ZERO
    };
    // A test's virtual time (`TestClock`), else the system time.
    #[allow(clippy::cast_possible_truncation)]
    let now = (now_ms.is_finite() && now_ms >= 0.0).then(|| now_ms.min(9e15) as i64);
    let wait = process_of(&state)
        .rate_hit(&key, limit, window, now)
        .await
        .map_err(|e| JsErrorBox::generic(format!("rate limits: {e}")))?;
    Ok(u32::try_from(wait).unwrap_or(u32::MAX))
}

/// The `.server` state stored under `key` as JSON text, or `null`.
#[op2]
#[serde]
fn op_k_shared_get(state: &mut OpState, #[string] key: &str) -> serde_json::Value {
    process(state)
        .shared_get(key)
        .map_or(serde_json::Value::Null, serde_json::Value::String)
}

/// Stores `json` as the `.server` state under `key`, for every worker.
#[op2(fast)]
fn op_k_shared_set(state: &mut OpState, #[string] key: &str, #[string] json: &str) {
    process(state).shared_set(key, json.to_owned());
}

/// Subscribes to broadcast `key`, closed when session `session` (a string,
/// or `null` for none) ends; returns the subscriber id.
#[op2]
fn op_k_broadcast_subscribe(
    state: &mut OpState,
    #[string] key: String,
    #[serde] session: serde_json::Value,
) -> f64 {
    let session = session.as_str().map(str::to_owned);
    // Ids stay far below 2^53.
    process(state).subscribe(key, session).0 as f64
}

/// Publishes `json` to every subscriber of `key` in the process.
#[op2(fast)]
fn op_k_broadcast_publish(state: &mut OpState, #[string] key: &str, #[string] json: &str) {
    process(state).publish(key, json);
}

/// Waits for subscriber `id`'s next element and resolves to its JSON text,
/// or to `null` once the subscription is closed or unknown.
#[op2]
#[serde]
async fn op_k_broadcast_next(state: Rc<RefCell<OpState>>, id: f64) -> serde_json::Value {
    let p = process(&state.borrow());
    let id = id as u64;
    loop {
        let notify = p.notifier(id);
        if let Some(next) = p.take(id) {
            return next.map_or(serde_json::Value::Null, serde_json::Value::String);
        }
        match notify {
            Some(n) => n.notified().await,
            None => return serde_json::Value::Null,
        }
    }
}

/// Removes subscriber `id` (its stream was dropped).
#[op2(fast)]
fn op_k_broadcast_close(state: &mut OpState, id: f64) {
    process(state).unsubscribe(id as u64);
}

/// Adds `member` (its codec JSON) under presence key `key` and returns the
/// lease's id ([`ProcessState::presence_join`]).
#[op2(fast)]
fn op_k_presence_join(state: &mut OpState, #[string] key: &str, #[string] member: &str) -> f64 {
    // Ids stay far below 2^53.
    process(state).presence_join(key, member) as f64
}

/// Replaces lease `id`'s member with `member` (codec JSON).
#[op2(fast)]
fn op_k_presence_update(state: &mut OpState, id: f64, #[string] member: &str) {
    process(state).presence_update(id as u64, member);
}

/// Ends lease `id`; idempotent.
#[op2(fast)]
fn op_k_presence_leave(state: &mut OpState, id: f64) {
    process(state).presence_leave(id as u64);
}

/// The members under presence key `key`, each as its codec JSON text, in
/// arrival order.
#[op2]
#[serde]
async fn op_k_presence_members(state: Rc<RefCell<OpState>>, #[string] key: String) -> Vec<String> {
    let p = process(&state.borrow());
    p.presence_members(&key).await
}

/// Sends server message `json` on channel `id` ([`crate::sockets::Hub::send`]):
/// `false` once the channel is gone, or was dropped because its outgoing
/// buffer overflowed, so the isolate ends it.
#[op2(fast)]
fn op_k_channel_send(state: &mut OpState, id: f64, #[string] json: &str) -> bool {
    process(state).channels().send(id as u64, json)
}

/// Closes channel `id` from the server: `reason` is `byServer` (its sending
/// stream ended) or `sessionEnded`.
#[op2(fast)]
fn op_k_channel_close(state: &mut OpState, id: f64, #[string] reason: &str) {
    let reason = crate::channels::CloseReason::from_name(reason)
        .unwrap_or(crate::channels::CloseReason::ByServer);
    process(state).channels().close(id as u64, reason);
}

/// The head of a streamed response.
#[derive(Deserialize)]
pub struct JsStreamHead {
    /// The status code.
    pub status: f64,
    /// The response headers, in order (repeats allowed).
    pub headers: Vec<(String, String)>,
}

/// Starts a streamed response to request `id`: sends the head now; the
/// body follows with `op_k_respond_chunk` and ends with `op_k_respond_end`.
///
/// # Errors
///
/// Not a server worker, or no request `id` is waiting.
#[op2]
fn op_k_respond_start(
    state: &mut OpState,
    id: u32,
    #[serde] head: JsStreamHead,
) -> Result<(), JsErrorBox> {
    let s = shared(state);
    let mut s = s.borrow_mut();
    let Some(queue) = s.requests.as_mut() else {
        return Err(JsErrorBox::generic("not a server worker"));
    };
    let Some(reply) = queue.waiting.remove(&id) else {
        return Err(JsErrorBox::generic(format!(
            "no request {id} is waiting for a response"
        )));
    };
    let (tx, rx) = mpsc::unbounded_channel();
    let mut response = WorkerResponse::from_js(JsResponse {
        status: head.status,
        headers: head.headers,
        body: JsBody::Text(String::new()),
    });
    response.stream = Some(rx);
    queue.streaming.insert(id, tx);
    let _ = reply.send(response);
    Ok(())
}

/// Sends one chunk of a streamed response. Returns `false` once the client
/// is gone (the handler should stop producing).
#[op2(fast)]
fn op_k_respond_chunk(state: &mut OpState, id: u32, #[buffer] chunk: &[u8]) -> bool {
    let s = shared(state);
    let mut s = s.borrow_mut();
    let Some(queue) = s.requests.as_mut() else {
        return false;
    };
    let open = queue
        .streaming
        .get(&id)
        .is_some_and(|tx| tx.send(chunk.to_vec()).is_ok());
    if !open {
        queue.streaming.remove(&id);
    }
    open
}

/// Ends the streamed response to request `id`; a no-op if it already ended
/// or the client is gone.
#[op2(fast)]
fn op_k_respond_end(state: &mut OpState, id: u32) {
    let s = shared(state);
    let mut s = s.borrow_mut();
    if let Some(queue) = s.requests.as_mut() {
        queue.streaming.remove(&id);
    }
}

/// The process's database, or the op error for an app without one.
///
/// # Errors
///
/// The app has no database configured.
fn database(state: &Rc<RefCell<OpState>>) -> Result<Arc<crate::db::ProcessDatabase>, JsErrorBox> {
    process(&state.borrow())
        .database()
        .ok_or_else(|| JsErrorBox::generic("database: this app has no database configured"))
}

/// A database failure as an op error (a panic in JavaScript); its message
/// has neither bound values nor the connection URL.
fn db_error(e: &k_db::DbError) -> JsErrorBox {
    JsErrorBox::generic(format!("database: {e}"))
}

/// Converts the parameters of a statement, naming the first bad one.
///
/// # Errors
///
/// A parameter that isn't `null`, a boolean, a number or a string, named
/// by its 1-based position.
fn db_params(params: Vec<serde_json::Value>) -> Result<Vec<k_db::Value>, JsErrorBox> {
    params
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            crate::db::param_from_json(p)
                .map_err(|e| JsErrorBox::generic(format!("database: parameter {}: {e}", i + 1)))
        })
        .collect()
}

/// Runs a read statement and returns its rows as value lists.
///
/// # Errors
///
/// No database, a bad parameter, or a database failure ([`db_error`]).
#[op2]
#[serde]
async fn op_k_db_query(
    state: Rc<RefCell<OpState>>,
    statement: u32,
    #[serde] params: Vec<serde_json::Value>,
    tx: u32,
) -> Result<serde_json::Value, JsErrorBox> {
    let db = database(&state)?;
    let rows = db
        .query(statement, db_params(params)?, tx)
        .await
        .map_err(|e| db_error(&e))?;
    Ok(serde_json::Value::Array(
        rows.into_iter()
            .map(|r| {
                serde_json::Value::Array(r.into_iter().map(crate::db::value_to_json).collect())
            })
            .collect(),
    ))
}

/// Runs a write statement: `{count}`, or `{unique: field}` when a
/// `@Unique` field's index refused it.
///
/// # Errors
///
/// No database, a bad parameter, or a database failure other than a unique
/// violation ([`db_error`]).
#[op2]
#[serde]
async fn op_k_db_execute(
    state: Rc<RefCell<OpState>>,
    statement: u32,
    #[serde] params: Vec<serde_json::Value>,
    tx: u32,
) -> Result<serde_json::Value, JsErrorBox> {
    let db = database(&state)?;
    Ok(
        match db
            .execute(statement, db_params(params)?, tx)
            .await
            .map_err(|e| db_error(&e))?
        {
            crate::db::WriteOutcome::Count(n) => serde_json::json!({ "count": n }),
            crate::db::WriteOutcome::Unique(field) => serde_json::json!({ "unique": field }),
        },
    )
}

/// Begins a transaction bound to the calling request.
///
/// # Errors
///
/// No database, or a database failure (including a lease timeout).
#[op2]
async fn op_k_db_begin(state: Rc<RefCell<OpState>>) -> Result<u32, JsErrorBox> {
    database(&state)?.begin().await.map_err(|e| db_error(&e))
}

/// Commits or rolls back a transaction.
///
/// # Errors
///
/// No database, a transaction that isn't open, or a database failure.
#[op2]
async fn op_k_db_finish(
    state: Rc<RefCell<OpState>>,
    tx: u32,
    commit: bool,
) -> Result<(), JsErrorBox> {
    database(&state)?
        .finish(tx, commit)
        .await
        .map_err(|e| db_error(&e))
}

/// A savepoint of a nested transaction: 0 begins, 1 releases, 2 rolls back.
///
/// # Errors
///
/// An unknown `op`, no database, a transaction that isn't open, or a
/// database failure.
#[op2]
async fn op_k_db_savepoint(
    state: Rc<RefCell<OpState>>,
    tx: u32,
    depth: u32,
    op: u32,
) -> Result<(), JsErrorBox> {
    let op = match op {
        0 => crate::db::Savepoint::Begin,
        1 => crate::db::Savepoint::Release,
        2 => crate::db::Savepoint::Rollback,
        _ => return Err(JsErrorBox::generic("database: unknown savepoint operation")),
    };
    database(&state)?
        .savepoint(tx, depth, op)
        .await
        .map_err(|e| db_error(&e))
}

/// `SELECT 1` within the timeout; `false` without a database.
#[op2]
async fn op_k_db_ping(state: Rc<RefCell<OpState>>, timeout_ms: f64) -> bool {
    let Ok(db) = database(&state) else {
        return false;
    };
    let timeout = if timeout_ms.is_finite() && timeout_ms > 0.0 {
        std::time::Duration::from_secs_f64(timeout_ms / 1000.0)
    } else {
        std::time::Duration::ZERO
    };
    db.ping(timeout).await
}

/// HMAC (RFC 2104) of `message` under `key` with SHA-256 (`bits` 256) or
/// SHA-512 (512), for `k.crypto.Hmac`.
#[must_use]
pub fn hmac(bits: u32, key: &[u8], message: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    fn mac<D: Digest>(block: usize, key: &[u8], message: &[u8]) -> Vec<u8> {
        let mut k = if key.len() > block {
            D::digest(key).to_vec()
        } else {
            key.to_vec()
        };
        k.resize(block, 0);
        let inner_key: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
        let outer_key: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
        let mut inner = D::new();
        inner.update(&inner_key);
        inner.update(message);
        let mut outer = D::new();
        outer.update(&outer_key);
        outer.update(inner.finalize());
        outer.finalize().to_vec()
    }
    if bits == 512 {
        mac::<sha2::Sha512>(128, key, message)
    } else {
        mac::<sha2::Sha256>(64, key, message)
    }
}

/// `Hmac.sha256` / `Hmac.sha512`: the MAC bytes.
#[op2]
#[buffer]
fn op_k_hmac(bits: u32, #[buffer] key: &[u8], #[buffer] message: &[u8]) -> Vec<u8> {
    hmac(bits, key, message)
}

/// Stores a background job (`DatabaseJobQueue.enqueue`) in transaction
/// `tx` (0: on its own), due after `delay_ms`; returns its id.
///
/// # Errors
///
/// No database, the OS refused randomness for the id, or the insert failed.
#[op2]
#[string]
async fn op_k_jobs_enqueue(
    state: Rc<RefCell<OpState>>,
    #[string] name: String,
    #[string] payload: String,
    delay_ms: f64,
    tx: u32,
) -> Result<String, JsErrorBox> {
    let db = database(&state)?;
    let id = crate::jobs::job_id().map_err(JsErrorBox::generic)?;
    let now = crate::jobs::now_ms();
    #[allow(clippy::cast_possible_truncation)]
    let delay = if delay_ms.is_finite() && delay_ms > 0.0 {
        delay_ms.min(1e15) as i64
    } else {
        0
    };
    db.enqueue_job(id, &name, payload, now.saturating_add(delay), now, tx)
        .await
        .map_err(|e| db_error(&e))
}

// ------------------------------------------------------------- outbound

/// An outcome of `op_k_fetch` for JavaScript: the body as a buffer.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum JsFetchOutcome {
    /// A response arrived ([`crate::outbound::FetchOutcome::Response`]).
    Response {
        /// The status code.
        status: u16,
        /// The headers, names lower case, in order.
        headers: Vec<(String, String)>,
        /// The body, as a `Uint8Array` on the JavaScript side.
        body: ToJsBuffer,
    },
    /// The request couldn't be sent or answered.
    Offline {
        /// Why, for the server log.
        message: String,
    },
    /// The request's timeout elapsed.
    Timeout,
    /// The request itself is malformed.
    Invalid {
        /// What is wrong.
        message: String,
    },
    /// `op_k_fetch_abort` was called for the request.
    Aborted,
}

/// Sends an HTTP request for `FetchHttpClient` (`crate::outbound::fetch`).
/// `id` names it for `op_k_fetch_abort`; `body` is sent when `has_body`.
#[op2]
#[serde]
async fn op_k_fetch(
    state: Rc<RefCell<OpState>>,
    id: u32,
    #[serde] request: crate::outbound::FetchRequest,
    #[buffer(copy)] body: Vec<u8>,
    has_body: bool,
) -> JsFetchOutcome {
    let abort = Rc::new(tokio::sync::Notify::new());
    let s = shared(&state.borrow());
    s.borrow_mut().fetches.insert(id, Rc::clone(&abort));
    let mut request = request;
    request.body = has_body.then_some(body);
    request.url = crate::outbound::replace_origin(&request.url, s.borrow().process.fetch_origins());
    let outcome = tokio::select! {
        o = crate::outbound::fetch(request) => Some(o),
        () = abort.notified() => None,
    };
    s.borrow_mut().fetches.remove(&id);
    match outcome {
        None => JsFetchOutcome::Aborted,
        Some(crate::outbound::FetchOutcome::Response {
            status,
            headers,
            body,
        }) => JsFetchOutcome::Response {
            status,
            headers,
            body: body.into(),
        },
        Some(crate::outbound::FetchOutcome::Offline { message }) => {
            JsFetchOutcome::Offline { message }
        }
        Some(crate::outbound::FetchOutcome::Timeout) => JsFetchOutcome::Timeout,
        Some(crate::outbound::FetchOutcome::Invalid { message }) => {
            JsFetchOutcome::Invalid { message }
        }
    }
}

/// Aborts request `id` of `op_k_fetch` (its signal was aborted); nothing
/// for a finished or unknown one.
#[op2(fast)]
fn op_k_fetch_abort(state: &mut OpState, id: u32) {
    if let Some(n) = shared(state).borrow_mut().fetches.remove(&id) {
        n.notify_one();
    }
}

/// Sends an email over SMTP for `SmtpEmailProvider`
/// (`crate::outbound::send_mail`).
#[op2]
#[serde]
async fn op_k_smtp_send(
    #[serde] settings: crate::outbound::SmtpSettings,
    #[serde] message: crate::outbound::MailMessage,
) -> crate::outbound::MailOutcome {
    crate::outbound::send_mail(settings, message).await
}

deno_core::extension!(
    k_engine,
    ops = [
        op_k_log,
        op_k_symbolicate,
        op_k_dev_errors,
        op_k_set_result,
        op_k_test_reset,
        op_k_test_tick,
        op_k_next_request,
        op_k_respond,
        op_k_respond_start,
        op_k_respond_chunk,
        op_k_respond_end,
        op_k_channel_send,
        op_k_channel_close,
        op_k_server_info,
        op_k_now,
        op_k_random_token,
        op_k_password_hash,
        op_k_password_verify,
        op_k_utf8_valid,
        op_k_session_get,
        op_k_session_create,
        op_k_session_delete,
        op_k_session_end_all,
        op_k_session_issue_state,
        op_k_session_consume_state,
        op_k_rate_limit,
        op_k_shared_get,
        op_k_shared_set,
        op_k_broadcast_subscribe,
        op_k_broadcast_publish,
        op_k_broadcast_next,
        op_k_broadcast_close,
        op_k_presence_join,
        op_k_presence_update,
        op_k_presence_leave,
        op_k_presence_members,
        op_k_db_query,
        op_k_db_execute,
        op_k_db_begin,
        op_k_db_finish,
        op_k_db_savepoint,
        op_k_db_ping,
        op_k_jobs_enqueue,
        op_k_hmac,
        op_k_fetch,
        op_k_fetch_abort,
        op_k_smtp_send,
    ],
);

/// Puts the shared state into a runtime's `OpState`.
pub fn install(state: &mut OpState, shared: Shared) {
    state.put(shared);
}

#[cfg(test)]
mod tests {
    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn hmac_matches_the_rfc_4231_vectors() {
        // Test case 2: key "Jefe", "what do ya want for nothing?".
        assert_eq!(
            hex(&super::hmac(256, b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex(&super::hmac(512, b"Jefe", b"what do ya want for nothing?")),
            "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737"
        );
        // Test case 6: a key longer than the block is hashed first.
        assert_eq!(
            hex(&super::hmac(
                256,
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }
}
