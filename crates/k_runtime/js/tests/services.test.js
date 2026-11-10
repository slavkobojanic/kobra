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
import * as k from "k:runtime";
import { assert, assertDeepEq, assertEq, assertThrows, installDom, test, tick } from "k:testing";
import {
  Instant, KPanic, afterHydration, browserStorage, clock, codecs, effect, fetchHttpClient, flush, hydrate, isHydrating, native,
  provide, root, session, sessionMirror, template, theme, unitCase,
} from "k:runtime";
import { themeSnippet } from "k:runtime/server";

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

// A reload keeps the scheme (07-themes-and-units.md): the remembered
// variant's classes are on `<html>` as soon as the service exists, even
// when nothing put them there before (a document without the snippet).
test("Theme applies the remembered variant when it is created", () => {
  installDom();
  const jar = cookies();
  const root$ = globalThis.document.documentElement;
  root$.setAttribute("class", "app");
  jar.set("k_theme", "highContrast");
  assertEq(theme().current, "highContrast");
  assertEq(root$.getAttribute("class"), "app k-theme-set k-theme-high-contrast");
  jar.set("k_theme", "base");
  theme();
  assertEq(root$.getAttribute("class"), "app k-theme-set");
  jar.delete("k_theme");
  root$.setAttribute("class", "app");
  theme();
  assertEq(root$.getAttribute("class"), "app", "no choice leaves the root alone");
});

// The static document's pre-paint snippet (`themeSnippet`, run inline in
// `<head>` before the stylesheet applies) gives `<html>` the same classes
// `theme()` would, and ignores a cookie that isn't a variant name.
test("the pre-paint snippet applies the remembered variant", () => {
  installDom();
  const jar = cookies();
  const root$ = globalThis.document.documentElement;
  const run = () => new Function(themeSnippet)();
  globalThis.location = { protocol: "http:", port: "" };
  root$.setAttribute("class", "");
  run();
  assertEq(root$.getAttribute("class") ?? "", "");
  jar.set("k_theme", "highContrast");
  run();
  assertEq(root$.getAttribute("class"), "k-theme-set k-theme-high-contrast");
  root$.setAttribute("class", "");
  jar.set("k_theme", "x%22%20onload%3D");
  run();
  assertEq(root$.getAttribute("class") ?? "", "");
  // The cookie is the app's, named as `theme()` names it: another app's
  // on the same host is no choice.
  jar.set("k_theme", "dark");
  globalThis.location = { protocol: "http:", port: "5203" };
  root$.setAttribute("class", "");
  run();
  assertEq(root$.getAttribute("class") ?? "", "");
  jar.set("k_theme_5203", "dark");
  run();
  assertEq(root$.getAttribute("class"), "k-theme-set k-theme-dark");
  globalThis.location = { protocol: "https:", port: "" };
  root$.setAttribute("class", "");
  run();
  assertEq(root$.getAttribute("class") ?? "", "");
  jar.set("__Host-k_theme", "dark");
  run();
  assertEq(root$.getAttribute("class"), "k-theme-set k-theme-dark");
  delete globalThis.location;
});

// Browsers scope cookies by host, not port, so the theme cookie is named
// as the session cookie is (07-themes-and-units.md): `k_theme_<port>` on
// an `http` origin with a port, `__Host-k_theme` (which must be `secure`)
// on `https`. Another app's choice on the same host is not this app's.
test("Theme remembers the choice in a cookie scoped to the app", () => {
  installDom();
  const written = [];
  const jar = cookies();
  const desc = Object.getOwnPropertyDescriptor(globalThis.document, "cookie");
  Object.defineProperty(globalThis.document, "cookie", {
    configurable: true,
    get: desc.get,
    set: (text) => {
      written.push(text);
      desc.set(text);
    },
  });
  const root$ = globalThis.document.documentElement;
  try {
    globalThis.location = { protocol: "http:", port: "5203" };
    jar.set("k_theme", "dark");
    root$.setAttribute("class", "");
    const t = theme();
    assertEq(t.current, "base", "another app's choice");
    t.apply("dark");
    assertEq(jar.get("k_theme_5203"), "dark");
    assertEq(jar.get("k_theme"), "dark", "the other app's cookie is untouched");
    assertEq(theme().current, "dark");
    globalThis.location = { protocol: "https:", port: "" };
    theme().apply("highContrast");
    assertEq(jar.get("__Host-k_theme"), "highContrast");
    assert(written.at(-1).endsWith("; secure"), written.at(-1));
  } finally {
    delete globalThis.location;
  }
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

// Keys are namespaced per app with `storage.lock`'s namespace
// (36-local-storage.md, "Keys"; todos-F).
test("BrowserStorage keeps a key's item under the app's namespace", () => {
  const items = memoryStorage(null);
  const s = browserStorage(ReadError, WriteError, "todos");
  const key = { name: "todos" };
  const codec = codecs.list(codecs.string);
  s.write(codec, key, ["a"]);
  assertEq(items.get("todos:todos"), '["a"]');
  assert(!items.has("todos"), "no bare item");
  assertDeepEq(s.read(codec, key), ["a"]);
  s.remove(key);
  assertEq(items.size, 0);
});

// An item a build before namespacing stored under the bare name is moved
// under the namespace the first time it is read, once.
test("BrowserStorage moves an item stored before namespacing, once", () => {
  const items = memoryStorage(null);
  items.set("todos", '["old"]');
  const s = browserStorage(ReadError, WriteError, "todos");
  const codec = codecs.list(codecs.string);
  assertDeepEq(s.read(codec, { name: "todos" }), ["old"]);
  assertEq(items.get("todos:todos"), '["old"]');
  assert(!items.has("todos"), "the bare item is gone");
  // A bare item written later (another app) is never read over ours.
  items.set("todos", '["other"]');
  assertDeepEq(s.read(codec, { name: "todos" }), ["old"]);
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

// A cancellation (a `@Load`'s `reload()` stopping the run before) can land
// after the response's head and before its body: `text()` then rejects
// with an `AbortError`, which reached the page as an uncaught error. It is
// the call's `offline` now, and the cancelled caller ignores it. (An
// unhandled rejection fails the test isolate.)
test("FetchHttpClient cancelled while reading the body rejects with offline, not an uncaught AbortError", async () => {
  fakeFetch(200, "");
  let signal = null;
  globalThis.fetch = (url, init) => {
    signal = init.signal;
    return Promise.resolve({
      ok: true,
      status: 200,
      headers: { get: () => null },
      text: () =>
        new Promise((_, reject) => {
          init.signal.addEventListener("abort", () => reject(new Error("AbortError: signal is aborted without reason")));
        }),
    });
  };
  const caller = new AbortController();
  const sent = fetchHttpClient(HttpError, HttpStatus).send(caller.signal, request({}));
  await tick();
  caller.abort();
  assert(signal.aborted, "the caller's cancellation aborts the fetch");
  let e = null;
  try {
    await sent;
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

// The mirror starts from a document's `k-data` once (the router or the
// first `Session` reader takes it): a `Session` first read after an rpc
// changed the session keeps the rpc's mirror, not the document's.
test("a Session first read after an rpc changed the session keeps the newer mirror", () => {
  const data = { session: { user: { id: "a1", roles: [] } } };
  session(data);
  sessionMirror.set({ user: null });
  const later = session(data);
  assertEq(later.user, null);
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

test("each browser native is also its own export, for release builds to import directly", () => {
  for (const name of ["k.crypto.Uuid$random", "k.crypto.Uuid$init", "k.crypto.Uuid$parse", "k.text.EmailAddress$init", "k.text.EmailAddress$parse", "k.storage.StorageKey$init"]) {
    const exported = k["native_" + name.replaceAll(".", "_")];
    assertEq(typeof exported, "function", name);
    assertEq(exported, native(name), name);
  }
  const missing = k.missingNative("k.server.JobQueue");
  assertThrows(() => missing.enqueue(1), "k.server.JobQueue.enqueue is not implemented in the browser yet");
});

test("a native the browser lacks loads and panics with its name when used", () => {
  const missing = native("k.server.JobQueue");
  const e = assertThrows(() => missing.enqueue(1), "k.server.JobQueue.enqueue is not implemented in the browser yet");
  assert(e instanceof KPanic);
});
