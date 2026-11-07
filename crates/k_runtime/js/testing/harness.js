/**
 * @module k:testing
 *
 * The test harness of the runtime's JavaScript unit tests
 * (docs/compiler/testing.md, "Runtime JS tests"). A test file imports
 * `test` and the assertions from `k:testing`, registers tests at the top
 * level, and the Rust driver (`crates/k_server/tests/runtime_js.rs`) runs
 * each registered test as its own cargo test, in a fresh isolate with a
 * fresh fake DOM.
 */

export { install as installDom } from "./dom.js";

/** @type {Array<{name: string, fn: () => unknown, pending: string|null}>} */
const registry = [];

/**
 * Registers a test. `fn` may be async; the test passes when it returns or
 * resolves without throwing.
 * @param {string} name unique within the file
 * @param {() => unknown} fn
 */
export function test(name, fn) {
  registry.push({ name, fn, pending: null });
}

/**
 * Registers a test-first test that is expected to fail until `reason` is
 * implemented. It passes while it fails, and fails once it passes, with a
 * message to turn it into a plain `test` (like a `.pending` corpus file).
 * @param {string} name
 * @param {string} reason what is missing, for example `when blocks (runtime.md §5)`
 * @param {() => unknown} fn
 */
test.pending = (name, reason, fn) => {
  registry.push({ name, fn, pending: reason });
};

/** An assertion failure. */
export class AssertionError extends Error {
  constructor(message) {
    super(message);
    this.name = "AssertionError";
  }
}

function show(value) {
  try {
    return JSON.stringify(value, (k, v) => (v instanceof Map ? { Map: [...v] } : v instanceof Set ? { Set: [...v] } : v));
  } catch {
    return String(value);
  }
}

/** Fails unless `value` is truthy. */
export function assert(value, message) {
  if (!value) throw new AssertionError(message ?? `expected a truthy value, got ${show(value)}`);
}

/** Fails unless `actual === expected` (`Object.is`). */
export function assertEq(actual, expected, message) {
  if (!Object.is(actual, expected)) {
    throw new AssertionError(`${message ? `${message}: ` : ""}expected ${show(expected)}, got ${show(actual)}`);
  }
}

/** Fails unless `actual` and `expected` serialize to the same JSON. */
export function assertDeepEq(actual, expected, message) {
  const a = show(actual);
  const e = show(expected);
  if (a !== e) throw new AssertionError(`${message ? `${message}: ` : ""}expected ${e}, got ${a}`);
}

/**
 * Fails unless `fn` throws; returns the thrown value. With `match`, the
 * error's message must contain it.
 */
export function assertThrows(fn, match) {
  try {
    fn();
  } catch (error) {
    if (match !== undefined && !String(error?.message ?? error).includes(match)) {
      throw new AssertionError(`expected an error containing ${show(match)}, got ${show(String(error?.message ?? error))}`);
    }
    return error;
  }
  throw new AssertionError("expected an exception, but nothing was thrown");
}

/** Resolves after pending microtasks (and the effects they flush) ran. */
export async function tick() {
  for (let i = 0; i < 16; i++) await null;
}

/**
 * The names of the registered tests, in registration order, as JSON. The
 * driver calls it once per file.
 * @returns {string}
 */
export function list() {
  return JSON.stringify(registry.map((t) => ({ name: t.name, pending: t.pending })));
}

/**
 * Runs the test called `name` and returns the outcome as JSON:
 * `{"outcome":"passed"}` or `{"outcome":"failed","message":…}`; pending
 * inversion is applied by the driver.
 * @param {string} name
 * @returns {Promise<string>}
 */
export async function run(name) {
  const entry = registry.find((t) => t.name === name);
  if (entry === undefined) return JSON.stringify({ outcome: "failed", message: `no test named ${name}` });
  try {
    await entry.fn();
    return JSON.stringify({ outcome: "passed" });
  } catch (error) {
    return JSON.stringify({ outcome: "failed", message: String(error?.stack ?? error) });
  }
}
