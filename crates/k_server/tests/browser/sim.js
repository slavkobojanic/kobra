/**
 * @module k:/test/sim.js
 *
 * A simulated browser tab for the browser-level tests of the examples
 * (`crates/k_server/tests/browser.rs`, docs/compiler/testing.md,
 * "Browser tests"). It installs the fake DOM and the globals a K client
 * reads (`location`, `history`, `localStorage`, `document.cookie`,
 * `fetch`, `matchMedia`), loads a server-rendered document's body into
 * the DOM, and queues every same-origin `fetch` in an outbox the Rust
 * driver forwards to a real K server; other origins are answered by a
 * scripted fake (`external`).
 *
 * The driver calls {@link boot}, then imports the client entry, then
 * alternates {@link drain} and {@link deliver} until nothing is in flight.
 */

import { installDom } from "k:testing";

const tab = {
  origin: "http://localhost",
  outbox: [],
  waiting: new Map(),
  next: 0,
  /** Every `fetch`, in order: `{method, url, body}`. */
  fetches: [],
  /** Full page loads (`location.href = …`). */
  loads: [],
  /** The pushed history entries. */
  pushed: [],
  cookies: new Map(),
  storage: new Map(),
  listeners: new Map(),
  external: null,
  /** The container's element nodes before the client ran. */
  server: [],
  serverHtml: null,
};

/** The tab's state, for assertions. */
export function state() {
  return tab;
}

function setUrl(path) {
  const q = path.indexOf("?");
  tab.location.pathname = q < 0 ? path : path.slice(0, q);
  tab.location.search = q < 0 ? "" : path.slice(q);
}

/**
 * Installs a fresh DOM and globals, and loads `html` (a server document)
 * at `url`.
 * @param {{url: string, html: string|null, storage?: Record<string,string>, cookies?: Record<string,string>}} o
 */
export function boot(o) {
  const doc = installDom();
  tab.location = {
    origin: tab.origin,
    pathname: "/",
    search: "",
    get href() {
      return tab.origin + this.pathname + this.search;
    },
    set href(v) {
      tab.loads.push(String(v));
    },
  };
  setUrl(o.url);
  globalThis.location = tab.location;
  globalThis.history = {
    pushState(_s, _t, path) {
      setUrl(String(path));
      tab.pushed.push(String(path));
    },
    replaceState(_s, _t, path) {
      setUrl(String(path));
    },
  };
  globalThis.addEventListener = (type, fn) => {
    const list = tab.listeners.get(type) ?? [];
    list.push(fn);
    tab.listeners.set(type, list);
  };
  globalThis.removeEventListener = () => {};
  for (const [k, v] of Object.entries(o.storage ?? {})) tab.storage.set(k, v);
  globalThis.localStorage = {
    getItem: (k) => (tab.storage.has(k) ? tab.storage.get(k) : null),
    setItem: (k, v) => tab.storage.set(k, String(v)),
    removeItem: (k) => tab.storage.delete(k),
  };
  for (const [k, v] of Object.entries(o.cookies ?? {})) tab.cookies.set(k, v);
  Object.defineProperty(doc, "cookie", {
    get: () => [...tab.cookies].map(([k, v]) => `${k}=${v}`).join("; "),
    set: (text) => {
      const [pair, ...attrs] = String(text).split(";");
      const eq = pair.indexOf("=");
      const name = pair.slice(0, eq).trim();
      if (attrs.some((a) => a.trim().toLowerCase() === "max-age=0")) tab.cookies.delete(name);
      else tab.cookies.set(name, pair.slice(eq + 1).trim());
    },
  });
  globalThis.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  globalThis.fetch = fakeFetch;
  // Fake timers: they fire only when a test calls `advance(ms)`.
  tab.now = 0;
  tab.timers = new Map();
  let timerIds = 0;
  const schedule = (repeat) => (fn, ms = 0) => {
    const id = ++timerIds;
    tab.timers.set(id, { fn, at: tab.now + Number(ms), every: repeat ? Math.max(1, Number(ms)) : 0 });
    return id;
  };
  globalThis.setTimeout = schedule(false);
  globalThis.setInterval = schedule(true);
  globalThis.clearTimeout = (id) => tab.timers.delete(id);
  globalThis.clearInterval = (id) => tab.timers.delete(id);
  let uuids = 0;
  globalThis.crypto = {
    randomUUID: () => `00000000-0000-4000-8000-${String(++uuids).padStart(12, "0")}`,
  };
  if (o.html === null) {
    doc.body.innerHTML = '<div id="k-app"></div>';
  } else {
    const start = o.html.indexOf("<body>");
    const end = o.html.lastIndexOf("</body>");
    if (start < 0 || end < 0) throw new Error("the document has no body");
    doc.body.innerHTML = o.html.slice(start + 6, end);
    const m = o.html.match(new RegExp('<html lang="[^"]*"(?: class="([^"]*)")?>'));
    if (m && m[1]) doc.documentElement.setAttribute("class", m[1]);
  }
  const app = doc.getElementById("k-app");
  tab.server = elements(app);
  tab.serverHtml = app.innerHTML;
}

/** The element nodes under `root`, in tree order. */
export function elements(root) {
  const out = [];
  const visit = (n) => {
    for (const c of n.childNodes) {
      if (c.nodeType === 1) out.push(c);
      visit(c);
    }
  };
  visit(root);
  return out;
}

/** `true` when `node` is in the document. */
function connected(node) {
  for (let n = node; n !== null; n = n.parentNode) if (n === document) return true;
  return false;
}

/**
 * Hydration report: the server's element nodes the client replaced
 * (a mismatch drops a range's server nodes), and whether the markup
 * changed.
 * @returns {{replaced: string[], same: boolean, before: string, after: string}}
 */
export function hydrationReport() {
  const app = document.getElementById("k-app");
  const replaced = tab.server.filter((n) => !connected(n)).map((n) => n.outerHTML.slice(0, 120));
  const after = app.innerHTML;
  return { replaced, same: after === tab.serverHtml, before: tab.serverHtml, after };
}

function headersOf(h) {
  const out = {};
  if (h === undefined || h === null) return out;
  if (typeof h.forEach === "function" && !Array.isArray(h)) {
    h.forEach((v, k) => {
      out[String(k).toLowerCase()] = String(v);
    });
    return out;
  }
  for (const [k, v] of Array.isArray(h) ? h : Object.entries(h)) out[String(k).toLowerCase()] = String(v);
  return out;
}

function response(r) {
  const headers = new Map(Object.entries(r.headers ?? {}).map(([k, v]) => [k.toLowerCase(), v]));
  const body = r.body ?? "";
  return {
    ok: r.status >= 200 && r.status < 300,
    status: r.status,
    statusText: "",
    url: r.url ?? "",
    headers: {
      get: (n) => headers.get(String(n).toLowerCase()) ?? null,
      has: (n) => headers.has(String(n).toLowerCase()),
      forEach: (f) => headers.forEach((v, k) => f(v, k)),
    },
    text: async () => body,
    json: async () => JSON.parse(body),
    arrayBuffer: async () => new TextEncoder().encode(body).buffer,
  };
}

function fakeFetch(input, init = {}) {
  const url = typeof input === "string" ? input : String(input?.url ?? input);
  const method = String(init.method ?? "GET").toUpperCase();
  const body = init.body === undefined || init.body === null ? "" : String(init.body);
  tab.fetches.push({ method, url, body });
  const path = url.startsWith(tab.origin + "/") ? url.slice(tab.origin.length) : url;
  if (!path.startsWith("/") || path.startsWith("//")) {
    if (tab.external === null) return Promise.reject(new TypeError(`no network in tests: ${url}`));
    return Promise.resolve(tab.external(url, { method, body, headers: headersOf(init.headers) })).then(response);
  }
  return new Promise((resolve, reject) => {
    const id = tab.next++;
    const headers = headersOf(init.headers);
    const jar = document.cookie;
    if (jar !== "") headers.cookie = jar;
    tab.outbox.push({ id, method, path, headers, body });
    tab.waiting.set(id, { resolve, reject });
    init.signal?.addEventListener?.("abort", () => {
      if (tab.waiting.delete(id)) reject(new Error("aborted"));
    });
  });
}

/**
 * Takes the queued same-origin requests, as JSON.
 * @returns {string}
 */
export function drain() {
  const out = tab.outbox;
  tab.outbox = [];
  return JSON.stringify(out);
}

/**
 * Settles queued requests with the server's responses.
 * @param {Array<{id: number, status: number, headers: Record<string,string>, body: string}>} responses
 */
export function deliver(responses) {
  for (const r of responses) {
    const w = tab.waiting.get(r.id);
    if (w === undefined) continue;
    tab.waiting.delete(r.id);
    w.resolve(response(r));
  }
}

/** Answers other origins with `handler(url, init) → {status, headers, body}`. */
export function external(handler) {
  tab.external = handler;
}

// ------------------------------------------------------------ querying

/** The text of `node`, whitespace collapsed. */
export function textOf(node) {
  return node.textContent.replace(new RegExp("\\s+", "g"), " ").trim();
}

/**
 * The elements under the app container matching `tag`, and optionally
 * whose text contains `text` or which have every attribute in `attrs`.
 */
export function all(tag, text, attrs) {
  const app = document.getElementById("k-app");
  return elements(app).filter(
    (e) =>
      (tag === "*" || e.localName === tag) &&
      (text === undefined || text === null || textOf(e).includes(text)) &&
      Object.entries(attrs ?? {}).every(([k, v]) => e.getAttribute(k) === v),
  );
}

/** The single match of {@link all}; throws with the page's markup otherwise. */
export function find(tag, text, attrs) {
  const found = all(tag, text, attrs);
  if (found.length === 0) throw new Error(`no <${tag}> ${text ?? ""} ${JSON.stringify(attrs ?? {})} in ${document.getElementById("k-app").innerHTML}`);
  return found[0];
}

/** Clicks the first match of {@link find}. */
export function click(tag, text, attrs) {
  const e = find(tag, text, attrs);
  const event = new Event("click", { bubbles: true });
  event.button = 0;
  e.dispatchEvent(event);
  return e;
}

/** Types `value` into an input: sets `value` and dispatches `input`. */
export function type(input, value) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

/** Sets a checkbox and dispatches `change`. */
export function check(input, checked) {
  input.checked = checked;
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

/** Submits a form: dispatches a cancelable `submit`. */
export function submit(form) {
  form.dispatchEvent(new Event("submit", { bubbles: true }));
}

/** The app container's markup. */
export function html() {
  return document.getElementById("k-app").innerHTML;
}

/** The text of the app container. */
export function text() {
  return textOf(document.getElementById("k-app"));
}

/** Advances the fake clock by `ms`, firing the timers that fall due. */
export function advance(ms) {
  const end = tab.now + ms;
  for (;;) {
    let next = null;
    for (const [id, t] of tab.timers) if (t.at <= end && (next === null || t.at < next[1].at)) next = [id, t];
    if (next === null) break;
    const [id, t] = next;
    tab.now = t.at;
    if (t.every) t.at += t.every;
    else tab.timers.delete(id);
    t.fn();
  }
  tab.now = end;
}
