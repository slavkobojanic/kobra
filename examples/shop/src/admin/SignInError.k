import k.rpc.RpcFailure

/// Why signing in failed.
public enum SignInError implements Error {
    /// The email address or the password is wrong. Which one is deliberately
    /// not revealed.
    case invalidCredentials

    /// The call failed outside the handler: the request failed validation,
    /// the rate limit that slows down password guessing was exceeded, or the
    /// browser couldn't reach the server.
    case rpcFailure(RpcFailure)
}
