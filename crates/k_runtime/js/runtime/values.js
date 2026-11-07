/**
 * @module k:runtime/values
 *
 * The runtime side of K's value representation (docs/compiler/codegen.md,
 * "Value representation"): panics, checked integer arithmetic, enum
 * singletons, structural equality and the text form used by string
 * interpolation.
 */

/**
 * A panic: a programmer error (an index out of range, a failed `assert`,
 * integer overflow) that K code can't catch. Every generated `catch`
 * rethrows it; it reaches the nearest `ErrorBoundary` in a view, becomes a
 * 500 on the server and a failed attempt in a job (docs/design/09-errors.md).
 */
export class KPanic extends Error {
  /**
   * @param {string} message what went wrong, for developers
   */
  constructor(message) {
    super(message);
    this.name = "KPanic";
  }
}

/**
 * Throws a {@link KPanic}.
 * @param {string} message
 * @returns {never}
 */
export function panic(message) {
  throw new KPanic(message);
}

/**
 * Returns `true` if `error` is a panic, which generated `catch` clauses
 * rethrow instead of matching.
 * @param {unknown} error
 * @returns {boolean}
 */
export function isPanic(error) {
  return error instanceof KPanic;
}

/**
 * Returns `value` if it is a safe integer, and panics otherwise. Generated
 * code wraps `Int` arithmetic in it in debug and test builds.
 * @param {number} value
 * @returns {number}
 */
export function checkInt(value) {
  if (!Number.isSafeInteger(value)) {
    panic(`integer overflow: ${value} is outside the Int range`);
  }
  return value;
}

/**
 * `Int` division: truncates toward zero and panics on a zero divisor (in
 * every build) or on overflow.
 * @param {number} a
 * @param {number} b
 * @returns {number}
 */
export function intDiv(a, b) {
  if (b === 0) panic("division by zero");
  return checkInt(Math.trunc(a / b));
}

/**
 * `Int` remainder, with the sign of the dividend; panics on a zero divisor.
 * @param {number} a
 * @param {number} b
 * @returns {number}
 */
export function intRem(a, b) {
  if (b === 0) panic("division by zero");
  const r = a % b;
  return r === 0 ? 0 : r;
}

/** The payload-free case of each tag (see {@link unitCase}). */
const UNIT_CASES = [];

/**
 * The frozen singleton of a payload-free enum case: one value per tag,
 * shared by every enum. Generated code compares such cases with `===`,
 * and a value is only ever compared with cases of its own type, so
 * sharing is safe; it makes a case built by one copy of an enum (the
 * browser's half of a `k test` run) the same value as another copy's (the
 * server's).
 * @param {number} tag the case's index in declaration order
 * @returns {Readonly<{$: number}>}
 */
export function unitCase(tag) {
  return (UNIT_CASES[tag] ??= Object.freeze({ $: tag }));
}

/**
 * Structural equality for `Equatable` values: numbers, strings and booleans
 * by value, `null` (nil) only with itself, arrays element-wise, `Map` and
 * `Set` by contents, plain objects (structs, enum cases) field by field.
 * @param {unknown} a
 * @param {unknown} b
 * @returns {boolean}
 */
export function equals(a, b) {
  if (a === b) return true;
  if (typeof a === "number" && typeof b === "number") return a === b || (a !== a && b !== b);
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
  if (typeof a.equals === "function") return a.equals(b);
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (!equals(a[i], b[i])) return false;
    return true;
  }
  if (a instanceof Map) {
    if (!(b instanceof Map) || a.size !== b.size) return false;
    for (const [k, v] of a) if (!b.has(k) || !equals(v, b.get(k))) return false;
    return true;
  }
  if (a instanceof Set) {
    if (!(b instanceof Set) || a.size !== b.size) return false;
    for (const v of a) if (!b.has(v)) return false;
    return true;
  }
  if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
  const keys = Object.keys(a);
  if (keys.length !== Object.keys(b).length) return false;
  for (const k of keys) if (!Object.hasOwn(b, k) || !equals(a[k], b[k])) return false;
  return true;
}

/**
 * The text of a value in string interpolation, for the types the runtime
 * can format without type information: strings as they are, `Int` and
 * `Double` in their shortest round-trip form (`2.0` for a whole `Double`
 * is the compiler's job, which knows the type), booleans as `true`/`false`,
 * `nil` as `nil`. Generated code formats other types with generated
 * functions.
 * @param {unknown} value
 * @returns {string}
 */
export function textOf(value) {
  if (typeof value === "string") return value;
  if (value === null || value === undefined) return "nil";
  return String(value);
}
