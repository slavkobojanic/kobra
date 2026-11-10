/**
 * @module k:/tests/outbound.test.js
 *
 * The server's I/O platform services (docs/compiler/server.md §18):
 * `FetchHttpClient` over a stand-in for `op_k_fetch`, the `TestHttpClient`
 * double, `SmtpEmailProvider` over a stand-in for `op_k_smtp_send`, and the
 * `RecordingEmailProvider` double, and `VapidWebPush` over stand-ins for the push ops and
 * `op_k_fetch` with the `RecordingWebPush` double (docs/design/50-push.md). The ops themselves are tested in Rust
 * (`crates/k_server/tests/outbound.rs`).
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import {
  Database,
  createApp,
  fetchHttpClient,
  inlineJobQueue,
  jobQueue,
  natives,
  recordingEmailProvider,
  recordingJobQueue,
  recordingWebPush,
  runJobBody,
  response,
  responseBody,
  smtpEmailProvider,
  smtpSettings,
  testHttpClient,
  vapidWebPush,
  wireEmail,
  wireRequest,
} from "k:runtime/server";
import { KPanic, Secret, codecs, unitCase } from "k:runtime";

const HttpError = {
  offline: unitCase(0),
  timeout: unitCase(1),
  status: (_0, body) => ({ $: 2, _0, body }),
  decoding: (message) => ({ $: 3, message }),
};
const HttpStatus = {
  ok: unitCase(0),
  created: unitCase(1),
  notFound: unitCase(2),
  serviceUnavailable: unitCase(3),
  other: (_0) => ({ $: 4, _0 }),
};
const CODES = [200, 201, 404, 503, null];

/** An `HttpRequest` to the search API, overridden by `extra`. */
const request = (extra) => ({
  method: unitCase(0),
  url: "https://api.example.com/v1/search",
  query: new Map(),
  headers: new Map(),
  secretHeaders: new Map(),
  bearerToken: null,
  body: null,
  timeout: 1000,
  ...extra,
});

/** The UTF-8 bytes of `t`. */
const bytes = (t) => new TextEncoder().encode(t);

/** A transport answering `outcome`, recording `[id, wire]`. */
function transport(outcome) {
  const sent = [];
  return { sent, fn: async (id, wire) => (sent.push([id, wire]), outcome) };
}

/** What promise `p` rejects with, or `undefined` when it resolves. */
async function rejection(p) {
  try {
    await p;
  } catch (e) {
    return e;
  }
  throw new Error("expected a rejection");
}

test("the wire request carries the query, secrets, bearer token and body type", () => {
  const w = wireRequest(
    request({
      method: unitCase(1),
      query: new Map([["q", "Bér lin"], ["n", "8"]]),
      headers: new Map([["Idempotency-Key", "checkout-1"]]),
      secretHeaders: new Map([["x-api-key", new Secret("k_1")]]),
      bearerToken: new Secret("sk_test_123"),
      body: { $: 2, _0: new Map([["mode", "payment"], ["line_items[0][quantity]", "2"]]) },
      timeout: 10000,
    }),
  );
  assertEq(w.method, "POST");
  assertEq(w.url, "https://api.example.com/v1/search?q=B%C3%A9r%20lin&n=8");
  const h = new Map(w.headers);
  assertEq(h.get("idempotency-key"), "checkout-1");
  assertEq(h.get("x-api-key"), "k_1");
  assertEq(h.get("authorization"), "Bearer sk_test_123");
  assertEq(h.get("content-type"), "application/x-www-form-urlencoded");
  assertEq(new TextDecoder().decode(w.body), "mode=payment&line_items%5B0%5D%5Bquantity%5D=2");
  assertEq(w.timeoutMs, 10000);
  const j = wireRequest(request({ body: { $: 0, _0: { codec: codecs.list(codecs.int), value: [1, 2] } } }));
  assertEq(new TextDecoder().decode(j.body), "[1,2]");
  assertEq(new Map(j.headers).get("content-type"), "application/json");
  assertEq(wireRequest(request({})).body, null);
});

test("send resolves to the response whatever its status", async () => {
  const t = transport({ kind: "response", status: 404, headers: [["content-type", "text/plain"], ["set-cookie", "a=1"], ["set-cookie", "b=2"], ["x-n", "1"], ["x-n", "2"]], body: bytes("missing") });
  const client = fetchHttpClient(HttpError, HttpStatus, CODES, t.fn);
  const r = await client.send(null, request({}));
  assertEq(r.status, HttpStatus.notFound);
  assertEq(r.isSuccess, false);
  assertEq(r.body, "missing");
  assertEq(r.headers.map.get("x-n"), "1, 2");
  assertEq(r.headers.map.get("set-cookie"), "a=1");
  assertDeepEq(r.headers.all.get("set-cookie"), ["a=1", "b=2"]);
  const other = await fetchHttpClient(HttpError, HttpStatus, CODES, transport({ kind: "response", status: 418, headers: [], body: bytes("") }).fn).send(null, request({}));
  assertDeepEq(other.status, { $: 4, _0: 418 });
});

test("offline and timeout reject with their HttpError cases; invalid is a panic", async () => {
  const off = fetchHttpClient(HttpError, HttpStatus, CODES, transport({ kind: "offline", message: "could not connect" }).fn);
  assertEq(await rejection(off.send(null, request({}))), HttpError.offline);
  const slow = fetchHttpClient(HttpError, HttpStatus, CODES, transport({ kind: "timeout" }).fn);
  assertEq(await rejection(slow.send(null, request({}))), HttpError.timeout);
  const bad = fetchHttpClient(HttpError, HttpStatus, CODES, transport({ kind: "invalid", message: "only http and https URLs can be fetched" }).fn);
  const e = await rejection(bad.send(null, request({})));
  assert(e instanceof KPanic, String(e));
});

test("fetchJson requires a 2xx status and decodes the body", async () => {
  const ok = fetchHttpClient(HttpError, HttpStatus, CODES, transport({ kind: "response", status: 200, headers: [], body: bytes('["a"]') }).fn);
  assertDeepEq(await ok.fetchJson(null, codecs.list(codecs.string), request({})), ["a"]);
  const down = fetchHttpClient(HttpError, HttpStatus, CODES, transport({ kind: "response", status: 503, headers: [], body: bytes("down") }).fn);
  assertDeepEq(await rejection(down.fetchJson(null, codecs.list(codecs.string), request({}))), { $: 2, _0: HttpStatus.serviceUnavailable, body: "down" });
  const wrong = fetchHttpClient(HttpError, HttpStatus, CODES, transport({ kind: "response", status: 200, headers: [], body: bytes('{"a": 1}') }).fn);
  assertEq((await rejection(wrong.fetchJson(null, codecs.list(codecs.string), request({})))).$, 3);
  const notJson = fetchHttpClient(HttpError, HttpStatus, CODES, transport({ kind: "response", status: 200, headers: [], body: bytes("<html>") }).fn);
  const e = await rejection(notJson.fetchJson(null, codecs.list(codecs.string), request({})));
  assertEq(e.$, 3);
  assert(e.message.startsWith("the body is not JSON"), e.message);
});

test("an aborted request is cancelled and never resumes", async () => {
  const aborted = [];
  let release;
  const pending = new Promise((r) => (release = r));
  const client = fetchHttpClient(HttpError, HttpStatus, CODES, () => pending, (id) => (aborted.push(id), release({ kind: "aborted" })));
  const controller = new AbortController();
  let settled = false;
  client.send(controller.signal, request({})).then(() => (settled = true), () => (settled = true));
  controller.abort();
  await pending;
  await Promise.resolve();
  assertEq(aborted.length, 1);
  assertEq(settled, false);
});

test("TestHttpClient answers canned paths, records requests and fails on others", async () => {
  const client = testHttpClient(HttpError, HttpStatus, CODES);
  client.respond("/v1/checkout/sessions", responseBody(response(HttpStatus.created), { $: 1, _0: "made" }));
  client.serveJson(codecs.list(codecs.int), "/v1/numbers", [1, 2]);
  client.serveText("/v1/down", "Service Unavailable", HttpStatus.serviceUnavailable);
  client.fail("/v1/offline", HttpError.offline);
  const made = await client.send(null, request({ url: "https://api.stripe.com/v1/checkout/sessions?x=1" }));
  assertEq(made.status, HttpStatus.created);
  assertEq(made.body, "made");
  assertEq(made.headers.map.get("content-type"), "text/plain; charset=utf-8");
  assertDeepEq(await client.fetchJson(null, codecs.list(codecs.int), request({ url: "https://h/v1/numbers" })), [1, 2]);
  assertDeepEq(await rejection(client.fetchJson(null, codecs.string, request({ url: "https://h/v1/down" }))), { $: 2, _0: HttpStatus.serviceUnavailable, body: "Service Unavailable" });
  assertEq(await rejection(client.send(null, request({ url: "https://h/v1/offline" }))), HttpError.offline);
  const e = await rejection(client.send(null, request({ url: "https://h/v1/unknown" })));
  assert(e instanceof KPanic && e.message.includes("GET https://h/v1/unknown"), String(e?.message));
  assertEq(client.requests.length, 5);
  assertEq(client.requests[0].url, "https://api.stripe.com/v1/checkout/sessions?x=1");
});

// ------------------------------------------------------------------ email

const EmailError = {
  unavailable: unitCase(0),
  rejected: (reason) => ({ $: 1, reason }),
  invalidMessage: (reason) => ({ $: 2, reason }),
};

const CONFIG = {
  SMTP_HOST: "mail.example",
  SMTP_PORT: 1025,
  SMTP_SECURITY: "none",
  SMTP_USERNAME: "orders",
  SMTP_PASSWORD: { $secret: "hunter2" },
  MAIL_FROM: "K Shop <orders@shop.example>",
};

/** An `Email` to Ana, overridden by `extra`. */
const email = (extra) => ({ to: "ana@example.com", subject: "Your order", text: "Thanks!", html: null, from: null, replyTo: null, cc: [], bcc: [], ...extra });

test("SMTP settings come from SmtpConfig's variables", () => {
  const { settings, from } = smtpSettings(CONFIG);
  assertDeepEq(settings, { host: "mail.example", port: 1025, security: "none", username: "orders", password: "hunter2" });
  assertEq(from, "K Shop <orders@shop.example>");
  assertDeepEq(smtpSettings({ SMTP_HOST: "h" }).settings, { host: "h", port: 587, security: "startTls", username: null, password: null });
});

test("the wire email defaults the sender and refuses a subject with a line break", () => {
  assertEq(wireEmail(email({}), "orders@shop.example").message.from, "orders@shop.example");
  assertEq(wireEmail(email({ from: "me@x.y" }), "orders@shop.example").message.from, "me@x.y");
  assertEq(wireEmail(email({ subject: "a\r\nBcc: x@y.z" }), "o@x.y").invalid, "the subject contains a line break");
  assert(wireEmail(email({}), null).invalid !== undefined);
});

test("SmtpEmailProvider maps the op's outcomes to EmailError", async () => {
  const sent = [];
  const outcomes = [{ kind: "sent" }, { kind: "rejected", reason: "550 No such user" }, { kind: "unavailable", message: "refused" }, { kind: "invalidMessage", reason: "bad address" }];
  const mailer = smtpEmailProvider(EmailError, CONFIG, async (s, m) => (sent.push([s, m]), outcomes.shift()));
  await mailer.send(null, email({ cc: ["c@x.y"] }));
  assertDeepEq(sent[0][1].cc, ["c@x.y"]);
  assertEq(sent[0][0].password, "hunter2");
  assertDeepEq(await rejection(mailer.send(null, email({}))), EmailError.rejected("550 No such user"));
  assertEq(await rejection(mailer.send(null, email({}))), EmailError.unavailable);
  assertDeepEq(await rejection(mailer.send(null, email({}))), EmailError.invalidMessage("bad address"));
  assertDeepEq(await rejection(mailer.send(null, email({ subject: "x\ny" }))), EmailError.invalidMessage("the subject contains a line break"));
  assertEq(sent.length, 4, "an invalid subject never reaches SMTP");
});

test("RecordingEmailProvider keeps messages and simulates failures", async () => {
  const mailer = recordingEmailProvider(EmailError);
  await mailer.send(null, email({}));
  assertEq(mailer.sent.length, 1);
  assertEq(mailer.sent[0].to, "ana@example.com");
  mailer.simulateFailure(EmailError.unavailable);
  assertEq(await rejection(mailer.send(null, email({}))), EmailError.unavailable);
  assertEq(mailer.sent.length, 1);
});

// ------------------------------------------------------------------- push

const WebPushError = {
  expired: unitCase(0),
  unavailable: unitCase(1),
  rejected: (status) => ({ $: 2, status }),
  tooLarge: unitCase(3),
  invalidSubscription: unitCase(4),
  notConfigured: unitCase(5),
};
const POINT = "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
const subscription = (endpoint = "https://push.example/s/1") => ({ endpoint, p256dh: POINT, auth: "BTBZMqHH6r4Tts7J_aSIgg" });
const pushHref = (p) => `/rooms/${p.name}`;

/**
 * A `VapidWebPush` over stand-ins: `answers` are the statuses (or `{kind}` outcomes) the push
 * service gives, `posted` records each request, `signed` each token signed; `clock` is seconds.
 */
function pusher({ key = true, contact = "mailto:ops@example.org" } = {}) {
  const t = { answers: [], posted: [], signed: [], clock: 1_700_000_000 };
  const config = () => ({ privateKey: key ? new Secret({ secret: new Uint8Array(32).fill(1), pub: new Uint8Array(65).fill(4) }) : null, contact });
  t.push = vapidWebPush(WebPushError, config, pushHref, {
    encrypt: (p256dh, auth, plaintext) => (p256dh.length === 65 ? Uint8Array.from([...plaintext, 2]) : null),
    vapid: (_k, aud, exp, sub) => (t.signed.push([aud, exp, sub]), `vapid t=${t.signed.length}, k=x`),
    fetch: async (_id, w) => {
      t.posted.push(w);
      const a = t.answers.shift() ?? 201;
      return typeof a === "number" ? { kind: "response", status: a, headers: [], body: new Uint8Array(0) } : a;
    },
    now: () => t.clock * 1000,
  });
  return t;
}

test("VapidWebPush posts the message with TTL, aes128gcm and a VAPID token for the origin", async () => {
  const t = pusher();
  await t.push.send(null, { title: "Sam", body: "Lunch?", tag: "general", opens: { $: 0, name: "general" } }, subscription(), 60000);
  assertEq(t.posted.length, 1);
  const w = t.posted[0];
  assertEq(w.url, "https://push.example/s/1");
  assertDeepEq(w.headers, [
    ["authorization", "vapid t=1, k=x"],
    ["content-encoding", "aes128gcm"],
    ["content-type", "application/octet-stream"],
    ["ttl", "60"],
  ]);
  const text = new TextDecoder().decode(w.body.subarray(0, -1));
  assertDeepEq(JSON.parse(text), { title: "Sam", body: "Lunch?", tag: "general", url: "/rooms/general" });
  assertDeepEq(t.signed, [["https://push.example", 1_700_000_000 + 12 * 3600, "mailto:ops@example.org"]]);
  assertEq(t.push.serverKey.length, 87);
});

test("VapidWebPush reuses a token for 11 hours, per push service", async () => {
  const t = pusher();
  const n = { title: "Hi", body: null, tag: null, opens: null };
  await t.push.send(null, n, subscription());
  assertEq(t.posted[0].headers[3][1], "86400", "a day by default");
  t.clock += 10 * 3600;
  await t.push.send(null, n, subscription());
  await t.push.send(null, n, subscription("https://other.example/s/1"));
  assertEq(t.signed.length, 2, "one token per origin");
  t.clock += 2 * 3600;
  await t.push.send(null, n, subscription());
  assertEq(t.signed.length, 3, "a new token after 11 hours");
});

test("VapidWebPush maps the push service's answers to WebPushError", async () => {
  const n = { title: "Hi", body: null, tag: null, opens: null };
  for (const [answer, error] of [
    [410, WebPushError.expired],
    [404, WebPushError.expired],
    [413, WebPushError.tooLarge],
    [429, WebPushError.unavailable],
    [502, WebPushError.unavailable],
    [403, WebPushError.rejected(403)],
    [{ kind: "offline", message: "refused" }, WebPushError.unavailable],
    [{ kind: "timeout" }, WebPushError.unavailable],
  ]) {
    const t = pusher();
    t.answers.push(answer);
    assertDeepEq(await rejection(t.push.send(null, n, subscription())), error, JSON.stringify(answer));
  }
  const ok = pusher();
  ok.answers.push(200, 202);
  await ok.push.send(null, n, subscription());
  await ok.push.send(null, n, subscription());
});

test("VapidWebPush refuses what it can't send before sending anything", async () => {
  const n = { title: "Hi", body: null, tag: null, opens: null };
  const t = pusher();
  assertEq(await rejection(t.push.send(null, n, subscription("http://push.example/s/1"))), WebPushError.invalidSubscription);
  assertEq(await rejection(t.push.send(null, n, { ...subscription(), p256dh: "AAAA" })), WebPushError.invalidSubscription);
  assertEq(await rejection(t.push.send(null, n, { ...subscription(), auth: "not base64url!" })), WebPushError.invalidSubscription);
  assertEq(await rejection(t.push.send(null, { ...n, body: "x".repeat(4000) }, subscription())), WebPushError.tooLarge);
  assertEq(t.posted.length, 0);
  // `http` only to loopback, for a local push service.
  await t.push.send(null, n, subscription("http://127.0.0.1:5300/s/1"));
  assertEq(t.signed[0][0], "http://127.0.0.1:5300");
  const none = pusher({ key: false });
  assertEq(none.push.serverKey, null);
  assertEq(await rejection(none.push.send(null, n, subscription())), WebPushError.notConfigured);
  const anonymous = pusher({ contact: null });
  await anonymous.push.send(null, n, subscription());
  assertEq(anonymous.signed[0][2], "");
});

test("RecordingWebPush records, expires, fails and unconfigures as the doc says", async () => {
  const push = recordingWebPush(WebPushError);
  const n = { title: "Hi", body: null, tag: null, opens: null };
  assertEq(push.serverKey.length, 87);
  await push.send(null, n, subscription());
  assertDeepEq(push.sent, [{ message: n, subscription: subscription(), timeToLive: 86400000 }]);
  push.expire(subscription());
  assertEq(await rejection(push.send(null, n, subscription())), WebPushError.expired);
  push.simulateFailure(WebPushError.unavailable);
  assertEq(await rejection(push.send(null, n, subscription("https://push.example/s/2"))), WebPushError.unavailable);
  push.stopFailing();
  await push.send(null, n, subscription("https://push.example/s/2"), 1000);
  push.setConfigured(false);
  assertEq(push.serverKey, null);
  assertEq(await rejection(push.send(null, n, subscription("https://push.example/s/2"))), WebPushError.notConfigured);
  assertEq(push.sent.length, 2);
});

// ------------------------------------------------------------------- jobs

const JobError = { retryable: (reason) => ({ $: 0, reason }), permanent: (reason) => ({ $: 1, reason }) };
const kind = { name: "order-confirmation" };

test("DatabaseJobQueue stores the encoded payload in the open transaction", async () => {
  const stored = [];
  const db = { tx: 0 };
  const queue = jobQueue(db, async (...a) => (stored.push(a), "id1"));
  await queue.enqueue(null, codecs.list(codecs.int), kind, [1, 2], 500);
  db.tx = 7;
  await queue.enqueue(null, codecs.list(codecs.int), kind, [3], undefined);
  assertDeepEq(stored, [
    ["order-confirmation", "[1,2]", 500, 0],
    ["order-confirmation", "[3]", 0, 7],
  ]);
  const broken = jobQueue(db, async () => {
    throw new Error("database: disk full");
  });
  const e = await rejection(broken.enqueue(null, codecs.int, kind, 1, 0));
  assert(e instanceof KPanic && e.message.includes("order-confirmation"), String(e?.message));
});

test("RecordingJobQueue keeps payloads by kind, in order", async () => {
  const q = recordingJobQueue();
  await q.enqueue(null, codecs.int, kind, 1, 0);
  await q.enqueue(null, codecs.int, { name: "other" }, 2, 0);
  await q.enqueue(null, codecs.int, kind, 3, 0);
  assertDeepEq(q.payloads(codecs.int, kind), [1, 3]);
  assertDeepEq(q.payloads(codecs.int, { name: "none" }), []);
});

test("a job's outcome: done, its JobError case, or a panic", async () => {
  assertDeepEq(await runJobBody({ run: async () => {} }, {}, null, {}), { outcome: "done" });
  assertDeepEq(await runJobBody({ run: async () => { throw JobError.retryable("smtp down"); } }, {}, null, {}), { outcome: "retryable", reason: "smtp down" });
  assertDeepEq(await runJobBody({ run: async () => { throw JobError.permanent("gone"); } }, {}, null, {}), { outcome: "permanent", reason: "gone" });
  assertDeepEq(await runJobBody({ run: async () => { throw new KPanic("a bug"); } }, {}, null, {}), { outcome: "panic", reason: "a bug" });
  assertDeepEq(await runJobBody({ run: async () => { throw new TypeError("x is undefined"); } }, {}, null, {}), { outcome: "panic", reason: "x is undefined" });
});

test("InlineJobQueue runs the job when the transaction commits, never on rollback", async () => {
  const ran = [];
  const spec = { jobs: { "order-confirmation": { run: async (_c, _$c, j) => ran.push(j) } } };
  const ops = {
    op_k_db_begin: async () => 1,
    op_k_db_finish: async () => {},
    op_k_db_savepoint: async () => {},
  };
  const db = new Database(ops);
  const q = inlineJobQueue(spec, {}, db);
  await q.enqueue(null, codecs.int, kind, 1);
  assertDeepEq(ran, [1], "outside a transaction it runs at once");
  await db.transaction(null, async () => {
    await q.enqueue(null, codecs.int, kind, 2);
    assertDeepEq(ran, [1], "not before the commit");
  });
  assertDeepEq(ran, [1, 2]);
  await rejection(
    db.transaction(null, async () => {
      await q.enqueue(null, codecs.int, kind, 3);
      throw JobError.permanent("rolled back");
    }),
  );
  await db.transaction(null, async () => {
    await rejection(
      db.transaction(null, async () => {
        await q.enqueue(null, codecs.int, kind, 4);
        throw JobError.permanent("savepoint rolled back");
      }),
    );
    await q.enqueue(null, codecs.int, kind, 5);
  });
  assertDeepEq(ran, [1, 2, 5]);
  const failing = inlineJobQueue({ jobs: { "order-confirmation": { run: async () => { throw JobError.retryable("down"); } } } }, {}, null);
  const e = await rejection(failing.enqueue(null, codecs.int, kind, 1));
  assert(e instanceof KPanic && e.message.includes("down"), String(e?.message));
});

test("createApp runs a job request in a fresh container and answers its outcome", async () => {
  const seen = [];
  const app = createApp({
    name: "t",
    rendering: "server",
    pages: [],
    redirects: [],
    rpcs: {},
    endpoints: [],
    container: (st) => ({ st, Mailer: { send: (p) => seen.push([p, st.job, st.sessionId]) } }),
    jobs: {
      mail: { run: async (c, _$c, j) => c.Mailer.send(j.to) },
      fails: { run: async () => { throw JobError.permanent("no such order"); } },
    },
    httpStatus: [],
    httpBody: [],
    assets: { styles: [], scripts: [] },
  });
  const job = (name, body) => ({ id: 1, method: "POST", path: "/", headers: [["cookie", "k_session=abc"]], body: new TextEncoder().encode(body), remote: "", job: { name, attempt: 1 } });
  const ok = await app(job("mail", '{"to": "ana@example.com"}'));
  assertEq(ok.status, 200);
  assertDeepEq(JSON.parse(ok.body), { outcome: "done" });
  assertDeepEq(seen, [["ana@example.com", true, null]], "no session, even with a cookie header");
  assertDeepEq(JSON.parse((await app(job("fails", "{}"))).body), { outcome: "permanent", reason: "no such order" });
  assertEq(JSON.parse((await app(job("unknown", "{}"))).body).outcome, "permanent");
  assertEq(JSON.parse((await app(job("mail", "not json"))).body).outcome, "permanent");
});

// ------------------------------------------------------------------ crypto

test("Hmac.sha256 takes a Secret or Bytes key and matches RFC 4231", async () => {
  const hmac = natives["k.crypto.Hmac"];
  const message = new TextEncoder().encode("what do ya want for nothing?");
  const viaSecret = await hmac.sha256(null, new Secret("Jefe"), message);
  assertEq(viaSecret.hex, "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
  const viaBytes = await hmac.sha256(null, new TextEncoder().encode("Jefe"), message);
  assertEq(viaBytes.hex, viaSecret.hex);
  assertEq((await hmac.sha512(null, new Secret("Jefe"), message)).bytes.length, 64);
  const matches = natives["k.crypto.Mac$matches"];
  assert(matches(viaSecret, viaBytes.bytes));
  assert(!matches(viaSecret, viaBytes.bytes.slice(1)));
  const tampered = viaBytes.bytes.slice();
  tampered[31] ^= 1;
  assert(!matches(viaSecret, tampered));
});

test("Bytes.parse(hex:) and (base64:) refuse what isn't their encoding", () => {
  const hex = natives["k.core.Bytes$parse$hex"];
  assertDeepEq([...hex("00ff7A")], [0, 255, 122]);
  assertEq(hex("abc"), null);
  assertEq(hex("zz"), null);
  assertEq(hex("+1"), null);
  assertDeepEq([...hex("")], []);
  const b64 = natives["k.core.Bytes$parse$base64"];
  assertDeepEq([...b64("aGk=")], [104, 105]);
  assertDeepEq([...b64("-_8")], [251, 255]);
  assertEq(b64("a"), null);
  assertEq(b64("a$b="), null);
});
