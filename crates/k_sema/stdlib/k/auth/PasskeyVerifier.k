/// Issues passkey challenges and verifies what the browser's
/// [`Passkeys`] returns (`docs/design/51-passkeys.md`). Server-only,
/// injectable only into server-side services.
///
/// The relying party is the app: its id is the host of
/// `K_PUBLIC_ORIGIN`, its origin `K_PUBLIC_ORIGIN`. Each options call
/// issues a challenge into the current session (replacing any earlier
/// challenge or OAuth state), and the next verification consumes it,
/// whether it succeeds or not. User verification is required, and the
/// attestation statement is never read (attestation `"none"`).
///
/// A verified sign-in signs nobody in: the app calls
/// [`SessionManager.signIn(userId:roles:)`] for the account that owns the
/// credential.
@ServerOnly
@Native
public service PasskeyVerifier {
    /// Issues a registration challenge.
    ///
    /// - Parameters:
    ///   - userHandle: the account's user handle, base64url, when it has
    ///     passkeys already; `nil` for a fresh random one (a sign-up).
    ///   - excluding: credential ids the device must not register again.
    /// - Returns: the options for [`Passkeys.register(_:name:)`].
    func registrationOptions(userHandle: String? = nil, excluding: List<String> = []) async -> PasskeyCreationOptions
    /// Verifies a registration against the session's challenge.
    ///
    /// - Parameter registration: what the browser returned.
    /// - Returns: the credential to store.
    /// - Throws: the [`PasskeyVerificationError`] of the first check that
    ///   failed.
    func verifyRegistration(_ registration: PasskeyRegistration) async throws PasskeyVerificationError -> PasskeyCredential
    /// Issues a sign-in challenge.
    ///
    /// - Parameter allowing: the credential ids the device may use; empty
    ///   for any passkey it holds for the site.
    /// - Returns: the options for [`Passkeys.signIn(_:)`].
    func signInOptions(allowing: List<String> = []) async -> PasskeyRequestOptions
    /// Verifies an assertion against the session's challenge and the
    /// stored credential.
    ///
    /// - Parameters:
    ///   - assertion: what the browser returned.
    ///   - credential: the stored credential whose `id` is the
    ///     assertion's `credentialId`.
    /// - Returns: the credential with its new counter and backup state,
    ///   to store.
    /// - Throws: the [`PasskeyVerificationError`] of the first check that
    ///   failed.
    func verifySignIn(_ assertion: PasskeyAssertion, credential: PasskeyCredential) async throws PasskeyVerificationError -> PasskeyCredential
}
