/**
 * @module k:/tests/notifications.test.js
 *
 * `k.device.BrowserNotifications` (docs/design/45-web-platform.md, "Notifications";
 * docs/compiler/runtime.md §12.12) against a scripted `Notification`, `navigator.serviceWorker`
 * and `navigator.permissions`, written from the Notifications API, Service Workers and Permissions
 * standards and each engine's documented behaviour: no service worker outside a secure context;
 * `Notification.permission` (`default`, `granted`, `denied`) and the promise form of
 * `requestPermission`; Chromium's quieter prompt answering `default` without asking; a
 * `PermissionStatus` whose `change` event follows the site's settings (Chromium, Gecko) or doesn't
 * fire (WebKit, re-read when the page becomes visible); WebKit on iOS, which has no `Notification`
 * in a Safari tab; Gecko's private windows, where `register` rejects; `showNotification`, which
 * rejects with a `TypeError` once the permission is gone; and the worker's `{k: "open"}` message.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { BrowserNotifications } from "k:/runtime/notifications.js";
import { computed, flush, root, unitCase } from "k:runtime";

const PROMPT = unitCase(0);
const GRANTED = unitCase(1);
const DENIED = unitCase(2);

/** `Page$href` for the test's pages: `{$: 0, id}` is `/reports/<id>`. */
const href = (p) => `/reports/${p.id}`;

/**
 * A scripted browser. `permission` is `Notification.permission`; `answer` how the user answers
 * the next prompt (`"default"` for a dismissed or quieted one); `registers` records each
 * `register` call, `registerFails` makes them reject; `shown` records each `showNotification`.
 * `change(p)` changes the setting as the site's settings would, firing `change` unless
 * `firesChange` is false; `post(data)` delivers a message from the worker and returns what the
 * page answered on the message's port.
 */
function fakeBrowser({ firesChange = true, permissions = true } = {}) {
  const fake = {
    permission: "default",
    answer: "granted",
    prompts: 0,
    registers: [],
    registerFails: false,
    shown: [],
    messageListeners: [],
  };
  const status = { state: "prompt", onchange: null };
  const registration = {
    showNotification: async (title, options) => {
      if (fake.permission !== "granted") throw new TypeError("No notification permission has been granted");
      fake.shown.push({ title, ...options });
    },
  };
  fake.Notification = function Notification() {
    throw new TypeError("Illegal constructor: use ServiceWorkerRegistration.showNotification()");
  };
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
      if (fake.registerFails) throw Object.assign(new Error("The operation is insecure."), { name: "SecurityError" });
      return registration;
    },
    get ready() {
      return Promise.resolve(registration);
    },
    addEventListener: (type, f) => type === "message" && fake.messageListeners.push(f),
  };
  fake.permissions = permissions
    ? {
        query: async (d) => {
          if (d?.name !== "notifications") throw new TypeError("unknown permission");
          return status;
        },
      }
    : undefined;
  fake.change = (p) => {
    fake.permission = p;
    status.state = p === "default" ? "prompt" : p;
    if (firesChange) status.onchange?.();
  };
  fake.post = (data) => {
    const answers = [];
    const port = { postMessage: (v) => answers.push(v) };
    for (const f of fake.messageListeners) f({ data, ports: [port] });
    return answers;
  };
  return fake;
}

/**
 * Runs `f` in a browser with `fake`'s APIs (none for `null`), in a context that is secure or
 * not, at `/start`.
 */
async function inBrowser(fake, secure, f, { withNotification = true } = {}) {
  const g = globalThis;
  const saved = [g.navigator, g.isSecureContext, g.document, g.Notification, g.location, g.history];
  g.navigator = { serviceWorker: fake?.serviceWorker, permissions: fake?.permissions };
  g.isSecureContext = secure;
  g.Notification = withNotification ? fake?.Notification : undefined;
  const listeners = [];
  g.document = {
    visibilityState: "visible",
    addEventListener: (type, l) => type === "visibilitychange" && listeners.push(l),
  };
  const location = { origin: "https://app.example", pathname: "/start", search: "" };
  g.location = location;
  g.history = {
    pushState: (_s, _t, url) => {
      location.pathname = url;
    },
  };
  if (fake) fake.becomeVisible = () => listeners.forEach((l) => l());
  try {
    return await f();
  } finally {
    [g.navigator, g.isSecureContext, g.document, g.Notification, g.location, g.history] = saved;
  }
}

/** Settles `p` into `{ok}` or `{error}`. */
const settle = (p) => p.then((ok) => ({ ok }), (error) => ({ error }));

const READY = { title: "Your export is ready", body: "Report 7", tag: "report-7", opens: { $: 0, id: "7" } };

test("outside a secure context there are no notifications: unsupported, and nothing is asked", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, false, async () => {
    const n = BrowserNotifications(href);
    assert(!n.isSupported);
    assertEq(n.permission, PROMPT);
    assertEq((await settle(n.show(null, READY))).error, unitCase(0));
    assertEq(await n.requestAccess(null), PROMPT);
    assertEq(fake.prompts, 0);
    assertEq(fake.registers.length, 0, "no worker registered");
  });
});

test("a Safari tab on iOS has no Notification: unsupported", async () => {
  const fake = fakeBrowser();
  await inBrowser(
    fake,
    true,
    async () => {
      const n = BrowserNotifications(href);
      assert(!n.isSupported);
      assertEq((await settle(n.show(null, READY))).error, unitCase(0));
    },
    { withNotification: false },
  );
});

test("the permission is read after hydration, reactively, and follows the change event", async () => {
  const fake = fakeBrowser();
  fake.permission = "granted";
  await inBrowser(fake, true, async () => {
    const n = BrowserNotifications(href);
    assert(n.isSupported);
    let seen = [];
    root(() => {
      seen = [computed(() => n.permission)];
    });
    await tick();
    assertEq(seen[0].get(), GRANTED);
    assertEq(fake.registers.length, 1, "already granted: the worker is registered at once");
    assertEq(fake.registers[0].url, "/k-sw.js");
    assertEq(fake.registers[0].scope, "/");
    assertEq(fake.registers[0].updateViaCache, "none");
    fake.change("denied");
    flush();
    assertEq(seen[0].get(), DENIED, "the site's settings changed");
  });
});

test("WebKit fires no change event: the permission is re-read when the page becomes visible", async () => {
  const fake = fakeBrowser({ firesChange: false });
  await inBrowser(fake, true, async () => {
    const n = BrowserNotifications(href);
    await tick();
    assertEq(n.permission, PROMPT);
    fake.change("granted");
    assertEq(n.permission, PROMPT, "no event yet");
    fake.becomeVisible();
    assertEq(n.permission, GRANTED);
  });
});

test("without the Permissions API the permission still comes from Notification.permission", async () => {
  const fake = fakeBrowser({ permissions: false });
  fake.permission = "denied";
  await inBrowser(fake, true, async () => {
    const n = BrowserNotifications(href);
    await tick();
    assertEq(n.permission, DENIED);
  });
});

test("showing before the user was asked is notAsked, never prompts and registers nothing", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    const n = BrowserNotifications(href);
    assertEq((await settle(n.show(null, READY))).error, unitCase(1));
    assertEq(fake.prompts, 0);
    assertEq(fake.registers.length, 0);
    assertEq(fake.shown.length, 0);
  });
});

test("requestAccess prompts once, then shows through the worker with the page's URL", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    const n = BrowserNotifications(href);
    assertEq(await n.requestAccess(null), GRANTED);
    assertEq(await n.requestAccess(null), GRANTED);
    assertEq(fake.prompts, 1, "asked once");
    await n.show(null, READY);
    assertEq(fake.registers.length, 1);
    assertEq(fake.shown.length, 1);
    const s = fake.shown[0];
    assertEq(s.title, "Your export is ready");
    assertEq(s.body, "Report 7");
    assertEq(s.tag, "report-7");
    assertEq(s.data.url, "/reports/7");
    assertEq(s.data.from, "/start");
    await n.show(null, { title: "Plain", body: null, tag: null, opens: null });
    assertEq(fake.registers.length, 1, "registered once");
    assertEq(fake.shown[1].body, undefined, "a nil body is left out");
    assertEq(fake.shown[1].tag, undefined);
    assertEq(fake.shown[1].data.url, null, "no page: the click keeps the tab's");
  });
});

test("a refused prompt is denied; a quieted one stays at prompt", async () => {
  const refused = fakeBrowser();
  refused.answer = "denied";
  await inBrowser(refused, true, async () => {
    const n = BrowserNotifications(href);
    assertEq(await n.requestAccess(null), DENIED);
    assertEq(n.permission, DENIED);
    assertEq((await settle(n.show(null, READY))).error, unitCase(2));
    assertEq(await n.requestAccess(null), DENIED, "denied: nothing is asked again");
    assertEq(refused.prompts, 1);
  });
  const quiet = fakeBrowser();
  quiet.answer = "default";
  await inBrowser(quiet, true, async () => {
    const n = BrowserNotifications(href);
    assertEq(await n.requestAccess(null), PROMPT, "Chromium's quieter prompt");
    assertEq((await settle(n.show(null, READY))).error, unitCase(1));
  });
});

test("Gecko's private windows have no service workers: show is unsupported, and retried", async () => {
  const fake = fakeBrowser();
  fake.permission = "granted";
  fake.registerFails = true;
  await inBrowser(fake, true, async () => {
    const n = BrowserNotifications(href);
    await tick();
    assertEq((await settle(n.show(null, READY))).error, unitCase(0));
    fake.registerFails = false;
    await n.show(null, READY);
    assertEq(fake.shown.length, 1, "a later show registers again");
  });
});

test("a permission revoked between the check and the call is denied", async () => {
  const fake = fakeBrowser();
  fake.permission = "granted";
  await inBrowser(fake, true, async () => {
    const n = BrowserNotifications(href);
    await tick();
    const shown = n.show(null, READY);
    fake.permission = "denied";
    assertEq((await settle(shown)).error, unitCase(2));
    assertEq(n.permission, DENIED);
  });
});

test("the worker's open message navigates with the router and answers on its port", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    BrowserNotifications(href);
    assertEq(fake.post({ k: "something else" }).length, 0, "other messages are ignored");
    assertDeepEq(fake.post({ k: "open", url: "/reports/9" }), [true]);
    assertEq(globalThis.location.pathname, "/reports/9");
  });
});
