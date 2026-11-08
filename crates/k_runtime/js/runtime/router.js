/**
 * @module k:runtime/router
 *
 * The client router (docs/compiler/runtime.md, "Router";
 * docs/design/12-routing.md): renders the page of the current URL from the
 * generated route table, intercepts internal link clicks and `popstate`,
 * and renders each page in its own scope, disposed when it is left.
 *
 * A route is `{ name, parse(url) → params | null, load() → Promise<{ layouts, render(params, ctx) }>, guards? }`;
 * `url` is `{ pathname, search }`; `layouts` are the page's layout
 * components, outermost first, each called with `{ content }` and its
 * context. The table is sorted by specificity, so
 * the first route whose `parse` matches wins.
 *
 * `guards` are the page's `requires` the browser can check against the
 * `Session` mirror (docs/design/26-auth-and-sessions.md): `["auth"]` and
 * `["role", caseName]`. A page they refuse is never rendered in the
 * browser: the router leaves for a full page load of its URL, which the
 * server answers (its 303 to the sign-in page with the return URL, or
 * 403), both when navigating to it and when the mirror loses what the
 * shown page requires (sign-out, `endAll`, an ended session).
 */

import { hydrate } from "./hydrate.js";
import { gateSession, seedSession, sessionMirror } from "./rpc.js";
import { batch, root } from "./signals.js";
import { splitTarget } from "./std.js";
import { slotMarkers } from "./view.js";

/** The running router, if any. */
let router = null;

/** An absolute URL's scheme (`https:`, `mailto:`). */
const SCHEME = new RegExp("^[a-zA-Z][a-zA-Z0-9+.-]*:");

/**
 * `true` when the `Session` mirror `m` satisfies `guards`: each needs a
 * user, and `["role", name]` one whose mirrored roles include `name`. A
 * mirror that lists no roles can't tell, so the page is the server's to
 * decide.
 * @param {Array<Array<string>>|undefined} guards
 * @param {{user: object|null}} m
 * @returns {boolean}
 */
function allows(guards, m) {
  const u = m.user;
  return !guards?.length || (!!u && guards.every((g) => g[0] !== "role" || u.roles?.includes?.(g[1]) === true));
}

/**
 * Leaves the app for a full page load of `url`, which the server answers;
 * the router does nothing more, so nothing renders in the meantime.
 */
function leave(r, url) {
  r.left = true;
  r.navigation++;
  const loc = globalThis.location;
  if (loc) loc.href = url;
}

/**
 * Renders the layouts of `page` from level `i` inward, then the page, each
 * in a root of its own whose disposal `r.mounted` (layouts) and `r.page`
 * keep; a layout records the markers of its slot when the slot renders.
 * Returns the outermost node rendered.
 */
function mountRoute(r, page, params, ctx, i) {
  const layouts = page.layouts ?? [];
  if (i === layouts.length) {
    return root((dispose) => {
      r.page = dispose;
      return page.render(params, ctx);
    });
  }
  return root((dispose) => {
    const level = { layout: layouts[i], dispose, slot: null };
    r.mounted[i] = level;
    const content = () => {
      level.slot = slotMarkers();
      return mountRoute(r, page, params, ctx, i + 1);
    };
    return layouts[i]({ content }, { ...ctx, key: `L${i}` });
  });
}

/**
 * The route of `r` that `url` matches and its parameters, else the
 * not-found page's, else `null`.
 */
function matchOf(r, url) {
  const parts = splitTarget(url);
  for (const route of r.routes) {
    const params = route.parse(parts);
    if (params !== null && params !== undefined) return { route, params };
  }
  return r.notFound ? { route: r.notFound, params: {} } : null;
}

/** `true` when the current `Session` mirror refuses the page of `match`. */
function refuses(match) {
  return match !== null && !allows(match.route.guards, sessionMirror.peek());
}

/**
 * Renders the page for `url` into the router's container. The layouts it
 * shares with the mounted page, outermost first, stay mounted: only what
 * is inside the innermost shared one's slot is disposed and rendered.
 * Except for the `first` page, whose guards the server checked when it
 * answered the document, a page whose guards the mirror refuses is a
 * full page load instead.
 */
async function show(url, first) {
  const r = router;
  if (r === null || r.left) return;
  const id = ++r.navigation;
  const match = matchOf(r, url);
  if (match === null) return;
  r.at = { route: match.route, url };
  if (!first && refuses(match)) return leave(r, url);
  const page = await match.route.load();
  if (id !== r.navigation) return;
  const data = r.data;
  r.data = null;
  const layouts = page.layouts ?? [];
  batch(() => {
    let kept = 0;
    while (kept < r.mounted.length && kept < layouts.length && r.mounted[kept].layout === layouts[kept]) kept++;
    r.page?.();
    r.page = null;
    while (r.mounted.length > kept) r.mounted.pop().dispose();
    r.shown = match.route;
    // Each mounted page gets its route's container (`.route` services).
    const ctx = { router: r, c: r.c() };
    const container = r.container;
    // The first page of a server-rendered document hydrates: it claims
    // the server's nodes and reads its loads from `k-data`.
    if (data) {
      hydrate(container, (h) => mountRoute(r, page, match.params, { ...h, ...ctx }, 0), data);
      return;
    }
    const [start, end] = kept ? r.mounted[kept - 1].slot : [null, null];
    const parent = kept ? end.parentNode : container;
    for (let n = kept ? start.nextSibling : container.firstChild; n !== null && n !== end;) {
      const next = n.nextSibling;
      parent.removeChild(n);
      n = next;
    }
    const node = mountRoute(r, page, match.params, ctx, kept);
    if (node) parent.insertBefore(node, end);
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
 * @param {Array<{name: string, parse: (url: {pathname: string, search: string}) => object|null, load: () => Promise<{render: Function}>, guards?: Array<Array<string>>}>} routes
 * @param {Element} container
 * @param {object} [options] `notFound`: the route of the not-found page;
 * `data`: the `k-data` payload of a server-rendered document, whose first
 * page is hydrated instead of rendered; `c`: makes the dependency
 * container of a mounted page (codegen.md §13), read as `ctx.c`
 * @returns {Promise<void>} settles once the first page is shown
 */
export function startRouter(routes, container, options) {
  const r = { routes, container, mounted: [], page: null, navigation: 0, notFound: options?.notFound ?? null, data: options?.data ?? null, c: options?.c ?? Object };
  router = r;
  // The guards read who is signed in from the document's mirror, whether
  // or not anything has read `Session` yet.
  seedSession(r.data);
  // Before a new mirror is set: the shown page it refuses is disposed at
  // once, so its body never reads it, and the page being navigated to (or
  // still shown) is left for the server when the mirror refuses it.
  gateSession((m) => {
    if (router !== r || r.left) return;
    if (!allows(r.shown?.guards, m)) {
      r.page?.();
      r.page = null;
    }
    if (r.at && !allows(r.at.route.guards, m)) leave(r, r.at.url);
  });
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
  return show(loc ? loc.pathname + loc.search : "/", true);
}

/**
 * Navigates to `url`: an internal path pushes history and renders its page;
 * another origin, or a page whose guards the `Session` mirror refuses, is
 * a full page load. Without a running router (server
 * rendering, tests) it only pushes history; once the router has left for
 * a full page load (a refused guard) it does nothing.
 * @param {string} url
 * @returns {Promise<void>|undefined}
 */
export function navigate(url) {
  const r = router;
  if (r?.left) return undefined;
  const loc = globalThis.location;
  const origin = loc?.origin ?? "";
  if (SCHEME.test(url) && !url.startsWith(origin + "/")) {
    if (loc) loc.href = url;
    return undefined;
  }
  const path = url.startsWith(origin) && origin !== "" ? url.slice(origin.length) : url;
  // A page the mirror refuses is loaded, not pushed: the load makes the
  // history entry, so Back returns to the page shown now.
  if (r && refuses(matchOf(r, path))) return leave(r, path);
  if (globalThis.history) globalThis.history.pushState(null, "", path);
  return show(path);
}
