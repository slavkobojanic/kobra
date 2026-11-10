/**
 * @module k:/tests/ssr.test.js
 *
 * SSR escaping and the output buffer (docs/compiler/runtime.md, "SSR").
 * Every dynamic value in server HTML goes through these functions.
 */
import { assertEq, test } from "k:testing";
import { codecs } from "k:runtime";
import { CLOSE, OPEN, Out, SEPARATOR, escapeAttribute, escapeText, serializeData } from "k:runtime/server";

test("escapeText escapes &, < and >", () => {
  assertEq(escapeText("a < b && c > d"), "a &lt; b &amp;&amp; c &gt; d");
  assertEq(escapeText("<script>alert(1)</script>"), "&lt;script&gt;alert(1)&lt;/script&gt;");
  assertEq(escapeText(""), "");
  assertEq(escapeText("plain é ✓"), "plain é ✓");
});

test("escapeAttribute also escapes both quotes", () => {
  assertEq(escapeAttribute(`" onclick="x' &`), "&quot; onclick=&quot;x&#39; &amp;");
});

test("serializeData can't close the script element or break a parser", () => {
  const json = serializeData({ html: "</script><script>alert(1)</script>", amp: "&", ls: "  " });
  assertEq(json.includes("</script"), false);
  assertEq(json.includes("<"), false);
  assertEq(json.includes("&"), false);
  assertEq(json.includes(" "), false);
  assertEq(JSON.parse(json).html, "</script><script>alert(1)</script>");
  assertEq(JSON.parse(json).ls, "  ");
});

test("Out escapes text and attributes and writes boolean attributes", () => {
  const out = new Out();
  out.raw("<button");
  out.attribute("title", 'say "hi"');
  out.attribute("disabled", true);
  out.attribute("hidden", false);
  out.attribute("aria-label", null);
  out.raw(">");
  out.text("1 < 2");
  out.raw("</button>");
  assertEq(out.html(), '<button title="say &quot;hi&quot;" disabled>1 &lt; 2</button>');
});

test("label ids are numbered in document order when the HTML is joined", () => {
  // The page renders first, its layout after, with the page's chunks in
  // the layout's slot: the layout's id still comes first in the document.
  const page = new Out();
  page.raw("<label");
  const a = page.uid();
  page.attribute("for", a);
  page.raw("></label><input");
  page.attribute("id", a);
  page.raw(">");
  const layout = new Out();
  layout.raw("<label");
  const b = layout.uid();
  layout.attribute("for", b);
  layout.raw("></label><input");
  layout.attribute("id", b);
  layout.raw("><main>");
  layout.chunks.push(page.chunks);
  layout.raw("</main>");
  assertEq(layout.html(), '<label for="k1"></label><input id="k1"><main><label for="k2"></label><input id="k2"></main>');
  assertEq(page.html(), '<label for="k1"></label><input id="k1">');
});

test("rewinding an Out takes back its HTML, load keys and ids", () => {
  const out = new Out();
  out.raw("<p>");
  const ctx = { key: "0" };
  assertEq(out.loadKey(ctx), "0.1");
  const back = out.mark();
  out.raw("<label");
  out.attribute("for", out.uid());
  out.raw("></label>");
  assertEq(out.loadKey(ctx), "0.2");
  back();
  assertEq(out.loadKey(ctx), "0.2");
  out.raw("<label");
  out.attribute("for", out.uid());
  out.raw("></label></p>");
  assertEq(out.html(), '<p><label for="k1"></label></p>');
});

test("the hydration markers are the documented comments", () => {
  assertEq(OPEN, "<!--[-->");
  assertEq(CLOSE, "<!--]-->");
  assertEq(SEPARATOR, "<!---->");
});

// `@Load` values are encoded with their codecs, and a `Double`'s encoding
// is the codec's number wrapper: `k-data` prints it as a number, as rpc
// bodies do, so hydration can decode it (a latitude, a temperature).
test("serializeData prints encoded doubles as numbers", () => {
  const json = serializeData({ latitude: codecs.double.encode(52.52), whole: codecs.double.encode(4), list: [codecs.double.encode(0.5)] });
  assertEq(json, '{"latitude":52.52,"whole":4.0,"list":[0.5]}');
  assertEq(codecs.double.decode(JSON.parse(json).latitude, "latitude"), 52.52);
});
