/**
 * @module k:/tests/services.test.js
 *
 * Dependency injection in the browser and the client bodies of the
 * stdlib's platform services (docs/compiler/runtime.md, "Services";
 * docs/design/10-services.md, 07-themes-and-units.md, 32-http-client.md,
 * 35-time.md, 36-local-storage.md): `provide`, `afterHydration`, `Clock`,
 * `Theme`, `BrowserStorage`, `FetchHttpClient`, the `Session` mirror and
 * browser `@Native` members.
 */
import { assert, assertDeepEq, assertEq, assertThrows, installDom, test, tick } from "k:testing";
import {
  Instant, KPanic, afterHydration, browserStorage, clock, codecs, effect, fetchHttpClient, flush, hydrate, isHydrating, native,
  provide, root, session, sessionMirror, template, theme, unitCase,
} from "k:runtime";

/** Runs `fn` inside a hydration pass over an empty container. */
function hydrating(fn) {
  const doc = globalThis.document;
  const container = doc.createElement("div");
  doc.body.appendChild(container);
  let out;
  hydrate(container, () => {
    out = fn();
    return template("<p></p>")();
  }, null);
  return out;
}

/** A cookie jar on the fake document. */
function cookies() {
  const jar = new Map();
  Object.defineProperty(globalThis.document, "cookie", {
    configurable: true,
    get: () => [...jar].map(([k, v]) => `${k}=${v}`).join("; "),
    set: (text) => {
      const [pair, ...attrs] = String(text).split(";");
      const [k, v] = pair.split("=");
      if (attrs.some((a) => a.trim() === "max-age=0")) jar.delete(k.trim());
      else jar.set(k.trim(), v.trim());
    },
  });
  return jar;
}

// ------------------------------------------------------------- containers

test("provide answers one name and delegates the rest to the container", () => {
  /** A stand-in for a generated container with one shared service. */
  class C {
    /** The container's `Shared` service, made once. */
    get Shared() {
      return (this.$s ??= { n: 1 });
    }
  }
  const c = new C();
  const ctx = { c, key: "0" };
  const child = provide(ctx, "Form", "mine");
  assertEq(child.key, "0");
  assertEq(child.c.Form, "mine");
  assertEq(child.c.Shared, c.Shared, "memoized on the original container");
  assertEq(ctx.c.Form, undefined, "the parent is unchanged");
});

test("afterHydration runs now outside a hydration pass", () => {
  let ran = false;
  afterHydration(() => (ran = true));
  assert(ran);
});

test("afterHydration waits for the end of the hydration pass", async () => {
  const seen = [];
  hydrating(() => afterHydration(() => seen.push(isHydrating())));
  assertDeepEq(seen, []);
  await tick();
  assertDeepEq(seen, [false]);
});

// ------------------------------------------------------------------ Clock

test("Clock is the server's render instant while the page hydrates", () => {
  const c = clock({ now: "2026-11-07T10:00:00Z" });
  const during = hydrating(() => c.now);
  assert(during instanceof Instant);
  assertEq(during.ms, Date.UTC(2026, 10, 7, 10));
  const after = c.now;
  assert(Math.abs(after.ms - Date.now()) < 5000, "the system time afterwards");
});

test("Clock without k-data is the system time", () => {
  const during = hydrating(() => clock(null).now);
  assert(Math.abs(during.ms - Date.now()) < 5000);
});

test("Clock timers use setTimeout and setInterval and cancel", () => {
  const calls = [];
  const saved = [globalThis.setTimeout, globalThis.setInterval, globalThis.clearTimeout, globalThis.clearInterval];
  globalThis.setTimeout = (f, d) => (calls.push(["timeout", d]), 1);
  globalThis.setInterval = (f, d) => (calls.push(["interval", d]), 2);
  globalThis.clearTimeout = (id) => calls.push(["clearTimeout", id]);
  globalThis.clearInterval = (id) => calls.push(["clearInterval", id]);
  try {
    const c = clock(null);
    c.after(300, () => {}).cancel();
    c.every(3000, () => {}).cancel();
  } finally {
    [globalThis.setTimeout, globalThis.setInterval, globalThis.clearTimeout, globalThis.clearInterval] = saved;
  }
  assertDeepEq(calls, [["timeout", 300], ["clearTimeout", 1], ["interval", 3000], ["clearInterval", 2]]);
});

// ------------------------------------------------------------------ Theme

test("Theme starts from the k_theme cookie, else base", () => {
  installDom();
  const jar = cookies();
  assertEq(theme().current, "base");
  jar.set("k_theme", "dark");
  assertEq(theme().current, "dark");
});

test("Theme.apply swaps the root class, remembers the choice and is reactive", () => {
  installDom();
  const jar = cookies();
  const root$ = globalThis.document.documentElement;
  root$.setAttribute("class", "app k-theme-dark");
  const t = theme();
  const seen = [];
  root(() => effect(() => seen.push(t.current)));
  flush();
  t.apply("highContrast");
  flush();
  assertEq(root$.getAttribute("class"), "app k-theme-set k-theme-high-contrast");
  assertEq(jar.get("k_theme"), "highContrast");
  t.apply("base");
  assertEq(root$.getAttribute("class"), "app k-theme-set");
  t.followSystem();
  flush();
  assertEq(root$.getAttribute("class"), "app");
  assertEq(jar.has("k_theme"), false);
  assertDeepEq(seen, ["base", "highContrast", "base"]);
});

// --------------------------------------------------------- BrowserStorage

const ReadError = { unavailable: unitCase(0), unreadable: unitCase(1) };
const WriteError = { unavailable: unitCase(0), quotaExceeded: unitCase(1) };

/** Installs an in-memory `localStorage` whose writes throw `fail` when given; returns its items. */
function memoryStorage(fail) {
  const items = new Map();
  globalThis.localStorage = {
    getItem: (k) => (items.has(k) ? items.get(k) : null),
    setItem: (k, v) => {
      if (fail) throw fail;
      items.set(k, v);
    },
    removeItem: (k) => items.delete(k),
  };
  return items;
}

test("BrowserStorage writes and reads values through their codec", () => {
  const items = memoryStorage(null);
  const s = browserStorage(ReadError, WriteError);
  const key = { name: "todos" };
  const codec = codecs.list(codecs.string);
  assertEq(s.read(codec, key), null, "nothing stored yet");
  s.write(codec, key, ["a", "b"]);
  assertEq(items.get("todos"), '["a","b"]');
  assertDeepEq(s.read(codec, key), ["a", "b"]);
  s.remove(key);
  assertEq(s.read(codec, key), null);
});

test("BrowserStorage throws the typed cases", () => {
  const items = memoryStorage(Object.assign(new Error("full"), { name: "QuotaExceededError" }));
  items.set("bad", "{not json");
  const s = browserStorage(ReadError, WriteError);
  const codec = codecs.list(codecs.string);
  assertEq(assertThrows(() => s.read(codec, { name: "bad" })), ReadError.unreadable);
  assertEq(assertThrows(() => s.write(codec, { name: "x" }, [])), WriteError.quotaExceeded);
  memoryStorage(new Error("blocked"));
  assertEq(assertThrows(() => browserStorage(ReadError, WriteError).write(codec, { name: "x" }, [])), WriteError.unavailable);
  globalThis.localStorage = undefined;
  assertEq(assertThrows(() => s.read(codec, { name: "x" })), ReadError.unavailable);
});

// -------------------------------------------------------- FetchHttpClient

const HttpError = {
  offline: unitCase(0),
  timeout: unitCase(1),
  status: (_0, body) => ({ $: 2, _0, body }),
  decoding: (message) => ({ $: 3, message }),
};
const HttpStatus = {
  ok: unitCase(0), created: unitCase(1), accepted: unitCase(2), noContent: unitCase(3), movedPermanently: unitCase(4),
  seeOther: unitCase(5), notModified: unitCase(6), badRequest: unitCase(7), unauthorized: unitCase(8),
  forbidden: unitCase(9), notFound: unitCase(10), methodNotAllowed: unitCase(11), conflict: unitCase(12),
  gone: unitCase(13), payloadTooLarge: unitCase(14), unsupportedMediaType: unitCase(15),
  unprocessableEntity: unitCase(16), tooManyRequests: unitCase(17), internalServerError: unitCase(18),
  badGateway: unitCase(19), serviceUnavailable: unitCase(20), gatewayTimeout: unitCase(21),
  other: (_0) => ({ $: 22, _0 }),
};

/** A fake `fetch` answering `status` and `body`, recording requests. */
function fakeFetch(status, body) {
  // Browsers have timers; the test isolate doesn't (the timeouts never fire).
  globalThis.setTimeout ??= () => 0;
  globalThis.clearTimeout ??= () => {};
  const sent = [];
  globalThis.fetch = (url, init) => {
    sent.push({ url, ...init });
    return Promise.resolve({
      ok: status >= 200 && status < 300,
      status,
      headers: { get: (n) => (n === "content-type" ? "application/json" : null) },
      text: async () => body,
    });
  };
  return sent;
}

/** An `HttpRequest` to the search API, overridden by `extra`. */
const request = (extra) => ({ method: unitCase(0), url: "https://api.example.com/v1/search", query: new Map(), headers: new Map(), secretHeaders: new Map(), bearerToken: null, body: null, timeout: 1000, ...extra });

test("FetchHttpClient sends the query, headers and method", async () => {
  const sent = fakeFetch(200, "{}");
  const client = fetchHttpClient(HttpError, HttpStatus);
  await client.send(null, request({ method: unitCase(1), query: new Map([["name", "Bér lin"], ["count", "8"]]), headers: new Map([["accept", "application/json"]]), body: { $: 1, _0: "hi" } }));
  assertEq(sent[0].url, "https://api.example.com/v1/search?name=B%C3%A9r%20lin&count=8");
  assertEq(sent[0].method, "POST");
  assertEq(sent[0].headers.accept, "application/json");
  assertEq(sent[0].headers["content-type"], "text/plain; charset=utf-8");
  assertEq(sent[0].body, "hi");
});

test("FetchHttpClient maps statuses to HttpStatus cases", async () => {
  fakeFetch(404, "missing");
  const client = fetchHttpClient(HttpError, HttpStatus);
  const r = await client.send(null, request({}));
  assertEq(r.status, HttpStatus.notFound);
  assertEq(r.isSuccess, false);
  assertEq(r.body, "missing");
  fakeFetch(418, "");
  assertDeepEq((await client.send(null, request({}))).status, { $: 22, _0: 418 });
});

test("fetchJson decodes a 2xx body and throws status and decoding errors", async () => {
  const client = fetchHttpClient(HttpError, HttpStatus);
  fakeFetch(200, '["a"]');
  assertDeepEq(await client.fetchJson(null, codecs.list(codecs.string), request({})), ["a"]);
  fakeFetch(503, "down");
  let e = null;
  try {
    await client.fetchJson(null, codecs.list(codecs.string), request({}));
  } catch (x) {
    e = x;
  }
  assertDeepEq(e, { $: 2, _0: HttpStatus.serviceUnavailable, body: "down" });
  fakeFetch(200, '{"a": 1}');
  e = null;
  try {
    await client.fetchJson(null, codecs.list(codecs.string), request({}));
  } catch (x) {
    e = x;
  }
  assertEq(e.$, 3);
});

test("FetchHttpClient rejects with offline when fetch fails", async () => {
  fakeFetch(200, "");
  globalThis.fetch = () => Promise.reject(new TypeError("Failed to fetch"));
  let e = null;
  try {
    await fetchHttpClient(HttpError, HttpStatus).send(null, request({}));
  } catch (x) {
    e = x;
  }
  assertEq(e, HttpError.offline);
});

// ---------------------------------------------------------------- Session

test("Session mirrors k-data and later rpc updates", () => {
  const s = session({ session: { user: { id: "a1", roles: [] } } });
  assertEq(s.user.id, "a1");
  sessionMirror.set({ user: null });
  assertEq(s.user, null);
});

// ---------------------------------------------------------------- natives

test("browser natives: StorageKey, Uuid.random", () => {
  assertDeepEq(native("k.storage.StorageKey$init")("todos"), { name: "todos" });
  const saved = globalThis.crypto;
  globalThis.crypto = { randomUUID: () => "0b6f2c1e-6a4f-4c39-9a57-2f1d3c4b5a61" };
  try {
    assertEq(native("k.crypto.Uuid$random")(), "0b6f2c1e-6a4f-4c39-9a57-2f1d3c4b5a61");
  } finally {
    globalThis.crypto = saved;
  }
});

test("a native the browser lacks loads and panics with its name when used", () => {
  const missing = native("k.server.JobQueue");
  const e = assertThrows(() => missing.enqueue(1), "k.server.JobQueue.enqueue is not implemented in the browser yet");
  assert(e instanceof KPanic);
});
