/**
 * @module k:runtime/crypto
 *
 * `k.crypto` on both sides (docs/design/33-crypto.md): `Hash`, `Hmac`,
 * `SecureRandom`, `ConstantTime` and the key types (`AesGcmKey` and
 * `Sealed`, HKDF and PBKDF2, key wrapping, Ed25519 and ECDSA P-256
 * signatures, X25519 agreement and `Envelope`).
 *
 * One internal interface, two backends ("Implementation"): `subtle` is the
 * browser's Web Crypto, `ops` is the server's Rust ops (`k_server`
 * `crypto.rs`, `ring` and `x25519-dalek`), which `app.js` hands over with
 * {@link useOps}. The browser picks `subtle`; the server, and `k test`
 * (whose browser container has no Web Crypto), pick `ops`. No primitive is
 * implemented here: HPKE (RFC 9180), HKDF's expand step and the wire
 * formats are composed from the backend's calls, the same way for both.
 *
 * Values: a secret or private key is a {@link Key} (printed as
 * `AesGcmKey(•••)`, never serialized); `Exportable<K>` is the same key,
 * generated or unwrapped as exportable; public keys and signatures are
 * their base64url text; `Sealed`, `Envelope`, `WrappedKey`, `EnvelopedKey`
 * and `PassphraseSalt` are their text forms, so `==`, hashing, `Codable`
 * and interpolation need nothing more. Each export is the body of a
 * `@Native` stdlib member, named `Owner$member` like the natives it
 * implements (codegen.md §16); in the browser this file is a chunk of its
 * own, shipped only to pages that use one of them, and the server has
 * them in `$s.natives`.
 */

import { KPanic, unitCase } from "./values.js";
import { DecodeFailure, Secret, bytesUtf8, codecs, stringify, utf8 } from "./std.js";

// ------------------------------------------------------------------ bytes

/** No bytes. */
const EMPTY = new Uint8Array(0);

/**
 * `parts` (byte arrays or arrays of numbers) joined.
 * @param {...(Uint8Array|number[])} parts
 * @returns {Uint8Array}
 */
function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/**
 * `value` as `length` big-endian bytes (RFC 9180's I2OSP).
 * @param {number} value
 * @param {number} length
 */
function i2osp(value, length) {
  const out = new Uint8Array(length);
  for (let i = length - 1, v = value; i >= 0; i--, v = Math.floor(v / 256)) out[i] = v & 255;
  return out;
}

/** The URL-safe base64 alphabet, by digit value. */
const B64URL = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/**
 * `b` as unpadded base64url (RFC 4648 §5).
 * @param {Uint8Array} b
 * @returns {string}
 */
export function b64url(b) {
  let s = "";
  for (let i = 0; i < b.length; i += 3) {
    const n = (b[i] << 16) | ((b[i + 1] ?? 0) << 8) | (b[i + 2] ?? 0);
    s += B64URL[n >> 18] + B64URL[(n >> 12) & 63];
    if (i + 1 < b.length) s += B64URL[(n >> 6) & 63];
    if (i + 2 < b.length) s += B64URL[n & 63];
  }
  return s;
}

/**
 * Unpadded base64url text as bytes, or `null` for anything else
 * (another alphabet, padding, an impossible length, nonzero spare bits),
 * so every value has exactly one text form.
 * @param {unknown} text
 * @returns {Uint8Array|null}
 */
export function fromB64url(text) {
  if (typeof text !== "string" || text.length % 4 === 1) return null;
  const out = new Uint8Array(Math.floor((text.length * 3) / 4));
  let bits = 0;
  let value = 0;
  let at = 0;
  for (const ch of text) {
    const d = B64URL.indexOf(ch);
    if (d < 0) return null;
    value = ((value << 6) | d) & 0xffffff;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out[at++] = (value >> bits) & 255;
    }
  }
  return (value & ((1 << bits) - 1)) === 0 ? out : null;
}

/** `b` as lowercase hex. */
const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

/** `x` as bytes: `Bytes` are a `Uint8Array` already. */
const bytes = (x) => (x instanceof Uint8Array ? x : Uint8Array.from(x ?? []));

/** The value inside a `Secret`, for `Secret<K>`'s forwarded operations. */
const reveal = (x) => (x instanceof Secret ? x.reveal() : x);

/** A panic naming what went wrong; crypto failures that aren't the caller's input. */
const fail = (message) => {
  throw new KPanic(message);
};

// ----------------------------------------------------------- randomness

/**
 * `count` bytes from the secure source: the browser's (in `k test`, the
 * test's seeded ChaCha20 stream: 21-testing.md, "Running tests"), else the
 * server's operating system.
 * @param {number} count
 * @returns {Uint8Array}
 */
export function random(count) {
  const c = globalThis.crypto;
  if (typeof c?.getRandomValues === "function") return c.getRandomValues(new Uint8Array(count));
  return new Uint8Array(rawOps().op_k_crypto_random(count));
}

/** Set by `kit.js`: `k test` is running (33-crypto.md, "Testing"). */
let testing = false;

/**
 * Marks the process as a `k test` run: `PassphraseSalt.generate()` records
 * 1,000 iterations, and such salts parse.
 * @param {boolean} on
 */
export function setTesting(on) {
  testing = on;
}

// ---------------------------------------------------------------- keys

/**
 * A secret or private key, or a `SharedSecret`: what the backend needs
 * (the raw secret on `ops`, a `CryptoKey` on `subtle`) and the raw public
 * key, none of it enumerable, so a key never serializes or prints.
 */
export class Key {
  /**
   * @param {string} type the K type's name
   * @param {{secret?: Uint8Array|null, handle?: CryptoKey|null, pub?: Uint8Array|null, exportable?: boolean}} parts
   */
  constructor(type, parts) {
    for (const [k, v] of Object.entries({ type, secret: null, handle: null, pub: null, exportable: false, ...parts })) {
      Object.defineProperty(this, k, { value: v, enumerable: false });
    }
  }

  /** The redacted text, `AesGcmKey(•••)`. */
  toString() {
    return `${this.type}(•••)`;
  }

  /** The redacted JSON form. */
  toJSON() {
    return this.toString();
  }
}

/** The private key types, and the fixed PKCS #8 prefix of each raw form for Web Crypto. */
const PRIVATE = {
  Ed25519PrivateKey: { pkcs8: "302e020100300506032b657004220420", alg: { name: "Ed25519" }, use: ["sign"], pair: ["sign", "verify"] },
  X25519PrivateKey: { pkcs8: "302e020100300506032b656e04220420", alg: { name: "X25519" }, use: ["deriveBits"], pair: ["deriveBits"] },
  EcdsaP256PrivateKey: {
    pkcs8: "308141020100301306072a8648ce3d020106082a8648ce3d030107042730250201010420",
    alg: { name: "ECDSA", namedCurve: "P-256" },
    use: ["sign"],
    pair: ["sign", "verify"],
  },
};

/** Two characters: one byte of hex. */
const PAIR = new RegExp("..", "g");

/** Hex as bytes (the fixed prefixes above). */
const fromHex = (h) => Uint8Array.from(h.match(PAIR) ?? [], (x) => parseInt(x, 16));

/** AES-GCM as Web Crypto names it. */
const AES = { name: "AES-GCM", length: 256 };

// ------------------------------------------------------------- backends

/** The server's ops, once `app.js` hands them over. */
let opsTable = null;

/**
 * Hands the server's ops to the `ops` backend (`app.js` and the tests).
 * @param {object} o the isolate's ops table
 */
export function useOps(o) {
  opsTable = o;
}

/** The server's ops, or a panic where there are none. */
function rawOps() {
  return opsTable ?? fail("k.crypto has no backend here: neither Web Crypto nor the server's ops");
}

/** `null` stays `null`; anything else becomes a `Uint8Array`. */
const orNull = (x) => (x === null || x === undefined ? null : new Uint8Array(x));

/**
 * The `ops` backend: the server's Rust ops. Every call is synchronous
 * except PBKDF2, which runs off the worker's thread.
 */
export const ops = {
  name: "ops",
  digest: (bits, data) => new Uint8Array(rawOps().op_k_crypto_digest(bits, data)),
  hmac: (bits, key, data) => new Uint8Array(rawOps().op_k_hmac(bits, key, data)),
  hkdf: (ikm, salt, info, length) => orNull(rawOps().op_k_crypto_hkdf(ikm, salt, info, length)),
  pbkdf2: async (passphrase, salt, iterations) => orNull(await rawOps().op_k_crypto_pbkdf2(passphrase, salt, iterations)),
  aesSeal: (key, nonce, aad, plaintext) => orNull(rawOps().op_k_crypto_aes_seal(key.secret, nonce, aad, plaintext)) ?? fail("AES-GCM sealing failed"),
  aesOpen: (key, nonce, aad, sealed) => orNull(rawOps().op_k_crypto_aes_open(key.secret, nonce, aad, sealed)),
  /**
   * The key of `type` with raw secret `raw`, or `null` when `raw` isn't
   * one (a wrong length, a P-256 scalar out of range). A P-256 key is its
   * scalar, or its wrapped form, the scalar and the point's `x ‖ y`, which
   * must agree.
   */
  fromRaw(type, raw, exportable) {
    const o = rawOps();
    let secret = bytes(raw);
    if (type === "AesGcmKey") return secret.length === 32 || secret.length === 16 ? new Key(type, { secret, exportable }) : null;
    let point = null;
    if (type === "EcdsaP256PrivateKey" && secret.length === 96) {
      point = secret.subarray(32);
      secret = secret.slice(0, 32);
    }
    const pub =
      type === "Ed25519PrivateKey"
        ? o.op_k_crypto_ed25519_public(secret)
        : type === "EcdsaP256PrivateKey"
          ? o.op_k_crypto_p256_public(secret)
          : o.op_k_crypto_x25519_public(secret);
    if (pub === null || pub === undefined) return null;
    if (point !== null && hex(point) !== hex(new Uint8Array(pub).subarray(1))) return null;
    return new Key(type, { secret, pub: new Uint8Array(pub), exportable });
  },
  generate(type, exportable) {
    for (;;) {
      const k = ops.fromRaw(type, random(32), exportable);
      if (k !== null) return k;
    }
  },
  exportRaw: (key) => key.secret,
  /** The form a key is wrapped in: its raw form, and a P-256 key's point too. */
  exportWrap: (key) => (key.type === "EcdsaP256PrivateKey" ? concat(key.secret, key.pub.subarray(1)) : key.secret),
  sign(key, message) {
    const o = rawOps();
    const s = key.type === "Ed25519PrivateKey" ? o.op_k_crypto_ed25519_sign(key.secret, message) : o.op_k_crypto_p256_sign(key.secret, message);
    return orNull(s) ?? fail("signing failed");
  },
  verify(type, pub, sig, message) {
    const o = rawOps();
    return type === "Ed25519PublicKey" ? o.op_k_crypto_ed25519_verify(pub, sig, message) : o.op_k_crypto_p256_verify(pub, sig, message);
  },
  agree: (key, pub) => orNull(rawOps().op_k_crypto_x25519_agree(key.secret, pub)),
};

/** The browser's `crypto.subtle`. */
const S = () => globalThis.crypto.subtle;

/** Public keys Web Crypto imported, by type and raw form. */
const imported = new Map();

/**
 * The `CryptoKey` of public key `pub` of `type` (a public key type's name),
 * imported on first use and cached.
 */
async function publicKey(type, pub) {
  const id = `${type}:${hex(pub)}`;
  let k = imported.get(id);
  if (!k) {
    const alg = type === "Ed25519PublicKey" ? { name: "Ed25519" } : type === "X25519PublicKey" ? { name: "X25519" } : PRIVATE.EcdsaP256PrivateKey.alg;
    k = await S().importKey("raw", pub, alg, true, type === "X25519PublicKey" ? [] : ["verify"]);
    if (imported.size > 256) imported.clear();
    imported.set(id, k);
  }
  return k;
}

/** An `ArrayBuffer` result as bytes. */
const u8 = (b) => new Uint8Array(b);

/** The `subtle` backend: the browser's Web Crypto. */
export const subtle = {
  name: "subtle",
  digest: async (bits, data) => u8(await S().digest(bits === 512 ? "SHA-512" : "SHA-256", data)),
  async hmac(bits, key, data) {
    // An empty key is the zero block, as HMAC pads it (Web Crypto refuses empty keys).
    const k = await S().importKey("raw", key.length ? key : new Uint8Array(32), { name: "HMAC", hash: bits === 512 ? "SHA-512" : "SHA-256" }, false, ["sign"]);
    return u8(await S().sign("HMAC", k, data));
  },
  async hkdf(ikm, salt, info, length) {
    if (length > 255 * 32) return null;
    const k = await S().importKey("raw", ikm, "HKDF", false, ["deriveBits"]);
    return u8(await S().deriveBits({ name: "HKDF", hash: "SHA-256", salt, info }, k, length * 8));
  },
  async pbkdf2(passphrase, salt, iterations) {
    if (iterations < 1) return null;
    const k = await S().importKey("raw", passphrase, "PBKDF2", false, ["deriveBits"]);
    return u8(await S().deriveBits({ name: "PBKDF2", hash: "SHA-256", salt, iterations }, k, 256));
  },
  aesSeal: async (key, nonce, aad, plaintext) => u8(await S().encrypt({ name: "AES-GCM", iv: nonce, additionalData: aad }, key.handle, plaintext)),
  async aesOpen(key, nonce, aad, sealed) {
    try {
      return u8(await S().decrypt({ name: "AES-GCM", iv: nonce, additionalData: aad }, key.handle, sealed));
    } catch {
      return null;
    }
  },
  async fromRaw(type, raw, exportable) {
    try {
      if (type === "AesGcmKey") {
        if (raw.length !== 32 && raw.length !== 16) return null;
        return new Key(type, { handle: await S().importKey("raw", raw, "AES-GCM", exportable, ["encrypt", "decrypt"]), exportable });
      }
      if (type === "EcdsaP256PrivateKey") {
        // Web Crypto imports a P-256 private key only with its point: the
        // wrapped form, `d ‖ x ‖ y`.
        if (raw.length !== 96) return null;
        const part = (a, b) => b64url(raw.subarray(a, b));
        const jwk = { kty: "EC", crv: "P-256", d: part(0, 32), x: part(32, 64), y: part(64, 96), ext: exportable };
        const handle = await S().importKey("jwk", jwk, PRIVATE[type].alg, exportable, ["sign"]);
        return new Key(type, { handle, pub: concat([4], raw.subarray(32)), exportable });
      }
      if (raw.length !== 32) return null;
      const p = PRIVATE[type];
      const pkcs8 = concat(fromHex(p.pkcs8), raw);
      // Web Crypto can't give a private key's public half; an exportable
      // copy's JWK has it.
      const open = await S().importKey("pkcs8", pkcs8, p.alg, true, p.use);
      const jwk = await S().exportKey("jwk", open);
      const pub = fromB64url(jwk.x);
      const handle = exportable ? open : await S().importKey("pkcs8", pkcs8, p.alg, false, p.use);
      return new Key(type, { handle, pub, exportable });
    } catch {
      return null;
    }
  },
  async generate(type, exportable) {
    if (type === "AesGcmKey") return new Key(type, { handle: await S().generateKey(AES, exportable, ["encrypt", "decrypt"]), exportable });
    const p = PRIVATE[type];
    const pair = await S().generateKey(p.alg, exportable, p.pair);
    return new Key(type, { handle: pair.privateKey, pub: u8(await S().exportKey("raw", pair.publicKey)), exportable });
  },
  async exportRaw(key) {
    if (key.type === "AesGcmKey") return u8(await S().exportKey("raw", key.handle));
    if (key.type === "EcdsaP256PrivateKey") return fromB64url((await S().exportKey("jwk", key.handle)).d);
    return u8(await S().exportKey("pkcs8", key.handle)).slice(-32);
  },
  /** The form a key is wrapped in: its raw form, and a P-256 key's point too. */
  async exportWrap(key) {
    const raw = await subtle.exportRaw(key);
    return key.type === "EcdsaP256PrivateKey" ? concat(raw, key.pub.subarray(1)) : raw;
  },
  async sign(key, message) {
    const alg = key.type === "Ed25519PrivateKey" ? "Ed25519" : { name: "ECDSA", hash: "SHA-256" };
    return u8(await S().sign(alg, key.handle, message));
  },
  async verify(type, pub, sig, message) {
    try {
      const alg = type === "Ed25519PublicKey" ? "Ed25519" : { name: "ECDSA", hash: "SHA-256" };
      return await S().verify(alg, await publicKey(type, pub), sig, message);
    } catch {
      return false;
    }
  },
  async agree(key, pub) {
    try {
      const shared = u8(await S().deriveBits({ name: "X25519", public: await publicKey("X25519PublicKey", pub) }, key.handle, 256));
      return shared.some((b) => b !== 0) ? shared : null;
    } catch {
      return null;
    }
  },
};

/**
 * The backend of this side: Web Crypto where there is one (browsers), the
 * ops otherwise (the server, and both containers of `k test`).
 */
export function backend() {
  return globalThis.crypto?.subtle ? subtle : ops;
}

/**
 * The synchronous backend, for the members that can't wait: parsing a
 * raw secret (server-only, since it takes a `Secret`) and `TestKeys`.
 */
function syncBackend() {
  return backend() === ops ? ops : fail("raw secret keys are parsed on the server");
}

// ---------------------------------------------------------- composites

/**
 * HKDF's expand step (RFC 5869 §2.3) from HMAC-SHA-256, for HPKE's
 * labeled expand, whose PRK Web Crypto can't take as HKDF input.
 */
async function expand(b, prk, info, length) {
  const out = new Uint8Array(length);
  let t = EMPTY;
  for (let i = 1, at = 0; at < length; i++) {
    t = await b.hmac(256, prk, concat(t, info, [i]));
    out.set(t.subarray(0, Math.min(t.length, length - at)), at);
    at += t.length;
  }
  return out;
}

/** RFC 9180's suite ids: the KEM's, and HPKE's for one AEAD. */
const KEM_SUITE = concat(utf8("KEM"), [0, 0x20]);
const hpkeSuite = (aead) => concat(utf8("HPKE"), [0, 0x20, 0, 1, 0, aead]);
const HPKE_V1 = utf8("HPKE-v1");

/** RFC 9180 §4's LabeledExtract (HKDF-Extract is HMAC with the salt as key). */
const labeledExtract = (b, suite, salt, label, ikm) => b.hmac(256, salt, concat(HPKE_V1, suite, utf8(label), ikm));

/** RFC 9180 §4's LabeledExpand. */
const labeledExpand = (b, suite, prk, label, info, length) => expand(b, prk, concat(i2osp(length, 2), HPKE_V1, suite, utf8(label), info), length);

/**
 * DHKEM(X25519, HKDF-SHA256)'s shared secret from the DH output and the
 * two public keys (RFC 9180 §4.1, ExtractAndExpand).
 */
async function kemShared(b, dh, enc, pkR) {
  const prk = await labeledExtract(b, KEM_SUITE, EMPTY, "eae_prk", dh);
  return labeledExpand(b, KEM_SUITE, prk, "shared_secret", concat(enc, pkR), 32);
}

/**
 * The base-mode key schedule (RFC 9180 §5.1): the AEAD key (`aead` 1:
 * AES-128-GCM, 2: AES-256-GCM) and the base nonce.
 */
async function keySchedule(b, aead, shared, info) {
  const suite = hpkeSuite(aead);
  const pskIdHash = await labeledExtract(b, suite, EMPTY, "psk_id_hash", EMPTY);
  const infoHash = await labeledExtract(b, suite, EMPTY, "info_hash", info);
  const context = concat([0], pskIdHash, infoHash);
  const secret = await labeledExtract(b, suite, shared, "secret", EMPTY);
  const key = await labeledExpand(b, suite, secret, "key", context, aead === 1 ? 16 : 32);
  const nonce = await labeledExpand(b, suite, secret, "base_nonce", context, 12);
  return { key, nonce };
}

/**
 * HPKE base mode, single shot (RFC 9180 §6.1): seals `plaintext` to
 * `pkR`, with a fresh ephemeral key unless `ephemeral` (a raw secret, for
 * the RFC's vectors) is given. Returns `enc`, the ciphertext and the
 * intermediate values the vectors check.
 */
export async function hpkeSeal(b, pkR, info, aad, plaintext, aead = 2, ephemeral = null) {
  const e = ephemeral ? await b.fromRaw("X25519PrivateKey", ephemeral, false) : await b.generate("X25519PrivateKey", false);
  const dh = await b.agree(e, pkR);
  if (dh === null) fail("the recipient's X25519 public key has small order");
  const shared = await kemShared(b, dh, e.pub, pkR);
  const { key, nonce } = await keySchedule(b, aead, shared, info);
  const k = await b.fromRaw("AesGcmKey", key, false);
  return { enc: e.pub, shared, key, nonce, ct: await b.aesSeal(k, nonce, aad, plaintext) };
}

/**
 * Opens HPKE base mode's single-shot `ct` with private key `sk` (a
 * {@link Key}); `null` when it isn't authentic.
 */
export async function hpkeOpen(b, sk, enc, info, aad, ct, aead = 2) {
  const dh = await b.agree(sk, enc);
  if (dh === null) return null;
  const shared = await kemShared(b, dh, enc, sk.pub);
  const { key, nonce } = await keySchedule(b, aead, shared, info);
  return b.aesOpen(await b.fromRaw("AesGcmKey", key, false), nonce, aad, ct);
}

// --------------------------------------------------------- wire formats

/** The version byte of every format here. */
const V1 = 1;

/**
 * The associated data of each AES-GCM format: a tag byte (`Sealed` 1,
 * `WrappedKey` 2) and its context, so a ciphertext of one format never
 * opens as another.
 */
const aadOf = (tag, context) => concat([tag], context);

/** The text prefixes of the formats (33-crypto.md, "Wire format"). */
const PREFIX = { sealed: "k1.", envelope: "k1e.", wrapped: "k1w.", enveloped: "k1k." };

/** The smallest valid `Sealed` or `WrappedKey`: version, nonce, tag. */
const SEALED_MIN = 1 + 12 + 16;

/** The smallest valid `Envelope` or `EnvelopedKey`: version, `enc`, tag. */
const ENVELOPE_MIN = 1 + 32 + 16;

/**
 * Text `prefix` + base64url of `b`, when `b` is a version-1 value at
 * least `min` bytes long; else `null`.
 */
function wire(prefix, b, min) {
  const x = b instanceof Uint8Array ? b : null;
  return x && x.length >= min && x[0] === V1 ? prefix + b64url(x) : null;
}

/** The bytes of wire text `text` with `prefix`, or `null`. */
function unwire(prefix, text, min) {
  if (typeof text !== "string" || !text.startsWith(prefix)) return null;
  const b = fromB64url(text.slice(prefix.length));
  return b && b.length >= min && b[0] === V1 ? b : null;
}

/** The text of `value`, given the `Codable` codec of its type, as bytes to seal. */
const plaintextOf = (codec, value) => (codec === codecs.bytes ? bytes(value) : utf8(stringify(codec.encode(value))));

/** `OpenError.authenticationFailed`. */
const AUTHENTICATION_FAILED = unitCase(0);

/**
 * The value of authentic `plaintext`: the bytes themselves for `Bytes`,
 * else its JSON decoded by `codec`, throwing `OpenError.undecodable(…)`
 * (`DecodingError.malformed` or `.mismatch`) when it isn't a `T`.
 */
function valueOf(codec, plaintext) {
  if (codec === codecs.bytes) return plaintext;
  let j;
  try {
    j = JSON.parse(bytesUtf8(plaintext) ?? "\u0000");
  } catch (e) {
    throw { $: 1, _0: { $: 0, message: String(e?.message ?? e) } };
  }
  try {
    return codec.decode(j, "");
  } catch (e) {
    if (e instanceof DecodeFailure) throw { $: 1, _0: { $: 1, path: e.path || "$", message: e.message } };
    throw e;
  }
}

/** AES-GCM under `key` with a fresh nonce: the version-1 `Sealed` layout. */
async function sealBytes(b, key, aad, plaintext) {
  const nonce = random(12);
  return concat([V1], nonce, await b.aesSeal(key, nonce, aad, plaintext));
}

/** Opens the `Sealed` layout `w`, throwing `.authenticationFailed`. */
async function openBytes(b, key, aad, w) {
  const pt = await b.aesOpen(key, w.subarray(1, 13), aad, w.subarray(13));
  if (pt === null) throw AUTHENTICATION_FAILED;
  return pt;
}

/** The HPKE `info` of `Envelope` and `EnvelopedKey`. */
const ENVELOPE_INFO = utf8("k.crypto.Envelope");
const ENVELOPED_KEY_INFO = utf8("k.crypto.EnvelopedKey");

/** HPKE to `pub` in the version-1 envelope layout: version, `enc`, ciphertext. */
async function envelopeBytes(b, pub, info, aad, plaintext) {
  const { enc, ct } = await hpkeSeal(b, pub, info, aad, plaintext);
  return concat([V1], enc, ct);
}

/** Opens the envelope layout `w` with `key`, throwing `.authenticationFailed`. */
async function openEnvelope(b, key, info, aad, w) {
  const pt = await hpkeOpen(b, key, w.subarray(1, 33), info, aad, w.subarray(33));
  if (pt === null) throw AUTHENTICATION_FAILED;
  return pt;
}

/**
 * A codec for a value held as its text, valid when `parse` accepts it;
 * `key` makes maps keyed by it JSON objects.
 */
const textCodec = (parse, what) => ({
  key: true,
  encode: (v) => v,
  decode: (j, p) => (typeof j === "string" && parse(j) !== null ? j : decodeFailure(p, `expected ${what}`)),
});

/** Throws the codec failure at path `p`. */
const decodeFailure = (p, message) => {
  throw new DecodeFailure(p || "$", message);
};

// -------------------------------------------------------- Hash, Hmac, …

/** `Hash.sha256(data:)` and `Hash.sha512(data:)`. */
export const Hash = {
  sha256: async (_c, data) => backend().digest(256, bytes(data)),
  sha512: async (_c, data) => backend().digest(512, bytes(data)),
};

/** An HMAC key: `Bytes`, or a `Secret<String>` used as UTF-8. */
const macKey = (key) => (key instanceof Secret ? utf8(String(key.reveal())) : bytes(key));

/** `Hmac.sha256` and `Hmac.sha512`: `{bytes, hex}`, compared with {@link Mac$matches}. */
export const Hmac = {
  async sha256(_c, key, message) {
    const b = await backend().hmac(256, macKey(key), bytes(message));
    return { bytes: b, hex: hex(b) };
  },
  async sha512(_c, key, message) {
    const b = await backend().hmac(512, macKey(key), bytes(message));
    return { bytes: b, hex: hex(b) };
  },
};

/**
 * `a` and `b` compared in time that depends only on their lengths: the
 * server's Rust loop, or a JavaScript loop of the same shape in the
 * browser (33-crypto.md, "Comparing in constant time").
 */
function equalBytes(a, b) {
  if (backend() === ops) return rawOps().op_k_crypto_equals(a, b);
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a[i] ^ b[i];
  return d === 0;
}

/** `mac.matches(_:)`. */
export const Mac$matches = (mac, other) => other instanceof Uint8Array && equalBytes(mac.bytes, other);

/** `ConstantTime.equals(_:_:)`. */
export const ConstantTime = { equals: (a, b) => equalBytes(bytes(a), bytes(b)) };

/** `SecureRandom.bytes(count:)` and `.token(bytes:)` (URL-safe base64). */
export const SecureRandom = {
  bytes: (count) => random(count),
  token: (count) => b64url(random(count)),
};

// ------------------------------------------------------------ AesGcmKey

/** The AES-GCM key of an operation's receiver: a key, or a `Secret` of one. */
const aesKey = (k) => reveal(k);

/** `AesGcmKey.generate()`. */
export const AesGcmKey$generate = async (_c) => backend().generate("AesGcmKey", false);

/** `AesGcmKey.generateExportable()`. */
export const AesGcmKey$generateExportable = async (_c) => backend().generate("AesGcmKey", true);

/** `AesGcmKey.parse(raw:)`: 32 secret bytes, non-exportable. */
export const AesGcmKey$parse = (raw) => {
  const r = reveal(raw);
  return r instanceof Uint8Array && r.length === 32 ? syncBackend().fromRaw("AesGcmKey", r, false) : null;
};

/**
 * HKDF input as bytes: a `SharedSecret`, or a `Secret` of bytes, of text
 * (UTF-8) or of an `AesGcmKey` (its raw bytes, never exposed).
 */
async function ikmOf(input) {
  if (input instanceof Key) return input.type === "AesGcmKey" ? backend().exportRaw(input) : input.secret;
  const v = reveal(input);
  if (v instanceof Key) {
    if (v.secret) return v.secret;
    return fail("a configured AesGcmKey's raw bytes are used on the server");
  }
  return typeof v === "string" ? utf8(v) : bytes(v);
}

/** `AesGcmKey.derive(from:salt:info:)`: HKDF-SHA256 to a non-exportable key. */
async function deriveKey(_c, input, salt, info) {
  const b = backend();
  const raw = await b.hkdf(await ikmOf(input), bytes(salt), utf8(String(info)), 32);
  return b.fromRaw("AesGcmKey", raw, false);
}

/** `AesGcmKey.derive(from:salt:info:)`. */
export const AesGcmKey$derive$from$salt$info = deriveKey;
/** `AesGcmKey.derive(from:salt:info:)`. */
export const AesGcmKey$derive$from$salt$info$1 = deriveKey;
/** `AesGcmKey.derive(from:salt:info:)`. */
export const AesGcmKey$derive$from$salt$info$2 = deriveKey;
/** `AesGcmKey.derive(from:salt:info:)`. */
export const AesGcmKey$derive$from$salt$info$3 = deriveKey;

/** `AesGcmKey.derive(fromPassphrase:salt:)`: PBKDF2-HMAC-SHA256 at the salt's work factor. */
export async function AesGcmKey$derive$fromPassphrase$salt(_c, passphrase, salt) {
  const s = parseSalt(salt) ?? fail("not a PassphraseSalt");
  const b = backend();
  const raw = await b.pbkdf2(utf8(String(passphrase)), s.salt, s.iterations);
  return b.fromRaw("AesGcmKey", raw ?? fail("PBKDF2 failed"), false);
}

/** `key.seal(_:context:)`: a `Sealed<T>` with a fresh random nonce. */
export async function AesGcmKey$seal(key, _c, codec, value, context) {
  const b = backend();
  const w = await sealBytes(b, aesKey(key), aadOf(1, bytes(context ?? EMPTY)), plaintextOf(codec, value));
  return PREFIX.sealed + b64url(w);
}

/** `key.open(_:context:)`: the value, or `OpenError`. */
export async function AesGcmKey$open(key, _c, codec, sealed, context) {
  const w = unwire(PREFIX.sealed, sealed, SEALED_MIN);
  if (w === null) throw AUTHENTICATION_FAILED;
  return valueOf(codec, await openBytes(backend(), aesKey(key), aadOf(1, bytes(context ?? EMPTY)), w));
}

/**
 * `key.wrap(_:)`: an exportable key's raw form (a P-256 key's with its
 * point, which Web Crypto needs to import it) sealed with the key type's
 * name as context.
 */
export async function AesGcmKey$wrap(key, _c, exportable) {
  const b = backend();
  const k = reveal(exportable);
  if (!(k instanceof Key) || !k.exportable) fail("only an Exportable key can be wrapped");
  const w = await sealBytes(b, aesKey(key), aadOf(2, utf8(k.type)), await b.exportWrap(k));
  return PREFIX.wrapped + b64url(w);
}

/** Unwraps `wrapped` as a key of `type` (from the static type, never the wire). */
async function unwrapKey(key, type, wrapped, exportable) {
  const b = backend();
  const w = unwire(PREFIX.wrapped, wrapped, SEALED_MIN);
  if (w === null) throw AUTHENTICATION_FAILED;
  const raw = await openBytes(b, aesKey(key), aadOf(2, utf8(type)), w);
  return (await b.fromRaw(type, raw, exportable)) ?? fail(`the wrapped ${type} is malformed`);
}

/** `key.unwrap(_:)`: the key, non-exportable. */
export const AesGcmKey$unwrap = (key, _c, type, wrapped) => unwrapKey(key, type, wrapped, false);

/** `key.unwrapExportable(_:)`. */
export const AesGcmKey$unwrapExportable = (key, _c, type, wrapped) => unwrapKey(key, type, wrapped, true);

// ------------------------------------------------- Exportable, Sealed, …

/** `exportable.key`: the same key; only the type forgets that it can leave. */
export const Exportable$key = (e) => e;

/** `exportable.exportRaw()`: the raw form as a `Secret<Bytes>` (server-only, as `Secret` is). */
export function Exportable$exportRaw(e) {
  const k = reveal(e);
  if (!k.secret) fail("a browser key's raw form is wrapped, not exported");
  return new Secret(k.secret);
}

/** `Sealed.bytes`. */
export const Sealed$bytes = (s) => unwire(PREFIX.sealed, s, SEALED_MIN);
/** `Sealed.parse(bytes:)`. */
export const Sealed$parse$bytes = (b) => wire(PREFIX.sealed, b, SEALED_MIN);
/** `Sealed.parse`. */
export const Sealed$parse = (text) => (unwire(PREFIX.sealed, text, SEALED_MIN) ? text : null);
/** The `Codable` codec of `Sealed`, which accepts only its text form. */
export const Sealed$codec = () => textCodec(Sealed$parse, "a sealed value");

/** `WrappedKey.parse`. */
export const WrappedKey$parse = (text) => (unwire(PREFIX.wrapped, text, SEALED_MIN) ? text : null);
/** The `Codable` codec of `WrappedKey`, which accepts only its text form. */
export const WrappedKey$codec = () => textCodec(WrappedKey$parse, "a wrapped key");

/** `Envelope.bytes`. */
export const Envelope$bytes = (s) => unwire(PREFIX.envelope, s, ENVELOPE_MIN);
/** `Envelope.parse(bytes:)`. */
export const Envelope$parse$bytes = (b) => wire(PREFIX.envelope, b, ENVELOPE_MIN);
/** `Envelope.parse`. */
export const Envelope$parse = (text) => (unwire(PREFIX.envelope, text, ENVELOPE_MIN) ? text : null);
/** The `Codable` codec of `Envelope`, which accepts only its text form. */
export const Envelope$codec = () => textCodec(Envelope$parse, "an envelope");

/** `EnvelopedKey.parse`. */
export const EnvelopedKey$parse = (text) => (unwire(PREFIX.enveloped, text, ENVELOPE_MIN) ? text : null);
/** The `Codable` codec of `EnvelopedKey`, which accepts only its text form. */
export const EnvelopedKey$codec = () => textCodec(EnvelopedKey$parse, "an enveloped key");

// -------------------------------------------------------- PassphraseSalt

/** The production work factor, and the floor below which a salt is refused outside tests. */
const ITERATIONS = 600000;
const MIN_ITERATIONS = 100000;
const TEST_ITERATIONS = 1000;

/** A `PassphraseSalt`'s text: the iterations and 16 salt bytes in base64url. */
const SALT = new RegExp("^pbkdf2-sha256\\$([1-9][0-9]{0,9})\\$([A-Za-z0-9_-]{22})$");

/**
 * A `PassphraseSalt`'s parts, or `null`: `pbkdf2-sha256$<iterations>$<16
 * salt bytes, base64url>`, with at least 100,000 iterations except in
 * `k test`.
 * @returns {{iterations: number, salt: Uint8Array}|null}
 */
function parseSalt(text) {
  const m = typeof text === "string" ? SALT.exec(text) : null;
  if (!m) return null;
  const iterations = Number(m[1]);
  const salt = fromB64url(m[2]);
  if (!salt || iterations > 0xffffffff || (iterations < MIN_ITERATIONS && !testing)) return null;
  return { iterations, salt };
}

/** `PassphraseSalt.generate()`: 16 random bytes at the current work factor (1,000 in `k test`). */
export const PassphraseSalt$generate = () => `pbkdf2-sha256$${testing ? TEST_ITERATIONS : ITERATIONS}$${b64url(random(16))}`;
/** `PassphraseSalt.iterations`. */
export const PassphraseSalt$iterations = (s) => parseSalt(s)?.iterations ?? 0;
/** `PassphraseSalt.parse`. */
export const PassphraseSalt$parse = (text) => (parseSalt(text) ? text : null);
/** The `Codable` codec of `PassphraseSalt`, which accepts only its text form. */
export const PassphraseSalt$codec = textCodec(PassphraseSalt$parse, "a passphrase salt");

// --------------------------------------------------------- public keys

/**
 * The low-order X25519 points (libsodium's list), compared with the top
 * bit cleared: `X25519PublicKey.parse` refuses them.
 */
const SMALL_ORDER = [
  "0000000000000000000000000000000000000000000000000000000000000000",
  "0100000000000000000000000000000000000000000000000000000000000000",
  "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800",
  "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f1157",
  "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
  "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
  "eeffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
];

/** Whether `raw` is a plausible public key of `type`, checked synchronously. */
function validPublic(type, raw) {
  if (!(raw instanceof Uint8Array)) return false;
  if (type === "EcdsaP256PublicKey") return raw.length === 65 && raw[0] === 4;
  if (raw.length !== 32) return false;
  if (type !== "X25519PublicKey") return true;
  const masked = raw.slice();
  masked[31] &= 0x7f;
  return !SMALL_ORDER.includes(hex(masked));
}

/** `parse(raw:)` of public key `type`: its text, or `null`. */
const parsePublic = (type) => (raw) => (validPublic(type, raw) ? b64url(raw) : null);

/** The raw form of public key text `k`. */
const rawPublic = (k) => fromB64url(k) ?? fail("not a public key");

/** `publicKey.fingerprint()`: the SHA-256 of the raw form, as hex. */
const fingerprint = async (k, _c) => hex(await backend().digest(256, rawPublic(k)));

/** The members every public key type has. */
function publicMembers(type) {
  const parse = parsePublic(type);
  return {
    parse,
    raw: rawPublic,
    fingerprint,
    codec: textCodec((t) => {
      const r = fromB64url(t);
      return r && parse(r);
    }, `a ${type}`),
  };
}

const ed = publicMembers("Ed25519PublicKey");
/** `Ed25519PublicKey.parse`. */
export const Ed25519PublicKey$parse = ed.parse;
/** `Ed25519PublicKey.raw`. */
export const Ed25519PublicKey$raw = ed.raw;
/** `Ed25519PublicKey.fingerprint`. */
export const Ed25519PublicKey$fingerprint = ed.fingerprint;
/** The `Codable` codec of `Ed25519PublicKey`, which accepts only its text form. */
export const Ed25519PublicKey$codec = ed.codec;
/** `publicKey.verify(_:of:)`: `false` for any invalid signature. */
export const Ed25519PublicKey$verify = async (k, _c, sig, message) => verify("Ed25519PublicKey", k, sig, message);

const p256 = publicMembers("EcdsaP256PublicKey");
/** `EcdsaP256PublicKey.parse`. */
export const EcdsaP256PublicKey$parse = p256.parse;
/** `EcdsaP256PublicKey.raw`. */
export const EcdsaP256PublicKey$raw = p256.raw;
/** `EcdsaP256PublicKey.fingerprint`. */
export const EcdsaP256PublicKey$fingerprint = p256.fingerprint;
/** The `Codable` codec of `EcdsaP256PublicKey`, which accepts only its text form. */
export const EcdsaP256PublicKey$codec = p256.codec;
/** `EcdsaP256PublicKey.verify`. */
export const EcdsaP256PublicKey$verify = async (k, _c, sig, message) => verify("EcdsaP256PublicKey", k, sig, message);

const x = publicMembers("X25519PublicKey");
/** `X25519PublicKey.parse`. */
export const X25519PublicKey$parse = x.parse;
/** `X25519PublicKey.raw`. */
export const X25519PublicKey$raw = x.raw;
/** `X25519PublicKey.fingerprint`. */
export const X25519PublicKey$fingerprint = x.fingerprint;
/** The `Codable` codec of `X25519PublicKey`, which accepts only its text form. */
export const X25519PublicKey$codec = x.codec;

/** Verifies signature text `sig` of `message` by public key text `k`. */
async function verify(type, k, sig, message) {
  const s = fromB64url(sig);
  const p = fromB64url(k);
  return s !== null && p !== null && (await backend().verify(type, p, s, bytes(message))) === true;
}

/** `recipient.seal(_:context:)`: an `Envelope<T>` (HPKE base mode). */
export async function X25519PublicKey$seal(k, _c, codec, value, context) {
  const w = await envelopeBytes(backend(), rawPublic(k), ENVELOPE_INFO, bytes(context ?? EMPTY), plaintextOf(codec, value));
  return PREFIX.envelope + b64url(w);
}

/** `recipient.wrap(_:)`: an `EnvelopedKey<K>`, the key type's name authenticated. */
export async function X25519PublicKey$wrap(k, _c, exportable) {
  const b = backend();
  const key = reveal(exportable);
  if (!(key instanceof Key) || !key.exportable) fail("only an Exportable key can be wrapped");
  const w = await envelopeBytes(b, rawPublic(k), ENVELOPED_KEY_INFO, utf8(key.type), await b.exportWrap(key));
  return PREFIX.enveloped + b64url(w);
}

// ------------------------------------------------------- signatures

/** The `bytes` and `parse(bytes:)` of a 64-byte signature type, and its codec. */
const signature = (what) => ({
  bytes: (s) => fromB64url(s) ?? fail("not a signature"),
  parse: (b) => (b instanceof Uint8Array && b.length === 64 ? b64url(b) : null),
  codec: textCodec((t) => (fromB64url(t)?.length === 64 ? t : null), what),
});

const edSig = signature("an Ed25519 signature");
/** `Ed25519Signature.bytes`. */
export const Ed25519Signature$bytes = edSig.bytes;
/** `Ed25519Signature.parse`. */
export const Ed25519Signature$parse = edSig.parse;
/** The `Codable` codec of `Ed25519Signature`, which accepts only its text form. */
export const Ed25519Signature$codec = edSig.codec;

const ecSig = signature("an ECDSA P-256 signature");
/** `EcdsaP256Signature.bytes`. */
export const EcdsaP256Signature$bytes = ecSig.bytes;
/** `EcdsaP256Signature.parse(bytes:)`. */
export const EcdsaP256Signature$parse$bytes = ecSig.parse;
/** The `Codable` codec of `EcdsaP256Signature`, which accepts only its text form. */
export const EcdsaP256Signature$codec = ecSig.codec;

/**
 * `EcdsaP256Signature.parse(der:)`: an ASN.1 `SEQUENCE { r INTEGER, s
 * INTEGER }` in DER, as the 64-byte `r ‖ s`; `null` for anything else.
 */
export function EcdsaP256Signature$parse$der(der) {
  const d = bytes(der);
  if (d[0] !== 0x30 || d[1] !== d.length - 2 || d[1] > 0x7f) return null;
  const out = new Uint8Array(64);
  let at = 2;
  for (const half of [0, 32]) {
    if (d[at] !== 2) return null;
    const len = d[at + 1];
    let v = d.subarray(at + 2, at + 2 + len);
    if (len === 0 || v.length !== len || (v[0] & 0x80) !== 0 || (len > 1 && v[0] === 0 && (v[1] & 0x80) === 0)) return null;
    if (v[0] === 0) v = v.subarray(1);
    if (v.length > 32) return null;
    out.set(v, half + 32 - v.length);
    at += 2 + len;
  }
  return at === d.length ? b64url(out) : null;
}

// ------------------------------------------------------ private keys

/** The members every private key type has. */
function privateMembers(type) {
  return {
    generate: async (_c) => backend().generate(type, false),
    generateExportable: async (_c) => backend().generate(type, true),
    parse: (raw) => {
      const r = reveal(raw);
      return r instanceof Uint8Array && r.length === 32 ? syncBackend().fromRaw(type, r, false) : null;
    },
    publicKey: (k) => b64url(reveal(k).pub),
  };
}

const edKey = privateMembers("Ed25519PrivateKey");
/** `Ed25519PrivateKey.generate`. */
export const Ed25519PrivateKey$generate = edKey.generate;
/** `Ed25519PrivateKey.generateExportable`. */
export const Ed25519PrivateKey$generateExportable = edKey.generateExportable;
/** `Ed25519PrivateKey.parse`. */
export const Ed25519PrivateKey$parse = edKey.parse;
/** `Ed25519PrivateKey.publicKey`. */
export const Ed25519PrivateKey$publicKey = edKey.publicKey;
/** `privateKey.sign(_:)`: deterministic. */
export const Ed25519PrivateKey$sign = async (k, _c, message) => b64url(await backend().sign(reveal(k), bytes(message)));

const ecKey = privateMembers("EcdsaP256PrivateKey");
/** `EcdsaP256PrivateKey.generate`. */
export const EcdsaP256PrivateKey$generate = ecKey.generate;
/** `EcdsaP256PrivateKey.generateExportable`. */
export const EcdsaP256PrivateKey$generateExportable = ecKey.generateExportable;
/** `EcdsaP256PrivateKey.parse`. */
export const EcdsaP256PrivateKey$parse = ecKey.parse;
/** `EcdsaP256PrivateKey.publicKey`. */
export const EcdsaP256PrivateKey$publicKey = ecKey.publicKey;
/** `privateKey.sign(_:)`: `r ‖ s`, with a random nonce. */
export const EcdsaP256PrivateKey$sign = async (k, _c, message) => b64url(await backend().sign(reveal(k), bytes(message)));

const xKey = privateMembers("X25519PrivateKey");
/** `X25519PrivateKey.generate`. */
export const X25519PrivateKey$generate = xKey.generate;
/** `X25519PrivateKey.generateExportable`. */
export const X25519PrivateKey$generateExportable = xKey.generateExportable;
/** `X25519PrivateKey.parse`. */
export const X25519PrivateKey$parse = xKey.parse;
/** `X25519PrivateKey.publicKey`. */
export const X25519PrivateKey$publicKey = xKey.publicKey;

/**
 * `privateKey.agree(with:)`: a `SharedSecret`. An all-zero result (a
 * small-order key, which `parse` refuses) is a panic.
 */
export async function X25519PrivateKey$agree(k, _c, theirs) {
  const shared = await backend().agree(reveal(k), rawPublic(theirs));
  return new Key("SharedSecret", { secret: shared ?? fail("X25519 agreement with a small-order public key") });
}

/** `privateKey.open(_:context:)`: an `Envelope<T>`'s value, or `OpenError`. */
export async function X25519PrivateKey$open(k, _c, codec, envelope, context) {
  const w = unwire(PREFIX.envelope, envelope, ENVELOPE_MIN);
  if (w === null) throw AUTHENTICATION_FAILED;
  return valueOf(codec, await openEnvelope(backend(), reveal(k), ENVELOPE_INFO, bytes(context ?? EMPTY), w));
}

/** `privateKey.unwrap(_:)`: an `EnvelopedKey<K>`'s key, non-exportable, as `type`. */
export async function X25519PrivateKey$unwrap(k, _c, type, enveloped) {
  const b = backend();
  const w = unwire(PREFIX.enveloped, enveloped, ENVELOPE_MIN);
  if (w === null) throw AUTHENTICATION_FAILED;
  const raw = await openEnvelope(b, reveal(k), ENVELOPED_KEY_INFO, utf8(type), w);
  return (await b.fromRaw(type, raw, false)) ?? fail(`the enveloped ${type} is malformed`);
}

// --------------------------------------------------------------- Hkdf

/** `Hkdf.sha256(from:salt:info:count:)`: raw output as a `Secret<Bytes>`. */
export const Hkdf = {
  async sha256(_c, input, salt, info, count) {
    const out = await backend().hkdf(await ikmOf(input), bytes(salt), utf8(String(info)), count);
    return new Secret(out ?? fail("HKDF output is at most 8160 bytes"));
  },
};

// ------------------------------------------------------------ TestKeys

/**
 * `k.test.TestKeys`: fixture keys by number, the same in every test. The
 * raw secret of key `n` of a kind is the SHA-256 of
 * `k.test.TestKeys/<kind>/<n>` (rehashed in the rare case it isn't a
 * valid P-256 scalar). Synchronous, on the `ops` backend.
 */
export const TestKeys = Object.fromEntries(
  [
    ["aesGcm", "AesGcmKey"],
    ["ed25519", "Ed25519PrivateKey"],
    ["ecdsaP256", "EcdsaP256PrivateKey"],
    ["x25519", "X25519PrivateKey"],
  ].map(([kind, type]) => [
    kind,
    (n) => {
      let seed = utf8(`k.test.TestKeys/${kind}/${n}`);
      for (;;) {
        seed = ops.digest(256, seed);
        const k = ops.fromRaw(type, seed, false);
        if (k !== null) return k;
      }
    },
  ]),
);
