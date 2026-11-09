/**
 * @module k:/tests/rtc.test.js
 *
 * `k.rtc` (docs/design/46-media-and-rtc.md, "Peer connections"; docs/compiler/runtime.md §12.15):
 * `runtime/rtc.js`'s negotiation between two connections over a scripted pair of signalling
 * channels and a relay that says `peerJoined` and `peerLeft`, on the in-process engine of
 * `testing/rtc.js`, with virtual timers; the stats a browser reports; the data channels' ids;
 * and the server's coturn credentials (`runtime/relay.js`) against RFC 2202's and Python's
 * `hmac` vectors.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { dataChannelId, peerConnections, peerStats } from "k:/runtime/rtc.js";
import { hmacSha1, iceServers, sha1, turnCredential } from "k:/runtime/relay.js";
import { fakeRtc } from "k:/testing/rtc.js";
import { unitCase } from "k:runtime";
import { PushStream } from "k:/runtime/std.js";

const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
const bytes = (s) => new TextEncoder().encode(s);

/** Lets every queued microtask run, the negotiation's many hops included. */
async function settle() {
  for (let i = 0; i < 400; i++) await null;
}

/** Virtual timers: `after(ms, f)`, `cancel(h)`, and `advance(ms)`. */
function timers() {
  let now = 0;
  let due = [];
  return {
    after(ms, f) {
      const h = { at: now + ms, f };
      due.push(h);
      return h;
    },
    cancel(h) {
      due = due.filter((x) => x !== h);
    },
    advance(ms) {
      now += ms;
      const ready = due.filter((h) => h.at <= now);
      due = due.filter((h) => h.at > now);
      for (const h of ready) h.f();
    },
  };
}

/**
 * A signalling channel of the relay's room: `messages` a stream of `Signal`s, `send` forwards to
 * the other peers, `close` leaves (`peerLeft` for the others). `sent` lists what it sent.
 */
function room() {
  const peers = [];
  return {
    join() {
      const ch = {
        messages: new PushStream(),
        sent: [],
        closed: false,
        send(_c, s) {
          ch.sent.push(s);
          if (ch.closed) return Promise.reject(unitCase(0));
          for (const p of peers) if (p !== ch) queueMicrotask(() => p.messages.push(s));
          return Promise.resolve();
        },
        close() {
          if (ch.closed) return;
          ch.closed = true;
          peers.splice(peers.indexOf(ch), 1);
          ch.messages.push(null);
          for (const p of peers) p.messages.push(unitCase(1));
        },
      };
      if (peers.length) ch.messages.push(unitCase(0));
      for (const p of peers) p.messages.push(unitCase(0));
      peers.push(ch);
      return ch;
    },
  };
}

/** A track as the kit's devices make them, with its handle. */
function track(kind) {
  const t = { kind, label: `Test ${kind}`, enabled: true, muted: false, stop() {} };
  const h = { kind: unitCase(kind === "video" ? 1 : 0), $t: t, state: unitCase(0) };
  t.$handle = h;
  return h;
}
const stream = (...tracks) => ({ $tracks: tracks, $s: null, audioTracks: tracks.filter((t) => t.kind.$ === 0), videoTracks: tracks.filter((t) => t.kind.$ === 1) });

/** A JSON codec of plain values, and a kind of it. */
const json = { encode: (v) => v, decode: (j) => j };
const kind = (name, shape = "s1") => ({ name, codec: json, shape });

/** Two connected peers on one engine, with their engine and timers. */
async function call() {
  const engine = fakeRtc();
  const clock = timers();
  const env = { RTCPeerConnection: engine.RTCPeerConnection, after: clock.after, cancel: clock.cancel };
  const pcs = peerConnections(env);
  const r = room();
  const ana = pcs.connect(null, r.join(), [], unitCase(0), unitCase(0));
  const bob = pcs.connect(null, r.join(), [], unitCase(0), unitCase(0));
  const [a, b] = await Promise.all([ana, bob]);
  await settle();
  return { a, b, engine, clock, pcs, r };
}

test("two peers pair through the relay and connect, one polite and one not", async () => {
  const { a, b } = await call();
  assertEq(a.state.$, 2, "ana is connected");
  assertEq(b.state.$, 2, "bob is connected");
  assert(a.$conn.polite !== b.$conn.polite, "exactly one is polite");
});

test("connect waits for a peer, and fails when the channel ends first", async () => {
  const engine = fakeRtc();
  const pcs = peerConnections({ RTCPeerConnection: engine.RTCPeerConnection, after: () => null, cancel() {} });
  const ch = room().join();
  const waiting = pcs.connect(null, ch, [], null, null);
  let settled = false;
  waiting.then(
    () => (settled = true),
    () => (settled = true),
  );
  await settle();
  assert(!settled, "still waiting for a peer");
  ch.messages.push(null);
  let error = null;
  try {
    await waiting;
  } catch (e) {
    error = e;
  }
  assertEq(error?.$, 1, "PeerError.signallingClosed");
});

test("no RTCPeerConnection is PeerError.unsupported", async () => {
  const pcs = peerConnections({ RTCPeerConnection: undefined, after: () => null, cancel() {} });
  assertEq(pcs.isSupported, false);
  let error = null;
  try {
    await pcs.connect(null, room().join(), [], null, null);
  } catch (e) {
    error = e;
  }
  assertEq(error?.$, 0);
});

test("a sent stream arrives as the peer's remote, and replace swaps it without renegotiating", async () => {
  const { a, b } = await call();
  const camera = track("video");
  const sender = a.send(stream(camera, track("audio")));
  await settle();
  assertEq(b.remote?.videoTracks.length, 1);
  assertEq(b.remote?.audioTracks.length, 1);
  assertEq(b.remote.videoTracks[0].$t.$src, camera.$t, "bob receives ana's camera");
  const offers = a.$conn.ch.sent.filter((s) => s.$ === 3).length + b.$conn.ch.sent.filter((s) => s.$ === 3).length;
  const screen = track("video");
  sender.replace(stream(screen, track("audio")));
  await settle();
  assertEq(b.remote.videoTracks[0].$t.$src, screen.$t, "the same received track now plays the screen");
  assertEq(a.$conn.ch.sent.filter((s) => s.$ === 3).length + b.$conn.ch.sent.filter((s) => s.$ === 3).length, offers, "no new descriptions");
  sender.stop();
  await settle();
  assertEq(b.remote, null, "stopping ends the peer's tracks");
});

test("glare: both peers offer at once, and perfect negotiation still delivers both streams", async () => {
  const { a, b, engine } = await call();
  engine.glare();
  a.send(stream(track("video")));
  b.send(stream(track("audio")));
  await settle();
  assertEq(b.remote?.videoTracks.length, 1, "bob has ana's video");
  assertEq(a.remote?.audioTracks.length, 1, "ana has bob's audio");
  assertEq(a.state.$, 2);
  assertEq(b.state.$, 2);
});

test("a lost network reconnects after an ICE restart by the impolite peer", async () => {
  const { a, b, engine } = await call();
  engine.lost();
  await settle();
  assertEq(a.state.$, 3, "reconnecting");
  assertEq(b.state.$, 3, "reconnecting");
  engine.restored();
  await settle();
  assertEq(a.state.$, 2);
  assertEq(b.state.$, 2);
});

test("failed ICE recovers only through the restart", async () => {
  const { a, b, engine } = await call();
  engine.iceFails();
  await settle();
  assertEq(a.state.$, 2, "the impolite peer restarted and both reconnected");
  assertEq(b.state.$, 2);
});

test("15 seconds without recovery close the call as failed", async () => {
  const { a, engine, clock } = await call();
  engine.lost();
  await settle();
  clock.advance(14_999);
  assertEq(a.state.$, 3);
  clock.advance(1);
  assertDeepEq(a.state, { $: 4, _0: unitCase(2) });
});

test("hanging up says bye: the peer closes byPeer, and both channels close", async () => {
  const { a, b } = await call();
  a.close();
  await settle();
  assertDeepEq(a.state, { $: 4, _0: unitCase(0) });
  assertDeepEq(b.state, { $: 4, _0: unitCase(1) });
  assert(a.$conn.ch.closed && b.$conn.ch.closed);
});

test("a peer that rejoins with a new id (a reload) starts the call over, keeping what was sent", async () => {
  const { a, b, r, pcs } = await call();
  a.send(stream(track("video")));
  await settle();
  b.$conn.ch.close();
  const again = await pcs.connect(null, r.join(), [], null, null);
  await settle();
  assertEq(a.state.$, 2, "ana is connected to the new tab");
  assertEq(again.state.$, 2);
  assertEq(again.remote?.videoTracks.length, 1, "the new tab receives ana's camera");
});

test("typed data channels: messages both ways, checked shapes, and the size limit", async () => {
  const { a, b } = await call();
  const chat = kind("chat");
  const ac = a.channel(chat);
  const bc = b.channel(chat);
  await ac.send(null, { text: "hi" });
  await bc.send(null, { text: "hello" });
  assertDeepEq(await bc.messages.next(), { text: "hi" });
  assertDeepEq(await ac.messages.next(), { text: "hello" });
  assert(ac.isOpen);
  assert(a.channel(chat) === ac, "one channel per kind");
  let error = null;
  try {
    await ac.send(null, { text: "x".repeat(70000) });
  } catch (e) {
    error = e;
  }
  assertDeepEq(error, { $: 1, limit: 65536 });
});

test("a data channel whose peer has another message shape closes incompatible", async () => {
  const { a, b } = await call();
  const mine = a.channel(kind("cursor", "aaaa"));
  b.channel(kind("cursor", "bbbb"));
  let error = null;
  try {
    await mine.send(null, { x: 1 });
  } catch (e) {
    error = e;
  }
  assertEq(error?.$, 2, "DataChannelError.incompatible");
  assertEq(await mine.messages.next(), null);
});

test("stats: the selected pair's round trip, bitrate and relay", async () => {
  const { a } = await call();
  assertDeepEq(await a.stats(), { roundTripTime: 1, outgoingBitrate: 1000000, packetsLost: 0, isRelayed: false });
  const report = new Map([
    ["T", { id: "T", type: "transport", selectedCandidatePairId: "P" }],
    ["P", { id: "P", type: "candidate-pair", localCandidateId: "L", currentRoundTripTime: 0.0425 }],
    ["L", { id: "L", type: "local-candidate", candidateType: "relay" }],
    ["I", { id: "I", type: "inbound-rtp", packetsLost: 3 }],
  ]);
  assertDeepEq(peerStats(report), { roundTripTime: 43, outgoingBitrate: null, packetsLost: 3, isRelayed: true });
  assertDeepEq(peerStats(null), { roundTripTime: null, outgoingBitrate: null, packetsLost: 0, isRelayed: false });
});

test("data channel ids are the compiler's (K2409)", () => {
  assertEq(dataChannelId("chat"), 721);
  assertEq(dataChannelId("cursor"), 467);
  assertEq(dataChannelId("al"), 240);
  assertEq(dataChannelId("el"), 240);
});

test("SHA-1 and HMAC-SHA1 match FIPS 180 and RFC 2202", () => {
  assertEq(hex(sha1(bytes("abc"))), "a9993e364706816aba3e25717850c26c9cd0d89d");
  assertEq(hex(sha1(bytes("a".repeat(1000)))), "291e9a6c66994949b57ba5e650361e98fc36b1ba");
  assertEq(hex(hmacSha1(new Uint8Array(20).fill(0x0b), bytes("Hi There"))), "b617318655057264e28bc0b6fb378c8ef146be00");
});

test("coturn credentials: <expiry>:<user>, signed with the shared secret", async () => {
  assertDeepEq(turnCredential("north", "ana", 1700003600), { username: "1700003600:ana", credential: "+5Wu0RaejH8eFX2gvpvBno8V880=" });
  // A key longer than a block is hashed first.
  assertEq(turnCredential("k".repeat(100), "", 0).credential.length, 28);
  const secret = { reveal: () => "north" };
  const clock = () => ({ now: { ms: 1700000000000 } });
  const none = iceServers(() => ({ urls: [], secret }), clock);
  assertDeepEq(await none.forClient(null, "ana", 3600000), []);
  const some = iceServers(() => ({ urls: ["turn:turn.example.org:3478?transport=udp"], secret }), clock);
  assertDeepEq(await some.forClient(null, "ana", 3600000), [
    { urls: ["turn:turn.example.org:3478?transport=udp"], username: "1700003600:ana", credential: "+5Wu0RaejH8eFX2gvpvBno8V880=" },
  ]);
});
