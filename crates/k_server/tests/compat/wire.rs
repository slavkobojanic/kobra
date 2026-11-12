//! Wire compatibility (`docs/design/48-releases-and-versioning.md`, "Wire
//! and stored formats across a rolling deploy"; `docs/compiler/release.md`
//! §4, "A wire or stored-format change").
//!
//! `tests/compat/wire/<minor>/<example>.json` is a transcript recorded from
//! an example's server by that minor's toolchain: each step's request and
//! what came back (the response, a stream's envelopes, a stored row, a call
//! the server made, and what the browser client decoded from a response).
//! Every minor's transcripts stay. The test replays each one against the
//! current build and checks it answers as recorded, and the current
//! browser client decodes each recorded response as recorded, so a change
//! to any encoding shows up here as a reviewed difference.
//!
//! * `paybox` (`examples/shop/paybox`): rpcs called with a service token,
//!   a declared error with a payload, `rpcFailure` for a call without a
//!   token, the `k_job` row a charge queues, and the event delivery that
//!   row makes when the current build's worker runs it.
//! * `notes` (`examples/notes`): the session cookie, `rpcFailure` for a
//!   guest, a server stream's envelopes, a declared error, and a page's
//!   `k-data` payload, which the current client hydrates.
//!
//! Values that differ on every run (ids, instants, ports, session cookies,
//! service tokens' expiry and MAC) are compared by their shape: an id
//! recorded in one response is mapped to the id of this run, so a later
//! recorded request that names it is sent with this run's id. A recorded
//! service token is signed again for now, keeping its version and caller.
//!
//! `K_BLESS=1` records the current minor's transcripts again; the older
//! minors' are never rewritten.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use k_codegen::{Entry, Environment, FileKind, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::config::{ConfigField, DevEnvironment, resolve};
use k_server::fakes::{FakeHttp, FakeRoute};
use k_server::testing::{StreamChunk, StreamingResponse, open_stream, send};
use k_server::{
    Engine, EngineOptions, JsModule, Output as LogOutput, Server, ServerBundle, ServerOptions,
};
use regex::Regex;
use serde_json::{Value, json};

use super::{collect, compat_dir};

/// The secret every app of these runs shares as `K_SERVICE_TOKEN`.
const SECRET: &str = "compat-service-secret";

/// `true` under `K_BLESS=1`: record instead of compare.
fn blessing() -> bool {
    std::env::var("K_BLESS").as_deref() == Ok("1")
}

/// This toolchain's minor, `0.1`.
fn current_minor() -> String {
    let mut parts = env!("CARGO_PKG_VERSION").split(['.', '-']);
    format!(
        "{}.{}",
        parts.next().unwrap_or("0"),
        parts.next().unwrap_or("0")
    )
}

/// The minors with recorded transcripts, oldest first; under `K_BLESS=1`
/// only the current one, which is recorded.
fn minors() -> Vec<String> {
    if blessing() {
        return vec![current_minor()];
    }
    let dir = compat_dir().join("wire");
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort_by_key(|m| {
        m.split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    });
    out
}

// ------------------------------------------------------------- normalising

/// What differs on every run, and the shape it is compared by.
static VOLATILE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"v1\.([^.\s\x22]+)\.[0-9]{1,16}\.[0-9a-f]{64}", "v1.$1.{expiry}.{mac}"),
        (
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            "{uuid}",
        ),
        (r"\b(ch|re)_[0-9a-f]{32}\b", "${1}_{id}"),
        (
            r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})",
            "{instant}",
        ),
        (r"127\.0\.0\.1:[0-9]+", "{host}"),
        (r"(k_session|k_csrf)=[^;\s\x22]+", "$1={value}"),
    ]
    .into_iter()
    .map(|(r, to)| (Regex::new(r).expect("a valid pattern"), to))
    .collect()
});

/// The ids a transcript maps from a recorded response to this run's.
static IDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|\b(?:ch|re)_[0-9a-f]{32}\b",
    )
    .expect("a valid pattern")
});

/// `text` with every volatile value replaced by its shape.
fn normalise(text: &str) -> String {
    VOLATILE.iter().fold(text.to_owned(), |t, (re, to)| {
        re.replace_all(&t, *to).into_owned()
    })
}

/// A JSON value with every string normalised.
fn normalised(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(normalise(s)),
        Value::Array(a) => Value::Array(a.iter().map(normalised).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (normalise(k), normalised(x)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Whether `now` says everything `recorded` does: the same scalars, arrays
/// of the same length, and every recorded object key (JSON text inside a
/// string is compared the same way). A later build may add a field to a
/// message, as a compatible change does; it may not drop or change one.
fn covers(recorded: &Value, now: &Value) -> bool {
    match (recorded, now) {
        (Value::Object(r), Value::Object(n)) => r
            .iter()
            .all(|(k, v)| n.get(k).is_some_and(|x| covers(v, x))),
        (Value::Array(r), Value::Array(n)) => {
            r.len() == n.len() && r.iter().zip(n).all(|(a, b)| covers(a, b))
        }
        (Value::String(r), Value::String(n)) if r != n => {
            match (
                serde_json::from_str::<Value>(r),
                serde_json::from_str::<Value>(n),
            ) {
                (Ok(a @ (Value::Object(_) | Value::Array(_))), Ok(b)) => covers(&a, &b),
                _ => false,
            }
        }
        (r, n) => r == n,
    }
}

/// Every string inside a JSON value, in order.
fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.iter().for_each(|(k, x)| {
            out.push(k.clone());
            strings(x, out);
        }),
        _ => {}
    }
}

/// A service token for `caller`, signed with [`SECRET`] now.
fn service_token(caller: &str) -> String {
    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
        + 60_000;
    let payload = format!("v1.{caller}.{expiry}");
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, SECRET.as_bytes());
    let mac = ring::hmac::sign(&key, payload.as_bytes());
    let hex: String = mac.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    format!("{payload}.{hex}")
}

// ------------------------------------------------------------ transcripts

/// One request, as recorded: `{method, path, headers, body}`.
#[derive(Clone, Debug)]
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Request {
    /// An rpc call: `POST path` with JSON `body`, signed by `caller` when
    /// given.
    fn rpc(path: &str, body: &Value, caller: Option<&str>) -> Request {
        let mut headers = vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("connect-protocol-version".to_owned(), "1".to_owned()),
        ];
        match caller {
            Some(caller) => headers.push(("x-k-service-token".to_owned(), service_token(caller))),
            // A browser's call, which the forgery check wants from the app's
            // own origin.
            None => headers.push(("origin".to_owned(), "http://localhost".to_owned())),
        }
        Request {
            method: "POST".to_owned(),
            path: path.to_owned(),
            headers,
            body: body.to_string(),
        }
    }

    /// A server-streaming call: one Connect envelope of `body`.
    fn stream(path: &str, body: &Value) -> Request {
        Request {
            method: "POST".to_owned(),
            path: path.to_owned(),
            headers: vec![
                (
                    "content-type".to_owned(),
                    "application/connect+json".to_owned(),
                ),
                ("connect-protocol-version".to_owned(), "1".to_owned()),
                ("origin".to_owned(), "http://localhost".to_owned()),
            ],
            body: body.to_string(),
        }
    }

    /// A page, as a browser asks for it.
    fn page(path: &str) -> Request {
        Request {
            method: "GET".to_owned(),
            path: path.to_owned(),
            headers: vec![("accept".to_owned(), "text/html".to_owned())],
            body: String::new(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "method": self.method,
            "path": self.path,
            "headers": self.headers.iter().map(|(n, v)| format!("{n}: {v}")).collect::<Vec<_>>(),
            "body": self.body,
        })
    }

    fn from_json(v: &Value) -> Request {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Request {
            method: s("method"),
            path: s("path"),
            headers: header_pairs(&v["headers"]),
            body: s("body"),
        }
    }

    /// The bytes sent: a streaming call's body is framed as one envelope.
    fn body_bytes(&self) -> Vec<u8> {
        let streaming = self
            .headers
            .iter()
            .any(|(n, v)| n == "content-type" && v == "application/connect+json");
        if !streaming {
            return self.body.clone().into_bytes();
        }
        let n = u32::try_from(self.body.len()).unwrap_or(u32::MAX);
        let mut out = vec![0];
        out.extend(n.to_be_bytes());
        out.extend(self.body.as_bytes());
        out
    }
}

/// The response headers a transcript records: those that are part of the
/// wire format rather than of the deployment.
const RECORDED_HEADERS: &[&str] = &["content-type", "cache-control", "set-cookie"];

/// Recorded `"name: value"` headers as pairs.
fn header_pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| {
            let (n, v) = h.as_str()?.split_once(": ")?;
            Some((n.to_owned(), v.to_owned()))
        })
        .collect()
}

/// A response's status and recorded headers.
fn head(status: u16, headers: &[(String, String)]) -> Value {
    let mut kept: Vec<Value> = headers
        .iter()
        .filter(|(n, _)| RECORDED_HEADERS.contains(&n.as_str()))
        .map(|(n, v)| Value::String(format!("{n}: {v}")))
        .collect();
    kept.sort_by_key(Value::to_string);
    json!({ "status": status, "headers": kept })
}

/// The Connect envelopes in `bytes`: `{flags, data}` each, and what is
/// left of an envelope still arriving.
fn envelopes(bytes: &[u8]) -> (Vec<Value>, usize) {
    let mut out = Vec::new();
    let mut at = 0;
    while bytes.len() >= at + 5 {
        let n = u32::from_be_bytes([bytes[at + 1], bytes[at + 2], bytes[at + 3], bytes[at + 4]]);
        let end = at + 5 + n as usize;
        if bytes.len() < end {
            break;
        }
        out.push(json!({
            "flags": bytes[at],
            "data": String::from_utf8_lossy(&bytes[at + 5..end]),
        }));
        at = end;
    }
    (out, at)
}

/// A recorded transcript being replayed, or one being recorded.
struct Transcript {
    /// The file it is read from or written to.
    path: PathBuf,
    /// The recorded steps (empty while recording).
    recorded: Vec<Value>,
    /// The steps of this run.
    steps: Vec<Value>,
    /// Recorded id to this run's.
    ids: BTreeMap<String, String>,
    /// `{name}` placeholders in requests, and their values in this run.
    vars: BTreeMap<String, String>,
    /// The cookies the server set, sent back like a browser would.
    cookies: BTreeMap<String, String>,
    /// The open streams, by step title.
    streams: BTreeMap<String, (StreamingResponse, Vec<u8>)>,
    /// What didn't match.
    problems: Vec<String>,
}

impl Transcript {
    /// The transcript `minor/name.json`: read for a replay, empty for a
    /// recording.
    fn open(minor: &str, name: &str) -> Transcript {
        let path = compat_dir()
            .join("wire")
            .join(minor)
            .join(format!("{name}.json"));
        let recorded = if blessing() {
            Vec::new()
        } else {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e} (record it with K_BLESS=1)", path.display()));
            let v: Value = serde_json::from_str(&text).expect("a transcript is JSON");
            v["steps"].as_array().cloned().unwrap_or_default()
        };
        Transcript {
            path,
            recorded,
            steps: Vec::new(),
            ids: BTreeMap::new(),
            vars: BTreeMap::new(),
            cookies: BTreeMap::new(),
            streams: BTreeMap::new(),
            problems: Vec::new(),
        }
    }

    /// The recorded step this one replays (`None` while recording).
    fn recorded_step(&self, title: &str) -> Option<&Value> {
        if blessing() {
            return None;
        }
        let i = self.steps.len();
        let step = self
            .recorded
            .get(i)
            .unwrap_or_else(|| panic!("{}: no step {i} (`{title}`)", self.path.display()));
        assert_eq!(
            step["title"].as_str(),
            Some(title),
            "{}: step {i}",
            self.path.display()
        );
        Some(step)
    }

    /// `text` with this run's ids and placeholders.
    fn mapped(&self, text: &str) -> String {
        let text = IDS.replace_all(text, |c: &regex::Captures<'_>| {
            self.ids
                .get(&c[0])
                .cloned()
                .unwrap_or_else(|| c[0].to_owned())
        });
        self.vars.iter().fold(text.into_owned(), |t, (k, v)| {
            t.replace(&format!("{{{k}}}"), v)
        })
    }

    /// The request to send: `made` while recording, else the recorded one
    /// with this run's ids, placeholders and a token signed now.
    fn request(&self, title: &str, made: Request) -> (Request, Request) {
        let Some(step) = self.recorded_step(title) else {
            let sent = self.prepared(&made);
            return (made, sent);
        };
        let recorded = Request::from_json(&step["request"]);
        let sent = self.prepared(&recorded);
        (recorded, sent)
    }

    /// `r` as sent in this run.
    fn prepared(&self, r: &Request) -> Request {
        let mut headers: Vec<(String, String)> = r
            .headers
            .iter()
            .map(|(n, v)| {
                let v = if n == "x-k-service-token" {
                    let caller = v.split('.').nth(1).unwrap_or_default();
                    assert!(v.starts_with("v1."), "a recorded token: {v}");
                    service_token(caller)
                } else {
                    self.mapped(v)
                };
                (n.clone(), v)
            })
            .collect();
        if !self.cookies.is_empty() {
            let jar: Vec<String> = self
                .cookies
                .iter()
                .map(|(n, v)| format!("{n}={v}"))
                .collect();
            headers.push(("cookie".to_owned(), jar.join("; ")));
        }
        Request {
            method: r.method.clone(),
            path: self.mapped(&r.path),
            headers,
            body: self.mapped(&r.body),
        }
    }

    /// Learns the ids of `seen` that `recorded` has in the same places.
    fn learn(&mut self, recorded: &Value, seen: &Value) {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        strings(recorded, &mut a);
        strings(seen, &mut b);
        for (r, s) in a.iter().zip(&b) {
            let ra: Vec<&str> = IDS.find_iter(r).map(|m| m.as_str()).collect();
            let sa: Vec<&str> = IDS.find_iter(s).map(|m| m.as_str()).collect();
            for (x, y) in ra.iter().zip(&sa) {
                self.ids
                    .entry((*x).to_owned())
                    .or_insert_with(|| (*y).to_owned());
            }
        }
    }

    /// Records step `title` with `request` and what was `seen`, or
    /// compares `seen` with the recorded step.
    fn step(&mut self, title: &str, request: Option<&Request>, seen: Value) {
        let mut step = json!({ "title": title });
        if let Some(r) = request {
            step["request"] = r.to_json();
        }
        for (k, v) in seen.as_object().into_iter().flatten() {
            step[k] = v.clone();
        }
        if let Some(recorded) = self.recorded_step(title).cloned() {
            for (k, v) in seen.as_object().into_iter().flatten() {
                let r = &recorded[k];
                if !covers(&normalised(r), &normalised(v)) {
                    self.problems.push(format!(
                        "`{title}`: {k} differs\n  recorded: {}\n  now:      {}",
                        normalised(r),
                        normalised(v)
                    ));
                }
                self.learn(r, v);
            }
        }
        self.steps.push(step);
    }

    /// Takes the cookies a response sets.
    fn take_cookies(&mut self, headers: &[(String, String)]) {
        for (n, v) in headers {
            if n == "set-cookie"
                && let Some((name, rest)) = v.split_once('=')
            {
                let value = rest.split(';').next().unwrap_or_default();
                if value.is_empty() || v.contains("Max-Age=0") {
                    self.cookies.remove(name);
                } else {
                    self.cookies.insert(name.to_owned(), value.to_owned());
                }
            }
        }
    }

    /// Sends a request and records or compares the response; returns the
    /// response body this run got and the one recorded (the same while
    /// recording).
    fn exchange(&mut self, server: &Server, title: &str, made: Request) -> (String, String) {
        let (recorded, sent) = self.request(title, made);
        let r = send(
            server.local_addr(),
            &sent.method,
            &sent.path,
            &sent.headers,
            &sent.body_bytes(),
        )
        .unwrap_or_else(|e| panic!("`{title}`: {e}"));
        self.take_cookies(&r.headers);
        let body = String::from_utf8_lossy(&r.body).into_owned();
        let recorded_body = self.recorded_step(title).map_or_else(
            || body.clone(),
            |s| s["body"].as_str().unwrap_or_default().to_owned(),
        );
        self.step(
            title,
            Some(&recorded),
            json!({ "response": head(r.status, &r.headers), "body": body }),
        );
        (body, recorded_body)
    }

    /// Opens a server stream; its envelopes are read by [`Transcript::read`].
    fn open_stream(&mut self, server: &Server, title: &str, made: Request) {
        let (recorded, sent) = self.request(title, made);
        let r = open_stream(
            server.local_addr(),
            &sent.method,
            &sent.path,
            &sent.headers,
            &sent.body_bytes(),
        )
        .unwrap_or_else(|e| panic!("`{title}`: {e}"));
        let seen = json!({ "response": head(r.status, &r.headers) });
        self.streams.insert(title.to_owned(), (r, Vec::new()));
        self.step(title, Some(&recorded), seen);
    }

    /// Reads `count` envelopes of the stream opened as `stream` (the
    /// recorded number when replaying); returns those of this run and the
    /// recorded ones.
    fn read(&mut self, title: &str, stream: &str, count: usize) -> (Vec<Value>, Vec<Value>) {
        let recorded = self
            .recorded_step(title)
            .map(|s| s["frames"].as_array().cloned().unwrap_or_default());
        let want = recorded.as_ref().map_or(count, Vec::len);
        let (r, buf) = self
            .streams
            .get_mut(stream)
            .unwrap_or_else(|| panic!("no stream `{stream}`"));
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut frames = Vec::new();
        while frames.len() < want && Instant::now() < deadline {
            let (got, used) = envelopes(buf);
            if !got.is_empty() {
                frames.extend(got);
                buf.drain(..used);
                continue;
            }
            match r.next_chunk(Duration::from_millis(200)) {
                Some(StreamChunk::Data(d)) => buf.extend(d),
                Some(StreamChunk::End) => break,
                None => {}
            }
        }
        let recorded = recorded.unwrap_or_else(|| frames.clone());
        self.step(title, None, json!({ "frames": frames }));
        (frames, recorded)
    }

    /// Records or compares rows a query returned; returns the recorded
    /// ones with this run's ids (this run's while recording).
    fn rows(&mut self, title: &str, rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
        let recorded = self.recorded_step(title).map(|s| s["rows"].clone());
        let seen = json!({ "rows": rows });
        self.step(title, None, seen.clone());
        let rows = recorded.unwrap_or_else(|| seen["rows"].clone());
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                row.as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| self.mapped(c.as_str().unwrap_or_default()))
                    .collect()
            })
            .collect()
    }

    /// Records or compares what a client decoded.
    fn decoded(&mut self, title: &str, decoded: Value) {
        self.step(title, None, json!({ "decoded": decoded }));
    }

    /// Writes the recording, or fails with every difference.
    fn finish(self, example: &str) {
        if blessing() {
            let text = serde_json::to_string_pretty(&json!({
                "example": example,
                "recordedBy": env!("CARGO_PKG_VERSION"),
                "steps": self.steps,
            }))
            .expect("serialises");
            std::fs::create_dir_all(self.path.parent().expect("a parent")).expect("creates");
            std::fs::write(&self.path, text + "\n").expect("writes the transcript");
            return;
        }
        let mut problems = self.problems;
        if self.steps.len() != self.recorded.len() {
            problems.push(format!(
                "{} recorded steps, {} replayed",
                self.recorded.len(),
                self.steps.len()
            ));
        }
        assert!(
            problems.is_empty(),
            "{}:\n{}",
            self.path.display(),
            problems.join("\n")
        );
    }
}

// ----------------------------------------------------------------- servers

/// The repository root.
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// An example compiled as `k dev` compiles it: the analysis files of
/// `examples/<dir>` (of its `member` in a workspace), development profile.
fn build(dir: &str, member: Option<&str>) -> (Output, PathBuf) {
    let root = repo().join("examples").join(dir);
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    let (files, project): (Vec<(ProjectPath, String)>, PathBuf) = match member {
        Some(m) => {
            let ws = k_sema::Workspace::from_files(files);
            let member = ws.member(m).expect("the member");
            (ws.analysis_files(member), root.join(m))
        }
        None => (
            files
                .into_iter()
                .filter_map(|(p, t)| Some((ProjectPath::new(&p).ok()?, t)))
                .collect(),
            root,
        ),
    };
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().expect("not cancelled");
    if let Some(d) = diagnostics.iter().find(|d| d.is_error()) {
        panic!("{dir} must check cleanly: {}: {}", d.code, d.message);
    }
    let output = generate(
        &analysis,
        &Options {
            profile: Profile::Development,
            environment: Environment::Development,
            entry: Entry::App,
        },
    )
    .expect("generates");
    (output, project)
}

/// A directory removed when dropped, holding a run's SQLite database.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("k_compat_{tag}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("creates the temporary directory");
        TempDir(dir)
    }

    fn url(&self) -> String {
        format!("sqlite:{}", self.0.join("app.db").display())
    }

    /// Runs `sql` and returns its rows as text.
    fn query(&self, sql: &str) -> Vec<Vec<String>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let db = k_db::Database::connect(&self.url(), &k_db::DatabaseOptions::default())
                .await
                .expect("connects");
            db.query(sql, &[])
                .await
                .expect("queries")
                .into_iter()
                .map(|r| {
                    r.values
                        .into_iter()
                        .map(|v| match v {
                            k_db::Value::Text(t) => t,
                            k_db::Value::Int(n) => n.to_string(),
                            k_db::Value::Null => String::new(),
                            other => format!("{other:?}"),
                        })
                        .collect()
                })
                .collect()
        })
    }

    /// Runs a statement with text parameters.
    fn execute(&self, sql: &str, params: &[&str]) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let db = k_db::Database::connect(&self.url(), &k_db::DatabaseOptions::default())
                .await
                .expect("connects");
            let params: Vec<k_db::Value> = params
                .iter()
                .map(|p| k_db::Value::Text((*p).to_owned()))
                .collect();
            db.execute(sql, &params).await.expect("executes");
        });
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Starts `output`'s server on a loopback port, with `env` as its `.env`
/// and its database in `db`; with the jobs worker when `jobs` is set.
fn start(
    output: &Output,
    project: &Path,
    db: &TempDir,
    env: &[(&str, String)],
    jobs: bool,
) -> Server {
    let bundle = ServerBundle {
        modules: output
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect(),
        entry: output.server_entry.clone().expect("a server entry"),
        rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
    };
    let fields: Vec<ConfigField> = output
        .manifest
        .config
        .iter()
        .filter_map(|c| {
            ConfigField::from_manifest(
                &c.env,
                &c.declared_at,
                &c.ty,
                c.secret,
                c.required,
                c.default.as_deref(),
                &c.cases,
                c.range,
            )
        })
        .collect();
    let mut dotenv: BTreeMap<String, String> = env
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    dotenv.insert("DATABASE_URL".to_owned(), db.url());
    let dev = DevEnvironment {
        root: project.to_path_buf(),
        dotenv,
        with_process: false,
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("an address"), bundle);
    o.workers = 1;
    o.public_origin = "http://localhost".into();
    o.shutdown_grace = Duration::from_secs(2);
    o.log = if std::env::var_os("K_E2E_LOG").is_some() {
        LogOutput::JsonLines
    } else {
        LogOutput::Capture
    };
    o.config =
        resolve(&fields, &dev).unwrap_or_else(|p| panic!("{}", k_server::config::report(&p)));
    o.database = k_server::db::DatabaseSetup::from_config(output.database.clone(), &o.config);
    o.forgery_exempt = output
        .manifest
        .endpoints
        .iter()
        .filter(|e| !e.uses_session)
        .map(|e| e.template.clone())
        .collect();
    if jobs {
        o.jobs = output.manifest.jobs.clone();
        o.job_poll = Duration::from_millis(20);
    }
    Server::start(o).expect("the example starts")
}

// ------------------------------------------------------------------ paybox

/// The fake merchant's events rpc that paybox calls back.
const CHARGE_SUCCEEDED: &str = "/pay_contracts.pay.PaymentEvents/ChargeSucceeded";

/// Replays (or records) paybox's transcript of `minor`.
fn paybox(minor: &str) {
    let (output, project) = build("shop", Some("paybox"));
    let merchant = FakeHttp::start(vec![FakeRoute::json("POST", CHARGE_SUCCEEDED, &json!({}))])
        .expect("the fake merchant starts");
    let db = TempDir::new("paybox");
    let env = [("K_SERVICE_TOKEN", SECRET.to_owned())];
    let mut t = Transcript::open(minor, "paybox");
    t.vars.insert("merchant".to_owned(), merchant.origin());
    let charge = |number: &str, reference: &str| {
        json!({ "request": {
            "reference": reference,
            "amount": { "amount": "12.50", "currency": "EUR" },
            "card": { "number": number },
            "description": "Mug",
        }})
    };
    let server = start(&output, &project, &db, &env, false);
    t.exchange(
        &server,
        "registerMerchant with a service token",
        Request::rpc(
            "/pay_contracts.pay.Payments/RegisterMerchant",
            &json!({ "callbackUrl": "{merchant}/" }),
            Some("shop"),
        ),
    );
    t.exchange(
        &server,
        "a declined charge is the declared error with its payload",
        Request::rpc(
            "/pay_contracts.pay.Payments/Charge",
            &charge("4000000000009995", "order-1"),
            Some("shop"),
        ),
    );
    t.exchange(
        &server,
        "a call without a service token is rpcFailure(.rejected(.unauthenticated))",
        Request::rpc(
            "/pay_contracts.pay.Payments/Charge",
            &charge("4242424242424242", "order-2"),
            None,
        ),
    );
    t.exchange(
        &server,
        "an invalid message is rpcFailure(.invalid)",
        Request::rpc(
            "/pay_contracts.pay.Payments/Charge",
            &json!({ "request": { "reference": 7 } }),
            Some("shop"),
        ),
    );
    t.exchange(
        &server,
        "an approved charge",
        Request::rpc(
            "/pay_contracts.pay.Payments/Charge",
            &charge("4242424242424242", "order-3"),
            Some("shop"),
        ),
    );
    server.shutdown().expect("stops");
    // The row the charge queued; the recorded one replaces it, so the
    // current build's worker runs a row as the recorded toolchain wrote it.
    let rows = t.rows(
        "the charge queued its event in k_job",
        db.query("SELECT name, payload, status FROM k_job ORDER BY created_at"),
    );
    for row in &rows {
        db.execute(
            "UPDATE k_job SET payload = ?1 WHERE name = ?2",
            &[&row[1], &row[0]],
        );
    }
    let server = start(&output, &project, &db, &env, true);
    let deadline = Instant::now() + Duration::from_secs(20);
    while merchant.requests().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let delivered: Vec<Value> = merchant
        .requests()
        .iter()
        .map(|r| {
            json!({
                "method": r.method,
                "path": r.path,
                "contentType": r.headers.get("content-type"),
                "token": r.headers.get("x-k-service-token"),
                "body": r.body,
            })
        })
        .collect();
    t.step(
        "the current worker runs the recorded row: the event reaches the merchant",
        None,
        json!({ "outbound": delivered }),
    );
    server.shutdown().expect("stops");
    t.finish("examples/shop/paybox");
}

#[test]
fn paybox_answers_every_recorded_request_as_recorded() {
    for minor in minors() {
        paybox(&minor);
    }
}

// ------------------------------------------------------------------- notes

/// The browser client of an example, loaded in V8 with the fake DOM and a
/// `fetch` that answers what [`Client::decode`] hands it.
struct Client {
    engine: Engine,
    /// The client entry's specifier.
    entry: String,
}

impl Client {
    fn new(output: &Output) -> Client {
        let modules: Vec<JsModule> = output
            .client
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: format!("k:/client/{}", f.path),
                source: f.contents.clone(),
            })
            .collect();
        let stubs = modules
            .iter()
            .find(|m| m.source.contains("export class NotesProtocol$client"))
            .map(|m| m.specifier.clone())
            .expect("the protocol stubs");
        let entry = output
            .client
            .iter()
            .find(|f| f.path.starts_with("app.") && f.path.ends_with(".js"))
            .map(|f| format!("k:/client/{}", f.path))
            .expect("a client entry");
        let mut engine = Engine::new(EngineOptions {
            modules,
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        })
        .expect("an engine");
        let boot = format!(
            r#"import {{ installDom }} from "k:testing";
installDom();
globalThis.$stubs = await import({stubs:?});
globalThis.$show = (v) => {{
  if (v === null || v === undefined) return null;
  if (typeof v === "bigint") return {{ $bigint: String(v) }};
  if (Array.isArray(v)) return v.map($show);
  if (v instanceof Map) return {{ $map: [...v].map(([k, x]) => [$show(k), $show(x)]) }};
  if (v instanceof Date) return {{ $date: v.toISOString() }};
  if (typeof v === "object") {{
    const o = {{}};
    const name = v.constructor?.name;
    if (name && name !== "Object") o.$type = name;
    for (const k of Object.keys(v)) o[k] = $show(v[k]);
    return o;
  }}
  return v;
}};
globalThis.$answer = null;
globalThis.$answers = {{}};
globalThis.$asked = [];
globalThis.fetch = async (path) => {{
  $asked.push(String(path));
  const a = $answers[String(path)] ?? $answer ?? {{ status: 404, headers: {{}}, body: "{{}}" }};
  const frames = a.frames ?? null;
  return {{
    ok: a.status >= 200 && a.status < 300,
    status: a.status,
    headers: {{ get: (n) => a.headers[n.toLowerCase()] ?? null }},
    text: async () => a.body ?? "",
    body: frames && {{
      getReader() {{
        let i = 0;
        return {{
          read: () => i < frames.length
            ? Promise.resolve({{ done: false, value: frames[i++] }})
            : a.hold ? new Promise(() => {{}}) : Promise.resolve({{ done: true }}),
          cancel: async () => {{}},
        }};
      }},
    }},
  }};
}};
globalThis.$frames = (list) => list.map((f) => {{
  const json = new TextEncoder().encode(f.data);
  const out = new Uint8Array(5 + json.length);
  const n = json.length;
  out.set([f.flags, n >>> 24, (n >>> 16) & 255, (n >>> 8) & 255, n & 255]);
  out.set(json, 5);
  return out;
}});
"#
        );
        engine.run_main(&boot).expect("the client loads");
        Client { engine, entry }
    }

    /// What the app shows when it starts on `path` from a server-rendered
    /// document whose `k-data` is `k_data` (the client renders the page
    /// itself rather than claim recorded markup), with `answers` (rpc path
    /// to answer) for the rpcs the page calls: the shown path, the text of
    /// `#k-app` and the rpcs called.
    fn hydrate(&mut self, path: &str, k_data: &str, answers: &Value) -> Value {
        let boot = format!(
            r#"globalThis.$answers = {answers};
// A page's stream stays open after its envelopes, as a live one does, and
// nothing repeats on a timer, so the event loop settles.
for (const a of Object.values($answers)) if (a.frames) {{ a.frames = $frames(a.frames); a.hold = true; }}
globalThis.setInterval = () => 0;
globalThis.$asked = [];
document.body.innerHTML = '<div id="k-app"></div><script type="application/json" id="k-data"></script>';
document.getElementById("k-data").textContent = {k_data:?};
const loc = {{ origin: "http://localhost", pathname: {path:?}, search: "", hash: "" }};
globalThis.location = loc;
globalThis.history = {{ pushState(_s, _t, p) {{ loc.pathname = p; }}, replaceState(_s, _t, p) {{ loc.pathname = p; }} }};
globalThis.$bootError = null;
import({entry:?}).catch((e) => {{ globalThis.$bootError = String(e?.stack ?? e); }});
"#,
            entry = self.entry
        );
        self.engine.run_main(&boot).expect("the app starts");
        // Each run settles the event loop; a few, for the page chunk, its load
        // and the stream.
        for _ in 0..5 {
            self.engine
                .run_main("await Promise.resolve();")
                .expect("runs");
        }
        let text = self
            .engine
            .run_main(
                r#"Deno.core.ops.op_k_set_result(JSON.stringify({
  path: location.pathname,
  text: document.getElementById("k-app").textContent,
  asked: [...new Set($asked)].sort(),
  error: $bootError,
}));"#,
            )
            .expect("runs")
            .expect("a result");
        serde_json::from_str(&text).expect("JSON")
    }

    /// What `call` (a stub call on `p`, a `NotesProtocol$client`) returns
    /// or throws when the server answers `answer`.
    fn decode(&mut self, answer: &Value, call: &str) -> Value {
        let main = format!(
            r#"globalThis.$answer = {answer};
if ($answer.frames) $answer.frames = $frames($answer.frames);
const p = new $stubs.NotesProtocol$client();
let out;
try {{
  const v = await {call};
  if (v && typeof v.next === "function") {{
    const items = [];
    for (let x; (x = await v.next()) !== null;) items.push($show(x));
    out = {{ stream: items }};
  }} else {{
    out = {{ value: $show(v) }};
  }}
}} catch (e) {{
  out = {{ thrown: $show(e), message: e?.message ?? null }};
}}
Deno.core.ops.op_k_set_result(JSON.stringify(out));
"#
        );
        let text = self
            .engine
            .run_main(&main)
            .expect("the client runs")
            .expect("a result");
        serde_json::from_str(&text).expect("JSON")
    }
}

/// The answer a recorded exchange gives the client's `fetch`.
fn answer(step: &Value) -> Value {
    let headers: serde_json::Map<String, Value> = header_pairs(&step["response"]["headers"])
        .into_iter()
        .map(|(n, v)| (n, Value::String(v)))
        .collect();
    json!({
        "status": step["response"]["status"],
        "headers": headers,
        "body": step["body"],
    })
}

/// The `k-data` payload of a page.
fn k_data(html: &str) -> String {
    let start = html
        .find(r#"<script type="application/json" id="k-data">"#)
        .map(|i| i + r#"<script type="application/json" id="k-data">"#.len());
    start
        .and_then(|s| {
            html[s..]
                .find("</script>")
                .map(|e| html[s..s + e].to_owned())
        })
        .unwrap_or_default()
}

/// Replays (or records) the notes example's transcript of `minor`.
fn notes(minor: &str) {
    let (output, project) = build("notes", None);
    let db = TempDir::new("notes");
    let mut t = Transcript::open(minor, "notes");
    let server = start(&output, &project, &db, &[], false);
    let mut client = Client::new(&output);
    let get = |id: &str| Request::rpc("/notes.NotesProtocol/Get", &json!({ "id": id }), None);
    let stranger = "00000000-0000-4000-8000-000000000000";

    let (_, recorded) = t.exchange(
        &server,
        "a guest's call is rpcFailure(.rejected(.unauthenticated))",
        get(stranger),
    );
    let step = t.recorded_step_or_last("a guest's call is rpcFailure(.rejected(.unauthenticated))");
    let decoded = client.decode(
        &answer_with(&step, &recorded),
        &format!("p.get(null, {stranger:?})"),
    );
    t.decoded("the client decodes the guest's failure", decoded);

    t.exchange(
        &server,
        "signing up sets the session cookie",
        Request::rpc(
            "/notes.accounts.AuthProtocol/SignUp",
            &json!({ "request": {
                "email": "ada@example.com",
                "password": "correct horse battery",
                "confirmPassword": "correct horse battery",
            }}),
            None,
        ),
    );
    t.open_stream(
        &server,
        "watching the notes opens a stream",
        Request::stream("/notes.NotesProtocol/Watch", &json!({})),
    );
    let (_, snapshot) = t.read(
        "the stream starts with a snapshot",
        "watching the notes opens a stream",
        1,
    );
    let (created, _) = t.exchange(
        &server,
        "creating a note",
        Request::rpc(
            "/notes.NotesProtocol/Create",
            &json!({ "draft": { "title": "Groceries", "body": "Milk, eggs", "tags": "home" } }),
            None,
        ),
    );
    let (_, change) = t.read(
        "the stream carries the change",
        "watching the notes opens a stream",
        1,
    );
    let mut frames = snapshot;
    frames.extend(change);
    let stream_answer = json!({ "status": 200, "headers": { "content-type": "application/connect+json" }, "frames": frames });
    let decoded = client.decode(&stream_answer, "p.watch(null)");
    t.decoded("the client decodes the stream's elements", decoded);

    let id = serde_json::from_str::<Value>(&created)
        .ok()
        .and_then(|v| v["id"].as_str().map(str::to_owned))
        .unwrap_or_default();
    let title = "reading the note back";
    let (_, recorded) = t.exchange(&server, title, get(&id));
    let step = t.recorded_step_or_last(title);
    let recorded_id = step["request"]["body"]
        .as_str()
        .and_then(|b| serde_json::from_str::<Value>(b).ok())
        .and_then(|v| v["id"].as_str().map(str::to_owned))
        .unwrap_or_default();
    let decoded = client.decode(
        &answer_with(&step, &recorded),
        &format!("p.get(null, {recorded_id:?})"),
    );
    t.decoded("the client decodes the note", decoded);

    let title = "a note that doesn't exist is the declared notFound";
    let (_, recorded) = t.exchange(&server, title, get(stranger));
    let step = t.recorded_step_or_last(title);
    let decoded = client.decode(
        &answer_with(&step, &recorded),
        &format!("p.get(null, {stranger:?})"),
    );
    t.decoded("the client decodes notFound", decoded);

    let title = "the notes page embeds its k-data";
    let (recorded, sent) = t.request(title, Request::page("/"));
    let r = send(
        server.local_addr(),
        &sent.method,
        &sent.path,
        &sent.headers,
        &sent.body_bytes(),
    )
    .expect("the page answers");
    let html = String::from_utf8_lossy(&r.body).into_owned();
    assert!(
        html.contains("Sign out"),
        "the page renders for the user: {html}"
    );
    t.step(
        title,
        Some(&recorded),
        json!({ "response": head(r.status, &r.headers), "kData": k_data(&html) }),
    );
    // The recorded payload starts the current client: it is signed in (a
    // guest would be sent to /sign-in), and the page's stream, answered
    // with the recorded envelopes, shows the note.
    let recorded_data = t.recorded_step_or_last(title)["kData"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let shown = client.hydrate(
        "/",
        &recorded_data,
        &json!({ "/notes.NotesProtocol/Watch": stream_answer }),
    );
    let text = shown["text"].as_str().unwrap_or_default();
    t.decoded(
        "the client starts from the recorded k-data",
        json!({
            "path": shown["path"],
            "signedIn": text.contains("Sign out"),
            "showsTheNote": text.contains("Groceries"),
            "rpcs": shown["asked"],
        }),
    );
    t.streams.clear();
    server.shutdown().expect("stops");
    t.finish("examples/notes");
}

/// The recorded answer of step `step`, with its recorded body.
fn answer_with(step: &Value, body: &str) -> Value {
    let mut a = answer(step);
    a["body"] = Value::String(body.to_owned());
    a
}

impl Transcript {
    /// The recorded step `title` (the one just recorded, while recording).
    fn recorded_step_or_last(&self, title: &str) -> Value {
        let step = if blessing() {
            self.steps.last().cloned()
        } else {
            self.recorded.get(self.steps.len() - 1).cloned()
        };
        let step = step.unwrap_or_default();
        assert_eq!(step["title"].as_str(), Some(title));
        step
    }
}

#[test]
fn notes_answers_every_recorded_request_and_the_client_decodes_every_response() {
    for minor in minors() {
        notes(&minor);
    }
}
