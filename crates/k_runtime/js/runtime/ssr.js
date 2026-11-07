/**
 * @module k:runtime/ssr
 *
 * Server-side rendering: HTML escaping, the output buffer generated
 * `$ssr` renderers append to, and the hydration markers
 * (docs/compiler/runtime.md, "Hydration protocol").
 *
 * Everything dynamic that reaches the HTML goes through `escapeText` or
 * `escapeAttribute`; generated renderers never concatenate unescaped
 * values. Static template text is escaped by the compiler.
 */

import { stringify } from "./std.js";

/** Opens a dynamic range (a `when`, `branch` or `each` block). */
export const OPEN = "<!--[-->";
/** Closes a dynamic range. */
export const CLOSE = "<!--]-->";
/** Separates two adjacent dynamic texts, which HTML would merge. */
export const SEPARATOR = "<!---->";

/** The entity of each character special in text content. */
const TEXT_ESCAPES = /** @type {Record<string, string>} */ ({
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
});

/** The entity of each character special in a quoted attribute value. */
const ATTRIBUTE_ESCAPES = /** @type {Record<string, string>} */ ({
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
});

/** The characters `escapeText` replaces. */
const TEXT_SPECIAL = new RegExp("[&<>]", "g");
/** The characters `escapeAttribute` replaces. */
const ATTRIBUTE_SPECIAL = new RegExp("[&<>\"']", "g");

/**
 * Escapes text content: `&`, `<` and `>`. Safe for any element content
 * except raw-text elements (`script`, `style`), which K views can't
 * produce.
 * @param {string} value
 * @returns {string}
 */
export function escapeText(value) {
  return value.replace(TEXT_SPECIAL, (c) => TEXT_ESCAPES[c]);
}

/**
 * Escapes an attribute value for a double-quoted attribute: `&`, `<`, `>`,
 * `"` and `'`.
 * @param {string} value
 * @returns {string}
 */
export function escapeAttribute(value) {
  return value.replace(ATTRIBUTE_SPECIAL, (c) => ATTRIBUTE_ESCAPES[c]);
}

/**
 * Serializes data for the `<script type="application/json" id="k-data">`
 * element: JSON as rpc bodies print it (`stringify`, so a codec's
 * `Double` is a number), with `<`, `>`, `&`, U+2028 and U+2029 escaped,
 * so no string can close the script element or break a parser.
 * @param {unknown} data
 * @returns {string}
 */
export function serializeData(data) {
  return stringify(data)
    .replaceAll("<", "\\u003c")
    .replaceAll(">", "\\u003e")
    .replaceAll("&", "\\u0026")
    .replaceAll(" ", "\\u2028")
    .replaceAll(" ", "\\u2029");
}

/**
 * The output of one server render: HTML chunks plus the data serialized
 * for hydration.
 */
export class Out {
  /**
   * @param {{n: number}} [ids] the id counter of the request, shared by the
   * page's and its layouts' buffers
   */
  constructor(ids = { n: 0 }) {
    /** @type {string[]} */
    this.chunks = [];
    /** @type {Record<string, unknown>} */
    this.data = {};
    this.ids = ids;
  }

  /**
   * A fresh label id (`k1`, `k2`, …), counted per request: rendering the
   * same page twice gives the same ids, they reveal nothing about other
   * requests, and they match the hydrating client's (`uid` restarts too).
   * @returns {string}
   */
  uid() {
    return `k${++this.ids.n}`;
  }

  /** Appends trusted, already-escaped HTML. @param {string} html */
  raw(html) {
    this.chunks.push(html);
  }

  /** Appends escaped text. @param {string} value */
  text(value) {
    this.chunks.push(escapeText(value));
  }

  /**
   * Appends ` name="value"`, escaped; nothing for `null` or `false`, and
   * a bare ` name` for `true`.
   * @param {string} name a compiler-checked attribute name
   * @param {string|boolean|null} value
   */
  attribute(name, value) {
    if (value === null || value === false) return;
    if (value === true) {
      this.chunks.push(` ${name}`);
      return;
    }
    this.chunks.push(` ${name}="${escapeAttribute(value)}"`);
  }

  /** Returns the HTML so far. @returns {string} */
  html() {
    return this.chunks.join("");
  }
}
