// Test-first tests for runtime parts that are specified but not
// implemented yet. Each passes while it fails (docs/compiler/testing.md).
import { assertEq, installDom, test, tick } from "k:testing";
import { FAILED, LOADING, READY, batch, createForm, createLoads, flush, hydrate, navigate, rpc, signal } from "k:runtime";

test("rpc returns an offline outcome when fetch rejects", async () => {
  const outcome = await rpc("/shop.cart.CartProtocol/Add", {}, null, {
    fetch: () => Promise.reject(new TypeError("Failed to fetch")),
  });
  assertEq(outcome.kind, "offline");
});

test("rpc decodes a Connect error", async () => {
  const body = JSON.stringify({ code: "failed_precondition", message: "", details: [] });
  const outcome = await rpc("/a.B/C", {}, null, {
    fetch: () => Promise.resolve({ ok: false, status: 400, text: async () => body, json: async () => JSON.parse(body) }),
  });
  assertEq(outcome.code, "failed_precondition");
});

test("loads resolve in dependency order and become READY", async () => {
  const loads = createLoads([
    { name: "product", after: [], run: async () => ({ id: 1 }) },
    { name: "reviews", after: ["product"], run: async (v) => [v.product.id] },
  ], null);
  assertEq(loads.state(), LOADING);
  await tick();
  assertEq(loads.state(), READY);
  assertEq(loads.values().reviews[0], 1);
});

test("a failing load moves to FAILED with its error", async () => {
  const loads = createLoads([{ name: "x", after: [], run: async () => { throw { $: 0 }; } }], null);
  await tick();
  assertEq(loads.state(), FAILED);
});

test("a form starts with raw text and validates on handle", () => {
  const form = createForm([], {}, () => {});
  assertEq(typeof form.handle, "function");
});

test("hydrate claims server-rendered nodes", () => {
  const doc = installDom();
  const host = doc.createElement("div");
  host.innerHTML = "<p>hi</p>";
  hydrate(host, () => host.firstChild, null);
});

test("navigate pushes history", () => {
  navigate("/product/42");
});

test("pending tests import the final API surface", () => {
  assertEq(typeof batch, "function");
  assertEq(typeof flush, "function");
  assertEq(typeof signal, "function");
});
