/**
 * @module k:/tests/observe.test.js
 *
 * The `appeared`, `disappeared` and `resized` element events (docs/design/45-web-platform.md,
 * "Element events"; docs/compiler/runtime.md §12.9) against scripted `IntersectionObserver` and
 * `ResizeObserver`s written from the Intersection Observer and Resize Observer standards: an
 * observer reports every target once when it starts observing (not intersecting, for an element
 * off screen), then on each threshold crossing; `intersectionRatio` below the threshold is not
 * visible; `rootMargin` and `threshold` are the options it was made with; a resize observer reports
 * the content box (`contentRect`) once after observing and then on each change.
 */
import { assert, assertDeepEq, assertEq, installDom, test } from "k:testing";
import { observeElement } from "k:/runtime/observe.js";
import { root } from "k:runtime";

/**
 * Scripted observer classes on `globalThis`: `made` records each observer's options, and
 * `report(el, entry)` calls every observer of the given kind watching `el`, as the browser does
 * after layout. Returns them with `restore()`.
 */
function fakeObservers() {
  const saved = [globalThis.IntersectionObserver, globalThis.ResizeObserver];
  const made = [];
  const watched = new Map();
  class Fake {
    constructor(callback, options) {
      this.callback = callback;
      this.options = options;
      made.push(this);
    }
    observe(el) {
      if (!watched.has(el)) watched.set(el, new Set());
      watched.get(el).add(this);
    }
    unobserve(el) {
      watched.get(el)?.delete(this);
    }
    disconnect() {}
  }
  const IO = class extends Fake {};
  const RO = class extends Fake {};
  globalThis.IntersectionObserver = IO;
  globalThis.ResizeObserver = RO;
  const report = (C, el, entry) => {
    for (const o of [...(watched.get(el) ?? [])]) if (o instanceof C) o.callback([{ target: el, ...entry }], o);
  };
  return {
    made,
    watched,
    intersect: (el, ratio) => report(IO, el, { isIntersecting: ratio > 0, intersectionRatio: ratio }),
    resize: (el, width, height) => report(RO, el, { contentRect: { width, height } }),
    restore() {
      [globalThis.IntersectionObserver, globalThis.ResizeObserver] = saved;
    },
  };
}

/** A fresh element in a fresh fake document. */
function element() {
  const doc = installDom();
  const el = doc.createElement("p");
  doc.body.appendChild(el);
  return el;
}

test("appeared fires on each entry, not continuously, and disappeared on each exit", () => {
  const f = fakeObservers();
  try {
    const el = element();
    const seen = [];
    root(() => {
      observeElement(el, "appeared", () => seen.push("in"), null, null);
      observeElement(el, "disappeared", () => seen.push("out"), null, null);
    });
    f.intersect(el, 0);
    f.intersect(el, 0.4);
    f.intersect(el, 1);
    f.intersect(el, 0);
    f.intersect(el, 0.2);
    assertDeepEq(seen, ["in", "out", "in"], "the first report of an element off screen fires nothing");
  } finally {
    f.restore();
  }
});

test("the threshold and margin are the observer's options, one observer per distinct set", () => {
  const f = fakeObservers();
  try {
    const a = element();
    const b = element();
    const c = element();
    root(() => {
      observeElement(a, "appeared", () => {}, 50, "400px");
      observeElement(b, "appeared", () => {}, 50, "400px");
      observeElement(c, "appeared", () => {}, null, null);
    });
    const io = f.made.filter((o) => o instanceof globalThis.IntersectionObserver);
    assertEq(io.length, 2);
    assertDeepEq(io.map((o) => o.options), [{ threshold: 0.5, rootMargin: "400px" }, { threshold: 0, rootMargin: "0px" }]);
  } finally {
    f.restore();
  }
});

test("less of the element than the threshold isn't visible", () => {
  const f = fakeObservers();
  try {
    const el = element();
    let n = 0;
    root(() => observeElement(el, "appeared", () => n++, 50, null));
    f.intersect(el, 0.25);
    assertEq(n, 0);
    f.intersect(el, 0.5);
    assertEq(n, 1);
  } finally {
    f.restore();
  }
});

test("resized passes the content box as pixel lengths", () => {
  const f = fakeObservers();
  try {
    const el = element();
    const sizes = [];
    root(() => observeElement(el, "resized", (s) => sizes.push(s)));
    f.resize(el, 320, 200.5);
    f.resize(el, 100, 50);
    assertDeepEq(sizes, [{ width: "320px", height: "200.5px" }, { width: "100px", height: "50px" }]);
    assertEq(f.made.filter((o) => o instanceof globalThis.ResizeObserver).length, 1, "one per page");
  } finally {
    f.restore();
  }
});

test("disposing the owner stops the handler and unobserves the element", () => {
  const f = fakeObservers();
  try {
    const el = element();
    const other = element();
    let n = 0;
    let dispose;
    root((d) => {
      dispose = d;
      observeElement(el, "appeared", () => n++, null, null);
    });
    root(() => observeElement(other, "appeared", () => {}, null, null));
    dispose();
    f.intersect(el, 1);
    assertEq(n, 0);
    assertEq(f.watched.get(el)?.size ?? 0, 0, "unobserved");
    assertEq(f.watched.get(other).size, 1, "the shared observer keeps its other elements");
  } finally {
    f.restore();
  }
});

test("an async handler's rejection with CANCELLED is dropped", async () => {
  const f = fakeObservers();
  try {
    const el = element();
    const { CANCELLED } = await import("k:runtime");
    let ran = false;
    root(() =>
      observeElement(el, "appeared", () => {
        ran = true;
        return Promise.reject(CANCELLED);
      }, null, null));
    f.intersect(el, 1);
    await null;
    assert(ran);
  } finally {
    f.restore();
  }
});

test("without observers nothing fires and nothing throws", () => {
  const f = fakeObservers();
  try {
    globalThis.IntersectionObserver = undefined;
    globalThis.ResizeObserver = undefined;
    const el = element();
    root(() => {
      observeElement(el, "appeared", () => {
        throw new Error("never");
      }, null, null);
      observeElement(el, "resized", () => {
        throw new Error("never");
      });
    });
  } finally {
    f.restore();
  }
});
