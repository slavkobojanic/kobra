// The server half of a `k test` run (docs/design/21-testing.md, "Which side
// a test runs on"; runtime.md §13): a request may carry the test's context
// (`request.test`), which the request state keeps; the server's `Clock` is
// then the test's virtual clock; services bound once per test are shared by
// every container of the test; a session change is reported to the test;
// a streamed rpc writes to the request's own sink instead of the front's
// ops; and `TestClient` sends requests through the app in-process.
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { RequestState, clock, createApp, perTest, publicUrls, route, sessionManager, testClient } from "k:runtime/server";
import { Instant, codecs, unitCase } from "k:runtime";
import { HttpResponse$text as responseText } from "k:/runtime/http.js";

const enc = new TextEncoder();
let nextId = 1;

function req(method, path, extra = {}) {
  return { id: nextId++, method, path, headers: [], remote: "127.0.0.1", ...extra, body: enc.encode(extra.body ?? "") };
}

/** A minimal spec with one unary and one streamed rpc. */
function spec(over = {}) {
  return {
    name: "t",
    rendering: "server",
    pages: [],
    redirects: [],
    rpcs: {
      "/t.P/Count": {
        guards: [],
        decode: () => [],
        validate: null,
        call: async () => {
          let n = 0;
          return { next: async () => (n < 2 ? ++n : null), close() {} };
        },
        encode: (v) => ({ value: v }),
        error: null,
        stream: true,
      },
    },
    endpoints: [],
    container: (st) => (st.test ? st.test.container(st) : { $r: st }),
    appMeta: null,
    signIn: null,
    session: { roles: {}, decodeUser: (j) => j },
    jobs: {},
    httpStatus: [200, 404, null],
    httpBody: ["json", "text", "form", "bytes"],
    assets: { styles: [], scripts: [] },
    ...over,
  };
}

test("a request's test context is kept by its state and picks its container", () => {
  const t = { container: (st) => ({ $r: st, mine: true }) };
  const st = new RequestState(spec(), req("GET", "/", { test: t }));
  assertEq(st.test, t);
  assertEq(st.c.mine, true);
  assertEq(new RequestState(spec(), req("GET", "/")).test, null);
});

test("the server clock of a test is the test's virtual clock", () => {
  const virtual = { now: new Instant(1767268800000) };
  const st = new RequestState(spec(), req("GET", "/", { test: { clock: virtual } }));
  assertEq(clock(st), virtual);
  assert(Math.abs(clock().now.ms - Date.now()) < 5000, "outside a test, the system time");
});

test("perTest shares one instance across containers of a test, and memoizes per container otherwise", () => {
  const t = { memo: new Map() };
  const a = { $r: { test: t } };
  const b = { $r: { test: t } };
  let made = 0;
  const make = () => ({ n: ++made });
  assertEq(perTest(a, "Fake", make), perTest(b, "Fake", make));
  assertEq(made, 1);
  const c = { $r: { test: null } };
  assertEq(perTest(c, "Fake", make), perTest(c, "Fake", make));
  assertEq(made, 2);
});

test("a session change is reported to the test", async () => {
  const changes = [];
  const st = new RequestState(spec(), req("GET", "/", { test: { sessionChanged: (s) => changes.push(s.sessionId) } }));
  await st.load();
  await sessionManager(st).signIn(null, "ada");
  assertEq(changes.length, 1);
  assertEq(changes[0], st.sessionId);
  await sessionManager(st).signOut(null);
  assertEq(changes.length, 2);
  assertEq(changes[1], null);
});

test("a streamed rpc writes its envelopes to the request's sink", async () => {
  const seen = { start: null, chunks: [], ended: false };
  let done;
  const ended = new Promise((r) => (done = r));
  const sink = {
    start: (head) => (seen.start = head),
    chunk: (bytes) => {
      seen.chunks.push(bytes);
      return true;
    },
    end: () => {
      seen.ended = true;
      done();
    },
  };
  const app = createApp(spec());
  const r = await app(req("POST", "/t.P/Count", { headers: [["content-type", "application/connect+json"]], body: "\u0000\u0000\u0000\u0000\u0002{}", sink }));
  assert(r && r.streamed, "the handler answers through the sink");
  await ended;
  assertEq(seen.start.status, 200);
  assertEq(seen.chunks.length, 3, "two elements and the end-of-stream envelope");
  assertEq(seen.chunks[2][0], 2);
  assertEq(new TextDecoder().decode(seen.chunks[0].subarray(5)), '{"value":1}');
});

test("TestClient sends a request through the app with the test's exchange", async () => {
  const HttpError = { offline: unitCase(0), timeout: unitCase(1), status: (_0, body) => ({ $: 2, _0, body }), decoding: (message) => ({ $: 3, message }) };
  const HttpStatus = { ok: unitCase(0), notFound: unitCase(1), other: (_0) => ({ $: 2, _0 }) };
  const sent = [];
  const t = {
    send: async (wire) => {
      sent.push(wire);
      return { status: 404, headers: [["Content-Type", "text/plain"], ["X-Trace", "1"]], body: enc.encode("nope") };
    },
  };
  const client = testClient({ test: t }, HttpError, HttpStatus, [200, 404, null]);
  const r = await client.send(null, { method: unitCase(1), url: "http://localhost/hook?x=1", query: new Map(), headers: new Map([["A", "b"]]), secretHeaders: new Map(), bearerToken: null, body: { $: 1, _0: "hi" }, timeout: 1000 });
  assertEq(sent[0].method, "POST");
  assertEq(sent[0].url, "http://localhost/hook?x=1");
  assertDeepEq(sent[0].headers.find(([n]) => n === "a"), ["a", "b"], "header names go out lower case");
  assertEq(r.status, HttpStatus.notFound);
  assertEq(responseText(r), "nope");
  assertEq(r.headers.map.get("x-trace"), "1");
});

test("PublicUrls builds absolute URLs of pages and endpoints", () => {
  const urls = publicUrls((p) => `/p/${p}`, (e) => `/hooks/${e}`);
  assertEq(urls.absolute("a"), "http://localhost/p/a");
  assertEq(urls.absolute$endpoint("stripe"), "http://localhost/hooks/stripe");
  assert(route !== undefined && codecs !== undefined);
});
