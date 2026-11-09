// A `@Load` runs while the server renders, where a channel can't open; an `@Effect` is the place.
import k.rpc.Channel
import k.rpc.RpcOnlyError

enum Event implements Codable {
    case posted(text: String)
}

protocol Chat {
    rpc room(id: Int) throws RpcOnlyError -> Channel<Event, Event, RpcOnlyError>
    rpc count(id: Int) throws RpcOnlyError -> Int
}

service RoomService {
    @Inject let chat: Chat

    @State var channel: Channel<Event, Event, RpcOnlyError>? = nil

    func join(room: Int) async throws RpcOnlyError -> Int {
        return try await open(room: room)
    }

    private func open(room: Int) async throws RpcOnlyError -> Int {
        channel = try await chat.room(id: room)
        return room
    }

    func count(room: Int) async throws RpcOnlyError -> Int {
        return try await chat.count(id: room)
    }
}

page RoomPage {
    @Inject let rooms: RoomService

    @Load let joined: Int = try await rooms.join(room: 1)
    @Load let count: Int = try await rooms.count(room: 1)

    @Effect
    func follow() async {
        do {
            let joined = try await rooms.join(room: 1)
        } catch .rpcFailure(let failure) {
        }
    }

    view {
        paragraph { "{joined} {count}" }
    }

    loading {
        paragraph { "Loading…" }
    }
}
