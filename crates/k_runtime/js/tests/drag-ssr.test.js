/**
 * @module k:/tests/drag-ssr.test.js
 *
 * `k.drag` on the server (docs/design/41-drag-and-drop.md, "Server rendering"): the components
 * render the elements and attributes the browser's do, slot content between block markers, so the
 * page reads before the client starts and the browser claims it while hydrating.
 */
import { assert, assertEq, test } from "k:testing";
import { Out, natives } from "k:runtime/server";
import { ATTRS, ReorderableList } from "k:/runtime/drag.js";
import { flush, hydrate, root, textNode } from "k:runtime";

const kind = natives["k.drag.DragKind$init"]("card");
/** The HTML that the server renderer of the `k.drag` component `name` writes for `props`. */
const render = (name, props) => {
  const out = new Out();
  natives[`k.drag.${name}$ssr`](props, {}, out);
  return out.html();
};

test("a kind is its name on the server too", () => {
  assertEq(kind.name, "card");
});

test("a draggable renders a focusable group named by its label around its content", () => {
  const html = render("Draggable", { kind: () => kind, payload: () => 1, label: () => 'Card "1" <b>', content: (out) => out.text("One") });
  assertEq(html, `<div ${ATTRS.draggable} aria-label="Card &quot;1&quot; &lt;b&gt;" ${ATTRS.source}><!--[-->One<!--]--></div>`);
});

test("a drop target renders a group named by its label", () => {
  const html = render("DropTarget", { kind: () => kind, label: () => "Trash", content: (out) => out.text("Drop here") });
  assertEq(html, `<div ${ATTRS.region} aria-label="Trash"><!--[-->Drop here<!--]--></div>`);
});

// `data-k-list`: the app's base stylesheet drops the list's bullets
// (07-themes-and-units.md, "Base styles").
test("a reorderable list renders a named list with a focusable item per element", () => {
  const items = [{ id: "a", title: "A & B" }, { id: "b", title: "C" }];
  const html = render("ReorderableList", { kind: () => kind, id: () => "todo", items: () => items, key: () => (t) => t.id, label: () => "To do", row: (t, out) => out.text(t.title) });
  assertEq(html, `<ul data-k-list aria-label="To do"><!--[--><li ${ATTRS.source}><!--[-->A &amp; B<!--]--></li><li ${ATTRS.source}><!--[-->C<!--]--></li><!--]--></ul>`);
});

test("the browser claims the server's list", () => {
  const items = [{ id: "a", title: "A" }];
  const props = { kind: () => kind, id: () => "todo", items: () => items, key: () => (t) => t.id, label: () => "To do" };
  const container = document.createElement("div");
  container.innerHTML = render("ReorderableList", { ...props, row: (t, out) => out.text(t.title) });
  document.body.appendChild(container);
  const ul = container.firstChild;
  const li = ul.childNodes[1];
  let node = null;
  root(() => {
    node = hydrate(container, () => ReorderableList({ ...props, row: (t) => textNode(t().title) }, {}), null);
  });
  flush();
  assertEq(node, ul);
  assertEq(ul.childNodes[1], li);
  const expected = document.createElement("div");
  expected.innerHTML = render("ReorderableList", { ...props, row: (t, out) => out.text(t.title) });
  assertEq(container.innerHTML, expected.innerHTML, "nothing changed");
  assert(li.getAttribute("tabindex") === "0");
});
