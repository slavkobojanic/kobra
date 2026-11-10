import k.rpc.RpcFailure

/// Why the server refused a command on the lobby's channel. The channel
/// stays open.
public enum LobbyError implements Error {
    /// The name is blank, longer than 24 characters, or has a space or one
    /// of `/ ? # %`.
    case invalidName
    /// There's a room by that name already.
    case nameTaken
    case rpcFailure(RpcFailure)
}
