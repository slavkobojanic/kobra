/**
 * @module k:/tests/crypto-vectors.js
 *
 * Runs the published vectors of `crypto-vectors.json` against one backend
 * of `k:runtime/crypto` (docs/design/33-crypto.md, "Testing"): the `ops`
 * backend in `crypto.test.js`, and the `subtle` backend in real Chrome
 * (`k_cli`'s `chrome::crypto_subtle_matches_the_vectors_and_the_ops_backend`).
 * It imports nothing, so a browser can load it next to `crypto.js`.
 */

/** Hex as bytes. */
const bin = (h) => Uint8Array.from((h ?? "").match(/../g) ?? [], (x) => parseInt(x, 16));

/** Bytes as hex, `null` as `null`. */
const hex = (b) => (b === null || b === undefined ? null : Array.from(b, (x) => x.toString(16).padStart(2, "0")).join(""));

/**
 * Checks every vector of `v` on backend `b` of crypto module `c`.
 * @param {object} c `k:runtime/crypto`'s exports
 * @param {object} b `c.ops` or `c.subtle`
 * @param {object} v the parsed `crypto-vectors.json`
 * @returns {Promise<string[]>} one line per failed vector; empty when all pass
 */
export async function checkVectors(c, b, v) {
  const failures = [];
  const check = (what, actual, expected) => {
    if (actual !== expected) failures.push(`${b.name} ${what}: expected ${expected}, got ${actual}`);
  };
  const run = async (what, f) => {
    try {
      await f();
    } catch (e) {
      failures.push(`${b.name} ${what}: threw ${e?.message ?? e}`);
    }
  };
  for (const [i, t] of v.aesGcm.entries()) {
    await run(`aes-gcm ${t.id ?? i}`, async () => {
      const key = await b.fromRaw("AesGcmKey", bin(t.key), false);
      if (t.valid) check(`aes-gcm ${t.id ?? i} seal`, hex(await b.aesSeal(key, bin(t.nonce), bin(t.aad), bin(t.pt))), t.ct);
      check(`aes-gcm ${t.id ?? i} open`, hex(await b.aesOpen(key, bin(t.nonce), bin(t.aad), bin(t.ct))), t.valid ? t.pt : null);
    });
  }
  for (const [i, t] of v.hkdf.entries()) {
    await run(`hkdf ${t.id ?? i}`, async () => {
      check(`hkdf ${t.id ?? i}`, hex(await b.hkdf(bin(t.ikm), bin(t.salt), bin(t.info), t.okm.length / 2)), t.okm);
    });
  }
  for (const [i, t] of v.ed25519.entries()) {
    await run(`ed25519 ${t.id ?? i}`, async () => {
      if (t.seed) {
        const key = await b.fromRaw("Ed25519PrivateKey", bin(t.seed), false);
        check(`ed25519 ${i} public`, hex(key.pub), t.public);
        check(`ed25519 ${i} sign`, hex(await b.sign(key, bin(t.msg))), t.sig);
      }
      check(`ed25519 ${t.id ?? i} verify`, await b.verify("Ed25519PublicKey", bin(t.public), bin(t.sig), bin(t.msg)), t.valid);
    });
  }
  for (const [i, t] of v.x25519.entries()) {
    await run(`x25519 ${t.id ?? i}`, async () => {
      const key = await b.fromRaw("X25519PrivateKey", bin(t.secret), false);
      if (t.ownPublic) check(`x25519 ${i} public`, hex(key.pub), t.ownPublic);
      check(`x25519 ${t.id ?? i} agree`, hex(await b.agree(key, bin(t.public))), t.shared);
    });
  }
  for (const [i, t] of v.p256.entries()) {
    await run(`p256 ${t.id ?? i}`, async () => {
      if (t.scalar) {
        // The wrapped form, `d ‖ x ‖ y`: Web Crypto imports a P-256 private key only with its point.
        const key = await b.fromRaw("EcdsaP256PrivateKey", Uint8Array.from([...bin(t.scalar), ...bin(t.public).subarray(1)]), false);
        check(`p256 ${i} public`, hex(key.pub), t.public);
        const ours = await b.sign(key, bin(t.msg));
        check(`p256 ${i} own signature`, await b.verify("EcdsaP256PublicKey", key.pub, ours, bin(t.msg)), true);
      }
      check(`p256 ${t.id ?? i} verify`, await b.verify("EcdsaP256PublicKey", bin(t.public), bin(t.sig), bin(t.msg)), t.valid);
    });
  }
  for (const t of v.hpke) {
    await run(`hpke aead ${t.aead}`, async () => {
      const sealed = await c.hpkeSeal(b, bin(t.pkRm), bin(t.info), bin(t.aad), bin(t.pt), t.aead, bin(t.skEm));
      check(`hpke ${t.aead} enc`, hex(sealed.enc), t.enc);
      check(`hpke ${t.aead} shared secret`, hex(sealed.shared), t.sharedSecret);
      check(`hpke ${t.aead} key`, hex(sealed.key), t.key);
      check(`hpke ${t.aead} base nonce`, hex(sealed.nonce), t.baseNonce);
      check(`hpke ${t.aead} ciphertext`, hex(sealed.ct), t.ct);
      const sk = await b.fromRaw("X25519PrivateKey", bin(t.skRm), false);
      check(`hpke ${t.aead} recipient public`, hex(sk.pub), t.pkRm);
      check(`hpke ${t.aead} open`, hex(await c.hpkeOpen(b, sk, bin(t.enc), bin(t.info), bin(t.aad), bin(t.ct), t.aead)), t.pt);
      check(`hpke ${t.aead} open with other aad`, await c.hpkeOpen(b, sk, bin(t.enc), bin(t.info), new Uint8Array(1), bin(t.ct), t.aead), null);
    });
  }
  return failures;
}

/**
 * Checks the key-format, RSA and JOSE vectors of `v` through the K members
 * of crypto module `c` (`RsaPublicKey$verify`, `Jws.verify`, `JwkSet$parse`,
 * …), on whichever backend `c` picks, and the private-key formats through
 * {@link c.privateRaw} and backend `b`.
 * @returns {Promise<string[]>} one line per failed vector
 */
export async function checkJoseVectors(c, b, v) {
  const failures = [];
  const check = (what, actual, expected) => {
    if (actual !== expected) failures.push(`${b.name} ${what}: expected ${expected}, got ${actual}`);
  };
  const run = async (what, f) => {
    try {
      await f();
    } catch (e) {
      failures.push(`${b.name} ${what}: threw ${e?.message ?? JSON.stringify(e)}`);
    }
  };
  /** Whether `p` resolves (a verified JWS) rather than rejecting. */
  const accepts = async (p) => {
    try {
      await p;
      return true;
    } catch {
      return false;
    }
  };
  for (const [g, group] of v.rsa.entries()) {
    await run(`rsa group ${g}`, async () => {
      const key = c.RsaPublicKey$parse$pem(group.pem);
      check(`rsa group ${g} pem`, typeof key, "string");
      check(`rsa group ${g} jwk`, c.RsaPublicKey$parse$jwk(group.jwk), key);
      check(`rsa group ${g} pem round trip`, c.RsaPublicKey$parse$pem(c.RsaPublicKey$pem(key)), key);
      const verify = group.pss ? c.RsaPublicKey$verifyPss : c.RsaPublicKey$verify;
      for (const [i, t] of group.tests.entries()) {
        const sig = c.b64url(bin(t.sig));
        check(`rsa ${group.pss ? "pss" : "pkcs1"} ${g}/${i}`, await verify(key, null, sig, bin(t.msg)), t.valid);
      }
    });
  }
  /** The verification key of a JWS vector: a public JWK of the right type, or an HMAC secret. */
  const keyOf = (t) => {
    if (t.hmac !== undefined) return c.secretOf(c.fromB64url(t.hmac) ?? new Uint8Array(0));
    for (const type of ["Ed25519PublicKey", "EcdsaP256PublicKey", "RsaPublicKey"]) {
      const k = c[`${type}$parse$jwk`](t.jwk ?? t.public);
      if (k !== null) return k;
    }
    return null;
  };
  for (const group of v.jws) {
    const key = keyOf(group);
    for (const t of group.tests) {
      await run(`jws ${group.comment} ${t.id}`, async () => {
        check(`jws ${group.comment} ${t.id}`, key !== null && (await accepts(c.Jws.verify(null, t.jws, key))), t.valid);
      });
    }
  }
  for (const t of v.jwkSet) {
    await run(`jwk set ${t.id}`, async () => {
      const key = t.hmac !== undefined ? keyOf(t) : c.JwkSet$parse(t.jwks);
      check(`jwk set ${t.id}`, key !== null && (await accepts(c.Jws.verify(null, t.jws, key))), t.valid);
    });
  }
  for (const [name, form] of [
    ["x25519Jwk", "jwk"],
    ["x25519Pem", "pem"],
  ]) {
    for (const t of v[name]) {
      await run(`${name} ${t.id}`, async () => {
        const pub = form === "jwk" ? c.X25519PublicKey$parse$jwk(t.public) : c.X25519PublicKey$parse$pem(t.public);
        const priv = c.privateRaw("X25519PrivateKey", form, t.private);
        let shared = null;
        if (pub !== null && priv !== null) {
          const key = await b.fromRaw("X25519PrivateKey", priv.raw, false);
          if (!priv.x || hex(priv.x) === hex(key.pub)) shared = hex(await b.agree(key, c.fromB64url(pub)));
        }
        check(`${name} ${t.id}`, shared, t.valid ? t.shared : null);
      });
    }
  }
  for (const t of v.rfcJose) {
    await run(`rfc ${t.rfc}`, async () => {
      const key = keyOf(t);
      check(`rfc ${t.rfc} verifies`, await accepts(c.Jws.verify(null, t.jws, key)), true);
      if (t.private) {
        const type = t.rfc === "8037 A.4" ? "Ed25519PrivateKey" : "EcdsaP256PrivateKey";
        const priv = c.privateRaw(type, "jwk", t.private);
        const k = await b.fromRaw(type, priv.raw, false);
        check(`rfc ${t.rfc} public half`, c.b64url(k.pub), c.b64url(c.fromB64url(key) ?? new Uint8Array(0)));
        const ours = await c.Jws.sign(null, t.payload ? bin(t.payload) : c.fromB64url(t.jws.split(".")[1]), k);
        if (t.deterministic) check(`rfc ${t.rfc} signs`, ours, t.jws);
        check(`rfc ${t.rfc} own signature verifies`, await accepts(c.Jws.verify(null, ours, key)), true);
      }
      if (t.raw) check(`rfc ${t.rfc} raw`, hex(c.fromB64url(key)), t.raw);
    });
  }
  return failures;
}
