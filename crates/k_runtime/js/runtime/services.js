/**
 * @module k:runtime/services
 *
 * Dependency injection in the browser and the client bodies of the
 * stdlib's platform services (docs/compiler/runtime.md, "Services";
 * docs/design/10-services.md). The generated client entry builds the
 * containers (codegen.md §13); this module supplies what they hold that
 * the compiler doesn't generate: `Clock`, `Theme`, `BrowserStorage`,
 * `FetchHttpClient`, the `Session` mirror, the `@Native` members the
 * browser implements, and `provide` for `@Scope(.component)` services.
 */

import { isHydrating } from "./hydrate.js";
import { signal } from "./signals.js";
import { KPanic, panic } from "./values.js";
import { sessionMirror } from "./rpc.js";
import { HTTP_CODES, Instant, parseEmail, parseInstant, parseUuid } from "./std.js";

/**
 * A `ctx` whose container answers `name` with `value` and everything
 * else as `ctx.c` does: the subtree of the component that injects a
 * `@Scope(.component)` service shares that one instance.
 * @param {{c: object}} ctx
 * @param {string} name the service's container name
 * @param {unknown} value
 * @returns {object}
 */
export function provide(ctx, name, value) {
  return { ...ctx, c: new Proxy(ctx.c, { get: (t, k) => (k === name ? value : t[k]) }) };
}

/**
 * Runs `f` after the hydration pass when one is running (a client-only
 * service's `init()`, so the first paint matches the server's), else now.
 * @param {() => void} f
 */
export function afterHydration(f) {
  if (isHydrating()) queueMicrotask(f);
  else f();
}

/**
 * The browser's `k.time.Clock`. While the first page hydrates, `now` is
 * the server's render instant (`k-data`), so the claimed markup matches;
 * afterwards it is the system time. Timers use `setTimeout`.
 * @param {{now?: string}|null} data the `k-data` payload
 * @returns {object}
 */
export function clock(data) {
  const rendered = data && data.now ? parseInstant(data.now) : null;
  const timer = (every, d, f) => {
    const id = (every ? setInterval : setTimeout)(f, d);
    return { cancel: () => (every ? clearInterval : clearTimeout)(id) };
  };
  return {
    /**
     * The current time; while hydrating, the time the server rendered at, so
     * the first client render matches the server's HTML.
     */
    get now() {
      return isHydrating() && rendered ? rendered : new Instant(Date.now());
    },
    every: (d, f) => timer(1, d, f),
    after: (d, f) => timer(0, d, f),
    sleep: (c, d) =>
      new Promise((resolve) => {
        const id = setTimeout(resolve, d);
        c?.addEventListener("abort", () => clearTimeout(id));
      }),
  };
}

/** The `k_theme` cookie's variant, or `null`. */
function themeCookie() {
  const m = new RegExp("(?:^|; )k_theme=([^;]*)").exec(globalThis.document?.cookie ?? "");
  return m ? decodeURIComponent(m[1]) : null;
}

/** Sets the theme cookie (`max-age=0` forgets it). */
function setThemeCookie(value, age) {
  document.cookie = `k_theme=${encodeURIComponent(value)}; path=/; max-age=${age}; samesite=lax`;
}

/**
 * The browser's `k.ui.Theme` (07-themes-and-units.md): `current` is
 * reactive and starts as the remembered variant (the `k_theme` cookie) or
 * `base`; `apply` swaps the `k-theme-<variant>` class on `<html>` (with
 * `k-theme-set`, which turns the `prefers-*` media rules off) and
 * remembers the choice; `followSystem` forgets it.
 * @returns {object}
 */
export function theme() {
  const current = signal(themeCookie() ?? "base");
  const root = () => document.documentElement;
  const set = (variant, explicit) => {
    const kept = (root().getAttribute("class") ?? "").split(" ").filter((c) => c !== "" && !c.startsWith("k-theme-"));
    if (explicit) kept.push("k-theme-set");
    if (variant !== "base") kept.push("k-theme-" + variant.replace(new RegExp("[A-Z]", "g"), (c) => "-" + c.toLowerCase()));
    root().setAttribute("class", kept.join(" "));
    current.set(variant);
  };
  return {
    /**
     * The applied theme variant (`"base"` when following the system), read
     * reactively.
     */
    get current() {
      return current.get();
    },
    /**
     * Applies `variant` and remembers it for a year in the theme cookie, so
     * the server renders it too.
     * @param {string} variant a theme variant's name
     */
    apply(variant) {
      set(variant, true);
      setThemeCookie(variant, 31536000);
    },
    /**
     * Returns to the base theme, which follows the system's preference, and
     * clears the theme cookie.
     */
    followSystem() {
      set("base", false);
      setThemeCookie("", 0);
    },
  };
}

/**
 * The browser's `k.storage.BrowserStorage` over `window.localStorage`,
 * values JSON-encoded with their codec. `R` and `W` are the generated
 * `StorageReadError` and `StorageWriteError` enums, whose cases it throws.
 * @param {object} R
 * @param {object} W
 * @returns {object}
 */
export function browserStorage(R, W) {
  const store = () => {
    try {
      const s = globalThis.localStorage;
      if (s) return s;
    } catch {
      // Storage is blocked.
    }
    return null;
  };
  return {
    /**
     * Reads and decodes the value stored under `key`; `null` when none is.
     * @param {object} codec the value's codec
     * @param {{name: string}} key the storage key
     * @returns {unknown}
     * @throws {object} `StorageReadError.unavailable` when storage is blocked,
     *   `StorageReadError.unreadable` when the stored text doesn't decode
     */
    read(codec, key) {
      const s = store();
      if (s === null) throw R.unavailable;
      let text;
      try {
        text = s.getItem(key.name);
      } catch {
        throw R.unavailable;
      }
      if (text === null) return null;
      try {
        return codec.decode(JSON.parse(text), "");
      } catch (e) {
        if (e instanceof KPanic) throw e;
        throw R.unreadable;
      }
    },
    /**
     * Encodes and stores `value` under `key`.
     * @param {object} codec the value's codec
     * @param {{name: string}} key the storage key
     * @param {unknown} value
     * @throws {object} `StorageWriteError.quotaExceeded` or
     *   `StorageWriteError.unavailable`
     */
    write(codec, key, value) {
      const s = store();
      if (s === null) throw W.unavailable;
      try {
        s.setItem(key.name, JSON.stringify(codec.encode(value)));
      } catch (e) {
        throw e && e.name === "QuotaExceededError" ? W.quotaExceeded : W.unavailable;
      }
    },
    /**
     * Removes the value stored under the key, the last argument (a codec may
     * precede it). Never throws.
     * @param {...unknown} a the call's arguments, the key last
     */
    remove(...a) {
      try {
        store()?.removeItem(a[a.length - 1].name);
      } catch {
        // Removing never throws.
      }
    },
  };
}

/** `HttpMethod`'s cases, by tag. */
const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

/** `a=1&b=2` of a `Map<String, String>`. */
function formText(map) {
  return [...map].map(([k, v]) => encodeURIComponent(k) + "=" + encodeURIComponent(v)).join("&");
}

/**
 * The browser's `k.http.FetchHttpClient` over `fetch`
 * (32-http-client.md): `send` resolves to an `HttpResponse` whatever its
 * status and rejects with `.offline` or `.timeout`; `fetchJson` (the
 * interface's default) also requires a 2xx status and decodes the body.
 * `E` and `S` are the generated `HttpError` and `HttpStatus` enums.
 * @param {object} E
 * @param {object} S
 * @returns {object}
 */
export function fetchHttpClient(E, S) {
  const status = (code) => {
    const i = HTTP_CODES.indexOf(code);
    return i < 0 ? S.other(code) : S[Object.keys(S)[i]];
  };
  const json = (codec, text) => {
    try {
      return codec.decode(JSON.parse(text), "");
    } catch (e) {
      if (e instanceof KPanic) throw e;
      throw E.decoding(String(e && e.message ? e.message : e));
    }
  };
  const send = (c, r) =>
    new Promise((resolve, reject) => {
      const q = formText(r.query ?? []);
      const url = String(r.url) + (q ? (String(r.url).includes("?") ? "&" : "?") + q : "");
      const headers = Object.fromEntries(r.headers ?? []);
      for (const [k, v] of r.secretHeaders ?? []) headers[k] = v.reveal();
      if (r.bearerToken) headers.authorization = "Bearer " + r.bearerToken.reveal();
      const b = r.body;
      let body;
      if (b) {
        body = b.$ === 0 ? JSON.stringify(b._0) : b.$ === 2 ? formText(b._0) : b._0;
        headers["content-type"] ??= ["application/json", "text/plain; charset=utf-8", "application/x-www-form-urlencoded", b.contentType][b.$];
      }
      const ctl = new AbortController();
      c?.addEventListener("abort", () => ctl.abort());
      const timer = setTimeout(() => {
        ctl.abort();
        reject(E.timeout);
      }, r.timeout ?? 30000);
      fetch(url, { method: METHODS[r.method.$], headers, body, signal: ctl.signal }).then(
        async (res) => {
          const text = await res.text();
          clearTimeout(timer);
          resolve({
            status: status(res.status),
            isSuccess: res.ok,
            body: text,
            bodyBytes: new TextEncoder().encode(text),
            headers: headerView(headerPairs(res.headers)),
            json: (codec) => json(codec, text),
          });
        },
        () => {
          clearTimeout(timer);
          reject(E.offline);
        },
      );
    });
  return {
    send,
    /**
     * Sends `r` and decodes a successful response's JSON body with `codec`.
     * @param {AbortSignal|null} c the call's cancellation signal
     * @param {object} codec the codec of the expected type
     * @param {object} r an `HttpRequest`
     * @returns {Promise<unknown>}
     * @throws {object} `HttpError.status` for a non-2xx response, or what
     *   `send` and `json` throw
     */
    async fetchJson(c, codec, r) {
      const res = await send(c, r);
      if (!res.isSuccess) throw E.status(res.status, res.body);
      return res.json(codec);
    },
  };
}

/**
 * The `[name, value]` pairs of a `fetch` response's `Headers`; a stand-in
 * that can only `get` gives its `Content-Type`.
 * @param {Headers|{get: Function}|undefined} h
 * @returns {Array<[string, string]>}
 */
function headerPairs(h) {
  if (h && typeof h[Symbol.iterator] === "function") return [...h];
  const type = h?.get?.("content-type");
  return typeof type === "string" ? [["content-type", type]] : [];
}

/**
 * `HttpHeaders` (`{map, all}`, as `http.js` reads them) of `[name, value]`
 * pairs: a repeated header reads as its values joined with `, `.
 * @param {Array<[string, string]>} pairs
 */
function headerView(pairs) {
  const map = new Map();
  const all = new Map();
  for (const [n, v] of pairs) {
    const k = String(n).toLowerCase();
    all.set(k, [...(all.get(k) ?? []), String(v)]);
    map.set(k, map.has(k) ? `${map.get(k)}, ${v}` : String(v));
  }
  return { map, all };
}

/**
 * The browser's `k.auth.Session`: a read-only mirror of the server's
 * session, started from `k-data` and updated by rpc responses that change
 * it (`x-k-session`).
 * @param {{session?: object}|null} data the `k-data` payload
 * @returns {{user: object|null}}
 */
export function session(data) {
  if (data && data.session) sessionMirror.set(data.session);
  return {
    /**
     * The signed-in user mirrored from the server (`{ id, roles }`), or
     * `null`; reactive, so a view updates on sign-in and sign-out.
     */
    get user() {
      return sessionMirror.get().user;
    },
  };
}

/**
 * Client implementations of `@Native` stdlib members without a generated
 * body, by `module.Owner$member`, as on the server (`natives` in
 * `app.js`). {@link native} looks them up.
 */
const NATIVES = {
  "k.crypto.Uuid$random": () => crypto.randomUUID(),
  "k.crypto.Uuid$init": (t) => parseUuid(String(t)) ?? stub("k.crypto.Uuid$init")(),
  "k.crypto.Uuid$parse": (t) => parseUuid(String(t)),
  // An `EmailAddress` is its text, as on the server.
  "k.text.EmailAddress$init": (t) => parseEmail(String(t)) ?? panic(`not an email address: ${JSON.stringify(t)}`),
  "k.text.EmailAddress$parse": (t) => parseEmail(String(t)),
  "k.storage.StorageKey$init": (name) => ({ name }),
};

/**
 * The browser's implementation of the `@Native` stdlib member `name`
 * (`module.Owner$member`), or a stand-in that panics with the name when
 * it is called, constructed or read from.
 * @param {string} name
 * @returns {Function}
 */
export function native(name) {
  return NATIVES[name] ?? stub(name);
}

/**
 * `true` when the browser implements the `@Native` stdlib member `name`
 * (as opposed to the panicking stand-in {@link native} gives otherwise).
 * @param {string} name
 * @returns {boolean}
 */
export function hasNative(name) {
  return Object.hasOwn(NATIVES, name);
}

/**
 * A stand-in for the missing browser native `name`: calling or
 * constructing it panics with its name, and any member of it is a
 * stand-in for `name.member`.
 * @param {string} name the member's qualified name
 * @returns {Function} a proxy
 */
function stub(name) {
  const fail = () => {
    throw new KPanic(`${name} is not implemented in the browser yet`);
  };
  return new Proxy(function missing() {}, {
    apply: fail,
    construct: fail,
    get: (_t, k) => (typeof k === "symbol" ? undefined : stub(`${name}.${String(k)}`)),
  });
}
