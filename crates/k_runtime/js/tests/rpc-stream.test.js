/**
 * @module k:/tests/rpc-stream.test.js
 *
 * Server-streaming rpcs in the browser (docs/design/29-server-streaming.md;
 * docs/compiler/runtime.md §8; server.md §6 for the wire): the request is
 * one Connect envelope, a refusal or a failure before the stream opens is
 * the call's outcome, and an opened stream yields each element decoded,
 * in order, as envelopes arrive, ending at the end-of-stream envelope, a
 * dropped connection or when its caller is cancelled.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { rpc } from "k:runtime";
import { streamed } from "k:/runtime/streams.js";

/** One Connect envelope: flags, big-endian length, the JSON. */
function envelope(flags, value) {
  const json = new TextEncoder().encode(JSON.stringify(value));
  const out = new Uint8Array(5 + json.length);
  out[0] = flags;
  out[1] = (json.length >>> 24) & 255;
  out[2] = (json.length >>> 16) & 255;
  out[3] = (json.length >>> 8) & 255;
  out[4] = json.length & 255;
  out.set(json, 5);
  return out;
}

/**
 * A fake streamed response: `push(bytes)` delivers a chunk, `end()` ends
 * the body, `fail()` drops the connection; `cancelled` tells whether the
 * reader was cancelled.
 */
function fakeResponse(status = 200) {
  const chunks = [];
  const waiters = [];
  const state = { cancelled: false };
  const deliver = (r) => {
    const w = waiters.shift();
    if (w) w(r);
    else chunks.push(r);
  };
  const reader = {
    read: () => (chunks.length ? Promise.resolve(chunks.shift()) : new Promise((resolve) => waiters.push(resolve))),
    cancel: async () => {
      state.cancelled = true;
      deliver({ done: true, value: undefined });
    },
  };
  return {
    state,
    push: (bytes) => deliver({ done: false, value: bytes }),
    end: () => deliver({ done: true, value: undefined }),
    fail: () => {
      const w = waiters.shift();
      if (w) w(Promise.reject(new TypeError("network error")));
      else chunks.push(Promise.reject(new TypeError("network error")));
    },
    response: { ok: status >= 200 && status < 300, status, headers: { get: () => null }, body: { getReader: () => reader } },
  };
}

/** A stand-in element decoder: wraps the element's `value`. */
const each = (j) => ({ decoded: j.value });

test("a streaming call sends one Connect envelope as application/connect+json", async () => {
  let sent = null;
  const s = fakeResponse();
  const pending = rpc("/notes.NotesProtocol/Watch", { since: 3 }, null, {
    ...streamed(each),
    fetch: (path, init) => {
      sent = { path, init };
      return Promise.resolve(s.response);
    },
  });
  await pending;
  assertEq(sent.path, "/notes.NotesProtocol/Watch");
  assertEq(sent.init.headers["Content-Type"], "application/connect+json");
  const body = sent.init.body;
  assert(body instanceof Uint8Array, "the body is bytes");
  assertEq(body[0], 0, "a message envelope");
  const n = (body[1] << 24) | (body[2] << 16) | (body[3] << 8) | body[4];
  assertEq(n, body.length - 5);
  assertDeepEq(JSON.parse(new TextDecoder().decode(body.subarray(5))), { since: 3 });
});

test("an opened stream yields each element decoded, in order, as it arrives", async () => {
  const s = fakeResponse();
  const o = await rpc("/a.B/Watch", {}, null, { ...streamed(each), fetch: () => Promise.resolve(s.response) });
  assertEq(o.ok, true);
  const stream = o.message;
  const first = stream.next();
  s.push(envelope(0, { value: 1 }));
  assertDeepEq(await first, { decoded: 1 });
  // Two envelopes in one chunk, and one split across two chunks.
  const two = new Uint8Array([...envelope(0, { value: 2 }), ...envelope(0, { value: 3 })]);
  s.push(two);
  const split = envelope(0, { value: 4 });
  s.push(split.subarray(0, 3));
  s.push(split.subarray(3));
  assertDeepEq(await stream.next(), { decoded: 2 });
  assertDeepEq(await stream.next(), { decoded: 3 });
  assertDeepEq(await stream.next(), { decoded: 4 });
});

test("the end-of-stream envelope ends the stream; an error in it is no error", async () => {
  const s = fakeResponse();
  const o = await rpc("/a.B/Watch", {}, null, { ...streamed(each), fetch: () => Promise.resolve(s.response) });
  s.push(envelope(0, { value: "a" }));
  s.push(envelope(2, { error: { code: "internal", message: "internal error" } }));
  const seen = [];
  for await (const v of o.message) seen.push(v.decoded);
  assertDeepEq(seen, ["a"]);
  assertEq(await o.message.next(), null, "an ended stream stays ended");
});

test("a dropped connection ends the stream like its end", async () => {
  const s = fakeResponse();
  const o = await rpc("/a.B/Watch", {}, null, { ...streamed(each), fetch: () => Promise.resolve(s.response) });
  s.push(envelope(0, { value: 1 }));
  s.fail();
  assertDeepEq(await o.message.next(), { decoded: 1 });
  assertEq(await o.message.next(), null);
});

test("a refusal before the stream opens is the call's Connect outcome", async () => {
  const body = JSON.stringify({ code: "unauthenticated", message: "sign in", details: [] });
  const o = await rpc("/a.B/Watch", {}, null, {
    ...streamed(each),
    fetch: () => Promise.resolve({ ok: false, status: 401, headers: { get: () => null }, text: async () => body }),
  });
  assertEq(o.ok, false);
  assertEq(o.kind, "connect");
  assertEq(o.code, "unauthenticated");
});

test("a network failure before the stream opens is offline", async () => {
  const o = await rpc("/a.B/Watch", {}, null, { ...streamed(each), fetch: () => Promise.reject(new TypeError("Failed to fetch")) });
  assertEq(o.kind, "offline");
});

test("the timeout covers opening the stream, not the stream", async () => {
  // Fake timers: the test fires them by hand.
  const timers = new Map();
  let ids = 0;
  globalThis.setTimeout = (fn) => (timers.set(++ids, fn), ids);
  globalThis.clearTimeout = (id) => timers.delete(id);
  try {
    const s = fakeResponse();
    let aborted = null;
    const o = await rpc("/a.B/Watch", {}, null, {
      ...streamed(each),
      timeoutMs: 1,
      fetch: (_p, init) => {
        aborted = init.signal;
        return Promise.resolve(s.response);
      },
    });
    assertEq(o.ok, true);
    for (const fn of timers.values()) fn();
    s.push(envelope(0, { value: "late" }));
    assertDeepEq(await o.message.next(), { decoded: "late" });
    assert(!aborted.aborted, "the open stream wasn't timed out");
  } finally {
    delete globalThis.setTimeout;
    delete globalThis.clearTimeout;
  }
});

test("cancelling the caller closes the stream and its connection", async () => {
  const s = fakeResponse();
  const controller = new AbortController();
  let fetchSignal = null;
  const o = await rpc("/a.B/Watch", {}, controller.signal, {
    ...streamed(each),
    fetch: (_p, init) => {
      fetchSignal = init.signal;
      return Promise.resolve(s.response);
    },
  });
  controller.abort();
  await tick();
  assert(fetchSignal.aborted, "the request is aborted");
  assert(s.state.cancelled, "the body's reader is cancelled");
});

test("leaving a for await loop closes the stream", async () => {
  const s = fakeResponse();
  const o = await rpc("/a.B/Watch", {}, null, { ...streamed(each), fetch: () => Promise.resolve(s.response) });
  s.push(envelope(0, { value: 1 }));
  for await (const v of o.message) {
    assertDeepEq(v, { decoded: 1 });
    break;
  }
  await tick();
  assert(s.state.cancelled, "break closed the stream");
});
