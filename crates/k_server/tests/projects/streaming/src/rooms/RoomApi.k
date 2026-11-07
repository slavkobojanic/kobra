import k.auth.Session
import k.auth.SessionManager
import k.rpc.RpcOnlyError

/// The server side of `RoomProtocol`.
public service RoomApi implements RoomProtocol {
    @Inject let channel: RoomChannel
    @Inject let sessions: SessionManager
    @Inject let session: Session

    rpc watch(room: String) async throws RoomError -> Stream<Message> {
        guard room != "closed" else {
            throw RoomError.closed(room: room)
        }

        let live = channel.subscribe(room: room)

        return live.startingWith(Message(room: room, text: "welcome to {room}"))
    }

    rpc texts(room: String) async throws RpcOnlyError -> Stream<String> {
        return channel.subscribe(room: room)
            .filter { message => message.text != "hidden" }
            .map { message => message.text }
    }

    rpc post(room: String, text: String) async throws RpcOnlyError {
        channel.publish(message: Message(room: room, text: text))
    }

    rpc privateFeed() async throws RpcOnlyError -> Stream<Message> {
        return channel.subscribe(room: "private-{session.user.id.name}")
            .startingWith(Message(room: "private", text: "hello {session.user.id.name}"))
    }

    rpc signIn() async throws RpcOnlyError {
        await sessions.signIn(userId: UserId(name: "ada"))
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }
}
