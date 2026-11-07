/**
 * @module k:runtime/router
 *
 * The client router (docs/compiler/runtime.md, "Router";
 * docs/design/12-routing.md): renders the page of the current URL from the
 * generated route table, intercepts internal link clicks and `popstate`,
 * and renders each page in its own scope, disposed when it is left.
 *
 * A route is `{ name, parse(url) → params | null, load() → Promise<{ render(params, ctx) }> }`;
 * `url` is `{ pathname, search }`. The table is sorted by specificity, so
 * the first route whose `parse` matches wins.
 */

import { hydrate } from "./hydrate.js";
import { batch, root } from "./signals.js";
import { splitTarget } from "./std.js";

/** The running router, if any. */
let router = null;

/** An absolute URL's scheme (`https:`, `mailto:`). */
const SCHEME = new RegExp("^[a-zA-Z][a-zA-Z0-9+.-]*:");

/** Renders the page for `url` into the router's container. */
async function show(url) {
  const r = router;
  if (r === null) return;
  const id = ++r.navigation;
  const parts = splitTarget(url);
  let match = null;
  for (const route of r.routes) {
    const params = route.parse(parts);
    if (params !== null && params !== undefined) {
      match = { route, params };
      break;
    }
  }
  if (match === null) {
    match = r.notFound ? { route: r.notFound, params: {} } : null;
    if (match === null) return;
  }
  const page = await match.route.load();
  if (id !== r.navigation) return;
  const data = r.data;
  r.data = null;
  batch(() => {
    if (r.dispose !== null) r.dispose();
    r.dispose = null;
    const container = r.container;
    root((dispose) => {
      r.dispose = dispose;
      // The first page of a server-rendered document hydrates: it claims
      // the server's nodes and reads its loads from `k-data`.
      if (data) return hydrate(container, (ctx) => page.render(match.params, { ...ctx, router: r }), data);
      for (const c of [...container.childNodes]) container.removeChild(c);
      const node = page.render(match.params, { router: r });
      if (node) container.appendChild(node);
    });
  });
}

/** The `a` element a click landed in, inside `container`, if any. */
function linkOf(target, container) {
  for (let n = target; n !== null && n !== undefined && n !== container; n = n.parentNode) {
    if (n.nodeType === 1 && n.localName === "a") return n;
  }
  return null;
}

/**
 * Starts the router over the generated route table: renders the current
 * URL's page and handles link clicks and `popstate` from then on.
 * @param {Array<{name: string, parse: (url: {pathname: string, search: string}) => object|null, load: () => Promise<{render: Function}>}>} routes
 * @param {Element} container
 * @param {object} [options] `notFound`: the route of the not-found page;
 * `data`: the `k-data` payload of a server-rendered document, whose first
 * page is hydrated instead of rendered
 * @returns {Promise<void>} settles once the first page is shown
 */
export function startRouter(routes, container, options) {
  router = { routes, container, dispose: null, navigation: 0, notFound: options?.notFound ?? null, data: options?.data ?? null };
  const loc = globalThis.location;
  container.addEventListener("click", (e) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const a = linkOf(e.target, container);
    if (a === null) return;
    const href = a.getAttribute("href");
    const target = a.getAttribute("target");
    if (href === null || (target !== null && target !== "" && target !== "_self")) return;
    const origin = globalThis.location?.origin ?? "";
    let path = href;
    if (href.startsWith(origin + "/")) path = href.slice(origin.length);
    else if (!href.startsWith("/") || href.startsWith("//")) return;
    e.preventDefault();
    navigate(path);
  });
  if (typeof globalThis.addEventListener === "function") {
    globalThis.addEventListener("popstate", () => {
      const l = globalThis.location;
      show(l.pathname + l.search);
    });
  }
  return show(loc ? loc.pathname + loc.search : "/");
}

/**
 * Navigates to `url`: an internal path pushes history and renders its page;
 * another origin is a full page load. Without a running router (server
 * rendering, tests) it only pushes history.
 * @param {string} url
 * @returns {Promise<void>|undefined}
 */
export function navigate(url) {
  const loc = globalThis.location;
  const origin = loc?.origin ?? "";
  if (SCHEME.test(url) && !url.startsWith(origin + "/")) {
    if (loc) loc.href = url;
    return undefined;
  }
  const path = url.startsWith(origin) && origin !== "" ? url.slice(origin.length) : url;
  if (globalThis.history) globalThis.history.pushState(null, "", path);
  return show(path);
}
