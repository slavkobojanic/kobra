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
import { CANCELLED } from "./values.js";

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

/** The descriptions of what is mounted, outermost first. */
const descriptions = [];

/** The innermost value of `entries` that has one, else `null`. */
function innermost(entries) {
  for (let i = entries.length; i--;) {
    const v = entries[i].v;
    if (v !== null && v !== undefined && v !== false) return String(v);
  }
  return null;
}

/** Shows the innermost title that has a value. */
function showTitle() {
  const v = innermost(titles);
  if (v !== null) document.title = v;
}

/**
 * Shows the innermost description that has a value in the document's
 * `<meta name="description">`, created when the document has none. With
 * none mounted, the document keeps what the server wrote (the app's).
 */
function showDescription() {
  const v = innermost(descriptions);
  if (v === null) return;
  const head = document.head;
  if (!head) return;
  let meta = null;
  for (let n = head.firstChild; n !== null && meta === null; n = n.nextSibling) {
    if (n.nodeType === 1 && n.localName === "meta" && n.getAttribute("name") === "description") meta = n;
  }
  if (meta === null) {
    meta = document.createElement("meta");
    meta.setAttribute("name", "description");
    head.appendChild(meta);
  }
  if (meta.getAttribute("content") !== v) meta.setAttribute("content", v);
}

/** Keeps `entries`' innermost value shown with `show` while the owner is mounted. */
function metaEntry(entries, show, get) {
  const entry = { v: null };
  entries.push(entry);
  renderEffect(() => {
    entry.v = get();
    show();
  });
  onCleanup(() => {
    entries.splice(entries.indexOf(entry), 1);
    show();
  });
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
  metaEntry(titles, showTitle, get);
}

/**
 * A `description(_:)` metadata entry in the browser, kept like `title`:
 * the document's `<meta name="description">` is the innermost mounted
 * description, so a client-rendered app's pages (whose shell the server
 * wrote without them) and client navigation both show the page's own.
 * @param {() => unknown} get
 */
export function description(get) {
  metaEntry(descriptions, showDescription, get);
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
 * An async function of a component, or an async lambda written in one,
 * as a value that is called with its K arguments alone (a prop, an
 * event's handler, `form.handle`'s action): calls `f(c, ...args)`, so it
 * runs under `c`, the component's lifetime or the run that made it. A
 * call that stops on its cancellation never settles instead of rejecting,
 * so a caller that drops the promise (`emit`) reports nothing.
 * @param {(c: AbortSignal|null, ...args: unknown[]) => Promise<unknown>} f
 * @param {AbortSignal|null} c
 * @returns {(...args: unknown[]) => Promise<unknown>}
 */
export function own(f, c) {
  return (...a) => f(c, ...a).then(undefined, (e) => (e === CANCELLED ? new Promise(() => {}) : Promise.reject(e)));
}

/**
 * A function value a component hands to a service (a listener the service
 * keeps, a `Clock` timer's action), bound to the component's lifetime
 * `life` (03-reactivity.md, "What a component leaves behind"). While
 * `life` is live, a call runs `f`; an async `f` takes its caller's context
 * first, as every K function value does, and runs under a context that the
 * caller's cancellation and `life` both abort. Once `life` is aborted a
 * call runs none of `f`: one of `Void` returns at once (an async one
 * resolves), and one with a value stops its caller with {@link CANCELLED},
 * as awaiting the disposed component's own work does. The bound value
 * names `life` as its `lifetime`, which the browser `Clock` stops a timer
 * by. Without a lifetime (outside a component's client renderer) it is `f`.
 * @param {Function} f the function value, in K's calling convention
 * @param {AbortSignal|null} life the component's lifetime (`$life`)
 * @param {number} mode 0 a synchronous `Void` function, 1 a synchronous one
 *   with a value, 2 an async `Void` one, 3 an async one with a value
 * @returns {Function}
 */
export function bound(f, life, mode) {
  if (!life) return f;
  const g =
    mode >= 2
      ? (c, ...a) => {
        if (life.aborted) return mode === 2 ? Promise.resolve() : Promise.reject(CANCELLED);
        return f(either(c, life), ...a);
      }
      : (...a) => {
        if (!life.aborted) return f(...a);
        if (mode === 1) throw CANCELLED;
        return undefined;
      };
  g.lifetime = life;
  return g;
}

/**
 * A signal aborted when `c` or `life` is: the context a bound async value
 * runs under. Without a caller's context it is `life`.
 * @param {AbortSignal|null|undefined} c the caller's context
 * @param {AbortSignal} life the component's lifetime
 * @returns {AbortSignal}
 */
function either(c, life) {
  if (!c || c === life || c.aborted) return c || life;
  if (typeof AbortSignal.any === "function") return AbortSignal.any([c, life]);
  const both = new AbortController();
  const stop = () => both.abort();
  c.addEventListener("abort", stop, { once: true });
  life.addEventListener("abort", stop, { once: true });
  return both.signal;
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
