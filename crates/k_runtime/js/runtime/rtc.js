/**
 * @module k:runtime/rtc
 *
 * `k.rtc` in the browser (docs/design/46-media-and-rtc.md, "Peer
 * connections"; docs/compiler/runtime.md §12.15): `BrowserPeerConnections`
 * over `RTCPeerConnection`, with the "perfect negotiation" pattern of the
 * WebRTC specification, ICE restarts, `MediaSender.replace`, typed data
 * channels and `stats()`.
 *
 * The connection logic, `peerConnections(env)`, takes the browser's parts
 * from `env` (`RTCPeerConnection`, `MediaStream`, `after(ms, f)` and
 * `cancel(handle)`), so `k test` runs the same negotiation over the kit's
 * in-process media engine (`testing/rtc.js`) on its `TestClock`.
 *
 * Signalling is the app's channel of `Signal`s, whose cases are `0`
 * `peerJoined`, `1` `peerLeft`, `{$: 2, peer}` `hello`, `{$: 3, type, sdp}`
 * `description`, `{$: 4, candidate, mid, line}` `candidate`, `5` `bye`.
 * A peer sends `hello` with its id when the relay says `peerJoined`; the
 * two ids decide which peer is polite (the larger one), so a peer that
 * rejoins after its channel restarted keeps its role. A `hello` with an
 * id this peer hasn't seen is a new peer (the other one reloaded): the
 * connection starts over with it, keeping the streams sent and the data
 * channels opened.
 *
 * The other enums: `PeerState` `0` `waitingForPeer`, `1` `connecting`, `2`
 * `connected`, `3` `reconnecting`, `{$: 4, _0: PeerClose}` `closed`;
 * `PeerClose` `0` `byApp`, `1` `byPeer`, `2` `failed`, `3`
 * `signallingLost`; `PeerError` `0` `unsupported`, `1` `signallingClosed`;
 * `IcePolicy` `0` `all`, `1` `relayOnly`; `Delivery` `0` `reliable`, `{$:
 * 1, retransmits}` `unordered`; `DataChannelError` `0` `closed`, `{$: 1,
 * limit}` `tooLarge`, `2` `incompatible`; `TrackKind`, `TrackState` and
 * `TrackEnd` as in `media.js`, whose stream and track handles these
 * mirror (a feature chunk imports only the runtime chunk).
 *
 * Shipped as its own client chunk, only to pages whose client uses
 * `k.rtc` (codegen.md §16), and measured against its own size budget.
 */

import { PushStream } from "./std.js";
import { batch, signal } from "./signals.js";
import { CANCELLED, unitCase } from "./values.js";

/** The largest encoded data channel message, and the buffer `send` waits below. */
const MAX_MESSAGE = 65536;
const MAX_BUFFERED = 1 << 20;

/** How long a connection may stay `.reconnecting` before it closes `.failed`. */
const RECONNECT_MS = 15000;

/**
 * The id both peers give the data channel named `name`: 1 plus the 32-bit
 * FNV-1a hash of its UTF-8 bytes modulo 1023 (id 0 is this file's own
 * channel). The compiler computes the same to report collisions (K2409).
 * @param {string} name
 * @returns {number}
 */
export function dataChannelId(name) {
  let h = 0x811c9dc5;
  for (const b of new TextEncoder().encode(name)) h = Math.imul(h ^ b, 0x01000193) >>> 0;
  return 1 + (h % 1023);
}

/**
 * A `MediaTrack` handle of a track received from the peer, as
 * `media.js`'s `mediaTrack`: it ends with `.remoteEnded` when the peer
 * stops sending it.
 * @param {MediaStreamTrack} t
 */
function remoteTrack(t) {
  const enabled = signal(t.enabled !== false);
  const muted = signal(t.muted === true);
  const state = signal(unitCase(0));
  const ends = new Set();
  const end = (why) => {
    if (state.peek().$ !== 0) return;
    state.set({ $: 1, _0: unitCase(why) });
    for (const f of [...ends]) f(why);
  };
  t.onmute = () => muted.set(true);
  t.onunmute = () => muted.set(false);
  t.onended = () => end(3);
  return {
    kind: unitCase(t.kind === "video" ? 1 : 0),
    label: String(t.label ?? ""),
    device: null,
    settings: { size: null, frameRate: null, sampleRate: null, channelCount: null, echoCancellation: null, noiseSuppression: null, autoGainControl: null },
    get isEnabled() {
      return enabled.get();
    },
    get isMuted() {
      return muted.get();
    },
    get state() {
      return state.get();
    },
    setEnabled(v) {
      t.enabled = v === true;
      enabled.set(v === true);
    },
    stop() {
      end(0);
    },
    equals(b) {
      return this === b;
    },
    $t: t,
    $end: end,
    $ends: ends,
  };
}

/** A remote `MediaStream` handle of track handles, as `media.js`'s `mediaStream`. */
function remoteStream(tracks, native) {
  const of = (k) => tracks.filter((t) => t.kind.$ === k);
  return {
    get audioTracks() {
      return of(0);
    },
    get videoTracks() {
      return of(1);
    },
    get isLive() {
      return tracks.some((t) => t.state.$ === 0);
    },
    isLocal: false,
    stop() {
      batch(() => tracks.forEach((t) => t.stop()));
    },
    equals(b) {
      return this === b;
    },
    $s: native,
    $tracks: tracks,
  };
}

/** The connections a `.service` lifetime closes when the page is left, and its path. */
let paged = null;
let pagedAt = null;

/** Closes `conn` when the page that connected is left (another path, or unloading). */
function closeWithPage(conn) {
  if (paged === null) {
    paged = new Set();
    const leave = (all) => {
      if (!all && globalThis.location?.pathname === pagedAt) return;
      for (const c of [...paged]) c.end(0);
    };
    const h = globalThis.history;
    for (const k of ["pushState", "replaceState"]) {
      const f = h?.[k];
      if (typeof f === "function")
        h[k] = function (...a) {
          const r = f.apply(this, a);
          leave(false);
          return r;
        };
    }
    globalThis.addEventListener?.("popstate", () => leave(false));
    globalThis.addEventListener?.("pagehide", () => leave(true));
  }
  pagedAt = globalThis.location?.pathname;
  paged.add(conn);
}

/**
 * The connection logic over `env`: `connect` and the `PeerConnection`,
 * `MediaSender` and `DataChannel` handles. `env.opened(conn, kind)` (the
 * kit's) hears of every data channel a connection opens; `env.log` keeps
 * what each data channel received, for `TestCallPeer.received`.
 * @param {{RTCPeerConnection: Function|undefined, MediaStream?: Function, after: Function, cancel: Function, opened?: Function, log?: boolean}} env
 */
export function peerConnections(env) {
  return {
    get isSupported() {
      return typeof env.RTCPeerConnection === "function";
    },
    async connect(c, signalling, servers, policy, lifetime) {
      if (typeof env.RTCPeerConnection !== "function") throw unitCase(0);
      const conn = new Conn(env, signalling, servers ?? [], policy?.$ === 1);
      if (c?.aborted) {
        conn.end(0);
        throw CANCELLED;
      }
      if (lifetime?.$ === 1) closeWithPage(conn);
      else c?.addEventListener("abort", () => conn.end(0));
      await conn.paired;
      return conn.handle;
    },
  };
}

/** One call: the signalling loop, the current `RTCPeerConnection`, and what was sent and opened. */
class Conn {
  constructor(env, ch, servers, relay) {
    this.env = env;
    this.ch = ch;
    this.config = {
      iceServers: servers.map((s) => ({ urls: s.urls.map(String), username: s.username ?? undefined, credential: s.credential ?? undefined })),
      iceTransportPolicy: relay ? "relay" : "all",
    };
    this.me = Math.random().toString(36).slice(2) + Date.now().toString(36);
    this.other = null;
    this.pc = null;
    this.polite = false;
    this.making = false;
    this.ignoring = false;
    this.timer = null;
    this.later = null;
    this.st = signal(unitCase(0));
    this.remote = signal(null);
    this.tracks = [];
    this.native = null;
    this.senders = new Set();
    this.channels = new Map();
    this.done = false;
    this.queue = Promise.resolve();
    this.paired = new Promise((ok, no) => {
      this.ok = ok;
      this.no = no;
    });
    this.paired.catch(() => {});
    this.handle = this.makeHandle();
    this.read();
  }

  /** Reads the channel's signals in order, each handled after the last. */
  async read() {
    for (;;) {
      const s = await this.ch.messages.next();
      if (s === null) {
        this.no(unitCase(1));
        this.end(3);
        return;
      }
      this.queue = this.queue.then(() => this.signal(s)).catch(() => {});
    }
  }

  /** Sends signal `s` on the channel, ignoring a failure: the channel reconnects by itself. */
  say(s) {
    if (!this.done) this.ch.send(null, s).catch(() => {});
  }

  /** Handles one signal from the relay or the peer. */
  async signal(s) {
    if (this.done) return;
    const pc = this.pc;
    switch (s.$) {
      case 0:
        return this.say({ $: 2, peer: this.me });
      case 1:
        if (this.st.peek().$ <= 1 && pc) {
          this.drop();
          this.st.set(unitCase(0));
        }
        return;
      case 2:
        if (s.peer === this.other && pc) {
          if (!this.polite) pc.restartIce();
          return;
        }
        this.other = s.peer;
        this.polite = this.me > s.peer;
        this.build();
        this.ok(this.handle);
        return;
      case 3: {
        if (!pc) return;
        const collision = s.type === "offer" && (this.making || pc.signalingState !== "stable");
        this.ignoring = !this.polite && collision;
        if (this.ignoring) return;
        await pc.setRemoteDescription({ type: s.type, sdp: s.sdp });
        if (s.type === "offer") {
          await pc.setLocalDescription();
          this.describe(pc);
          if (pc === this.pc && this.later) {
            const attach = this.later;
            this.later = null;
            attach();
          }
        }
        return;
      }
      case 4:
        if (!pc) return;
        try {
          await pc.addIceCandidate({ candidate: s.candidate, sdpMid: s.mid ?? undefined, sdpMLineIndex: s.line ?? undefined });
        } catch (e) {
          if (!this.ignoring) throw e;
        }
        return;
      case 5:
        return this.end(1);
    }
  }

  /** Sends `pc`'s local description. */
  describe(pc) {
    const d = pc.localDescription;
    if (d && pc === this.pc) this.say({ $: 3, type: d.type, sdp: d.sdp });
  }

  /** Starts a new `RTCPeerConnection` with the current peer, with what was sent and opened. */
  build() {
    this.drop();
    const pc = (this.pc = new this.env.RTCPeerConnection(this.config));
    this.st.set(unitCase(1));
    pc.onnegotiationneeded = async () => {
      try {
        this.making = true;
        await pc.setLocalDescription();
        this.describe(pc);
      } catch {
        // A newer description replaced it; the next round renegotiates.
      } finally {
        this.making = false;
      }
    };
    pc.onicecandidate = ({ candidate: c }) => {
      if (c && c.candidate && pc === this.pc) this.say({ $: 4, candidate: c.candidate, mid: c.sdpMid ?? null, line: c.sdpMLineIndex ?? null });
    };
    pc.oniceconnectionstatechange = () => this.ice(pc);
    pc.ontrack = (e) => pc === this.pc && this.track(e);
    // Our own channel, so the first offer already carries the data
    // transport and later channels open without renegotiating. The
    // impolite peer makes the first offer, and the polite one adds its
    // own after answering it: a first offer rolled back makes Chrome's
    // gathering for the answer stall (and a TURN allocation clash).
    const attach = () => {
      pc.createDataChannel("k", { negotiated: true, id: 0 });
      for (const s of this.senders) s.attach(pc);
      for (const d of this.channels.values()) d.attach(pc);
    };
    if (this.polite) this.later = attach;
    else attach();
  }

  /** Closes the current `RTCPeerConnection`; its remote tracks end. */
  drop() {
    const pc = this.pc;
    if (!pc) return;
    this.pc = null;
    this.later = null;
    this.env.cancel(this.timer);
    this.timer = null;
    for (const s of this.senders) s.native = [];
    for (const d of this.channels.values()) d.detach();
    pc.close();
    batch(() => {
      for (const t of this.tracks) t.$end(3);
      this.tracks = [];
      this.native = null;
      this.remote.set(null);
    });
  }

  /** Follows `pc`'s ICE state: `.connected`, or `.reconnecting` with a restart by the impolite peer. */
  ice(pc) {
    if (pc !== this.pc) return;
    const s = pc.iceConnectionState;
    if (s === "connected" || s === "completed") {
      this.env.cancel(this.timer);
      this.timer = null;
      this.st.set(unitCase(2));
    } else if (s === "disconnected" || s === "failed") {
      if (this.st.peek().$ === 2) this.st.set(unitCase(3));
      this.timer ??= this.env.after(RECONNECT_MS, () => this.end(2));
      if (!this.polite) pc.restartIce();
    }
  }

  /** A track arrived from the peer: it joins `remote`, and ends when the peer stops sending it. */
  track(e) {
    const t = e.track;
    if (this.tracks.some((h) => h.$t === t)) return;
    const h = remoteTrack(t);
    const stream = e.streams?.[0];
    if (stream) stream.onremovetrack = (r) => this.tracks.find((x) => x.$t === r.track)?.$end(3);
    h.$ends.add(() => {
      this.tracks = this.tracks.filter((x) => x !== h);
      this.native?.removeTrack?.(t);
      this.remote.set(this.tracks.length ? remoteStream([...this.tracks], this.native) : null);
    });
    this.tracks.push(h);
    if (typeof this.env.MediaStream === "function") {
      this.native ??= new this.env.MediaStream();
      this.native.addTrack(t);
    }
    this.remote.set(remoteStream([...this.tracks], this.native));
  }

  /**
   * Ends the call with `PeerClose` case `why`: tells the peer when the app
   * hung up, closes the channel, the connection, its tracks and channels.
   * @param {number} why
   */
  end(why) {
    if (this.done) return;
    const close = () => this.ch.close();
    if (why === 0) {
      // The peer hears `bye` before the channel closes, or after 2 s without it.
      this.ch.send(null, unitCase(5)).then(close, close);
      this.env.after(2000, close);
    } else close();
    this.done = true;
    paged?.delete(this);
    this.drop();
    for (const d of this.channels.values()) d.end(unitCase(0));
    this.st.set({ $: 4, _0: unitCase(why) });
    this.no(unitCase(1));
  }

  /** The `PeerConnection` handle of this call. */
  makeHandle() {
    const conn = this;
    return {
      get state() {
        return conn.st.get();
      },
      get remote() {
        return conn.remote.get();
      },
      send(stream) {
        const s = new Sender(conn, stream);
        conn.senders.add(s);
        if (conn.pc && !conn.later) s.attach(conn.pc);
        return s.handle;
      },
      channel(kind, delivery) {
        let d = conn.channels.get(kind.name);
        if (!d) {
          conn.channels.set(kind.name, (d = new Data(conn, kind, delivery)));
          if (conn.pc && !conn.later) d.attach(conn.pc);
          conn.env.opened?.(conn, kind);
        }
        return d.handle;
      },
      async stats() {
        return peerStats(conn.pc ? await conn.pc.getStats() : null);
      },
      close() {
        conn.end(0);
      },
      $conn: conn,
    };
  }
}

/**
 * A `PeerStats` of an `RTCStatsReport` (`null`: none yet): the selected
 * candidate pair's round-trip time and estimated bitrate, whether its
 * local candidate is a relay, and the packets lost of every inbound track.
 * @param {Map<string, object>|null} report
 */
export function peerStats(report) {
  const by = new Map();
  let lost = 0;
  let pair = null;
  report?.forEach((s) => {
    by.set(s.id, s);
    if (s.type === "inbound-rtp") lost += s.packetsLost | 0;
  });
  for (const s of by.values()) {
    if (s.type === "transport" && s.selectedCandidatePairId) pair = by.get(s.selectedCandidatePairId) ?? pair;
  }
  for (const s of by.values()) {
    if (!pair && s.type === "candidate-pair" && (s.selected || (s.nominated && s.state === "succeeded"))) pair = s;
  }
  const rtt = pair?.currentRoundTripTime;
  const rate = pair?.availableOutgoingBitrate;
  return {
    roundTripTime: typeof rtt === "number" ? Math.round(rtt * 1000) : null,
    outgoingBitrate: typeof rate === "number" ? Math.round(rate) : null,
    packetsLost: lost,
    isRelayed: by.get(pair?.localCandidateId)?.candidateType === "relay",
  };
}

/**
 * The tracks one `send(stream)` sends, paged across a new connection with a
 * new peer. `native[i]` is the `RTCRtpSender` of `tracks[i]`.
 */
class Sender {
  constructor(conn, stream) {
    this.conn = conn;
    this.tracks = [...stream.$tracks];
    this.streams = stream.$s ? [stream.$s] : [];
    this.native = [];
    this.stopped = false;
    this.handle = {
      replace: (s) => this.replace(s),
      stop: () => this.stop(),
    };
  }

  /** Adds the tracks to `pc`. */
  attach(pc) {
    if (!this.stopped) this.native = this.tracks.map((t) => pc.addTrack(t.$t, ...this.streams));
  }

  /**
   * Sends `stream`'s tracks instead: each replaces the track of its kind in
   * place (`replaceTrack`, no renegotiation), a kind not sent yet is added,
   * and a kind `stream` lacks keeps sending (a screen share keeps the
   * microphone).
   */
  replace(stream) {
    if (this.stopped) return;
    // Not attached yet (the polite peer before its first answer): `attach` adds them.
    const pc = this.conn.later ? null : this.conn.pc;
    for (const t of stream.$tracks) {
      const i = this.tracks.findIndex((x) => x.kind.$ === t.kind.$);
      if (i >= 0) {
        this.tracks[i] = t;
        if (pc) this.native[i]?.replaceTrack(t.$t);
      } else {
        this.tracks.push(t);
        if (pc) this.native.push(pc.addTrack(t.$t, ...this.streams));
      }
    }
  }

  /** Stops sending: the senders leave the connection, and the peer's tracks end. */
  stop() {
    if (this.stopped) return;
    this.stopped = true;
    this.conn.senders.delete(this);
    for (const s of this.native) this.conn.pc?.removeTrack(s);
    this.native = [];
  }
}

/**
 * One typed data channel, paged across a new connection. When it opens,
 * each side sends `{"k": shape}`, its message type's fingerprint; a
 * different one closes it `.incompatible`. Messages are `{"m": json}`, the
 * codec's encoding, received before the fingerprint is checked are held.
 */
class Data {
  constructor(conn, kind, delivery) {
    this.conn = conn;
    this.kind = kind;
    this.options = delivery?.$ === 1 ? { ordered: false, maxRetransmits: delivery.retransmits } : { ordered: true };
    this.dc = null;
    this.matched = false;
    this.held = [];
    this.waiting = [];
    this.error = null;
    this.open = signal(false);
    this.log = conn.env.log ? [] : null;
    this.messages = new PushStream();
    this.messages.close = () => this.end(unitCase(0));
    const d = this;
    this.handle = {
      messages: this.messages,
      get isOpen() {
        return d.open.get();
      },
      send: (c, m) => this.send(m),
      close: () => this.end(unitCase(0)),
      $data: this,
    };
  }

  /** Opens the channel on `pc`, under the id of its name. */
  attach(pc) {
    if (this.error) return;
    const dc = (this.dc = pc.createDataChannel(this.kind.name, { negotiated: true, id: dataChannelId(this.kind.name), ...this.options }));
    dc.bufferedAmountLowThreshold = MAX_BUFFERED;
    dc.onopen = () => dc.send(JSON.stringify({ k: this.kind.shape }));
    dc.onmessage = (e) => dc === this.dc && this.got(e.data);
    // The peer went (a reload, a lost network): sends wait for the next connection.
    dc.onclose = () => dc === this.dc && this.detach();
  }

  /** Forgets the channel of a connection that ended; a new one starts over. */
  detach() {
    this.dc = null;
    this.matched = false;
    this.open.set(false);
  }

  /** Handles one received text frame. */
  got(text) {
    let f;
    try {
      f = JSON.parse(text);
    } catch {
      return;
    }
    if ("k" in f) {
      if (f.k !== this.kind.shape) return this.end(unitCase(2));
      this.matched = true;
      this.open.set(true);
      for (const w of this.waiting.splice(0)) w.ok();
      for (const m of this.held.splice(0)) this.deliver(m);
    } else if (this.matched) this.deliver(f.m);
    else this.held.push(f.m);
  }

  /** Decodes and delivers one message; one that doesn't decode is skipped. */
  deliver(json) {
    let v;
    try {
      v = this.kind.codec.decode(json, "");
    } catch {
      return;
    }
    this.log?.push(v);
    this.messages.push(v);
  }

  /** Sends `m` once the channel is open and checked, below 1 MiB buffered. */
  async send(m) {
    if (this.error) throw this.error;
    const text = JSON.stringify({ m: this.kind.codec.encode(m) });
    if (new TextEncoder().encode(text).length > MAX_MESSAGE) throw { $: 1, limit: MAX_MESSAGE };
    while (!this.matched || this.dc?.readyState !== "open") {
      await new Promise((ok, no) => this.waiting.push({ ok, no }));
    }
    const dc = this.dc;
    if (dc.bufferedAmount > MAX_BUFFERED) await new Promise((ok) => dc.addEventListener("bufferedamountlow", ok, { once: true }));
    dc.send(text);
  }

  /** Ends the channel with `DataChannelError` `error` (`.closed`, `.incompatible`). */
  end(error) {
    if (this.error) return;
    this.error = error;
    this.dc?.close();
    this.dc = null;
    this.open.set(false);
    for (const w of this.waiting.splice(0)) w.no(error);
    this.messages.push(null);
  }
}

/**
 * The browser's `k.rtc.BrowserPeerConnections`: the browser's
 * `RTCPeerConnection` and `MediaStream`, with timers from `setTimeout`.
 * @returns {object}
 */
export function BrowserPeerConnections() {
  return peerConnections({
    RTCPeerConnection: globalThis.RTCPeerConnection,
    MediaStream: globalThis.MediaStream,
    after: (ms, f) => setTimeout(f, ms),
    cancel: (h) => clearTimeout(h),
  });
}
