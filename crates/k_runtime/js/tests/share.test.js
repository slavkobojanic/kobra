/**
 * @module k:/tests/share.test.js
 *
 * `k.device.BrowserShare` and `ShareButton` (docs/design/45-web-platform.md, "Web Share";
 * docs/compiler/runtime.md §12.6) against a scripted `navigator.share` written from the Web Share
 * standard and each engine's documented behaviour: no `share` in desktop Gecko or Chromium on
 * Linux, none outside a secure context, `AbortError` when the user closes the sheet,
 * `NotAllowedError` when the activation is missing or already used (WebKit's short window),
 * `InvalidStateError` while a sheet is open, and `canShare` refusing content the sheet can't take.
 * The button shares where it can and copies the link where it can't.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { Out, natives } from "k:runtime/server";
import { BrowserShare, ShareButton, shareData } from "k:/runtime/share.js";
import { flush, pageUrl, root, unitCase } from "k:runtime";

/** A scripted `navigator` whose `share` rejects with the `DOMException` named by `refuse` once. */
function fakeNavigator({ canShare = true } = {}) {
  const fake = { shared: [], refuse: null };
  fake.navigator = {
    share: async (data) => {
      const e = fake.refuse;
      fake.refuse = null;
      if (e === "TypeError") throw new TypeError("bad data");
      if (e) throw Object.assign(new Error(e), { name: e });
      fake.shared.push(data);
    },
    canShare: () => canShare,
  };
  return fake;
}

/** Runs `f` with `navigator` = `nav` in a context that is secure or not. */
async function inBrowser(nav, secure, f) {
  const saved = [globalThis.navigator, globalThis.isSecureContext];
  globalThis.navigator = nav;
  globalThis.isSecureContext = secure;
  try {
    return await f();
  } finally {
    [globalThis.navigator, globalThis.isSecureContext] = saved;
  }
}

/** Settles `p` into `{ok}` or `{error}`. */
const settle = (p) => p.then((ok) => ({ ok }), (error) => ({ error }));

const link = { url: "https://example.com/recipes/7", text: null, title: "Soup" };

test("the share data has the content's parts and nothing for the missing ones", () => {
  assertDeepEq(shareData(link), { url: "https://example.com/recipes/7", title: "Soup" });
  assertDeepEq(shareData({ url: null, text: "hi", title: null }), { text: "hi" });
});

test("share opens the sheet with the content and resolves when a target was picked", async () => {
  const fake = fakeNavigator();
  await inBrowser(fake.navigator, true, async () => {
    const share = BrowserShare();
    assert(share.isSupported);
    await share.share(null, link);
    assertDeepEq(fake.shared, [{ url: "https://example.com/recipes/7", title: "Soup" }]);
  });
});

test("no share sheet (desktop Gecko, Chromium on Linux, an insecure context) is unsupported", async () => {
  await inBrowser({}, true, async () => {
    assert(!BrowserShare().isSupported);
    assertEq((await settle(BrowserShare().share(null, link))).error, unitCase(0));
  });
  const fake = fakeNavigator();
  await inBrowser(fake.navigator, false, async () => {
    assert(!BrowserShare().isSupported);
    assertEq((await settle(BrowserShare().share(null, link))).error, unitCase(0));
    assertEq(fake.shared.length, 0);
  });
});

test("content canShare refuses, or share rejects as a TypeError, is unsupported", async () => {
  const fake = fakeNavigator({ canShare: false });
  await inBrowser(fake.navigator, true, async () => {
    assertEq((await settle(BrowserShare().share(null, link))).error, unitCase(0));
    assertEq(fake.shared.length, 0, "the sheet never opened");
  });
  const typed = fakeNavigator();
  typed.refuse = "TypeError";
  await inBrowser(typed.navigator, true, async () => {
    assertEq((await settle(BrowserShare().share(null, link))).error, unitCase(0));
  });
});

test("closing the sheet is cancelled; a refusal or a sheet already open is denied", async () => {
  const fake = fakeNavigator();
  await inBrowser(fake.navigator, true, async () => {
    const share = BrowserShare();
    fake.refuse = "AbortError";
    assertEq((await settle(share.share(null, link))).error, unitCase(1));
    fake.refuse = "NotAllowedError";
    assertEq((await settle(share.share(null, link))).error, unitCase(2), "no activation, or a used one");
    fake.refuse = "InvalidStateError";
    assertEq((await settle(share.share(null, link))).error, unitCase(2), "a sheet is open");
  });
});

test("a page's link is made absolute against the page's origin, and stays a path on the server", () => {
  const saved = globalThis.location;
  try {
    globalThis.location = { origin: "https://recipes.example" };
    assertEq(pageUrl("/recipes/7"), "https://recipes.example/recipes/7");
    globalThis.location = undefined;
    assertEq(pageUrl("/recipes/7"), "/recipes/7");
  } finally {
    globalThis.location = saved;
  }
});

/** A container with a recording `Share` (supported or not) and `Clipboard`, and a clock. */
function container(supported) {
  const c = {
    shared: [],
    copied: [],
    shareError: null,
    copyError: null,
    timers: [],
    Share: {
      isSupported: supported,
      share: async (_c, content) => {
        if (c.shareError) throw c.shareError;
        c.shared.push(content);
      },
    },
    Clipboard: {
      write: async (_c, text) => {
        if (c.copyError) throw c.copyError;
        c.copied.push(text);
      },
    },
    Clock: {
      after: (d, f) => {
        const t = { d, f, live: true, cancel: () => void (t.live = false) };
        c.timers.push(t);
        return t;
      },
    },
  };
  return c;
}

/** Mounts a `ShareButton` for `content` with `events` in `c`, clicks it, and waits. */
async function click(c, content, events = {}) {
  let el = null;
  root(() => {
    el = ShareButton({ content: () => content, label: () => "Share recipe", ...events }, { c });
  });
  document.body.appendChild(el);
  el.firstChild.dispatchEvent(new Event("click"));
  await tick();
  flush();
  return { button: el.firstChild, live: el.lastChild };
}

test("where the browser can share, the button shares and fires shared", async () => {
  const c = container(true);
  const fired = [];
  const { button, live } = await click(c, link, { on$shared: () => fired.push("shared"), on$copied: () => fired.push("copied") });
  assertEq(button.getAttribute("aria-label"), "Share recipe");
  assertDeepEq(c.shared, [link]);
  assertDeepEq(c.copied, []);
  assertDeepEq(fired, ["shared"]);
  assertEq(live.textContent, "", "the sheet was the feedback");
});

test("closing the sheet fires nothing and says nothing", async () => {
  const c = container(true);
  c.shareError = unitCase(1);
  const fired = [];
  const { live } = await click(c, link, { on$failed: (e) => fired.push(e) });
  assertDeepEq(fired, []);
  assertEq(live.textContent, "");
});

test("a refused share fires failed(.denied), and says Couldn't share when nothing listens", async () => {
  const c = container(true);
  c.shareError = unitCase(2);
  const fired = [];
  await click(c, link, { on$failed: (e) => fired.push(e) });
  assertDeepEq(fired, [unitCase(0)]);
  const { live } = await click(c, link);
  assertEq(live.textContent, "Couldn't share");
});

test("where the browser can't share, the button copies the link and says Link copied for two seconds", async () => {
  const c = container(false);
  const fired = [];
  const { button, live } = await click(c, link, { on$copied: () => fired.push("copied") });
  assertDeepEq(c.copied, ["https://example.com/recipes/7"]);
  assertDeepEq(fired, ["copied"]);
  assertEq(live.textContent, "Link copied");
  assertEq(button.textContent, "Link copied");
  assertEq(c.timers[0].d, 2000);
  c.timers[0].f();
  assertEq(button.textContent, "Share recipe");
});

test("content the sheet can't take is copied instead; text says Copied", async () => {
  const c = container(true);
  c.shareError = unitCase(0);
  const { live } = await click(c, { url: null, text: "hello", title: null });
  assertDeepEq(c.copied, ["hello"]);
  assertEq(live.textContent, "Copied");
});

test("a copy that fails is failed(.denied), or .unsupported without a clipboard", async () => {
  const fired = [];
  const denied = container(false);
  denied.copyError = unitCase(1);
  await click(denied, link, { on$failed: (e) => fired.push(e) });
  const none = container(false);
  none.copyError = unitCase(0);
  await click(none, link, { on$failed: (e) => fired.push(e) });
  assertDeepEq(fired, [unitCase(0), unitCase(1)]);
});

test("the server renders the button whether or not the browser can share", () => {
  const out = new Out();
  natives["k.device.ShareButton$ssr"]({ content: () => link, label: () => "Share recipe" }, {}, out);
  assert(out.html().startsWith('<span class="k-status-button"><button type="button" aria-label="Share recipe">Share recipe</button><span role="status"'), out.html());
});
