import k.rpc.RpcFailure

/// Why making an account failed.
public enum SignUpError implements Error, Codable {
    /// Another account has this email address.
    case emailTaken

    /// Shelf isn't taking new accounts (`SHELF_SIGN_UPS` is `false`).
    case closed

    /// The call didn't reach `AuthApi`.
    case rpcFailure(RpcFailure)
}
