/**
 * @module k:runtime/sw
 *
 * The service worker `k-sw.js` (docs/design/45-web-platform.md,
 * "Notifications"; docs/compiler/runtime.md §12.12), which `k build` emits
 * at the site's root for an app whose client ships the notifications
 * chunk (codegen.md §19). A classic worker script: no imports, no app
 * code, and no `fetch` handler, so it never stands between the app and
 * the network. An `@Offline` app's worker has `runtime/sw-offline.js`
 * before it (docs/design/54-offline.md), which caches the build and
 * answers its files and navigations; this file stays the same.
 *
 * - `install` and `activate` take over at once (`skipWaiting`,
 *   `clients.claim`), so a new build's worker replaces the old one without
 *   waiting for every tab to close, and every open tab of the app is a
 *   client it may navigate.
 * - `notificationclick` closes the notification and opens its page: it
 *   focuses the app's focused tab (else its most recent one) and posts it
 *   `{k: "open", url}`; the tab's notifications chunk navigates with the
 *   router and answers on the message's port. A tab that doesn't answer
 *   within a second (it never loaded the chunk) is navigated by the worker,
 *   which reloads it at the URL. With no tab open, a new one opens at the
 *   URL. A notification without a page (`url: null`) only focuses the tab,
 *   or opens the page that showed it (`from`).
 * - `push` (docs/design/50-push.md) shows the message `WebPush` sent, the
 *   JSON `{title, body, tag, url}` of a `SystemNotification` whose page
 *   the server turned into its path, with `from` `/`, so a click opens it
 *   as above. A message that isn't that JSON is shown with the site's
 *   host as its title: a push that shows nothing costs the site its push
 *   permission in Chromium.
 */

/** How long a tab has to say it opened the page itself, in milliseconds. */
const ANSWER_WITHIN = 1000;

self.addEventListener("install", () => self.skipWaiting());

self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));

self.addEventListener("notificationclick", (e) => {
  e.notification.close();
  const data = e.notification.data ?? {};
  e.waitUntil(open(data.url ?? null, data.from ?? "/"));
});

self.addEventListener("push", (e) => {
  let m = null;
  try {
    m = e.data ? e.data.json() : null;
  } catch {
    // Not the stdlib's JSON.
  }
  const ok = m !== null && typeof m === "object" && typeof m.title === "string";
  const title = ok ? m.title : self.registration.scope.split("/")[2];
  const options = { body: (ok && m.body) || undefined, tag: (ok && m.tag) || undefined, data: { url: (ok && m.url) || null, from: "/" } };
  e.waitUntil(self.registration.showNotification(title, options));
});

/**
 * Focuses a tab of the app and opens `url` in it, or opens a new tab.
 * @param {string|null} url the notification's page; `null` keeps the tab's
 * @param {string} from the page that showed the notification
 */
async function open(url, from) {
  const tabs = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
  const tab = tabs.find((t) => t.focused) ?? tabs[0];
  if (!tab) return self.clients.openWindow(url ?? from);
  try {
    await tab.focus();
  } catch {
    // Focusing needs the click's activation, which may have lapsed.
  }
  if (url === null || (await answered(tab, url))) return undefined;
  try {
    return await tab.navigate(url);
  } catch {
    // A tab this worker doesn't control can't be navigated from here.
    return self.clients.openWindow(url);
  }
}

/**
 * Asks `tab` to open `url` with its router; `true` when it says it did.
 * @param {WindowClient} tab
 * @param {string} url
 * @returns {Promise<boolean>}
 */
function answered(tab, url) {
  return new Promise((resolve) => {
    const channel = new MessageChannel();
    channel.port1.onmessage = () => resolve(true);
    setTimeout(() => resolve(false), ANSWER_WITHIN);
    tab.postMessage({ k: "open", url }, [channel.port2]);
  });
}
