import k.rpc.RpcFailure

/// An error that isn't Codable keeps the structural form.
public enum PlainError implements Error {
    case refused(count: Int)
    case rpcFailure(RpcFailure)
}
