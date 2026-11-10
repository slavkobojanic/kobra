/**
 * @module k:/tests/offline.test.js
 *
 * An `@Offline` app's page side, `startOffline` and `k.device.AppUpdates` (docs/design/54-offline.md;
 * docs/compiler/runtime.md §12.20), against a scripted `navigator.serviceWorker`, written from the
 * Service Workers standard: no `serviceWorker` outside a secure context; `register` resolving to a
 * registration whose `update()` checks for a new worker; `controller`, the worker that controls the
 * page (`null` on the first visit) and `controllerchange` when another takes over; a worker's
 * `postMessage` with a transferred port, which the worker answers on; and the page's `load`.
 * Each test runs in an isolate of its own, so the chunk's state is fresh.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { AppUpdates, startOffline } from "k:/runtime/offline.js";
import { computed, flush, root } from "k:runtime";

/**
 * `URL`, which the test isolate doesn't have: enough of it for absolute and root-relative URLs
 * (`href`, `origin`, `pathname`).
 */
globalThis.URL ??= class {
  constructor(u, base = "") {
    const abs = /^[a-z]+:/.test(u) ? u : base.replace(/\/$/, "") + u;
    const m = /^([a-z]+:\/\/[^/?#]+)([^?#]*)/.exec(abs);
    this.href = abs;
    this.origin = m[1];
    this.pathname = m[2] || "/";
  }
};

const ENTRY = "https://app.example/assets/app.a1.js";

/** A worker that caches `build` (`null`: a development worker, which doesn't answer). */
function controller(build) {
  return {
    asked: 0,
    postMessage(data, ports) {
      this.asked++;
      if (data?.k === "build" && build !== null) queueMicrotask(() => ports[0].peer.onmessage?.({ data: build }));
    },
  };
}

/**
 * A scripted browser: `controller` is the page's worker; `loaded` whether the page's `load` has
 * fired. `registers` records each `register`, `updates` each `update()`; `takeOver(c)` makes `c`
 * the controller and fires `controllerchange`; `load()` and `show()` fire `load` and a
 * `visibilitychange` to visible.
 */
function fakeBrowser({ secure = true, current = null, loaded = true } = {}) {
  const listeners = {};
  const on = (type, f) => (listeners[type] ??= []).push(f);
  const fire = (type) => (listeners[type] ?? []).forEach((f) => f({}));
  const fake = { registers: [], updates: 0, reloads: 0 };
  const registration = { update: async () => fake.updates++ };
  const sw = {
    controller: current,
    register: async (url, options) => {
      fake.registers.push({ url, ...options });
      return registration;
    },
    addEventListener: (type, f) => on(`sw:${type}`, f),
  };
  const document = { readyState: loaded ? "complete" : "loading", visibilityState: "visible", addEventListener: (t, f) => on(`doc:${t}`, f) };
  globalThis.isSecureContext = secure;
  globalThis.navigator = secure ? { serviceWorker: sw } : {};
  globalThis.document = document;
  globalThis.addEventListener = (t, f) => on(`win:${t}`, f);
  globalThis.location = { reload: () => fake.reloads++ };
  globalThis.MessageChannel = class {
    constructor() {
      this.port1 = { onmessage: null };
      this.port2 = { peer: this.port1 };
    }
  };
  fake.takeOver = (c) => {
    sw.controller = c;
    fire("sw:controllerchange");
  };
  fake.load = () => {
    document.readyState = "complete";
    fire("win:load");
  };
  fake.show = () => fire("doc:visibilitychange");
  return fake;
}

/** `AppUpdates().isAvailable`, read through a computed so its reactivity is checked too. */
function watch() {
  const updates = AppUpdates();
  let read;
  root(() => {
    read = computed(() => updates.isAvailable);
  });
  return () => read.get();
}

test("the worker is registered at the root, without the HTTP cache, once the page has loaded", async () => {
  const fake = fakeBrowser({ loaded: false });
  startOffline(ENTRY);
  await tick();
  assertDeepEq(fake.registers, [], "not before load");
  fake.load();
  await tick();
  assertDeepEq(fake.registers, [{ url: "/k-sw.js", scope: "/", updateViaCache: "none" }]);
  startOffline(ENTRY);
  await tick();
  assertEq(fake.registers.length, 1, "once per page");
});

test("without service workers (outside a secure context) nothing happens and no update is reported", async () => {
  fakeBrowser({ secure: false });
  startOffline(ENTRY);
  await tick();
  assertEq(AppUpdates().isAvailable, false);
});

test("a worker caching the page's own build reports no update", async () => {
  const worker = controller("/assets/app.a1.js");
  fakeBrowser({ current: worker });
  const available = watch();
  startOffline(ENTRY);
  await tick();
  flush();
  assertEq(worker.asked, 1);
  assertEq(available(), false);
});

test("a page opened from an older build learns at once that a newer one has taken over", async () => {
  fakeBrowser({ current: controller("/assets/app.b2.js") });
  const available = watch();
  startOffline(ENTRY);
  await tick();
  flush();
  assertEq(available(), true);
});

test("a newer build's worker taking over while the page is open makes an update available", async () => {
  const fake = fakeBrowser({ current: controller("/assets/app.a1.js") });
  const available = watch();
  startOffline(ENTRY);
  await tick();
  flush();
  assertEq(available(), false);
  fake.takeOver(controller("/assets/app.b2.js"));
  await tick();
  flush();
  assertEq(available(), true);
});

test("the first worker taking over a page, and a development worker, report nothing", async () => {
  const fake = fakeBrowser({ current: null });
  const available = watch();
  startOffline(ENTRY);
  fake.takeOver(controller("/assets/app.a1.js"));
  await tick();
  fake.takeOver(controller(null));
  await tick();
  flush();
  assertEq(available(), false);
});

test("the page asks the browser to look for a new build whenever it becomes visible again", async () => {
  const fake = fakeBrowser();
  startOffline(ENTRY);
  await tick();
  fake.show();
  await tick();
  assertEq(fake.updates, 1);
});

test("apply() reloads, so the new worker answers with the new build", async () => {
  const fake = fakeBrowser();
  AppUpdates().apply();
  assertEq(fake.reloads, 1);
  assert(true);
});
