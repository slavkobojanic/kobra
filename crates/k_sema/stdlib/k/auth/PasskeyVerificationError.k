/// Why [`PasskeyVerifier`] refused a registration or a sign-in
/// (`docs/design/51-passkeys.md`). Every case means "don't sign in"; the
/// cases say why, for logs and tests.
public enum PasskeyVerificationError implements Error, Equatable {
    /// The challenge wasn't issued to this session, was used already,
    /// expired after ten minutes, or was for the other ceremony. Fetch new
    /// options and ask again.
    case challengeMismatch
    /// The ceremony ran on another origin than `K_PUBLIC_ORIGIN`, or in a
    /// frame: a phishing page's passkey prompt ends here.
    case originMismatch
    /// The authenticator signed for another relying party.
    case rpIdMismatch
    /// The device didn't check the user (the user-present or
    /// user-verified flag is unset).
    case userNotVerified
    /// The passkey's key is none of ES256, EdDSA (Ed25519) and RS256.
    case unsupportedAlgorithm
    /// The signature doesn't verify under the stored key.
    case invalidSignature
    /// The signature counter didn't increase: the passkey may have been
    /// cloned.
    case signCountRegressed
    /// The assertion is for another credential or user handle than the
    /// stored one, or the registration's credential id isn't the one the
    /// authenticator wrote.
    case credentialMismatch
    /// Something doesn't parse: base64url, JSON, CBOR or the
    /// authenticator data.
    case malformed
}
