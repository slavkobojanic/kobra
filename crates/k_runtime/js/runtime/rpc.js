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
 * status, code, message, details}` for a Connect error body; `{ok: false,
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
    let request;
    try {
      request = doFetch(path, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Connect-Protocol-Version": "1" },
        body: JSON.stringify(message ?? {}),
        signal: controller?.signal,
      });
    } catch {
      stop();
      finish({ ok: false, kind: "offline" });
      return;
    }
    Promise.resolve(request).then(
      async (response) => {
        stop();
        let body = null;
        try {
          const textBody = await response.text();
          body = textBody === "" ? {} : JSON.parse(textBody);
        } catch {
          finish({ ok: false, kind: "offline" });
          return;
        }
        const sessionHeader = response.headers && typeof response.headers.get === "function" ? response.headers.get("x-k-session") : null;
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
            status: response.status,
            code: body && typeof body.code === "string" ? body.code : "unknown",
            message: body && typeof body.message === "string" ? body.message : "",
            details: body && Array.isArray(body.details) ? body.details : [],
          });
        }
      },
      () => {
        stop();
        finish({ ok: false, kind: "offline" });
      },
    );
  });
}

/**
 * Maps a failed rpc outcome to the rpc's typed error
 * (docs/compiler/codegen.md §15): a `failed_precondition` with a
 * `k.TypedError` detail is the named case (`E.case(...payload)`); network
 * outcomes are `rpcFailure(.offline)` / `.timeout`; other codes are
 * `rpcFailure(.rejected)`; anything else is a panic.
 * @param {object} o the outcome
 * @param {object|null} E the error enum's constructor object
 * @returns {unknown}
 */
export function rpcError(o, E) {
  const failure = (name, ...args) => {
    const f = E && E.rpcFailure;
    if (typeof f !== "function") return new Error(`rpc failed: ${name}`);
    return f({ $: ["offline", "timeout", "rejected", "invalid"].indexOf(name), _0: args[0] ?? null });
  };
  if (o.kind === "offline") return failure("offline");
  if (o.kind === "timeout") return failure("timeout");
  if (o.code === "failed_precondition" && E) {
    const detail = (o.details ?? []).find((d) => d && d.type === "k.TypedError");
    const value = detail ? detail.value : null;
    if (value && typeof value.case === "string" && E[value.case] !== undefined) {
      const c = E[value.case];
      if (typeof c !== "function") return c;
      const fields = Object.keys(value).filter((k) => k !== "case");
      return c(...fields.map((k) => value[k]));
    }
  }
  if (o.code === "invalid_argument") return failure("invalid", o.details);
  if (["unauthenticated", "permission_denied", "resource_exhausted"].includes(o.code)) return failure("rejected", o.code);
  return new Error(`rpc failed: ${o.code ?? o.kind}`);
}
