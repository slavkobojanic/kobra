/**
 * @module k:testing/channels
 *
 * Channels in `k test` (docs/design/44-channels.md, "Testing"; docs/compiler/runtime.md §13):
 *
 * - **The in-process front** ({@link createFront}) stands where the Rust front
 *   (`k_server::sockets`) stands in production: the browser's channel client
 *   (`runtime/channels.js`) talks to it through a fake WebSocket, in the real `k.channel.v1`
 *   JSON frames, and it hands each open, message and close to the app's handler as the Rust
 *   front does (`request.channel`), answering with `opened`, `refused`, `ack` and `close`. It
 *   numbers and buffers the server's messages, confirms receipts, keeps a dropped socket's
 *   channels for the resume window, repeats remembered acknowledgements, applies the rate limit
 *   and sends heartbeats, all on the test's `TestClock`. The app's `op_k_channel_send` and
 *   `op_k_channel_close` reach it instead of the Rust front.
 * - **`TestChannel<C, S, E>`** ({@link testChannel}): a scripted channel with no server.
 *
 * Each browser of a test (the test's own and each `TestPeer`) is `{ctx, mirror, offline}`:
 * `ctx` makes its requests (with its session cookie), `mirror` is its `Session` mirror, and
 * `offline` whether it has no network.
 */

import { decodeChannelFrame, encodeChannelFrame } from "../runtime/channels.js";
import { flush, signal } from "../runtime/signals.js";
import { Instant, PushStream } from "../runtime/std.js";
import { KPanic, unitCase } from "../runtime/values.js";

/** The resume window, the heartbeat, and the outgoing buffer's bound (44-channels.md, "Limits"). */
const WINDOW = 30_000;
const BEAT = 25_000;
const BUFFER = 1000;
/** Messages a second per channel, and the burst. */
const RATE = 20;
const BURST = 50;
/** Acknowledgements remembered per channel, to repeat for a re-sent message. */
const ACKS = 64;

/** The UTF-8 bytes of `text`. */
const utf8 = (text) => new TextEncoder().encode(text);

/**
 * The id of the next server channel, across tests: an id is never reused, so a channel the app
 * still holds from an earlier test can't be mistaken for a new one.
 */
let nextId = 1;

/** The JSON body of an app response. */
function answer(r) {
  const text = typeof r?.body === "string" ? r.body : new TextDecoder().decode(r?.body ?? new Uint8Array(0));
  try {
    return JSON.parse(text);
  } catch {
    return { error: { code: "internal", message: "internal error" } };
  }
}

/**
 * The in-process channels front of one test.
 * @param {{now: {ms: number}, after: Function, every: Function}} clock the test's `TestClock`
 * @param {(p: Promise<unknown>) => Promise<unknown>} track marks server work in flight
 */
export function createFront(clock, track) {
  /** Server channels by id, and by resume token. */
  const chans = new Map();
  const tokens = new Map();
  /** Every socket still connected. */
  const sockets = new Set();
  let nextToken = 1;

  /** Sends frame `f` to socket `s`, after what was sent before it. */
  const put = (s, f) => {
    if (!s.live) return;
    front.activity++;
    const text = encodeChannelFrame(f);
    Promise.resolve().then(() => s.live && s.onmessage?.({ data: text }));
  };

  /** Hands a channel event to the app's handler, as the Rust front does. */
  const event = (sc, kind, fields, body) => {
    const req = sc.browser.ctx.request("POST", sc.method, [], body ?? new Uint8Array(0));
    req.channel = { kind, id: sc.id, seq: fields.seq ?? 0, reason: fields.reason ?? "" };
    return track(Promise.resolve(sc.browser.ctx.app(req))).then(answer);
  };

  /** Ends server channel `sc` in the app with `reason` and forgets it. */
  const end = (sc, reason) => {
    if (sc.ended) return;
    forget(sc);
    event(sc, "close", { reason });
  };

  /** Forgets server channel `sc`: its token, its socket number and its window. */
  const forget = (sc) => {
    sc.ended = true;
    sc.window?.cancel();
    chans.delete(sc.id);
    tokens.delete(sc.token);
    if (sc.sock && sc.sock.chans.get(sc.ch) === sc) sc.sock.chans.delete(sc.ch);
    sc.sock = null;
  };

  /**
   * Socket `s` is gone: its channels wait for a resume for the window, or end now when it isn't
   * `resumable`. With `code`, the browser hears the close.
   */
  const drop = (s, resumable, code) => {
    if (!s.live) return;
    s.live = false;
    sockets.delete(s);
    s.beat.cancel();
    for (const sc of s.chans.values()) {
      sc.sock = null;
      if (!resumable) end(sc, "byClient");
      else sc.window = clock.after(WINDOW, () => end(sc, "byClient"));
    }
    s.chans.clear();
    if (code) Promise.resolve().then(() => s.onclose?.({ code, reason: "" }));
  };

  /** Runs the queued messages of `sc`, one at a time, in order. */
  const pump = async (sc) => {
    if (sc.busy) return;
    sc.busy = true;
    while (sc.queue.length && !sc.ended) {
      const { seq, value } = sc.queue.shift();
      let ack;
      if (!take(sc)) ack = { kind: "ack", ch: 0, seq, error: { code: "resource_exhausted", message: "too many messages", retryAfter: 1 } };
      else {
        const out = await event(sc, "msg", { seq }, utf8(JSON.stringify(value)));
        ack = out.error === undefined ? { kind: "ack", ch: 0, seq } : { kind: "ack", ch: 0, seq, error: out.error };
      }
      sc.handled = seq;
      sc.acks.set(seq, ack);
      if (sc.acks.size > ACKS) sc.acks.delete(sc.acks.keys().next().value);
      if (sc.sock) put(sc.sock, { ...ack, ch: sc.ch });
    }
    sc.busy = false;
  };

  /** Takes a token of `sc`'s rate limit; `false` when it has none. */
  const take = (sc) => {
    const now = clock.now.ms;
    sc.bucket = Math.min(BURST, sc.bucket + ((now - sc.filled) / 1000) * RATE);
    sc.filled = now;
    if (sc.bucket < 1) return false;
    sc.bucket -= 1;
    return true;
  };

  /** Handles frame `f` from the browser on socket `s`. */
  const frame = async (s, f) => {
    front.activity++;
    const sc = f.ch ? s.chans.get(f.ch) : undefined;
    switch (f.kind) {
      case "open": {
        if (sc) return drop(s, true, 1002);
        const c = {
          id: nextId++, browser: s.browser, cookie: s.browser.ctx.cookie, method: f.method, request: f.request,
          sock: s, ch: f.ch, token: null, opened: false, seq: 0, out: [], handled: 0, acks: new Map(),
          queue: [], busy: false, ended: false, window: null, bucket: BURST, filled: clock.now.ms,
        };
        chans.set(c.id, c);
        s.chans.set(f.ch, c);
        const out = await event(c, "open", {}, utf8(JSON.stringify(f.request ?? {})));
        if (out.error !== undefined || c.ended) {
          forget(c);
          if (out.error !== undefined && s.live) put(s, { kind: "refused", ch: f.ch, error: out.error });
          return;
        }
        c.token = `test.${nextToken++}`;
        tokens.set(c.token, c);
        c.opened = true;
        if (c.sock) {
          put(c.sock, { kind: "opened", ch: c.ch, resume: c.token });
          for (const m of c.out) put(c.sock, { kind: "msg", ch: c.ch, seq: m.seq, value: m.value });
        }
        return;
      }
      case "resume": {
        const c = tokens.get(f.token);
        if (!c || c.ended || c.cookie !== s.browser.ctx.cookie) return put(s, { kind: "refused", ch: f.ch, error: { code: "not_found", message: "unknown resume token" } });
        c.window?.cancel();
        c.window = null;
        if (c.sock && c.sock.chans.get(c.ch) === c) c.sock.chans.delete(c.ch);
        c.sock = s;
        c.ch = f.ch;
        s.chans.set(f.ch, c);
        c.out = c.out.filter((m) => m.seq > f.received);
        put(s, { kind: "opened", ch: f.ch, resume: c.token });
        for (const m of c.out) put(s, { kind: "msg", ch: f.ch, seq: m.seq, value: m.value });
        return;
      }
      case "msg": {
        if (!sc) return;
        if (f.seq <= sc.handled) {
          const ack = sc.acks.get(f.seq);
          if (ack) put(s, { ...ack, ch: f.ch });
          return;
        }
        if (sc.queue.some((m) => m.seq === f.seq)) return;
        sc.queue.push({ seq: f.seq, value: f.value });
        return pump(sc);
      }
      case "rcvd":
        if (sc) sc.out = sc.out.filter((m) => m.seq > f.seq);
        return;
      case "close":
        if (sc) end(sc, "byClient");
        return;
      default:
        return;
    }
  };

  const front = {
    /**
     * Counts the frames either way, so `Screen.settle()` keeps waiting while
     * channel traffic moves: a broadcast's next element arrives only after a
     * turn of the event loop, and what it sends after another.
     */
    activity: 0,
    /**
     * A new socket of `browser`, as `channelEnv`'s `socket`: it opens after the current task,
     * or fails as without a network while the browser is offline.
     */
    connect(browser) {
      const s = {
        browser, live: true, chans: new Map(), onopen: null, onmessage: null, onclose: null,
        beat: { cancel() {} },
        send(text) {
          if (!s.live) return;
          let f;
          try {
            f = decodeChannelFrame(text);
          } catch {
            return drop(s, true, 1002);
          }
          Promise.resolve().then(() => s.live && frame(s, f));
        },
        close() {
          // After the frames sent before it, as a socket sends them first.
          Promise.resolve().then(() => drop(s, true, 0));
        },
      };
      Promise.resolve().then(() => {
        if (!s.live) return;
        if (browser.offline) {
          s.live = false;
          return s.onclose?.({ code: 1006, reason: "" });
        }
        sockets.add(s);
        // Heartbeats arrive at once: virtual time can pass many of them in one `advance`.
        s.beat = clock.every(BEAT, () => s.live && s.onmessage?.({ data: encodeChannelFrame({ kind: "beat" }) }));
        s.onopen?.();
      });
      return s;
    },
    /** Drops every socket of `browser` (`TestChannels.drop`, going offline). */
    drop(browser, resumable) {
      for (const s of [...sockets]) if (s.browser === browser) drop(s, resumable, 1006);
    },
    /** The channels `browser` has open: `{method, arguments}`, oldest first. */
    open(browser) {
      return [...chans.values()]
        .filter((c) => c.browser === browser && c.opened && !c.ended)
        .map((c) => ({ method: c.method, arguments: JSON.stringify(c.request ?? {}) }));
    },
    /** `op_k_channel_send`: the server sends a message; `false` once the channel is gone or overflowed. */
    send(id, json) {
      const c = chans.get(id);
      if (!c || c.ended) return false;
      const m = { seq: ++c.seq, value: JSON.parse(json) };
      c.out.push(m);
      if (c.out.length > BUFFER) {
        // The overflow drops the socket; this channel restarts.
        const s = c.sock;
        forget(c);
        if (s) drop(s, true, 1013);
        return false;
      }
      if (c.opened && c.sock) put(c.sock, { kind: "msg", ch: c.ch, seq: m.seq, value: m.value });
      return true;
    },
    /** `op_k_channel_close`: the server closed channel `id` for `reason`. */
    close(id, reason) {
      const c = chans.get(id);
      if (!c) return;
      const s = c.sock;
      const ch = c.ch;
      forget(c);
      if (!s) return;
      put(s, { kind: "close", ch, reason });
      if (reason === "sessionEnded" && !s.ended) {
        s.ended = true;
        put(s, { kind: "session", user: null });
      }
    },
    /** Forgets everything at the end of a test, telling nobody. */
    reset() {
      for (const s of sockets) {
        s.live = false;
        s.beat.cancel();
      }
      sockets.clear();
      for (const c of chans.values()) c.window?.cancel();
      chans.clear();
      tokens.clear();
    },
  };
  return front;
}

// ------------------------------------------------------------ TestChannel

/**
 * `k.test.TestChannel<C, S, E>`: a scripted channel (44-channels.md, "TestChannel"). `channel`
 * is a `Channel` as the browser's: `state`, `messages`, `send($c, m)`, `close()`. The test plays
 * the server with `deliver`, `refuseNext`, `drop` and `close`; `sent` holds what was accepted.
 * @param {object|null} E the refusal enum's constructor object (for `rpcFailure(.offline)`)
 * @param {{now: {ms: number}, after: Function}} clock the test's `TestClock`
 */
export function testChannel(E, clock) {
  const offline = () => (E?.rpcFailure ? E.rpcFailure({ $: 2, _0: null }) : new KPanic("the channel is offline"));
  const st = signal(unitCase(1));
  const messages = new PushStream();
  const sent = [];
  /** Sends waiting for the connection, and messages delivered while dropped. */
  let waiting = [];
  let held = [];
  let refusal = null;
  let down = null;
  let done = false;
  const accept = (m) => {
    if (refusal !== null) {
      const e = refusal;
      refusal = null;
      throw e;
    }
    sent.push(m);
  };
  const finish = (reason) => {
    if (done) return;
    done = true;
    down?.cancel();
    st.set({ $: 3, _0: reason });
    messages.push(null);
    for (const w of waiting.splice(0)) w.no(offline());
    flush();
  };
  const channel = {
    get state() {
      return st.get();
    },
    messages,
    send(_c, m) {
      if (done) return Promise.reject(offline());
      if (down === null) {
        try {
          accept(m);
        } catch (e) {
          return Promise.reject(e);
        }
        flush();
        return Promise.resolve();
      }
      return new Promise((ok, no) => {
        const w = { m, ok, no, timer: clock.after(10_000, () => (waiting = waiting.filter((x) => x !== w), no(offline()))) };
        waiting.push(w);
      });
    },
    close() {
      finish(unitCase(0));
    },
  };
  messages.close = () => channel.close();
  return {
    channel,
    get sent() {
      return [...sent];
    },
    deliver(m) {
      if (done) return;
      if (down !== null) held?.push(m);
      else messages.push(m);
      flush();
    },
    refuseNext(e) {
      refusal = e;
    },
    drop(resumable = true) {
      if (done || down !== null) return;
      held = resumable === false ? null : [];
      st.set({ $: 2, attempt: 1, at: new Instant(clock.now.ms + 500) });
      down = clock.after(500, () => {
        down = null;
        st.set(unitCase(1));
        for (const m of held ?? []) messages.push(m);
        held = [];
        for (const w of waiting.splice(0)) {
          w.timer.cancel();
          try {
            accept(w.m);
            w.ok();
          } catch (e) {
            w.no(e);
          }
        }
      });
      flush();
    },
    close(reason) {
      finish(reason);
    },
  };
}
