/**
 * @module k:runtime/push
 *
 * `k.device.BrowserPushSubscriptions` in the browser
 * (docs/design/50-push.md; docs/compiler/runtime.md §12.16): the Push API
 * on the registration of the app's service worker, `/k-sw.js`, which
 * `k build` emits for apps that ship this chunk (codegen.md §19), with the
 * notification permission push needs kept in a signal.
 *
 * The enums are the stdlib's cases: `PermissionState` `unitCase(0)`
 * `prompt`, `1` `granted`, `2` `denied`; `PushError` `0` `unsupported`,
 * `1` `notAsked`, `2` `denied`, `3` `unavailable`. A `PushSubscription` is
 * `{endpoint, p256dh, auth}` (texts), an `EcdsaP256PublicKey` the base64url
 * text of its point.
 *
 * Shipped as its own client chunk, only to apps whose client injects
 * `PushSubscriptions` (codegen.md §16), and measured against its own size
 * budget. A feature chunk imports only the runtime chunk, so it registers
 * the worker itself, as the notifications chunk does.
 */

import { afterHydration } from "./services.js";
import { signal } from "./signals.js";
import { unitCase } from "./values.js";

/** The service worker's URL, which is also its registration's key. */
const PUSH_WORKER = "/k-sw.js";

/** `PermissionState` by `Notification.permission` (`default` asks). */
const PUSH_STATES = { default: 0, granted: 1, denied: 2 };

/** The base64url alphabet, by digit value. */
const PUSH_B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/** Base64url text (a key the codec already checked) as bytes. */
const keyBytes = (t) => {
  const out = [];
  let v = 0;
  let n = 0;
  for (const c of t) {
    v = ((v << 6) | PUSH_B64.indexOf(c)) & 0xffffff;
    n += 6;
    if (n >= 8) out.push((v >> (n -= 8)) & 255);
  }
  return Uint8Array.from(out);
};

/** Whether the `ArrayBuffer` `a` holds the bytes `b`. */
const sameKey = (a, b) => a?.byteLength === b.length && new Uint8Array(a).every((x, i) => x === b[i]);

/** A browser `PushSubscription` as the K value. */
const subscriptionValue = (s) => {
  const j = s.toJSON();
  return { endpoint: j.endpoint, p256dh: j.keys.p256dh, auth: j.keys.auth };
};

/**
 * The browser's `k.device.BrowserPushSubscriptions`. `isSupported` is
 * constant for the page's life; `permission` is reactive, `.prompt` while
 * the page hydrates, then `Notification.permission`, re-read as the
 * notifications chunk does.
 *
 * `subscribe` registers the worker (scope `/`, `updateViaCache: "none"`)
 * and subscribes with `userVisibleOnly`, keeping a subscription made with
 * the same key and replacing one made with another. `current` and
 * `unsubscribe` never register the worker.
 * @returns {object}
 */
export function BrowserPushSubscriptions() {
  const g = globalThis;
  const container = () =>
    g.isSecureContext === true && typeof g.Notification === "function" && typeof g.PushManager === "function"
      ? g.navigator?.serviceWorker
      : undefined;
  const state = signal(unitCase(0));
  const read = () => {
    const s = PUSH_STATES[g.Notification?.permission] ?? 0;
    if (state.peek().$ !== s) state.set(unitCase(s));
    return s;
  };
  let ready = null;
  const registration = () =>
    (ready ??= container()
      .register(PUSH_WORKER, { scope: "/", updateViaCache: "none" })
      .then(() => container().ready));
  /** The browser's subscription, or `null`, without registering. */
  const existing = async () => {
    try {
      const r = await container()?.getRegistration("/");
      return (await r?.pushManager?.getSubscription()) ?? null;
    } catch {
      return null;
    }
  };
  if (container()) {
    afterHydration(read);
    g.navigator.permissions?.query?.({ name: "notifications" }).then(
      (status) => status.addEventListener?.("change", read),
      () => {},
    );
    g.document?.addEventListener?.("visibilitychange", () => g.document.visibilityState === "visible" && read());
  }
  return {
    get isSupported() {
      return typeof container()?.register === "function";
    },
    get permission() {
      return state.get();
    },
    async requestAccess(_c) {
      if (!container() || read() !== 0) return state.peek();
      await g.Notification.requestPermission();
      read();
      return state.peek();
    },
    async subscribe(_c, key) {
      if (!container()) throw unitCase(0);
      const now = read();
      if (now !== 1) throw unitCase(now === 0 ? 1 : 2);
      let reg;
      try {
        reg = await registration();
      } catch {
        // No worker here (Gecko's private windows).
        ready = null;
        throw unitCase(0);
      }
      const server = keyBytes(String(key));
      try {
        let s = await reg.pushManager.getSubscription();
        if (s && !sameKey(s.options?.applicationServerKey, server)) {
          await s.unsubscribe();
          s = null;
        }
        return subscriptionValue(s ?? (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: server })));
      } catch (e) {
        // Revoked between the check and the call, else the push service.
        if (e?.name === "NotAllowedError" && read() === 2) throw unitCase(2);
        throw unitCase(3);
      }
    },
    async current(_c) {
      const s = await existing();
      return s ? subscriptionValue(s) : null;
    },
    async unsubscribe(_c) {
      try {
        await (await existing())?.unsubscribe();
      } catch {
        // Already gone.
      }
    },
  };
}
