import k.rpc.RpcFailure

/// Why a command was refused: a refusal's payload follows the same rules.
public enum CommandError implements Error, Codable {
    /// Not the sender's message.
    case notYours(author: Int, at: String?)
    /// The call failed outside the handler.
    case rpcFailure(RpcFailure)
}
