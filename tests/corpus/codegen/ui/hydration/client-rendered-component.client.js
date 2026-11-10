// file: load.[hash].js
/**
 * @module k:runtime/load
 *
 * The `@Load` state machine of a page, layout or component
 * (docs/compiler/runtime.md, "@Load"; docs/design/14-async-data.md):
 * dependency-ordered, parallel where independent, cancelled when inputs
 * change, with `reload()` swapping all values atomically.
 */

import { batch, computed, getOwner, onCleanup, renderEffect, runWithOwner, scope, signal, untrack } from "./runtime.[hash].js";
import { CANCELLED } from "./runtime.[hash].js";

/** Nothing has resolved yet: `loading` renders. */
export const LOADING = 0;
/** Every load resolved: `view` renders with the values. */
export const READY = 1;
/** `reload()` runs while the previous values stay on screen. */
export const RELOADING = 2;
/** A load threw: `failed` renders with the error. */
export const FAILED = 3;

/**
 * @typedef {object} LoadSpec
 * @property {string} name the field
 * @property {string[]} after the names of the loads it reads (its dependencies)
 * @property {boolean} [keep] `reload()` keeps its value while its inputs are unchanged
 * @property {(values: Record<string, unknown>, signal: AbortSignal) => Promise<unknown>} run
 *   its synchronous reads are the load's inputs: changing one starts over
 * @property {() => unknown} [inputs] the same reads without the request, so a
 *   hydrated group (which runs nothing) still knows its inputs
 */

/**
 * Creates the load group of one declaration. The group belongs to the
 * current owner: disposing it cancels every running load.
 *
 * Each load watches its own inputs: a change re-runs that load and the
 * loads that read it, and keeps the others' values (14-async-data.md,
 * "Rules"); the group renders `loading` until the re-run ones resolve.
 * `reload()` re-runs every load except those marked `keep` whose inputs are unchanged.
 * @param {LoadSpec[]} loads in dependency order
 * @param {Record<string, unknown>|null} serialized values from `k-data` during hydration
 * @param {unknown} [failed] the error a server render failed with (from
 *   `k-data`, decoded): the group starts FAILED with it, and like a
 *   hydrated group runs nothing until an input changes or `reload()`
 * @returns {{state: () => number, values: () => Record<string, unknown>|null, ready: () => boolean, error: () => unknown, reload: () => void}}
 */
export function createLoads(loads, serialized, failed) {
  const owner = getOwner();
  const hydrated = serialized || failed !== undefined;
  const state = signal(serialized ? READY : hydrated ? FAILED : LOADING);
  const values = signal(serialized ? { ...serialized } : null);
  const error = signal(serialized ? null : failed ?? null);
  // Whether there are values: changes only between none and some, so an
  // effect waiting for the loads doesn't re-run on every reload.
  const ready = computed(() => values.get() !== null);
  /** The resolved values of the current runs, by name. */
  let got = serialized ? { ...serialized } : {};
  /** The running loads: name → their `AbortController`. */
  const running = new Map();
  /** Each load's input watch: name → its disposal. */
  const watches = new Map();
  /** Bumped by `reload()`, a failure and disposal: older runs are stale. */
  let epoch = 0;

  /** Stops the run of `name`, if any. */
  const stop = (name) => {
    running.get(name)?.abort();
    running.delete(name);
  };

  /**
   * Runs `read` tracked, in a scope of the group's owner that replaces the
   * load's previous watch; a change of what it read re-runs the load.
   */
  const watch = (spec, read) => {
    watches.get(spec.name)?.();
    runWithOwner(owner, () =>
      scope((dispose) => {
        watches.set(spec.name, dispose);
        let started = false;
        renderEffect(() => {
          if (started) {
            queueMicrotask(() => {
              if (watches.get(spec.name) === dispose) invalidate(spec.name);
            });
            return;
          }
          started = true;
          read();
        });
      }),
    );
  };

  /** The load `name` and every load that reads it, directly or not. */
  const affected = (name) => {
    const out = new Set([name]);
    for (const spec of loads) if (spec.after.some((a) => out.has(a))) out.add(spec.name);
    return out;
  };

  /** An input of `name` changed: re-runs it and what reads it. */
  const invalidate = (name) => {
    for (const n of affected(name)) {
      stop(n);
      delete got[n];
    }
    batch(() => {
      state.set(LOADING);
      values.set(null);
      error.set(null);
    });
    launch(epoch);
  };

  /** Starts every load whose dependencies resolved and that has no value. */
  const launch = (ep) => {
    for (const spec of loads) {
      if (spec.name in got || running.has(spec.name) || !spec.after.every((a) => a in got)) continue;
      const controller = new AbortController();
      running.set(spec.name, controller);
      let promise;
      const snapshot = { ...got };
      watch(spec, () => {
        promise = spec.run(snapshot, controller.signal);
      });
      const current = () => ep === epoch && running.get(spec.name) === controller && !controller.signal.aborted;
      Promise.resolve(promise).then(
        (v) => {
          if (!current()) return;
          running.delete(spec.name);
          got[spec.name] = v;
          if (loads.every((l) => l.name in got)) {
            // In declaration order, whichever resolved last.
            const all = {};
            for (const l of loads) all[l.name] = got[l.name];
            batch(() => {
              values.set(all);
              state.set(READY);
              error.set(null);
            });
          } else {
            launch(ep);
          }
        },
        (e) => {
          // A run stopped by the cancellation (its own, or a disposed
          // component's function called for a value) is not a failure.
          if (!current() || e === CANCELLED) return;
          // The inputs stay watched: a change starts over from FAILED.
          for (const c of running.values()) c.abort();
          running.clear();
          epoch++;
          got = {};
          batch(() => {
            error.set(e);
            values.set(null);
            state.set(FAILED);
          });
        },
      );
    }
  };

  /**
   * Re-runs every load: the values stay while `reloading`. A reload keeps
   * the value of a load marked `keep` (`@Load(keepOnReload: true)`) whose
   * inputs haven't changed, when the loads it reads are kept too (an input
   * change already re-ran it; 14-async-data.md, "Reloading").
   */
  const start = (reloading) => {
    for (const c of running.values()) c.abort();
    running.clear();
    const ep = ++epoch;
    got = {};
    const previous = reloading ? values.peek() : null;
    if (previous !== null) {
      for (const spec of loads) {
        if (spec.keep && spec.name in previous && spec.after.every((a) => a in got)) got[spec.name] = previous[spec.name];
      }
      if (loads.every((l) => l.name in got)) return;
    }
    batch(() => {
      state.set(reloading ? RELOADING : LOADING);
      if (!reloading) values.set(null);
      error.set(null);
    });
    if (loads.length === 0) {
      batch(() => {
        values.set({});
        state.set(READY);
      });
      return;
    }
    launch(ep);
  };

  if (owner !== null) onCleanup(() => {
    epoch++;
    for (const c of running.values()) c.abort();
    running.clear();
    for (const dispose of watches.values()) dispose();
    watches.clear();
  });
  if (hydrated) {
    // Hydrated: nothing runs until an input changes or `reload()`.
    for (const spec of loads) {
      if (typeof spec.inputs === "function") watch(spec, spec.inputs);
    }
  } else {
    untrack(() => start(false));
  }

  return {
    state: () => state.get(),
    values: () => values.get(),
    ready: () => ready.get(),
    error: () => error.get(),
    reload: () => untrack(() => start(state.peek() !== FAILED && values.peek() !== null)),
  };
}
// file: shared.[hash].js
import * as $k from "./runtime.[hash].js";
import * as $k_load from "./load.[hash].js";

const TodoList$t0 = $k.template("<h2> </h2>");
const TodoList$t1 = $k.template("<p> </p>");

export async function count($c, title) {
  return $k.graphemeCount(title);
}

function TodoList$view(props, ctx) {
  if (props.rows === undefined) {
    const $d1 = 3;
    props.rows = () => $d1;
  }
  const shown = $k.signal(0);
  const $loads = $k_load.createLoads([{ name: "total", after: [], run: async (v, $c) => {
    return $k.live($c, await $k.guard($c, count($c, props.title())));
  }, inputs: () => [props.title()] }], null);
  const $m6 = $k.markers();
  $k.branch($m6.start, $m6.end, () => [0, 1, 1, 2][$loads.state()], [() => {
    const $root4 = TodoList$t1();
    const $e5 = $root4.firstChild;
    $k.renderEffect(() => $k.text($e5, `Loading ${props.title()} (${String(props.rows())} rows)…`));
    return $root4;
  }, () => {
    const $root2 = TodoList$t0();
    const $e3 = $root2.firstChild;
    $k.renderEffect(() => $k.text($e3, `${props.title()}: ${String(shown.get())} of ${String($loads.values().total)}`));
    return $root2;
  }, () => null]);
  return $m6.node;
}

export function TodoList(props, ctx) {
  if (props.rows === undefined) {
    const $d1 = 3;
    props.rows = () => $d1;
  }
  const $m9 = $k.markers();
  const $r10 = $k.clientReady();
  $k.branch($m9.start, $m9.end, () => $r10() ? 1 : 0, [() => {
    const $root7 = TodoList$t1();
    const $e8 = $root7.firstChild;
    $k.renderEffect(() => $k.text($e8, `Loading ${props.title()} (${String(props.rows())} rows)…`));
    return $root7;
  }, () => TodoList$view(props, { ...ctx, data: null })]);
  return $m9.node;
}
