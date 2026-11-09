import k.rpc.RpcFailure

/// Why a line was refused.
public enum CommandError implements Error {
    case empty
    case rpcFailure(RpcFailure)
}
