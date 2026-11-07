// Signals: reads, writes, computed laziness and cutoff, effect scheduling,
// batching, untracked reads, cleanups, the owner tree, async cancellation
// and error boundaries (docs/compiler/runtime.md, "Signals").
import { assert, assertDeepEq, assertEq, assertThrows, test, tick } from "k:testing";
import {
  batch, catchError, computed, effect, flush, isTracking, onCleanup, renderEffect, root, scope, signal, untrack,
} from "k:runtime";

test("a signal returns its initial value", () => {
  assertEq(signal(1).get(), 1);
  assertEq(signal(null).get(), null);
});

test("set replaces the value and update applies a function", () => {
  const s = signal(1);
  s.set(2);
  assertEq(s.get(), 2);
  s.update((v) => v * 10);
  assertEq(s.peek(), 20);
});

test("a computed value is lazy: it runs on first read only", () => {
  let runs = 0;
  const c = computed(() => ++runs);
  assertEq(runs, 0);
  assertEq(c.get(), 1);
  assertEq(c.get(), 1);
  assertEq(runs, 1);
});

test("a computed value recomputes only after a source changed", () => {
  const a = signal(1);
  let runs = 0;
  const c = computed(() => { runs++; return a.get() * 2; });
  assertEq(c.get(), 2);
  a.set(1);
  assertEq(c.get(), 2);
  assertEq(runs, 1, "setting an equal value notifies nobody");
  a.set(3);
  assertEq(runs, 1, "still lazy after the write");
  assertEq(c.get(), 6);
  assertEq(runs, 2);
});

test("an unchanged intermediate value stops propagation", () => {
  const a = signal(1);
  const parity = computed(() => a.get() % 2);
  let runs = 0;
  const label = computed(() => { runs++; return parity.get() === 0 ? "even" : "odd"; });
  assertEq(label.get(), "odd");
  a.set(3);
  assertEq(label.get(), "odd");
  assertEq(runs, 1, "parity stayed 1, so label did not rerun");
});

test("a diamond recomputes its bottom once per change", () => {
  const a = signal(1);
  const b = computed(() => a.get() + 1);
  const c = computed(() => a.get() * 2);
  let runs = 0;
  const d = computed(() => { runs++; return b.get() + c.get(); });
  assertEq(d.get(), 4);
  a.set(2);
  assertEq(d.get(), 7);
  assertEq(runs, 2);
});

test("conditional reads are tracked exactly", () => {
  const flag = signal(true);
  const x = signal("x");
  const y = signal("y");
  let runs = 0;
  const c = computed(() => { runs++; return flag.get() ? x.get() : y.get(); });
  assertEq(c.get(), "x");
  y.set("y2");
  assertEq(c.get(), "x");
  assertEq(runs, 1, "y is not a dependency while flag is true");
  flag.set(false);
  assertEq(c.get(), "y2");
  x.set("x2");
  assertEq(c.get(), "y2");
  assertEq(runs, 2, "x stopped being a dependency");
});

test("a computed value that throws rethrows to each reader and retries", () => {
  const a = signal(0);
  const c = computed(() => { if (a.get() === 0) throw new Error("zero"); return 10 / a.get(); });
  assertThrows(() => c.get(), "zero");
  assertThrows(() => c.get(), "zero");
  a.set(2);
  assertEq(c.get(), 5);
});

test("a render effect runs immediately and re-runs after a write, in a microtask", async () => {
  const a = signal(1);
  const seen = [];
  root(() => renderEffect(() => seen.push(a.get())));
  assertDeepEq(seen, [1]);
  a.set(2);
  assertDeepEq(seen, [1], "not synchronous outside batch");
  await tick();
  assertDeepEq(seen, [1, 2]);
});

test("writes outside a batch coalesce into one effect run per microtask", async () => {
  const a = signal(1);
  const b = signal(1);
  let runs = 0;
  root(() => renderEffect(() => { a.get(); b.get(); runs++; }));
  a.set(2);
  b.set(2);
  a.set(3);
  await tick();
  assertEq(runs, 2);
});

test("a user effect's first run waits for the flush", async () => {
  const seen = [];
  root(() => effect(() => seen.push("ran")));
  assertDeepEq(seen, []);
  await tick();
  assertDeepEq(seen, ["ran"]);
});

test("batch flushes synchronously when the outermost batch ends", () => {
  const a = signal(1);
  const seen = [];
  root(() => renderEffect(() => seen.push(a.get())));
  batch(() => {
    a.set(2);
    batch(() => a.set(3));
    assertDeepEq(seen, [1], "inner batch does not flush");
  });
  assertDeepEq(seen, [1, 3]);
});

test("batch returns the function's result", () => {
  assertEq(batch(() => 42), 42);
});

test("flush runs render effects before user effects, each in creation order", () => {
  const a = signal(0);
  const order = [];
  root(() => {
    effect(() => { a.get(); order.push("user1"); });
    renderEffect(() => { a.get(); order.push("render1"); });
    effect(() => { a.get(); order.push("user2"); });
    renderEffect(() => { a.get(); order.push("render2"); });
  });
  flush();
  order.length = 0;
  batch(() => a.set(1));
  assertDeepEq(order, ["render1", "render2", "user1", "user2"]);
});

test("untrack reads without subscribing", () => {
  const a = signal(1);
  const b = signal(1);
  let runs = 0;
  root(() => renderEffect(() => { a.get(); untrack(() => b.get()); runs++; }));
  batch(() => b.set(2));
  assertEq(runs, 1);
  batch(() => a.set(2));
  assertEq(runs, 2);
});

test("isTracking is true only inside a tracked computation", () => {
  assertEq(isTracking(), false);
  let inside = null;
  let untracked = null;
  root(() => renderEffect(() => { inside = isTracking(); untracked = untrack(isTracking); }));
  assertEq(inside, true);
  assertEq(untracked, false);
});

test("onCleanup runs before each re-run and on dispose", () => {
  const a = signal(1);
  const log = [];
  const dispose = root((d) => {
    renderEffect(() => { const v = a.get(); log.push(`run ${v}`); onCleanup(() => log.push(`clean ${v}`)); });
    return d;
  });
  batch(() => a.set(2));
  dispose();
  assertDeepEq(log, ["run 1", "clean 1", "run 2", "clean 2"]);
  batch(() => a.set(3));
  assertEq(log.length, 4, "a disposed effect never runs again");
});

test("onCleanup outside an owner is ignored", () => {
  onCleanup(() => { throw new Error("must not run"); });
});

test("disposing a scope disposes effects created inside, children first", () => {
  const a = signal(1);
  const log = [];
  root(() => {
    scope((disposeOuter) => {
      onCleanup(() => log.push("outer"));
      scope(() => { onCleanup(() => log.push("inner")); renderEffect(() => log.push(`effect ${a.get()}`)); });
      disposeOuter();
    });
  });
  batch(() => a.set(2));
  assertDeepEq(log, ["effect 1", "inner", "outer"]);
});

test("effects created during an effect run are disposed before it re-runs", () => {
  const outer = signal(1);
  const inner = signal(1);
  const log = [];
  root(() => renderEffect(() => {
    const o = outer.get();
    renderEffect(() => log.push(`inner ${o}/${inner.get()}`));
  }));
  batch(() => outer.set(2));
  batch(() => inner.set(2));
  assertDeepEq(log, ["inner 1/1", "inner 2/1", "inner 2/2"]);
});

test("an effect writing a signal it does not read settles in one flush", () => {
  const a = signal(1);
  const b = signal(0);
  root(() => renderEffect(() => b.set(a.get() * 2)));
  batch(() => a.set(5));
  assertEq(b.get(), 10);
});

test("an effect's write to a value it read in the same run does not re-run it", () => {
  const a = signal(0);
  let runs = 0;
  root(() => renderEffect(() => { runs++; if (a.get() < 5) a.set(a.get() + 1); }));
  assertEq(a.peek(), 1);
  assertEq(runs, 1);
});

test("effects that re-trigger each other forever are stopped with an error", () => {
  const a = signal(0);
  const b = signal(0);
  root(() => {
    renderEffect(() => b.set(a.get() + 1));
    renderEffect(() => a.set(b.get() + 1));
  });
  assertThrows(() => batch(() => a.set(100)), "re-triggering");
  assert(a.peek() > 100);
});

test("an async effect receives an AbortSignal aborted by the next run", async () => {
  const a = signal(1);
  const signals = [];
  root(() => effect((abort) => { a.get(); signals.push(abort); return Promise.resolve(); }));
  flush();
  batch(() => a.set(2));
  assertEq(signals.length, 2);
  assertEq(signals[0].aborted, true);
  assertEq(signals[1].aborted, false);
});

test("disposing an async effect aborts its running signal", () => {
  let signalSeen = null;
  const dispose = root((d) => { effect((abort) => { signalSeen = abort; }); return d; });
  flush();
  dispose();
  assertEq(signalSeen.aborted, true);
});

test("a rejection after cancellation is not reported", async () => {
  const a = signal(1);
  const errors = [];
  let rejectFirst = null;
  root(() => catchError(() => effect((abort) => {
    a.get();
    return new Promise((_, reject) => { if (rejectFirst === null) rejectFirst = reject; });
  }), (e) => errors.push(e)));
  flush();
  batch(() => a.set(2));
  rejectFirst(new Error("cancelled request"));
  await tick();
  assertDeepEq(errors, []);
});

test("an effect error reaches the nearest catchError and other effects still run", () => {
  const a = signal(0);
  const errors = [];
  let other = 0;
  root(() => {
    catchError(() => renderEffect(() => { if (a.get() > 0) throw new Error("boom"); }), (e) => errors.push(e.message));
    renderEffect(() => { a.get(); other++; });
  });
  batch(() => a.set(1));
  assertDeepEq(errors, ["boom"]);
  assertEq(other, 2);
});

test("an async effect failure reaches catchError", async () => {
  const errors = [];
  root(() => catchError(() => effect(async () => { throw new Error("late"); }), (e) => errors.push(e.message)));
  flush();
  await tick();
  assertDeepEq(errors, ["late"]);
});

test("a computed read inside an effect makes the effect depend on it", () => {
  const a = signal(2);
  const sq = computed(() => a.get() * a.get());
  const seen = [];
  root(() => renderEffect(() => seen.push(sq.get())));
  batch(() => a.set(3));
  batch(() => a.set(-3));
  assertDeepEq(seen, [4, 9], "-3 squared is still 9, so the effect is not re-run");
});

test("Object.is equality: NaN equals NaN, +0 differs from -0", () => {
  const s = signal(NaN);
  let runs = 0;
  root(() => renderEffect(() => { s.get(); runs++; }));
  batch(() => s.set(NaN));
  assertEq(runs, 1);
  const z = signal(0);
  let zr = 0;
  root(() => renderEffect(() => { z.get(); zr++; }));
  batch(() => z.set(-0));
  assertEq(zr, 2);
});
