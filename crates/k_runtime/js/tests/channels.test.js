/**
 * @module k:/tests/channels.test.js
 *
 * The `k.channel.v1` frame codec (`runtime/channels.js`; docs/design/44-channels.md, "The wire")
 * against the golden vectors it shares with the Rust front's codec (`channel-frames.json`, also
 * read by `crates/k_server/tests/channels.rs`): every frame decodes to its value and encodes back
 * to exactly its text or bytes, in JSON and in Protobuf, and every invalid frame is rejected.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import {
  decodeChannelFrame,
  decodeChannelFrameBinary,
  encodeChannelFrame,
  encodeChannelFrameBinary,
} from "k:/runtime/channels.js";
import vectors from "k:/tests/channel-frames.json.js";

/** @param {string} s @returns {Uint8Array} */
const unhex = (s) => Uint8Array.from(s.match(/../g) || [], (b) => parseInt(b, 16));

/** @param {Uint8Array} b @returns {string} */
const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

/** The binary frame of a vector, with its hex payloads as bytes. */
function binaryFrame(f) {
  const out = { ...f };
  for (const key of ["request", "error", "value", "user"]) {
    if (typeof out[key] === "string") out[key] = unhex(out[key]);
  }
  return out;
}

/** The frame with byte payloads as hex, for comparing. */
function hexed(f) {
  const out = { ...f };
  for (const key of Object.keys(out)) {
    if (out[key] instanceof Uint8Array) out[key] = hex(out[key]);
  }
  return out;
}

test("JSON frames match the golden vectors", () => {
  for (const v of vectors.frames) {
    assertDeepEq(decodeChannelFrame(v.json), v.frame, `decoding ${v.name}`);
    assertEq(encodeChannelFrame(v.frame), v.json, `encoding ${v.name}`);
  }
});

test("binary frames match the golden vectors", () => {
  for (const v of vectors.frames) {
    assertDeepEq(hexed(decodeChannelFrameBinary(unhex(v.binary))), v.binaryFrame, `decoding ${v.name}`);
    assertEq(hex(encodeChannelFrameBinary(binaryFrame(v.binaryFrame))), v.binary, `encoding ${v.name}`);
  }
});

test("invalid frames are rejected", () => {
  for (const v of vectors.invalidJson) {
    let threw = false;
    try {
      decodeChannelFrame(v.json);
    } catch {
      threw = true;
    }
    assert(threw, `${v.name} decoded`);
  }
  for (const v of vectors.invalidBinary) {
    let threw = false;
    try {
      decodeChannelFrameBinary(unhex(v.binary));
    } catch {
      threw = true;
    }
    assert(threw, `${v.name} decoded`);
  }
});

test("a frame decodes from what it encodes to, past the vectors", () => {
  const frames = [
    { kind: "msg", ch: 0xffffffff, seq: 2 ** 53 - 1, value: { a: [1, "two", null] } },
    { kind: "resume", ch: 9, token: "ünïcode ✓", received: 128, sent: 16384 },
    { kind: "open", ch: 3, method: "", request: null },
  ];
  for (const f of frames) assertDeepEq(decodeChannelFrame(encodeChannelFrame(f)), f, f.kind);
  const binary = { kind: "msg", ch: 0xffffffff, seq: 2 ** 53 - 1, value: Uint8Array.of(0, 255) };
  assertDeepEq(hexed(decodeChannelFrameBinary(encodeChannelFrameBinary(binary))), hexed(binary), "binary");
});
