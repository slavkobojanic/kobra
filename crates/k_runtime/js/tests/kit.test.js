/**
 * @module k:/tests/kit.test.js
 *
 * The test kit behind `k test` (docs/design/21-testing.md, "Rendering and
 * querying", "Test doubles and defaults"; docs/compiler/runtime.md,
 * "The test kit"): accessibility queries over a rendered view, actions
 * that flush, `TestClock`, `MemoryStorage`, `TestHttpClient`, the recording
 * doubles, expectation failures and the runner.
 */
import { assert, assertDeepEq, assertEq, assertThrows, installDom, test } from "k:testing";
import { Instant, KPanic, effect, flush, isPanic, on, root, signal, unitCase } from "k:runtime";
import * as kit from "k:testing/kit";

/** Mounts `html` as a rendered view and returns its screen. */
function screenOf(html) {
  installDom();
  kit.begin({});
  return kit.render(() => {
    const t = document.createElement("template");
    t.innerHTML = html;
    return t.content;
  });
}

/** Expects `fn` to fail the test with a message containing `text`. */
function failsWith(fn, text) {
  try {
    fn();
  } catch (e) {
    assert(isPanic(e), `a test failure is a panic: ${e}`);
    assert(String(e.message).includes(text), `${e.message} mentions ${text}`);
    return;
  }
  throw new Error(`expected a failure mentioning ${text}`);
}

test("names and labels match exactly after whitespace normalisation", () => {
  const s = screenOf(
    '<h2>  Your   cart </h2><button>Add</button><button>Add to cart</button>' +
      '<label for="e">Email</label><input id="e"><label for="a">Email address</label><input id="a">' +
      '<button aria-label="Close dialog">×</button>',
  );
  assert(s.heading("Your cart").exists);
  assertEq(s.heading("Your cart").level, 2);
  assert(s.button("Add").exists, "Add matches only the button named Add");
  assert(s.button("Add to cart").exists);
  assert(!s.button("Add to").exists);
  assert(s.textField("Email").exists);
  assert(s.textField("Email address").exists);
  assert(s.button("Close dialog").exists, "aria-label is the accessible name");
  assertEq(s.buttons().length, 3);
});

test("visually hidden text is part of a button's name", () => {
  const s = screenOf('<button>Edit<span style="position: absolute; width: 1px"> Buy milk</span></button>');
  assert(s.button("Edit Buy milk").isVisible);
});

test("text(_) matches the deepest element; text(containing:) a substring", () => {
  const s = screenOf("<div><p>Total: <b>$9.99</b></p><p>Total</p></div>");
  assertEq(s.text("Total").$text(), "Total");
  assert(!s.text("$9").exists);
  assert(s.text$containing("$9.99").exists);
  assertEq(s.text$containing("Total").exists, false, "two elements contain Total: not exactly one");
});

test("a query that matches nothing exists = false; acting on it fails", () => {
  const s = screenOf("<p>hi</p>");
  const missing = s.button("Save");
  assert(!missing.exists);
  assert(!missing.isVisible);
  failsWith(() => missing.click(), 'button(named: "Save") found nothing');
  failsWith(() => missing.$text(), 'button(named: "Save") found nothing');
});

test("an action on a handle that matches several elements fails, naming the count", () => {
  const s = screenOf("<button>Go</button><button>Go</button>");
  failsWith(() => s.button("Go").click(), 'button(named: "Go") found 2 elements');
});

test("queries on a handle are scoped to its subtree", () => {
  const s = screenOf("<ul><li>Buy milk <button>Delete</button></li><li>Walk the dog <button>Delete</button></li></ul>");
  assertEq(s.items().length, 2);
  assert(s.list().exists);
  assert(s.item$containing("Walk").button("Delete").exists);
  assertEq(s.item$containing("Walk").$text(), "Walk the dog Delete");
});

test("hidden elements exist but aren't visible", () => {
  const s = screenOf('<div hidden><button>A</button></div><button aria-hidden="true">B</button><div style="display: none"><button>C</button></div><button>D</button>');
  for (const n of ["A", "B", "C"]) {
    assert(s.button(n).exists, n);
    assert(!s.button(n).isVisible, n);
  }
  assert(s.button("D").isVisible);
});

test("element state: checked, pressed, disabled, value and live regions", () => {
  const s = screenOf(
    '<input type="checkbox" aria-label="Done"><button aria-pressed="true">Bold</button><button disabled>Save</button>' +
      '<label for="t">Title</label><input id="t" value="x"><p role="alert">Too long</p><p role="status">Saved</p>' +
      '<label for="p">Password</label><input id="p" type="password"><label for="n">Age</label><input id="n" type="number">' +
      '<label for="d">Notes</label><textarea id="d"></textarea><label for="s">Size</label><select id="s"></select>' +
      '<nav aria-label="Main"><a href="/">Home</a></nav>',
  );
  assert(!s.checkbox("Done").isChecked);
  assert(s.button("Bold").isPressed);
  assert(s.button("Save").isDisabled);
  assertEq(s.textField("Title").value, "x");
  assertEq(s.alert().$text(), "Too long");
  assertEq(s.status().$text(), "Saved");
  assert(s.passwordField("Password").exists);
  assert(!s.textField("Password").exists, "a password field isn't a text field");
  assert(s.numberField("Age").exists);
  assert(s.textArea("Notes").exists);
  assert(s.dropdown("Size").exists);
  assert(s.navigation("Main").link("Home").exists);
  assertEq(s.links().length, 1);
  assertEq(s.headings().length, 0);
});

test("click toggles a checkbox and fires change; a disabled control ignores clicks", () => {
  const s = screenOf('<input type="checkbox" aria-label="Done"><button disabled>Save</button>');
  const box = document.body.firstChild.firstChild;
  let changes = 0;
  box.addEventListener("change", () => changes++);
  s.checkbox("Done").click();
  assert(s.checkbox("Done").isChecked);
  assertEq(changes, 1);
  let clicks = 0;
  document.body.firstChild.lastChild.addEventListener("click", () => clicks++);
  s.button("Save").click();
  assertEq(clicks, 0);
});

test("fill replaces the value and fires input; Enter and a submit button submit the form", () => {
  const s = screenOf('<form><label for="f">Todo</label><input id="f"><button type="submit">Add</button><button type="button">Cancel</button></form>');
  const form = document.body.firstChild.firstChild;
  const inputs = [];
  let submits = 0;
  form.addEventListener("input", (e) => inputs.push(e.target.value));
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    submits++;
  });
  s.textField("Todo").fill("Buy milk");
  assertDeepEq(inputs, ["Buy milk"]);
  assertEq(s.textField("Todo").value, "Buy milk");
  s.textField("Todo").press(unitCase(0));
  assertEq(submits, 1, "Enter submits");
  s.button("Add").click();
  assertEq(submits, 2, "a submit button submits");
  s.button("Cancel").click();
  assertEq(submits, 2, "a type=button button doesn't");
  let escapes = 0;
  form.addEventListener("keydown", (e) => {
    if (e.key === "Escape") escapes++;
  });
  s.textField("Todo").press(unitCase(1));
  assertEq(escapes, 1);
});

test("actions flush synchronous updates", () => {
  installDom();
  kit.begin({});
  const n = signal(0);
  const s = kit.render(() => {
    const b = document.createElement("button");
    b.textContent = "Add";
    on(b, "click", () => n.set(n.get() + 1));
    const p = document.createElement("p");
    effect(() => {
      p.textContent = `Count ${n.get()}`;
    });
    const f = document.createDocumentFragment();
    f.appendChild(b);
    f.appendChild(p);
    return f;
  });
  flush();
  s.button("Add").click();
  assert(s.text("Count 1").exists, "no settle needed");
});

test("unmount removes the view and runs its cleanups", () => {
  installDom();
  kit.begin({});
  const s = kit.render(() => document.createElement("p"));
  assertEq(document.body.childNodes.length, 1);
  s.unmount();
  assertEq(document.body.childNodes.length, 0);
});

test("TestClock starts at 2026-01-01T12:00:00Z and fires due timers in order on advance", () => {
  const clock = kit.testClock();
  assertEq(clock.now.ms, Date.UTC(2026, 0, 1, 12, 0, 0));
  const fired = [];
  clock.after(2000, () => fired.push("after 2s"));
  const every = clock.every(1000, () => fired.push(`tick ${clock.now.ms - Date.UTC(2026, 0, 1, 12)}`));
  clock.advance(2500);
  assertDeepEq(fired, ["tick 1000", "after 2s", "tick 2000"], "timers due at the same instant fire in the order they were set");
  every.cancel();
  clock.advance(5000);
  assertEq(fired.length, 3, "a cancelled timer doesn't fire");
  assertEq(clock.now.ms, Date.UTC(2026, 0, 1, 12) + 7500);
  clock.set(new Instant(0));
  assertEq(clock.now.ms, 0);
});

test("TestClock sleep resolves when virtual time passes", async () => {
  const clock = kit.testClock();
  let woke = false;
  const p = clock.sleep(null, 1000).then(() => {
    woke = true;
  });
  await Promise.resolve();
  assert(!woke);
  clock.advance(1000);
  await p;
  assert(woke);
});

const R = { unavailable: unitCase(0), unreadable: unitCase(1) };
const W = { unavailable: unitCase(0), quotaExceeded: unitCase(1) };
const codec = { encode: (v) => ({ n: v }), decode: (j) => j.n };

test("MemoryStorage stores encoded values and simulates failures", () => {
  const s = kit.memoryStorage(R, W);
  const key = { name: "count" };
  assertEq(s.read(codec, key), null);
  s.write(codec, key, 3);
  assertEq(s.read(codec, key), 3);
  s.writeRaw("count", "{{ not json");
  assertEq(assertThrows(() => s.read(codec, key)), R.unreadable);
  s.failReads(R.unavailable);
  assertEq(assertThrows(() => s.read(codec, key)), R.unavailable);
  s.failWrites(W.quotaExceeded);
  assertEq(assertThrows(() => s.write(codec, key, 1)), W.quotaExceeded);
  s.stopFailing();
  s.write(codec, key, 4);
  assertEq(s.read(codec, key), 4);
  s.remove(codec, key);
  assertEq(s.read(codec, key), null);
});

test("TestHttpClient answers canned paths and fails the test on any other", async () => {
  const E = { offline: unitCase(0), timeout: unitCase(1), status: (s) => ({ $: 2, _0: s }) };
  const http = kit.testHttpClient(E, {});
  http.serveText("/hello", "hi", { $: 0 });
  const r = await http.send(null, { method: { $: 0 }, url: "https://api.example.com/hello?x=1", headers: new Map(), body: null });
  assertEq(r.body, "hi");
  assertEq(http.requests.length, 1);
  http.fail("/down", E.offline);
  let threw = null;
  try {
    await http.send(null, { url: "https://api.example.com/down" });
  } catch (e) {
    threw = e;
  }
  assertEq(threw, E.offline);
  let failure = null;
  try {
    await http.send(null, { url: "https://api.example.com/other" });
  } catch (e) {
    failure = e;
  }
  assert(isPanic(failure) && failure.message.includes("https://api.example.com/other"), "an unexpected request fails the test, naming the URL");
});

test("the recording doubles keep what they were given", async () => {
  const q = kit.recordingJobQueue();
  const kind = { name: "order-confirmation" };
  await q.enqueue(null, codec, kind, { id: 1 }, 0);
  assertDeepEq(q.payloads(codec, kind), [{ id: 1 }]);
  assertDeepEq(q.payloads(codec, { name: "other" }), []);
  const mail = kit.recordingEmailProvider();
  await mail.send(null, { to: "a@example.com" });
  assertEq(mail.sent.length, 1);
  mail.simulateFailure("down");
  let threw = null;
  try {
    await mail.send(null, {});
  } catch (e) {
    threw = e;
  }
  assertEq(threw, "down");
});

test("fail throws a panic carrying the expectation and the values", () => {
  let e = null;
  try {
    kit.fail({ start: 3, end: 9, source: "a == 1", values: [["a", () => "0"]] });
  } catch (x) {
    e = x;
  }
  assert(e instanceof KPanic);
  assertDeepEq(e.kTest, { start: 3, end: 9, message: "expectation failed: a == 1", values: [["a", "0"]] });
});

test("the runner gives every test a fresh container and reports each outcome", async () => {
  const made = [];
  /** A stand-in for a generated test container; records each instance. */
  class Container {
    /** Records this instance in `made`. */
    constructor() {
      made.push(this);
    }
  }
  /** A stand-in for a generated test group: one method per test. */
  class Group {
    /** Keeps the test's container. */
    constructor(c) {
      this.c = c;
    }
    async $t0() {}
    /** Fails an expectation. */
    async $t1() {
      kit.fail({ start: 1, end: 2, source: "x", values: [] });
    }
    /** Panics. */
    async $t2() {
      throw new KPanic("boom");
    }
    /** Throws a typed error. */
    async $t3() {
      throw unitCase(1);
    }
  }
  const results = await kit.run(
    [{ name: "G", file: "src/a/A.test.k", container: () => new Container(), group: Group, tests: [
      { name: "passes", method: "$t0", start: 0, end: 1 },
      { name: "fails", method: "$t1", start: 0, end: 1 },
      { name: "panics", method: "$t2", start: 0, end: 1 },
      { name: "throws", method: "$t3", start: 5, end: 6 },
      { name: "skipped", method: "$t0", start: 0, end: 1 },
    ] }],
    ["G › passes", "G › fails", "G › panics", "G › throws"],
  );
  assertEq(made.length, 4, "one container per test");
  assertDeepEq(results.map((r) => r.outcome), ["pass", "fail", "fail", "fail"]);
  assertEq(results[1].failure.message, "expectation failed: x");
  assertEq(results[2].failure.message, "the test panicked: boom");
  assertEq(results[3].failure.message, "the test threw an error it didn't catch");
  assertEq(results[3].failure.start, 5);
});

// A failed test's watchdog stops with it: with server work of the next
// test in flight, a watchdog still running for a failed test would spin
// (the failure settles its race at once) and starve the run.
test("a failed test's watchdog stops when the test does", async () => {
  let spins = 0;
  const server = {
    spec: { session: null },
    app: async () => ({ status: 200, headers: [], body: "" }),
    groups: [(st) => ({ $r: st })],
    reset() {},
  };
  /** A test group whose first test fails and whose second keeps server work in flight. */
  class Group {
    /** Panics. */
    async $t0() {
      throw new KPanic("first fails");
    }
    /** Spins through 20 event-loop turns, counting them in `spins`. */
    async $t1() {
      // Server work in flight for a while: many event-loop turns.
      for (let i = 0; i < 20; i++) {
        await new Promise((resolve) => Promise.resolve().then(resolve));
        spins++;
      }
    }
  }
  const results = await kit.run(
    [{ name: "G", file: "f", container: () => ({}), group: Group, tests: [
      { name: "fails", method: "$t0", start: 0, end: 1 },
      { name: "then passes", method: "$t1", start: 0, end: 1 },
    ] }],
    ["G › fails", "G › then passes"],
    server,
  );
  assertDeepEq(results.map((r) => r.outcome), ["fail", "pass"]);
  assertEq(spins, 20);
  assertEq(kit.watchdogs(), 0, "no watchdog is left running");
});

test("a test that never finishes its async work is reported, not hung", async () => {
  /** A test group whose only test never finishes. */
  class Group {
    /** Returns a promise that never settles. */
    $t0() {
      return new Promise(() => {});
    }
  }
  const results = await kit.run(
    [{ name: "G", file: "f", container: () => ({}), group: Group, tests: [{ name: "hangs", method: "$t0", start: 0, end: 1 }] }],
    ["G › hangs"],
  );
  assertEq(results[0].outcome, "fail");
  assert(results[0].failure.message.includes("never finished"), results[0].failure.message);
});

// Test code runs in one process: a server-only stdlib member it calls is
// the server's, while the browser's own natives (deterministic ids) win.
test("native answers the browser's members first, then the server's", async () => {
  assertEq(typeof kit.native("k.crypto.Hmac").sha256, "function");
  const hex = kit.native("k.core.Bytes$parse$hex")("0aff");
  assertDeepEq([...hex], [10, 255]);
  kit.installBrowser();
  assertEq(kit.native("k.crypto.Uuid$random")(), "00000000-0000-4000-8000-000000000001");
});

// ------------------------------------------------------- k.drag and k.device

test("TestFiles answers the chosen files in order, else .cancelled, and records saves", async () => {
  const files = kit.testFiles();
  const a = { name: "a.json", mediaType: "application/json", bytes: new Uint8Array([1]) };
  const b = { name: "b.json", mediaType: "application/json", bytes: new Uint8Array([2]) };
  let threw = null;
  try {
    await files.open(null, [unitCase(0)]);
  } catch (e) {
    threw = e;
  }
  assertEq(threw, unitCase(0), "nothing chosen: the user closed the picker");
  files.choose(a);
  files.choose(b);
  assertEq(await files.open(null, []), a);
  assertEq(await files.open(null, []), b);
  files.choose(a);
  files.choose(b);
  assertDeepEq((await files.openAll(null, [])).map((f) => f.name), ["a.json", "b.json"]);
  assertEq(files.requests.length, 4);
  assertEq(files.requests[0][0], unitCase(0), "the types of each request");
  files.choose(a);
  files.failOpen({ $: 1, limit: 16777216 });
  threw = null;
  try {
    await files.open(null, []);
  } catch (e) {
    threw = e;
  }
  assertDeepEq(threw, { $: 1, limit: 16777216 });
  assertEq(await files.open(null, []), a, "failOpen applies to the next open only");
  files.save(a);
  assertDeepEq(files.saved, [a]);
  files.failSaves(unitCase(0));
  assertEq(assertThrows(() => files.save(b)), unitCase(0));
  assertEq(files.saved.length, 1, "a failed save keeps nothing");
});

test("press knows the arrow keys left and right, and characters", () => {
  const s = screenOf("<button>Go</button>");
  const keys = [];
  document.body.firstChild.firstChild.addEventListener("keydown", (e) => keys.push(e.key));
  s.button("Go").press({ $: 6 });
  s.button("Go").press({ $: 7 });
  s.button("Go").press({ $: 8, _0: "n" });
  assertDeepEq(keys, ["ArrowLeft", "ArrowRight", "n"], "and .character(_) types its text");
});

test("list(named:) finds a list by its accessible name", () => {
  const s = screenOf('<ul aria-label="To do"><li>a</li></ul><ul aria-label="Done"></ul>');
  assert(s.list$named("To do").exists);
  assertEq(s.list$named("To do").items().length, 1);
  assert(!s.list$named("Doing").exists);
});

test("drag(to:) presses, moves past the threshold onto the target, releases and clicks there", () => {
  const s = screenOf('<ul aria-label="A"><li>card</li></ul><ul aria-label="B"></ul>');
  const seen = [];
  const host = document.body.firstChild;
  for (const type of ["pointerdown", "pointermove", "pointerup", "click"]) {
    host.addEventListener(type, (e) => seen.push([type, e.target.getAttribute("aria-label") ?? e.target.textContent, e.clientX, e.pointerType]));
  }
  s.item("card").drag(s.list$named("B"));
  assertDeepEq(seen, [
    ["pointerdown", "card", 0, "mouse"],
    ["pointermove", "B", 8, "mouse"],
    ["pointermove", "B", 16, "mouse"],
    ["pointerup", "B", 16, "mouse"],
    ["click", "B", 16, "mouse"],
  ]);
});
