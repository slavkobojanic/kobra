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
 * the action runs; `handle(action)` returns the submit handler.
 */

import { attr, on } from "./dom.js";
import { batch, renderEffect, signal, untrack } from "./signals.js";
import { graphemeCount } from "./std.js";

/**
 * @typedef {object} FieldSpec
 * @property {string} name the struct field
 * @property {(raw: string) => {ok: true, value: unknown}|{ok: false, message: string}} parse
 * @property {Array<(value: unknown) => string|null>} constraints each returns a message or `null`
 */

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
 * @returns {object}
 */
export function createForm(fields, initial, validate) {
  const id = ++formIds;
  const state = new Map();
  const order = [];
  for (const spec of fields) {
    order.push(spec.name);
    state.set(spec.name, {
      spec,
      raw: signal(initial && initial[spec.name] !== undefined ? String(initial[spec.name]) : ""),
      error: signal(null),
      touched: signal(false),
    });
  }
  const errors = signal([]);
  const submitting = signal(false);
  const touched = signal(false);
  const validated = signal(false);
  let current = null;

  /** Sets the per-field errors from a list of `{path, message}`. */
  const show = (list) => {
    batch(() => {
      errors.set(list);
      for (const [name, f] of state) {
        const e = list.find((x) => x.path === name);
        f.error.set(e ? e.message : null);
      }
      validated.set(true);
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
      if (!r.ok) {
        parsed = false;
        list.push({ path: name, message: r.message });
        continue;
      }
      value[name] = r.value;
      for (const c of f.spec.constraints) {
        const m = c(r.value);
        if (m !== null && m !== undefined) {
          list.push({ path: name, message: m });
          break;
        }
      }
    }
    if (parsed && list.length === 0 && validate) {
      validate(value, {
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
        renderEffect(() => {
          const raw = f.raw.get();
          untrack(() => {
            if (input.value !== raw) input.value = raw;
          });
        });
        on(input, "input", () => field(name).set(input.value));
        input.addEventListener("blur", () => f.touched.set(true));
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
      const at = t.indexOf("@");
      return at > 0 && at < t.length - 1 && t.indexOf(".", at) > at + 1 ? { ok: true, value: t } : { ok: false, message };
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
