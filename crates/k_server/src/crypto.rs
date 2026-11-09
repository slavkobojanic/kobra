//! `k.crypto`'s primitives on the server: the `ops` backend of the
//! stdlib's crypto module (`k:/runtime/crypto.js`,
//! `docs/design/33-crypto.md`, "Implementation").
//!
//! Each primitive is a plain function, tested against published vectors
//! below, and an op that `crypto.js` calls. AES-256-GCM, HKDF-SHA256,
//! PBKDF2-HMAC-SHA256, Ed25519 and ECDSA P-256 come from `ring` (already
//! compiled in as rustls's provider); static X25519 keys come from
//! `x25519-dalek`, because `ring`'s agreement keys are ephemeral only.
//! Composite protocols (HPKE, the `Sealed` wire format, key wrapping) are
//! built from these calls in `crypto.js`, the same way for both backends.
//!
//! | Op | JavaScript signature |
//! |---|---|
//! | `op_k_crypto_random` | `(count) => Uint8Array` (the OS generator) |
//! | `op_k_crypto_digest` | `(bits, data) => Uint8Array` (SHA-256 or SHA-512) |
//! | `op_k_crypto_hkdf` | `(ikm, salt, info, length) => Uint8Array \| null` |
//! | `op_k_crypto_pbkdf2` | `(passphrase, salt, iterations) => Promise<Uint8Array \| null>` |
//! | `op_k_crypto_aes_seal` | `(key, nonce, aad, plaintext) => Uint8Array \| null` (ciphertext and tag) |
//! | `op_k_crypto_aes_open` | `(key, nonce, aad, sealed) => Uint8Array \| null` (`null`: not authentic) |
//! | `op_k_crypto_ed25519_public` | `(seed) => Uint8Array \| null` |
//! | `op_k_crypto_ed25519_sign` | `(seed, message) => Uint8Array \| null` |
//! | `op_k_crypto_ed25519_verify` | `(public, signature, message) => boolean` |
//! | `op_k_crypto_p256_public` | `(scalar) => Uint8Array \| null` (uncompressed point) |
//! | `op_k_crypto_p256_sign` | `(scalar, message) => Uint8Array \| null` (`r‖s`) |
//! | `op_k_crypto_p256_verify` | `(public, signature, message) => boolean` |
//! | `op_k_crypto_x25519_public` | `(secret) => Uint8Array \| null` |
//! | `op_k_crypto_x25519_agree` | `(secret, public) => Uint8Array \| null` (`null`: an all-zero result) |
//! | `op_k_crypto_equals` | `(a, b) => boolean`, in time that depends only on the lengths |
//! | `op_k_crypto_rsa_verify` | `(n, e, signature, message, pss) => boolean` (RSASSA-PKCS1-v1_5 or PSS, SHA-256) |
//!
//! [`new_key_jwks`] makes the keys `k keys new` prints.

use std::num::NonZeroU32;

use deno_core::op2;
use ring::signature::{self, EcdsaKeyPair, Ed25519KeyPair, KeyPair};
use ring::{aead, digest, hkdf, pbkdf2};

/// SHA-256 (`bits` 256) or SHA-512 (any other) of `data`.
#[must_use]
pub fn digest(bits: u32, data: &[u8]) -> Vec<u8> {
    let alg = if bits == 512 {
        &digest::SHA512
    } else {
        &digest::SHA256
    };
    digest::digest(alg, data).as_ref().to_vec()
}

/// An HKDF output length, for `ring`'s typed expand.
struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

/// HKDF-SHA256 (RFC 5869) of `ikm` with `salt` and `info`, `len` bytes
/// long; `None` past HKDF's limit of 255 × 32 bytes. An empty salt is the
/// RFC's default, 32 zero bytes.
#[must_use]
pub fn hkdf_sha256(ikm: &[u8], salt: &[u8], info: &[u8], len: usize) -> Option<Vec<u8>> {
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(ikm);
    let info = [info];
    let okm = prk.expand(&info, Len(len)).ok()?;
    let mut out = vec![0; len];
    okm.fill(&mut out).ok()?;
    Some(out)
}

/// PBKDF2-HMAC-SHA256 of `passphrase` with `salt`: a 32-byte key; `None`
/// for zero iterations.
#[must_use]
pub fn pbkdf2_sha256(passphrase: &[u8], salt: &[u8], iterations: u32) -> Option<Vec<u8>> {
    let n = NonZeroU32::new(iterations)?;
    let mut out = vec![0; 32];
    pbkdf2::derive(pbkdf2::PBKDF2_HMAC_SHA256, n, salt, passphrase, &mut out);
    Some(out)
}

/// The AES-GCM key `key`: AES-256 for 32 bytes, and AES-128 for 16, which
/// only HPKE's AES-128-GCM test vectors use (`AesGcmKey` is always 256
/// bits); `None` for any other length.
fn aes_key(key: &[u8]) -> Option<aead::LessSafeKey> {
    let alg = if key.len() == 16 {
        &aead::AES_128_GCM
    } else {
        &aead::AES_256_GCM
    };
    aead::UnboundKey::new(alg, key)
        .ok()
        .map(aead::LessSafeKey::new)
}

/// AES-GCM: `plaintext` encrypted under `key` with the 12-byte
/// `nonce`, authenticating `aad`, as the ciphertext followed by the
/// 16-byte tag; `None` for a key or nonce of the wrong length.
#[must_use]
pub fn aes_gcm_seal(key: &[u8], nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> Option<Vec<u8>> {
    let key = aes_key(key)?;
    let nonce = aead::Nonce::try_assume_unique_for_key(nonce).ok()?;
    let mut out = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, aead::Aad::from(aad), &mut out)
        .ok()?;
    Some(out)
}

/// The plaintext of AES-256-GCM `sealed` (ciphertext and tag), or `None`
/// when it isn't authentic under `key`, `nonce` and `aad`, or a length is
/// wrong.
#[must_use]
pub fn aes_gcm_open(key: &[u8], nonce: &[u8], aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    let key = aes_key(key)?;
    let nonce = aead::Nonce::try_assume_unique_for_key(nonce).ok()?;
    let mut buf = sealed.to_vec();
    let len = key
        .open_in_place(nonce, aead::Aad::from(aad), &mut buf)
        .ok()?
        .len();
    buf.truncate(len);
    Some(buf)
}

/// The Ed25519 key pair of the 32-byte `seed` (RFC 8032's private key).
fn ed25519_pair(seed: &[u8]) -> Option<Ed25519KeyPair> {
    Ed25519KeyPair::from_seed_unchecked(seed).ok()
}

/// The Ed25519 public key of `seed`.
#[must_use]
pub fn ed25519_public(seed: &[u8]) -> Option<Vec<u8>> {
    Some(ed25519_pair(seed)?.public_key().as_ref().to_vec())
}

/// The Ed25519 signature of `message` under `seed` (deterministic).
#[must_use]
pub fn ed25519_sign(seed: &[u8], message: &[u8]) -> Option<Vec<u8>> {
    Some(ed25519_pair(seed)?.sign(message).as_ref().to_vec())
}

/// Whether `sig` is a valid Ed25519 signature of `message` by `public`;
/// `false` for anything malformed.
#[must_use]
pub fn ed25519_verify(public: &[u8], sig: &[u8], message: &[u8]) -> bool {
    signature::UnparsedPublicKey::new(&signature::ED25519, public)
        .verify(message, sig)
        .is_ok()
}

/// The ECDSA P-256 key pair of the 32-byte big-endian `scalar`, or `None`
/// when it isn't a valid private key (zero, or not below the group order).
///
/// `ring` builds a P-256 pair only from PKCS #8 or from both halves, so
/// `p256` checks the scalar and computes its public point, and `ring` takes
/// both. Signing draws its nonces from the operating system.
fn p256_pair(scalar: &[u8]) -> Option<EcdsaKeyPair> {
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    if scalar.len() != 32 {
        return None;
    }
    let secret = p256::SecretKey::from_slice(scalar).ok()?;
    let point = secret.public_key().to_encoded_point(false);
    EcdsaKeyPair::from_private_key_and_public_key(
        &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
        scalar,
        point.as_bytes(),
        &ring::rand::SystemRandom::new(),
    )
    .ok()
}

/// The uncompressed public point (65 bytes, `0x04 ‖ x ‖ y`) of `scalar`.
#[must_use]
pub fn p256_public(scalar: &[u8]) -> Option<Vec<u8>> {
    Some(p256_pair(scalar)?.public_key().as_ref().to_vec())
}

/// The ECDSA P-256 SHA-256 signature of `message` under `scalar`, as the
/// 64-byte `r ‖ s` Web Crypto and JWS use; the nonce is random.
#[must_use]
pub fn p256_sign(scalar: &[u8], message: &[u8]) -> Option<Vec<u8>> {
    let pair = p256_pair(scalar)?;
    let sig = pair.sign(&ring::rand::SystemRandom::new(), message).ok()?;
    Some(sig.as_ref().to_vec())
}

/// Whether `sig` (`r ‖ s`) is a valid ECDSA P-256 SHA-256 signature of
/// `message` by the uncompressed point `public`; `false` for anything
/// malformed, including a point that isn't on the curve.
#[must_use]
pub fn p256_verify(public: &[u8], sig: &[u8], message: &[u8]) -> bool {
    signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, public)
        .verify(message, sig)
        .is_ok()
}

/// `bytes` as a 32-byte array, or `None`.
fn array32(bytes: &[u8]) -> Option<[u8; 32]> {
    bytes.try_into().ok()
}

/// The X25519 public key of the 32-byte `secret` (RFC 7748, clamped).
#[must_use]
pub fn x25519_public(secret: &[u8]) -> Option<Vec<u8>> {
    let s = x25519_dalek::StaticSecret::from(array32(secret)?);
    Some(x25519_dalek::PublicKey::from(&s).as_bytes().to_vec())
}

/// X25519(`secret`, `public`), or `None` when a length is wrong or the
/// result is all zeros (a small-order public key: docs/design/33-crypto.md,
/// "Key agreement").
#[must_use]
pub fn x25519_agree(secret: &[u8], public: &[u8]) -> Option<Vec<u8>> {
    let s = x25519_dalek::StaticSecret::from(array32(secret)?);
    let p = x25519_dalek::PublicKey::from(array32(public)?);
    let shared = s.diffie_hellman(&p);
    if !shared.was_contributory() {
        return None;
    }
    Some(shared.as_bytes().to_vec())
}

/// Whether `a` and `b` are equal, in time that depends only on their
/// lengths: different lengths return at once, equal ones are compared
/// with no data-dependent branch (`ConstantTime.equals`).
#[must_use]
pub fn constant_time_equals(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let diff = a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y));
    std::hint::black_box(diff) == 0
}

/// Whether `sig` is a valid RSA signature with SHA-256 of `message` by
/// the public key with modulus `n` and exponent `e` (big-endian, no
/// leading zeros): RSASSA-PSS with MGF1-SHA-256 and a 32-byte salt when
/// `pss`, else RSASSA-PKCS1-v1_5. Verify only, for interop
/// (`RsaPublicKey`); moduli of 2048 to 8192 bits, as `ring` accepts.
#[must_use]
pub fn rsa_verify(n: &[u8], e: &[u8], sig: &[u8], message: &[u8], pss: bool) -> bool {
    let params = if pss {
        &signature::RSA_PSS_2048_8192_SHA256
    } else {
        &signature::RSA_PKCS1_2048_8192_SHA256
    };
    signature::RsaPublicKeyComponents { n, e }
        .verify(params, message, sig)
        .is_ok()
}

/// `bytes` as unpadded base64url (RFC 4648 §5), as JWKs write them.
#[must_use]
pub fn base64url(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(char::from(A[(n >> (18 - 6 * i) & 63) as usize]));
        }
    }
    out
}

/// A new key of `kind` (`ed25519`, `x25519`, `p256-ecdsa` or `aes-gcm`)
/// from the operating system's generator, as `k keys new` prints it: the
/// private JWK, with `alg`, `use` and `key_ops` set for the key's type
/// (`docs/design/33-crypto.md`, "Keys in configuration"), and the public
/// JWK, `None` for an AES key. `None` for another kind.
///
/// # Errors
///
/// The operating system refused randomness.
pub fn new_key_jwks(kind: &str) -> Result<Option<(String, Option<String>)>, String> {
    let random = || -> Result<[u8; 32], String> {
        let mut b = [0u8; 32];
        getrandom::fill(&mut b).map_err(|e| e.to_string())?;
        Ok(b)
    };
    let jwk = |fields: &[(&str, serde_json::Value)]| {
        let mut m = serde_json::Map::new();
        for (k, v) in fields {
            m.insert((*k).to_owned(), v.clone());
        }
        serde_json::Value::Object(m).to_string()
    };
    let s = |t: &str| serde_json::Value::String(t.to_owned());
    let ops = |o: &[&str]| serde_json::Value::Array(o.iter().map(|x| s(x)).collect());
    Ok(Some(match kind {
        "aes-gcm" => {
            let k = random()?;
            let private = jwk(&[
                ("kty", s("oct")),
                ("k", s(&base64url(&k))),
                ("alg", s("A256GCM")),
                ("use", s("enc")),
                ("key_ops", ops(&["encrypt", "decrypt"])),
            ]);
            (private, None)
        }
        "ed25519" | "x25519" => {
            let d = random()?;
            let (crv, x, alg, usage, private_ops, public_ops) = if kind == "ed25519" {
                let x = ed25519_public(&d).ok_or("Ed25519 key generation failed")?;
                (
                    "Ed25519",
                    x,
                    Some("EdDSA"),
                    "sig",
                    vec!["sign"],
                    vec!["verify"],
                )
            } else {
                let x = x25519_public(&d).ok_or("X25519 key generation failed")?;
                ("X25519", x, None, "enc", vec!["deriveBits"], vec![])
            };
            let mut private = vec![
                ("kty", s("OKP")),
                ("crv", s(crv)),
                ("x", s(&base64url(&x))),
                ("d", s(&base64url(&d))),
            ];
            let mut public = vec![("kty", s("OKP")), ("crv", s(crv)), ("x", s(&base64url(&x)))];
            if let Some(alg) = alg {
                private.push(("alg", s(alg)));
                public.push(("alg", s(alg)));
            }
            private.push(("use", s(usage)));
            public.push(("use", s(usage)));
            private.push(("key_ops", ops(&private_ops)));
            if !public_ops.is_empty() {
                public.push(("key_ops", ops(&public_ops)));
            }
            (jwk(&private), Some(jwk(&public)))
        }
        "p256-ecdsa" => {
            let (d, point) = loop {
                let d = random()?;
                if let Some(p) = p256_public(&d) {
                    break (d, p);
                }
            };
            let (x, y) = (base64url(&point[1..33]), base64url(&point[33..]));
            let common = |extra: Vec<(&'static str, serde_json::Value)>| {
                let mut f = vec![
                    ("kty", s("EC")),
                    ("crv", s("P-256")),
                    ("x", s(&x)),
                    ("y", s(&y)),
                ];
                f.extend(extra);
                jwk(&f)
            };
            let private = common(vec![
                ("d", s(&base64url(&d))),
                ("alg", s("ES256")),
                ("use", s("sig")),
                ("key_ops", ops(&["sign"])),
            ]);
            let public = common(vec![
                ("alg", s("ES256")),
                ("use", s("sig")),
                ("key_ops", ops(&["verify"])),
            ]);
            (private, Some(public))
        }
        _ => return Ok(None),
    }))
}

/// `true` for the `k.crypto` key types a config field may hold.
#[must_use]
pub fn is_config_key_type(name: &str) -> bool {
    matches!(
        name,
        "AesGcmKey"
            | "Ed25519PrivateKey"
            | "EcdsaP256PrivateKey"
            | "X25519PrivateKey"
            | "Ed25519PublicKey"
            | "EcdsaP256PublicKey"
            | "X25519PublicKey"
            | "RsaPublicKey"
    )
}

/// Base64 text as bytes: unpadded base64url when `url`, else standard
/// base64 with optional padding; `None` for anything else.
fn decode64(text: &str, url: bool) -> Option<Vec<u8>> {
    let t = if url {
        text
    } else {
        text.trim_end_matches('=')
    };
    let mut out = Vec::with_capacity(t.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in t.bytes() {
        let d = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' if url => 62,
            b'_' if url => 63,
            b'+' if !url => 62,
            b'/' if !url => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(d);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).ok()?);
        }
    }
    (t.len() % 4 != 1).then_some(out)
}

/// What is wrong with configuration text `text` for key type `ty`, checked
/// at startup without parsing the key (`k.crypto` does that): a JWK of
/// the type's `kty` and `crv` with its private member (`d`, or `k` for an
/// AES key) exactly when the type is secret, a PEM with the right label
/// (`PRIVATE KEY` or `PUBLIC KEY`), or base64url of the raw form's
/// length. `None` when it has the shape; the message never quotes the
/// text, which may be a secret.
#[must_use]
pub fn config_key_problem(ty: &str, text: &str) -> Option<String> {
    let t = text.trim();
    let public = ty.ends_with("PublicKey");
    let (kty, crv) = match ty {
        "AesGcmKey" => ("oct", None),
        "Ed25519PrivateKey" | "Ed25519PublicKey" => ("OKP", Some("Ed25519")),
        "X25519PrivateKey" | "X25519PublicKey" => ("OKP", Some("X25519")),
        "EcdsaP256PrivateKey" | "EcdsaP256PublicKey" => ("EC", Some("P-256")),
        _ => ("RSA", None),
    };
    let expected = format!("is not a {ty} (a JWK, a PEM, or base64url of the raw key)");
    if t.starts_with('{') {
        let Ok(serde_json::Value::Object(j)) = serde_json::from_str::<serde_json::Value>(t) else {
            return Some(format!("is not a {ty} JWK: not a JSON object"));
        };
        let text_of = |k: &str| j.get(k).and_then(serde_json::Value::as_str);
        if text_of("kty") != Some(kty) || crv.is_some_and(|c| text_of("crv") != Some(c)) {
            return Some(format!("is a JWK of another key type, not a {ty}"));
        }
        let secret_member = if ty == "AesGcmKey" { "k" } else { "d" };
        match (public, text_of(secret_member)) {
            (true, Some(_)) => {
                return Some(format!(
                    "is a private JWK; a {ty} field takes the public key only"
                ));
            }
            (false, None) => {
                return Some(format!(
                    "is a JWK without its private part (`{secret_member}`)"
                ));
            }
            (false, Some(d)) if decode64(d, true).is_none_or(|b| b.len() != 32) => {
                return Some(format!(
                    "is a {ty} JWK whose `{secret_member}` isn't 32 bytes of base64url"
                ));
            }
            _ => {}
        }
        return None;
    }
    if let Some(rest) = t.strip_prefix("-----BEGIN ") {
        let label = if public { "PUBLIC KEY" } else { "PRIVATE KEY" };
        let body = rest
            .strip_prefix(label)
            .and_then(|r| r.strip_prefix("-----"))
            .and_then(|r| r.strip_suffix(&format!("-----END {label}-----")));
        if ty == "AesGcmKey" {
            return Some("is a PEM; an AesGcmKey is a JWK or base64url of its 32 bytes".to_owned());
        }
        return match body {
            Some(b) if decode64(&b.split_whitespace().collect::<String>(), false).is_some() => None,
            _ => Some(format!("is not a {label} PEM")),
        };
    }
    let len = match ty {
        "EcdsaP256PublicKey" => 65,
        "RsaPublicKey" => return Some(format!("{expected}; an RsaPublicKey is a JWK or a PEM")),
        _ => 32,
    };
    match decode64(t, true) {
        Some(b) if b.len() == len => None,
        _ => Some(expected),
    }
}

/// `count` bytes from the operating system's generator.
///
/// # Errors
///
/// The operating system refused randomness.
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_random(#[smi] count: u32) -> Result<Vec<u8>, deno_error::JsErrorBox> {
    let mut out = vec![0; count as usize];
    getrandom::fill(&mut out).map_err(|e| deno_error::JsErrorBox::generic(e.to_string()))?;
    Ok(out)
}

/// [`digest`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_digest(bits: u32, #[buffer] data: &[u8]) -> Vec<u8> {
    digest(bits, data)
}

/// [`hkdf_sha256`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_hkdf(
    #[buffer] ikm: &[u8],
    #[buffer] salt: &[u8],
    #[buffer] info: &[u8],
    #[smi] len: u32,
) -> Option<Vec<u8>> {
    hkdf_sha256(ikm, salt, info, len as usize)
}

/// [`pbkdf2_sha256`], off the worker's thread: at 600,000 iterations it
/// takes a noticeable fraction of a second.
#[op2]
#[buffer]
pub(crate) async fn op_k_crypto_pbkdf2(
    #[buffer(copy)] passphrase: Vec<u8>,
    #[buffer(copy)] salt: Vec<u8>,
    iterations: u32,
) -> Option<Vec<u8>> {
    let work = move || pbkdf2_sha256(&passphrase, &salt, iterations);
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.spawn_blocking(work).await.ok().flatten(),
        Err(_) => work(),
    }
}

/// [`aes_gcm_seal`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_aes_seal(
    #[buffer] key: &[u8],
    #[buffer] nonce: &[u8],
    #[buffer] aad: &[u8],
    #[buffer] plaintext: &[u8],
) -> Option<Vec<u8>> {
    aes_gcm_seal(key, nonce, aad, plaintext)
}

/// [`aes_gcm_open`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_aes_open(
    #[buffer] key: &[u8],
    #[buffer] nonce: &[u8],
    #[buffer] aad: &[u8],
    #[buffer] sealed: &[u8],
) -> Option<Vec<u8>> {
    aes_gcm_open(key, nonce, aad, sealed)
}

/// [`ed25519_public`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_ed25519_public(#[buffer] seed: &[u8]) -> Option<Vec<u8>> {
    ed25519_public(seed)
}

/// [`ed25519_sign`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_ed25519_sign(
    #[buffer] seed: &[u8],
    #[buffer] message: &[u8],
) -> Option<Vec<u8>> {
    ed25519_sign(seed, message)
}

/// [`ed25519_verify`].
#[op2(fast)]
pub(crate) fn op_k_crypto_ed25519_verify(
    #[buffer] public: &[u8],
    #[buffer] sig: &[u8],
    #[buffer] message: &[u8],
) -> bool {
    ed25519_verify(public, sig, message)
}

/// [`p256_public`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_p256_public(#[buffer] scalar: &[u8]) -> Option<Vec<u8>> {
    p256_public(scalar)
}

/// [`p256_sign`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_p256_sign(
    #[buffer] scalar: &[u8],
    #[buffer] message: &[u8],
) -> Option<Vec<u8>> {
    p256_sign(scalar, message)
}

/// [`p256_verify`].
#[op2(fast)]
pub(crate) fn op_k_crypto_p256_verify(
    #[buffer] public: &[u8],
    #[buffer] sig: &[u8],
    #[buffer] message: &[u8],
) -> bool {
    p256_verify(public, sig, message)
}

/// [`x25519_public`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_x25519_public(#[buffer] secret: &[u8]) -> Option<Vec<u8>> {
    x25519_public(secret)
}

/// [`x25519_agree`].
#[op2]
#[buffer]
pub(crate) fn op_k_crypto_x25519_agree(
    #[buffer] secret: &[u8],
    #[buffer] public: &[u8],
) -> Option<Vec<u8>> {
    x25519_agree(secret, public)
}

/// [`constant_time_equals`].
#[op2(fast)]
pub(crate) fn op_k_crypto_equals(#[buffer] a: &[u8], #[buffer] b: &[u8]) -> bool {
    constant_time_equals(a, b)
}

/// [`rsa_verify`].
#[op2(fast)]
pub(crate) fn op_k_crypto_rsa_verify(
    #[buffer] n: &[u8],
    #[buffer] e: &[u8],
    #[buffer] sig: &[u8],
    #[buffer] message: &[u8],
    pss: bool,
) -> bool {
    rsa_verify(n, e, sig, message, pss)
}

#[cfg(test)]
mod tests {
    //! Published vectors for each primitive. The same algorithms are run
    //! again through `crypto.js` on both backends by the runtime's JS tests
    //! (`crypto.test.js`, ops) and the real-Chrome test (Web Crypto).

    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn bytes(h: &str) -> Vec<u8> {
        (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex"))
            .collect()
    }

    #[test]
    fn aes_gcm_matches_the_nist_vectors() {
        // McGrew and Viega, "The Galois/Counter Mode of Operation", test
        // cases 14 and 16 (the AES-256 ones, as in NIST's CAVP files).
        let k = "0000000000000000000000000000000000000000000000000000000000000000";
        let n = "000000000000000000000000";
        let out = aes_gcm_seal(
            &bytes(k),
            &bytes(n),
            &[],
            &bytes("00000000000000000000000000000000"),
        )
        .expect("seals");
        assert_eq!(
            hex(&out),
            "cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919"
        );
        let k = "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308";
        let n = "cafebabefacedbaddecaf888";
        let p = "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b39";
        let a = "feedfacedeadbeeffeedfacedeadbeefabaddad2";
        let out = aes_gcm_seal(&bytes(k), &bytes(n), &bytes(a), &bytes(p)).expect("seals");
        assert_eq!(
            hex(&out),
            "522dc1f099567d07f47f37a32a84427d643a8cdcbfe5c0c97598a2bd2555d1aa8cb08e48590dbb3da7b08b1056828838c5f61e6393ba7a0abcc9f662\
             76fc6ece0f4e1768cddf8853bb2d551b"
        );
        assert_eq!(
            aes_gcm_open(&bytes(k), &bytes(n), &bytes(a), &out).map(|p| hex(&p)),
            Some(p.to_owned())
        );
        // Any changed bit, in the ciphertext, the tag or the associated
        // data, fails authentication.
        let mut bad = out.clone();
        bad[0] ^= 1;
        assert_eq!(aes_gcm_open(&bytes(k), &bytes(n), &bytes(a), &bad), None);
        let mut bad = out.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0x80;
        assert_eq!(aes_gcm_open(&bytes(k), &bytes(n), &bytes(a), &bad), None);
        assert_eq!(aes_gcm_open(&bytes(k), &bytes(n), b"other", &out), None);
        // Wrong lengths are refused, never truncated or padded.
        assert_eq!(aes_gcm_seal(&bytes(&k[..48]), &bytes(n), &[], &[]), None);
        assert_eq!(aes_gcm_seal(&bytes(k), &bytes("cafebabe"), &[], &[]), None);
        assert_eq!(aes_gcm_open(&bytes(k), &bytes(n), &[], &[0; 15]), None);
    }

    #[test]
    fn hkdf_matches_rfc_5869() {
        // Test case 1.
        let okm = hkdf_sha256(
            &bytes("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"),
            &bytes("000102030405060708090a0b0c"),
            &bytes("f0f1f2f3f4f5f6f7f8f9"),
            42,
        )
        .expect("expands");
        assert_eq!(
            hex(&okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
        // Test case 3: no salt and no info.
        let okm = hkdf_sha256(
            &bytes("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"),
            &[],
            &[],
            42,
        )
        .expect("expands");
        assert_eq!(
            hex(&okm),
            "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8"
        );
        assert_eq!(hkdf_sha256(b"k", b"", b"", 255 * 32 + 1), None);
    }

    #[test]
    fn pbkdf2_matches_the_published_sha256_vectors() {
        // RFC 7914, section 11 (PBKDF2-HMAC-SHA256), first 32 bytes.
        let out = pbkdf2_sha256(b"passwd", b"salt", 1).expect("derives");
        assert_eq!(
            hex(&out),
            "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc"
        );
        let out = pbkdf2_sha256(b"password", b"salt", 4096).expect("derives");
        assert_eq!(
            hex(&out),
            "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
        );
        assert_eq!(pbkdf2_sha256(b"p", b"s", 0), None);
    }

    #[test]
    fn ed25519_matches_rfc_8032() {
        // Section 7.1, tests 1 and 2.
        let seed = bytes("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let public = ed25519_public(&seed).expect("public key");
        assert_eq!(
            hex(&public),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        let sig = ed25519_sign(&seed, b"").expect("signs");
        assert_eq!(
            hex(&sig),
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
        );
        assert!(ed25519_verify(&public, &sig, b""));
        assert!(!ed25519_verify(&public, &sig, b"x"));
        let seed = bytes("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb");
        let public = ed25519_public(&seed).expect("public key");
        assert_eq!(
            hex(&public),
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"
        );
        let sig = ed25519_sign(&seed, &[0x72]).expect("signs");
        assert_eq!(
            hex(&sig),
            "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"
        );
        // Wycheproof-style edge cases: a truncated signature, a public key
        // of the wrong length, and S not reduced modulo the group order.
        assert!(!ed25519_verify(&public, &sig[..63], &[0x72]));
        assert!(!ed25519_verify(&public[..31], &sig, &[0x72]));
        let mut unreduced = sig.clone();
        unreduced[63] |= 0xe0;
        assert!(!ed25519_verify(&public, &unreduced, &[0x72]));
        assert_eq!(ed25519_public(&seed[..31]), None);
    }

    #[test]
    fn x25519_matches_rfc_7748() {
        // Section 6.1: Alice's and Bob's keys and their shared secret.
        let a = bytes("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let b = bytes("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
        let ap = x25519_public(&a).expect("public");
        let bp = x25519_public(&b).expect("public");
        assert_eq!(
            hex(&ap),
            "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"
        );
        assert_eq!(
            hex(&bp),
            "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f"
        );
        let shared = "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742";
        assert_eq!(x25519_agree(&a, &bp).map(|s| hex(&s)), Some(shared.into()));
        assert_eq!(x25519_agree(&b, &ap).map(|s| hex(&s)), Some(shared.into()));
        // Section 5.2's first scalar-multiplication vector.
        let k = bytes("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4");
        let u = bytes("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c");
        assert_eq!(
            x25519_agree(&k, &u).map(|s| hex(&s)),
            Some("c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552".into())
        );
        // A small-order public key (zero, and order-8 points) gives an
        // all-zero result, which is refused.
        assert_eq!(x25519_agree(&a, &[0; 32]), None);
        let order8 = bytes("e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800");
        assert_eq!(x25519_agree(&a, &order8), None);
        assert_eq!(x25519_agree(&a, &bp[..31]), None);
    }

    #[test]
    fn p256_signs_and_verifies_and_matches_the_rfc_6979_key() {
        // RFC 6979, appendix A.2.5: the private scalar and its public point.
        let x = bytes("c9afa9d845ba75166b5c215767b1d6934e50c3db36e89b127b8a622b120f6721");
        let public = p256_public(&x).expect("public");
        assert_eq!(
            hex(&public),
            "0460fed4ba255a9d31c961eb74c6356d68c049b8923b61fa6ce669622e60f29fb67903fe1008b8bc99a41ae9e95628bc64f2f1b20c2d7e9f5177a3c294d4462299"
        );
        // The signature of "sample" with SHA-256 from the RFC (deterministic
        // there; any valid signature verifies the same way).
        let sig = bytes(
            "efd48b2aacb6a8fd1140dd9cd45e81d69d2c877b56aaf991c34d0ea84eaf3716f7cb1c942d657c41d436c7a1b6e29f65f3e900dbb9aff4064dc4ab2f843acda8",
        );
        assert!(p256_verify(&public, &sig, b"sample"));
        assert!(!p256_verify(&public, &sig, b"test"));
        let ours = p256_sign(&x, b"sample").expect("signs");
        assert_eq!(ours.len(), 64);
        assert!(p256_verify(&public, &ours, b"sample"));
        // Zero, the group order and a short scalar aren't private keys.
        assert_eq!(p256_public(&[0; 32]), None);
        let n = bytes("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551");
        assert_eq!(p256_public(&n), None);
        assert_eq!(p256_public(&x[..31]), None);
        // A point off the curve never verifies.
        let mut off = public.clone();
        off[64] ^= 1;
        assert!(!p256_verify(&off, &sig, b"sample"));
    }

    #[test]
    fn constant_time_equality_compares_whole_values() {
        assert!(constant_time_equals(b"abc", b"abc"));
        assert!(!constant_time_equals(b"abc", b"abd"));
        assert!(!constant_time_equals(b"abc", b"ab"));
        assert!(constant_time_equals(b"", b""));
    }

    #[test]
    fn digests_match_fips_180() {
        assert_eq!(
            hex(&digest(256, b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(hex(&digest(512, b"abc")).starts_with("ddaf35a193617aba"));
    }

    #[test]
    fn base64url_is_unpadded_rfc_4648() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn new_keys_are_jwks_of_their_kind() {
        for kind in ["ed25519", "x25519", "p256-ecdsa", "aes-gcm"] {
            let (private, public) = new_key_jwks(kind).expect("random").expect("a kind");
            let p: serde_json::Value = serde_json::from_str(&private).expect("JSON");
            assert!(p.get("d").or_else(|| p.get("k")).is_some(), "{private}");
            if let Some(public) = public {
                let q: serde_json::Value = serde_json::from_str(&public).expect("JSON");
                assert!(q.get("d").is_none(), "{public}");
                assert_eq!(q["x"], p["x"]);
            }
        }
        assert_eq!(new_key_jwks("rsa").expect("random"), None);
        // Two calls never make the same key.
        assert_ne!(
            new_key_jwks("ed25519").unwrap(),
            new_key_jwks("ed25519").unwrap()
        );
    }

    #[test]
    fn rsa_verifies_rfc_7515_a_2() {
        // RFC 7515 A.2's RS256 example: the key's modulus and the signature
        // of its signing input.
        let n = "a1f8160ae2e3c9b465ce8d2d656263362b927dbe29e1f02477fc1625cc90a136e38bd93497c5b6ea63dd7711e67c7429f956b0fb8a8f089adc4b69893cc1333f53edd019b87784252fec914fe4857769594bea4280d32c0f55bf62944f130396bc6e9bdf6ebdd2bda3678eeca0c668f701b38dbffb38c8342ce2fe6d27fade4a5a4874979dd4b9cf9adec4c75b05852c2c0f5ef8a5c1750392f944e8ed64c110c6b647609aa4783aeb9c6c9ad755313050638b83665c6f6f7a82a396702a1f641b82d3ebf2392219491fb686872c5716f50af8358d9a8b9d17c340728f7f87d89a18d8fcab67ad84590c2ecf759339363c07034d6f606f9e21e05456cae5e9a1";
        let sig = "702e218943e88fd11eb5d82dbf7845f34106ae1b81fff7731116add1717d83656d420afd3c96eedd73a2663e5166687b000b87226e0187ed1073f945e582adfcef16d85a798ee8c66ddb3db8975b17d09402beedd5d9d97007108db28160d5f8040ca7445762b81fbe7ff9d92e0ae76f24f25b33bbe6f44ae61eb1040acb20044d3ef9128ed40130795bd4bd3b41eecad066ab651981fde48df77f372dc38b9fafdd3befb18b5da3cc3c2eb02f9e3a41d612caad15911273a05f23b9e838faaf849d698429ef5a1e88798236c3d40e604522a544c8f27a7a2db80663d16cf7caea56de405cb2215a45b2c25566b55ac1a748a070dfc8a32a469543d019eefb47";
        let input = b"eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ";
        let e = [1, 0, 1];
        assert!(rsa_verify(&bytes(n), &e, &bytes(sig), input, false));
        assert!(!rsa_verify(&bytes(n), &e, &bytes(sig), input, true));
        let mut changed = bytes(sig);
        changed[10] ^= 1;
        assert!(!rsa_verify(&bytes(n), &e, &changed, input, false));
        assert!(!rsa_verify(&bytes(n), &e, &bytes(sig), b"other", false));
    }

    #[test]
    fn configured_keys_are_checked_for_shape_without_quoting_them() {
        let seed = base64url(&[7; 32]);
        assert_eq!(config_key_problem("Ed25519PrivateKey", &seed), None);
        assert_eq!(config_key_problem("AesGcmKey", &seed), None);
        let private = format!(r#"{{"kty":"OKP","crv":"Ed25519","d":"{seed}","x":"{seed}"}}"#);
        assert_eq!(config_key_problem("Ed25519PrivateKey", &private), None);
        let problem = config_key_problem("Ed25519PublicKey", &private).expect("a problem");
        assert!(problem.contains("private JWK"), "{problem}");
        assert!(!problem.contains(&seed));
        assert!(config_key_problem("X25519PrivateKey", &private).is_some());
        assert!(config_key_problem("Ed25519PrivateKey", "not a key").is_some());
        let pem = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEINTuctv5E1hK1bbY8fdp+K06/nwoy/HU++CXqI9EdVhC\n-----END PRIVATE KEY-----\n";
        assert_eq!(config_key_problem("Ed25519PrivateKey", pem), None);
        assert!(config_key_problem("Ed25519PublicKey", pem).is_some());
        assert!(config_key_problem("RsaPublicKey", &seed).is_some());
        assert!(is_config_key_type("RsaPublicKey"));
        assert!(!is_config_key_type("Sealed"));
    }
}
