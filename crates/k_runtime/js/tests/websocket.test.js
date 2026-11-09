/**
 * @module k:/tests/websocket.test.js
 *
 * `k.http`'s raw WebSockets (docs/design/44-channels.md, "Raw WebSockets:
 * `k.http`"): the browser's `StandardWebSocketClient` over a fake
 * `WebSocket`, the server's over a stand-in for the `op_k_ws_*` ops, the
 * `TestWebSocketClient` and `TestWebSocketServer` doubles, `Backoff` and
 * `WebSocketMessage.json()`. The ops themselves run against a local echo
 * server in Rust (`crates/k_server/tests/websocket_client.rs`).
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { Backoff, KPanic, backoffDelay, codecs, messageJson, unitCase } from "k:runtime";
import { webSocketClient } from "k:runtime/server";
import { StandardWebSocketClient, TestWebSocketServer, testWebSocketClient } from "k:/runtime/websocket.js";

// The test isolate has no timers: the handshake's timeout and `send`'s wait
// run when a test fires them.
const timers = [];
if (typeof globalThis.setTimeout !== "function") {
  globalThis.setTimeout = (f) => timers.push(f);
  globalThis.clearTimeout = (id) => {
    timers[id - 1] = null;
  };
}
/** Runs every timer set so far. */
const fireTimers = () => {
  for (const f of timers.splice(0)) f?.();
};

const E = {
  offline: unitCase(0),
  timeout: unitCase(1),
  refused: (status) => ({ $: 2, status }),
  tooLarge: unitCase(3),
  closed: unitCase(4),
};
const M = { text: (_0) => ({ $: 0, _0 }), binary: (_0) => ({ $: 1, _0 }) };
const St = { open: unitCase(0), closing: unitCase(1), closed: (code, reason) => ({ $: 2, code, reason }) };
const S = { ok: unitCase(0), forbidden: unitCase(1), other: (_0) => ({ $: 2, _0 }) };
const D = { malformed: (message) => ({ $: 0, message }), mismatch: (path, message) => ({ $: 1, path, message }) };

/** A request with the defaults the generated struct has. */
const request = (url, more = {}) => ({ url, subprotocol: null, headers: new Map(), secretHeaders: new Map(), bearerToken: null, timeout: 30000, maxMessage: 1000000, ...more });

/** A fake browser `WebSocket`: the test plays the endpoint with `open`, `receive` and `drop`. */
function fakeBrowser() {
  const made = [];
  const make = (url, protocol) => {
    const ws = {
      url,
      protocol: "",
      asked: protocol,
      bufferedAmount: 0,
      sent: [],
      closedWith: null,
      send(d) {
        this.sent.push(d);
      },
      close(code, reason) {
        if (code !== undefined && code !== 1000 && (code < 3000 || code > 4999)) throw new Error("InvalidAccessError");
        this.closedWith = [code ?? null, reason ?? null];
      },
      open(p = "") {
        this.protocol = p;
        this.onopen?.();
      },
      receive(data) {
        this.onmessage?.({ data });
      },
      drop(code = 1006, reason = "") {
        this.onclose?.({ code, reason });
      },
    };
    made.push(ws);
    return ws;
  };
  return { made, make };
}

test("the browser's client opens a socket, sends, receives text and bytes, and closes", async () => {
  const b = fakeBrowser();
  const client = StandardWebSocketClient(E, M, St, b.make);
  const pending = client.connect(null, request("wss://feed.example/v1", { subprotocol: "feed.v2" }));
  const ws = b.made[0];
  assertEq(ws.url, "wss://feed.example/v1");
  assertEq(ws.asked, "feed.v2");
  assertEq(ws.binaryType, "arraybuffer");
  ws.open("feed.v2");
  const socket = await pending;
  assertEq(socket.subprotocol, "feed.v2");
  assertDeepEq(socket.state, St.open);
  await socket.send(null, M.text("hi"));
  await socket.send(null, M.binary(new Uint8Array([1, 2])));
  assertEq(ws.sent[0], "hi");
  assertDeepEq([...ws.sent[1]], [1, 2]);
  ws.receive("tick");
  ws.receive(new Uint8Array([9]).buffer);
  assertDeepEq(await socket.messages.next(), M.text("tick"));
  const bin = await socket.messages.next();
  assertEq(bin.$, 1);
  assertDeepEq([...bin._0], [9]);
  socket.close(4000, "bye");
  assertDeepEq(socket.state, St.closing);
  assertDeepEq(ws.closedWith, [4000, "bye"]);
  socket.close(4000, "again");
  ws.drop(4000, "bye");
  assertDeepEq(socket.state, St.closed(4000, "bye"));
  assertEq(await socket.messages.next(), null, "messages end when the socket closes");
  let thrown = null;
  try {
    await socket.send(null, M.text("late"));
  } catch (e) {
    thrown = e;
  }
  assertEq(thrown, E.closed);
});

test("a handshake the browser refuses is offline, and a slow one a timeout", async () => {
  const b = fakeBrowser();
  const client = StandardWebSocketClient(E, M, St, b.make);
  const refused = client.connect(null, request("wss://feed.example/v1"));
  b.made[0].drop(1006);
  let thrown = null;
  try {
    await refused;
  } catch (e) {
    thrown = e;
  }
  assertEq(thrown, E.offline);
  const slow = client.connect(null, request("wss://feed.example/v1", { timeout: 5 }));
  fireTimers();
  thrown = null;
  try {
    await slow;
  } catch (e) {
    thrown = e;
  }
  assertEq(thrown, E.timeout);
  assert(b.made[1].closedWith !== null, "the slow socket was closed");
});

test("an incoming message over maxMessage closes the socket, and send throws tooLarge", async () => {
  const b = fakeBrowser();
  const client = StandardWebSocketClient(E, M, St, b.make);
  const pending = client.connect(null, request("wss://feed.example/v1", { maxMessage: 4 }));
  b.made[0].open();
  const socket = await pending;
  b.made[0].receive("abcd");
  b.made[0].receive("abcde");
  assertDeepEq(await socket.messages.next(), M.text("abcd"));
  assertEq(await socket.messages.next(), null);
  assertDeepEq(socket.state, St.closed(1009, "too large"));
  assert(b.made[0].closedWith !== null, "the browser's socket was closed (with a code it allows)");
  let thrown = null;
  try {
    await socket.send(null, M.text("x"));
  } catch (e) {
    thrown = e;
  }
  assertEq(thrown, E.tooLarge);
});

test("a cancelled owner closes its socket, and a cancelled connect never settles", async () => {
  const b = fakeBrowser();
  const client = StandardWebSocketClient(E, M, St, b.make);
  const owner = new AbortController();
  const pending = client.connect(owner.signal, request("wss://feed.example/v1"));
  b.made[0].open();
  const socket = await pending;
  owner.abort();
  assertDeepEq(socket.state, St.closing);
  assertDeepEq(b.made[0].closedWith, [1000, ""]);
  const second = new AbortController();
  let settled = false;
  client.connect(second.signal, request("wss://feed.example/v1")).then(() => (settled = true), () => (settled = true));
  second.abort();
  b.made[1].drop();
  await tick();
  assertEq(settled, false);
});

test("the server's client maps the ops' outcomes and pumps messages until the close", async () => {
  const log = [];
  // The endpoint speaks once the test has sent.
  let speak;
  const spoken = new Promise((r) => (speak = r));
  const incoming = [{ kind: "text", text: "a" }, { kind: "binary", bytes: new Uint8Array([5]) }, { kind: "closed", code: 1001, reason: "away" }];
  const transport = {
    connect: async (id, w) => {
      log.push(["connect", w]);
      if (w.url.endsWith("/refuse")) return { kind: "refused", status: 403 };
      if (w.url.endsWith("/slow")) return { kind: "timeout" };
      if (w.url.endsWith("/down")) return { kind: "offline", message: "refused" };
      return { kind: "open", protocol: "feed.v2" };
    },
    next: async () => {
      await spoken;
      return incoming.shift();
    },
    send: async (id, text, bytes, binary) => {
      log.push(["send", text, [...bytes], binary]);
      return true;
    },
    close: (id, code, reason) => log.push(["close", code, reason]),
  };
  const client = webSocketClient(E, M, St, S, [200, 403, null], transport);
  const socket = await client.connect(null, request("wss://feed.example/v1", { headers: new Map([["X-Feed", "1"]]), timeout: 5000, maxMessage: 64 }));
  assertDeepEq(log[0], ["connect", { url: "wss://feed.example/v1", protocol: null, headers: [["x-feed", "1"]], timeoutMs: 5000, maxMessage: 64 }]);
  assertEq(socket.subprotocol, "feed.v2");
  await socket.send(null, M.text("hi"));
  await socket.send(null, M.binary(new Uint8Array([7])));
  assertDeepEq(log[1], ["send", "hi", [], false]);
  assertDeepEq(log[2], ["send", "", [7], true]);
  speak();
  const seen = [];
  for await (const m of socket.messages) seen.push(m.$ === 0 ? m._0 : [...m._0]);
  assertDeepEq(seen, ["a", [5]]);
  assertDeepEq(socket.state, St.closed(1001, "away"));
  const failures = [];
  for (const path of ["/refuse", "/slow", "/down"]) {
    try {
      await client.connect(null, request(`wss://feed.example${path}`));
    } catch (e) {
      failures.push(e);
    }
  }
  assertDeepEq(failures, [E.refused(S.forbidden), E.timeout, E.offline]);
});

test("TestWebSocketClient connects only to scripted endpoints, shared by every connection", async () => {
  const sockets = testWebSocketClient(E, M, St);
  const feed = TestWebSocketServer();
  sockets.serve("wss://feed.example/v1", feed);
  const a = await sockets.connect(null, request("wss://feed.example/v1?symbol=BTC", { subprotocol: "feed.v2" }));
  const b = await sockets.connect(null, request("wss://feed.example/v1"));
  assertEq(feed.connections, 2);
  assertEq(a.subprotocol, "feed.v2");
  await a.send(null, M.text("one"));
  await b.send(null, M.text("two"));
  assertDeepEq(feed.received, [M.text("one"), M.text("two")]);
  feed.send(M.text("tick"));
  assertDeepEq(await a.messages.next(), M.text("tick"));
  assertDeepEq(await b.messages.next(), M.text("tick"));
  a.close(1000, "");
  await tick();
  assertDeepEq(a.state, St.closed(1000, ""));
  assertEq(feed.connections, 1);
  feed.close(1011, "restarting");
  assertDeepEq(b.state, St.closed(1011, "restarting"));
  assertEq(sockets.requests.length, 2);
  sockets.fail("wss://feed.example/v2", E.refused(S.forbidden));
  let thrown = null;
  try {
    await sockets.connect(null, request("wss://feed.example/v2"));
  } catch (e) {
    thrown = e;
  }
  assertDeepEq(thrown, E.refused(S.forbidden));
  thrown = null;
  try {
    await sockets.connect(null, request("wss://elsewhere.example/"));
  } catch (e) {
    thrown = e;
  }
  assert(thrown instanceof KPanic && thrown.message.includes("wss://elsewhere.example/"), String(thrown?.message));
});

test("Backoff draws below each attempt's cap, up to 30 s, and starts again after reset", () => {
  assertEq(backoffDelay(1, 0.999), 499);
  assertEq(backoffDelay(2, 0.5), 500);
  assertEq(backoffDelay(7, 0.999), 29970);
  assertEq(backoffDelay(50, 0.999), 29970, "the last cap repeats");
  const random = Math.random;
  try {
    Math.random = () => 0.999;
    const b = new Backoff();
    const delays = [];
    for (let i = 0; i < 8; i++) delays.push(b.next());
    assertDeepEq(delays, [499, 999, 1998, 3996, 7992, 15984, 29970, 29970]);
    b.reset();
    assertEq(b.next(), 499);
  } finally {
    Math.random = random;
  }
});

test("WebSocketMessage.json() decodes text and UTF-8 bytes, or throws DecodingError", () => {
  const codec = codecs.int;
  assertEq(messageJson(M.text("42"), codec, D), 42);
  assertEq(messageJson(M.binary(new TextEncoder().encode("7")), codec, D), 7);
  let thrown = null;
  try {
    messageJson(M.text("nope"), codec, D);
  } catch (e) {
    thrown = e;
  }
  assertEq(thrown.$, 0);
  thrown = null;
  try {
    messageJson(M.text('"text"'), codec, D);
  } catch (e) {
    thrown = e;
  }
  assertEq(thrown.$, 1);
});
