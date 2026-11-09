/**
 * @module k:/tests/tabs.test.js
 *
 * `k.device.BrowserTabMessaging` (docs/design/45-web-platform.md, "Messages between tabs";
 * docs/compiler/runtime.md §12.10) against a scripted `BroadcastChannel` written from the HTML
 * standard: a message posted on one channel object reaches every other open object of the same
 * name in the same origin and profile, never the one that posted it, as a structured clone, later; a closed object hears nothing. Each "tab" here is a `BrowserTabMessaging` of its own,
 * so two of them stand for two tabs.
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { BrowserTabMessaging } from "k:/runtime/tabs.js";
import { codecs, flush, root } from "k:runtime";

/**
 * Installs a scripted `BroadcastChannel`; returns the record of every object opened (`name`,
 * `closed`) and every message posted (`name`, `data`).
 */
function fakeBroadcast() {
  const open = new Set();
  const record = { channels: [], posted: [] };
  globalThis.BroadcastChannel = class {
    constructor(name) {
      this.name = String(name);
      this.onmessage = null;
      this.closed = false;
      open.add(this);
      record.channels.push(this);
    }
    postMessage(data) {
      if (this.closed) throw Object.assign(new Error("closed"), { name: "InvalidStateError" });
      const copy = JSON.parse(JSON.stringify(data));
      record.posted.push({ name: this.name, data: copy });
      for (const other of open) {
        if (other !== this && other.name === this.name) {
          // Later, as the standard queues a task (a microtask here, so `tick` drains it).
          queueMicrotask(() => other.onmessage?.({ data: JSON.parse(JSON.stringify(copy)) }));
        }
      }
    }
    close() {
      this.closed = true;
      open.delete(this);
    }
  };
  return record;
}

/** A codec of `{id}` objects that rejects anything without a numeric `id`, like a generated one. */
const item = {
  encode: (v) => ({ id: v.id }),
  decode: (j) => {
    if (typeof j?.id !== "number") throw new Error("not an item");
    return { id: j.id };
  },
};
const cart = { name: "cart" };

test("a message posted in one tab reaches the other tab's listener, not its own", async () => {
  fakeBroadcast();
  const a = BrowserTabMessaging("shop", "/assets/app.1.js");
  const b = BrowserTabMessaging("shop", "/assets/app.1.js");
  const heardA = [];
  const heardB = [];
  a.listen(item, cart, (v) => heardA.push(v));
  b.listen(item, cart, (v) => heardB.push(v));
  a.post(item, cart, { id: 7 });
  await tick();
  assertDeepEq(heardB, [{ id: 7 }]);
  assertDeepEq(heardA, [], "never the tab that posted it");
});

test("channels are namespaced per app and named per channel", async () => {
  const record = fakeBroadcast();
  const shop = BrowserTabMessaging("shop", "b1");
  const notes = BrowserTabMessaging("notes", "b1");
  const heard = [];
  notes.listen(item, cart, (v) => heard.push(v));
  shop.listen(item, { name: "other" }, (v) => heard.push(v));
  shop.post(item, cart, { id: 1 });
  await tick();
  assertDeepEq(heard, [], "another app's cart and another channel hear nothing");
  assertEq(record.posted[0].name, "shop:cart");
  assertEq(record.channels.filter((c) => c.name === "shop:cart").length, 1, "one object per channel and tab");
});

test("a tab running another build drops the message", async () => {
  fakeBroadcast();
  const old = BrowserTabMessaging("shop", "/assets/app.1.js");
  const next = BrowserTabMessaging("shop", "/assets/app.2.js");
  const heard = [];
  next.listen(item, cart, (v) => heard.push(v));
  old.post(item, cart, { id: 1 });
  await tick();
  assertDeepEq(heard, []);
});

test("a message crosses as its codec's JSON and a message that doesn't decode is dropped", async () => {
  const record = fakeBroadcast();
  const a = BrowserTabMessaging("shop", "b");
  const b = BrowserTabMessaging("shop", "b");
  const heard = [];
  b.listen(item, cart, (v) => heard.push(v));
  a.post(item, cart, { id: 3, extra: () => 0 });
  a.post(codecs.int, cart, 5);
  await tick();
  assertDeepEq(record.posted.map((p) => JSON.parse(p.data.m)), [{ id: 3 }, 5], "the JSON text of the codec's form");
  assertDeepEq(heard, [{ id: 3 }], "the Int doesn't decode as an item");
});

test("cancelling a subscription stops its handler, and cancelling twice does nothing", async () => {
  fakeBroadcast();
  const a = BrowserTabMessaging("shop", "b");
  const b = BrowserTabMessaging("shop", "b");
  const heard = [];
  const sub = b.listen(item, cart, (v) => heard.push(v.id));
  const other = b.listen(item, cart, (v) => heard.push(-v.id));
  a.post(item, cart, { id: 1 });
  await tick();
  sub.cancel();
  sub.cancel();
  a.post(item, cart, { id: 2 });
  await tick();
  assertDeepEq(heard, [1, -1, -2]);
  other.cancel();
});

test("a handler's writes run as one batch", async () => {
  fakeBroadcast();
  const a = BrowserTabMessaging("shop", "b");
  const b = BrowserTabMessaging("shop", "b");
  let flushed = 0;
  b.listen(item, cart, () => {
    flush();
    flushed++;
  });
  a.post(item, cart, { id: 1 });
  await tick();
  assertEq(flushed, 1);
});

test("messages streams the other tabs' messages and ends with its owner", async () => {
  fakeBroadcast();
  const a = BrowserTabMessaging("shop", "b");
  const b = BrowserTabMessaging("shop", "b");
  let stream = null;
  const dispose = root((d) => {
    stream = b.messages(item, cart);
    return d;
  });
  a.post(item, cart, { id: 1 });
  a.post(item, cart, { id: 2 });
  await tick();
  assertDeepEq(await stream.next(), { id: 1 });
  assertDeepEq(await stream.next(), { id: 2 });
  dispose();
  a.post(item, cart, { id: 3 });
  await tick();
  assertEq(await stream.next(), null, "ended with its owner; later messages aren't queued");
});

test("leaving a for await early stops the stream", async () => {
  fakeBroadcast();
  const a = BrowserTabMessaging("shop", "b");
  const b = BrowserTabMessaging("shop", "b");
  const stream = b.messages(item, cart);
  a.post(item, cart, { id: 1 });
  await tick();
  for await (const v of stream) {
    assertEq(v.id, 1);
    break;
  }
  a.post(item, cart, { id: 2 });
  await tick();
  assertEq(await stream.next(), null);
});

test("without BroadcastChannel nothing is posted or heard, and nothing throws", async () => {
  delete globalThis.BroadcastChannel;
  const a = BrowserTabMessaging("shop", "b");
  const heard = [];
  const sub = a.listen(item, cart, (v) => heard.push(v));
  a.post(item, cart, { id: 1 });
  await tick();
  sub.cancel();
  assert(heard.length === 0);
});
