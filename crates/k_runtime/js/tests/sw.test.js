/**
 * @module k:/tests/sw.test.js
 *
 * The service worker `k-sw.js` (docs/design/45-web-platform.md, "Notifications";
 * docs/compiler/runtime.md §12.12) in a scripted `ServiceWorkerGlobalScope`, written from the
 * Service Workers and Notifications standards: `skipWaiting` on install and `clients.claim` on
 * activate; `notificationclick` closing the notification; `clients.matchAll({type: "window",
 * includeUncontrolled: true})`, which lists the most recently focused tab first in Chromium;
 * `WindowClient.focus`, which rejects once the click's activation has lapsed; `postMessage` with a
 * transferred port; `WindowClient.navigate`, which rejects for a tab the worker doesn't control;
 * and `clients.openWindow` when the app has no tab. Each test runs in an isolate of its own, so
 * the worker's script is evaluated once per test, against that test's scope.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";

/**
 * A tab: `focused`, whether it `answers` the worker's message on its port (it loaded the
 * notifications chunk), whether it is `controlled` (else `navigate` rejects), whether `focus`
 * fails. `log` records what happened to it.
 */
function tab(url, { focused = false, answers = true, controlled = true, focusFails = false } = {}) {
  const t = { url, focused, log: [] };
  t.focus = async () => {
    if (focusFails) throw new Error("InvalidAccessError");
    t.log.push("focus");
    return t;
  };
  t.postMessage = (data, ports) => {
    t.log.push(["message", data]);
    if (answers) queueMicrotask(() => ports[0].peer.onmessage?.({ data: true }));
  };
  t.navigate = async (to) => {
    if (!controlled) throw new TypeError("not controlled");
    t.log.push(["navigate", to]);
    return t;
  };
  return t;
}

/**
 * Installs a scripted worker scope with `tabs` open, evaluates `k-sw.js` in it and returns the
 * scope: `dispatch(type, event)` runs the listeners and waits for `waitUntil`; `opened` records
 * `openWindow`; `timers` holds the pending `setTimeout` callbacks, which `expire()` runs.
 */
async function worker(tabs) {
  const listeners = {};
  const scope = {
    opened: [],
    skipped: 0,
    claimed: 0,
    timers: [],
    addEventListener: (type, f) => (listeners[type] ??= []).push(f),
    skipWaiting: async () => {
      scope.skipped++;
    },
    clients: {
      matchAll: async (options) => {
        scope.matched = options;
        return tabs;
      },
      openWindow: async (url) => {
        scope.opened.push(url);
        return null;
      },
      claim: async () => {
        scope.claimed++;
      },
    },
  };
  scope.dispatch = async (type, event = {}) => {
    const waits = [];
    for (const f of listeners[type] ?? []) f({ ...event, waitUntil: (p) => waits.push(p) });
    await Promise.all(waits);
  };
  scope.expire = () => scope.timers.splice(0).forEach((f) => f());
  globalThis.self = scope;
  globalThis.setTimeout = (f) => scope.timers.push(f);
  globalThis.MessageChannel = class {
    constructor() {
      this.port1 = { onmessage: null };
      this.port2 = { peer: this.port1 };
    }
  };
  await import("k:/runtime/sw.js");
  return scope;
}

/** A `notificationclick` for a notification with `data`, which records its `close`. */
function click(data) {
  const notification = { data, closed: false, close: () => (notification.closed = true) };
  return { notification };
}

test("the worker takes over at once: skipWaiting on install, clients.claim on activate", async () => {
  const sw = await worker([]);
  await sw.dispatch("install");
  await sw.dispatch("activate");
  assertEq(sw.skipped, 1);
  assertEq(sw.claimed, 1);
});

test("a click closes the notification, focuses the focused tab and lets its router open the page", async () => {
  const other = tab("https://app.example/a");
  const current = tab("https://app.example/b", { focused: true });
  const sw = await worker([other, current]);
  const e = click({ url: "/reports/7", from: "/b" });
  await sw.dispatch("notificationclick", e);
  assert(e.notification.closed);
  assertDeepEq(sw.matched, { type: "window", includeUncontrolled: true });
  assertDeepEq(current.log, ["focus", ["message", { k: "open", url: "/reports/7" }]]);
  assertDeepEq(other.log, [], "only one tab");
  assertDeepEq(sw.opened, []);
});

test("without a focused tab the most recent one is used", async () => {
  const recent = tab("https://app.example/a");
  const sw = await worker([recent, tab("https://app.example/b")]);
  await sw.dispatch("notificationclick", click({ url: "/reports/7", from: "/" }));
  assertEq(recent.log[0], "focus");
});

test("a tab that doesn't answer within a second is navigated by the worker", async () => {
  const silent = tab("https://app.example/a", { answers: false });
  const sw = await worker([silent]);
  const done = sw.dispatch("notificationclick", click({ url: "/reports/7", from: "/" }));
  await tick();
  assertEq(sw.timers.length, 1, "waiting for the answer");
  sw.expire();
  await done;
  assertDeepEq(silent.log.at(-1), ["navigate", "/reports/7"]);
});

test("an uncontrolled tab that doesn't answer: the page opens in a new tab", async () => {
  const silent = tab("https://app.example/a", { answers: false, controlled: false });
  const sw = await worker([silent]);
  const done = sw.dispatch("notificationclick", click({ url: "/reports/7", from: "/" }));
  await tick();
  sw.expire();
  await done;
  assertDeepEq(sw.opened, ["/reports/7"]);
});

test("a notification without a page only focuses the tab", async () => {
  const current = tab("https://app.example/b", { focused: true });
  const sw = await worker([current]);
  await sw.dispatch("notificationclick", click({ url: null, from: "/b" }));
  assertDeepEq(current.log, ["focus"]);
  assertDeepEq(sw.opened, []);
});

test("with no tab open, a new one opens at the page, or at the page that showed it", async () => {
  const sw = await worker([]);
  await sw.dispatch("notificationclick", click({ url: "/reports/7", from: "/b" }));
  await sw.dispatch("notificationclick", click({ url: null, from: "/b" }));
  await sw.dispatch("notificationclick", click(null));
  assertDeepEq(sw.opened, ["/reports/7", "/b", "/"]);
});

test("a focus the browser refuses still opens the page", async () => {
  const current = tab("https://app.example/b", { focused: true, focusFails: true });
  const sw = await worker([current]);
  await sw.dispatch("notificationclick", click({ url: "/reports/7", from: "/" }));
  assertDeepEq(current.log, [["message", { k: "open", url: "/reports/7" }]]);
});
