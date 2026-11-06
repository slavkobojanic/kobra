import k.auth.GuardRejection

/// Why signing in failed.
public enum SignInError implements Error {
    /// No account matches the email and password together. Which of the two
    /// was wrong is deliberately not revealed.
    case invalidCredentials

    /// The rate limit refused the call, which slows down password guessing.
    case rejected(GuardRejection)
}
