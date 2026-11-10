/**
 * @module k:/tests/refs.test.js
 *
 * `k.ui.ElementRef` (docs/design/53-element-references.md): an operation runs once the view has
 * caught up, at the end of the batch that called it, on the element attached then; it does
 * nothing while none is, or while it isn't in the document; a heading takes the focus from code
 * only; a text field's text can be selected; scrolling follows the alignment and reduced motion.
 */
import { assert, assertEq, test, tick } from "k:testing";
import { ElementRef } from "k:/runtime/refs.js";
import { batch, flush, root, signal, when } from "k:runtime";

/** An element of `tag` in the document. */
function shown(tag) {
  const el = document.createElement(tag);
  document.body.appendChild(el);
  return el;
}

test("focus runs at the end of the batch, on the element attached by then", () => {
  const ref = ElementRef();
  const input = shown("input");
  batch(() => {
    ref.focus(false);
    assert(document.activeElement !== input, "not before the view caught up");
    root(() => ref.$attach(input));
  });
  assertEq(document.activeElement, input);
});

test("an operation outside a batch runs in the next flush", async () => {
  const ref = ElementRef();
  const input = shown("input");
  root(() => ref.$attach(input));
  ref.focus(false);
  await tick();
  assertEq(document.activeElement, input);
});

test("nothing happens while no element is attached, or once it is removed", () => {
  const ref = ElementRef();
  const before = document.activeElement;
  batch(() => ref.focus(false));
  assertEq(document.activeElement, before, "never attached");
  const input = document.createElement("input");
  root(() => ref.$attach(input));
  batch(() => ref.focus(false));
  assertEq(document.activeElement, before, "not in the document");
  document.body.appendChild(input);
  let dispose = null;
  const other = ElementRef();
  root((d) => {
    dispose = d;
    other.$attach(input);
  });
  dispose();
  batch(() => other.focus(false));
  assertEq(document.activeElement, before, "its owner was disposed");
});

test("a branch that inserts the element and focuses it in one handler finds it", () => {
  const ref = ElementRef();
  const open = signal(false);
  const host = shown("div");
  const start = document.createComment("");
  const end = document.createComment("");
  host.appendChild(start);
  host.appendChild(end);
  root(() => {
    when(
      start,
      end,
      () => open.get(),
      () => {
        const input = document.createElement("input");
        ref.$attach(input);
        return input;
      },
      null,
    );
  });
  flush();
  batch(() => {
    open.set(true);
    ref.focus(false);
  });
  assertEq(document.activeElement?.localName, "input");
});

test("an element that can't take the focus gets tabindex -1; a field's text can be selected", () => {
  const ref = ElementRef();
  const h1 = shown("h1");
  h1.tabIndex = -1;
  root(() => ref.$attach(h1));
  batch(() => ref.focus(false));
  assertEq(h1.getAttribute("tabindex"), "-1");
  assertEq(document.activeElement, h1);
  const button = shown("button");
  button.tabIndex = 0;
  const b = ElementRef();
  root(() => b.$attach(button));
  batch(() => b.focus(false));
  assertEq(button.getAttribute("tabindex"), null, "a button keeps its own Tab stop");
  const field = shown("input");
  field.value = "Buy milk";
  const f = ElementRef();
  root(() => f.$attach(field));
  batch(() => f.focus(true));
  assertEq(field.selectionEnd - field.selectionStart, 8);
  batch(() => f.blur());
  assert(document.activeElement !== field, "blur takes the focus away");
});

test("scrollIntoView aligns as asked and jumps when the user prefers reduced motion", () => {
  const ref = ElementRef();
  const el = shown("section");
  root(() => ref.$attach(el));
  batch(() => ref.scrollIntoView({ $: 1 }, true));
  assertEq(el.scrolledIntoView.block, "center");
  assertEq(el.scrolledIntoView.behavior, "smooth");
  const saved = globalThis.matchMedia;
  globalThis.matchMedia = (q) => ({ matches: q === "(prefers-reduced-motion: reduce)" });
  try {
    batch(() => ref.scrollIntoView({ $: 0 }, true));
  } finally {
    globalThis.matchMedia = saved;
  }
  assertEq(el.scrolledIntoView.block, "start");
  assertEq(el.scrolledIntoView.behavior, "auto");
  batch(() => ref.scrollIntoView(undefined, false));
  assertEq(el.scrolledIntoView.block, "nearest");
});
