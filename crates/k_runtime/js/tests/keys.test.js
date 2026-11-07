// `k.ui.KeyboardShortcut` (docs/design/43-keyboard-shortcuts.md; docs/compiler/runtime.md §12.5): a key
// pressed anywhere on the page fires `pressed` while the component is mounted, never while the user
// types in a field or holds a modifier, and letters match in either case.
import { assert, assertEq, test } from "k:testing";
import { KeyboardShortcut } from "k:/runtime/keys.js";
import { flush, root } from "k:runtime";

/** Mounts a shortcut for `key`, counting presses; returns the count reader and the disposer. */
function shortcut(key) {
  let count = 0;
  let dispose = null;
  root((d) => {
    dispose = d;
    document.body.appendChild(KeyboardShortcut({ key: () => key, label: () => "New card", on$pressed: () => count++ }, {}));
  });
  flush();
  return { count: () => count, dispose };
}

const press = (el, key, extra = {}) => {
  const e = Object.assign(new Event("keydown", { bubbles: true }), { key, ...extra });
  el.dispatchEvent(e);
  return e;
};

test("the key pressed anywhere on the page fires pressed and isn't typed", () => {
  const s = shortcut("n");
  const box = document.createElement("div");
  document.body.appendChild(box);
  const e = press(box, "n");
  assertEq(s.count(), 1);
  assert(e.defaultPrevented, "the key does nothing else");
  press(box, "N", { shiftKey: true });
  assertEq(s.count(), 2, "letters match in either case");
  press(box, "m");
  assertEq(s.count(), 2);
});

test("keys typed into a field, or with Control, Alt or Meta, are not shortcuts", () => {
  const s = shortcut("n");
  for (const tag of ["input", "textarea", "select"]) {
    const field = document.createElement(tag);
    document.body.appendChild(field);
    assert(!press(field, "n").defaultPrevented);
  }
  const box = document.createElement("div");
  document.body.appendChild(box);
  press(box, "n", { ctrlKey: true });
  press(box, "n", { metaKey: true });
  press(box, "n", { altKey: true });
  assertEq(s.count(), 0);
});

test("a key handled already (a drag's Space, a dialog's Escape) is not a shortcut", () => {
  const s = shortcut("Escape");
  const box = document.createElement("div");
  document.body.appendChild(box);
  box.addEventListener("keydown", (e) => e.preventDefault());
  press(box, "Escape");
  assertEq(s.count(), 0);
});

test("a key name matches exactly, and unmounting removes the shortcut", () => {
  const s = shortcut("?");
  press(document.body, "?");
  assertEq(s.count(), 1);
  s.dispose();
  press(document.body, "?");
  assertEq(s.count(), 1, "gone with its component");
});

test("the shortcut renders nothing visible", () => {
  let node = null;
  root(() => {
    node = KeyboardShortcut({ key: () => "n", label: () => "New card" }, {});
  });
  assertEq(node.nodeType, 8, "a comment");
});
