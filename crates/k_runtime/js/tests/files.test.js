// `k.device.BrowserFiles` (docs/design/42-files.md; docs/compiler/runtime.md §12.4): the picker is a
// hidden file input with the types' `accept`, the chosen files come back read into `Bytes`, closing
// the picker or a file over the limit is a typed error, and `save` downloads through an object URL.
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { BrowserFiles, acceptOf } from "k:/runtime/files.js";
import { unitCase } from "k:runtime";

const json = unitCase(0);
const text = unitCase(1);
const other = (mediaType, fileExtension) => ({ $: 8, mediaType, fileExtension });

/** A browser `File` of `content`. */
const browserFile = (name, type, content) => {
  const bytes = new TextEncoder().encode(content);
  return { name, type, size: bytes.length, arrayBuffer: async () => bytes.buffer };
};

/** The picker the last `open` showed. */
const picker = () => [...document.body.childNodes].find((n) => n.localName === "input");

/** Settles `p` into `{ok}` or `{error}`. */
const settle = (p) => p.then((ok) => ({ ok }), (error) => ({ error }));

test("the accept attribute lists each type's media type and extension", () => {
  assertEq(acceptOf([json, text]), "application/json,.json,text/plain,.txt");
  assertEq(acceptOf([other("text/x-k", "k")]), "text/x-k,.k");
  assertEq(acceptOf([]), "");
});

test("open shows a hidden file input for the types, in the user's action, and reads the choice", async () => {
  let clicked = 0;
  const files = BrowserFiles();
  const result = settle(files.open(null, [json]));
  const input = picker();
  assertEq(input.getAttribute("type"), "file");
  assertEq(input.getAttribute("accept"), "application/json,.json");
  assert(!input.hasAttribute("multiple"));
  assertEq(input.style.getPropertyValue("display"), "none");
  input.addEventListener("click", () => clicked++);
  input.files = [browserFile("board.json", "application/json", '{"a":1}')];
  input.dispatchEvent(new Event("change"));
  const { ok } = await result;
  assertEq(ok.name, "board.json");
  assertEq(ok.mediaType, "application/json");
  assert(ok.bytes instanceof Uint8Array);
  assertEq(new TextDecoder().decode(ok.bytes), '{"a":1}');
  assertEq(picker(), undefined, "the input is removed");
  assertEq(clicked, 0, "the click happened synchronously, before the test listened");
});

test("the picker is opened synchronously, so the browser sees the user's gesture", () => {
  const clicks = [];
  const saved = globalThis.Element.prototype.click;
  globalThis.Element.prototype.click = function () {
    clicks.push(this.localName);
  };
  try {
    BrowserFiles().open(null, []);
    assertDeepEq(clicks, ["input"], "clicked before open returned");
    assert(!picker().hasAttribute("accept"), "no types: any file");
  } finally {
    globalThis.Element.prototype.click = saved;
  }
});

test("openAll allows several files and answers them in order", async () => {
  const result = settle(BrowserFiles().openAll(null, [text]));
  const input = picker();
  assert(input.hasAttribute("multiple"));
  input.files = [browserFile("a.txt", "text/plain", "A"), browserFile("b.txt", "", "B")];
  input.dispatchEvent(new Event("change"));
  const { ok } = await result;
  assertDeepEq(ok.map((f) => [f.name, f.mediaType]), [["a.txt", "text/plain"], ["b.txt", ""]]);
});

test("closing the picker is .cancelled; nothing chosen is too", async () => {
  const closed = settle(BrowserFiles().open(null, []));
  picker().dispatchEvent(new Event("cancel"));
  assertEq((await closed).error, unitCase(0));
  assertEq(picker(), undefined);
  const empty = settle(BrowserFiles().open(null, []));
  const input = picker();
  input.files = [];
  input.dispatchEvent(new Event("change"));
  assertEq((await empty).error, unitCase(0));
});

test("a file over 16 MiB is .tooLarge with the limit, and an unreadable one .unreadable", async () => {
  const big = settle(BrowserFiles().open(null, []));
  let input = picker();
  input.files = [{ name: "big.bin", type: "", size: 16777217, arrayBuffer: async () => new ArrayBuffer(0) }];
  input.dispatchEvent(new Event("change"));
  assertDeepEq((await big).error, { $: 1, limit: 16777216 });
  const broken = settle(BrowserFiles().open(null, []));
  input = picker();
  input.files = [{ name: "gone.txt", type: "", size: 1, arrayBuffer: async () => { throw new Error("NotReadableError"); } }];
  input.dispatchEvent(new Event("change"));
  await tick();
  assertEq((await broken).error, unitCase(2));
});

test("save downloads the bytes as the file's name through an object URL", () => {
  const made = [];
  const revoked = [];
  const timers = [];
  const saved = [globalThis.URL, globalThis.Blob, globalThis.setTimeout];
  globalThis.Blob = class {
    constructor(parts, options) {
      this.parts = parts;
      this.type = options.type;
    }
  };
  globalThis.URL = { createObjectURL: (b) => (made.push(b), "blob:k/1"), revokeObjectURL: (u) => revoked.push(u) };
  globalThis.setTimeout = (f) => timers.push(f);
  const clicks = [];
  const listen = (e) => clicks.push([e.target.getAttribute("href"), e.target.getAttribute("download")]);
  const original = globalThis.Element.prototype.click;
  globalThis.Element.prototype.click = function () {
    listen({ target: this });
  };
  try {
    const bytes = new TextEncoder().encode("[]");
    BrowserFiles().save({ name: "board.json", mediaType: "application/json", bytes });
    assertEq(made.length, 1);
    assertEq(made[0].type, "application/json");
    assertEq(made[0].parts[0], bytes);
    assertDeepEq(clicks, [["blob:k/1", "board.json"]]);
    assert(![...document.body.childNodes].some((n) => n.localName === "a"), "the link is removed");
    timers[0]();
    assertDeepEq(revoked, ["blob:k/1"], "the URL is freed later");
    BrowserFiles().save({ name: "x", mediaType: "", bytes });
    assertEq(made[1].type, "application/octet-stream", "an unknown type downloads as bytes");
  } finally {
    globalThis.Element.prototype.click = original;
    [globalThis.URL, globalThis.Blob, globalThis.setTimeout] = saved;
  }
});

test("save throws FileSaveError.unavailable when the browser refuses downloads", () => {
  const saved = globalThis.URL;
  globalThis.URL = undefined;
  try {
    let thrown = null;
    try {
      BrowserFiles().save({ name: "x", mediaType: "", bytes: new Uint8Array(0) });
    } catch (e) {
      thrown = e;
    }
    assertEq(thrown, unitCase(0));
  } finally {
    globalThis.URL = saved;
  }
});
