// Signals as the client UI uses them: batching, glitch-free reads, diamond
// dependencies, effect cleanup, async effect cancellation, `untracked` and
// the lowered form of `@Effect(on:)` (docs/compiler/runtime.md §3,
// docs/design/03-reactivity.md). The basics are in signals.test.js; these
// cases pin the guarantees generated components rely on.
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import * as k from "k:runtime";
import {
  batch, catchError, computed, effect, flush, onCleanup, renderEffect, root, scope, signal, untrack,
} from "k:runtime";

// ---------------------------------------------------------------- batching

// Nested batches flush once, when the outermost one ends.
test("nested batches flush once at the outermost end", () => {
  const a = signal(0);
  const seen = [];
  root(() => renderEffect(() => seen.push(a.get())));
  batch(() => {
    a.set(1);
    batch(() => a.set(2));
    assertDeepEq(seen, [0], "the inner batch's end doesn't flush");
    a.set(3);
  });
  assertDeepEq(seen, [0, 3]);
});

// A batch whose function throws still flushes the writes made before the throw.
test("a throwing batch still flushes and rethrows", () => {
  const a = signal(0);
  const seen = [];
  root(() => renderEffect(() => seen.push(a.get())));
  let caught = null;
  try {
    batch(() => {
      a.set(1);
      throw new Error("boom");
    });
  } catch (e) {
    caught = e;
  }
  assertEq(caught?.message, "boom");
  assertDeepEq(seen, [0, 1]);
});

// Reads inside a batch see the latest writes, even of computed values that
// are only marked stale.
test("reads inside a batch see the latest value", () => {
  const a = signal(1);
  const doubled = computed(() => a.get() * 2);
  assertEq(doubled.get(), 2);
  batch(() => {
    a.set(5);
    assertEq(a.get(), 5);
    assertEq(doubled.get(), 10);
  });
});

// Writing many signals in one batch re-runs an effect that reads all of them
// once.
test("a batch of a thousand writes runs a reader once", () => {
  const signals = Array.from({ length: 1000 }, (_, i) => signal(i));
  let runs = 0;
  root(() => renderEffect(() => { runs++; for (const s of signals) s.get(); }));
  batch(() => { for (const s of signals) s.set(s.peek() + 1); });
  assertEq(runs, 2);
});

// Writes outside any batch are not visible to effects until the microtask
// flush, and then in one run.
test("unbatched writes flush in one microtask run", async () => {
  const a = signal(0);
  const b = signal(0);
  const seen = [];
  root(() => renderEffect(() => seen.push(`${a.get()}${b.get()}`)));
  a.set(1);
  b.set(1);
  assertDeepEq(seen, ["00"]);
  await tick();
  assertDeepEq(seen, ["00", "11"]);
});

// A write that sets the current value notifies nobody, and so schedules
// nothing.
test("writing an equal value schedules no flush", async () => {
  const a = signal("x");
  let runs = 0;
  root(() => renderEffect(() => { a.get(); runs++; }));
  a.set("x");
  await tick();
  assertEq(runs, 1);
});

// Arrays compare by identity, not content: a new array with the same items
// is a change (value semantics come from never mutating, codegen.md §5).
test("a new array with equal items is a change", () => {
  const items = signal([1, 2]);
  let runs = 0;
  root(() => renderEffect(() => { items.get(); runs++; }));
  batch(() => items.set([1, 2]));
  assertEq(runs, 2);
});

// ---------------------------------------------------------------- glitch freedom

// An effect reading a signal and a value derived from it never sees them
// out of step.
test("an effect never observes a source and its derived value out of step", () => {
  const a = signal(1);
  const doubled = computed(() => a.get() * 2);
  const seen = [];
  root(() => renderEffect(() => seen.push([a.get(), doubled.get()])));
  batch(() => a.set(2));
  batch(() => a.set(3));
  for (const [x, y] of seen) assertEq(y, x * 2, `saw ${x} with ${y}`);
  assertEq(seen.length, 3);
});

// A diamond read by an effect runs the effect once per write, with a
// consistent bottom.
test("a diamond under an effect runs it once per write", () => {
  const a = signal(1);
  const left = computed(() => a.get() + 1);
  const right = computed(() => a.get() * 10);
  const bottom = computed(() => `${left.get()}/${right.get()}`);
  const seen = [];
  root(() => renderEffect(() => seen.push(bottom.get())));
  batch(() => a.set(2));
  assertDeepEq(seen, ["2/10", "3/20"]);
});

// A deeper diamond (two paths of different lengths) still recomputes the
// bottom once.
test("paths of different lengths into one node recompute it once", () => {
  const a = signal(1);
  const short = computed(() => a.get());
  const long1 = computed(() => a.get() + 1);
  const long2 = computed(() => long1.get() + 1);
  let runs = 0;
  const bottom = computed(() => { runs++; return short.get() + long2.get(); });
  assertEq(bottom.get(), 4);
  a.set(2);
  assertEq(bottom.get(), 6);
  assertEq(runs, 2);
});

// One side of a diamond that keeps its value doesn't cause a second run.
test("an unchanged side of a diamond doesn't recompute the bottom twice", () => {
  const a = signal(2);
  const parity = computed(() => a.get() % 2);
  const half = computed(() => Math.floor(a.get() / 2));
  let runs = 0;
  const bottom = computed(() => { runs++; return `${parity.get()}:${half.get()}`; });
  assertEq(bottom.get(), "0:1");
  a.set(4);
  assertEq(bottom.get(), "0:2");
  assertEq(runs, 2);
  a.set(5);
  assertEq(bottom.get(), "1:2", "half stays 2, parity changes");
  assertEq(runs, 3);
});

// A dependency read only in a branch that is no longer taken stops
// triggering the reader.
test("a dropped conditional dependency no longer triggers", () => {
  const show = signal(true);
  const detail = signal("a");
  let runs = 0;
  root(() => renderEffect(() => { runs++; if (show.get()) detail.get(); }));
  batch(() => show.set(false));
  assertEq(runs, 2);
  batch(() => detail.set("b"));
  assertEq(runs, 2, "detail is no longer read");
});

// Render effects created in creation order run parent before child, so a
// parent's binding is current before a child's (runtime.md §3.6).
test("render effects re-run parents before children", () => {
  const a = signal(0);
  const order = [];
  root(() => {
    renderEffect(() => {
      order.push(`parent ${a.get()}`);
      renderEffect(() => order.push(`child ${a.get()}`));
    });
  });
  order.length = 0;
  batch(() => a.set(1));
  assertEq(order[0], "parent 1");
  assertEq(order.filter((o) => o === "child 1").length, 1, "the re-created child runs once");
});

// ---------------------------------------------------------------- effect cleanup

// Cleanups run in reverse registration order, before the next run.
test("cleanups run in reverse order before the next run", () => {
  const a = signal(0);
  const log = [];
  root(() => renderEffect(() => {
    const v = a.get();
    onCleanup(() => log.push(`first ${v}`));
    onCleanup(() => log.push(`second ${v}`));
  }));
  batch(() => a.set(1));
  assertDeepEq(log, ["second 0", "first 0"]);
});

// A cleanup sees the values of the run that registered it.
test("a cleanup closes over its own run's values", () => {
  const id = signal("a");
  const closed = [];
  root(() => effect(() => { const v = id.get(); onCleanup(() => closed.push(v)); }));
  flush();
  batch(() => id.set("b"));
  batch(() => id.set("c"));
  assertDeepEq(closed, ["a", "b"]);
});

// Disposing a component's scope runs every cleanup of every effect in it,
// children before parents (unmount, runtime.md §3.5).
test("disposing a scope runs nested cleanups children first", () => {
  const log = [];
  let dispose = null;
  root(() => {
    scope((d) => {
      dispose = d;
      onCleanup(() => log.push("component"));
      renderEffect(() => {
        onCleanup(() => log.push("binding"));
        scope(() => onCleanup(() => log.push("child component")));
      });
    });
  });
  dispose();
  assertDeepEq(log, ["child component", "binding", "component"]);
});

// A disposed effect never runs again, even if it was queued.
test("an effect disposed while queued doesn't run", () => {
  const a = signal(0);
  let runs = 0;
  let stop = null;
  root(() => { stop = renderEffect(() => { a.get(); runs++; }); });
  batch(() => {
    a.set(1);
    stop();
  });
  assertEq(runs, 1);
});

// Effects of a scope disposed during a flush don't run later in that flush.
test("disposing a scope from an earlier effect skips its queued effects", () => {
  const a = signal(0);
  const log = [];
  root(() => {
    let disposeChild = null;
    renderEffect(() => {
      if (a.get() === 1 && disposeChild !== null) disposeChild();
    });
    scope((d) => {
      disposeChild = d;
      renderEffect(() => log.push(a.get()));
    });
  });
  batch(() => a.set(1));
  assertDeepEq(log, [0]);
});

// A cleanup that throws reaches the error boundary and doesn't stop the other
// cleanups.
test("a throwing cleanup doesn't stop the others", () => {
  const log = [];
  const errors = [];
  let dispose = null;
  root((d) => {
    dispose = d;
    catchError(() => {
      onCleanup(() => log.push("after"));
      onCleanup(() => { throw new Error("cleanup failed"); });
      onCleanup(() => log.push("first"));
    }, (e) => errors.push(e.message));
  });
  dispose();
  assertDeepEq(log, ["first", "after"]);
  assertDeepEq(errors, ["cleanup failed"]);
});

// A computed value's cleanups run before it recomputes.
test("a computed value's cleanups run before it recomputes", () => {
  const a = signal(1);
  const log = [];
  const c = computed(() => { const v = a.get(); onCleanup(() => log.push(v)); return v; });
  c.get();
  a.set(2);
  c.get();
  assertDeepEq(log, [1]);
});

// ---------------------------------------------------------------- async effects

// A superseded async run's signal is aborted before the next run starts, and
// the new run gets a fresh signal.
test("each async run gets a fresh signal and aborts the previous one", () => {
  const id = signal(1);
  const signals = [];
  root(() => renderEffect((s) => { id.get(); signals.push(s); }));
  batch(() => id.set(2));
  batch(() => id.set(3));
  assertEq(signals.length, 3);
  assert(signals[0].aborted && signals[1].aborted, "superseded runs are aborted");
  assert(!signals[2].aborted, "the current run is not");
  assert(signals[1] !== signals[2]);
});

// A user effect with a parameter gets an AbortSignal too, aborted on
// unmount.
test("a user effect's signal is aborted when its scope is disposed", () => {
  let captured = null;
  let dispose = null;
  root((d) => {
    dispose = d;
    effect((s) => { captured = s; });
  });
  flush();
  assert(captured !== null && !captured.aborted);
  dispose();
  assert(captured.aborted);
});

// An async effect's writes after cancellation are the caller's to suppress
// (`$k.guard`); the runtime only guarantees the abort is visible at once.
test("a cancelled async run sees its signal aborted when it resumes", async () => {
  const id = signal(1);
  const results = [];
  let release = null;
  root(() => effect(async (s) => {
    const mine = id.get();
    await new Promise((r) => { if (mine === 1) release = r; else r(); });
    results.push([mine, s.aborted]);
  }));
  flush();
  batch(() => id.set(2));
  release();
  await tick();
  assertDeepEq(results.sort(), [[1, true], [2, false]]);
});

// Reads after the first await are not tracked (runtime.md §3.4).
test("reads after an await don't subscribe", async () => {
  const before = signal(0);
  const after = signal(0);
  let runs = 0;
  root(() => effect(async () => {
    runs++;
    before.get();
    await null;
    after.get();
  }));
  flush();
  await tick();
  batch(() => after.set(1));
  await tick();
  assertEq(runs, 1, "after was read after the await");
  batch(() => before.set(1));
  await tick();
  assertEq(runs, 2);
});

// A rejection of a run that was not cancelled reaches the error boundary.
test("a rejection of a live async run reaches the boundary", async () => {
  const errors = [];
  root(() => catchError(() => effect(async (s) => { await null; throw new Error("network"); }), (e) => errors.push(e.message)));
  flush();
  await tick();
  assertDeepEq(errors, ["network"]);
});

// `$k.guard($c, promise)` settles like the promise while `$c` is live, and
// never settles once it is aborted, so nothing after a cancelled await runs
// (codegen.md §8).
test("guard never resumes after the signal is aborted", async () => {
  assertEq(typeof k.guard, "function", "k:runtime exports guard");
  const live = new AbortController();
  assertEq(await k.guard(live.signal, Promise.resolve(7)), 7);
  const dead = new AbortController();
  let resumed = false;
  let release = null;
  const p = k.guard(dead.signal, new Promise((r) => { release = r; })).then(() => { resumed = true; });
  dead.abort();
  release(1);
  await tick();
  assertEq(resumed, false);
  void p;
});

// `guard` with a `null` context (outside any cancellable run) is the promise
// itself.
test("guard with no context passes the promise through", async () => {
  assertEq(await k.guard(null, Promise.resolve("x")), "x");
});

// `guard` rejects like the promise while the signal is live, so typed errors
// still reach the `catch` of a running effect.
test("guard passes rejections through while live", async () => {
  const live = new AbortController();
  let caught = null;
  try {
    await k.guard(live.signal, Promise.reject({ $: 0 }));
  } catch (e) {
    caught = e;
  }
  assertDeepEq(caught, { $: 0 });
});

// ---------------------------------------------------------------- untracked

// `untrack` inside a computed value doesn't subscribe it.
test("untrack inside a computed value doesn't subscribe it", () => {
  const a = signal(1);
  const b = signal(10);
  let runs = 0;
  const c = computed(() => { runs++; return a.get() + untrack(() => b.get()); });
  assertEq(c.get(), 11);
  b.set(20);
  assertEq(c.get(), 11, "b is not a dependency");
  a.set(2);
  assertEq(c.get(), 22, "the next run reads b's current value");
  assertEq(runs, 2);
});

// `untrack` returns its function's value and restores tracking afterwards.
test("tracking resumes after an untracked read", () => {
  const a = signal(1);
  const b = signal(2);
  const seen = [];
  root(() => renderEffect(() => {
    const x = untrack(() => a.get());
    seen.push(x + b.get());
  }));
  batch(() => a.set(5));
  assertDeepEq(seen, [3]);
  batch(() => b.set(3));
  assertDeepEq(seen, [3, 8]);
});

// Nested `untrack` calls restore the outer state correctly.
test("nested untrack restores the outer tracking state", () => {
  const a = signal(0);
  let runs = 0;
  root(() => renderEffect(() => {
    runs++;
    untrack(() => untrack(() => a.peek()));
    a.get();
  }));
  batch(() => a.set(1));
  assertEq(runs, 2);
});

// ---------------------------------------------------------------- @Effect(on:)

// The lowering of `@Effect(on: [userId])`: read the listed values, run the
// body untracked. Body reads don't re-run it; listed values do.
test("an effect with explicit dependencies ignores reads in its body", () => {
  const userId = signal(1);
  const name = signal("Ada");
  const log = [];
  root(() => effect(() => {
    userId.get();
    untrack(() => log.push(`${userId.peek()} ${name.get()}`));
  }));
  flush();
  batch(() => name.set("Grace"));
  assertDeepEq(log, ["1 Ada"]);
  batch(() => userId.set(2));
  assertDeepEq(log, ["1 Ada", "2 Grace"]);
});

// The lowering of `@Effect(on: [])`: no dependencies, so it runs once after
// mount and never again.
test("an effect with no dependencies runs once after mount", () => {
  const a = signal(0);
  let runs = 0;
  root(() => effect(() => untrack(() => { a.get(); runs++; })));
  assertEq(runs, 0, "it waits for the flush after mount");
  flush();
  batch(() => a.set(1));
  flush();
  assertEq(runs, 1);
});

// A user effect runs after the render effects of the same flush, so it sees
// the DOM the bindings produced.
test("a user effect runs after the bindings of the same write", () => {
  installTextDom();
  const a = signal("a");
  const node = globalThis.document.createTextNode("");
  const seen = [];
  root(() => {
    effect(() => seen.push(`effect saw ${node.data} for ${a.get()}`));
    renderEffect(() => { node.data = a.get(); });
  });
  flush();
  batch(() => a.set("b"));
  assertDeepEq(seen, ["effect saw a for a", "effect saw b for b"]);
});

function installTextDom() {
  // The harness installs a fresh fake DOM before each test; this only
  // documents that the test uses it.
  assert(globalThis.document !== undefined, "the fake DOM is installed");
}
