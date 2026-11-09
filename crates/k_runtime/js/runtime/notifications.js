/**
 * @module k:runtime/notifications
 *
 * `k.device.BrowserNotifications` in the browser
 * (docs/design/45-web-platform.md, "Notifications"; docs/compiler/runtime.md
 * §12.12): notifications shown through the app's service worker,
 * `/k-sw.js`, which `k build` emits for apps that ship this chunk
 * (codegen.md §19), and the permission from the Notifications API, kept
 * in a signal so views that read it follow the site's settings.
 *
 * The enums are the stdlib's cases: `PermissionState` `unitCase(0)`
 * `prompt`, `1` `granted`, `2` `denied`; `NotificationError` `0`
 * `unsupported`, `1` `notAsked`, `2` `denied`.
 *
 * Shipped as its own client chunk, only to apps whose client notifies
 * (codegen.md §16), and measured against its own size budget.
 */

import { navigate } from "./router.js";
import { afterHydration } from "./services.js";
import { signal } from "./signals.js";
import { unitCase } from "./values.js";

/** The service worker's URL, which is also its registration's key. */
const WORKER = "/k-sw.js";

/** `PermissionState` by `Notification.permission` (`default` asks). */
const NOTIFY_STATES = { default: 0, granted: 1, denied: 2 };

/**
 * The browser's `k.device.BrowserNotifications`. `isSupported` is constant
 * for the page's life; `permission` is reactive, `.prompt` while the page
 * hydrates, then `Notification.permission`, re-read on the Permissions
 * API's `change` event where the engine fires it and when the page
 * becomes visible again where it doesn't.
 *
 * The service worker is registered (scope `/`, `updateViaCache: "none"`)
 * on the first `show`, or at once when the permission is already granted,
 * so a click on a notification shown before a reload still finds it. A
 * click focuses a tab of the app and posts it `{k: "open", url}`; the tab
 * that hears it navigates with the router and answers on the message's
 * port, and the worker loads the URL itself when no tab answers.
 * @param {((page: object) => string)|null} href the app's `Page$href`
 * @returns {object}
 */
export function BrowserNotifications(href) {
  const g = globalThis;
  const container = () =>
    g.isSecureContext === true && typeof g.Notification === "function" ? g.navigator?.serviceWorker : undefined;
  const state = signal(unitCase(0));
  const read = () => {
    const s = NOTIFY_STATES[g.Notification?.permission] ?? 0;
    if (state.peek().$ !== s) state.set(unitCase(s));
    return s;
  };
  let ready = null;
  /** The active registration, registering the worker the first time. */
  const registration = () =>
    (ready ??= container()
      .register(WORKER, { scope: "/", updateViaCache: "none" })
      .then(() => container().ready));
  const sw = container();
  if (sw) {
    afterHydration(() => read() === 1 && registration().catch(() => (ready = null)));
    g.navigator.permissions?.query?.({ name: "notifications" }).then(
      (status) => (status.onchange = read),
      () => {},
    );
    g.document?.addEventListener?.("visibilitychange", () => g.document.visibilityState === "visible" && read());
    sw.addEventListener?.("message", (e) => {
      if (e.data?.k !== "open") return;
      navigate(e.data.url);
      e.ports?.[0]?.postMessage(true);
    });
  }
  return {
    get isSupported() {
      return typeof container()?.register === "function";
    },
    get permission() {
      return state.get();
    },
    async requestAccess(_c) {
      if (!container()) return state.peek();
      if (read() !== 0) return state.peek();
      // Chromium's quieter prompt may answer `default` without asking.
      await g.Notification.requestPermission();
      read();
      return state.peek();
    },
    async show(_c, n) {
      if (!container()) throw unitCase(0);
      const now = read();
      if (now !== 1) throw unitCase(now === 0 ? 1 : 2);
      let reg;
      try {
        reg = await registration();
      } catch {
        // No worker here (Gecko's private windows): nothing can show it.
        ready = null;
        throw unitCase(0);
      }
      const l = g.location;
      const data = { url: n.opens == null ? null : href(n.opens), from: l ? l.pathname + l.search : "/" };
      try {
        await reg.showNotification(n.title, { body: n.body ?? undefined, tag: n.tag ?? undefined, data });
      } catch {
        // The permission was revoked between the check and the call.
        read();
        throw unitCase(2);
      }
    },
  };
}
