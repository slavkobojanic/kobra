/**
 * @module k:runtime/rpc
 *
 * The RPC client generated protocol stubs call
 * (docs/compiler/runtime.md, "RPC client"; docs/design/28-rpc-wire-format.md):
 * Connect-compatible unary calls as JSON over HTTP. It never rejects for
 * network problems: it resolves to an outcome the generated stub maps to the
 * rpc's typed error.
 */

import { signal } from "./signals.js";

/** The default time an rpc waits for a response, in milliseconds. */
export const DEFAULT_TIMEOUT_MS = 30_000;

/**
 * The `Session` mirror the server sends with pages and with responses that
 * change the session (`x-k-session`).
 */
export const sessionMirror = signal({ user: null });

/** The `k-data` payloads the mirror was started from. */
const seeded = new WeakSet();

/**
 * Starts the mirror from a server-rendered document's `k-data` payload
 * (its `session`), once per payload: the router does at start, so its
 * guards see who is signed in, and so does the first `Session` reader; a
 * later reader of the same document never undoes an rpc's newer mirror.
 * @param {{session?: object}|null|undefined} data
 */
export function seedSession(data) {
  if (data?.session && !seeded.has(data)) {
    seeded.add(data);
    sessionMirror.set(data.session);
  }
}

/**
 * Runs with each mirror an rpc response brings, before it is set: the
 * router's check of the shown page's guards (router.js), so a page that
 * requires a user is left before anything reads the mirror without one.
 */
let sessionGate = null;

/**
 * Makes `fn` the check that runs with each mirror an rpc response brings,
 * before the mirror is set (the router's page guards).
 * @param {(mirror: {user: object|null}) => void} fn
 */
export function gateSession(fn) {
  sessionGate = fn;
}

/**
 * Sets the `Session` mirror from an rpc response's `x-k-session` header,
 * after the router's check of it ({@link gateSession}); a missing or
 * malformed header changes nothing.
 * @param {string|null|undefined} text the header's value
 */
function takeMirror(text) {
  if (!text) return;
  let mirror = null;
  try {
    mirror = JSON.parse(text);
  } catch {
    // A malformed mirror is ignored.
  }
  if (mirror) {
    sessionGate?.(mirror);
    sessionMirror.set(mirror);
  }
}

/**
 * Calls `path` with `message` (already encoded by the stub's codec).
 *
 * Resolves to `{ok: true, message}` on 2xx; `{ok: false, kind: "connect",
 * code, details, retryAfter}` for a Connect error body (`retryAfter` the
 * `Retry-After` seconds, 0 without one); `{ok: false,
 * kind: "offline"}` when the request fails or the body is unreadable;
 * `{ok: false, kind: "timeout"}` after `timeoutMs`. An aborted `signal`
 * makes the promise never settle. No retries.
 *
 * With `options.stream` (a server-streaming rpc's, from `streamed` in
 * `streams.js`) the request is `stream.body(message)` as
 * `application/connect+json`, and a 2xx resolves at once to `{ok: true,
 * message: stream.read(response, controller, signal)}`, the open stream;
 * the timeout covers only opening it (29-server-streaming.md).
 * @param {string} path `/package.Service/Method`
 * @param {unknown} message the JSON request
 * @param {AbortSignal|null} signal the caller's cancellation context
 * @param {{timeoutMs?: number, fetch?: typeof fetch, stream?: {body: Function, read: Function}}} [options]
 * @returns {Promise<object>}
 */
export function rpc(path, message, signal, options) {
  const doFetch = options?.fetch ?? globalThis.fetch;
  const timeoutMs = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const stream = options?.stream;
  if (signal && signal.aborted) return new Promise(() => {});
  return new Promise((resolve) => {
    let done = false;
    const controller = typeof AbortController === "function" ? new AbortController() : null;
    const onAbort = () => controller?.abort();
    // A settled call stops listening to the caller's signal, which (a
    // component's lifetime) outlives it; an opened stream keeps listening,
    // so leaving the page still closes it.
    const finish = (outcome) => {
      if (!(stream && outcome.ok)) signal?.removeEventListener?.("abort", onAbort);
      if (done || (signal && signal.aborted)) return;
      done = true;
      resolve(outcome);
    };
    if (signal) signal.addEventListener("abort", onAbort);
    const timer = typeof setTimeout === "function"
      ? setTimeout(() => {
        controller?.abort();
        finish({ ok: false, kind: "timeout" });
      }, timeoutMs)
      : null;
    const stop = () => {
      if (timer !== null) clearTimeout(timer);
    };
    const offline = () => {
      stop();
      finish({ ok: false, kind: "offline" });
    };
    let request;
    try {
      request = doFetch(path, {
        method: "POST",
        headers: { "Content-Type": stream ? "application/connect+json" : "application/json", "Connect-Protocol-Version": "1" },
        body: stream ? stream.body(message) : JSON.stringify(message ?? {}),
        signal: controller?.signal,
      });
    } catch {
      offline();
      return;
    }
    Promise.resolve(request).then(
      async (response) => {
        const header = (n) => response.headers?.get?.(n);
        if (stream && response.ok) {
          stop();
          // An opened stream can be the first response to learn of a dead
          // session (its head clears the cookie): its mirror counts too.
          takeMirror(header("x-k-session"));
          finish({ ok: true, message: stream.read(response, controller, signal) });
          return;
        }
        let body;
        try {
          body = JSON.parse((await response.text()) || "{}");
        } catch {
          offline();
          return;
        }
        stop();
        takeMirror(header("x-k-session"));
        if (response.ok) {
          finish({ ok: true, message: body });
        } else {
          finish({
            ok: false,
            kind: "connect",
            code: `${body?.code ?? "unknown"}`,
            details: Array.isArray(body?.details) ? body.details : [],
            retryAfter: +header("retry-after") || 0,
          });
        }
      },
      offline,
    );
  });
}

/**
 * Maps a failed rpc outcome to the rpc's typed error
 * (docs/compiler/codegen.md §15): a `failed_precondition` with a
 * `k.TypedError` detail is the case its `debug.case` names, decoded with
 * `codec` (a Codable error's) from `{case, ...debug.fields}`, or without
 * one built from its `debug.fields` in order (`E.case(...payload)`); a
 * detail the codec refuses is a panic; network
 * outcomes are `rpcFailure(.offline)` / `.timeout`; `invalid_argument` is
 * `rpcFailure(.invalid(ValidationError))` with the detail's failures; a
 * refused guard is `rpcFailure(.rejected(GuardRejection))` (`Retry-After`
 * as the `rateLimited` wait); anything else is a panic.
 * @param {object} o the outcome
 * @param {object|null} E the error enum's constructor object
 * @param {{decode: Function}} [codec] the error's codec, if it is Codable
 * @returns {unknown}
 */
export function rpcError(o, E, codec) {
  const bad = Error(`rpc failed: ${o.code ?? o.kind}`);
  // `rpcFailure` of `RpcFailure` case `tag`, in declaration order:
  // rejected, invalid, offline, timeout.
  const failure = (tag, _0 = null) => (E?.rpcFailure ? E.rpcFailure({ $: tag, _0 }) : bad);
  if (o.kind === "offline") return failure(2);
  if (o.kind === "timeout") return failure(3);
  // `{type: "k.TypedError", debug: {error, case, fields}}` (server.md §6).
  const find = (t) => o.details?.find((x) => x?.type === t)?.debug;
  const d = o.code === "failed_precondition" && E && find("k.TypedError");
  const c = d && E[d.case];
  if (c) {
    if (!codec) return typeof c === "function" ? c(...Object.values(d.fields ?? {})) : c;
    try {
      return codec.decode({ ...d.fields, case: d.case }, "error");
    } catch {
      return bad;
    }
  }
  if (o.code === "invalid_argument") return failure(1, { failures: find("k.ValidationError")?.failures ?? [] });
  // `GuardRejection`: unauthenticated, forbidden, rateLimited(retryAfter:).
  const r = ["unauthenticated", "permission_denied", "resource_exhausted"].indexOf(o.code);
  return r < 0 ? bad : failure(0, { $: r, retryAfter: (o.retryAfter ?? 0) * 1000 });
}
