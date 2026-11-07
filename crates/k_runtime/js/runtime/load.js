/**
 * @module k:runtime/load
 *
 * The `@Load` state machine of a page, layout or component
 * (docs/compiler/runtime.md, "@Load"; docs/design/14-async-data.md):
 * dependency-ordered, parallel where independent, cancelled when inputs
 * change, with `reload()` swapping all values atomically.
 */

import { batch, getOwner, onCleanup, renderEffect, runWithOwner, scope, signal, untrack } from "./signals.js";

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
 * @property {(values: Record<string, unknown>, signal: AbortSignal) => Promise<unknown>} run
 *   its synchronous reads are the load's inputs: changing one starts over
 * @property {() => unknown} [inputs] the same reads without the request, so a
 *   hydrated group (which runs nothing) still knows its inputs
 */

/**
 * Creates the load group of one declaration. The group belongs to the
 * current owner: disposing it cancels every running load.
 * @param {LoadSpec[]} loads in dependency order
 * @param {Record<string, unknown>|null} serialized values from `k-data` during hydration
 * @returns {{state: () => number, values: () => Record<string, unknown>|null, error: () => unknown, reload: () => void}}
 */
export function createLoads(loads, serialized) {
  const owner = getOwner();
  const state = signal(serialized ? READY : LOADING);
  const values = signal(serialized ? { ...serialized } : null);
  const error = signal(null);
  let generation = 0;
  let controllers = [];
  let runScope = null;

  const abort = () => {
    for (const c of controllers) c.abort();
    controllers = [];
  };
  const cancel = () => {
    abort();
    if (runScope !== null) runScope();
    runScope = null;
  };

  /**
   * Runs `read` tracked, in a scope of the group's owner that the next run
   * disposes; a change of what it read starts the group over.
   */
  const track = (read) => {
    runWithOwner(owner, () =>
      scope((dispose) => {
        const previous = runScope;
        runScope = () => {
          dispose();
          if (previous) previous();
        };
        let started = false;
        renderEffect(() => {
          if (started) {
            queueMicrotask(() => start(false));
            return;
          }
          started = true;
          read();
        });
      }),
    );
  };

  const start = (reloading) => {
    cancel();
    const gen = ++generation;
    const got = {};
    const begun = new Set();
    let left = loads.length;
    batch(() => {
      state.set(reloading ? RELOADING : LOADING);
      if (!reloading) values.set(null);
      error.set(null);
    });
    const launch = () => {
      for (const spec of loads) {
        if (begun.has(spec.name) || !spec.after.every((a) => a in got)) continue;
        begun.add(spec.name);
        const controller = new AbortController();
        controllers.push(controller);
        let promise;
        const snapshot = { ...got };
        track(() => {
          promise = spec.run(snapshot, controller.signal);
        });
        Promise.resolve(promise).then(
          (v) => {
            if (gen !== generation || controller.signal.aborted) return;
            got[spec.name] = v;
            left--;
            if (left === 0) {
              batch(() => {
                values.set({ ...got });
                state.set(READY);
              });
            } else {
              launch();
            }
          },
          (e) => {
            if (gen !== generation || controller.signal.aborted) return;
            // The inputs stay watched: a change starts over from FAILED.
            abort();
            generation++;
            batch(() => {
              error.set(e);
              values.set(null);
              state.set(FAILED);
            });
          },
        );
      }
    };
    if (loads.length === 0) {
      batch(() => {
        values.set({});
        state.set(READY);
      });
      return;
    }
    launch();
  };

  if (owner !== null) onCleanup(() => {
    generation++;
    cancel();
  });
  if (serialized) {
    // Hydrated: nothing runs until an input changes or `reload()`.
    for (const spec of loads) {
      if (typeof spec.inputs === "function") track(spec.inputs);
    }
  } else {
    untrack(() => start(false));
  }

  return {
    state: () => state.get(),
    values: () => values.get(),
    error: () => error.get(),
    reload: () => untrack(() => start(state.peek() !== FAILED && values.peek() !== null)),
  };
}
