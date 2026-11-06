import k.rpc.RpcFailure

/// Why signing in failed.
public enum SignInError implements Error {
    /// The email address or the password is wrong. Which one is deliberately
    /// not revealed.
    case invalidCredentials

    /// The call was refused before it ran: the request failed validation, or
    /// the rate limit that slows down password guessing was exceeded.
    case rpcFailure(RpcFailure)
}
