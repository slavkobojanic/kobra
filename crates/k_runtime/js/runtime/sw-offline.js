/**
 * @module k:runtime/sw-offline
 *
 * The offline half of the service worker `k-sw.js`
 * (docs/design/54-offline.md; docs/compiler/runtime.md §12.20), which
 * `k build` puts before `runtime/sw.js` for an `@Offline` app, after the
 * line that sets `self.kOffline` to the build's list:
 *
 * ```js
 * self.kOffline = { version, build, files, shell };
 * ```
 *
 * `version` names the build's cache (`k-<version>`, and `k-<version>-pages`
 * for the server-rendered pages kept on the way), `build` is the path of
 * its entry chunk, `files` every client file below `/assets/` but the
 * source maps, and `shell` the client-rendered app's document (`/`) or
 * `null`. A development worker (`k dev`, `k run` without `--release`)
 * sets `self.kOffline` to `null`: it caches nothing, answers no request,
 * and deletes what a release worker cached on the same origin.
 *
 * - `install` caches `files` and the shell into `k-<version>`; one that
 *   can't be fetched fails the install, and the browser keeps the old
 *   worker and tries again on its next update check. A shell answered
 *   `no-store` (it carries a session) isn't kept, and its navigations go
 *   to the network first, as a server-rendered app's do.
 * - `activate` deletes the caches of every build but this one and the one
 *   before it, which a tab still running that build may load its lazy
 *   chunks from.
 * - `fetch` answers only `GET`s of this origin: a file below `/assets/`
 *   from the caches (else the network, nothing kept); a navigation with
 *   the cached shell, else from the network, keeping a `200` HTML page not
 *   marked `no-store` (at most `PAGES` per build, oldest dropped first),
 *   and from the caches when the network fails. Everything else (rpcs,
 *   streams, `/k-sw.js`) goes to the network untouched, so an rpc made
 *   offline fails as it does without a worker, with `rpcFailure(.offline)`.
 * - `message` `{k: "build"}` answers `build` on the message's port: a page
 *   that runs another build knows a newer one has taken over
 *   (`AppUpdates`, `runtime/offline.js`).
 */

/** How many server-rendered pages one build keeps for offline use. */
const PAGES = 50;

const config = self.kOffline;
const own = config ? `k-${config.version}` : null;

/** `true` for a response that says it mustn't be stored. */
const noStore = (r) => (r.headers.get("cache-control") ?? "").includes("no-store");

self.addEventListener("install", (e) => {
  if (!config) return;
  e.waitUntil(
    caches.open(own).then(async (cache) => {
      await cache.addAll(config.files);
      if (config.shell === null) return;
      const r = await fetch(config.shell, { cache: "reload" });
      if (!r.ok) throw new Error(`the shell answered ${r.status}`);
      if (!noStore(r)) await cache.put(config.shell, r);
    }),
  );
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches.keys().then((names) => {
      // A build's caches are `k-<version>` and `k-<version>-pages`.
      const build = (n) => n.slice(0, n.indexOf("-", 2) >>> 0);
      const builds = names.filter((n) => n.startsWith("k-") && build(n) === n && n !== own);
      const keep = own ? [own, builds.at(-1)] : [];
      return Promise.all(names.filter((n) => n.startsWith("k-") && !keep.includes(build(n))).map((n) => caches.delete(n)));
    }),
  );
});

if (config) {
  self.addEventListener("fetch", (e) => {
    const r = e.request;
    const url = new URL(r.url);
    if (r.method !== "GET" || url.origin !== self.location.origin) return;
    if (url.pathname.startsWith("/assets/")) {
      e.respondWith(caches.match(r).then((hit) => hit ?? fetch(r)));
    } else if (r.mode === "navigate") {
      e.respondWith(navigation(e));
    }
  });

  self.addEventListener("message", (e) => {
    if (e.data?.k === "build") e.ports?.[0]?.postMessage(config.build);
  });
}

/**
 * The answer to a navigation: the cached shell, else the network (the page
 * kept for later), else a page kept earlier, else a network error.
 * @param {FetchEvent} e
 * @returns {Promise<Response>}
 */
async function navigation(e) {
  const r = e.request;
  if (config.shell !== null) {
    const shell = await caches.match(config.shell, { cacheName: own });
    if (shell) return shell;
  }
  try {
    const answer = await fetch(r);
    if (answer.status === 200 && (answer.headers.get("content-type") ?? "").includes("html") && !noStore(answer)) {
      e.waitUntil(keep(r, answer.clone()));
    }
    return answer;
  } catch {
    return (await caches.match(r, { ignoreVary: true })) ?? Response.error();
  }
}

/**
 * Keeps `answer` for `r` in this build's pages, dropping the oldest page
 * beyond `PAGES`. A full quota only means the page isn't kept.
 * @param {Request} r
 * @param {Response} answer
 */
async function keep(r, answer) {
  try {
    const pages = await caches.open(`${own}-pages`);
    await pages.put(r, answer);
    const keys = await pages.keys();
    await Promise.all(keys.slice(0, Math.max(0, keys.length - PAGES)).map((k) => pages.delete(k)));
  } catch {
    // Over the quota, or the cache was deleted by a newer worker.
  }
}
