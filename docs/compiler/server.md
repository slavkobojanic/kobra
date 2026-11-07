# The Server Runtime

This document specifies `k_server`: V8 through `deno_core`, HTTP through `hyper`, and everything a request
goes through, plus `dist/app`. Code generation is [codegen.md](codegen.md), the JavaScript runtime
[runtime.md](runtime.md), persistence [db.md](db.md); the overview is
[architecture.md §9](architecture.md#9-server-runtime).

Contents:

1. [Status and scope](#1-status-and-scope)
2. [Public API](#2-public-api)
3. [Process model](#3-process-model)
4. [Ops](#4-ops)
5. [Request lifecycle](#5-request-lifecycle)
6. [RPC dispatch](#6-rpc-dispatch)
7. [Server-side rendering](#7-server-side-rendering)
8. [Configuration and secrets](#8-configuration-and-secrets)
9. [Sessions, auth and request forgery](#9-sessions-auth-and-request-forgery)
10. [Endpoint pages](#10-endpoint-pages)
11. [Background jobs](#11-background-jobs)
12. [Packaging: `dist/app`](#12-packaging-distapp)
13. [Static assets](#13-static-assets)
14. [Startup and graceful shutdown](#14-startup-and-graceful-shutdown)
15. [Logging, panics and errors](#15-logging-panics-and-errors)
16. [Testing](#16-testing)
17. [Performance budgets](#17-performance-budgets)
18. [Outgoing HTTP and email](#18-outgoing-http-and-email)

---

## 1. Status and scope

| Part | Module | Status |
|---|---|---|
| Engine: isolates, module map, ops, timeouts | `engine`, `ops` | **implemented** |
| HTTP front: hyper, workers, static assets, body limit, RPC preconditions, forgery check | `http`, `front` | **implemented** |
| Graceful shutdown, `SIGTERM` | `http` | **implemented** |
| Process-wide state: sessions, rate limits, `.server` state, broadcasts | `process` | **implemented**; sessions and rate limits in the database when there is one ([§9](#9-sessions-auth-and-request-forgery)) |
| Configuration from env, `.env` and `NAME_FILE`; startup validation (exit 78) | `config`, `k_cli` | **implemented** |
| `dist/app` archive, `serve`, `migrate`, `jobs list`/`jobs retry`, `probe`, `config` | `app`, `k_cli` | **implemented** (v0 format) |
| Request handling: routing, SSR, RPC dispatch, guards, endpoint pages, sessions | `k:runtime/app` (JS) over the generated spec | **implemented** |
| Server streaming (Connect envelopes over chunked HTTP/1.1) | `k:runtime/app`, `ops`, `http` | **implemented**; back-pressure on slow readers is open |
| Database: open and migrate at startup, statements by number, request-bound transactions | `db`, `ops`, `k_db` | **implemented** ([db.md §6, §9](db.md#6-drivers-the-manifest-and-ops)); one connection per process |
| Background jobs: `DatabaseJobQueue`, the jobs worker, retries, schedules | `jobs`, `db`, `k_db::stdlib` | **implemented** ([§11](#11-background-jobs)) |
| Outgoing HTTP (`FetchHttpClient`) and SMTP email (`SmtpEmailProvider`) | `outbound`, `ops` | **implemented** ([§18](#18-outgoing-http-and-email)) |
| In-process rpc calls during SSR: guards and validation | `k:runtime/app` (`callLocal`) | **implemented** ([§6](#6-rpc-dispatch)) |
| V8 snapshot | — | v0 target, not yet built (cold start is already within budget without it) |

**Deferred:** binary Protobuf (`application/proto` is `415` in v0), client and bidirectional streaming,
HTTP/2 at the server (serve it at the proxy, design 38), TLS termination, `fetch` to third parties through
deno's web extensions (v0 ships a K-specific fetch op, [§18](#18-outgoing-http-and-email)), OAuth helpers
beyond the bound state, flushing the SSR shell before loads resolve, the `storage.lock` payload
compatibility check of `k check` (the compiler already reads job names from the lock).

**Decisions taken while implementing:**

- **The request pipeline is JavaScript.** The Rust front does only what must happen before JavaScript
  (assets, body limit, rpc preconditions, the forgery check); everything else is `k:runtime/app`
  (`crates/k_runtime/js/runtime/app.js`), driven by a plain object the generated server entry builds,
  the **spec** ([codegen.md §13–§15](codegen.md#13-services-and-dependency-injection)). The generated code
  is tables and renderers; the protocol logic (Connect errors, cookies, guards, the document) is written
  and unit-tested once (`crates/k_runtime/js/tests/server-app.test.js`).
- **Shared state lives in Rust.** Isolates share nothing, so sessions, rate-limit windows, the `@State` of
  `@Scope(.server)` services and broadcasts are in one `process::ProcessState` behind ops, the same for
  every worker. `.server` state crosses as JSON (plain data); a read-modify-write is not atomic across
  workers.
- **Sessions and rate limits follow the database.** With a database (`DATABASE_URL` and a build with
  entities) they are rows of the stdlib's `k_session` and `k_rate_limit` tables ([db.md
  §12](db.md#12-the-stdlibs-tables)), so they survive restarts and every replica sees the same ones;
  without one they live in `process::ProcessState`, in memory, with the same contract
  ([§9](#9-sessions-auth-and-request-forgery)).
- **The database is the process's, on its own runtime.** `db::ProcessDatabase` lives on the `k-db`
  thread's runtime (a Postgres connection is a task that must outlive any one request) and is shared by
  every worker through `ProcessState`; each op spawns its work there. v0 has one connection, leased to a
  transaction for its duration ([db.md §9](db.md#9-transactions)).
- **Jobs run in the worker isolates.** The jobs worker hands each run to an isolate through the request
  queues, marked as a job (`ops::JobRun`, which no HTTP request can carry), so a job gets a fresh
  container like a request and no extra isolate is needed ([§11](#11-background-jobs)).
- **Session-free endpoints never see cookies.** The front strips `Cookie` headers from requests to the
  endpoint templates listed in `ServerOptions::forgery_exempt` (the manifest's endpoints with
  `uses_session: false`), so a session cookie neither reaches a webhook nor triggers the forgery check.
- **Refused requests are drained.** A front refusal (and a streamed body over the limit) first reads and
  drops the body up to twice the limit, so a client that writes its whole request before reading sees
  the status instead of a reset connection.

---

## 2. Public API

`crates/k_server/src/lib.rs`:

| Item | Purpose |
|---|---|
| `Engine::new(EngineOptions) -> Result<Engine, EngineError>` | One isolate with the embedded runtime and `options.modules` (`JsModule { specifier, source }`), log output (`Output::{Capture, JsonLines}`) and a per-run `timeout`. |
| `Engine::run_main(code) -> Result<Option<String>, EngineError>` | Evaluates an ES module, runs the event loop to completion, returns what the code passed to `op_k_set_result`. |
| `Engine::take_output() -> Vec<LogLine>` | Captured `console`/`Log` lines. |
| `EngineError::{Load, Uncaught, Timeout, Internal}` | |
| `Server::start(ServerOptions) -> Result<Server, ServerError>` | Starts workers and the listener; returns once bound and every isolate started. |
| `Server::local_addr`, `Server::shutdown`, `Server::run_until_signal` | |
| `ServerOptions { addr, workers, bundle, assets, public_origin, max_body_bytes, shutdown_grace, log, config, forgery_exempt }` | `ServerOptions::new(addr, bundle)` fills the documented defaults; `config` is the validated configuration (`config::resolve`), `forgery_exempt` the session-free endpoint templates. |
| `ServerBundle { modules, entry, rpc_paths }`, `StaticAsset { path, content_type, body, immutable }` | |
| `front::{check, FrontPolicy, FrontResponse, has_session_cookie, looks_like_rpc}` | The pre-JavaScript checks, pure. |
| `config::{resolve, report, ConfigField, ConfigKind, ConfigValue, Environment, ProcessEnvironment, DevEnvironment, parse_dotenv, Problem, EXIT_CONFIG}` | Startup configuration; `ConfigField::from_manifest` reads a build-manifest entry, `ConfigKind::IntRange` is an `@Range` `Int`, `DevEnvironment` is `.env` under the process environment. |
| `process::{ProcessState, SessionView, random_token, base64url}` | The state every worker shares. |
| `app::{append, read, read_file, probe, AppPayload, ConfigSpec, EmbeddedFile, MAGIC}` | `dist/app`; the payload carries the config table and the session-free endpoint templates. |
| `testing::{parse_script, run_script, send, Exchange, Expect, HttpResponse}` | HTTP request scripts ([testing.md §8.2](testing.md#82-server-end-to-end-tests)). |
| `testing::{open_stream, StreamingResponse, StreamChunk}` | A request whose response body is read as it arrives (`next_chunk(wait)`), for a server stream; dropping it closes the connection ([testing.md §4.3](testing.md#43-browser-tests)). |
| `ServerOptions { fetch_origins }`, `outbound::replace_origin`, `ProcessState::{with_fetch_origins, fetch_origins}` | `(origin, replacement)` pairs: an outgoing `HttpClient` request to that exact origin (or below it) goes to the replacement, for tests that stand a `FakeHttp` in for a third party the app hard-codes. Empty in production: `k run` and `dist/app` never set it. |
| `ServerOptions { jobs, job_poll }` | The build's job specs (`k_db::stdlib::JobSpec`, from the manifest) and how often the worker polls (250 ms). |
| `jobs::{JobWorker, JobWorkerOptions, Dispatch, fate, job_id, now_ms}` | The background jobs worker ([§11](#11-background-jobs)). |
| `outbound::{fetch, send_mail, build_message, FetchRequest, FetchOutcome, SmtpSettings, SmtpSecurity, MailMessage, MailOutcome}` | Outgoing HTTP and SMTP ([§18](#18-outgoing-http-and-email)). |
| `fakes::{FakeHttp, FakeRoute, RecordedRequest, FakeSmtp, SmtpBehavior, ReceivedMail}` | Fake third parties for tests ([testing.md §8.3](testing.md#83-fake-third-parties)). |
| `db::ProcessDatabase::{open_with, enqueue_job, claim_jobs, finish_job, list_jobs, requeue_job, claim_schedule, …}` | The stdlib tables' operations; `open_with(setup, false)` opens without migrating (`jobs list`). |
| `ops::hmac(bits, key, message)` | HMAC-SHA-256/512 (RFC 2104), behind `op_k_hmac`. |

**One engine per thread.** An `Engine` is not `Send`, and at most one may be alive on a thread (V8 enters
one isolate per thread); servers and test drivers create one per thread.

---

## 3. Process model

```text
                       ┌─────────── tokio multi-thread runtime (thread "k-http") ───────────┐
TCP ── accept ── hyper http1 ── static asset? ── front::check ── round robin ─┐            │
                       └──────────────────────────────────────────────────────┼────────────┘
                                                                              ▼
                       worker thread "k-worker-i": current-thread tokio + one JsRuntime
                       entry module: await serve(handler)  ◀── op_k_next_request / op_k_respond
```

- **N worker isolates**, default one per core. Each runs the bundle's entry module, which calls `serve()`;
  requests interleave on the isolate's event loop, so a handler awaiting the database doesn't block the
  worker.
- **Dispatch** is round robin over unbounded per-worker queues (v0); a worker whose isolate died answers
  `503` through its closed queue. *Planned*: least-loaded dispatch.
- **Isolation.** Isolates share nothing: no JavaScript state crosses requests on different workers, and
  within a worker request state lives only in the request's container ([§5](#5-request-lifecycle)).
- **Globals** inside an isolate: what V8 provides (`JSON`, `Math`, `Date`, `Intl`, `Promise`, typed
  arrays), plus a bootstrap that installs `console`, `queueMicrotask`, `AbortController`/`AbortSignal`,
  `TextEncoder`/`TextDecoder`. **No ambient I/O**: no `fetch`, `require`, `process` or `Deno.*` file APIs;
  every effect on the world goes through a K op. (A test asserts it.)

---

## 4. Ops

Ops are the only way server JavaScript reaches Rust (`Deno.core.ops.*`):

| Op | JS signature | Status |
|---|---|---|
| `op_k_log` | `(level, message) => void` | implemented: captured, or one JSON line on stdout |
| `op_k_set_result` | `(text) => void` | implemented (drivers and tests) |
| `op_k_test_reset` | `() => void` | implemented (`k test`): a fresh process state, and a fresh in-memory database with the build's migrations when the engine has a test database (`Engine::set_test_database`) |
| `op_k_test_tick` | `() => Promise<void>` | implemented (`k test`): one turn of the event loop, so completed ops deliver between runs of microtasks |
| `op_k_next_request` | `() => Promise<Request \| null>` | implemented: `null` once the server shuts down |
| `op_k_respond` | `(id, {status, headers, body}) => void` | implemented: body is a string or `Uint8Array`; a status outside `100..=999` (or not an integer) is a generic `500` |
| `op_k_respond_start` / `op_k_respond_chunk` / `op_k_respond_end` | `(id, {status, headers})`, `(id, bytes) => open`, `(id)` | implemented: a streamed body (chunked) |
| `op_k_server_info` | `() => {publicOrigin, config}` | implemented: config values typed, secrets as `{"$secret": v}` |
| `op_k_now` | `() => number` | implemented: epoch ms; `TestClock` replaces it in tests |
| `op_k_random_token` | `() => string` | implemented: 32 CSPRNG bytes, base64url (43 characters) |
| `op_k_utf8_valid` | `(bytes) => boolean` | implemented: behind the bootstrap's `TextDecoder` with `fatal: true`, which rpc bodies, `request.text()` and `request.json()` use, so a body that isn't UTF-8 is refused, never decoded lossily |
| `op_k_session_get` / `_create` / `_delete` / `_end_all` | the session store; each returns a promise (a database read or write when the server has one) | implemented ([§9](#9-sessions-auth-and-request-forgery)) |
| `op_k_session_issue_state` / `_consume_state` | `(id) => Promise<state \| null>`, `(id, state) => Promise<boolean>`: OAuth state bound to the session row | implemented ([§9](#9-sessions-auth-and-request-forgery)) |
| `op_k_rate_limit` | `(key, limit, windowMs, nowMs) => Promise<retryAfterSeconds>` (`0` allowed) | implemented: sliding window per key, in `k_rate_limit` with a database; with `nowMs ≥ 0` (a test's virtual time) counted exactly in memory |
| `op_k_shared_get` / `op_k_shared_set` | `(key) => json \| null`, `(key, json)` | implemented: `.server` service state |
| `op_k_broadcast_subscribe` / `_publish` / `_next` / `_close` | keyed broadcasts | implemented: a subscriber with 1,000 undelivered elements, or whose session ends, is closed |
| `op_k_sleep` | `(ms, signal) => Promise<void>` | v0: cancellable |
| `op_k_db_query` | `(statement, params, tx) => Promise<row[]>` (a row: values in select order; `tx` 0: none) | implemented ([db.md §6](db.md#6-drivers-the-manifest-and-ops)) |
| `op_k_db_execute` | `(statement, params, tx) => Promise<{count} \| {unique: field}>` | implemented |
| `op_k_db_begin` / `op_k_db_finish` | `() => Promise<tx>`, `(tx, commit) => Promise<void>`: transactions bound to the request | implemented |
| `op_k_db_savepoint` | `(tx, depth, op)`: 0 `SAVEPOINT`, 1 `RELEASE`, 2 `ROLLBACK TO` and `RELEASE` | implemented |
| `op_k_db_ping` | `(timeoutMs) => Promise<boolean>`, `false` without a database | implemented |
| `op_k_jobs_enqueue` | `(name, payloadJson, delayMs, tx) => Promise<id>`: a `k_job` row in transaction `tx` (0: on its own) | implemented ([§11](#11-background-jobs)) |
| `op_k_password_hash` / `op_k_password_verify` | `(password) => Promise<string>`, `(password, hash) => Promise<boolean>`: Argon2id (m = 19 MiB, t = 2, p = 1) PHC strings on the blocking pool; a malformed hash matches nothing (`tests/e2e/passwords`) | implemented |
| `op_k_hmac` | `(bits, key, message) => Uint8Array`: HMAC-SHA-256 (`bits` 256) or -512 | implemented (`Hmac.sha256`/`sha512`; `Mac.matches` compares in constant time in JavaScript) |
| `op_k_hash` | crypto (`Hash`) | v0, not built yet |
| `op_k_fetch` / `op_k_fetch_abort` | `(id, {method, url, headers, timeoutMs}, body, hasBody) => Promise<outcome>`, `(id) => void` | implemented ([§18](#18-outgoing-http-and-email)) |
| `op_k_smtp_send` | `(settings, message) => Promise<outcome>` | implemented ([§18](#18-outgoing-http-and-email)) |

The request handed to JavaScript is `{ id, method, path, headers: [[name, value]…] (lower case, in order),
body: Uint8Array, remote, job }` (`remote` the peer's IP address; `job` is `null` for HTTP and
`{name, attempt}` for a job run, whose body is the payload JSON, [§11](#11-background-jobs)); the response is `{ status, headers: [[name, value]…], body: string | Uint8Array }`.

---

## 5. Request lifecycle

```text
hyper request
  ├─ GET/HEAD of a static asset ─────────────▶ bytes from memory, immutable caching (§13)
  ├─ front::check ── refuse? ────────────────▶ 405 / 413 / 415 / 403 / 404 without running JavaScript
  ├─ read the body, at most max_body_bytes ─▶ 413 if a streamed body exceeds it
  └─ worker: serve(createApp(spec))           k:runtime/app over the generated spec
          rpc path ─▶ (§6) │ endpoint match ─▶ (§10) │ GET/HEAD: redirect ─▶ 308 │ page ─▶ (§7) │ 404
          other methods on a page ─▶ 405 Allow: GET, HEAD
          RequestState: cookies, the session (loaded on first use), the request container
          guards (`requires`) ─▶ redirect / 401 / 403 / 429
          the work ─▶ response; Set-Cookie when the session changed, Cache-Control when it was read
```

- **Front checks** (`front::check`, unit-tested in full): a declared `Content-Length` over the limit →
  `413` (`resource_exhausted` JSON for an rpc); an rpc-shaped path (`/{package}.{Service}/{Method}`) the
  app doesn't have → `404` Connect `unimplemented` (never cached); an rpc not `POST` → `405` with
  `Allow: POST`; without `Content-Type: application/json` (or `application/connect+json`, which only a
  streaming rpc accepts) or `Connect-Protocol-Version: 1` → `415`; a non-`GET`/`HEAD` request
  with the session cookie and neither `Origin` equal to `K_PUBLIC_ORIGIN` nor `Sec-Fetch-Site:
  same-origin` → `403`.
- **Slow clients** (slowloris): a connection must send a request's headers within
  `front::HEADER_READ_TIMEOUT` (10 s, hyper's header read timeout over a tokio timer) and its body
  within `front::BODY_READ_TIMEOUT` (20 s), or it is closed (a stalled body gets `408` and
  `Connection: close`); no client holds a connection, a task or a worker forever.
- **Every response** carries `X-Content-Type-Options: nosniff`; rpc responses also
  `Cache-Control: private, no-store`, as does every response whose rendering read `Session`.
- **Request scope.** Each request gets a fresh request container ([codegen.md §13](codegen.md#13-services-and-dependency-injection)):
  `.app`, `.request` and `.route` services, the server-side instances of client services, and the
  `Session` mirror. It is dropped with the response; nothing leaks to the next request.
- **Cancellation.** The request's `AbortSignal` (`$c`) is aborted when the client disconnects; handlers'
  database and fetch calls observe it. (v0 creates the signal; aborting it on disconnect is planned.)
- **Broken handlers.** A handler that throws a non-`Error`, rejects, or returns something that isn't a
  response costs that request a `500` whose body says nothing about the failure; the worker's loop goes
  on (`http/broken-handlers`).
- **Client address**: the socket address, or the right-most `X-Forwarded-For` hop that isn't in
  `K_TRUSTED_PROXIES` ([27-endpoint-pages.md](../design/27-endpoint-pages.md#client-address-and-proxies)).
  v0 uses the socket address (`X-Forwarded-For` is ignored until trusted proxies are read).

---

## 6. RPC dispatch

Wire format: [28-rpc-wire-format.md](../design/28-rpc-wire-format.md). v0 is JSON only: unary rpcs and
server streaming.

- **Path**: `POST /{package}.{Service}/{Method}`, names from `protocol.lock` (`/shop.store.StoreProtocol/PlaceOrder`).
- **Request**: a generated message whose fields are the parameters' internal names (`{"request": {...}}`);
  `{}` for no parameters. **Response**: a struct return is the message itself, other types
  `{"value": …}`, no return type `{}`.
- **Handler steps**: decode (a body that isn't valid UTF-8 or isn't a JSON object, or a field that
  doesn't decode → `invalid_argument`; unknown fields are ignored; typed text values such as `Url`,
  `EmailAddress`, `Uuid` and `Color` must parse, [codegen.md §5.1](codegen.md#51-json-codable)) → validate
  every `Validatable` value in the parameters, also inside optionals, lists and sets and in nested
  fields (every field's first failing constraint, `@Email`, `@Matches` and `@Check` included, then
  `validate(rules:)` when the fields pass; failure → `invalid_argument` with a `k.ValidationError` detail
  `{"failures": [{"path": "requests[1].handle", "message": …}]}`; `@Check` runs the named service
  function in full, in the request container) → guards (the
  protocol's `requires`, then the rpc's) → call the implementation in the request container → encode.
- **Server streaming** (`-> Stream<T>`): the request is one Connect envelope (`[flags 0][length u32
  BE][JSON]`) with `Content-Type: application/connect+json`; decode, validation, guards and an error the
  handler throws before returning the stream answer like a unary rpc (a status and a Connect error
  body). Then `200` with `application/connect+json`, one envelope per element (a struct is the message,
  anything else `{"value": …}`), and an end-of-stream envelope (flags `0x02`) `{}` or `{"error": {…}}`.
  A stream ends when its source does, when the client goes away, or when the session it was opened under
  ends on any worker. An enveloped request to a unary rpc is `415`.
- **Errors** (Connect error body `{"code", "message", "details"}`):

  | Outcome | Code | HTTP |
  |---|---|---|
  | declared error case | `failed_precondition`, detail `{"type": "k.TypedError", "debug": {"error": "shop.cart.CartError", "case": "outOfStock", "fields": {"available": 3}}}`; a `Codable` error's `fields` are its codec's encoding without `case` (an enum payload as its case name, a `Date` as RFC 3339), which the client decodes with the same codec; any other error's are the payload's structural JSON | 400 |
  | invalid request | `invalid_argument`, detail `k.ValidationError` with field paths | 400 |
  | `Authenticated` refused | `unauthenticated` | 401 |
  | `Role` refused | `permission_denied` | 403 |
  | `RateLimit` refused | `resource_exhausted`, `Retry-After` | 429 |
  | body over 1 MB | `resource_exhausted` | 413 |
  | wrong content type / no `Connect-Protocol-Version` | — | 415 |
  | unknown rpc | `unimplemented` | 404 |
  | panic | `internal`, no details outside development | 500 |

- **In-process calls** (a service holding the protocol during server rendering, or any server code):
  the request container's `Protocol$local` client calls `$s.callLocal(c, $spec.rpcs[path], $c, args)`,
  which runs the rpc's validation and then its guards exactly as above, against the same request
  (session, client address, rate-limit windows), and then the implementation in the same container. A
  failure is the rpc's own error, through the table entry's `failure: (f) => E.rpcFailure(f)`:
  `rpcFailure(.invalid(ValidationError))` with the failures, or `rpcFailure(.rejected(…))` with
  `GuardRejection.unauthenticated`, `.forbidden` or `.rateLimited(retryAfter:)`. `RpcFailure`'s cases
  are tagged in declaration order (rejected, invalid, offline, timeout), the same in the browser's
  client (`rpcError`, [runtime.md §8](runtime.md#8-rpc-client)), so a page's `catch .rpcFailure(.rejected)`
  behaves alike on both sides (`tests/e2e/in-process-rpc`).
- **Dispatch overhead budget**: ≤ 50 µs at p50 excluding the handler; a generated rpc's whole loopback
  round trip is 47 µs p50 against 45 µs for a hand-written trivial handler, so the dispatch through
  `k:runtime/app` costs about 2 µs ([§17](#17-performance-budgets)).

---

## 7. Server-side rendering

- A view page request: match the route (typed parameters; a failed parse is a `404`) → guards (a refused
  `Authenticated` is `303 See Other` to the `@SignInPage` with `returnTo` set to the refused page's URL;
  `Role` is `403`; `RateLimit` is `429` with `Retry-After`) → request container → the page's `$ssr`
  renderer, which **awaits its `@Load`s** in source order (each after the loads it reads) → each layout
  around it, innermost first, its slot writing the page's HTML → the HTML document.
- **Metadata** (22-metadata.md): the app's `metadata { }`, then each layout's (outermost first), then
  the page's, each entry replaced by a later layer. A page's metadata is evaluated after its loads; when a
  load fails, the `failed` block's own `metadata { }` replaces the page's, which isn't evaluated.
  `canonical(.page)` is made absolute with the public origin; every value is escaped.
- **`Theme` during server rendering** is `theme(st)` (`app.js`): `current` is the variant the `k_theme`
  cookie remembers, or `base`; `apply` and `followSystem` do nothing.
- **The document**: `<!doctype html><html lang="…">` (the `locale`; theme classes from the `k_theme`
  cookie are planned), `<head>` with `charset`, `viewport`, the metadata and the stylesheet links, then
  `<body><div id="k-app">…</div>`, the `k-data` script ([runtime.md §6](runtime.md#6-hydration-protocol):
  `{"v":1,"now":…,"session":{"user":…},"c":{"0":{"loads":…}}}`) and the app chunk's module script (it
  imports the runtime, shared and page chunks). The asset names are the hashed client files of the same
  build, written into the spec.
- **Failed loads** render `failed` with the typed error (serialized for hydration as
  `{"failed": <the error's codec>}`); `status(_:)` in its `metadata` sets the HTTP status (`404` for not
  found). An error with no `failed` block reaches the nearest `ErrorBoundary`; a panic is a `500` page
  without `k-data`.
- **Rendering is a function of the request**: label ids are counted per request (`Out.uid`, shared by
  the page's and its layouts' buffers), never by an isolate-global counter, so the same page renders the
  same HTML on every request and worker and reveals nothing about earlier requests.
- **Effects never run** on the server, and client-only services keep their initial state
  ([10-services.md](../design/10-services.md#serverclient-boundary)).
- **Streaming**: v0 renders the whole document, then sends it with `Content-Length`. Flushing the shell
  and each loading boundary as it resolves is *deferred*.
- With `@Rendering(.client)` the server sends one shell with the app-wide metadata, no body and no `k-data`
  for every page the route table matches (other paths are still `404`).
- Budget: SSR of a 1,000-node page with data loaded ≤ 3 ms; measured at 0.16 ms p50 for the whole
  loopback round trip ([§17](#17-performance-budgets)).

---

## 8. Configuration and secrets

[25-config-and-secrets.md](../design/25-config-and-secrets.md), implemented by `config::resolve`:

- The build manifest lists every `@Env` field reachable from the build environment's dependency graph
  (`server::reachable` in `k_codegen`: from pages, layouts, components, endpoints and the services that
  implement a protocol or a stdlib interface, through `@Inject`, an interface following its `use` rule
  for that environment), so a config only the production implementation injects isn't required by
  `k run`/`k dev`. The stdlib's own configs are always listed (`PORT`, `K_PUBLIC_ORIGIN`, `K_SHUTDOWN_GRACE`, `K_TRUSTED_PROXIES`, `DATABASE_URL`,
  `DATABASE_PASSWORD`, `K_JOB_CONCURRENCY`, `K_SESSION_MAX_AGE`, `K_SESSION_IDLE`, SMTP). The runtime reads
  no other variable.
- For each field: `NAME_FILE` (secrets only, one trailing `\n` or `\r\n` removed) → `NAME` → the default →
  missing. Both set, or `NAME_FILE` for a non-secret, is a problem.
- Values are parsed by kind: `Int` (safe integers), `Double` (finite), `Bool` (`true`/`false`), `Duration`
  (`300ms`, `30s`, `5m`, `2h`, `1d`, decimals allowed), enums by case name, `List<T>` comma-separated,
  `String`; constraint annotations run after parsing.
- **All problems at once**, sorted by variable, printed by `config::report`, then exit **78**, before any
  port or database connection is opened.
- **Secrets are never printed**: a malformed or constraint-failing secret shows as `Secret(•••)`, and the
  `Debug` form of a secret value is `Secret(•••)` (tests assert both). `dist/app config` prints the table
  of variables, never values of secrets.
- **Where values come from.** `dist/app serve` resolves the archive's config table against the process
  environment; `k run` (and the e2e harness) against the **development environment**,
  `config::DevEnvironment`: an optional `.env` at the project root under the process environment (which
  wins; the harness leaves the process out), `NAME_FILE` paths relative to the project root. The stdlib's
  server variables are always read; `K_PUBLIC_ORIGIN` falls back to `http://localhost:{PORT}`. A
  relative `sqlite:` path in `DATABASE_URL` is relative to the project root too, and its directory is
  created (`DatabaseSetup::rooted_at`), for `k run`, `k dev` and `k migrate` alike, so each member that
  `k dev` runs at a workspace root has its own database (`sqlite:.k/dev.sqlite` is
  `<member>/.k/dev.sqlite`). `dist/app` keeps the working directory.
- A `Secret<T>?` field is a secret like `Secret<T>`: it may come from `NAME_FILE`, is never printed, and
  reaches JavaScript as `Secret` or `null`.
- Values reach JavaScript once per isolate through `op_k_server_info`, already typed (a `Duration` in
  ms, an enum by case name, a list as an array). The generated `C$load()` wraps a secret in `Secret`
  (`$s.configSecret`), whose text and JSON forms are `Secret(•••)`; only `reveal()` and `matches(_:)` read
  it. *Planned*: opaque handles that only ops can open.

---

## 9. Sessions, auth and request forgery

[26-auth-and-sessions.md](../design/26-auth-and-sessions.md):

- **Storage**: with a database, `k_session` in the app's database ([db.md §12](db.md#12-the-stdlibs-tables),
  outside `schema.lock`): the id's SHA-256 (the cookie's token is 32 random bytes, base64url; the
  table alone never holds a usable session), the user id as JSON and its text form for `endAll`, the
  roles, created and last-seen times, and the bound OAuth state's hash with its expiry. Sessions
  therefore survive a restart and are shared by every replica, and revocation is immediate everywhere
  (`crates/k_server/tests/sessions.rs`). Without a database the same rows live in
  `process::ProcessState`, in memory. The session is loaded once per request, before it is handled
  (`RequestState.load()`, an async `op_k_session_get`), so reading `Session` stays synchronous. The
  cookie holds only the token:
  `HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000`, plus `Secure` and the `__Host-k_session` name when
  `K_PUBLIC_ORIGIN` is `https`, `k_session` otherwise; signing out sends it empty with `Max-Age=0`. A
  cookie the server never issued (or one that expired or was revoked) is a guest.
- **Lifetime**: idle timeout (default 7 days, sliding; a stored session's last-seen time is written at
  most once a minute) and
  absolute lifetime (30 days from `signIn`), or `K_SESSION_IDLE` and `K_SESSION_MAX_AGE` from the
  validated configuration (`ProcessState::new` reads them). `signIn` always issues a new id (no fixation); `signOut` and
  `endAll` delete rows, so revocation is immediate.
- **OAuth state**: `issueState()` binds a fresh random state (43 characters) to this browser's session
  row, stored as its hash with a 10-minute expiry (`process::OAUTH_STATE_LIFETIME`); a visitor without a
  session first gets a **guest session** (a row without a user, and its cookie) to bind it to.
  `consumeState(state)` answers `true` once, for the state last issued to the same session before it
  expires, comparing hashes in constant time; it clears the state either way, so a wrong guess burns
  it, another browser's cookie can't use it, and a replay fails. A guest session reads as no user;
  `signIn` replaces it with a new id like any session (no fixation), and `endAll` never ends it.
- **The mirror**: SSR serializes `Session.user` into `k-data`; an rpc that changes the session returns the
  new mirror in `x-k-session` so the browser's `Session` follows.
- **Guards** run on the server for every request: `Authenticated`, `Role(…)` (`Authenticated` first, so a
  guest is `401`/redirected rather than `403`), `RateLimit(…)` (a sliding window per guard, counted per
  live session when signed in and per client address otherwise; a refusal carries `Retry-After` in
  whole seconds), and on in-process rpc calls during SSR ([§6](#6-rpc-dispatch)). With a database the
  window is `k_rate_limit`'s two counters per key (the previous and the current fixed window, weighted
  by the time elapsed, `k_db::stdlib::rate_decision`), so every replica counts against the same limit;
  the read-then-increment isn't atomic across replicas, so a burst racing on several replicas can pass
  a few extra calls. Without a database it is an exact sliding log in memory.
- **Responses**: a response whose handling read `Session` (or changed it) is `Cache-Control: private,
  no-store`; one that signed in or out carries `Set-Cookie`; an rpc that changed the session also
  `x-k-session` with the new mirror (`{"user": {"id": …, "roles": […]}}` or `{"user": null}`).
- **Request forgery**: the front's checks of [§5](#5-request-lifecycle) are always on: rpc content types
  and the Connect header, and the `Origin`/`Sec-Fetch-Site` check for any state-changing request carrying
  the session cookie. CORS is never granted for rpc routes.

---

### 9.1 Service tokens

An rpc whose protocol (or itself) `requires ServiceAuthenticated`
([40-workspaces-and-services.md](../design/40-workspaces-and-services.md)) admits only calls carrying a
valid service token in `x-k-service-token`, signed with the app's `K_SERVICE_TOKEN` (the stdlib's
`ServiceConfig`, a required secret of every app that serves such a protocol). The token format and its
checks are in [runtime.md §8.1](runtime.md#81-remote-clients); a missing, forged or expired token is `401
unauthenticated`. The front's request-forgery check doesn't apply (a service call carries no session
cookie), and in-process calls skip the guard. `ServiceCaller.name` is the verified caller's name for the
rest of the request. The calling side's client posts with `op_k_fetch`, so `fetch_origins` in tests
apply to it too.

## 10. Endpoint pages

[27-endpoint-pages.md](../design/27-endpoint-pages.md): the endpoint table maps a URL template and method
to a generated handler that builds an `IncomingRequest` (lower-cased headers joined with `, `, `rawBody`
as received, the client address), calls `func name(request:) async -> HttpResponse` and writes the
response. An unhandled method is `405` with `Allow`; `HEAD` follows `GET`; bodies over 1 MB are `413` before
the handler. Endpoint pages that don't inject `Session` never see the cookie (the front strips `Cookie`
for their templates, [§1](#1-status-and-scope)), so webhooks need no forgery check; those that do are
subject to it and their responses are private. Setting `set-cookie` (any letter case) from a handler
panics. The handler's `$e` carries the parsed `@Param`s and the request container its `@Inject`s read;
`HttpResponse`'s builder, `request.headers[…]`, `.all(…)`, `json()` and `text()` are runtime natives
(`$s.response`, `$s.header`, …). The request URL is the public origin plus the request target.

---

## 11. Background jobs

[30-background-jobs.md](../design/30-background-jobs.md), `jobs`, `db`, `k_db::stdlib`:

- **The build's jobs** are the manifest's `jobs` ([codegen.md §15.4](codegen.md#154-background-jobs)): each
  job's name (`@JobName`, else the name `storage.lock` records for its declaration, else its service
  name in kebab case), its `@Retry` attempts and first backoff (default 5 attempts from 10 s), and its
  `@Schedule`. They travel in `ServerOptions::jobs` and `AppPayload::jobs`.
- **`k_job`** ([db.md §12](db.md#12-the-stdlibs-tables)): id (16 random bytes, base64url), name, payload
  JSON, status (`queued`, `running`, `dead`), attempts, run after, lease end, last error, timestamps.
- **Enqueueing** (`DatabaseJobQueue`, `$s.jobQueue(Database)` in the request container):
  `op_k_jobs_enqueue` writes the row in the request's open transaction (`Database.tx`), so it commits
  or rolls back with it (also with a savepoint), or on its own outside one. A failure to store is a
  panic. Without a database, `enqueue` panics with the reason.
- **The worker** (`jobs::JobWorker`, thread `k-jobs`) starts with the server when the build has jobs
  and a database (`k run`, `k dev` and `dist/app serve` alike); without a database the server logs a
  warning and runs no worker. Every `job_poll` (250 ms) it enqueues due schedule runs, then claims up to
  `K_JOB_CONCURRENCY` (default 4) jobs minus those running. A claim is a conditional update
  (`queued` and due, or `running` with an expired lease) with a 15-minute lease, so replicas racing for
  a row see one winner, and a job whose process died runs again once its lease expires (at least
  once). Each run goes to a worker isolate like a request, marked as a job; the isolate runs it in a
  fresh container with no session (reading `Session` there panics) and answers `{"outcome": "done" |
  "retryable" | "permanent" | "panic", "reason"}`.
- **Fates** (`jobs::fate`): done deletes the row; `JobError.retryable` or a panic with attempts left is
  queued again after `backoff × 2^(attempt−1)` (capped at a day, jittered ±20%); `permanent`, or the
  last attempt, leaves it `dead` with its reason, logged at error level. Dead jobs are never retried
  automatically.
- **Schedules** (`k_db::stdlib::Schedule`: `.every(interval)`, `.daily(at:)`, `.weekly(on:at:)`, UTC):
  each due time is enqueued once across replicas, claimed through `k_job_schedule` (the replica whose
  update moves `last_due` forward enqueues it).
- **Shutdown**: the worker stops claiming, waits for running jobs up to `K_SHUTDOWN_GRACE`, and returns
  unfinished ones to the queue without counting the attempt; only then are the request queues closed.
- **Commands**: `dist/app jobs list [--dead]` prints one tab-separated line per job (id, name, status,
  attempts, due or death time in RFC 3339, last error); `dist/app jobs retry <id>` re-queues a dead job
  with fresh attempts (exit 1 when there is none). Neither runs migrations.
- **Tests**: `tests/e2e/jobs` (enqueue, transactional enqueue and rollback, retry then success, dead
  after the last attempt, permanent failure, delay, schedule), `db.rs` unit tests (claims, leases,
  fates, schedules), `jobs.rs` unit tests (fates, jitter), `crates/k_cli/tests/cli.rs` (`migrate`, `jobs`
  commands) and the shop's production test, where a paid order's confirmation is sent by the worker.
- **Test doubles**: a test build binds `JobQueue` to `RecordingJobQueue` (stores payloads,
  `payloads(of:)`), and `use InlineJobQueue for JobQueue` runs each job when the enqueuing transaction
  commits (`Database.afterCommit`), failing the caller on a retryable failure.

---

## 12. Packaging: `dist/app`

`dist/app` is a copy of the runtime executable with the app appended as a trailer:

```text
[executable bytes][payload JSON][payload length: u64 little-endian][magic "K-APP-v1"]
```

- The **payload** (`app::AppPayload`): the app name, the server modules (`{path, text}`), the entry
  module, the rpc paths, the hashed client assets, the **config table** (`ConfigSpec`, the manifest's
  `@Env` fields) and the session-free endpoint templates. The last two default to empty when reading an
  older payload (the server's own variables are then assumed). Migrations and (once built) the V8
  snapshot are added with the features that use them; the magic's version changes with an incompatible
  format.
- `app::read_file` reads the last 16 bytes, then only the payload, so the check is cheap for the plain `k`
  binary.
- **v0 runtime executable**: `k` itself. At startup `k` checks its own executable for a trailer and, if
  there is one, behaves as the app: `serve` (default), `migrate`, `jobs`, `probe <url>`, `config`. A slim runtime binary
  without the compiler is *deferred*.
- Commands ([38-deployment.md](../design/38-deployment.md#commands)), all implemented: `serve`
  (configuration → database and pending migrations, exit 1 on failure → listener → jobs worker),
  `migrate` (configuration, then the pending migrations; prints each applied one, or `the database is up
  to date`; exit 0, 1 when a migration failed and was rolled back, 78 for invalid configuration),
  `config` (each variable with its type, whether it is required, its default and whether it is a secret;
  never a value), `probe <url>` (exit 0 on a 2xx within 3 s, 1 otherwise; HTTP only), `jobs list
  [--dead]` and `jobs retry <id>` ([§11](#11-background-jobs)). The payload carries the build's database
  manifest (`AppPayload::database`) and job specs (`AppPayload::jobs`).
- **`k build`** writes `dist/app`, `dist/public/assets/*` and `dist/manifest.json` from a release build
  for production; **`k run`** serves a development build on `--port` (default 8080) until `SIGTERM` or
  Ctrl-C, with the development environment of [§8](#8-configuration-and-secrets). **`k dev`** serves the
  same development build and watches the project (every file below its root except hidden directories,
  `target`, `dist` and `node_modules`, plus `.env`; polled every 250 ms by size and modification time).
  On a change it rebuilds and restarts the server on the same port; there is no HMR in v0, so a restart
  drops in-memory state such as sessions. The first build must succeed (it fails like `k run`); after
  that a build with errors is reported and the last good build keeps serving, and a restart that fails
  (a configuration problem) is reported and retried on the next change. `k run` and `k dev` run the jobs
  worker too, against the development database ([§11](#11-background-jobs)).

---

## 13. Static assets

Assets are served from memory by the Rust front: `GET`/`HEAD` of `/assets/<hashed name>` →
`Cache-Control: public, max-age=31536000, immutable` and the content type by extension; unhashed files
(`index.html`) get `no-cache`. `HEAD` sends no body. Precompressed variants (brotli/gzip chosen by
`Accept-Encoding`) are *v0, planned*.

---

## 14. Startup and graceful shutdown

- **Startup order** (design 38): configuration (exit 78 on any problem) → database connection →
  pending migrations (exit 1 on failure, before the port opens; `ServerError::Database`, whose message
  has neither the URL nor the password) and the stdlib's tables → worker isolates → jobs worker →
  listener.
  `Server::start` returns only after every isolate has started. Each applied migration is logged as
  `applied migration <name>`.
- **Shutdown** (`SIGTERM`/`SIGINT` in `run_until_signal`, or `Server::shutdown`): stop accepting; let open
  connections finish their in-flight requests through hyper's graceful shutdown, for at most
  `K_SHUTDOWN_GRACE` (default 25 s); then stop the jobs worker ([§11](#11-background-jobs)); then close
  the worker queues, so each `serve()` returns once the
  requests already queued are answered; join the workers; exit 0. A worker whose entry module failed is
  reported as `ServerError::Worker`.

---

## 15. Logging, panics and errors

- Logs are JSON lines on stdout: `{"level": "...", "message": "..."}` (v0), with request ids and K stack
  traces from `k.observe` later. A panic is logged with the method, the path and the JavaScript stack; a
  worker whose entry module fails logs why at once (and answers `503` from then on). Fields named `password` and `Secret` values are redacted by the checker's
  warnings and by `Secret`'s text form.
- **A panic in a handler** is a `500` with a request id and no details in production; in development the
  page shows the K stack trace mapped through the source maps. The handler's error never reaches the
  client otherwise (an e2e case asserts the message is absent).
- **Infrastructure failures** (a lost database connection) are panics ([09-errors.md](../design/09-errors.md#panics)).
- **Never panic in Rust on input**: malformed HTTP is hyper's `400`, oversized bodies `413`, invalid
  headers are skipped when copying a response, and a response from JavaScript with an invalid status is
  `500`.

---

## 16. Testing

- `crates/k_server/tests/engine.rs`: loading, results, output capture, uncaught errors, syntax errors,
  timeouts, isolation, no ambient I/O.
- `crates/k_runtime/js/tests/server-app.test.js`: `k:runtime/app` against hand-written specs (routing and
  typed parameters, Connect errors, validation, guards, endpoints, sessions, `.server` state).
- `crates/k_server/tests/stage3_server.rs`: what a script can't express (cookie attributes, session
  fixation, `x-k-session`, `Retry-After`, `k-data` as JSON, streaming envelopes, body limits on generated
  handlers, the split on served assets, the manifest's config table, `dist/app` end to end). One test
  stays pending: closing a subscriber that falls 1,000 elements behind, which TCP buffering hides from the
  server over loopback.
- Unit tests in `process.rs` (tokens, sessions, lifetimes, rate windows, broadcasts), `config.rs`
  (manifest kinds, ranges, `.env`, the development environment) and `http.rs` (endpoint templates).
- `crates/k_server/tests/runtime_js.rs`: the runtime's JS unit tests ([testing.md §4.1](testing.md#41-runtime-js-tests)).
- `crates/k_server/tests/corpus_run.rs`: the execution corpus ([testing.md §3.10](testing.md#310-the-run-suite)).
- `crates/k_server/tests/e2e.rs`: request scripts against hand-written bundles (`tests/http/*`) and K
  projects (`tests/e2e/*`) ([testing.md §8.2](testing.md#82-server-end-to-end-tests)).
- `crates/k_server/tests/server.rs`: streamed body limits, concurrency, graceful shutdown with a request in
  flight, start-up failures.
- `crates/k_server/tests/config.rs`: configuration sources, grammars, reporting and redaction.
- `crates/k_cli/tests/cli.rs`: an appended app serves, answers `probe`, exits 0 on `SIGTERM` and 78 on
  invalid configuration.
- `crates/k_server/tests/outbound.rs`: `fetch` and `send_mail` against the fakes (responses, timeouts,
  refused connections, invalid requests, SMTP reply classes, header injection, no secret in a message).
- `crates/k_server/tests/sessions.rs`: sessions in `k_session` survive a restart and are shared by two
  replicas (sign-in, `endAll`, revocation), the table holds hashes only, rate limits count across
  replicas, OAuth state is bound and single-use.
- `crates/k_server/tests/shop_production.rs`: the shop built for production against a fake Stripe and a
  fake SMTP server: `PlaceOrder` starts a Checkout session (the exact form, bearer token and
  idempotency key Stripe receives), 4xx/5xx/slow answers cancel the order and release the stock, the
  signed webhook marks it paid, and the jobs worker sends exactly one confirmation email.
- `tests/e2e/http-client`, `tests/e2e/email`, `tests/e2e/jobs`, `tests/e2e/in-process-rpc`: request
  scripts with fakes ([testing.md §8.3](testing.md#83-fake-third-parties)).
- `crates/k_runtime/js/tests/outbound.test.js`: the JavaScript halves (`FetchHttpClient`,
  `TestHttpClient`, `SmtpEmailProvider`, `RecordingEmailProvider`, the job queues, job runs, `Hmac`).
- `crates/k_server/tests/perf_budget.rs`: the budgets below (ignored; run in release).

---

## 17. Performance budgets

| Area | Budget | Measured (release, 14-core Apple M-series) |
|---|---|---|
| `dist/app` cold start to first response | ≤ 150 ms | 35 ms (1 worker, no snapshot) |
| Isolate creation + runtime load | — | 3.2 ms each |
| RPC dispatch overhead, p50 | ≤ 50 µs | trivial handler 45 µs p50 for the whole loopback round trip; a generated rpc (decode, guards, container, handler, encode) 47 µs p50, 65 µs p99 |
| Trivial RPC throughput | ≥ 30,000 req/s on 8 cores | 124,000 req/s on 14 cores (budget scaled: 52,500); generated rpc 126,000 req/s |
| SSR of a 1,000-node page | ≤ 3 ms | 0.16 ms p50, 0.21 ms p99 for the whole round trip (a page with a load and 500 two-element rows) |

Run `cargo test --release -p k_server --test perf_budget -- --ignored --nocapture --test-threads 1`;
method and history in [performance.md §11](performance.md#11-stage-3-runtime-and-server).

---

## 18. Outgoing HTTP and email

### HTTP: `FetchHttpClient`

[32-http-client.md](../design/32-http-client.md). On the server `HttpClient` binds to `FetchHttpClient`
(`$s.fetchHttpClient(HttpError, HttpStatus, codes)`, `k:runtime/outbound.js`) unless the app binds
another implementation; a test build binds `TestHttpClient`.

- **JavaScript** builds the wire request (`wireRequest`): the URL with its `query`, the headers
  lower-cased, `secretHeaders` and `bearerToken` revealed only here (`Authorization: Bearer …`), the
  body by its `HttpBody` case with its `Content-Type` unless set (`.json` through the value's codec,
  `.form` URL-encoded, `.text` UTF-8, `.bytes` with its content type), and the timeout (default 30 s).
- **`op_k_fetch`** first applies `ServerOptions::fetch_origins` (`outbound::replace_origin`, tests only),
  then (`outbound::fetch`) sends it with hyper's client over rustls with the Mozilla root
  store (`webpki-roots`): `http` and `https` only, HTTP/1.1, one connection pool per worker thread,
  redirects not followed (a `3xx` is a response). The timeout covers connecting through the body's
  last byte (at most 10 minutes). A body over 32 MiB is refused. The outcome is a response (status,
  headers in order, body bytes), `offline` (DNS, a refused or reset connection, TLS, an oversized
  body; its message names neither headers nor the body), `timeout`, or `invalid` (a URL that isn't
  `http(s)`, a header HTTP can't carry: a panic, a bug of the caller).
- **`send`** resolves to a received `HttpResponse` whatever its status: `status` (an `HttpStatus` case,
  or `.other(code)` only for a code without a name), `isSuccess`, `headers` (`$s.header` reads them;
  repeated values join with `, ` except `set-cookie`), `bodyBytes`, `body` and `json()`, which throws
  `.decoding` for a body that isn't JSON or doesn't match the type. `offline` and `timeout` reject
  with those `HttpError` cases (an offline failure is logged at warning level without the query).
  `fetchJson` is the interface's default over `send`.
- **Cancellation**: when the request's signal aborts (the client went away), `op_k_fetch_abort`
  cancels the exchange and the awaiting code never resumes.
- **`TestHttpClient`** (`$s.testHttpClient`): `respond(path:with:)`, `serveJson`, `serveText`,
  `fail(path:error:)` and `requests`; a request to a path without a canned answer is a panic naming the
  method and URL, so a test never reaches the network.

### Email: `SmtpEmailProvider`

[31-email.md](../design/31-email.md). `EmailProvider` binds to `SmtpEmailProvider`
(`$s.smtpEmailProvider(EmailError, $s.info().config)`); a test build binds `RecordingEmailProvider`.

- **Configuration**: `SmtpConfig` (`SMTP_HOST`, `SMTP_PORT` 587, `SMTP_SECURITY` `startTls`/`tls`/`none`,
  `SMTP_USERNAME`, `SMTP_PASSWORD` (a secret, usually `_FILE`), `MAIL_FROM`) is in the manifest, and so
  required at startup, only when the reachable graph sends email through it
  ([codegen.md §15.2](codegen.md#152-configuration)).
- **JavaScript** (`wireEmail`) refuses a subject with a carriage return or line feed as
  `invalidMessage` before anything is sent, defaults the sender to `MAIL_FROM`, and passes the
  addresses as text (`EmailAddress` values are validated already).
- **`op_k_smtp_send`** (`outbound::send_mail`) builds the message with lettre (plain text, or
  `multipart/alternative` with the HTML part; `Reply-To`, `Cc`; `Bcc` only as envelope recipients) and
  sends it over a fresh connection with a 30-second timeout per step: required STARTTLS, implicit TLS
  or plain text, with credentials when a user name is set. Outcomes map to `EmailError`: a `5xx` reply
  is `rejected` with the server's code and text; a connection failure, a timeout, TLS or a `4xx` reply
  is `unavailable` (logged without the password); a message lettre can't write is `invalidMessage`.
- **`RecordingEmailProvider`**: `sent`, oldest first, and `simulateFailure(error:)`.

### Crypto natives

`Hmac.sha256`/`sha512` take a `Bytes` key or a `Secret<String>` (revealed only to the op) and return a
`Mac` (`{bytes, hex}`) through `op_k_hmac`; `Mac.matches` compares in constant time. `Bytes.parse(hex:)`
and `Bytes.parse(base64:)` are server natives. The shop's Stripe webhook verification runs on them.
