/**
 * @module k:runtime/http
 *
 * `k.http.HttpResponse` and `HttpHeaders` on both sides (32-http-client.md,
 * 27-endpoint-pages.md): one value that is built (`HttpResponse(status:)`,
 * `.headers(_:)`, `.body(_:)`) by endpoints and by test fakes of
 * `HttpClient`, and read (`status`, `isSuccess`, `headers`, `body`,
 * `bodyBytes`, `json()`) by the code that receives it. A built response
 * reads exactly like one an `HttpClient` received (`received` in
 * `outbound.js`), so a fake can stand in for the network.
 *
 * Each export is the body of a `@Native` stdlib member, named
 * `Owner$member` like the natives it implements: generated code imports
 * them like any native. In the browser this file is a chunk of its own,
 * shipped only to apps that use one of them (codegen.md §16), and measured
 * against its own size budget; the server has them in `$s.natives`.
 */

import { KPanic, isPanic } from "./values.js";
import { DecodeFailure, httpCode, stringify, toJSON } from "./std.js";

/**
 * `HttpResponse(status:)`: a built response (27-endpoint-pages.md), with
 * its headers as `[name, value]` pairs and its body as an `HttpBody` value
 * or `null`. The same value is read by the code that receives it from a
 * test fake of `HttpClient`, through {@link HttpResponse$text} and the other
 * readers, exactly as a response an `HttpClient` received.
 * @param {object} status an `HttpStatus` value
 * @returns {{status: object, headers: Array<[string, string]>, body: object|null}}
 */
export function HttpResponse$init(status) {
  return { status, headers: [], body: null };
}

/**
 * `response.headers(map)`: adds headers. Setting `set-cookie` (in any
 * letter case) is a panic: sessions own cookies (27-endpoint-pages.md).
 * @param {object} r a built response
 * @param {Map<string, string>} map
 */
export function HttpResponse$headers(r, map) {
  const headers = r.headers.slice();
  for (const [n, v] of map) {
    const name = String(n).toLowerCase();
    if (name === "set-cookie") throw new KPanic("an endpoint may not set set-cookie; use SessionManager");
    headers.push([name, String(v)]);
  }
  return { ...r, headers };
}

/**
 * `response.body(body)`: sets the body (an `HttpBody` value).
 * @param {object} r a built response
 * @param {object} body
 */
export function HttpResponse$body(r, body) {
  return { ...r, body };
}

/** `HttpBody`'s content types by case: json, text, form, bytes. */
const HTTP_BODY_TYPES = ["application/json", "text/plain; charset=utf-8", "application/x-www-form-urlencoded", "application/octet-stream"];

/** `true` for a built response (headers as pairs), `false` for a received one. */
function isBuilt(r) {
  return Array.isArray(r?.headers);
}

/**
 * The bytes and content type of a built response's `HttpBody` (json,
 * text, form, bytes, by case), as the server sends them.
 * @param {object|null} b
 * @returns {[Uint8Array, string|null]}
 */
function httpBodyBytes(b) {
  if (b === null || b === undefined) return [new Uint8Array(0), null];
  const type = b.contentType ?? HTTP_BODY_TYPES[b.$] ?? null;
  switch (b.$) {
    case 0:
      return [new TextEncoder().encode(stringify(b._0 && b._0.codec ? b._0.codec.encode(b._0.value) : toJSON(b._0))), type];
    case 1:
      return [new TextEncoder().encode(String(b._0)), type];
    case 2: {
      const pairs = [];
      for (const [k, v] of b._0) pairs.push(`${encodeURIComponent(k)}=${encodeURIComponent(v)}`);
      return [new TextEncoder().encode(pairs.join("&")), type];
    }
    default:
      return [b._0 instanceof Uint8Array ? b._0 : Uint8Array.from(b._0 ?? []), type];
  }
}

/** `response.bodyBytes`: the body's bytes. */
export function HttpResponse$bodyBytes(r) {
  return isBuilt(r) ? httpBodyBytes(r.body)[0] : r.bodyBytes;
}

/** `response.body`: the body as text (UTF-8, lossy). */
export function HttpResponse$text(r) {
  return isBuilt(r) ? new TextDecoder().decode(httpBodyBytes(r.body)[0]) : r.body;
}

/** `response.isSuccess`: a 2xx status. */
export function HttpResponse$isSuccess(r) {
  if (!isBuilt(r)) return r.isSuccess;
  const code = httpCode(r.status);
  return code >= 200 && code < 300;
}

/**
 * `response.headers`: the `HttpHeaders` of a response, `{map, all}` by
 * lower-case name, for {@link HttpHeaders$get} and {@link HttpHeaders$all}. A built
 * response's body adds its `Content-Type` unless one is set.
 * @param {object} r
 * @returns {{map: Map<string, string>, all: Map<string, string[]>}}
 */
export function HttpResponse$headerView(r) {
  if (!isBuilt(r)) return r.headers;
  const pairs = r.headers.slice();
  const type = httpBodyBytes(r.body)[1];
  if (type !== null && !pairs.some(([n]) => n === "content-type")) pairs.unshift(["content-type", type]);
  return headerView(pairs);
}

/**
 * `HttpHeaders` from `[name, value]` pairs: a repeated header reads as its
 * values joined with `, `, except `set-cookie`, which reads as the first
 * (`all` has every value).
 * @param {Array<[string, string]>} pairs
 * @returns {{map: Map<string, string>, all: Map<string, string[]>}}
 */
export function headerView(pairs) {
  const map = new Map();
  const all = new Map();
  for (const [n, v] of pairs) {
    const name = String(n).toLowerCase();
    if (!all.has(name)) all.set(name, []);
    all.get(name).push(String(v));
    if (!map.has(name)) map.set(name, String(v));
    else if (name !== "set-cookie") map.set(name, `${map.get(name)}, ${v}`);
  }
  return { map, all };
}

/** `headers[name]`: the values joined with `, `, or `nil`. */
export function HttpHeaders$get(h, name) {
  return h.map.get(String(name).toLowerCase()) ?? null;
}

/** `headers.all(name)`: every value, in order. */
export function HttpHeaders$all(h, name) {
  return h.all.get(String(name).toLowerCase())?.slice() ?? [];
}

/**
 * `response.json()`: the body decoded by `codec`; a body that isn't JSON,
 * or doesn't match the type, throws `E.decoding(message)`.
 * @param {object} r
 * @param {{decode: Function}} codec
 * @param {object} E the generated `HttpError` enum
 */
export function HttpResponse$json(r, codec, E) {
  if (!isBuilt(r) && typeof r.json === "function") return r.json(codec);
  let j;
  try {
    j = JSON.parse(HttpResponse$text(r));
  } catch (e) {
    throw E.decoding(`the body is not JSON: ${String(e?.message ?? e)}`);
  }
  try {
    return codec.decode(j, "");
  } catch (e) {
    if (e instanceof DecodeFailure) throw E.decoding(e.path ? `${e.path}: ${e.message}` : e.message);
    if (isPanic(e)) throw e;
    throw E.decoding(String(e?.message ?? e));
  }
}

/**
 * `HttpClient.fetchJson(request:)`, the interface's default method, for a
 * user type that implements `HttpClient` (a test fake): its own `send`,
 * then a 2xx status (else `E.status(status, body)`), then the body decoded
 * by `codec` (else `E.decoding(message)`).
 * @param {{send: Function}} client
 * @param {object} E the generated `HttpError` enum
 * @param {AbortSignal|null} $c
 * @param {{decode: Function}} codec
 * @param {object} request an `HttpRequest`
 */
export async function HttpClient$fetchJson(client, E, $c, codec, request) {
  const r = await client.send($c, request);
  if (!HttpResponse$isSuccess(r)) throw E.status(r.status, HttpResponse$text(r));
  return HttpResponse$json(r, codec, E);
}
