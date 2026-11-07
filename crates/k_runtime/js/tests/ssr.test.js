// SSR escaping and the output buffer (docs/compiler/runtime.md, "SSR").
// Every dynamic value in server HTML goes through these functions.
import { assertEq, test } from "k:testing";
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

test("the hydration markers are the documented comments", () => {
  assertEq(OPEN, "<!--[-->");
  assertEq(CLOSE, "<!--]-->");
  assertEq(SEPARATOR, "<!---->");
});
