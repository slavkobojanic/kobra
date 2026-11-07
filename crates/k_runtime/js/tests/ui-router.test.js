// The client router (docs/compiler/runtime.md §10, docs/design/12-routing.md):
// rendering the current page, intercepting internal links, `popstate`,
// parameters from the path and the query, `navigate`, superseded
// navigations and disposal of the page that is left.
//
// The test isolate has no `window`, `location` or `history`, so each test
// installs the small fakes below on `globalThis`. Assumptions beyond
// runtime.md §10 (to be pinned there): the router reads
// `globalThis.location` (`pathname`, `search`, `origin`, `href`), writes
// `globalThis.history.pushState(state, "", url)`, listens for `popstate`
// with `globalThis.addEventListener`, intercepts clicks on `a` elements
// inside `container` with an `href` attribute, and renders a route by
// calling `(await route.load()).render(params, ctx)` inside a fresh scope.
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { navigate, onCleanup, startRouter } from "k:runtime";

const ROUTER = "router (runtime.md §10)";
const ORIGIN = "https://shop.example";

/** Installs a fake location, history and window event target at `path`. */
function browser(path) {
  const listeners = new Map();
  const pushed = [];
  const location = {
    origin: ORIGIN,
    get href() { return ORIGIN + this.pathname + this.search; },
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
  return { location, pushed, back };
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
