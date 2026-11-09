import k.rpc.RpcFailure

/// Why a command was refused.
public enum CommandError implements Error, Codable {
    /// Not the sender's message.
    case notYours
    /// The call failed outside the handler.
    case rpcFailure(RpcFailure)
}
