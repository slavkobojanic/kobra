/**
 * @module k:/tests/ui-blocks.test.js
 *
 * The control-flow blocks of views: `when` (`if`, `if let`), `branch`
 * (`match`) and `each` (`for … key`) with keyed reconciliation
 * (docs/compiler/runtime.md §5.3, docs/compiler/codegen.md §11.1). A block
 * owns the nodes between two comment markers and a child scope per branch
 * or row. All of these are test-first until the blocks land.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { batch, branch, each, onCleanup, renderEffect, root, signal, text, when } from "k:runtime";

const BLOCKS = "blocks (runtime.md §5.3)";
const KEYED = "keyed lists (runtime.md §5.3)";

/**
 * A host element holding `before`, the block's markers and `after`, so tests
 * can check that a block never touches nodes outside its range.
 */
function range() {
  const doc = globalThis.document;
  const host = doc.createElement("div");
  const before = doc.createTextNode("<");
  const start = doc.createComment("[");
  const end = doc.createComment("]");
  const after = doc.createTextNode(">");
  host.appendChild(before);
  host.appendChild(start);
  host.appendChild(end);
  host.appendChild(after);
  doc.body.appendChild(host);
  return { doc, host, start, end, before, after };
}

/** The nodes strictly between the markers. */
function inside(r) {
  const nodes = [];
  for (let n = r.start.nextSibling; n !== null && n !== r.end; n = n.nextSibling) nodes.push(n);
  return nodes;
}

/** The text between the markers. */
function content(r) {
  return inside(r).map((n) => n.textContent).join("");
}

/** An element with the given text. */
function el(tag, value) {
  const e = globalThis.document.createElement(tag);
  e.textContent = value;
  return e;
}

// ---------------------------------------------------------------- when

// `if` without `else`: renders while true, renders nothing while false.
test("when renders its branch while the condition is true", () => {
  const r = range();
  const show = signal(true);
  root(() => when(r.start, r.end, () => show.get(), () => el("p", "yes"), null));
  assertEq(content(r), "yes");
  batch(() => show.set(false));
  assertEq(content(r), "");
  assertEq(inside(r).length, 0);
  batch(() => show.set(true));
  assertEq(content(r), "yes");
});

// `if`/`else`: the `otherwise` branch renders while the condition is false.
test("when renders otherwise while the condition is false", () => {
  const r = range();
  const show = signal(false);
  root(() => when(r.start, r.end, () => show.get(), () => el("p", "yes"), () => el("p", "no")));
  assertEq(content(r), "no");
  batch(() => show.set(true));
  assertEq(content(r), "yes");
  batch(() => show.set(false));
  assertEq(content(r), "no");
});

// The nodes outside the markers are never touched.
test("when keeps the nodes outside its markers", () => {
  const r = range();
  const show = signal(true);
  root(() => when(r.start, r.end, () => show.get(), () => el("p", "yes"), () => el("p", "no")));
  batch(() => show.set(false));
  assertEq(r.host.firstChild, r.before);
  assertEq(r.host.lastChild, r.after);
  assertEq(r.host.textContent, "<no>");
});

// A condition that changes value but not truthiness doesn't re-render: the
// branch's nodes are the same objects.
test("when doesn't re-render while the condition stays true", () => {
  const r = range();
  const count = signal(1);
  let renders = 0;
  root(() => when(r.start, r.end, () => count.get() > 0, () => { renders++; return el("p", "some"); }, null));
  const first = inside(r)[0];
  batch(() => count.set(2));
  batch(() => count.set(3));
  assertEq(renders, 1);
  assertEq(inside(r)[0], first);
});

// Switching branches disposes the old branch's scope: its effects stop and
// its cleanups run.
test("switching branches disposes the old branch", () => {
  const r = range();
  const show = signal(true);
  const label = signal("a");
  const log = [];
  root(() => when(r.start, r.end, () => show.get(), () => {
    const p = el("p", " ");
    renderEffect(() => { log.push(`bind ${label.get()}`); text(p.firstChild, label.get()); });
    onCleanup(() => log.push("cleanup"));
    return p;
  }, null));
  batch(() => show.set(false));
  batch(() => label.set("b"));
  assertDeepEq(log, ["bind a", "cleanup"]);
});

// The `if let` lowering: the condition tests `!== null` and the branch reads
// the value through its own binding, so a change between two non-nil values
// updates the text in place.
test("an if-let branch updates in place between non-nil values", () => {
  const r = range();
  const nickname = signal("Ada");
  let renders = 0;
  root(() => when(r.start, r.end, () => nickname.get() !== null, () => {
    renders++;
    const span = el("span", " ");
    renderEffect(() => text(span.firstChild, nickname.get() ?? ""));
    return span;
  }, null));
  batch(() => nickname.set("Grace"));
  assertEq(content(r), "Grace");
  assertEq(renders, 1);
  batch(() => nickname.set(null));
  assertEq(content(r), "");
});

// A branch may render several nodes (a fragment); all of them are removed on
// switch.
test("a branch may render a fragment of several nodes", () => {
  const r = range();
  const show = signal(true);
  root(() => when(r.start, r.end, () => show.get(), () => {
    const f = globalThis.document.createDocumentFragment();
    f.appendChild(el("dt", "K"));
    f.appendChild(el("dd", "a language"));
    return f;
  }, null));
  assertEq(inside(r).length, 2);
  batch(() => show.set(false));
  assertEq(inside(r).length, 0);
});

// Nested blocks: disposing the outer branch disposes the inner one too.
test("disposing an outer branch disposes a nested block", () => {
  const r = range();
  const outer = signal(true);
  const inner = signal(true);
  const log = [];
  root(() => when(r.start, r.end, () => outer.get(), () => {
    const box = globalThis.document.createElement("div");
    const s = globalThis.document.createComment("[");
    const e = globalThis.document.createComment("]");
    box.appendChild(s);
    box.appendChild(e);
    when(s, e, () => inner.get(), () => { onCleanup(() => log.push("inner cleanup")); return el("p", "in"); }, null);
    return box;
  }, null));
  assertEq(content(r), "in");
  batch(() => outer.set(false));
  assertDeepEq(log, ["inner cleanup"]);
  batch(() => inner.set(false));
  assertDeepEq(log, ["inner cleanup"], "the inner block no longer reacts");
});

// Disposing the owner of the block removes nothing from the DOM by itself
// but stops every reaction (unmount removes the component's range).
test("a disposed when no longer reacts", () => {
  const r = range();
  const show = signal(true);
  let dispose = null;
  root((d) => { dispose = d; when(r.start, r.end, () => show.get(), () => el("p", "yes"), () => el("p", "no")); });
  dispose();
  batch(() => show.set(false));
  assertEq(content(r), "yes");
});

// ---------------------------------------------------------------- branch

// `match`: the arm the selector returns renders.
test("branch renders the selected arm", () => {
  const r = range();
  const tag = signal(0);
  root(() => branch(r.start, r.end, () => tag.get(), [() => el("p", "idle"), () => el("p", "loaded"), () => el("p", "failed")]));
  assertEq(content(r), "idle");
  batch(() => tag.set(2));
  assertEq(content(r), "failed");
  batch(() => tag.set(1));
  assertEq(content(r), "loaded");
});

// Two cases sharing an arm (`case .idle, .loading`) select the same index, so
// moving between them doesn't re-render.
test("branch doesn't re-render when the index stays the same", () => {
  const r = range();
  const state = signal({ $: 0 });
  const arm = (s) => (s.$ <= 1 ? 0 : 1);
  let renders = 0;
  root(() => branch(r.start, r.end, () => arm(state.get()), [() => { renders++; return el("p", "…"); }, () => el("p", "done")]));
  const first = inside(r)[0];
  batch(() => state.set({ $: 1 }));
  assertEq(renders, 1);
  assertEq(inside(r)[0], first);
});

// A payload change within the same case updates the arm's bindings in place.
test("a payload change in the same arm updates in place", () => {
  const r = range();
  const state = signal({ $: 1, _0: "Ada" });
  let renders = 0;
  root(() => branch(r.start, r.end, () => (state.get().$ === 1 ? 1 : 0), [
    () => el("p", "idle"),
    () => {
      renders++;
      const h = el("h2", " ");
      renderEffect(() => { const s = state.get(); if (s.$ === 1) text(h.firstChild, s._0); });
      return h;
    },
  ]));
  batch(() => state.set({ $: 1, _0: "Grace" }));
  assertEq(content(r), "Grace");
  assertEq(renders, 1);
});

// Switching arms disposes the old arm's scope.
test("switching arms disposes the old arm", () => {
  const r = range();
  const tag = signal(0);
  const log = [];
  root(() => branch(r.start, r.end, () => tag.get(), [
    () => { onCleanup(() => log.push("zero")); return el("p", "0"); },
    () => { onCleanup(() => log.push("one")); return el("p", "1"); },
  ]));
  batch(() => tag.set(1));
  batch(() => tag.set(0));
  assertDeepEq(log, ["zero", "one"]);
});

// ---------------------------------------------------------------- each

/**
 * Renders `each` over `items` with rows `<li>key</li>` that record their
 * creation, and returns helpers for the assertions.
 */
function keyed(initial, key = (x) => x) {
  const r = range();
  const items = signal(initial);
  const created = [];
  const disposed = [];
  root(() => each(r.start, r.end, () => items.get(), key, (item, index) => {
    const li = el("li", " ");
    created.push(key(item()));
    renderEffect(() => text(li.firstChild, String(item().label ?? item())));
    onCleanup(() => disposed.push(key(item())));
    return li;
  }));
  const rows = () => inside(r);
  const texts = () => rows().map((n) => n.textContent);
  return { r, items, created, disposed, rows, texts };
}

// The rows render in list order.
test("each renders one row per item in order", () => {
  const t = keyed(["a", "b", "c"]);
  assertDeepEq(t.texts(), ["a", "b", "c"]);
  assertDeepEq(t.created, ["a", "b", "c"]);
});

// An empty list renders nothing; going to and from empty works.
test("each handles the empty list both ways", () => {
  const t = keyed([]);
  assertEq(t.rows().length, 0);
  batch(() => t.items.set(["a", "b"]));
  assertDeepEq(t.texts(), ["a", "b"]);
  batch(() => t.items.set([]));
  assertEq(t.rows().length, 0);
  assertDeepEq(t.disposed.sort(), ["a", "b"]);
  assertEq(t.r.host.textContent, "<>", "the markers' neighbours stay");
});

// Appending creates only the new row; existing rows keep their nodes.
test("appending creates only the new row", () => {
  const t = keyed(["a", "b"]);
  const [a, b] = t.rows();
  batch(() => t.items.set(["a", "b", "c"]));
  assertDeepEq(t.texts(), ["a", "b", "c"]);
  assertDeepEq(t.created, ["a", "b", "c"]);
  assertEq(t.rows()[0], a);
  assertEq(t.rows()[1], b);
});

// Prepending inserts before the first row.
test("prepending inserts before the first row", () => {
  const t = keyed(["b", "c"]);
  const [b, c] = t.rows();
  batch(() => t.items.set(["a", "b", "c"]));
  assertDeepEq(t.texts(), ["a", "b", "c"]);
  assertEq(t.rows()[1], b);
  assertEq(t.rows()[2], c);
});

// Inserting in the middle keeps the neighbours.
test("inserting in the middle keeps both neighbours", () => {
  const t = keyed(["a", "c"]);
  const [a, c] = t.rows();
  batch(() => t.items.set(["a", "b", "c"]));
  assertDeepEq(t.texts(), ["a", "b", "c"]);
  assertEq(t.rows()[0], a);
  assertEq(t.rows()[2], c);
  assertDeepEq(t.created, ["a", "c", "b"]);
});

// Removing a row removes its node and disposes its scope.
test("removing a row disposes it", () => {
  const t = keyed(["a", "b", "c"]);
  const [a, , c] = t.rows();
  batch(() => t.items.set(["a", "c"]));
  assertDeepEq(t.texts(), ["a", "c"]);
  assertDeepEq(t.disposed, ["b"]);
  assertEq(t.rows()[0], a);
  assertEq(t.rows()[1], c);
});

// Reversing moves the existing nodes: nothing is re-created.
test("reversing moves rows instead of re-creating them", () => {
  const t = keyed(["a", "b", "c", "d"]);
  const before = t.rows();
  batch(() => t.items.set(["d", "c", "b", "a"]));
  assertDeepEq(t.texts(), ["d", "c", "b", "a"]);
  assertDeepEq(t.rows(), [...before].reverse());
  assertEq(t.created.length, 4);
  assertEq(t.disposed.length, 0);
});

/** Counts DOM moves and insertions under the block's host. */
function countInserts(host) {
  let count = 0;
  const original = host.insertBefore.bind(host);
  host.insertBefore = (node, ref) => { count++; return original(node, ref); };
  // appendChild goes through insertBefore in the fake DOM, so this counts both.
  return () => count;
}

// Moving the last row to the front is one move: the longest increasing
// subsequence keeps the other rows in place.
test("moving one row to the front moves exactly one node", () => {
  const t = keyed([1, 2, 3, 4, 5]);
  const moves = countInserts(t.r.host);
  batch(() => t.items.set([5, 1, 2, 3, 4]));
  assertDeepEq(t.texts(), ["5", "1", "2", "3", "4"]);
  assertEq(moves(), 1);
});

// Swapping two rows moves at most two nodes.
test("swapping two rows moves at most two nodes", () => {
  const t = keyed([1, 2, 3, 4, 5]);
  const moves = countInserts(t.r.host);
  batch(() => t.items.set([1, 4, 3, 2, 5]));
  assertDeepEq(t.texts(), ["1", "4", "3", "2", "5"]);
  assert(moves() <= 2, `moved ${moves()} nodes`);
});

// An unchanged list moves nothing.
test("an equal list with a new identity moves nothing", () => {
  const t = keyed([1, 2, 3]);
  const moves = countInserts(t.r.host);
  batch(() => t.items.set([1, 2, 3]));
  assertEq(moves(), 0);
  assertEq(t.created.length, 3);
});

// Replacing every key re-creates every row and disposes the old ones.
test("replacing every key re-creates every row", () => {
  const t = keyed(["a", "b"]);
  batch(() => t.items.set(["x", "y"]));
  assertDeepEq(t.texts(), ["x", "y"]);
  assertDeepEq(t.disposed.sort(), ["a", "b"]);
  assertDeepEq(t.created, ["a", "b", "x", "y"]);
});

// A mixed edit: removal, insertion and a move in one update.
test("a mixed edit removes, inserts and moves in one update", () => {
  const t = keyed(["a", "b", "c", "d"]);
  const [a, , c, d] = t.rows();
  batch(() => t.items.set(["d", "a", "e", "c"]));
  assertDeepEq(t.texts(), ["d", "a", "e", "c"]);
  assertDeepEq(t.disposed, ["b"]);
  assertEq(t.rows()[0], d);
  assertEq(t.rows()[1], a);
  assertEq(t.rows()[3], c);
});

// An item that changes but keeps its key re-renders in place through the
// row's item signal: the row factory doesn't run again.
test("an updated item with the same key re-renders in place", () => {
  const t = keyed([{ id: 1, label: "milk" }, { id: 2, label: "eggs" }], (x) => x.id);
  const first = t.rows()[0];
  batch(() => t.items.set([{ id: 1, label: "oat milk" }, { id: 2, label: "eggs" }]));
  assertDeepEq(t.texts(), ["oat milk", "eggs"]);
  assertEq(t.rows()[0], first);
  assertDeepEq(t.created, [1, 2]);
});

// The row's index signal follows moves.
test("a row's index follows its moves", () => {
  const r = range();
  const items = signal(["a", "b", "c"]);
  const indexOf = new Map();
  root(() => each(r.start, r.end, () => items.get(), (x) => x, (item, index) => {
    const li = el("li", " ");
    renderEffect(() => indexOf.set(item(), index()));
    return li;
  }));
  batch(() => items.set(["c", "a", "b"]));
  assertDeepEq([...indexOf.entries()].sort(), [["a", 1], ["b", 2], ["c", 0]]);
});

// Rows may be fragments of several nodes; moves keep each row's nodes
// together.
test("multi-node rows move as a unit", () => {
  const r = range();
  const items = signal(["a", "b"]);
  root(() => each(r.start, r.end, () => items.get(), (x) => x, (item) => {
    const f = globalThis.document.createDocumentFragment();
    f.appendChild(el("dt", item()));
    f.appendChild(el("dd", item().toUpperCase()));
    return f;
  }));
  batch(() => items.set(["b", "a"]));
  assertEq(content(r), "bBaA");
});

// An `each` inside a `when` branch: hiding the branch disposes every row.
test("hiding a branch disposes the rows of a list inside it", () => {
  const r = range();
  const show = signal(true);
  const items = signal([1, 2]);
  const disposed = [];
  root(() => when(r.start, r.end, () => show.get(), () => {
    const ul = globalThis.document.createElement("ul");
    const s = globalThis.document.createComment("[");
    const e = globalThis.document.createComment("]");
    ul.appendChild(s);
    ul.appendChild(e);
    each(s, e, () => items.get(), (x) => x, (item) => { const v = item(); onCleanup(() => disposed.push(v)); return el("li", String(v)); });
    return ul;
  }, null));
  batch(() => show.set(false));
  assertDeepEq(disposed.sort(), [1, 2]);
});

// The list's nodes stay between the markers, after `before` and before
// `after`, whatever the edit.
test("each never touches nodes outside its markers", () => {
  const t = keyed(["a", "b", "c"]);
  batch(() => t.items.set(["c", "x"]));
  assertEq(t.r.host.firstChild, t.r.before);
  assertEq(t.r.host.lastChild, t.r.after);
  assertEq(t.r.host.textContent, "<cx>");
});
