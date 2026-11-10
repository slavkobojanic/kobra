//! Web Push's message encryption and sender identification on the server
//! (`docs/design/50-push.md`): RFC 8291's `aes128gcm` encryption of a push
//! message to a browser's subscription, and RFC 8292's VAPID token signed
//! with the app's `EcdsaP256PrivateKey`. `VapidWebPush` (`k:/runtime/outbound.js`)
//! calls the two ops below and posts the result with `op_k_fetch`.
//!
//! The ECDH P-256 and AES-128-GCM steps are internal to Web Push and not
//! `k.crypto` API. ECDH is the `p256` crate's arithmetic (already compiled
//! in to compute a raw scalar's public point), because `ring`'s agreement
//! keys can't be built from fixed bytes, and RFC 8291's worked example
//! needs a fixed ephemeral key: one code path serves the vector and
//! production. HKDF-SHA256, AES-128-GCM and ECDSA come from `ring`, through
//! [`crate::crypto`].
//!
//! [`decrypt`] and [`verify_vapid`] are the push service's side, used by
//! the tests' fake push service; no app reaches them.
//!
//! | Op | JavaScript signature |
//! |---|---|
//! | `op_k_push_encrypt` | `(p256dh, auth, plaintext) => Uint8Array \| null` (`null`: malformed subscription keys) |
//! | `op_k_push_vapid` | `(scalar, audience, expires, subject) => string` (the `Authorization` value; `""` for a bad key) |

use deno_core::op2;
use p256::elliptic_curve::point::AffineCoordinates;
use p256::elliptic_curve::sec1::ToEncodedPoint;

use crate::crypto::{aes_gcm_open, aes_gcm_seal, base64url, from_base64url, hkdf_sha256};

/// The record size every message declares: one record of at most 4,096
/// bytes, which is what push services accept.
pub const RECORD_SIZE: u32 = 4096;

/// The largest plaintext one record holds: 4,096 bytes less the header
/// (86), the padding delimiter (1) and the tag (16).
pub const MAX_PLAINTEXT: usize = 3993;

/// The JOSE header of every VAPID token, `{"typ":"JWT","alg":"ES256"}`.
const VAPID_HEADER: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9";

/// The longest a VAPID token may live, per RFC 8292 §2: 24 hours.
pub const VAPID_MAX_LIFETIME: u64 = 24 * 60 * 60;

/// The uncompressed public point of P-256 scalar `scalar`, or `None`.
fn public_point(scalar: &[u8]) -> Option<Vec<u8>> {
    if scalar.len() != 32 {
        return None;
    }
    let secret = p256::SecretKey::from_slice(scalar).ok()?;
    Some(
        secret
            .public_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec(),
    )
}

/// The ECDH P-256 shared secret (the x coordinate) of `scalar` and the
/// uncompressed point `public`; `None` for a point that is malformed or
/// not on the curve.
fn ecdh(scalar: &[u8], public: &[u8]) -> Option<Vec<u8>> {
    if public.len() != 65 || public[0] != 4 || scalar.len() != 32 {
        return None;
    }
    let secret = p256::SecretKey::from_slice(scalar).ok()?;
    let public = p256::PublicKey::from_sec1_bytes(public).ok()?;
    let shared = (public.to_projective() * *secret.to_nonzero_scalar()).to_affine();
    Some(shared.x().to_vec())
}

/// The content encryption key and nonce of RFC 8291 §3.3–3.4 and RFC 8188
/// §2.2–2.3, for the browser's point `ua_public`, the sender's
/// `as_public`, the ECDH secret, the authentication secret and the salt.
fn keys(
    ecdh_secret: &[u8],
    auth: &[u8],
    ua_public: &[u8],
    as_public: &[u8],
    salt: &[u8],
) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(ua_public);
    key_info.extend_from_slice(as_public);
    let ikm = hkdf_sha256(ecdh_secret, auth, &key_info, 32)?;
    let cek = hkdf_sha256(&ikm, salt, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = hkdf_sha256(&ikm, salt, b"Content-Encoding: nonce\0", 12)?;
    Some((cek, nonce))
}

/// `plaintext` encrypted to the subscription keys `ua_public` (`p256dh`,
/// 65 bytes) and `auth` (16 bytes) with the ephemeral scalar `ephemeral`
/// and the 16-byte `salt`: the whole `aes128gcm` body, one record with the
/// `0x02` delimiter and no padding. `None` for malformed keys or a
/// plaintext over [`MAX_PLAINTEXT`].
#[must_use]
pub fn encrypt_with(
    ephemeral: &[u8],
    salt: &[u8],
    ua_public: &[u8],
    auth: &[u8],
    plaintext: &[u8],
) -> Option<Vec<u8>> {
    if auth.len() != 16 || salt.len() != 16 || plaintext.len() > MAX_PLAINTEXT {
        return None;
    }
    let as_public = public_point(ephemeral)?;
    let secret = ecdh(ephemeral, ua_public)?;
    let (cek, nonce) = keys(&secret, auth, ua_public, &as_public, salt)?;
    let mut record = plaintext.to_vec();
    record.push(2);
    let sealed = aes_gcm_seal(&cek, &nonce, &[], &record)?;
    let mut body = Vec::with_capacity(86 + sealed.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(65);
    body.extend_from_slice(&as_public);
    body.extend_from_slice(&sealed);
    Some(body)
}

/// A new P-256 scalar and its uncompressed point, from the operating
/// system's generator.
///
/// # Errors
///
/// The operating system refused randomness.
pub fn new_key_pair() -> Result<(Vec<u8>, Vec<u8>), String> {
    loop {
        let mut scalar = vec![0; 32];
        getrandom::fill(&mut scalar).map_err(|e| e.to_string())?;
        if let Some(point) = public_point(&scalar) {
            return Ok((scalar, point));
        }
    }
}

/// [`encrypt_with`] with a fresh ephemeral key and salt: what each push
/// message is sent with.
#[must_use]
pub fn encrypt(ua_public: &[u8], auth: &[u8], plaintext: &[u8]) -> Option<Vec<u8>> {
    let (ephemeral, _) = new_key_pair().ok()?;
    let mut salt = vec![0; 16];
    getrandom::fill(&mut salt).ok()?;
    encrypt_with(&ephemeral, &salt, ua_public, auth, plaintext)
}

/// The push service's and browser's side: `body` decrypted with the
/// subscription's private scalar `ua_scalar` and `auth`, padding and
/// delimiter removed. `None` for anything that isn't one authentic
/// `aes128gcm` record.
#[must_use]
pub fn decrypt(ua_scalar: &[u8], auth: &[u8], body: &[u8]) -> Option<Vec<u8>> {
    if body.len() < 21 {
        return None;
    }
    let salt = &body[..16];
    let rs = u32::from_be_bytes(body[16..20].try_into().ok()?);
    let id_len = usize::from(body[20]);
    let key_id = body.get(21..21 + id_len)?;
    let sealed = body.get(21 + id_len..)?;
    if sealed.len() > rs as usize || sealed.len() < 17 {
        return None;
    }
    let ua_public = public_point(ua_scalar)?;
    let secret = ecdh(ua_scalar, key_id)?;
    let (cek, nonce) = keys(&secret, auth, &ua_public, key_id, salt)?;
    let mut record = aes_gcm_open(&cek, &nonce, &[], sealed)?;
    while record.last() == Some(&0) {
        record.pop();
    }
    (record.pop() == Some(2)).then_some(record)
}

/// The VAPID token (RFC 8292 §2) for push service origin `audience`,
/// valid until `expires` (seconds since the epoch), naming `subject` (a
/// `mailto:` or `https:` contact) when given, signed with P-256 scalar
/// `scalar`. `None` for a scalar that isn't a key.
#[must_use]
pub fn vapid_token(
    scalar: &[u8],
    audience: &str,
    expires: u64,
    subject: Option<&str>,
) -> Option<String> {
    let mut claims = serde_json::Map::new();
    claims.insert("aud".into(), audience.into());
    claims.insert("exp".into(), expires.into());
    if let Some(s) = subject {
        claims.insert("sub".into(), s.into());
    }
    let input = format!(
        "{VAPID_HEADER}.{}",
        base64url(serde_json::Value::Object(claims).to_string().as_bytes())
    );
    let signature = crate::crypto::p256_sign(scalar, input.as_bytes())?;
    Some(format!("{input}.{}", base64url(&signature)))
}

/// The `Authorization` header value of RFC 8292 §3: `vapid t=<token>,
/// k=<the public key>`, or `None` for a scalar that isn't a key.
#[must_use]
pub fn vapid_authorization(
    scalar: &[u8],
    audience: &str,
    expires: u64,
    subject: Option<&str>,
) -> Option<String> {
    let token = vapid_token(scalar, audience, expires, subject)?;
    let public = public_point(scalar)?;
    Some(format!("vapid t={token}, k={}", base64url(&public)))
}

/// What a verified VAPID header says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VapidClaims {
    /// `aud`: the push service origin the token is for.
    pub audience: String,
    /// `exp`, in seconds since the epoch.
    pub expires: u64,
    /// `sub`, the sender's contact, when given.
    pub subject: Option<String>,
    /// `k`: the sender's uncompressed public point.
    pub public_key: Vec<u8>,
}

impl VapidClaims {
    /// Checks what a push service checks beyond the signature: the token
    /// is for `audience` and, at `now` (seconds since the epoch), not
    /// expired and not valid for more than 24 hours.
    ///
    /// # Errors
    ///
    /// What is wrong, for the test's message.
    pub fn check(&self, audience: &str, now: u64) -> Result<(), String> {
        if self.audience != audience {
            return Err(format!("aud is {:?}, not {audience:?}", self.audience));
        }
        if self.expires <= now {
            return Err("the token has expired".into());
        }
        if self.expires > now + VAPID_MAX_LIFETIME {
            return Err("the token is valid for more than 24 hours".into());
        }
        Ok(())
    }
}

/// The push service's side of RFC 8292: parses `vapid t=…, k=…`, checks
/// that the header is ES256 and the signature verifies under `k`, and
/// returns the claims. The caller checks `aud` and `exp`
/// ([`VapidClaims::check`]).
///
/// # Errors
///
/// What is wrong with the header, for the test's message.
pub fn verify_vapid(authorization: &str) -> Result<VapidClaims, String> {
    let params = authorization
        .strip_prefix("vapid ")
        .ok_or("the scheme isn't vapid")?;
    let (mut token, mut key) = (None, None);
    for p in params.split(',') {
        let p = p.trim();
        if let Some(t) = p.strip_prefix("t=") {
            token = Some(t);
        } else if let Some(k) = p.strip_prefix("k=") {
            key = Some(k);
        }
    }
    let token = token.ok_or("no t= parameter")?;
    let public_key = from_base64url(key.ok_or("no k= parameter")?).ok_or("k isn't base64url")?;
    let mut parts = token.split('.');
    let (Some(h), Some(c), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("the token isn't three parts".into());
    };
    let header: serde_json::Value =
        serde_json::from_slice(&from_base64url(h).ok_or("the header isn't base64url")?)
            .map_err(|e| format!("the header isn't JSON: {e}"))?;
    if header.get("alg").and_then(serde_json::Value::as_str) != Some("ES256") {
        return Err(format!("the header isn't ES256: {header}"));
    }
    let signature = from_base64url(s).ok_or("the signature isn't base64url")?;
    if !crate::crypto::p256_verify(&public_key, &signature, format!("{h}.{c}").as_bytes()) {
        return Err("the signature doesn't verify under k".into());
    }
    let claims: serde_json::Value =
        serde_json::from_slice(&from_base64url(c).ok_or("the claims aren't base64url")?)
            .map_err(|e| format!("the claims aren't JSON: {e}"))?;
    Ok(VapidClaims {
        audience: claims
            .get("aud")
            .and_then(serde_json::Value::as_str)
            .ok_or("no aud claim")?
            .to_owned(),
        expires: claims
            .get("exp")
            .and_then(serde_json::Value::as_u64)
            .ok_or("no exp claim")?,
        subject: claims
            .get("sub")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        public_key,
    })
}

/// [`encrypt`].
#[op2]
#[buffer]
pub(crate) fn op_k_push_encrypt(
    #[buffer] p256dh: &[u8],
    #[buffer] auth: &[u8],
    #[buffer] plaintext: &[u8],
) -> Option<Vec<u8>> {
    encrypt(p256dh, auth, plaintext)
}

/// [`vapid_authorization`]; an empty `subject` names none, and `""` is
/// returned for a scalar that isn't a key.
#[op2]
#[string]
pub(crate) fn op_k_push_vapid(
    #[buffer] scalar: &[u8],
    #[string] audience: String,
    expires: f64,
    #[string] subject: String,
) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let expires = expires.max(0.0) as u64;
    let subject = (!subject.is_empty()).then_some(subject.as_str());
    vapid_authorization(scalar, &audience, expires, subject).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn b(text: &str) -> Vec<u8> {
        from_base64url(text).unwrap()
    }

    // RFC 8291 §5 and Appendix A.
    const PLAINTEXT: &str = "When I grow up, I want to be a watermelon";
    const AS_PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
    const AS_PUBLIC: &str =
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    const UA_PRIVATE: &str = "q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94";
    const UA_PUBLIC: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";
    const SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
    const BODY: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN";

    #[test]
    fn rfc_8291_intermediate_values() {
        assert_eq!(public_point(&b(AS_PRIVATE)).unwrap(), b(AS_PUBLIC));
        assert_eq!(public_point(&b(UA_PRIVATE)).unwrap(), b(UA_PUBLIC));
        let secret = ecdh(&b(AS_PRIVATE), &b(UA_PUBLIC)).unwrap();
        assert_eq!(
            base64url(&secret),
            "kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs"
        );
        assert_eq!(ecdh(&b(UA_PRIVATE), &b(AS_PUBLIC)).unwrap(), secret);
        let (cek, nonce) = keys(&secret, &b(AUTH), &b(UA_PUBLIC), &b(AS_PUBLIC), &b(SALT)).unwrap();
        assert_eq!(base64url(&cek), "oIhVW04MRdy2XN9CiKLxTg");
        assert_eq!(base64url(&nonce), "4h_95klXJ5E_qnoN");
    }

    #[test]
    fn rfc_8291_example_encrypts_byte_for_byte() {
        let body = encrypt_with(
            &b(AS_PRIVATE),
            &b(SALT),
            &b(UA_PUBLIC),
            &b(AUTH),
            PLAINTEXT.as_bytes(),
        )
        .unwrap();
        assert_eq!(base64url(&body), BODY);
        assert_eq!(
            decrypt(&b(UA_PRIVATE), &b(AUTH), &b(BODY)).unwrap(),
            PLAINTEXT.as_bytes()
        );
    }

    #[test]
    fn random_messages_round_trip_through_the_production_path() {
        let (ua, ua_public) = new_key_pair().unwrap();
        let auth = [7u8; 16];
        for len in [0usize, 1, 15, 16, 17, 100, 1000, MAX_PLAINTEXT] {
            let mut message = vec![0u8; len];
            getrandom::fill(&mut message).unwrap();
            let body = encrypt(&ua_public, &auth, &message).unwrap();
            assert!(body.len() <= RECORD_SIZE as usize, "{len}: {}", body.len());
            assert_eq!(body.len(), 86 + len + 17);
            assert_eq!(decrypt(&ua, &auth, &body).unwrap(), message, "{len}");
            // A fresh key and salt each time.
            assert_ne!(encrypt(&ua_public, &auth, &message).unwrap(), body);
        }
        assert!(encrypt(&ua_public, &auth, &vec![0; MAX_PLAINTEXT + 1]).is_none());
    }

    #[test]
    fn tampering_and_malformed_keys_are_refused() {
        let mut body = b(BODY);
        let last = body.len() - 1;
        body[last] ^= 1;
        assert!(decrypt(&b(UA_PRIVATE), &b(AUTH), &body).is_none());
        assert!(decrypt(&b(UA_PRIVATE), &[0; 16], &b(BODY)).is_none());
        assert!(decrypt(&b(UA_PRIVATE), &b(AUTH), &[0; 10]).is_none());
        let (e, s) = (b(AS_PRIVATE), b(SALT));
        // A point off the curve, a compressed point, a short auth secret.
        let mut off = b(UA_PUBLIC);
        off[64] ^= 1;
        assert!(encrypt_with(&e, &s, &off, &b(AUTH), b"x").is_none());
        assert!(encrypt_with(&e, &s, &b(UA_PUBLIC)[..33], &b(AUTH), b"x").is_none());
        assert!(encrypt_with(&e, &s, &b(UA_PUBLIC), &[1; 8], b"x").is_none());
    }

    // RFC 8292 §2.4 and §3.1.
    const RFC_8292_HEADER: &str = "vapid t=eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJhdWQiOiJodHRwczovL3B1c2guZXhhbXBsZS5uZXQiLCJleHAiOjE0NTM1MjM3NjgsInN1YiI6Im1haWx0bzpwdXNoQGV4YW1wbGUuY29tIn0.i3CYb7t4xfxCDquptFOepC9GAu_HLGkMlMuCGSK2rpiUfnK9ojFwDXb1JrErtmysazNjjvW2L9OkSSHzvoD1oA, k=BA1Hxzyi1RUM1b5wjxsn7nGxAszw2u61m164i3MrAIxHF6YK5h4SDYic-dRuU_RCPCfA5aq9ojSwk5Y2EmClBPs";

    #[test]
    fn rfc_8292_example_verifies() {
        let claims = verify_vapid(RFC_8292_HEADER).unwrap();
        assert_eq!(claims.audience, "https://push.example.net");
        assert_eq!(claims.expires, 1_453_523_768);
        assert_eq!(claims.subject.as_deref(), Some("mailto:push@example.com"));
        claims
            .check("https://push.example.net", 1_453_523_768 - 3600)
            .unwrap();
        assert!(
            claims
                .check("https://push.example.net", 1_453_523_768)
                .is_err()
        );
        assert!(
            claims
                .check("https://other.example", 1_453_523_000)
                .is_err()
        );
    }

    #[test]
    fn changed_vapid_tokens_are_refused() {
        // Another claim, another header, another signature, another key.
        let claims = RFC_8292_HEADER.replace(
            "eyJhdWQiOiJodHRwczovL3B1c2guZXhhbXBsZS5uZXQi",
            "eyJhdWQiOiJodHRwczovL3B1c2guZXhhbXBsZS5uZXUi",
        );
        assert!(verify_vapid(&claims).is_err());
        let header = RFC_8292_HEADER.replacen(
            "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9",
            "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzM4NCJ9",
            1,
        );
        assert!(verify_vapid(&header).is_err());
        let signature = RFC_8292_HEADER.replace("i3CYb7t4", "i3CYb7t5");
        assert!(verify_vapid(&signature).is_err());
        let (_, other) = new_key_pair().unwrap();
        let key = format!(
            "{}k={}",
            &RFC_8292_HEADER[..RFC_8292_HEADER.find("k=").unwrap()],
            base64url(&other)
        );
        assert!(verify_vapid(&key).is_err());
        assert!(verify_vapid("Bearer abc").is_err());
    }

    #[test]
    fn signed_tokens_verify_and_name_the_key() {
        let (scalar, public) = new_key_pair().unwrap();
        let header = vapid_authorization(
            &scalar,
            "https://push.example.net",
            2_000_000_000,
            Some("mailto:ops@example.org"),
        )
        .unwrap();
        let claims = verify_vapid(&header).unwrap();
        assert_eq!(claims.public_key, public);
        assert_eq!(claims.audience, "https://push.example.net");
        assert_eq!(claims.subject.as_deref(), Some("mailto:ops@example.org"));
        let without =
            vapid_authorization(&scalar, "https://a.example", 2_000_000_000, None).unwrap();
        assert_eq!(verify_vapid(&without).unwrap().subject, None);
        assert!(vapid_authorization(&[0; 32], "https://a.example", 1, None).is_none());
    }
}
