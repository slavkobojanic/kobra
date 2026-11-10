import k.auth.PasskeyAssertion
import k.auth.PasskeyCreationOptions
import k.auth.PasskeyRegistration
import k.auth.PasskeyRequestOptions
import k.auth.RateLimit
import k.rpc.RpcOnlyError
import k.text.EmailAddress

/// The contract for creating accounts and signing in and out.
///
/// A successful call changes the session cookie, and the browser's
/// `Session` picks the change up from the response, so none of these rpcs
/// returns the user. Signing in and up are rate limited per client, which
/// slows down password guessing and bulk sign-ups.
///
/// Every rpc's error enum has the `rpcFailure` case. Besides refusals and
/// invalid requests, it carries `.offline` and `.timeout` when the browser
/// can't reach the server.
public protocol AuthProtocol {
    /// Reports whether an email address is free to register.
    ///
    /// Backs the `@Check` on `SignUpRequest.email`. A sign-up form reveals
    /// whether an address is registered in any case; the rate limits on
    /// `signIn` and `signUp` are what protect accounts.
    ///
    /// - Parameter email: the address. The server compares it in any letter
    ///   case.
    /// - Returns: `true` when no account uses the address.
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.
    rpc isEmailAvailable(email: EmailAddress) throws RpcOnlyError -> Bool

    /// Creates an account and signs it in.
    ///
    /// - Parameter request: the sign-up form, validated before this runs.
    /// - Throws: `SignUpError.emailTaken` when the email was registered after
    ///   validation ran, and `.rpcFailure` when the request is invalid, after
    ///   10 attempts in a minute, or when the server can't be reached.
    rpc signUp(request: SignUpRequest) throws SignUpError requires RateLimit(10.perMinute)

    /// Signs an existing account in.
    ///
    /// - Parameter credentials: the sign-in form.
    /// - Throws: `SignInError.invalidCredentials` when the email and password
    ///   don't match an account, and `.rpcFailure` when the form is invalid,
    ///   after 10 attempts in a minute, or when the server can't be reached.
    rpc signIn(credentials: Credentials) throws SignInError requires RateLimit(10.perMinute)

    /// Issues the challenge for signing up with a passkey, into this
    /// browser's session. The page fetches it before the click, because
    /// the device's passkey sheet may only open from the click itself.
    ///
    /// - Returns: the options for `Passkeys.register(_:name:)`.
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.
    rpc passkeySignUpOptions() throws RpcOnlyError -> PasskeyCreationOptions

    /// Creates an account whose only way in is the new passkey, and signs
    /// it in.
    ///
    /// - Parameters:
    ///   - email: the address typed, also the passkey's label on the device.
    ///   - registration: what the browser's `Passkeys.register` returned.
    /// - Throws: `SignUpError.emailTaken`, `.passkeyNotAccepted` when the
    ///   server's checks refuse the passkey (a used or expired challenge,
    ///   another origin, no user verification), and `.rpcFailure`.
    rpc signUpWithPasskey(email: EmailAddress, registration: PasskeyRegistration) throws SignUpError requires RateLimit(10.perMinute)

    /// Issues the challenge for signing in with a passkey.
    ///
    /// - Returns: the options for `Passkeys.signIn(_:)`.
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.
    rpc passkeySignInOptions() throws RpcOnlyError -> PasskeyRequestOptions

    /// Signs in the account a passkey belongs to.
    ///
    /// - Parameter assertion: what the browser's `Passkeys.signIn` returned.
    /// - Throws: `SignInError.passkeyNotAccepted` for an unknown passkey or
    ///   one the server's checks refuse, and `.rpcFailure`.
    rpc signInWithPasskey(assertion: PasskeyAssertion) throws SignInError requires RateLimit(10.perMinute)

    /// Ends the current session. Signing out when not signed in does nothing.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached,
    ///   in which case the session is still active.
    rpc signOut() throws RpcOnlyError
}
