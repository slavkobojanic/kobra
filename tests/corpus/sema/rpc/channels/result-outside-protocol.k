// A `Channel` is the result of a protocol's rpc and of its handler only, and never optional there.
import k.rpc.Channel
import k.rpc.RpcOnlyError

enum Event implements Codable {
    case posted(text: String)
}

protocol Chat {
    rpc room(id: Int) throws RpcOnlyError -> Channel<Event, Event, RpcOnlyError>
    rpc lobby() throws RpcOnlyError -> Channel<Event, Event, RpcOnlyError>?
}

service RoomService {
    @Inject let chat: Chat

    func open(room: Int) async throws RpcOnlyError -> Channel<Event, Event, RpcOnlyError> {
        return try await chat.room(id: room)
    }

    func current() -> Channel<Event, Event, RpcOnlyError>? {
        return nil
    }
}
