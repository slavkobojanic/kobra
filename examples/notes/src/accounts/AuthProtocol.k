import k.auth.RateLimit

/// The contract for creating accounts and signing in and out.
///
/// A successful call changes the session cookie, and the browser's
/// `Session` picks the change up from the response, so none of these rpcs
/// returns the user. Signing in and up are rate limited per client, which
/// slows down password guessing and bulk sign-ups.
public protocol AuthProtocol {
    /// Reports whether an email address is free to register.
    ///
    /// Backs the `@Check` on `SignUpRequest.email`. A sign-up form reveals
    /// whether an address is registered in any case; the rate limits on
    /// `signIn` and `signUp` are what protect accounts.
    ///
    /// - Parameter email: the address as typed. The server normalizes it.
    /// - Returns: `true` when no account uses the address.
    rpc isEmailAvailable(email: String) -> Bool

    /// Creates an account and signs it in.
    ///
    /// - Parameter request: the sign-up form, validated before this runs.
    /// - Throws: `SignUpError.emailTaken` when the email was registered after
    ///   validation ran, and `.rejected` after 10 attempts in a minute.
    rpc signUp(request: SignUpRequest) requires RateLimit(10.perMinute) throws SignUpError

    /// Signs an existing account in.
    ///
    /// - Parameter credentials: the sign-in form.
    /// - Throws: `SignInError.invalidCredentials` when the email and password
    ///   don't match an account, and `.rejected` after 10 attempts in a
    ///   minute.
    rpc signIn(credentials: Credentials) requires RateLimit(10.perMinute) throws SignInError

    /// Ends the current session. Signing out when not signed in does nothing.
    rpc signOut()
}
