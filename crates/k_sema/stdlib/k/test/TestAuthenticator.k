import k.auth.PasskeyAlgorithm
import k.auth.PasskeyAssertion
import k.auth.PasskeyCreationOptions
import k.auth.PasskeyError
import k.auth.PasskeyRegistration
import k.auth.PasskeyRequestOptions
import k.auth.Passkeys

/// The default [`Passkeys`] in `k test` (`docs/design/51-passkeys.md`,
/// "Tests"): a working authenticator. It holds real key pairs (key `n` of
/// [`TestKeys.ecdsaP256(_:)`] and [`TestKeys.ed25519(_:)`], or a fixed
/// RSA key), writes real attestation objects and authenticator data, and
/// signs real assertions, so the server's real
/// [`k.auth.PasskeyVerifier`] checks what it produces.
///
/// Its sheet is always accepted, with user verification, at the app's
/// origin; its counter starts at `0` and counts each assertion.
@TestDefault(replaces: Passkeys)
@Native
public service TestAuthenticator implements Passkeys {
    /// `true` unless [`setSupported(_:)`] said otherwise.
    var isSupported: Bool
    /// The credential ids it holds, base64url, oldest first.
    var passkeys: List<String>
    /// How many sheets it showed.
    var prompts: Int

    /// Makes a passkey as a device would, or throws `.unsupported`,
    /// `.cancelled` after [`dismissNext()`], or `.alreadyRegistered` for an
    /// excluded credential it holds.
    func register(_ options: PasskeyCreationOptions, name: String) async throws PasskeyError -> PasskeyRegistration
    /// Signs with the newest passkey it holds that the options allow, or
    /// throws `.cancelled` when it holds none (or after [`dismissNext()`]).
    func signIn(_ options: PasskeyRequestOptions) async throws PasskeyError -> PasskeyAssertion

    /// The key type of the next registrations; `.es256` by default.
    func useAlgorithm(_ algorithm: PasskeyAlgorithm)
    /// Makes the next ceremony throw `.cancelled`, as a dismissed sheet.
    func dismissNext()
    /// `false` leaves the user-verified flag unset, as a security key
    /// without a PIN does.
    func setUserVerified(_ value: Bool)
    /// The origin it writes into the client data, as a browser on that
    /// page would: a phishing page's, for tests. The app's by default.
    func setOrigin(_ origin: String)
    /// The counter the next assertion reports; later ones count on from
    /// it. A lower one plays a cloned authenticator.
    func setSignCount(_ count: Int)
    /// Forgets every passkey: the device holds none.
    func forgetPasskeys()
    /// Sets [`isSupported`].
    func setSupported(_ value: Bool)
}
