/**
 * @module k:runtime/store
 *
 * Path stores for `@State` structs (docs/compiler/runtime.md §3.3).
 */

import { batch, signal } from "./signals.js";

/**
 * Reads the value at `path` below `value`; `undefined` when the path
 * runs through `null` or `undefined`.
 * @param {unknown} value the root
 * @param {Array<string|number>} path field names and list indexes
 * @returns {unknown}
 */
function pathGet(value, path) {
  let v = value;
  for (const k of path) {
    if (v === null || v === undefined) return undefined;
    v = v[k];
  }
  return v;
}

/**
 * Returns a copy of `value` with `v` at `path[i..]`, copying each object
 * and array along the path and sharing everything else (structural
 * sharing; `value` is never mutated).
 * @param {any} value the root of the remaining path
 * @param {Array<string|number>} path
 * @param {number} i the position in `path` reached so far
 * @param {unknown} v the new value
 * @returns {unknown}
 */
function pathSet(value, path, i, v) {
  if (i === path.length) return v;
  const k = path[i];
  const inner = pathSet(value[k], path, i + 1, v);
  if (Array.isArray(value)) {
    const copy = value.slice();
    copy[k] = inner;
    return copy;
  }
  return { ...value, [k]: inner };
}

/**
 * A path store backing a `@State` struct (docs/compiler/runtime.md §3.3):
 * signals are created lazily per accessed path; a write replaces the value
 * along the path (the struct path copy) and notifies readers of that path,
 * of its ancestors and of its descendants, but not of its siblings.
 */
export class Store {
  /**
   * Creates a store holding `initial`.
   * @param {unknown} initial
   */
  constructor(initial) {
    this.value = initial;
    /** @type {Map<string, {path: string[], signal: Signal}>} */
    this.paths = new Map();
  }
  /**
   * Reads the value at `path` (`[]` is the whole value), tracked.
   * @param {string[]} path
   * @returns {unknown}
   */
  get(path) {
    const key = path.join(".");
    let entry = this.paths.get(key);
    if (entry === undefined) {
      entry = { path, signal: signal(pathGet(this.value, path)) };
      this.paths.set(key, entry);
    }
    return entry.signal.get();
  }
  /**
   * Writes `value` at `path`.
   * @param {string[]} path
   * @param {unknown} value
   */
  set(path, value) {
    this.value = pathSet(this.value, path, 0, value);
    batch(() => {
      for (const entry of this.paths.values()) {
        const p = entry.path;
        if (p.every((k, i) => i >= path.length || k === path[i])) entry.signal.set(pathGet(this.value, p));
      }
    });
  }
  /**
   * A `{ get, set }` binding pair over `path` (`bind user.address.city`).
   * @param {string[]} path
   * @returns {{get: () => unknown, set: (v: unknown) => void}}
   */
  binding(path) {
    return { get: () => this.get(path), set: (v) => this.set(path, v) };
  }
}

/**
 * Creates a {@link Store}.
 * @param {unknown} initial
 * @returns {Store}
 */
export function store(initial) {
  return new Store(initial);
}
