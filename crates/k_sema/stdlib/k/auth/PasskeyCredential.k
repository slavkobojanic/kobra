/// A verified passkey: what the app stores for an account
/// (`docs/design/51-passkeys.md`, "Storage"), returned by
/// [`PasskeyVerifier.verifyRegistration(_:)`] and, with its new counter,
/// by [`PasskeyVerifier.verifySignIn(_:credential:)`]. An entity column
/// can't hold a struct, so the app's entity has these fields.
public struct PasskeyCredential implements Equatable, Codable {
    /// The credential id, base64url. Unique: store it in a `@Unique`
    /// column.
    let id: String
    /// The credential's public key, a COSE_Key as the authenticator wrote
    /// it, base64url: ES256, EdDSA (Ed25519) or RS256.
    let publicKey: String
    /// The signature counter; `0` for passkeys that don't count.
    let signCount: Int
    /// The user handle the passkey was made with, base64url.
    let userHandle: String
    /// Whether the passkey may be synced between devices (the BE flag).
    let backupEligible: Bool
    /// Whether it is synced now (the BS flag).
    let backedUp: Bool
}
