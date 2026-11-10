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
  constructor() {
    /**
     * The HTML: text, label ids ({@link Uid}), and the chunks of another
     * buffer (a page's in its layout's slot).
     * @type {Array<string|Uid|Array<any>>}
     */
    this.chunks = [];
    /** @type {Record<string, unknown>} */
    this.data = {};
    /** @type {Map<string, number>} instances with loads per base key (`loadKey`) */
    this.loadKeys = new Map();
  }

  /**
   * What the buffer holds now, as a function that takes back everything
   * written since: its HTML (label ids included) and the load keys drawn.
   * An `ErrorBoundary` whose content panics rewinds before its fallback,
   * as the hydrating client puts its counters back (`counters`).
   * @returns {() => void}
   */
  mark() {
    const n = this.chunks.length, keys = new Map(this.loadKeys);
    return () => {
      this.chunks.length = n;
      this.loadKeys = keys;
    };
  }

  /**
   * The `k-data` key of a component instance's `@Load` values:
   * `"{key}.{n}"` for the `n`-th instance with loads rendered under the
   * page's or layout's `ctx.key`, as the hydrating client counts them
   * (`loadKey` in `hydrate.js`).
   * @param {{key?: string}} ctx
   * @returns {string}
   */
  loadKey(ctx) {
    const base = ctx.key ?? "0";
    const n = (this.loadKeys.get(base) ?? 0) + 1;
    this.loadKeys.set(base, n);
    return `${base}.${n}`;
  }

  /**
   * A fresh label id. It is numbered (`k1`, `k2`, …) when the HTML is
   * joined ({@link joinHtml}), in document order: rendering the same page
   * twice gives the same ids, they reveal nothing about other requests,
   * and they match the hydrating client's, which renders a layout before
   * its page (the server renders the page first) and restarts `uid` at
   * `k1` with each pass.
   * @returns {Uid}
   */
  uid() {
    return new Uid();
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
    if (value instanceof Uid) {
      this.chunks.push(` ${name}="`, value, '"');
      return;
    }
    this.chunks.push(` ${name}="${escapeAttribute(value)}"`);
  }

  /** Returns the HTML so far. @returns {string} */
  html() {
    return joinHtml(this.chunks);
  }
}

/** A label id of a server render ({@link Out#uid}), numbered when joined. */
class Uid {}

/**
 * The HTML of `chunks`: nested chunks in place, each label id numbered
 * `k1`, `k2`, … by its first appearance.
 * @param {Array<string|Uid|Array<any>>} chunks
 * @returns {string}
 */
export function joinHtml(chunks) {
  const ids = new Map();
  const out = [];
  const walk = (cs) => {
    for (const c of cs) {
      if (typeof c === "string") out.push(c);
      else if (Array.isArray(c)) walk(c);
      else {
        if (!ids.has(c)) ids.set(c, `k${ids.size + 1}`);
        out.push(ids.get(c));
      }
    }
  };
  walk(chunks);
  return out.join("");
}
