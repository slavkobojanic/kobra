/**
 * @module k:/tests/drag.test.js
 *
 * `k.drag` in the browser (docs/design/41-drag-and-drop.md; docs/compiler/runtime.md §12.3):
 * `Draggable`, `DropTarget` and `ReorderableList` in the fake DOM. Pointer drags (mouse and touch)
 * and the keyboard alternative reach the same events with the same payloads; nothing moves until the
 * drop, the app's state change redraws, and every keyboard step is announced.
 */
import { assert, assertDeepEq, assertEq, installDom, test } from "k:testing";
import { DragKind$init, Draggable, DropTarget, ReorderableList } from "k:/runtime/drag.js";
import { flush, hydrate, root, signal, textNode } from "k:runtime";

const card = DragKind$init("card");
const other = DragKind$init("column");

/** Mounts `make()` in the body inside a root, and returns its node. */
function mount(make) {
  let node = null;
  root(() => {
    node = make();
  });
  document.body.appendChild(node);
  flush();
  return node;
}

/** A text slot. */
const say = (s) => () => document.createTextNode(s);

/** Dispatches a bubbling event of `type` with `extra` fields on `el`. */
function fire(el, type, extra = {}) {
  const e = Object.assign(new Event(type, { bubbles: true }), extra);
  el.dispatchEvent(e);
  return e;
}

/** Presses `key` on `el`. */
const press = (el, key) => fire(el, "keydown", { key });

/** The live region's text. */
const announced = () => document.getElementById("k-drag-live")?.textContent ?? "";

/** A board of two lists of `{id, title}` that applies `moved` like an app. */
function board(lists = { todo: ["a", "b", "c"], done: ["d"] }, over = {}) {
  const state = {};
  const moves = [];
  for (const [id, keys] of Object.entries(lists)) state[id] = signal(keys.map((k) => ({ id: k, title: `Card ${k}` })));
  const apply = (m) => {
    moves.push({ ...m, item: m.item.id });
    const from = [...state[m.fromList].peek()];
    from.splice(m.fromIndex, 1);
    state[m.fromList].set(from);
    const to = [...state[m.toList].peek()];
    to.splice(m.toIndex, 0, m.item);
    state[m.toList].set(to);
  };
  const host = document.createElement("div");
  for (const id of Object.keys(lists)) {
    host.appendChild(
      mount(() =>
        ReorderableList({
          kind: () => card,
          id: () => id,
          items: () => state[id].get(),
          key: () => (t) => t.id,
          label: () => (id === "todo" ? "To do" : id === "done" ? "Done" : id),
          itemLabel: () => (t) => t.title,
          row: (t) => document.createTextNode(t.title),
          on$moved: apply,
          ...(over[id] ?? {}),
        }),
      ),
    );
  }
  document.body.appendChild(host);
  const list = (id) => [...host.childNodes].find((ul) => ul.getAttribute("aria-label") === (id === "todo" ? "To do" : id === "done" ? "Done" : id));
  const rows = (id) => [...list(id).childNodes].filter((n) => n.nodeType === 1 && n.getAttribute("class") !== "k-drop-indicator");
  const titles = (id) => rows(id).map((r) => r.textContent);
  return { state, moves, list, rows, titles };
}

// ------------------------------------------------------------- rendering

test("a draggable is a focusable group named by its label and described by the help text", () => {
  const el = mount(() => Draggable({ kind: () => card, payload: () => 7, label: () => "Card 7", content: say("Seven") }));
  assertEq(el.localName, "div");
  assertEq(el.getAttribute("role"), "group");
  assertEq(el.getAttribute("aria-roledescription"), "draggable");
  assertEq(el.getAttribute("aria-label"), "Card 7");
  assertEq(el.getAttribute("tabindex"), "0");
  assertEq(el.getAttribute("aria-describedby"), "k-drag-help");
  assertEq(el.textContent, "Seven");
  const help = document.getElementById("k-drag-help");
  assert(help !== null && help.textContent.includes("Space or Enter"), "one help text for every source");
  assert(el.style.getPropertyValue("touch-action") === "manipulation", "touch can still scroll and zoom");
});

test("a reorderable list is a list named by its label, with one focusable item per element", () => {
  const b = board();
  const ul = b.list("todo");
  assertEq(ul.localName, "ul");
  assertDeepEq(b.titles("todo"), ["Card a", "Card b", "Card c"]);
  for (const li of b.rows("todo")) {
    assertEq(li.localName, "li");
    assertEq(li.getAttribute("tabindex"), "0");
    assertEq(li.getAttribute("aria-describedby"), "k-drag-help");
  }
});

test("rows are kept by key: a reorder of the items moves the same elements", () => {
  const b = board();
  const [a, bb, c] = b.rows("todo");
  b.state.todo.set([...b.state.todo.peek()].reverse());
  flush();
  assertDeepEq(b.rows("todo"), [c, bb, a]);
  b.state.todo.set(b.state.todo.peek().map((t) => (t.id === "b" ? { ...t, title: "Renamed" } : t)));
  flush();
  assertEq(b.rows("todo")[1], bb, "a changed item under the same key keeps its row");
  assertEq(bb.textContent, "Renamed", "and its row slot draws it again");
});

// --------------------------------------------------------------- keyboard

test("Space picks a row up, arrows move it within its list, Enter drops and fires moved once", () => {
  const b = board();
  const [a] = b.rows("todo");
  a.focus();
  const down = press(a, " ");
  assert(down.defaultPrevented, "Space doesn't scroll the page");
  assert(a.hasAttribute("data-k-dragging"));
  assertEq(announced(), "Picked up Card a. Position 1 of 3 in To do.");
  press(a, "ArrowDown");
  assertEq(announced(), "Card a: Position 2 of 3 in To do.");
  press(a, "ArrowDown");
  press(a, "ArrowDown");
  assertEq(announced(), "Card a: Position 3 of 3 in To do.", "the last position is the end");
  assertDeepEq(b.titles("todo"), ["Card a", "Card b", "Card c"], "nothing moves before the drop");
  press(a, "Enter");
  assertDeepEq(b.moves, [{ item: "a", fromList: "todo", fromIndex: 0, toList: "todo", toIndex: 2 }]);
  assertDeepEq(b.titles("todo"), ["Card b", "Card c", "Card a"]);
  assert(!a.hasAttribute("data-k-dragging"));
  assertEq(document.activeElement.textContent, "Card a", "the moved row keeps the focus");
  assertEq(announced(), "Dropped Card a. Position 3 of 3 in To do.");
});

test("ArrowRight carries a row to the next list, at its position or that list's end", () => {
  const b = board();
  const [, , c] = b.rows("todo");
  press(c, "Enter");
  press(c, "ArrowRight");
  assertEq(announced(), "Card c: Position 2 of 2 in Done.", "index 2 is past the end of Done: its end");
  assert(b.list("done").hasAttribute("data-k-drop-target"));
  assertEq(b.list("done").style.getPropertyValue("outline"), "2px solid var(--k-color-drop-target, #93C5FD)", "the dropTarget token");
  assert(!b.list("todo").hasAttribute("data-k-drop-target"));
  assertEq(b.list("todo").style.getPropertyValue("outline"), "");
  press(c, "ArrowUp");
  press(c, " ");
  assertDeepEq(b.moves, [{ item: "c", fromList: "todo", fromIndex: 2, toList: "done", toIndex: 0 }]);
  assertDeepEq(b.titles("done"), ["Card c", "Card d"]);
  assertDeepEq(b.titles("todo"), ["Card a", "Card b"]);
  assertEq(document.activeElement.textContent, "Card c", "the row in its new list has the focus");
});

test("ArrowLeft and ArrowRight wrap around, and skip a list whose rule rejects the item", () => {
  const b = board({ todo: ["a"], done: ["d"], later: [] }, { done: { accepts: () => (t) => t.id !== "a" } });
  const [a] = b.rows("todo");
  press(a, " ");
  press(a, "ArrowRight");
  assertEq(announced(), "Card a: Position 1 of 1 in later.", "Done rejects Card a");
  press(a, "ArrowRight");
  assertEq(announced(), "Card a: Position 1 of 1 in To do.", "back to its own list");
  press(a, "ArrowLeft");
  assertEq(announced(), "Card a: Position 1 of 1 in later.");
});

test("Escape cancels: nothing fires and the row is where it was", () => {
  const b = board();
  const [a] = b.rows("todo");
  press(a, " ");
  press(a, "ArrowRight");
  press(a, "Escape");
  assertDeepEq(b.moves, []);
  assertEq(announced(), "Cancelled. Card a is back where it was.");
  assert(!b.list("done").hasAttribute("data-k-drop-target"));
  assert(!a.hasAttribute("data-k-dragging"));
  assertEq(document.querySelector?.(".k-drop-indicator") ?? null, null);
});

test("leaving the row with the focus cancels a keyboard drag", () => {
  const b = board();
  const [a, bb] = b.rows("todo");
  a.focus();
  press(a, " ");
  bb.focus();
  assertEq(announced(), "Cancelled. Card a is back where it was.");
  press(a, "ArrowDown");
  assertDeepEq(b.moves, []);
});

test("dropping a row where it was fires nothing", () => {
  const b = board();
  const [a] = b.rows("todo");
  press(a, " ");
  press(a, "ArrowDown");
  press(a, "ArrowUp");
  press(a, " ");
  assertDeepEq(b.moves, []);
  assertEq(announced(), "Dropped Card a where it was.");
});

test("while carrying, an indicator marks where the item would land", () => {
  const b = board();
  const [a, bb, c] = b.rows("todo");
  press(a, " ");
  press(a, "ArrowDown");
  const ul = b.list("todo");
  const nodes = [...ul.childNodes].filter((n) => n.nodeType === 1);
  const at = nodes.findIndex((n) => n.getAttribute("class") === "k-drop-indicator");
  assert(at >= 0, "an indicator");
  assertEq(nodes[at].getAttribute("aria-hidden"), "true");
  assertDeepEq(nodes.filter((n) => n.$kr), [a, bb, c]);
  assertEq(nodes[at - 1], bb, "after Card b: Card a would be second");
  press(a, "Escape");
  assert(![...ul.childNodes].some((n) => n.nodeType === 1 && n.getAttribute("class") === "k-drop-indicator"), "gone after the drag");
});

test("keys from a control inside a row are the control's", () => {
  const b = board({ todo: ["a"] });
  const [a] = b.rows("todo");
  const button = document.createElement("button");
  a.appendChild(button);
  const e = press(button, " ");
  assert(!e.defaultPrevented);
  assert(!a.hasAttribute("data-k-dragging"));
});

test("a draggable goes to the drop targets of its kind that accept it, and fires dropped", () => {
  const dropped = [];
  const el = mount(() => Draggable({ kind: () => card, payload: () => ({ id: 7, locked: false }), label: () => "Card 7", content: say("7") }));
  const target = (label, extra) => mount(() => DropTarget({ kind: () => card, label: () => label, content: say(label), on$dropped: (p) => dropped.push([label, p.id]), ...extra }));
  target("Locked only", { accepts: () => (p) => p.locked });
  mount(() => DropTarget({ kind: () => other, label: () => "Columns", content: say("c"), on$dropped: () => dropped.push("wrong kind") }));
  const trash = target("Trash", {});
  press(el, "Enter");
  assertEq(announced(), "Picked up Card 7. ");
  press(el, "ArrowDown");
  assertEq(announced(), "Card 7: Over Trash.");
  assert(trash.hasAttribute("data-k-drop-target"));
  assertEq(trash.getAttribute("role"), "group");
  assertEq(trash.getAttribute("aria-label"), "Trash");
  press(el, "Enter");
  assertDeepEq(dropped, [["Trash", 7]]);
  assertEq(announced(), "Dropped Card 7 on Trash.");
});

test("a row can be dropped on a drop target of its kind; a list takes no draggable", () => {
  const b = board({ todo: ["a"], done: [] });
  const dropped = [];
  const trash = mount(() => DropTarget({ kind: () => card, label: () => "Trash", content: say("Trash"), on$dropped: (p) => dropped.push(p.id) }));
  const [a] = b.rows("todo");
  press(a, " ");
  press(a, "ArrowRight");
  press(a, "ArrowRight");
  assert(trash.hasAttribute("data-k-drop-target"));
  press(a, " ");
  assertDeepEq(dropped, ["a"]);
  assertDeepEq(b.moves, []);
  const free = mount(() => Draggable({ kind: () => card, payload: () => ({ id: "x" }), label: () => "X", content: say("x") }));
  press(free, " ");
  press(free, "ArrowRight");
  assertEq(announced(), "X: Over Trash.", "lists aren't targets of a draggable");
});

// ---------------------------------------------------------------- pointer

test("a mouse drag starts after 4px, follows with a copy, and drops on the row it is over", () => {
  const b = board();
  const [a] = b.rows("todo");
  const [d] = b.rows("done");
  fire(a, "pointerdown", { pointerType: "mouse", button: 0, clientX: 10, clientY: 10 });
  fire(a, "pointermove", { pointerType: "mouse", clientX: 12, clientY: 11 });
  assert(!a.hasAttribute("data-k-dragging"), "3px is still a click");
  fire(d, "pointermove", { pointerType: "mouse", clientX: 200, clientY: 40 });
  assert(a.hasAttribute("data-k-dragging"));
  const copies = [...document.body.childNodes].filter((n) => n.nodeType === 1 && n.getAttribute("aria-hidden") === "true" && n.$kr === undefined && n.textContent === "Card a");
  assertEq(copies.length, 1, "a copy follows the pointer");
  assertEq(copies[0].style.getPropertyValue("pointer-events"), "none");
  assertEq(copies[0].style.getPropertyValue("transform"), "translate(190px, 30px)");
  assert(b.list("done").hasAttribute("data-k-drop-target"));
  fire(d, "pointerup", { pointerType: "mouse", clientX: 200, clientY: 40 });
  assertDeepEq(b.moves, [{ item: "a", fromList: "todo", fromIndex: 0, toList: "done", toIndex: 0 }]);
  assertEq(copies[0].parentNode, null, "the copy is gone");
  const click = fire(d, "click");
  assert(click.defaultPrevented, "the click that ends a drag is swallowed");
  assert(!fire(d, "click").defaultPrevented, "only that one");
});

test("the pointer finds the region under it by hit testing, and an empty spot of a list is its end", () => {
  const b = board();
  const ul = b.list("done");
  ul.rect = { left: 300, top: 0, width: 100, height: 400 };
  const [a] = b.rows("todo");
  fire(a, "pointerdown", { pointerType: "pen", button: 0, clientX: 0, clientY: 0 });
  fire(document.body, "pointermove", { pointerType: "pen", clientX: 350, clientY: 300 });
  fire(document.body, "pointerup", { pointerType: "pen", clientX: 350, clientY: 300 });
  assertDeepEq(b.moves, [{ item: "a", fromList: "todo", fromIndex: 0, toList: "done", toIndex: 1 }]);
});

test("dropping a mouse drag outside every region cancels it, as does Escape", () => {
  const b = board();
  const [a] = b.rows("todo");
  fire(a, "pointerdown", { pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
  fire(document.body, "pointermove", { pointerType: "mouse", clientX: 50, clientY: 50 });
  fire(document.body, "pointerup", { pointerType: "mouse", clientX: 50, clientY: 50 });
  assertDeepEq(b.moves, []);
  assert(!a.hasAttribute("data-k-dragging"));
  fire(a, "pointerdown", { pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
  fire(b.rows("done")[0], "pointermove", { pointerType: "mouse", clientX: 50, clientY: 50 });
  press(document.body, "Escape");
  fire(b.rows("done")[0], "pointerup", { pointerType: "mouse", clientX: 50, clientY: 50 });
  assertDeepEq(b.moves, [], "Escape cancelled it");
});

test("a pointer pressed on a control inside a row, or with another button, never drags", () => {
  const b = board({ todo: ["a"], done: ["d"] });
  const [a] = b.rows("todo");
  const button = document.createElement("button");
  a.appendChild(button);
  fire(button, "pointerdown", { pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
  fire(b.rows("done")[0], "pointermove", { pointerType: "mouse", clientX: 90, clientY: 90 });
  fire(a, "pointerdown", { pointerType: "mouse", button: 2, clientX: 0, clientY: 0 });
  fire(b.rows("done")[0], "pointermove", { pointerType: "mouse", clientX: 90, clientY: 90 });
  assert(!a.hasAttribute("data-k-dragging"));
});

test("touch: a swipe scrolls, a press of 250ms drags and stops the page from scrolling", () => {
  const timers = [];
  const saved = globalThis.setTimeout;
  globalThis.setTimeout = (f, ms) => timers.push([f, ms]);
  try {
    const b = board();
    const [a] = b.rows("todo");
    const [d] = b.rows("done");
    fire(a, "pointerdown", { pointerType: "touch", button: 0, clientX: 0, clientY: 0 });
    fire(a, "pointermove", { pointerType: "touch", clientX: 0, clientY: 30 });
    timers[0][0]();
    assert(!a.hasAttribute("data-k-dragging"), "moving first is a scroll");
    assertEq(timers[0][1], 250);
    fire(a, "pointerdown", { pointerType: "touch", button: 0, clientX: 0, clientY: 0 });
    fire(a, "pointermove", { pointerType: "touch", clientX: 3, clientY: 3 });
    assert(!a.hasAttribute("data-k-dragging"), "not before the press is long enough");
    timers[1][0]();
    assert(a.hasAttribute("data-k-dragging"), "a long press starts the drag");
    assert(fire(d, "touchmove").defaultPrevented, "the page doesn't scroll under the drag");
    fire(d, "pointermove", { pointerType: "touch", clientX: 200, clientY: 40 });
    fire(d, "pointerup", { pointerType: "touch", clientX: 200, clientY: 40 });
    assertDeepEq(b.moves, [{ item: "a", fromList: "todo", fromIndex: 0, toList: "done", toIndex: 0 }]);
    assert(!fire(d, "touchmove").defaultPrevented, "scrolling works again");
  } finally {
    globalThis.setTimeout = saved;
  }
});

test("a pointer cancelled by the browser cancels the drag", () => {
  const b = board();
  const [a] = b.rows("todo");
  fire(a, "pointerdown", { pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
  fire(b.rows("done")[0], "pointermove", { pointerType: "mouse", clientX: 90, clientY: 90 });
  fire(document.body, "pointercancel", { pointerType: "mouse" });
  assert(!a.hasAttribute("data-k-dragging"));
  fire(b.rows("done")[0], "pointerup", { pointerType: "mouse", clientX: 90, clientY: 90 });
  assertDeepEq(b.moves, []);
});

// -------------------------------------------------------------- hydration

test("a hydrating list claims the server's elements and its rows' content", () => {
  const container = document.createElement("div");
  container.innerHTML = '<ul aria-label="To do"><!--[--><li tabindex="0"><!--[-->Card a<!--]--></li><li tabindex="0"><!--[-->Card b<!--]--></li><!--]--></ul>';
  document.body.appendChild(container);
  const ul = container.firstChild;
  const [la, lb] = [...ul.childNodes].filter((n) => n.nodeType === 1);
  const text = la.childNodes[1];
  const items = signal([{ id: "a", title: "Card a" }, { id: "b", title: "Card b" }]);
  hydrate(container, () => ReorderableList({ kind: () => card, id: () => "todo", items: () => items.get(), key: () => (t) => t.id, label: () => "To do", row: (t) => textNode(t.title) }), null);
  assertEq(container.firstChild, ul, "the list is the server's");
  const rows = [...ul.childNodes].filter((n) => n.nodeType === 1);
  assertDeepEq(rows, [la, lb], "and so are its rows");
  assertEq(la.childNodes[1], text, "and the rows' content");
  assertEq(la.getAttribute("aria-describedby"), "k-drag-help");
  press(la, " ");
  press(la, "ArrowDown");
  assertEq(announced(), "a: Position 2 of 2 in To do.", "a hydrated row drags; without itemLabel it is named by its key");
});

test("a hydrating draggable claims the server's element and its content", () => {
  const container = document.createElement("div");
  container.innerHTML = '<div role="group" aria-label="Card 7"><!--[-->Seven<!--]--></div>';
  document.body.appendChild(container);
  const server = container.firstChild;
  const content = server.childNodes[1];
  hydrate(container, () => Draggable({ kind: () => card, payload: () => 7, label: () => "Card 7", content: () => textNode("Seven") }), null);
  assertEq(container.firstChild, server);
  assertEq(server.childNodes[1], content);
  assertEq(server.getAttribute("tabindex"), "0");
});

test("a fresh document (the next test of `k test`) gets its own listeners and live region", () => {
  const first = board({ todo: ["a"], done: [] });
  fire(first.rows("todo")[0], "pointerdown", { pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
  fire(first.list("done"), "pointermove", { pointerType: "mouse", clientX: 20, clientY: 20 });
  fire(first.list("done"), "pointerup", { pointerType: "mouse", clientX: 20, clientY: 20 });
  assertEq(first.moves.length, 1);
  installDom();
  const second = board({ todo: ["b"], done: [] });
  fire(second.rows("todo")[0], "pointerdown", { pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
  fire(second.list("done"), "pointermove", { pointerType: "mouse", clientX: 20, clientY: 20 });
  fire(second.list("done"), "pointerup", { pointerType: "mouse", clientX: 20, clientY: 20 });
  assertDeepEq(second.moves, [{ item: "b", fromList: "todo", fromIndex: 0, toList: "done", toIndex: 0 }]);
  press(second.rows("done")[0], " ");
  assertEq(announced(), "Picked up Card b. Position 1 of 1 in Done.", "announced in the new document");
});
