/**
 * @module k:/tests/push.test.js
 *
 * `k.device.BrowserPushSubscriptions` (docs/design/50-push.md; docs/compiler/runtime.md §12.16)
 * against a scripted `Notification`, `PushManager`, `navigator.serviceWorker` and
 * `navigator.permissions`, written from the Push API and Service Workers standards: no Push API
 * outside a secure context or without `PushManager` (a Safari tab on iOS); `subscribe` with
 * `userVisibleOnly` and the server key's bytes, rejecting with `NotAllowedError` without the
 * permission and with `AbortError` when the push service can't be reached; `InvalidStateError`
 * for a subscription made with another key, which the binding avoids by unsubscribing first;
 * `getSubscription` answering `null` without one; and `PushSubscription.toJSON()`'s
 * `{endpoint, expirationTime, keys: {p256dh, auth}}`.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { BrowserPushSubscriptions } from "k:/runtime/push.js";
import { unitCase } from "k:runtime";

const PROMPT = unitCase(0);
const GRANTED = unitCase(1);
const DENIED = unitCase(2);

/** A server key (RFC 8291's example application server key) and its bytes. */
const KEY = "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
const OTHER = "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
const bytesOf = (t) => {
  const A = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  const out = [];
  let bits = "";
  for (const c of t) bits += A.indexOf(c).toString(2).padStart(6, "0");
  for (let i = 0; i + 8 <= bits.length; i += 8) out.push(parseInt(bits.slice(i, i + 8), 2));
  return out;
};

/**
 * A scripted browser: `permission` is `Notification.permission`, `answer` how the user answers
 * the next prompt; `registers` and `subscribes` record the calls; `fail` names the error the next
 * `subscribe` rejects with; `registered` whether a registration exists for `getRegistration`.
 */
function fakeBrowser() {
  const fake = { permission: "default", answer: "granted", prompts: 0, registers: [], subscribes: [], fail: null, registered: false, made: 0, current: null };
  const status = { listeners: [], addEventListener: (type, f) => type === "change" && status.listeners.push(f) };
  const pushManager = {
    getSubscription: async () => fake.current,
    subscribe: async (options) => {
      fake.subscribes.push(options);
      if (fake.permission !== "granted") throw Object.assign(new Error("Registration failed - permission denied"), { name: "NotAllowedError" });
      if (fake.fail) {
        const name = fake.fail;
        fake.fail = null;
        throw Object.assign(new Error("Registration failed - push service error"), { name });
      }
      if (fake.current) throw Object.assign(new Error("A subscription with a different applicationServerKey already exists"), { name: "InvalidStateError" });
      const n = ++fake.made;
      const s = {
        endpoint: `https://push.example/${n}`,
        options: { userVisibleOnly: true, applicationServerKey: options.applicationServerKey.buffer.slice(0) },
        toJSON: () => ({ endpoint: s.endpoint, expirationTime: null, keys: { p256dh: OTHER, auth: "BTBZMqHH6r4Tts7J_aSIgg" } }),
        unsubscribe: async () => {
          if (fake.current === s) fake.current = null;
          return true;
        },
      };
      fake.current = s;
      return s;
    },
  };
  const registration = { pushManager };
  fake.Notification = function Notification() {};
  Object.defineProperty(fake.Notification, "permission", { get: () => fake.permission });
  fake.Notification.requestPermission = async () => {
    if (fake.permission === "default") {
      fake.prompts++;
      fake.permission = fake.answer;
    }
    return fake.permission;
  };
  fake.serviceWorker = {
    register: async (url, options) => {
      fake.registers.push({ url, ...options });
      fake.registered = true;
      return registration;
    },
    getRegistration: async () => (fake.registered ? registration : undefined),
    get ready() {
      return Promise.resolve(registration);
    },
    addEventListener: () => {},
  };
  fake.permissions = { query: async () => status };
  fake.change = (p) => {
    fake.permission = p;
    status.listeners.forEach((f) => f());
  };
  return fake;
}

/** Runs `f` in a browser with `fake`'s APIs, secure or not, with or without `PushManager`. */
async function inBrowser(fake, secure, f, { withPush = true } = {}) {
  const g = globalThis;
  const saved = [g.navigator, g.isSecureContext, g.document, g.Notification, g.PushManager];
  g.navigator = { serviceWorker: fake.serviceWorker, permissions: fake.permissions };
  g.isSecureContext = secure;
  g.Notification = fake.Notification;
  g.PushManager = withPush ? function PushManager() {} : undefined;
  g.document = { visibilityState: "visible", addEventListener: () => {} };
  try {
    return await f();
  } finally {
    [g.navigator, g.isSecureContext, g.document, g.Notification, g.PushManager] = saved;
  }
}

/** Settles `p` into `{ok}` or `{error}`. */
const settle = (p) => p.then((ok) => ({ ok }), (error) => ({ error }));

test("outside a secure context, or without the Push API, push is unsupported and nothing is asked", async () => {
  for (const [secure, withPush] of [[false, true], [true, false]]) {
    const fake = fakeBrowser();
    await inBrowser(
      fake,
      secure,
      async () => {
        const p = BrowserPushSubscriptions();
        assert(!p.isSupported);
        assertEq(await p.requestAccess(null), PROMPT);
        assertDeepEq(await settle(p.subscribe(null, KEY)), { error: unitCase(0) });
        assertEq(await p.current(null), null);
        assertEq(fake.prompts, 0);
        assertDeepEq(fake.registers, []);
      },
      { withPush },
    );
  }
});

test("subscribing before asking is notAsked and never prompts; requestAccess prompts once", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    const p = BrowserPushSubscriptions();
    assert(p.isSupported);
    assertDeepEq(await settle(p.subscribe(null, KEY)), { error: unitCase(1) });
    assertEq(fake.prompts, 0);
    assertEq(await p.requestAccess(null), GRANTED);
    assertEq(await p.requestAccess(null), GRANTED);
    assertEq(fake.prompts, 1);
    assertEq(p.permission, GRANTED);
  });
});

test("subscribe registers the worker and subscribes visibly with the key's bytes", async () => {
  const fake = fakeBrowser();
  fake.permission = "granted";
  await inBrowser(fake, true, async () => {
    const p = BrowserPushSubscriptions();
    const s = await p.subscribe(null, KEY);
    assertDeepEq(s, { endpoint: "https://push.example/1", p256dh: OTHER, auth: "BTBZMqHH6r4Tts7J_aSIgg" });
    assertDeepEq(fake.registers, [{ url: "/k-sw.js", scope: "/", updateViaCache: "none" }]);
    assertEq(fake.subscribes.length, 1);
    assertEq(fake.subscribes[0].userVisibleOnly, true);
    assertDeepEq([...fake.subscribes[0].applicationServerKey], bytesOf(KEY));
    assertDeepEq(await p.current(null), s);
  });
});

test("the same key keeps the subscription; another key replaces it", async () => {
  const fake = fakeBrowser();
  fake.permission = "granted";
  await inBrowser(fake, true, async () => {
    const p = BrowserPushSubscriptions();
    const first = await p.subscribe(null, KEY);
    assertDeepEq(await p.subscribe(null, KEY), first);
    assertEq(fake.subscribes.length, 1, "no second subscribe");
    const rotated = await p.subscribe(null, OTHER);
    assertEq(rotated.endpoint, "https://push.example/2");
    assertEq(fake.subscribes.length, 2);
  });
});

test("an unreachable push service is unavailable; a revoked permission is denied", async () => {
  const fake = fakeBrowser();
  fake.permission = "granted";
  await inBrowser(fake, true, async () => {
    const p = BrowserPushSubscriptions();
    fake.fail = "AbortError";
    assertDeepEq(await settle(p.subscribe(null, KEY)), { error: unitCase(3) });
    fake.change("denied");
    assertEq(p.permission, DENIED);
    assertDeepEq(await settle(p.subscribe(null, KEY)), { error: unitCase(2) });
  });
});

test("current and unsubscribe never register the worker", async () => {
  const fake = fakeBrowser();
  fake.permission = "granted";
  await inBrowser(fake, true, async () => {
    const p = BrowserPushSubscriptions();
    assertEq(await p.current(null), null);
    await p.unsubscribe(null);
    assertDeepEq(fake.registers, []);
    await p.subscribe(null, KEY);
    await p.unsubscribe(null);
    assertEq(await p.current(null), null);
    await p.unsubscribe(null);
  });
});
