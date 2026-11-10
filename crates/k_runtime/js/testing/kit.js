/**
 * @module k:testing/kit
 *
 * The test kit behind `k test` (docs/design/21-testing.md;
 * docs/compiler/runtime.md, "The test kit"): what the generated test
 * bundle calls that the compiler doesn't generate.
 *
 * - `render` mounts a view in the test's browser container and returns a
 *   `Screen`; `Screen` and every element handle answer the accessibility
 *   queries (`button(named:)`, `textField(labeled:)`, `text(_)`, …) and the
 *   actions (`click`, `fill`, `press`, `drag(to:)`), each action flushing
 *   synchronous updates.
 * - The hermetic defaults of `k.test`: `TestClock` (virtual time),
 *   `MemoryStorage`, `TestHttpClient`, `TestFiles`, `TestClipboard`,
 *   `TestShare`, `TestGeolocation`, `TestNotifications`, `TestPushSubscriptions`, `TestTabMessaging`,
 *   `TestWakeLock`,
 *   `TestPageVisibility`, `TestConnectivity`, `TestAppUpdates`, `TestPreferences`,
 *   `TestViewport`,
 *   `RecordingJobQueue` and `RecordingEmailProvider`; and the channels'
 *   helpers `TestChannels`, `TestPeers` and `TestChannel` (`./channels.js`).
 * - Fake `IntersectionObserver` and `ResizeObserver`, which report only
 *   what a handle's `appear`, `disappear` and `resize` say.
 * - `fail` and `failThrows`: a failed `expect`, as a panic carrying the
 *   expectation's source range and the values of its sub-expressions.
 * - `run`: runs the selected tests of the generated groups, each with a
 *   fresh container and a fresh document, and reports each outcome.
 *
 * The generated client code reaches the kit through its runtime import
 * (`$k.$test`), and `native` answers the `k.test` natives before the
 * browser's own, so the kit, the runtime and the generated code share one
 * runtime instance.
 */

import { batch, flush, onCleanup, root, signal } from "../runtime/signals.js";
import { KPanic, equals, isPanic, unitCase } from "../runtime/values.js";
import { Instant, PushStream, Secret, httpCode, stringify } from "../runtime/std.js";
import { HttpResponse$isSuccess, HttpResponse$json, HttpResponse$text } from "../runtime/http.js";
import { received } from "../runtime/outbound.js";
import { hasNative, native as browserNative } from "../runtime/services.js";
import { sessionMirror, takeMirror } from "../runtime/rpc.js";
import { navigate } from "../runtime/router.js";
import { RequestState, info as serverInfo, sessionManager, natives as serverNatives } from "../runtime/app.js";
import { channelEnv } from "../runtime/channels.js";
import { install as installDom } from "./dom.js";
import { createFront, testChannel as scriptedChannel } from "./channels.js";
import { TestWebSocketServer, testWebSocketClient as scriptedSockets } from "../runtime/websocket.js";
import { setTesting as cryptoTesting } from "../runtime/crypto.js";
import { collectionIndex, collectionKey, collectionRecord, collectionValues } from "../runtime/collections.js";
import { audioMeter, mediaStream, mediaTrack, recordedTracks, recordingSession } from "../runtime/media.js";
import { peerConnections } from "../runtime/rtc.js";
import { fakeRtc } from "./rtc.js";
import { serveWorker } from "../runtime/workers.js";

// `k test`'s fast passphrase salts (33-crypto.md, "Testing"): 1,000
// iterations, which parse only here.
cryptoTesting(true);

// ---------------------------------------------------------------- state

/**
 * The running test: its container, its screens, and the errors thrown by
 * work it scheduled (effects flushed in a microtask).
 */
let current = { container: null, screens: [], errors: [], clock: null, server: null, offline: false, front: null, context: null, sockets: null, media: null, rtc: null };

/**
 * Starts a test with `container` (the generated browser container): the
 * views it renders are mounted with it. `clock` is the test's virtual
 * clock and `server` its server container, when the run has a server.
 * @param {object} container
 * @param {object|null} [clock]
 * @param {object|null} [server]
 */
export function begin(container, clock = null, server = null) {
  current = { container, screens: [], errors: [], clock, server, offline: false, front: null, context: null, sockets: null, media: null, rtc: null };
}

/**
 * The running test's server container (21-testing.md, "Which side a test
 * runs on"): where a test group's server-side `@Inject` fields come from.
 * @returns {object}
 */
export function server() {
  if (current.server === null) throw new KPanic("this test has no server: the project has no server code to inject from");
  return current.server;
}

/**
 * The running test's `TestClock`, shared by its browser and server
 * containers; a fresh one outside a run.
 * @returns {object}
 */
export function sharedClock() {
  current.clock ??= testClock();
  return current.clock;
}

// ------------------------------------------------- server work in flight

/**
 * The async server work in flight (database statements, sessions, password
 * hashing, in-process requests): a test isn't idle, nor hung, while any is
 * outstanding. Open streams waiting for their next element don't count.
 * @type {Set<Promise<unknown>>}
 */
const inflight = new Set();

/**
 * Tracks `p` in {@link inflight} until it settles.
 * @template T
 * @param {Promise<T>|T} p
 * @returns {Promise<T>|T}
 */
function track(p) {
  if (!p || typeof p.then !== "function") return p;
  inflight.add(p);
  const done = () => inflight.delete(p);
  p.then(done, done);
  return p;
}

/** The async ops of server work that {@link inflight} tracks. */
const TRACKED_OPS = [
  "op_k_db_query",
  "op_k_db_execute",
  "op_k_db_begin",
  "op_k_db_finish",
  "op_k_db_savepoint",
  "op_k_db_ping",
  "op_k_session_get",
  "op_k_session_create",
  "op_k_session_delete",
  "op_k_session_end_all",
  "op_k_session_issue_state",
  "op_k_session_issue_challenge",
  "op_k_session_consume_state",
  "op_k_rate_limit",
  "op_k_password_hash",
  "op_k_password_verify",
  "op_k_jobs_enqueue",
  "op_k_fetch",
  "op_k_smtp_send",
];

/** `true` once `trackOps` has wrapped the ops. */
let opsTracked = false;

/** Wraps the {@link TRACKED_OPS} once, so their calls are tracked. */
function trackOps() {
  if (opsTracked) return;
  opsTracked = true;
  const ops = globalThis.Deno?.core?.ops;
  if (!ops) return;
  for (const name of TRACKED_OPS) {
    const op = ops[name];
    if (typeof op !== "function") continue;
    try {
      ops[name] = (...a) => track(op(...a));
    } catch {
      // A read-only op table: the watchdog then relies on idle rounds.
    }
  }
}

/** Waits until no tracked server work is in flight (bounded). */
async function serverIdle() {
  for (let i = 0; i < 10000 && inflight.size > 0; i++) {
    await Promise.race([...inflight].map((p) => p.then(() => null, () => null)));
    await idle();
  }
}

// ------------------------------------------------- the in-process server

/** The body of a request without one. */
const EMPTY = new Uint8Array(0);
/** The id of the next in-process request. */
let nextRequest = 1;

/**
 * A response body that chunks are pushed to, read by `getReader()` like a
 * `fetch` body stream. `cancel()` (the reader's) makes later pushes
 * report `false`, which ends a server stream.
 */
function pushedBody() {
  const chunks = [];
  const waiters = [];
  let closed = false;
  const deliver = (r) => {
    const w = waiters.shift();
    if (w) w(r);
    else chunks.push(r);
  };
  return {
    /**
     * Delivers a chunk to the reader.
     * @param {Uint8Array} bytes
     * @returns {boolean} `false` once the body is closed or the reader cancelled
     */
    push(bytes) {
      if (closed) return false;
      deliver({ done: false, value: bytes });
      return true;
    },
    /** Ends the body: the reader's next read is done. Idempotent. */
    end() {
      if (closed) return;
      closed = true;
      deliver({ done: true, value: undefined });
    },
    reader: {
      read: () => (chunks.length ? Promise.resolve(chunks.shift()) : new Promise((resolve) => waiters.push(resolve))),
      /**
       * The reader's `cancel()`: closes the body, so later pushes report
       * `false`.
       */
      async cancel() {
        if (closed) return;
        closed = true;
        deliver({ done: true, value: undefined });
      },
    },
  };
}

/**
 * The `fetch` response of an in-process exchange.
 * @param {number} status
 * @param {Array<[string, string]>} headers
 * @param {string|Uint8Array|null} body a unary body, or `null` for a stream
 * @param {ReturnType<typeof pushedBody>|null} stream
 */
function fetchResponse(status, headers, body, stream) {
  const get = (n) => {
    const name = String(n).toLowerCase();
    const vs = headers.filter(([k]) => k.toLowerCase() === name).map(([, v]) => v);
    return vs.length ? vs.join(", ") : null;
  };
  const bytes = body === null ? EMPTY : typeof body === "string" ? new TextEncoder().encode(body) : body;
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get },
    text: async () => new TextDecoder().decode(bytes),
    body: { getReader: () => (stream ? stream.reader : { read: async () => ({ done: true, value: undefined }), cancel: async () => {} }) },
  };
}

/**
 * The context of one test with a server: its clock, its per-test instances,
 * its group's server container factory, its one session (the session
 * cookie every in-process request carries) and the direct server container
 * the test injects from.
 */
class TestContext {
  /**
   * @param {object} server the server entry's `$test`
   * @param {(st: object) => object} container the group's server container factory
   */
  constructor(server, container) {
    this.app = server.app;
    this.spec = server.spec;
    this.container = container;
    this.clock = testClock();
    this.memo = new Map();
    /** @type {string|null} the session id the test's browser holds */
    this.cookie = null;
    /** @type {string|null} the guest session id it holds (`k_guest`) */
    this.guest = null;
    /** @type {RequestState|null} the state of the test's own server container */
    this.direct = null;
  }

  /** The request headers every in-process request of the test carries. */
  cookies() {
    const jar = [];
    if (this.cookie !== null) jar.push(`k_session=${this.cookie}`);
    if (this.guest !== null) jar.push(`k_guest=${this.guest}`);
    return jar.length === 0 ? [] : [["cookie", jar.join("; ")]];
  }

  /**
   * A session change on any of the test's requests (`SessionManager`):
   * the browser's cookie and `Session` mirror, and the test's own server
   * state, follow it, so the test acts as one browser tab.
   * @param {RequestState} st
   */
  sessionChanged(st) {
    this.cookie = st.sessionId;
    this.guest = st.guestId;
    if (this.direct !== null && st !== this.direct) {
      this.direct.sessionId = st.sessionId;
      this.direct.guestId = st.guestId;
      this.direct.loaded = st.loaded;
    }
    try {
      sessionMirror.set(st.mirror());
    } catch {
      // A session that isn't loaded yet has no mirror to share.
    }
  }

  /** A fresh request of this test for the app's handler. */
  request(method, path, headers, body, sink) {
    return { id: nextRequest++, method, path, headers: [...headers, ...this.cookies()], body: body ?? EMPTY, remote: "127.0.0.1", test: this, sink };
  }

  /**
   * `TestClient.send`: one exchange through the whole app, answered as
   * `{status, headers, body}`. A streamed response is read to its end.
   * @param {{method: string, url: string, headers: Array<[string, string]>, body: Uint8Array|null}} wire
   */
  async send(wire) {
    const path = pathAndQuery(wire.url);
    const stream = pushedBody();
    let head = null;
    const sink = { start: (h) => (head = h), chunk: (b) => stream.push(b), end: () => stream.end() };
    const r = await track(this.app(this.request(wire.method, path, wire.headers, wire.body, sink)));
    if (r && r.streamed) {
      const parts = [];
      for (;;) {
        const { done, value } = await stream.reader.read();
        if (done) break;
        parts.push(value);
      }
      const n = parts.reduce((a, p) => a + p.length, 0);
      const body = new Uint8Array(n);
      let at = 0;
      for (const p of parts) {
        body.set(p, at);
        at += p.length;
      }
      return { status: head?.status ?? 200, headers: head?.headers ?? [], body };
    }
    return { status: r.status, headers: r.headers, body: r.body };
  }

  /**
   * The browser's `fetch` in a test: every request goes to the app's
   * handler in-process, through the real wire format.
   * @param {string} url
   * @param {{method?: string, headers?: Record<string, string>, body?: string|Uint8Array, signal?: AbortSignal}} [init]
   */
  fetch(url, init = {}) {
    const headers = Object.entries(init.headers ?? {}).map(([n, v]) => [n.toLowerCase(), String(v)]);
    const body = init.body === undefined || init.body === null ? EMPTY : typeof init.body === "string" ? new TextEncoder().encode(init.body) : init.body;
    return new Promise((resolve, reject) => {
      const stream = pushedBody();
      init.signal?.addEventListener?.("abort", () => stream.reader.cancel());
      const sink = {
        start: (h) => resolve(fetchResponse(h.status, h.headers, null, stream)),
        chunk: (b) => stream.push(b),
        end: () => stream.end(),
      };
      track(this.app(this.request(String(init.method ?? "GET").toUpperCase(), pathAndQuery(url), headers, body, sink))).then((r) => {
        if (r && r.streamed) return;
        resolve(fetchResponse(r.status, r.headers, r.body, null));
      }, reject);
    });
  }
}

/** The path and query of a URL (absolute or not). */
function pathAndQuery(url) {
  const text = String(url ?? "/");
  const scheme = text.indexOf("://");
  if (scheme < 0) return text.startsWith("/") ? text : `/${text}`;
  const slash = text.indexOf("/", scheme + 3);
  return slash < 0 ? "/" : text.slice(slash);
}

/**
 * An error thrown by a microtask (an effect the runtime flushes after a
 * write) belongs to the running test, which then fails with it, instead of
 * ending the whole run.
 */
const baseMicrotask = globalThis.queueMicrotask ?? ((f) => Promise.resolve().then(f));
globalThis.queueMicrotask = (f) =>
  baseMicrotask(() => {
    try {
      f();
    } catch (e) {
      current.errors.push(e);
    }
  });

// ------------------------------------------------------------ randomness

/**
 * One ChaCha20 block (RFC 8439, section 2.3): 64 bytes of keystream for
 * `key` (8 words), block `counter` and `nonce` (3 words).
 * @param {Uint32Array} key
 * @param {number} counter
 * @param {Uint32Array} nonce
 * @returns {Uint8Array}
 */
export function chacha20Block(key, counter, nonce) {
  const s = new Uint32Array(16);
  s.set([0x61707865, 0x3320646e, 0x79622d32, 0x6b206574]);
  s.set(key, 4);
  s[12] = counter >>> 0;
  s.set(nonce, 13);
  const x = s.slice();
  const rotl = (v, c) => (v << c) | (v >>> (32 - c));
  const quarter = (a, b, c, d) => {
    x[a] = (x[a] + x[b]) >>> 0; x[d] = rotl(x[d] ^ x[a], 16);
    x[c] = (x[c] + x[d]) >>> 0; x[b] = rotl(x[b] ^ x[c], 12);
    x[a] = (x[a] + x[b]) >>> 0; x[d] = rotl(x[d] ^ x[a], 8);
    x[c] = (x[c] + x[d]) >>> 0; x[b] = rotl(x[b] ^ x[c], 7);
  };
  for (let i = 0; i < 10; i++) {
    quarter(0, 4, 8, 12); quarter(1, 5, 9, 13); quarter(2, 6, 10, 14); quarter(3, 7, 11, 15);
    quarter(0, 5, 10, 15); quarter(1, 6, 11, 12); quarter(2, 7, 8, 13); quarter(3, 4, 9, 14);
  }
  const out = new Uint8Array(64);
  for (let i = 0; i < 16; i++) {
    const v = (x[i] + s[i]) >>> 0;
    out[4 * i] = v & 255;
    out[4 * i + 1] = (v >>> 8) & 255;
    out[4 * i + 2] = (v >>> 16) & 255;
    out[4 * i + 3] = v >>> 24;
  }
  return out;
}

/**
 * A test's random source (docs/design/33-crypto.md, "Testing";
 * 44-channels.md, "Reconnecting"): a ChaCha20 keystream whose key is
 * derived from `seed`, the test's full name, so every run of a test draws
 * the same values and two tests draw different ones. The key's eight words
 * are 32-bit FNV-1a hashes of the name's UTF-8 bytes, each started from a
 * different offset; the nonce is zero.
 *
 * `bytes(array)` fills an integer typed array, as `crypto.getRandomValues`
 * does; `double()` is uniform in [0, 1) with 53 random bits, as
 * `Math.random` is.
 * @param {string} seed
 */
export function seededRandom(seed) {
  const utf8 = new TextEncoder().encode(String(seed));
  const key = new Uint32Array(8);
  for (let w = 0; w < 8; w++) {
    let h = (0x811c9dc5 ^ Math.imul(w + 1, 0x9e3779b9)) >>> 0;
    for (const b of utf8) h = Math.imul(h ^ b, 0x01000193) >>> 0;
    key[w] = h;
  }
  const nonce = new Uint32Array(3);
  let counter = 0;
  let block = new Uint8Array(0);
  let at = 0;
  const byte = () => {
    if (at === block.length) {
      block = chacha20Block(key, counter++, nonce);
      at = 0;
    }
    return block[at++];
  };
  return {
    bytes(a) {
      const view = new Uint8Array(a.buffer, a.byteOffset, a.byteLength);
      for (let i = 0; i < view.length; i++) view[i] = byte();
      return a;
    },
    double() {
      // 53 bits: 7 bytes, the top 5 bits of the last dropped.
      let v = 0;
      for (let i = 0; i < 6; i++) v = v * 256 + byte();
      v = v * 32 + (byte() >>> 3);
      return v / 2 ** 53;
    },
  };
}

/** The running test's random source ({@link installBrowser}). */
let random = seededRandom("");

/**
 * The running test's random source, for doubles that draw (a reconnect's
 * jitter): the same stream `crypto.getRandomValues` and `Math.random` draw
 * from.
 */
export function testRandom() {
  return random;
}

/**
 * Installs what a client reads from the browser besides the DOM: a
 * deterministic `crypto.randomUUID` (each test's ids start again from
 * `00000000-0000-4000-8000-000000000001`), `crypto.getRandomValues` and
 * `Math.random` drawing from the test's random source seeded with `seed`
 * ({@link seededRandom}), `location` at `/`, `history`, `matchMedia`
 * (nothing matches), an `IntersectionObserver` and a `ResizeObserver` that
 * report nothing until a handle says so ({@link FakeObserver}), and an
 * empty `document.title`.
 * @param {string} [seed] the test's full name
 */
export function installBrowser(seed = "") {
  let n = 0;
  const hex = (v, w) => v.toString(16).padStart(w, "0");
  random = seededRandom(seed);
  globalThis.crypto = {
    randomUUID: () => `00000000-0000-4000-8000-${hex(++n, 12)}`,
    getRandomValues: (a) => random.bytes(a),
  };
  Math.random = () => random.double();
  const location = { origin: "http://localhost", pathname: "/", search: "", hash: "" };
  Object.defineProperty(location, "href", {
    get: () => location.origin + location.pathname + location.search,
    set: (v) => {
      const path = String(v).startsWith(location.origin) ? String(v).slice(location.origin.length) : String(v);
      const q = path.indexOf("?");
      location.pathname = q < 0 ? path : path.slice(0, q);
      location.search = q < 0 ? "" : path.slice(q);
    },
  });
  globalThis.location = location;
  globalThis.history = {
    pushState: (_s, _t, url) => {
      location.href = url;
    },
    replaceState: (_s, _t, url) => {
      location.href = url;
    },
  };
  globalThis.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  // New classes per test, so the runtime's shared observers are made again.
  watching = new Map();
  globalThis.IntersectionObserver = class extends FakeObserver {};
  globalThis.ResizeObserver = class extends FakeObserver {};
  // The window's events, new per test: the channels' client listens for
  // `online` (`TestConnectivity.goOnline()` dispatches it).
  const listeners = new Map();
  globalThis.addEventListener = (type, f) => {
    if (!listeners.has(type)) listeners.set(type, new Set());
    listeners.get(type).add(f);
  };
  globalThis.removeEventListener = (type, f) => listeners.get(type)?.delete(f);
  globalThis.dispatchEvent = (e) => {
    for (const f of [...(listeners.get(e.type) ?? [])]) f(e);
    return true;
  };
  globalThis.document.title = "";
}

/**
 * The elements the fake observers watch, each with its observers; made
 * again by each test's `installBrowser`.
 * @type {Map<object, Set<FakeObserver>>}
 */
let watching = new Map();

/**
 * An `IntersectionObserver` or `ResizeObserver` with no layout behind it
 * (45-web-platform.md, "Tests"): it reports an element only when a
 * handle's `appear()`, `disappear()` or `resize(width:height:)` says so.
 */
class FakeObserver {
  /** @param {(entries: object[], observer: object) => void} callback */
  constructor(callback) {
    this.callback = callback;
  }
  /** Watches `el`. */
  observe(el) {
    if (!watching.has(el)) watching.set(el, new Set());
    watching.get(el).add(this);
  }
  /** Stops watching `el`. */
  unobserve(el) {
    watching.get(el)?.delete(this);
  }
  /** Stops watching everything. */
  disconnect() {
    for (const set of watching.values()) set.delete(this);
  }
}

/**
 * Reports `entry` for `el` to every observer of class `C` watching it,
 * then flushes the updates its handlers made.
 * @param {object} el
 * @param {Function} C `IntersectionObserver` or `ResizeObserver`
 * @param {object} entry
 */
function report(el, C, entry) {
  for (const o of [...(watching.get(el) ?? [])]) {
    if (o instanceof C) o.callback([{ target: el, ...entry }], o);
  }
  flush();
}

/** Unmounts every view the running test rendered. */
function end() {
  for (const s of current.screens.splice(0)) s.unmount();
  stopTestMedia();
  current.rtc?.$reset();
  current.front?.reset();
  channelEnv({ socket: noSocket, browser: () => null });
  active = null;
}

// ------------------------------------------------------------ failures

/**
 * A failed test: a panic (so no K `catch` intercepts it) with the details
 * the runner reports in `kTest`.
 * @param {string} message
 * @param {number} start the source range's start offset
 * @param {number} end its end offset
 * @param {Array<[string, string]>} values
 */
function failure(message, start, end, values) {
  const e = new KPanic(message);
  e.kTest = { start, end, message, values };
  return e;
}

/** Text of a value for a failure message, never throwing. */
function shown(f) {
  try {
    return String(f());
  } catch (e) {
    return `<${e && e.message ? e.message : e}>`;
  }
}

/**
 * A failed `expect`: throws.
 * @param {{start: number, end: number, source: string, values: Array<[string, () => string]>}} info
 */
export function fail(info) {
  throw failure(
    `expectation failed: ${info.source}`,
    info.start,
    info.end,
    info.values.map(([text, f]) => [text, shown(f)]),
  );
}

/**
 * A failed `expect throws`: throws. `got` describes what was thrown
 * instead, or is `null` when nothing was.
 * @param {{start: number, end: number, pattern: string, got: (() => string)|null}} info
 */
export function failThrows(info) {
  const got = info.got === null ? "nothing was thrown" : `it threw ${shown(info.got)}`;
  throw failure(`expected an error matching ${info.pattern}, but ${got}`, info.start, info.end, []);
}

/** Quotes a string value for a failure message. */
export function quote(s) {
  return JSON.stringify(String(s));
}

// ------------------------------------------------------------- queries

/** Whitespace runs collapsed, the ends trimmed. */
function norm(s) {
  return String(s ?? "")
    .split(new RegExp("\\s+"))
    .filter((p) => p !== "")
    .join(" ");
}

/** Every element below `node`, in document order (not `node` itself). */
function descendants(node) {
  const out = [];
  const walk = (n) => {
    for (let c = n.firstChild; c !== null; c = c.nextSibling) {
      if (c.nodeType === 1) {
        out.push(c);
        walk(c);
      } else if (c.nodeType === 11) {
        walk(c);
      }
    }
  };
  walk(node);
  return out;
}

/** The attribute `name` of `el`, or `null` (also for non-elements). */
const attr = (el, name) => (typeof el.getAttribute === "function" ? el.getAttribute(name) : null);
/** The explicit ARIA `role` of `el`, or `null`. */
const role = (el) => attr(el, "role");
/** The `type` of an input, lower case; `text` when absent. */
const type = (el) => (attr(el, "type") ?? "text").toLowerCase();
/** The tag name of `el`, lower case. */
const tag = (el) => el.localName;

/** The element's text, normalised. */
function textOf(el) {
  return norm(el.textContent);
}

/** The root of the tree `el` is in. */
function treeRoot(el) {
  let n = el;
  while (n.parentNode !== null && n.parentNode !== undefined) n = n.parentNode;
  return n;
}

/** The accessible name of a form control: its label. */
function labelOf(el) {
  const aria = attr(el, "aria-label");
  if (aria !== null) return norm(aria);
  const by = attr(el, "aria-labelledby");
  const top = treeRoot(el);
  if (by !== null) {
    const named = descendants(top).filter((e) => by.split(" ").includes(e.id));
    if (named.length) return norm(named.map((e) => e.textContent).join(" "));
  }
  const id = el.id;
  if (id) {
    const label = descendants(top).find((e) => tag(e) === "label" && attr(e, "for") === id);
    if (label) return textOf(label);
  }
  for (let p = el.parentNode; p !== null && p !== undefined && p.nodeType === 1; p = p.parentNode) {
    if (tag(p) === "label") return textOf(p);
  }
  return "";
}

/** The accessible name of a button, link, heading or landmark. */
function nameOf(el) {
  const aria = attr(el, "aria-label");
  if (aria !== null) return norm(aria);
  if (tag(el) === "input") return norm(el.value ?? attr(el, "value") ?? "");
  return textOf(el);
}

/** What makes an element one of each kind the queries name: its tag, `type` or ARIA role. */
const KINDS = {
  heading: (el) => new RegExp("^h[1-6]$").test(tag(el)) || role(el) === "heading",
  link: (el) => (tag(el) === "a" && attr(el, "href") !== null) || role(el) === "link",
  button: (el) =>
    tag(el) === "button" || role(el) === "button" || (tag(el) === "input" && ["button", "submit", "reset"].includes(type(el))),
  textField: (el) => (tag(el) === "input" && ["text", "email", "search", "url", "tel"].includes(type(el))) || role(el) === "textbox",
  passwordField: (el) => tag(el) === "input" && type(el) === "password",
  textArea: (el) => tag(el) === "textarea",
  numberField: (el) => tag(el) === "input" && type(el) === "number",
  dropdown: (el) => tag(el) === "select",
  checkbox: (el) => (tag(el) === "input" && type(el) === "checkbox") || role(el) === "checkbox",
  item: (el) => tag(el) === "li",
  list: (el) => tag(el) === "ul" || tag(el) === "ol",
  navigation: (el) => tag(el) === "nav" || role(el) === "navigation",
  alert: (el) => role(el) === "alert",
  status: (el) => role(el) === "status",
  video: (el) => tag(el) === "video",
  audio: (el) => tag(el) === "audio",
};

/** Keeps only the deepest of `els`: none that contains another. */
function deepest(els) {
  return els.filter((e) => !els.some((o) => o !== e && contains(e, o)));
}

/**
 * `true` if `b` is a proper descendant of `a`.
 * @param {object} a
 * @param {object} b
 * @returns {boolean}
 */
function contains(a, b) {
  for (let n = b.parentNode; n !== null && n !== undefined; n = n.parentNode) if (n === a) return true;
  return false;
}

/** `true` when `el` and its ancestors are shown. */
function visible(el) {
  for (let n = el; n !== null && n !== undefined && n.nodeType === 1; n = n.parentNode) {
    if (attr(n, "hidden") !== null || attr(n, "aria-hidden") === "true") return false;
    if (n.style && n.style.getPropertyValue("display") === "none") return false;
  }
  return true;
}

/** The keys of `Key`, by case tag. */
const KEYS = ["Enter", "Escape", "Tab", " ", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];

/** A bubbling event of `type` with `extra` fields. */
function event(type, extra) {
  const e = new globalThis.Event(type, { bubbles: true });
  return Object.assign(e, extra ?? {});
}

/** The form `el` belongs to. */
function formOf(el) {
  for (let n = el.parentNode; n !== null && n !== undefined && n.nodeType === 1; n = n.parentNode) {
    if (tag(n) === "form") return n;
  }
  return null;
}

/** `true` for a control that is disabled. */
function disabled(el) {
  return el.disabled === true || attr(el, "disabled") !== null || attr(el, "aria-disabled") === "true";
}

/** Submits `form` as a browser does: a cancelable `submit` event. */
function submit(form) {
  form.dispatchEvent(event("submit"));
}

/**
 * The queries every handle answers, over the elements below the roots
 * `scope()` returns.
 */
class Queries {
  /**
   * The roots to search below; none for the base class.
   * @returns {Array<object>} the elements to search below
   */
  scope() {
    return [];
  }

  /** Every element below the scope. */
  all() {
    return this.scope().flatMap(descendants);
  }

  /**
   * A handle whose elements are those `pick` keeps of every element in
   * scope, re-evaluated on each use.
   * @param {string} describe the query as written, for failure messages
   * @param {(els: object[]) => object[]} pick
   * @returns {Handle}
   */
  find(describe, pick) {
    return new Handle(describe, () => pick(this.all()));
  }

  /**
   * The elements of `kind` whose accessible name (or, with `how` set to
   * `"label"`, whose label) is `name`, whitespace-normalised.
   * @param {string} kind a key of `KINDS`
   * @param {string} name
   * @param {"label"} [how]
   * @returns {Handle}
   */
  named(kind, name, how) {
    const wanted = norm(name);
    const of = how === "label" ? labelOf : nameOf;
    const verb = how === "label" ? "labeled" : "named";
    return this.find(`${kind}(${verb}: ${quote(name)})`, (els) => els.filter((e) => KINDS[kind](e) && of(e) === wanted));
  }

  /**
   * The elements of `kind`, whatever their name.
   * @param {string} kind a key of `KINDS`
   * @returns {Handle}
   */
  only(kind) {
    return this.find(`${kind}()`, (els) => els.filter(KINDS[kind]));
  }

  /**
   * One handle per element of `kind` in scope, in document order, each
   * bound to its element.
   * @param {string} kind a key of `KINDS`
   * @returns {Handle[]}
   */
  every(kind) {
    return this.all()
      .filter(KINDS[kind])
      .map((e) => new Handle(`${kind}s()`, () => [e]));
  }

  /** `heading(name)`: the heading named `name`. */
  heading(name) {
    return this.named("heading", name);
  }
  /** `link(name)`: the link named `name`. */
  link(name) {
    return this.named("link", name);
  }
  /** `button(name)`: the button named `name`. */
  button(name) {
    return this.named("button", name);
  }
  /** `navigation(name)`: the navigation landmark named `name`. */
  navigation(name) {
    return this.named("navigation", name);
  }
  /** `textField(label)`: the text field labelled `label`. */
  textField(label) {
    return this.named("textField", label, "label");
  }
  /** `passwordField(label)`: the password field labelled `label`. */
  passwordField(label) {
    return this.named("passwordField", label, "label");
  }
  /** `textArea(label)`: the text area labelled `label`. */
  textArea(label) {
    return this.named("textArea", label, "label");
  }
  /** `numberField(label)`: the number field labelled `label`. */
  numberField(label) {
    return this.named("numberField", label, "label");
  }
  /** `dropdown(label)`: the `<select>` labelled `label`. */
  dropdown(label) {
    return this.named("dropdown", label, "label");
  }
  /** `checkbox(label)`: the checkbox labelled `label`. */
  checkbox(label) {
    return this.named("checkbox", label, "label");
  }
  /** `text(_)`: the deepest elements whose whole text is `text`. */
  text(text) {
    const wanted = norm(text);
    return this.find(`text(${quote(text)})`, (els) => deepest(els.filter((e) => textOf(e) === wanted)));
  }
  /** `text(containing:)`: the deepest elements whose text contains `text`. */
  text$containing(text) {
    const wanted = norm(text);
    return this.find(`text(containing: ${quote(text)})`, (els) => deepest(els.filter((e) => textOf(e).includes(wanted))));
  }
  /** `item(containing:)`: the list items whose text contains `text`. */
  item$containing(text) {
    const wanted = norm(text);
    return this.find(`item(containing: ${quote(text)})`, (els) => els.filter((e) => KINDS.item(e) && textOf(e).includes(wanted)));
  }
  /** `item(containing:)`, by the name the compiler gives it: it has no overload, so no label suffix. */
  item(text) {
    return this.item$containing(text);
  }
  /** `list()`: the list in scope. */
  list() {
    return this.only("list");
  }
  /** `list(named:)`: the list named `name`. */
  list$named(name) {
    return this.named("list", name);
  }
  /** `alert()`: the element with role `alert`. */
  alert() {
    return this.only("alert");
  }
  /** `status()`: the element with role `status`. */
  status() {
    return this.only("status");
  }
  /** `video(labeled:)`: the `video` element whose accessible name is `label`. */
  video(label) {
    return this.named("video", label);
  }
  /** `audio(labeled:)`: the `audio` element whose accessible name is `label`. */
  audio(label) {
    return this.named("audio", label);
  }
  /** `items()`: a handle per list item, in document order. */
  items() {
    return this.every("item");
  }
  /** `buttons()`: a handle per button, in document order. */
  buttons() {
    return this.every("button");
  }
  /** `headings()`: a handle per heading, in document order. */
  headings() {
    return this.every("heading");
  }
  /** `links()`: a handle per link, in document order. */
  links() {
    return this.every("link");
  }
}

/**
 * A handle to the elements a query matched, evaluated on every use: a
 * handle keeps answering as the view changes.
 */
class Handle extends Queries {
  /**
   * Creates a handle.
   * @param {string} describe the query as written, for failure messages
   * @param {() => object[]} match evaluates the query
   */
  constructor(describe, match) {
    super();
    this.describe = describe;
    this.match = match;
  }

  /**
   * Queries on a handle search below its element, when it matched exactly
   * one.
   * @returns {object[]}
   */
  scope() {
    const m = this.match();
    return m.length === 1 ? m : [];
  }

  /** The one element, or a test failure naming the query. */
  one() {
    const m = this.match();
    if (m.length === 1) return m[0];
    throw failure(
      m.length === 0 ? `${this.describe} found nothing` : `${this.describe} found ${m.length} elements`,
      -1,
      -1,
      [],
    );
  }

  /** `true` when the query matches exactly one element. */
  get exists() {
    return this.match().length === 1;
  }
  /**
   * `true` when the query matches exactly one element and neither it nor
   * an ancestor is hidden.
   */
  get isVisible() {
    const m = this.match();
    return m.length === 1 && visible(m[0]);
  }
  /**
   * Whether the `video` or `audio` element has something to play: a
   * stream with a live track, a file or a URL (the playback chunk's
   * `$kSource`).
   */
  get isPlaying() {
    const s = this.one().$kSource;
    return s ? (s.$ === 0 ? s._0.isLive === true : true) : false;
  }
  /** Whether the `video` or `audio` element plays no sound. */
  get isMuted() {
    const el = this.one();
    return el.muted === true || attr(el, "muted") !== null;
  }
  /** Whether the checkbox is checked (the property, or `aria-checked`). */
  get isChecked() {
    const el = this.one();
    return el.checked === true || attr(el, "aria-checked") === "true";
  }
  /**
   * Whether the element is the document's focused element and still in
   * the view (53-element-references.md, "Tests").
   */
  get isFocused() {
    const el = this.one();
    return el.ownerDocument?.activeElement === el && inDocument(el);
  }
  /** Whether code scrolled the element into view (`scrollIntoView`). */
  get wasScrolledIntoView() {
    return this.one().scrolledIntoView != null;
  }
  /** Whether the toggle button is pressed (`aria-pressed`). */
  get isPressed() {
    return attr(this.one(), "aria-pressed") === "true";
  }
  /** Whether the control is disabled. */
  get isDisabled() {
    return disabled(this.one());
  }
  /** The control's current value, as text. */
  get value() {
    const el = this.one();
    return String(el.value ?? attr(el, "value") ?? "");
  }
  /** `text`, the element's text content (a method: `text(_)` is a query). */
  $text() {
    return textOf(this.one());
  }
  /** The heading's level: `aria-level`, or the `h1`–`h6` tag; 0 otherwise. */
  get level() {
    const el = this.one();
    const aria = attr(el, "aria-level");
    if (aria !== null) return Number(aria);
    const m = new RegExp("^h([1-6])$").exec(tag(el));
    return m ? Number(m[1]) : 0;
  }

  /**
   * Clicks the element as a user does: nothing for a disabled control; a
   * checkbox or radio toggles and fires `click`, `input` and `change`; a
   * submit button that isn't prevented submits its form. Effects are
   * flushed afterwards.
   */
  click() {
    const el = this.one();
    if (disabled(el)) return;
    if (tag(el) === "input" && (type(el) === "checkbox" || type(el) === "radio")) {
      el.checked = type(el) === "radio" ? true : !el.checked;
      el.dispatchEvent(event("click"));
      el.dispatchEvent(event("input"));
      el.dispatchEvent(event("change"));
      flush();
      return;
    }
    const e = event("click");
    el.dispatchEvent(e);
    const isSubmit = (tag(el) === "button" && (attr(el, "type") ?? "submit") === "submit") || (tag(el) === "input" && type(el) === "submit");
    const form = isSubmit && !e.defaultPrevented ? formOf(el) : null;
    if (form !== null) submit(form);
    flush();
  }

  /**
   * Types `text` into the field, replacing its value, and fires `input` and
   * `change`.
   * @param {string} text
   * @throws {Error} a test failure when the field is disabled
   */
  fill(text) {
    const el = this.one();
    if (disabled(el)) throw failure(`${this.describe} is disabled`, -1, -1, []);
    el.value = String(text);
    el.dispatchEvent(event("input"));
    el.dispatchEvent(event("change"));
    flush();
  }

  /**
   * Moves the focus away from the element: fires `blur`, then a bubbling
   * `focusout`, as leaving a form field does.
   */
  blur() {
    const el = this.one();
    el.dispatchEvent(new globalThis.Event("blur"));
    el.dispatchEvent(event("focusout"));
    flush();
  }

  /**
   * Presses `key` on the element (`keydown`, then `keyup`); an unprevented
   * Enter outside a text area submits the enclosing form.
   * @param {{$: number, _0?: string}} [key] a `Key` case; Enter by default
   */
  press(key) {
    const el = this.one();
    const name = key && key.$ === 8 ? String(key._0) : KEYS[key && key.$] ?? "Enter";
    const down = event("keydown", { key: name });
    el.dispatchEvent(down);
    const form = name === "Enter" && !down.defaultPrevented && tag(el) !== "textarea" ? formOf(el) : null;
    if (form !== null) submit(form);
    el.dispatchEvent(event("keyup", { key: name }));
    flush();
  }

  /**
   * Drags this element onto `target` with the mouse, as a user does
   * (41-drag-and-drop.md, "Tests"): presses the pointer here, moves it
   * past the drag threshold onto `target`, releases it there, and clicks
   * where the browser would (the click a drag ends with, which the drag
   * swallows).
   */
  /**
   * Makes the element appear in the viewport: every intersection observer
   * watching it reports it fully visible (45-web-platform.md, "Element
   * events").
   */
  appear() {
    report(this.one(), globalThis.IntersectionObserver, { isIntersecting: true, intersectionRatio: 1 });
  }

  /** Makes the element leave the viewport. */
  disappear() {
    report(this.one(), globalThis.IntersectionObserver, { isIntersecting: false, intersectionRatio: 0 });
  }

  /**
   * Gives the element's content box a new size: every resize observer
   * watching it reports it.
   * @param {string} width a `Length` in pixels (`"320px"`)
   * @param {string} height
   */
  resize(width, height) {
    report(this.one(), globalThis.ResizeObserver, { contentRect: { width: Number.parseFloat(width), height: Number.parseFloat(height) } });
  }

  drag(target) {
    const from = this.one();
    const to = target.one();
    const at = (x) => ({ pointerType: "mouse", pointerId: 1, isPrimary: true, button: 0, clientX: x, clientY: x });
    from.dispatchEvent(event("pointerdown", at(0)));
    to.dispatchEvent(event("pointermove", at(8)));
    to.dispatchEvent(event("pointermove", at(16)));
    to.dispatchEvent(event("pointerup", at(16)));
    to.dispatchEvent(event("click", at(16)));
    flush();
  }
}

/** `true` when `el` is in a document (the test DOM keeps the focus on removed elements). */
function inDocument(el) {
  let n = el;
  while (n.parentNode) n = n.parentNode;
  return n.nodeType === 9;
}

/** A rendered view: the queries over its container. */
class Screen extends Queries {
  /**
   * Creates the screen of a mounted view.
   * @param {object|null} host the view's container element
   * @param {(() => void)|null} dispose unmounts the view
   */
  constructor(host, dispose) {
    super();
    this.host = host;
    this.dispose = dispose;
  }

  /**
   * Queries on a screen search its container; none once unmounted.
   * @returns {object[]}
   */
  scope() {
    return this.host === null ? [] : [this.host];
  }

  /**
   * The focused element of the view: a handle matching the element of the
   * view that is its document's active element, else nothing.
   */
  get focused() {
    // A cloned template's nodes may keep the template's document as their
    // owner in the test DOM, so each element is asked about its own.
    return new Handle("focused", () => {
      const found = [];
      const visit = (n) => {
        if (n.nodeType === 1 && n.ownerDocument?.activeElement === n) found.push(n);
        for (const c of n.childNodes ?? []) visit(c);
      };
      if (this.host !== null && inDocument(this.host)) for (const c of this.host.childNodes) visit(c);
      return found;
    });
  }

  /** The document's title. */
  get title() {
    return String(globalThis.document?.title ?? "");
  }

  /** The page the router is on: the page of the browser's current URL. */
  get currentPage() {
    const loc = globalThis.location;
    const url = { pathname: loc?.pathname ?? "/", search: loc?.search ?? "" };
    for (const r of pages) {
      const params = r.parse(url);
      if (params !== null) return r.value(params);
    }
    throw new KPanic(`Screen.currentPage: no page has the URL ${url.pathname}${url.search}`);
  }

  /**
   * Waits until the view is quiet: its pending first load, every microtask
   * and event-loop turn, and in-flight server work; then flushes effects.
   */
  async settle() {
    if (this.pending) await this.pending;
    // At least two rounds with a front: the first round's turn of the
    // event loop may have delivered what only the second sees.
    let mark = -1;
    await idle();
    await serverIdle();
    // Channel traffic waits on turns of the event loop (a broadcast's next
    // element): more rounds while it moves (44-channels.md, "Testing").
    for (let i = 0; i < 200 && current.front && current.front.activity !== mark; i++) {
      mark = current.front.activity;
      await idle();
      await serverIdle();
    }
    if (this.pending) await this.pending;
    flush();
  }

  /** Disposes the view and removes its container. Idempotent. */
  unmount() {
    if (this.host === null) return;
    this.dispose?.();
    stopTestMedia();
    this.host.remove();
    this.host = null;
    flush();
  }
}

/**
 * `render(view)`: mounts `mount(ctx)` (the generated view's renderer) in a
 * new container in the document's body, with the test's container, and
 * returns its screen.
 * @param {(ctx: object) => object} mount
 * @param {() => string} [href] a rendered page's URL
 */
export function render(mount, href) {
  // A rendered page puts the browser on its URL.
  if (typeof href === "function" && globalThis.location) globalThis.location.href = href();
  const doc = globalThis.document;
  const host = doc.createElement("div");
  doc.body.appendChild(host);
  let dispose = null;
  let pending = null;
  root((d) => {
    dispose = d;
    const node = mount({ c: current.container });
    if (node && typeof node.then === "function") {
      // A page with `@Load` fields renders once its chunk is ready.
      // A page that fails to render fails the test with the reason.
      pending = node.then(
        (n) => {
          if (n && host.parentNode !== null) host.appendChild(n);
          flush();
        },
        (e) => {
          current.errors.push(e);
        },
      );
    } else if (node) {
      host.appendChild(node);
    }
  });
  flush();
  const s = new Screen(host, dispose);
  s.pending = pending;
  current.screens.push(s);
  return s;
}

/**
 * The values of a page's props (`{name: () => value}`), by name: the
 * parameters `Page` values are built from.
 * @param {Record<string, () => unknown>} props
 * @returns {Record<string, unknown>}
 */
export function propValues(props) {
  const out = {};
  for (const k of Object.keys(props)) out[k] = typeof props[k] === "function" ? props[k]() : props[k];
  return out;
}

/**
 * Lets pending work run: many microtask turns, then one turn of the event
 * loop (`op_k_test_tick`), so ops that completed meanwhile (a stream's
 * next element from the server) deliver their results.
 */
async function idle() {
  for (let i = 0; i < 2000; i++) await null;
  const tick = globalThis.Deno?.core?.ops?.op_k_test_tick;
  if (typeof tick === "function") await tick();
}

// -------------------------------------------------------------- doubles

/** The start of virtual time: 2026-01-01T12:00:00Z. */
const EPOCH = Date.UTC(2026, 0, 1, 12, 0, 0);

/** `k.test.TestClock`: virtual time with timers fired by `advance`. */
export function testClock() {
  let now = EPOCH;
  let seq = 0;
  const timers = [];
  const add = (due, every, f) => {
    const t = { due, every, f, seq: seq++, live: true };
    timers.push(t);
    return {
      /** Cancels the timer. */
      cancel() {
        t.live = false;
      },
    };
  };
  return {
    /** The virtual current time. */
    get now() {
      return new Instant(now);
    },
    every: (d, f) => add(now + Math.max(1, d), Math.max(1, d), f),
    after: (d, f) => add(now + Math.max(0, d), 0, f),
    sleep: (_c, d) => new Promise((resolve) => add(now + Math.max(0, d), 0, resolve)),
    /**
     * Moves virtual time forward by `d` milliseconds, firing every timer due
     * on the way in due order (ties in creation order) and flushing effects
     * after each.
     * @param {number} d milliseconds; negative counts as 0
     */
    advance(d) {
      const target = now + Math.max(0, d);
      for (;;) {
        let next = null;
        for (const t of timers) {
          if (t.live && t.due <= target && (next === null || t.due < next.due || (t.due === next.due && t.seq < next.seq))) next = t;
        }
        if (next === null) break;
        now = next.due;
        if (next.every > 0) next.due += next.every;
        else next.live = false;
        next.f();
        flush();
      }
      now = target;
      flush();
    },
    /**
     * Sets the virtual time to `instant` without firing timers.
     * @param {{ms: number}} instant
     */
    set(instant) {
      now = instant.ms;
    },
  };
}

/** `k.test.MemoryStorage`: `LocalStorage` in memory, able to fail. */
export function memoryStorage(R, W) {
  const items = new Map();
  let readError = null;
  let writeError = null;
  return {
    /**
     * Reads and decodes the value under `key`; `null` when none is stored.
     * @throws {unknown} the error given to `failReads`, or
     *   `StorageReadError.unreadable`
     */
    read(codec, key) {
      if (readError !== null) throw readError;
      const text = items.get(key.name);
      if (text === undefined) return null;
      try {
        return codec.decode(JSON.parse(text), "");
      } catch (e) {
        if (isPanic(e)) throw e;
        throw R.unreadable;
      }
    },
    /**
     * Encodes and stores `value` under `key`.
     * @throws {unknown} the error given to `failWrites`
     */
    write(codec, key, value) {
      if (writeError !== null) throw writeError;
      items.set(key.name, JSON.stringify(codec.encode(value)));
    },
    /** Removes the value under the key, the last argument. */
    remove(...a) {
      items.delete(a[a.length - 1].name);
    },
    /** Stores `text` as is under `name`, to test reading unreadable data. */
    writeRaw(name, text) {
      items.set(String(name), String(text));
    },
    /** Makes every later read throw `error`. */
    failReads(error) {
      readError = error;
    },
    /** Makes every later write throw `error`. */
    failWrites(error) {
      writeError = error;
    },
    /** Makes reads and writes succeed again. */
    stopFailing() {
      readError = null;
      writeError = null;
    },
  };
}

/**
 * `k.test.MemoryCollections`: `Collections` in memory
 * (52-indexed-collections.md, "Tests"), empty for every test. Records are
 * `runtime/collections.js`' (`{k, i, v}`, the value's JSON text in `v`),
 * ordered as IndexedDB orders keys: numbers before text, text by UTF-16
 * code units. `StorageReadError` and `StorageWriteError` are
 * `unitCase(0)` `unavailable` and `unitCase(1)` `unreadable` or
 * `quotaExceeded`; `StoragePersistence` `unitCase(0)` `persistent`,
 * `unitCase(1)` `bestEffort`.
 * @param {object} schema the program's collections, as the browser's
 */
export function memoryCollections(schema) {
  const stores = new Map();
  let readError = null;
  let writeError = null;
  let persistence = unitCase(1);
  let estimate = null;
  const cmp = (a, b) => (typeof a !== typeof b ? (typeof a === "number" ? -1 : 1) : a < b ? -1 : a > b ? 1 : 0);
  const store = (n) => {
    let s = stores.get(n);
    if (!s) stores.set(n, (s = new Map()));
    return s;
  };
  const id = (k) => `${typeof k}:${k}`;
  const records = (n) => [...store(n).values()].sort((x, y) => cmp(x.k, y.k));
  const reading = () => {
    if (readError !== null) throw readError;
  };
  const writing = () => {
    if (writeError !== null) throw writeError;
  };
  const record = (n, cv, value) => collectionRecord(schema[n], JSON.stringify(cv.encode(value)));
  const keyOf = (n, ck, key) => collectionKey(schema[n][1], ck.encode(key));
  const at = (index) => {
    const found = collectionIndex(schema, index);
    if (found) return found;
    throw new KPanic(`the index \`${index.collection}.${index.field}\` isn't declared as a constant or a field, so MemoryCollections doesn't know it`);
  };
  const indexed = (index, keep) => {
    const [s, f] = at(index);
    return records(s)
      .filter((r) => r.i[f] !== undefined && keep(r.i[f]))
      .sort((x, y) => cmp(x.i[f], y.i[f]) || cmp(x.k, y.k));
  };
  return {
    async get(_c, ck, cv, key, c) {
      reading();
      const r = store(c.name).get(id(keyOf(c.name, ck, key)));
      return r ? collectionValues(cv, [r], unitCase(1))[0] : null;
    },
    async all(_c, _ck, cv, c) {
      reading();
      return collectionValues(cv, records(c.name), unitCase(1));
    },
    async count(_c, _ck, _cv, c) {
      reading();
      return store(c.name).size;
    },
    async find(_c, cf, cv, value, index) {
      reading();
      const k = collectionKey(at(index)[2], cf.encode(value));
      return collectionValues(cv, indexed(index, (x) => cmp(x, k) === 0), unitCase(1));
    },
    async range(_c, cf, cv, index, from, to, reversed, limit) {
      reading();
      const kind = at(index)[2];
      const lo = from == null ? undefined : collectionKey(kind, cf.encode(from));
      const hi = to == null ? undefined : collectionKey(kind, cf.encode(to));
      let out = indexed(index, (x) => (lo === undefined || cmp(x, lo) >= 0) && (hi === undefined || cmp(x, hi) <= 0));
      if (reversed) out.reverse();
      if (limit != null) out = out.slice(0, Math.max(0, limit));
      return collectionValues(cv, out, unitCase(1));
    },
    async put(_c, _ck, cv, value, c) {
      writing();
      const r = record(c.name, cv, value);
      store(c.name).set(id(r.k), r);
    },
    async putAll(_c, _ck, cv, values, c) {
      writing();
      for (const r of values.map((v) => record(c.name, cv, v))) store(c.name).set(id(r.k), r);
    },
    async delete(_c, ck, _cv, key, c) {
      writing();
      store(c.name).delete(id(keyOf(c.name, ck, key)));
    },
    async clear(_c, _ck, _cv, c) {
      writing();
      store(c.name).clear();
    },
    async persistence(_c) {
      return persistence;
    },
    async requestPersistence(_c) {
      return persistence;
    },
    async estimate(_c) {
      return estimate;
    },
    writeRaw(collection, text) {
      const n = String(collection);
      const r = schema[n] && collectionRecord(schema[n], String(text));
      if (!r) throw new KPanic(`writeRaw: the text for \`${n}\` needs a collection and a key field`);
      store(n).set(id(r.k), r);
    },
    failReads(error) {
      readError = error;
    },
    failWrites(error) {
      writeError = error;
    },
    stopFailing() {
      readError = null;
      writeError = null;
    },
    setPersistence(value) {
      persistence = value;
    },
    setEstimate(value) {
      estimate = value;
    },
  };
}

/**
 * `k.test.MemoryKeyStore`: `KeyStore` in memory (33-crypto.md, "Testing"),
 * empty for every test. Keys are kept as they are (the ops backend's
 * values); `failAll` makes every `get`, `put` and `delete` throw its
 * error until `stopFailing`. `requestPersistence` answers `persistence`,
 * `.persistent` (`unitCase(0)`) unless set.
 */
export function memoryKeyStore() {
  const keys = new Map();
  let error = null;
  let persistence = unitCase(0);
  const check = () => {
    if (error !== null) throw error;
  };
  /** The name of a `KeyName`, the call's last argument. */
  const nameOf = (a) => String(a[a.length - 1].name);
  return {
    get names() {
      return [...keys.keys()].sort();
    },
    get persistence() {
      return persistence;
    },
    async get(...a) {
      check();
      return keys.get(nameOf(a)) ?? null;
    },
    async put(_c, key, name) {
      check();
      keys.set(String(name.name), key instanceof Secret ? key.reveal() : key);
    },
    async delete(...a) {
      check();
      keys.delete(nameOf(a));
    },
    async requestPersistence(_c) {
      return persistence;
    },
    failAll(e) {
      error = e;
    },
    stopFailing() {
      error = null;
    },
    setPersistence(p) {
      persistence = p;
    },
  };
}

/**
 * `k.test.TestFiles`: `Files` without a picker or downloads
 * (42-files.md, "Tests"). `open` and `openAll` answer the files the test
 * chose (`choose`), else throw `.cancelled`; `failOpen` makes the next one
 * throw instead. `save` records the file in `saved`, unless `failSaves`
 * set an error. The errors are the enums' cases (`unitCase`).
 */
export function testFiles() {
  let chosen = [];
  let openError = null;
  let saveError = null;
  const saved = [];
  const requests = [];
  const take = (types, all) => {
    requests.push(types);
    if (openError !== null) {
      const e = openError;
      openError = null;
      throw e;
    }
    if (chosen.length === 0) throw unitCase(0);
    const files = all ? chosen : chosen.slice(0, 1);
    chosen = all ? [] : chosen.slice(1);
    return files;
  };
  return {
    /** A copy of the files saved so far, oldest first. */
    get saved() {
      return [...saved];
    },
    /** A copy of the file types of each `open` and `openAll`, oldest first. */
    get requests() {
      return [...requests];
    },
    open: async (_c, types) => take(types, false)[0],
    openAll: async (_c, types) => take(types, true),
    /** Records `file`, or throws the error given to `failSaves`. */
    save(file) {
      if (saveError !== null) throw saveError;
      saved.push(file);
    },
    /**
     * Queues `file` as what the user picks in the next `open` (or the next
     * `openAll`, with every queued file).
     */
    choose(file) {
      chosen.push(file);
    },
    /** Makes the next `open` or `openAll` throw `error`. */
    failOpen(error) {
      openError = error;
    },
    /** Makes every later `save` throw `error`. */
    failSaves(error) {
      saveError = error;
    },
  };
}

/**
 * `k.test.TestClipboard`: a clipboard in memory (45-web-platform.md,
 * "Clipboard"), empty for every test. `write` and `readText` count in
 * `calls`, throw the error given to `failWrites`/`failReads`, and throw
 * `.unsupported` (`unitCase(0)`) after `setSupported(false)`. `set` puts
 * text there as another application would.
 */
export function testClipboard() {
  let text = null;
  let supported = true;
  let calls = 0;
  let writeError = null;
  let readError = null;
  const check = (error) => {
    calls++;
    if (!supported) throw unitCase(0);
    if (error !== null) throw error;
  };
  return {
    get text() {
      return text;
    },
    get isSupported() {
      return supported;
    },
    get calls() {
      return calls;
    },
    async write(_c, value) {
      check(writeError);
      text = String(value);
    },
    async readText(_c) {
      check(readError);
      return text === "" ? null : text;
    },
    /** Puts `value` on the clipboard, as if another application copied it. */
    set(value) {
      text = String(value);
    },
    failWrites(error) {
      writeError = error;
    },
    failReads(error) {
      readError = error;
    },
    setSupported(value) {
      supported = value === true;
    },
  };
}

/**
 * `k.test.TestShare`: no share sheet (45-web-platform.md, "Web Share").
 * `share` records the content in `shared`, unless `cancelNext` (`.cancelled`,
 * `unitCase(1)`) or `failNext` set what the next one throws, or
 * `setSupported(false)` made every share `.unsupported` (`unitCase(0)`).
 */
export function testShare() {
  const shared = [];
  let supported = true;
  let next = null;
  return {
    /** A copy of the contents shared so far, oldest first. */
    get shared() {
      return [...shared];
    },
    get isSupported() {
      return supported;
    },
    async share(_c, content) {
      if (!supported) throw unitCase(0);
      const e = next;
      next = null;
      if (e !== null) throw e;
      shared.push(content);
    },
    cancelNext() {
      next = unitCase(1);
    },
    failNext(error) {
      next = error;
    },
    setSupported(value) {
      supported = value === true;
    },
  };
}

/**
 * `k.test.TestGeolocation`: a device that hasn't moved
 * (45-web-platform.md, "Geolocation"). Supported, `permission` `.prompt`
 * (`unitCase(0)`; reactive, like the browser's), prompts answered
 * `.granted` (`1`), and no fix: `currentPosition` throws `.unavailable`
 * (`LocationError` `3`) until `move`. `watch` streams start with the
 * current position and end when the permission stops being granted or
 * their owner is disposed.
 */
export function testGeolocation() {
  const permission = signal(unitCase(0));
  let supported = true;
  let answer = unitCase(1);
  let prompts = 0;
  let accuracy = null;
  let at = null;
  let next = null;
  const watches = new Set();
  const setPermission = (p) => {
    permission.set(p);
    if (p.$ !== 1) for (const s of [...watches]) s.close();
  };
  /** Throws `.unsupported`, or `.notAsked`/`.denied` unless granted. */
  const check = (p) => {
    if (!supported) throw unitCase(0);
    if (p.$ !== 1) throw unitCase(p.$ === 0 ? 1 : 2);
  };
  return {
    get isSupported() {
      return supported;
    },
    get permission() {
      return permission.get();
    },
    get prompts() {
      return prompts;
    },
    get lastAccuracy() {
      return accuracy;
    },
    async requestAccess(_c) {
      if (!supported || permission.peek().$ !== 0) return permission.peek();
      prompts++;
      setPermission(answer);
      return answer;
    },
    async currentPosition(_c, a) {
      if (!supported) throw unitCase(0);
      accuracy = a ?? unitCase(0);
      const e = next;
      next = null;
      if (e !== null) throw e;
      check(permission.peek());
      if (at === null) throw unitCase(3);
      return at;
    },
    watch(a) {
      check(permission.get());
      accuracy = a ?? unitCase(0);
      const stream = new PushStream();
      stream.close = () => {
        if (watches.delete(stream)) stream.push(null);
      };
      watches.add(stream);
      if (at !== null) stream.push(at);
      onCleanup(stream.close);
      return stream;
    },
    move(p) {
      at = p;
      for (const s of watches) s.push(p);
    },
    answerPrompts(p) {
      answer = p;
    },
    setPermission,
    failNext(e) {
      next = e;
    },
    setSupported(value) {
      supported = value === true;
    },
  };
}

// ------------------------------------------------------------ media

/**
 * Ties a test stream to its owner, as the browser's `ownStream` does:
 * `.owner` stops it when `c` aborts (its component unmounts); `.service`
 * when a screen of the test unmounts, or the test ends.
 * @param {object} stream
 * @param {AbortSignal|null} c
 * @param {{$: number}|null} lifetime
 */
function ownTestStream(stream, c, lifetime) {
  if (c?.aborted) {
    stream.stop();
    return stream;
  }
  if (lifetime?.$ === 1) (current.media ??= new Set()).add(stream);
  else c?.addEventListener("abort", () => stream.stop());
  return stream;
}

/** Stops the test's `.service` streams (a screen unmounted, or the test ended). */
function stopTestMedia() {
  for (const s of current.media ?? []) s.stop();
  current.media?.clear();
}

/**
 * `k.test.TestSource`: the sound a test device makes on `clock`, as
 * segments `[from, to, level]` in milliseconds; the latest that covers
 * the time wins, and silence otherwise. `tracks` are the live tracks it
 * feeds.
 * @param {object} clock the `TestClock`
 */
function testSource(clock, make) {
  const segments = [];
  const tracks = new Set();
  const at = () => clock.now.ms;
  return {
    speak(level, d) {
      segments.push([at(), at() + d, Math.max(0, Math.min(1, level))]);
    },
    silence(d) {
      segments.push([at(), at() + d, 0]);
    },
    stopSharing() {
      batch(() => {
        for (const t of [...tracks]) if (t.$screen) t.$end(2);
      });
      flush();
    },
    newStream() {
      return make();
    },
    $levelAt(ms) {
      for (let i = segments.length - 1; i >= 0; i--) {
        const [from, to, level] = segments[i];
        if (ms >= from && ms < to) return level;
      }
      return 0;
    },
    $tracks: tracks,
  };
}

/** The test devices: `[id, kind tag, label]`. */
const TEST_DEVICES = [
  ["test-microphone", 0, "Test Microphone"],
  ["test-camera", 1, "Test Camera"],
  ["test-speaker", 2, "Test Speaker"],
];

/**
 * `k.test.TestMediaDevices`: no prompt, no device (46-media-and-rtc.md,
 * "Tests"). Supported; both permissions `.prompt` (reactive); prompts
 * answered `.granted`; one test microphone, camera and speaker, labelled
 * once a kind was granted. Captures return synthetic streams of
 * `mediaTrack` handles fed by the devices' `TestSource`s; `liveTracks`
 * are those not yet ended.
 */
export function testMediaDevices() {
  const clock = sharedClock();
  const perms = [signal(unitCase(0)), signal(unitCase(0))];
  const plugged = signal(TEST_DEVICES.map(([id, kind, label]) => ({ id, kind, label })));
  const labelled = signal(false);
  let supported = true;
  let answer = unitCase(1);
  let prompts = 0;
  let failure = null;
  let cancelScreen = false;
  const captures = [];
  const live = new Set();
  // A source's own stream (`newStream()`, what a `TestCallPeer` sends): the
  // device of another person, so no permission and not in `liveTracks`.
  const own = (kind) => () => {
    const [id, , label] = TEST_DEVICES[kind];
    const t = { kind: kind === 1 ? "video" : "audio", label, enabled: true, muted: false, stop() {} };
    const h = mediaTrack(t, { device: id });
    t.$handle = h;
    h.$source = kind === 1 ? sources.camera : sources.microphone;
    return mediaStream([h], null);
  };
  const sources = { microphone: testSource(clock, own(0)), camera: testSource(clock, own(1)), screen: testSource(clock, own(1)) };
  /** `[camera?, microphone?]` permission indexes of a request. */
  const kinds = (r) => (r.$ === 0 ? [1] : r.$ === 1 ? [0] : [0, 1]);
  const track = (kind, id, label, source, screen) => {
    const t = { kind: kind === 1 ? "video" : "audio", label, enabled: true, muted: false, stop() {} };
    const settings = kind === 1
      ? { size: { width: 1280, height: 720 }, frameRate: 30, sampleRate: null, channelCount: null, echoCancellation: null, noiseSuppression: null, autoGainControl: null }
      : { size: null, frameRate: null, sampleRate: 48000, channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true };
    const h = mediaTrack(t, { device: id, screen, settings });
    t.$handle = h;
    h.$source = source;
    h.$screen = screen;
    live.add(h);
    source.$tracks.add(h);
    h.$ends.add(() => {
      live.delete(h);
      source.$tracks.delete(h);
    });
    return h;
  };
  const device = (choice, kind) => {
    const id = choice?.value ?? TEST_DEVICES[kind][0];
    const d = plugged.peek().find((x) => x.id === id && x.kind === kind);
    if (!d) throw unitCase(4);
    return d;
  };
  const now = (names) => {
    const s = names.map((i) => perms[i].peek().$);
    return unitCase(s.includes(2) ? 2 : s.every((x) => x === 1) ? 1 : 0);
  };
  return {
    get isSupported() {
      return supported;
    },
    get cameraPermission() {
      return perms[0].get();
    },
    get microphonePermission() {
      return perms[1].get();
    },
    get devices() {
      const named = labelled.get();
      return plugged.get().map((d) => ({ id: { value: d.id }, kind: unitCase(d.kind), label: named ? d.label : null }));
    },
    get prompts() {
      return prompts;
    },
    get captures() {
      return [...captures];
    },
    get liveTracks() {
      return [...live];
    },
    get microphone() {
      return sources.microphone;
    },
    get camera() {
      return sources.camera;
    },
    async requestAccess(_c, request) {
      const names = kinds(request);
      if (!supported || now(names).$ !== 0) return now(names);
      prompts++;
      batch(() => {
        for (const i of names) if (perms[i].peek().$ === 0) perms[i].set(answer);
        if (answer.$ === 1) labelled.set(true);
      });
      return now(names);
    },
    async capture(c, request, lifetime) {
      captures.push(request);
      if (!supported) throw unitCase(0);
      if (failure !== null) throw failure;
      for (const i of kinds(request)) {
        const p = perms[i].peek().$;
        if (p !== 1) throw unitCase(p === 0 ? 1 : 2);
      }
      const audio = request.$ === 0 || request.$ === 2 ? request._0 : null;
      const video = request.$ === 1 ? request._0 : request.$ === 2 ? request._1 : null;
      const mic = audio && device(audio.device, 0);
      const cam = video && device(video.device, 1);
      const tracks = [];
      if (mic) tracks.push(track(0, mic.id, mic.label, sources.microphone, false));
      if (cam) tracks.push(track(1, cam.id, cam.label, sources.camera, false));
      return ownTestStream(mediaStream(tracks, null), c, lifetime);
    },
    async captureScreen(c, request, lifetime) {
      if (!supported) throw unitCase(0);
      if (cancelScreen) {
        cancelScreen = false;
        throw unitCase(3);
      }
      if (failure !== null) throw failure;
      const tracks = [track(1, null, "Test Screen", sources.camera, true)];
      if (request?.audio === true) tracks.push(track(0, null, "Test Screen Audio", sources.screen, true));
      return ownTestStream(mediaStream(tracks, null), c, lifetime);
    },
    setPermission(kind, state) {
      const i = kind.$ === 0 ? 0 : 1;
      batch(() => {
        perms[i].set(state);
        if (state.$ === 1) labelled.set(true);
        // A permission revoked while capturing ends the kind's tracks.
        if (state.$ !== 1) for (const t of [...live]) if (!t.$screen && t.kind.$ === (i === 0 ? 1 : 0)) t.$end(2);
      });
      flush();
    },
    answerPrompts(a) {
      answer = a;
    },
    failCaptures(e) {
      failure = e;
    },
    cancelScreenPicker() {
      cancelScreen = true;
    },
    add(d) {
      plugged.set([...plugged.peek(), { id: d.id.value, kind: d.kind.$, label: d.label ?? d.id.value }]);
      flush();
    },
    remove(id) {
      batch(() => {
        plugged.set(plugged.peek().filter((d) => d.id !== id.value));
        for (const t of [...live]) if (t.device?.value === id.value) t.$end(1);
      });
      flush();
    },
    setSupported(v) {
      supported = v === true;
    },
  };
}

/**
 * `k.test.TestRecorder`: records 100 bytes a second of a synthetic
 * stream while recording, on the test's clock; the file is those bytes
 * (`i % 251`), `audio/webm` or `video/webm` (or the `.exact` type).
 */
export function testRecorder() {
  const clock = sharedClock();
  let started = 0;
  let next = null;
  return {
    get started() {
      return started;
    },
    supports(format) {
      return format.$ !== 3 || next?.$ !== 0;
    },
    start(stream, format, limit) {
      const e = next;
      next = null;
      if (e !== null) throw e;
      if (recordedTracks(stream, format) === null) throw unitCase(1);
      started++;
      let total = 0;
      let session = null;
      const mediaType = format.$ === 3 ? format.mediaType : format.$ === 2 ? "video/webm" : "audio/webm";
      const rec = {
        start() {},
        pause() {},
        resume() {},
        stop: () => ({ bytes: Uint8Array.from({ length: total }, (_, i) => i % 251), mediaType }),
      };
      const data = rec;
      session = recordingSession(stream, format, limit, rec, () => clock.now.ms, (f) =>
        clock.every(1000, () => {
          if (session?.state.$ === 0) {
            total += 100;
            data.data(100);
          }
          f();
        }),
      );
      return session;
    },
    failNext(e) {
      next = e;
    },
  };
}

/**
 * `k.test.TestAudio`: a meter reads the level its stream's microphone's
 * `TestSource` scripts, sampled 30 times a second of the test's clock;
 * `play` records its source and resolves at once.
 */
export function testAudio() {
  const clock = sharedClock();
  const played = [];
  return {
    get played() {
      return [...played];
    },
    meter(stream, lifetime) {
      const t = stream.audioTracks[0];
      if (!t) throw unitCase(1);
      // The RMS whose level, on the meter's -60 dB scale, is the scripted one.
      const sample = () => {
        const l = t.$source?.$levelAt(clock.now.ms) ?? 0;
        return l <= 0 ? 0 : 10 ** ((l * 60 - 60) / 20);
      };
      const m = audioMeter(stream, sample, (n) => Array.from({ length: Math.max(0, n) }, () => 0), () => clock.now.ms, (f) => {
        const h = clock.every(34, f);
        return () => h.cancel();
      });
      if (lifetime?.$ !== 1) onCleanup(m.stop);
      return m;
    },
    async play(_c, source) {
      played.push(source);
    },
  };
}

// -------------------------------------------------------------- calls

/**
 * `k.test.TestPeerConnections`, one per test (46-media-and-rtc.md,
 * "Loopback peers"): `runtime/rtc.js`'s real negotiation over the app's
 * real channel and relay, with the in-process engine of `testing/rtc.js`
 * and timers on the test's clock. Every data channel a connection opens
 * is opened by the test's call peers too, so `received(kind)` holds what
 * the app sent even when the test never opened the channel itself.
 */
export function testPeerConnections() {
  if (current.rtc) return current.rtc;
  const clock = sharedClock();
  const engine = fakeRtc();
  const conns = new Set();
  const peers = new Set();
  const env = {
    RTCPeerConnection: engine.RTCPeerConnection,
    after: (ms, f) => clock.after(ms, () => (f(), flush())),
    cancel: (h) => h?.cancel?.(),
    log: true,
    opened(conn, kind) {
      conns.add(conn);
      for (const p of peers) if (p !== conn) p.handle.channel(kind);
    },
  };
  const pcs = peerConnections(env);
  const net = [() => engine.lost(), () => engine.restored(), () => engine.iceFails(), () => engine.glare()];
  return (current.rtc = {
    get isSupported() {
      return true;
    },
    async connect(c, signalling, servers, policy, lifetime) {
      const h = await pcs.connect(c, signalling, servers, policy, lifetime);
      conns.add(h.$conn);
      for (const k of h.$conn.channels.values()) for (const p of peers) p.handle.channel(k.kind);
      return h;
    },
    async peer(c, signalling) {
      const h = await pcs.connect(null, signalling, [], unitCase(0), unitCase(0));
      const conn = h.$conn;
      peers.add(conn);
      for (const other of conns) for (const d of other.channels.values()) h.channel(d.kind);
      return testCallPeer(h);
    },
    simulate(e) {
      net[e.$]();
      flush();
    },
    $reset() {
      for (const c of [...conns, ...peers]) c.end(0);
      engine.reset();
    },
  });
}

/**
 * `k.test.TestCallPeer` over a connection handle `h`: `remote` is the
 * app's own track handles (`$handle` of the tracks it sent), so a test
 * sees the app's mute.
 * @param {object} h
 */
function testCallPeer(h) {
  const conn = h.$conn;
  return {
    get state() {
      return h.state;
    },
    get remote() {
      const stream = h.remote;
      if (!stream) return null;
      return mediaStream(
        stream.$tracks.map((t) => t.$t.$src?.$handle ?? t),
        null,
        false,
      );
    },
    send: (stream) => h.send(stream),
    close: () => h.close(),
    channel: (kind) => h.channel(kind),
    received(kind) {
      h.channel(kind);
      return [...(conn.channels.get(kind.name)?.log ?? [])];
    },
  };
}

/**
 * `k.test.TestNotifications`: nothing reaches the operating system
 * (45-web-platform.md, "Notifications"). Supported, `permission` `.prompt`
 * (`unitCase(0)`; reactive, like the browser's), prompts answered
 * `.granted` (`1`). `show` records each notification in `shown`, or
 * throws `NotificationError` `0` (`unsupported`), `1` (`notAsked`) or `2`
 * (`denied`) as the browser would. `click` plays the user clicking a shown
 * notification: the router opens its `opens` page (`href` is the app's
 * `Page$href`), and a notification without one leaves the tab as it is.
 * @param {((page: object) => string)|null} href
 */
export function testNotifications(href) {
  const permission = signal(unitCase(0));
  // Reactive like `permission`, so a `@Derived` that read it follows
  // `setSupported` (a browser never changes it while the page is open).
  const supported = signal(true);
  let answer = unitCase(1);
  let prompts = 0;
  const shown = [];
  return {
    get isSupported() {
      return supported.get();
    },
    get permission() {
      return permission.get();
    },
    get prompts() {
      return prompts;
    },
    /** A copy of the notifications shown so far, oldest first. */
    get shown() {
      return [...shown];
    },
    async requestAccess(_c) {
      if (!supported.peek() || permission.peek().$ !== 0) return permission.peek();
      prompts++;
      permission.set(answer);
      return answer;
    },
    async show(_c, n) {
      if (!supported.peek()) throw unitCase(0);
      const p = permission.peek().$;
      if (p !== 1) throw unitCase(p === 0 ? 1 : 2);
      shown.push(n);
      return undefined;
    },
    answerPrompts(p) {
      answer = p;
    },
    setPermission(p) {
      permission.set(p);
      flush();
    },
    setSupported(value) {
      supported.set(value === true);
      flush();
    },
    click(n) {
      if (!shown.some((s) => equals(s, n))) {
        throw new KPanic("TestNotifications.click: that notification was never shown");
      }
      if (n.opens != null && href) navigate(href(n.opens));
      flush();
    },
  };
}

/**
 * `k.test.TestPushSubscriptions`: no prompt opens and no push service is
 * contacted (docs/design/50-push.md, "Tests"). Supported, `permission`
 * `.prompt` (reactive), prompts answered `.granted`. `subscribe` throws
 * `PushError` `0` (`unsupported`), `1` (`notAsked`), `2` (`denied`) as the
 * browser would, or what `failNextSubscribe` set; otherwise it keeps the
 * subscription it has for the same key, and makes a new one (endpoint
 * `https://push.test/subscriptions/<n>`, RFC 8291's example keys) for a
 * first or another key.
 */
export function testPushSubscriptions() {
  const permission = signal(unitCase(0));
  const supported = signal(true);
  let answer = unitCase(1);
  let prompts = 0;
  let made = 0;
  let subscription = null;
  let key = null;
  let failure = null;
  return {
    get isSupported() {
      return supported.get();
    },
    get permission() {
      return permission.get();
    },
    get prompts() {
      return prompts;
    },
    get subscription() {
      return subscription;
    },
    get serverKey() {
      return key;
    },
    async requestAccess(_c) {
      if (!supported.peek() || permission.peek().$ !== 0) return permission.peek();
      prompts++;
      permission.set(answer);
      return answer;
    },
    async subscribe(_c, serverKey) {
      if (!supported.peek()) throw unitCase(0);
      const p = permission.peek().$;
      if (p !== 1) throw unitCase(p === 0 ? 1 : 2);
      if (failure !== null) {
        const e = failure;
        failure = null;
        throw e;
      }
      if (subscription === null || key !== serverKey) {
        made++;
        subscription = {
          endpoint: `https://push.test/subscriptions/${made}`,
          p256dh: "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
          auth: "BTBZMqHH6r4Tts7J_aSIgg",
        };
      }
      key = serverKey;
      return subscription;
    },
    async current(_c) {
      return subscription;
    },
    async unsubscribe(_c) {
      subscription = null;
    },
    answerPrompts(p) {
      answer = p;
    },
    setPermission(p) {
      permission.set(p);
      flush();
    },
    setSupported(value) {
      supported.set(value === true);
      flush();
    },
    failNextSubscribe(error) {
      failure = error;
    },
  };
}

/**
 * `k.test.TestAuthenticator`: a working authenticator for `Passkeys`
 * (docs/design/51-passkeys.md, "Tests"). The CBOR, the keys and the
 * signatures are `k_server::passkey`'s test ceremonies
 * (`op_k_passkey_test_register` and `_assert`), so the server's real
 * verifier checks what this produces. It holds `{id, algorithm, key,
 * userHandle, rpId}` per passkey (key `n` is the `n`th passkey made), its
 * counter starts at 0 and counts each assertion, and its client data names
 * the public origin unless `setOrigin` says otherwise. `PasskeyAlgorithm`
 * `0` is `es256`, `1` `edDSA`, `2` `rs256`; `PasskeyError` `0`
 * `unsupported`, `1` `cancelled`, `2` `alreadyRegistered`.
 */
export function testAuthenticator() {
  const ops = globalThis.Deno?.core?.ops;
  const ALGORITHMS = ["es256", "edDSA", "rs256"];
  let supported = true;
  let algorithm = 0;
  let dismiss = false;
  let verified = true;
  let origin = null;
  let count = 0;
  let prompts = 0;
  let made = 0;
  let held = [];
  /** One ceremony: its JSON result, or a panic naming what failed. */
  const ceremony = (op, input) => {
    if (!ops?.[op]) throw new KPanic("TestAuthenticator: no passkey ops here");
    const out = ops[op](JSON.stringify({ ...input, origin: origin ?? serverInfo().publicOrigin.replace(new RegExp("/+$"), ""), userVerified: verified }));
    if (out === "") throw new KPanic("TestAuthenticator: the ceremony's input was refused");
    return JSON.parse(out);
  };
  /** Opens the sheet: `.unsupported`, or `.cancelled` once after `dismissNext`. */
  const sheet = () => {
    if (!supported) throw unitCase(0);
    prompts++;
    if (dismiss) {
      dismiss = false;
      throw unitCase(1);
    }
  };
  return {
    get isSupported() {
      return supported;
    },
    get passkeys() {
      return held.map((p) => p.id);
    },
    get prompts() {
      return prompts;
    },
    async register(_c, o, _name) {
      sheet();
      const excluded = new Set(o.excluded ?? []);
      if (held.some((p) => p.rpId === o.rpId && excluded.has(p.id))) throw unitCase(2);
      const key = ++made;
      const out = ceremony("op_k_passkey_test_register", { algorithm: ALGORITHMS[algorithm], key, rpId: o.rpId, challenge: o.challenge, signCount: 0 });
      // A device keeps one passkey per site and user handle.
      held = held.filter((p) => !(p.rpId === o.rpId && p.userHandle === o.userHandle));
      held.push({ id: out.credentialId, algorithm: ALGORITHMS[algorithm], key, userHandle: o.userHandle, rpId: o.rpId });
      return { credentialId: out.credentialId, clientDataJSON: out.clientDataJSON, attestationObject: out.attestationObject };
    },
    async signIn(_c, o) {
      sheet();
      const allowed = o.allowed ?? [];
      const usable = held.filter((p) => p.rpId === o.rpId && (allowed.length === 0 || allowed.includes(p.id)));
      const p = usable[usable.length - 1];
      if (!p) throw unitCase(1);
      count++;
      const out = ceremony("op_k_passkey_test_assert", { algorithm: p.algorithm, key: p.key, rpId: o.rpId, challenge: o.challenge, signCount: count });
      return { credentialId: p.id, clientDataJSON: out.clientDataJSON, authenticatorData: out.authenticatorData, signature: out.signature, userHandle: p.userHandle };
    },
    useAlgorithm(a) {
      algorithm = a.$;
    },
    dismissNext() {
      dismiss = true;
    },
    setUserVerified(value) {
      verified = value === true;
    },
    setOrigin(o) {
      origin = String(o);
    },
    setSignCount(n) {
      // The next assertion reports `n`.
      count = n - 1;
    },
    forgetPasskeys() {
      held = [];
    },
    setSupported(value) {
      supported = value === true;
    },
  };
}

/**
 * `k.test.TestTabMessaging`: a tab with no other tabs (45-web-platform.md,
 * "Messages between tabs"). `post` records the message's JSON per channel
 * name, which `posted` decodes again; `deliver` plays another tab posting:
 * the message's JSON text, decoded for every listener and stream of the
 * channel, then a flush. Every member takes the message type's codec first.
 */
export function testTabMessaging() {
  const posted = new Map();
  const subs = new Map();
  const listeners = (c) => {
    if (!subs.has(c.name)) subs.set(c.name, new Set());
    return subs.get(c.name);
  };
  const sub = (codec, c, f) => {
    const set = listeners(c);
    const g = (text) => f(codec.decode(JSON.parse(text), ""));
    set.add(g);
    return () => set.delete(g);
  };
  return {
    post(codec, c, message) {
      if (!posted.has(c.name)) posted.set(c.name, []);
      posted.get(c.name).push(stringify(codec.encode(message)));
    },
    listen(codec, c, handler) {
      return { cancel: sub(codec, c, handler) };
    },
    messages(codec, c) {
      const stream = new PushStream();
      const off = sub(codec, c, (v) => stream.push(v));
      stream.close = () => off() && stream.push(null);
      onCleanup(stream.close);
      return stream;
    },
    deliver(codec, c, message) {
      const text = stringify(codec.encode(message));
      batch(() => {
        for (const f of [...listeners(c)]) f(text);
      });
      flush();
    },
    posted(codec, c) {
      return (posted.get(c.name) ?? []).map((text) => codec.decode(JSON.parse(text), ""));
    },
  };
}

/**
 * `k.test.TestWakeLock`: the screen's wake lock in `k test`
 * (45-web-platform.md, "KeepScreenAwake"). `request` answers at once with
 * a sentinel (`release()`, `onrelease`), or throws the case number of the
 * failure `refuse` set; `isHeld` while a sentinel isn't released.
 */
export function testWakeLock() {
  let held = 0;
  let refusal = null;
  return {
    get isHeld() {
      return held > 0;
    },
    refuse(failure) {
      refusal = failure.$;
    },
    request() {
      if (refusal !== null) throw refusal;
      held++;
      const s = {
        onrelease: null,
        release() {
          if (s.released) return;
          s.released = true;
          held--;
          s.onrelease?.();
        },
      };
      return s;
    },
  };
}

/**
 * `k.test.TestPageVisibility` (45-web-platform.md, "Page visibility"): the
 * page is visible until `hide()`. Each change flushes, so the test reads
 * the updated view at once.
 */
export function testPageVisibility() {
  const visible = signal(true);
  const set = (v) => (visible.set(v), flush());
  return {
    get isVisible() {
      return visible.get();
    },
    hide: () => set(false),
    show: () => set(true),
  };
}

/**
 * `k.test.TestConnectivity` (45-web-platform.md, "Connectivity"): online
 * until `goOffline()`, which also makes the browser's requests fail as
 * without a network (so its rpcs throw `rpcFailure(.offline)`) until
 * `goOnline()`.
 */
export function testConnectivity() {
  const online = signal(true);
  const set = (v) => {
    const was = !current.offline;
    current.offline = !v;
    online.set(v);
    // Offline drops the channels' socket (it resumes once back online).
    if (was && !v) current.front?.drop(MAIN, true);
    if (!was && v) globalThis.dispatchEvent?.(new globalThis.Event("online"));
    flush();
  };
  return {
    get isOnline() {
      return online.get();
    },
    goOffline: () => set(false),
    goOnline: () => set(true),
  };
}

/** A copy of an encoded message, as structured clone makes one between threads. */
const cloned = (v) =>
  v instanceof Uint8Array
    ? v.slice()
    : Array.isArray(v)
      ? v.map(cloned)
      : v !== null && typeof v === "object"
        ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, cloned(x)]))
        : v;

/**
 * A `@Worker` service in `k test` (55-workers.md, "Tests"): the opener `workerClient` calls, giving
 * a worker whose host (`serveWorker`, the worker chunk's) runs in this thread. Each message crosses
 * in a microtask, copied; `terminate()` drops every message after it, so a cancelled call's answer
 * is never seen, as in the browser.
 *
 * @param {() => any} make builds the service
 * @param {Record<string, [Function, Function, Function]>} host its methods' codecs
 */
export function inlineWorker(make, host) {
  return () => {
    let live = true;
    const scope = {
      onmessage: null,
      postMessage: (data) => queueMicrotask(() => live && port.onmessage?.({ data: cloned(data) })),
    };
    const port = {
      onmessage: null,
      onerror: null,
      onmessageerror: null,
      postMessage: (data) => queueMicrotask(() => live && scope.onmessage?.({ data: cloned(data) })),
      terminate: () => {
        live = false;
      },
    };
    serveWorker(scope, make, host);
    return port;
  };
}

/**
 * `k.test.TestAppUpdates` (54-offline.md, "Tests"): no newer build until
 * `announce()`; `apply()` only counts, in `applied`. Each change flushes,
 * so the test reads the updated view at once.
 */
export function testAppUpdates() {
  const available = signal(false);
  const applied = signal(0);
  return {
    get isAvailable() {
      return available.get();
    },
    get applied() {
      return applied.get();
    },
    announce: () => (available.set(true), flush()),
    apply: () => (applied.set(applied.peek() + 1), flush()),
  };
}

/**
 * `k.test.TestPreferences` (45-web-platform.md, "Preferences"): every
 * preference `false` until the test sets it (`set(prefersDark:)` is
 * `set$prefersDark`).
 */
export function testPreferences() {
  const t = {};
  for (const name of ["prefersDark", "prefersReducedMotion", "prefersMoreContrast", "forcedColors"]) {
    const s = signal(false);
    Object.defineProperty(t, name, { get: () => s.get(), enumerable: true });
    t[`set$${name}`] = (v) => (s.set(v === true), flush());
  }
  return t;
}

/**
 * `k.test.TestViewport` (45-web-platform.md, "Viewport"): 1280×800, a
 * fine pointer (`PointerKind` `unitCase(0)`) that can hover, until the test
 * changes one.
 */
export function testViewport() {
  const size = signal({ width: "1280px", height: "800px" });
  const pointer = signal(unitCase(0));
  const hover = signal(true);
  const set = (s, v) => (s.set(v), flush());
  return {
    get size() {
      return size.get();
    },
    get pointer() {
      return pointer.get();
    },
    get canHover() {
      return hover.get();
    },
    resize: (width, height) => set(size, { width, height }),
    set$pointer: (p) => set(pointer, p),
    set$canHover: (v) => set(hover, v === true),
  };
}

/**
 * `Position.sample(latitude:longitude:)`: a position with 1 km accuracy
 * and nothing else known, measured at the test's clock.
 * @param {number} latitude
 * @param {number} longitude
 */
function positionSample(latitude, longitude) {
  return {
    latitude,
    longitude,
    accuracyMeters: 1000,
    altitudeMeters: null,
    headingDegrees: null,
    speedMetersPerSecond: null,
    measuredAt: sharedClock().now,
  };
}

/** The path of a URL, without its query. */
function pathOf(url) {
  const text = String(url ?? "");
  const scheme = text.indexOf("://");
  const rest = scheme < 0 ? text : text.slice(text.indexOf("/", scheme + 3) < 0 ? text.length : text.indexOf("/", scheme + 3));
  const q = rest.search(new RegExp("[?#]"));
  return (q < 0 ? rest : rest.slice(0, q)) || "/";
}

/**
 * `k.test.TestHttpClient`: canned responses by path. A response given with
 * `respond(path:with:)` is returned as built; `serveJson` and `serveText`
 * answer like a received response (`received`), so the code under test
 * reads them as it reads the network's.
 * @param {object} E the generated `HttpError` enum
 * @param {object} S the generated `HttpStatus` enum
 */
export function testHttpClient(E, S) {
  const answers = new Map();
  const requests = [];
  const ok = S && S.ok !== undefined ? S.ok : { $: 0 };
  const answer = (status, type, bytes) => ({ response: received(E, status, httpCode(status), [["content-type", type]], bytes) });
  return {
    requests,
    /**
     * Records `request` and answers what is canned for its path.
     * @throws {KPanic} when nothing is canned for the path
     */
    async send(_c, request) {
      requests.push(request);
      const a = answers.get(pathOf(request.url));
      if (a === undefined) throw new KPanic(`TestHttpClient has no response for ${request.url}; give it one with respond(path:with:)`);
      if (a.error !== undefined) throw a.error;
      return a.response;
    },
    /**
     * Sends `request` and decodes a successful response's JSON with `codec`.
     * @throws {object} `HttpError.status` for a non-2xx response
     */
    async fetchJson(c, codec, request) {
      const r = await this.send(c, request);
      if (!HttpResponse$isSuccess(r)) throw E.status(r.status, HttpResponse$text(r));
      return HttpResponse$json(r, codec, E);
    },
    /** Cans `response`, returned as built, for requests to `path`. */
    respond(path, response) {
      answers.set(String(path), { response });
    },
    /** Cans a 200 JSON response holding `value` for requests to `path`. */
    serveJson(codec, path, value) {
      answers.set(String(path), answer(ok, "application/json", new TextEncoder().encode(JSON.stringify(codec.encode(value)))));
    },
    /**
     * Cans a plain-text response for requests to `path`, `ok` unless
     * `status` is given.
     */
    serveText(path, text, status) {
      answers.set(String(path), answer(status ?? ok, "text/plain; charset=utf-8", new TextEncoder().encode(String(text))));
    },
    /** Makes requests to `path` throw `error`. */
    fail(path, error) {
      answers.set(String(path), { error });
    },
    E,
  };
}

/** `k.test.RecordingJobQueue`: stores jobs without running them. */
export function recordingJobQueue() {
  const jobs = [];
  return {
    /**
     * Records the job without running it: its payload JSON-encoded with
     * `codec`, as the worker would receive it, or as it is when the caller
     * has no codec (`null`, which `InlineJobQueue` accepts too).
     */
    async enqueue(_c, codec, kind, payload, delay) {
      const json = codec ? JSON.stringify(codec.encode(payload)) : null;
      jobs.push({ name: String(kind.name), json, payload, delay: delay ?? 0 });
    },
    /**
     * The payloads of the jobs of `kind`, in enqueue order: decoded with
     * `codec`, or as enqueued when either side had no codec.
     */
    payloads(codec, kind) {
      return jobs
        .filter((j) => j.name === String(kind.name))
        .map((j) => (codec && j.json !== null ? codec.decode(JSON.parse(j.json), "") : j.payload));
    },
  };
}

/** `k.test.RecordingEmailProvider`: keeps sent messages. */
export function recordingEmailProvider() {
  const sent = [];
  let error = null;
  return {
    sent,
    /** Records `email`, or throws the simulated failure. */
    async send(_c, email) {
      if (error !== null) throw error;
      sent.push(email);
    },
    /** Makes every later `send` throw `e`. */
    simulateFailure(e) {
      error = e;
    },
  };
}

// ------------------------------------------------------------- channels

/**
 * The test's own browser for the channels (`./channels.js`): its requests
 * are the test's (`current.context`, the test's one session), its mirror
 * the runtime's, and it is offline after `TestConnectivity.goOffline()`.
 */
const MAIN = {
  get ctx() {
    return current.context;
  },
  mirror: sessionMirror,
  get offline() {
    return current.offline;
  },
};

/**
 * The `TestPeer` whose protocol client is calling now, so the rpc or
 * channel it starts runs in that browser; `null` for the test's own. The
 * stubs start their request synchronously, so a call sets it only while it
 * runs.
 * @type {object|null}
 */
let active = null;

/** A socket that can't open: a test without a server has no channels. */
function noSocket() {
  throw new Error("this test has no server");
}

/**
 * Points the channels' client at the test: its sockets at an in-process
 * front over the app's handler (`context`, the test's `TestContext`, when
 * it has a server), its timers at the `TestClock`, and its browsers at the
 * test's own and its peers.
 * @param {object} clock
 * @param {object|null} context
 */
function installChannels(clock, context) {
  current.context = context;
  current.front = context === null ? null : createFront(clock, track);
  const ops = globalThis.Deno?.core?.ops;
  if (context !== null && ops && !ops.$kChannels) {
    // The app's channel runtime sends through the front instead of Rust's.
    try {
      ops.op_k_channel_send = (id, json) => current.front?.send(id, json) ?? false;
      ops.op_k_channel_close = (id, reason) => current.front?.close(id, reason);
      ops.$kChannels = true;
    } catch {
      // A read-only op table: channels then can't reach the browser.
    }
  }
  channelEnv({
    socket: (b) => (current.front === null ? noSocket() : current.front.connect(b ?? MAIN)),
    now: () => clock.now.ms,
    after: (ms, f) => clock.after(ms, f),
    cancel: (t) => t?.cancel?.(),
    online: (b) => !(b ?? MAIN).offline,
    browser: () => active,
    mirror: (b) => (b === null ? sessionMirror : b.mirror),
    take: (b, text) => (b === null ? takeMirror(text) : setMirror(b, text)),
  });
}

/** Sets peer `b`'s mirror from a mirror's JSON `text`. */
function setMirror(b, text) {
  try {
    if (text) b.mirror.set(JSON.parse(text));
  } catch {
    // A malformed mirror is ignored, as `takeMirror` does.
  }
}

/**
 * A peer's `fetch`: its request goes to the app in-process with its own
 * cookie, and a changed session reaches its mirror, not the test's.
 */
function peerFetch(b, url, init) {
  return b.ctx.fetch(url, init).then((r) => {
    setMirror(b, r.headers.get("x-k-session"));
    return { ...r, headers: { get: (n) => (String(n).toLowerCase() === "x-k-session" ? null : r.headers.get(n)) } };
  });
}

/**
 * `k.test.TestChannels` (44-channels.md, "TestChannels"): the test's own
 * connection. `open` lists its open channels; `drop` drops its socket,
 * resumable or not.
 */
export function testChannels() {
  return {
    get open() {
      return current.front === null ? [] : current.front.open(MAIN);
    },
    drop(resumable) {
      current.front?.drop(MAIN, resumable !== false);
      flush();
    },
  };
}

/**
 * A new browser of the running test: its own cookie and `Session` mirror,
 * the test's server, clock and per-test instances.
 */
function newPeer() {
  const T = current.context;
  if (T === null) throw new KPanic("this test has no server: a peer is another browser of the same server");
  const ctx = Object.create(T);
  ctx.cookie = null;
  ctx.guest = null;
  ctx.direct = null;
  const b = { ctx, mirror: signal({ user: null }), offline: false };
  ctx.sessionChanged = (st) => {
    ctx.cookie = st.sessionId;
    ctx.guest = st.guestId;
    try {
      b.mirror.set(st.mirror());
    } catch {
      // A session that isn't loaded yet has no mirror to share.
    }
  };
  /** A request state of the peer, its session loaded, for `SessionManager`. */
  const state = async () => {
    const st = new RequestState(T.spec, ctx.request("GET", "/", [], EMPTY, undefined));
    await track(st.load());
    return st;
  };
  b.handle = {
    /** A client of the protocol whose stub is `stub`, calling as this peer. */
    client(stub) {
      return new Proxy(stub, {
        get(t, k) {
          const v = t[k];
          if (typeof v !== "function") return v;
          return (...a) => {
            const was = active;
            active = b;
            try {
              return v.apply(t, a);
            } finally {
              active = was;
            }
          };
        },
      });
    },
    async signOut(_c) {
      const st = await state();
      await track(sessionManager(st).signOut(null));
      flush();
    },
  };
  b.signIn = async (userId, roles) => {
    const st = await state();
    await track(sessionManager(st).signIn(null, userId, roles));
  };
  return b;
}

/**
 * `k.test.TestPeers` (44-channels.md, "TestPeer"): more browsers in the
 * test, each a `TestPeer` with its own session.
 */
export function testPeers() {
  return {
    async signIn(_c, userId, roles) {
      const b = newPeer();
      await b.signIn(userId, roles);
      return b.handle;
    },
    guest() {
      return newPeer().handle;
    },
  };
}

/**
 * `TestChannel()`: a scripted channel on the test's clock; `E` is its
 * refusal enum's constructor object.
 * @param {object|null} E
 */
export function testChannel(E) {
  return scriptedChannel(E, sharedClock());
}

/**
 * `k.test.TestWebSocketClient` (44-channels.md, "Testing"): one per test,
 * shared with the test's server containers (the same memo key as the
 * server's `perTest`), so a test scripts the endpoints a server service
 * connects to as well as the browser's.
 * @param {object} E the generated `WebSocketError` enum
 * @param {object} M the generated `WebSocketMessage` enum
 * @param {object} St the generated `WebSocketState` enum
 */
export function testWebSocketClient(E, M, St) {
  const memo = current.context?.memo;
  if (!memo) return (current.sockets ??= scriptedSockets(E, M, St));
  if (!memo.has("TestWebSocketClient")) memo.set("TestWebSocketClient", scriptedSockets(E, M, St));
  return memo.get("TestWebSocketClient");
}

/** `TestWebSocketServer()`: a scripted endpoint (`../runtime/websocket.js`). */
export const testWebSocketServer = TestWebSocketServer;

/**
 * The browser's `@Native` members, with `k.test`'s first, then the
 * server's for those the browser doesn't implement.
 * @param {string} name `module.Owner$member`
 */
export function native(name) {
  if (name === "k.test.render") return render;
  if (name === "k.device.Position$sample") return positionSample;
  // Test code runs in the test's one process: a server-only stdlib member
  // it calls (`Hmac.sha256` to sign a webhook payload) is the server's.
  if (!hasNative(name) && Object.hasOwn(serverNatives, name)) return serverNatives[name];
  return browserNative(name);
}

// --------------------------------------------------------------- runner

/**
 * The app's pages by URL (the test bundle's `$pages`): `{parse(url),
 * value(params)}`, most specific first.
 * @type {Array<{parse: Function, value: Function}>}
 */
let pages = [];

/** The watchdogs running: none once a run has returned. */
let running = 0;

/**
 * The number of test watchdogs still running (0 between runs).
 * @returns {number}
 */
export function watchdogs() {
  return running;
}

/** What a test that ended without finishing reports. */
const HUNG = {};

/**
 * Runs the tests named in `selected` (`Group › description`), in order:
 * each with a fresh document and a fresh container (`group.container()`),
 * its group class built over that container, then the test's method.
 * `routes` are the bundle's `$pages`, which `Screen.currentPage` matches
 * the browser's URL against.
 * With `server` (the server entry's `$test`), each test also gets a fresh
 * process and database (`op_k_test_reset`), its group's server container
 * over the test's one session, and a `fetch` that answers every request
 * in-process through the app's handler.
 * Returns one result per test run:
 * `{group, name, file, outcome: "pass"|"fail", failure?}`, where a failure
 * is `{message, start, end, values}` with the source range of the failed
 * expectation, or of the test's name when it panicked, threw or never
 * finished.
 * @param {Array<object>} groups the generated `$groups`
 * @param {string[]} selected
 */
export async function run(groups, selected, server = null, routes = []) {
  pages = routes;
  const wanted = new Set(selected);
  const results = [];
  if (server !== null) trackOps();
  for (const [gi, g] of groups.entries()) {
    for (const t of g.tests) {
      const full = `${g.name} › ${t.name}`;
      if (!wanted.has(full)) continue;
      installDom();
      installBrowser(full);
      let outcome = "pass";
      let fault = null;
      try {
        let clock = testClock();
        let direct = null;
        let context = null;
        if (server !== null) {
          // A fresh process and database, and the test's one session.
          globalThis.Deno?.core?.ops?.op_k_test_reset?.();
          server.reset();
          sessionMirror.set({ user: null });
          const T = new TestContext(server, server.groups[gi]);
          clock = T.clock;
          const st = new RequestState(server.spec, T.request("GET", "/", [], EMPTY, undefined));
          await st.load();
          T.direct = st;
          direct = T.container(st);
          // `TestConnectivity.goOffline()` fails the browser's requests as
          // a device without a network does.
          globalThis.fetch = (url, init) => {
            const b = active;
            if (b !== null) return b.offline ? Promise.reject(new TypeError("Failed to fetch")) : peerFetch(b, url, init);
            return current.offline ? Promise.reject(new TypeError("Failed to fetch")) : T.fetch(url, init);
          };
          context = T;
        }
        begin(null, clock, direct);
        installChannels(clock, context);
        context = null;
        const container = g.container();
        current.container = container;
        const instance = new g.group(container);
        let finished = false;
        // Finished either way: a failed test's watchdog must stop too.
        const done = Promise.resolve(instance[t.method](null)).then(
          () => {
            finished = true;
            return null;
          },
          (e) => {
            finished = true;
            throw e;
          },
        );
        // Many idle rounds without finishing, with no server work in
        // flight: nothing will finish it.
        const watchdog = (async () => {
          running++;
          try {
            for (let i = 0; i < 50 && !finished; ) {
              await idle();
              if (finished) break;
              if (inflight.size > 0) await Promise.race([done, ...inflight].map((p) => p.then(() => null, () => null)));
              else i++;
            }
          } finally {
            running--;
          }
          return HUNG;
        })();
        const result = await Promise.race([done, watchdog]).finally(() => watchdog);
        if (result === HUNG) {
          fault = { message: "the test never finished: it is waiting for something that doesn't happen (an unanswered request, or a timer of the TestClock that isn't advanced)", start: t.start, end: t.end, values: [] };
        }
        flush();
      } catch (e) {
        fault = describe(e, t);
      }
      try {
        end();
      } catch (e) {
        fault ??= describe(e, t);
      }
      if (fault === null && current.errors.length > 0) fault = describe(current.errors[0], t);
      if (fault !== null) outcome = "fail";
      results.push({ group: g.name, name: t.name, file: g.file, outcome, ...(fault === null ? {} : { failure: fault }) });
    }
  }
  return results;
}

/** Whether `e` is an `Error` value's box, `{error, text}` and nothing else. */
function isErrorBox(e) {
  if (e === null || typeof e !== "object" || typeof e.text !== "string" || !("error" in e)) return false;
  const keys = Object.keys(e);
  return keys.length === 2 && keys.includes("error") && keys.includes("text");
}

/** The failure of a test that ended with `e`. */
function describe(e, t) {
  if (e && e.kTest) {
    const f = e.kTest;
    return { message: f.message, start: f.start < 0 ? t.start : f.start, end: f.start < 0 ? t.end : f.end, values: f.values };
  }
  if (isPanic(e)) return { message: `the test panicked: ${e.message}`, start: t.start, end: t.end, values: [] };
  if (e instanceof Error) return { message: `the test failed with a JavaScript error: ${e.message}`, start: t.start, end: t.end, values: [] };
  // An `Error` value is the box `{error, text}` (codegen.md §7): its text,
  // as the run corpus prints it. Otherwise the error's structure (its case
  // tag `$` and payload), for the report.
  let shape = "an unprintable value";
  try {
    shape = isErrorBox(e) ? e.text : (JSON.stringify(e) ?? String(e));
  } catch {
    // A cyclic error keeps the placeholder.
  }
  return { message: "the test threw an error it didn't catch", start: t.start, end: t.end, values: [["the error", shape.length > 300 ? `${shape.slice(0, 300)}…` : shape]] };
}
