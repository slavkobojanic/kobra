import k.rpc.RpcFailure

/// Codable, so its detail is decoded with its codec.
public enum Refusal implements Error, Codable {
    case tooLong(limit: Int)
    case rpcFailure(RpcFailure)
}
