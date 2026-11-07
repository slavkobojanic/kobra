/**
 * @module k:runtime/keys
 *
 * `k.ui.KeyboardShortcut` in the browser
 * (docs/design/43-keyboard-shortcuts.md; docs/compiler/runtime.md §12.5):
 * one `keydown` listener on the document per mounted shortcut, removed
 * with the component. A key is a shortcut only when nothing handled it
 * already, no Control, Alt or Meta is held, and the user isn't typing in
 * a field; letters match in either case.
 *
 * Shipped as its own client chunk, only to apps that use shortcuts
 * (codegen.md §16), and measured against its own size budget.
 */

import { batch, onCleanup } from "./signals.js";

/** `true` for an element that takes typing. */
function typing(n) {
  return n != null && (["input", "textarea", "select"].includes(n.localName) || n.isContentEditable === true);
}

/**
 * The browser's `KeyboardShortcut`: listens for `key` on the page while
 * mounted and fires `pressed`; renders an empty comment.
 * @param {Record<string, Function>} props generated getters and `on$pressed`
 * @param {object} _ctx
 * @returns {Comment}
 */
export function KeyboardShortcut(props, _ctx) {
  const root = document.documentElement;
  const listener = (e) => {
    if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey || typing(e.target)) return;
    const k = String(props.key());
    const got = String(e.key);
    if (got !== k && !(k.length === 1 && got.toLowerCase() === k.toLowerCase())) return;
    e.preventDefault();
    batch(() => props.on$pressed?.());
  };
  root.addEventListener("keydown", listener);
  onCleanup(() => root.removeEventListener("keydown", listener));
  return document.createComment("");
}
