/**
 * @module k:/tests/sw-offline.test.js
 *
 * The offline half of the service worker `k-sw.js` (docs/design/54-offline.md;
 * docs/compiler/runtime.md §12.20) in a scripted `ServiceWorkerGlobalScope`, written from the
 * Service Workers standard: `CacheStorage` keeps its caches in creation order (`keys()`), and
 * `caches.match` searches them in that order unless `cacheName` names one; `Cache.addAll` rejects,
 * keeping nothing, when any fetch fails or answers other than `ok`; `Cache.keys()` lists requests
 * in insertion order; `fetch` rejects with a `TypeError` without a network; `respondWith` and
 * `waitUntil` take promises; `Response.error()` is a network error. Each test runs in an isolate of
 * its own, so the worker's script is evaluated once per test, after `self.kOffline` is set.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";

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

const ORIGIN = "https://app.example";

/** A response: `status`, `type` (`text/html` for documents), `cache` its `cache-control`. */
function response(body, { status = 200, type = "text/javascript", cache = null } = {}) {
  const headers = { "content-type": type, "cache-control": cache };
  const r = {
    body,
    status,
    ok: status >= 200 && status < 300,
    headers: { get: (n) => headers[n] ?? null },
    clone: () => response(body, { status, type, cache }),
  };
  return r;
}

/** The URL a request or a path names, absolute. */
const key = (r) => new URL(typeof r === "string" ? r : r.url, ORIGIN).href;

/** A scripted `Cache`: insertion-ordered entries by absolute URL. */
function cache(scope) {
  const entries = new Map();
  const c = {
    entries,
    async put(r, res) {
      entries.delete(key(r));
      entries.set(key(r), res);
    },
    async addAll(list) {
      const got = [];
      for (const r of list) {
        const res = await scope.fetch(r);
        if (!res.ok) throw new TypeError(`${r} answered ${res.status}`);
        got.push([r, res]);
      }
      for (const [r, res] of got) await c.put(r, res);
    },
    async match(r) {
      return entries.get(key(r));
    },
    async keys() {
      return [...entries.keys()].map((url) => ({ url }));
    },
    async delete(r) {
      return entries.delete(key(r));
    },
  };
  return c;
}

/**
 * Installs a scripted worker scope whose network answers `site` (path → response; a missing path
 * is a 404) until `scope.offline = true`, evaluates the worker with `config` as its list, and
 * returns the scope: `dispatch(type, event)` runs the listeners and waits for `waitUntil`, and
 * returns what `respondWith` was given (`undefined` when no listener answered).
 */
async function worker(config, site = {}, { notifications = false } = {}) {
  const listeners = {};
  const stores = new Map();
  const scope = {
    offline: false,
    fetched: [],
    location: { origin: ORIGIN },
    addEventListener: (type, f) => (listeners[type] ??= []).push(f),
    skipWaiting: async () => {},
    clients: { claim: async () => {}, matchAll: async () => [], openWindow: async () => null },
    registration: { scope: `${ORIGIN}/`, showNotification: async () => {} },
    kOffline: config,
    async fetch(r) {
      const url = new URL(key(r));
      scope.fetched.push(url.pathname);
      if (scope.offline) throw new TypeError("Failed to fetch");
      return site[url.pathname] ?? response("not found", { status: 404, type: "text/plain" });
    },
    caches: {
      async open(name) {
        if (!stores.has(name)) stores.set(name, cache(scope));
        return stores.get(name);
      },
      async keys() {
        return [...stores.keys()];
      },
      async delete(name) {
        return stores.delete(name);
      },
      async match(r, options = {}) {
        for (const [name, c] of stores) {
          if (options.cacheName !== undefined && name !== options.cacheName) continue;
          const hit = await c.match(r);
          if (hit) return hit;
        }
        return undefined;
      },
    },
    stores,
  };
  scope.listeners = listeners;
  scope.dispatch = async (type, event = {}) => {
    const waits = [];
    let answer;
    for (const f of listeners[type] ?? []) {
      f({ ...event, waitUntil: (p) => waits.push(p), respondWith: (p) => (answer = p) });
    }
    const out = answer === undefined ? undefined : await answer;
    await Promise.all(waits);
    return out;
  };
  /** The paths cached under `name`. */
  scope.cached = (name) => [...(stores.get(name)?.entries.keys() ?? [])].map((u) => new URL(u).pathname);
  globalThis.self = scope;
  globalThis.caches = scope.caches;
  globalThis.fetch = scope.fetch;
  globalThis.Response = { error: () => ({ type: "error", status: 0 }) };
  await import("k:/runtime/sw-offline.js");
  if (notifications) await import("k:/runtime/sw.js");
  return scope;
}

/** A `GET` of `path`: a navigation when `navigate`. */
function get(path, navigate = false) {
  return { request: { url: `${ORIGIN}${path}`, method: "GET", mode: navigate ? "navigate" : "cors" } };
}

const BUILD = {
  version: "a1",
  build: "/assets/app.a1.js",
  files: ["/assets/app.a1.js", "/assets/runtime.a1.js", "/assets/app.a1.css"],
  shell: "/",
};

/** The static host of `BUILD`. */
const SITE = {
  "/": response("<!doctype html>shell", { type: "text/html", cache: "no-cache" }),
  "/assets/app.a1.js": response("app"),
  "/assets/runtime.a1.js": response("runtime"),
  "/assets/app.a1.css": response("css", { type: "text/css" }),
};

test("install keeps the build's files and the shell in the build's cache", async () => {
  const sw = await worker(BUILD, SITE);
  await sw.dispatch("install");
  assertDeepEq(sw.cached("k-a1"), ["/assets/app.a1.js", "/assets/runtime.a1.js", "/assets/app.a1.css", "/"]);
});

test("a file that can't be fetched fails the install and keeps nothing", async () => {
  const site = { ...SITE };
  delete site["/assets/runtime.a1.js"];
  const sw = await worker(BUILD, site);
  let failed = false;
  try {
    await sw.dispatch("install");
  } catch {
    failed = true;
  }
  assert(failed, "the install fails, so the browser keeps the old worker");
  assertDeepEq(sw.cached("k-a1"), []);
});

test("a shell answered no-store (it carries a session) isn't kept, and navigations go to the network", async () => {
  const site = { ...SITE, "/": response("<!doctype html>signed in", { type: "text/html", cache: "private, no-store" }) };
  const sw = await worker(BUILD, site);
  await sw.dispatch("install");
  assertDeepEq(sw.cached("k-a1"), ["/assets/app.a1.js", "/assets/runtime.a1.js", "/assets/app.a1.css"]);
  sw.fetched.length = 0;
  const r = await sw.dispatch("fetch", get("/", true));
  assertEq(r.body, "<!doctype html>signed in");
  assertDeepEq(sw.fetched, ["/"]);
  assertDeepEq(sw.cached("k-a1-pages"), [], "a no-store page is never kept");
});

test("a client-rendered app opens offline: every navigation is the cached shell, every file the cached one", async () => {
  const sw = await worker(BUILD, SITE);
  await sw.dispatch("install");
  sw.offline = true;
  sw.fetched.length = 0;
  assertEq((await sw.dispatch("fetch", get("/", true))).body, "<!doctype html>shell");
  assertEq((await sw.dispatch("fetch", get("/todos/7?filter=done", true))).body, "<!doctype html>shell");
  assertEq((await sw.dispatch("fetch", get("/assets/runtime.a1.js"))).body, "runtime");
  assertDeepEq(sw.fetched, [], "nothing reached the network");
});

test("a file the build doesn't list comes from the network and isn't kept", async () => {
  const sw = await worker(BUILD, { ...SITE, "/assets/app.a1.js.map": response("{}", { type: "application/json" }) });
  await sw.dispatch("install");
  assertEq((await sw.dispatch("fetch", get("/assets/app.a1.js.map"))).body, "{}");
  assert(!sw.cached("k-a1").includes("/assets/app.a1.js.map"));
});

test("rpcs, other methods, other origins and the worker itself are left to the network", async () => {
  const sw = await worker(BUILD, SITE);
  await sw.dispatch("install");
  const post = { request: { url: `${ORIGIN}/_k/rpc/notes.NotesProtocol/save`, method: "POST", mode: "cors" } };
  assertEq(await sw.dispatch("fetch", post), undefined);
  assertEq(await sw.dispatch("fetch", get("/_k/rpc/notes.NotesProtocol/list")), undefined);
  assertEq(await sw.dispatch("fetch", get("/k-sw.js")), undefined);
  const elsewhere = { request: { url: "https://api.open-meteo.com/v1/forecast", method: "GET", mode: "cors" } };
  assertEq(await sw.dispatch("fetch", elsewhere), undefined);
});

/** A server-rendered app's build: no shell. */
const SERVED = { ...BUILD, shell: null };

test("a server-rendered page comes from the network, is kept, and is served from the cache offline", async () => {
  const page = response("<!doctype html>Berlin: 21°", { type: "text/html; charset=utf-8" });
  const sw = await worker(SERVED, { ...SITE, "/place/berlin": page });
  await sw.dispatch("install");
  assertDeepEq(sw.cached("k-a1"), ["/assets/app.a1.js", "/assets/runtime.a1.js", "/assets/app.a1.css"]);
  assertEq((await sw.dispatch("fetch", get("/place/berlin", true))).body, "<!doctype html>Berlin: 21°");
  assertDeepEq(sw.cached("k-a1-pages"), ["/place/berlin"]);
  sw.offline = true;
  assertEq((await sw.dispatch("fetch", get("/place/berlin", true))).body, "<!doctype html>Berlin: 21°");
  assertDeepEq(await sw.dispatch("fetch", get("/place/paris", true)), { type: "error", status: 0 }, "never visited: a network error");
});

test("signed-in pages, errors and non-HTML answers aren't kept", async () => {
  const sw = await worker(SERVED, {
    ...SITE,
    "/notes": response("<!doctype html>Ada's notes", { type: "text/html", cache: "private, no-store" }),
    "/missing": response("<!doctype html>Not found", { status: 404, type: "text/html" }),
    "/export.csv": response("a,b", { type: "text/csv" }),
  });
  await sw.dispatch("install");
  for (const path of ["/notes", "/missing", "/export.csv"]) await sw.dispatch("fetch", get(path, true));
  assertDeepEq(sw.cached("k-a1-pages"), []);
});

test("a build keeps at most 50 pages, dropping the oldest", async () => {
  const site = { ...SITE };
  for (let i = 0; i < 52; i++) site[`/p/${i}`] = response(`page ${i}`, { type: "text/html" });
  const sw = await worker(SERVED, site);
  await sw.dispatch("install");
  for (let i = 0; i < 52; i++) await sw.dispatch("fetch", get(`/p/${i}`, true));
  const kept = sw.cached("k-a1-pages");
  assertEq(kept.length, 50);
  assertEq(kept[0], "/p/2");
  assertEq(kept.at(-1), "/p/51");
});

test("activate keeps this build's caches and the previous build's, and deletes the older ones", async () => {
  const sw = await worker({ ...BUILD, version: "c3" }, SITE);
  for (const name of ["k-a1", "k-a1-pages", "k-b2", "k-b2-pages"]) await sw.caches.open(name);
  await sw.dispatch("install");
  await sw.dispatch("activate");
  assertDeepEq(await sw.caches.keys(), ["k-b2", "k-b2-pages", "k-c3"]);
});

test("a tab still on the previous build loads its lazy chunk from the previous build's cache", async () => {
  const sw = await worker({ ...BUILD, version: "b2" }, SITE);
  const old = await sw.caches.open("k-a1");
  await old.put("/assets/pages/report.a1.js", response("old page chunk"));
  await sw.dispatch("install");
  await sw.dispatch("activate");
  sw.offline = true;
  assertEq((await sw.dispatch("fetch", get("/assets/pages/report.a1.js"))).body, "old page chunk");
});

test("the worker says which build it caches, on the message's port", async () => {
  const sw = await worker(BUILD, SITE);
  const answers = [];
  await sw.dispatch("message", { data: { k: "build" }, ports: [{ postMessage: (m) => answers.push(m) }] });
  await sw.dispatch("message", { data: { k: "other" }, ports: [{ postMessage: (m) => answers.push(m) }] });
  assertDeepEq(answers, ["/assets/app.a1.js"]);
});

test("a development worker caches nothing, answers nothing and deletes what a release worker cached", async () => {
  const sw = await worker(null, SITE);
  for (const name of ["k-a1", "k-a1-pages"]) await sw.caches.open(name);
  await sw.dispatch("install");
  await sw.dispatch("activate");
  assertDeepEq(await sw.caches.keys(), []);
  assertEq(sw.listeners.fetch, undefined, "no fetch handler");
  assertEq(sw.listeners.message, undefined, "no build to tell");
  assertDeepEq(sw.fetched, []);
});

test("one worker: the offline handlers sit next to the notification and push handlers", async () => {
  const sw = await worker(BUILD, SITE, { notifications: true });
  for (const type of ["install", "activate", "fetch", "message", "notificationclick", "push"]) {
    assert(sw.listeners[type]?.length > 0, `a ${type} listener`);
  }
  await sw.dispatch("install");
  await sw.dispatch("activate");
  assertDeepEq(await sw.caches.keys(), ["k-a1"]);
});
