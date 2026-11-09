/**
 * @module k:/tests/channel-client.test.js
 *
 * The channels' browser client (`runtime/channels.js`; docs/design/44-channels.md, "The client",
 * "Reconnecting and resuming", "Heartbeats") against a scripted server: a fake `WebSocket` whose
 * frames the test reads and answers, virtual timers, the seeded random source of `k test`, and
 * the window's `online`, `visibilitychange`, `pagehide` and `pageshow` events. The server side
 * of each exchange is what `k_server::sockets` does (`crates/k_server/tests/channel_server.rs`
 * drives the real one).
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { seededRandom } from "k:testing/kit";
import { channelEnv, decodeChannelFrame, encodeChannelFrame, openChannel } from "k:/runtime/channels.js";
import { flush, rpcError, sessionMirror, unitCase } from "k:runtime";

/** A minimal event target standing in for the window and its document. */
function target(props = {}) {
  const listeners = new Map();
  return Object.assign(props, {
    addEventListener(type, f) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(f);
    },
    removeEventListener(type, f) {
      listeners.get(type)?.delete(f);
    },
    dispatchEvent(e) {
      for (const f of listeners.get(e.type) ?? []) f(e);
    },
  });
}

/**
 * A scripted network: `sockets` opened (each with `sent`, the frames the client sent, decoded,
 * and `closed`, the code it closed with), virtual time (`advance`), the window's events
 * (`fire`), `online` and `events`, the window events the client dispatched.
 */
function network(seed = "channels") {
  const n = { sockets: [], timers: [], now: 0, online: true, events: [] };
  const random = seededRandom(seed);
  n.draws = [];
  globalThis.Event ??= class {
    constructor(type) {
      this.type = type;
    }
  };
  const win = target();
  for (const k of ["addEventListener", "removeEventListener", "dispatchEvent"]) {
    Object.defineProperty(globalThis, k, { configurable: true, writable: true, value: win[k] });
  }
  win.addEventListener("k-online", () => n.events.push("k-online"));
  win.addEventListener("k-offline", () => n.events.push("k-offline"));
  n.document = target({ visibilityState: "visible" });
  Object.defineProperty(globalThis, "document", { configurable: true, writable: true, value: n.document });
  n.fire = (type) => win.dispatchEvent(new Event(type));
  channelEnv({
    socket: () => {
      const s = {
        sent: [],
        closed: null,
        send(text) {
          this.sent.push(decodeChannelFrame(text));
        },
        close(code) {
          this.closed = code;
        },
      };
      n.sockets.push(s);
      return s;
    },
    now: () => n.now,
    after: (ms, f) => {
      const t = { due: n.now + ms, f, live: true };
      n.timers.push(t);
      return t;
    },
    cancel: (t) => {
      if (t) t.live = false;
    },
    random: () => {
      const d = random.double();
      n.draws.push(d);
      return d;
    },
    online: () => n.online,
  });
  /** Moves time on by `ms`, firing due timers in order. */
  n.advance = (ms) => {
    const end = n.now + ms;
    for (;;) {
      const due = n.timers.filter((t) => t.live && t.due <= end).sort((a, b) => a.due - b.due)[0];
      if (!due) break;
      n.now = due.due;
      due.live = false;
      due.f();
    }
    n.now = end;
  };
  /** The latest socket. */
  n.last = () => n.sockets[n.sockets.length - 1];
  /** The server sends `frame` on socket `s` (the latest by default). */
  n.say = (frame, s = n.last()) => s.onmessage({ data: encodeChannelFrame(frame) });
  /** The socket opens. */
  n.up = (s = n.last()) => {
    s.isOpen = true;
    s.onopen();
  };
  /** The socket closes with `code` and `reason`. */
  n.kill = (code = 1006, reason = "", s = n.last()) => s.onclose({ code, reason });
  /** The frames sent on socket `s` of `kind`. */
  n.sent = (kind, s = n.last()) => s.sent.filter((f) => f.kind === kind);
  return n;
}

/** The test's refusal enum, as generated code builds one (`notAllowed`, `rpcFailure(RpcFailure)`). */
const E = { notAllowed: unitCase(0), rpcFailure: (_0) => ({ $: 1, _0 }) };

/** Codecs of a channel of plain JSON messages; a server message `{case: "unknown"}` doesn't decode. */
const codec = {
  encode: (m) => m,
  decode: (j) => {
    if (j?.case === "unknown") throw new Error("unknown case");
    return j;
  },
  refusal: (o) => rpcError(o, E),
};

/** `rpcFailure(.offline)` of {@link E}. */
const OFFLINE = E.rpcFailure({ $: 2, _0: null });

/** Opens a channel on `n`'s socket and has the server accept it with token `tok`. */
async function opened(n, method = "/t.P/Room", tok = "tok-1") {
  const p = openChannel(method, { id: "a" }, null, codec);
  if (!n.last().isOpen) n.up();
  const open = n.sent("open").pop();
  n.say({ kind: "opened", ch: open.ch, resume: tok });
  const o = await p;
  assert(o.ok, `the channel opened: ${JSON.stringify(o)}`);
  return o.message;
}

/** Settles `p` into `{ok, value}` without letting a rejection escape. */
const outcome = (p) => p.then((value) => ({ ok: true, value }), (value) => ({ ok: false, value }));

/** Takes `count` messages from a channel's stream. */
async function take(ch, count) {
  const out = [];
  for (let i = 0; i < count; i++) out.push(await ch.messages.next());
  return out;
}

test("the first open sends `open` on a new socket and resolves with the channel when the server opens it", async () => {
  const n = network();
  const p = openChannel("/t.P/Room", { id: "a" }, null, codec);
  assertEq(n.sockets.length, 1, "one socket, opened for the first channel");
  n.up();
  assertDeepEq(n.sent("open"), [{ kind: "open", ch: 1, method: "/t.P/Room", request: { id: "a" } }]);
  n.say({ kind: "opened", ch: 1, resume: "tok-1" });
  const o = await p;
  assert(o.ok);
  assertDeepEq(o.message.state, unitCase(1), "open");
  assertDeepEq(n.events, ["k-online"], "the socket opening says the network is there");
});

test("a second channel shares the socket under its own number", async () => {
  const n = network();
  const a = await opened(n);
  const b = await opened(n, "/t.P/Pair", "tok-2");
  assertEq(n.sockets.length, 1);
  assertDeepEq(n.sent("open").map((f) => f.ch), [1, 2]);
  n.say({ kind: "msg", ch: 2, seq: 1, value: { case: "to-b" } });
  n.say({ kind: "msg", ch: 1, seq: 1, value: { case: "to-a" } });
  assertDeepEq(await take(a, 1), [{ case: "to-a" }]);
  assertDeepEq(await take(b, 1), [{ case: "to-b" }]);
});

test("server messages arrive in order; a repeated one and a case this tab doesn't know are skipped", async () => {
  const n = network();
  const ch = await opened(n);
  n.say({ kind: "msg", ch: 1, seq: 1, value: { case: "one" } });
  n.say({ kind: "msg", ch: 1, seq: 1, value: { case: "one" } });
  n.say({ kind: "msg", ch: 1, seq: 2, value: { case: "unknown" } });
  n.say({ kind: "msg", ch: 1, seq: 3, value: { case: "three" } });
  assertDeepEq(await take(ch, 2), [{ case: "one" }, { case: "three" }]);
});

test("a refused open is the server's error; the socket closes with no channel left", async () => {
  const n = network();
  const p = openChannel("/t.P/Room", { id: "closed" }, null, codec);
  n.up();
  n.say({ kind: "refused", ch: 1, error: { code: "failed_precondition", message: "x", details: [{ type: "k.TypedError", debug: { case: "closed", fields: {} } }] } });
  const o = await p;
  assertEq(o.ok, false);
  assertEq(o.code, "failed_precondition");
  assertEq(o.details[0].debug.case, "closed");
  assertEq(n.last().closed, 1000, "no channel is left, so the socket closes");
});

test("a rate-limited open carries its wait", async () => {
  const n = network();
  const p = openChannel("/t.P/Room", { id: "a" }, null, codec);
  n.up();
  n.say({ kind: "refused", ch: 1, error: { code: "resource_exhausted", message: "x", retryAfter: 3 } });
  const o = await p;
  assertDeepEq([o.code, o.retryAfter], ["resource_exhausted", 3]);
});

test("the first open isn't retried: a socket that fails is `offline`, a silent server `timeout`", async () => {
  const n = network();
  const p = openChannel("/t.P/Room", { id: "a" }, null, codec);
  n.kill(1006);
  assertDeepEq(await p, { ok: false, kind: "offline" });
  assertEq(n.timers.filter((t) => t.live).length, 0, "nothing reconnects");
  const q = openChannel("/t.P/Room", { id: "a" }, null, codec);
  n.up();
  n.advance(30_000);
  assertDeepEq(await q, { ok: false, kind: "timeout" });
});

test("a send is transmitted with the next sequence number and settles with the acknowledgement", async () => {
  const n = network();
  const ch = await opened(n);
  const one = outcome(ch.send(null, { case: "post", text: "hi" }));
  const two = outcome(ch.send(null, { case: "refuse" }));
  const three = outcome(ch.send(null, { case: "late" }));
  assertDeepEq(n.sent("msg"), [
    { kind: "msg", ch: 1, seq: 1, value: { case: "post", text: "hi" } },
    { kind: "msg", ch: 1, seq: 2, value: { case: "refuse" } },
    { kind: "msg", ch: 1, seq: 3, value: { case: "late" } },
  ]);
  n.say({ kind: "ack", ch: 1, seq: 1 });
  n.say({ kind: "ack", ch: 1, seq: 2, error: { code: "failed_precondition", message: "x", details: [{ type: "k.TypedError", debug: { case: "notAllowed", fields: {} } }] } });
  n.say({ kind: "ack", ch: 1, seq: 3, error: { code: "unavailable", message: "the channel is closed" } });
  assertDeepEq(await one, { ok: true, value: undefined });
  assertDeepEq(await two, { ok: false, value: E.notAllowed }, "the typed refusal");
  assertDeepEq(await three, { ok: false, value: OFFLINE }, "a channel gone in the server is offline");
});

test("a refusal for a rate limit or an invalid message is `rpcFailure` with its detail", async () => {
  const n = network();
  const ch = await opened(n);
  const limited = outcome(ch.send(null, { case: "post" }));
  n.say({ kind: "ack", ch: 1, seq: 1, error: { code: "resource_exhausted", message: "x", retryAfter: 2 } });
  assertDeepEq(await limited, { ok: false, value: E.rpcFailure({ $: 0, _0: { $: 2, retryAfter: 2000 } }) });
  const big = await outcome(ch.send(null, { case: "post", text: "x".repeat(70_000) }));
  assertEq(big.value.$, 1);
  assertEq(big.value._0.$, 1, "rpcFailure(.invalid(…))");
  assertEq(n.sent("msg").length, 1, "a message over 64 KiB is never transmitted");
});

test("at most 32 messages are in flight; the next waits for an acknowledgement", async () => {
  const n = network();
  const ch = await opened(n);
  for (let i = 0; i < 33; i++) ch.send(null, { i }).catch(() => {});
  assertEq(n.sent("msg").length, 32);
  n.say({ kind: "ack", ch: 1, seq: 1 });
  assertEq(n.sent("msg").length, 33);
  assertDeepEq(n.sent("msg")[32], { kind: "msg", ch: 1, seq: 33, value: { i: 32 } });
});

test("close() sends `close`, ends the stream, fails later sends and closes the socket after the last channel", async () => {
  const n = network();
  const ch = await opened(n);
  const loop = (async () => {
    const seen = [];
    for await (const m of ch.messages) seen.push(m);
    return seen;
  })();
  n.say({ kind: "msg", ch: 1, seq: 1, value: { case: "one" } });
  await tick();
  ch.close();
  ch.close();
  assertDeepEq(n.sent("close"), [{ kind: "close", ch: 1, reason: "byClient" }], "once: close is idempotent");
  assertDeepEq(ch.state, { $: 3, _0: unitCase(0) }, "closed(byClient)");
  assertDeepEq(await loop, [{ case: "one" }]);
  assertDeepEq(await outcome(ch.send(null, { case: "post" })), { ok: false, value: OFFLINE });
  assertEq(n.last().closed, 1000);
});

test("cancelling the owner closes the channel; a cancelled send that wasn't transmitted is withdrawn", async () => {
  const n = network();
  const owner = new AbortController();
  const p = openChannel("/t.P/Room", { id: "a" }, owner.signal, codec);
  n.up();
  n.say({ kind: "opened", ch: 1, resume: "tok" });
  const ch = (await p).message;
  n.kill(1006);
  const sender = new AbortController();
  ch.send(sender.signal, { case: "draft" });
  sender.abort();
  n.advance(1000);
  n.up();
  n.say({ kind: "opened", ch: 2, resume: "tok" });
  assertEq(n.sent("msg").length, 0, "the withdrawn message never goes");
  owner.abort();
  assertDeepEq(ch.state, { $: 3, _0: unitCase(0) });
  assertDeepEq(n.sent("close"), [{ kind: "close", ch: 2, reason: "byClient" }]);
});

test("the server's close ends the channel with its reason and nothing reconnects it", async () => {
  const n = network();
  const ch = await opened(n);
  n.say({ kind: "close", ch: 1, reason: "byServer" });
  assertDeepEq(ch.state, { $: 3, _0: unitCase(1) });
  assertEq(await ch.messages.next(), null);
  const s = await opened(n, "/t.P/Room", "tok-2");
  n.say({ kind: "close", ch: 2, reason: "sessionEnded" });
  assertDeepEq(s.state, { $: 3, _0: unitCase(2) });
});

test("a dropped socket reconnects after a jittered delay and resumes: nothing lost, nothing twice", async () => {
  const n = network();
  const ch = await opened(n);
  n.say({ kind: "msg", ch: 1, seq: 1, value: { n: 1 } });
  n.say({ kind: "msg", ch: 1, seq: 2, value: { n: 2 } });
  const acked = outcome(ch.send(null, { case: "a" }));
  const unacked = outcome(ch.send(null, { case: "b" }));
  n.say({ kind: "ack", ch: 1, seq: 1 });
  n.kill(1006);
  const delay = Math.floor(n.draws[0] * 500);
  const reconnecting = () => [ch.state.$, ch.state.attempt, ch.state.at.ms];
  assertDeepEq(reconnecting(), [2, 1, delay], "reconnecting(attempt: 1, at:) up to 0.5 s away");
  const waiting = outcome(ch.send(null, { case: "c" }));
  n.advance(delay - 1);
  assertEq(n.sockets.length, 1, "not before its time");
  n.advance(1);
  assertEq(n.sockets.length, 2);
  n.up();
  assertDeepEq(ch.state, unitCase(0), "connecting once the socket is back, until the server reattaches it");
  assertDeepEq(n.sent("resume"), [{ kind: "resume", ch: 2, token: "tok-1", received: 2, sent: 2 }]);
  n.say({ kind: "opened", ch: 2, resume: "tok-1" });
  assertDeepEq(ch.state, unitCase(1));
  assertDeepEq(n.sent("msg"), [
    { kind: "msg", ch: 2, seq: 2, value: { case: "b" } },
    { kind: "msg", ch: 2, seq: 3, value: { case: "c" } },
  ], "the unacknowledged message again, with its number, then the one that waited");
  n.say({ kind: "msg", ch: 2, seq: 2, value: { n: 2 } });
  n.say({ kind: "msg", ch: 2, seq: 3, value: { n: 3 } });
  n.say({ kind: "ack", ch: 2, seq: 2 });
  n.say({ kind: "ack", ch: 2, seq: 3 });
  assertDeepEq(await take(ch, 3), [{ n: 1 }, { n: 2 }, { n: 3 }]);
  assertDeepEq([(await acked).ok, (await unacked).ok, (await waiting).ok], [true, true, true]);
});

test("a resume the server refuses restarts the channel: transmitted messages fail `offline`, waiting ones go on the new one", async () => {
  const n = network();
  const ch = await opened(n);
  n.say({ kind: "msg", ch: 1, seq: 1, value: { n: 1 } });
  const transmitted = outcome(ch.send(null, { case: "a" }));
  n.kill(1006);
  const waiting = outcome(ch.send(null, { case: "b" }));
  n.advance(500);
  n.up();
  n.say({ kind: "refused", ch: 2, error: { code: "not_found" } });
  assertDeepEq(await transmitted, { ok: false, value: OFFLINE }, "the old server may or may not have handled it");
  assertDeepEq(n.sent("open"), [{ kind: "open", ch: 3, method: "/t.P/Room", request: { id: "a" } }], "the same arguments");
  assertDeepEq(ch.state, unitCase(0), "connecting");
  n.say({ kind: "opened", ch: 3, resume: "tok-2" });
  assertDeepEq(n.sent("msg"), [{ kind: "msg", ch: 3, seq: 1, value: { case: "b" } }], "numbered from 1 on the new channel");
  n.say({ kind: "msg", ch: 3, seq: 1, value: { snapshot: true } });
  n.say({ kind: "ack", ch: 3, seq: 1 });
  assertDeepEq(await take(ch, 2), [{ n: 1 }, { snapshot: true }]);
  assertEq((await waiting).ok, true);
});

test("a restart the server refuses closes the channel as refused", async () => {
  const n = network();
  const ch = await opened(n);
  n.kill(1006);
  n.advance(500);
  n.up();
  n.say({ kind: "refused", ch: 2, error: { code: "not_found" } });
  n.say({ kind: "refused", ch: 3, error: { code: "failed_precondition" } });
  assertDeepEq(ch.state, { $: 3, _0: unitCase(3) });
});

test("messages wait at most 10 seconds and at most 32 while reconnecting", async () => {
  const n = network();
  const ch = await opened(n);
  n.online = false;
  n.kill(1006);
  const first = outcome(ch.send(null, { i: 0 }));
  for (let i = 1; i < 32; i++) ch.send(null, { i }).catch(() => {});
  assertDeepEq(await outcome(ch.send(null, { i: 32 })), { ok: false, value: OFFLINE }, "the 33rd at once");
  n.advance(10_000);
  assertDeepEq(await first, { ok: false, value: OFFLINE }, "after 10 s");
});

test("each attempt's delay is drawn up to 0.5, 1, 2, 4, 8, 16, then 30 s, the same draws for the same seed", async () => {
  const delays = [];
  for (const run of [0, 1]) {
    const n = network("backoff");
    const ch = await opened(n);
    const mine = [];
    n.kill(1006);
    for (let i = 0; i < 9; i++) {
      const at = ch.state.at.ms;
      mine.push(at - n.now);
      n.advance(at - n.now);
      n.kill(1006);
    }
    const caps = [500, 1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000];
    mine.forEach((d, i) => assert(d >= 0 && d < caps[i], `attempt ${i + 1}: ${d} ms under ${caps[i]}`));
    assertEq(ch.state.attempt, 10);
    assertEq(n.events.filter((e) => e === "k-offline").length, 9, "each failed reconnect says offline");
    delays[run] = mine;
  }
  assertDeepEq(delays[0], delays[1], "a test's timings repeat exactly");
});

test("offline, nothing is tried; coming online or becoming visible tries at once", async () => {
  const n = network();
  await opened(n);
  n.online = false;
  n.kill(1006);
  n.advance(60_000);
  assertEq(n.sockets.length, 1, "no attempt while offline");
  n.online = true;
  n.fire("online");
  assertEq(n.sockets.length, 2, "at once when the network is back");
  n.kill(1006);
  n.document.visibilityState = "hidden";
  n.document.dispatchEvent(new Event("visibilitychange"));
  assertEq(n.sockets.length, 2);
  n.document.visibilityState = "visible";
  n.document.dispatchEvent(new Event("visibilitychange"));
  assertEq(n.sockets.length, 3, "at once when the page is shown");
});

test("a socket that closes going away reconnects after the server's delay; an overflow keeps the backoff", async () => {
  const n = network();
  const ch = await opened(n);
  n.kill(1001, "k.retry=1234");
  assertEq(ch.state.at.ms - n.now, 1234);
  n.advance(1234);
  assertEq(n.sockets.length, 2);
  n.up();
  n.say({ kind: "opened", ch: 2, resume: "tok-1" });
  n.kill(1013, "k.overflow");
  n.advance(500);
  n.up();
  n.kill(1006);
  assertEq(ch.state.attempt, 2, "the attempt after an overflow isn't reset by the next open");
});

test("a server heartbeat is answered with one and with receipts; receipts follow messages within a second", async () => {
  const n = network();
  await opened(n);
  n.say({ kind: "msg", ch: 1, seq: 1, value: {} });
  n.say({ kind: "msg", ch: 1, seq: 2, value: {} });
  assertDeepEq(n.sent("rcvd"), []);
  n.advance(1000);
  assertDeepEq(n.sent("rcvd"), [{ kind: "rcvd", ch: 1, seq: 2 }]);
  n.say({ kind: "msg", ch: 1, seq: 3, value: {} });
  n.say({ kind: "beat" });
  assertDeepEq(n.sent("beat"), [{ kind: "beat" }]);
  assertDeepEq(n.sent("rcvd"), [{ kind: "rcvd", ch: 1, seq: 2 }, { kind: "rcvd", ch: 1, seq: 3 }]);
  n.advance(1000);
  assertEq(n.sent("rcvd").length, 2, "nothing new to confirm");
});

test("a socket silent for 60 seconds is dead: it is closed and the channels reconnect", async () => {
  const n = network();
  const ch = await opened(n);
  n.advance(30_000);
  n.say({ kind: "beat" });
  n.advance(59_999);
  assertEq(n.sockets[0].closed, null, "a frame 30 s in reset the clock");
  n.advance(1);
  assertEq(n.sockets[0].closed, 1000);
  assertEq(ch.state.$, 2, "reconnecting");
});

test("the back-forward cache: `pagehide` drops the socket, `pageshow` resumes at once", async () => {
  const n = network();
  const ch = await opened(n);
  n.fire("pagehide");
  assertEq(n.sockets[0].closed, 1000);
  n.advance(60_000);
  assertEq(n.sockets.length, 1, "a hidden page doesn't reconnect");
  n.fire("pageshow");
  assertEq(n.sockets.length, 2);
  n.up();
  assertDeepEq(n.sent("resume"), [{ kind: "resume", ch: 2, token: "tok-1", received: 0, sent: 0 }]);
  n.say({ kind: "opened", ch: 2, resume: "tok-1" });
  assertDeepEq(ch.state, unitCase(1));
});

test("the guest mirror from the socket ends the session in the page; a new session replaces the socket and restarts", async () => {
  const n = network();
  sessionMirror.set({ user: { id: "ana" } });
  const ch = await opened(n);
  n.say({ kind: "session", user: null });
  assertDeepEq(sessionMirror.peek(), { user: null });
  flush();
  await tick();
  assertEq(n.sockets[0].closed, 1000, "the socket of the old session is closed");
  assertEq(n.sockets.length, 2, "and a new one opened at once");
  n.up();
  assertDeepEq(n.sent("open"), [{ kind: "open", ch: 2, method: "/t.P/Room", request: { id: "a" } }], "restarted, not resumed");
  n.say({ kind: "opened", ch: 2, resume: "tok-9" });
  assertDeepEq(ch.state, unitCase(1));
});
