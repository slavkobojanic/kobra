/**
 * @module k:runtime/hydrate
 *
 * Hydration: attaching the client to server-rendered HTML without
 * re-rendering it (docs/compiler/runtime.md, "Hydration protocol").
 * While a page hydrates, `template()` claims existing nodes instead of
 * cloning, and the serialized `k-data` supplies `@Load` results and
 * request-scoped state so nothing is fetched twice.
 *
 * Claiming works on *ranges*: the container, and every block range
 * (`<!--[-->…<!--]-->`) found inside a claimed node. Templates claim the
 * next node of the active range; a block makes its own range active while
 * it renders its content. A node that doesn't match the template is a
 * mismatch: the range's server content is dropped and the client renders it.
 */

import { batch } from "./signals.js";
import { localTimeZone } from "./std.js";

/** `true` while a hydration pass is claiming server-rendered nodes. */
let hydrating = false;

/** The ranges templates claim from: `{ next, end, parent }`, innermost last. */
let active = [];

/** Server block ranges inside claimed nodes, by their start marker. */
let pending = new Map();

/**
 * Returns `true` while a hydration pass is claiming server-rendered nodes.
 * @returns {boolean}
 */
export function isHydrating() {
  return hydrating;
}

/** The counter behind `uid()`; reset by each hydration pass. */
let nextUid = 0;

/**
 * A fresh id for linking a `<label>` to its control. Ids restart at `k1`
 * with each hydration pass, as each server render's do (`Out.uid`), so a
 * page's ids depend only on the page and the client's match the server's.
 * @returns {string}
 */
export function uid() {
  return `k${++nextUid}`;
}

/**
 * Reads and parses the `<script type="application/json" id="k-data">`
 * element, or returns `null` when the page has none (a static-only app).
 * @param {Document} doc
 * @returns {Record<string, unknown>|null}
 */
export function readData(doc) {
  const script = doc.getElementById("k-data");
  if (script === null) return null;
  return JSON.parse(script.textContent ?? "null");
}

/**
 * Hydrates `container` with `render` (a generated factory): the factory's
 * templates claim the server's nodes, its bindings attach to them, and a
 * root that doesn't match is replaced. With an empty container this is
 * client rendering. User effects run in the flush after the pass.
 * @param {Element} container
 * @param {(ctx: object) => Node} render
 * @param {Record<string, unknown>|null} data the `k-data` payload
 * @returns {Node} the rendered root
 */
export function hydrate(container, render, data) {
  nextUid = 0;
  const range = { next: container.firstChild, end: null };
  active = [range];
  pending = new Map();
  let node;
  batch(() => {
    hydrating = true;
    try {
      node = render({ data });
    } finally {
      hydrating = false;
      active = [];
      pending = new Map();
    }
  });
  if (node && node.parentNode !== container) container.appendChild(node);
  if (data !== null && data !== undefined) rememberTimeZone(container.ownerDocument);
  if (afterPassQueue.length > 0) {
    // Queued after the `init()`s the pass queued (`afterHydration`), so
    // what runs here sees the state they loaded.
    const queued = afterPassQueue;
    afterPassQueue = [];
    queueMicrotask(() => batch(() => queued.forEach((f) => f())));
  }
  return node;
}

/** What runs once the current hydration pass is over (`afterPass`). */
let afterPassQueue = [];

/**
 * Runs `f` once the hydration pass is over and the client-only services'
 * `init()`s it queued have run, or now when no pass is running: the swap
 * of a client-rendered component from `loading` to `view`
 * (47-client-rendered-components.md).
 * @param {() => void} f
 */
export function afterPass(f) {
  if (hydrating) afterPassQueue.push(f);
  else f();
}

/** Sets the `k_tz` cookie so later server renders use the browser's zone. */
function rememberTimeZone(doc) {
  try {
    doc.cookie = `k_tz=${encodeURIComponent(localTimeZone())}; path=/; max-age=31536000; samesite=lax`;
  } catch {
    // No cookies or no Intl: the server keeps UTC.
  }
}

/**
 * The range templates claim from now: the innermost active one, or, once
 * it is used up, the first block range found in a claimed node (a child
 * inserted by hand into its parent's range).
 */
function currentRange() {
  let r = active.at(-1);
  if (r && (r.next === null || r.next === r.end) && pending.size > 0) {
    const [start, [end, held]] = pending.entries().next().value;
    pending.delete(start);
    end.parentNode.insertBefore(held, end);
    r = { next: start.nextSibling, end };
    active.push(r);
  }
  return r;
}

/**
 * Claims the next server node of the active range if it is a text
 * (`type` 3) or an empty comment, a `<!---->` separator (`type` 8);
 * otherwise, and when not hydrating, returns `make()`.
 * @param {number} type
 * @param {() => Node} make
 * @returns {Node}
 */
function claimNode(type, make) {
  // The active range itself: an exhausted range must not pull in a held
  // block range (`currentRange`), which belongs to a template's block.
  const r = hydrating ? active.at(-1) : null;
  const n = r ? r.next : null;
  if (n && n !== r.end && n.nodeType === type && (type === 3 || n.data === "")) {
    r.next = n.nextSibling;
    return n;
  }
  return make();
}

/**
 * The text node of a text at the top of a view or block: the server's
 * while hydrating, or a new one (also when the server rendered none, an
 * empty text).
 * @param {string} value
 * @returns {Text}
 */
export function textNode(value) {
  return claimNode(3, () => document.createTextNode(value));
}

/**
 * The `<!---->` that keeps two dynamic texts at the top of a view or block
 * apart, as the server writes it: the server's while hydrating, or a new one.
 * @returns {Comment}
 */
export function separator() {
  return claimNode(8, () => document.createComment(""));
}

/**
 * The markers of a block at the top of a view or block: during hydration
 * the server's `<!--[-->…<!--]-->` range is claimed, moved into the
 * returned fragment with its content (which the block then claims);
 * otherwise a fresh pair. `null` when there is no server range there.
 * @returns {{node: DocumentFragment, start: Comment, end: Comment}|null}
 */
export function claimMarkers() {
  const r = hydrating ? active.at(-1) : null;
  const start = r ? r.next : null;
  if (!start || start === r.end || start.nodeType !== 8 || start.data !== "[") return null;
  const end = closing(start);
  if (end === null) return null;
  r.next = end.nextSibling;
  const node = document.createDocumentFragment();
  for (let n = start, next; n !== r.next; n = next) {
    next = n.nextSibling;
    node.appendChild(n);
  }
  return { node, start, end };
}

/**
 * Makes the server range starting at marker `start` the active one while
 * `fn` runs (a block rendering its content during hydration), then removes
 * the server nodes `fn` didn't claim.
 * @template T
 * @param {Comment} start
 * @param {Comment} end
 * @param {() => T} fn
 * @returns {T}
 */
export function claimRange(start, end, fn) {
  const entry = pending.get(start);
  pending.delete(start);
  if (entry) end.parentNode.insertBefore(entry[1], end);
  const range = { next: start.nextSibling, end };
  active.push(range);
  try {
    return fn();
  } finally {
    const i = active.indexOf(range);
    if (i >= 0) active.splice(i, 1);
    // Unclaimed server nodes in the range are stale.
    drop(range.next, end);
  }
}

/**
 * Removes `n` and its following siblings up to (not including) `end`, or
 * to the last sibling when `end` is `null`.
 * @param {Node|null} n
 * @param {Node|null} end
 */
export function drop(n, end) {
  while (n !== null && n !== end) {
    const next = n.nextSibling;
    n.parentNode.removeChild(n);
    n = next;
  }
}

/** `true` if the server node can stand for the template node. */
function sameShape(t, s) {
  if (s === null || t.nodeType !== s.nodeType) return false;
  return t.nodeType !== 1 || t.localName === s.localName;
}

/** Finds the comment that closes the range opened at `start`. */
function closing(start) {
  let depth = 0;
  for (let n = start.nextSibling; n !== null; n = n.nextSibling) {
    if (n.nodeType === 8) {
      if (n.data === "[") depth++;
      else if (n.data === "]") {
        if (depth === 0) return n;
        depth--;
      }
    }
  }
  return null;
}

/**
 * Aligns the children of server element `s` with template element `t`:
 * creates the text nodes the server left out (empty texts), keeps block
 * ranges for the blocks to claim, and replaces children of another shape.
 */
function reconcile(t, s) {
  // A textarea's text is its initial value, which its binding owns.
  if (s.localName === "textarea") return;
  let sc = s.firstChild;
  for (let tc = t.firstChild; tc !== null; tc = tc.nextSibling) {
    if (tc.nodeType === 3) {
      if (sc !== null && sc.nodeType === 3) {
        sc = sc.nextSibling;
      } else {
        s.insertBefore(s.ownerDocument.createTextNode(""), sc);
      }
      continue;
    }
    if (tc.nodeType === 8 && tc.data === "[" && sc !== null && sc.nodeType === 8 && sc.data === "[") {
      const close = closing(sc);
      if (close !== null) {
        // The range's server content is held aside until its block claims
        // it, so paths into the claimed node match the template.
        const held = s.ownerDocument.createDocumentFragment();
        while (sc.nextSibling !== close) held.appendChild(sc.nextSibling);
        pending.set(sc, [close, held]);
        sc = close;
        // The template's `]` aligns with the server's.
        tc = tc.nextSibling;
        if (tc === null) break;
        sc = sc.nextSibling;
        continue;
      }
    }
    if (sameShape(tc, sc)) {
      if (tc.nodeType === 1) reconcile(tc, sc);
      sc = sc.nextSibling;
      continue;
    }
    s.insertBefore(tc.cloneNode(true), sc);
    if (sc !== null) {
      const next = sc.nextSibling;
      s.removeChild(sc);
      sc = next;
    }
  }
  drop(sc, null);
}

/**
 * Claims the next server-rendered node as it is, children included, for a
 * native component that owns its subtree (`LineChart`): the node when it
 * is a `tag` element, otherwise `null`, with the range's server content
 * dropped (a mismatch). Always `null` outside a hydration pass.
 * @param {string} tag
 * @returns {Element|null}
 */
export function claimRaw(tag) {
  return hydrating ? claimNext((n) => n.localName === tag) : null;
}

/**
 * Claims the next server node of the current range when `fits` accepts
 * it; otherwise drops the range's server content (a mismatch) and returns
 * `null`.
 * @param {(n: Node) => boolean} fits
 * @returns {Node|null}
 */
function claimNext(fits) {
  const range = currentRange();
  const node = range ? range.next : null;
  if (node && node !== range.end && fits(node)) {
    range.next = node.nextSibling;
    return node;
  }
  if (range) {
    drop(range.next, range.end);
    range.next = range.end;
  }
  return null;
}

/**
 * Claims the next server-rendered node for a template during hydration.
 * Returns the server node when it has the template's shape; otherwise the
 * range's server content is removed and a fresh clone is returned for the
 * caller to insert.
 * @param {Element} prepared the parsed template root
 * @returns {Element}
 */
export function claimTemplate(prepared) {
  const node = claimNext((n) => sameShape(prepared, n));
  if (node) reconcile(prepared, node);
  // A mismatch rendered fresh.
  return node ?? /** @type {Element} */ (prepared.cloneNode(true));
}
