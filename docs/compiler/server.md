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

---

## 1. Status and scope

| Part | Module | Status |
|---|---|---|
| Engine: isolates, module map, ops, timeouts | `engine`, `ops` | **implemented** |
| HTTP front: hyper, workers, static assets, body limit, RPC preconditions, forgery check | `http`, `front` | **implemented** |
| Graceful shutdown, `SIGTERM` | `http` | **implemented** |
| Process-wide state: sessions, rate limits, `.server` state, broadcasts | `process` | **implemented** (in memory, v0) |
| Configuration from env, `.env` and `NAME_FILE`; startup validation (exit 78) | `config`, `k_cli` | **implemented** |
| `dist/app` archive, `serve`, `probe`, `config` | `app`, `k_cli` | **implemented** (v0 format) |
| Request handling: routing, SSR, RPC dispatch, guards, endpoint pages, sessions | `k:runtime/app` (JS) over the generated spec | **implemented** |
| Server streaming (Connect envelopes over chunked HTTP/1.1) | `k:runtime/app`, `ops`, `http` | **implemented**; back-pressure on slow readers is open |
| Database: open and migrate at startup, statements by number, request-bound transactions | `db`, `ops`, `k_db` | **implemented** ([db.md §6, §9](db.md#6-drivers-the-manifest-and-ops)); one connection per process |
| Jobs worker | ops + `k_db` | v0, specified here, not built yet |
| V8 snapshot | — | v0 target, not yet built (cold start is already within budget without it) |

**Deferred:** binary Protobuf (`application/proto` is `415` in v0), client and bidirectional streaming,
HTTP/2 at the server (serve it at the proxy, design 38), TLS termination, `fetch` to third parties through
deno's web extensions (v0 ships a K-specific fetch op), email over SMTP, OAuth helpers, flushing the SSR
shell before loads resolve.

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
- **Sessions are in memory in v0** (they last as long as the process and aren't shared between replicas).
  The contract is the `k_session` one of [§9](#9-sessions-auth-and-request-forgery); the table replaces
  the map with the stdlib's own migrations (planned; the app's database is wired in, §4).
- **The database is the process's, on its own runtime.** `db::ProcessDatabase` lives on the `k-db`
  thread's runtime (a Postgres connection is a task that must outlive any one request) and is shared by
  every worker through `ProcessState`; each op spawns its work there. v0 has one connection, leased to a
  transaction for its duration ([db.md §9](db.md#9-transactions)).
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
| `op_k_next_request` | `() => Promise<Request \| null>` | implemented: `null` once the server shuts down |
| `op_k_respond` | `(id, {status, headers, body}) => void` | implemented: body is a string or `Uint8Array`; a status outside `100..=999` (or not an integer) is a generic `500` |
| `op_k_respond_start` / `op_k_respond_chunk` / `op_k_respond_end` | `(id, {status, headers})`, `(id, bytes) => open`, `(id)` | implemented: a streamed body (chunked) |
| `op_k_server_info` | `() => {publicOrigin, config}` | implemented: config values typed, secrets as `{"$secret": v}` |
| `op_k_now` | `() => number` | implemented: epoch ms; `TestClock` replaces it in tests |
| `op_k_random_token` | `() => string` | implemented: 32 CSPRNG bytes, base64url (43 characters) |
| `op_k_utf8_valid` | `(bytes) => boolean` | implemented: behind the bootstrap's `TextDecoder` with `fatal: true`, which rpc bodies, `request.text()` and `request.json()` use, so a body that isn't UTF-8 is refused, never decoded lossily |
| `op_k_session_get` / `_create` / `_delete` / `_end_all` | the session store | implemented ([§9](#9-sessions-auth-and-request-forgery)) |
| `op_k_rate_limit` | `(key, limit, windowMs) => retryAfterSeconds` (`0` allowed) | implemented: sliding window per key |
| `op_k_shared_get` / `op_k_shared_set` | `(key) => json \| null`, `(key, json)` | implemented: `.server` service state |
| `op_k_broadcast_subscribe` / `_publish` / `_next` / `_close` | keyed broadcasts | implemented: a subscriber with 1,000 undelivered elements, or whose session ends, is closed |
| `op_k_sleep` | `(ms, signal) => Promise<void>` | v0: cancellable |
| `op_k_db_query` | `(statement, params, tx) => Promise<row[]>` (a row: values in select order; `tx` 0: none) | implemented ([db.md §6](db.md#6-drivers-the-manifest-and-ops)) |
| `op_k_db_execute` | `(statement, params, tx) => Promise<{count} \| {unique: field}>` | implemented |
| `op_k_db_begin` / `op_k_db_finish` | `() => Promise<tx>`, `(tx, commit) => Promise<void>`: transactions bound to the request | implemented |
| `op_k_db_savepoint` | `(tx, depth, op)`: 0 `SAVEPOINT`, 1 `RELEASE`, 2 `ROLLBACK TO` and `RELEASE` | implemented |
| `op_k_db_ping` | `(timeoutMs) => Promise<boolean>`, `false` without a database | implemented |
| `op_k_jobs_enqueue` | `(kind, payload, delay, txId?)` | v0 (`k_job`) |
| `op_k_password_hash` / `op_k_password_verify` | `(password) => Promise<string>`, `(password, hash) => Promise<boolean>`: Argon2id (m = 19 MiB, t = 2, p = 1) PHC strings on the blocking pool; a malformed hash matches nothing (`tests/e2e/passwords`) | implemented |
| `op_k_hash`, `op_k_hmac` | crypto (`Hash`, `Hmac`) | v0, not built yet |
| `op_k_fetch` | `(request, signal) => response` | v0, for `HttpClient` on the server |

The request handed to JavaScript is `{ id, method, path, headers: [[name, value]…] (lower case, in order),
body: Uint8Array, remote }` (`remote` the peer's IP address); the response is `{ status, headers: [[name, value]…], body: string | Uint8Array }`.

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
  | declared error case | `failed_precondition`, detail `{"type": "k.TypedError", "debug": {"error": "shop.cart.CartError", "case": "outOfStock", "fields": {"available": 3}}}` | 400 |
  | invalid request | `invalid_argument`, detail `k.ValidationError` with field paths | 400 |
  | `Authenticated` refused | `unauthenticated` | 401 |
  | `Role` refused | `permission_denied` | 403 |
  | `RateLimit` refused | `resource_exhausted`, `Retry-After` | 429 |
  | body over 1 MB | `resource_exhausted` | 413 |
  | wrong content type / no `Connect-Protocol-Version` | — | 415 |
  | unknown rpc | `unimplemented` | 404 |
  | panic | `internal`, no details outside development | 500 |

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
  server variables are always read; `K_PUBLIC_ORIGIN` falls back to `http://localhost:{PORT}`.
- Values reach JavaScript once per isolate through `op_k_server_info`, already typed (a `Duration` in
  ms, an enum by case name, a list as an array). The generated `C$load()` wraps a secret in `Secret`
  (`$s.configSecret`), whose text and JSON forms are `Secret(•••)`; only `reveal()` and `matches(_:)` read
  it. *Planned*: opaque handles that only ops can open.

---

## 9. Sessions, auth and request forgery

[26-auth-and-sessions.md](../design/26-auth-and-sessions.md):

- **Storage**: `k_session` in the app's database (managed by the stdlib migrations, outside
  `schema.lock`): id (32 random bytes, base64url), user id, roles, created, last seen, CSRF-free OAuth
  state. **v0** keeps the same rows in `process::ProcessState`, in memory. The cookie holds only the id:
  `HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000`, plus `Secure` and the `__Host-k_session` name when
  `K_PUBLIC_ORIGIN` is `https`, `k_session` otherwise; signing out sends it empty with `Max-Age=0`. A
  cookie the server never issued (or one that expired or was revoked) is a guest.
- **Lifetime**: idle timeout (default 7 days, sliding, written at most once per minute per session) and
  absolute lifetime (30 days from `signIn`), or `K_SESSION_IDLE` and `K_SESSION_MAX_AGE` from the
  validated configuration (`ProcessState::new` reads them). `signIn` always issues a new id (no fixation); `signOut` and
  `endAll` delete rows, so revocation is immediate.
- **OAuth state (open)**: v0's `issueState()` returns a fresh random token that is not stored, and
  `consumeState(_:)` always answers `false`, so an OAuth callback that checks it (as design 26 requires)
  refuses every sign-in. Binding the state to the session row, including a guest session created for it,
  is not built yet; no example uses it.
- **The mirror**: SSR serializes `Session.user` into `k-data`; an rpc that changes the session returns the
  new mirror in `x-k-session` so the browser's `Session` follows.
- **Guards** run on the server for every request: `Authenticated`, `Role(…)` (`Authenticated` first, so a
  guest is `401`/redirected rather than `403`), `RateLimit(…)` (a sliding window per guard, counted per
  live session when signed in and per client address otherwise; a refusal carries `Retry-After` in
  whole seconds). *Planned*: guards on in-process calls during SSR (v0 checks the page's guards only).
- **Responses**: a response whose handling read `Session` (or changed it) is `Cache-Control: private,
  no-store`; one that signed in or out carries `Set-Cookie`; an rpc that changed the session also
  `x-k-session` with the new mirror (`{"user": {"id": …, "roles": […]}}` or `{"user": null}`).
- **Request forgery**: the front's checks of [§5](#5-request-lifecycle) are always on: rpc content types
  and the Connect header, and the `Origin`/`Sec-Fetch-Site` check for any state-changing request carrying
  the session cookie. CORS is never granted for rpc routes.

---

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

[30-background-jobs.md](../design/30-background-jobs.md), v0 (specified, not built in this round):

- `k_job` table: id, name (`@JobName` or the locked derived name), payload JSON, run after, attempts,
  status (`queued`, `running`, `dead`), last error, timestamps.
- `enqueue` inside a transaction writes the row in that transaction; commit makes it visible.
- **The worker** runs inside `dist/app serve`: `K_JOB_CONCURRENCY` (default 4) claims at a time, with
  `SELECT … FOR UPDATE SKIP LOCKED` on Postgres and a single-writer claim on SQLite; each run in a fresh
  container in a worker isolate; `JobError.retryable` and panics back off (`@Retry`, default 5 attempts
  from 10 s, doubling, jittered), `permanent` and exhausted attempts mark the job `dead`.
- **Schedules** (`@Schedule`) are enqueued once per due time across replicas (a unique row per job and
  due time).
- On `SIGTERM` the worker stops claiming and waits for running jobs within the grace period; unfinished
  jobs return to the queue.

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
  there is one, behaves as the app: `serve` (default), `probe <url>`, `config`. A slim runtime binary
  without the compiler is *deferred*.
- Commands ([38-deployment.md](../design/38-deployment.md#commands)): `serve` (configuration → database →
  migrations → listener → job worker), `migrate`, `config`, `probe <url>` (exit 0 on a 2xx within 3 s, 1
  otherwise; HTTP only), `jobs list --dead`, `jobs retry <id>`. v0 implements `serve` (configuration →
  database and pending migrations, exit 1 on failure → listener; without jobs), `probe` and `config` (each
  variable with its type, whether it is required, its default and whether it is a secret; never a value).
  `migrate` is planned. The payload carries the build's database manifest (`AppPayload::database`).
- **`k build`** writes `dist/app`, `dist/public/assets/*` and `dist/manifest.json` from a release build
  for production; **`k run`** serves a development build on `--port` (default 8080) until `SIGTERM` or
  Ctrl-C, with the development environment of [§8](#8-configuration-and-secrets). **`k dev`** serves the
  same development build and watches the project (every file below its root except hidden directories,
  `target`, `dist` and `node_modules`, plus `.env`; polled every 250 ms by size and modification time).
  On a change it rebuilds and restarts the server on the same port; there is no HMR in v0, so a restart
  drops in-memory state such as sessions. The first build must succeed (it fails like `k run`); after
  that a build with errors is reported and the last good build keeps serving, and a restart that fails
  (a configuration problem) is reported and retried on the next change. The job worker joins `k dev`
  with the jobs runtime (§11).

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
  has neither the URL nor the password) → worker isolates → listener → job worker (planned).
  `Server::start` returns only after every isolate has started. Each applied migration is logged as
  `applied migration <name>`.
- **Shutdown** (`SIGTERM`/`SIGINT` in `run_until_signal`, or `Server::shutdown`): stop accepting; let open
  connections finish their in-flight requests through hyper's graceful shutdown, for at most
  `K_SHUTDOWN_GRACE` (default 25 s); then close the worker queues, so each `serve()` returns once the
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
