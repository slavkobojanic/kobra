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

// ------------------------------------------- what a component leaves behind

// A disposed component's handlers do nothing. Its nodes can outlive it: a
// page the session gate disposed stays on screen until the full page load
// the router left for replaces the document, and a click on it must not
// run a body that reads the narrowed user (03-reactivity.md, "What a
// component leaves behind").
test("a handler of a disposed owner does nothing", () => {
  const button = globalThis.document.createElement("button");
  globalThis.document.body.appendChild(button);
  let calls = 0;
  let dispose = null;
  root((d) => {
    dispose = d;
    on(button, "click", () => {
      calls++;
    });
  });
  button.click();
  assertEq(calls, 1);
  dispose();
  button.click();
  assertEq(calls, 1, "the disposed owner's handler did not run");
});

// A synchronous handler stopped by the cancellation (a bound function of
// another disposed component returning a value) is as silent as an async
// one.
test("a handler that throws the cancellation is silent", () => {
  const button = globalThis.document.createElement("button");
  globalThis.document.body.appendChild(button);
  let calls = 0;
  root(() =>
    on(button, "click", () => {
      calls++;
      throw k.CANCELLED;
    }),
  );
  button.click();
  assertEq(calls, 1);
});

// A function value a component hands to a service (a listener it keeps,
// a timer's action) is bound to the component's lifetime: it is the
// function while the component is live.
test("bound runs the function while its lifetime is live", async () => {
  assertEq(typeof k.bound, "function", "k:runtime exports bound");
  const life = new AbortController();
  const got = [];
  const f = k.bound((a, b) => (got.push([a, b]), a + b), life.signal, 1);
  assertEq(f(2, 3), 5);
  const caller = new AbortController();
  const g = k.bound(async ($c, a) => {
    got.push([$c.aborted, a]);
    return a * 2;
  }, life.signal, 3);
  assertEq(await g(caller.signal, 4), 8);
  assertEq(await g(null, 5), 10, "a caller without a context runs it under the lifetime");
  assertDeepEq(got, [[2, 3], [false, 4], [false, 5]]);
  assertEq(f.lifetime, life.signal, "the bound value names its lifetime");
});

// Once the component is disposed, calling the value runs none of its
// code: a function of `Void` returns at once (an async one resolves), and
// one returning a value stops its caller with the cancellation, as
// awaiting the component's own work does.
test("bound runs nothing once its lifetime is aborted", async () => {
  const life = new AbortController();
  let calls = 0;
  const sync = k.bound(() => { calls++; }, life.signal, 0);
  const value = k.bound(() => (calls++, 1), life.signal, 1);
  const later = k.bound(async () => { calls++; }, life.signal, 2);
  const answer = k.bound(async () => (calls++, 1), life.signal, 3);
  life.abort();
  assertEq(sync(), undefined);
  let thrown = null;
  try {
    value();
  } catch (e) {
    thrown = e;
  }
  assertEq(thrown, k.CANCELLED);
  assertEq(await later(null), undefined);
  let rejected = null;
  await answer(null).then(undefined, (e) => { rejected = e; });
  assertEq(rejected, k.CANCELLED);
  assertEq(calls, 0);
});

// An async bound value started while the component is live runs under a
// context that its caller's cancellation and the component's both abort,
// so the body stops at its next await when the component goes away.
test("a bound async value stops when its lifetime ends mid-run", async () => {
  const life = new AbortController();
  const caller = new AbortController();
  const gate = deferred();
  const seen = [];
  const f = k.bound(async ($c) => {
    seen.push("start");
    k.live($c, await k.guard($c, gate.promise));
    seen.push("after");
  }, life.signal, 2);
  f(caller.signal);
  await tick();
  life.abort();
  gate.resolve(1);
  await tick();
  await tick();
  assertDeepEq(seen, ["start"], "the caller is live, the component is not");
  assertEq(caller.signal.aborted, false);
});

// Without a lifetime (outside a component's client renderer) the value is
// the function itself.
test("bound without a lifetime is the function", () => {
  const f = () => 1;
  assertEq(k.bound(f, null, 1), f);
});

// The browser Clock stops a timer whose action is bound to a lifetime
// when that lifetime ends, so a page's timers stop with it.
test("a Clock timer whose action is bound stops with its lifetime", () => {
  const calls = [];
  const saved = [globalThis.setTimeout, globalThis.setInterval, globalThis.clearTimeout, globalThis.clearInterval];
  globalThis.setTimeout = (f, d) => (calls.push(["timeout", d]), 1);
  globalThis.setInterval = (f, d) => (calls.push(["interval", d]), 2);
  globalThis.clearTimeout = (id) => calls.push(["clearTimeout", id]);
  globalThis.clearInterval = (id) => calls.push(["clearInterval", id]);
  try {
    const life = new AbortController();
    const c = k.clock(null);
    c.after(300, k.bound(() => {}, life.signal, 0));
    c.every(3000, k.bound(() => {}, life.signal, 0));
    c.after(500, () => {});
    life.abort();
  } finally {
    [globalThis.setTimeout, globalThis.setInterval, globalThis.clearTimeout, globalThis.clearInterval] = saved;
  }
  assertDeepEq(calls, [["timeout", 300], ["interval", 3000], ["timeout", 500], ["clearTimeout", 1], ["clearInterval", 2]]);
});

// A live run that calls a disposed component's bound function for a value
// is stopped by the cancellation, and that is silent wherever the run
// ends: an effect (sync or async) reports nothing to its error boundary,
// and a load is not a failure.
test("a live run stopped by a disposed component's function reports nothing", async () => {
  const life = new AbortController();
  life.abort();
  const stale = k.bound(() => 1, life.signal, 1);
  const staleAsync = k.bound(async () => 1, life.signal, 3);
  const errors = [];
  let group = null;
  root(() =>
    catchError(
      () => {
        effect(() => {
          stale();
        });
        effect(async ($c) => {
          k.live($c, await k.guard($c, staleAsync($c)));
        });
        group = createLoads([{ name: "n", after: [], run: async (v, $c) => k.live($c, await k.guard($c, staleAsync($c))) }], null);
      },
      (e) => errors.push(e),
    ),
  );
  flush();
  await tick();
  await tick();
  assertDeepEq(errors, []);
  assertEq(group.state(), k.LOADING);
  assertEq(group.error(), null);
});
