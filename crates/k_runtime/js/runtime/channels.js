/**
 * @module k:/runtime/channels.js
 *
 * Channels in the browser (docs/design/44-channels.md, "The client"; docs/compiler/runtime.md
 * §8.2): the frames of `k.channel.v1`, the channels' WebSocket protocol, and the client every
 * channel rpc's stub opens a `Channel` with.
 *
 * **Frames** are pure encode and decode, the same codec as the Rust front's (`k_server::channels`),
 * both tested against `tests/channel-frames.json`. A frame is `{ kind, ch, … }` with the kind's
 * fields: `open` (`method`, `request`), `opened` (`resume`), `refused` (`error`), `msg` (`seq`,
 * `value`), `ack` (`seq`, `error` when refused), `rcvd` (`seq`), `resume` (`token`, `received`,
 * `sent`), `close` (`reason`), `session` (`user`, `null` for a guest; no `ch`) and `beat` (no
 * fields). Payloads are already-encoded messages of the app's protocols: JSON values in JSON
 * frames (development) and `Uint8Array`s in binary frames (production, the Protobuf message
 * `k.channel.v1.Frame`, documented in the Rust module). Decoding throws an `Error` for a
 * malformed frame.
 *
 * **The client** keeps one socket per page, `/_k/channels`, for every open channel, and owns
 * reconnecting: backoff with full jitter, resuming with each channel's token, restarting when
 * the server forgot it, heartbeats, receipts and the back-forward cache. Shipped as its own
 * client chunk, only to apps whose client calls a channel rpc (codegen.md §16), and measured
 * against its own size budget.
 */

import { sessionMirror, takeMirror } from "./rpc.js";
import { effect, root, signal, untrack } from "./signals.js";
import { Instant, PushStream } from "./std.js";
import { unitCase } from "./values.js";

/** The kinds, in Protobuf field-number order (from 1). */
const CHANNEL_KINDS = ["open", "opened", "refused", "msg", "ack", "rcvd", "resume", "close", "session", "beat"];

/** The close reasons, in Protobuf enum-number order (from 1). */
const CHANNEL_REASONS = ["byClient", "byServer", "sessionEnded", "refused"];

/**
 * The fields of each kind with frames: `c` a channel number, `n` a whole number, `s` a string,
 * `p` a payload, `o` an optional payload, `r` a close reason; in JSON key order and with their
 * Protobuf field numbers (the position plus one).
 */
const CHANNEL_FIELDS = {
  open: [["ch", "c"], ["method", "s"], ["request", "p"]],
  opened: [["ch", "c"], ["resume", "s"]],
  refused: [["ch", "c"], ["error", "p"]],
  msg: [["ch", "c"], ["seq", "n"], ["value", "p"]],
  ack: [["ch", "c"], ["seq", "n"], ["error", "o"]],
  rcvd: [["ch", "c"], ["seq", "n"]],
  resume: [["ch", "c"], ["token", "s"], ["received", "n"], ["sent", "n"]],
  close: [["ch", "c"], ["reason", "r"]],
  session: [["user", "o"]],
  beat: [],
};

/**
 * Throws the decoding error `message`.
 * @param {string} message
 * @returns {never}
 */
function badFrame(message) {
  throw new Error(`invalid channel frame: ${message}`);
}

/**
 * Checks a decoded value of type `t` (`c`, `n`, `s` or `r`) for field `key` of `kind`.
 * @param {string} kind
 * @param {string} key
 * @param {string} t
 * @param {unknown} v
 * @returns {unknown}
 */
function channelScalar(kind, key, t, v) {
  if (t === "s" ? typeof v === "string" : t === "r" ? CHANNEL_REASONS.includes(/** @type {string} */ (v)) : Number.isSafeInteger(v) && /** @type {number} */ (v) >= (t === "c" ? 1 : 0) && (t !== "c" || /** @type {number} */ (v) <= 0xffffffff)) return v;
  return badFrame(`\`${kind}.${key}\` is ${t === "s" ? "a string" : t === "r" ? "a close reason" : "a whole number"}`);
}

/**
 * Encodes a JSON text frame.
 * @param {{kind: string} & Record<string, any>} frame
 * @returns {string}
 */
export function encodeChannelFrame(frame) {
  /** @type {Record<string, unknown>} */
  const body = {};
  for (const [key, t] of CHANNEL_FIELDS[frame.kind]) {
    const v = frame[key];
    if (t === "o") {
      if (v != null) body[key] = v;
      else if (key === "user") body[key] = null;
    } else body[key] = v;
  }
  return JSON.stringify({ [frame.kind]: body });
}

/**
 * Decodes a JSON text frame.
 * @param {string} text
 * @returns {{kind: string} & Record<string, any>}
 */
export function decodeChannelFrame(text) {
  let outer;
  try {
    outer = JSON.parse(text);
  } catch (e) {
    return badFrame(String(e));
  }
  if (outer === null || typeof outer !== "object" || Array.isArray(outer)) return badFrame("a frame is a JSON object");
  const keys = Object.keys(outer);
  if (keys.length !== 1) return badFrame("a frame has exactly one kind");
  const kind = keys[0];
  const fields = Object.hasOwn(CHANNEL_FIELDS, kind) ? CHANNEL_FIELDS[kind] : badFrame(`\`${kind}\` isn't a kind of frame`);
  const body = outer[kind];
  if (body === null || typeof body !== "object" || Array.isArray(body)) return badFrame(`\`${kind}\` is a JSON object`);
  /** @type {{kind: string} & Record<string, any>} */
  const frame = { kind };
  for (const [key, t] of fields) {
    const v = body[key];
    if (t === "o") frame[key] = v ?? null;
    else if (t === "p") frame[key] = v === undefined ? badFrame(`\`${kind}.${key}\` is missing`) : v;
    else frame[key] = channelScalar(kind, key, t, v);
  }
  if (kind === "ack" && frame.error === null) delete frame.error;
  return frame;
}

/**
 * Appends `v` (a whole number up to 2^53 - 1) as a varint.
 * @param {number[]} out
 * @param {number} v
 */
function channelVarint(out, v) {
  while (v >= 128) {
    out.push((v % 128) + 128);
    v = Math.floor(v / 128);
  }
  out.push(v);
}

/**
 * Appends field `n` with length-delimited `bytes`.
 * @param {number[]} out
 * @param {number} n
 * @param {Uint8Array | number[]} bytes
 */
function channelBytes(out, n, bytes) {
  channelVarint(out, n * 8 + 2);
  channelVarint(out, bytes.length);
  for (const b of bytes) out.push(b);
}

/**
 * Encodes a binary frame (`k.channel.v1.Frame`), canonically: zeros and empty strings and bytes
 * left out, optional payloads written whenever present.
 * @param {{kind: string} & Record<string, any>} frame
 * @returns {Uint8Array}
 */
export function encodeChannelFrameBinary(frame) {
  /** @type {number[]} */
  const body = [];
  CHANNEL_FIELDS[frame.kind].forEach(([key, t], i) => {
    const v = frame[key];
    if (t === "s") {
      if (v) channelBytes(body, i + 1, new TextEncoder().encode(v));
    } else if (t === "p" || t === "o") {
      if (v != null && (t === "o" || v.length)) channelBytes(body, i + 1, v);
    } else {
      const n = t === "r" ? CHANNEL_REASONS.indexOf(v) + 1 : v;
      if (n) {
        channelVarint(body, (i + 1) * 8);
        channelVarint(body, n);
      }
    }
  });
  /** @type {number[]} */
  const out = [];
  channelBytes(out, CHANNEL_KINDS.indexOf(frame.kind) + 1, body);
  return Uint8Array.from(out);
}

/**
 * The fields of Protobuf message `bytes`: `[number, value]`, a value being a number (a varint),
 * a `Uint8Array` (length-delimited) or `null` (fixed-width, which no frame field uses).
 * @param {Uint8Array} bytes
 * @returns {[number, number | Uint8Array | null][]}
 */
function channelFields(bytes) {
  let at = 0;
  const varint = () => {
    let v = 0;
    for (let scale = 1; ; scale *= 128) {
      if (at >= bytes.length) return badFrame("a varint is cut short");
      const b = bytes[at++];
      v += (b & 127) * scale;
      if (b < 128) return v;
      if (scale > 2 ** 49) return badFrame("a varint is too large");
    }
  };
  /** @type {[number, number | Uint8Array | null][]} */
  const out = [];
  while (at < bytes.length) {
    const key = varint();
    const n = Math.floor(key / 8);
    const type = key % 8;
    if (!n) badFrame("field number 0");
    if (type === 0) out.push([n, varint()]);
    else if (type === 2) {
      const len = varint();
      if (at + len > bytes.length) badFrame("a field is longer than the frame");
      out.push([n, bytes.subarray(at, (at += len))]);
    } else if (type === 1 || type === 5) {
      at += type === 1 ? 8 : 4;
      if (at > bytes.length) badFrame("a fixed-width field is cut short");
      out.push([n, null]);
    } else badFrame(`wire type ${type} isn't supported`);
  }
  return out;
}

/**
 * Decodes a binary frame.
 * @param {Uint8Array} bytes
 * @returns {{kind: string} & Record<string, any>}
 */
export function decodeChannelFrameBinary(bytes) {
  const top = channelFields(bytes).filter(([n, v]) => n <= 10 && v instanceof Uint8Array).pop();
  if (!top) return badFrame("no known kind of frame");
  const kind = CHANNEL_KINDS[top[0] - 1];
  const fields = channelFields(/** @type {Uint8Array} */ (top[1]));
  /** @type {{kind: string} & Record<string, any>} */
  const frame = { kind };
  CHANNEL_FIELDS[kind].forEach(([key, t], i) => {
    const found = fields.filter(([n]) => n === i + 1).pop();
    const v = found ? found[1] : undefined;
    const bytesWanted = t === "s" || t === "p" || t === "o";
    if (v !== undefined && bytesWanted !== v instanceof Uint8Array) badFrame(`field ${i + 1} of \`${kind}\` has the wrong wire type`);
    if (t === "o") frame[key] = v === undefined ? null : Uint8Array.from(/** @type {Uint8Array} */ (v));
    else if (t === "p") frame[key] = v === undefined ? new Uint8Array(0) : Uint8Array.from(/** @type {Uint8Array} */ (v));
    else if (t === "s") {
      try {
        frame[key] = v === undefined ? "" : new TextDecoder("utf-8", { fatal: true }).decode(/** @type {Uint8Array} */ (v));
      } catch {
        badFrame(`field ${i + 1} of \`${kind}\` isn't UTF-8`);
      }
    } else {
      const n = v === undefined ? 0 : v;
      frame[key] = channelScalar(kind, key, t, t === "r" ? CHANNEL_REASONS[/** @type {number} */ (n) - 1] : n);
    }
  });
  if (kind === "ack" && frame.error === null) delete frame.error;
  return frame;
}

// ------------------------------------------------------------------ the client

/** The cap of each reconnect's random delay, by attempt (the last repeats), in ms. */
const BACKOFF = [500, 1000, 2000, 4000, 8000, 16000, 30000];

/** The outcome of a call or a send that couldn't reach the server (rpc.js `rpcError`). */
const OFFLINE = { ok: false, kind: "offline" };

/**
 * What the client reads the world through, replaced in tests ({@link channelEnv}): the socket,
 * the time and timers (the browser's, as `Clock`'s), the jitter's random source (`Math.random`,
 * which `k test` seeds), whether the browser is online, and which browser opens a channel now
 * with its `Session` mirror: a page is one browser (`null`); `k test`'s peers are more, each
 * with its own socket and session (44-channels.md, "TestPeer"). Members taking `b` get it.
 */
const env = {
  socket: (_b) => new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/_k/channels`, "k.channel.v1"),
  now: () => Date.now(),
  after: (ms, f) => setTimeout(f, ms),
  cancel: (t) => clearTimeout(t),
  random: () => Math.random(),
  online: (_b) => globalThis.navigator?.onLine !== false,
  browser: () => null,
  mirror: (_b) => sessionMirror,
  take: (_b, text) => takeMirror(text),
};

/** Each browser's one socket and what it carries, made when its first channel opens. */
let hubs = new Map();

/**
 * Replaces parts of {@link env} and forgets the sockets and channels without a word to the
 * server: a test's fresh start.
 * @param {Partial<typeof env>} parts
 */
export function channelEnv(parts) {
  for (const h of hubs.values()) h.stop();
  hubs = new Map();
  Object.assign(env, parts);
}

/** The connect outcome of a `refused` or `ack` error body (rpc.js `rpcError` reads it). */
const refusal = (e) => ({ ok: false, kind: "connect", code: `${e?.code ?? "unknown"}`, details: Array.isArray(e?.details) ? e.details : [], retryAfter: +e?.retryAfter || 0 });

/**
 * Opens a channel: the stub of channel rpc `method` calls it with the encoded `request`, the
 * caller's cancellation context `$c` and the channel's codecs (`encode` of a `C`, `decode` of an
 * `S` and `refusal` mapping a failed outcome to `E`). Resolves like `rpc` (rpc.js): `{ok: true,
 * message}` with the open `Channel`, a `connect` outcome when the server refused, `offline` when
 * the socket failed and `timeout` after 30 seconds; the first open isn't retried. Cancelling
 * `$c` closes the channel (`byClient`), and before it opened leaves the promise unsettled.
 * @param {string} method
 * @param {unknown} request
 * @param {AbortSignal|null} $c
 * @param {{encode: Function, decode: Function, refusal: Function}} codec
 * @returns {Promise<object>}
 */
export function openChannel(method, request, $c, codec) {
  if ($c?.aborted) return new Promise(() => {});
  return new Promise((resolve) => {
    const c = new Channel(method, request, codec);
    const t = env.after(30_000, () => c.opening && c.fail({ ok: false, kind: "timeout" }));
    c.opening = (o) => {
      env.cancel(t);
      c.opening = null;
      resolve(o);
    };
    $c?.addEventListener("abort", () => c.close());
    const b = env.browser();
    if (!hubs.has(b)) hubs.set(b, new Hub(b));
    (c.hub = hubs.get(b)).add(c);
  });
}

/**
 * A channel in the browser, `k.rpc.Channel`'s client members: `state` (a `ChannelState`,
 * reactive), `messages` (a stream of `S`, across reconnects, ending when the channel closes for
 * good; leaving its `for await` closes the channel), `send` and `close`.
 */
class Channel {
  constructor(method, request, codec) {
    this.method = method;
    this.request = request;
    this.codec = codec;
    this.st = signal(unitCase(0));
    this.messages = new PushStream();
    this.messages.close = () => this.close();
    /** Its number on the current socket (0: none), and whether it is open there. */
    this.ch = 0;
    this.on = false;
    /** The resume token, `null` before the first `opened` and once the server forgot it. */
    this.token = null;
    /** The last server message received, and the last one confirmed with `rcvd`. */
    this.got = 0;
    this.told = 0;
    /** The last sequence number transmitted, and the sends not acknowledged yet, in order. */
    this.seq = 0;
    this.out = [];
    this.done = false;
    this.opening = null;
    /** The socket of the browser that opened it. */
    this.hub = null;
  }

  get state() {
    return this.st.get();
  }

  /** The first open failed with outcome `o`: the caller gets it, and the channel is gone. */
  fail(o) {
    this.done = true;
    this.hub.remove(this);
    this.opening?.(o);
  }

  /**
   * Sends `m` and settles with the server's acknowledgement: resolves when it was handled,
   * rejects with the refusal `E`, or `rpcFailure(.offline)` when it couldn't be delivered
   * (closed channel, too many waiting, 10 s without a socket, a restart after transmitting it),
   * or `.invalid` over 64 KiB. Cancelling `c` withdraws it if not transmitted yet, and
   * otherwise ignores the answer.
   * @param {AbortSignal|null} c
   * @param {unknown} m
   * @returns {Promise<void>}
   */
  send(c, m) {
    if (c?.aborted) return new Promise(() => {});
    return new Promise((ok, no) => {
      const fail = (o) => no(this.codec.refusal(o));
      if (this.done) return fail(OFFLINE);
      const v = this.codec.encode(m);
      if (new TextEncoder().encode(JSON.stringify(v)).length > 65536) {
        return fail({ ok: false, kind: "connect", code: "invalid_argument", details: [{ type: "k.ValidationError", debug: { failures: [{ path: "", message: "the message is larger than 64 KiB" }] } }] });
      }
      if (!this.on && this.out.filter((p) => !p.seq).length >= 32) return fail(OFFLINE);
      const p = { v, seq: 0, ok, no: fail, wait: null };
      this.out.push(p);
      c?.addEventListener("abort", () => {
        if (!p.seq) this.drop(p);
        p.ok = p.no = () => {};
      });
      this.pump();
    });
  }

  /** Forgets send `p`, and its wait. */
  drop(p) {
    env.cancel(p.wait);
    this.out.splice(this.out.indexOf(p), 1);
  }

  /**
   * Moves the sends along: while open, transmits waiting ones with at most 32 unacknowledged;
   * otherwise starts each one's 10 seconds, after which it fails `offline`.
   */
  pump() {
    let flying = this.out.filter((p) => p.seq).length;
    for (const p of this.out) {
      if (this.on) {
        if (!p.seq && flying < 32) {
          p.seq = ++this.seq;
          flying++;
          this.hub.put({ kind: "msg", ch: this.ch, seq: p.seq, value: p.v });
        }
      } else {
        p.wait ??= env.after(10_000, () => {
          this.drop(p);
          p.no(OFFLINE);
        });
      }
    }
  }

  /** Closes the channel for good (`byClient`); idempotent. */
  close() {
    this.end(0, true);
  }

  /**
   * Ends the channel with `ChannelClose` case `reason`: its state, its stream, its waiting
   * sends (`offline`), and a `close` frame when `tell`.
   * @param {number} reason
   * @param {boolean} tell
   */
  end(reason, tell) {
    if (this.done) return;
    this.done = true;
    this.opening = null;
    if (tell && this.ch && this.hub.up) this.hub.put({ kind: "close", ch: this.ch, reason: CHANNEL_REASONS[reason] });
    this.st.set({ $: 3, _0: unitCase(reason) });
    this.messages.push(null);
    for (const p of this.out.splice(0)) {
      env.cancel(p.wait);
      p.no(OFFLINE);
    }
    this.hub.remove(this);
  }

  /**
   * Handles frame `f` for this channel.
   * @param {{kind: string} & Record<string, any>} f
   */
  frame(f) {
    const k = f.kind;
    if (k === "opened") {
      this.token = f.resume;
      this.on = true;
      this.st.set(unitCase(1));
      this.opening?.({ ok: true, message: this });
      // After a resume, what the server may not have handled goes again; it skips any it had.
      for (const p of this.out) {
        env.cancel(p.wait);
        p.wait = null;
        if (p.seq) this.hub.put({ kind: "msg", ch: this.ch, seq: p.seq, value: p.v });
      }
      this.pump();
    } else if (k === "refused") {
      this.hub.chans.delete(this.ch);
      this.ch = 0;
      if (this.opening) this.fail(refusal(f.error));
      else if (this.token && f.error?.code === "not_found") {
        // The server forgot it (past the window, a deploy, another replica): open it again.
        this.token = null;
        this.hub.attach(this);
      } else this.end(3, false);
    } else if (k === "msg") {
      if (f.seq <= this.got) return;
      this.got = f.seq;
      this.hub.receipt();
      let v;
      try {
        v = this.codec.decode(f.value);
      } catch (e) {
        // A case this tab doesn't know (a newer server): skipped (44-channels.md, "rpc.lock").
        console.warn("a channel message was skipped:", e);
        return;
      }
      this.messages.push(v);
    } else if (k === "ack") {
      const p = this.out.find((x) => x.seq === f.seq);
      if (!p) return;
      this.drop(p);
      if (f.error === undefined) p.ok();
      else p.no(f.error?.code === "unavailable" ? OFFLINE : refusal(f.error));
      this.pump();
    } else if (k === "close") {
      this.end(CHANNEL_REASONS.indexOf(f.reason), false);
    }
  }
}

/**
 * The page's socket: opens it for the first channel and closes it after the last, numbers
 * channels on it, answers heartbeats, confirms receipts, notices silence, and reconnects with
 * backoff while channels live, resuming or restarting each.
 */
class Hub {
  /** @param {unknown} b the browser ({@link env}) */
  constructor(b) {
    this.b = b;
    this.ws = null;
    this.up = false;
    /** Channels by number on the current socket, and every channel not closed for good. */
    this.chans = new Map();
    this.live = new Set();
    this.next = 1;
    /** The number of the next attempt (0 while connected), and its timer. */
    this.attempt = 0;
    this.timer = null;
    this.watch = null;
    this.rcvd = null;
    this.heard = 0;
    /** The page is in the back-forward cache (`pagehide`), so nothing reconnects. */
    this.hidden = false;
    /** The last drop was an overflow, so the next connect doesn't reset the backoff. */
    this.slow = false;
    /** The mirror's user the socket was opened under. */
    this.user = null;
    const g = globalThis;
    const now = () => this.live.size && !this.ws && !this.hidden && env.online(b) && this.connect();
    this.off = [
      [g, "online", now],
      [g, "k-online", now],
      [g.document, "visibilitychange", () => g.document.visibilityState !== "hidden" && now()],
      [g, "pagehide", () => {
        this.hidden = true;
        this.drop(this.ws, 1000, "");
      }],
      [g, "pageshow", () => {
        this.hidden = false;
        now();
      }],
    ];
    for (const [t, type, f] of this.off) t?.addEventListener?.(type, f);
    // A change of session replaces the socket: its channels restart under the new one.
    this.dispose = root((dispose) => {
      effect(() => {
        const user = JSON.stringify(env.mirror(b).get().user ?? null);
        untrack(() => {
          if (this.ws && user !== this.user) {
            for (const c of this.live) c.token = null;
            this.drop(this.ws, 1000, "", true);
          }
        });
      });
      return dispose;
    });
  }

  /** Stops everything without telling anyone (a test's reset). */
  stop() {
    this.live.clear();
    this.drop(this.ws, 1000, "");
    for (const [t, type, f] of this.off) t?.removeEventListener?.(type, f);
    this.dispose();
  }

  /** Adds new channel `c`, opening it now if the socket is up. */
  add(c) {
    this.live.add(c);
    if (this.up) this.attach(c);
    else if (!this.ws && !this.timer) this.connect();
  }

  /** Forgets channel `c`; the socket closes after the last one. */
  remove(c) {
    this.chans.delete(c.ch);
    this.live.delete(c);
    if (!this.live.size) this.drop(this.ws, 1000, "");
  }

  /** Sends frame `f` if the socket is up. */
  put(f) {
    if (this.up) this.ws.send(encodeChannelFrame(f));
  }

  /** Opens the socket now. */
  connect() {
    env.cancel(this.timer);
    this.timer = null;
    this.user = JSON.stringify(env.mirror(this.b).peek().user ?? null);
    let s;
    try {
      s = this.ws = env.socket(this.b);
    } catch {
      return this.drop(null, 1006, "");
    }
    s.onopen = () => {
      this.up = true;
      this.heard = env.now();
      if (!this.slow) this.attempt = 0;
      this.slow = false;
      this.watch = env.after(60_000, () => this.silence());
      globalThis.dispatchEvent?.(new Event("k-online"));
      for (const c of this.live) this.attach(c);
    };
    s.onmessage = (e) => this.frame(e.data);
    s.onclose = (e) => this.drop(s, e.code, e.reason);
  }

  /**
   * Opens channel `c` on the socket under a new number: resumes it with its token, or opens it
   * (again), dropping what was transmitted and not acknowledged before a restart (`offline`).
   */
  attach(c) {
    this.chans.delete(c.ch);
    c.ch = this.next++;
    this.chans.set(c.ch, c);
    if (!c.opening) c.st.set(unitCase(0));
    if (c.token) return this.put({ kind: "resume", ch: c.ch, token: c.token, received: c.got, sent: c.seq });
    if (!c.opening) {
      for (const p of c.out.filter((x) => x.seq)) {
        c.drop(p);
        p.no(OFFLINE);
      }
      c.seq = c.got = c.told = 0;
    }
    this.put({ kind: "open", ch: c.ch, method: c.method, request: c.request });
  }

  /** Closes the socket after `silence` without a frame (`4000`, as the server would). */
  silence() {
    const left = this.heard + 60_000 - env.now();
    if (left > 0) this.watch = env.after(left, () => this.silence());
    else this.drop(this.ws, 4000, "");
  }

  /** Sends `rcvd` for each channel with messages not confirmed yet. */
  confirm() {
    env.cancel(this.rcvd);
    this.rcvd = null;
    for (const c of this.chans.values()) {
      if (c.on && c.got > c.told) this.put({ kind: "rcvd", ch: c.ch, seq: (c.told = c.got) });
    }
  }

  /** A message arrived: confirm it within a second. */
  receipt() {
    this.rcvd ??= env.after(1000, () => this.confirm());
  }

  /** Handles one text frame from the server. */
  frame(text) {
    this.heard = env.now();
    let f;
    try {
      f = decodeChannelFrame(text);
    } catch {
      return;
    }
    if (f.kind === "beat") {
      this.put({ kind: "beat" });
      this.confirm();
    } else if (f.kind === "session") env.take(this.b, JSON.stringify({ user: f.user }));
    else this.chans.get(f.ch)?.frame(f);
  }

  /**
   * The socket `s` is gone (`code`, `reason` of its close; nothing if it isn't the current
   * one): channels opening for the first time fail `offline`, the others wait for the next
   * attempt, after the server's `k.retry=<ms>` (1001), at once (`now`, a change of session) or
   * after a random delay up to the attempt's cap, which only an online, shown page tries.
   * @param {WebSocket|null} s
   * @param {number} code
   * @param {string} reason
   * @param {boolean} [now]
   */
  drop(s, code, reason, now) {
    if (s !== this.ws) return;
    const was = this.up;
    if (s) {
      s.onclose = s.onmessage = s.onopen = null;
      try {
        s.close(1000);
      } catch {
        // Already closing.
      }
    }
    this.ws = null;
    this.up = false;
    for (const t of [this.watch, this.rcvd, this.timer]) env.cancel(t);
    this.watch = this.rcvd = this.timer = null;
    this.chans.clear();
    for (const c of this.live) {
      c.on = false;
      if (c.opening) c.fail(OFFLINE);
      else c.pump();
    }
    if (!this.live.size) return;
    if (!was && code !== 1000) globalThis.dispatchEvent?.(new Event("k-offline"));
    if (code === 1013) this.slow = true;
    const n = ++this.attempt;
    const retry = code === 1001 && new RegExp("k\\.retry=(\\d+)").exec(reason);
    const delay = now ? 0 : retry ? +retry[1] : Math.floor(env.random() * BACKOFF[Math.min(n, BACKOFF.length) - 1]);
    const at = { $: 2, attempt: n, at: new Instant(env.now() + delay) };
    for (const c of this.live) c.st.set(at);
    if (now) this.connect();
    else if (!this.hidden) this.timer = env.after(delay, () => (this.timer = null, env.online(this.b) && this.connect()));
  }
}
