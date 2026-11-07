// `k.http.HttpResponse` on both sides (32-http-client.md, 27-endpoint-pages.md):
// one value that is built (`HttpResponse(status:)`, `.headers(_:)`,
// `.body(_:)`) by endpoints and by test fakes of `HttpClient`, and read
// (`status`, `isSuccess`, `headers`, `body`, `bodyBytes`, `json()`) by the
// code that receives it. A built response reads exactly like one an
// `HttpClient` received, in the browser and on the server.
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { KPanic, codecs, native, unitCase } from "k:runtime";
import {
  HttpHeaders$all as headerAll,
  HttpHeaders$get as header,
  HttpResponse$body as httpResponseBody,
  HttpResponse$bodyBytes as responseBytes,
  HttpResponse$headerView as responseHeaderView,
  HttpResponse$headers as httpResponseHeaders,
  HttpResponse$init as httpResponse,
  HttpResponse$isSuccess as responseSuccess,
  HttpResponse$json as responseJson,
  HttpResponse$text as responseText,
} from "k:/runtime/http.js";
import { natives, received } from "k:runtime/server";

const HttpError = {
  offline: unitCase(0),
  timeout: unitCase(1),
  status: (_0, body) => ({ $: 2, _0, body }),
  decoding: (message) => ({ $: 3, message }),
};

// `HttpStatus` cases in the stdlib's order: ok is 0, notFound is 10.
const ok = unitCase(0);
const notFound = unitCase(10);

test("a built response reads its status, success, text and bytes", () => {
  const r = httpResponseBody(httpResponse(ok), { $: 1, _0: "héllo" });
  assertEq(r.status, ok);
  assertEq(responseSuccess(r), true);
  assertEq(responseText(r), "héllo");
  assertDeepEq([...responseBytes(r)], [...new TextEncoder().encode("héllo")]);
  assertEq(responseSuccess(httpResponse(notFound)), false);
  assertEq(responseText(httpResponse(notFound)), "", "no body is an empty text");
});

test("a built JSON body reads as JSON and decodes with the codec", () => {
  const r = httpResponseBody(httpResponse(ok), { $: 0, _0: { codec: codecs.list(codecs.int), value: [1, 2] } });
  assertEq(responseText(r), "[1,2]");
  assertDeepEq(responseJson(r, codecs.list(codecs.int), HttpError), [1, 2]);
  assertEq(header(responseHeaderView(r), "Content-Type"), "application/json");
});

test("a body that doesn't decode is HttpError.decoding", () => {
  const notJson = httpResponseBody(httpResponse(ok), { $: 1, _0: "<html>" });
  let e = null;
  try {
    responseJson(notJson, codecs.int, HttpError);
  } catch (x) {
    e = x;
  }
  assertEq(e?.$, 3);
  assert(String(e.message).includes("not JSON"), String(e?.message));
  const wrong = httpResponseBody(httpResponse(ok), { $: 1, _0: '"text"' });
  e = null;
  try {
    responseJson(wrong, codecs.int, HttpError);
  } catch (x) {
    e = x;
  }
  assertEq(e?.$, 3);
});

test("built headers read like received ones, case-insensitively", () => {
  const r = httpResponseHeaders(httpResponse(ok), new Map([["X-Trace", "a"], ["Link", "<a>"]]));
  const h = responseHeaderView(httpResponseHeaders(r, new Map([["link", "<b>"]])));
  assertEq(header(h, "x-trace"), "a");
  assertEq(header(h, "LINK"), "<a>, <b>");
  assertDeepEq(headerAll(h, "link"), ["<a>", "<b>"]);
  assertEq(header(h, "missing"), null);
  assertDeepEq(headerAll(h, "missing"), []);
});

test("a received response reads through the same helpers", () => {
  const r = received(HttpError, ok, 200, [["Content-Type", "application/json"]], new TextEncoder().encode("[3]"));
  assertEq(responseSuccess(r), true);
  assertEq(responseText(r), "[3]");
  assertDeepEq(responseJson(r, codecs.list(codecs.int), HttpError), [3]);
  assertEq(header(responseHeaderView(r), "content-type"), "application/json");
});

test("the server has every HttpResponse and HttpHeaders native", () => {
  for (const name of ["init", "body", "headers", "text", "bodyBytes", "isSuccess", "headerView", "json"]) {
    assertEq(typeof natives[`k.http.HttpResponse$${name}`], "function", name);
  }
  assertEq(natives["k.http.HttpHeaders$get"], header);
  assertEq(typeof natives["k.http.HttpClient$fetchJson"], "function");
});

test("set-cookie can't be set on a built response", () => {
  let threw = false;
  try {
    httpResponseHeaders(httpResponse(ok), new Map([["Set-Cookie", "a=b"]]));
  } catch (e) {
    threw = e instanceof KPanic;
  }
  assert(threw);
});

test("the browser implements EmailAddress and HttpResponse natives", () => {
  assertEq(native("k.text.EmailAddress$init")("ada@example.com"), "ada@example.com");
  assertEq(native("k.text.EmailAddress$parse")("not an address"), null);
  let threw = false;
  try {
    native("k.text.EmailAddress$init")("nope");
  } catch (e) {
    threw = e instanceof KPanic;
  }
  assert(threw, "an invalid address panics");
});
