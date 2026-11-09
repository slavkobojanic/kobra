/**
 * @module k:runtime/relay
 *
 * `k.rtc` on the server (docs/design/46-media-and-rtc.md, "Signalling over
 * a channel" and "STUN and TURN without third-party accounts";
 * docs/compiler/server.md §20): `SignallingRelay` and its `RelayPeer`s on
 * `Presence` and `Broadcast`, and `IceServers` with coturn's TURN REST
 * credentials, whose HMAC-SHA1 lives here and nowhere else.
 *
 * A room is a presence key whose members are the peers' random ids, in
 * the order they joined, and a broadcast of `{from, signal}` envelopes
 * (the signal in `Signal`'s codec encoding) that each peer reads, skipping
 * its own. Both live in the process's Rust state, and with Postgres in
 * every replica's, so the two peers of a room may be on different
 * replicas, and a crashed replica's peers leave when their leases expire.
 * `Signal`'s cases are those of `rtc.js`: `0` `peerJoined`, `1`
 * `peerLeft`.
 */

import { ServerBroadcast, ServerPresence } from "./app.js";
import { bytesBase64, utf8 } from "./std.js";
import { unitCase } from "./values.js";

/** A codec that passes JSON values through: members and envelopes are already JSON. */
const RAW = { encode: (v) => v, decode: (j) => j };

/** A random peer id, unique within a process run. */
let counter = 0;
const peerId = () => `${(++counter).toString(36)}.${Math.random().toString(36).slice(2, 10)}`;

/**
 * `SignallingRelay<Key>`: rooms of at most `peersPerRoom` peers.
 */
export class SignallingRelay {
  /**
   * @param {string} name a process-unique name (the declaration's)
   * @param {{encode: Function, decode: Function}} signal `Signal`'s codec
   * @param {number} [peersPerRoom]
   */
  constructor(name, signal, peersPerRoom) {
    this.signal = signal;
    this.capacity = peersPerRoom ?? 2;
    this.presence = new ServerPresence(`${name}|peers`, RAW, RAW);
    this.mail = new ServerBroadcast(`${name}|signals`, RAW);
  }

  /**
   * Joins room `key` for the calling request: `null` when it is full. The
   * peer subscribes to the room's signals and changes before it joins, so
   * it misses no `hello` from a peer that hears it arrive; a peer that
   * finds itself beyond the capacity among the members in arrival order
   * leaves again, so two racing joins can't both take the last place.
   * @param {AbortSignal|null} _c
   * @param {unknown} key
   * @returns {Promise<RelayPeer|null>}
   */
  async join(_c, key) {
    if ((await this.presence.members(null, key)).length >= this.capacity) return null;
    const me = peerId();
    const mail = this.mail.subscribe(key);
    const changes = this.presence.watch(key);
    const lease = this.presence.join(key, me);
    const present = await this.presence.members(null, key);
    const at = present.indexOf(me);
    if (at < 0 || at >= this.capacity) {
      lease.leave();
      mail.close();
      changes.close();
      return null;
    }
    return new RelayPeer(this, key, me, mail, changes, lease, present.length > 1);
  }
}

/**
 * `RelayPeer`: one peer of a room. `signals` yields `peerJoined` first
 * when someone was already there, then the other peers' signals and the
 * room's arrivals and departures as they happen.
 */
class RelayPeer {
  constructor(relay, key, me, mail, changes, lease, someone) {
    this.relay = relay;
    this.key = key;
    this.me = me;
    this.lease = lease;
    const items = someone ? [unitCase(0)] : [];
    const waiters = [];
    let open = 2;
    const push = (v) => {
      const w = waiters.shift();
      if (w) w(v);
      else items.push(v);
    };
    const pump = async (source, map) => {
      for (;;) {
        const v = await source.next();
        if (v === null) break;
        const s = map(v);
        if (s !== null) push(s);
      }
      if (--open === 0) push(null);
    };
    pump(mail, (e) => (e.from === me ? null : relay.signal.decode(e.signal, "")));
    pump(changes, (c) => (c._0 === me ? null : c.case === "joined" ? unitCase(0) : c.case === "left" ? unitCase(1) : null));
    this.signals = {
      next() {
        if (items.length) return Promise.resolve(items[0] === null ? null : items.shift());
        return new Promise((resolve) => waiters.push(resolve));
      },
      close() {
        mail.close();
        changes.close();
      },
      [Symbol.asyncIterator]() {
        return { next: () => this.next().then((v) => (v === null ? { done: true, value: undefined } : { done: false, value: v })) };
      },
    };
  }

  /** Forwards `signal` to the other peers of the room. */
  forward(signal) {
    this.relay.mail.publish(this.key, { from: this.me, signal: this.relay.signal.encode(signal) });
  }

  /** Leaves the room now; idempotent. */
  leave() {
    this.lease.leave();
  }
}

// ------------------------------------------------------------- ICE servers

/**
 * SHA-1 (FIPS 180-4) of `m`. Used only for coturn's credentials, which
 * require HMAC-SHA1; `k.crypto` doesn't offer it.
 * @param {Uint8Array} m
 * @returns {Uint8Array}
 */
export function sha1(m) {
  const n = (((m.length + 8) >> 6) + 1) * 64;
  const b = new Uint8Array(n);
  b.set(m);
  b[m.length] = 0x80;
  const bits = m.length * 8;
  const v = new DataView(b.buffer);
  v.setUint32(n - 8, Math.floor(bits / 2 ** 32));
  v.setUint32(n - 4, bits >>> 0);
  let [h0, h1, h2, h3, h4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
  const w = new Uint32Array(80);
  const rol = (x, k) => (x << k) | (x >>> (32 - k));
  for (let off = 0; off < n; off += 64) {
    for (let i = 0; i < 16; i++) w[i] = v.getUint32(off + i * 4);
    for (let i = 16; i < 80; i++) w[i] = rol(w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16], 1);
    let [a, bb, c, d, e] = [h0, h1, h2, h3, h4];
    for (let i = 0; i < 80; i++) {
      const f = i < 20 ? (bb & c) | (~bb & d) : i < 40 ? bb ^ c ^ d : i < 60 ? (bb & c) | (bb & d) | (c & d) : bb ^ c ^ d;
      const k = i < 20 ? 0x5a827999 : i < 40 ? 0x6ed9eba1 : i < 60 ? 0x8f1bbcdc : 0xca62c1d6;
      const t = (rol(a, 5) + f + e + k + w[i]) >>> 0;
      [e, d, c, bb, a] = [d, c, rol(bb, 30) >>> 0, a, t];
    }
    h0 = (h0 + a) >>> 0;
    h1 = (h1 + bb) >>> 0;
    h2 = (h2 + c) >>> 0;
    h3 = (h3 + d) >>> 0;
    h4 = (h4 + e) >>> 0;
  }
  const out = new Uint8Array(20);
  const o = new DataView(out.buffer);
  [h0, h1, h2, h3, h4].forEach((h, i) => o.setUint32(i * 4, h));
  return out;
}

/**
 * HMAC-SHA1 (RFC 2104) of `message` under `key`.
 * @param {Uint8Array} key
 * @param {Uint8Array} message
 * @returns {Uint8Array}
 */
export function hmacSha1(key, message) {
  const k = new Uint8Array(64);
  k.set(key.length > 64 ? sha1(key) : key);
  const inner = new Uint8Array(64 + message.length);
  const outer = new Uint8Array(64 + 20);
  for (let i = 0; i < 64; i++) {
    inner[i] = k[i] ^ 0x36;
    outer[i] = k[i] ^ 0x5c;
  }
  inner.set(message, 64);
  outer.set(sha1(inner), 64);
  return sha1(outer);
}

/**
 * coturn's TURN REST credential (`--use-auth-secret`): the user name
 * `<expiry>:<user>`, with the expiry in Unix seconds, and the credential
 * the Base64 HMAC-SHA1 of the user name under the shared secret.
 * @param {string} secret
 * @param {string} user
 * @param {number} expiresAt Unix seconds
 * @returns {{username: string, credential: string}}
 */
export function turnCredential(secret, user, expiresAt) {
  const username = `${expiresAt}:${user}`;
  return { username, credential: bytesBase64(hmacSha1(utf8(secret), utf8(username))) };
}

/**
 * `k.rtc.IceServers`: no servers unless `TurnConfig` names some; then one
 * entry with every URL and a credential that expires `validFor`
 * milliseconds from now on `clock()`.
 * @param {() => {urls: string[], secret: {reveal(): string}|null}} turn the container's `TurnConfig`
 * @param {() => {now: {ms: number}}} clock the request's clock
 */
export function iceServers(turn, clock) {
  return {
    async forClient(_c, user, validFor) {
      const config = turn();
      const urls = config?.urls ?? [];
      if (urls.length === 0 || !config.secret) return [];
      const expiresAt = Math.floor((clock().now.ms + validFor) / 1000);
      return [{ urls: urls.map(String), ...turnCredential(String(config.secret.reveal()), user, expiresAt) }];
    },
  };
}
