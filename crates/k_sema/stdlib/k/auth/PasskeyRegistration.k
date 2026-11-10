/// A new passkey as the browser reports it
/// (`docs/design/51-passkeys.md`), from [`Passkeys.register(_:name:)`] to
/// [`PasskeyVerifier.verifyRegistration(_:)`]. Every field is the
/// browser's bytes as base64url; nothing in it is trusted before the
/// server verifies it.
public struct PasskeyRegistration implements Equatable, Codable {
    /// The credential id (`rawId`).
    let credentialId: String
    /// The client data JSON.
    let clientDataJSON: String
    /// The CBOR attestation object.
    let attestationObject: String
}
