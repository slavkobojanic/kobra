/**
 * @module k:runtime/std
 *
 * Native bodies of the K standard library (docs/compiler/runtime.md §12):
 * collection, string and number helpers that generated code calls through
 * `$k`, the exact `Decimal` and `Money` types, `Instant` (K's `Date`),
 * hash collections with structural keys, JSON codecs and the helpers of
 * typed errors and async cancellation (docs/compiler/codegen.md §5–§8).
 *
 * Panics use the messages the language specifies ("index out of range: 5
 * is not in 0..<1", "currency mismatch: USD and EUR").
 */

import { CANCELLED, KPanic, checkInt, equals, isPanic, panic } from "./values.js";

// ------------------------------------------------------------------ values

/**
 * The inner `nil` of a nested optional (`T??`, only through generics such
 * as `List<Int?>.first`): distinct from the outer `null`.
 */
export const NONE = Object.freeze({
  /**
   * The text of the inner `nil`.
   * @returns {string} the text of the inner `nil`
   */
  toString: () => "nil",
  /**
   * Encodes as JSON `null`.
   * @returns {null}
   */
  toJSON: () => null,
});

/**
 * Wraps a possibly-`null` element as a nested optional's inner value.
 * @param {unknown} v
 * @returns {unknown}
 */
function some(v) {
  return v === null ? NONE : v;
}

/**
 * Panics: generated `default` arms of exhaustive matches, which only corrupt
 * data could reach.
 * @returns {never}
 */
export function unreachable() {
  return panic("unreachable code was reached");
}

/**
 * Formats a `Double` for interpolation: the shortest round-trip form with at
 * least one decimal (`2.0`, `0.1`, `1e+21`).
 * @param {number} n
 * @returns {string}
 */
export function formatDouble(n) {
  if (Number.isInteger(n) && Math.abs(n) < 1e21) return (Object.is(n, -0) ? "-0" : String(n)) + ".0";
  return String(n);
}

/**
 * The interpolation text of an optional: `nil`, or `format(value)`.
 * @param {unknown} v
 * @param {(v: any) => string} format
 * @returns {string}
 */
export function optText(v, format) {
  return v === null || v === undefined || v === NONE ? "nil" : format(v);
}

/**
 * The text of a list: `[a, b]` with each element formatted.
 * @param {unknown[]} xs
 * @param {(v: any) => string} format
 * @returns {string}
 */
export function listText(xs, format) {
  return "[" + xs.map((x) => format(x)).join(", ") + "]";
}

/**
 * Applies `f` to a value unless it is `nil`.
 * @param {unknown} v
 * @param {(v: any) => unknown} f
 * @returns {unknown}
 */
export function mapOpt(v, f) {
  return v === null ? null : f(v);
}

/**
 * A copy of a collection for copy-on-write: arrays, `Map`, `Set` and the
 * hash collections.
 * @template T
 * @param {T} c
 * @returns {T}
 */
export function copy(c) {
  if (Array.isArray(c)) return c.slice();
  if (c instanceof HashMap) return new HashMap(c);
  if (c instanceof HashSet) return new HashSet(c);
  if (c instanceof Map) return new Map(c);
  if (c instanceof Set) return new Set(c);
  return c;
}

/**
 * Logs a line at `level` (`debug`, `info`, `warning`, `error`).
 * @param {string} level
 * @param {string} message
 */
export function log(level, message) {
  const c = globalThis.console;
  if (level === "debug") c.debug(message);
  else if (level === "warning") c.warn(message);
  else if (level === "error") c.error(message);
  else c.info(message);
}

/**
 * Returns a promise that settles like `p` while `signal` is not aborted, and
 * never settles once it is: a cancelled `await` never resumes
 * (docs/compiler/codegen.md §8).
 * @template T
 * @param {AbortSignal | null} signal
 * @param {Promise<T> | T} p
 * @returns {Promise<T>}
 */
export function guard(signal, p) {
  if (!signal) return Promise.resolve(p);
  if (signal.aborted) return new Promise(() => {});
  return new Promise((resolve, reject) => {
    Promise.resolve(p).then(
      (v) => { if (!signal.aborted) resolve(v); },
      (e) => { if (!signal.aborted) reject(e); },
    );
  });
}

/**
 * The value an `await` resumed with, while `signal` is live; once it is
 * aborted, throws {@link CANCELLED}, which stops the body. Generated code
 * wraps every `await` of a body with a cancellation context in it, and
 * calls it with no value first thing in each `catch`: a promise can settle
 * while the run is live and the run be cancelled before the body resumes,
 * so this check, not {@link guard}, is what keeps code after an `await`
 * from running on a disposed page (docs/compiler/codegen.md §8).
 * @template T
 * @param {AbortSignal | null} signal
 * @param {T} [v]
 * @returns {T}
 */
export function live(signal, v) {
  if (signal?.aborted) throw CANCELLED;
  return v;
}

/**
 * Converts a non-K exception into a panic; K values and panics pass through.
 * @param {unknown} e
 * @returns {unknown}
 */
export function toPanic(e) {
  if (e instanceof Error && !(e instanceof KPanic)) return new KPanic(e.message);
  return e;
}

/**
 * `Result.of { … }`: the block's value as `.success`, a typed error as
 * `.failure`; panics propagate.
 * @param {() => unknown} block
 * @param {{success: Function, failure: Function}} Result the generated enum
 * @returns {unknown}
 */
export function resultOf(block, Result) {
  try {
    return Result.success(block());
  } catch (e) {
    if (isPanic(e) || e instanceof Error) throw toPanic(e);
    return Result.failure(e);
  }
}

/**
 * Runs an execution-corpus `main` and returns its result line
 * (docs/compiler/testing.md §3.10).
 * @param {() => unknown} invoke calls `main`
 * @param {((v: any) => unknown) | null} encode the result's codec, `null` for `Void`
 * @param {((e: any) => unknown) | null} describe describes a typed error
 * @returns {Promise<string>}
 */
export async function runMain(invoke, encode, describe) {
  try {
    const v = await invoke();
    if (!encode) return "returns void";
    return "returns " + stringify(encode(v));
  } catch (e) {
    if (isPanic(e) || e instanceof Error) return "panics " + e.message;
    if (describe) return "throws " + stringify(describe(e));
    return "throws " + stringify(toJSON(e));
  }
}

/**
 * The payload of an enum value as JSON fields (everything but the tag):
 * with `encode` (a Codable error's encoder) the codec's form without its
 * `case` key, so the client can decode it with the same codec; without,
 * the payload's structural JSON.
 * @param {object} e
 * @param {(v: object) => unknown} [encode]
 * @returns {object}
 */
export function payloadFields(e, encode) {
  const out = {};
  const j = encode ? encode(e) : e;
  if (j && typeof j === "object") for (const k of Object.keys(j)) if (k !== "$" && k !== "case") out[k] = encode ? j[k] : toJSON(j[k]);
  return out;
}

/**
 * A structural JSON form of any value (for values without a codec).
 * @param {unknown} v
 * @returns {unknown}
 */
export function toJSON(v) {
  if (v === null || v === undefined || v === NONE) return null;
  if (Array.isArray(v)) return v.map(toJSON);
  if (v instanceof Map || v instanceof HashMap) {
    const o = {};
    for (const [k, x] of v) o[String(k)] = toJSON(x);
    return o;
  }
  if (v instanceof Set || v instanceof HashSet) return [...v].map(toJSON);
  if (typeof v === "object" && typeof v.toJSON === "function") return v.toJSON();
  if (typeof v === "object") {
    const o = {};
    for (const k of Object.keys(v)) o[k] = toJSON(v[k]);
    return o;
  }
  return v;
}

/**
 * A `Secret<T>`: the value is reachable only through `reveal()`, and its
 * text and JSON forms are redacted.
 */
export class Secret {
  /**
   * Wraps `value`, kept in a non-enumerable property so it never serializes.
   * @param {unknown} value
   */
  constructor(value) {
    Object.defineProperty(this, "v", { value, enumerable: false });
  }
  /**
   * The wrapped value.
   * @returns {unknown} the wrapped value
   */
  reveal() {
    return this.v;
  }
  /**
   * Compares with `candidate` in constant time (for strings).
   * @param {unknown} candidate
   * @returns {boolean}
   */
  matches(candidate) {
    const a = String(this.v);
    const b = String(candidate);
    let diff = a.length ^ b.length;
    for (let i = 0; i < Math.max(a.length, b.length); i++) diff |= (a.charCodeAt(i) | 0) ^ (b.charCodeAt(i) | 0);
    return diff === 0;
  }
  /**
   * The redacted text, `Secret(•••)`.
   * @returns {string}
   */
  toString() {
    return "Secret(•••)";
  }
  /**
   * The redacted JSON form, `"Secret(•••)"`.
   * @returns {string}
   */
  toJSON() {
    return "Secret(•••)";
  }
}

// ------------------------------------------------------------- comparison

/**
 * Orders two values of a `Comparable` type: numbers, strings (by UTF-16 code
 * units), booleans, and values with a `compare` method (`Decimal`, `Money`,
 * `Instant`); enums by tag, lists lexicographically.
 * @param {any} a
 * @param {any} b
 * @returns {number} negative, zero or positive
 */
export function compare(a, b) {
  if (typeof a === "number" || typeof a === "string" || typeof a === "boolean") return a < b ? -1 : a > b ? 1 : 0;
  if (a && typeof a.compare === "function") return a.compare(b);
  if (Array.isArray(a)) {
    for (let i = 0; i < Math.min(a.length, b.length); i++) {
      const c = compare(a[i], b[i]);
      if (c !== 0) return c;
    }
    return a.length - b.length;
  }
  if (a && typeof a === "object" && "$" in a) return a.$ - b.$;
  return 0;
}

/**
 * A comparator by `key`, ascending.
 * @param {(v: any) => any} key
 * @returns {(a: any, b: any) => number}
 */
export function byKey(key) {
  return (a, b) => compare(key(a), key(b));
}

/**
 * A comparator by `key`, descending.
 * @param {(v: any) => any} key
 * @returns {(a: any, b: any) => number}
 */
export function byKeyDesc(key) {
  return (a, b) => compare(key(b), key(a));
}

/**
 * A comparator by several `[key, descending]` pairs (`sortedBy` followed by
 * `thenBy`).
 * @param {...[(v: any) => any, boolean]} keys
 * @returns {(a: any, b: any) => number}
 */
export function byKeys(...keys) {
  return (a, b) => {
    for (const [key, desc] of keys) {
      const c = desc ? compare(key(b), key(a)) : compare(key(a), key(b));
      if (c !== 0) return c;
    }
    return 0;
  };
}

/**
 * A structural hash agreeing with {@link equals}.
 * @param {unknown} v
 * @returns {string}
 */
export function hash(v) {
  if (v === null || v === undefined) return "n";
  if (typeof v === "number") return "d" + (Number.isNaN(v) ? "NaN" : String(v));
  if (typeof v === "string") return "s" + v.length + ":" + v;
  if (typeof v === "boolean") return v ? "t" : "f";
  if (Array.isArray(v)) return "[" + v.map(hash).join(",") + "]";
  if (typeof v.hashKey === "function") return v.hashKey();
  if (v instanceof Map || v instanceof HashMap) return "{" + [...v].map(([k, x]) => hash(k) + "=" + hash(x)).sort().join(",") + "}";
  if (v instanceof Set || v instanceof HashSet) return "<" + [...v].map(hash).sort().join(",") + ">";
  return "(" + Object.keys(v).map((k) => k + ":" + hash(v[k])).join(",") + ")";
}

/**
 * A `Map` whose keys compare structurally (struct keys). Iterates in
 * insertion order like `Map`.
 */
export class HashMap {
  /**
   * Creates a map holding `entries`, later duplicates replacing earlier ones.
   * @param {Iterable<[any, any]>} [entries]
   */
  constructor(entries) {
    this.m = new Map();
    if (entries) for (const [k, v] of entries) this.set(k, v);
  }
  /**
   * The number of entries.
   * @returns {number}
   */
  get size() {
    return this.m.size;
  }
  /**
   * The value of key `k` (compared structurally), or `undefined`.
   * @param {any} k
   * @returns {any}
   */
  get(k) {
    const e = this.m.get(hash(k));
    return e === undefined ? undefined : e[1];
  }
  /**
   * Sets the value of key `k`, keeping the first-inserted key object and its position.
   * @param {any} k
   * @param {any} v
   * @returns {HashMap}
   */
  set(k, v) {
    const h = hash(k);
    const e = this.m.get(h);
    if (e) e[1] = v;
    else this.m.set(h, [k, v]);
    return this;
  }
  /**
   * `true` if the map has key `k`.
   * @param {any} k
   * @returns {boolean}
   */
  has(k) {
    return this.m.has(hash(k));
  }
  /**
   * Removes key `k`; `true` if it was present.
   * @param {any} k
   * @returns {boolean}
   */
  delete(k) {
    return this.m.delete(hash(k));
  }
  /**
   * The keys in insertion order.
   * @returns {IterableIterator<any>}
   */
  *keys() {
    for (const e of this.m.values()) yield e[0];
  }
  /**
   * The values in insertion order.
   * @returns {IterableIterator<any>}
   */
  *values() {
    for (const e of this.m.values()) yield e[1];
  }
  /**
   * The `[key, value]` entries in insertion order.
   * @returns {IterableIterator<[any, any]>}
   */
  *[Symbol.iterator]() {
    for (const e of this.m.values()) yield [e[0], e[1]];
  }
  /**
   * Structural equality: the same keys with structurally equal values, in any order.
   * @param {any} o
   * @returns {boolean}
   */
  equals(o) {
    if (!(o instanceof HashMap) || o.size !== this.size) return false;
    for (const [k, v] of this) if (!o.has(k) || !equals(v, o.get(k))) return false;
    return true;
  }
}

/** A `Set` whose elements compare structurally. */
export class HashSet {
  /**
   * Creates a set holding `items`.
   * @param {Iterable<any>} [items]
   */
  constructor(items) {
    this.m = new Map();
    if (items) for (const v of items) this.add(v);
  }
  /**
   * The number of elements.
   * @returns {number}
   */
  get size() {
    return this.m.size;
  }
  /**
   * Adds `v` unless a structurally equal element is present.
   * @param {any} v
   * @returns {HashSet}
   */
  add(v) {
    const h = hash(v);
    if (!this.m.has(h)) this.m.set(h, v);
    return this;
  }
  /**
   * `true` if the set has an element structurally equal to `v`.
   * @param {any} v
   * @returns {boolean}
   */
  has(v) {
    return this.m.has(hash(v));
  }
  /**
   * Removes the element structurally equal to `v`; `true` if one was present.
   * @param {any} v
   * @returns {boolean}
   */
  delete(v) {
    return this.m.delete(hash(v));
  }
  /**
   * The elements in insertion order.
   * @returns {IterableIterator<any>}
   */
  [Symbol.iterator]() {
    return this.m.values();
  }
  /**
   * Structural equality: the same elements, in any order.
   * @param {any} o
   * @returns {boolean}
   */
  equals(o) {
    if (!(o instanceof HashSet) || o.size !== this.size) return false;
    for (const v of this) if (!o.has(v)) return false;
    return true;
  }
}

// ------------------------------------------------------------ collections

/**
 * Panics with the out-of-range message for index `i` of a list of `n`
 * elements.
 * @param {number} i
 * @param {number} n
 * @returns {never}
 */
function outOfRange(i, n) {
  return panic(`index out of range: ${i} is not in 0..<${n}`);
}

/**
 * `xs[i]`: panics when `i` is out of range.
 * @param {any[]} xs
 * @param {number} i
 * @returns {any}
 */
export function at(xs, i) {
  if (!(i >= 0 && i < xs.length)) outOfRange(i, xs.length);
  return xs[i];
}

/**
 * `xs[i] = v` on an owned array; panics when out of range.
 * @param {any[]} xs
 * @param {number} i
 * @param {any} v
 * @returns {any[]}
 */
export function setAt(xs, i, v) {
  if (!(i >= 0 && i < xs.length)) outOfRange(i, xs.length);
  xs[i] = v;
  return xs;
}

/**
 * `insert(e, at: i)`; `i` may equal the count.
 * @param {any[]} xs
 * @param {any} v
 * @param {number} i
 */
export function insertAt(xs, v, i) {
  if (!(i >= 0 && i <= xs.length)) panic(`index out of range: ${i} is not in 0...${xs.length}`);
  xs.splice(i, 0, v);
}

/**
 * `remove(at: i)`: removes and returns the element.
 * @param {any[]} xs
 * @param {number} i
 * @returns {any}
 */
export function removeAt(xs, i) {
  if (!(i >= 0 && i < xs.length)) outOfRange(i, xs.length);
  return xs.splice(i, 1)[0];
}

/**
 * `removeWhere`: removes in place the elements matching `f`.
 * @param {any[]} xs
 * @param {(v: any) => boolean} f
 */
export function removeWhere(xs, f) {
  let j = 0;
  for (let i = 0; i < xs.length; i++) if (!f(xs[i])) xs[j++] = xs[i];
  xs.length = j;
}

/**
 * `first`, or `null` for an empty list.
 * @param {any[]} xs
 * @returns {any}
 */
export function first(xs) {
  return xs.length ? xs[0] : null;
}

/**
 * `last`, or `null` for an empty list.
 * @param {any[]} xs
 * @returns {any}
 */
export function last(xs) {
  return xs.length ? xs[xs.length - 1] : null;
}

/**
 * `first` of a list of optionals: `NONE` for a `nil` element.
 * @param {any[]} xs
 * @returns {any}
 */
export function firstNested(xs) {
  return xs.length ? some(xs[0]) : null;
}

/**
 * `last` of a list of optionals.
 * @param {any[]} xs
 * @returns {any}
 */
export function lastNested(xs) {
  return xs.length ? some(xs[xs.length - 1]) : null;
}

/**
 * `compactMap`: the non-`nil` results.
 * @param {any[]} xs
 * @param {(v: any) => any} f
 * @returns {any[]}
 */
export function compactMap(xs, f) {
  const out = [];
  for (const x of xs) {
    const y = f(x);
    if (y !== null && y !== undefined) out.push(y);
  }
  return out;
}

/**
 * `distinct`: first occurrences, in order.
 * @param {any[]} xs
 * @param {boolean} primitive keys compare with `===`
 * @returns {any[]}
 */
export function distinct(xs, primitive) {
  const seen = primitive ? new Set() : new HashSet();
  const out = [];
  for (const x of xs) {
    if (!seen.has(x)) {
      seen.add(x);
      out.push(x);
    }
  }
  return out;
}

/**
 * `indexWhere`, or `null`.
 * @param {any[]} xs
 * @param {(v: any) => boolean} f
 * @returns {number | null}
 */
export function indexWhere(xs, f) {
  for (let i = 0; i < xs.length; i++) if (f(xs[i])) return i;
  return null;
}

/**
 * `firstWhere`, or `null`.
 * @param {any[]} xs
 * @param {(v: any) => boolean} f
 * @returns {any}
 */
export function firstWhere(xs, f) {
  for (const x of xs) if (f(x)) return x;
  return null;
}

/**
 * `firstWhere` on a list of optionals.
 * @param {any[]} xs
 * @param {(v: any) => boolean} f
 * @returns {any}
 */
export function firstWhereNested(xs, f) {
  for (const x of xs) if (f(x)) return some(x);
  return null;
}

/**
 * `count(where:)`.
 * @param {any[]} xs
 * @param {(v: any) => boolean} f
 * @returns {number}
 */
export function countWhere(xs, f) {
  let n = 0;
  for (const x of xs) if (f(x)) n++;
  return n;
}

/**
 * `groupedBy`: a map from key to the elements with it, in first-seen order.
 * @param {any[]} xs
 * @param {(v: any) => any} key
 * @param {boolean} primitive
 * @returns {Map<any, any[]> | HashMap}
 */
export function groupedBy(xs, key, primitive) {
  const m = primitive ? new Map() : new HashMap();
  for (const x of xs) {
    const k = key(x);
    const g = m.get(k);
    if (g === undefined) m.set(k, [x]);
    else g.push(x);
  }
  return m;
}

/**
 * `partitioned`: `[matching, rest]`.
 * @param {any[]} xs
 * @param {(v: any) => boolean} f
 * @returns {[any[], any[]]}
 */
export function partitioned(xs, f) {
  const a = [];
  const b = [];
  for (const x of xs) (f(x) ? a : b).push(x);
  return [a, b];
}

/**
 * `chunked(size)`; panics for a size below 1.
 * @param {any[]} xs
 * @param {number} n
 * @returns {any[][]}
 */
export function chunked(xs, n) {
  if (n < 1) panic(`chunk size must be at least 1, not ${n}`);
  const out = [];
  for (let i = 0; i < xs.length; i += n) out.push(xs.slice(i, i + n));
  return out;
}

/**
 * `windowed(size)`; panics for a size below 1.
 * @param {any[]} xs
 * @param {number} n
 * @returns {any[][]}
 */
export function windowed(xs, n) {
  if (n < 1) panic(`window size must be at least 1, not ${n}`);
  const out = [];
  for (let i = 0; i + n <= xs.length; i++) out.push(xs.slice(i, i + n));
  return out;
}

/**
 * `prefix(n)`: at most the first `n` elements.
 * @param {any[]} xs
 * @param {number} n
 * @returns {any[]}
 */
export function prefix(xs, n) {
  return xs.slice(0, Math.max(0, n));
}

/**
 * The first element of `xs` whose `key` is smallest (`sign` 1) or largest
 * (`sign` -1) by `compare`; `null` for an empty list. Linear.
 * @param {any[]} xs
 * @param {(x: any) => unknown} key
 * @param {1|-1} sign
 * @returns {any}
 */
function extreme(xs, key, sign) {
  if (!xs.length) return null;
  let best = xs[0];
  let bk = key(best);
  for (let i = 1; i < xs.length; i++) {
    const k = key(xs[i]);
    if (compare(k, bk) * sign < 0) {
      best = xs[i];
      bk = k;
    }
  }
  return best;
}

/**
 * `minBy`: the first element with the smallest key, or `null`.
 * @param {any[]} xs
 * @param {(v: any) => any} key
 * @returns {any}
 */
export function minBy(xs, key) {
  return extreme(xs, key, 1);
}

/**
 * `maxBy`: the first element with the largest key, or `null`.
 * @param {any[]} xs
 * @param {(v: any) => any} key
 * @returns {any}
 */
export function maxBy(xs, key) {
  return extreme(xs, key, -1);
}

/**
 * `minBy` on a list of optionals.
 * @param {any[]} xs
 * @param {(v: any) => any} key
 * @returns {any}
 */
export function minByNested(xs, key) {
  return xs.length ? some(extreme(xs, key, 1)) : null;
}

/**
 * `maxBy` on a list of optionals.
 * @param {any[]} xs
 * @param {(v: any) => any} key
 * @returns {any}
 */
export function maxByNested(xs, key) {
  return xs.length ? some(extreme(xs, key, -1)) : null;
}

/**
 * `contains` with structural equality.
 * @param {any[]} xs
 * @param {any} v
 * @returns {boolean}
 */
export function contains(xs, v) {
  for (const x of xs) if (equals(x, v)) return true;
  return false;
}

/**
 * `enumerated`: `[index, element]` pairs.
 * @param {any[]} xs
 * @returns {[number, any][]}
 */
export function enumerated(xs) {
  return xs.map((x, i) => [i, x]);
}

/**
 * `map[key]` / `get`: the value or `null`.
 * @param {Map<any, any> | HashMap} m
 * @param {any} k
 * @returns {any}
 */
export function mapGet(m, k) {
  const v = m.get(k);
  return v === undefined ? null : v;
}

/**
 * `map[key]` on a map of optionals: `NONE` for a stored `nil`.
 * @param {Map<any, any> | HashMap} m
 * @param {any} k
 * @returns {any}
 */
export function mapGetNested(m, k) {
  return m.has(k) ? some(m.get(k)) : null;
}

/**
 * Sets an entry of an owned map and returns it.
 * @param {Map<any, any> | HashMap} m
 * @param {any} k
 * @param {any} v
 * @returns {Map<any, any> | HashMap}
 */
export function mapSet(m, k, v) {
  m.set(k, v);
  return m;
}

/**
 * Removes an entry of an owned map and returns it (`map[key] = nil`).
 * @param {Map<any, any> | HashMap} m
 * @param {any} k
 * @returns {Map<any, any> | HashMap}
 */
export function mapRemoved(m, k) {
  m.delete(k);
  return m;
}

/** A range `lo..<hi` or `lo...hi`. */
class Range {
  /**
   * @param {any} lo
   * @param {any} hi
   * @param {boolean} closed
   */
  constructor(lo, hi, closed) {
    this.lo = lo;
    this.hi = hi;
    this.closed = closed;
  }
  /**
   * `true` if `v` lies in the range (`compare`), the upper bound included only when closed.
   * @param {any} v
   * @returns {boolean}
   */
  contains(v) {
    return compare(v, this.lo) >= 0 && (this.closed ? compare(v, this.hi) <= 0 : compare(v, this.hi) < 0);
  }
  /**
   * Structural equality: the same bounds and the same kind.
   * @param {any} o
   * @returns {boolean}
   */
  equals(o) {
    return o instanceof Range && o.closed === this.closed && equals(o.lo, this.lo) && equals(o.hi, this.hi);
  }
  /**
   * The JSON form, `{ lowerBound, upperBound }`.
   * @returns {object}
   */
  toJSON() {
    return { lowerBound: toJSON(this.lo), upperBound: toJSON(this.hi) };
  }
}

/**
 * `lo..<hi`.
 * @param {any} lo
 * @param {any} hi
 * @returns {Range}
 */
export function range(lo, hi) {
  return new Range(lo, hi, false);
}

/**
 * `lo...hi`.
 * @param {any} lo
 * @param {any} hi
 * @returns {Range}
 */
export function closedRange(lo, hi) {
  return new Range(lo, hi, true);
}

/**
 * The values of an `Int` range, in order.
 * @param {Range} r
 * @returns {number[]}
 */
export function rangeValues(r) {
  const out = [];
  const end = r.closed ? r.hi : r.hi - 1;
  for (let i = r.lo; i <= end; i++) out.push(i);
  return out;
}

// ---------------------------------------------------------------- strings

/**
 * The grapheme segmenter behind `graphemes`, or `null` where
 * `Intl.Segmenter` is missing (then code points are used).
 */
const segmenter = typeof Intl !== "undefined" && Intl.Segmenter ? new Intl.Segmenter() : null;

/**
 * The characters of a string: grapheme clusters through `Intl.Segmenter`
 * (every supported browser, and the server's V8, which has ICU; a test
 * guards it), code points where it is missing.
 * @param {string} s
 * @returns {string[]}
 */
function graphemes(s) {
  return segmenter ? Array.from(segmenter.segment(s), (x) => x.segment) : Array.from(s);
}

/**
 * `String.count`: the number of characters (grapheme clusters).
 * @param {string} s
 * @returns {number}
 */
export function graphemeCount(s) {
  let ascii = true;
  for (let i = 0; i < s.length; i++) {
    if (s.charCodeAt(i) > 0x7e) {
      ascii = false;
      break;
    }
  }
  return ascii ? s.length : graphemes(s).length;
}

/**
 * The UTF-8 bytes of a string.
 * @param {string} s
 * @returns {Uint8Array}
 */
export function utf8(s) {
  if (typeof TextEncoder === "function") return new TextEncoder().encode(s);
  const out = [];
  for (const ch of s) {
    const c = ch.codePointAt(0);
    if (c < 0x80) out.push(c);
    else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
    else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
  }
  return Uint8Array.from(out);
}

/**
 * `split(separator:limit:)`.
 * @param {string} s
 * @param {string} sep
 * @param {number | null} [limit]
 * @returns {string[]}
 */
export function split(s, sep, limit) {
  const parts = s.split(sep);
  if (limit === undefined || limit === null || limit < 1 || parts.length <= limit) return parts;
  return [...parts.slice(0, limit - 1), parts.slice(limit - 1).join(sep)];
}

/**
 * `prefix(n)` of a string: at most `n` characters.
 * @param {string} s
 * @param {number} n
 * @returns {string}
 */
export function strPrefix(s, n) {
  return graphemes(s).slice(0, Math.max(0, n)).join("");
}

/**
 * `repeated(times:)`; panics for a negative count.
 * @param {string} s
 * @param {number} n
 * @returns {string}
 */
export function repeat(s, n) {
  if (n < 0) panic(`repeat count must not be negative: ${n}`);
  return s.repeat(n);
}

/**
 * `truncated(length:ending:)`: at most `length` characters, `ending`
 * included when cut.
 * @param {string} s
 * @param {number} length
 * @param {string} [ending]
 * @returns {string}
 */
export function truncated(s, length, ending = "…") {
  const g = graphemes(s);
  if (g.length <= length) return s;
  const e = graphemes(ending).length;
  return g.slice(0, Math.max(0, length - e)).join("") + ending;
}

// ---------------------------------------------------------------- numbers

/**
 * `Int.parse`: a decimal integer in the safe range, or `null`.
 * @param {string} s
 * @returns {number | null}
 */
export function intParse(s) {
  const t = s.trim();
  if (t === "" || t !== s) return null;
  let i = t[0] === "-" || t[0] === "+" ? 1 : 0;
  if (i === t.length) return null;
  for (; i < t.length; i++) if (t.charCodeAt(i) < 48 || t.charCodeAt(i) > 57) return null;
  const n = Number(t);
  return Number.isSafeInteger(n) ? n : null;
}

/**
 * `Double.parse`: a decimal number, or `null`.
 * @param {string} s
 * @returns {number | null}
 */
export function doubleParse(s) {
  if (s.trim() === "" || s.trim() !== s) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

/**
 * `Double.rounded()`: to the nearest integer, halves away from zero.
 * @param {number} x
 * @returns {number}
 */
export function round(x) {
  const r = Math.sign(x) * Math.round(Math.abs(x));
  return checkInt(r === 0 ? 0 : r);
}

/**
 * Formats a number in a `NumberStyle` (by tag: decimal, wholeNumber,
 * percent, currency).
 * @param {number} n
 * @param {{$: number}} style
 * @returns {string}
 */
export function formatNumber(n, style) {
  const tag = style ? style.$ : 0;
  if (typeof Intl === "undefined") return String(n);
  if (tag === 1) return new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 }).format(n);
  if (tag === 2) return new Intl.NumberFormat("en-US", { style: "percent" }).format(n);
  return new Intl.NumberFormat("en-US").format(n);
}

/**
 * Builds CSS `calc()` text from `a op b`, flattening nested `calc(…)`.
 * @param {string} text
 * @returns {string}
 */
export function calc(text) {
  let t = String(text);
  // A leading `calc(…)` operand of `+`/`-` is flattened; others nest.
  if (t.startsWith("calc(")) {
    let depth = 0;
    let end = -1;
    for (let i = 4; i < t.length; i++) {
      if (t[i] === "(") depth++;
      else if (t[i] === ")" && --depth === 0) {
        end = i;
        break;
      }
    }
    const rest = t.slice(end + 1);
    if (end > 0 && (rest.startsWith(" + ") || rest.startsWith(" - "))) t = t.slice(5, end) + rest;
  }
  return "calc(" + t.split("calc(").join("(") + ")";
}

/**
 * `Color.hex(text)`: the text when it is a hex color (`#RGB`, `#RGBA`,
 * `#RRGGBB`, `#RRGGBBAA`); panics otherwise, so runtime text never
 * reaches a `style` value as anything but a color.
 * @param {string} text
 * @returns {string}
 */
export function colorHex(text) {
  return HEX_RE.test(text) ? text : panic(`not a hex color: ${JSON.stringify(text)}`);
}

/** A hex color: `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`. */
const HEX_RE = new RegExp("^#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$");

/**
 * A color with its alpha multiplied (`color-mix` for non-literal colors).
 * @param {string} color
 * @param {number} alpha
 * @returns {string}
 */
export function colorOpacity(color, alpha) {
  if (color.length === 7 && color[0] === "#") {
    const a = Math.round(alpha * 255).toString(16).toUpperCase().padStart(2, "0");
    return color + a;
  }
  return `color-mix(in srgb, ${color} ${Math.round(alpha * 100)}%, transparent)`;
}

/**
 * `a.mix(b, percent)`: `percent` of `b` mixed into `a`, so
 * `.primary.mix(.white, 20%)` is the primary color lightened by 20% white
 * (07-themes-and-units.md, "Colors").
 * @param {string} a
 * @param {string} b
 * @param {number} percent the share of `b`, `0...100`
 * @returns {string}
 */
export function colorMix(a, b, percent) {
  return `color-mix(in srgb, ${a}, ${b} ${percent}%)`;
}

/**
 * A color from components.
 * @param {number} r
 * @param {number} g
 * @param {number} b
 * @returns {string}
 */
export function colorRgb(r, g, b) {
  const h = (v) => Math.max(0, Math.min(255, v)).toString(16).toUpperCase().padStart(2, "0");
  return "#" + h(r) + h(g) + h(b);
}

/**
 * `Bytes.hex`: lower-case hexadecimal.
 * @param {Uint8Array} b
 * @returns {string}
 */
export function bytesHex(b) {
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}

/** The standard base64 alphabet, by digit value. */
const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/**
 * `Bytes.base64`: standard base64 with padding, without `btoa` (which the
 * server's isolate doesn't have).
 * @param {Uint8Array} b
 * @returns {string}
 */
export function bytesBase64(b) {
  let s = "";
  for (let i = 0; i < b.length; i += 3) {
    const n = (b[i] << 16) | ((b[i + 1] ?? 0) << 8) | (b[i + 2] ?? 0);
    s += B64[n >> 18] + B64[(n >> 12) & 63] + (i + 1 < b.length ? B64[(n >> 6) & 63] : "=") + (i + 2 < b.length ? B64[n & 63] : "=");
  }
  return s;
}

/**
 * `Bytes.utf8Text`: the bytes decoded as UTF-8, or `null` if invalid.
 * @param {Uint8Array} b
 * @returns {string | null}
 */
export function bytesUtf8(b) {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(b);
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------- Instant

/** K's `Date`: an instant, in epoch milliseconds. */
export class Instant {
  /**
   * Creates the instant `ms` milliseconds after the Unix epoch.
   * @param {number} ms
   */
  constructor(ms) {
    this.ms = ms;
  }
  /**
   * The instant `duration` milliseconds later.
   * @param {number} duration milliseconds
   * @returns {Instant}
   */
  plus(duration) {
    return new Instant(this.ms + duration);
  }
  /**
   * The duration from `o` to this instant.
   * @param {Instant} o
   * @returns {number} the duration between, in ms
   */
  minus(o) {
    return this.ms - o.ms;
  }
  /**
   * Whole seconds since the Unix epoch, rounded down.
   * @returns {number}
   */
  unixSeconds() {
    return Math.floor(this.ms / 1000);
  }
  /**
   * Compares in time: -1, 0 or 1.
   * @param {Instant} o
   * @returns {number}
   */
  compare(o) {
    return this.ms < o.ms ? -1 : this.ms > o.ms ? 1 : 0;
  }
  /**
   * Equality: the same millisecond.
   * @param {unknown} o
   * @returns {boolean}
   */
  equals(o) {
    return o instanceof Instant && o.ms === this.ms;
  }
  /**
   * A key equal for equal instants, for `HashMap` and `HashSet`.
   * @returns {string}
   */
  hashKey() {
    return "I" + this.ms;
  }
  /**
   * The JSON form.
   * @returns {string} RFC 3339 in UTC, milliseconds only when non-zero
   */
  toJSON() {
    const s = new Date(this.ms).toISOString();
    return s.endsWith(".000Z") ? s.slice(0, -5) + "Z" : s;
  }
  /**
   * The RFC 3339 text, as `toJSON`.
   * @returns {string}
   */
  toString() {
    return this.toJSON();
  }
  /**
   * Formats the instant (`DateStyle` by tag: date, time, dateTime,
   * relative).
   * @param {{$: number}} style
   * @param {Instant | null} [reference]
   * @returns {string}
   */
  formatted(style, reference) {
    const d = new Date(this.ms);
    const tag = style ? style.$ : 2;
    if (tag === 3 && reference) {
      const s = Math.round((this.ms - reference.ms) / 1000);
      const abs = Math.abs(s);
      const [n, unit] = abs < 60 ? [s, "second"] : abs < 3600 ? [Math.round(s / 60), "minute"] : abs < 86400 ? [Math.round(s / 3600), "hour"] : [Math.round(s / 86400), "day"];
      if (typeof Intl !== "undefined" && Intl.RelativeTimeFormat) return new Intl.RelativeTimeFormat("en", { numeric: "auto" }).format(n, unit);
      return `${n} ${unit}s`;
    }
    if (tag === 0) return d.toISOString().slice(0, 10);
    if (tag === 1) return d.toISOString().slice(11, 16);
    return d.toISOString().slice(0, 16).replace("T", " ");
  }
}

// --------------------------------------------------------------- TimeZone

/**
 * `TimeZone.parse`: an IANA zone name in its canonical spelling
 * (`europe/berlin` is `Europe/Berlin`), or `null` for a name the engine's
 * time zone data doesn't know. A `TimeZone` is its name: equal, hashable
 * and encoded as that text (35-time.md).
 * @param {string} name
 * @returns {string | null}
 */
export function parseTimeZone(name) {
  try {
    // An unknown name, and the empty one, is a `RangeError`.
    return new Intl.DateTimeFormat("en", { timeZone: name }).resolvedOptions().timeZone;
  } catch {
    return null;
  }
}

/**
 * `TimeZone.local` in the browser: the browser's zone, UTC when it reports
 * none. The server's is the `k_tz` cookie's (`requestTimeZone` of the
 * server runtime).
 * @returns {string}
 */
export function localTimeZone() {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
}

// -------------------------------------------------------------- TimeOfDay

/**
 * `TimeOfDay.parse`: `HH:MM` on a 24-hour clock, two ASCII digits each,
 * or `null`. A `TimeOfDay` is that text, so it is equal, hashable, printed
 * and encoded as itself (35-time.md); `k_sema::literals::time_of_day`
 * checks a `TimeOfDay("…")` literal by the same rule.
 * @param {unknown} text
 * @returns {string | null}
 */
export function parseTimeOfDay(text) {
  if (typeof text !== "string" || text.length !== 5 || text[2] !== ":") return null;
  for (const i of [0, 1, 3, 4]) {
    const c = text.charCodeAt(i);
    if (c < 48 || c > 57) return null;
  }
  return Number(text.slice(0, 2)) < 24 && Number(text.slice(3)) < 60 ? text : null;
}

/**
 * `TimeOfDay.hour`, `0...23`.
 * @param {string} t
 * @returns {number}
 */
export function timeOfDayHour(t) {
  return Number(t.slice(0, 2));
}

/**
 * `TimeOfDay.minute`, `0...59`.
 * @param {string} t
 * @returns {number}
 */
export function timeOfDayMinute(t) {
  return Number(t.slice(3));
}

/**
 * `Date(unixSeconds:)`.
 * @param {number} s
 * @returns {Instant}
 */
export function instantFromSeconds(s) {
  return new Instant(s * 1000);
}

/** An RFC 3339 date-time with a `Z` or numeric offset and optional fractional seconds. */
const RFC3339 = new RegExp("^(\\d{4})-(\\d{2})-(\\d{2})[Tt](\\d{2}):(\\d{2}):(\\d{2})(\\.\\d+)?([Zz]|[+-]\\d{2}:\\d{2})$");

/**
 * `Date.parse`: an RFC 3339 timestamp with a zone, or `null`.
 * @param {string} text
 * @returns {Instant | null}
 */
export function parseInstant(text) {
  const m = RFC3339.exec(text);
  if (!m) return null;
  const [y, mo, d, h, mi, s] = m.slice(1, 7).map(Number);
  if (mo < 1 || mo > 12 || d < 1 || d > 31 || h > 23 || mi > 59 || s > 60) return null;
  const frac = m[7] ? Math.floor(Number("0" + m[7]) * 1000) : 0;
  let ms = Date.UTC(y, mo - 1, d, h, mi, s, frac);
  if (m[8] !== "Z" && m[8] !== "z") {
    const sign = m[8][0] === "-" ? -1 : 1;
    const oh = Number(m[8].slice(1, 3));
    const om = Number(m[8].slice(4, 6));
    ms -= sign * (oh * 60 + om) * 60000;
  }
  return Number.isFinite(ms) ? new Instant(ms) : null;
}

// Typed text values (`Url`, `EmailAddress`, `Uuid`, `Color`) are their
// text at run time. These parsers are the only way in from runtime text
// (`T.parse`, JSON decoding, rpc parameters), on the client and the server
// alike, so neither side depends on `URL` or other platform classes.

/** Characters no `Url` contains: controls, spaces and backslashes. */
const URL_BAD = new RegExp("[\\u0000-\\u0020\\u007f\\\\]");
/** scheme `://` [userinfo `@`] host [`:` port] [path, query, fragment] */
const URL_RE = new RegExp("^([a-zA-Z][a-zA-Z0-9+.-]*)://(?:[^/?#@]*@)?(\\[[0-9a-fA-F:.]+\\]|[^/?#:@\\[\\]]+)(?::[0-9]{1,5})?([/?#].*)?$");
/** The schemes a `Url` may have: never `javascript:`, `data:` or another script scheme. */
const URL_SCHEMES = ["http", "https", "ws", "wss"];

/**
 * `Url.parse`: an absolute `http`, `https`, `ws` or `wss` URL with a host,
 * or `null`. Any other scheme (`javascript:`, `data:`, even with a host)
 * is refused, so a parsed or decoded `Url` is always safe as a link target.
 * @param {unknown} text
 * @returns {string | null}
 */
export function parseUrl(text) {
  if (typeof text !== "string" || URL_BAD.test(text)) return null;
  const m = URL_RE.exec(text);
  return m && URL_SCHEMES.includes(m[1].toLowerCase()) ? text : null;
}

/**
 * The host of a URL, lower-cased (`""` for a relative one, such as a
 * `Page`'s `url`).
 * @param {string} url
 * @returns {string}
 */
export function urlHost(url) {
  const m = URL_RE.exec(url);
  return m ? m[2].toLowerCase() : "";
}

/**
 * The codes of `HttpStatus`'s named cases, in declaration order (`other`
 * comes last and holds its own code). A codegen test keeps it equal to the
 * stdlib's enum.
 */
export const HTTP_CODES = [200, 201, 202, 204, 301, 303, 304, 400, 401, 403, 404, 405, 409, 410, 413, 415, 422, 429, 500, 502, 503, 504];

/**
 * `status.code` of an `HttpStatus` value.
 * @param {{$: number, _0?: number}} s
 * @returns {number}
 */
export function httpCode(s) {
  return HTTP_CODES[s.$] ?? s._0;
}

/**
 * The path of a URL (`/` when it has none), without query or fragment.
 * @param {string} url
 * @returns {string}
 */
export function urlPath(url) {
  const m = URL_RE.exec(url);
  const rest = m ? m[3] || "" : url;
  const path = rest.split(new RegExp("[?#]"))[0];
  return path.startsWith("/") ? path : "/" + path;
}

// ------------------------------------------------------------ route URLs

/**
 * Splits a request target into its path and query (both still encoded;
 * the query without its `?`).
 * @param {string} target
 * @returns {{pathname: string, search: string}}
 */
export function splitTarget(target) {
  const clean = target.split("#")[0];
  const q = clean.indexOf("?");
  return { pathname: q < 0 ? clean : clean.slice(0, q), search: q < 0 ? "" : clean.slice(q + 1) };
}

/**
 * Percent-decodes one URL component; `undefined` if it is malformed.
 * @param {string} text
 * @returns {string|undefined}
 */
function decodeComponent(text) {
  try {
    return decodeURIComponent(text);
  } catch {
    return undefined;
  }
}

/**
 * The query string as a map of decoded names to their first decoded value
 * (`+` is a space). Malformed pairs are skipped.
 * @param {string} search with or without its `?`
 * @returns {Map<string, string>}
 */
export function parseQuery(search) {
  const out = new Map();
  for (const pair of search.replace(new RegExp("^[?]"), "").split("&")) {
    const eq = pair.indexOf("=");
    const k = decodeComponent((eq < 0 ? pair : pair.slice(0, eq)).replaceAll("+", " "));
    const v = decodeComponent((eq < 0 ? "" : pair.slice(eq + 1)).replaceAll("+", " "));
    if (pair === "" || k === undefined || v === undefined || out.has(k)) continue;
    out.set(k, v);
  }
  return out;
}

/** A failed parameter parse: the URL doesn't match the route. */
export const NO = Symbol("k.no-match");

/** The text of an `Int` route or query parameter. */
const INT_RE = new RegExp("^-?[0-9]+$");
/** The text of a `Double` route or query parameter. */
const DOUBLE_RE = new RegExp("^-?[0-9]+(\\.[0-9]+)?([eE][-+]?[0-9]+)?$");

/**
 * Parsers of route parameters by type (12-routing.md, "Parameters"), the
 * same on the client and the server: each takes the decoded text and
 * returns the value, or {@link NO} for a non-match (a 404 on the server,
 * the next route on the client).
 */
export const routeParam = {
  /** `String`: any text; empty only in the query. */
  string: (t) => t,
  /** `Int`: an optional `-` and decimal digits, in the safe range. */
  int: (t) => (INT_RE.test(t) && Number.isSafeInteger(+t) ? +t : NO),
  /** `Double`: a finite decimal number. */
  double: (t) => (DOUBLE_RE.test(t) && Number.isFinite(+t) ? +t : NO),
  /** `Bool`: `true` or `false`. */
  bool: (t) => (t === "true" ? true : t === "false" ? false : NO),
  /** `Uuid`: the canonical 8-4-4-4-12 hexadecimal form. */
  uuid: (t) => parseUuid(t) ?? NO,
  /**
   * A payload-free enum, by exact case name.
   * @param {Record<string, unknown>} cases
   */
  cases: (cases) => (t) => (Object.hasOwn(cases, t) ? cases[t] : NO),
  /**
   * `Page` on the client: a path that one of `table()`'s routes matches,
   * as that route's `Page` value.
   * @param {() => Array<{parse: Function, value: Function}>} table
   */
  page: (table) => (t) => {
    if (!t.startsWith("/") || t.startsWith("//") || t.includes("\\")) return NO;
    for (const r of table()) {
      const p = r.parse(splitTarget(t));
      if (p) return r.value(p);
    }
    return NO;
  },
};

/**
 * Matches a URL against a route template, on either side.
 *
 * `segments` are the template's path segments (`["product", "{id}"]`);
 * `params` describe the page's parameters in declaration order as
 * `[name, parse, required, default]`. A required parameter is a path
 * segment; an optional one is read from the query (never from the path,
 * and a query parameter never overrides a path one), taking `default()`
 * when absent. Each path segment is percent-decoded on its own, so `%2F`
 * stays inside one parameter; a malformed escape doesn't match. Unknown
 * query parameters are ignored.
 * @param {{pathname: string, search: string}} url
 * @param {string[]} segments
 * @param {Array<[string, (t: string) => unknown, boolean, (() => unknown)|null]>} params
 * @returns {Record<string, unknown>|null} the parameters, or `null`
 */
export function route(url, segments, params) {
  const parts = url.pathname === "/" ? [] : url.pathname.split("/").slice(1);
  if (parts.length !== segments.length) return null;
  const raw = {};
  for (let i = 0; i < parts.length; i++) {
    const s = segments[i];
    const text = decodeComponent(parts[i]);
    if (text === undefined) return null;
    if (s[0] === "{") {
      if (text === "") return null;
      raw[s.slice(1, -1)] = text;
    } else if (s !== parts[i]) {
      return null;
    }
  }
  let query = null;
  const out = {};
  for (const [name, parse, required, fallback] of params) {
    let text = raw[name];
    if (!required) {
      query ??= parseQuery(url.search);
      text = query.get(name);
      if (text === undefined) {
        out[name] = fallback ? fallback() : null;
        continue;
      }
    }
    const v = text === undefined ? NO : parse(text);
    if (v === NO) return null;
    out[name] = v;
  }
  return out;
}

/** An email address: no spaces, brackets, quotes or separators, one `@`, a dotted domain. */
const EMAIL_RE = new RegExp('^[^\\s@<>()\\[\\]\\\\,;:"]+@[^\\s@<>()\\[\\]\\\\,;:"]+\\.[^\\s@<>()\\[\\]\\\\,;:".]+$');

/**
 * Characters a display name needs quotes for: RFC 5322's specials, less
 * the `.` its obsolete phrase syntax allows (`J. Doe <j@doe.example>`).
 */
const SPECIALS = new RegExp('[()<>\\[\\]:;@\\\\,"]');
/** `Name <address>`: the name as written and the address. */
const NAMED_EMAIL = new RegExp("^(.*?)\\s*<([^<>]*)>$");
/** A quoted display name, with `\"` and `\\` escapes. */
const QUOTED_NAME = new RegExp('^"((?:[^"\\\\]|\\\\.)*)"$');
/** An escape in a quoted name. */
const ESCAPE = new RegExp("\\\\(.)", "g");
/** What quoting a name escapes. */
const QUOTED = new RegExp('["\\\\]', "g");
/** Runs of whitespace, one space in a display name. */
const SPACES = new RegExp("\\s+", "g");
/** Line breaks and other control characters, never part of an address. */
const CONTROLS = new RegExp("[\\u0000-\\u001f\\u007f]");

/**
 * A bare address with its domain lower-cased, or `null`.
 * @param {string} t trimmed text
 * @returns {string | null}
 */
function bareEmail(t) {
  if (!EMAIL_RE.test(t)) return null;
  const at = t.lastIndexOf("@");
  return t.slice(0, at + 1) + t.slice(at + 1).toLowerCase();
}

/**
 * A display name as written, quoted or plain words without specials:
 * unquoted, its spaces collapsed; `null` if it isn't one or is empty.
 * @param {string} t
 * @returns {string | null}
 */
function nameOf(t) {
  const q = QUOTED_NAME.exec(t);
  return (q ? q[1].replace(ESCAPE, "$1") : SPECIALS.test(t) ? "" : t).replace(SPACES, " ").trim() || null;
}

/**
 * `EmailAddress.parse` (31-email.md): a bare address, or a display name and
 * an address in angle brackets (`K Shop <orders@shop.example>`). Trims
 * surrounding whitespace and lower-cases the domain; the name is kept
 * unquoted unless it holds a special. `null` if it isn't an address, or
 * contains a line break or another control character.
 * @param {unknown} text
 * @returns {string | null} the normalized message-header form
 */
export function parseEmail(text) {
  if (typeof text !== "string" || CONTROLS.test(text)) return null;
  const t = text.trim();
  const m = NAMED_EMAIL.exec(t);
  if (!m) return bareEmail(t);
  const a = bareEmail(m[2]);
  const n = nameOf(m[1]);
  return a && n ? `${SPECIALS.test(n) ? `"${n.replace(QUOTED, "\\$&")}"` : n} <${a}>` : null;
}

/**
 * `address.address`: the `local@domain` part of a parsed address.
 * @param {string} email a parsed `EmailAddress`
 * @returns {string}
 */
export function emailAddress(email) {
  return NAMED_EMAIL.exec(email)?.[2] ?? email;
}

/**
 * `address.displayName`: the display name of a parsed address, unquoted,
 * or `null` for a bare address.
 * @param {string} email a parsed `EmailAddress`
 * @returns {string | null}
 */
export function emailDisplayName(email) {
  const m = NAMED_EMAIL.exec(email);
  return m ? nameOf(m[1]) : null;
}

/** A UUID in its hyphenated form, any case. */
const UUID_RE = new RegExp("^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$");

/**
 * `Uuid.parse`: canonical 8-4-4-4-12 text in either case, as lower case;
 * `null` otherwise.
 * @param {unknown} text
 * @returns {string | null}
 */
export function parseUuid(text) {
  return typeof text === "string" && UUID_RE.test(text) ? text.toLowerCase() : null;
}

/** The tokens of a CSS color value: a hex color, a word, a number, punctuation. */
const COLOR_TOKEN = new RegExp("#[0-9a-fA-F]+|-?-?[a-zA-Z][a-zA-Z0-9-]*|-?[0-9.]+%?|[(), ]", "y");
/** The functions a color value may call (what `Color` natives produce, and CSS color functions). */
const COLOR_FUNCTIONS = ["color-mix", "var", "rgb", "rgba", "hsl", "hsla"];

/**
 * A `Color` from text (JSON decoding): a hex color (`#RGB`, `#RGBA`,
 * `#RRGGBB`, `#RRGGBBAA`), a keyword, or a call of a color function, with
 * nothing that could end a CSS declaration or load a resource (`;`, `:`,
 * `{`, quotes, `url(`); `null` otherwise. A decoded color reaches `style`
 * attributes as a custom property value, so this is what keeps it a color.
 * @param {unknown} text
 * @returns {string | null}
 */
export function parseColor(text) {
  if (typeof text !== "string" || text.length === 0 || text.length > 200) return null;
  COLOR_TOKEN.lastIndex = 0;
  let depth = 0;
  let prev = "";
  while (COLOR_TOKEN.lastIndex < text.length) {
    const m = COLOR_TOKEN.exec(text);
    if (m === null) return null;
    const t = m[0];
    if (t[0] === "#" && ![4, 5, 7, 9].includes(t.length)) return null;
    if (t === "(") {
      if (!COLOR_FUNCTIONS.includes(prev.toLowerCase())) return null;
      depth++;
    } else if (t === ")" && --depth < 0) return null;
    prev = t;
  }
  return depth === 0 ? text : null;
}

/**
 * A URL with `path` joined onto its path.
 * @param {string} url
 * @param {string} path
 * @returns {string}
 */
export function urlAppending(url, path) {
  const base = url.endsWith("/") ? url.slice(0, -1) : url;
  return base + (path.startsWith("/") ? path : "/" + path);
}

// ------------------------------------------------------------------ JSON

/** A decoding failure at a JSON path; `Json.decode` turns it into `DecodingError`. */
export class DecodeFailure {
  /**
   * @param {string} path
   * @param {string} message
   */
  constructor(path, message) {
    this.path = path;
    this.message = message;
  }
}

/**
 * Throws a {@link DecodeFailure}.
 * @param {string} path
 * @param {string} message
 * @returns {never}
 */
export function decodeFail(path, message) {
  throw new DecodeFailure(path || "$", message);
}

/**
 * Requires a JSON object.
 * @param {unknown} j
 * @param {string} p
 */
export function expectObject(j, p) {
  if (j === null || typeof j !== "object" || Array.isArray(j)) decodeFail(p, "expected an object");
}

/**
 * The case name of an encoded enum value: a string, or an object's `case`.
 * @param {unknown} j
 * @param {string} p
 * @returns {string}
 */
export function caseName(j, p) {
  if (typeof j === "string") return j;
  if (j && typeof j === "object" && typeof j.case === "string") return j.case;
  return decodeFail(p, "expected an enum case");
}

/**
 * Decodes a payload-free enum case by name.
 * @param {unknown} j
 * @param {string} p
 * @param {object} cases the enum's constructor object
 * @param {string[]} names
 * @returns {unknown}
 */
export function decodeUnitCase(j, p, cases, names) {
  if (typeof j !== "string" || !names.includes(j)) return decodeFail(p, "expected one of " + names.join(", "));
  return cases[j];
}

/** A `Double` in encoded JSON, printed with at least one decimal. */
class JsonDouble {
  /**
   * Wraps the double `v`.
   * @param {number} v
   */
  constructor(v) {
    this.v = v;
  }
}

/**
 * Prints encoded JSON: like `JSON.stringify`, but `Double` values keep a
 * decimal point (`2.0`).
 * @param {unknown} j
 * @returns {string}
 */
export function stringify(j) {
  if (j instanceof JsonDouble) return Number.isFinite(j.v) ? formatDouble(j.v) : "null";
  if (Array.isArray(j)) return "[" + j.map(stringify).join(",") + "]";
  if (j !== null && typeof j === "object") {
    if (typeof j.toJSON === "function") return stringify(j.toJSON());
    return "{" + Object.keys(j).filter((k) => j[k] !== undefined).map((k) => JSON.stringify(k) + ":" + stringify(j[k])).join(",") + "}";
  }
  if (j === undefined) return "null";
  return JSON.stringify(j);
}

/**
 * The codec of a primitive whose JSON form is its value: `encode` is the
 * identity, `decode` fails with `expected <what>` unless `check` accepts.
 * @param {(j: unknown) => boolean} check
 * @param {string} what the expected kind, for the error
 * @returns {{encode: Function, decode: Function}}
 */
function prim(check, what) {
  return {
    encode: (v) => v,
    decode: (j, p) => (check(j) ? j : decodeFail(p, "expected " + what)),
  };
}

/**
 * The codec of a typed text value: a JSON string that `parse` accepts.
 * `key` marks it as a codec of map keys that are JSON object keys.
 * @param {(t: unknown) => string | null} parse
 * @param {string} what
 */
function typedText(parse, what) {
  return {
    key: true,
    encode: (v) => v,
    decode: (j, p) => parse(j) ?? decodeFail(p, "expected " + what),
  };
}

/** A byte: an integer in `0...255`. */
const isByte = (x) => Number.isInteger(x) && x >= 0 && x <= 255;

/** The codecs of builtin types (docs/design/28-rpc-wire-format.md). */
export const codecs = {
  int: prim((j) => Number.isSafeInteger(j), "an integer"),
  double: {
    encode: (v) => new JsonDouble(v),
    decode: (j, p) => (typeof j === "number" ? j : decodeFail(p, "expected a number")),
  },
  string: { ...prim((j) => typeof j === "string", "a string"), key: true },
  email: typedText(parseEmail, "an email address"),
  url: typedText(parseUrl, "an absolute http(s) URL"),
  uuid: typedText(parseUuid, "a UUID"),
  color: typedText(parseColor, "a color"),
  timeZone: typedText(parseTimeZone, "an IANA time zone"),
  timeOfDay: typedText(parseTimeOfDay, "a time of day as HH:MM"),
  bool: prim((j) => typeof j === "boolean", "a boolean"),
  duration: prim((j) => typeof j === "number", "a duration in milliseconds"),
  any: { encode: (v) => toJSON(v), decode: (j) => j },
  bytes: {
    encode: (v) => Array.from(v),
    decode: (j, p) => (Array.isArray(j) && j.every(isByte) ? Uint8Array.from(j) : decodeFail(p, "expected bytes")),
  },
  date: {
    encode: (v) => v.toJSON(),
    decode: (j, p) => (typeof j === "string" && parseInstant(j)) || decodeFail(p, "expected an RFC 3339 timestamp"),
  },
  /**
   * @param {{encode: Function, decode: Function}} c
   * @returns {{encode: Function, decode: Function}}
   */
  optional: (c) => ({
    encode: (v) => (v === null || v === NONE ? null : c.encode(v)),
    decode: (j, p) => (j === null || j === undefined ? null : c.decode(j, p)),
  }),
  /**
   * @param {{encode: Function, decode: Function}} c
   * @returns {{encode: Function, decode: Function}}
   */
  list: (c) => ({
    encode: (v) => v.map((x) => c.encode(x)),
    decode: (j, p) => (Array.isArray(j) ? j.map((x, i) => c.decode(x, `${p}[${i}]`)) : decodeFail(p, "expected an array")),
  }),
  /**
   * @param {{encode: Function, decode: Function}} c
   * @param {boolean} primitive
   * @returns {{encode: Function, decode: Function}}
   */
  set: (c, primitive) => ({
    encode: (v) => [...v].map((x) => c.encode(x)),
    decode: (j, p) => {
      if (!Array.isArray(j)) decodeFail(p, "expected an array");
      const s = primitive ? new Set() : new HashSet();
      j.forEach((x, i) => s.add(c.decode(x, `${p}[${i}]`)));
      return s;
    },
  }),
  /**
   * Maps whose keys are text (`String`, `Uuid`, …: a codec with `key`) are
   * JSON objects; other keys are `[key, value]` pairs. Each shape decodes
   * only from itself, and an encoded object has no prototype, so a
   * `__proto__` key is an entry like any other.
   * @param {{encode: Function, decode: Function, key?: boolean}} kc
   * @param {{encode: Function, decode: Function}} vc
   * @param {boolean} primitive
   * @returns {{encode: Function, decode: Function}}
   */
  map: (kc, vc, primitive) => ({
    encode: (m) => {
      if (!kc.key) return [...m].map(([k, v]) => [kc.encode(k), vc.encode(v)]);
      const o = Object.create(null);
      for (const [k, v] of m) o[k] = vc.encode(v);
      return o;
    },
    decode: (j, p) => {
      const m = primitive ? new Map() : new HashMap();
      if (!kc.key) {
        if (!Array.isArray(j)) decodeFail(p, "expected an array of [key, value] pairs");
        j.forEach((e, i) => {
          const at = `${p}[${i}]`;
          if (!Array.isArray(e) || e.length !== 2) decodeFail(at, "expected a [key, value] pair");
          m.set(kc.decode(e[0], at + "[0]"), vc.decode(e[1], at + "[1]"));
        });
        return m;
      }
      expectObject(j, p);
      for (const k of Object.keys(j)) m.set(kc.decode(k, p), vc.decode(j[k], `${p}.${k}`));
      return m;
    },
  }),
  /**
   * @param {{encode: Function, decode: Function}[]} cs
   * @returns {{encode: Function, decode: Function}}
   */
  tuple: (cs) => ({
    encode: (v) => cs.map((c, i) => c.encode(v[i])),
    decode: (j, p) => (Array.isArray(j) && j.length === cs.length ? cs.map((c, i) => c.decode(j[i], `${p}[${i}]`)) : decodeFail(p, "expected an array")),
  }),
  /**
   * @param {{encode: Function, decode: Function}} c
   * @param {boolean} closed
   * @returns {{encode: Function, decode: Function}}
   */
  range: (c, closed) => ({
    encode: (r) => ({ lowerBound: c.encode(r.lo), upperBound: c.encode(r.hi) }),
    decode: (j, p) => {
      expectObject(j, p);
      return new Range(c.decode(j.lowerBound, p), c.decode(j.upperBound, p), closed);
    },
  }),
};

/** `Json.encode` and `Json.decode` (k.encoding.Json). */
export const Json = {
  /**
   * @param {{encode: Function}} codec
   * @param {unknown} value
   * @returns {string}
   */
  encode(codec, value) {
    return stringify(codec.encode(value));
  },
  /**
   * Decodes `text`; throws the K `DecodingError` (`malformed` or
   * `mismatch`), never a JS exception.
   * @param {{decode: Function}} codec
   * @param {string | Uint8Array} text
   * @param {{malformed: Function, mismatch: Function}} DecodingError
   * @returns {unknown}
   */
  decode(codec, text, DecodingError) {
    let j;
    try {
      j = JSON.parse(typeof text === "string" ? text : new TextDecoder("utf-8", { fatal: true }).decode(text));
    } catch (e) {
      throw DecodingError.malformed(String(e && e.message ? e.message : e));
    }
    try {
      return codec.decode(j, "");
    } catch (e) {
      if (e instanceof DecodeFailure) throw DecodingError.mismatch(e.path || "$", e.message);
      throw e;
    }
  },
};

// ---------------------------------------------------------------- streams

/**
 * An async stream fed by pushes (`Broadcast` subscriptions, a server
 * stream read in the browser): `next()` resolves with the next value, and
 * with `null` once `null` was pushed (the end); `for await` iterates, and
 * leaving the loop early calls `close`, when the stream has one.
 */
export class PushStream {
  /** Creates an open stream with nothing pushed. */
  constructor() {
    this.items = [];
    this.waiters = [];
  }
  /**
   * Delivers `v` to the oldest waiting `next()`, or queues it.
   * @param {unknown} v a value, or `null` to end the stream
   */
  push(v) {
    if (v === null) {
      for (const w of this.waiters.splice(0)) w(null);
      this.items.push(null);
      return;
    }
    const w = this.waiters.shift();
    if (w) w(v);
    else this.items.push(v);
  }
  /**
   * The next value; `null` once the stream has ended.
   * @returns {Promise<unknown>}
   */
  next() {
    if (this.items[0] === null) return Promise.resolve(null);
    if (this.items.length) return Promise.resolve(this.items.shift());
    return new Promise((resolve) => this.waiters.push(resolve));
  }
  /**
   * Iterates the stream with `for await`.
   * @returns {AsyncIterator<unknown>}
   */
  [Symbol.asyncIterator]() {
    return iterate(this);
  }
}

/**
 * The `for await` iterator of a stream: `next()` until it yields `null`;
 * leaving the loop early calls the stream's `close`, when it has one.
 * @param {{next: () => Promise<unknown>, close?: () => void}} s
 * @returns {AsyncIterator<unknown>}
 */
function iterate(s) {
  return {
    next: () => s.next().then((value) => ({ done: value === null, value })),
    return: async () => (s.close?.(), { done: true }),
  };
}

/** A derived stream that transforms another one lazily. */
class MappedStream {
  /**
   * @param {{next: () => Promise<unknown>}} source
   * @param {(v: any) => {keep: boolean, value: any}} step
   * @param {unknown[]} [initial]
   */
  constructor(source, step, initial) {
    this.source = source;
    this.step = step;
    this.initial = initial ? initial.slice() : [];
  }
  /**
   * The next value that the step keeps (initial values first); `null` at the end.
   * @returns {Promise<unknown>}
   */
  async next() {
    if (this.initial.length) return this.initial.shift();
    for (;;) {
      const v = await this.source.next();
      if (v === null) return null;
      const r = this.step(v);
      if (r.keep) return r.value;
    }
  }
  /**
   * Iterates the stream with `for await`.
   * @returns {AsyncIterator<unknown>}
   */
  [Symbol.asyncIterator]() {
    return iterate(this);
  }
}

/**
 * `Stream.filter`.
 * @param {{next: Function}} s
 * @param {(v: any) => boolean} f
 * @returns {MappedStream}
 */
export function streamFilter(s, f) {
  return new MappedStream(s, (v) => ({ keep: f(v), value: v }));
}

/**
 * `Stream.map`.
 * @param {{next: Function}} s
 * @param {(v: any) => any} f
 * @returns {MappedStream}
 */
export function streamMap(s, f) {
  return new MappedStream(s, (v) => ({ keep: true, value: f(v) }));
}

/**
 * `Stream.startingWith`.
 * @param {{next: Function}} s
 * @param {unknown} v
 * @returns {MappedStream}
 */
export function streamStartingWith(s, v) {
  return new MappedStream(s, (x) => ({ keep: true, value: x }), [v]);
}

/** `k.rpc.Broadcast`: publishes values to the subscribers of a key. */
export class Broadcast {
  /** Creates a broadcast without subscribers. */
  constructor() {
    this.subs = new HashMap();
  }
  /**
   * @param {unknown} key
   * @param {unknown} value
   */
  publish(key, value) {
    const list = this.subs.get(key);
    if (list) for (const s of list) s.push(value);
  }
  /**
   * @param {unknown} key
   * @returns {PushStream}
   */
  subscribe(key) {
    const s = new PushStream();
    const list = this.subs.get(key);
    if (list) list.push(s);
    else this.subs.set(key, [s]);
    return s;
  }
}

/**
 * The class names of `global("name")`: the name as written.
 * @param {string} name
 * @returns {string}
 */
export function globalClass(name) {
  return name;
}
