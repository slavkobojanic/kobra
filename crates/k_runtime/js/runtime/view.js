/**
 * @module k:runtime/view
 *
 * Helpers generated view code uses besides the bindings and blocks: child
 * components and slots placed between markers, block ranges at a view's
 * root, the cancellation context of a component's handlers, page props
 * and links (docs/compiler/codegen.md §10, §11). Route matching is shared
 * with the server (`route` in `std.js`); `uid` is in `hydrate.js`.
 */

import { claimMarkers, claimRange, isHydrating } from "./hydrate.js";
import { place } from "./dom.js";
import { onCleanup, renderEffect } from "./signals.js";

/**
 * Places the nodes `render()` returns between `start` and `end` (a child
 * component or slot). During hydration the server's nodes in that range
 * are claimed instead.
 * @param {Comment} start
 * @param {Comment} end
 * @param {() => Node|null} render
 */
export function child(start, end, render) {
  const outer = slot;
  slot = [start, end];
  try {
    place(isHydrating() ? claimRange(start, end, render) : render(), end);
  } finally {
    slot = outer;
  }
}

/** The markers of the `child` range being rendered, innermost. */
let slot = null;

/**
 * The `[start, end]` markers of the child range whose content is being
 * rendered: a layout's slot, read by the router so a later navigation can
 * replace only what is between them.
 * @returns {[Comment, Comment]|null}
 */
export function slotMarkers() {
  return slot;
}

/** The titles of what is mounted, outermost first. */
const titles = [];

/** Shows the innermost title that has a value. */
function showTitle() {
  for (let i = titles.length; i--;) {
    const v = titles[i].v;
    if (v !== null && v !== undefined && v !== false) {
      document.title = String(v);
      return;
    }
  }
}

/**
 * A `title(_:)` metadata entry in the browser (22-metadata.md): while the
 * current owner (a page or layout) is mounted, `document.title` is the
 * innermost mounted title, kept up to date as `get` changes. `get` may
 * return `null` or `false` while it has no value yet (a page still
 * loading the data its title reads), which leaves the title below it.
 * @param {() => unknown} get
 */
export function title(get) {
  const entry = { v: null };
  titles.push(entry);
  renderEffect(() => {
    entry.v = get();
    showTitle();
  });
  onCleanup(() => {
    titles.splice(titles.indexOf(entry), 1);
    showTitle();
  });
}

/**
 * Creates a pair of block markers for a block at the root of a view: a
 * fragment holding `<!--[-->` and `<!--]-->`, or, while hydrating, the
 * server's pair (`claimMarkers`), so the block isn't marked twice.
 * @returns {{node: DocumentFragment, start: Comment, end: Comment}}
 */
export function markers() {
  const claimed = isHydrating() ? claimMarkers() : null;
  if (claimed) return claimed;
  const node = document.createDocumentFragment();
  const start = document.createComment("[");
  const end = document.createComment("]");
  node.appendChild(start);
  node.appendChild(end);
  return { node, start, end };
}

/**
 * An `AbortSignal` aborted when the current owner (a component instance)
 * is disposed: the `$c` of its async handlers.
 * @returns {AbortSignal|null}
 */
export function lifetime() {
  if (typeof AbortController !== "function") return null;
  const controller = new AbortController();
  onCleanup(() => controller.abort());
  return controller.signal;
}

/**
 * The props of a page from its route parameters: one getter per parameter.
 * @param {Record<string, unknown>} params
 * @returns {Record<string, () => unknown>}
 */
export function paramProps(params) {
  const props = {};
  for (const k of Object.keys(params ?? {})) props[k] = () => params[k];
  return props;
}

/**
 * Appends the optional parameters that differ from their defaults:
 * `["name=", text, value, default]` entries.
 * @param {string} path
 * @param {Array<[string, string, unknown, unknown]>} entries
 * @returns {string}
 */
export function withQuery(path, entries) {
  const q = entries
    .filter(([, , v, d]) => v !== d)
    .map(([k, t]) => k + encodeURIComponent(t));
  return q.length ? `${path}?${q.join("&")}` : path;
}

/**
 * The serialized `@Load` values of the instance rendered with `ctx`
 * (hydration), or `null`; with `key` `"failed"`, the error its server
 * render failed with instead (`undefined` when it didn't).
 * @param {{data?: any, key?: string}|null} ctx
 * @param {string} [key]
 * @returns {unknown}
 */
export function loadsOf(ctx, key = "loads") {
  const c = ctx && ctx.data && ctx.data.c;
  const entry = c ? c[ctx.key ?? "0"] : null;
  return entry ? entry[key] : null;
}

/**
 * Joins class names, skipping empty ones.
 * @param {...(string|false|null|undefined)} names
 * @returns {string}
 */
export function joinClasses(...names) {
  return names.filter((n) => n).join(" ");
}
