import k.rpc.RpcFailure

/// Why a sign-up failed.
public enum SignUpError implements Error {
    /// The email is registered.
    case emailTaken

    /// The handle is taken.
    case handleTaken

    /// The rpc failed.
    case rpcFailure(RpcFailure)
}
