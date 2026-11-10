import k.rpc.RpcFailure

public enum JoinError implements Error {
    case full
    case rpcFailure(RpcFailure)
}
