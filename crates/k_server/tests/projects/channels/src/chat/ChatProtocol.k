import k.auth.Authenticated
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// Rooms as channels, and the rpcs the tests drive them with.
public protocol ChatProtocol {
    /// Joins a room: a snapshot of the stored notes, then the room's events.
    rpc room(id: String) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError>

    /// Follows two rooms at once (`Stream.merge`).
    rpc pair(first: String, second: String) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError>

    /// A room only a signed-in user may join.
    rpc members(id: String) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> requires Authenticated

    /// Announces `text` to `room` from outside any channel.
    rpc announce(room: String, text: String) throws RpcOnlyError

    /// The stored notes, read through a separate request.
    rpc notes() throws RpcOnlyError -> List<String>

    rpc signIn(name: String) throws RpcOnlyError

    rpc signOut() throws RpcOnlyError
}
