/**
 * @module k:runtime/dom
 *
 * The DOM renderer generated client code runs on: template cloning,
 * binding helpers and the control-flow blocks (docs/compiler/runtime.md,
 * "DOM renderer"). There is no virtual DOM; each dynamic part of a view is
 * one render effect that writes one node.
 */

import { batch, computed, getOwner, renderEffect, runWithOwner, scope, signal, untrack } from "./signals.js";
import { claimRange, claimTemplate, isHydrating } from "./hydrate.js";
import { CANCELLED } from "./values.js";

/**
 * Prepares a static HTML fragment once and returns a factory that clones
 * it, or, during hydration, claims the matching server-rendered nodes.
 * The fragment must have exactly one root element.
 * @param {string} html compiler-generated, already-escaped HTML
 * @returns {() => Element}
 */
export function template(html) {
  let prepared = null;
  return () => {
    if (prepared === null) {
      const t = document.createElement("template");
      t.innerHTML = html;
      prepared = t.content.firstChild;
    }
    if (isHydrating()) return claimTemplate(prepared);
    return /** @type {Element} */ (prepared.cloneNode(true));
  };
}

/**
 * Sets the text of a text node, writing only when it changed.
 * @param {Text} node
 * @param {string} value
 */
export function text(node, value) {
  if (node.data !== value) node.data = value;
}

/**
 * Sets an attribute; removes it for `null` or `false`, sets it empty for
 * `true`.
 * @param {Element} element
 * @param {string} name a compiler-checked attribute name
 * @param {string|boolean|null} value
 */
export function attr(element, name, value) {
  if (value === null || value === false) element.removeAttribute(name);
  else element.setAttribute(name, value === true ? "" : value);
}

/**
 * Sets a DOM property (`value`, `checked`), writing only when it changed.
 * @param {Element} element
 * @param {string} name
 * @param {unknown} value
 */
export function prop(element, name, value) {
  if (element[name] !== value) element[name] = value;
}

/**
 * Sets the `class` attribute from the static classes and the dynamic ones
 * (`.class([.panel, .expanded when x])`). Falsy entries are skipped.
 * @param {Element} element
 * @param {string} staticClasses hashed class names, space-separated
 * @param {Array<string|false|null|undefined>} dynamicClasses
 */
export function classList(element, staticClasses, dynamicClasses) {
  let value = staticClasses;
  for (const c of dynamicClasses) if (c) value = value === "" ? c : `${value} ${c}`;
  if (element.getAttribute("class") !== value) element.setAttribute("class", value);
}

/**
 * Sets a CSS custom property on an element (reactive style values,
 * `{accent}` in a style block).
 * @param {HTMLElement} element
 * @param {string} name `--k-…`
 * @param {string|null} value
 */
export function style(element, name, value) {
  if (value === null) element.style.removeProperty(name);
  else element.style.setProperty(name, value);
}

/** Bubbling events handled by one listener per document (runtime.md §5.4). */
const DELEGATED = new Set(["click", "input", "submit", "keydown"]);

/** `true` when `node` is in its document. */
function connected(n) {
  while (n.parentNode !== null) n = n.parentNode;
  return n.nodeType === 9;
}

/** Dispatches a delegated event to the handlers on the target's ancestors. */
function delegate(e) {
  for (let n = e.target; n !== null && n !== undefined; n = n.parentNode) {
    const h = n.$kh && n.$kh[e.type];
    if (h) {
      h(e);
      if (e.cancelBubble || e.propagationStopped) return;
    }
  }
}

/**
 * Listens to an event. The handler runs inside `batch`, so the DOM is
 * current when it returns; a promise it returns (an async handler) is not
 * awaited, and its rejection with {@link CANCELLED} (the handler stopped
 * because its component was disposed) is dropped, as is a synchronous
 * throw of it. Once the owner that was current when `on` was called (the
 * component) is disposed, the handler no longer runs: a disposed page's
 * nodes can stay on screen (the session gate leaves for a full page load
 * that replaces them later), and a click on them must not run its body
 * (03-reactivity.md, "What a component leaves behind"). Bubbling events (`click`, `input`, `submit`, `keydown`) on an
 * element already in the document are delegated: one listener per document
 * dispatches by a per-element handler table.
 * @param {EventTarget} target
 * @param {string} event
 * @param {(event: Event) => unknown} handler
 */
export function on(target, event, handler) {
  const owner = getOwner();
  const run = (e) => {
    if (owner !== null && owner.disposed) return;
    let r;
    try {
      r = batch(() => handler(e));
    } catch (x) {
      if (x === CANCELLED) return;
      throw x;
    }
    r?.then?.(undefined, (x) => {
      if (x !== CANCELLED) throw x;
    });
  };
  if (DELEGATED.has(event) && target.nodeType === 1 && connected(target)) {
    const root = target.ownerDocument.documentElement;
    const installed = (root.$kd ??= new Set());
    if (!installed.has(event)) {
      installed.add(event);
      root.addEventListener(event, delegate);
    }
    (target.$kh ??= {})[event] = run;
    return;
  }
  target.addEventListener(event, run);
}

/**
 * Removes the nodes strictly between `start` and `end`.
 * @param {Node} start
 * @param {Node} end
 */
export function clear(start, end) {
  const parent = end.parentNode;
  for (let n = start.nextSibling; n !== null && n !== end;) {
    const next = n.nextSibling;
    parent.removeChild(n);
    n = next;
  }
}

/**
 * Renders `f` in a child scope of `parent` (untracked) and returns the
 * nodes it produced (a fragment's children) and the scope's disposer.
 */
function mount(parent, start, end, f) {
  return runWithOwner(parent, () =>
    scope((dispose) => {
      const hydrating = isHydrating();
      const node = hydrating ? claimRange(start, end, f) : f();
      const nodes = place(node, end);
      return [nodes, dispose];
    }),
  );
}

/**
 * Inserts `node` (or a fragment's children) before `end` unless it is
 * already there, and returns the nodes it placed.
 * @param {Node|null} node
 * @param {Node} end
 * @returns {Node[]}
 */
export function place(node, end) {
  if (node === null || node === undefined) return [];
  const parent = end.parentNode;
  if (node.nodeType === 11) {
    const nodes = [...node.childNodes];
    parent.insertBefore(node, end);
    return nodes;
  }
  if (node.parentNode !== parent) parent.insertBefore(node, end);
  return [node];
}

/**
 * Renders `render()` while `condition()` is true, between the comment
 * markers `start` and `end`, in its own child scope (`if`, `if let`).
 * Only a change of the condition's truth re-renders.
 * @param {Comment} start
 * @param {Comment} end
 * @param {() => boolean} condition
 * @param {() => Node} render
 * @param {(() => Node)|null} otherwise
 */
export function when(start, end, condition, render, otherwise) {
  branch(start, end, () => (condition() ? 0 : 1), [render, otherwise]);
}

/**
 * Renders the arm `select()` returns, between the markers (`match`). The
 * arm re-renders only when the index changes; its old scope is disposed.
 * @param {Comment} start
 * @param {Comment} end
 * @param {() => number} select the index of the matching arm
 * @param {Array<(() => Node)|null>} arms
 */
export function branch(start, end, select, arms) {
  const parent = getOwner();
  const index = computed(() => select());
  let shown = -1;
  let dispose = null;
  renderEffect(() => {
    const i = index.get();
    if (i === shown) return;
    shown = i;
    untrack(() => {
      if (dispose !== null) dispose();
      dispose = null;
      if (!isHydrating()) clear(start, end);
      const f = arms[i];
      if (f) dispose = mount(parent, start, end, f)[1];
      else if (isHydrating()) claimRange(start, end, () => null);
    });
  });
}
