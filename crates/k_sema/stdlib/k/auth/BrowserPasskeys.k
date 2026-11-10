/// The standard [`Passkeys`]: `navigator.credentials` with a public-key
/// credential, asking for a discoverable credential, user verification
/// and attestation `"none"`.
@ClientOnly
@Native
public service BrowserPasskeys implements Passkeys {
    /// `true` in a secure context with WebAuthn; see
    /// [`Passkeys.isSupported`].
    var isSupported: Bool

    /// `navigator.credentials.create`; see
    /// [`Passkeys.register(_:name:)`].
    @UserAction
    func register(_ options: PasskeyCreationOptions, name: String) async throws PasskeyError -> PasskeyRegistration
    /// `navigator.credentials.get`; see [`Passkeys.signIn(_:)`].
    @UserAction
    func signIn(_ options: PasskeyRequestOptions) async throws PasskeyError -> PasskeyAssertion
}
