/// Passkeys in the browser (`docs/design/51-passkeys.md`): the WebAuthn
/// ceremonies that make a passkey for this site and sign in with one. The
/// server checks what they return with [`PasskeyVerifier`]. A client-only
/// platform interface: [`BrowserPasskeys`] is injected with no `use`
/// rule, and [`k.test.TestAuthenticator`] replaces it in tests.
///
/// Both ceremonies open the device's passkey sheet, so both are
/// `@UserAction`: fetch the options (and their challenge) from the server
/// before the click, in an effect, never after an `await` in the click.
@Platform(side: .client)
public interface Passkeys {
    /// Whether this browser can make and use passkeys here: `false`
    /// outside a secure context, without WebAuthn, and during server
    /// rendering.
    var isSupported: Bool

    /// Makes a passkey for this site with the server's `options`, from
    /// [`PasskeyVerifier.registrationOptions(userHandle:excluding:)`].
    ///
    /// - Parameters:
    ///   - options: the server's options, with their challenge.
    ///   - name: the label the device shows for the passkey, usually the
    ///     email typed; the server never relies on it.
    /// - Returns: the registration to send to
    ///   [`PasskeyVerifier.verifyRegistration(_:)`].
    /// - Throws: [`PasskeyError.unsupported`] where [`isSupported`] is
    ///   `false`, `.cancelled` when the sheet was dismissed or timed out,
    ///   `.alreadyRegistered` when the device holds an excluded
    ///   credential, and `.failed` otherwise.
    @UserAction
    func register(_ options: PasskeyCreationOptions, name: String) async throws PasskeyError -> PasskeyRegistration

    /// Signs in with a passkey the device holds for this site.
    ///
    /// - Parameter options: the server's options, from
    ///   [`PasskeyVerifier.signInOptions(allowing:)`].
    /// - Returns: the assertion to send to
    ///   [`PasskeyVerifier.verifySignIn(_:credential:)`].
    /// - Throws: [`PasskeyError.unsupported`], `.cancelled` when the sheet
    ///   was dismissed, timed out or the device has no passkey for the
    ///   site (browsers don't tell these apart), and `.failed`.
    @UserAction
    func signIn(_ options: PasskeyRequestOptions) async throws PasskeyError -> PasskeyAssertion
}
