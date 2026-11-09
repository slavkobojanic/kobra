import k.rpc.RpcFailure

/// Why a command was refused.
public enum CommandError implements Error {
    case notAllowed
    case rpcFailure(RpcFailure)
}
