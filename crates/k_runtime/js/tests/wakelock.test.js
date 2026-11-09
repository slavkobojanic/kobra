/**
 * @module k:/tests/wakelock.test.js
 *
 * `k.device.KeepScreenAwake` (docs/design/45-web-platform.md, "KeepScreenAwake";
 * docs/compiler/runtime.md §12.11) against a scripted `navigator.wakeLock` written from the Screen
 * Wake Lock standard: `request("screen")` resolves with a `WakeLockSentinel` only in a secure
 * context and while the document is visible, and rejects with `NotAllowedError` otherwise (also for a
 * policy or the user agent's battery saver, as Chromium does); the user agent releases every sentinel
 * when the page is hidden, firing `release`; `release()` fires it too. An engine without the API
 * has no `navigator.wakeLock`. The component renders an empty comment, and in `k test` holds the
 * container's `WakeLock` (the kit's `TestWakeLock`) instead.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { KeepScreenAwake } from "k:/runtime/wakelock.js";
import { Out, natives } from "k:runtime/server";
import { flush, root, unitCase } from "k:runtime";
import * as kit from "k:testing/kit";

/**
 * A scripted browser: `refuse` makes requests reject with `NotAllowedError`; `held` counts the
 * sentinels not yet released; `requests` counts calls. `hide()` and `show()` change the document's
 * visibility as switching tabs does.
 */
function fakeBrowser({ secure = true, api = true } = {}) {
  const fake = { refuse: false, held: 0, requests: 0, sentinels: new Set() };
  globalThis.isSecureContext = secure;
  let state = "visible";
  Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
  // The fake DOM's document has no events of its own: `visibilitychange` here.
  const listeners = new Set();
  document.addEventListener = (type, f) => type === "visibilitychange" && listeners.add(f);
  document.removeEventListener = (type, f) => listeners.delete(f);
  const changed = () => {
    for (const f of [...listeners]) f(new Event("visibilitychange"));
  };
  const release = (s) => {
    if (s.released) return;
    s.released = true;
    fake.held--;
    fake.sentinels.delete(s);
    s.onrelease?.({});
  };
  globalThis.navigator = api
    ? {
        wakeLock: {
          async request(type) {
            fake.requests++;
            assertEq(type, "screen");
            if (fake.refuse || state !== "visible") throw Object.assign(new Error("refused"), { name: "NotAllowedError" });
            const s = { released: false, onrelease: null, release: async () => release(s) };
            fake.held++;
            fake.sentinels.add(s);
            return s;
          },
        },
      }
    : {};
  fake.hide = () => {
    state = "hidden";
    for (const s of [...fake.sentinels]) release(s);
    changed();
  };
  fake.show = () => {
    state = "visible";
    changed();
  };
  return fake;
}

/** Mounts a `KeepScreenAwake` with container `c`, recording `failed`; returns the failures, the node and the disposer. */
function mount(c = {}) {
  const failures = [];
  let node = null;
  let dispose = null;
  root((d) => {
    dispose = d;
    node = KeepScreenAwake({ on$failed: (f) => failures.push(f) }, { c });
    document.body.appendChild(node);
  });
  flush();
  return { failures, node, dispose };
}

test("while mounted it holds the screen's wake lock and renders nothing", async () => {
  const fake = fakeBrowser();
  const m = mount();
  await tick();
  assertEq(fake.held, 1);
  assertEq(m.node.nodeType, 8, "an empty comment");
  m.dispose();
  await tick();
  assertEq(fake.held, 0, "unmounting releases it");
});

test("hidden, the browser releases the lock; shown again, it asks again", async () => {
  const fake = fakeBrowser();
  const m = mount();
  await tick();
  fake.hide();
  await tick();
  assertEq(fake.held, 0);
  assertEq(fake.requests, 1, "it doesn't ask while hidden");
  fake.show();
  await tick();
  assertEq(fake.held, 1);
  assertEq(fake.requests, 2);
  assertDeepEq(m.failures, []);
  m.dispose();
  await tick();
  fake.show();
  await tick();
  assertEq(fake.requests, 2, "unmounted, it no longer listens");
});

test("unmounted while the request is pending, the lock is released when it comes", async () => {
  const fake = fakeBrowser();
  const m = mount();
  m.dispose();
  await tick();
  assertEq(fake.held, 0);
  assertEq(m.failures.length, 0);
});

test("a refusal (battery saver, a policy) fires failed(.unavailable)", async () => {
  const fake = fakeBrowser();
  fake.refuse = true;
  const m = mount();
  await tick();
  assertDeepEq(m.failures, [unitCase(1)]);
  m.dispose();
});

test("no wake lock API, or an insecure context, fires failed(.unsupported) without asking", async () => {
  fakeBrowser({ api: false });
  const a = mount();
  await tick();
  assertDeepEq(a.failures, [unitCase(0)]);
  a.dispose();
  const fake = fakeBrowser({ secure: false });
  const b = mount();
  await tick();
  assertDeepEq(b.failures, [unitCase(0)]);
  assertEq(fake.requests, 0);
  b.dispose();
});

test("in k test it holds the container's WakeLock, which can refuse", async () => {
  fakeBrowser();
  const lock = kit.testWakeLock();
  const m = mount({ WakeLock: lock });
  assert(lock.isHeld, "held at once");
  m.dispose();
  assert(!lock.isHeld, "released with the component");
  lock.refuse(unitCase(1));
  const n = mount({ WakeLock: lock });
  assert(!lock.isHeld);
  assertDeepEq(n.failures, [unitCase(1)]);
  n.dispose();
});

test("the server renders nothing for it", () => {
  const out = new Out();
  natives["k.device.KeepScreenAwake$ssr"]({}, {}, out);
  assertEq(out.chunks.join(""), "");
});
