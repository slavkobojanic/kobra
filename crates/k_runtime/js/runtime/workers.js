/**
 * @module k:/runtime/workers.js
 *
 * `@Worker` services (docs/design/55-workers.md; runtime.md §12.21): the page's side, `workerClient`,
 * which the generated proxy class of a worker service calls, and the worker's side, `serveWorker`,
 * which a worker chunk calls with its global scope. Shipped as a chunk of its own, only to apps with a
 * `@Worker` service, so the runtime chunk doesn't grow.
 *
 * The protocol: the page posts `{k: "call", id, m, a}` (the method's name and its encoded arguments);
 * the worker posts `{k: "ready"}` once its module has run, then for each call `{k: "ok", id, v}`, `{k:
 * "error", id, e}` (a typed error, encoded) or `{k: "crash", id, text}` (a panic, or an exception that
 * isn't a K error). Values are what the codecs make (JSON values, and top-level `Bytes` as
 * `Uint8Array`), so structured clone copies them exactly.
 */
import { KPanic } from "./values.js";

/**
 * The page's side of one worker service: one worker, started at the first call; calls queue in order
 * and run one at a time. A queued call that is cancelled leaves the queue; a running one terminates
 * the worker, and the next call starts a new one. A cancelled call never settles. A worker that fails
 * to load fails every queued call with `failure("unavailable")`; one that crashes fails the running
 * call with `failure("crashed")` and is replaced for the queued ones.
 *
 * @param {() => Worker} open starts the worker (`new Worker(url, {type: "module", name})`; in `k test`
 *   an in-process host)
 * @param {string} name the service's name, for the console
 * @returns {{call: Function}}
 */
export function workerClient(open, name) {
  /** @type {any} */
  let w = null;
  let ready = false;
  /** @type {any} */
  let current = null;
  /** @type {any[]} */
  const queue = [];
  let seq = 0;

  const done = (c) => c.$c?.removeEventListener("abort", c.abort);
  const fail = (c, why) => (done(c), c.reject(c.failure(why)));
  const kill = () => {
    w?.terminate();
    w = null;
    ready = false;
  };
  const start = () => {
    let next;
    try {
      next = open();
    } catch {
      for (const c of queue.splice(0)) fail(c, "unavailable");
      return;
    }
    w = next;
    next.onmessage = (e) => receive(next, e.data);
    next.onerror = (e) => {
      e?.preventDefault?.();
      lost(next, e?.message);
    };
    next.onmessageerror = () => lost(next, "a message couldn't be read");
  };
  const pump = () => {
    if (current || !queue.length) return;
    if (!w) start();
    if (!w || !ready) return;
    current = queue.shift();
    try {
      w.postMessage({ k: "call", id: current.id, m: current.m, a: current.a });
    } catch (e) {
      crash(String(e));
    }
  };
  const crash = (text) => {
    const c = current;
    current = null;
    kill();
    console.error(`K worker ${name} crashed: ${text}`);
    if (c) fail(c, "crashed");
    pump();
  };
  const lost = (from, text) => {
    if (from !== w) return;
    if (ready) return crash(text ?? "the worker stopped");
    kill();
    for (const c of queue.splice(0)) fail(c, "unavailable");
  };
  const receive = (from, d) => {
    if (from !== w) return;
    if (d?.k === "ready") {
      ready = true;
      return pump();
    }
    const c = current;
    if (!c || d?.id !== c.id) return;
    if (d.k === "crash") return crash(d.text);
    current = null;
    done(c);
    try {
      if (d.k === "ok") c.resolve(c.decode(d.v));
      else c.reject(c.decodeError(d.e));
    } catch (e) {
      c.reject(new KPanic(`K worker ${name} answered what its codecs can't read: ${e?.message ?? e}`));
    }
    pump();
  };
  const cancel = (c) => {
    done(c);
    if (current === c) {
      current = null;
      kill();
      return pump();
    }
    const i = queue.indexOf(c);
    if (i >= 0) queue.splice(i, 1);
  };

  return {
    /**
     * Calls method `m` with the encoded arguments `a`; resolves with `decode(v)`, rejects with
     * `decodeError(e)` or `failure("crashed" | "unavailable")`, and never settles once `$c` aborts.
     */
    call($c, m, a, decode, decodeError, failure) {
      return new Promise((resolve, reject) => {
        if ($c?.aborted) return;
        const c = { id: ++seq, m, a, resolve, reject, decode, decodeError, failure, $c };
        c.abort = () => cancel(c);
        $c?.addEventListener("abort", c.abort, { once: true });
        queue.push(c);
        pump();
      });
    },
  };
}

/**
 * The worker's side: answers calls on `scope` (the worker's global scope) with the service `make()`
 * builds at the first call. `host` maps each method to `[decodeArguments, encodeResult,
 * encodeError]`. A thrown K error (an enum value) is encoded; a panic or any other exception is a
 * crash, reported with its text.
 *
 * @param {any} scope `globalThis` in the worker, or the in-process host's scope in `k test`
 * @param {() => any} make builds the service
 * @param {Record<string, [Function, Function, Function]>} host
 */
export function serveWorker(scope, make, host) {
  let service = null;
  scope.onmessage = async (event) => {
    const d = event.data;
    if (d?.k !== "call") return;
    const post = (m) => scope.postMessage({ ...m, id: d.id });
    try {
      const [decode, encode, encodeError] = host[d.m];
      service ??= make();
      let v;
      try {
        v = await service[d.m](null, ...decode(d.a));
      } catch (e) {
        if (e === null || typeof e !== "object" || e instanceof Error) throw e;
        return post({ k: "error", e: encodeError(e) });
      }
      post({ k: "ok", v: encode(v) });
    } catch (e) {
      post({ k: "crash", text: e?.message ?? String(e) });
    }
  };
  scope.postMessage({ k: "ready" });
}
