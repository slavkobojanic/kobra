// Components as codegen lowers them (docs/compiler/codegen.md §10): a
// factory `Name(props, ctx)` over the runtime, props as getters, `@Binding`
// as `{get, set}` pairs, `@Emits` handlers read from `props.on$name`, slots
// as functions returning nodes, `@Stylable` getters, one scope per instance.
// The factories below are written exactly as the generator emits them, so
// these tests pin the contract between generated code and the runtime.
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import {
  KPanic, batch, catchError, classList, effect, flush, on, onCleanup, panic, renderEffect, root, scope, signal, template, text,
} from "k:runtime";

/** Inserts `node` between a component's markers, as generated code does. */
function place(end, node) {
  end.parentNode.insertBefore(node, end);
}

// `component Greeting { @Prop let name: String; view { text { "Hello, {name}" } } }`
const t0 = template("<span> </span>");
function Greeting(props, ctx) {
  const root$ = t0();
  ctx.renders = (ctx.renders ?? 0) + 1;
  $bind(root$.firstChild, () => `Hello, ${props.name()}`);
  return root$;
}

function $bind(node, value) {
  renderEffect(() => text(node, value()));
}

// A parent's signal flows into the child through the getter: the child's
// binding updates, the child's factory doesn't run again.
test("a prop getter carries the parent's signal into the child", () => {
  const user = signal("Ada");
  const ctx = {};
  let node = null;
  root(() => scope(() => { node = Greeting({ name: () => user.get() }, ctx); }));
  assertEq(node.textContent, "Hello, Ada");
  batch(() => user.set("Grace"));
  assertEq(node.textContent, "Hello, Grace");
  assertEq(ctx.renders, 1);
});

// `component Stepper { @Binding var value: Int; @Prop let step: Int = 1 … }`
const t1 = template("<div><button>-</button><span> </span><button>+</button></div>");
function Stepper(props, ctx) {
  const step = props.step ?? (() => 1);
  const root$ = t1();
  const minus = root$.firstChild;
  const label = minus.nextSibling.firstChild;
  const plus = root$.lastChild;
  $bind(label, () => `${props.value.get()}`);
  on(minus, "click", () => props.value.set(props.value.get() - step()));
  on(plus, "click", () => props.value.set(props.value.get() + step()));
  return root$;
}

// The child's write goes through `set` to the parent's signal, and both
// DOMs are current when the click handler returns.
test("a binding write updates the parent's state and DOM in one click", () => {
  const quantity = signal(1);
  let child = null;
  const total = globalThis.document.createTextNode(" ");
  root(() => {
    renderEffect(() => text(total, `Total: ${quantity.get() * 3}`));
    scope(() => { child = Stepper({ value: { get: () => quantity.get(), set: ($v) => quantity.set($v) } }, {}); });
  });
  child.lastChild.click();
  assertEq(quantity.peek(), 2);
  assertEq(child.textContent, "-2+");
  assertEq(total.data, "Total: 6");
});

// An option that isn't set takes its default in the child; one that is set
// is a getter too.
test("a chained option is a getter and an unset option takes its default", () => {
  const value = signal(10);
  const pair = { get: () => value.get(), set: ($v) => value.set($v) };
  let a = null;
  let b = null;
  root(() => {
    scope(() => { a = Stepper({ value: pair }, {}); });
    scope(() => { b = Stepper({ value: pair, step: () => 5 }, {}); });
  });
  a.lastChild.click();
  assertEq(value.peek(), 11);
  b.lastChild.click();
  assertEq(value.peek(), 16);
  assertEq(a.textContent, "-16+", "both children read the same binding");
});

// `Binding(get: { celsius * 9 / 5 + 32 }, set: { f => celsius = (f - 32) * 5 / 9 })`
test("a custom binding converts in both directions", () => {
  const celsius = signal(20);
  let child = null;
  root(() => scope(() => {
    child = Stepper({
      value: { get: () => celsius.get() * 9 / 5 + 32, set: (f) => celsius.set((f - 32) * 5 / 9) },
      step: () => 9,
    }, {});
  }));
  assertEq(child.textContent, "-68+");
  child.lastChild.click();
  assertEq(celsius.peek(), 25);
  assertEq(child.textContent, "-77+");
});

// `component Counter { @Emits let changed: Event<Int> … changed.emit(count) }`
const t2 = template("<button>Add</button>");
function Counter(props, ctx) {
  const count = signal(0);
  const changed = props.on$changed ?? null;
  const root$ = t2();
  on(root$, "click", () => {
    count.set(count.get() + 1);
    changed?.(count.get());
  });
  return root$;
}

// `emit` calls the parent's handler inside the child's batch, so the parent's
// DOM is current when the click returns.
test("emit runs the parent's handler inside the child's batch", () => {
  const last = signal(0);
  const label = globalThis.document.createTextNode(" ");
  let button = null;
  let runs = 0;
  root(() => {
    renderEffect(() => { runs++; text(label, `last ${last.get()}`); });
    scope(() => { button = Counter({ on$changed: (value) => last.set(value) }, {}); });
  });
  button.click();
  button.click();
  assertEq(label.data, "last 2");
  assertEq(runs, 3, "one binding run per click");
});

// An event nobody listens to does nothing.
test("an event without a listener does nothing", () => {
  let button = null;
  root(() => scope(() => { button = Counter({}, {}); }));
  button.click();
  assertEq(button.textContent, "Add");
});

// `component Card { @Slot let content: View; view { article { content } } }`
const t3 = template("<article><!--[--><!--]--></article>");
function Card(props, ctx) {
  const root$ = t3();
  const end = root$.lastChild;
  place(end, props.content());
  if (props.footer !== undefined && props.footer !== null) place(end, props.footer());
  return root$;
}

// An unscoped slot is a function returning nodes; the child places them
// between its markers.
test("a slot's nodes are placed between the child's markers", () => {
  const p = template("<p>Review before checkout.</p>");
  let card = null;
  root(() => scope(() => { card = Card({ content: () => p() }, {}); }));
  assertEq(card.outerHTML, "<article><!--[--><p>Review before checkout.</p><!--]--></article>");
});

// An optional slot is `null` when not given.
test("an optional slot that wasn't given is null", () => {
  const p = template("<p>x</p>");
  let card = null;
  root(() => scope(() => { card = Card({ content: () => p(), footer: null }, {}); }));
  assertEq(card.textContent, "x");
});

// Slot content is created while the child runs, so its bindings belong to
// the child's instance and are disposed with it.
test("slot bindings are disposed with the child that placed them", () => {
  const name = signal("a");
  const log = [];
  let disposeChild = null;
  root(() => scope((d) => {
    disposeChild = d;
    Card({
      content: () => {
        const n = globalThis.document.createTextNode(" ");
        renderEffect(() => { log.push(name.get()); text(n, name.get()); });
        return n;
      },
    }, {});
  }));
  disposeChild();
  batch(() => name.set("b"));
  assertDeepEq(log, ["a"]);
});

// A scoped slot is a function of its arguments; the child calls it with the
// row's item.
test("a scoped slot receives the child's data", () => {
  const seen = [];
  const row = (item) => { seen.push(item.name); return globalThis.document.createTextNode(item.name); };
  const host = globalThis.document.createElement("ul");
  root(() => scope(() => {
    for (const item of [{ id: 1, name: "Lamp" }, { id: 2, name: "Desk" }]) host.appendChild(row(item));
  }));
  assertDeepEq(seen, ["Lamp", "Desk"]);
  assertEq(host.textContent, "LampDesk");
});

// `@Stylable let container: Classes` applied with `.class([.row, container])`:
// the parent's getter returns its own hashed classes, reactively.
test("stylable classes from the parent are applied reactively", () => {
  const selected = signal(false);
  const t = template('<div class="CartRow__row">row</div>');
  let el = null;
  root(() => scope(() => {
    const props = { container: () => [selected.get() && "CartList__highlighted"] };
    el = t();
    renderEffect(() => classList(el, "CartRow__row", props.container()));
  }));
  assertEq(el.getAttribute("class"), "CartRow__row");
  batch(() => selected.set(true));
  assertEq(el.getAttribute("class"), "CartRow__row CartList__highlighted");
});

// Unmounting an instance disposes its scope: effects stop, cleanups run, the
// running async effect is aborted (codegen.md §10).
test("unmounting a component stops its effects and aborts its async work", () => {
  const tick$ = signal(0);
  const log = [];
  let aborted = null;
  let unmount = null;
  root(() => scope((d) => {
    unmount = d;
    effect((s) => { log.push(tick$.get()); aborted = s; });
    onCleanup(() => log.push("unmounted"));
  }));
  flush();
  unmount();
  batch(() => tick$.set(1));
  flush();
  assertDeepEq(log, [0, "unmounted"]);
  assert(aborted.aborted);
});

// A child's user effects run after the whole parent factory returned, i.e.
// after mount (runtime.md §3.6).
test("child effects run after the parent mounted", () => {
  const order = [];
  root(() => {
    order.push("parent start");
    scope(() => effect(() => order.push("child effect")));
    order.push("parent end");
  });
  flush();
  assertDeepEq(order, ["parent start", "parent end", "child effect"]);
});

// A panic in a child's binding reaches the nearest ErrorBoundary
// (`catchError`) and doesn't escape the flush (design 09).
test("a panic in a child binding reaches the error boundary", () => {
  const value = signal(1);
  const caught = [];
  root(() => catchError(() => scope(() => {
    const n = globalThis.document.createTextNode(" ");
    renderEffect(() => { if (value.get() === 0) panic("division by zero"); text(n, String(10 / value.get())); });
  }), (e) => caught.push(e)));
  batch(() => value.set(0));
  assertEq(caught.length, 1);
  assert(caught[0] instanceof KPanic);
});

// An async handler's promise isn't awaited by `on`, so the click returns at
// once and the handler finishes later.
test("an async handler is not awaited", async () => {
  const saved = signal(false);
  const button = globalThis.document.createElement("button");
  root(() => on(button, "click", async () => { await null; saved.set(true); }));
  button.click();
  assertEq(saved.peek(), false);
  await tick();
  assertEq(saved.peek(), true);
});
