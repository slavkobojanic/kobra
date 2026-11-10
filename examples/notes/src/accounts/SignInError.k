import k.rpc.RpcFailure

/// Why signing in failed.
public enum SignInError implements Error {
    /// No account matches the email and password together. Which of the two
    /// was wrong is deliberately not revealed.
    case invalidCredentials

    /// The passkey isn't one of this app's, or the server's checks refused
    /// it (a used or expired challenge, another site, no user
    /// verification, or a counter that went back, as a cloned passkey's).
    case passkeyNotAccepted

    /// The call failed outside `AuthApi`: the rate limit refused it, which
    /// slows down password guessing, the form was invalid, or the browser
    /// couldn't reach the server.
    case rpcFailure(RpcFailure)
}
