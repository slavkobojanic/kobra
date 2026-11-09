// Only the handler of a channel rpc builds a `Channel(sending:receiving:)`.
import k.rpc.Broadcast
import k.rpc.Channel
import k.rpc.RpcOnlyError

enum Event implements Codable {
    case posted(text: String)
}

protocol Chat {
    rpc room(id: Int) throws RpcOnlyError -> Channel<Event, Event, RpcOnlyError>
}

@Scope(.server)
service Rooms {
    private let events: Broadcast<Int, Event> = Broadcast()

    func subscribe(room: Int) -> Stream<Event> {
        return events.subscribe(key: room)
    }
}

service ChatApi implements Chat {
    @Inject let rooms: Rooms

    rpc room(id: Int) async throws RpcOnlyError -> Channel<Event, Event, RpcOnlyError> {
        return build(id: id)
    }

    private func build(id: Int) -> Channel<Event, Event, RpcOnlyError> {
        return Channel(sending: rooms.subscribe(room: id)) { event => }
    }
}

service RoomService {
    @State var channel: Channel<Event, Event, RpcOnlyError>? = nil

    func fake(events: Stream<Event>) {
        channel = Channel(sending: events) { event => }
    }
}
