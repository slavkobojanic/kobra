/**
 * @module k:/tests/ui-hydrate.test.js
 *
 * Hydration (docs/compiler/runtime.md §6, docs/compiler/codegen.md §11.3):
 * the client factories claim the server-rendered nodes instead of creating
 * them, text bindings and handlers attach to what is there, blocks find their
 * markers, `@Load` values come from `k-data` so nothing is fetched twice, and a
 * mismatch client-renders only the range that differs.
 *
 * The server HTML in these tests is what the `$ssr` renderers write
 * (`Out`, `OPEN`, `CLOSE`, `SEPARATOR`), parsed by the fake DOM.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import {
  READY, batch, createLoads, each, effect, hydrate, isHydrating, on, readData, renderEffect, signal, template, text, when,
} from "k:runtime";
import { CLOSE, OPEN, Out, SEPARATOR, serializeData } from "k:runtime/server";

const HYDRATE = "hydration (runtime.md §6)";

/** A container holding `html`, as the browser parsed the server's page. */
function page(html) {
  const container = globalThis.document.createElement("div");
  container.innerHTML = html;
  globalThis.document.body.appendChild(container);
  return container;
}

// The Counter of architecture.md §7.2: its server HTML and its client factory.
const counterTemplate = template("<div><span> </span><button>Add</button></div>");
/** The server HTML of the counter: a span with its text and a button. */
function counterHtml(label, count) {
  const out = new Out();
  out.raw("<div><span>");
  out.text(`${label}: ${count}`);
  out.raw("</span><button>Add</button></div>");
  return out.html();
}
/** The client counter: claims or creates its template and binds its text and click. */
function Counter(state) {
  const root$ = counterTemplate();
  const label = root$.firstChild.firstChild;
  renderEffect(() => text(label, `Count: ${state.count.get()}`));
  on(root$.lastChild, "click", () => state.count.set(state.count.peek() + 1));
  return root$;
}

// ---------------------------------------------------------------- claiming

// The server's root element is claimed: after hydration it is the same node,
// and nothing was added next to it.
test("hydrate claims the server's root element", () => {
  const container = page(counterHtml("Count", 1));
  const server = container.firstChild;
  const state = { count: signal(1) };
  hydrate(container, () => Counter(state), null);
  assertEq(container.firstChild, server);
  assertEq(container.childNodes.length, 1);
  assertEq(container.innerHTML, "<div><span>Count: 1</span><button>Add</button></div>");
});

// A text binding attaches to the server's text node, whatever text it holds,
// and updates that node later.
test("a text binding attaches to the server's text node", () => {
  const container = page(counterHtml("Count", 1));
  const serverText = container.firstChild.firstChild.firstChild;
  const state = { count: signal(1) };
  hydrate(container, () => Counter(state), null);
  batch(() => state.count.set(5));
  assertEq(container.firstChild.firstChild.firstChild, serverText);
  assertEq(serverText.data, "Count: 5");
});

// Handlers attach to the claimed elements.
test("handlers attach to claimed elements", () => {
  const container = page(counterHtml("Count", 1));
  const state = { count: signal(1) };
  hydrate(container, () => Counter(state), null);
  container.firstChild.lastChild.click();
  assertEq(state.count.peek(), 2);
  assertEq(container.firstChild.firstChild.textContent, "Count: 2");
});

// `isHydrating()` is true only while the factories run.
test("isHydrating is true only during the pass", () => {
  const container = page(counterHtml("Count", 1));
  let during = null;
  hydrate(container, () => { during = isHydrating(); return Counter({ count: signal(1) }); }, null);
  assertEq(during, true);
  assertEq(isHydrating(), false);
});

// After hydration, templates clone again: a component mounted later creates
// fresh nodes.
test("templates clone again after hydration", () => {
  const container = page(counterHtml("Count", 1));
  hydrate(container, () => Counter({ count: signal(1) }), null);
  const fresh = Counter({ count: signal(9) });
  assert(fresh !== container.firstChild);
  assertEq(fresh.parentNode, null);
  assertEq(fresh.firstChild.textContent, "Count: 9");
});

// User effects created during hydration run in the first flush after it,
// never while nodes are still being claimed.
test("user effects run after hydration completes", () => {
  const container = page(counterHtml("Count", 1));
  const seen = [];
  hydrate(container, () => {
    effect(() => seen.push(isHydrating()));
    return Counter({ count: signal(1) });
  }, null);
  batch(() => {});
  assertDeepEq(seen, [false]);
});

// Two adjacent dynamic texts are kept apart by `<!---->` and claimed one by
// one.
test("adjacent dynamic texts are claimed separately", () => {
  const out = new Out();
  out.raw("<p>");
  out.text("Ada");
  out.raw(SEPARATOR);
  out.text("Lovelace");
  out.raw("</p>");
  const container = page(out.html());
  const t = template("<p> <!----> </p>");
  const first = signal("Ada");
  const last = signal("Lovelace");
  hydrate(container, () => {
    const p = t();
    renderEffect(() => text(p.firstChild, first.get()));
    renderEffect(() => text(p.lastChild, last.get()));
    return p;
  }, null);
  batch(() => last.set("Byron"));
  assertEq(container.innerHTML, "<p>Ada<!---->Byron</p>");
});

// A dynamic text that was empty on the server has no text node in the parsed
// HTML; hydration creates one instead of binding the wrong node.
test("an empty server text still gets a binding", () => {
  const container = page("<p></p>");
  const t = template("<p> </p>");
  const value = signal("");
  hydrate(container, () => {
    const p = t();
    renderEffect(() => text(p.firstChild, value.get()));
    return p;
  }, null);
  batch(() => value.set("now"));
  assertEq(container.innerHTML, "<p>now</p>");
});

// Escaped server text is claimed as its decoded value.
test("escaped server text is claimed as its value", () => {
  const out = new Out();
  out.raw("<p>");
  out.text("1 < 2 & 3");
  out.raw("</p>");
  const container = page(out.html());
  const t = template("<p> </p>");
  let node = null;
  hydrate(container, () => { const p = t(); node = p.firstChild; return p; }, null);
  assertEq(node.data, "1 < 2 & 3");
});

// ---------------------------------------------------------------- blocks

// A `when` block finds its markers and claims the branch the server rendered.
test("a when block claims the server's branch", () => {
  const container = page(`<div>${OPEN}<p>yes</p>${CLOSE}</div>`);
  const serverP = container.firstChild.childNodes[1];
  const outer = template("<div><!--[--><!--]--></div>");
  const yes = template("<p>yes</p>");
  const show = signal(true);
  hydrate(container, () => {
    const div = outer();
    when(div.firstChild, div.lastChild, () => show.get(), () => yes(), null);
    return div;
  }, null);
  assertEq(container.firstChild.childNodes[1], serverP);
  assertEq(container.innerHTML, "<div><!--[--><p>yes</p><!--]--></div>");
  batch(() => show.set(false));
  assertEq(container.innerHTML, "<div><!--[--><!--]--></div>");
});

// An `each` block claims the server's rows in order and later moves them.
test("an each block claims the server's rows", () => {
  const container = page(`<ul>${OPEN}<li>a</li><li>b</li>${CLOSE}</ul>`);
  const [, a, b] = container.firstChild.childNodes;
  const outer = template("<ul><!--[--><!--]--></ul>");
  const row = template("<li> </li>");
  const items = signal(["a", "b"]);
  hydrate(container, () => {
    const ul = outer();
    each(ul.firstChild, ul.lastChild, () => items.get(), (x) => x, (item) => {
      const li = row();
      renderEffect(() => text(li.firstChild, item()));
      return li;
    });
    return ul;
  }, null);
  batch(() => items.set(["b", "a"]));
  const [, first, second] = container.firstChild.childNodes;
  assertEq(first, b);
  assertEq(second, a);
});

// A child component's range is claimed in place: the parent's insertion
// between the markers moves nothing and duplicates nothing.
test("a child component's range is claimed in place", () => {
  const container = page(`<div>${OPEN}<span>child</span>${CLOSE}</div>`);
  const outer = template("<div><!--[--><!--]--></div>");
  const child = template("<span>child</span>");
  hydrate(container, () => {
    const div = outer();
    const end = div.lastChild;
    end.parentNode.insertBefore(child(), end);
    return div;
  }, null);
  assertEq(container.innerHTML, "<div><!--[--><span>child</span><!--]--></div>");
});

// ---------------------------------------------------------------- mismatches

// When a block's server content doesn't match, only that range is
// client-rendered; the nodes around it stay the server's.
test("a mismatch inside a block client-renders that range", () => {
  const container = page(`<div><h1>Title</h1>${OPEN}<p>stale</p>${CLOSE}</div>`);
  const serverDiv = container.firstChild;
  const serverH1 = serverDiv.firstChild;
  const outer = template("<div><h1>Title</h1><!--[--><!--]--></div>");
  const fresh = template("<section>fresh</section>");
  hydrate(container, () => {
    const div = outer();
    when(div.firstChild.nextSibling, div.lastChild, () => true, () => fresh(), null);
    return div;
  }, null);
  assertEq(container.firstChild, serverDiv);
  assertEq(serverDiv.firstChild, serverH1);
  assertEq(container.innerHTML, "<div><h1>Title</h1><!--[--><section>fresh</section><!--]--></div>");
});

// A mismatch at the root replaces the root.
test("a mismatch at the root replaces it", () => {
  const container = page("<p>server</p>");
  hydrate(container, () => Counter({ count: signal(3) }), null);
  assertEq(container.childNodes.length, 1);
  assertEq(container.innerHTML, "<div><span>Count: 3</span><button>Add</button></div>");
});

// A mismatch in an attribute-only difference is not a mismatch: attributes
// bound by effects are rewritten, the node is kept.
test("a differing bound attribute keeps the node", () => {
  const container = page('<button aria-pressed="false">Bold</button>');
  const server = container.firstChild;
  const t = template("<button>Bold</button>");
  hydrate(container, () => {
    const b = t();
    renderEffect(() => b.setAttribute("aria-pressed", "true"));
    return b;
  }, null);
  assertEq(container.firstChild, server);
  assertEq(server.getAttribute("aria-pressed"), "true");
});

// With no server HTML (client rendering), hydrate renders into the empty
// container.
test("hydrate client-renders an empty container", () => {
  const container = page("");
  hydrate(container, () => Counter({ count: signal(0) }), null);
  assertEq(container.innerHTML, "<div><span>Count: 0</span><button>Add</button></div>");
});

// ---------------------------------------------------------------- data

// `readData` returns the `k-data` payload; nothing else is needed to restore
// the page's loads.
test("readData parses the k-data script", () => {
  const doc = globalThis.document;
  const script = doc.createElement("script");
  script.setAttribute("type", "application/json");
  script.setAttribute("id", "k-data");
  script.textContent = serializeData({ v: 1, now: "2026-11-07T10:00:00Z", session: { user: null }, c: { 0: { loads: { product: { id: 1, name: "</script>" } } } } });
  doc.body.appendChild(script);
  const data = readData(doc);
  assertEq(data.v, 1);
  assertEq(data.c["0"].loads.product.name, "</script>");
});

// A page's load group created from the serialized values doesn't fetch
// again during hydration: no double fetch.
test("hydrated loads are not fetched again", async () => {
  const container = page("<h1>Lamp</h1>");
  const data = { v: 1, now: "2026-11-07T10:00:00Z", session: { user: null }, c: { 0: { loads: { product: { name: "Lamp" } } } } };
  let fetches = 0;
  const t = template("<h1> </h1>");
  let loads = null;
  hydrate(container, () => {
    loads = createLoads([{ name: "product", after: [], run: async () => { fetches++; return { name: "other" }; } }], data.c["0"].loads);
    const h = t();
    renderEffect(() => text(h.firstChild, loads.values().product.name));
    return h;
  }, data);
  await tick();
  assertEq(fetches, 0);
  assertEq(loads.state(), READY);
  assertEq(container.innerHTML, "<h1>Lamp</h1>");
});

// After the first load the runtime remembers the browser's time zone in the
// `k_tz` cookie, so later server renders format dates in it (design 35).
test("hydration sets the k_tz cookie", () => {
  const container = page(counterHtml("Count", 1));
  hydrate(container, () => Counter({ count: signal(1) }), { v: 1, now: "2026-11-07T10:00:00Z", session: { user: null }, c: {} });
  assert(String(globalThis.document.cookie ?? "").includes("k_tz="), String(globalThis.document.cookie));
});

// A textarea's text is its initial value: hydration keeps the server's text
// although the template has none (the binding sets `value`).
test("hydration keeps a textarea's server text", () => {
  const editor = template("<form><textarea></textarea></form>");
  const container = page("<form><textarea>Milk and eggs</textarea></form>");
  const server = container.firstChild.firstChild;
  hydrate(container, () => editor(), null);
  assertEq(container.firstChild.firstChild, server);
  assertEq(server.textContent, "Milk and eggs");
});
