/**
 * @module k:/tests/environment.test.js
 *
 * `k.device`'s environment services (docs/design/45-web-platform.md, "Page visibility",
 * "Connectivity", "Preferences", "Viewport"; docs/compiler/runtime.md §12.8) against a scripted
 * browser written from the Page Visibility, HTML (`navigator.onLine`, `online`/`offline`, `pageshow`),
 * CSSOM View (`innerWidth`, `matchMedia` and its `change` event) and Fetch standards: a
 * `visibilitychange` that flips `document.visibilityState`, a page restored from the back-forward
 * cache, `onLine` changing with its events, a `fetch` that rejects with a `TypeError` without a
 * network and with an `AbortError` when aborted, media queries that fire `change`, and `resize` events
 * that arrive many times per frame. While the first page hydrates each member answers the server's
 * value.
 */
import { assert, assertDeepEq, assertEq, installDom, test, tick } from "k:testing";
import { Connectivity, PageVisibility, Preferences, Viewport } from "k:/runtime/environment.js";
import { effect, flush, hydrate, root, rpc, template, unitCase } from "k:runtime";

/** A minimal `EventTarget` whose `fire(type)` dispatches to its listeners. */
function target(props = {}) {
  const listeners = new Map();
  return Object.assign(props, {
    addEventListener(type, f) {
      if (!listeners.has(type)) listeners.set(type, []);
      listeners.get(type).push(f);
    },
    fire(type, e = {}) {
      for (const f of listeners.get(type) ?? []) f(e);
    },
  });
}

/**
 * A scripted browser on `globalThis` (over a fresh fake DOM): `document.visibilityState`, `navigator.onLine`, the window's
 * events, `innerWidth`/`innerHeight`, `matchMedia` over the `media` set (each query a target whose
 * `matches` reads it), `requestAnimationFrame` queued until `frame()`, and `fetch` answered by
 * `answer(url, init)`. Restores the globals with `restore()`.
 */
function fakeBrowser() {
  const saved = {};
  for (const k of ["navigator", "addEventListener", "innerWidth", "innerHeight", "matchMedia", "requestAnimationFrame", "fetch"]) {
    saved[k] = Object.getOwnPropertyDescriptor(globalThis, k);
  }
  const b = {
    media: new Set(),
    queries: new Map(),
    frames: [],
    answer: () => Promise.resolve({ ok: true, status: 200, headers: { get: () => null }, text: async () => "{}" }),
  };
  // The fake DOM's document, so hydration has one, with a visibility state and its own events.
  installDom();
  b.document = target(globalThis.document);
  b.document.visibilityState = "visible";
  b.navigator = { onLine: true };
  b.window = target();
  const define = (k, v) => Object.defineProperty(globalThis, k, { configurable: true, writable: true, value: v });
  define("navigator", b.navigator);
  define("addEventListener", b.window.addEventListener);
  define("innerWidth", 1280);
  define("innerHeight", 800);
  define("matchMedia", (q) => {
    if (!b.queries.has(q)) {
      const m = target();
      Object.defineProperty(m, "matches", { get: () => b.media.has(q) });
      b.queries.set(q, m);
    }
    return b.queries.get(q);
  });
  define("requestAnimationFrame", (f) => b.frames.push(f));
  define("fetch", (url, init) => b.answer(url, init));
  /** Sets the media queries that match, firing `change` on those that changed. */
  b.setMedia = (...queries) => {
    const before = new Set(b.media);
    b.media = new Set(queries);
    for (const [q, m] of b.queries) if (before.has(q) !== b.media.has(q)) m.fire("change");
  };
  /** Runs the queued animation frames. */
  b.frame = () => {
    const fs = b.frames.splice(0);
    for (const f of fs) f(0);
  };
  b.restore = () => {
    for (const [k, d] of Object.entries(saved)) {
      if (d) Object.defineProperty(globalThis, k, d);
      else delete globalThis[k];
    }
  };
  return b;
}

/** Runs `fn` inside a hydration pass over an empty container, returning its result. */
function hydrating(fn) {
  const doc = globalThis.document;
  const container = doc.createElement("div");
  doc.body.appendChild(container);
  let out;
  hydrate(container, () => {
    out = fn();
    return template("<p></p>")();
  }, null);
  return out;
}

/** Runs `read` in an effect, returning the values it saw. */
function follow(read) {
  const seen = [];
  root(() => effect(() => seen.push(read())));
  flush();
  return seen;
}

test("PageVisibility follows visibilitychange", () => {
  const b = fakeBrowser();
  try {
    const v = PageVisibility();
    const seen = follow(() => v.isVisible);
    b.document.visibilityState = "hidden";
    b.document.fire("visibilitychange");
    flush();
    b.document.visibilityState = "visible";
    b.document.fire("visibilitychange");
    flush();
    assertDeepEq(seen, [true, false, true]);
  } finally {
    b.restore();
  }
});

test("PageVisibility re-reads the state when the page is restored from the back-forward cache", () => {
  const b = fakeBrowser();
  try {
    b.document.visibilityState = "hidden";
    const v = PageVisibility();
    assertEq(v.isVisible, false);
    b.document.visibilityState = "visible";
    b.window.fire("pageshow", { persisted: true });
    assertEq(v.isVisible, true);
  } finally {
    b.restore();
  }
});

test("PageVisibility answers visible while hydrating, then the browser's state", async () => {
  const b = fakeBrowser();
  try {
    b.document.visibilityState = "hidden";
    const v = hydrating(() => {
      const s = PageVisibility();
      assertEq(s.isVisible, true, "the server's value during the pass");
      return s;
    });
    await tick();
    assertEq(v.isVisible, false);
  } finally {
    b.restore();
  }
});

test("Connectivity follows navigator.onLine and its events", () => {
  const b = fakeBrowser();
  try {
    b.navigator.onLine = false;
    const c = Connectivity();
    const seen = follow(() => c.isOnline);
    b.navigator.onLine = true;
    b.window.fire("online");
    flush();
    b.navigator.onLine = false;
    b.window.fire("offline");
    flush();
    assertDeepEq(seen, [false, true, false]);
  } finally {
    b.restore();
  }
});

test("Connectivity answers online while hydrating", async () => {
  const b = fakeBrowser();
  try {
    b.navigator.onLine = false;
    const c = hydrating(() => {
      const s = Connectivity();
      assertEq(s.isOnline, true);
      return s;
    });
    await tick();
    assertEq(c.isOnline, false);
  } finally {
    b.restore();
  }
});

test("an rpc that fails for want of a network takes the app offline until one succeeds", async () => {
  const b = fakeBrowser();
  try {
    const c = Connectivity();
    b.answer = () => Promise.reject(new TypeError("Failed to fetch"));
    const failed = await rpc("/notes.NotesProtocol/List", {}, null);
    assertEq(failed.kind, "offline");
    assertEq(c.isOnline, false, "navigator.onLine still says true, but the rpc knows better");
    b.answer = () => Promise.resolve({ ok: true, status: 200, headers: { get: () => null }, text: async () => "{}" });
    const ok = await rpc("/notes.NotesProtocol/List", {}, null);
    assert(ok.ok);
    assertEq(c.isOnline, true);
  } finally {
    b.restore();
  }
});

test("the browser's online event brings the app back after a failed rpc", async () => {
  const b = fakeBrowser();
  try {
    const c = Connectivity();
    b.answer = () => Promise.reject(new TypeError("Failed to fetch"));
    await rpc("/a.B/C", {}, null);
    assertEq(c.isOnline, false);
    b.window.fire("online");
    assertEq(c.isOnline, true);
  } finally {
    b.restore();
  }
});

test("an aborted request and another origin's failure aren't evidence of being offline", async () => {
  const b = fakeBrowser();
  try {
    const c = Connectivity();
    const abort = new Error("aborted");
    abort.name = "AbortError";
    b.answer = () => Promise.reject(abort);
    await globalThis.fetch("/a.B/C").catch(() => null);
    assertEq(c.isOnline, true, "an abort");
    b.answer = () => Promise.reject(new TypeError("CORS"));
    await globalThis.fetch("https://api.example.com/v1").catch(() => null);
    assertEq(c.isOnline, true, "another origin");
  } finally {
    b.restore();
  }
});

test("Preferences follow their media queries", () => {
  const b = fakeBrowser();
  try {
    b.setMedia("(prefers-reduced-motion: reduce)");
    const p = Preferences(null);
    assertEq(p.prefersReducedMotion, true);
    assertEq(p.prefersDark, false);
    const seen = follow(() => [p.prefersDark, p.prefersMoreContrast, p.forcedColors]);
    b.setMedia("(prefers-color-scheme: dark)", "(prefers-contrast: more)", "(forced-colors: active)");
    flush();
    assertDeepEq(seen.at(-1), [true, true, true]);
    assertEq(p.prefersReducedMotion, false);
  } finally {
    b.restore();
  }
});

test("Preferences keep the server's client-hint values while hydrating", async () => {
  const b = fakeBrowser();
  try {
    b.setMedia("(prefers-contrast: more)");
    const p = hydrating(() => {
      const s = Preferences({ p: [true, false] });
      assertDeepEq([s.prefersDark, s.prefersReducedMotion, s.prefersMoreContrast], [true, false, false]);
      return s;
    });
    await tick();
    assertDeepEq([p.prefersDark, p.prefersReducedMotion, p.prefersMoreContrast], [false, false, true]);
  } finally {
    b.restore();
  }
});

test("Viewport reads the layout viewport, the pointer and hover", () => {
  const b = fakeBrowser();
  try {
    b.setMedia("(pointer: coarse)");
    const v = Viewport();
    assertDeepEq(v.size, { width: "1280px", height: "800px" });
    assertEq(v.pointer, unitCase(1));
    assertEq(v.canHover, false);
  } finally {
    b.restore();
  }
});

test("Viewport coalesces resizes to one update per animation frame", () => {
  const b = fakeBrowser();
  try {
    const v = Viewport();
    const seen = follow(() => v.size);
    for (const w of [1000, 800, 390]) {
      globalThis.innerWidth = w;
      globalThis.innerHeight = 844;
      b.window.fire("resize");
    }
    assertEq(b.frames.length, 1, "one frame requested");
    b.frame();
    flush();
    assertDeepEq(seen, [{ width: "1280px", height: "800px" }, { width: "390px", height: "844px" }]);
    b.window.fire("resize");
    b.frame();
    flush();
    assertEq(seen.length, 2, "an unchanged size updates nobody");
  } finally {
    b.restore();
  }
});

test("Viewport follows a change of the primary pointer", () => {
  const b = fakeBrowser();
  try {
    b.setMedia("(pointer: fine)", "(hover: hover)");
    const v = Viewport();
    assertEq(v.pointer, unitCase(0));
    b.setMedia("(pointer: coarse)");
    b.frame();
    assertEq(v.pointer, unitCase(1));
    assertEq(v.canHover, false);
  } finally {
    b.restore();
  }
});

test("Viewport is unknown while hydrating, as the server rendered it", async () => {
  const b = fakeBrowser();
  try {
    b.setMedia("(pointer: fine)");
    const v = hydrating(() => {
      const s = Viewport();
      assertDeepEq([s.size, s.pointer, s.canHover], [null, null, null]);
      return s;
    });
    await tick();
    assertDeepEq(v.size, { width: "1280px", height: "800px" });
    assertEq(v.pointer, unitCase(0));
  } finally {
    b.restore();
  }
});
