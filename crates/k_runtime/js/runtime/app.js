/**
 * @module k:runtime/app
 *
 * The server half of a generated app (docs/compiler/server.md §5–§10): what
 * a request goes through after the Rust front's checks. The generated
 * server entry describes the app as a plain object, the **spec**, and
 * calls `serve(createApp(spec))`; everything here is driven by it:
 *
 * ```text
 * request ─▶ rpc path?       ─▶ decode ▸ validate ▸ guards ▸ handler ▸ encode   (§6)
 *          ├▶ endpoint match? ─▶ guards ▸ IncomingRequest ▸ handler ▸ response (§10)
 *          ├▶ GET/HEAD:
 *          │    redirect?     ─▶ 308 to the renamed page's URL                 (codegen §14)
 *          │    page match?   ─▶ guards ▸ loads ▸ $ssr renderers ▸ document    (§7)
 *          └▶ 404 / 405
 * ```
 *
 * Every request gets a {@link RequestState} (cookies, the lazily loaded
 * session, what the response must add) and a fresh request container from
 * `spec.container(state)`, so no state crosses requests. Process-wide state
 * (sessions, rate limits, `.server` service state, broadcasts) lives in
 * Rust behind ops, shared by every worker isolate.
 *
 * Errors follow docs/design/09-errors.md: a K typed error is a value (any
 * thrown non-`Error`), a panic is a `KPanic` or any other `Error`. A panic
 * answers 500 with no details; a typed error of an rpc is Connect
 * `failed_precondition` with a `k.TypedError` detail.
 */

import { KPanic, isPanic, panic } from "./values.js";
import { DecodeFailure, Instant, NO, Secret, parseEmail, parseQuery, parseUuid, route, routeParam, splitTarget, stringify, toJSON } from "./std.js";
import { Out, escapeAttribute, escapeText, serializeData } from "./ssr.js";
import { runJobBody } from "./outbound.js";

const ops = globalThis.Deno?.core?.ops;

/** Methods that can't change state, which skip the forgery check. */
const SAFE = new Set(["GET", "HEAD"]);

const TEXT = "text/plain; charset=utf-8";
const HTML = "text/html; charset=utf-8";
const JSON_TYPE = "application/json";
const PRIVATE = "private, no-store";

// ----------------------------------------------------------------- startup

let serverInfo = null;

/**
 * What the Rust side knows about this process: the public origin and the
 * validated configuration. Read once per isolate.
 * @returns {{publicOrigin: string, config: Record<string, unknown>}}
 */
export function info() {
  if (serverInfo === null) {
    const read = ops?.op_k_server_info;
    serverInfo = typeof read === "function" ? read() : { publicOrigin: "http://localhost", config: {} };
  }
  return serverInfo;
}

/**
 * A configuration value (`@Env("NAME")`), as validated at startup: text,
 * numbers (a `Duration` in milliseconds), booleans, enum case names,
 * arrays, `null` for a missing optional. Panics for a variable the startup
 * configuration doesn't have, which only a build/startup mismatch causes.
 * @param {string} name
 * @returns {unknown}
 */
export function config(name) {
  const c = info().config;
  if (!Object.hasOwn(c, name)) throw new KPanic(`the configuration has no ${name}; was the server started with this build's manifest?`);
  return c[name];
}

/**
 * A secret configuration value, wrapped in `Secret` so it never prints.
 * @param {string} name
 * @returns {Secret|null}
 */
export function configSecret(name) {
  const v = config(name);
  if (v === null) return null;
  return new Secret(v && typeof v === "object" && "$secret" in v ? v.$secret : v);
}

/**
 * An enum configuration value: the case named by the variable.
 * @param {string} name
 * @param {Record<string, unknown>} cases case name → value
 * @returns {unknown}
 */
export function configEnum(name, cases) {
  const v = config(name);
  if (v === null) return null;
  if (Array.isArray(v)) return v.map((x) => cases[x]);
  return cases[v];
}

// ------------------------------------------------------------------- URLs

// Splitting targets, the query, typed parameter parsers and `route` are
// shared with the client router (`std.js`), so both sides match a URL to
// the same parameters.
export { parseQuery, route, splitTarget };

/** The milliseconds of an ISO timestamp, which `Date` values omit when zero. */
const MILLIS = new RegExp("\\.[0-9]{3}Z$");

/**
 * Parsers of route parameters by type (12-routing.md, "Parameters"): the
 * shared {@link routeParam} parsers, plus `page`, which matches this app's
 * view pages on the server.
 */
export const param = {
  ...routeParam,
  /**
   * `Page`: a path of one of this app's view pages (never another site:
   * the text must start with one `/`).
   * @param {() => object} app the spec, read lazily
   */
  page: (app) => (t) => {
    if (!t.startsWith("/") || t.startsWith("//") || t.includes("\\")) return NO;
    const m = matchPage(app(), splitTarget(t));
    return m === null ? NO : m.route.value(m.params);
  },
};

/**
 * The first view page whose route matches `url`.
 * @param {object} spec
 * @param {{pathname: string, search: string}} url
 * @returns {{route: object, params: Record<string, unknown>}|null}
 */
function matchPage(spec, url) {
  for (const r of spec.pages) {
    const params = r.match(url);
    if (params !== null) return { route: r, params };
  }
  return null;
}

/**
 * The props of a page from its parameters: one getter per parameter, as
 * the generated renderers read them.
 * @param {Record<string, unknown>} params
 * @returns {Record<string, () => unknown>}
 */
function props(params) {
  const p = {};
  for (const k of Object.keys(params)) p[k] = () => params[k];
  return p;
}

// --------------------------------------------------------------- requests

/**
 * Everything one request carries through its handling: the parsed request,
 * the session (loaded on first use) and what the response must add
 * (a session cookie, the session mirror, `Cache-Control`).
 */
export class RequestState {
  /**
   * @param {object} spec
   * @param {object} request as `op_k_next_request` gives it
   */
  constructor(spec, request) {
    this.spec = spec;
    this.request = request;
    this.method = request.method;
    this.url = splitTarget(request.path);
    this.headers = request.headers;
    const origin = info().publicOrigin;
    this.origin = origin;
    this.cookieName = origin.startsWith("https:") ? "__Host-k_session" : "k_session";
    this.cookies = parseCookies(request.headers);
    this.sessionId = this.cookies.get(this.cookieName) ?? null;
    /** @type {object|null|undefined} the session user, `undefined` until loaded */
    this.loaded = undefined;
    /** `true` once K code read `Session` (the response is then private). */
    this.sessionRead = false;
    /** `true` once `signIn`, `signOut` or `endAll` changed this browser's session. */
    this.sessionChanged = false;
    /** @type {string|null} a `Set-Cookie` value to send */
    this.setCookie = null;
    this.clientAddress = request.remote ?? "";
    this.container = null;
    const controller = typeof AbortController === "function" ? new AbortController() : null;
    /** The request's cancellation signal (`$c`). */
    this.signal = controller ? controller.signal : null;
  }

  /** The request container, created on first use. */
  get c() {
    this.container ??= this.spec.container(this);
    return this.container;
  }

  /**
   * Loads the session of the request's cookie (`op_k_session_get`, from
   * the database when the server has one). Called once before the request
   * is handled; a cookie the server never issued, or one that expired or
   * was revoked, is no session. A guest session (one created to bind OAuth
   * state) keeps its id but has no user.
   */
  async load() {
    if (this.loaded !== undefined) return;
    if (!this.sessionId || !ops?.op_k_session_get) {
      this.sessionId = null;
      this.loaded = null;
      return;
    }
    const row = await ops.op_k_session_get(this.sessionId);
    if (row === null) {
      this.sessionId = null;
      this.loaded = null;
    } else {
      this.loaded = row.user === null ? null : { id: this.decodeUser(row.user), roles: new Set(row.roles.map((r) => this.decodeRole(r))) };
    }
  }

  /**
   * The signed-in user (`SessionUser`), or `null` for a guest, after
   * {@link RequestState#load}.
   * @returns {{id: unknown, roles: Set<unknown>}|null}
   */
  user() {
    if (this.loaded === undefined) throw new KPanic(this.job ? "a background job has no session" : "the session was read before it was loaded");
    return this.loaded;
  }

  /** The session's id when it is a signed-in session, after loading. */
  liveSessionId() {
    return this.user() === null ? null : this.sessionId;
  }

  decodeUser(json) {
    const d = this.spec.session?.decodeUser;
    return d ? d(json) : json;
  }

  decodeRole(name) {
    const cases = this.spec.session?.roles ?? {};
    return Object.hasOwn(cases, name) ? cases[name] : name;
  }

  encodeRole(role) {
    const cases = this.spec.session?.roles ?? {};
    for (const k of Object.keys(cases)) if (cases[k] === role) return k;
    return String(role);
  }

  /** The `Set-Cookie` value for session `id` (an empty id clears it). */
  cookie(id) {
    const secure = this.cookieName.startsWith("__Host-") ? "; Secure" : "";
    const life = id === "" ? "; Max-Age=0" : "; Max-Age=2592000";
    return `${this.cookieName}=${id}; Path=/; HttpOnly; SameSite=Lax${secure}${life}`;
  }

  /** The `Session` mirror the browser receives: `{user}`. */
  mirror() {
    const u = this.user();
    return { user: u === null ? null : { id: toJSON(u.id), roles: [...u.roles].map((r) => this.encodeRole(r)) } };
  }
}

/**
 * The cookies of every `Cookie` header, by name (the first of a name wins).
 * @param {Array<[string, string]>} headers
 * @returns {Map<string, string>}
 */
export function parseCookies(headers) {
  const out = new Map();
  for (const [n, v] of headers) {
    if (n !== "cookie") continue;
    for (const part of v.split(";")) {
      const eq = part.indexOf("=");
      if (eq < 0) continue;
      const name = part.slice(0, eq).trim();
      if (!out.has(name)) out.set(name, part.slice(eq + 1).trim());
    }
  }
  return out;
}

/**
 * The `Session` service of a request: a read-only view whose `user` marks
 * the response private (server.md §5).
 * @param {RequestState} st
 */
export function session(st) {
  return {
    get user() {
      st.sessionRead = true;
      return st.user();
    },
  };
}

/**
 * The server's `k.time.Clock` (one per request container): `now` is the
 * system time in milliseconds (`op_k_now`). The server runtime has no
 * timers yet, so `sleep`, `every` and `after` panic with their name rather
 * than silently never firing.
 * @returns {{now: Instant, sleep: Function, every: Function, after: Function}}
 */
export function clock() {
  return {
    get now() {
      return new Instant(ops?.op_k_now ? ops.op_k_now() : Date.now());
    },
    sleep: missingNative("k.time.Clock.sleep"),
    every: missingNative("k.time.Clock.every"),
    after: missingNative("k.time.Clock.after"),
  };
}

/**
 * The `SessionManager` of a request (26-auth-and-sessions.md): `signIn`
 * always issues a new id (no fixation) and replaces the current session;
 * `signOut` and `endAll` delete rows, so revocation is immediate.
 * @param {RequestState} st
 */
export function sessionManager(st) {
  const signedOut = () => {
    st.sessionId = null;
    st.loaded = null;
    st.setCookie = st.cookie("");
    st.sessionChanged = true;
  };
  return {
    async signIn(_c, userId, roles) {
      // The current session (a guest's too) ends: a planted id is never
      // adopted.
      const old = st.sessionId;
      if (old) await ops.op_k_session_delete(old);
      const list = roles ? [...roles] : [];
      const id = await ops.op_k_session_create(toJSON(userId), list.map((r) => st.encodeRole(r)));
      st.sessionId = id;
      st.loaded = { id: userId, roles: new Set(list) };
      st.setCookie = st.cookie(id);
      st.sessionChanged = true;
    },
    async signOut(_c) {
      if (st.user() === null) return;
      await ops.op_k_session_delete(st.sessionId);
      signedOut();
    },
    async endAll(_c, userId) {
      const mine = st.user();
      await ops.op_k_session_end_all(toJSON(userId));
      if (mine !== null && stringify(toJSON(mine.id)) === stringify(toJSON(userId))) signedOut();
    },
    /**
     * A fresh OAuth state bound to this browser's session; a visitor
     * without one gets a guest session (and its cookie) to bind it to.
     */
    async issueState(_c) {
      st.user();
      if (!st.sessionId) {
        st.sessionId = await ops.op_k_session_create(null, []);
        st.setCookie = st.cookie(st.sessionId);
      }
      const state = await ops.op_k_session_issue_state(st.sessionId);
      if (state === null) throw new KPanic("sessions: the session to bind the OAuth state to is gone");
      return state;
    },
    /** `true` once for the state last issued to this browser's session. */
    async consumeState(_c, state) {
      st.user();
      if (!st.sessionId) return false;
      return ops.op_k_session_consume_state(st.sessionId, String(state));
    },
  };
}

// ----------------------------------------------------------------- guards

/**
 * Evaluates guards in order (server.md §9): `["auth"]` (`Authenticated`),
 * `["role", value]` (`Role`) and `["rate", key, count, windowMs]`
 * (`RateLimit`, counted per session when signed in and per client address
 * otherwise). Returns `null` when all pass, or the first refusal.
 * @param {RequestState} st
 * @param {Array<Array<unknown>>} guards
 * @returns {{kind: "unauthenticated"|"forbidden"|"rate", retryAfter?: number}|null}
 */
export async function checkGuards(st, guards) {
  for (const g of guards ?? []) {
    switch (g[0]) {
      case "auth":
        if (st.user() === null) return { kind: "unauthenticated" };
        break;
      case "role": {
        const u = st.user();
        if (u === null) return { kind: "unauthenticated" };
        if (!u.roles.has(g[1])) return { kind: "forbidden" };
        break;
      }
      case "rate": {
        const live = st.liveSessionId();
        const who = live ? `s:${live}` : `a:${st.clientAddress}`;
        const wait = await ops.op_k_rate_limit(`${g[1]}|${who}`, g[2], g[3]);
        if (wait > 0) return { kind: "rate", retryAfter: wait };
        break;
      }
      default:
        throw new KPanic(`unknown guard ${String(g[0])}`);
    }
  }
  return null;
}

// --------------------------------------------------------------- responses

function respond(status, type, body, headers = []) {
  return { status, headers: type ? [["content-type", type], ...headers] : headers, body };
}

/**
 * A Connect error response (28-rpc-wire-format.md, "Errors").
 * @param {number} status
 * @param {string} code
 * @param {string} message
 * @param {Array<object>} [details]
 * @param {Array<[string, string]>} [headers]
 */
function connectError(status, code, message, details, headers = []) {
  const body = { code, message };
  if (details && details.length) body.details = details;
  return respond(status, JSON_TYPE, stringify(body), headers);
}

/** Adds what the request state requires to a response. */
function finish(st, response, rpc) {
  const headers = response.headers;
  if (st.setCookie !== null) headers.push(["set-cookie", st.setCookie]);
  if (rpc && st.sessionChanged) headers.push(["x-k-session", JSON.stringify(st.mirror())]);
  const isPrivate = rpc || st.sessionRead || st.sessionChanged;
  if (isPrivate && !headers.some(([n]) => n === "cache-control")) headers.push(["cache-control", PRIVATE]);
  if (st.method === "HEAD") response.body = "";
  return response;
}

/** `true` for a K typed error (a thrown value), `false` for a panic. */
function isTypedError(e) {
  return !(isPanic(e) || e instanceof Error || e === null || e === undefined);
}

/** Logs a panic with what the request was, never the error to the client. */
function logPanic(st, where, e) {
  const text = e && e.stack ? e.stack : String(e);
  ops?.op_k_log?.("error", `${st.method} ${st.url.pathname}: panic in ${where}: ${text}`);
}

// --------------------------------------------------------------------- rpc

/**
 * Decodes UTF-8 strictly: bytes that aren't UTF-8 throw (a request body is
 * never decoded lossily, so a handler sees exactly what was sent).
 * @param {Uint8Array} bytes
 * @returns {string}
 */
function utf8(bytes) {
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}

/**
 * Handles an rpc (server.md §6): decode → validate → guards → handler →
 * encode, each failure with its Connect code.
 * @param {RequestState} st
 * @param {object} rpc the spec's entry for the path
 */
async function handleRpc(st, rpc) {
  const type = (st.headers.find(([n]) => n === "content-type")?.[1] ?? "").split(";")[0].trim().toLowerCase();
  const enveloped = type === "application/connect+json";
  if (enveloped && !rpc.stream) return respond(415, TEXT, "a unary rpc accepts application/json");
  let message;
  try {
    let bytes = st.request.body;
    if (enveloped) bytes = unenvelope(bytes);
    message = JSON.parse(utf8(bytes));
  } catch {
    return connectError(400, "invalid_argument", "the request body is not JSON");
  }
  if (message === null || typeof message !== "object" || Array.isArray(message)) {
    return connectError(400, "invalid_argument", "the request body is not a message");
  }
  let args;
  try {
    args = rpc.decode(message);
  } catch (e) {
    if (e instanceof DecodeFailure) return connectError(400, "invalid_argument", `${e.path}: ${e.message}`);
    throw e;
  }
  if (rpc.validate) {
    const failures = await rpc.validate(args, st.c, st.signal);
    if (failures.length) {
      return connectError(400, "invalid_argument", "the request is invalid", [
        { type: "k.ValidationError", debug: { failures } },
      ]);
    }
  }
  const refused = await checkGuards(st, rpc.guards);
  if (refused !== null) return rpcRefusal(refused);
  let result;
  try {
    result = await rpc.call(st.c, st.signal, args);
  } catch (e) {
    if (isTypedError(e) && rpc.error) {
      const detail = { type: "k.TypedError", debug: { error: rpc.error.name, case: rpc.error.text(e), fields: rpc.error.fields(e) } };
      return connectError(400, "failed_precondition", `${rpc.error.name}.${rpc.error.text(e)}`, [detail]);
    }
    logPanic(st, "an rpc", e);
    return connectError(500, "internal", "internal error");
  }
  if (rpc.stream) return { stream: result, encode: rpc.encode };
  return respond(200, JSON_TYPE, stringify(rpc.encode(result)));
}

/**
 * An in-process call of an rpc during server rendering (or from any server
 * code holding the protocol): its validation, then its guards, as over HTTP
 * (28-rpc-wire-format.md, "Failures outside the handler"), then the
 * implementation in the same container. A failure is the rpc's own error
 * `rpcFailure(.invalid(ValidationError))` or `rpcFailure(.rejected(…))`,
 * tagged like the browser's (rpc.js `rpcError`).
 * @param {object} c the request container
 * @param {object} rpc the spec's entry
 * @param {AbortSignal|null} $c
 * @param {unknown[]} args the decoded parameters
 */
export async function callLocal(c, rpc, $c, args) {
  const st = c.$r;
  const fail = (tag, payload) => {
    throw rpc.failure ? rpc.failure({ $: tag, _0: payload }) : new KPanic("an rpc without an error enum failed outside its handler");
  };
  if (rpc.validate) {
    const failures = await rpc.validate(args, c, $c);
    if (failures.length) fail(1, { failures });
  }
  const refused = await checkGuards(st, rpc.guards);
  if (refused !== null) fail(0, { $: ["unauthenticated", "forbidden", "rate"].indexOf(refused.kind), retryAfter: (refused.retryAfter ?? 0) * 1000 });
  return rpc.call(c, $c, args);
}

/** The Connect error for a refused guard. */
function rpcRefusal(refused) {
  switch (refused.kind) {
    case "unauthenticated":
      return connectError(401, "unauthenticated", "sign in to call this rpc");
    case "forbidden":
      return connectError(403, "permission_denied", "the signed-in user may not call this rpc");
    default:
      return connectError(429, "resource_exhausted", "too many calls; try again later", undefined, [["retry-after", String(refused.retryAfter)]]);
  }
}

/**
 * The message of a single Connect envelope (`[flags][length u32 BE][data]`);
 * throws if the bytes are not exactly one uncompressed message envelope.
 * @param {Uint8Array} bytes
 * @returns {Uint8Array}
 */
function unenvelope(bytes) {
  if (bytes.length < 5 || bytes[0] !== 0) throw new Error("not one message envelope");
  const n = ((bytes[1] << 24) | (bytes[2] << 16) | (bytes[3] << 8) | bytes[4]) >>> 0;
  if (bytes.length !== 5 + n) throw new Error("the envelope length doesn't match");
  return bytes.subarray(5);
}

/** One Connect streaming envelope: flags, big-endian length, JSON. */
function envelope(flags, json) {
  const bytes = new TextEncoder().encode(json);
  const out = new Uint8Array(5 + bytes.length);
  out[0] = flags;
  const n = bytes.length;
  out[1] = (n >>> 24) & 255;
  out[2] = (n >>> 16) & 255;
  out[3] = (n >>> 8) & 255;
  out[4] = n & 255;
  out.set(bytes, 5);
  return out;
}

/**
 * Streams a server-streaming rpc's elements as Connect envelopes until the
 * stream ends, the client goes away or the stream is closed (its session
 * ended, or it fell too far behind): then the end-of-stream envelope.
 * @param {RequestState} st
 * @param {{stream: object, encode: Function}} s
 */
async function pumpStream(st, s) {
  const id = st.request.id;
  ops.op_k_respond_start(id, {
    status: 200,
    headers: [["content-type", "application/connect+json"], ["cache-control", PRIVATE], ...(st.setCookie ? [["set-cookie", st.setCookie]] : [])],
  });
  // A stream opened under a session ends when the session does (signOut,
  // endAll, expiry), on whichever worker that happens.
  const sid = st.liveSessionId();
  const watch = sid ? ops.op_k_broadcast_subscribe(`k.session|${sid}`, sid) : null;
  const ENDED = {};
  let ended = new Promise(() => {});
  if (watch !== null) {
    const p = ops.op_k_broadcast_next(watch);
    unref(p);
    ended = p.then(() => ENDED);
  }
  let end = "{}";
  try {
    for (;;) {
      const next = s.stream.next();
      const v = await Promise.race([next, ended]);
      if (v === ENDED || v === null || v === undefined) break;
      if (!ops.op_k_respond_chunk(id, envelope(0, stringify(s.encode(v))))) break;
    }
  } catch (e) {
    logPanic(st, "a stream", e);
    end = stringify({ error: { code: "internal", message: "internal error" } });
  }
  closeStream(s.stream);
  if (watch !== null) ops.op_k_broadcast_close(watch);
  ops.op_k_respond_chunk(id, envelope(2, end));
  ops.op_k_respond_end(id);
}

/** Closes a stream and the streams it is mapped from (their `source`). */
function closeStream(stream) {
  for (let s = stream, n = 0; s && n < 64; s = s.source, n++) {
    if (typeof s.close === "function") s.close();
  }
}

/**
 * Lets the isolate's event loop finish while `promise` (a pending op) is
 * outstanding, so an idle subscription never keeps a worker alive at
 * shutdown.
 * @param {Promise<unknown>} promise
 */
function unref(promise) {
  const u = globalThis.Deno?.core?.unrefOpPromise;
  if (typeof u === "function") u(promise);
}

// --------------------------------------------------------------- endpoints

/**
 * The `IncomingRequest` an endpoint handler receives (27-endpoint-pages.md,
 * "The incoming request").
 * @param {RequestState} st
 */
function incoming(st) {
  const headers = new Map();
  const all = new Map();
  for (const [n, v] of st.headers) {
    headers.set(n, headers.has(n) ? `${headers.get(n)}, ${v}` : v);
    if (!all.has(n)) all.set(n, []);
    all.get(n).push(v);
  }
  return {
    method: st.method,
    url: st.origin + st.request.path,
    query: parseQuery(st.url.search),
    headers: { map: headers, all },
    rawBody: st.request.body,
    clientAddress: st.clientAddress,
  };
}

/** `request.headers[name]`: the values joined with `, `, or `nil`. */
export function header(h, name) {
  return h.map.get(String(name).toLowerCase()) ?? null;
}

/** `request.headers.all(name)`: every value, in order. */
export function headerAll(h, name) {
  return h.all.get(String(name).toLowerCase())?.slice() ?? [];
}

/** `request.text()`: the body as UTF-8, or `nil` if it isn't. */
export function requestText(r) {
  try {
    return utf8(r.rawBody);
  } catch {
    return null;
  }
}

/**
 * `request.json()`: the body decoded by `codec`; throws the stdlib's
 * `DecodingError` cases (a typed error).
 * @param {object} r
 * @param {{decode: Function}} codec
 * @param {object} DecodingError the generated enum
 */
export function requestJson(r, codec, DecodingError) {
  let j;
  try {
    j = JSON.parse(utf8(r.rawBody));
  } catch (e) {
    throw DecodingError.malformed(String(e?.message ?? e));
  }
  try {
    return codec.decode(j, "");
  } catch (e) {
    if (e instanceof DecodeFailure) throw DecodingError.mismatch(e.path || "$", e.message);
    throw e;
  }
}

/** A `HttpResponse` value: status code, headers, body. */
export function response(status) {
  return { status, headers: [], body: null };
}

/**
 * `response.headers(map)`: adds headers. Setting `set-cookie` (in any
 * letter case) is a panic: sessions own cookies (27-endpoint-pages.md).
 * @param {object} r
 * @param {Map<string, string>} map
 */
export function responseHeaders(r, map) {
  const headers = r.headers.slice();
  for (const [n, v] of map) {
    const name = String(n).toLowerCase();
    if (name === "set-cookie") throw new KPanic("an endpoint may not set set-cookie; use SessionManager");
    headers.push([name, String(v)]);
  }
  return { ...r, headers };
}

/** `response.body(body)`: sets the body (`HttpBody` value). */
export function responseBody(r, body) {
  return { ...r, body };
}

/** `HttpResponse.redirect(to:)` / `(url:)`: 303 See Other. */
export function redirect(location) {
  return { status: 303, headers: [["location", String(location)]], body: null };
}

/**
 * Converts an endpoint's `HttpResponse` to the front's response: the
 * status code from the generated `HttpStatus` table, the body by its case
 * (`text`, `json`, `bytes`, `form`), which also sets `Content-Type`.
 * @param {object} spec
 * @param {object} r
 */
function sendHttpResponse(spec, r) {
  if (!r || typeof r !== "object") throw new KPanic("an endpoint handler returned no response");
  const headers = r.headers.slice();
  const status = statusCode(spec, r.status);
  let body = "";
  const b = r.body;
  const type = (t) => {
    if (!headers.some(([n]) => n === "content-type")) headers.unshift(["content-type", t]);
  };
  if (b !== null && b !== undefined) {
    switch (spec.httpBody[b.$]) {
      case "text":
        type(TEXT);
        body = String(b._0);
        break;
      case "json":
        type(JSON_TYPE);
        body = stringify(b._0 && b._0.codec ? b._0.codec.encode(b._0.value) : toJSON(b._0));
        break;
      case "bytes":
        type(b.contentType ?? "application/octet-stream");
        body = b._0 instanceof Uint8Array ? b._0 : new Uint8Array(b._0);
        break;
      case "form": {
        type("application/x-www-form-urlencoded");
        const pairs = [];
        for (const [k, v] of b._0) pairs.push(`${encodeURIComponent(k)}=${encodeURIComponent(v)}`);
        body = pairs.join("&");
        break;
      }
      default:
        throw new KPanic("an unknown response body");
    }
  }
  return { status, headers, body };
}

/** The numeric code of an `HttpStatus` value. */
function statusCode(spec, s) {
  if (typeof s === "number") return s;
  const code = spec.httpStatus[s.$];
  return code === null ? s._0 : code;
}

/**
 * Handles an endpoint page request (server.md §10). `HEAD` follows `GET`;
 * a method without a handler is 405 with `Allow`.
 * @param {RequestState} st
 * @param {object} ep
 * @param {Record<string, unknown>} params
 */
async function handleEndpoint(st, ep, params) {
  const method = st.method === "HEAD" ? "GET" : st.method;
  const handler = Object.hasOwn(ep.methods, method) ? ep.methods[method] : null;
  if (handler === null) {
    const allow = Object.keys(ep.methods).flatMap((m) => (m === "GET" ? ["GET", "HEAD"] : [m]));
    return respond(405, TEXT, "method not allowed", [["allow", allow.join(", ")]]);
  }
  const refused = await checkGuards(st, ep.guards);
  if (refused !== null) return plainRefusal(st, refused);
  if (ep.session) st.sessionRead = true;
  const r = await handler({ params, c: st.c }, st.signal, incoming(st));
  return sendHttpResponse(st.spec, r);
}

/** A refused guard on a page or endpoint, as plain text. */
function plainRefusal(st, refused) {
  switch (refused.kind) {
    case "unauthenticated":
      return respond(401, TEXT, "sign in to continue");
    case "forbidden":
      return respond(403, TEXT, "forbidden");
    default:
      return respond(429, TEXT, "too many requests", [["retry-after", String(refused.retryAfter)]]);
  }
}

// --------------------------------------------------------------- metadata

/**
 * The metadata a render collects (22-metadata.md): each layer (app,
 * layouts outermost first, page) sets entries; a later layer replaces an
 * earlier one's entry. Generated `metadata { }` blocks call these methods.
 */
export class Metadata {
  constructor() {
    /** @type {Record<string, unknown>} */
    this.entries = {};
  }
  title(text) {
    this.entries.title = text;
  }
  description(text) {
    this.entries.description = text;
  }
  canonical(href) {
    this.entries.canonical = href;
  }
  socialPreview(image, title) {
    this.entries.preview = { image, title };
  }
  icon(url) {
    this.entries.icon = url;
  }
  manifest(name, shortName) {
    this.entries.manifest = { name, shortName };
  }
  themeColor(color) {
    this.entries.themeColor = color;
  }
  locale(tag) {
    this.entries.locale = tag;
  }
  structuredData(data) {
    this.entries.structuredData = data;
  }
  status(code) {
    this.entries.status = code;
  }
}

/** Merges metadata layers; later layers win per entry. */
function mergeMeta(layers) {
  const out = {};
  for (const l of layers) Object.assign(out, l.entries);
  return out;
}

/**
 * The `<head>` contents of a document, every value escaped.
 * @param {Record<string, unknown>} m merged metadata
 * @param {object} spec
 */
function head(m, spec) {
  let h = '<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">';
  if (m.title !== undefined) h += `<title>${escapeText(String(m.title))}</title>`;
  if (m.description !== undefined) h += `<meta name="description" content="${escapeAttribute(String(m.description))}">`;
  if (m.canonical !== undefined) h += `<link rel="canonical" href="${escapeAttribute(info().publicOrigin + String(m.canonical))}">`;
  if (m.themeColor !== undefined) h += `<meta name="theme-color" content="${escapeAttribute(String(m.themeColor))}">`;
  if (m.icon !== undefined) h += `<link rel="icon" href="${escapeAttribute(String(m.icon))}">`;
  if (m.preview !== undefined) {
    h += `<meta property="og:title" content="${escapeAttribute(String(m.preview.title))}">`;
    h += `<meta property="og:image" content="${escapeAttribute(String(m.preview.image))}">`;
  }
  for (const href of spec.assets.styles) h += `<link rel="stylesheet" href="${escapeAttribute(href)}">`;
  return h;
}

/**
 * The HTML document around a rendered body (server.md §7).
 * @param {object} spec
 * @param {RequestState} st
 * @param {Record<string, unknown>} meta merged metadata
 * @param {string} body the rendered layouts and page
 * @param {object|null} data the `k-data` payload, `null` for none
 */
function documentHtml(spec, st, meta, body, data) {
  const lang = escapeAttribute(String(meta.locale ?? "en"));
  let html = `<!doctype html><html lang="${lang}"><head>${head(meta, spec)}</head><body><div id="k-app">${body}</div>`;
  if (data !== null) html += `<script type="application/json" id="k-data">${serializeData(data)}</script>`;
  for (const src of spec.assets.scripts) html += `<script type="module" src="${escapeAttribute(src)}"></script>`;
  return html + "</body></html>";
}

/** The render instant as RFC 3339 UTC without fractions. */
function nowText() {
  const ms = ops?.op_k_now ? ops.op_k_now() : Date.now();
  return new Date(ms).toISOString().replace(MILLIS, "Z");
}

/**
 * Renders a view page (server.md §7): the page first (awaiting its loads),
 * then each layout around it, innermost first; metadata merged app →
 * layouts → page (or the `failed` block's). A failed load with
 * `status(_:)` sets the HTTP status.
 * @param {RequestState} st
 * @param {object} r the route
 * @param {Record<string, unknown>} params
 */
async function renderPage(st, r, params) {
  const spec = st.spec;
  if (spec.rendering === "client") {
    const meta = new Metadata();
    spec.appMeta?.(meta);
    return respond(200, HTML, documentHtml(spec, st, mergeMeta([meta]), "", null));
  }
  const pageMeta = new Metadata();
  const ids = { n: 0 };
  const out = new Out(ids);
  const ctx = { c: st.c, key: "0", meta: pageMeta, status: 200, failed: null };
  await r.render(props(params), ctx, out);
  const data = { v: 1, now: nowText(), session: null, c: {} };
  Object.assign(data.c, out.data);
  let html = out.html();
  const layoutMetas = [];
  const layouts = r.layouts ?? [];
  for (let i = layouts.length - 1; i >= 0; i--) {
    const lm = new Metadata();
    const lo = new Out(ids);
    const inner = html;
    const lctx = { c: st.c, key: `L${i}`, meta: lm, status: 200, failed: null };
    await layouts[i]({ content: (o) => o.raw(inner) }, lctx, lo);
    Object.assign(data.c, lo.data);
    html = lo.html();
    layoutMetas.unshift(lm);
  }
  const appMeta = new Metadata();
  spec.appMeta?.(appMeta);
  const meta = mergeMeta([appMeta, ...layoutMetas, pageMeta]);
  data.session = st.mirror();
  const status = typeof meta.status === "number" ? meta.status : 200;
  return respond(status, HTML, documentHtml(spec, st, meta, html, data));
}

/** A small HTML page for a status without a page of its own. */
function statusPage(spec, status, text) {
  const meta = new Metadata();
  spec.appMeta?.(meta);
  const m = mergeMeta([meta]);
  m.title = text;
  return respond(status, HTML, documentHtml({ ...spec, assets: { styles: spec.assets.styles, scripts: [] } }, null, m, `<p>${escapeText(text)}</p>`, null));
}

/**
 * Handles a page request: guards (a refused `Authenticated` redirects to
 * the `@SignInPage` with `returnTo`), then the render.
 */
async function handlePage(st, r, params) {
  const refused = await checkGuards(st, r.guards);
  if (refused !== null) {
    if (refused.kind === "unauthenticated" && st.spec.signIn) {
      const location = st.spec.signIn(r.value(params));
      return respond(303, TEXT, "", [["location", location]]);
    }
    if (refused.kind === "rate") return { ...statusPage(st.spec, 429, "Too many requests"), headers: [["content-type", HTML], ["retry-after", String(refused.retryAfter)]] };
    return refused.kind === "forbidden" ? statusPage(st.spec, 403, "Forbidden") : statusPage(st.spec, 401, "Sign in to continue");
  }
  return renderPage(st, r, params);
}

// ---------------------------------------------------------------- the app

/**
 * Builds the request handler of a generated app.
 *
 * `spec` (generated, see docs/compiler/codegen.md §13–§15):
 * `pages` (route table sorted by specificity: `{name, match(url), value(params),
 * render, layouts, guards}`), `redirects` (`{match(url), to(params) → href}`),
 * `rpcs` (path → `{decode, validate, guards, call(c, $c, args), encode, error, stream}`),
 * `endpoints` (`{match(url), methods, guards, session}`), `container(state)`,
 * `appMeta(meta)`, `signIn(page) → href`, `session` (`{roles, decodeUser}`),
 * `httpStatus` and `httpBody` (tag tables), `assets` (`{styles, scripts}`),
 * `rendering` (`"server"` or `"client"`).
 * @param {object} spec
 * @returns {(request: object) => Promise<object>}
 */
export function createApp(spec) {
  return async (request) => {
    if (request.job) return runJob(spec, request);
    const st = new RequestState(spec, request);
    try {
      await st.load();
      return await dispatch(st);
    } catch (e) {
      logPanic(st, "a request", e);
      const rpc = Object.hasOwn(spec.rpcs, st.url.pathname);
      return finish(st, rpc ? connectError(500, "internal", "internal error") : statusPage(spec, 500, "Something went wrong"), rpc);
    }
  };
}

/**
 * Runs a background job the jobs worker handed over (server.md §11): a
 * fresh container with no session (`st.job`), the payload decoded by the
 * spec's entry, and the outcome as the response body for the worker,
 * `{"outcome": "done" | "retryable" | "permanent" | "panic", "reason"}`.
 * A panic is logged with the job's name.
 * @param {object} spec
 * @param {object} request with `job: {name, attempt}` and the payload JSON as its body
 */
async function runJob(spec, request) {
  const job = spec.jobs?.[request.job.name];
  let outcome;
  if (!job) {
    outcome = { outcome: "permanent", reason: `the build has no job named ${request.job.name}` };
  } else {
    const st = new RequestState(spec, { ...request, headers: [], path: "/" });
    st.job = true;
    let payload;
    try {
      payload = JSON.parse(utf8(request.body));
    } catch (e) {
      payload = undefined;
      outcome = { outcome: "permanent", reason: `the payload isn't JSON: ${String(e?.message ?? e)}` };
    }
    if (outcome === undefined) {
      outcome = await runJobBody(job, st.c, st.signal, payload);
      if (outcome.outcome === "panic") logPanic(st, `job ${request.job.name} (attempt ${request.job.attempt})`, new KPanic(outcome.reason));
    }
  }
  return { status: 200, headers: [["content-type", JSON_TYPE]], body: JSON.stringify(outcome) };
}

async function dispatch(st) {
  const spec = st.spec;
  const path = st.url.pathname;
  if (Object.hasOwn(spec.rpcs, path)) {
    const r = await handleRpc(st, spec.rpcs[path]);
    if (r.stream) {
      pumpStream(st, r);
      return STREAMED;
    }
    return finish(st, r, true);
  }
  for (const ep of spec.endpoints) {
    const params = ep.match(st.url);
    if (params !== null) return finish(st, await handleEndpoint(st, ep, params), false);
  }
  const page = matchPage(spec, st.url);
  if (!SAFE.has(st.method)) {
    if (page !== null) return finish(st, respond(405, TEXT, "method not allowed", [["allow", "GET, HEAD"]]), false);
    return finish(st, statusPage(spec, 404, "Not found"), false);
  }
  for (const rd of spec.redirects) {
    const params = rd.match(st.url);
    if (params !== null) return finish(st, respond(308, TEXT, "", [["location", rd.to(params)]]), false);
  }
  if (page !== null) return finish(st, await handlePage(st, page.route, page.params), false);
  return finish(st, statusPage(spec, 404, "Not found"), false);
}

/**
 * Returned by the handler for a streamed rpc, whose response is sent with
 * the streaming ops instead (see `serve` in k:runtime/server).
 */
export const STREAMED = Object.freeze({ streamed: true });

// ------------------------------------------------------- server services

/**
 * Backs the `@State` fields of a `@Scope(.server)` service with the
 * process-wide store, so every worker isolate sees the same values
 * (10-services.md, "Scopes"). Values cross as JSON (plain data: numbers,
 * strings, booleans, lists, structs, payload-free enums by tag).
 * Read-modify-write sequences are not atomic across workers.
 * @param {object} instance
 * @param {string} service a process-unique name
 * @param {string[]} fields
 * @returns {object} the instance
 */
export function shareState(instance, service, fields) {
  for (const f of fields) {
    const key = `${service}.${f}`;
    const initial = instance[f];
    if (ops?.op_k_shared_get?.(key) === null) ops.op_k_shared_set(key, JSON.stringify(initial ?? null));
    Object.defineProperty(instance, f, {
      get: () => {
        const j = ops.op_k_shared_get(key);
        return j === null ? initial : JSON.parse(j);
      },
      set: (v) => ops.op_k_shared_set(key, JSON.stringify(v ?? null)),
      enumerable: true,
    });
  }
  return instance;
}

/**
 * A process-wide keyed broadcast (`Broadcast<K, V>`) backed by Rust, so a
 * publish on any worker reaches subscribers on every worker
 * (29-server-streaming.md). Elements cross as JSON through `encode` and
 * `decode`.
 */
export class ServerBroadcast {
  /**
   * @param {string} name a process-unique name
   */
  constructor(name) {
    this.name = name;
  }
  /** Publishes `value` to every subscriber of `key`. */
  publish(key, value) {
    ops.op_k_broadcast_publish(`${this.name}|${stringify(toJSON(key))}`, JSON.stringify(value));
  }
  /**
   * Subscribes to `key`: a stream of later elements, closed when `session`
   * (a session id) ends.
   */
  subscribe(key, session = null) {
    const id = ops.op_k_broadcast_subscribe(`${this.name}|${stringify(toJSON(key))}`, session);
    let done = false;
    return {
      async next() {
        if (done) return null;
        const p = ops.op_k_broadcast_next(id);
        unref(p);
        const j = await p;
        if (j === null) {
          done = true;
          return null;
        }
        return JSON.parse(j);
      },
      close() {
        done = true;
        ops.op_k_broadcast_close(id);
      },
    };
  }
}

// ----------------------------------------------------- native stdlib bodies

const UUID_HEX = "0123456789abcdef";

/** A random version-4 UUID from the operating system's CSPRNG. */
function randomUuid() {
  const token = ops.op_k_random_token();
  const bytes = [];
  for (let i = 0; i < 16; i++) bytes.push(token.charCodeAt(i) ^ token.charCodeAt(i + 16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  let s = "";
  for (let i = 0; i < 16; i++) {
    if (i === 4 || i === 6 || i === 8 || i === 10) s += "-";
    s += UUID_HEX[(bytes[i] >> 4) & 15] + UUID_HEX[bytes[i] & 15];
  }
  return s;
}

/**
 * A valid Argon2id PHC string with the same parameters as `Password.hash`,
 * whose 32-byte digest is a fixed text rather than the output of hashing
 * any password: verifying against it costs what a real verification costs
 * and matches nothing (33-crypto.md, "Passwords").
 */
const UNUSABLE_HASH = "$argon2id$v=19$m=19456,t=2,p=1$azAvdW51c2FibGUtc2FsdA$Szogbm8gcGFzc3dvcmQgaGFzaGVzIHRvIHRoaXMhISE";

/**
 * `Password.verify(password:hash:)` for a `String` or `Secret<String>` hash:
 * Argon2id in constant time on the blocking pool (`op_k_password_verify`).
 * @param {AbortSignal|null} _c
 * @param {string} password
 * @param {string|Secret} hash
 * @returns {Promise<boolean>}
 */
async function verifyPassword(_c, password, hash) {
  const text = hash instanceof Secret ? hash.reveal() : hash;
  return ops.op_k_password_verify(String(password), String(text));
}

/**
 * The server's `k.crypto.Password` (33-crypto.md, "Passwords"): `hash` is an
 * Argon2id PHC string with a fresh salt (`op_k_password_hash`), `verify`
 * accepts a plain or a `Secret` hash (both overload names point at it).
 */
const password = {
  /**
   * @param {AbortSignal|null} _c
   * @param {string} text
   * @returns {Promise<string>}
   */
  async hash(_c, text) {
    return ops.op_k_password_hash(String(text));
  },
  verify: verifyPassword,
  verify$password$hash: verifyPassword,
  verify$password$hash$1: verifyPassword,
  unusableHash: UNUSABLE_HASH,
};

/**
 * `Hmac.sha256` / `Hmac.sha512` (33-crypto.md): the key is `Bytes` or a
 * `Secret<String>` (its UTF-8 bytes, revealed only here); the MAC is
 * `{bytes, hex}`, compared with {@link macMatches}.
 * @param {number} bits 256 or 512
 */
function hmacOf(bits) {
  return async (_c, key, message) => {
    const k = key instanceof Secret ? new TextEncoder().encode(String(key.reveal())) : key instanceof Uint8Array ? key : Uint8Array.from(key ?? []);
    const m = message instanceof Uint8Array ? message : Uint8Array.from(message ?? []);
    const bytes = new Uint8Array(ops.op_k_hmac(bits, k, m));
    return { bytes, hex: [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("") };
  };
}

/**
 * `Bytes.parse(hex:)`: an even number of hexadecimal digits in either
 * case, else `nil`.
 * @param {string} text
 * @returns {Uint8Array|null}
 */
function bytesFromHex(text) {
  const t = String(text);
  if (t.length % 2 !== 0) return null;
  const out = new Uint8Array(t.length / 2);
  for (let i = 0; i < out.length; i++) {
    const pair = t.slice(2 * i, 2 * i + 2);
    const n = Number.parseInt(pair, 16);
    if (!Number.isInteger(n) || !HEX_PAIR.test(pair)) return null;
    out[i] = n;
  }
  return out;
}

const HEX_PAIR = new RegExp("^[0-9a-fA-F]{2}$");

/**
 * `Bytes.parse(base64:)`: standard or URL-safe base64, padded or not,
 * else `nil`.
 * @param {string} text
 * @returns {Uint8Array|null}
 */
function bytesFromBase64(text) {
  let t = String(text).replaceAll("-", "+").replaceAll("_", "/");
  while (t.endsWith("=")) t = t.slice(0, -1);
  if (!BASE64.test(t) || t.length % 4 === 1) return null;
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const out = [];
  let bits = 0;
  let value = 0;
  for (const ch of t) {
    value = (value << 6) | alphabet.indexOf(ch);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out.push((value >> bits) & 0xff);
    }
  }
  return Uint8Array.from(out);
}

const BASE64 = new RegExp("^[A-Za-z0-9+/]*$");

/**
 * `mac.matches(other)`: compares in time independent of where the bytes
 * differ (33-crypto.md, "Comparing MACs").
 * @param {{bytes: Uint8Array}} mac
 * @param {Uint8Array} other
 */
function macMatches(mac, other) {
  const a = mac.bytes;
  if (!(other instanceof Uint8Array) || other.length !== a.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a[i] ^ other[i];
  return d === 0;
}

/**
 * Server implementations of `@Native` stdlib members that have no
 * generated body, by `module.Owner$member` (the generated bundle falls
 * back to {@link missingNative}). A `Uuid` is its lowercase canonical text,
 * an `EmailAddress` its text.
 */
export const natives = {
  "k.crypto.Password": password,
  "k.crypto.Hmac": { sha256: hmacOf(256), sha512: hmacOf(512) },
  "k.crypto.Mac$matches": macMatches,
  "k.core.Bytes$parse$hex": bytesFromHex,
  "k.core.Bytes$parse$base64": bytesFromBase64,
  "k.crypto.Uuid$random": randomUuid,
  "k.crypto.Uuid$init": (text) => parseUuid(String(text)) ?? panic(`not a UUID: ${JSON.stringify(text)}`),
  "k.crypto.Uuid$parse": (text) => parseUuid(String(text)),
  "k.text.EmailAddress$init": (text) => parseEmail(String(text)) ?? panic(`not an email address: ${JSON.stringify(text)}`),
  "k.text.EmailAddress$parse": (text) => parseEmail(String(text)),
};

/**
 * The server's `k.ui.Theme` (07-themes-and-units.md, "During server
 * rendering"): `current` is the variant the `k_theme` cookie remembers, or
 * `base`; `apply` and `followSystem` do nothing on the server.
 * @param {RequestState} st
 * @returns {{current: string, apply: Function, followSystem: Function}}
 */
export function theme(st) {
  let current = "base";
  try {
    current = decodeURIComponent(st.cookies.get("k_theme") ?? "base") || "base";
  } catch {
    // A malformed cookie is no choice.
  }
  return { current, apply() {}, followSystem() {} };
}

/**
 * The server's `k.server.PublicUrls` (12-routing.md, "Absolute URLs"):
 * `absolute(page)` is the public origin (`K_PUBLIC_ORIGIN`, without a
 * trailing slash) followed by the page's path. Endpoint URLs
 * (`absolute(endpoint:)`) are not built in v0 and panic with their name.
 * @param {(page: unknown) => string} href the app's `Page$href`
 * @returns {{absolute: (page: unknown) => string, absolute$endpoint: Function}}
 */
export function publicUrls(href) {
  return {
    absolute(page) {
      let origin = info().publicOrigin;
      while (origin.endsWith("/")) origin = origin.slice(0, -1);
      return origin + href(page);
    },
    absolute$endpoint: missingNative("k.server.PublicUrls.absolute(endpoint:)"),
  };
}

/**
 * A stand-in for a `@Native` stdlib member the server runtime doesn't
 * implement yet: loading it is fine, calling or constructing it (or a
 * member of it) panics with its name, so only that request fails.
 * @param {string} name `module.Owner$member`
 * @returns {Function}
 */
export function missingNative(name) {
  const fail = () => {
    throw new KPanic(`${name} is not implemented on the server yet`);
  };
  return new Proxy(function missing() {}, {
    apply: fail,
    construct: fail,
    get: (_t, key) => (typeof key === "symbol" ? undefined : missingNative(`${name}.${String(key)}`)),
  });
}
