import k.rpc.RpcFailure

/// Why joining failed.
public enum RoomError implements Error, Codable {
    /// Not a member of the room.
    case notAMember
    /// The call failed outside the handler.
    case rpcFailure(RpcFailure)
}
