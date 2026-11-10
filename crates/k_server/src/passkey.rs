//! Passkeys on the server (`docs/design/51-passkeys.md`): WebAuthn
//! Level 3's registration and authentication checks for
//! `k.auth.PasskeyVerifier`, and the authenticator `k.test.TestAuthenticator`
//! plays in `k test`.
//!
//! Everything a browser hands over is parsed here, once: the client data
//! (JSON), the attestation object and the authenticator data (CBOR and a
//! fixed layout), and the credential's COSE key, whose signature is then
//! checked with `ring` (ES256 with DER signatures, Ed25519, and RS256 with
//! moduli of 2,048 to 8,192 bits). The attestation statement is never read:
//! K asks for attestation `"none"` and ignores any statement a device sends
//! anyway. A failure is one [`Refusal`], whose name is the
//! `PasskeyVerificationError` case the runtime throws.
//!
//! The CBOR reader takes only what WebAuthn uses: integers, byte and text
//! strings, arrays, maps (with no duplicate keys), `true`, `false` and
//! `null`, definite lengths only, at most [`MAX_DEPTH`] levels deep.
//!
//! | Op | JavaScript signature |
//! |---|---|
//! | `op_k_passkey_verify_registration` | `(json) => json`: `{clientDataJSON, attestationObject, credentialId, rpId, origin, challenge, requireUserVerification}` to `{ok: {id, publicKey, signCount, backupEligible, backedUp}}` or `{error}` |
//! | `op_k_passkey_verify_assertion` | `(json) => json`: `{clientDataJSON, authenticatorData, signature, publicKey, signCount, rpId, origin, challenge, requireUserVerification}` to `{ok: {signCount, backupEligible, backedUp}}` or `{error}` |
//! | `op_k_passkey_test_register` / `_assert` | `TestAuthenticator`'s ceremonies: `(json) => json`, see [`test_register`] and [`test_assert`] |

use deno_core::op2;
use ring::signature;
use serde_json::{Value, json};

use crate::crypto::{base64url, digest, from_base64url};

/// The longest credential id WebAuthn allows (§5.4.3), in bytes.
pub const MAX_CREDENTIAL_ID: usize = 1023;

/// How deeply CBOR may nest: an attestation object is three levels.
pub const MAX_DEPTH: usize = 8;

/// The authenticator data's user-present flag (WebAuthn §6.1).
const UP: u8 = 0x01;
/// The user-verified flag.
const UV: u8 = 0x04;
/// The backup-eligibility flag: a synced passkey.
const BE: u8 = 0x08;
/// The backup-state flag.
const BS: u8 = 0x10;
/// The attested-credential-data flag.
const AT: u8 = 0x40;
/// The extension-data flag.
const ED: u8 = 0x80;

/// Why a registration or an assertion was refused: the cases of
/// `k.auth.PasskeyVerificationError`, in its order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refusal {
    /// The challenge isn't this session's, or the client data is for the
    /// other ceremony.
    ChallengeMismatch,
    /// The client data's origin isn't the app's, or it ran in a frame.
    OriginMismatch,
    /// The RP id hash isn't the SHA-256 of the app's RP id.
    RpIdMismatch,
    /// The user-present or (when required) user-verified flag is unset.
    UserNotVerified,
    /// The credential's key is none of ES256, EdDSA and RS256.
    UnsupportedAlgorithm,
    /// The signature doesn't verify under the credential's key.
    InvalidSignature,
    /// The signature counter didn't increase.
    SignCountRegressed,
    /// The credential id isn't the one reported, or stored.
    CredentialMismatch,
    /// Something doesn't parse.
    Malformed,
}

impl Refusal {
    /// The K case name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Refusal::ChallengeMismatch => "challengeMismatch",
            Refusal::OriginMismatch => "originMismatch",
            Refusal::RpIdMismatch => "rpIdMismatch",
            Refusal::UserNotVerified => "userNotVerified",
            Refusal::UnsupportedAlgorithm => "unsupportedAlgorithm",
            Refusal::InvalidSignature => "invalidSignature",
            Refusal::SignCountRegressed => "signCountRegressed",
            Refusal::CredentialMismatch => "credentialMismatch",
            Refusal::Malformed => "malformed",
        }
    }
}

// ------------------------------------------------------------------ CBOR

/// A CBOR data item, as far as WebAuthn uses CBOR.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Cbor {
    /// A major type 0 or 1 integer.
    Int(i128),
    /// A byte string.
    Bytes(Vec<u8>),
    /// A text string.
    Text(String),
    /// An array.
    Array(Vec<Cbor>),
    /// A map, in the order written.
    Map(Vec<(Cbor, Cbor)>),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
}

impl Cbor {
    /// The value under integer key `k` of a map.
    fn get(&self, k: i128) -> Option<&Cbor> {
        match self {
            Cbor::Map(m) => m
                .iter()
                .find(|(key, _)| *key == Cbor::Int(k))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// The value under text key `k` of a map.
    fn get_text(&self, k: &str) -> Option<&Cbor> {
        match self {
            Cbor::Map(m) => m
                .iter()
                .find(|(key, _)| matches!(key, Cbor::Text(t) if t == k))
                .map(|(_, v)| v),
            _ => None,
        }
    }
}

/// The first CBOR data item of `input` and the bytes it took, or `None`
/// when it isn't one this reader takes.
#[must_use]
pub fn cbor_item(input: &[u8]) -> Option<(Cbor, usize)> {
    let mut at = 0;
    let item = read(input, &mut at, 0)?;
    Some((item, at))
}

/// `input` as exactly one CBOR data item, with nothing after it.
#[must_use]
pub fn cbor_decode(input: &[u8]) -> Option<Cbor> {
    let (item, used) = cbor_item(input)?;
    (used == input.len()).then_some(item)
}

/// The argument of the head at `*at`: its major type and value.
fn head(b: &[u8], at: &mut usize) -> Option<(u8, u64)> {
    let first = *b.get(*at)?;
    *at += 1;
    let info = first & 0x1f;
    let n = match info {
        0..=23 => return Some((first >> 5, u64::from(info))),
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        // 28 to 30 are reserved, 31 is an indefinite length.
        _ => return None,
    };
    let bytes = b.get(*at..*at + n)?;
    *at += n;
    Some((
        first >> 5,
        bytes.iter().fold(0u64, |v, &x| (v << 8) | u64::from(x)),
    ))
}

/// The data item at `*at`, `depth` levels down, advancing `*at` past it.
fn read(b: &[u8], at: &mut usize, depth: usize) -> Option<Cbor> {
    if depth > MAX_DEPTH {
        return None;
    }
    let (major, v) = head(b, at)?;
    let rest = b.len() - *at;
    let len = |v: u64| usize::try_from(v).ok().filter(|&n| n <= rest);
    Some(match major {
        0 => Cbor::Int(i128::from(v)),
        1 => Cbor::Int(-1 - i128::from(v)),
        2 | 3 => {
            let n = len(v)?;
            let bytes = b[*at..*at + n].to_vec();
            *at += n;
            if major == 2 {
                Cbor::Bytes(bytes)
            } else {
                Cbor::Text(String::from_utf8(bytes).ok()?)
            }
        }
        4 => {
            // Each item takes at least a byte, so a count over what is
            // left is refused before anything is allocated.
            let n = len(v)?;
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push(read(b, at, depth + 1)?);
            }
            Cbor::Array(items)
        }
        5 => {
            let n = len(v)?;
            let mut pairs: Vec<(Cbor, Cbor)> = Vec::with_capacity(n);
            for _ in 0..n {
                let k = read(b, at, depth + 1)?;
                if pairs.iter().any(|(seen, _)| *seen == k) {
                    return None;
                }
                let value = read(b, at, depth + 1)?;
                pairs.push((k, value));
            }
            Cbor::Map(pairs)
        }
        7 => match v {
            20 => Cbor::Bool(false),
            21 => Cbor::Bool(true),
            22 => Cbor::Null,
            // Floats, `undefined` and simple values WebAuthn doesn't use.
            _ => return None,
        },
        // Tags (6).
        _ => return None,
    })
}

/// A CBOR writer for what [`test_register`] and [`test_assert`] produce.
struct Writer(Vec<u8>);

impl Writer {
    /// A head of major type `major` with argument `v`, in its shortest form.
    fn head(&mut self, major: u8, v: u64) {
        let m = major << 5;
        match v {
            0..=23 => self.0.push(m | u8::try_from(v).unwrap_or(0)),
            24..=0xff => self.0.extend([m | 24, u8::try_from(v).unwrap_or(0)]),
            0x100..=0xffff => {
                self.0.push(m | 25);
                self.0.extend(u16::try_from(v).unwrap_or(0).to_be_bytes());
            }
            _ => {
                self.0.push(m | 26);
                self.0.extend(u32::try_from(v).unwrap_or(0).to_be_bytes());
            }
        }
    }

    /// An integer, of major type 0 or 1.
    fn int(&mut self, v: i64) {
        if v >= 0 {
            self.head(0, v.unsigned_abs());
        } else {
            self.head(1, (-1 - v).unsigned_abs());
        }
    }

    /// A byte string.
    fn bytes(&mut self, b: &[u8]) {
        self.head(2, b.len() as u64);
        self.0.extend_from_slice(b);
    }

    /// A text string.
    fn text(&mut self, t: &str) {
        self.head(3, t.len() as u64);
        self.0.extend_from_slice(t.as_bytes());
    }

    /// The head of a map of `n` pairs.
    fn map(&mut self, n: u64) {
        self.head(5, n);
    }
}

// ---------------------------------------------------- authenticator data

/// The credential an authenticator data's attested credential data
/// carries (WebAuthn §6.5.1).
#[derive(Debug, PartialEq, Eq)]
pub struct Attested<'a> {
    /// The authenticator's model id (all zeros with attestation `"none"`).
    pub aaguid: &'a [u8],
    /// The credential id.
    pub id: &'a [u8],
    /// The credential's public key, a COSE_Key, as written.
    pub key: &'a [u8],
}

/// Parsed authenticator data (WebAuthn §6.1).
#[derive(Debug, PartialEq, Eq)]
pub struct AuthData<'a> {
    /// The SHA-256 of the RP id the authenticator signed for.
    pub rp_id_hash: &'a [u8],
    /// The flags byte.
    pub flags: u8,
    /// The signature counter.
    pub sign_count: u32,
    /// The attested credential, when the AT flag is set.
    pub credential: Option<Attested<'a>>,
}

/// `b` as authenticator data, refusing a truncation, trailing bytes, a
/// credential id over [`MAX_CREDENTIAL_ID`] bytes, a COSE key or
/// extensions that aren't CBOR, and a backed-up flag without the backup
/// eligibility flag.
///
/// # Errors
///
/// [`Refusal::Malformed`].
pub fn parse_auth_data(b: &[u8]) -> Result<AuthData<'_>, Refusal> {
    let m = Refusal::Malformed;
    if b.len() < 37 {
        return Err(m);
    }
    let flags = b[32];
    if flags & BS != 0 && flags & BE == 0 {
        return Err(m);
    }
    let sign_count = u32::from_be_bytes([b[33], b[34], b[35], b[36]]);
    let mut at = 37;
    let credential = if flags & AT == 0 {
        None
    } else {
        let aaguid = b.get(at..at + 16).ok_or(m)?;
        let n = b.get(at + 16..at + 18).ok_or(m)?;
        let n = usize::from(u16::from_be_bytes([n[0], n[1]]));
        if n == 0 || n > MAX_CREDENTIAL_ID {
            return Err(m);
        }
        at += 18;
        let id = b.get(at..at + n).ok_or(m)?;
        at += n;
        let (_, used) = cbor_item(&b[at..]).ok_or(m)?;
        let key = &b[at..at + used];
        at += used;
        Some(Attested { aaguid, id, key })
    };
    if flags & ED != 0 {
        let (ext, used) = cbor_item(&b[at..]).ok_or(m)?;
        if !matches!(ext, Cbor::Map(_)) {
            return Err(m);
        }
        at += used;
    }
    if at != b.len() {
        return Err(m);
    }
    Ok(AuthData {
        rp_id_hash: &b[..32],
        flags,
        sign_count,
        credential,
    })
}

// -------------------------------------------------------------- COSE keys

/// A credential public key of a supported algorithm.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CoseKey {
    /// ES256 (COSE `-7`): the uncompressed P-256 point.
    Es256(Vec<u8>),
    /// EdDSA (COSE `-8`) on Ed25519: the 32-byte key.
    Ed25519(Vec<u8>),
    /// RS256 (COSE `-257`): the modulus and exponent, big-endian without
    /// leading zeros.
    Rs256 {
        /// The modulus.
        n: Vec<u8>,
        /// The public exponent.
        e: Vec<u8>,
    },
}

/// `b` (exactly one CBOR map) as a COSE_Key (RFC 9052 §7, RFC 9053).
///
/// # Errors
///
/// [`Refusal::UnsupportedAlgorithm`] for a key type, algorithm or curve
/// other than the three, or an RSA modulus outside 2,048 to 8,192 bits;
/// [`Refusal::Malformed`] for anything that isn't such a key, including a
/// P-256 point off the curve.
pub fn parse_cose_key(b: &[u8]) -> Result<CoseKey, Refusal> {
    let m = Refusal::Malformed;
    let key = cbor_decode(b).ok_or(m)?;
    let int = |k| match key.get(k) {
        Some(Cbor::Int(v)) => Ok(*v),
        _ => Err(m),
    };
    let bytes = |k| match key.get(k) {
        Some(Cbor::Bytes(v)) => Ok(v.clone()),
        _ => Err(m),
    };
    let (kty, alg) = (int(1)?, int(3)?);
    match (kty, alg) {
        (2, -7) => {
            if int(-1)? != 1 {
                return Err(Refusal::UnsupportedAlgorithm);
            }
            let (x, y) = (bytes(-2)?, bytes(-3)?);
            if x.len() != 32 || y.len() != 32 {
                return Err(m);
            }
            let mut point = vec![4u8];
            point.extend(x);
            point.extend(y);
            p256::PublicKey::from_sec1_bytes(&point).map_err(|_| m)?;
            Ok(CoseKey::Es256(point))
        }
        (1, -8) => {
            if int(-1)? != 6 {
                return Err(Refusal::UnsupportedAlgorithm);
            }
            let x = bytes(-2)?;
            if x.len() != 32 {
                return Err(m);
            }
            Ok(CoseKey::Ed25519(x))
        }
        (3, -257) => {
            let strip = |v: Vec<u8>| {
                let z = v.iter().take_while(|&&x| x == 0).count();
                v[z..].to_vec()
            };
            let (n, e) = (strip(bytes(-1)?), strip(bytes(-2)?));
            if n.is_empty() || e.is_empty() || e.len() > 4 {
                return Err(m);
            }
            let bits = n.len() * 8 - n[0].leading_zeros() as usize;
            if !(2048..=8192).contains(&bits) {
                return Err(Refusal::UnsupportedAlgorithm);
            }
            Ok(CoseKey::Rs256 { n, e })
        }
        _ => Err(Refusal::UnsupportedAlgorithm),
    }
}

impl CoseKey {
    /// Whether `sig` is this key's signature of `message`: DER for
    /// ES256, as WebAuthn writes it.
    #[must_use]
    pub fn verify(&self, message: &[u8], sig: &[u8]) -> bool {
        match self {
            CoseKey::Es256(point) => {
                signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, point)
                    .verify(message, sig)
                    .is_ok()
            }
            CoseKey::Ed25519(x) => signature::UnparsedPublicKey::new(&signature::ED25519, x)
                .verify(message, sig)
                .is_ok(),
            CoseKey::Rs256 { n, e } => signature::RsaPublicKeyComponents { n, e }
                .verify(&signature::RSA_PKCS1_2048_8192_SHA256, message, sig)
                .is_ok(),
        }
    }
}

// ------------------------------------------------------------ the checks

/// What a ceremony must match: the app's RP id and origin, the challenge
/// the session issued (base64url), and whether user verification is
/// required (always, in production).
#[derive(Clone, Copy, Debug)]
pub struct Expected<'a> {
    /// The RP id: the host of `K_PUBLIC_ORIGIN`.
    pub rp_id: &'a str,
    /// `K_PUBLIC_ORIGIN`.
    pub origin: &'a str,
    /// The challenge, as the client data writes it.
    pub challenge: &'a str,
    /// Whether the UV flag must be set.
    pub require_user_verification: bool,
}

/// Checks the client data JSON (WebAuthn §7.1 steps 7–10, §7.2 steps
/// 10–13) for ceremony `ty`.
fn check_client_data(json: &[u8], ty: &str, e: &Expected<'_>) -> Result<(), Refusal> {
    let v: Value = serde_json::from_slice(json).map_err(|_| Refusal::Malformed)?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).ok_or(Refusal::Malformed);
    if !v.is_object() {
        return Err(Refusal::Malformed);
    }
    if text("type")? != ty || text("challenge")? != e.challenge {
        return Err(Refusal::ChallengeMismatch);
    }
    if text("origin")? != e.origin {
        return Err(Refusal::OriginMismatch);
    }
    match v.get("crossOrigin") {
        None | Some(Value::Bool(false)) => {}
        Some(Value::Bool(true)) => return Err(Refusal::OriginMismatch),
        Some(_) => return Err(Refusal::Malformed),
    }
    if v.get("topOrigin").is_some() {
        return Err(Refusal::OriginMismatch);
    }
    Ok(())
}

/// Checks the RP id hash and the flags.
fn check_auth_data(a: &AuthData<'_>, e: &Expected<'_>) -> Result<(), Refusal> {
    if !crate::crypto::constant_time_equals(a.rp_id_hash, &digest(256, e.rp_id.as_bytes())) {
        return Err(Refusal::RpIdMismatch);
    }
    if a.flags & UP == 0 || (e.require_user_verification && a.flags & UV == 0) {
        return Err(Refusal::UserNotVerified);
    }
    Ok(())
}

/// A verified registration: what the app stores.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Registered {
    /// The credential id.
    pub id: Vec<u8>,
    /// The COSE_Key, as written.
    pub public_key: Vec<u8>,
    /// The signature counter at registration.
    pub sign_count: u32,
    /// The BE flag: a synced passkey.
    pub backup_eligible: bool,
    /// The BS flag.
    pub backed_up: bool,
}

/// Verifies a registration (WebAuthn §7.1) whose browser-reported raw id
/// is `credential_id`. The attestation statement isn't read.
///
/// # Errors
///
/// The first check that fails.
pub fn verify_registration(
    client_data_json: &[u8],
    attestation_object: &[u8],
    credential_id: &[u8],
    e: &Expected<'_>,
) -> Result<Registered, Refusal> {
    check_client_data(client_data_json, "webauthn.create", e)?;
    let m = Refusal::Malformed;
    let att = cbor_decode(attestation_object).ok_or(m)?;
    match (
        att.get_text("fmt"),
        att.get_text("attStmt"),
        att.get_text("authData"),
    ) {
        (Some(Cbor::Text(_)), Some(Cbor::Map(_)), Some(Cbor::Bytes(_))) => {}
        _ => return Err(m),
    }
    let Some(Cbor::Bytes(raw)) = att.get_text("authData") else {
        return Err(m);
    };
    let a = parse_auth_data(raw)?;
    check_auth_data(&a, e)?;
    let c = a.credential.as_ref().ok_or(m)?;
    if c.id != credential_id {
        return Err(Refusal::CredentialMismatch);
    }
    parse_cose_key(c.key)?;
    Ok(Registered {
        id: c.id.to_vec(),
        public_key: c.key.to_vec(),
        sign_count: a.sign_count,
        backup_eligible: a.flags & BE != 0,
        backed_up: a.flags & BS != 0,
    })
}

/// A verified assertion: the credential's new state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Asserted {
    /// The new signature counter.
    pub sign_count: u32,
    /// The BE flag.
    pub backup_eligible: bool,
    /// The BS flag.
    pub backed_up: bool,
}

/// Verifies an assertion (WebAuthn §7.2) against the stored COSE key and
/// counter.
///
/// # Errors
///
/// The first check that fails.
pub fn verify_assertion(
    client_data_json: &[u8],
    authenticator_data: &[u8],
    sig: &[u8],
    public_key: &[u8],
    stored_count: u32,
    e: &Expected<'_>,
) -> Result<Asserted, Refusal> {
    check_client_data(client_data_json, "webauthn.get", e)?;
    let a = parse_auth_data(authenticator_data)?;
    if a.credential.is_some() {
        return Err(Refusal::Malformed);
    }
    check_auth_data(&a, e)?;
    let key = parse_cose_key(public_key)?;
    let mut signed = authenticator_data.to_vec();
    signed.extend(digest(256, client_data_json));
    if !key.verify(&signed, sig) {
        return Err(Refusal::InvalidSignature);
    }
    if (a.sign_count != 0 || stored_count != 0) && a.sign_count <= stored_count {
        return Err(Refusal::SignCountRegressed);
    }
    Ok(Asserted {
        sign_count: a.sign_count,
        backup_eligible: a.flags & BE != 0,
        backed_up: a.flags & BS != 0,
    })
}

// ------------------------------------------------------------------- ops

/// A JSON field as text.
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a str, Refusal> {
    v.get(k).and_then(Value::as_str).ok_or(Refusal::Malformed)
}

/// A base64url JSON field as bytes.
fn field_bytes(v: &Value, k: &str) -> Result<Vec<u8>, Refusal> {
    from_base64url(field(v, k)?).ok_or(Refusal::Malformed)
}

/// The expectation fields of an op's input.
fn expected(v: &Value) -> Result<Expected<'_>, Refusal> {
    Ok(Expected {
        rp_id: field(v, "rpId")?,
        origin: field(v, "origin")?,
        challenge: field(v, "challenge")?,
        require_user_verification: v
            .get("requireUserVerification")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

/// The op result of `r`.
fn outcome(r: Result<Value, Refusal>) -> String {
    match r {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "error": e.name() }),
    }
    .to_string()
}

/// [`verify_registration`] for `PasskeyVerifier.verifyRegistration`.
#[must_use]
pub fn registration_op(input: &str) -> String {
    outcome((|| {
        let v: Value = serde_json::from_str(input).map_err(|_| Refusal::Malformed)?;
        let r = verify_registration(
            &field_bytes(&v, "clientDataJSON")?,
            &field_bytes(&v, "attestationObject")?,
            &field_bytes(&v, "credentialId")?,
            &expected(&v)?,
        )?;
        Ok(json!({
            "id": base64url(&r.id),
            "publicKey": base64url(&r.public_key),
            "signCount": r.sign_count,
            "backupEligible": r.backup_eligible,
            "backedUp": r.backed_up,
        }))
    })())
}

/// [`verify_assertion`] for `PasskeyVerifier.verifySignIn`.
#[must_use]
pub fn assertion_op(input: &str) -> String {
    outcome((|| {
        let v: Value = serde_json::from_str(input).map_err(|_| Refusal::Malformed)?;
        let stored = v
            .get("signCount")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(Refusal::Malformed)?;
        let r = verify_assertion(
            &field_bytes(&v, "clientDataJSON")?,
            &field_bytes(&v, "authenticatorData")?,
            &field_bytes(&v, "signature")?,
            &field_bytes(&v, "publicKey")?,
            stored,
            &expected(&v)?,
        )?;
        Ok(json!({
            "signCount": r.sign_count,
            "backupEligible": r.backup_eligible,
            "backedUp": r.backed_up,
        }))
    })())
}

/// Verifies a registration: JSON in, JSON out ([`registration_op`]).
#[op2]
#[string]
pub(crate) fn op_k_passkey_verify_registration(#[string] input: String) -> String {
    registration_op(&input)
}

/// Verifies an assertion: JSON in, JSON out ([`assertion_op`]).
#[op2]
#[string]
pub(crate) fn op_k_passkey_verify_assertion(#[string] input: String) -> String {
    assertion_op(&input)
}

// ------------------------------------------------------ TestAuthenticator

/// The fixed RSA key `TestAuthenticator` signs RS256 with: 2,048 bits, a
/// PKCS #1 `RSAPrivateKey` in DER, made for these tests and used nowhere
/// else.
const TEST_RSA_KEY: &[u8] = include_bytes!("passkey/test-rsa-2048.der");

/// The raw private key of `TestKeys.<kind>(n)` (`k:runtime/crypto`): the
/// SHA-256 of `k.test.TestKeys/<kind>/<n>`, rehashed until it is a valid
/// P-256 scalar for `ecdsaP256`.
fn test_key(kind: &str, n: u64) -> Vec<u8> {
    let mut seed = format!("k.test.TestKeys/{kind}/{n}").into_bytes();
    loop {
        seed = digest(256, &seed);
        if kind != "ecdsaP256" || p256::SecretKey::from_slice(&seed).is_ok() {
            return seed;
        }
    }
}

/// The P-256 pair of `scalar`, signing with DER signatures.
fn p256_der_pair(scalar: &[u8]) -> Option<signature::EcdsaKeyPair> {
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    let secret = p256::SecretKey::from_slice(scalar).ok()?;
    let point = secret.public_key().to_encoded_point(false);
    signature::EcdsaKeyPair::from_private_key_and_public_key(
        &signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        scalar,
        point.as_bytes(),
        &ring::rand::SystemRandom::new(),
    )
    .ok()
}

/// The COSE_Key of test credential `n` of `algorithm` (`es256`, `edDSA`,
/// `rs256`), in CTAP2's canonical key order.
fn test_cose_key(algorithm: &str, n: u64) -> Option<Vec<u8>> {
    let mut w = Writer(Vec::new());
    match algorithm {
        "es256" => {
            let point = crate::crypto::p256_public(&test_key("ecdsaP256", n))?;
            w.map(5);
            w.int(1);
            w.int(2);
            w.int(3);
            w.int(-7);
            w.int(-1);
            w.int(1);
            w.int(-2);
            w.bytes(&point[1..33]);
            w.int(-3);
            w.bytes(&point[33..]);
        }
        "edDSA" => {
            let x = crate::crypto::ed25519_public(&test_key("ed25519", n))?;
            w.map(4);
            w.int(1);
            w.int(1);
            w.int(3);
            w.int(-8);
            w.int(-1);
            w.int(6);
            w.int(-2);
            w.bytes(&x);
        }
        "rs256" => {
            let pair = signature::RsaKeyPair::from_der(TEST_RSA_KEY).ok()?;
            let c = signature::RsaPublicKeyComponents::<Vec<u8>>::from(pair.public());
            w.map(4);
            w.int(1);
            w.int(3);
            w.int(3);
            w.int(-257);
            w.int(-1);
            w.bytes(&c.n);
            w.int(-2);
            w.bytes(&c.e);
        }
        _ => return None,
    }
    Some(w.0)
}

/// Test credential `n`'s signature of `message`.
fn test_sign(algorithm: &str, n: u64, message: &[u8]) -> Option<Vec<u8>> {
    let rng = ring::rand::SystemRandom::new();
    match algorithm {
        "es256" => Some(
            p256_der_pair(&test_key("ecdsaP256", n))?
                .sign(&rng, message)
                .ok()?
                .as_ref()
                .to_vec(),
        ),
        "edDSA" => crate::crypto::ed25519_sign(&test_key("ed25519", n), message),
        "rs256" => {
            let pair = signature::RsaKeyPair::from_der(TEST_RSA_KEY).ok()?;
            let mut sig = vec![0; pair.public().modulus_len()];
            pair.sign(&signature::RSA_PKCS1_SHA256, &rng, message, &mut sig)
                .ok()?;
            Some(sig)
        }
        _ => None,
    }
}

/// The id of test credential `n` of `algorithm`: 16 bytes of the SHA-256
/// of `k.test.TestAuthenticator/<algorithm>/<n>`.
#[must_use]
pub fn test_credential_id(algorithm: &str, n: u64) -> Vec<u8> {
    digest(
        256,
        format!("k.test.TestAuthenticator/{algorithm}/{n}").as_bytes(),
    )[..16]
        .to_vec()
}

/// The client data a browser at `origin` writes.
fn test_client_data(ty: &str, challenge: &str, origin: &str) -> Vec<u8> {
    json!({ "type": ty, "challenge": challenge, "origin": origin, "crossOrigin": false })
        .to_string()
        .into_bytes()
}

/// Authenticator data: the RP id hash, the flags, the counter and the
/// attested credential when there is one.
fn test_auth_data(rp_id: &str, flags: u8, count: u32, attested: Option<(&[u8], &[u8])>) -> Vec<u8> {
    let mut a = digest(256, rp_id.as_bytes());
    a.push(flags | if attested.is_some() { AT } else { 0 });
    a.extend(count.to_be_bytes());
    if let Some((id, key)) = attested {
        a.extend([0u8; 16]);
        a.extend(u16::try_from(id.len()).unwrap_or(0).to_be_bytes());
        a.extend(id);
        a.extend(key);
    }
    a
}

/// The common fields of a test ceremony's input.
struct TestInput<'a> {
    /// `es256`, `edDSA` or `rs256`.
    algorithm: &'a str,
    /// The credential's number, which picks its key.
    key: u64,
    /// The RP id the authenticator signs for.
    rp_id: &'a str,
    /// The origin the client data names.
    origin: &'a str,
    /// The challenge, base64url.
    challenge: &'a str,
    /// The flags, without AT.
    flags: u8,
    /// The signature counter.
    count: u32,
}

/// The common fields of a test ceremony's JSON input.
fn test_input(v: &Value) -> Option<TestInput<'_>> {
    let flag = |k: &str, f: u8| {
        if v.get(k).and_then(Value::as_bool).unwrap_or(false) {
            f
        } else {
            0
        }
    };
    Some(TestInput {
        algorithm: v.get("algorithm")?.as_str()?,
        key: v.get("key")?.as_u64()?,
        rp_id: v.get("rpId")?.as_str()?,
        origin: v.get("origin")?.as_str()?,
        challenge: v.get("challenge")?.as_str()?,
        flags: UP | flag("userVerified", UV),
        count: u32::try_from(v.get("signCount")?.as_u64()?).ok()?,
    })
}

/// `TestAuthenticator`'s registration: `{algorithm, key, rpId, origin,
/// challenge, userVerified, signCount}` to `{credentialId, clientDataJSON,
/// attestationObject}` (base64url), with attestation `"none"`; `""` for
/// input it can't use.
#[must_use]
pub fn test_register(input: &str) -> String {
    let Some(out) = (|| {
        let v: Value = serde_json::from_str(input).ok()?;
        let t = test_input(&v)?;
        let id = test_credential_id(t.algorithm, t.key);
        let key = test_cose_key(t.algorithm, t.key)?;
        let auth = test_auth_data(t.rp_id, t.flags, t.count, Some((&id, &key)));
        let mut w = Writer(Vec::new());
        w.map(3);
        w.text("fmt");
        w.text("none");
        w.text("attStmt");
        w.map(0);
        w.text("authData");
        w.bytes(&auth);
        Some(json!({
            "credentialId": base64url(&id),
            "clientDataJSON": base64url(&test_client_data("webauthn.create", t.challenge, t.origin)),
            "attestationObject": base64url(&w.0),
        }))
    })() else {
        return String::new();
    };
    out.to_string()
}

/// `TestAuthenticator`'s assertion with the same input as
/// [`test_register`]: `{clientDataJSON, authenticatorData, signature}`.
#[must_use]
pub fn test_assert(input: &str) -> String {
    let Some(out) = (|| {
        let v: Value = serde_json::from_str(input).ok()?;
        let t = test_input(&v)?;
        let client = test_client_data("webauthn.get", t.challenge, t.origin);
        let auth = test_auth_data(t.rp_id, t.flags, t.count, None);
        let mut signed = auth.clone();
        signed.extend(digest(256, &client));
        let sig = test_sign(t.algorithm, t.key, &signed)?;
        Some(json!({
            "clientDataJSON": base64url(&client),
            "authenticatorData": base64url(&auth),
            "signature": base64url(&sig),
        }))
    })() else {
        return String::new();
    };
    out.to_string()
}

/// [`test_register`].
#[op2]
#[string]
pub(crate) fn op_k_passkey_test_register(#[string] input: String) -> String {
    test_register(&input)
}

/// [`test_assert`].
#[op2]
#[string]
pub(crate) fn op_k_passkey_test_assert(#[string] input: String) -> String {
    test_assert(&input)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    const VECTORS: &str = include_str!("passkey/webauthn-vectors.json");

    fn hex(t: &str) -> Vec<u8> {
        (0..t.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&t[i..i + 2], 16).unwrap())
            .collect()
    }

    /// One WebAuthn §16 example: its registration and authentication.
    struct Vector {
        reg_challenge: String,
        reg_client: Vec<u8>,
        attestation: Vec<u8>,
        credential_id: Vec<u8>,
        auth_challenge: String,
        auth_client: Vec<u8>,
        auth_data: Vec<u8>,
        signature: Vec<u8>,
    }

    fn vector(name: &str) -> Vector {
        let all: Value = serde_json::from_str(VECTORS).unwrap();
        let v = &all[name];
        let r = |k: &str| hex(v["registration"][k].as_str().unwrap());
        let a = |k: &str| hex(v["authentication"][k].as_str().unwrap());
        Vector {
            reg_challenge: base64url(&r("challenge")),
            reg_client: r("clientDataJSON"),
            attestation: r("attestationObject"),
            credential_id: r("credential_id"),
            auth_challenge: base64url(&a("challenge")),
            auth_client: a("clientDataJSON"),
            auth_data: a("authenticatorData"),
            signature: a("signature"),
        }
    }

    /// The spec's examples set neither UV in every case nor counters.
    fn spec<'a>(challenge: &'a str) -> Expected<'a> {
        Expected {
            rp_id: "example.org",
            origin: "https://example.org",
            challenge,
            require_user_verification: false,
        }
    }

    fn register(v: &Vector) -> Result<Registered, Refusal> {
        verify_registration(
            &v.reg_client,
            &v.attestation,
            &v.credential_id,
            &spec(&v.reg_challenge),
        )
    }

    fn assert_with(
        v: &Vector,
        key: &[u8],
        auth_data: &[u8],
        sig: &[u8],
    ) -> Result<Asserted, Refusal> {
        verify_assertion(
            &v.auth_client,
            auth_data,
            sig,
            key,
            0,
            &spec(&v.auth_challenge),
        )
    }

    #[test]
    fn the_specs_vectors_register_and_authenticate() {
        for name in [
            "none.ES256",
            "packed-self.ES256",
            "none.ES256.long-credential-id",
            "packed.RS256",
            "packed.EdDSA",
        ] {
            let v = vector(name);
            let r = register(&v).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(r.id, v.credential_id, "{name}");
            let key = parse_cose_key(&r.public_key).unwrap();
            let expected = match name {
                "packed.RS256" => "rs",
                "packed.EdDSA" => "ed",
                _ => "es",
            };
            assert!(
                matches!(
                    (&key, expected),
                    (CoseKey::Rs256 { .. }, "rs")
                        | (CoseKey::Ed25519(_), "ed")
                        | (CoseKey::Es256(_), "es")
                ),
                "{name}"
            );
            let a = assert_with(&v, &r.public_key, &v.auth_data, &v.signature)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(a.sign_count, 0, "{name}");
        }
    }

    #[test]
    fn the_specs_es256_vector_has_the_expected_shape() {
        let v = vector("none.ES256");
        let att = cbor_decode(&v.attestation).unwrap();
        assert_eq!(att.get_text("fmt"), Some(&Cbor::Text("none".into())));
        let Some(Cbor::Bytes(raw)) = att.get_text("authData") else {
            panic!("no authData")
        };
        let a = parse_auth_data(raw).unwrap();
        assert_eq!(a.flags, 0x59);
        assert_eq!(a.rp_id_hash, digest(256, b"example.org").as_slice());
        assert_eq!(
            a.credential.as_ref().unwrap().aaguid,
            hex("8446ccb9ab1db374750b2367ff6f3a1f").as_slice()
        );
        let r = register(&v).unwrap();
        assert!(r.backup_eligible && r.backed_up);
        // Required user verification refuses it: its UV flag is unset.
        let strict = Expected {
            require_user_verification: true,
            ..spec(&v.reg_challenge)
        };
        assert_eq!(
            verify_registration(&v.reg_client, &v.attestation, &v.credential_id, &strict),
            Err(Refusal::UserNotVerified)
        );
    }

    #[test]
    fn cross_origin_and_top_origin_client_data_are_refused() {
        for name in ["none.ES256.crossOrigin", "none.ES256.topOrigin"] {
            let all: Value = serde_json::from_str(VECTORS).unwrap();
            let r = &all[name]["registration"];
            let h = |k: &str| hex(r[k].as_str().unwrap());
            assert_eq!(
                verify_registration(
                    &h("clientDataJSON"),
                    &h("attestationObject"),
                    &h("credential_id"),
                    &spec(&base64url(&h("challenge")))
                ),
                Err(Refusal::OriginMismatch),
                "{name}"
            );
        }
    }

    #[test]
    fn a_changed_challenge_origin_rp_or_ceremony_is_refused() {
        let v = vector("none.ES256");
        let check = |e: Expected<'_>| {
            verify_registration(&v.reg_client, &v.attestation, &v.credential_id, &e)
        };
        assert_eq!(check(spec("AAAA")), Err(Refusal::ChallengeMismatch));
        assert_eq!(
            check(Expected {
                origin: "https://example.com",
                ..spec(&v.reg_challenge)
            }),
            Err(Refusal::OriginMismatch)
        );
        assert_eq!(
            check(Expected {
                rp_id: "example.com",
                ..spec(&v.reg_challenge)
            }),
            Err(Refusal::RpIdMismatch)
        );
        // A registration's client data presented as a sign-in's.
        let r = register(&v).unwrap();
        assert_eq!(
            verify_assertion(
                &v.reg_client,
                &v.auth_data,
                &v.signature,
                &r.public_key,
                0,
                &spec(&v.reg_challenge)
            ),
            Err(Refusal::ChallengeMismatch)
        );
        assert_eq!(
            verify_registration(
                &v.reg_client,
                &v.attestation,
                b"another id",
                &spec(&v.reg_challenge)
            ),
            Err(Refusal::CredentialMismatch)
        );
    }

    #[test]
    fn every_flipped_byte_of_an_assertion_is_refused() {
        for name in ["none.ES256", "packed.RS256", "packed.EdDSA"] {
            let v = vector(name);
            let key = register(&v).unwrap().public_key;
            for i in 0..v.auth_data.len() {
                let mut a = v.auth_data.clone();
                a[i] ^= 0x01;
                assert!(
                    assert_with(&v, &key, &a, &v.signature).is_err(),
                    "{name} authData byte {i}"
                );
            }
            for i in 0..v.signature.len() {
                let mut s = v.signature.clone();
                s[i] ^= 0x80;
                assert!(
                    assert_with(&v, &key, &v.auth_data, &s).is_err(),
                    "{name} signature byte {i}"
                );
            }
            let mut client = v.auth_client.clone();
            let last = client.len() - 2;
            client[last] ^= 0x01;
            assert!(
                verify_assertion(
                    &client,
                    &v.auth_data,
                    &v.signature,
                    &key,
                    0,
                    &spec(&v.auth_challenge)
                )
                .is_err(),
                "{name} client data"
            );
        }
    }

    #[test]
    fn a_signature_under_another_key_is_refused() {
        let v = vector("none.ES256");
        let other = register(&vector("packed-self.ES256")).unwrap().public_key;
        assert_eq!(
            assert_with(&v, &other, &v.auth_data, &v.signature),
            Err(Refusal::InvalidSignature)
        );
    }

    #[test]
    fn truncations_and_trailing_bytes_are_malformed() {
        let v = vector("none.ES256");
        for n in 0..v.attestation.len() {
            assert!(
                verify_registration(
                    &v.reg_client,
                    &v.attestation[..n],
                    &v.credential_id,
                    &spec(&v.reg_challenge)
                )
                .is_err(),
                "attestation cut at {n}"
            );
        }
        let mut long = v.attestation.clone();
        long.push(0);
        assert_eq!(
            verify_registration(
                &v.reg_client,
                &long,
                &v.credential_id,
                &spec(&v.reg_challenge)
            ),
            Err(Refusal::Malformed)
        );
        for n in 0..v.auth_data.len() {
            assert_eq!(
                parse_auth_data(&v.auth_data[..n]),
                Err(Refusal::Malformed),
                "authData cut at {n}"
            );
        }
        let mut long = v.auth_data.clone();
        long.push(0);
        assert_eq!(parse_auth_data(&long), Err(Refusal::Malformed));
    }

    #[test]
    fn the_cbor_reader_refuses_what_webauthn_never_writes() {
        assert_eq!(
            cbor_decode(&[0x5f, 0x41, 0x00, 0xff]),
            None,
            "indefinite bytes"
        );
        assert_eq!(cbor_decode(&[0xbf, 0xff]), None, "indefinite map");
        assert_eq!(
            cbor_decode(&[0xa2, 0x01, 0x00, 0x01, 0x00]),
            None,
            "duplicate key"
        );
        assert_eq!(cbor_decode(&[0xc0, 0x00]), None, "tag");
        assert_eq!(cbor_decode(&[0xf9, 0x00, 0x00]), None, "half float");
        assert_eq!(cbor_decode(&[0xf7]), None, "undefined");
        assert_eq!(
            cbor_decode(&[0x5b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            None,
            "huge length"
        );
        assert_eq!(
            cbor_decode(&[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            None,
            "huge array"
        );
        assert_eq!(cbor_decode(&[0x62, 0xff, 0xfe]), None, "invalid UTF-8");
        assert_eq!(cbor_decode(&[0x1c]), None, "reserved");
        let deep: Vec<u8> = std::iter::repeat_n(0x81u8, MAX_DEPTH + 2)
            .chain([0x00])
            .collect();
        assert_eq!(cbor_decode(&deep), None, "too deep");
        assert_eq!(
            cbor_decode(&[0xa2, 0x01, 0x02, 0x20, 0xf5]),
            Some(Cbor::Map(vec![
                (Cbor::Int(1), Cbor::Int(2)),
                (Cbor::Int(-1), Cbor::Bool(true))
            ]))
        );
        assert_eq!(
            cbor_decode(&[0x3b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Some(Cbor::Int(-18_446_744_073_709_551_616))
        );
    }

    /// A COSE map entry: the key, and an integer, bytes or `null`.
    type Entry<'a> = (i64, Option<i64>, Option<&'a [u8]>);

    #[test]
    fn cose_keys_of_other_algorithms_or_off_the_curve_are_refused() {
        let key = |pairs: &[Entry<'_>]| {
            let mut w = Writer(Vec::new());
            w.map(pairs.len() as u64);
            for (k, i, b) in pairs {
                w.int(*k);
                match (i, b) {
                    (Some(i), _) => w.int(*i),
                    (_, Some(b)) => w.bytes(b),
                    _ => w.0.push(0xf6),
                }
            }
            w.0
        };
        // ES384 and Ed448.
        assert_eq!(
            parse_cose_key(&key(&[
                (1, Some(2), None),
                (3, Some(-35), None),
                (-1, Some(2), None)
            ])),
            Err(Refusal::UnsupportedAlgorithm)
        );
        assert_eq!(
            parse_cose_key(&key(&[
                (1, Some(1), None),
                (3, Some(-8), None),
                (-1, Some(7), None),
                (-2, None, Some(&[0; 57]))
            ])),
            Err(Refusal::UnsupportedAlgorithm)
        );
        // A P-256 point that isn't on the curve.
        assert_eq!(
            parse_cose_key(&key(&[
                (1, Some(2), None),
                (3, Some(-7), None),
                (-1, Some(1), None),
                (-2, None, Some(&[1; 32])),
                (-3, None, Some(&[2; 32]))
            ])),
            Err(Refusal::Malformed)
        );
        // A 1,024-bit RSA modulus.
        let mut n = vec![0x80; 128];
        n[127] = 1;
        assert_eq!(
            parse_cose_key(&key(&[
                (1, Some(3), None),
                (3, Some(-257), None),
                (-1, None, Some(&n)),
                (-2, None, Some(&[1, 0, 1]))
            ])),
            Err(Refusal::UnsupportedAlgorithm)
        );
        assert_eq!(
            parse_cose_key(&key(&[(1, Some(2), None)])),
            Err(Refusal::Malformed),
            "no alg"
        );
    }

    #[test]
    fn a_backed_up_flag_without_eligibility_and_a_long_id_are_malformed() {
        let mut a = vector("none.ES256").auth_data;
        a[32] = UP | BS;
        assert_eq!(parse_auth_data(&a), Err(Refusal::Malformed));
        let mut a = digest(256, b"example.org");
        a.push(UP | AT);
        a.extend([0; 4]);
        a.extend([0; 16]);
        a.extend(1024u16.to_be_bytes());
        a.extend(vec![7; 1024]);
        a.extend(test_cose_key("edDSA", 1).unwrap());
        assert_eq!(parse_auth_data(&a), Err(Refusal::Malformed));
    }

    /// A test ceremony's input.
    fn input(algorithm: &str, challenge: &str, uv: bool, count: u32) -> String {
        json!({
            "algorithm": algorithm, "key": 1, "rpId": "localhost", "origin": "http://localhost:8080",
            "challenge": challenge, "userVerified": uv, "signCount": count,
        })
        .to_string()
    }

    fn local(challenge: &str) -> Expected<'_> {
        Expected {
            rp_id: "localhost",
            origin: "http://localhost:8080",
            challenge,
            require_user_verification: true,
        }
    }

    #[test]
    fn the_test_authenticator_signs_what_the_verifier_accepts() {
        for alg in ["es256", "edDSA", "rs256"] {
            let reg: Value =
                serde_json::from_str(&test_register(&input(alg, "cmVn", true, 0))).unwrap();
            let b = |v: &Value, k: &str| from_base64url(v[k].as_str().unwrap()).unwrap();
            let r = verify_registration(
                &b(&reg, "clientDataJSON"),
                &b(&reg, "attestationObject"),
                &b(&reg, "credentialId"),
                &local("cmVn"),
            )
            .unwrap_or_else(|e| panic!("{alg}: {e:?}"));
            assert_eq!(r.id, test_credential_id(alg, 1));
            let a: Value =
                serde_json::from_str(&test_assert(&input(alg, "Z2V0", true, 5))).unwrap();
            let check = |stored| {
                verify_assertion(
                    &b(&a, "clientDataJSON"),
                    &b(&a, "authenticatorData"),
                    &b(&a, "signature"),
                    &r.public_key,
                    stored,
                    &local("Z2V0"),
                )
            };
            assert_eq!(check(4).map(|x| x.sign_count), Ok(5), "{alg}");
            assert_eq!(check(5), Err(Refusal::SignCountRegressed), "{alg}");
            assert_eq!(check(9), Err(Refusal::SignCountRegressed), "{alg}");
        }
        // Without UV, required verification refuses it.
        let a: Value =
            serde_json::from_str(&test_register(&input("es256", "cmVn", false, 0))).unwrap();
        let b = |k: &str| from_base64url(a[k].as_str().unwrap()).unwrap();
        assert_eq!(
            verify_registration(
                &b("clientDataJSON"),
                &b("attestationObject"),
                &b("credentialId"),
                &local("cmVn")
            ),
            Err(Refusal::UserNotVerified)
        );
    }

    #[test]
    fn the_test_es256_key_is_test_keys_key() {
        let k = parse_cose_key(&test_cose_key("es256", 1).unwrap()).unwrap();
        let expected = crate::crypto::p256_public(&test_key("ecdsaP256", 1)).unwrap();
        assert_eq!(k, CoseKey::Es256(expected));
    }

    #[test]
    fn op_input_that_isnt_json_or_base64url_is_malformed() {
        assert_eq!(registration_op("nope"), r#"{"error":"malformed"}"#);
        let bad = json!({
            "clientDataJSON": "!!", "attestationObject": "", "credentialId": "", "rpId": "x", "origin": "y", "challenge": "z",
        });
        assert_eq!(
            registration_op(&bad.to_string()),
            r#"{"error":"malformed"}"#
        );
        assert_eq!(assertion_op("{}"), r#"{"error":"malformed"}"#);
    }
}
