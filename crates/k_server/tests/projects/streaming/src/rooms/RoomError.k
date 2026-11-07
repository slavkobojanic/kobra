import k.rpc.RpcFailure

/// Why a room can't be watched.
public enum RoomError implements Error {
    /// The room name is reserved.
    case closed(room: String)

    case rpcFailure(RpcFailure)
}
