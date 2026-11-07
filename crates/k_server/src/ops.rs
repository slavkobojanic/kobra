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
//! | `op_k_session_get` / `_create` / `_delete` / `_end_all` | the session store | `Session`, `SessionManager` |
//! | `op_k_rate_limit` | `(key, limit, windowMs) => retryAfterSeconds` (0: allowed) | `RateLimit` |
//! | `op_k_shared_get` / `op_k_shared_set` | `(key) => json \| null`, `(key, json)` | `@Scope(.server)` state |
//! | `op_k_broadcast_subscribe` / `_publish` / `_next` / `_close` | keyed broadcasts | `Broadcast` |
//! | `op_k_db_query` | `(statement, params, tx) => Promise<row[]>`, a row a value list in select order | repositories |
//! | `op_k_db_execute` | `(statement, params, tx) => Promise<{count} \| {unique: field}>` | repository writes |
//! | `op_k_db_begin` / `op_k_db_finish` | `() => Promise<tx>`, `(tx, commit) => Promise<void>` | `Database.transaction` |
//! | `op_k_db_savepoint` | `(tx, depth, op: 0 begin, 1 release, 2 roll back) => Promise<void>` | nested transactions |
//! | `op_k_db_ping` | `(timeoutMs) => Promise<boolean>` | `Database.ping` |
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
    /// The body.
    pub body: ToJsBuffer,
    /// The peer's IP address (the socket's; `K_TRUSTED_PROXIES` handling
    /// is the generated handler's).
    pub remote: String,
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
    /// The headers.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: JsBody,
}

/// A response converted to plain Rust values, sent back to the HTTP task.
#[derive(Debug)]
pub struct WorkerResponse {
    /// The status code.
    pub status: u16,
    /// The headers.
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
    /// The headers.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
    /// The peer's IP address.
    pub remote: String,
    /// Where the response goes.
    pub reply: oneshot::Sender<WorkerResponse>,
}

/// The receiving end of a worker's request queue.
pub struct RequestQueue {
    receiver: Rc<tokio::sync::Mutex<mpsc::UnboundedReceiver<PendingRequest>>>,
    waiting: HashMap<u32, oneshot::Sender<WorkerResponse>>,
    streaming: HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>,
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
        }
    }
}

type Shared = Rc<RefCell<EngineState>>;

fn shared(state: &OpState) -> Shared {
    Rc::clone(state.borrow::<Shared>())
}

#[op2(fast)]
fn op_k_log(state: &mut OpState, #[string] level: &str, #[string] message: &str) {
    let s = shared(state);
    let mut s = s.borrow_mut();
    match s.mode {
        Output::Capture => s.output.push(LogLine {
            level: level.to_owned(),
            message: message.to_owned(),
        }),
        Output::JsonLines => {
            let line = serde_json::json!({ "level": level, "message": message });
            println!("{line}");
        }
    }
}

#[op2(fast)]
fn op_k_set_result(state: &mut OpState, #[string] text: &str) {
    shared(state).borrow_mut().result = Some(text.to_owned());
}

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
        remote: request.remote,
    }))
}

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

fn process(state: &OpState) -> Arc<ProcessState> {
    Arc::clone(&shared(state).borrow().process)
}

/// What the generated app reads once at startup.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfo {
    public_origin: String,
    config: serde_json::Map<String, serde_json::Value>,
}

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
fn password_hash(password: &str) -> Result<String, String> {
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

#[op2]
#[string]
async fn op_k_password_hash(#[string] password: String) -> Result<String, JsErrorBox> {
    off_thread(move || password_hash(&password))
        .await
        .unwrap_or_else(|| Err("password hashing failed".to_owned()))
        .map_err(JsErrorBox::generic)
}

#[op2]
async fn op_k_password_verify(#[string] password: String, #[string] hash: String) -> bool {
    off_thread(move || password_verify(&password, &hash))
        .await
        .unwrap_or(false)
}

#[op2]
#[serde]
fn op_k_session_get(state: &mut OpState, #[string] id: &str) -> Option<SessionView> {
    process(state).session(id)
}

#[op2]
#[string]
fn op_k_session_create(
    state: &mut OpState,
    #[serde] user: serde_json::Value,
    #[serde] roles: Vec<String>,
) -> Result<String, JsErrorBox> {
    process(state)
        .create_session(user, roles)
        .map_err(JsErrorBox::generic)
}

#[op2(fast)]
fn op_k_session_delete(state: &mut OpState, #[string] id: &str) {
    process(state).delete_session(id);
}

#[op2]
fn op_k_session_end_all(state: &mut OpState, #[serde] user: serde_json::Value) -> u32 {
    u32::try_from(process(state).end_all_sessions(&user)).unwrap_or(u32::MAX)
}

#[op2(fast)]
fn op_k_rate_limit(state: &mut OpState, #[string] key: &str, limit: u32, window_ms: f64) -> u32 {
    let window = if window_ms.is_finite() && window_ms > 0.0 {
        std::time::Duration::from_secs_f64(window_ms / 1000.0)
    } else {
        std::time::Duration::ZERO
    };
    u32::try_from(process(state).rate_limit(key, limit, window)).unwrap_or(u32::MAX)
}

#[op2]
#[serde]
fn op_k_shared_get(state: &mut OpState, #[string] key: &str) -> serde_json::Value {
    process(state)
        .shared_get(key)
        .map_or(serde_json::Value::Null, serde_json::Value::String)
}

#[op2(fast)]
fn op_k_shared_set(state: &mut OpState, #[string] key: &str, #[string] json: &str) {
    process(state).shared_set(key, json.to_owned());
}

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

#[op2(fast)]
fn op_k_broadcast_publish(state: &mut OpState, #[string] key: &str, #[string] json: &str) {
    process(state).publish(key, json);
}

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

#[op2(fast)]
fn op_k_broadcast_close(state: &mut OpState, id: f64) {
    process(state).unsubscribe(id as u64);
}

/// The head of a streamed response.
#[derive(Deserialize)]
pub struct JsStreamHead {
    /// The status code.
    pub status: f64,
    /// The headers.
    pub headers: Vec<(String, String)>,
}

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

#[op2(fast)]
fn op_k_respond_end(state: &mut OpState, id: u32) {
    let s = shared(state);
    let mut s = s.borrow_mut();
    if let Some(queue) = s.requests.as_mut() {
        queue.streaming.remove(&id);
    }
}

/// The process's database, or the op error for an app without one.
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
#[op2]
async fn op_k_db_begin(state: Rc<RefCell<OpState>>) -> Result<u32, JsErrorBox> {
    database(&state)?.begin().await.map_err(|e| db_error(&e))
}

/// Commits or rolls back a transaction.
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

deno_core::extension!(
    k_engine,
    ops = [
        op_k_log,
        op_k_set_result,
        op_k_next_request,
        op_k_respond,
        op_k_respond_start,
        op_k_respond_chunk,
        op_k_respond_end,
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
        op_k_rate_limit,
        op_k_shared_get,
        op_k_shared_set,
        op_k_broadcast_subscribe,
        op_k_broadcast_publish,
        op_k_broadcast_next,
        op_k_broadcast_close,
        op_k_db_query,
        op_k_db_execute,
        op_k_db_begin,
        op_k_db_finish,
        op_k_db_savepoint,
        op_k_db_ping,
    ],
);

/// Puts the shared state into a runtime's `OpState`.
pub fn install(state: &mut OpState, shared: Shared) {
    state.put(shared);
}
