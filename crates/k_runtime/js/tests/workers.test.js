/**
 * @module k:/tests/workers.test.js
 *
 * `@Worker` services (docs/design/55-workers.md; runtime.md §12.21): the page's `workerClient`
 * against a scripted `Worker` written from the HTML standard's dedicated workers (a worker posts
 * nothing until its module has run; `postMessage` copies; `terminate()` stops it at once and drops
 * what it would still post; `error` fires for a script that fails to load or an uncaught exception),
 * and the worker's `serveWorker` against a scripted worker scope. The generated proxy passes the
 * codecs; here they are small functions that tag what they decode.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { serveWorker, workerClient } from "k:/runtime/workers.js";
import { isPanic } from "k:runtime";

/** A scripted dedicated worker: records what the page posts; `ready()`, `answer(m)` and `fail()` play the worker's side. */
function fakeWorker() {
  return {
    posted: [],
    terminated: false,
    onmessage: null,
    onerror: null,
    onmessageerror: null,
    postMessage(data) {
      this.posted.push(JSON.parse(JSON.stringify(data)));
    },
    terminate() {
      this.terminated = true;
    },
    ready() {
      this.onmessage?.({ data: { k: "ready" } });
    },
    answer(m) {
      if (!this.terminated) this.onmessage?.({ data: m });
    },
    fail(message = "Uncaught Error: boom") {
      this.onerror?.({ message, preventDefault() {} });
    },
  };
}

/** An opener that makes a new fake worker each time and keeps them in `workers`. */
function opener() {
  const workers = [];
  const open = () => {
    const w = fakeWorker();
    workers.push(w);
    return w;
  };
  return { workers, open };
}

/** Starts a call and records how it settled in `outcome` (`null` while pending). */
function start(client, $c, m, a) {
  const outcome = { value: null };
  client
    .call(
      $c,
      m,
      a,
      (v) => ({ decoded: v }),
      (e) => ({ error: e }),
      (f) => ({ failure: f }),
    )
    .then(
      (v) => (outcome.value = { ok: v }),
      (e) => (outcome.value = { thrown: e }),
    );
  return outcome;
}

/** `console.error` silenced for `fn`, returning what it logged. */
async function quietly(fn) {
  const logged = [];
  const error = console.error;
  console.error = (...a) => logged.push(a.join(" "));
  try {
    await fn();
  } finally {
    console.error = error;
  }
  return logged;
}

test("the worker starts at the first call, which is posted once the worker is ready", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  assertEq(workers.length, 0, "nothing starts before a call");
  const call = start(client, null, "count", [100]);
  assertEq(workers.length, 1);
  assertDeepEq(workers[0].posted, [], "nothing is posted before the worker is ready");
  workers[0].ready();
  assertDeepEq(workers[0].posted, [{ k: "call", id: 1, m: "count", a: [100] }]);
  workers[0].answer({ k: "ok", id: 1, v: 25 });
  await tick();
  assertDeepEq(call.value, { ok: { decoded: 25 } }, "the result is decoded");
});

test("calls run one at a time, in the order they were made, on one worker", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  const first = start(client, null, "count", [10]);
  const second = start(client, null, "count", [20]);
  workers[0].ready();
  assertEq(workers[0].posted.length, 1, "the second waits for the first");
  workers[0].answer({ k: "ok", id: 1, v: 4 });
  assertDeepEq(
    workers[0].posted.map((p) => p.a),
    [[10], [20]],
  );
  workers[0].answer({ k: "ok", id: 2, v: 8 });
  await tick();
  assertDeepEq([first.value, second.value], [{ ok: { decoded: 4 } }, { ok: { decoded: 8 } }]);
  assertEq(workers.length, 1, "one worker for every call");
});

test("a typed error is thrown as its error codec decodes it, and the worker keeps running", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  const call = start(client, null, "count", [1e9]);
  workers[0].ready();
  workers[0].answer({ k: "error", id: 1, e: { case: "tooLarge", limit: 100 } });
  await tick();
  assertDeepEq(call.value, { thrown: { error: { case: "tooLarge", limit: 100 } } });
  start(client, null, "count", [5]);
  assertEq(workers[0].terminated, false);
  assertEq(workers[0].posted.length, 2, "the next call goes to the same worker");
});

test("a queued call that is cancelled leaves the queue and never settles", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  const running = start(client, null, "count", [10]);
  const c = new AbortController();
  const queued = start(client, c.signal, "count", [20]);
  workers[0].ready();
  c.abort();
  workers[0].answer({ k: "ok", id: 1, v: 4 });
  await tick();
  assertDeepEq(running.value, { ok: { decoded: 4 } });
  assertEq(queued.value, null, "a cancelled call never settles");
  assertEq(workers[0].posted.length, 1, "the worker never saw the cancelled call");
  assertEq(workers[0].terminated, false, "cancelling a queued call leaves the worker alone");
});

test("a running call that is cancelled terminates the worker; the next call starts a new one", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  const c = new AbortController();
  const long = start(client, c.signal, "count", [3e7]);
  const next = start(client, null, "count", [100]);
  workers[0].ready();
  c.abort();
  assertEq(workers[0].terminated, true, "the only way to stop a synchronous computation");
  assertEq(workers.length, 2, "the queued call starts a new worker");
  workers[0].answer({ k: "ok", id: 1, v: 1857859 });
  workers[1].ready();
  assertDeepEq(workers[1].posted, [{ k: "call", id: 2, m: "count", a: [100] }]);
  workers[1].answer({ k: "ok", id: 2, v: 25 });
  await tick();
  assertEq(long.value, null, "the cancelled call never settles, even if its old worker answers");
  assertDeepEq(next.value, { ok: { decoded: 25 } });
});

test("a call made with a signal already aborted never starts a worker", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  const c = new AbortController();
  c.abort();
  const call = start(client, c.signal, "count", [1]);
  await tick();
  assertEq(workers.length, 0);
  assertEq(call.value, null);
});

test("a crash fails the running call with crashed, is logged, and the next call starts a new worker", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  let crashed = null;
  let next = null;
  const logged = await quietly(async () => {
    crashed = start(client, null, "crash", [3]);
    next = start(client, null, "count", [10]);
    workers[0].ready();
    workers[0].answer({ k: "crash", id: 1, text: "index 3 out of range" });
    await tick();
  });
  assertDeepEq(crashed.value, { thrown: { failure: "crashed" } });
  assert(logged.some((l) => l.includes("K worker Sieve crashed: index 3 out of range")), logged.join("\n"));
  assertEq(workers[0].terminated, true);
  assertEq(workers.length, 2, "the queued call goes on in a new worker");
  workers[1].ready();
  workers[1].answer({ k: "ok", id: 2, v: 4 });
  await tick();
  assertDeepEq(next.value, { ok: { decoded: 4 } });
});

test("an error event after the worker is ready is a crash of the running call", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  let call = null;
  await quietly(async () => {
    call = start(client, null, "count", [10]);
    workers[0].ready();
    workers[0].fail("out of memory");
    await tick();
  });
  assertDeepEq(call.value, { thrown: { failure: "crashed" } });
  assertEq(workers[0].terminated, true);
});

test("a worker that fails before it is ready fails every queued call with unavailable; the next call tries again", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  const a = start(client, null, "count", [1]);
  const b = start(client, null, "count", [2]);
  workers[0].fail("Failed to load module script");
  await tick();
  assertDeepEq([a.value, b.value], [{ thrown: { failure: "unavailable" } }, { thrown: { failure: "unavailable" } }]);
  start(client, null, "count", [3]);
  assertEq(workers.length, 2, "a later call starts a worker again");
});

test("no Worker at all (the opener throws) is unavailable", async () => {
  const client = workerClient(() => {
    throw new ReferenceError("Worker is not defined");
  }, "Sieve");
  const call = start(client, null, "count", [1]);
  await tick();
  assertDeepEq(call.value, { thrown: { failure: "unavailable" } });
});

test("an answer the codecs can't read is a panic", async () => {
  const { workers, open } = opener();
  const client = workerClient(open, "Sieve");
  const outcome = { value: null };
  client
    .call(
      null,
      "count",
      [1],
      () => {
        throw new Error("expected an integer");
      },
      (e) => e,
      (f) => f,
    )
    .catch((e) => (outcome.value = e));
  workers[0].ready();
  workers[0].answer({ k: "ok", id: 1, v: "x" });
  await tick();
  assert(isPanic(outcome.value), String(outcome.value));
});

// ------------------------------------------------------------- the worker

/** A scripted worker scope: what the host posts, and `call(m, a)` posting a call to it. */
function fakeScope() {
  const scope = {
    posted: [],
    onmessage: null,
    postMessage(m) {
      this.posted.push(m);
    },
  };
  let id = 0;
  scope.call = async (m, a) => {
    await scope.onmessage({ data: { k: "call", id: ++id, m, a } });
  };
  return scope;
}

test("the worker announces itself, builds the service at the first call and answers with its codecs", async () => {
  const scope = fakeScope();
  let built = 0;
  const make = () => {
    built++;
    return {
      async double(_c, n) {
        return n * 2;
      },
    };
  };
  serveWorker(scope, make, { double: [(a) => [a[0] + 0], (v) => ({ encoded: v }), (e) => e] });
  assertDeepEq(scope.posted, [{ k: "ready" }], "ready once its module has run");
  assertEq(built, 0, "the service is built at the first call");
  await scope.call("double", [4]);
  await scope.call("double", [5]);
  assertEq(built, 1);
  assertDeepEq(scope.posted.slice(1), [
    { k: "ok", v: { encoded: 8 }, id: 1 },
    { k: "ok", v: { encoded: 10 }, id: 2 },
  ]);
});

test("a thrown K error is encoded; a panic or another exception is a crash with its text", async () => {
  const scope = fakeScope();
  const tooLarge = { $: 0, limit: 100 };
  const service = {
    async refuse() {
      throw tooLarge;
    },
    async panic() {
      throw new Error("index 3 out of range");
    },
  };
  serveWorker(scope, () => service, {
    refuse: [() => [], (v) => v, (e) => ({ case: "tooLarge", limit: e.limit })],
    panic: [() => [], (v) => v, (e) => e],
  });
  await scope.call("refuse", []);
  await scope.call("panic", []);
  assertDeepEq(scope.posted.slice(1), [
    { k: "error", e: { case: "tooLarge", limit: 100 }, id: 1 },
    { k: "crash", text: "index 3 out of range", id: 2 },
  ]);
});

test("the page and the worker work together through a pair of ports", async () => {
  const pair = () => {
    const page = { onmessage: null, onerror: null, terminate() {} };
    const scope = { onmessage: null };
    page.postMessage = (d) => queueMicrotask(() => scope.onmessage({ data: structuredCopy(d) }));
    scope.postMessage = (d) => queueMicrotask(() => page.onmessage({ data: structuredCopy(d) }));
    serveWorker(scope, () => ({ async add(_c, a, b) {
      return a + b;
    } }), { add: [(a) => a, (v) => v, (e) => e] });
    return page;
  };
  const client = workerClient(pair, "Adder");
  const sum = await client.call(null, "add", [2, 3], (v) => v, (e) => e, (f) => f);
  assertEq(sum, 5);
});

/** A copy of a message, as structured clone makes one. */
function structuredCopy(v) {
  return JSON.parse(JSON.stringify(v));
}
