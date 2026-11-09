import k.rpc.Channel

/// Joins a room: a snapshot first, then every event.
public protocol ChatProtocol {
//              ^^^^^^^^^^^^ side: shared
    /// The room's channel.
    rpc room(id: Int) throws RoomError -> Channel<ChatCommand, ChatEvent, PostError>
}
