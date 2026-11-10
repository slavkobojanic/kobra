/**
 * @module k:runtime/offline
 *
 * An `@Offline` app in the browser (docs/design/54-offline.md;
 * docs/compiler/runtime.md §12.20): registering its service worker,
 * `/k-sw.js`, and `k.device.AppUpdates`.
 *
 * The entry of an `@Offline` app imports this chunk once the page has
 * loaded, `import("./offline.<hash>.js").then((m) => m.startOffline(
 * import.meta.url))`, so it costs the first render nothing; an app that
 * injects `AppUpdates` imports it with the entry instead, and both are the
 * same module.
 *
 * Shipped as its own client chunk, only to `@Offline` apps (codegen.md
 * §16), and measured against its own size budget.
 */

import { signal } from "./signals.js";

/** Whether a newer build than the page's has taken over. */
const updateReady = signal(false);

/** Whether `startOffline` has run in this page. */
let offlineStarted = false;

/**
 * Registers the worker (scope `/`, `updateViaCache: "none"`, as the
 * notifications chunk does) after the page's `load`, and asks the worker
 * that controls the page which build it caches, now and whenever another
 * worker takes over (`controllerchange`): an answer other than `entry`'s
 * path means a newer build is ready. The browser checks for a new worker
 * on navigations; the page also asks it to whenever it becomes visible
 * again, so a tab left open finds a deploy. Does nothing without service
 * workers or outside a secure context.
 * @param {string} entry the URL of the page's entry chunk
 */
export function startOffline(entry) {
  const g = globalThis;
  const sw = g.navigator?.serviceWorker;
  if (offlineStarted || g.isSecureContext !== true || typeof sw?.register !== "function") return;
  offlineStarted = true;
  const own = new URL(entry).pathname;
  const ask = () => {
    const c = sw.controller;
    if (!c) return;
    const channel = new MessageChannel();
    channel.port1.onmessage = (e) => typeof e.data === "string" && e.data !== own && updateReady.set(true);
    c.postMessage({ k: "build" }, [channel.port2]);
  };
  sw.addEventListener("controllerchange", ask);
  ask();
  const register = () => {
    const ready = sw.register("/k-sw.js", { scope: "/", updateViaCache: "none" });
    ready.catch(() => {});
    g.document?.addEventListener?.("visibilitychange", () => {
      if (g.document.visibilityState === "visible") ready.then((r) => r.update()).catch(() => {});
    });
  };
  if (g.document?.readyState === "complete") register();
  else g.addEventListener("load", register, { once: true });
}

/**
 * The browser's `k.device.AppUpdates`: `isAvailable` is the signal
 * `startOffline` sets; `apply()` reloads, and the new worker answers the
 * navigation with the new build.
 * @returns {{isAvailable: boolean, apply: () => void}}
 */
export function AppUpdates() {
  return {
    get isAvailable() {
      return updateReady.get();
    },
    apply() {
      globalThis.location.reload();
    },
  };
}
