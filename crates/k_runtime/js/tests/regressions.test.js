// Regressions found by adversarial testing of the runtime
// (docs/compiler/testing.md §7: runtime bugs are pinned next to the code).
// An open finding is a `test.pending` with the behaviour the design
// requires: it passes while the bug is there and fails once it is fixed,
// with a message to turn it into `test`; a fixed one is a plain `test`
// that guards the fix. The generated-code findings
// (client routing, keyed rows, rpc decoding, XSS) are in
// crates/k_server/tests/regressions/.
import { assert, assertEq, test } from "k:testing";
import { batch, codecs, DecodeFailure, each, root, signal, when } from "k:runtime";
import { requestText } from "k:runtime/server";

/** A host element with block markers, in the document. */
function range() {
  const doc = globalThis.document;
  const host = doc.createElement("div");
  const start = doc.createComment("[");
  const end = doc.createComment("]");
  host.appendChild(start);
  host.appendChild(end);
  doc.body.appendChild(host);
  return { doc, host, start, end };
}

/**
 * A row as codegen emits one whose body is an `if`: a fragment holding a
 * pair of markers and the `when` block between them (`markers()` + `when`).
 */
function blockRow(item) {
  const doc = globalThis.document;
  const node = doc.createDocumentFragment();
  const start = doc.createComment("[");
  const end = doc.createComment("]");
  node.appendChild(start);
  node.appendChild(end);
  when(start, end, () => item().on, () => {
    const e = doc.createElement("span");
    e.textContent = `[${item().id}]`;
    return e;
  }, null);
  return node;
}

// `each` (list.js) records a row's nodes once, when the row is created
// (`nodes` = the fragment's children at that moment). A block at the root
// of the row inserts its content between its markers later, so that
// content isn't in `nodes`: a move takes only the markers along, and a
// removal leaves the content in the document (runtime.md §5.3: "removed
// rows are disposed", "multi-node rows move as a unit").
test(
  "a keyed row whose block shows content later still moves and leaves as a unit",
  () => {
    const r = range();
    const items = signal([
      { id: 1, on: true },
      { id: 2, on: false },
    ]);
    root(() => each(r.start, r.end, () => items.get(), (x) => x.id, (item) => blockRow(item)));
    assertEq(r.host.textContent, "[1]");
    // Row 2 shows its content in place (same key, new item).
    batch(() => items.set([
      { id: 1, on: true },
      { id: 2, on: true },
    ]));
    assertEq(r.host.textContent, "[1][2]");
    // Reordering must move row 2's content with its markers.
    batch(() => items.set([
      { id: 2, on: true },
      { id: 1, on: true },
    ]));
    assertEq(r.host.textContent, "[2][1]");
    // Removing every row must remove all their nodes.
    batch(() => items.set([]));
    assertEq(r.host.textContent, "");
    assertEq(r.host.childNodes.length, 2, "only the list's own markers remain");
  },
);

// `request.text()` is "the body as UTF-8, or `nil` if it isn't valid
// UTF-8" (27-endpoint-pages.md), but `requestText` uses a non-fatal
// `TextDecoder`, which never throws: invalid bytes become U+FFFD and a
// corrupted string reaches the handler (a webhook verifying a signature
// over text sees different text than was signed).
test(
  "request.text() is nil for a body that isn't valid UTF-8",
  () => {
    assertEq(requestText({ rawBody: new Uint8Array([0x68, 0x69, 0xff, 0xfe]) }), null);
  },
);

// The near miss: valid UTF-8 (multi-byte included) is decoded.
test("request.text() decodes a valid UTF-8 body", () => {
  assertEq(requestText({ rawBody: new TextEncoder().encode("héllo 😀") }), "héllo 😀");
});

// `Bytes` decode with `Uint8Array.from(j)`, which coerces every element:
// `[256, -1, "x", 1.5, null]` becomes `[0, 255, 0, 1, 0]` without an error,
// so an rpc parameter of type `Bytes` accepts arbitrary JSON arrays and
// the handler sees bytes the client never sent. A field that doesn't
// decode is `invalid_argument` (server.md §6): the codec must fail.
test(
  "Bytes decoding refuses elements that aren't bytes",
  () => {
    for (const bad of [[256], [-1], ["x"], [1.5], [null]]) {
      let failure = null;
      try {
        codecs.bytes.decode(bad, "$");
      } catch (e) {
        failure = e;
      }
      assert(failure instanceof DecodeFailure, `${JSON.stringify(bad)} must not decode`);
    }
  },
);

// The near miss: real bytes decode.
test("Bytes decoding accepts bytes", () => {
  const b = codecs.bytes.decode([0, 1, 255], "$");
  assertEq(b.length, 3);
  assertEq(b[2], 255);
});

// `String.count` and `@Length` count grapheme clusters through
// `Intl.Segmenter` (std.js `graphemes`); the server's isolate must have it,
// or both would silently count code points.
test("the isolate segments grapheme clusters", () => {
  assertEq(typeof Intl.Segmenter, "function");
  assertEq(Array.from(new Intl.Segmenter().segment("👍🏽e\u0301🇺🇸")).length, 3);
});
