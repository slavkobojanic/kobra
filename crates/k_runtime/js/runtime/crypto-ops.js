/**
 * @module k:runtime/crypto-ops
 *
 * The server's half of `k.crypto`'s `ops` backend (docs/design/33-crypto.md,
 * "Implementation"): the Rust ops of `k_server`'s `crypto.rs` under the
 * names `crypto.js` calls, handed over with `useOps(cryptoOps(ops))`.
 * Server-only, so the browser's crypto chunk names no op.
 */

/**
 * `crypto.js`'s table of the isolate's ops `o`.
 * @param {object} o `Deno.core.ops`
 */
export function cryptoOps(o) {
  return {
    random: (count) => o.op_k_crypto_random(count),
    digest: (bits, data) => o.op_k_crypto_digest(bits, data),
    hmac: (bits, key, data) => o.op_k_hmac(bits, key, data),
    hkdf: (ikm, salt, info, length) => o.op_k_crypto_hkdf(ikm, salt, info, length),
    pbkdf2: (passphrase, salt, iterations) => o.op_k_crypto_pbkdf2(passphrase, salt, iterations),
    aesSeal: (key, nonce, aad, plaintext) => o.op_k_crypto_aes_seal(key, nonce, aad, plaintext),
    aesOpen: (key, nonce, aad, sealed) => o.op_k_crypto_aes_open(key, nonce, aad, sealed),
    ed25519Public: (seed) => o.op_k_crypto_ed25519_public(seed),
    ed25519Sign: (seed, message) => o.op_k_crypto_ed25519_sign(seed, message),
    ed25519Verify: (pub, sig, message) => o.op_k_crypto_ed25519_verify(pub, sig, message),
    p256Public: (scalar) => o.op_k_crypto_p256_public(scalar),
    p256Sign: (scalar, message) => o.op_k_crypto_p256_sign(scalar, message),
    p256Verify: (pub, sig, message) => o.op_k_crypto_p256_verify(pub, sig, message),
    x25519Public: (secret) => o.op_k_crypto_x25519_public(secret),
    x25519Agree: (secret, pub) => o.op_k_crypto_x25519_agree(secret, pub),
    equals: (a, b) => o.op_k_crypto_equals(a, b),
    rsaVerify: (n, e, sig, message, pss) => o.op_k_crypto_rsa_verify(n, e, sig, message, pss),
  };
}
