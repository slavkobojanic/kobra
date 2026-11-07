/**
 * @module k:/tests/remote.test.js
 *
 * Calls between services (docs/design/40-workspaces-and-services.md): service
 * tokens, the remote client over a stand-in for `op_k_fetch`, the `TestRemote`
 * double, and the `ServiceAuthenticated` guard with `ServiceCaller`.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import {
  DEFAULT_REMOTE_TIMEOUT_MS,
  SERVICE_TOKEN_HEADER,
  TOKEN_LIFETIME_MS,
  checkGuards,
  remote,
  remoteCall,
  remoteOf,
  serviceCaller,
  serviceToken,
  testRemote,
  verifyServiceToken,
} from "k:runtime/server";
import { KPanic, Secret } from "k:runtime";

/** A deterministic stand-in for HMAC-SHA256: not secure, but keyed. */
function fakeHmac(key, message) {
  const out = new Uint8Array(8);
  const text = `${key}|${message}`;
  for (let i = 0; i < text.length; i++) out[i % 8] = (out[i % 8] * 31 + text.charCodeAt(i)) & 255;
  return out;
}

const NOW = 1_700_000_000_000;
/** The UTF-8 bytes of `t`. */
const bytes = (t) => new TextEncoder().encode(t);

/** A transport answering `outcome`, recording what was sent. */
function transport(outcome) {
  const sent = [];
  return { sent, fn: async (id, wire, body) => (sent.push({ id, wire, body: new TextDecoder().decode(body) }), outcome) };
}

/** A `remote` for paybox whose transport answers `outcome`, its options overridden by `extra`. */
function client(outcome, extra) {
  const t = transport(outcome);
  const r = remote("shop", "http://paybox.internal:8081/", new Secret("s3cret"), 2500, {
    transport: t.fn,
    abort: () => {},
    now: () => NOW,
    hmac: fakeHmac,
    ...extra,
  });
  return { r, t };
}

test("a signed token verifies with the same secret, names the caller and expires", () => {
  const token = serviceToken("shop", new Secret("s3cret"), NOW, fakeHmac);
  assert(token.startsWith(`v1.shop.${NOW + TOKEN_LIFETIME_MS}.`));
  assertEq(verifyServiceToken(token, new Secret("s3cret"), NOW, fakeHmac), "shop");
  assertEq(verifyServiceToken(token, "s3cret", NOW + TOKEN_LIFETIME_MS, fakeHmac), "shop");
  assertEq(verifyServiceToken(token, "s3cret", NOW + TOKEN_LIFETIME_MS + 1, fakeHmac), null);
  assertEq(verifyServiceToken(token, "other", NOW, fakeHmac), null);
});

test("tampered, malformed and unsigned tokens are refused without throwing", () => {
  const token = serviceToken("shop", "s3cret", NOW, fakeHmac);
  const [v, caller, expiry, mac] = token.split(".");
  for (const bad of [
    `${v}.admin.${expiry}.${mac}`,
    `${v}.${caller}.${Number(expiry) + 1}.${mac}`,
    `${v}.${caller}.${expiry}.${mac.slice(1)}`,
    `${v}.${caller}.${expiry}`,
    `v2.${caller}.${expiry}.${mac}`,
    `${v}.${caller}.1e99.${mac}`,
    `${v}.%E0%A4%A.${expiry}.${mac}`,
    "",
    null,
    42,
    "x".repeat(5000),
  ]) {
    assertEq(verifyServiceToken(bad, "s3cret", NOW, fakeHmac), null, String(bad).slice(0, 40));
  }
  // An empty secret verifies nothing, not even a token signed with it.
  assertEq(verifyServiceToken(serviceToken("shop", "", NOW, fakeHmac), "", NOW, fakeHmac), null);
  // A token valid for implausibly long (a clock far behind) is refused.
  assertEq(verifyServiceToken(serviceToken("shop", "s3cret", NOW + 3 * TOKEN_LIFETIME_MS, fakeHmac), "s3cret", NOW, fakeHmac), null);
});

test("a caller name with dots survives the token", () => {
  const token = serviceToken("pay.box", "k", NOW, fakeHmac);
  assertEq(verifyServiceToken(token, "k", NOW, fakeHmac), "pay.box");
});

test("a call posts the message with the Connect headers and a token, to the URL without its trailing slash", async () => {
  const { r, t } = client({ kind: "response", status: 200, headers: [], body: bytes('{"id":"ch_1"}') });
  const o = await remoteCall(r, "/pay_contracts.pay.Payments/Charge", { request: { cents: 5 } }, null);
  assertDeepEq(o, { ok: true, message: { id: "ch_1" } });
  const [{ wire, body }] = t.sent;
  assertEq(wire.method, "POST");
  assertEq(wire.url, "http://paybox.internal:8081/pay_contracts.pay.Payments/Charge");
  assertEq(wire.timeoutMs, 2500);
  assertEq(body, '{"request":{"cents":5}}');
  const header = (n) => wire.headers.find(([k]) => k === n)?.[1];
  assertEq(header("content-type"), "application/json");
  assertEq(header("connect-protocol-version"), "1");
  assertEq(verifyServiceToken(header(SERVICE_TOKEN_HEADER), "s3cret", NOW, fakeHmac), "shop");
});

test("errors, failures and timeouts become the outcomes rpcError maps", async () => {
  const typed = { code: "failed_precondition", details: [{ type: "k.TypedError", debug: { case: "declined" } }] };
  let { r } = client({ kind: "response", status: 400, headers: [["Retry-After", "3"]], body: bytes(JSON.stringify(typed)) });
  assertDeepEq(await remoteCall(r, "/p.S/M", {}, null), { ok: false, kind: "connect", code: "failed_precondition", details: typed.details, retryAfter: 3 });
  ({ r } = client({ kind: "response", status: 502, headers: [], body: bytes("<html>bad gateway") }));
  assertDeepEq(await remoteCall(r, "/p.S/M", {}, null), { ok: false, kind: "offline" });
  ({ r } = client({ kind: "timeout" }));
  assertDeepEq(await remoteCall(r, "/p.S/M", {}, null), { ok: false, kind: "timeout" });
  ({ r } = client({ kind: "offline", message: "connection refused" }));
  assertDeepEq(await remoteCall(r, "/p.S/M", {}, null), { ok: false, kind: "offline" });
  ({ r } = client(null, { transport: () => Promise.reject(new Error("boom")) }));
  assertDeepEq(await remoteCall(r, "/p.S/M", {}, null), { ok: false, kind: "offline" });
  ({ r } = client(null, { transport: () => { throw new Error("sync"); } }));
  assertDeepEq(await remoteCall(r, "/p.S/M", {}, null), { ok: false, kind: "offline" });
});

test("an aborted call never settles and cancels the request", async () => {
  const cancelled = [];
  let release;
  const { r } = client(null, {
    transport: () => new Promise((resolve) => (release = resolve)),
    abort: (id) => cancelled.push(id),
  });
  const controller = new AbortController();
  let settled = false;
  remoteCall(r, "/p.S/M", {}, controller.signal).then(() => (settled = true));
  controller.abort();
  release({ kind: "response", status: 200, headers: [], body: bytes("{}") });
  await Promise.resolve();
  await Promise.resolve();
  assertEq(settled, false);
  assertEq(cancelled.length, 1);
  let pre = false;
  remoteCall(r, "/p.S/M", {}, controller.signal).then(() => (pre = true));
  await Promise.resolve();
  assertEq(pre, false);
});

test("streaming between services panics with the rpc's path", () => {
  const { r } = client({ kind: "timeout" });
  let caught = null;
  try {
    remoteCall(r, "/p.S/Watch", {}, null, { body: () => {} });
  } catch (e) {
    caught = e;
  }
  assert(caught instanceof KPanic);
  assert(String(caught.message).includes("/p.S/Watch"));
});

test("the TestRemote double is offline and sends nothing", async () => {
  const r = testRemote("shop");
  assertDeepEq(await remoteCall(r, "/p.S/M", {}, null), { ok: false, kind: "offline" });
});

test("a RemoteService config gives the URL, the token and its timeout, or the default", () => {
  const token = new Secret("t");
  const r = remoteOf("shop", { url: "https://pay.example/", token, timeout: 1500 });
  assertEq(r.url, "https://pay.example");
  assertEq(r.token, token);
  assertEq(r.timeoutMs, 1500);
  assertEq(remoteOf("shop", { url: "u", token }).timeoutMs, DEFAULT_REMOTE_TIMEOUT_MS);
  assertEq(remote("shop", "u", token, -1).timeoutMs, DEFAULT_REMOTE_TIMEOUT_MS);
});

test("ServiceAuthenticated admits a valid token and ServiceCaller names its caller", async () => {
  const now = Date.now();
  const token = serviceToken("shop", "s3cret", now);
  const st = { headers: [[SERVICE_TOKEN_HEADER, token]], serviceCaller: null, test: null };
  assertEq(await checkGuards(st, [["service", () => new Secret("s3cret")]]), null);
  assertEq(serviceCaller(st).name, "shop");
  const refused = { headers: [[SERVICE_TOKEN_HEADER, token]], serviceCaller: null, test: null };
  assertDeepEq(await checkGuards(refused, [["service", () => new Secret("other")]]), { kind: "unauthenticated" });
  assertEq(serviceCaller(refused).name, null);
  const missing = { headers: [], serviceCaller: null, test: null };
  assertDeepEq(await checkGuards(missing, [["service", () => { throw new Error("no config"); }]]), { kind: "unauthenticated" });
});
