// A `Channel` can't be inside another type; an optional `Channel` is fine.
import k.rpc.Channel
import k.rpc.RpcOnlyError

enum Event implements Codable {
    case posted(text: String)
}

protocol Chat {
    rpc room(id: Int) throws RpcOnlyError -> Channel<Event, Event, RpcOnlyError>
}

service RoomService {
    @Inject let chat: Chat

    @State var current: Channel<Event, Event, RpcOnlyError>? = nil
    @State var open: List<Channel<Event, Event, RpcOnlyError>> = []
    @State var byRoom: Map<Int, Channel<Event, Event, RpcOnlyError>?> = [:]

    func follow(rooms: List<Int>) async throws RpcOnlyError {
        for room in rooms {
            let channel = try await chat.room(id: room)
            let pair: List<Channel<Event, Event, RpcOnlyError>> = [channel]
            current = channel
        }
    }
}
