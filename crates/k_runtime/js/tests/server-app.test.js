/**
 * @module k:/tests/server-app.test.js
 *
 * The server half of an app (docs/compiler/server.md §5–§10): routing and
 * typed parameters, rpc dispatch with Connect errors, endpoints, sessions,
 * guards and the document. These drive `createApp` with hand-written specs
 * (the generated entry builds the same shape) and call the handler
 * directly, without HTTP.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import {
  Metadata,
  Out,
  theme as serverTheme,
  themeSnippet,
  requestTimeZone,
  ServerBroadcast,
  clock,
  createApp,
  param,
  parseCookies,
  parseQuery,
  redirect,
  response,
  responseBody,
  responseHeaders,
  route,
  sessionManager,
  shareState,
  splitTarget,
} from "k:runtime/server";
import { DecodeFailure, Instant, KPanic, Money, codecs, unitCase } from "k:runtime";

const enc = new TextEncoder();
let nextId = 1;

/** A request as the Rust front hands it over. */
function req(method, path, { headers = [], body = "" } = {}) {
  return { id: nextId++, method, path, headers, body: enc.encode(body), remote: "203.0.113.5" };
}

/** The value of header `name` of response `r`, or `undefined`. */
function header(r, name) {
  return r.headers.find(([n]) => n === name)?.[1];
}

/** The body of response `r` as text. */
function text(r) {
  return typeof r.body === "string" ? r.body : new TextDecoder().decode(r.body);
}

const Tab = { overview: unitCase(0), reviews: unitCase(1) };

/** A spec with one page, one rpc and one endpoint, plus overrides. */
function spec(over = {}) {
  const s = {
    name: "t",
    rendering: "server",
    pages: [
      {
        name: "product",
        match: (u) => route(u, ["product", "{id}"], [["id", param.int, true, null], ["tab", param.cases(Tab), false, () => Tab.overview]]),
        value: (p) => ({ $: 0, ...p }),
        render: (props, ctx, out) => {
          ctx.meta.title(`Product ${props.id()}`);
          out.raw("<p>");
          out.text(`product ${props.id()} ${props.tab() === Tab.reviews ? "reviews" : "overview"}`);
          out.raw("</p>");
        },
        layouts: [],
        guards: [],
      },
    ],
    redirects: [{ match: (u) => route(u, ["old", "{id}"], [["id", param.int, true, null]]), to: (p) => `/product/${p.id}` }],
    rpcs: {
      "/t.P/Echo": {
        guards: [],
        decode: (j) => [codecs.string.decode(j.text, "text")],
        validate: null,
        call: async (_c, _$c, [t]) => {
          if (t === "boom") throw new KPanic("a bug");
          if (t === "typed") throw { $: 1, available: 3 };
          return t;
        },
        encode: (v) => ({ value: v }),
        error: { name: "t.E", text: () => "outOfStock", fields: (e) => ({ available: e.available }) },
        stream: false,
      },
    },
    endpoints: [
      {
        match: (u) => route(u, ["hook"], []),
        methods: { POST: async (_e, _c, r) => responseBody(response({ $: 0 }), { $: 1, _0: `got ${new TextDecoder().decode(r.rawBody)}` }) },
        guards: [],
        session: false,
      },
    ],
    container: (st) => ({ $r: st }),
    appMeta: (m) => m.locale("en"),
    signIn: (page) => `/sign-in?returnTo=${encodeURIComponent(`/product/${page.id}`)}`,
    session: { roles: {}, decodeUser: (j) => j },
    httpStatus: [200, 404, null],
    httpBody: ["json", "text", "form", "bytes"],
    assets: { styles: ["/assets/app.css"], scripts: ["/assets/app.js"] },
  };
  return { ...s, ...over };
}

// ------------------------------------------------------------------ URLs

test("splitTarget separates the path from the query and drops the fragment", () => {
  assertDeepEq(splitTarget("/a/b?x=1#frag"), { pathname: "/a/b", search: "x=1" });
  assertDeepEq(splitTarget("/"), { pathname: "/", search: "" });
});

test("parseQuery decodes names and values, keeps the first and skips malformed pairs", () => {
  const q = parseQuery("a=1&b=x%20y&a=2&c&bad=%E0%A4%A&d=a+b");
  assertEq(q.get("a"), "1");
  assertEq(q.get("b"), "x y");
  assertEq(q.get("c"), "");
  assertEq(q.has("bad"), false);
  assertEq(q.get("d"), "a b");
});

test("Int parameters are decimal digits with an optional minus, in the safe range", () => {
  assertEq(param.int("42"), 42);
  assertEq(param.int("-7"), -7);
  for (const bad of ["4.2", "1e3", "", "+1", " 1", "99999999999999999999", "0x10"]) {
    assert(typeof param.int(bad) === "symbol", bad);
  }
});

test("enum parameters match exact case names only, never inherited properties", () => {
  const p = param.cases(Tab);
  assertEq(p("reviews"), Tab.reviews);
  for (const bad of ["Reviews", "constructor", "__proto__", "toString", ""]) assert(typeof p(bad) === "symbol", bad);
});

test("Uuid parameters need the canonical form", () => {
  assertEq(param.uuid("0B6F3C8E-2D4A-4F1B-9C3E-5A7D8E9F0A1B"), "0b6f3c8e-2d4a-4f1b-9c3e-5a7d8e9f0a1b");
  assert(typeof param.uuid("0b6f3c8e-2d4a-4f1b-9c3e") === "symbol");
});

test("a Page parameter is a path of this app, never another site", () => {
  const app = spec();
  const p = param.page(() => app);
  assertDeepEq(p("/product/3?tab=reviews"), { $: 0, id: 3, tab: Tab.reviews });
  for (const bad of ["https://evil.example/", "//evil.example/", "/\\evil", "/nowhere", "product/3"]) {
    assert(typeof p(bad) === "symbol", bad);
  }
});

test("route decodes each segment on its own and rejects empty parameters", () => {
  const segs = ["files", "{name}"];
  const ps = [["name", param.string, true, null]];
  assertDeepEq(route(splitTarget("/files/a%2Fb"), segs, ps), { name: "a/b" });
  assertEq(route(splitTarget("/files/"), segs, ps), null);
  assertEq(route(splitTarget("/files/a/b"), segs, ps), null);
  assertEq(route(splitTarget("/files/%E0%A4%A"), segs, ps), null);
  assertDeepEq(route(splitTarget("/"), [], []), {});
});

test("parseCookies reads every Cookie header and keeps the first of a name", () => {
  const c = parseCookies([["cookie", "a=1; k_session=x"], ["cookie", "k_session=y; b=2"]]);
  assertEq(c.get("k_session"), "x");
  assertEq(c.get("b"), "2");
});

// ------------------------------------------------------------------ pages

test("a page renders a document with escaped metadata, assets and k-data", async () => {
  const r = await createApp(spec())(req("GET", "/product/5?tab=reviews"));
  assertEq(r.status, 200);
  const html = text(r);
  assert(html.startsWith("<!doctype html><html lang=\"en\">"), html);
  assert(html.includes("<title>Product 5</title>"));
  assert(html.includes("product 5 reviews"));
  assert(html.includes('<link rel="stylesheet" href="/assets/app.css">'));
  assert(html.includes('<script type="module" src="/assets/app.js"></script>'));
  assert(html.includes('"session":{"user":null}'));
  assertEq(header(r, "set-cookie"), undefined);
});

test("a page's document preloads the module graph and the page's own chunks", async () => {
  const assets = {
    styles: ["/assets/app.css"],
    scripts: ["/assets/app.js"],
    preload: ["/assets/app.js", "/assets/runtime.js", "/assets/shared.js"],
    pages: { product: ["/assets/pages/product.js", "/assets/forms.js"], home: ["/assets/pages/home.js"] },
  };
  const html = text(await createApp(spec({ assets }))(req("GET", "/product/5")));
  for (const href of [...assets.preload, ...assets.pages.product]) {
    assert(html.includes(`<link rel="modulepreload" href="${href}">`), href);
  }
  assert(!html.includes("/assets/pages/home.js"), "only the rendered page's chunk");
  assert(html.indexOf("modulepreload") < html.indexOf("</head>"));
  // A status page runs no script, so it preloads none.
  const missing = text(await createApp(spec({ assets }))(req("GET", "/nowhere")));
  assert(!missing.includes("modulepreload"), missing);
  // Assets without a graph (an older build) preload nothing.
  assert(!text(await createApp(spec())(req("GET", "/product/5"))).includes("modulepreload"));
});

test("a parameter that doesn't parse, an unknown path and an unknown enum case are 404s", async () => {
  const app = createApp(spec());
  for (const path of ["/product/x", "/product/1?tab=nope", "/nowhere", "/product/1/extra"]) {
    assertEq((await app(req("GET", path))).status, 404, path);
  }
});

test("HEAD answers like GET without a body; other methods on a page are 405", async () => {
  const app = createApp(spec());
  const head = await app(req("HEAD", "/product/1"));
  assertEq(head.status, 200);
  assertEq(head.body, "");
  const post = await app(req("POST", "/product/1"));
  assertEq(post.status, 405);
  assertEq(header(post, "allow"), "GET, HEAD");
});

test("a recorded redirect is 308 to the renamed page with its parameters", async () => {
  const r = await createApp(spec())(req("GET", "/old/9"));
  assertEq(r.status, 308);
  assertEq(header(r, "location"), "/product/9");
});

test("a panic while rendering is a 500 that reveals nothing", async () => {
  const pages = [{ ...spec().pages[0], render: () => { throw new Error("SECRET-DETAIL"); } }];
  const r = await createApp(spec({ pages }))(req("GET", "/product/1"));
  assertEq(r.status, 500);
  assertEq(text(r).includes("SECRET"), false);
});

test("an Authenticated page redirects a guest to the sign-in page with 303", async () => {
  const pages = [{ ...spec().pages[0], guards: [["auth"]] }];
  const r = await createApp(spec({ pages }))(req("GET", "/product/4"));
  assertEq(r.status, 303);
  assertEq(header(r, "location"), "/sign-in?returnTo=%2Fproduct%2F4");
});

test("Metadata keeps the last value of each entry", () => {
  const m = new Metadata();
  m.title("a");
  m.title("b");
  m.status(404);
  assertDeepEq(m.entries, { title: "b", status: 404 });
  assert(new Out().html() === "");
});

// ------------------------------------------------------------------- rpcs

/**
 * Posts `body` to the `Echo` rpc of `app`; answers the status, the parsed JSON and the
 * response.
 */
async function call(app, body, headers = [["content-type", "application/json"]]) {
  const r = await app(req("POST", "/t.P/Echo", { headers, body }));
  return { status: r.status, json: JSON.parse(text(r)), r };
}

test("an rpc answers its value wrapped, never cached", async () => {
  const { status, json, r } = await call(createApp(spec()), '{"text":"hi"}');
  assertEq(status, 200);
  assertDeepEq(json, { value: "hi" });
  assertEq(header(r, "cache-control"), "private, no-store");
});

test("bodies that aren't messages and fields that don't decode are invalid_argument", async () => {
  const app = createApp(spec());
  for (const body of ["not json", "[1]", '"s"', "null", '{"text":3}', "{}"]) {
    const { status, json } = await call(app, body);
    assertEq(status, 400, body);
    assertEq(json.code, "invalid_argument", body);
  }
});

test("a declared error is failed_precondition with a k.TypedError detail", async () => {
  const { status, json } = await call(createApp(spec()), '{"text":"typed"}');
  assertEq(status, 400);
  assertEq(json.code, "failed_precondition");
  assertDeepEq(json.details[0], { type: "k.TypedError", debug: { error: "t.E", case: "outOfStock", fields: { available: 3 } } });
});

test("a panic in a handler is internal with no details", async () => {
  const { status, json } = await call(createApp(spec()), '{"text":"boom"}');
  assertEq(status, 500);
  assertEq(json.code, "internal");
  assertEq(json.details, undefined);
});

test("validation failures are invalid_argument with every failure listed", async () => {
  const rpcs = { "/t.P/Echo": { ...spec().rpcs["/t.P/Echo"], validate: () => [{ path: "text", message: "too short" }, { path: "x", message: "bad" }] } };
  const { status, json } = await call(createApp(spec({ rpcs })), '{"text":"a"}');
  assertEq(status, 400);
  assertEq(json.details[0].type, "k.ValidationError");
  assertEq(json.details[0].debug.failures.length, 2);
});

test("guards refuse with the Connect codes and Retry-After", async () => {
  const base = spec().rpcs["/t.P/Echo"];
  const auth = createApp(spec({ rpcs: { "/t.P/Echo": { ...base, guards: [["auth"]] } } }));
  assertEq((await call(auth, '{"text":"a"}')).json.code, "unauthenticated");
  const limited = createApp(spec({ rpcs: { "/t.P/Echo": { ...base, guards: [["rate", `test-${nextId}`, 1, 60000]] } } }));
  assertEq((await call(limited, '{"text":"a"}')).status, 200);
  const refused = await call(limited, '{"text":"a"}');
  assertEq(refused.status, 429);
  assertEq(refused.json.code, "resource_exhausted");
  assert(Number(header(refused.r, "retry-after")) >= 1);
});

test("an enveloped request to a unary rpc is 415", async () => {
  const r = await createApp(spec())(req("POST", "/t.P/Echo", { headers: [["content-type", "application/connect+json"]], body: "{}" }));
  assertEq(r.status, 415);
});

// --------------------------------------------------------------- endpoints

test("an endpoint gets the raw body; other methods are 405 with Allow", async () => {
  const app = createApp(spec());
  const ok = await app(req("POST", "/hook", { body: "raw ✓" }));
  assertEq(ok.status, 200);
  assertEq(text(ok), "got raw ✓");
  assertEq(header(ok, "content-type"), "text/plain; charset=utf-8");
  const get = await app(req("GET", "/hook"));
  assertEq(get.status, 405);
  assertEq(header(get, "allow"), "POST");
});

test("an endpoint may not set set-cookie in any letter case", () => {
  for (const name of ["set-cookie", "Set-Cookie", "SET-COOKIE"]) {
    let threw = false;
    try {
      responseHeaders(response({ $: 0 }), new Map([[name, "a=b"]]));
    } catch (e) {
      threw = e instanceof KPanic;
    }
    assert(threw, name);
  }
  assertDeepEq(redirect("/x"), { status: 303, headers: [["location", "/x"]], body: null });
});

// ---------------------------------------------------------------- sessions

test("signIn issues a fresh id with the documented cookie; signOut clears and revokes it", async () => {
  let cookie = null;
  const rpcs = {
    "/t.P/Echo": {
      ...spec().rpcs["/t.P/Echo"],
      call: async (c, $c, [t]) => {
        const m = sessionManager(c.$r);
        if (t === "in") await m.signIn($c, { name: "ada" });
        if (t === "out") await m.signOut($c);
        return c.$r.user()?.id.name ?? "guest";
      },
    },
  };
  const app = createApp(spec({ rpcs }));
  const signed = await call(app, '{"text":"in"}');
  assertEq(signed.json.value, "ada");
  const set = header(signed.r, "set-cookie");
  assert(/^k_session=[A-Za-z0-9_-]{43}; Path=\/; HttpOnly; SameSite=Lax/.test(set), set);
  assertDeepEq(JSON.parse(header(signed.r, "x-k-session")), { user: { id: { name: "ada" }, roles: [] } });
  cookie = set.split(";")[0];
  const again = await call(app, '{"text":"who"}', [["content-type", "application/json"], ["cookie", cookie]]);
  assertEq(again.json.value, "ada");
  assertEq(header(again.r, "x-k-session"), undefined);
  const out = await call(app, '{"text":"out"}', [["content-type", "application/json"], ["cookie", cookie]]);
  assert(header(out.r, "set-cookie").includes("Max-Age=0"));
  const after = await call(app, '{"text":"who"}', [["content-type", "application/json"], ["cookie", cookie]]);
  assertEq(after.json.value, "guest");
});

// A session the server no longer has (signed out elsewhere, ended with
// `endAll`, expired) changes the browser's session too: the rpc response
// carries the guest mirror, so the router can leave a guarded page
// (26-auth-and-sessions.md), and the dead cookie is cleared.
test("an rpc with a session the server no longer has tells the browser it ended", async () => {
  const rpcs = {
    "/t.P/Echo": {
      ...spec().rpcs["/t.P/Echo"],
      call: async (c, $c, [t]) => {
        const m = sessionManager(c.$r);
        if (t === "in") await m.signIn($c, { name: "ada" });
        if (t === "out") await m.signOut($c);
        return c.$r.user()?.id.name ?? "guest";
      },
    },
  };
  const app = createApp(spec({ rpcs }));
  const signed = await call(app, '{"text":"in"}');
  const cookie = header(signed.r, "set-cookie").split(";")[0];
  // Another tab signs out with the same cookie; this one learns on its
  // next call.
  await call(app, '{"text":"out"}', [["content-type", "application/json"], ["cookie", cookie]]);
  const stale = await call(app, '{"text":"who"}', [["content-type", "application/json"], ["cookie", cookie]]);
  assertEq(stale.json.value, "guest");
  assertDeepEq(JSON.parse(header(stale.r, "x-k-session")), { user: null });
  assert(header(stale.r, "set-cookie").includes("Max-Age=0"), "the dead cookie is cleared");
  // A browser with no session cookie has nothing to learn.
  const guest = await call(app, '{"text":"who"}');
  assertEq(header(guest.r, "x-k-session"), undefined);
  assertEq(header(guest.r, "set-cookie"), undefined);
});

// A refused guard is the moment the browser's mirror is most likely stale
// (a role taken away, a cookie an earlier response cleared), so every
// `Authenticated` or `Role` refusal carries the mirror, and the router
// leaves a page the server no longer allows (26-auth-and-sessions.md).
test("an rpc refused by a session guard carries the session mirror", async () => {
  const rpcs = {
    "/t.P/Echo": {
      ...spec().rpcs["/t.P/Echo"],
      call: async (c, $c) => {
        await sessionManager(c.$r).signIn($c, { name: "ada" });
        return "in";
      },
    },
    "/t.P/Admin": { ...spec().rpcs["/t.P/Echo"], guards: [["auth"], ["role", "admin"]] },
  };
  const app = createApp(spec({ rpcs }));
  const admin = (headers) => app(req("POST", "/t.P/Admin", { headers: [["content-type", "application/json"], ...headers], body: '{"text":"x"}' }));
  const guest = await admin([]);
  assertEq(guest.status, 401);
  assertDeepEq(JSON.parse(header(guest, "x-k-session")), { user: null });
  const signed = await call(app, '{"text":"in"}');
  const cookie = header(signed.r, "set-cookie").split(";")[0];
  const forbidden = await admin([["cookie", cookie]]);
  assertEq(forbidden.status, 403);
  assertDeepEq(JSON.parse(header(forbidden, "x-k-session")), { user: { id: { name: "ada" }, roles: [] } });
});

// Every server-rendered document embeds the `Session` mirror in `k-data`,
// so one rendered for a signed-in user is private even when nothing on it
// read `Session`: a shared cache must never hand that user's id and roles
// to someone else.
test("a document rendered for a signed-in user is never shared", async () => {
  const rpcs = {
    "/t.P/Echo": {
      ...spec().rpcs["/t.P/Echo"],
      call: async (c, $c) => {
        await sessionManager(c.$r).signIn($c, { name: "ada" });
        return "in";
      },
    },
  };
  const app = createApp(spec({ rpcs }));
  const guest = await app(req("GET", "/product/1"));
  assertEq(guest.status, 200);
  assertEq(header(guest, "cache-control"), undefined, "a guest's document may be cached");
  const signed = await call(app, '{"text":"in"}');
  const cookie = header(signed.r, "set-cookie").split(";")[0];
  const r = await app(req("GET", "/product/1", { headers: [["cookie", cookie]] }));
  assertEq(r.status, 200);
  assert(text(r).includes('"name":"ada"'), "the document carries the mirror");
  assertEq(header(r, "cache-control"), "private, no-store");
});

test("a forged session cookie is a guest", async () => {
  const pages = [{ ...spec().pages[0], guards: [["auth"]] }];
  const r = await createApp(spec({ pages }))(req("GET", "/product/1", { headers: [["cookie", "k_session=forged"]] }));
  assertEq(r.status, 303);
});

// ------------------------------------------------------------ server state

test("shareState keeps .server state in the process store", () => {
  const key = `svc${nextId++}`;
  const a = shareState({ hits: 0 }, key, ["hits"]);
  const b = shareState({ hits: 0 }, key, ["hits"]);
  a.hits = a.hits + 1;
  b.hits = b.hits + 1;
  assertEq(a.hits, 2);
  assertEq(b.hits, 2);
  assert(new DecodeFailure("p", "m").path === "p");
});

test("the server clock reads the system time and refuses timers", () => {
  const c = clock();
  const before = Date.now();
  const now = c.now.ms;
  assert(Math.abs(now - before) < 5000, `now ${now} is the system time`);
  let threw = null;
  try {
    c.sleep(null, 10);
  } catch (e) {
    threw = e;
  }
  assert(threw instanceof KPanic && threw.message.includes("Clock.sleep"), String(threw));
});

// `Theme` during server rendering (07-themes-and-units.md): the variant the
// `k_theme` cookie remembers, or `base`; `apply` does nothing.
test("the server's Theme reads the k_theme cookie", () => {
  const st = (cookie) => ({ cookies: new Map(cookie === null ? [] : [["k_theme", cookie]]) });
  assertEq(serverTheme(st(null)).current, "base");
  assertEq(serverTheme(st("dark")).current, "dark");
  const t = serverTheme(st("dark"));
  t.apply("base");
  assertEq(t.current, "dark");
});

// The remembered variant is on `<html>` before the first paint
// (07-themes-and-units.md, "Switching variants at runtime"): the server
// writes the classes from the `k_theme` cookie; an unknown or malformed
// value is no choice. A client-rendered document (a static build's
// `index.html`, rendered once without a cookie) also carries the pre-paint
// snippet, which does the same from `document.cookie`.
test("the document puts the remembered theme's classes on <html>", async () => {
  const themed = spec({ themes: ["dark", "highContrast"] });
  const page = async (s, cookie) => {
    const headers = cookie === null ? [] : [["cookie", `k_theme=${cookie}`]];
    return text(await createApp(s)(req("GET", "/product/5", { headers })));
  };
  assert((await page(themed, null)).startsWith('<!doctype html><html lang="en"><head>'), "no choice, no class");
  assert((await page(themed, "dark")).startsWith('<!doctype html><html lang="en" class="k-theme-set k-theme-dark">'));
  assert((await page(themed, "highContrast")).includes('class="k-theme-set k-theme-high-contrast"'));
  assert((await page(themed, "base")).includes('<html lang="en" class="k-theme-set">'));
  for (const bad of ["sepia", "x%22%3E", "%E0%A4%A"]) {
    assert((await page(themed, bad)).startsWith('<!doctype html><html lang="en"><head>'), bad);
  }
  // Without variants there is nothing to remember.
  assert((await page(spec(), "dark")).startsWith('<!doctype html><html lang="en"><head>'));
  assert(!(await page(themed, null)).includes("<script>"), "a server-rendered page needs no snippet");
  const client = await page({ ...themed, rendering: "client" }, null);
  const snippet = client.match(new RegExp('initial-scale=1"><script>(.*?)</script>'));
  assert(snippet !== null, client);
  assertEq(snippet[1], themeSnippet);
});

// `TimeZone.local` during server rendering is the zone the browser reported
// in `k_tz` (35-time.md, "Server rendering").
test("requestTimeZone reads the k_tz cookie of the rendered request", () => {
  const ctx = (tz) => ({ c: { $r: { cookies: new Map(tz === undefined ? [] : [["k_tz", tz]]) } } });
  assertEq(requestTimeZone(ctx("Europe%2FBerlin")), "Europe/Berlin");
  assertEq(requestTimeZone(ctx("america/new_york")), "America/New_York", "canonical spelling");
  assertEq(requestTimeZone(ctx(undefined)), "UTC", "a first visit");
  assertEq(requestTimeZone(ctx("Mars%2FOlympus")), "UTC", "an unknown zone");
  assertEq(requestTimeZone(ctx("%E0%A4%A")), "UTC", "a malformed cookie");
  assertEq(requestTimeZone(null), "UTC", "outside a view");
});

// A `.server` broadcast crosses worker isolates as JSON, so its values are
// encoded with the element type's codec and decoded on the way out: a
// subscriber gets the published value back with its types (a `Date` is an
// `Instant`), which the stream's encoder then reads (29-server-streaming.md).
test("a ServerBroadcast delivers values decoded with their codec", async () => {
  const codec = {
    encode: (v) => ({ at: codecs.date.encode(v.at), n: v.n }),
    decode: (j, p) => ({ at: codecs.date.decode(j.at, p + ".at"), n: j.n }),
  };
  const b = new ServerBroadcast("test.Changes@1", codec);
  const s = b.subscribe("k");
  const at = new Instant(1767268800000);
  b.publish("k", { at, n: 2 });
  const got = await s.next();
  assert(got.at instanceof Instant, "the date is an Instant again");
  assertEq(got.at.ms, at.ms);
  assertEq(got.n, 2);
  s.close();
});

// `structuredData(…)` in `metadata` (22-metadata.md) is a schema.org
// JSON-LD script in the document's head, escaped like `k-data`.
test("structured data is a JSON-LD script in the head", async () => {
  const pages = [
    {
      ...spec().pages[0],
      render: (props, ctx, out) => {
        ctx.meta.structuredData({ $: 0, name: "Mug </script>", price: new Money(2400n, unitCase(0)) });
        out.raw("<p>mug</p>");
      },
    },
    {
      ...spec().pages[0],
      match: (u) => route(u, ["article"], []),
      render: (props, ctx, out) => {
        ctx.meta.structuredData({ $: 1, headline: "News" });
        out.raw("<p>news</p>");
      },
    },
  ];
  const app = createApp(spec({ pages }));
  const r = await app(req("GET", "/product/1"));
  const head = text(r).split("</head>")[0];
  assert(head.includes('<script type="application/ld+json">'), head);
  assert(head.includes('"@type":"Product"'), head);
  assert(head.includes('"name":"Mug \\u003c/script\\u003e"'), "escaped like k-data");
  assert(head.includes('"offers":{"@type":"Offer","price":"24.00","priceCurrency":"'), head);
  const a = await app(req("GET", "/article"));
  assert(text(a).includes('{"@context":"https://schema.org","@type":"Article","headline":"News"}'), text(a));
});

// ------------------------------------------------------------- manifest

// The app's `manifest(name:shortName:)` is linked from every document and
// served at `/manifest.webmanifest` (22-metadata.md; todos-G, weather-F),
// which `k build` also writes next to a static app's `index.html`.
test("the app's manifest is linked from the head and served", async () => {
  const app = createApp(spec({ appMeta: (m) => { m.locale("en"); m.manifest("Weather", "Weather"); m.themeColor("#0EA5E9"); } }));
  const page = await app(req("GET", "/product/5"));
  assert(text(page).includes('<link rel="manifest" href="/manifest.webmanifest">'), text(page));
  const r = await app(req("GET", "/manifest.webmanifest"));
  assertEq(r.status, 200);
  assertEq(header(r, "content-type"), "application/manifest+json");
  assertDeepEq(JSON.parse(text(r)), { name: "Weather", short_name: "Weather", start_url: "/", display: "standalone", theme_color: "#0EA5E9" });
  const none = createApp(spec());
  assert(!text(await none(req("GET", "/product/5"))).includes('rel="manifest"'), "no link without a manifest");
  assertEq((await none(req("GET", "/manifest.webmanifest"))).status, 404);
});
