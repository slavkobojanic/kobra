import k.rpc.RpcFailure

/// Why a room can't be joined.
public enum RoomError implements Error {
    case noSuchRoom
    /// The room has a password, and this person hasn't given it yet.
    case passwordRequired
    /// `unlock(room:password:)` was given the wrong password.
    case wrongPassword
    /// A moderator removed this person from the room.
    case removed
    case rpcFailure(RpcFailure)
}
