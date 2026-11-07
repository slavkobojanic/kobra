// The fake DOM (k:testing/dom) and the DOM renderer's binding helpers
// (docs/compiler/runtime.md, "DOM renderer").
import { assert, assertEq, assertThrows, installDom, test } from "k:testing";
import { attr, batch, classList, each, on, prop, renderEffect, root, signal, style, template, text, when } from "k:runtime";

test("the fake DOM parses and serializes compiler-style HTML", () => {
  const doc = installDom();
  const t = doc.createElement("template");
  t.innerHTML = '<div class="a b"><span> </span><!--[--><button type="button">Add &amp; go</button><br><input value="x"></div>';
  assertEq(t.innerHTML, '<div class="a b"><span> </span><!--[--><button type="button">Add &amp; go</button><br><input value="x"></div>');
  assertEq(t.content.firstChild.childNodes.length, 5);
  assertEq(t.content.firstChild.childNodes[1].data, "[");
});

test("the fake DOM rejects malformed HTML", () => {
  const doc = installDom();
  const t = doc.createElement("template");
  assertThrows(() => { t.innerHTML = "<div><span></div>"; }, "unexpected");
  assertThrows(() => { t.innerHTML = "<div>"; }, "unclosed");
  assertThrows(() => { t.innerHTML = "<div a=b></div>"; }, "unquoted");
});

test("the fake DOM keeps the tree consistent on insert, move and remove", () => {
  const doc = installDom();
  const a = doc.createElement("box");
  const b = doc.createElement("b");
  const c = doc.createElement("c");
  a.appendChild(b);
  a.insertBefore(c, b);
  assertEq(a.firstChild, c);
  assertEq(c.nextSibling, b);
  assertEq(b.previousSibling, c);
  doc.body.appendChild(b);
  assertEq(a.childNodes.length, 1);
  assertEq(b.parentNode, doc.body);
  b.remove();
  assertEq(b.parentNode, null);
});

test("events bubble to ancestors and stopPropagation stops them", () => {
  const doc = installDom();
  const outer = doc.createElement("div");
  const inner = doc.createElement("button");
  outer.appendChild(inner);
  const log = [];
  outer.addEventListener("click", () => log.push("outer"));
  inner.addEventListener("click", (e) => { log.push("inner"); if (log.length > 2) e.stopPropagation(); });
  inner.click();
  inner.click();
  assertEq(log.join(","), "inner,outer,inner");
});

test("getElementById finds the first element in tree order", () => {
  const doc = installDom();
  const s = doc.createElement("script");
  s.setAttribute("id", "k-data");
  doc.body.appendChild(s);
  assertEq(doc.getElementById("k-data"), s);
  assertEq(doc.getElementById("missing"), null);
});

test("template clones a fresh copy each time", () => {
  installDom();
  const make = template("<div><span> </span></div>");
  const a = make();
  const b = make();
  assert(a !== b);
  a.firstChild.firstChild.data = "changed";
  assertEq(b.firstChild.firstChild.data, " ");
});

test("text writes only changed values", () => {
  const doc = installDom();
  const node = doc.createTextNode("a");
  let writes = 0;
  let data = "a";
  Object.defineProperty(node, "data", { get: () => data, set: (v) => { writes++; data = v; } });
  text(node, "a");
  text(node, "b");
  assertEq(writes, 1);
});

test("attr sets, clears and sets boolean attributes", () => {
  const doc = installDom();
  const el = doc.createElement("button");
  attr(el, "aria-pressed", "true");
  assertEq(el.getAttribute("aria-pressed"), "true");
  attr(el, "disabled", true);
  assertEq(el.outerHTML, '<button aria-pressed="true" disabled></button>');
  attr(el, "disabled", false);
  attr(el, "aria-pressed", null);
  assertEq(el.outerHTML, "<button></button>");
});

test("prop and style write properties and custom properties", () => {
  const doc = installDom();
  const el = doc.createElement("input");
  prop(el, "value", "hi");
  assertEq(el.value, "hi");
  style(el, "--k-accent", "#fff");
  assertEq(el.style.getPropertyValue("--k-accent"), "#fff");
  style(el, "--k-accent", null);
  assertEq(el.style.getPropertyValue("--k-accent"), "");
});

test("classList combines static and conditional classes", () => {
  const doc = installDom();
  const el = doc.createElement("div");
  classList(el, "panel_h1", [false, "expanded_h1", null]);
  assertEq(el.getAttribute("class"), "panel_h1 expanded_h1");
  classList(el, "", ["only_h1"]);
  assertEq(el.getAttribute("class"), "only_h1");
});

test("a binding effect keeps a text node in sync with a signal", () => {
  installDom();
  const count = signal(1);
  const root$ = template("<div><span> </span><button>Add</button></div>")();
  const label = root$.firstChild.firstChild;
  root(() => {
    renderEffect(() => text(label, `Count: ${count.get()}`));
    on(root$.lastChild, "click", () => count.set(count.peek() + 1));
  });
  assertEq(root$.outerHTML, "<div><span>Count: 1</span><button>Add</button></div>");
  root$.lastChild.click();
  assertEq(label.data, "Count: 2", "on() batches, so the DOM is current after the handler");
});

test("an event handler's writes flush once", () => {
  installDom();
  const doc = globalThis.document;
  const a = signal(0);
  let runs = 0;
  const button = doc.createElement("button");
  root(() => {
    renderEffect(() => { a.get(); runs++; });
    on(button, "click", () => { a.set(1); a.set(2); a.set(3); });
  });
  button.click();
  assertEq(runs, 2);
  assertEq(batch(() => a.peek()), 3);
});

test("when renders and removes a branch between markers", () => {
  const doc = installDom();
  const host = doc.createElement("div");
  const start = doc.createComment("[");
  const end = doc.createComment("]");
  host.appendChild(start);
  host.appendChild(end);
  const show = signal(true);
  root(() => when(start, end, () => show.get(), () => doc.createTextNode("yes"), null));
  assertEq(host.textContent, "yes");
  batch(() => show.set(false));
  assertEq(host.textContent, "");
});

test("each reorders keyed rows by moving nodes", () => {
  const doc = installDom();
  const host = doc.createElement("list");
  const start = doc.createComment("[");
  const end = doc.createComment("]");
  host.appendChild(start);
  host.appendChild(end);
  const items = signal([1, 2, 3]);
  root(() => each(start, end, () => items.get(), (i) => i, (item) => {
    const li = doc.createElement("item");
    li.textContent = String(item());
    return li;
  }));
  const first = host.childNodes[1];
  batch(() => items.set([3, 2, 1]));
  assertEq(host.textContent, "321");
  assertEq(host.childNodes[3], first, "row 1 was moved, not recreated");
});
