import k.auth.GuardRejection

/// Why signing in failed.
public enum SignInError implements Error {
    /// The email address or the password is wrong. Which one is deliberately
    /// not revealed.
    case invalidCredentials

    /// A `requires` guard refused the call; for sign-in, the rate limit that
    /// slows down password guessing.
    case rejected(GuardRejection)
}
