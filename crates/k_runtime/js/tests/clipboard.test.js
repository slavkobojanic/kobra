/**
 * @module k:/tests/clipboard.test.js
 *
 * `k.device.BrowserClipboard` and `CopyButton` (docs/design/45-web-platform.md, "Clipboard";
 * docs/compiler/runtime.md §12.6) against a scripted `navigator.clipboard` written from the
 * Clipboard API standard and each engine's documented behaviour: no clipboard outside a secure
 * context, `NotAllowedError` for a refused write or read (Chromium without focus, Gecko and WebKit
 * when the user dismisses the per-paste "Paste" confirmation), Gecko's `NotFoundError` and
 * Chromium's `""` for a clipboard without text. The button copies at the click, says "Copied" for
 * two seconds on the container's `Clock`, fires its events, and the server renders it as the
 * browser does.
 */
import { assert, assertEq, test, tick } from "k:testing";
import { Out, natives } from "k:runtime/server";
import { BrowserClipboard, CopyButton, STATUS_SHOWN } from "k:/runtime/clipboard.js";
import { batch, flush, hydrate, root, signal, unitCase } from "k:runtime";

/**
 * A scripted `navigator.clipboard`: `text` is what it holds; `refuse` names the `DOMException`
 * the next call rejects with; `calls` records each call.
 */
function fakeClipboard() {
  const fake = { text: "", refuse: null, calls: [] };
  const answer = (name, value) => {
    fake.calls.push(name);
    const e = fake.refuse;
    fake.refuse = null;
    if (e) return Promise.reject(Object.assign(new Error(e), { name: e }));
    return Promise.resolve(value());
  };
  fake.api = {
    writeText: (t) => answer("writeText", () => void (fake.text = t)),
    readText: () => answer("readText", () => fake.text),
  };
  return fake;
}

/** Runs `f` with `navigator.clipboard` = `api` in a context that is secure or not. */
async function inBrowser(api, secure, f) {
  const saved = [globalThis.navigator, globalThis.isSecureContext];
  globalThis.navigator = { clipboard: api };
  globalThis.isSecureContext = secure;
  try {
    return await f();
  } finally {
    [globalThis.navigator, globalThis.isSecureContext] = saved;
  }
}

/** Settles `p` into `{ok}` or `{error}`. */
const settle = (p) => p.then((ok) => ({ ok }), (error) => ({ error }));

test("write puts the text on the clipboard in a secure context", async () => {
  const fake = fakeClipboard();
  await inBrowser(fake.api, true, async () => {
    const clipboard = BrowserClipboard();
    assert(clipboard.isSupported);
    await clipboard.write(null, "https://example.com/join/42");
    assertEq(fake.text, "https://example.com/join/42");
  });
});

test("outside a secure context there is no clipboard: unsupported, and nothing is called", async () => {
  const fake = fakeClipboard();
  await inBrowser(fake.api, false, async () => {
    const clipboard = BrowserClipboard();
    assert(!clipboard.isSupported);
    assertEq((await settle(clipboard.write(null, "x"))).error, unitCase(0));
    assertEq((await settle(clipboard.readText(null))).error, unitCase(0));
    assertEq(fake.calls.length, 0);
  });
  await inBrowser(undefined, true, async () => {
    assert(!BrowserClipboard().isSupported, "an engine without the API");
  });
});

test("a refused write or read is denied (NotAllowedError: no focus, no activation, a dismissed paste)", async () => {
  const fake = fakeClipboard();
  await inBrowser(fake.api, true, async () => {
    const clipboard = BrowserClipboard();
    fake.refuse = "NotAllowedError";
    assertEq((await settle(clipboard.write(null, "x"))).error, unitCase(1));
    fake.refuse = "NotAllowedError";
    assertEq((await settle(clipboard.readText(null))).error, unitCase(1));
    fake.refuse = "SecurityError";
    assertEq((await settle(clipboard.readText(null))).error, unitCase(1));
  });
});

test("readText answers the text, and nil for a clipboard without text in every engine", async () => {
  const fake = fakeClipboard();
  await inBrowser(fake.api, true, async () => {
    const clipboard = BrowserClipboard();
    fake.text = "pasted";
    assertEq(await clipboard.readText(null), "pasted");
    fake.text = "";
    assertEq(await clipboard.readText(null), null, "Chromium: an image on the clipboard reads as empty text");
    fake.refuse = "NotFoundError";
    assertEq(await clipboard.readText(null), null, "Gecko: no text is NotFoundError");
  });
});

/** A container with a recording clipboard and a clock whose timers the test fires. */
function container() {
  const timers = [];
  const c = {
    copied: [],
    clipboardError: null,
    Clipboard: {
      write: async (_c, text) => {
        if (c.clipboardError) throw c.clipboardError;
        c.copied.push(text);
      },
    },
    Clock: {
      after: (d, f) => {
        const t = { d, f, live: true, cancel: () => void (t.live = false) };
        timers.push(t);
        return t;
      },
    },
    /** Fires the live timers. */
    fire() {
      for (const t of timers.splice(0)) if (t.live) t.f();
      flush();
    },
    timers,
  };
  return c;
}

/** Mounts a `CopyButton` with `props` in `c`; returns its element, button and live region. */
function mount(props, c) {
  let el = null;
  root(() => {
    el = CopyButton(props, { c });
  });
  document.body.appendChild(el);
  return { el, button: el.firstChild, live: el.lastChild };
}

test("the button is named by its label and has a polite live region", () => {
  const { el, button, live } = mount({ text: () => "x", label: () => "Copy invite link" }, container());
  assertEq(el.localName, "span");
  assertEq(button.localName, "button");
  assertEq(button.getAttribute("type"), "button");
  assertEq(button.getAttribute("aria-label"), "Copy invite link");
  assertEq(button.textContent, "Copy invite link");
  assertEq(live.getAttribute("role"), "status");
  assertEq(live.textContent, "");
});

test("a click copies the text as it is then, says Copied for two seconds and fires copied", async () => {
  const c = container();
  const text = signal("first");
  let copied = 0;
  const { button, live } = mount({ text: () => text.get(), label: () => "Copy", on$copied: () => copied++ }, c);
  batch(() => text.set("second"));
  button.dispatchEvent(new Event("click"));
  await tick();
  flush();
  assertEq(c.copied[0], "second", "read at the click");
  assertEq(copied, 1);
  assertEq(button.textContent, "Copied");
  assertEq(live.textContent, "Copied");
  assertEq(button.getAttribute("aria-label"), "Copy", "the name stays the label");
  assertEq(c.timers[0].d, STATUS_SHOWN);
  assertEq(STATUS_SHOWN, 2000);
  c.fire();
  assertEq(button.textContent, "Copy");
  assertEq(live.textContent, "");
});

test("a second copy restarts the two seconds", async () => {
  const c = container();
  const { button } = mount({ text: () => "x", label: () => "Copy" }, c);
  button.dispatchEvent(new Event("click"));
  await tick();
  const first = c.timers[0];
  button.dispatchEvent(new Event("click"));
  await tick();
  assert(!first.live, "the first timer is cancelled");
  assertEq(button.textContent, "Copied");
});

test("a failure fires failed with the error, and says Couldn't copy only when nothing listens", async () => {
  const c = container();
  c.clipboardError = unitCase(1);
  const failures = [];
  const listened = mount({ text: () => "x", label: () => "Copy", on$failed: (e) => failures.push(e) }, c);
  listened.button.dispatchEvent(new Event("click"));
  await tick();
  assertEq(failures[0], unitCase(1));
  assertEq(listened.live.textContent, "", "the app reports it");
  const ignored = mount({ text: () => "x", label: () => "Copy" }, c);
  ignored.button.dispatchEvent(new Event("click"));
  await tick();
  assertEq(ignored.live.textContent, "Couldn't copy");
  assertEq(ignored.button.textContent, "Couldn't copy");
});

test("unmounting cancels the timer", async () => {
  const c = container();
  let el = null;
  let dispose = null;
  root((d) => {
    dispose = d;
    el = CopyButton({ text: () => "x", label: () => "Copy" }, { c });
  });
  el.firstChild.dispatchEvent(new Event("click"));
  await tick();
  dispose();
  assert(!c.timers[0].live);
});

test("the server renders the same button, which the browser takes over while hydrating", () => {
  const out = new Out();
  natives["k.device.CopyButton$ssr"]({ text: () => "x", label: () => 'Copy "it" <now>' }, {}, out);
  const html = out.html();
  assert(html.startsWith('<span class="k-status-button"><button type="button" aria-label="Copy &quot;it&quot; &lt;now&gt;">Copy "it" &lt;now&gt;</button><span role="status"'), html);
  const host = document.createElement("div");
  host.innerHTML = html;
  document.body.appendChild(host);
  const span = host.firstChild;
  let node = null;
  const c = container();
  root(() => {
    node = hydrate(host, () => CopyButton({ text: () => "x", label: () => 'Copy "it" <now>' }, { c }), null);
  });
  flush();
  assertEq(node, span, "the server's span is kept");
  assertEq(host.childNodes.length, 1);
  assertEq(span.firstChild.getAttribute("aria-label"), 'Copy "it" <now>');
});
