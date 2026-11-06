import k.rpc.RpcFailure

/// Why signing in failed.
public enum SignInError implements Error {
    /// No account matches the email and password together. Which of the two
    /// was wrong is deliberately not revealed.
    case invalidCredentials

    /// The call failed before `AuthApi` saw it: the rate limit refused it,
    /// which slows down password guessing, or the form was invalid.
    case rpcFailure(RpcFailure)
}
