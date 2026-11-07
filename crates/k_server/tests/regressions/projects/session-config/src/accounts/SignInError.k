import k.rpc.RpcFailure

/// Why signing in failed.
public enum SignInError implements Error {
    case invalidCredentials
    case rpcFailure(RpcFailure)
}
