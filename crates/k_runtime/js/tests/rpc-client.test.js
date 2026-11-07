// The RPC client's typed errors (docs/compiler/runtime.md §8;
// docs/design/28-rpc-wire-format.md): a declared error case arrives as
// `failed_precondition` with a `k.TypedError` detail whose `debug` names
// the case and its payload fields, as the server sends it (server.md §6).
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { rpcError, unitCase } from "k:runtime";

const CheckoutError = {
  productNotFound: unitCase(0),
  outOfStock: (available) => ({ $: 1, available }),
  rpcFailure: (_0) => ({ $: 2, _0 }),
};

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

// `RpcFailure`'s cases are tagged in declaration order, as generated
// `match`es test them: rejected 0, invalid 1, offline 2, timeout 3.
test("network outcomes are rpcFailure(.offline) and rpcFailure(.timeout)", () => {
  assertDeepEq(rpcError({ ok: false, kind: "offline" }, CheckoutError), { $: 2, _0: { $: 2, _0: null } });
  assertDeepEq(rpcError({ ok: false, kind: "timeout" }, CheckoutError), { $: 2, _0: { $: 3, _0: null } });
});

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
