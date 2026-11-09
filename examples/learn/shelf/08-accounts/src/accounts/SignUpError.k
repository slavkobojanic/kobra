import k.rpc.RpcFailure

/// Why making an account failed.
public enum SignUpError implements Error, Codable {
    /// Another account has this email address.
    case emailTaken

    /// The call didn't reach `AuthApi`.
    case rpcFailure(RpcFailure)
}
