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

/**
 * Calls `path` with `message` (already encoded by the stub's codec).
 *
 * Resolves to `{ok: true, message}` on 2xx; `{ok: false, kind: "connect",
 * code, details, retryAfter}` for a Connect error body (`retryAfter` the
 * `Retry-After` seconds, 0 without one); `{ok: false,
 * kind: "offline"}` when the request fails or the body is unreadable;
 * `{ok: false, kind: "timeout"}` after `timeoutMs`. An aborted `signal`
 * makes the promise never settle. No retries.
 * @param {string} path `/package.Service/Method`
 * @param {unknown} message the JSON request
 * @param {AbortSignal|null} signal the caller's cancellation context
 * @param {{timeoutMs?: number, fetch?: typeof fetch}} [options]
 * @returns {Promise<object>}
 */
export function rpc(path, message, signal, options) {
  const doFetch = options?.fetch ?? globalThis.fetch;
  const timeoutMs = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  if (signal && signal.aborted) return new Promise(() => {});
  return new Promise((resolve) => {
    let done = false;
    const finish = (outcome) => {
      if (done || (signal && signal.aborted)) return;
      done = true;
      resolve(outcome);
    };
    const controller = typeof AbortController === "function" ? new AbortController() : null;
    if (signal) signal.addEventListener("abort", () => controller?.abort());
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
        headers: { "Content-Type": "application/json", "Connect-Protocol-Version": "1" },
        body: JSON.stringify(message ?? {}),
        signal: controller?.signal,
      });
    } catch {
      offline();
      return;
    }
    Promise.resolve(request).then(
      async (response) => {
        let body;
        try {
          body = JSON.parse((await response.text()) || "{}");
        } catch {
          offline();
          return;
        }
        stop();
        const header = (n) => response.headers?.get?.(n);
        const sessionHeader = header("x-k-session");
        if (sessionHeader) {
          try {
            sessionMirror.set(JSON.parse(sessionHeader));
          } catch {
            // A malformed mirror is ignored.
          }
        }
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
 * `k.TypedError` detail is the case its `debug.case` names, built from its
 * `debug.fields` in order (`E.case(...payload)`); network
 * outcomes are `rpcFailure(.offline)` / `.timeout`; `invalid_argument` is
 * `rpcFailure(.invalid(ValidationError))` with the detail's failures; a
 * refused guard is `rpcFailure(.rejected(GuardRejection))` (`Retry-After`
 * as the `rateLimited` wait); anything else is a panic.
 * @param {object} o the outcome
 * @param {object|null} E the error enum's constructor object
 * @returns {unknown}
 */
export function rpcError(o, E) {
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
  if (c) return typeof c === "function" ? c(...Object.values(d.fields ?? {})) : c;
  if (o.code === "invalid_argument") return failure(1, { failures: find("k.ValidationError")?.failures ?? [] });
  // `GuardRejection`: unauthenticated, forbidden, rateLimited(retryAfter:).
  const r = ["unauthenticated", "permission_denied", "resource_exhausted"].indexOf(o.code);
  return r < 0 ? bad : failure(0, { $: r, retryAfter: (o.retryAfter ?? 0) * 1000 });
}
