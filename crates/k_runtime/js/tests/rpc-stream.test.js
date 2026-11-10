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
import { rpc, sessionMirror } from "k:runtime";
import { gateSession } from "k:/runtime/rpc.js";
import { streamed } from "k:/runtime/streams.js";
import { PushStream, streamFilter, streamMap, streamMerge, streamPrefix } from "k:/runtime/std.js";

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

test("an end that carries the session's mirror sets it, through the router's gate, before the stream ends", async () => {
  // A stream ended because its session did: the router must leave a
  // guarded page before the page's code sees the end and opens the stream
  // again under a session that is gone.
  sessionMirror.set({ user: { id: "ana" } });
  const order = [];
  gateSession((m) => order.push(["gate", m.user]));
  try {
    const s = fakeResponse();
    const o = await rpc("/a.B/Watch", {}, null, { ...streamed(each), fetch: () => Promise.resolve(s.response) });
    const ended = o.message.next().then((v) => order.push(["end", v]));
    s.push(envelope(2, { metadata: { "x-k-session": [JSON.stringify({ user: null })] } }));
    await ended;
    assertDeepEq(order, [["gate", null], ["end", null]]);
    assertDeepEq(sessionMirror.peek(), { user: null });
  } finally {
    gateSession(null);
  }
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

test("Stream.merge yields both streams' elements as they arrive and ends after both", async () => {
  const a = new PushStream();
  const b = new PushStream();
  let closed = 0;
  a.close = () => closed++;
  b.close = () => closed++;
  const merged = streamMerge(a, b);
  const first = merged.next();
  b.push("b1");
  assertEq(await first, "b1");
  a.push("a1");
  a.push("a2");
  assertEq(await merged.next(), "a1");
  assertEq(await merged.next(), "a2");
  a.push(null);
  b.push("b2");
  assertEq(await merged.next(), "b2");
  b.push(null);
  assertEq(await merged.next(), null);
  merged.close();
  assertEq(closed, 2, "closing the merge closes both sources");
});

test("Stream.filter and Stream.map await an async function, one element at a time, in order", async () => {
  const src = new PushStream();
  const order = [];
  let release;
  const slow = new Promise((r) => (release = r));
  // K's async function values take the caller's context first.
  const kept = streamFilter(src, async (c, v) => {
    order.push(`check ${v}`);
    if (v === 1) await slow;
    return v !== 2;
  });
  const out = streamMap(kept, async (c, v) => {
    order.push(`map ${v}`);
    return v * 10;
  });
  src.push(1);
  src.push(2);
  src.push(3);
  const first = out.next();
  await tick();
  assertDeepEq(order, ["check 1"], "the second element waits for the first's call");
  release();
  assertEq(await first, 10);
  assertEq(await out.next(), 30);
  assertDeepEq(order, ["check 1", "map 1", "check 2", "check 3", "map 3"]);
});

test("Stream.prefix(while:) ends at the first refused element and closes its source", async () => {
  const src = new PushStream();
  let closed = 0;
  src.close = () => closed++;
  const small = streamPrefix(src, async (c, v) => v < 3);
  src.push(1);
  src.push(2);
  src.push(3);
  src.push(4);
  assertEq(await small.next(), 1);
  assertEq(await small.next(), 2);
  assertEq(closed, 0);
  assertEq(await small.next(), null, "3 is refused and dropped");
  assertEq(closed, 1, "the source is closed once the prefix ends");
  assertEq(await small.next(), null, "and the stream stays ended");
  assertEq(closed, 1);
});

test("leaving a for await over a derived stream closes its source and cancels the call running", async () => {
  const src = new PushStream();
  let closed = 0;
  src.close = () => closed++;
  const mapped = streamMap(src, async (c, v) => v);
  src.push(1);
  for await (const v of mapped) {
    assertEq(v, 1);
    break;
  }
  assertEq(closed, 1, "break closed the source through the derived stream");
  let signal = null;
  const src2 = new PushStream();
  const pending = streamFilter(src2, (c) => ((signal = c), new Promise(() => {})));
  src2.push(1);
  pending.next();
  await tick();
  assert(signal && !signal.aborted, "the call runs with a live context");
  pending.close();
  assert(signal.aborted, "closing the stream aborts the call's context");
});
