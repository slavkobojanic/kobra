/**
 * @module k:/tests/cancellation.test.js
 *
 * Work a component or page starts is owned by it and stops when it is
 * disposed (docs/design/03-reactivity.md, "Async work is owned";
 * docs/compiler/codegen.md §8; runtime.md §3.4). Generated code passes the
 * owner's `AbortSignal` (`$c`) to every async call, guards every `await`
 * (`guard`) and checks the context again when the body resumes (`live`),
 * and when a `catch` is entered: a promise can settle while the owner is
 * still live and the owner be disposed before the body resumes (one
 * microtask later), so the check at resumption is what keeps code after an
 * `await` from running on a disposed page. The bodies below are written
 * the way the compiler prints them.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import * as k from "k:runtime";
import { catchError, createLoads, effect, flush, lifetime, on, root, signal } from "k:runtime";

/** A promise and the functions that settle it. */
function deferred() {
  let resolve = null;
  let reject = null;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}

// ------------------------------------------------------------------ live

// `live` is the value an await resumed with while the context is live, and
// with no context (outside any cancellable run).
test("live passes the value through while the context is live", () => {
  assertEq(typeof k.live, "function", "k:runtime exports live");
  const c = new AbortController();
  assertEq(k.live(c.signal, 7), 7);
  assertEq(k.live(null, "x"), "x");
  assertEq(k.live(c.signal), undefined, "the check of a catch takes no value");
});

// Once the context is aborted, `live` throws the cancellation: a panic, so
// every generated `catch` rethrows it before any K code runs, and the same
// value every time, so the runtime can tell it from a failure.
test("live throws the cancellation once the context is aborted", () => {
  const c = new AbortController();
  c.abort();
  let thrown = null;
  try {
    k.live(c.signal, 1);
  } catch (e) {
    thrown = e;
  }
  assert(thrown !== null, "live stopped the body");
  assertEq(thrown, k.CANCELLED);
  assert(k.isPanic(thrown), "K catches rethrow it");
});

// ------------------------------------------------- bodies and their owners

// The awaited promise settles while the component is live, and the
// component is disposed before the body resumes: the body stops at the
// `await` and nothing after it runs.
test("a body whose owner is disposed while its await resumes stops there", async () => {
  const d = deferred();
  const ran = [];
  let dispose = null;
  let body = null;
  root((x) => {
    dispose = x;
    const $life = lifetime();
    body = (async ($c) => {
      const v = k.live($c, await k.guard($c, d.promise));
      ran.push(`after ${v}`);
    })($life);
  });
  d.resolve(1);
  queueMicrotask(() => dispose());
  let outcome = null;
  await body.then(() => { outcome = "finished"; }, (e) => { outcome = e; });
  assertDeepEq(ran, [], "no code after the await ran on the disposed component");
  assertEq(outcome, k.CANCELLED, "the body unwound with the cancellation");
});

// The same for a rejection: the `catch` it enters stops before the K code
// in it runs, so an error handler never writes the state of a disposed
// component.
test("a catch entered after the owner was disposed stops before its body", async () => {
  const d = deferred();
  const ran = [];
  let dispose = null;
  let body = null;
  root((x) => {
    dispose = x;
    const $life = lifetime();
    body = (async ($c) => {
      try {
        k.live($c, await k.guard($c, d.promise));
        ran.push("after");
      } catch (e) {
        if (k.isPanic(e)) throw e;
        k.live($c);
        ran.push(`caught ${e.$}`);
      }
    })($life);
  });
  d.reject({ $: 0 });
  queueMicrotask(() => dispose());
  await body.then(() => {}, () => {});
  assertDeepEq(ran, []);
});

// While the owner stays live, typed errors still reach the `catch`.
test("a live body still catches its typed errors", async () => {
  const ran = [];
  await root(() => {
    const $life = lifetime();
    return (async ($c) => {
      try {
        k.live($c, await k.guard($c, Promise.reject({ $: 2 })));
      } catch (e) {
        if (k.isPanic(e)) throw e;
        k.live($c);
        ran.push(`caught ${e.$}`);
      }
    })($life);
  });
  assertDeepEq(ran, ["caught 2"]);
});

// ------------------------------------------------------------- effects

// A dependency changes while an async effect's run is awaiting: the
// superseded run stops even when its promise settles first, and only the
// newest run writes.
test("an effect re-run stops the previous run at its await", async () => {
  const id = signal(1);
  const pending = new Map();
  const written = [];
  root(() =>
    effect(async ($c) => {
      const want = id.get();
      const d = deferred();
      pending.set(want, d);
      const v = k.live($c, await k.guard($c, d.promise));
      written.push(v);
    }),
  );
  flush();
  // The first run's request answers, and before the run resumes the id
  // changes: the re-run aborts it.
  pending.get(1).resolve("profile 1");
  queueMicrotask(() => {
    id.set(2);
    flush();
  });
  await tick();
  pending.get(2).resolve("profile 2");
  await tick();
  assertDeepEq(written, ["profile 2"]);
});

// A run stopped by its cancellation is not a failure: the error boundary
// never sees it.
test("a stopped effect run never reaches the error boundary", async () => {
  const id = signal(1);
  const errors = [];
  const pending = [];
  root(() =>
    catchError(
      () =>
        effect(async ($c) => {
          id.get();
          const d = deferred();
          pending.push(d);
          k.live($c, await k.guard($c, d.promise));
        }),
      (e) => errors.push(e),
    ),
  );
  flush();
  pending[0].resolve(0);
  queueMicrotask(() => {
    id.set(2);
    flush();
  });
  await tick();
  assertDeepEq(errors, []);
});

// ----------------------------------------------------------------- loads

// A load run stopped by the unmount of its declaration leaves the group as
// it was: not `FAILED`, no error.
test("a stopped load run is not a failure", async () => {
  const d = deferred();
  let group = null;
  let dispose = null;
  root((x) => {
    dispose = x;
    group = createLoads([{ name: "note", after: [], run: async (v, $c) => k.live($c, await k.guard($c, d.promise)) }], null);
  });
  d.resolve("note");
  queueMicrotask(() => dispose());
  await tick();
  assertEq(group.state(), k.LOADING);
  assertEq(group.error(), null);
});

// -------------------------------------------------------------- handlers

// Nothing awaits an async handler, so a handler stopped by its
// cancellation would end in an unhandled rejection: `on` keeps it silent.
// (An unhandled rejection fails the test isolate.)
test("a stopped async handler is silent", async () => {
  const button = globalThis.document.createElement("button");
  globalThis.document.body.appendChild(button);
  let calls = 0;
  on(button, "click", () => {
    calls++;
    return Promise.reject(k.CANCELLED);
  });
  button.click();
  await tick();
  assertEq(calls, 1);
});

// A component's async function, or an async lambda written in one, used as
// a value (a prop, an event handler, `form.handle`'s action) is called with
// its K arguments alone and runs under the context it was bound to.
test("own calls the function with its context and passes its outcome on", async () => {
  assertEq(typeof k.own, "function", "k:runtime exports own");
  const c = new AbortController();
  const got = [];
  const f = k.own(async ($c, a, b) => {
    got.push([$c === c.signal, a, b]);
    return a + b;
  }, c.signal);
  assertEq(await f(2, 3), 5);
  assertDeepEq(got, [[true, 2, 3]]);
  let caught = null;
  try {
    await k.own(async () => { throw { $: 1 }; }, c.signal)();
  } catch (e) {
    caught = e;
  }
  assertDeepEq(caught, { $: 1 }, "a typed error reaches the caller that awaits it");
});

// A bound value stopped by its cancellation never settles instead of
// rejecting, so a caller that drops its promise (a child's `emit`) reports
// nothing, and one that awaits it stops there too.
test("a stopped bound value never settles", async () => {
  const c = new AbortController();
  c.abort();
  let settled = false;
  const f = k.own(async ($c) => k.live($c, await null), c.signal);
  f();
  f().then(() => { settled = true; }, () => { settled = true; });
  await tick();
  assertEq(settled, false);
});
