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
 *
 * Also here: JWK and PEM for every key type, `RsaPublicKey` (base64url of
 * its SPKI DER), JWS and JWT with the algorithm taken from the key's type
 * (`Jws`, `Jwt`, `JwkSet`), configured keys ({@link keyFromConfig}) and
 * `BrowserKeyStore` (IndexedDB). DER and PEM are read and written here;
 * every signature and digest is the backend's.
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
  return new Uint8Array(rawOps().random(count));
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
 * Hands the server's ops to the `ops` backend (`app.js` and the tests):
 * the table of `crypto-ops.js`, which names the Rust ops, so no op name
 * ships in this module's browser chunk.
 * @param {object} o the table `crypto-ops.js` makes of the isolate's ops
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
  digest: (bits, data) => new Uint8Array(rawOps().digest(bits, data)),
  hmac: (bits, key, data) => new Uint8Array(rawOps().hmac(bits, key, data)),
  hkdf: (ikm, salt, info, length) => orNull(rawOps().hkdf(ikm, salt, info, length)),
  pbkdf2: async (passphrase, salt, iterations) => orNull(await rawOps().pbkdf2(passphrase, salt, iterations)),
  aesSeal: (key, nonce, aad, plaintext) => orNull(rawOps().aesSeal(key.secret, nonce, aad, plaintext)) ?? fail("AES-GCM sealing failed"),
  aesOpen: (key, nonce, aad, sealed) => orNull(rawOps().aesOpen(key.secret, nonce, aad, sealed)),
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
        ? o.ed25519Public(secret)
        : type === "EcdsaP256PrivateKey"
          ? o.p256Public(secret)
          : o.x25519Public(secret);
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
    const s = key.type === "Ed25519PrivateKey" ? o.ed25519Sign(key.secret, message) : o.p256Sign(key.secret, message);
    return orNull(s) ?? fail("signing failed");
  },
  verify(type, pub, sig, message) {
    const o = rawOps();
    return type === "Ed25519PublicKey" ? o.ed25519Verify(pub, sig, message) : o.p256Verify(pub, sig, message);
  },
  agree: (key, pub) => orNull(rawOps().x25519Agree(key.secret, pub)),
  rsaVerify: (n, e, sig, message, pss) => rawOps().rsaVerify(n, e, sig, message, pss) === true,
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
  async rsaVerify(n, e, sig, message, pss) {
    try {
      const name = pss ? "RSA-PSS" : "RSASSA-PKCS1-v1_5";
      const k = await S().importKey("jwk", { kty: "RSA", n: b64url(n), e: b64url(e) }, { name, hash: "SHA-256" }, false, ["verify"]);
      return await S().verify(pss ? { name, saltLength: 32 } : name, k, sig, message);
    } catch {
      return false;
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
  if (backend() === ops) return rawOps().equals(a, b);
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
export const AesGcmKey$parse$raw = (raw) => {
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
/** `Ed25519PublicKey.parse(raw:)`. */
export const Ed25519PublicKey$parse$raw = ed.parse;
/** `Ed25519PublicKey.raw`. */
export const Ed25519PublicKey$raw = ed.raw;
/** `Ed25519PublicKey.fingerprint`. */
export const Ed25519PublicKey$fingerprint = ed.fingerprint;
/** The `Codable` codec of `Ed25519PublicKey`, which accepts only its text form. */
export const Ed25519PublicKey$codec = ed.codec;
/** `publicKey.verify(_:of:)`: `false` for any invalid signature. */
export const Ed25519PublicKey$verify = async (k, _c, sig, message) => verify("Ed25519PublicKey", k, sig, message);

const p256 = publicMembers("EcdsaP256PublicKey");
/** `EcdsaP256PublicKey.parse(raw:)`. */
export const EcdsaP256PublicKey$parse$raw = p256.parse;
/** `EcdsaP256PublicKey.raw`. */
export const EcdsaP256PublicKey$raw = p256.raw;
/** `EcdsaP256PublicKey.fingerprint`. */
export const EcdsaP256PublicKey$fingerprint = p256.fingerprint;
/** The `Codable` codec of `EcdsaP256PublicKey`, which accepts only its text form. */
export const EcdsaP256PublicKey$codec = p256.codec;
/** `EcdsaP256PublicKey.verify`. */
export const EcdsaP256PublicKey$verify = async (k, _c, sig, message) => verify("EcdsaP256PublicKey", k, sig, message);

const x = publicMembers("X25519PublicKey");
/** `X25519PublicKey.parse(raw:)`. */
export const X25519PublicKey$parse$raw = x.parse;
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
/** `Ed25519PrivateKey.parse(raw:)`. */
export const Ed25519PrivateKey$parse$raw = edKey.parse;
/** `Ed25519PrivateKey.publicKey`. */
export const Ed25519PrivateKey$publicKey = edKey.publicKey;
/** `privateKey.sign(_:)`: deterministic. */
export const Ed25519PrivateKey$sign = async (k, _c, message) => b64url(await backend().sign(reveal(k), bytes(message)));

const ecKey = privateMembers("EcdsaP256PrivateKey");
/** `EcdsaP256PrivateKey.generate`. */
export const EcdsaP256PrivateKey$generate = ecKey.generate;
/** `EcdsaP256PrivateKey.generateExportable`. */
export const EcdsaP256PrivateKey$generateExportable = ecKey.generateExportable;
/** `EcdsaP256PrivateKey.parse(raw:)`. */
export const EcdsaP256PrivateKey$parse$raw = ecKey.parse;
/** `EcdsaP256PrivateKey.publicKey`. */
export const EcdsaP256PrivateKey$publicKey = ecKey.publicKey;
/** `privateKey.sign(_:)`: `r ‖ s`, with a random nonce. */
export const EcdsaP256PrivateKey$sign = async (k, _c, message) => b64url(await backend().sign(reveal(k), bytes(message)));

const xKey = privateMembers("X25519PrivateKey");
/** `X25519PrivateKey.generate`. */
export const X25519PrivateKey$generate = xKey.generate;
/** `X25519PrivateKey.generateExportable`. */
export const X25519PrivateKey$generateExportable = xKey.generateExportable;
/** `X25519PrivateKey.parse(raw:)`. */
export const X25519PrivateKey$parse$raw = xKey.parse;
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

// ---------------------------------------------------- DER, PEM and JWK

/**
 * The DER element at `at` of `b`: its tag and the bounds of its content,
 * or `null` when it isn't one (an indefinite or non-minimal length, or one
 * running past the end).
 * @returns {{tag: number, start: number, end: number}|null}
 */
function der(b, at) {
  if (at + 2 > b.length) return null;
  let len = b[at + 1];
  let p = at + 2;
  if (len & 0x80) {
    const n = len & 0x7f;
    if (n < 1 || n > 2 || p + n > b.length || b[p] === 0) return null;
    len = n === 1 ? b[p] : b[p] * 256 + b[p + 1];
    p += n;
    if (len < 128) return null;
  }
  return p + len <= b.length ? { tag: b[at], start: p, end: p + len } : null;
}

/** The elements inside `node` (a SEQUENCE's), or `null` when they don't tile it. */
function items(b, node) {
  const out = [];
  for (let at = node.start; at < node.end; ) {
    const c = der(b, at);
    if (c === null || c.end > node.end) return null;
    out.push(c);
    at = c.end;
  }
  return out;
}

/** The whole of `b` as one element with tag `tag`, or `null`. */
function only(b, tag) {
  const d = der(b, 0);
  return d && d.tag === tag && d.end === b.length ? d : null;
}

/** A DER element: `tag`, its length and `body`. */
function tlv(tag, body) {
  const n = body.length;
  return concat([tag], n < 128 ? [n] : n < 256 ? [0x81, n] : [0x82, n >> 8, n & 255], body);
}

/** Big-endian `b` without leading zeros (at least one byte). */
function unsigned(b) {
  let i = 0;
  while (i < b.length - 1 && b[i] === 0) i++;
  return b.subarray(i);
}

/** A DER INTEGER of the unsigned big-endian `b`. */
const derInt = (b) => {
  const u = unsigned(b);
  return tlv(2, u[0] & 0x80 ? concat([0], u) : u);
};

/** The algorithm identifiers of SPKI and PKCS #8, as hex, by key type. */
const ALG_IDS = {
  Ed25519: "300506032b6570",
  X25519: "300506032b656e",
  EcdsaP256: "301306072a8648ce3d020106082a8648ce3d030107",
  Rsa: "300d06092a864886f70d0101010500",
};

/** A key type name's role suffix. */
const KEY_SUFFIX = new RegExp("(PublicKey|PrivateKey)$");

/** The family of a key type: `Ed25519PrivateKey` and `Ed25519PublicKey` are `Ed25519`. */
const family = (type) => type.replace(KEY_SUFFIX, "");

/** The standard base64 alphabet's two characters that differ from base64url. */
const fromStd = (t) => t.replaceAll("+", "-").replaceAll("/", "_");

/** Whitespace, which PEM bodies break lines with. */
const SPACE = new RegExp("\\s+", "g");

/** Standard base64 and its padding. */
const BASE64 = new RegExp("^([A-Za-z0-9+/]*)(=*)$");

/**
 * A line break, decoded at run time: a release chunk is one line, and a
 * minifier writes a line break it can compute as a literal one.
 */
const NL = JSON.parse('"\\n"');

/** At most 64 characters: one line of a PEM body. */
const PEM_LINE = new RegExp(".{1,64}", "g");

/**
 * The DER inside PEM text with `label` (`PUBLIC KEY`, `PRIVATE KEY`), or
 * `null`: one block, standard base64 with its padding, any line breaks.
 */
function pemBytes(text, label) {
  if (typeof text !== "string") return null;
  const t = text.trim();
  const head = `-----BEGIN ${label}-----`;
  const tail = `-----END ${label}-----`;
  if (!t.startsWith(head) || !t.endsWith(tail)) return null;
  const body = t.slice(head.length, t.length - tail.length).replace(SPACE, "");
  const m = BASE64.exec(body);
  if (!m || (m[1].length + m[2].length) % 4 !== 0 || m[2].length > 2) return null;
  return fromB64url(fromStd(m[1]));
}

/** `b` as PEM with `label`: standard base64 in lines of 64. */
function pemText(b, label) {
  let t = b64url(b).replaceAll("-", "+").replaceAll("_", "/");
  t += "=".repeat((4 - (t.length % 4)) % 4);
  return [`-----BEGIN ${label}-----`, ...t.match(PEM_LINE), `-----END ${label}-----`, ""].join(NL);
}

/** A JSON object's text as an object, or `null`. */
function jsonObject(text) {
  try {
    const j = JSON.parse(String(text));
    return j && typeof j === "object" && !Array.isArray(j) ? j : null;
  } catch {
    return null;
  }
}

/**
 * The JWK members of each key family: `kty`, `crv`, the `alg` values it
 * may name, its `use`, and the `key_ops` of its private and public halves.
 */
const JWK = {
  Ed25519: { kty: "OKP", crv: "Ed25519", algs: ["EdDSA"], use: "sig", ops: ["sign", "verify"] },
  X25519: { kty: "OKP", crv: "X25519", algs: ["ECDH-ES"], use: "enc", ops: ["deriveBits", null] },
  EcdsaP256: { kty: "EC", crv: "P-256", algs: ["ES256"], use: "sig", ops: ["sign", "verify"] },
  Rsa: { kty: "RSA", algs: ["RS256", "PS256"], use: "sig", ops: [null, "verify"] },
  AesGcm: { kty: "oct", algs: ["A256GCM"], use: "enc", ops: ["encrypt", null] },
};

/**
 * Whether JWK `j` is a key of `fam` for operation `op`: its `kty` and
 * `crv`, and its `alg`, `use` and `key_ops` when present.
 */
function jwkFits(j, fam, op) {
  const f = JWK[fam];
  if (!j || j.kty !== f.kty || (f.crv && j.crv !== f.crv)) return false;
  if (j.alg !== undefined && !f.algs.includes(j.alg)) return false;
  if (j.use !== undefined && j.use !== f.use) return false;
  return j.key_ops === undefined || (Array.isArray(j.key_ops) && (op === null || j.key_ops.includes(op)));
}

/** A JWK member's bytes, `length` long when given, or `null`. */
function member(j, name, length) {
  const b = typeof j[name] === "string" ? fromB64url(j[name]) : null;
  return b && (length === undefined || b.length === length) ? b : null;
}

/** The JWK text of `fields` and the members every key of `fam` names. */
function jwkText(fam, fields, op) {
  const f = JWK[fam];
  const j = { kty: f.kty, ...(f.crv ? { crv: f.crv } : {}), ...fields };
  if (fam !== "X25519") j.alg = f.algs[0];
  j.use = f.use;
  if (op) j.key_ops = [op];
  return JSON.stringify(j);
}

// ------------------------------------------------------------ public keys

/**
 * The raw form of public key `type` in JWK text `text` (RSA: its SPKI
 * DER), or `null`. A JWK with a private part (`d`) isn't a public key.
 */
function publicFromJwk(type, text) {
  const j = jsonObject(text);
  const fam = family(type);
  if (!j || j.d !== undefined || !jwkFits(j, fam, JWK[fam].ops[1])) return null;
  if (fam === "Rsa") {
    const n = member(j, "n");
    const e = member(j, "e");
    // Through DER, so a JWK key meets the same checks as a PEM one.
    return n && e ? publicFromSpki(type, rsaSpki(n, e)) : null;
  }
  if (fam === "EcdsaP256") {
    const x = member(j, "x", 32);
    const y = member(j, "y", 32);
    return x && y ? concat([4], x, y) : null;
  }
  return member(j, "x", 32);
}

/** An RSA key's SPKI DER from its modulus and exponent. */
const rsaSpki = (n, e) => tlv(0x30, concat(fromHex(ALG_IDS.Rsa), tlv(3, concat([0], tlv(0x30, concat(derInt(n), derInt(e)))))));

/**
 * The raw form of public key `type` in SPKI DER `b` (RSA: the DER
 * itself), or `null`.
 */
function publicFromSpki(type, b) {
  const top = b && only(b, 0x30);
  const parts = top && items(b, top);
  if (!parts || parts.length !== 2 || parts[1].tag !== 3) return null;
  const fam = family(type);
  const alg = hex(b.subarray(parts[0].start - 2, parts[0].end));
  const bits = b.subarray(parts[1].start, parts[1].end);
  if (bits[0] !== 0) return null;
  if (fam === "Rsa") {
    const ne = alg === ALG_IDS.Rsa || alg === "300b06092a864886f70d010101" ? rsaParts(bits.subarray(1)) : null;
    return ne && rsaSpki(...ne);
  }
  return alg === ALG_IDS[fam] ? bits.slice(1) : null;
}

/**
 * The modulus and exponent of an RSA `SEQUENCE { n, e }`, or `null`; the
 * key is refused unless the modulus has 2,048 to 8,192 bits and the
 * exponent is odd and at least 3.
 */
function rsaParts(b) {
  const top = only(b, 0x30);
  const parts = top && items(b, top);
  if (!parts || parts.length !== 2 || parts.some((p) => p.tag !== 2)) return null;
  const [n, e] = parts.map((p) => b.subarray(p.start, p.end));
  if ([n, e].some((x) => x.length === 0 || x[0] & 0x80 || (x.length > 1 && x[0] === 0 && !(x[1] & 0x80)))) return null;
  const un = unsigned(n);
  const ue = unsigned(e);
  const bits = (un.length - 1) * 8 + Math.floor(Math.log2(un[0])) + 1;
  if (bits < 2048 || bits > 8192 || !(ue[ue.length - 1] & 1) || (ue.length === 1 && ue[0] < 3) || ue.length > 4) return null;
  return [un, ue];
}

/** The modulus and exponent of `RsaPublicKey` text `k`. */
function rsaOf(k) {
  const b = fromB64url(k);
  const top = b && only(b, 0x30);
  const parts = top && items(b, top);
  return (parts && rsaParts(b.subarray(parts[1].start + 1, parts[1].end))) ?? fail("not an RSA public key");
}

/** Public key text of `type` from its raw form (RSA: SPKI), or `null`. */
const publicText = (type, raw) => (raw === null ? null : family(type) === "Rsa" ? b64url(raw) : parsePublic(type)(raw));

/** The SPKI DER of public key text `k` of `type`. */
function spkiOf(type, k) {
  const raw = rawPublic(k);
  return family(type) === "Rsa" ? raw : concat(tlv(0x30, concat(fromHex(ALG_IDS[family(type)]), tlv(3, concat([0], raw)))));
}

/** The JWK text of public key text `k` of `type`. */
function publicJwk(type, k) {
  const fam = family(type);
  const op = JWK[fam].ops[1];
  if (fam === "Rsa") {
    const [n, e] = rsaOf(k);
    return jwkText(fam, { n: b64url(n), e: b64url(e) }, op);
  }
  const raw = rawPublic(k);
  if (fam === "EcdsaP256") return jwkText(fam, { x: b64url(raw.subarray(1, 33)), y: b64url(raw.subarray(33)) }, op);
  return jwkText(fam, { x: b64url(raw) }, op);
}

/** `parse(jwk:)`, `parse(pem:)`, `jwk` and `pem` of public key `type`. */
function publicFormats(type) {
  return {
    jwk: (text) => publicText(type, publicFromJwk(type, text)),
    pem: (text) => publicText(type, publicFromSpki(type, pemBytes(text, "PUBLIC KEY"))),
    toJwk: (k) => publicJwk(type, k),
    toPem: (k) => pemText(spkiOf(type, k), "PUBLIC KEY"),
  };
}

const edF = publicFormats("Ed25519PublicKey");
/** `Ed25519PublicKey.parse(jwk:)`. */
export const Ed25519PublicKey$parse$jwk = edF.jwk;
/** `Ed25519PublicKey.parse(pem:)`: SPKI. */
export const Ed25519PublicKey$parse$pem = edF.pem;
/** `Ed25519PublicKey.jwk`. */
export const Ed25519PublicKey$jwk = edF.toJwk;
/** `Ed25519PublicKey.pem`. */
export const Ed25519PublicKey$pem = edF.toPem;

const ecF = publicFormats("EcdsaP256PublicKey");
/** `EcdsaP256PublicKey.parse(jwk:)`. */
export const EcdsaP256PublicKey$parse$jwk = ecF.jwk;
/** `EcdsaP256PublicKey.parse(pem:)`: SPKI. */
export const EcdsaP256PublicKey$parse$pem = ecF.pem;
/** `EcdsaP256PublicKey.jwk`. */
export const EcdsaP256PublicKey$jwk = ecF.toJwk;
/** `EcdsaP256PublicKey.pem`. */
export const EcdsaP256PublicKey$pem = ecF.toPem;

const xF = publicFormats("X25519PublicKey");
/** `X25519PublicKey.parse(jwk:)`. */
export const X25519PublicKey$parse$jwk = xF.jwk;
/** `X25519PublicKey.parse(pem:)`: SPKI. */
export const X25519PublicKey$parse$pem = xF.pem;
/** `X25519PublicKey.jwk`. */
export const X25519PublicKey$jwk = xF.toJwk;
/** `X25519PublicKey.pem`. */
export const X25519PublicKey$pem = xF.toPem;

const rsaF = publicFormats("RsaPublicKey");
/** `RsaPublicKey.parse(jwk:)`: `kty` `RSA`, at least 2,048 bits. */
export const RsaPublicKey$parse$jwk = rsaF.jwk;
/** `RsaPublicKey.parse(pem:)`: SPKI. */
export const RsaPublicKey$parse$pem = rsaF.pem;
/** `RsaPublicKey.jwk`. */
export const RsaPublicKey$jwk = rsaF.toJwk;
/** `RsaPublicKey.pem`. */
export const RsaPublicKey$pem = rsaF.toPem;
/** `RsaPublicKey.fingerprint()`: the SHA-256 of its SPKI DER, as hex. */
export const RsaPublicKey$fingerprint = fingerprint;
/** The `Codable` codec of `RsaPublicKey`: base64url of its SPKI DER. */
export const RsaPublicKey$codec = textCodec((t) => {
  const b = fromB64url(t);
  return b && publicFromSpki("RsaPublicKey", b) ? t : null;
}, "an RSA public key");

/** Verifies RSA signature text `sig` of `message` by key text `k`. */
async function rsaVerify(k, sig, message, pss) {
  const s = fromB64url(sig);
  const [n, e] = rsaOf(k);
  return s !== null && (await backend().rsaVerify(n, e, s, bytes(message), pss)) === true;
}

/** `RsaPublicKey.verify(_:of:)`: RSASSA-PKCS1-v1_5 with SHA-256 (JWT `RS256`). */
export const RsaPublicKey$verify = async (k, _c, sig, message) => rsaVerify(k, sig, message, false);
/** `RsaPublicKey.verifyPss(_:of:)`: RSASSA-PSS, SHA-256, MGF1-SHA-256, a 32-byte salt (JWT `PS256`). */
export const RsaPublicKey$verifyPss = async (k, _c, sig, message) => rsaVerify(k, sig, message, true);

/** `RsaSignature.bytes`. */
export const RsaSignature$bytes = (s) => fromB64url(s) ?? fail("not a signature");
/** `RsaSignature.parse(bytes:)`: 256 to 1,024 bytes. */
export const RsaSignature$parse = (b) => (b instanceof Uint8Array && b.length >= 256 && b.length <= 1024 ? b64url(b) : null);
/** The `Codable` codec of `RsaSignature`. */
export const RsaSignature$codec = textCodec((t) => RsaSignature$parse(fromB64url(t)), "an RSA signature");

// ------------------------------------------------------- secret keys

/**
 * The raw form of secret key `type` in JWK text `text`, as
 * `backend.fromRaw` takes it (P-256: `d ‖ x ‖ y`), or `null`. The public
 * half a JWK names must be the key's own, which `fromRaw` checks for
 * P-256 and {@link privateFromJwk}'s callers for the others.
 * @returns {{raw: Uint8Array, x: Uint8Array|null}|null}
 */
function privateFromJwk(type, text) {
  const j = jsonObject(reveal(text));
  const fam = family(type === "AesGcmKey" ? "AesGcm" : type);
  if (!j || !jwkFits(j, fam, JWK[fam].ops[0])) return null;
  if (fam === "AesGcm") {
    const k = member(j, "k", 32);
    return k && { raw: k, x: null };
  }
  const d = member(j, "d", 32);
  if (!d) return null;
  if (fam === "EcdsaP256") {
    const x = member(j, "x", 32);
    const y = member(j, "y", 32);
    return x && y ? { raw: concat(d, x, y), x: null } : null;
  }
  const x = j.x === undefined ? null : member(j, "x", 32);
  return j.x !== undefined && !x ? null : { raw: d, x };
}

/**
 * The raw form of private key `type` in PKCS #8 DER `b` (`OneAsymmetricKey`
 * v1 or v2, RFC 5958), or `null`; a public key it carries comes back as
 * `x` (Ed25519, X25519) or inside the raw form (P-256's `d ‖ x ‖ y`).
 * @returns {{raw: Uint8Array, x: Uint8Array|null}|null}
 */
function privateFromPkcs8(type, b) {
  const top = b && only(b, 0x30);
  const parts = top && items(b, top);
  if (!parts || parts.length < 3 || parts[0].tag !== 2 || parts[2].tag !== 4) return null;
  const version = b.subarray(parts[0].start, parts[0].end);
  if (version.length !== 1 || version[0] > 1) return null;
  const fam = family(type);
  if (hex(b.subarray(parts[1].start - 2, parts[1].end)) !== ALG_IDS[fam]) return null;
  const inner = b.subarray(parts[2].start, parts[2].end);
  if (fam !== "EcdsaP256") {
    const key = only(inner, 4);
    if (!key || key.end - key.start !== 32) return null;
    const pub = parts.find((p) => p.tag === 0x81);
    const x = pub ? b.subarray(pub.start + 1, pub.end) : null;
    return pub && (b[pub.start] !== 0 || x.length !== 32) ? null : { raw: inner.slice(2), x };
  }
  const ec = only(inner, 0x30);
  const fields = ec && items(inner, ec);
  if (!fields || fields.length < 2 || fields[1].tag !== 4 || fields[1].end - fields[1].start !== 32) return null;
  const d = inner.subarray(fields[1].start, fields[1].end);
  const pub = fields.find((f) => f.tag === 0xa1);
  if (!pub) return { raw: d.slice(), x: null };
  const bits = der(inner, pub.start);
  const point = bits && bits.tag === 3 ? inner.subarray(bits.start, bits.end) : null;
  return point && point.length === 66 && point[0] === 0 && point[1] === 4 ? { raw: concat(d, point.subarray(2)), x: null } : null;
}

/**
 * The raw form of secret key `type` in `text` (`form` `jwk` or `pem`),
 * for either backend's `fromRaw`, and the public half it names, if any;
 * `null` when it isn't one. For the vector tests, which run in browsers
 * too, where `parse` of a secret key (a `Secret`) can't.
 */
export const privateRaw = (type, form, text) => (form === "jwk" ? privateFromJwk(type, text) : privateFromPkcs8(type, pemBytes(text, "PRIVATE KEY")));

/** `value` in a `Secret`, for the vector tests (which import nothing but this module). */
export const secretOf = (value) => new Secret(value);

/** The PKCS #8 DER of a private key on the server's backend (its raw secret and point). */
function pkcs8Of(key) {
  const fam = family(key.type);
  if (fam !== "EcdsaP256") return concat(fromHex(PRIVATE[key.type].pkcs8), key.secret);
  const ec = tlv(0x30, concat(fromHex("020101"), tlv(4, key.secret), tlv(0xa1, tlv(3, concat([0], key.pub)))));
  return tlv(0x30, concat(fromHex("020100"), fromHex(ALG_IDS.EcdsaP256), tlv(4, ec)));
}

/**
 * A secret key of `type` from a parsed JWK or PKCS #8 (`parsed`), on the
 * server's backend, exportable or not; `null` when the public half it
 * names isn't the key's own.
 */
function secretFrom(type, parsed, exportable) {
  if (!parsed) return null;
  const k = syncBackend().fromRaw(type, parsed.raw, exportable);
  return k && (!parsed.x || hex(parsed.x) === hex(k.pub)) ? k : null;
}

/** `parse(raw:)`, `parse(jwk:)` and `parse(pem:)` of secret key `type`, exportable or not. */
function secretParsers(type, exportable) {
  return {
    raw: (raw) => {
      const r = reveal(raw);
      return r instanceof Uint8Array && r.length === 32 ? syncBackend().fromRaw(type, r, exportable) : null;
    },
    jwk: (text) => secretFrom(type, privateFromJwk(type, text), exportable),
    pem: (text) => (type === "AesGcmKey" ? null : secretFrom(type, privateFromPkcs8(type, pemBytes(reveal(text), "PRIVATE KEY")), exportable)),
  };
}

const aesP = secretParsers("AesGcmKey", false);
/** `AesGcmKey.parse(jwk:)`: `kty` `oct`, 32 bytes. */
export const AesGcmKey$parse$jwk = aesP.jwk;
const edP = secretParsers("Ed25519PrivateKey", false);
/** `Ed25519PrivateKey.parse(jwk:)`. */
export const Ed25519PrivateKey$parse$jwk = edP.jwk;
/** `Ed25519PrivateKey.parse(pem:)`: PKCS #8. */
export const Ed25519PrivateKey$parse$pem = edP.pem;
const ecP = secretParsers("EcdsaP256PrivateKey", false);
/** `EcdsaP256PrivateKey.parse(jwk:)`. */
export const EcdsaP256PrivateKey$parse$jwk = ecP.jwk;
/** `EcdsaP256PrivateKey.parse(pem:)`: PKCS #8. */
export const EcdsaP256PrivateKey$parse$pem = ecP.pem;
const xP = secretParsers("X25519PrivateKey", false);
/** `X25519PrivateKey.parse(jwk:)`. */
export const X25519PrivateKey$parse$jwk = xP.jwk;
/** `X25519PrivateKey.parse(pem:)`: PKCS #8. */
export const X25519PrivateKey$parse$pem = xP.pem;

/** `Exportable<K>.parse(raw:)`: `type` is `K`'s name, from the call's static type. */
export const Exportable$parse$raw = (type, raw) => secretParsers(type, true).raw(raw);
/** `Exportable<K>.parse(jwk:)`. */
export const Exportable$parse$jwk = (type, text) => secretParsers(type, true).jwk(text);
/** `Exportable<K>.parse(pem:)` (private keys). */
export const Exportable$parse$pem = (type, text) => secretParsers(type, true).pem(text);

/** `exportable.exportJwk()`: the JWK, with `alg`, `use` and `key_ops`, as a `Secret<String>`. */
export function Exportable$exportJwk(e) {
  const k = reveal(e);
  if (!k.secret) fail("a browser key's JWK is wrapped, not exported");
  if (k.type === "AesGcmKey") return new Secret(jwkText("AesGcm", { k: b64url(k.secret) }, "encrypt"));
  const fam = family(k.type);
  const pub = k.pub;
  const point = fam === "EcdsaP256" ? { x: b64url(pub.subarray(1, 33)), y: b64url(pub.subarray(33)) } : { x: b64url(pub) };
  return new Secret(jwkText(fam, { ...point, d: b64url(k.secret) }, JWK[fam].ops[0]));
}

/** `exportable.exportPem()`: PKCS #8 PEM of a private key, as a `Secret<String>`. */
export function Exportable$exportPem(e) {
  const k = reveal(e);
  if (!k.secret) fail("a browser key's PEM is wrapped, not exported");
  if (k.type === "AesGcmKey") fail("an AesGcmKey has no PEM form; export it as raw bytes or a JWK");
  return new Secret(pemText(pkcs8Of(k), "PRIVATE KEY"));
}

/**
 * A key of `type` from configuration text (`docs/design/33-crypto.md`,
 * "Keys in configuration"): a JWK, a PEM (SPKI or PKCS #8) or base64url
 * of the raw form. Startup has checked the text's shape; a key that still
 * doesn't parse is a panic naming the type, never the text.
 */
export function keyFromConfig(type, text) {
  const t = String(text).trim();
  const secret = !type.endsWith("PublicKey");
  const f = secret ? secretParsers(type, false) : publicFormats(type);
  const raw = secret ? f.raw : family(type) === "Rsa" ? () => null : parsePublic(type);
  const key = t.startsWith("{") ? f.jwk(t) : t.startsWith("-----") ? f.pem(t) : raw(fromB64url(t));
  return key ?? fail(`the configured ${type} isn't a valid key`);
}

// -------------------------------------------------------- JWS and JWT

/** `JwtError`'s unit cases and payload cases, as the generated enum builds them. */
const JWT = {
  malformed: unitCase(0),
  wrongAlgorithm: (alg) => ({ $: 1, _0: String(alg) }),
  unknownKeyId: (kid) => ({ $: 2, _0: kid ?? null }),
  badSignature: unitCase(3),
  expired: unitCase(4),
  notYetValid: unitCase(5),
  wrongIssuer: unitCase(6),
  wrongAudience: unitCase(7),
  missingExpiry: unitCase(8),
  claims: (e) => ({ $: 9, _0: e }),
};

/** The JWS signature algorithm each public key family verifies. */
const ALG_OF = { Ed25519: "EdDSA", EcdsaP256: "ES256" };

/** An HS256 key's bytes: a `Secret` of bytes or of text (UTF-8), at least 32 bytes (RFC 7518 §3.2). */
function hmacKey(secret) {
  const k = macKey(secret instanceof Secret && secret.reveal() instanceof Uint8Array ? secret.reveal() : secret);
  return k.length >= 32 ? k : fail("an HS256 key is at least 32 bytes");
}

/**
 * The verifier of `key` (a public key's text, a `Secret` of HMAC key
 * material, or a `JwkSet`) for a JWS with `header`: the algorithms it
 * accepts and its check of the signature; throws `JwtError`.
 */
function verifierOf(key, header) {
  if (key && typeof key === "object" && Array.isArray(key.keys)) {
    const kid = typeof header.kid === "string" ? header.kid : null;
    const found = key.keys.filter((k) => (kid === null ? key.keys.length === 1 : k.kid === kid));
    if (found.length !== 1) throw JWT.unknownKeyId(kid);
    const v = verifierOf(found[0].key, header);
    return { ...v, algs: v.algs.filter((a) => found[0].algs.includes(a)) };
  }
  if (key instanceof Secret) {
    const k = hmacKey(key);
    return {
      algs: ["HS256"],
      check: async (_alg, input, sig) => equalBytes((await backend().hmac(256, k, input)), sig),
    };
  }
  const raw = rawPublic(key);
  if (raw.length === 32 || raw.length === 65) {
    const type = raw.length === 32 ? "Ed25519PublicKey" : "EcdsaP256PublicKey";
    return { algs: [ALG_OF[family(type)]], check: (_alg, input, sig) => backend().verify(type, raw, sig, input) };
  }
  const [n, e] = rsaOf(key);
  return { algs: ["RS256", "PS256"], check: (alg, input, sig) => backend().rsaVerify(n, e, sig, input, alg === "PS256") };
}

/** The parts of compact JWS `compact`: header, payload bytes, signing input and signature; throws `.malformed`. */
function jwsParts(compact) {
  const parts = typeof compact === "string" ? compact.split(".") : [];
  if (parts.length !== 3) throw JWT.malformed;
  const [h, p, s] = parts.map(fromB64url);
  const header = h && jsonObject(bytesUtf8(h));
  if (!header || p === null || s === null || typeof header.alg !== "string" || header.crit !== undefined) throw JWT.malformed;
  return { header, payload: p, input: utf8(`${parts[0]}.${parts[1]}`), sig: s };
}

/** Verifies compact JWS `compact` with `key`; its payload, or throws `JwtError`. */
async function jwsVerify(compact, key) {
  const { header, payload, input, sig } = jwsParts(compact);
  const v = verifierOf(key, header);
  if (!v.algs.includes(header.alg)) throw JWT.wrongAlgorithm(header.alg);
  if (!(await v.check(header.alg, input, sig))) throw JWT.badSignature;
  return payload;
}

/** Signs `payload` with `key` (a private key, a `Secret` of one, or HMAC key material) as compact JWS. */
async function jwsSign(payload, key, extra) {
  const k = reveal(key);
  const alg = k instanceof Key ? ALG_OF[family(k.type)] : "HS256";
  const head = b64url(utf8(JSON.stringify({ alg, ...extra })));
  const input = `${head}.${b64url(bytes(payload))}`;
  const sig = k instanceof Key ? await backend().sign(k, utf8(input)) : await backend().hmac(256, hmacKey(key), utf8(input));
  return `${input}.${b64url(sig)}`;
}

/** `Jws.verify(_:key:)` and `Jws.sign(_:key:)`: the signature layer alone. */
export const Jws = {
  verify: async (_c, compact, key) => jwsVerify(compact, key),
  sign: async (_c, payload, key) => jwsSign(payload, key, {}),
};

/** A NumericDate claim (seconds, possibly fractional), or `undefined` when absent; throws `.malformed` otherwise. */
function numericDate(claims, name) {
  const v = claims[name];
  if (v === undefined) return undefined;
  if (typeof v !== "number" || !Number.isFinite(v)) throw JWT.malformed;
  return v * 1000;
}

/**
 * `Jwt.verify(_:key:issuer:audience:now:leeway:)`: the signature, `exp`
 * (required), `nbf`, `iss` and `aud`, then the claims decoded by `codec`.
 */
async function jwtVerify(_c, codec, token, key, issuer, audience, now, leeway) {
  const claims = jsonObject(bytesUtf8(await jwsVerify(token, key)) ?? "");
  if (!claims) throw JWT.malformed;
  const at = now.ms;
  const slack = leeway ?? 60000;
  const exp = numericDate(claims, "exp");
  const nbf = numericDate(claims, "nbf");
  if (exp === undefined) throw JWT.missingExpiry;
  if (at > exp + slack) throw JWT.expired;
  if (nbf !== undefined && at < nbf - slack) throw JWT.notYetValid;
  if (claims.iss !== issuer) throw JWT.wrongIssuer;
  const aud = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (!aud.includes(audience)) throw JWT.wrongAudience;
  try {
    return codec.decode(claims, "");
  } catch (e) {
    if (e instanceof DecodeFailure) throw JWT.claims({ $: 1, path: e.path || "$", message: e.message });
    throw e;
  }
}

/** `Jwt.sign(_:key:keyId:expiresIn:now:)`: a compact JWT with `iat` and `exp` set. */
async function jwtSign(_c, codec, claims, key, keyId, expiresIn, now) {
  const body = codec.encode(claims);
  if (!body || typeof body !== "object" || Array.isArray(body)) fail("JWT claims encode as a JSON object");
  const iat = Math.floor(now.ms / 1000);
  const payload = { ...body, iat, exp: iat + Math.floor(expiresIn / 1000) };
  return jwsSign(utf8(JSON.stringify(payload)), key, { typ: "JWT", ...(keyId === null || keyId === undefined ? {} : { kid: String(keyId) }) });
}

/** `Jwt.verify` and `Jwt.sign`. */
export const Jwt = { verify: jwtVerify, sign: jwtSign };

/**
 * `JwkSet.parse(_:)`: a JWKS document's public signature keys that K
 * supports, each with its `kid` and the algorithms it may verify (its
 * `alg`, else its family's). Symmetric keys, private keys, encryption keys,
 * other curves and RSA keys under 2,048 bits are left out; `nil` for text
 * that isn't a JWKS.
 */
export function JwkSet$parse(text) {
  const j = jsonObject(text);
  if (!j || !Array.isArray(j.keys)) return null;
  const keys = [];
  for (const k of j.keys) {
    for (const type of ["Ed25519PublicKey", "EcdsaP256PublicKey", "RsaPublicKey"]) {
      const key = publicText(type, publicFromJwk(type, JSON.stringify(k)));
      if (key === null) continue;
      const algs = family(type) === "Rsa" ? (k.alg ? [k.alg] : JWK.Rsa.algs) : [ALG_OF[family(type)]];
      keys.push({ kid: typeof k.kid === "string" ? k.kid : null, key, algs });
    }
  }
  return { keys };
}

/** `jwkSet.keyIds`: the `kid` of each key kept, in order. */
export const JwkSet$keyIds = (s) => s.keys.map((k) => k.kid).filter((k) => k !== null);

// -------------------------------------------------------------- KeyStore

/** `KeyName(name:)`: the name, which fixes the key type statically. */
export const KeyName$init = (name) => ({ name: String(name) });
/** `keyName.name`. */
export const KeyName$name = (k) => k.name;

/** `KeyStoreError.unavailable`. */
const UNAVAILABLE = unitCase(0);

/**
 * `k.crypto.BrowserKeyStore`: keys as structured-cloned, non-extractable
 * `CryptoKey`s in IndexedDB (`k-keys:<namespace>`, one object store), so
 * a key survives a reload without being written down as bytes. An
 * extractable key is re-imported non-extractable before it is stored.
 * Every failure to reach IndexedDB is `KeyStoreError.unavailable`.
 * @param {string} namespace the app's storage namespace
 */
export function BrowserKeyStore(namespace) {
  let db = null;
  const open = () =>
    (db ??= new Promise((resolve, reject) => {
      const r = globalThis.indexedDB.open(`k-keys:${namespace}`, 1);
      r.onupgradeneeded = () => r.result.createObjectStore("keys");
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    }).catch(() => {
      db = null;
      throw UNAVAILABLE;
    }));
  const run = async (mode, f) => {
    const d = await open().catch(() => {
      throw UNAVAILABLE;
    });
    return new Promise((resolve, reject) => {
      try {
        const t = d.transaction("keys", mode);
        const req = f(t.objectStore("keys"));
        t.oncomplete = () => resolve(req.result);
        t.onerror = t.onabort = () => reject(UNAVAILABLE);
      } catch {
        reject(UNAVAILABLE);
      }
    });
  };
  return {
    async get(_c, name) {
      const r = await run("readonly", (s) => s.get(name.name));
      return r ? new Key(r.type, { handle: r.handle, pub: r.pub ?? null }) : null;
    },
    async put(_c, key, name) {
      const k = reveal(key);
      const handle = k.handle.extractable ? (await subtle.fromRaw(k.type, await subtle.exportWrap(k), false)).handle : k.handle;
      await run("readwrite", (s) => s.put({ type: k.type, handle, pub: k.pub }, name.name));
    },
    async delete(_c, name) {
      await run("readwrite", (s) => s.delete(name.name));
    },
    async requestPersistence(_c) {
      try {
        return unitCase((await globalThis.navigator.storage.persist()) ? 0 : 1);
      } catch {
        return unitCase(1);
      }
    },
  };
}
