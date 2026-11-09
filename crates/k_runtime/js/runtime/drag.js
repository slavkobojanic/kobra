/**
 * @module k:runtime/drag
 *
 * `k.drag` in the browser (docs/design/41-drag-and-drop.md;
 * docs/compiler/runtime.md §12.3): `Draggable`, `DropTarget` and
 * `ReorderableList`, one drag at a time per page.
 *
 * - **Pointer.** Pointer events serve the mouse, pens and touch alike. A
 *   mouse or pen drag starts once the pointer moved 4px with the button
 *   down; a touch drag starts after a 250ms press that moved less than
 *   8px, so a swipe still scrolls, and while it runs `touchmove` is
 *   cancelled so the page doesn't. The target is the drop region under the
 *   pointer (`elementFromPoint`, else the event's target); over a list's
 *   row the item takes that row's position. A copy of the dragged element
 *   follows the pointer.
 * - **Keyboard** (the WAI-ARIA alternative to dragging): Space or Enter on
 *   a focused source picks it up, ArrowUp/ArrowDown move it within a
 *   list, ArrowLeft/ArrowRight (or any arrow for a `Draggable`) go to the
 *   previous or next region that accepts it, Space or Enter drops, Escape
 *   or leaving the element cancels. Each step is announced in an
 *   assertive live region, and every source is described by one hidden
 *   help text. After a keyboard drop the moved row gets the focus back.
 *
 * Nothing moves until the drop: a list shows an indicator where the item
 * would land, the region under the drag carries `data-k-drop-target`, and
 * the source `data-k-dragging`. A drop fires the component's event in a
 * batch (`dropped`, `moved`), and the app's state change redraws.
 *
 * Shipped as its own client chunk, only to apps that use `k.drag`
 * (codegen.md §16), and measured against its own size budget. The server
 * renders the same elements (`$s.natives`, `app.js`) with `ATTRS`.
 */

import { claimRange, claimRaw, isHydrating } from "./hydrate.js";
import { place } from "./dom.js";
import { each } from "./list.js";
import { batch, flush, onCleanup, renderEffect, untrack } from "./signals.js";

/** The id of the hidden help text that describes every drag source. */
const HELP = "k-drag-help";

/**
 * The static attributes of the elements, shared with the server's
 * rendering so the hydrating client claims identical markup: a source
 * (`Draggable`'s element and every list row) and a drop region.
 */
export const ATTRS = {
  source: `tabindex="0" aria-describedby="${HELP}" style="touch-action: manipulation; user-select: none; -webkit-user-select: none; -webkit-touch-callout: none"`,
  draggable: 'role="group" aria-roledescription="draggable"',
  region: 'role="group"',
};

/** `VisuallyHidden`'s styles (codegen.md §10). */
const OFFSCREEN = "position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0";

/** The drag in progress, or `null`. */
let drag = null;
/** A pointer pressed on a source that hasn't become a drag yet. */
let pressed = null;
/** Every mounted drop region (lists included). */
const regions = new Set();
/** The list indicator and the pointer's copy, made on first use. */
let marker = null;
/** The copy of the dragged element that follows the pointer, made on first use. */
let ghost = null;
/** The root element the document-wide pointer listeners are installed on. */
let listening = null;
/** Swallows the click that ends a pointer drag. */
let swallow = false;

/** Sets the attributes of an `ATTRS` entry on `el`. */
function attrs(el, text) {
  for (const [, name, value] of text.matchAll(new RegExp('([a-z-]+)="([^"]*)"', "g"))) el.setAttribute(name, value);
}

/** Creates a hidden element with `id` in the body, once. */
function hidden(id, setup) {
  let el = document.getElementById(id);
  if (el === null) {
    el = document.createElement("div");
    el.setAttribute("id", id);
    el.setAttribute("style", OFFSCREEN);
    setup(el);
    document.body.appendChild(el);
  }
  return el;
}

/** Announces `text` in the assertive live region. */
function say(text) {
  hidden("k-drag-live", (el) => {
    el.setAttribute("aria-live", "assertive");
    el.setAttribute("aria-atomic", "true");
  }).textContent = text;
}

/**
 * Makes `el` a drag source: `make()` describes the drag it starts
 * (`{kind, payload, label, list, index, key}`).
 */
function source(el, make) {
  attrs(el, ATTRS.source);
  hidden(HELP, (h) => {
    h.textContent = "Press Space or Enter to pick up. While carrying, the arrow keys move, Space or Enter drops and Escape cancels.";
  });
  const begin = (pointer) => {
    drag = { ...make(), el, pointer, target: null, at: -1 };
    el.setAttribute("data-k-dragging", "");
    if (drag.list) {
      drag.target = drag.list;
      drag.at = drag.index;
    }
  };
  el.addEventListener("keydown", (e) => {
    if (e.target !== el) return;
    const k = e.key;
    if (drag === null) {
      if (k !== " " && k !== "Enter") return;
      e.preventDefault();
      begin(false);
      mark();
      say(`Picked up ${drag.label}. ${where()}`);
      return;
    }
    if (drag.el !== el || drag.pointer) return;
    const step = { ArrowUp: [0, -1], ArrowDown: [0, 1], ArrowLeft: [-1, 0], ArrowRight: [1, 0] }[k];
    if (step) move(step[0], step[1]);
    else if (k === " " || k === "Enter") release();
    else if (k === "Escape") cancel();
    else return;
    e.preventDefault();
  });
  el.addEventListener("blur", () => {
    if (drag !== null && drag.el === el && !drag.pointer) cancel();
  });
  el.addEventListener("pointerdown", (e) => {
    swallow = false;
    if (drag !== null || (e.button ?? 0) !== 0 || control(e.target, el)) return;
    const touch = e.pointerType === "touch";
    pressed = { x: e.clientX ?? 0, y: e.clientY ?? 0, ready: !touch, begin, el };
    if (touch && typeof setTimeout === "function") {
      const p = pressed;
      setTimeout(() => {
        if (pressed === p) start(p, p.x, p.y, p.el);
      }, 250);
    }
    listen(el.ownerDocument.documentElement);
  });
}

/** `true` when `target` is a control inside `el`, which keeps its own clicks and keys. */
function control(target, el) {
  for (let n = target; n && n !== el; n = n.parentNode) {
    if (["button", "input", "select", "textarea", "a"].includes(n.localName)) return true;
  }
  return false;
}

/** Starts the pointer drag pressed `p`, with the pointer at (x, y) over `under`. */
function start(p, x, y, under) {
  pressed = null;
  p.begin(true);
  const r = p.el.getBoundingClientRect?.() ?? { left: 0, top: 0, width: 0 };
  ghost = p.el.cloneNode(true);
  for (const n of [ghost, ...(ghost.querySelectorAll?.("[id]") ?? [])]) n.removeAttribute("id");
  ghost.setAttribute("aria-hidden", "true");
  ghost.setAttribute("style", `position: fixed; left: ${r.left}px; top: ${r.top}px; width: ${r.width}px; margin: 0; pointer-events: none; z-index: 2147483647; opacity: 0.85`);
  document.body.appendChild(ghost);
  drag.x = p.x;
  drag.y = p.y;
  over(x, y, under);
}

/** Installs the document-wide pointer listeners on `root`, once per document. */
function listen(root) {
  if (listening === root) return;
  listening = root;
  root.addEventListener("pointermove", (e) => {
    const x = e.clientX ?? 0;
    const y = e.clientY ?? 0;
    if (pressed !== null) {
      const far = Math.hypot(x - pressed.x, y - pressed.y);
      if (!pressed.ready) {
        if (far > 8) pressed = null;
      } else if (far > 4) start(pressed, x, y, e.target);
      return;
    }
    if (drag !== null && drag.pointer) over(x, y, e.target);
  });
  root.addEventListener("pointerup", () => {
    pressed = null;
    if (drag !== null && drag.pointer) {
      swallow = true;
      release();
    }
  });
  root.addEventListener("pointercancel", () => {
    pressed = null;
    if (drag !== null && drag.pointer) cancel();
  });
  root.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && drag !== null && drag.pointer) cancel();
  });
  root.addEventListener("click", (e) => {
    if (swallow) {
      swallow = false;
      e.stopPropagation();
      e.preventDefault();
    }
  }, true);
  root.addEventListener("touchmove", (e) => {
    if (drag !== null && drag.pointer) e.preventDefault();
  }, { passive: false });
}

/** Follows the pointer at (x, y): the copy, the region under it and the position there. */
function over(x, y, fallback) {
  ghost?.style.setProperty("transform", `translate(${x - drag.x}px, ${y - drag.y}px)`);
  const hit = document.elementFromPoint?.(x, y) ?? fallback;
  let n = hit;
  let row = null;
  while (n && !regions.has(n.$kd)) {
    if (n.$kr) row = n;
    n = n.parentNode;
  }
  const t = n ? n.$kd : near(hit);
  if (t && accepts(t)) {
    drag.target = t;
    drag.at = t.list ? (row && row.$kr === t ? t.rows().indexOf(row) : lastAt(t)) : -1;
  } else {
    drag.target = null;
  }
  mark();
}

/**
 * The region of the drag's kind around the point when the pointer is over
 * no region: the one region inside the innermost element under the pointer
 * that holds any, such as a column's list when the pointer is on the
 * column's heading, its padding or below an empty (zero-height) list
 * (41-drag-and-drop.md). An element holding several (the board) is no
 * target, so the drop stays where it was.
 */
function near(hit) {
  for (let a = hit; a && a.nodeType === 1 && a !== a.ownerDocument?.body; a = a.parentNode) {
    let found = null;
    for (const r of regions) {
      if (r.kind() !== drag.kind || !within(r.el, a)) continue;
      if (found !== null) return null;
      found = r;
    }
    if (found !== null) return found;
  }
  return null;
}

/** `true` when `n` is `a` or inside it. */
function within(n, a) {
  for (; n; n = n.parentNode) if (n === a) return true;
  return false;
}

/** `true` when region `t` accepts the drag: its kind, and its rule (a list always takes back its own item). */
function accepts(t) {
  return t.kind() === drag.kind && (t === drag.list || (drag.list !== null || !t.list) && t.accepts(drag.payload));
}

/** The last position the drag can take in list `t`. */
function lastAt(t) {
  return t.items().length - (t === drag.list ? 1 : 0);
}

/** The keyboard's step: within a list (`dy`), or to the previous or next accepting region. */
function move(dx, dy) {
  const t = drag.target;
  if (dy !== 0 && t !== null && t.list) {
    drag.at = Math.max(0, Math.min(lastAt(t), drag.at + dy));
  } else {
    const all = ordered().filter(accepts);
    if (all.length > 0) {
      const i = all.indexOf(t);
      const next = all[i < 0 ? (dx + dy > 0 ? 0 : all.length - 1) : (i + dx + dy + all.length) % all.length];
      drag.at = next.list ? Math.min(Math.max(drag.at, 0), lastAt(next)) : -1;
      drag.target = next;
    }
  }
  mark();
  say(drag.target === null ? `${drag.label} is over no drop target.` : `${drag.label}: ${where()}`);
}

/** The mounted regions in document order. */
function ordered() {
  const out = [];
  const walk = (n) => {
    for (let c = n.firstChild; c !== null; c = c.nextSibling) {
      if (c.$kd && regions.has(c.$kd)) out.push(c.$kd);
      walk(c);
    }
  };
  walk(document.documentElement);
  return out;
}

/** Where the drag is, in words. */
function where() {
  const t = drag.target;
  if (t === null) return "";
  if (!t.list) return `Over ${t.label()}.`;
  return `Position ${drag.at + 1} of ${lastAt(t) + 1} in ${t.label()}.`;
}

/** Shows where the drag is: the region's attribute and a list's indicator. */
function mark() {
  for (const r of regions) {
    r.el.removeAttribute("data-k-drop-target");
    r.el.style.removeProperty("outline");
  }
  marker?.remove();
  const t = drag?.target;
  if (!t) return;
  t.el.setAttribute("data-k-drop-target", "");
  t.el.style.setProperty("outline", "2px solid var(--k-color-drop-target, #93C5FD)");
  if (t.list) {
    if (marker?.ownerDocument !== document) marker = indicator();
    const rows = t.rows().filter((r) => r !== drag.el);
    t.end.parentNode.insertBefore(marker, rows[drag.at] ?? t.end);
  }
}

/** The list indicator: a line where the item would land. */
function indicator() {
  const li = document.createElement("li");
  li.setAttribute("aria-hidden", "true");
  li.setAttribute("class", "k-drop-indicator");
  li.setAttribute("style", "list-style: none; height: 0; margin: 0; border-top: 2px solid var(--k-color-drop-indicator, #2563EB)");
  return li;
}

/** Ends the drag and removes what it showed; returns the drag. */
function end() {
  const d = drag;
  drag = null;
  mark();
  ghost?.remove();
  ghost = null;
  d.el.removeAttribute("data-k-dragging");
  return d;
}

/** Cancels the drag. */
function cancel() {
  const d = end();
  say(`Cancelled. ${d.label} is back where it was.`);
}

/** Drops: fires the region's event, then gives a moved row the focus back. */
function release() {
  const d = end();
  const t = d.target;
  if (t === null || (t === d.list && d.at === d.index)) {
    say(`Dropped ${d.label} where it was.`);
    return;
  }
  say(t.list ? `Dropped ${d.label}. Position ${d.at + 1} of ${t.items().length + (t === d.list ? 0 : 1)} in ${t.label()}.` : `Dropped ${d.label} on ${t.label()}.`);
  batch(() => t.drop(d));
  flush();
  if (!d.pointer && t.list) {
    const row = t.rows().find((r) => r.$kk === d.key);
    row?.focus?.();
  }
}

/** Registers region `t` (`el`, `kind`, `label`, `accepts`, `drop`; lists also `items`, `rows`, `end`) while mounted. */
function region(t) {
  t.el.$kd = t;
  regions.add(t);
  onCleanup(() => regions.delete(t));
}

/**
 * The element of a native component holding a slot between markers: the
 * server's element while hydrating (its slot content claimed), else a new
 * one. Returns the element and its markers.
 * @param {string} tag
 * @param {(() => Node)|null} render the slot's content
 * @returns {[Element, Comment, Comment]}
 */
function host(tag, render) {
  let el = claimRaw(tag);
  let start = el?.firstChild;
  let close = el?.lastChild;
  if (!el || start?.nodeType !== 8 || close?.nodeType !== 8 || start === close) {
    el ??= document.createElement(tag);
    while (el.firstChild) el.removeChild(el.firstChild);
    start = el.appendChild(document.createComment("["));
    close = el.appendChild(document.createComment("]"));
  }
  if (render) place(isHydrating() ? claimRange(start, close, render) : render(), close);
  return [el, close, start];
}

/** A function prop's value, or `fallback` when it isn't set. */
const fn = (props, name, fallback) => (props[name] ? props[name]() : fallback);

/**
 * `DragKind(name:)`: a kind is its name.
 * @param {string} name
 * @returns {{name: string}}
 */
export function DragKind$init(name) {
  return { name: String(name) };
}

/**
 * The browser's `Draggable`: its content in a focusable group named
 * `label`, a source of drags of `kind` carrying `payload`.
 * @param {Record<string, Function>} props generated getters and the `content` slot
 * @param {object} _ctx
 * @returns {Element}
 */
export function Draggable(props, _ctx) {
  const [el] = host("div", props.content);
  attrs(el, ATTRS.draggable);
  renderEffect(() => el.setAttribute("aria-label", props.label()));
  source(el, () => ({ kind: props.kind().name, payload: props.payload(), label: props.label(), list: null, index: -1, key: null }));
  return el;
}

/**
 * The browser's `DropTarget`: its content in a group named `label` that
 * accepts drags of `kind` its `accepts` rule allows, firing `dropped`.
 * @param {Record<string, Function>} props
 * @param {object} _ctx
 * @returns {Element}
 */
export function DropTarget(props, _ctx) {
  const [el] = host("div", props.content);
  attrs(el, ATTRS.region);
  renderEffect(() => el.setAttribute("aria-label", props.label()));
  region({
    el,
    kind: () => props.kind().name,
    label: () => props.label(),
    accepts: (v) => fn(props, "accepts", () => true)(v),
    drop: (d) => props.on$dropped?.(d.payload),
  });
  return el;
}

/**
 * The browser's `ReorderableList`: a `ul` named `label` with one focusable
 * `li` per item (kept by `key`, drawn by the `row` slot), whose items move
 * within it and to other lists of the same kind; the receiving list fires
 * `moved` with a `ListMove`.
 * @param {Record<string, Function>} props
 * @param {object} _ctx
 * @returns {Element}
 */
export function ReorderableList(props, _ctx) {
  const [ul, close, open] = host("ul", null);
  // The app's base stylesheet drops the list's bullets, margin and padding
  // (`:where(ul[data-k-list])`, 07-themes-and-units.md, "Base styles").
  ul.setAttribute("data-k-list", "");
  renderEffect(() => ul.setAttribute("aria-label", props.label()));
  const key = (v) => String(props.key()(v));
  const name = (v) => fn(props, "itemLabel", () => "")(v) || key(v);
  const t = {
    el: ul,
    end: close,
    list: true,
    kind: () => props.kind().name,
    label: () => props.label(),
    id: () => props.id(),
    items: () => props.items(),
    rows: () => [...ul.childNodes].filter((n) => n.$kr === t),
    accepts: (v) => fn(props, "accepts", () => true)(v),
    drop: (d) => props.on$moved?.({ item: d.payload, fromList: d.list.id(), fromIndex: d.index, toList: t.id(), toIndex: d.at }),
  };
  region(t);
  each(open, close, () => props.items(), key, (item, index) => {
    const [li, end, start] = host("li", null);
    li.$kr = t;
    renderEffect(() => {
      li.$kk = key(item());
    });
    // The row slot reads its item through the accessor, so a changed item
    // under the same key updates the row in place (its focus stays).
    untrack(() => place(isHydrating() ? claimRange(start, end, () => props.row(item)) : props.row(item), end));
    source(li, () => ({ kind: t.kind(), payload: item(), label: name(item()), list: t, index: index(), key: key(item()) }));
    return li;
  });
  return ul;
}
