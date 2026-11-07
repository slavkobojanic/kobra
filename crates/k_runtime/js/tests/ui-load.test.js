/**
 * @module k:/tests/ui-load.test.js
 *
 * The `@Load` state machine (docs/compiler/runtime.md §7,
 * docs/design/14-async-data.md): LOADING → READY | FAILED, dependency-ordered
 * and parallel loads, cancellation when an input changes, `reload()` with an
 * atomic swap through RELOADING, and serialized values from hydration that
 * are never fetched again.
 *
 * Assumptions beyond runtime.md §7 (to be pinned there): `values()` is `null`
 * outside READY and RELOADING; `state()`, `values()` and `error()` are
 * reactive reads; the group belongs to the owner it is created in.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { FAILED, LOADING, READY, RELOADING, batch, createLoads, renderEffect, root, signal } from "k:runtime";

const LOAD = "@Load (runtime.md §7)";

/** A promise the test resolves or rejects by hand, with a record of aborts. */
function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

/**
 * A controllable load: each run is recorded with its inputs and signal and
 * settles when the test calls `resolve`/`reject` on it.
 */
function controlled(name, after = [], read = () => null) {
  const runs = [];
  return {
    runs,
    spec: {
      name,
      after,
      run: (values, signal) => {
        const input = read(values);
        const d = deferred();
        runs.push({ values, signal, input, ...d });
        return d.promise;
      },
    },
  };
}

// ---------------------------------------------------------------- first load

// A group starts LOADING and becomes READY once every load resolved.
test("a group starts LOADING and becomes READY", async () => {
  const product = controlled("product");
  const loads = root(() => createLoads([product.spec], null));
  assertEq(loads.state(), LOADING);
  assertEq(loads.values(), null, "no values before READY");
  product.runs[0].resolve({ id: 1, name: "Lamp" });
  await tick();
  assertEq(loads.state(), READY);
  assertDeepEq(loads.values(), { product: { id: 1, name: "Lamp" } });
});

// Independent loads start together, before either resolves.
test("independent loads run in parallel", async () => {
  const product = controlled("product");
  const related = controlled("related");
  root(() => createLoads([product.spec, related.spec], null));
  await tick();
  assertEq(product.runs.length, 1);
  assertEq(related.runs.length, 1, "related didn't wait for product");
});

// A load that reads another waits for it and receives its value.
test("a dependent load waits for its dependency", async () => {
  const product = controlled("product");
  const reviews = controlled("reviews", ["product"], (v) => v.product.id);
  const loads = root(() => createLoads([product.spec, reviews.spec], null));
  await tick();
  assertEq(reviews.runs.length, 0);
  product.runs[0].resolve({ id: 7 });
  await tick();
  assertEq(reviews.runs.length, 1);
  assertEq(reviews.runs[0].input, 7);
  assertEq(loads.state(), LOADING, "reviews hasn't resolved");
  reviews.runs[0].resolve([]);
  await tick();
  assertEq(loads.state(), READY);
});

// A load with two dependencies waits for both, whatever order they resolve
// in.
test("a load with two dependencies waits for both", async () => {
  const a = controlled("a");
  const b = controlled("b");
  const total = controlled("total", ["a", "b"], (v) => v.a + v.b);
  root(() => createLoads([a.spec, b.spec, total.spec], null));
  await tick();
  b.runs[0].resolve(2);
  await tick();
  assertEq(total.runs.length, 0);
  a.runs[0].resolve(1);
  await tick();
  assertEq(total.runs.length, 1);
  assertEq(total.runs[0].input, 3);
});

// `state()` is reactive: a view switching on it re-renders on each
// transition.
test("state is reactive", async () => {
  const x = controlled("x");
  const seen = [];
  root(() => {
    const loads = createLoads([x.spec], null);
    renderEffect(() => seen.push(loads.state()));
  });
  x.runs[0].resolve(1);
  await tick();
  assertDeepEq(seen, [LOADING, READY]);
});

// ---------------------------------------------------------------- failure

// A load that throws moves the group to FAILED with the thrown typed error.
test("a failing load moves the group to FAILED", async () => {
  const x = controlled("x");
  const loads = root(() => createLoads([x.spec], null));
  x.runs[0].reject({ $: 0 });
  await tick();
  assertEq(loads.state(), FAILED);
  assertDeepEq(loads.error(), { $: 0 });
  assertEq(loads.values(), null);
});

// When a load fails, the loads that depend on it never start.
test("dependents of a failed load never start", async () => {
  const product = controlled("product");
  const reviews = controlled("reviews", ["product"]);
  root(() => createLoads([product.spec, reviews.spec], null));
  product.runs[0].reject({ $: 0 });
  await tick();
  assertEq(reviews.runs.length, 0);
});

// A failure while a sibling is still running cancels the sibling: its value
// could never be shown.
test("a failure cancels the loads still running", async () => {
  const a = controlled("a");
  const b = controlled("b");
  root(() => createLoads([a.spec, b.spec], null));
  await tick();
  a.runs[0].reject({ $: 1 });
  await tick();
  assert(b.runs[0].signal.aborted);
});

// ---------------------------------------------------------------- input changes

// A load's synchronous reads are its inputs: changing one cancels the run
// and starts over in LOADING (a new `@Param` on client navigation).
test("an input change cancels the run and starts over", async () => {
  const id = signal(1);
  const product = controlled("product", [], () => id.get());
  const loads = root(() => createLoads([product.spec], null));
  await tick();
  batch(() => id.set(2));
  await tick();
  assertEq(product.runs.length, 2);
  assert(product.runs[0].signal.aborted, "the old run is cancelled");
  assertEq(product.runs[1].input, 2);
  assertEq(loads.state(), LOADING);
});

// A superseded run that resolves late never overwrites the current values.
test("a superseded run's late result is ignored", async () => {
  const id = signal(1);
  const product = controlled("product", [], () => id.get());
  const loads = root(() => createLoads([product.spec], null));
  await tick();
  batch(() => id.set(2));
  await tick();
  product.runs[1].resolve("two");
  await tick();
  product.runs[0].resolve("one");
  await tick();
  assertEq(loads.values().product, "two");
});

// An input change while READY shows LOADING, not the stale values: they
// describe something else (design 14, "What renders during a re-run").
test("an input change from READY goes back to LOADING", async () => {
  const id = signal(1);
  const product = controlled("product", [], () => id.get());
  const loads = root(() => createLoads([product.spec], null));
  product.runs[0].resolve("one");
  await tick();
  assertEq(loads.state(), READY);
  batch(() => id.set(2));
  await tick();
  assertEq(loads.state(), LOADING);
  assertEq(loads.values(), null);
});

// An input change in FAILED starts over too.
test("an input change from FAILED starts over", async () => {
  const id = signal(1);
  const product = controlled("product", [], () => id.get());
  const loads = root(() => createLoads([product.spec], null));
  product.runs[0].reject({ $: 0 });
  await tick();
  batch(() => id.set(2));
  await tick();
  assertEq(loads.state(), LOADING);
  product.runs[1].resolve("two");
  await tick();
  assertEq(loads.state(), READY);
});

// Disposing the owner (unmounting, leaving the page) cancels every running
// load.
test("disposing the owner cancels the running loads", async () => {
  const a = controlled("a");
  let dispose = null;
  root((d) => { dispose = d; createLoads([a.spec], null); });
  await tick();
  dispose();
  assert(a.runs[0].signal.aborted);
});

// ---------------------------------------------------------------- reload

// `reload()` from READY keeps the old values on screen in RELOADING and
// swaps them all at once when the new run resolved.
test("reload keeps the old values until all new ones resolved", async () => {
  const a = controlled("a");
  const b = controlled("b");
  const loads = root(() => createLoads([a.spec, b.spec], null));
  a.runs[0].resolve(1);
  b.runs[0].resolve(10);
  await tick();
  loads.reload();
  assertEq(loads.state(), RELOADING);
  await tick();
  a.runs[1].resolve(2);
  await tick();
  assertDeepEq(loads.values(), { a: 1, b: 10 }, "a's new value waits for b");
  b.runs[1].resolve(20);
  await tick();
  assertEq(loads.state(), READY);
  assertDeepEq(loads.values(), { a: 2, b: 20 });
});

// The swap is atomic: a binding reading two values never sees one old and
// one new.
test("a reload's swap is atomic for readers", async () => {
  const a = controlled("a");
  const b = controlled("b");
  const seen = [];
  let loads = null;
  root(() => {
    loads = createLoads([a.spec, b.spec], null);
    renderEffect(() => { const v = loads.values(); if (v !== null) seen.push(`${v.a}/${v.b}`); });
  });
  a.runs[0].resolve(1);
  b.runs[0].resolve(1);
  await tick();
  loads.reload();
  await tick();
  b.runs[1].resolve(2);
  await tick();
  a.runs[1].resolve(2);
  await tick();
  assertDeepEq(seen, ["1/1", "2/2"]);
});

// `reload()` is synchronous and cancels a run still in flight.
test("reload cancels the run in flight", async () => {
  const a = controlled("a");
  const loads = root(() => createLoads([a.spec], null));
  a.runs[0].resolve(1);
  await tick();
  loads.reload();
  loads.reload();
  await tick();
  assertEq(a.runs.length, 3);
  assert(a.runs[1].signal.aborted);
  assert(!a.runs[2].signal.aborted);
});

// A reload that fails moves to FAILED.
test("a failed reload moves to FAILED", async () => {
  const a = controlled("a");
  const loads = root(() => createLoads([a.spec], null));
  a.runs[0].resolve(1);
  await tick();
  loads.reload();
  await tick();
  a.runs[1].reject({ $: 2 });
  await tick();
  assertEq(loads.state(), FAILED);
  assertDeepEq(loads.error(), { $: 2 });
});

// `reload()` from FAILED goes through LOADING (there are no values to keep).
test("reload from FAILED goes through LOADING", async () => {
  const a = controlled("a");
  const loads = root(() => createLoads([a.spec], null));
  a.runs[0].reject({ $: 0 });
  await tick();
  loads.reload();
  assertEq(loads.state(), LOADING);
  await tick();
  a.runs[1].resolve(5);
  await tick();
  assertEq(loads.state(), READY);
  assertEq(loads.values().a, 5);
});

// Reloading re-runs dependent loads in dependency order, with the new values.
test("reload re-runs dependents with the new values", async () => {
  const product = controlled("product");
  const reviews = controlled("reviews", ["product"], (v) => v.product);
  const loads = root(() => createLoads([product.spec, reviews.spec], null));
  product.runs[0].resolve("p1");
  await tick();
  reviews.runs[0].resolve("r1");
  await tick();
  loads.reload();
  await tick();
  assertEq(reviews.runs.length, 1, "reviews waits for the new product");
  product.runs[1].resolve("p2");
  await tick();
  assertEq(reviews.runs[1].input, "p2");
});

// ---------------------------------------------------------------- hydration

// With serialized values the group starts READY and runs nothing: the
// server's results are never fetched twice.
test("serialized values start READY without running", async () => {
  const product = controlled("product");
  const loads = root(() => createLoads([product.spec], { product: { id: 1 } }));
  assertEq(loads.state(), READY);
  assertDeepEq(loads.values(), { product: { id: 1 } });
  await tick();
  assertEq(product.runs.length, 0);
});

// After hydration an input change or a reload runs the loads as usual.
test("a hydrated group runs again on reload", async () => {
  const product = controlled("product");
  const loads = root(() => createLoads([product.spec], { product: "server" }));
  loads.reload();
  assertEq(loads.state(), RELOADING);
  await tick();
  assertEq(product.runs.length, 1);
});

// A hydrated group still tracks its inputs: the first input change after
// hydration runs the load.
test("a hydrated group runs when an input changes", async () => {
  const id = signal(1);
  const product = controlled("product", [], () => id.get());
  // A hydrated group runs nothing, so it learns its inputs from `inputs`,
  // the load's reads without the request (runtime.md §7).
  product.spec.inputs = () => id.get();
  const loads = root(() => createLoads([product.spec], { product: "server" }));
  batch(() => id.set(2));
  await tick();
  assertEq(product.runs.length, 1);
  assertEq(product.runs[0].input, 2);
  assertEq(loads.state(), LOADING);
});

// A load that failed on the server renders `failed`; hydration starts the
// group FAILED with the server's error, decoded, and runs nothing, so the
// failed block claims the server's markup (14-async-data.md).
test("a group hydrated from a failed render starts FAILED with the server's error", async () => {
  const product = controlled("product");
  const error = { $: 0, id: 7 };
  const loads = root(() => createLoads([product.spec], null, error));
  assertEq(loads.state(), FAILED);
  assertEq(loads.error(), error);
  assertEq(loads.values(), null);
  await tick();
  assertEq(product.runs.length, 0, "nothing loads again");
});

test("a group hydrated as FAILED loads again on reload and when an input changes", async () => {
  const id = signal(1);
  const product = controlled("product", [], () => id.get());
  product.spec.inputs = () => id.get();
  const loads = root(() => createLoads([product.spec], null, { $: 1 }));
  loads.reload();
  assertEq(loads.state(), LOADING, "reload from FAILED goes through LOADING");
  await tick();
  assertEq(product.runs.length, 1);
  const again = controlled("product", [], () => id.get());
  again.spec.inputs = () => id.get();
  const other = root(() => createLoads([again.spec], null, { $: 1 }));
  batch(() => id.set(2));
  await tick();
  assertEq(again.runs.length, 1, "an input change after hydration starts over");
  assertEq(other.state(), LOADING);
});
