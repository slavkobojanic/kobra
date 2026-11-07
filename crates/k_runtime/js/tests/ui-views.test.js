/**
 * @module k:/tests/ui-views.test.js
 *
 * View bindings as generated code uses them: attributes (including ARIA
 * state), input value bindings, class lists, reactive style values and
 * events (docs/compiler/runtime.md §5.2, §5.4; docs/compiler/codegen.md §11).
 */
import { assert, assertEq, test } from "k:testing";
import { attr, batch, classList, on, prop, renderEffect, root, signal, style, template } from "k:runtime";

// ARIA state is text: `.pressed(isOn)` writes "true" or "false" and never
// removes the attribute, because an absent `aria-pressed` means "not a toggle".
test("an aria-pressed binding writes true and false as text", () => {
  const isOn = signal(false);
  const button = template('<button>Bold</button>')();
  root(() => renderEffect(() => attr(button, "aria-pressed", String(isOn.get()))));
  assertEq(button.outerHTML, '<button aria-pressed="false">Bold</button>');
  batch(() => isOn.set(true));
  assertEq(button.outerHTML, '<button aria-pressed="true">Bold</button>');
});

// A boolean HTML attribute (`.disabled`) is present for true, absent for
// false.
test("a disabled binding adds and removes the attribute", () => {
  const isBusy = signal(false);
  const button = template("<button>Send</button>")();
  root(() => renderEffect(() => attr(button, "disabled", isBusy.get())));
  assertEq(button.hasAttribute("disabled"), false);
  batch(() => isBusy.set(true));
  assertEq(button.outerHTML, "<button disabled>Send</button>");
  batch(() => isBusy.set(false));
  assertEq(button.outerHTML, "<button>Send</button>");
});

// A dynamic `aria-label` follows its value; `null` removes it.
test("an aria-label binding follows its value", () => {
  const title = signal("Filters");
  const nav = template("<nav></nav>")();
  root(() => renderEffect(() => attr(nav, "aria-label", title.get())));
  assertEq(nav.getAttribute("aria-label"), "Filters");
  batch(() => title.set("Sort"));
  assertEq(nav.getAttribute("aria-label"), "Sort");
  batch(() => title.set(null));
  assertEq(nav.hasAttribute("aria-label"), false);
});

// Attribute values are set as data, never parsed: markup in a value stays
// text and is escaped when serialized.
test("attribute values are never parsed as markup", () => {
  const value = signal('"><script>alert(1)</script>');
  const el = template("<section></section>")();
  root(() => renderEffect(() => attr(el, "aria-label", value.get())));
  assertEq(el.childNodes.length, 0);
  assertEq(el.getAttribute("aria-label"), '"><script>alert(1)</script>');
  assert(!el.outerHTML.includes("<script>"), el.outerHTML);
});

// `textField(value: bind name)`: the signal drives the `value` property and
// the `input` event writes the signal. Writing back the value the user typed
// doesn't touch the property again, so the caret stays where it is.
test("a text field binding doesn't rewrite the value the user typed", () => {
  const name = signal("");
  const input = template("<input>")();
  let writes = 0;
  let current = "";
  Object.defineProperty(input, "value", { get: () => current, set: (v) => { writes++; current = v; } });
  root(() => {
    renderEffect(() => prop(input, "value", name.get()));
    on(input, "input", () => name.set(input.value));
  });
  assertEq(writes, 0, "the initial empty value is already there");
  current = "Ad";
  input.dispatchEvent(new globalThis.Event("input", { bubbles: true }));
  assertEq(name.peek(), "Ad");
  assertEq(writes, 0);
  batch(() => name.set("Ada"));
  assertEq(current, "Ada");
  assertEq(writes, 1);
});

// `checkbox(isChecked: bind done)` binds the `checked` property both ways.
test("a checkbox binding follows the signal and writes it on change", () => {
  const done = signal(false);
  const box = template('<input type="checkbox">')();
  root(() => {
    renderEffect(() => prop(box, "checked", done.get()));
    on(box, "change", () => done.set(box.checked));
  });
  assertEq(box.checked, false);
  box.checked = true;
  box.dispatchEvent(new globalThis.Event("change", { bubbles: true }));
  assertEq(done.peek(), true);
  batch(() => done.set(false));
  assertEq(box.checked, false);
});

// `.class([.panel, .expanded when isExpanded])` toggles the conditional class
// and keeps the static one.
test("a conditional class toggles and keeps the static classes", () => {
  const isExpanded = signal(false);
  const el = template('<div class="Accordion__panel">x</div>')();
  root(() => renderEffect(() => classList(el, "Accordion__panel", [isExpanded.get() && "Accordion__expanded"])));
  assertEq(el.getAttribute("class"), "Accordion__panel");
  batch(() => isExpanded.set(true));
  assertEq(el.getAttribute("class"), "Accordion__panel Accordion__expanded");
  batch(() => isExpanded.set(false));
  assertEq(el.getAttribute("class"), "Accordion__panel");
});

// A class list without static classes doesn't leave a leading space.
test("a class list of only conditional classes has no stray spaces", () => {
  const a = signal(true);
  const b = signal(true);
  const el = template("<div>x</div>")();
  root(() => renderEffect(() => classList(el, "", [a.get() && "X__a", b.get() && "X__b"])));
  assertEq(el.getAttribute("class"), "X__a X__b");
  batch(() => a.set(false));
  assertEq(el.getAttribute("class"), "X__b");
});

// `{accent}` in a style block: the element's custom property follows the
// signal (codegen.md §12, "Reactive values").
test("a reactive style value updates the element's custom property", () => {
  const accent = signal("#3B82F6");
  const el = template('<section class="Summary__summary">x</section>')();
  root(() => renderEffect(() => style(el, "--k-Summary-0", accent.get())));
  assertEq(el.style.getPropertyValue("--k-Summary-0"), "#3B82F6");
  batch(() => accent.set("var(--k-color-muted)"));
  assertEq(el.style.getPropertyValue("--k-Summary-0"), "var(--k-color-muted)");
});

// `mouseEnter` is the DOM's `mouseenter`; handlers run batched.
test("handlers of several events on one element each run batched", () => {
  const log = signal([]);
  const el = template("<div>zone</div>")();
  let runs = 0;
  root(() => {
    renderEffect(() => { log.get(); runs++; });
    on(el, "mouseenter", () => { log.set([...log.peek(), "enter"]); log.set([...log.peek(), "again"]); });
    on(el, "mouseleave", () => log.set([...log.peek(), "leave"]));
  });
  el.dispatchEvent(new globalThis.Event("mouseenter"));
  el.dispatchEvent(new globalThis.Event("mouseleave"));
  assertEq(log.peek().join(","), "enter,again,leave");
  assertEq(runs, 3, "one binding run per event");
});

// Delegation (runtime.md §5.4, v0): bubbling events like `click` are handled
// by one listener per document, so a thousand rows install no listener of
// their own.
test("delegated clicks install no listener per element", () => {
  const doc = globalThis.document;
  const buttons = [];
  let clicks = 0;
  root(() => {
    for (let i = 0; i < 100; i++) {
      const b = doc.createElement("button");
      doc.body.appendChild(b);
      on(b, "click", () => clicks++);
      buttons.push(b);
    }
  });
  const own = buttons.reduce((n, b) => n + (b.listeners.get("click")?.length ?? 0), 0);
  assertEq(own, 0);
  buttons[42].click();
  assertEq(clicks, 1);
});

// With delegation, `stopPropagation` in an inner handler still keeps an
// outer element's handler from running.
test("delegated handlers respect stopPropagation", () => {
  const doc = globalThis.document;
  const outer = doc.createElement("div");
  const inner = doc.createElement("button");
  outer.appendChild(inner);
  doc.body.appendChild(outer);
  const log = [];
  root(() => {
    on(outer, "click", () => log.push("outer"));
    on(inner, "click", (e) => { log.push("inner"); e.stopPropagation(); });
  });
  inner.click();
  assertEq(log.join(","), "inner");
  assertEq((inner.listeners.get("click")?.length ?? 0) + (outer.listeners.get("click")?.length ?? 0), 0);
});
