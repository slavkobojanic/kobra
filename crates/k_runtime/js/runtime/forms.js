/**
 * @module k:runtime/forms
 *
 * `@Form` state: raw input per field, parsing, constraint validation,
 * touched/submitting flags and `handle(action)` (docs/compiler/runtime.md,
 * "Forms"; docs/design/15-forms-and-validation.md).
 *
 * The object `createForm` returns mirrors `k.validation.Form`:
 * `field(name)` gives `{ value(), set(raw), error(), isTouched(), touch(),
 * attach(input, message?) }`; `isValid()`, `isSubmitting()`, `isTouched()`
 * and `errors()` are reactive reads; `value()` is the parsed struct while
 * the action runs; `handle(action)` returns the submit handler; `reset()`
 * puts every field back to its initial text and clears errors and touched
 * flags.
 */

import { attr, on } from "./dom.js";
import { isHydrating } from "./hydrate.js";
import { batch, renderEffect, signal, untrack } from "./signals.js";
import { graphemeCount, parseEmail } from "./std.js";

/**
 * @typedef {object} FieldSpec
 * @property {string} name the struct field
 * @property {(raw: string) => {ok: true, value: unknown}|{ok: false, message: string}} parse
 * @property {Array<(value: unknown) => string|null>} constraints each returns a message or `null`
 * @property {Array<{run: (signal: AbortSignal, value: unknown) => Promise<boolean>, message: string}>} [checks]
 *   the field's `@Check` rules: run debounced on blur, `false` is a failure
 */

/** How long after a field loses focus its `@Check` rules run, in ms. */
const CHECK_DELAY = 300;

/**
 * The number of forms created in this document, for the ids of their
 * error messages. These ids exist only in the browser (attached when a
 * field is wired, never server-rendered), so they don't take label ids
 * (`uid`) that the server rendering has to match.
 */
let formIds = 0;

/**
 * Creates the state of one `@Form`.
 * @param {FieldSpec[]} fields generated from the struct's constraint annotations
 * @param {Record<string, string>} initial raw text per field (`.empty(…)`)
 * @param {(value: object, rules: object) => void} validate the struct's `validate(rules:)`, or a no-op
 * @param {{after: (ms: number, f: () => void) => {cancel(): void}}} [clock] the `Clock` that
 *   debounces `@Check` rules; given when a field has some
 * @returns {object}
 */
export function createForm(fields, initial, validate, clock) {
  const id = ++formIds;
  const state = new Map();
  const order = [];
  for (const spec of fields) {
    order.push(spec.name);
    const start = initial && initial[spec.name] !== undefined ? String(initial[spec.name]) : "";
    state.set(spec.name, {
      spec,
      start,
      raw: signal(start),
      error: signal(null),
      touched: signal(false),
      // `@Check`: the pending timer and run, and the text a check refused.
      timer: null,
      run: null,
      refused: null,
    });
  }
  const errors = signal([]);
  const submitting = signal(false);
  const touched = signal(false);
  const validated = signal(false);
  let current = null;

  /**
   * The form's field a failure path names: the path itself, or, for a
   * path the server prefixed with the rpc parameter's name
   * (`request.email`), the rest of it when the form has that field.
   */
  const fieldOf = (path) => {
    const p = String(path);
    if (state.has(p)) return p;
    const dot = p.indexOf(".");
    return dot > 0 && state.has(p.slice(dot + 1)) ? p.slice(dot + 1) : p;
  };

  /** Sets the per-field errors from a list of `{path, message}`. */
  const show = (list) => {
    batch(() => {
      errors.set(list);
      for (const [name, f] of state) {
        const e = list.find((x) => fieldOf(x.path) === name);
        f.error.set(e ? e.message : null);
      }
      validated.set(true);
    });
  };

  /**
   * The message of the field's sync rule that `r`, its parse, fails, or
   * `null`.
   */
  const failure = (f, r) => {
    if (!r.ok) return r.message;
    for (const c of f.spec.constraints) {
      const m = c(r.value);
      if (m != null) return m;
    }
    return null;
  };

  /**
   * Runs the `@Check` rules of field `name` on its text once it parses and
   * passes its constraints. An answer for text the field no longer holds,
   * or from a run a later one replaced (whose signal is aborted), is
   * dropped; a refusal shows under the field until a check passes.
   */
  const runChecks = async (name) => {
    const f = state.get(name);
    const raw = f.raw.peek();
    const r = f.spec.parse(raw);
    f.run?.abort();
    if (failure(f, r) !== null) return;
    const run = (f.run = new AbortController());
    let message = null;
    try {
      for (const c of f.spec.checks) {
        if (!(await c.run(run.signal, r.value))) {
          message = c.message;
          break;
        }
      }
    } catch {
      // No answer (the service threw): the server's check decides.
      return;
    }
    if (run.signal.aborted || f.raw.peek() !== raw) return;
    f.refused = message === null ? null : raw;
    batch(() => {
      const rest = errors.peek().filter((e) => fieldOf(e.path) !== name);
      errors.set(message === null ? rest : [...rest, { path: name, message }]);
      if (message !== null || f.error.peek() === f.message) f.error.set(message);
      f.message = message;
    });
  };

  /** Validates: parse, constraints, then the cross-field rules. */
  const check = () => {
    const list = [];
    const value = {};
    let parsed = true;
    for (const name of order) {
      const f = state.get(name);
      const r = f.spec.parse(f.raw.peek());
      // A `@Check` that refused this very text still holds.
      const m = failure(f, r) ?? (f.refused === f.raw.peek() ? f.message : null);
      if (!r.ok) parsed = false;
      else value[name] = r.value;
      if (m !== null) list.push({ path: name, message: m });
    }
    if (parsed && list.length === 0 && validate) {
      validate(value, {
        /**
         * Records `message` as an error of the field at `path` unless `ok`.
         * @param {boolean} ok whether the rule holds
         * @param {string} path the field's name
         * @param {string} message the error to show
         */
        check(ok, path, message) {
          if (!ok) list.push({ path, message });
        },
      });
    }
    show(list);
    return list.length === 0 ? value : null;
  };

  const field = (name) => {
    const f = state.get(name);
    if (f === undefined) throw new Error(`k runtime: the form has no field ${name}`);
    return {
      value: () => f.raw.get(),
      set: (raw) => batch(() => {
        f.raw.set(raw);
        touched.set(true);
      }),
      error: () => f.error.get(),
      isTouched: () => f.touched.get(),
      touch: () => f.touched.set(true),
      attach: (input, message) => {
        renderEffect(() => {
          const e = f.error.get();
          // Without a message element, one is made under the input when
          // the first error shows, so hydration claims the server's markup.
          if (e !== null && !message) {
            message = document.createElement("p");
            message.setAttribute("role", "alert");
            input.parentNode.insertBefore(message, input.nextSibling);
          }
          if (!message) return;
          if (!message.id) message.setAttribute("id", `k-form${id}-${name}-error`);
          attr(input, "aria-invalid", e === null ? null : "true");
          attr(input, "aria-describedby", e === null ? null : message.id);
          message.textContent = e ?? "";
        });
        // Text typed into the server's field before the page's scripts ran
        // (a slow load: an email typed into the sign-in form) isn't the
        // markup's value: hydration keeps it and the field takes it, as if
        // typed now, rather than wiping it.
        renderEffect(() => {
          const raw = f.raw.get();
          untrack(() => {
            if (input.value === raw) return;
            if (isHydrating() && input.value !== input.defaultValue) queueMicrotask(() => field(name).set(input.value));
            else input.value = raw;
          });
        });
        on(input, "input", () => field(name).set(input.value));
        input.addEventListener("blur", () => {
          f.touched.set(true);
          if (!f.spec.checks?.length || !clock) return;
          f.timer?.cancel();
          f.timer = clock.after(CHECK_DELAY, () => runChecks(name));
        });
      },
    };
  };

  return {
    field,
    isValid: () => validated.get() && errors.get().length === 0,
    isSubmitting: () => submitting.get(),
    isTouched: () => touched.get(),
    errors: () => errors.get(),
    value: () => current,
    /**
     * Puts every field back to its initial text, and clears the errors,
     * the touched flags and `isValid`. Pending `@Check` runs are cancelled
     * and their refusals forgotten. Called in the action, after the value
     * was used, it empties the form for the next entry.
     */
    reset() {
      batch(() => {
        for (const f of state.values()) {
          f.timer?.cancel();
          f.timer = null;
          f.run?.abort();
          f.run = null;
          f.refused = null;
          f.message = null;
          f.raw.set(f.start);
          f.error.set(null);
          f.touched.set(false);
        }
        errors.set([]);
        touched.set(false);
        validated.set(false);
      });
    },
    /**
     * The submit handler: validates, refuses while submitting, calls
     * `action` with the parsed struct, and shows a `ValidationError` it
     * throws under its fields. Never rejects for those.
     * @param {(value: object) => unknown} action
     * @returns {(event?: Event) => Promise<void>}
     */
    handle(action) {
      return async (event) => {
        if (event && typeof event.preventDefault === "function") event.preventDefault();
        if (submitting.peek()) return;
        const value = check();
        if (value === null) return;
        current = value;
        submitting.set(true);
        try {
          await action(value);
        } catch (e) {
          if (e && Array.isArray(e.failures)) show(e.failures.map((x) => ({ path: x.path, message: x.message })));
          else throw e;
        } finally {
          current = null;
          submitting.set(false);
        }
      };
    },
  };
}

/**
 * The `parse` of a form field by its K type: `text`, `int`, `double`,
 * `bool`, `email` or `decimal`; `message` is the `@Format` message.
 * @param {string} kind
 * @param {string} message
 * @param {boolean} optional an empty text is `nil`
 * @returns {(raw: string) => {ok: true, value: unknown}|{ok: false, message: string}}
 */
export function fieldParser(kind, message, optional) {
  return (raw) => {
    const t = String(raw).trim();
    if (optional && t === "") return { ok: true, value: null };
    if (kind === "int") {
      const n = Number(t);
      return t !== "" && Number.isSafeInteger(n) ? { ok: true, value: n } : { ok: false, message };
    }
    if (kind === "double") {
      const n = Number(t);
      return t !== "" && Number.isFinite(n) ? { ok: true, value: n } : { ok: false, message };
    }
    if (kind === "bool") return { ok: true, value: raw === "true" };
    if (kind === "email") {
      const e = parseEmail(t);
      return e === null ? { ok: false, message } : { ok: true, value: e };
    }
    return { ok: true, value: raw };
  };
}

/**
 * `@Length(min:max:)`, counting characters as `String.count` does
 * (grapheme clusters, not UTF-16 code units).
 * @param {number|null} min
 * @param {number|null} max
 * @param {string} message
 * @returns {(v: string) => string|null}
 */
export function lengthRule(min, max, message) {
  return (v) => {
    const n = graphemeCount(v);
    return (min !== null && n < min) || (max !== null && n > max) ? message : null;
  };
}

/**
 * `@Range(lo...hi)`.
 * @param {number|null} lo
 * @param {number|null} hi
 * @param {string} message
 * @returns {(v: number) => string|null}
 */
export function rangeRule(lo, hi, message) {
  return (v) => ((lo !== null && v < lo) || (hi !== null && v > hi) ? message : null);
}

/**
 * `@Required`: a non-empty value.
 * @param {string} message
 * @returns {(v: unknown) => string|null}
 */
export function requiredRule(message) {
  return (v) => (v === null || v === "" ? message : null);
}
