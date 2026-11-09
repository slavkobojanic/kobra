import k.rpc.RpcFailure

/// Why a room can't be joined.
public enum RoomError implements Error {
    case closed
    case rpcFailure(RpcFailure)
}
