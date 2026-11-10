/// A passkey's signature as the browser reports it
/// (`docs/design/51-passkeys.md`), from [`Passkeys.signIn(_:)`] to
/// [`PasskeyVerifier.verifySignIn(_:credential:)`]. Every field is the
/// browser's bytes as base64url.
public struct PasskeyAssertion implements Equatable, Codable {
    /// The id of the credential that signed: look the stored
    /// [`PasskeyCredential`] up by it.
    let credentialId: String
    /// The client data JSON.
    let clientDataJSON: String
    /// The authenticator data.
    let authenticatorData: String
    /// The signature over the authenticator data and the client data's
    /// hash.
    let signature: String
    /// The user handle the passkey was made with, when the device sends it.
    let userHandle: String?
}
