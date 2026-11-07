/**
 * @module k:/runtime/outbound.js
 *
 * The server's bodies of the stdlib's I/O platform services
 * (docs/compiler/server.md §18): `k.http.FetchHttpClient` over `op_k_fetch`,
 * the `k.test.TestHttpClient` double, `k.server.SmtpEmailProvider` over
 * `op_k_smtp_send` and the `k.test.RecordingEmailProvider` double. The
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
  const map = new Map();
  const all = new Map();
  for (const [n, v] of headers) {
    const name = String(n).toLowerCase();
    if (!all.has(name)) all.set(name, []);
    all.get(name).push(v);
    // A repeated header reads as its values joined, except `set-cookie`,
    // which reads as the first (`all` has every one).
    if (!map.has(name)) map.set(name, v);
    else if (name !== "set-cookie") map.set(name, `${map.get(name)}, ${v}`);
  }
  const text = new TextDecoder().decode(bytes);
  return {
    status,
    isSuccess: code >= 200 && code < 300,
    headers: { map, all },
    bodyBytes: bytes,
    body: text,
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
    get requests() {
      return requests.slice();
    },
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
    respond(path, response) {
      const [bytes, type] = bodyBytes(response.body);
      const headers = [...(response.headers ?? [])];
      if (type !== null && !headers.some(([n]) => n === "content-type")) headers.unshift(["content-type", type]);
      const code = codeOf(response.status);
      canned.set(String(path), { response: received(E, statusOf(code), code, headers, bytes) });
    },
    serveJson(codec, path, value) {
      const bytes = new TextEncoder().encode(stringify(codec.encode(value)));
      canned.set(String(path), { response: received(E, statusOf(200), 200, [["content-type", "application/json"]], bytes) });
    },
    serveText(path, text, status) {
      const code = status === undefined ? 200 : codeOf(status);
      const bytes = new TextEncoder().encode(String(text));
      canned.set(String(path), { response: received(E, statusOf(code), code, [["content-type", "text/plain; charset=utf-8"]], bytes) });
    },
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
    get sent() {
      return sent.slice();
    },
    async send(_c, email) {
      if (failure !== undefined) throw failure;
      if (hasLineBreak(String(email.subject))) throw EmailError.invalidMessage("the subject contains a line break");
      sent.push(email);
    },
    simulateFailure(error) {
      failure = error;
    },
  };
}

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
    async enqueue(_c, _codec, kind, payload, delay) {
      queued.push({ name: String(kind.name), payload, delay: delay ?? 0 });
    },
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
