import k.rpc.RpcFailure

/// Why a command was refused: a different refusal type breaks.
public enum PostError implements Error, Codable {
    /// Not the sender's message.
    case notYours
    /// The call failed outside the handler.
    case rpcFailure(RpcFailure)
}
