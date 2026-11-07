/**
 * @module k:runtime/store
 *
 * Path stores for `@State` structs (docs/compiler/runtime.md §3.3).
 */

import { batch, signal } from "./signals.js";

function pathGet(value, path) {
  let v = value;
  for (const k of path) {
    if (v === null || v === undefined) return undefined;
    v = v[k];
  }
  return v;
}

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
  /** @param {unknown} initial */
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
        const n = Math.min(p.length, path.length);
        let related = true;
        for (let i = 0; i < n; i++) {
          if (p[i] !== path[i]) {
            related = false;
            break;
          }
        }
        if (related) entry.signal.set(pathGet(this.value, p));
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
