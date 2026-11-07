/**
 * @module k:runtime/remote
 *
 * Calls between the services of one system
 * (docs/design/40-workspaces-and-services.md; docs/compiler/runtime.md,
 * "Remote clients"): the server-side client a `use remote(Config) for
 * Protocol` rule or `RemoteClient.at` builds, and the service tokens that
 * authenticate its calls.
 *
 * A call is the app's own RPC wire format (28-rpc-wire-format.md): a
 * Connect unary POST of the JSON message to `url + /package.Service/Method`,
 * sent with `op_k_fetch` (no CORS, no cookies). It resolves to the same
 * outcomes as the browser's `rpc` (`rpc.js`), so the generated stub maps
 * them to the rpc's typed error with `rpcError`: a Connect error body,
 * `offline` when the request fails, `timeout` after the config's timeout.
 *
 * A **service token** is `v1.<caller>.<expiry>.<mac>`: the calling app's
 * name (URI-encoded, `.` as `%2E`), the expiry in milliseconds since the epoch (one
 * minute after signing) and the hex HMAC-SHA256 of the first three parts
 * under the shared secret. It travels in the `x-k-service-token` header;
 * the serving app's `ServiceAuthenticated` guard verifies it
 * (`verifyServiceToken`, in constant time) with its `K_SERVICE_TOKEN`.
 */

import { KPanic } from "./values.js";
import { Secret } from "./std.js";

/** The ops the Rust server exposes to this isolate; `undefined` outside it. */
const ops = globalThis.Deno?.core?.ops;

/** The header that carries the service token. */
export const SERVICE_TOKEN_HEADER = "x-k-service-token";

/** How long a signed token is valid, in milliseconds. */
export const TOKEN_LIFETIME_MS = 60_000;

/** The default time a remote call waits for its answer, in milliseconds. */
export const DEFAULT_REMOTE_TIMEOUT_MS = 10_000;

/** The UTF-8 encoder of request bodies. */
const encoder = new TextEncoder();

/** An expiry: 1 to 16 decimal digits. */
const DIGITS = new RegExp("^[0-9]{1,16}$");

/** The HMAC-SHA256 of `message` under `key` (both text), as bytes. */
function defaultHmac(key, message) {
  return new Uint8Array(ops.op_k_hmac(256, encoder.encode(key), encoder.encode(message)));
}

/** Lower-case hex of bytes. */
function hex(bytes) {
  let out = "";
  for (const b of bytes) out += b.toString(16).padStart(2, "0");
  return out;
}

/** The text of a `Secret` or a plain string; `""` for anything else. */
function reveal(secret) {
  if (secret instanceof Secret) return String(secret.reveal());
  return typeof secret === "string" ? secret : "";
}

/**
 * Signs a service token for `caller`.
 * @param {string} caller the calling app's `[package] name`
 * @param {Secret|string} secret the shared secret
 * @param {number} now milliseconds since the epoch
 * @param {(key: string, message: string) => Uint8Array} [hmac] stands in for `op_k_hmac` in tests
 * @returns {string}
 */
export function serviceToken(caller, secret, now, hmac = defaultHmac) {
  // `.` separates the parts, so it is escaped in the name too.
  const name = encodeURIComponent(String(caller)).split(".").join("%2E");
  const payload = `v1.${name}.${Math.floor(now + TOKEN_LIFETIME_MS)}`;
  return `${payload}.${hex(hmac(reveal(secret), payload))}`;
}

/**
 * Verifies a service token: the caller's name when it is well formed,
 * signed with `secret` and not expired (nor valid for implausibly long),
 * else `null`. An empty secret verifies nothing. Never throws.
 * @param {unknown} token the header's value
 * @param {Secret|string|null} secret the serving app's `K_SERVICE_TOKEN`
 * @param {number} now milliseconds since the epoch
 * @param {(key: string, message: string) => Uint8Array} [hmac]
 * @returns {string|null}
 */
export function verifyServiceToken(token, secret, now, hmac = defaultHmac) {
  const key = reveal(secret);
  if (typeof token !== "string" || key === "" || token.length > 4096) return null;
  const parts = token.split(".");
  if (parts.length !== 4 || parts[0] !== "v1" || !DIGITS.test(parts[2])) return null;
  const expiry = Number(parts[2]);
  if (expiry < now || expiry > now + 2 * TOKEN_LIFETIME_MS) return null;
  let expected;
  try {
    expected = hex(hmac(key, `${parts[0]}.${parts[1]}.${parts[2]}`));
  } catch {
    return null;
  }
  const given = parts[3];
  // Constant time in the length of the expected MAC.
  let diff = given.length ^ expected.length;
  for (let i = 0; i < expected.length; i++) diff |= expected.charCodeAt(i) ^ given.charCodeAt(i % Math.max(1, given.length));
  if (diff !== 0) return null;
  try {
    return decodeURIComponent(parts[1]);
  } catch {
    return null;
  }
}

/**
 * Where a remote client calls and how: the base URL (without a trailing
 * `/`), the secret its calls are signed with, the timeout, and the
 * calling app's name.
 * @typedef {object} Remote
 * @property {string} caller
 * @property {string} url
 * @property {Secret|string|null} token
 * @property {number} timeoutMs
 * @property {boolean} [offline] a `TestRemote` double: every call is offline
 * @property {Function} [transport] stands in for `op_k_fetch`
 * @property {Function} [abort] stands in for `op_k_fetch_abort`
 * @property {() => number} [now] stands in for the clock
 * @property {Function} [hmac] stands in for `op_k_hmac`
 */

/**
 * A remote binding from its parts. A missing or non-positive timeout is
 * the default of 10 seconds.
 * @param {string} caller
 * @param {unknown} url
 * @param {Secret|string|null} token
 * @param {unknown} [timeoutMs]
 * @param {object} [hooks] `{transport, abort, now, hmac}` for tests
 * @returns {Remote}
 */
export function remote(caller, url, token, timeoutMs, hooks) {
  const t = typeof timeoutMs === "number" && timeoutMs > 0 ? timeoutMs : DEFAULT_REMOTE_TIMEOUT_MS;
  let base = String(url ?? "");
  while (base.endsWith("/")) base = base.slice(0, -1);
  return { caller: String(caller), url: base, token, timeoutMs: t, ...(hooks ?? {}) };
}

/**
 * The remote binding of a `RemoteService` config instance: its `url`,
 * `token` and optional `timeout` (a `Duration`, in milliseconds).
 * @param {string} caller
 * @param {{url: unknown, token: Secret, timeout?: number}} config
 * @returns {Remote}
 */
export function remoteOf(caller, config) {
  return remote(caller, config?.url, config?.token ?? null, config?.timeout);
}

/**
 * The `.test` double of a remote client (`k.test.TestRemote<P>`): every
 * call is `rpcFailure(.offline)`, and nothing leaves the process.
 * @param {string} caller
 * @returns {Remote}
 */
export function testRemote(caller) {
  return { caller: String(caller), url: "", token: null, timeoutMs: DEFAULT_REMOTE_TIMEOUT_MS, offline: true };
}

/** The id of the next remote call, which cancellation names. */
let nextCall = 1;

/** The value of header `name` among `[name, value]` pairs. */
function headerOf(pairs, name) {
  for (const [n, v] of pairs ?? []) if (String(n).toLowerCase() === name) return String(v);
  return null;
}

/**
 * Calls `path` with `message` (encoded by the stub's codec) at the remote
 * service. Resolves like `rpc` (`rpc.js`): `{ok: true, message}`,
 * `{ok: false, kind: "connect", code, details, retryAfter}`,
 * `{ok: false, kind: "offline"}` or `{ok: false, kind: "timeout"}`. An
 * aborted `signal` makes the promise never settle, and cancels the request.
 * A streaming rpc (`stream`) panics: streaming between services is not
 * supported yet.
 * @param {Remote} r
 * @param {string} path `/package.Service/Method`
 * @param {unknown} message
 * @param {AbortSignal|null} signal
 * @param {unknown} [stream]
 * @returns {Promise<object>}
 */
export function remoteCall(r, path, message, signal, stream) {
  if (stream) throw new KPanic(`${path}: streaming rpcs between services aren't supported yet`);
  if (signal && signal.aborted) return new Promise(() => {});
  if (r.offline) return Promise.resolve({ ok: false, kind: "offline" });
  const now = r.now ? r.now() : (ops?.op_k_now ? ops.op_k_now() : Date.now());
  const headers = [
    ["content-type", "application/json"],
    ["connect-protocol-version", "1"],
    [SERVICE_TOKEN_HEADER, serviceToken(r.caller, r.token, now, r.hmac ?? defaultHmac)],
  ];
  const body = encoder.encode(JSON.stringify(message ?? {}));
  const id = nextCall++;
  const send = r.transport ?? ((i, w, b) => ops.op_k_fetch(i, w, b, true));
  const cancel = r.abort ?? ((i) => ops.op_k_fetch_abort(i));
  return new Promise((resolve) => {
    let done = false;
    const finish = (o) => {
      if (done || (signal && signal.aborted)) return;
      done = true;
      resolve(o);
    };
    signal?.addEventListener?.("abort", () => cancel(id));
    let sent;
    try {
      sent = send(id, { method: "POST", url: r.url + path, headers, timeoutMs: r.timeoutMs }, body);
    } catch {
      finish({ ok: false, kind: "offline" });
      return;
    }
    Promise.resolve(sent).then(
      (o) => {
        switch (o?.kind) {
          case "response": {
            let parsed;
            try {
              const bytes = o.body instanceof Uint8Array ? o.body : new Uint8Array(o.body ?? []);
              const text = new TextDecoder().decode(bytes);
              parsed = JSON.parse(text || "{}");
            } catch {
              finish({ ok: false, kind: "offline" });
              return;
            }
            if (o.status >= 200 && o.status < 300) {
              finish({ ok: true, message: parsed });
            } else {
              finish({
                ok: false,
                kind: "connect",
                code: `${parsed?.code ?? "unknown"}`,
                details: Array.isArray(parsed?.details) ? parsed.details : [],
                retryAfter: +(headerOf(o.headers, "retry-after") ?? 0) || 0,
              });
            }
            return;
          }
          case "timeout":
            finish({ ok: false, kind: "timeout" });
            return;
          case "aborted":
            // Cancelled: the awaiting code never resumes.
            return;
          default:
            finish({ ok: false, kind: "offline" });
        }
      },
      () => finish({ ok: false, kind: "offline" }),
    );
  });
}
