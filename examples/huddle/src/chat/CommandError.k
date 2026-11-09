import k.rpc.RpcFailure

/// Why the server refused one message on a room's channel. The channel
/// stays open.
public enum CommandError implements Error {
    /// The text is blank or longer than 500 characters.
    case invalidText
    /// Only a message's author deletes it.
    case notYourMessage
    /// The message was already deleted.
    case messageGone
    case rpcFailure(RpcFailure)
}
