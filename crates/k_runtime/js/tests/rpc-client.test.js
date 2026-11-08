/**
 * @module k:/tests/rpc-client.test.js
 *
 * The RPC client's typed errors (docs/compiler/runtime.md §8;
 * docs/design/28-rpc-wire-format.md): a declared error case arrives as
 * `failed_precondition` with a `k.TypedError` detail whose `debug` names
 * the case and its payload fields, as the server sends it (server.md §6).
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { payloadFields, rpc, rpcError, unitCase } from "k:runtime";

const CheckoutError = {
  productNotFound: unitCase(0),
  outOfStock: (available) => ({ $: 1, available }),
  rpcFailure: (_0) => ({ $: 2, _0 }),
};

/** A Connect failure carrying a `k.TypedError` detail whose debug value is `debug`. */
const typed = (debug) => ({
  ok: false,
  kind: "connect",
  status: 400,
  code: "failed_precondition",
  message: "",
  details: [{ type: "k.TypedError", debug }],
});

test("a payload-free declared case is the enum's singleton", () => {
  const e = rpcError(typed({ error: "shop.CheckoutError", case: "productNotFound", fields: {} }), CheckoutError);
  assertEq(e, CheckoutError.productNotFound);
});

test("a declared case with a payload is built from its fields", () => {
  const e = rpcError(typed({ error: "shop.CheckoutError", case: "outOfStock", fields: { available: 3 } }), CheckoutError);
  assertDeepEq(e, { $: 1, available: 3 });
});

// A Codable error's payload crosses the wire in its codec's form; the
// client decodes it with the error's codec, so an enum payload is a case
// value again and a date an `Instant`, not the raw JSON.
const Reason = { tooLate: unitCase(0), tooEarly: unitCase(1) };
const ShippingError = {
  noSlot: (reason, until) => ({ $: 0, reason, until }),
  closed: unitCase(1),
  rpcFailure: (_0) => ({ $: 2, _0 }),
};
const ShippingError$codec = {
  encode: () => {
    throw new Error("the client only decodes");
  },
  decode: (j, p) => {
    const c = typeof j === "string" ? j : j.case;
    if (c === "closed") return ShippingError.closed;
    if (c !== "noSlot") throw new Error(`unknown case ${c} at ${p}`);
    return ShippingError.noSlot(Reason[j.reason], new Date(j.until));
  },
};

test("a Codable error's payload is decoded with the error's codec", () => {
  const o = typed({ error: "shop.ShippingError", case: "noSlot", fields: { reason: "tooEarly", until: "2026-01-02T00:00:00Z" } });
  const e = rpcError(o, ShippingError, ShippingError$codec);
  assertEq(e.$, 0);
  assertEq(e.reason, Reason.tooEarly, "the enum payload is its case value");
  assert(e.until instanceof Date, "the date payload is decoded");
  assertEq(e.until.toISOString(), "2026-01-02T00:00:00.000Z");
});

test("a payload-free case of a Codable error decodes to its singleton", () => {
  const e = rpcError(typed({ error: "shop.ShippingError", case: "closed", fields: {} }), ShippingError, ShippingError$codec);
  assertEq(e, ShippingError.closed);
});

test("fields the error's codec refuses are a panic, not a wrong value", () => {
  const e = rpcError(typed({ error: "shop.ShippingError", case: "gone", fields: {} }), ShippingError, ShippingError$codec);
  assert(e instanceof Error, String(e));
});

// `RpcFailure`'s cases are tagged in declaration order, as generated
// `match`es test them: rejected 0, invalid 1, offline 2, timeout 3.
test("network outcomes are rpcFailure(.offline) and rpcFailure(.timeout)", () => {
  assertDeepEq(rpcError({ ok: false, kind: "offline" }, CheckoutError), { $: 2, _0: { $: 2, _0: null } });
  assertDeepEq(rpcError({ ok: false, kind: "timeout" }, CheckoutError), { $: 2, _0: { $: 3, _0: null } });
});

/** A Connect failure with `code`, `details` and `retryAfter`. */
const connect = (code, details = [], retryAfter = 0) => ({ ok: false, kind: "connect", code, details, retryAfter });

test("invalid_argument is rpcFailure(.invalid) with the ValidationError's failures", () => {
  const failures = [{ path: "request.email", message: "Enter a valid email address" }];
  const e = rpcError(connect("invalid_argument", [{ type: "k.ValidationError", debug: { failures } }]), CheckoutError);
  assertDeepEq(e, { $: 2, _0: { $: 1, _0: { failures } } });
  assertDeepEq(rpcError(connect("invalid_argument"), CheckoutError), { $: 2, _0: { $: 1, _0: { failures: [] } } });
});

test("a refused guard is rpcFailure(.rejected) with its GuardRejection case", () => {
  assertEq(rpcError(connect("unauthenticated"), CheckoutError)._0._0.$, 0);
  assertEq(rpcError(connect("permission_denied"), CheckoutError)._0._0.$, 1);
  const limited = rpcError(connect("resource_exhausted", [], 30), CheckoutError);
  assertEq(limited._0.$, 0);
  assertEq(limited._0._0.$, 2);
  assertEq(limited._0._0.retryAfter, 30000, "Retry-After seconds as a Duration");
});

test("an unknown code is a panic, not a typed error", () => {
  const e = rpcError(connect("internal"), CheckoutError);
  assert(e instanceof Error && e.message === "rpc failed: internal", String(e));
});

// The server's side of the same contract: a Codable error's fields are
// its codec's encoding without the `case` key (server.md §6).
test("payloadFields of a Codable error is its codec's form without the case", () => {
  const encode = (v) => (v.$ === 1 ? "closed" : { case: "noSlot", reason: ["tooLate", "tooEarly"][v.reason.$], until: v.until.toISOString() });
  const e = ShippingError.noSlot(Reason.tooLate, new Date("2026-01-02T00:00:00Z"));
  assertDeepEq(payloadFields(e, encode), { reason: "tooLate", until: "2026-01-02T00:00:00.000Z" });
  assertDeepEq(payloadFields(ShippingError.closed, encode), {});
});

test("payloadFields without a codec is the payload's structural JSON", () => {
  assertDeepEq(payloadFields(CheckoutError.outOfStock(3)), { available: 3 });
});

// ------------------------------------------------------- the caller's signal

/** A caller's signal that counts the abort listeners it holds. */
function countingSignal() {
  const listeners = new Set();
  return {
    aborted: false,
    listeners,
    addEventListener: (type, f) => listeners.add(f),
    removeEventListener: (type, f) => listeners.delete(f),
  };
}

// A component's lifetime outlives each rpc it makes: once a unary call
// settles, the call no longer listens to it, so a page that makes many
// calls doesn't collect one listener per call until it unmounts.
test("a settled unary rpc stops listening to the caller's signal", async () => {
  const signal = countingSignal();
  const ok = { ok: true, status: 200, headers: { get: () => null }, text: async () => "{}" };
  for (let i = 0; i < 3; i++) {
    await rpc("/a.B/C", {}, signal, { fetch: () => Promise.resolve(ok) });
  }
  await rpc("/a.B/C", {}, signal, { fetch: () => Promise.reject(new TypeError("Failed to fetch")) });
  assertEq(signal.listeners.size, 0);
});
