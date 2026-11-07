import k.auth.Authenticated
import k.rpc.RpcOnlyError

/// Streams of room messages.
public protocol RoomProtocol {
    /// Every message posted to `room` from now on, after a greeting.
    rpc watch(room: String) throws RoomError -> Stream<Message>

    /// The texts of `room`'s messages: a scalar element.
    rpc texts(room: String) throws RpcOnlyError -> Stream<String>

    /// Posts a message to a room.
    rpc post(room: String, text: String) throws RpcOnlyError

    /// A stream behind a guard.
    rpc privateFeed() throws RpcOnlyError -> Stream<Message> requires Authenticated

    /// Signs in as `ada`, so `privateFeed` opens.
    rpc signIn() throws RpcOnlyError

    /// Signs out, which closes every stream opened under the session.
    rpc signOut() throws RpcOnlyError
}
