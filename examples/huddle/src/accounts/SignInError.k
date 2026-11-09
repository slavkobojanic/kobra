import k.rpc.RpcFailure

/// Why signing in failed.
public enum SignInError implements Error {
    /// The name is blank, longer than 24 characters, or has a space.
    case invalidName
    case rpcFailure(RpcFailure)
}
