/**
 * @module k:/tests/ui-router.test.js
 *
 * The client router (docs/compiler/runtime.md §10, docs/design/12-routing.md):
 * rendering the current page, intercepting internal links, `popstate`,
 * parameters from the path and the query, `navigate`, superseded
 * navigations and disposal of the page that is left.
 *
 * The test isolate has no `window`, `location` or `history`, so each test
 * installs the small fakes below on `globalThis`. Assumptions beyond
 * runtime.md §10 (to be pinned there): the router reads
 * `globalThis.location` (`pathname`, `search`, `origin`, `href`), writes
 * `globalThis.history.pushState(state, "", url)`, listens for `popstate`
 * with `globalThis.addEventListener`, intercepts clicks on `a` elements
 * inside `container` with an `href` attribute, and renders a route by
 * calling `(await route.load()).render(params, ctx)` inside a fresh scope.
 * A route's `guards` (codegen.md §14) are checked against the `Session`
 * mirror (`sessionMirror`), which rpc responses change through
 * `x-k-session`; a full page load is a write of `location.href`.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import * as k from "k:runtime";
import { child, description, flush, navigate, onCleanup, renderEffect, root, rpc, sessionMirror, signal, startRouter, title } from "k:runtime";

const ROUTER = "router (runtime.md §10)";
const ORIGIN = "https://shop.example";

/** Installs a fake location, history and window event target at `path`. */
function browser(path) {
  const listeners = new Map();
  const pushed = [];
  /** Full page loads (`location.href = …`). */
  const loads = [];
  const location = {
    origin: ORIGIN,
    get href() { return ORIGIN + this.pathname + this.search; },
    set href(url) { loads.push(String(url)); },
    pathname: "/",
    search: "",
  };
  const go = (url) => {
    const q = url.indexOf("?");
    location.pathname = q < 0 ? url : url.slice(0, q);
    location.search = q < 0 ? "" : url.slice(q);
  };
  go(path);
  globalThis.location = location;
  globalThis.history = {
    pushState(state, title, url) { pushed.push(String(url)); go(String(url).replace(ORIGIN, "")); },
    replaceState(state, title, url) { go(String(url).replace(ORIGIN, "")); },
  };
  globalThis.addEventListener = (type, fn) => {
    const list = listeners.get(type) ?? [];
    list.push(fn);
    listeners.set(type, list);
  };
  globalThis.removeEventListener = () => {};
  const back = (url) => {
    go(url);
    for (const fn of listeners.get("popstate") ?? []) fn({ type: "popstate", state: null });
  };
  return { location, pushed, loads, back };
}

/** Parses `/segment/{param}` paths and `?a=b` queries without `URL`. */
function parser(segments) {
  return (url) => {
    const parts = url.pathname.split("/").filter((p) => p !== "");
    if (parts.length !== segments.length) return null;
    const params = {};
    for (let i = 0; i < parts.length; i++) {
      const s = segments[i];
      if (s.startsWith(":")) params[s.slice(1)] = decodeURIComponent(parts[i]);
      else if (s !== parts[i]) return null;
    }
    for (const pair of url.search.replace("?", "").split("&").filter((p) => p !== "")) {
      const [k, v = ""] = pair.split("=");
      params[decodeURIComponent(k)] = decodeURIComponent(v);
    }
    return params;
  };
}

/**
 * A route whose page renders `<main>` with a text describing its params and,
 * with `options.links`, one `a` per given `href` (or `[href, target]`).
 */
function route(name, segments, log, options = {}) {
  return {
    name,
    parse: parser(segments),
    load: async () => {
      if (options.gate) await options.gate;
      return {
        /** Renders the page, logging the render and, on cleanup, the leave. */
        render(params) {
          log.push(`render ${name} ${JSON.stringify(params)}`);
          onCleanup(() => log.push(`leave ${name}`));
          const doc = globalThis.document;
          const main = doc.createElement("main");
          main.appendChild(doc.createTextNode(`${name}${Object.keys(params).length ? " " + JSON.stringify(params) : ""}`));
          for (const link of options.links ?? []) {
            const [href, target] = Array.isArray(link) ? link : [link, null];
            const a = doc.createElement("a");
            a.setAttribute("href", href);
            if (target !== null) a.setAttribute("target", target);
            const span = doc.createElement("span");
            span.textContent = "go";
            a.appendChild(span);
            main.appendChild(a);
          }
          return main;
        },
      };
    },
  };
}

/** An empty container in the document. */
function app() {
  const container = globalThis.document.createElement("div");
  globalThis.document.body.appendChild(container);
  return { container };
}

/** The rendered link with `href` inside `container`. */
function linkTo(container, href) {
  const walk = (n) => {
    if (n.nodeType === 1 && n.localName === "a" && n.getAttribute("href") === href) return n;
    for (const c of n.childNodes) {
      const found = walk(c);
      if (found !== null) return found;
    }
    return null;
  };
  const a = walk(container);
  if (a === null) throw new Error(`no rendered link to ${href}`);
  return a;
}

/** A bubbling click with modifier keys, as a browser would dispatch it. */
function click(target, init = {}) {
  const e = new globalThis.Event("click", { bubbles: true });
  Object.assign(e, { button: 0, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false }, init);
  target.dispatchEvent(e);
  return e;
}

const LINKS = ["/product/7", "/pricing", "https://k.dev/docs", ["/pricing", "_blank"]];
/** The route table of the router tests, recording renders in `log`. */
const ROUTES = (log) => [route("home", [], log, { links: LINKS }), route("product", ["product", ":id"], log), route("pricing", ["pricing"], log)];

// ---------------------------------------------------------------- start

// Starting renders the page of the current URL into the container.
test("startRouter renders the page of the current URL", async () => {
  browser("/pricing");
  const log = [];
  const { container } = app();
  startRouter(ROUTES(log), container);
  await tick();
  assertDeepEq(log, ["render pricing {}"]);
  assertEq(container.textContent, "pricing");
});

// Path parameters are parsed into the page's params.
test("path parameters reach the page", async () => {
  browser("/product/42");
  const log = [];
  startRouter(ROUTES(log), app().container);
  await tick();
  assertDeepEq(log, ['render product {"id":"42"}']);
});

// Optional parameters come from the query string.
test("query parameters reach the page", async () => {
  browser("/product/42?tab=reviews");
  const log = [];
  startRouter(ROUTES(log), app().container);
  await tick();
  assertDeepEq(log, ['render product {"id":"42","tab":"reviews"}']);
});

// Routes are tried in table order (the table is sorted by specificity), so
// the first match wins.
test("the first matching route wins", async () => {
  browser("/product/new");
  const log = [];
  const routes = [route("newProduct", ["product", "new"], log), route("product", ["product", ":id"], log)];
  startRouter(routes, app().container);
  await tick();
  assertDeepEq(log, ["render newProduct {}"]);
});

// ---------------------------------------------------------------- links

// A click on an internal link is intercepted: history is pushed, the new page
// renders, and the browser's own navigation is prevented.
test("an internal link click navigates without a page load", async () => {
  const b = browser("/");
  const log = [];
  const { container } = app();
  startRouter(ROUTES(log), container);
  await tick();
  const e = click(linkTo(container, "/product/7"));
  await tick();
  assert(e.defaultPrevented);
  assertDeepEq(b.pushed, ["/product/7"]);
  assertEq(log[log.length - 1], 'render product {"id":"7"}');
});

// A click on an element inside a link counts as a click on the link.
test("a click inside a link's content is intercepted", async () => {
  const b = browser("/");
  const log = [];
  const { container } = app();
  startRouter(ROUTES(log), container);
  await tick();
  click(linkTo(container, "/pricing").firstChild);
  await tick();
  assertDeepEq(b.pushed, ["/pricing"]);
});

// Clicks with a modifier key or another button open a new tab or window:
// the browser handles them.
test("modified clicks are left to the browser", async () => {
  const b = browser("/");
  const { container } = app();
  startRouter(ROUTES([]), container);
  await tick();
  const link = linkTo(container, "/pricing");
  for (const init of [{ metaKey: true }, { ctrlKey: true }, { shiftKey: true }, { altKey: true }, { button: 1 }]) {
    const e = click(link, init);
    assertEq(e.defaultPrevented, false, JSON.stringify(init));
  }
  assertDeepEq(b.pushed, []);
});

// Links with a `target` or to another origin are left to the browser.
test("target and external links are left to the browser", async () => {
  const b = browser("/");
  const { container } = app();
  startRouter(ROUTES([]), container);
  await tick();
  const external = linkTo(container, "https://k.dev/docs");
  const blank = [...linkTo(container, "/pricing").parentNode.childNodes].find((n) => n.nodeType === 1 && n.getAttribute("target") === "_blank");
  assertEq(click(external).defaultPrevented, false);
  assertEq(click(blank).defaultPrevented, false);
  assertDeepEq(b.pushed, []);
});

// A click that another handler already prevented is not intercepted.
test("a prevented click is not intercepted", async () => {
  const b = browser("/");
  const { container } = app();
  startRouter(ROUTES([]), container);
  await tick();
  const link = linkTo(container, "/pricing");
  link.addEventListener("click", (e) => e.preventDefault());
  click(link);
  await tick();
  assertDeepEq(b.pushed, []);
});

// ---------------------------------------------------------------- navigation

// `navigate(url)` pushes history and renders the page.
test("navigate pushes history and renders", async () => {
  const b = browser("/");
  const log = [];
  startRouter(ROUTES(log), app().container);
  await tick();
  navigate("/product/9?tab=reviews");
  await tick();
  assertDeepEq(b.pushed, ["/product/9?tab=reviews"]);
  assertEq(log[log.length - 1], 'render product {"id":"9","tab":"reviews"}');
});

// Back and forward (`popstate`) render the page for the URL without pushing.
test("popstate renders the page without pushing history", async () => {
  const b = browser("/");
  const log = [];
  startRouter(ROUTES(log), app().container);
  await tick();
  b.back("/pricing");
  await tick();
  assertDeepEq(b.pushed, []);
  assertEq(log[log.length - 1], "render pricing {}");
});

// The same page with other parameters stays mounted: each parameter is
// reactive, so what reads a changed one updates, and a `@Load` re-runs only
// for the parameters it reads (14-async-data.md; weather-E1).
test("a navigation to the same page with other parameters updates it in place", async () => {
  browser("/product/1");
  const ids = [];
  let renders = 0;
  const routes = [{
    name: "product",
    parse: parser(["product", ":id"]),
    load: async () => ({
      render(params) {
        renders++;
        const main = globalThis.document.createElement("main");
        renderEffect(() => {
          ids.push(params.id);
          main.textContent = `product ${params.id}`;
        });
        return main;
      },
    }),
  }];
  const { container } = app();
  startRouter(routes, container);
  await tick();
  const main = container.firstChild;
  navigate("/product/2");
  await tick();
  flush();
  assertEq(renders, 1, "not rendered again");
  assertEq(container.firstChild, main, "the same nodes");
  assertEq(container.textContent, "product 2");
  assertDeepEq(ids, ["1", "2"]);
  navigate("/product/2");
  await tick();
  flush();
  assertDeepEq(ids, ["1", "2"], "an unchanged parameter notifies nothing");
});

// Leaving a page disposes its scope: its cleanups run before the next page
// renders, and its nodes are gone.
test("leaving a page disposes it", async () => {
  browser("/");
  const log = [];
  const { container } = app();
  startRouter(ROUTES(log), container);
  await tick();
  navigate("/pricing");
  await tick();
  assertDeepEq(log, ["render home {}", "leave home", "render pricing {}"]);
  assertEq(container.textContent, "pricing", "the home page's nodes are gone");
});

// A navigation that is overtaken by another before its chunk loaded never
// renders.
test("a superseded navigation never renders", async () => {
  browser("/");
  const log = [];
  let release = null;
  const gate = new Promise((r) => { release = r; });
  const routes = [route("home", [], log), route("slow", ["slow"], log, { gate }), route("pricing", ["pricing"], log)];
  const { container } = app();
  startRouter(routes, container);
  await tick();
  navigate("/slow");
  navigate("/pricing");
  await tick();
  release();
  await tick();
  assertEq(log.includes("render slow {}"), false);
  assertEq(log[log.length - 1], "render pricing {}");
  assertEq(container.textContent, "pricing");
});

// Percent-encoded parameters are decoded before the page sees them.
test("encoded parameters are decoded", async () => {
  browser("/");
  const log = [];
  startRouter(ROUTES(log), app().container);
  await tick();
  navigate("/product/a%20b?tab=x%26y");
  await tick();
  assertEq(log[log.length - 1], 'render product {"id":"a b","tab":"x&y"}');
});

// ---------------------------------------------------------------- layouts

/**
 * A layout as generated code writes one: `<div class="shell"><!--[-->`
 * slot `<!--]--></div>`, its content placed with `child`, and a log of its
 * renders and disposals.
 */
function layout(name, log) {
  return (props) => {
    log.push(`render ${name}`);
    onCleanup(() => log.push(`leave ${name}`));
    const doc = globalThis.document;
    const div = doc.createElement("div");
    div.setAttribute("class", name);
    const start = doc.createComment("[");
    const end = doc.createComment("]");
    div.appendChild(start);
    div.appendChild(end);
    child(start, end, () => props.content());
    return div;
  };
}

/** A route whose page is `<main>` inside `layouts`, outermost first. */
function laidOut(name, segments, log, layouts, pageTitle) {
  return {
    name,
    parse: parser(segments),
    load: async () => ({
      layouts,
      /** Renders the page, logging the render. */
      render(params) {
        log.push(`render ${name}`);
        onCleanup(() => log.push(`leave ${name}`));
        if (pageTitle !== undefined) title(() => pageTitle);
        const main = globalThis.document.createElement("main");
        main.appendChild(globalThis.document.createTextNode(name));
        return main;
      },
    }),
  };
}

// Layouts two pages share stay mounted across a navigation (12-routing.md):
// only the content of the innermost shared one is replaced.
test("a navigation keeps the layouts both pages share", async () => {
  browser("/");
  const log = [];
  const shell = layout("shell", log);
  const shop = layout("shop", log);
  const routes = [
    laidOut("home", [], log, [shell]),
    laidOut("product", ["product", ":id"], log, [shell, shop]),
    laidOut("cart", ["cart"], log, [shell, shop]),
    laidOut("bare", ["bare"], log, []),
  ];
  const { container } = app();
  startRouter(routes, container);
  await tick();
  const outer = container.firstChild;
  assertEq(outer.getAttribute("class"), "shell");
  assertDeepEq(log, ["render shell", "render home"]);
  log.length = 0;
  navigate("/product/7");
  await tick();
  assertDeepEq(log, ["leave home", "render shop", "render product"]);
  assertEq(container.firstChild, outer, "the shared layout's element stays");
  assertEq(container.textContent, "product");
  log.length = 0;
  navigate("/cart");
  await tick();
  assertDeepEq(log, ["leave product", "render cart"], "both layouts stay");
  log.length = 0;
  navigate("/");
  await tick();
  assertDeepEq(log, ["leave cart", "leave shop", "render home"]);
  assertEq(container.firstChild, outer);
  log.length = 0;
  navigate("/bare");
  await tick();
  assertDeepEq(log, ["leave home", "leave shell", "render bare"]);
  assertEq(container.firstChild.localName, "main");
  assertEq(container.childNodes.length, 1, "nothing of the layout is left");
});

// ---------------------------------------------------------------- titles

// `document.title` follows the metadata of what is mounted: the innermost
// title wins, and leaving a page restores its layout's (22-metadata.md).
test("document.title follows the mounted page and layouts", async () => {
  browser("/");
  globalThis.document.title = "server title";
  const log = [];
  const titled = (name) => (props) => {
    title(() => `${name} default`);
    return layout(name, log)(props);
  };
  const shell = titled("shell");
  const routes = [laidOut("home", [], log, [shell], "Home"), laidOut("about", ["about"], log, [shell])];
  startRouter(routes, app().container);
  await tick();
  assertEq(globalThis.document.title, "Home");
  navigate("/about");
  await tick();
  assertEq(globalThis.document.title, "shell default", "the layout's title once the page has none");
  navigate("/");
  await tick();
  assertEq(globalThis.document.title, "Home");
});

// A page's description is kept in `<meta name="description">` like its
// title, created when the document has none (a client-rendered shell), and
// left as the server wrote it once nothing mounted has one (todos-G).
test("the description meta follows the mounted page's description", () => {
  const head = globalThis.document.head;
  while (head.firstChild) head.removeChild(head.firstChild);
  const metas = () => [...head.childNodes].filter((n) => n.localName === "meta");
  const text = signal("All your notes.");
  const dispose = root((d) => {
    description(() => text.get());
    return d;
  });
  assertEq(metas().length, 1);
  assertEq(metas()[0].getAttribute("name"), "description");
  assertEq(metas()[0].getAttribute("content"), "All your notes.");
  text.set("Two notes.");
  flush();
  assertEq(metas().length, 1, "the same element");
  assertEq(metas()[0].getAttribute("content"), "Two notes.");
  dispose();
  assertEq(metas()[0].getAttribute("content"), "Two notes.", "kept once nothing has one");
});

test("a reactive title updates, and an empty one falls back to the layer below", () => {
  globalThis.document.title = "";
  const name = signal("Berlin");
  const ready = signal(true);
  root(() => {
    title(() => "Weather");
    title(() => ready.get() && `${name.get()} weather`);
  });
  assertEq(globalThis.document.title, "Berlin weather");
  name.set("Bern");
  flush();
  assertEq(globalThis.document.title, "Bern weather");
  ready.set(false);
  flush();
  assertEq(globalThis.document.title, "Weather", "a title that isn't ready yet");
});

// ---------------------------------------------------------------- guards

const ADA = { id: "ada", roles: [] };
const ADMIN = { id: "grace", roles: ["admin"] };

/**
 * A guarded route, as codegen.md §14 emits it: `guards` beside `parse` and
 * `load`. Its page reads the signed-in user's id as a body narrowed by
 * `requires Authenticated` does (26-auth-and-sessions.md), recording each
 * read in `seen`; `loaded` counts its chunk loads.
 */
function guarded(name, segments, log, guards, options = {}) {
  const r = {
    name,
    guards,
    parse: parser(segments),
    loaded: 0,
    seen: [],
    load: async () => {
      r.loaded++;
      if (options.gate) await options.gate;
      return {
        /** Renders the page, reading `session.user.id` reactively. */
        render() {
          log.push(`render ${name}`);
          onCleanup(() => log.push(`leave ${name}`));
          const main = globalThis.document.createElement("main");
          const t = globalThis.document.createTextNode("");
          renderEffect(() => {
            const user = sessionMirror.get().user;
            // A narrowed body has no `nil` check, so it would throw here;
            // the probe records the read instead (pages of earlier tests
            // stay mounted in their own containers).
            r.seen.push(user === null ? "NO USER" : user.id);
            t.data = `${name} for ${user?.id}`;
          });
          main.appendChild(t);
          return main;
        },
      };
    },
  };
  return r;
}

/** The guard routes: home links to the account and admin pages. */
function guardRoutes(log, options = {}) {
  return {
    home: route("home", [], log, { links: ["/account", "/admin"] }),
    account: guarded("account", ["account"], log, [["auth"]], options),
    admin: guarded("admin", ["admin"], log, [["auth"], ["role", "admin"]]),
    pricing: route("pricing", ["pricing"], log),
  };
}

/** Starts the router over `routes` (from {@link guardRoutes}). */
function startGuarded(routes) {
  const { container } = app();
  startRouter([routes.home, routes.account, routes.admin, routes.pricing], container);
  return container;
}

/**
 * An rpc whose response carries the `Session` mirror `mirror` in
 * `x-k-session`, as the server sends it when a call changed the session
 * (sign-out, `endAll`, an expired session).
 */
function rpcChangingSession(mirror) {
  const response = {
    ok: true,
    status: 200,
    headers: { get: (n) => (n === "x-k-session" ? JSON.stringify(mirror) : null) },
    text: async () => "{}",
  };
  return rpc("/t.Auth/SignOut", {}, null, { fetch: () => Promise.resolve(response) });
}

// A guest following a link to a page that requires `Authenticated` is sent
// to the server, which answers with its 303 to the sign-in page and the
// return URL (26-auth-and-sessions.md, "What a refused page does"). The
// page's chunk is never loaded and nothing is rendered with no user.
test("a guest following a link to a guarded page gets a full page load", async () => {
  sessionMirror.set({ user: null });
  const b = browser("/");
  const log = [];
  const routes = guardRoutes(log);
  const container = startGuarded(routes);
  await tick();
  const e = click(linkTo(container, "/account"));
  await tick();
  assert(e.defaultPrevented, "the router decides, not the browser");
  assertDeepEq(b.loads, ["/account"]);
  assertDeepEq(b.pushed, [], "the load makes the history entry");
  assertEq(routes.account.loaded, 0, "the guarded page's chunk is not loaded");
  assertDeepEq(routes.account.seen, []);
  assertEq(log.includes("render account"), false);
});

// A signed-in user stays on the client: no round trip.
test("a signed-in user navigates to a guarded page on the client", async () => {
  sessionMirror.set({ user: ADA });
  const b = browser("/");
  const log = [];
  const routes = guardRoutes(log);
  const container = startGuarded(routes);
  await tick();
  click(linkTo(container, "/account"));
  await tick();
  assertDeepEq(b.loads, []);
  assertDeepEq(b.pushed, ["/account"]);
  assertEq(container.textContent, "account for ada");
});

// `navigate` and back/forward check the guards like a link does.
test("navigate and popstate to a guarded page as a guest load it from the server", async () => {
  sessionMirror.set({ user: null });
  const b = browser("/");
  const log = [];
  const routes = guardRoutes(log);
  startGuarded(routes);
  await tick();
  b.back("/account");
  await tick();
  assertDeepEq(b.loads, ["/account"]);
  assertEq(routes.account.loaded, 0);
  sessionMirror.set({ user: null });
  const c = browser("/");
  const again = guardRoutes([]);
  startGuarded(again);
  await tick();
  navigate("/account?tab=security");
  await tick();
  assertDeepEq(c.loads, ["/account?tab=security"], "the query is kept, so the return URL is the whole page");
  assertEq(again.account.loaded, 0);
});

// The server answered the document of the first page after checking its
// guards, so the router doesn't check them again (and can't loop).
test("the first page is not checked again", async () => {
  sessionMirror.set({ user: ADA });
  const b = browser("/account");
  const log = [];
  const routes = guardRoutes(log);
  const container = startGuarded(routes);
  await tick();
  assertDeepEq(b.loads, []);
  assertEq(container.textContent, "account for ada");
});

// `Role` is checked against the roles the mirror lists (their case names);
// a refusal goes to the server, which answers 403. A mirror that doesn't
// list roles can't tell, so the server decides.
test("a role guard checks the mirrored roles and otherwise defers to the server", async () => {
  for (const [user, loads] of [
    [null, ["/admin"]],
    [ADA, ["/admin"]],
    [{ id: "lin" }, ["/admin"]],
    [ADMIN, []],
  ]) {
    sessionMirror.set({ user });
    const b = browser("/");
    const routes = guardRoutes([]);
    const container = startGuarded(routes);
    await tick();
    click(linkTo(container, "/admin"));
    await tick();
    assertDeepEq(b.loads, loads, JSON.stringify(user));
    if (loads.length === 0) assertEq(container.textContent, "admin for grace");
    else assertEq(routes.admin.loaded, 0, JSON.stringify(user));
  }
});

// Signing out on a guarded page (or `endAll`, or the session expiring: any
// rpc response whose mirror has no user) leaves the page the same way: it
// is disposed before the mirror changes, so its narrowed body never runs
// with no user, and the server is asked for the page again.
test("losing the user on a guarded page leaves it without rendering it for a guest", async () => {
  sessionMirror.set({ user: ADA });
  const b = browser("/account?tab=security");
  const log = [];
  const routes = guardRoutes(log);
  startGuarded(routes);
  await tick();
  assertDeepEq(routes.account.seen, ["ada"]);
  await rpcChangingSession({ user: null });
  flush();
  await tick();
  assertDeepEq(routes.account.seen, ["ada"], "the narrowed body never ran with no user");
  assert(log.includes("leave account"), "the page was disposed");
  assertEq(sessionMirror.peek().user, null, "the mirror still follows the server");
  assertDeepEq(b.loads, ["/account?tab=security"]);
  // The page is being left: what the app does next (its own
  // `Router.navigate(to: .signIn)`) no longer renders anything.
  navigate("/pricing");
  await tick();
  assertDeepEq(b.pushed, []);
  assertEq(log.includes("render pricing {}"), false);
});

// Losing a role the page requires leaves it too.
test("losing a required role on a guarded page leaves it", async () => {
  sessionMirror.set({ user: ADMIN });
  const b = browser("/admin");
  const routes = guardRoutes([]);
  startGuarded(routes);
  await tick();
  await rpcChangingSession({ user: { id: "grace", roles: [] } });
  flush();
  await tick();
  assertDeepEq(b.loads, ["/admin"]);
  assertDeepEq(routes.admin.seen, ["grace"]);
});

// A server-streaming rpc that opens is the first response to learn of a
// dead session as often as a unary one (its head clears the cookie, so no
// later response could tell): its mirror is checked before the stream is
// handed to the caller.
test("an opened stream's session mirror leaves a guarded page too", async () => {
  sessionMirror.set({ user: ADA });
  const b = browser("/account");
  const routes = guardRoutes([]);
  startGuarded(routes);
  await tick();
  const reader = { read: () => new Promise(() => {}), cancel: async () => {} };
  const response = {
    ok: true,
    status: 200,
    headers: { get: (n) => (n === "x-k-session" ? JSON.stringify({ user: null }) : null) },
    body: { getReader: () => reader },
  };
  const stream = { body: () => new Uint8Array(5), read: () => ({ opened: true }) };
  const r = await rpc("/t.Feed/Watch", {}, null, { fetch: () => Promise.resolve(response), stream });
  flush();
  await tick();
  assertDeepEq(r, { ok: true, message: { opened: true } });
  assertEq(sessionMirror.peek().user, null);
  assertDeepEq(b.loads, ["/account"]);
});

// A session change the page's guards still accept, or one on a page with
// no guards, changes nothing about the page.
test("a session change the shown page allows keeps it", async () => {
  sessionMirror.set({ user: ADA });
  const b = browser("/account");
  const routes = guardRoutes([]);
  const container = startGuarded(routes);
  await tick();
  await rpcChangingSession({ user: { id: "ada", roles: ["admin"] } });
  flush();
  await tick();
  assertDeepEq(b.loads, []);
  assertEq(container.textContent, "account for ada");
  sessionMirror.set({ user: ADA });
  const c = browser("/pricing");
  const log = [];
  startGuarded(guardRoutes(log));
  await tick();
  await rpcChangingSession({ user: null });
  await tick();
  assertDeepEq(c.loads, []);
  assertEq(log.includes("leave pricing"), false);
});

// A navigation to a guarded page that is still loading its chunk when the
// user is lost never renders it: the server is asked for it instead.
test("losing the user while a guarded page loads sends it to the server", async () => {
  sessionMirror.set({ user: ADA });
  const b = browser("/");
  const log = [];
  let release = null;
  const gate = new Promise((r) => { release = r; });
  const routes = guardRoutes(log, { gate });
  startGuarded(routes);
  await tick();
  navigate("/account");
  await tick();
  await rpcChangingSession({ user: null });
  release();
  await tick();
  assertDeepEq(b.loads, ["/account"]);
  assertDeepEq(routes.account.seen, []);
  assertEq(log.includes("render account"), false);
});

// Leaving a guarded page for an open one while the user is lost: the
// guarded page goes at once and the navigation goes on, on the client.
test("losing the user while leaving a guarded page for an open one stays on the client", async () => {
  sessionMirror.set({ user: ADA });
  const b = browser("/account");
  const log = [];
  let release = null;
  const gate = new Promise((r) => { release = r; });
  const routes = guardRoutes(log);
  routes.pricing = route("pricing", ["pricing"], log, { gate });
  const container = startGuarded(routes);
  await tick();
  navigate("/pricing");
  await tick();
  await rpcChangingSession({ user: null });
  flush();
  assertDeepEq(routes.account.seen, ["ada"]);
  assert(log.includes("leave account"));
  release();
  await tick();
  assertDeepEq(b.loads, []);
  assertEq(container.textContent, "pricing");
});

// A handler of a guarded page awaits an rpc, and while its answer comes
// back another rpc's response ends the session (03-reactivity.md, "Async
// work is owned"; 26-auth-and-sessions.md, "In the browser"). The page is
// disposed before the mirror changes, and the handler, which runs under
// the page's lifetime, stops at its `await`: the code after it, which
// reads the narrowed `session.user` without a `nil` check, never runs.
test("a guarded page's handler awaiting an rpc stops when the session ends", async () => {
  sessionMirror.set({ user: ADA });
  browser("/account");
  const log = [];
  const read = [];
  let handler = null;
  const routes = guardRoutes(log);
  routes.account = {
    name: "account",
    guards: [["auth"]],
    parse: parser(["account"]),
    load: async () => ({
      /** The page: its save handler awaits an rpc, then reads the user. */
      render() {
        onCleanup(() => log.push("leave account"));
        const $life = k.lifetime();
        handler = () => (async ($c) => {
          k.live($c, await rpc("/t.Notes/Save", {}, $c, { fetch: () => save.promise }));
          read.push(sessionMirror.peek().user.id);
        })($life);
        return globalThis.document.createElement("main");
      },
    }),
  };
  let answer = null;
  const save = { promise: new Promise((r) => { answer = r; }) };
  startGuarded(routes);
  await tick();
  const running = handler();
  // Both responses arrive in the same task: the save's answer first, the
  // sign-out's (its mirror has no user) right behind it.
  const ok = (mirror) => ({
    ok: true,
    status: 200,
    headers: { get: (n) => (n === "x-k-session" && mirror ? JSON.stringify(mirror) : null) },
    text: async () => "{}",
  });
  let signOut = null;
  const ended = rpc("/t.Auth/SignOut", {}, null, { fetch: () => new Promise((r) => { signOut = r; }) });
  answer(ok(null));
  signOut(ok({ user: null }));
  await ended;
  await running.then(() => {}, () => {});
  await tick();
  assert(log.includes("leave account"), "the page was disposed");
  assertDeepEq(read, [], "nothing after the await ran on the disposed page");
});

// ---------------------------------------------------------------- focus and scroll

/**
 * A fake browser with a history of entries and states, a window scroll
 * position and `pagehide` (53-element-references.md, "Focus the framework
 * moves"): `back()`/`forward()` move through the entries and dispatch
 * `popstate` with the entry's state, as browsers do, without scrolling
 * (`scrollRestoration` is the router's to set). `earlier` are the entries
 * before the current one, as a reload keeps them.
 */
function historyBrowser(path, state = null, earlier = []) {
  const b = browser(path);
  const entries = [...earlier, { url: path, state }];
  let at = earlier.length;
  const listeners = new Map();
  globalThis.addEventListener = (type, fn) => listeners.set(type, [...(listeners.get(type) ?? []), fn]);
  const go = (url) => {
    const q = url.indexOf("?");
    b.location.pathname = q < 0 ? url : url.slice(0, q);
    b.location.search = q < 0 ? "" : url.slice(q);
  };
  const copy = (v) => (v == null ? null : JSON.parse(JSON.stringify(v)));
  const history = {
    scrollRestoration: "auto",
    get state() { return entries[at].state; },
    pushState(s, title, url) {
      entries.splice(at + 1);
      entries.push({ url: String(url), state: copy(s) });
      at++;
      b.pushed.push(String(url));
      go(String(url));
    },
    replaceState(s, title, url) {
      entries[at] = { url: url === undefined ? entries[at].url : String(url), state: copy(s) };
    },
  };
  globalThis.history = history;
  const scroll = { x: 0, y: 0 };
  Object.defineProperty(globalThis, "scrollX", { get: () => scroll.x, configurable: true });
  Object.defineProperty(globalThis, "scrollY", { get: () => scroll.y, configurable: true });
  globalThis.scrollTo = (x, y) => { scroll.x = x; scroll.y = y; };
  const move = (by) => {
    at += by;
    go(entries[at].url.split("#")[0]);
    for (const fn of listeners.get("popstate") ?? []) fn({ type: "popstate", state: entries[at].state });
  };
  const fire = (type) => { for (const fn of listeners.get(type) ?? []) fn({ type }); };
  return { ...b, history, entries, scroll, back: () => move(-1), forward: () => move(1), fire };
}

/** A route whose page is a `main` with an `h1` naming it, and `links`. */
function headed(name, segments, options = {}) {
  return {
    name,
    parse: parser(segments),
    load: async () => ({
      render() {
        const doc = globalThis.document;
        const main = doc.createElement("main");
        const h1 = doc.createElement("h1");
        if (options.tabindex !== undefined) h1.setAttribute("tabindex", options.tabindex);
        h1.textContent = name;
        main.appendChild(h1);
        if (options.target) {
          const p = doc.createElement("p");
          p.setAttribute("id", options.target);
          main.appendChild(p);
        }
        return main;
      },
    }),
  };
}

const HEADED = () => [
  headed("home", []),
  headed("product", ["product", ":id"]),
  headed("pricing", ["pricing"], { tabindex: "0", target: "plans" }),
];

// The first page is the document's: the browser placed the focus and the
// scroll, so the router moves neither.
test("the first page keeps the focus and the scroll where they are", async () => {
  const b = historyBrowser("/");
  b.scroll.y = 120;
  const { container } = app();
  const before = globalThis.document.activeElement;
  await startRouter(HEADED(), container);
  assertEq(globalThis.document.activeElement, before);
  assertEq(b.scroll.y, 120);
  assertEq(container.getElementsByTagName("h1")[0].hasAttribute("tabindex"), false);
});

// A navigation to another page focuses its first `h1`, which gets
// `tabindex="-1"` so it can take the focus without becoming a Tab stop, and
// starts the page at the top.
test("a navigation focuses the new page's heading and starts at the top", async () => {
  const b = historyBrowser("/");
  const { container } = app();
  await startRouter(HEADED(), container);
  b.scroll.y = 500;
  await navigate("/product/7");
  const h1 = container.getElementsByTagName("h1")[0];
  assertEq(h1.textContent, "product");
  assertEq(h1.getAttribute("tabindex"), "-1");
  assertEq(globalThis.document.activeElement, h1);
  assertDeepEq([b.scroll.x, b.scroll.y], [0, 0]);
});

// A heading that is already focusable keeps its own `tabindex`.
test("a heading's own tabindex is kept", async () => {
  historyBrowser("/");
  const { container } = app();
  await startRouter(HEADED(), container);
  await navigate("/pricing");
  const h1 = container.getElementsByTagName("h1")[0];
  assertEq(h1.getAttribute("tabindex"), "0");
  assertEq(globalThis.document.activeElement, h1);
});

// A URL with a fragment starts at the fragment's element instead of the top.
test("a navigation to a fragment scrolls to its element", async () => {
  const b = historyBrowser("/");
  const { container } = app();
  await startRouter(HEADED(), container);
  b.scroll.y = 300;
  await navigate("/pricing#plans");
  const p = globalThis.document.getElementById("plans");
  assertDeepEq(p.scrolledIntoView, {});
  assertEq(b.scroll.y, 300, "scrollIntoView did the scrolling");
});

// Back and Forward put each page where it was scrolled when it was left.
test("back and forward restore the scroll position of each page", async () => {
  const b = historyBrowser("/");
  const { container } = app();
  await startRouter(HEADED(), container);
  assertEq(b.history.scrollRestoration, "manual");
  b.scroll.y = 400;
  await navigate("/product/7");
  assertEq(b.scroll.y, 0);
  b.scroll.y = 250;
  b.back();
  await tick();
  assertEq(container.getElementsByTagName("h1")[0].textContent, "home");
  assertEq(b.scroll.y, 400);
  assertEq(globalThis.document.activeElement, container.getElementsByTagName("h1")[0]);
  b.forward();
  await tick();
  assertEq(container.getElementsByTagName("h1")[0].textContent, "product");
  assertEq(b.scroll.y, 250);
});

// The same page with other parameters updates in place: the focus and the
// scroll stay where the user is (a filter typed into the query).
test("an update in place keeps the focus and the scroll", async () => {
  const b = historyBrowser("/product/7?tab=details");
  const { container } = app();
  await startRouter(HEADED(), container);
  const h1 = container.getElementsByTagName("h1")[0];
  const field = globalThis.document.createElement("input");
  globalThis.document.body.appendChild(field);
  field.focus();
  b.scroll.y = 640;
  await navigate("/product/7?tab=reviews");
  assertEq(container.getElementsByTagName("h1")[0], h1, "the page stayed mounted");
  assertEq(globalThis.document.activeElement, field);
  assertEq(b.scroll.y, 640);
});

// Leaving the document records the position in the entry's state, and the
// first page after a reload of that entry is put back there.
test("a reload restores the position the page was left at", async () => {
  const b = historyBrowser("/");
  await startRouter(HEADED(), app().container);
  b.scroll.y = 900;
  b.fire("pagehide");
  const state = b.history.state;
  assertDeepEq(state.p, [0, 900]);
  const again = historyBrowser("/", state);
  await startRouter(HEADED(), app().container);
  assertEq(again.scroll.y, 900);
});

// Going back to an entry made after a reload, whose position only the
// entry's state has, restores it from there.
test("back after a reload restores the position from the entry's state", async () => {
  const b = historyBrowser("/");
  const { container } = app();
  await startRouter(HEADED(), container);
  b.scroll.y = 75;
  await navigate("/pricing");
  const [home, pricing] = b.entries;
  const again = historyBrowser("/pricing", pricing.state, [home]);
  await startRouter(HEADED(), app().container);
  again.back();
  await tick();
  assertEq(again.scroll.y, 75);
});
