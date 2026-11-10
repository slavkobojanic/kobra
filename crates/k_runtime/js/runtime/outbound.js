/**
 * @module k:/runtime/outbound.js
 *
 * The server's bodies of the stdlib's I/O platform services
 * (docs/compiler/server.md §18): `k.http.FetchHttpClient` over `op_k_fetch`,
 * the `k.test.TestHttpClient` double, `k.server.SmtpEmailProvider` over
 * `op_k_smtp_send` and the `k.test.RecordingEmailProvider` double,
 * `k.http.StandardWebSocketClient` over the `op_k_ws_*` ops,
 * `k.server.VapidWebPush` over `op_k_push_encrypt`, `op_k_push_vapid` and
 * `op_k_fetch`, and the `k.test.RecordingWebPush` double. The
 * generated request container constructs them, passing the generated
 * stdlib enums whose cases they return and throw.
 *
 * Values follow the generated shapes: an `HttpRequest` is a struct with
 * `method` (an `HttpMethod` case, `.$` its index), `url` (text), `query` and
 * `headers` (`Map`s), `secretHeaders` (a `Map` of `Secret`s), `bearerToken`
 * (a `Secret` or `null`), `body` (an `HttpBody` case or `null`) and
 * `timeout` (milliseconds). A received `HttpResponse` is
 * `{status, isSuccess, headers: {map, all}, bodyBytes, body, json(codec)}`;
 * `headers` has the shape `$s.header` reads, like an incoming request's.
 */

import { KPanic, panic } from "./values.js";
import { DecodeFailure, Secret, stringify, toJSON } from "./std.js";
import { headerView } from "./http.js";
import { openedSocket } from "./websocket.js";
import { b64url, fromB64url } from "./crypto.js";

export { testWebSocketClient, TestWebSocketServer } from "./websocket.js";

/** The ops the Rust server exposes to this isolate; `undefined` outside it. */
const ops = globalThis.Deno?.core?.ops;

/** `HttpMethod`'s cases, by index. */
const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

/** `HttpBody`'s cases, by index: json, text, form, bytes. */
const BODY_TYPES = ["application/json", "text/plain; charset=utf-8", "application/x-www-form-urlencoded"];

/** `a=1&b=2` of a `Map<String, String>`. */
function formText(map) {
  return [...map].map(([k, v]) => encodeURIComponent(k) + "=" + encodeURIComponent(v)).join("&");
}

/**
 * The `HttpStatus` value of a status code: the named case whose code it
 * is (`codes` lists them in declaration order, `null` for `other`), else
 * `S.other(code)`, so `.other` never holds a named code (32-http-client.md).
 * @param {object} S the generated `HttpStatus` enum
 * @param {Array<number|null>} codes
 * @returns {(code: number) => unknown}
 */
export function httpStatus(S, codes) {
  const names = Object.keys(S);
  return (code) => {
    const i = codes.indexOf(code);
    return i < 0 ? S.other(code) : S[names[i]];
  };
}

/**
 * The wire form of an outgoing request: the URL with its query, the
 * headers (secrets revealed, `Authorization` from `bearerToken`, the body's
 * `Content-Type` unless set), the body bytes and the timeout.
 * @param {object} r an `HttpRequest`
 * @returns {{method: string, url: string, headers: Array<[string, string]>, body: Uint8Array|null, timeoutMs: number}}
 */
export function wireRequest(r) {
  const q = formText(r.query ?? []);
  const base = String(r.url);
  const url = base + (q ? (base.includes("?") ? "&" : "?") + q : "");
  const headers = new Map();
  for (const [k, v] of r.headers ?? []) headers.set(String(k).toLowerCase(), String(v));
  for (const [k, v] of r.secretHeaders ?? []) headers.set(String(k).toLowerCase(), String(v instanceof Secret ? v.reveal() : v));
  if (r.bearerToken) headers.set("authorization", "Bearer " + String(r.bearerToken.reveal()));
  let body = null;
  const b = r.body;
  if (b) {
    let text;
    switch (b.$) {
      case 0:
        text = stringify(b._0 && b._0.codec ? b._0.codec.encode(b._0.value) : toJSON(b._0));
        break;
      case 2:
        text = formText(b._0);
        break;
      case 3:
        body = b._0 instanceof Uint8Array ? b._0 : new Uint8Array(b._0 ?? []);
        break;
      default:
        text = String(b._0);
    }
    if (text !== undefined) body = new TextEncoder().encode(text);
    if (!headers.has("content-type")) headers.set("content-type", b.$ === 3 ? String(b.contentType ?? "application/octet-stream") : BODY_TYPES[b.$]);
  }
  const timeoutMs = typeof r.timeout === "number" && r.timeout > 0 ? r.timeout : 30000;
  return { method: METHODS[r.method?.$ ?? 0] ?? "GET", url, headers: [...headers], body, timeoutMs };
}

/**
 * A received `HttpResponse`. `json(codec)` decodes the body, throwing
 * `E.decoding(message)` when it isn't the expected JSON.
 * @param {object} E the generated `HttpError` enum
 * @param {unknown} status an `HttpStatus` value
 * @param {number} code its numeric code
 * @param {Array<[string, string]>} headers names lower case
 * @param {Uint8Array} bytes
 */
export function received(E, status, code, headers, bytes) {
  const { map, all } = headerView(headers);
  const text = new TextDecoder().decode(bytes);
  return {
    status,
    isSuccess: code >= 200 && code < 300,
    headers: { map, all },
    bodyBytes: bytes,
    body: text,
    /**
     * Decodes the body as JSON with `codec`.
     * @param {object} codec the codec of the expected type
     * @returns {unknown} the decoded value
     * @throws {object} `HttpError.decoding` when the body is not JSON or does
     *   not decode; a panic inside the codec propagates unchanged
     */
    json(codec) {
      let j;
      try {
        j = JSON.parse(text);
      } catch (e) {
        throw E.decoding(`the body is not JSON: ${String(e?.message ?? e)}`);
      }
      try {
        return codec.decode(j, "");
      } catch (e) {
        if (e instanceof DecodeFailure) throw E.decoding(e.path ? `${e.path}: ${e.message}` : e.message);
        if (e instanceof KPanic) throw e;
        throw E.decoding(String(e?.message ?? e));
      }
    },
  };
}

/** `fetchJson`, the interface's default method, over `send`. */
function withFetchJson(E, client) {
  client.fetchJson = async (c, codec, r) => {
    const res = await client.send(c, r);
    if (!res.isSuccess) throw E.status(res.status, res.body);
    return res.json(codec);
  };
  return client;
}

/** The id of the next outgoing fetch, which `op_k_fetch_abort` names. */
let nextFetch = 1;

/**
 * The server's `k.http.FetchHttpClient` (32-http-client.md): `send`
 * resolves to the response whatever its status and rejects with
 * `E.offline` or `E.timeout`; when the request's signal `c` is aborted the
 * request is cancelled and the call never resumes (cancellation isn't an
 * error). `transport` stands in for `op_k_fetch` in tests: `(id, wire) =>
 * Promise<{kind, status?, headers?, body?, message?}>`.
 * @param {object} E the generated `HttpError` enum
 * @param {object} S the generated `HttpStatus` enum
 * @param {Array<number|null>} codes `HttpStatus`'s codes in declaration order
 * @param {Function} [transport]
 * @param {Function} [abort] stands in for `op_k_fetch_abort`
 */
export function fetchHttpClient(E, S, codes, transport, abort) {
  const statusOf = httpStatus(S, codes);
  const send = transport ?? ((id, w) => ops.op_k_fetch(id, { method: w.method, url: w.url, headers: w.headers, timeoutMs: w.timeoutMs }, w.body ?? new Uint8Array(0), w.body !== null));
  const cancel = abort ?? ((id) => ops.op_k_fetch_abort(id));
  return withFetchJson(E, {
    /**
     * Sends `r` through the fetch op. Resolves with the response for any HTTP
     * status; rejects with `HttpError.timeout` or `HttpError.offline`, or a
     * panic for a request the op refuses. A cancelled call never settles, so
     * the awaiting code never resumes.
     * @param {AbortSignal|null} c the call's cancellation signal
     * @param {object} r an `HttpRequest`
     * @returns {Promise<object>} a received `HttpResponse`
     */
    send(c, r) {
      const wire = wireRequest(r);
      const id = nextFetch++;
      if (c?.aborted) return new Promise(() => {});
      return new Promise((resolve, reject) => {
        c?.addEventListener?.("abort", () => cancel(id));
        send(id, wire).then((o) => {
          switch (o.kind) {
            case "response":
              resolve(received(E, statusOf(o.status), o.status, o.headers, o.body instanceof Uint8Array ? o.body : new Uint8Array(o.body ?? [])));
              break;
            case "timeout":
              reject(E.timeout);
              break;
            case "offline":
              logWarning(`HttpClient: ${wire.method} ${redactUrl(wire.url)} failed: ${o.message}`);
              reject(E.offline);
              break;
            case "aborted":
              // Cancelled: the awaiting code never resumes.
              break;
            default:
              reject(new KPanic(`HttpClient: ${wire.method} ${redactUrl(wire.url)}: ${o.message ?? "an invalid request"}`));
          }
        }, reject);
      });
    },
  });
}

/** The URL without its query, which may carry tokens. */
function redactUrl(url) {
  const q = url.indexOf("?");
  return q < 0 ? url : url.slice(0, q) + "?…";
}

/**
 * Logs `message` at warning level through `op_k_log`, when the server
 * runtime provides it.
 * @param {string} message
 */
function logWarning(message) {
  if (ops?.op_k_log) ops.op_k_log("warning", message);
}

/** The scheme and authority of a URL, then its path. */
const URL_PATH = new RegExp("^[a-z][a-z0-9+.-]*://[^/?#]*([^?#]*)", "i");

/** `true` if `text` has a carriage return or line feed. */
const hasLineBreak = (text) => text.includes("\r") || text.includes("\n");

/** The path of a URL (`/v1/x` of `https://h/v1/x?q`). */
function pathOf(url) {
  const m = URL_PATH.exec(String(url));
  return m ? m[1] || "/" : String(url);
}

/**
 * The bytes and type of an `HttpBody` value for a canned response.
 * @param {object|null} b
 */
function bodyBytes(b) {
  if (!b) return [new Uint8Array(0), null];
  const w = wireRequest({ body: b, method: { $: 1 }, url: "" });
  return [w.body ?? new Uint8Array(0), w.headers.find(([n]) => n === "content-type")?.[1] ?? null];
}

/**
 * `k.test.TestHttpClient` (32-http-client.md, "Tests"): answers only the
 * paths it has a canned response for, records every request, and fails
 * the test (a panic naming the method and URL) on any other.
 * @param {object} E the generated `HttpError` enum
 * @param {object} S the generated `HttpStatus` enum
 * @param {Array<number|null>} codes
 */
export function testHttpClient(E, S, codes) {
  const statusOf = httpStatus(S, codes);
  const codeOf = (s) => {
    if (typeof s === "number") return s;
    const c = codes[s?.$];
    return c === null || c === undefined ? s?._0 : c;
  };
  /** @type {Map<string, {response?: object, error?: unknown}>} */
  const canned = new Map();
  const requests = [];
  return withFetchJson(E, {
    /** A copy of every request sent so far, oldest first. */
    get requests() {
      return requests.slice();
    },
    /**
     * Records `r` and answers the canned response for its path, or throws the
     * canned error; panics when nothing is canned for the path.
     * @param {unknown} _c the cancellation context (unused)
     * @param {object} r an `HttpRequest`
     */
    async send(_c, r) {
      requests.push(r);
      const answer = canned.get(pathOf(r.url));
      if (!answer) {
        const w = wireRequest(r);
        panic(`TestHttpClient has no response for ${w.method} ${w.url}; add one with respond(path:with:)`);
      }
      if (answer.error !== undefined) throw answer.error;
      return answer.response;
    },
    /**
     * Cans `response` (an `HttpResponse` being built) for requests to `path`,
     * adding the body's content type unless the response sets one.
     * @param {string} path a URL path, without the query
     * @param {object} response
     */
    respond(path, response) {
      const [bytes, type] = bodyBytes(response.body);
      const headers = [...(response.headers ?? [])];
      if (type !== null && !headers.some(([n]) => n === "content-type")) headers.unshift(["content-type", type]);
      const code = codeOf(response.status);
      canned.set(String(path), { response: received(E, statusOf(code), code, headers, bytes) });
    },
    /**
     * Cans a 200 JSON response holding `value` for requests to `path`.
     * @param {object} codec the codec of `value`'s type
     * @param {string} path
     * @param {unknown} value
     */
    serveJson(codec, path, value) {
      const bytes = new TextEncoder().encode(stringify(codec.encode(value)));
      canned.set(String(path), { response: received(E, statusOf(200), 200, [["content-type", "application/json"]], bytes) });
    },
    /**
     * Cans a plain-text response for requests to `path`, 200 unless `status`
     * is given.
     * @param {string} path
     * @param {string} text
     * @param {object|number} [status] an `HttpStatus` or a code
     */
    serveText(path, text, status) {
      const code = status === undefined ? 200 : codeOf(status);
      const bytes = new TextEncoder().encode(String(text));
      canned.set(String(path), { response: received(E, statusOf(code), code, [["content-type", "text/plain; charset=utf-8"]], bytes) });
    },
    /**
     * Makes requests to `path` throw `error`.
     * @param {string} path
     * @param {unknown} error an `HttpError`
     */
    fail(path, error) {
      canned.set(String(path), { error });
    },
  });
}

// ------------------------------------------------------------------ email

/**
 * The SMTP settings of `SmtpConfig` from the validated configuration
 * (`op_k_server_info`'s `config`, secrets as `{"$secret": value}`).
 * @param {Record<string, unknown>} config
 */
export function smtpSettings(config) {
  const plain = (v) => (v && typeof v === "object" && "$secret" in v ? v.$secret : v);
  const host = plain(config.SMTP_HOST);
  if (typeof host !== "string" || host === "") panic("SmtpEmailProvider needs SMTP_HOST");
  const port = Number(plain(config.SMTP_PORT) ?? 587);
  const security = String(plain(config.SMTP_SECURITY) ?? "startTls");
  const username = plain(config.SMTP_USERNAME) ?? null;
  const password = plain(config.SMTP_PASSWORD) ?? null;
  return {
    settings: { host, port, security, username: username === null ? null : String(username), password: password === null ? null : String(password) },
    from: plain(config.MAIL_FROM) ?? null,
  };
}

/**
 * The wire form of an `Email` for `op_k_smtp_send`, or the reason it
 * can't be sent as written (a line break in the subject).
 * @param {object} email an `Email`
 * @param {string|null} defaultFrom `MAIL_FROM`
 * @returns {{message?: object, invalid?: string}}
 */
export function wireEmail(email, defaultFrom) {
  const subject = String(email.subject);
  if (hasLineBreak(subject)) return { invalid: "the subject contains a line break" };
  const from = email.from ?? defaultFrom;
  if (from === null || from === undefined) return { invalid: "no sender: set MAIL_FROM or the email's from" };
  const list = (l) => [...(l ?? [])].map(String);
  return {
    message: {
      from: String(from),
      to: String(email.to),
      subject,
      text: String(email.text),
      html: email.html ?? null,
      replyTo: email.replyTo ?? null,
      cc: list(email.cc),
      bcc: list(email.bcc),
    },
  };
}

/** The id of the next outgoing WebSocket, which the `op_k_ws_*` ops name. */
let nextSocket = 1;

/**
 * The server's `k.http.StandardWebSocketClient` (44-channels.md, "Raw
 * WebSockets"): `connect` resolves to an open `WebSocket` and rejects with
 * `E.offline`, `E.timeout` or `E.refused(status:)`; a cancelled call never
 * settles and drops the handshake, and a socket closes when the run that
 * opened it is cancelled. `transport` stands in for the ops in tests:
 * `{connect(id, wire), next(id), send(id, text, bytes, binary), close(id, code, reason)}`.
 * @param {object} E the generated `WebSocketError` enum
 * @param {object} M the generated `WebSocketMessage` enum
 * @param {object} St the generated `WebSocketState` enum
 * @param {object} S the generated `HttpStatus` enum
 * @param {Array<number|null>} codes `HttpStatus`'s codes in declaration order
 * @param {object} [transport]
 */
export function webSocketClient(E, M, St, S, codes, transport) {
  const statusOf = httpStatus(S, codes);
  const op = transport ?? {
    connect: (id, w) => ops.op_k_ws_connect(id, w),
    next: (id) => ops.op_k_ws_next(id),
    send: (id, text, bytes, binary) => ops.op_k_ws_send(id, text, bytes, binary),
    close: (id, code, reason) => ops.op_k_ws_close(id, code, reason),
  };
  const empty = new Uint8Array(0);
  return {
    /**
     * Opens the socket `r` describes.
     * @param {AbortSignal|null} c the call's cancellation signal
     * @param {object} r a `WebSocketRequest`
     */
    connect(c, r) {
      if (c?.aborted) return new Promise(() => {});
      const id = nextSocket++;
      const w = wireRequest({ ...r, query: null, body: null, method: null });
      const wire = { url: w.url, protocol: r.subprotocol ?? null, headers: w.headers, timeoutMs: w.timeoutMs, maxMessage: typeof r.maxMessage === "number" ? r.maxMessage : 1000000 };
      return new Promise((resolve, reject) => {
        const cancel = () => op.close(id, 1000, "");
        c?.addEventListener?.("abort", cancel);
        op.connect(id, wire).then((o) => {
          c?.removeEventListener?.("abort", cancel);
          switch (o.kind) {
            case "open": {
              const s = openedSocket(
                { E, St },
                {
                  send: (m) => op.send(id, m.$ === 0 ? String(m._0) : "", m.$ === 0 ? empty : m._0, m.$ !== 0),
                  close: (code, reason) => op.close(id, code, reason),
                  buffered: () => 0,
                },
                o.protocol ?? null,
                c,
              );
              (async () => {
                for (;;) {
                  const n = await op.next(id);
                  if (n.kind === "text") s.message(M.text(n.text));
                  else if (n.kind === "binary") s.message(M.binary(n.bytes instanceof Uint8Array ? n.bytes : new Uint8Array(n.bytes ?? [])));
                  else if (n.kind === "tooLarge") return s.tooLarge();
                  else return s.closed(n.code ?? 1006, n.reason ?? "");
                }
              })();
              resolve(s.socket);
              break;
            }
            case "refused":
              reject(E.refused(statusOf(o.status)));
              break;
            case "timeout":
              reject(E.timeout);
              break;
            case "offline":
              logWarning(`WebSocketClient: ${redactUrl(wire.url)} failed: ${o.message}`);
              reject(E.offline);
              break;
            case "aborted":
              // Cancelled: the awaiting code never resumes.
              break;
            default:
              reject(new KPanic(`WebSocketClient: ${redactUrl(wire.url)}: ${o.message ?? "an invalid request"}`));
          }
        }, reject);
      });
    },
  };
}

/**
 * The server's `k.server.SmtpEmailProvider` (31-email.md): `send` resolves
 * once the server accepted the message and throws `EmailError`'s cases
 * otherwise. `transport` stands in for `op_k_smtp_send` in tests.
 * @param {object} EmailError the generated enum
 * @param {Record<string, unknown>} config the validated configuration
 * @param {Function} [transport]
 */
export function smtpEmailProvider(EmailError, config, transport) {
  const sendMail = transport ?? ((settings, message) => ops.op_k_smtp_send(settings, message));
  let resolved = null;
  return {
    /**
     * Sends `email` over SMTP, resolving once the server accepts it.
     * @param {unknown} _c the cancellation context (unused)
     * @param {object} email an `Email`
     * @throws {object} `EmailError.invalidMessage`, `EmailError.rejected` or
     *   `EmailError.unavailable`
     */
    async send(_c, email) {
      resolved ??= smtpSettings(config);
      const w = wireEmail(email, resolved.from);
      if (w.invalid !== undefined) throw EmailError.invalidMessage(w.invalid);
      const o = await sendMail(resolved.settings, w.message);
      switch (o.kind) {
        case "sent":
          return;
        case "rejected":
          throw EmailError.rejected(o.reason);
        case "invalidMessage":
          throw EmailError.invalidMessage(o.reason);
        default:
          logWarning(`SmtpEmailProvider: ${o.message ?? "the SMTP server is unavailable"}`);
          throw EmailError.unavailable;
      }
    },
  };
}

/**
 * `k.test.RecordingEmailProvider` (31-email.md, "Tests"): keeps every
 * message sent, oldest first; `simulateFailure(error)` makes every later
 * `send` throw `error`. A message with a line break in its subject throws
 * `invalidMessage`, as the real provider does.
 * @param {object} EmailError the generated enum
 */
export function recordingEmailProvider(EmailError) {
  const sent = [];
  let failure;
  return {
    /** A copy of every message sent so far, oldest first. */
    get sent() {
      return sent.slice();
    },
    /**
     * Records `email`, or throws the simulated failure or `invalidMessage`
     * for a subject with a line break.
     * @param {unknown} _c the cancellation context (unused)
     * @param {object} email an `Email`
     */
    async send(_c, email) {
      if (failure !== undefined) throw failure;
      if (hasLineBreak(String(email.subject))) throw EmailError.invalidMessage("the subject contains a line break");
      sent.push(email);
    },
    /**
     * Makes every later `send` throw `error`.
     * @param {object} error an `EmailError`
     */
    simulateFailure(error) {
      failure = error;
    },
  };
}

// ------------------------------------------------------------------- push

/** The largest message one `aes128gcm` record holds (`k_server::push::MAX_PLAINTEXT`). */
const PUSH_MAX_PLAINTEXT = 3993;

/** How long a VAPID token is valid, and how long one is reused, in seconds. */
const VAPID_LIFE = 12 * 3600;
const VAPID_REUSE = 11 * 3600;

/** `timeToLive`'s default, one day, in milliseconds. */
const PUSH_TTL = 86400000;

/**
 * VAPID tokens by push service origin and signing key, kept for the
 * isolate: a `VapidWebPush` is made per request container, and a token is
 * good for every request the isolate serves in its 11 hours.
 */
const vapidTokens = new Map();

/** `scheme://authority` at the start of a push endpoint. */
const PUSH_ENDPOINT = new RegExp("^(https?)://([^/?#]+)", "i");

/** Loopback hosts, the only ones a push endpoint may reach over `http`. */
const LOOPBACK = ["localhost", "127.0.0.1", "[::1]"];

/**
 * The push service origin of `endpoint` (the VAPID `aud`), or `null` when
 * it isn't `https` (or `http` to a loopback host).
 * @param {string} endpoint
 * @returns {string|null}
 */
export function pushOrigin(endpoint) {
  const m = PUSH_ENDPOINT.exec(String(endpoint));
  if (!m) return null;
  const scheme = m[1].toLowerCase();
  const authority = m[2].toLowerCase();
  if (authority.includes("@")) return null;
  const host = authority.startsWith("[") ? authority.slice(0, authority.indexOf("]") + 1) : authority.split(":")[0];
  if (scheme !== "https" && !LOOPBACK.includes(host)) return null;
  return `${scheme}://${authority}`;
}

/**
 * The JSON text `k-sw.js` shows: a `SystemNotification` with its page
 * turned into the page's path.
 * @param {object} n a `SystemNotification`
 * @param {((page: object) => string)|null} href the app's `Page$href`
 */
export function pushPayload(n, href) {
  return JSON.stringify({
    title: String(n.title),
    body: n.body ?? null,
    tag: n.tag ?? null,
    url: n.opens == null || !href ? null : href(n.opens),
  });
}

/**
 * The server's `k.server.VapidWebPush` (docs/design/50-push.md): `send`
 * encrypts the message to the subscription (RFC 8291), signs a VAPID token
 * for the push service's origin (RFC 8292), reusing one per origin for 11
 * of its 12 hours across the isolate's requests, and posts it (RFC 8030). It resolves on any `2xx` and
 * throws `WebPushError`'s cases otherwise. `serverKey` is the configured
 * key's public half, `null` without one. `io` stands in for the ops in
 * tests: `{encrypt(p256dh, auth, plaintext), vapid(scalar, aud, exp, sub),
 * fetch(id, wire), now()}`.
 * @param {object} E the generated `WebPushError` enum
 * @param {() => {privateKey: unknown, contact: string|null}} config the container's `PushConfig`
 * @param {((page: object) => string)|null} href the app's `Page$href`
 * @param {object} [io]
 */
export function vapidWebPush(E, config, href, io) {
  const encrypt = io?.encrypt ?? ((p, a, m) => ops.op_k_push_encrypt(p, a, m));
  const vapid = io?.vapid ?? ((k, aud, exp, sub) => ops.op_k_push_vapid(k, aud, exp, sub));
  const post =
    io?.fetch ?? ((id, w) => ops.op_k_fetch(id, { method: "POST", url: w.url, headers: w.headers, timeoutMs: w.timeoutMs }, w.body, true));
  const now = io?.now ?? (() => Date.now());
  /** `{secret, pub, contact}` of the configured key, or `null`; read once. */
  let identity;
  const resolve = () => {
    if (identity !== undefined) return identity;
    const c = config();
    const k = c?.privateKey instanceof Secret ? c.privateKey.reveal() : c?.privateKey;
    identity = k ? { secret: k.secret, pub: k.pub, contact: c.contact ?? "" } : null;
    return identity;
  };
  const tokens = io ? new Map() : vapidTokens;
  /** The `Authorization` value for push service `aud`, reused while fresh. */
  const authorization = (id, aud) => {
    const t = Math.floor(now() / 1000);
    const at = `${aud} ${b64url(id.pub)} ${id.contact}`;
    const kept = tokens.get(at);
    if (kept && kept.until > t) return kept.value;
    const value = vapid(id.secret, aud, t + VAPID_LIFE, id.contact);
    if (!value) panic("VapidWebPush: K_PUSH_PRIVATE_KEY isn't a P-256 private key");
    tokens.set(at, { value, until: t + VAPID_REUSE });
    return value;
  };
  return {
    /** The configured key's public half, base64url of its point, or `null`. */
    get serverKey() {
      const id = resolve();
      return id ? b64url(id.pub) : null;
    },
    /**
     * Sends `n` to `sub`, kept by the push service for `ttl` milliseconds.
     * @param {unknown} _c the cancellation context (unused)
     * @param {object} n a `SystemNotification`
     * @param {{endpoint: string, p256dh: string, auth: string}} sub a `PushSubscription`
     * @param {number} [ttl] a `Duration`; one day when left out
     */
    async send(_c, n, sub, ttl) {
      const id = resolve();
      if (!id) throw E.notConfigured;
      const aud = pushOrigin(sub.endpoint);
      const p256dh = fromB64url(String(sub.p256dh));
      const auth = fromB64url(String(sub.auth));
      if (!aud || !p256dh || !auth) throw E.invalidSubscription;
      const plaintext = new TextEncoder().encode(pushPayload(n, href));
      if (plaintext.length > PUSH_MAX_PLAINTEXT) throw E.tooLarge;
      const body = encrypt(p256dh, auth, plaintext);
      if (!body) throw E.invalidSubscription;
      const seconds = Math.max(0, Math.floor((ttl ?? PUSH_TTL) / 1000));
      const headers = [
        ["authorization", authorization(id, aud)],
        ["content-encoding", "aes128gcm"],
        ["content-type", "application/octet-stream"],
        ["ttl", String(seconds)],
      ];
      const o = await post(nextFetch++, { url: String(sub.endpoint), headers, body: new Uint8Array(body), timeoutMs: 30000 });
      if (o.kind !== "response") {
        logWarning(`WebPush: ${aud} ${o.kind === "timeout" ? "timed out" : (o.message ?? "is unreachable")}`);
        throw E.unavailable;
      }
      const status = o.status;
      if (status >= 200 && status < 300) return;
      if (status === 404 || status === 410) throw E.expired;
      if (status === 413) throw E.tooLarge;
      if (status === 429 || status >= 500) throw E.unavailable;
      throw E.rejected(status);
    },
  };
}

/**
 * `k.test.RecordingWebPush` (docs/design/50-push.md, "Tests"): keeps every
 * message sent as a `SentPush`, oldest first. `serverKey` is a fixed
 * point (RFC 8291's example application server key) until
 * `setConfigured(false)`; `expire(sub)` makes later sends to `sub` throw
 * `expired`, `simulateFailure(e)` every later send throw `e`.
 * @param {object} E the generated `WebPushError` enum
 */
export function recordingWebPush(E) {
  const sent = [];
  const expired = new Set();
  let failure;
  let configured = true;
  return {
    get serverKey() {
      return configured ? TEST_PUSH_KEY : null;
    },
    /** A copy of every message sent so far, oldest first. */
    get sent() {
      return sent.slice();
    },
    async send(_c, n, sub, ttl) {
      if (!configured) throw E.notConfigured;
      if (failure !== undefined) throw failure;
      if (!pushOrigin(sub.endpoint)) throw E.invalidSubscription;
      if (expired.has(String(sub.endpoint))) throw E.expired;
      if (new TextEncoder().encode(pushPayload(n, null)).length > PUSH_MAX_PLAINTEXT) throw E.tooLarge;
      sent.push({ message: n, subscription: sub, timeToLive: ttl ?? PUSH_TTL });
    },
    expire(sub) {
      expired.add(String(sub.endpoint));
    },
    simulateFailure(error) {
      failure = error;
    },
    stopFailing() {
      failure = undefined;
    },
    setConfigured(value) {
      configured = value === true;
    },
  };
}

/** `RecordingWebPush.serverKey`: RFC 8291's example application server public key. */
export const TEST_PUSH_KEY = "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";

// ------------------------------------------------------------------- jobs

/**
 * The server's `k.server.DatabaseJobQueue` (30-background-jobs.md): stores
 * the job in the request's open transaction (`db.tx`), so it commits or
 * rolls back with it, or on its own outside one. `enqueue` doesn't throw;
 * a failure to store is a panic (an infrastructure failure).
 * @param {{tx: number}|null} db the container's `Database`
 * @param {Function} [store] stands in for `op_k_jobs_enqueue` in tests
 */
export function jobQueue(db, store) {
  const put = store ?? ((name, json, delay, tx) => {
    if (!ops?.op_k_jobs_enqueue) panic("the job queue needs the server runtime");
    return ops.op_k_jobs_enqueue(name, json, delay, tx);
  });
  return {
    /**
     * Stores one job of `kind` with its encoded payload, to run after `delay`
     * milliseconds, in the open transaction if there is one.
     * @param {unknown} _c the cancellation context (unused)
     * @param {object|null} codec the payload's codec; `null` for no payload
     * @param {{name: string}} kind the job kind
     * @param {unknown} payload
     * @param {number} [delay] milliseconds, 0 when absent
     * @throws {KPanic} when the job can't be stored
     */
    async enqueue(_c, codec, kind, payload, delay) {
      const json = codec ? stringify(codec.encode(payload)) : "{}";
      try {
        await put(String(kind.name), json, typeof delay === "number" ? delay : 0, db?.tx ?? 0);
      } catch (e) {
        if (e instanceof KPanic) throw e;
        throw new KPanic(`the job queue couldn't store ${kind.name}: ${String(e?.message ?? e)}`);
      }
    },
  };
}

/**
 * `k.test.RecordingJobQueue`: stores jobs without running them;
 * `payloads(of:)` returns a kind's payloads in order.
 */
export function recordingJobQueue() {
  const queued = [];
  return {
    /** Records the job without running it. */
    async enqueue(_c, _codec, kind, payload, delay) {
      queued.push({ name: String(kind.name), payload, delay: delay ?? 0 });
    },
    /**
     * The payloads of the jobs of `kind` enqueued so far, in order.
     * @param {unknown} _codec the payload's codec (unused)
     * @param {{name: string}} kind
     * @returns {unknown[]}
     */
    payloads(_codec, kind) {
      return queued.filter((q) => q.name === String(kind.name)).map((q) => q.payload);
    },
  };
}

/**
 * `k.test.InlineJobQueue`: runs each job when the enqueuing transaction
 * commits, or at once outside one, in the same container; a retryable
 * failure (or a panic) fails the caller with a panic naming the job.
 * @param {object} spec the app spec (its `jobs` table)
 * @param {object} c the container
 * @param {{afterCommit: Function}|null} db the container's `Database`
 */
export function inlineJobQueue(spec, c, db) {
  return {
    /**
     * Runs the job of `kind` with `payload` after the open transaction
     * commits, or at once outside one.
     * @param {AbortSignal|null} $c the caller's cancellation signal
     * @param {object|null} codec the payload's codec
     * @param {{name: string}} kind
     * @param {unknown} payload
     * @throws {KPanic} when the build has no such job, or it fails retryably
     *   or panics
     */
    async enqueue($c, codec, kind, payload) {
      const job = spec?.jobs?.[String(kind.name)];
      if (!job) panic(`InlineJobQueue: the build has no job named ${kind.name}`);
      const json = codec ? JSON.parse(stringify(codec.encode(payload))) : {};
      const run = async () => {
        const o = await runJobBody(job, c, $c, json);
        if (o.outcome === "retryable" || o.outcome === "panic") panic(`InlineJobQueue: ${kind.name} failed: ${o.reason}`);
      };
      if (db?.afterCommit) await db.afterCommit(run);
      else await run();
    },
  };
}

/**
 * Runs a job's `run(payload:)` and describes the outcome for the jobs
 * worker: `done`, `retryable` or `permanent` (the `JobError` cases, by
 * index) with the reason, or `panic` with the message.
 * @param {{run: Function}} job the spec's entry
 * @param {object} c a fresh container
 * @param {AbortSignal|null} $c
 * @param {unknown} payload the payload's JSON value
 * @returns {Promise<{outcome: string, reason?: string}>}
 */
export async function runJobBody(job, c, $c, payload) {
  try {
    await job.run(c, $c, payload);
    return { outcome: "done" };
  } catch (e) {
    if (e instanceof KPanic || e instanceof Error || e === null || typeof e !== "object") {
      return { outcome: "panic", reason: String(e?.message ?? e) };
    }
    const reason = typeof e.reason === "string" ? e.reason : "";
    return { outcome: e.$ === 0 ? "retryable" : "permanent", reason };
  }
}
