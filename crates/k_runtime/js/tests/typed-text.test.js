// Typed text values, their codecs, shared route matching and the
// hydration helpers for texts and markers at the top of a view
// (docs/compiler/runtime.md §6, §10, §12; docs/design/28-rpc-wire-format.md).
// The parsers are the only way runtime text becomes a `Url`, `EmailAddress`,
// `Uuid` or `Color`, so each refuses everything that isn't one.
import { assert, assertEq, assertThrows, test } from "k:testing";
import {
  DecodeFailure,
  HashMap,
  NO,
  bytesBase64,
  codecs,
  colorHex,
  hydrate,
  markers,
  parseColor,
  parseEmail,
  parseQuery,
  parseUrl,
  parseUuid,
  route,
  routeParam,
  separator,
  splitTarget,
  stringify,
  textNode,
  uid,
  urlHost,
  urlPath,
} from "k:runtime";

/** `true` when `codec.decode(j)` throws a `DecodeFailure`. */
function refuses(codec, j) {
  try {
    codec.decode(j, "$");
    return false;
  } catch (e) {
    return e instanceof DecodeFailure;
  }
}

test("Url.parse accepts absolute http(s) and ws(s) URLs with a host", () => {
  for (const ok of ["https://example.com", "http://a.b/c?d=1#e", "HTTPS://Example.COM:8443/x", "wss://h/s", "https://user@h/", "http://[::1]:80/", 'https://e.com/?a=1&b="x"']) {
    assertEq(parseUrl(ok), ok, ok);
  }
});

test("Url.parse refuses script schemes, relative text and controls", () => {
  for (const bad of ["javascript:alert(1)", " JaVaScRiPt:alert(1)", "javascript://example.com/%0Aalert(1)", "data:text/html,x", "vbscript://h/", "file:///etc/passwd", "/relative", "example.com", "https://", "https://exa mple.com", "https://e.com/\nx", "https://e.com\\x", "", "mailto:a@b.c"]) {
    assertEq(parseUrl(bad), null, JSON.stringify(bad));
  }
  assertEq(parseUrl(42), null);
});

test("Url host and path read the text without a URL class", () => {
  assertEq(urlHost("https://Example.com:8080/a/b?c#d"), "example.com");
  assertEq(urlPath("https://example.com/a/b?c#d"), "/a/b");
  assertEq(urlPath("https://example.com"), "/");
  assertEq(urlPath("https://example.com?q"), "/");
  assertEq(urlHost("/product/42?tab=x"), "");
  assertEq(urlPath("/product/42?tab=x"), "/product/42");
});

test("EmailAddress.parse trims, lower-cases the domain and refuses markup", () => {
  assertEq(parseEmail("  Ana@Example.COM "), "Ana@example.com");
  for (const bad of ["nobody", "<script>alert(1)</script>", "a@b", "a b@c.de", "a@b.c\nBcc: x@y.z", "<a@b.cd>", "a@@b.cd", "", "a@b.cd,c@d.ef"]) {
    assertEq(parseEmail(bad), null, JSON.stringify(bad));
  }
});

test("Uuid.parse accepts the canonical form in either case only", () => {
  assertEq(parseUuid("123E4567-E89B-12D3-A456-426614174000"), "123e4567-e89b-12d3-a456-426614174000");
  for (const bad of ["' OR 1=1 --", "123e4567e89b12d3a456426614174000", "123e4567-e89b-12d3-a456-42661417400", "{123e4567-e89b-12d3-a456-426614174000}"]) {
    assertEq(parseUuid(bad), null, bad);
  }
});

test("a decoded Color is a color, never CSS that ends a declaration", () => {
  for (const ok of ["#3B82F6", "#fff", "#ffff", "#3B82F6CC", "transparent", "var(--k-color-primary)", "color-mix(in srgb, #fff 50%, transparent)", "rgb(1, 2, 3)"]) {
    assertEq(parseColor(ok), ok, ok);
  }
  for (const bad of ["red; background: url(https://evil.example/x)", "url(x)", "#12", "#12345", "red}", "expression(alert(1))", "a:b", '"x"', "var(--x", "rgb(1,2,3))", ""]) {
    assertEq(parseColor(bad), null, bad);
  }
});

test("Color.hex accepts hex colors and panics for anything else", () => {
  assertEq(colorHex("#3B82F6"), "#3B82F6");
  assertThrows(() => colorHex("red; color: blue"), "not a hex color");
  assertThrows(() => colorHex("#3B82F6 red"), "not a hex color");
});

test("typed text codecs validate on decode", () => {
  assertEq(codecs.email.decode("ana@example.com", "$"), "ana@example.com");
  assertEq(codecs.uuid.decode("123e4567-e89b-12d3-a456-426614174000", "$"), "123e4567-e89b-12d3-a456-426614174000");
  assert(refuses(codecs.url, "javascript:alert(1)"));
  assert(refuses(codecs.email, "x"));
  assert(refuses(codecs.uuid, 1));
  assert(refuses(codecs.color, "red;x:y"));
});

test("a map with text keys is an object, other keys are pairs, each only from itself", () => {
  const byName = codecs.map(codecs.string, codecs.int, true);
  assertEq(stringify(byName.encode(new Map([["__proto__", 1], ["a", 2]]))), '{"__proto__":1,"a":2}');
  assertEq(byName.decode(JSON.parse('{"__proto__": 1}'), "$").get("__proto__"), 1);
  assert(refuses(byName, []));
  assert(refuses(byName, [["a", 1]]));
  const byNumber = codecs.map(codecs.int, codecs.string, true);
  assertEq(stringify(byNumber.encode(new Map())), "[]");
  assertEq(byNumber.decode([[1, "a"]], "$").get(1), "a");
  for (const bad of [{}, [null], [[1]], [["one", "a"]], [[1, 2]], [[1, "a", 3]]]) {
    assert(refuses(byNumber, bad), JSON.stringify(bad));
  }
  const structural = codecs.map(codecs.tuple([codecs.int]), codecs.int, false);
  assert(structural.decode([[[1], 2]], "$") instanceof HashMap);
});

test("a failed pair names its index in the path", () => {
  const m = codecs.map(codecs.int, codecs.string, true);
  try {
    m.decode([[1, "a"], [2, 3]], "scores");
    assert(false, "must not decode");
  } catch (e) {
    assertEq(e.path, "scores[1][1]");
  }
});

test("Bytes.base64 encodes with padding and without btoa", () => {
  const b = (xs) => bytesBase64(new Uint8Array(xs));
  assertEq(b([]), "");
  assertEq(b([0x66]), "Zg==");
  assertEq(b([0x66, 0x6f]), "Zm8=");
  assertEq(b([0x66, 0x6f, 0x6f]), "Zm9v");
  assertEq(b([0xff, 0xfe, 0xfd, 0x00]), "//79AA==");
});

const PRODUCT = [["id", routeParam.int, true, null], ["sort", routeParam.string, false, () => "new"]];

test("route parses typed parameters and refuses ones that don't parse", () => {
  const r = route(splitTarget("/product/41?sort=price+asc"), ["product", "{id}"], PRODUCT);
  assertEq(r.id, 41);
  assertEq(r.sort, "price asc");
  assertEq(route(splitTarget("/product/abc"), ["product", "{id}"], PRODUCT), null);
  assertEq(route(splitTarget("/product/%E0%A4%A"), ["product", "{id}"], PRODUCT), null);
  assertEq(route(splitTarget("/product/9007199254740993"), ["product", "{id}"], PRODUCT), null);
  assertEq(route(splitTarget("/product/41"), ["product", "{id}"], PRODUCT).sort, "new");
});

test("a query parameter never overrides a path parameter", () => {
  const params = [["name", routeParam.string, true, null]];
  assertEq(route(splitTarget("/tag/lamp?name=evil"), ["tag", "{name}"], params).name, "lamp");
  assertEq(route({ pathname: "/tag/lamp", search: "?name=evil" }, ["tag", "{name}"], params).name, "lamp");
});

test("parseQuery decodes plus as a space and skips malformed pairs", () => {
  const q = parseQuery("?a=1+2&b=%E0%A4%A&a=3&c");
  assertEq(q.get("a"), "1 2");
  assertEq(q.has("b"), false);
  assertEq(q.get("c"), "");
});

test("the typed parameter parsers", () => {
  assertEq(routeParam.int("-7"), -7);
  assertEq(routeParam.int("1.5"), NO);
  assertEq(routeParam.double("1.5e2"), 150);
  assertEq(routeParam.double("Infinity"), NO);
  assertEq(routeParam.bool("true"), true);
  assertEq(routeParam.bool("yes"), NO);
  assertEq(routeParam.uuid("nope"), NO);
  const page = routeParam.page(() => [{ parse: (u) => (u.pathname === "/home" ? {} : null), value: () => "home" }]);
  assertEq(page("/home"), "home");
  assertEq(page("//evil.example"), NO);
  assertEq(page("/nowhere"), NO);
});

test("hydration restarts label ids, as each server render does", () => {
  uid();
  uid();
  const doc = globalThis.document;
  const host = doc.createElement("div");
  doc.body.appendChild(host);
  let id = null;
  hydrate(host, () => {
    id = uid();
    return null;
  }, null);
  assertEq(id, "k1");
});

test("texts, separators and markers at the top of a view are claimed while hydrating", () => {
  const doc = globalThis.document;
  const host = doc.createElement("div");
  host.appendChild(doc.createTextNode("a"));
  host.appendChild(doc.createComment(""));
  host.appendChild(doc.createTextNode("b"));
  host.appendChild(doc.createComment("["));
  host.appendChild(doc.createTextNode("c"));
  host.appendChild(doc.createComment("]"));
  doc.body.appendChild(host);
  const before = [...host.childNodes];
  let m = null;
  hydrate(host, () => {
    const f = doc.createDocumentFragment();
    f.appendChild(textNode("a"));
    f.appendChild(separator());
    f.appendChild(textNode(""));
    m = markers();
    f.appendChild(m.node);
    return f;
  }, null);
  const after = [...host.childNodes];
  assertEq(after.length, before.length);
  for (let i = 0; i < after.length; i++) assert(after[i] === before[i], `node ${i} is the server's`);
  assert(m.start === before[3] && m.end === before[5]);
});

test("outside hydration the helpers create fresh nodes", () => {
  const t = textNode("x");
  assertEq(t.nodeType, 3);
  assertEq(t.textContent, "x");
  assertEq(separator().nodeType, 8);
  const m = markers();
  assertEq(m.node.childNodes.length, 2);
});
