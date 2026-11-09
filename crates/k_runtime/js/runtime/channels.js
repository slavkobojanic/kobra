/**
 * @module k:/runtime/channels.js
 *
 * The frames of `k.channel.v1`, the channels' WebSocket protocol (docs/design/44-channels.md,
 * "The wire"), as pure encode and decode; nothing opens a socket yet. The Rust front has the same
 * codec (`k_server::channels`), and both are tested against `tests/channel-frames.json`.
 *
 * A frame is `{ kind, ch, … }` with the kind's fields: `open` (`method`, `request`), `opened`
 * (`resume`), `refused` (`error`), `msg` (`seq`, `value`), `ack` (`seq`, `error` when refused),
 * `rcvd` (`seq`), `resume` (`token`, `received`, `sent`), `close` (`reason`), `session` (`user`,
 * `null` for a guest; no `ch`) and `beat` (no fields). Payloads are already-encoded messages of
 * the app's protocols: JSON values in JSON frames (development) and `Uint8Array`s in binary
 * frames (production, the Protobuf message `k.channel.v1.Frame`, documented in the Rust module).
 * Decoding throws an `Error` for a malformed frame.
 */

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
