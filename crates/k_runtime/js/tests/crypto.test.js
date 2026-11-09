/**
 * @module k:/tests/crypto.test.js
 *
 * `k.crypto`'s key types on the server's `ops` backend
 * (docs/design/33-crypto.md): the published vectors (`crypto-vectors.js`,
 * which the real-Chrome test runs on Web Crypto too), the K members as the
 * generated code calls them (`Owner$member(receiver, $c, codec, …)`), the
 * wire formats, `OpenError`'s cases and the parsing rules.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { DecodeFailure, Instant, Secret, codecs, utf8 } from "k:runtime";
import * as c from "k:/runtime/crypto.js";
import { cryptoOps } from "k:/runtime/crypto-ops.js";
import vectors from "k:/tests/crypto-vectors.json.js";
import { checkJoseVectors, checkVectors } from "k:/tests/crypto-vectors.js";

c.useOps(cryptoOps(Deno.core.ops));

/** Bytes as hex. */
const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

/** Whether `f` throws. */
const throws = (f) => {
  try {
    f();
  } catch {
    return true;
  }
  return false;
};

/** `OpenError.authenticationFailed`, as the generated enum makes it. */
const AUTH = { $: 0 };

/** The error `p` rejects with. */
async function rejection(p) {
  try {
    await p;
  } catch (e) {
    return e;
  }
  throw new Error("expected a rejection");
}

/** A record codec, standing in for a generated `Note$codec`. */
const note = {
  encode: (v) => ({ body: v.body }),
  decode: (j, p) => {
    if (typeof j?.body !== "string") throw new DecodeFailure(`${p}.body`, "expected a string");
    return { body: j.body };
  },
};

test("the ops backend matches the published vectors", async () => {
  assertEq(c.backend(), c.ops);
  const failures = await checkVectors(c, c.ops, vectors);
  assertEq(failures.length, 0, failures.slice(0, 10).join("\n"));
});

test("sealed values open with the same key and context, and only then", async () => {
  const key = await c.AesGcmKey$generate(null);
  assertEq(String(key), "AesGcmKey(•••)");
  assertEq(JSON.stringify({ key }), '{"key":"AesGcmKey(•••)"}');
  const ctx = utf8("note:1");
  const sealed = await c.AesGcmKey$seal(key, null, codecs.string, "hi", ctx);
  assert(sealed.startsWith("k1."), sealed);
  assertEq(await c.AesGcmKey$open(key, null, codecs.string, sealed, ctx), "hi");
  // A fresh nonce every time.
  assert(sealed !== (await c.AesGcmKey$seal(key, null, codecs.string, "hi", ctx)));
  assertDeepEq(await rejection(c.AesGcmKey$open(key, null, codecs.string, sealed, utf8("note:2"))), AUTH);
  assertDeepEq(await rejection(c.AesGcmKey$open(key, null, codecs.string, sealed)), AUTH);
  const other = await c.AesGcmKey$generate(null);
  assertDeepEq(await rejection(c.AesGcmKey$open(other, null, codecs.string, sealed, ctx)), AUTH);
  // A changed byte, and text that isn't a sealed value.
  const b = c.Sealed$bytes(sealed);
  b[b.length - 1] ^= 1;
  assertDeepEq(await rejection(c.AesGcmKey$open(key, null, codecs.string, c.Sealed$parse$bytes(b), ctx)), AUTH);
  assertDeepEq(await rejection(c.AesGcmKey$open(key, null, codecs.string, "k1.AAAA", ctx)), AUTH);
});

test("bytes are sealed as they are and other values as their JSON", async () => {
  const key = c.TestKeys.aesGcm(1);
  const raw = Uint8Array.of(0, 1, 2, 255);
  const sealed = await c.AesGcmKey$seal(key, null, codecs.bytes, raw);
  // Version, nonce, the 4 bytes and the tag.
  assertEq(c.Sealed$bytes(sealed).length, 1 + 12 + 4 + 16);
  assertDeepEq([...(await c.AesGcmKey$open(key, null, codecs.bytes, sealed))], [0, 1, 2, 255]);
  const json = await c.AesGcmKey$seal(key, null, codecs.list(codecs.int), [1, 2]);
  assertEq(c.Sealed$bytes(json).length, 1 + 12 + 5 + 16);
});

test("an authentic value of another type is undecodable, with the decoding error", async () => {
  const key = c.TestKeys.aesGcm(2);
  const sealed = await c.AesGcmKey$seal(key, null, codecs.string, "text");
  const e = await rejection(c.AesGcmKey$open(key, null, codecs.int, sealed));
  assertEq(e.$, 1);
  assertEq(e._0.$, 1, "DecodingError.mismatch");
  assertEq(e._0.path, "$");
  const bytes = await c.AesGcmKey$seal(key, null, codecs.bytes, Uint8Array.of(0xff));
  const m = await rejection(c.AesGcmKey$open(key, null, codecs.string, bytes));
  assertEq(m._0.$, 0, "DecodingError.malformed");
});

test("the wire forms parse only themselves", () => {
  assertEq(c.Sealed$parse("k1."), null);
  assertEq(c.Sealed$parse("k2.AQ"), null);
  assertEq(c.Sealed$parse$bytes(new Uint8Array(28)), null, "too short");
  const ok = new Uint8Array(29);
  ok[0] = 1;
  const text = c.Sealed$parse$bytes(ok);
  assertEq(c.Sealed$parse(text), text);
  ok[0] = 2;
  assertEq(c.Sealed$parse$bytes(ok), null, "an unknown version");
  assertEq(c.Envelope$parse(text), null, "a sealed value isn't an envelope");
  assertEq(c.fromB64url("AB"), null, "spare bits must be zero");
  assertEq(c.fromB64url("AA=="), null, "no padding");
  const codec = c.Sealed$codec(codecs.string);
  assertEq(codec.decode(text, ""), text);
  let threw = false;
  try {
    codec.decode("k1.", "$.x");
  } catch (e) {
    threw = e.path === "$.x";
  }
  assert(threw, "a malformed sealed value fails decoding at its path");
});

test("wrapped keys unwrap as their own type only, from the right key", async () => {
  const wrapping = c.TestKeys.aesGcm(3);
  const vault = await c.AesGcmKey$generateExportable(null);
  const wrapped = await c.AesGcmKey$wrap(wrapping, null, vault);
  assert(wrapped.startsWith("k1w."), wrapped);
  const back = await c.AesGcmKey$unwrap(wrapping, null, "AesGcmKey", wrapped);
  assertEq(back.exportable, false);
  const sealed = await c.AesGcmKey$seal(vault, null, codecs.string, "x");
  assertEq(await c.AesGcmKey$open(back, null, codecs.string, sealed), "x");
  assertDeepEq(await rejection(c.AesGcmKey$unwrap(wrapping, null, "X25519PrivateKey", wrapped)), AUTH);
  assertDeepEq(await rejection(c.AesGcmKey$unwrap(c.TestKeys.aesGcm(4), null, "AesGcmKey", wrapped)), AUTH);
  const again = await c.AesGcmKey$unwrapExportable(wrapping, null, "AesGcmKey", wrapped);
  assertEq(again.exportable, true);
  // A plain key can't be wrapped: only `Exportable<K>` can.
  assert((await rejection(c.AesGcmKey$wrap(wrapping, null, back))).message.includes("Exportable"));
  // Private keys wrap too.
  const signer = await c.Ed25519PrivateKey$generateExportable(null);
  const w = await c.AesGcmKey$wrap(wrapping, null, signer);
  const s = await c.AesGcmKey$unwrap(wrapping, null, "Ed25519PrivateKey", w);
  assertEq(c.Ed25519PrivateKey$publicKey(s), c.Ed25519PrivateKey$publicKey(signer));
  // A P-256 key is wrapped with its point, which must agree with the scalar.
  const ec = await c.EcdsaP256PrivateKey$generateExportable(null);
  const we = await c.AesGcmKey$wrap(wrapping, null, ec);
  const back256 = await c.AesGcmKey$unwrap(wrapping, null, "EcdsaP256PrivateKey", we);
  assertEq(c.EcdsaP256PrivateKey$publicKey(back256), c.EcdsaP256PrivateKey$publicKey(ec));
  const other = c.TestKeys.ecdsaP256(2);
  const mixed = Uint8Array.from([...ec.secret, ...other.pub.subarray(1)]);
  assertEq(c.ops.fromRaw("EcdsaP256PrivateKey", mixed, false), null);
});

test("a passphrase derives the same key from the same salt", async () => {
  const salt = c.PassphraseSalt$generate();
  // `kit.js` isn't loaded here: this is a production salt.
  assert(salt.startsWith("pbkdf2-sha256$600000$"), salt);
  assertEq(c.PassphraseSalt$iterations(salt), 600000);
  const weak = "pbkdf2-sha256$1000$AAAAAAAAAAAAAAAAAAAAAA";
  assertEq(c.PassphraseSalt$parse(weak), null, "a test salt doesn't parse outside k test");
  c.setTesting(true);
  try {
    assertEq(c.PassphraseSalt$parse(weak), weak);
    const fast = c.PassphraseSalt$generate();
    assert(fast.startsWith("pbkdf2-sha256$1000$"), fast);
    const a = await c.AesGcmKey$derive$fromPassphrase$salt(null, "correct horse", fast);
    const b = await c.AesGcmKey$derive$fromPassphrase$salt(null, "correct horse", fast);
    const wrong = await c.AesGcmKey$derive$fromPassphrase$salt(null, "battery staple", fast);
    const vault = await c.AesGcmKey$generateExportable(null);
    const wrapped = await c.AesGcmKey$wrap(a, null, vault);
    assert(await c.AesGcmKey$unwrap(b, null, "AesGcmKey", wrapped));
    assertDeepEq(await rejection(c.AesGcmKey$unwrap(wrong, null, "AesGcmKey", wrapped)), AUTH);
  } finally {
    c.setTesting(false);
  }
});

test("hkdf derives keys and raw bytes from shared and configured secrets", async () => {
  const a = c.TestKeys.x25519(1);
  const b = c.TestKeys.x25519(2);
  const ab = await c.X25519PrivateKey$agree(a, null, c.X25519PrivateKey$publicKey(b));
  const ba = await c.X25519PrivateKey$agree(new Secret(b), null, c.X25519PrivateKey$publicKey(a));
  assertEq(String(ab), "SharedSecret(•••)");
  const salt = Uint8Array.of(1);
  const k1 = await c.AesGcmKey$derive$from$salt$info(null, ab, salt, "chat/v1");
  const k2 = await c.AesGcmKey$derive$from$salt$info(null, ba, salt, "chat/v1");
  const sealed = await c.AesGcmKey$seal(k1, null, codecs.string, "hello");
  assertEq(await c.AesGcmKey$open(k2, null, codecs.string, sealed), "hello");
  const other = await c.AesGcmKey$derive$from$salt$info(null, ab, salt, "chat/v2");
  assertDeepEq(await rejection(c.AesGcmKey$open(other, null, codecs.string, sealed)), AUTH);
  const raw = await c.Hkdf.sha256(null, new Secret("text"), new Uint8Array(0), "p", 10);
  assert(raw instanceof Secret);
  assertEq(raw.reveal().length, 10);
  const fromKey = await c.Hkdf.sha256(null, new Secret(c.TestKeys.aesGcm(1)), new Uint8Array(0), "p", 32);
  assertEq(fromKey.reveal().length, 32);
});

test("signatures verify with their key only, and a configured key forwards", async () => {
  const msg = utf8("manifest");
  const key = c.TestKeys.ed25519(1);
  const configured = new Secret(key);
  const sig = await c.Ed25519PrivateKey$sign(configured, null, msg);
  assertEq(sig, await c.Ed25519PrivateKey$sign(key, null, msg), "Ed25519 is deterministic");
  const pub = c.Ed25519PrivateKey$publicKey(configured);
  assert(await c.Ed25519PublicKey$verify(pub, null, sig, msg));
  assert(!(await c.Ed25519PublicKey$verify(pub, null, sig, utf8("other"))));
  assert(!(await c.Ed25519PublicKey$verify(c.Ed25519PrivateKey$publicKey(c.TestKeys.ed25519(2)), null, sig, msg)));
  assertEq((await c.Ed25519PublicKey$fingerprint(pub, null)).length, 64);
  const ec = c.TestKeys.ecdsaP256(1);
  const es = await c.EcdsaP256PrivateKey$sign(ec, null, msg);
  assertEq(c.EcdsaP256Signature$bytes(es).length, 64);
  assert(await c.EcdsaP256PublicKey$verify(c.EcdsaP256PrivateKey$publicKey(ec), null, es, msg));
  assertEq(c.EcdsaP256PublicKey$raw(c.EcdsaP256PrivateKey$publicKey(ec)).length, 65);
});

test("an ECDSA signature reads from DER", () => {
  const r = new Uint8Array(32).fill(0x11);
  const s = new Uint8Array(32).fill(0x81);
  // `s` has its top bit set, so DER prefixes a zero byte.
  const der = Uint8Array.of(0x30, 0x45, 0x02, 0x20, ...r, 0x02, 0x21, 0x00, ...s);
  const sig = c.EcdsaP256Signature$parse$der(der);
  assertDeepEq([...c.EcdsaP256Signature$bytes(sig)], [...r, ...s]);
  const short = Uint8Array.of(0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x02);
  const padded = c.EcdsaP256Signature$bytes(c.EcdsaP256Signature$parse$der(short));
  assertEq(padded[31], 1);
  assertEq(padded[63], 2);
  assertEq(c.EcdsaP256Signature$parse$der(der.subarray(0, 40)), null);
  assertEq(c.EcdsaP256Signature$parse$der(Uint8Array.of(0x30, 0x06, 0x02, 0x01, 0x81, 0x02, 0x01, 0x02)), null, "a negative integer");
});

test("public keys parse by length, and X25519 refuses small-order points", () => {
  assertEq(c.Ed25519PublicKey$parse$raw(new Uint8Array(31)), null);
  assert(c.Ed25519PublicKey$parse$raw(new Uint8Array(32)));
  assertEq(c.EcdsaP256PublicKey$parse$raw(new Uint8Array(65)), null, "not uncompressed");
  assertEq(c.X25519PublicKey$parse$raw(new Uint8Array(32)), null, "zero");
  const order8 = Uint8Array.from("e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b880".match(/../g), (x) => parseInt(x, 16));
  assertEq(c.X25519PublicKey$parse$raw(order8), null, "with the top bit set too");
  const pub = c.X25519PrivateKey$publicKey(c.TestKeys.x25519(1));
  assertEq(c.X25519PublicKey$parse$raw(c.X25519PublicKey$raw(pub)), pub);
  assertEq(c.X25519PublicKey$codec.decode(pub, ""), pub);
});

test("envelopes open with the recipient's key and context only", async () => {
  const me = c.TestKeys.x25519(1);
  const pub = c.X25519PrivateKey$publicKey(me);
  const ctx = utf8("share:1");
  const env = await c.X25519PublicKey$seal(pub, null, note, { body: "for you" }, ctx);
  assert(env.startsWith("k1e."), env);
  assertDeepEq(await c.X25519PrivateKey$open(new Secret(me), null, note, env, ctx), { body: "for you" });
  assertDeepEq(await rejection(c.X25519PrivateKey$open(c.TestKeys.x25519(2), null, note, env, ctx)), AUTH);
  assertDeepEq(await rejection(c.X25519PrivateKey$open(me, null, note, env)), AUTH);
  const noteKey = await c.AesGcmKey$generateExportable(null);
  const enveloped = await c.X25519PublicKey$wrap(pub, null, noteKey);
  assert(enveloped.startsWith("k1k."), enveloped);
  const k = await c.X25519PrivateKey$unwrap(me, null, "AesGcmKey", enveloped);
  const sealed = await c.AesGcmKey$seal(noteKey, null, codecs.string, "x");
  assertEq(await c.AesGcmKey$open(k, null, codecs.string, sealed), "x");
  assertDeepEq(await rejection(c.X25519PrivateKey$unwrap(me, null, "Ed25519PrivateKey", enveloped)), AUTH);
});

test("constant-time comparison compares whole values", () => {
  assert(c.ConstantTime.equals(Uint8Array.of(1, 2), Uint8Array.of(1, 2)));
  assert(!c.ConstantTime.equals(Uint8Array.of(1, 2), Uint8Array.of(1, 3)));
  assert(!c.ConstantTime.equals(Uint8Array.of(1, 2), Uint8Array.of(1)));
  assert(c.Mac$matches({ bytes: Uint8Array.of(9) }, Uint8Array.of(9)));
  assert(!c.Mac$matches({ bytes: Uint8Array.of(9) }, [9]));
});

test("hashes, macs and random values", async () => {
  const abc = utf8("abc");
  const d = await c.Hash.sha256(null, abc);
  assertEq(d.length, 32);
  assertEq(d[0], 0xba);
  assertEq((await c.Hash.sha512(null, abc)).length, 64);
  const mac = await c.Hmac.sha256(null, new Secret("Jefe"), utf8("what do ya want for nothing?"));
  assertEq(mac.hex, "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
  assertEq((await c.Hmac.sha512(null, Uint8Array.of(1), abc)).bytes.length, 64);
  assertEq(c.SecureRandom.bytes(7).length, 7);
  assert(/^[A-Za-z0-9_-]{22}$/.test(c.SecureRandom.token(16)));
});

test("test keys are the same for the same number and differ between numbers", () => {
  assertEq(c.Ed25519PrivateKey$publicKey(c.TestKeys.ed25519(1)), c.Ed25519PrivateKey$publicKey(c.TestKeys.ed25519(1)));
  assert(c.Ed25519PrivateKey$publicKey(c.TestKeys.ed25519(1)) !== c.Ed25519PrivateKey$publicKey(c.TestKeys.ed25519(2)));
  assertEq(c.X25519PrivateKey$publicKey(c.TestKeys.x25519(7)), c.X25519PrivateKey$publicKey(c.TestKeys.x25519(7)));
  assertEq(c.EcdsaP256PublicKey$raw(c.EcdsaP256PrivateKey$publicKey(c.TestKeys.ecdsaP256(3)))[0], 4);
});

test("raw secret keys parse and export on the server", () => {
  const raw = new Uint8Array(32).fill(7);
  const k = c.Ed25519PrivateKey$parse$raw(new Secret(raw));
  assert(k);
  assertEq(c.Ed25519PrivateKey$parse$raw(new Secret(new Uint8Array(31))), null);
  assertEq(c.AesGcmKey$parse$raw(new Secret(new Uint8Array(16))), null, "AES keys are 256 bits");
  assertEq(c.EcdsaP256PrivateKey$parse$raw(new Secret(new Uint8Array(32))), null, "zero isn't a scalar");
  const e = c.Exportable$exportRaw(c.TestKeys.x25519(1));
  assert(e instanceof Secret);
});

// ------------------------------------------------- key formats and JOSE

test("the ops backend matches the key-format, RSA and JOSE vectors", async () => {
  const failures = await checkJoseVectors(c, c.ops, vectors);
  assertEq(failures.length, 0, failures.slice(0, 10).join("\n"));
});

test("every key type round-trips through JWK and PEM, and refuses another type's", () => {
  for (const [kind, type] of [
    ["ed25519", "Ed25519"],
    ["x25519", "X25519"],
    ["ecdsaP256", "EcdsaP256"],
  ]) {
    const pub = c[`${type}PrivateKey$publicKey`](c.TestKeys[kind](1));
    const jwk = c[`${type}PublicKey$jwk`](pub);
    const pem = c[`${type}PublicKey$pem`](pub);
    assertEq(c[`${type}PublicKey$parse$jwk`](jwk), pub, jwk);
    assertEq(c[`${type}PublicKey$parse$pem`](pem), pub, pem);
    assert(pem.startsWith("-----BEGIN PUBLIC KEY-----\n") && pem.endsWith("-----END PUBLIC KEY-----\n"), pem);
    for (const other of ["Ed25519", "X25519", "EcdsaP256"].filter((o) => o !== type)) {
      assertEq(c[`${other}PublicKey$parse$jwk`](jwk), null, `${type} as ${other}`);
      assertEq(c[`${other}PublicKey$parse$pem`](pem), null, `${type} as ${other}`);
    }
  }
  // A private JWK isn't a public key.
  assertEq(c.Ed25519PublicKey$parse$jwk(JSON.stringify({ kty: "OKP", crv: "Ed25519", x: "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo", d: "nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A" })), null);
  // `alg`, `use` and `key_ops` must fit the type when present.
  const x = "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo";
  assert(c.Ed25519PublicKey$parse$jwk(JSON.stringify({ kty: "OKP", crv: "Ed25519", x, alg: "EdDSA", use: "sig", key_ops: ["verify"] })));
  assertEq(c.Ed25519PublicKey$parse$jwk(JSON.stringify({ kty: "OKP", crv: "Ed25519", x, alg: "ES256" })), null);
  assertEq(c.Ed25519PublicKey$parse$jwk(JSON.stringify({ kty: "OKP", crv: "Ed25519", x, use: "enc" })), null);
  assertEq(c.Ed25519PublicKey$parse$jwk(JSON.stringify({ kty: "OKP", crv: "Ed25519", x, key_ops: ["encrypt"] })), null);
  assertEq(c.Ed25519PublicKey$parse$jwk("not json"), null);
  assertEq(c.Ed25519PublicKey$parse$pem("-----BEGIN PUBLIC KEY-----\nnot base64!\n-----END PUBLIC KEY-----"), null);
});

test("secret keys import from JWK and PKCS #8, and export as exportable keys", async () => {
  for (const [kind, type] of [
    ["ed25519", "Ed25519PrivateKey"],
    ["x25519", "X25519PrivateKey"],
    ["ecdsaP256", "EcdsaP256PrivateKey"],
    ["aesGcm", "AesGcmKey"],
  ]) {
    const raw = new Secret(c.ops.exportRaw(c.TestKeys[kind](1)));
    const e = c.Exportable$parse$raw(type, raw);
    assert(e && e.exportable, type);
    const jwk = c.Exportable$exportJwk(e);
    assert(jwk instanceof Secret, "a Secret");
    const back = c.Exportable$parse$jwk(type, jwk);
    assertEq(hex(c.ops.exportRaw(back)), hex(raw.reveal()), `${type} JWK`);
    assertEq(c[`${type}$parse$jwk`](jwk).exportable, false, "a plain import isn't exportable");
    if (type === "AesGcmKey") continue;
    const pem = c.Exportable$exportPem(e);
    assert(pem.reveal().startsWith("-----BEGIN PRIVATE KEY-----"), pem.reveal());
    assertEq(hex(c.ops.exportRaw(c[`${type}$parse$pem`](pem))), hex(raw.reveal()), `${type} PEM`);
    // A JWK whose public half isn't the key's own is refused.
    const j = JSON.parse(jwk.reveal());
    j.x = c.b64url(new Uint8Array(32).fill(9));
    assertEq(c[`${type}$parse$jwk`](new Secret(JSON.stringify(j))), null, `${type} with another public half`);
  }
  // A P-256 PKCS #8 as OpenSSL writes it, with the curve and the public key
  // (RFC 7515 A.3's key), and RFC 8410 §10.3's version 2 Ed25519 key.
  const p256 = c.EcdsaP256PrivateKey$parse$pem(
    new Secret("-----BEGIN PRIVATE KEY-----\nMIGTAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBHkwdwIBAQQgjpsQnnGQmL+YBIff\nH1136cspYG6+0iY7X1fCE9+E9LKgCgYIKoZIzj0DAQehRANCAAR/zc4ncPbEXUGD\ny+5v20t7WAczNXvp7xO6z248e9FURcfxRM0bvZt+hyzf7bnuufSzaV1uqQskrYpG\nIyiFiOWt\n-----END PRIVATE KEY-----"),
  );
  assertEq(c.EcdsaP256PrivateKey$publicKey(p256), c.EcdsaP256PublicKey$parse$jwk(JSON.stringify({ kty: "EC", crv: "P-256", x: "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU", y: "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0" })));
  const v2 = c.Ed25519PrivateKey$parse$pem(new Secret("-----BEGIN PRIVATE KEY-----\nMHICAQEwBQYDK2VwBCIEINTuctv5E1hK1bbY8fdp+K06/nwoy/HU++CXqI9EdVhC\noB8wHQYKKoZIhvcNAQkJFDEPDA1DdXJkbGUgQ2hhaXJzgSEAGb9ECWmEzf6FQbrB\nZ9w7lshQhqowtrbLDFw4rXAxZuE=\n-----END PRIVATE KEY-----"));
  assertEq(c.Ed25519PrivateKey$publicKey(v2), "Gb9ECWmEzf6FQbrBZ9w7lshQhqowtrbLDFw4rXAxZuE");
  assert(throws(() => c.Exportable$exportPem(c.Exportable$parse$raw("AesGcmKey", new Secret(new Uint8Array(32))))), "an AES key has no PEM");
});

test("configured keys parse from a JWK, a PEM or base64url, and never print", () => {
  const seed = c.b64url(new Uint8Array(32).fill(7));
  const k = c.keyFromConfig("Ed25519PrivateKey", seed);
  assertEq(String(k), "Ed25519PrivateKey(•••)");
  const pub = c.Ed25519PrivateKey$publicKey(k);
  assertEq(c.keyFromConfig("Ed25519PublicKey", pub), pub);
  assertEq(c.keyFromConfig("Ed25519PublicKey", c.Ed25519PublicKey$jwk(pub)), pub);
  assertEq(c.keyFromConfig("Ed25519PublicKey", ` ${c.Ed25519PublicKey$pem(pub)} `), pub);
  const jwk = c.Exportable$exportJwk(c.Exportable$parse$raw("Ed25519PrivateKey", new Secret(new Uint8Array(32).fill(7)))).reveal();
  assertEq(c.Ed25519PrivateKey$publicKey(c.keyFromConfig("Ed25519PrivateKey", jwk)), pub);
  const message = (() => {
    try {
      c.keyFromConfig("X25519PrivateKey", jwk);
    } catch (e) {
      return String(e?.message ?? e);
    }
    return "";
  })();
  assert(message.includes("X25519PrivateKey") && !message.includes(seed), message);
});

test("JWTs carry iat and exp, and verify only within their window", async () => {
  const claims = { encode: (v) => ({ iss: v.iss, aud: v.aud, sub: v.sub }), decode: (j) => ({ iss: j.iss, aud: j.aud, sub: j.sub }) };
  const now = new Instant(1_700_000_000_000);
  const key = c.TestKeys.ed25519(3);
  const pub = c.Ed25519PrivateKey$publicKey(key);
  const token = await c.Jwt.sign(null, claims, { iss: "i", aud: "a", sub: "s" }, new Secret(key), "k1", 3_600_000, now);
  const [head, body] = token.split(".").slice(0, 2).map((p) => JSON.parse(new TextDecoder().decode(c.fromB64url(p))));
  assertDeepEq(head, { alg: "EdDSA", typ: "JWT", kid: "k1" });
  assertDeepEq(body, { iss: "i", aud: "a", sub: "s", iat: 1_700_000_000, exp: 1_700_003_600 });
  const verify = (at, opts = {}) => c.Jwt.verify(null, claims, token, opts.key ?? pub, opts.iss ?? "i", opts.aud ?? "a", new Instant(at));
  assertDeepEq(await verify(1_700_000_000_000), { iss: "i", aud: "a", sub: "s" });
  // A minute's leeway after `exp`, then `.expired`.
  await verify(1_700_003_659_000);
  assertDeepEq(await rejection(verify(1_700_003_661_000)), { $: 4 });
  assertDeepEq(await rejection(verify(1_700_000_000_000, { iss: "j" })), { $: 6 });
  assertDeepEq(await rejection(verify(1_700_000_000_000, { aud: "b" })), { $: 7 });
  assertDeepEq(await rejection(verify(1_700_000_000_000, { key: c.Ed25519PrivateKey$publicKey(c.TestKeys.ed25519(4)) })), { $: 3 });
  assertDeepEq(await rejection(verify(1_700_000_000_000, { key: c.EcdsaP256PrivateKey$publicKey(c.TestKeys.ecdsaP256(1)) })), { $: 1, _0: "EdDSA" });
  // `alg: none` is never accepted, and a token without `exp` isn't a JWT here.
  const none = `${c.b64url(utf8('{"alg":"none"}'))}.${token.split(".")[1]}.`;
  assertDeepEq(await rejection(c.Jwt.verify(null, claims, none, pub, "i", "a", now)), { $: 1, _0: "none" });
  const bare = await c.Jws.sign(null, utf8('{"iss":"i","aud":"a"}'), key);
  assertDeepEq(await rejection(c.Jwt.verify(null, claims, bare, pub, "i", "a", now)), { $: 8 });
  // A short HMAC secret is a panic, not a weak key.
  assert(await rejection(c.Jws.sign(null, utf8("x"), new Secret("short"))) instanceof Error);
});
