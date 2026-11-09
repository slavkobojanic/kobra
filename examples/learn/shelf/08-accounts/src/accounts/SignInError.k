import k.rpc.RpcFailure

/// Why signing in failed.
public enum SignInError implements Error, Codable {
    /// No account has this email address and password. Which of the two is
    /// wrong isn't said, on purpose.
    case invalidCredentials

    /// The call didn't reach `AuthApi`.
    case rpcFailure(RpcFailure)
}
