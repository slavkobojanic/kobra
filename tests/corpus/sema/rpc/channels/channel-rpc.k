// A channel rpc: declared in a protocol, built by its handler, used by a client service.
import k.auth.Authenticated
import k.auth.AuthUserId
import k.auth.Session
import k.crypto.Uuid
import k.rpc.Broadcast
import k.rpc.Channel
import k.rpc.ChannelState
import k.rpc.RpcFailure
import k.rpc.RpcOnlyError

struct AccountId implements AuthUserId {
    let value: Uuid
}

enum Command implements Codable {
    case post(text: String)
    case typing
}

enum Event implements Codable {
    case posted(text: String, by: AccountId)
}

enum CommandError implements Error, Codable {
    case notYours
    case rpcFailure(RpcFailure)
}

protocol Chat requires Authenticated {
    rpc room(id: Int) throws RpcOnlyError -> Channel<Command, Event, CommandError>
}

@Scope(.server)
service Rooms {
    private let events: Broadcast<Int, Event> = Broadcast()

    func subscribe(room: Int) -> Stream<Event> {
        return events.subscribe(key: room)
    }

    func publish(room: Int, event: Event) {
        events.publish(key: room, value: event)
    }
}

service ChatApi implements Chat {
    @Inject let session: Session
    @Inject let rooms: Rooms

    rpc room(id: Int) async throws RpcOnlyError -> Channel<Command, Event, CommandError> {
        return Channel(sending: rooms.subscribe(room: id)) { command =>
            match command {
                case .post(let text) {
                    rooms.publish(room: id, event: .posted(text: text, by: session.user.id))
                    //                                                     ^^^^^^^^^^^^ type: k.auth.SessionUser
                }
                case .typing {
                    throw CommandError.notYours
                }
            }
        }
    }
}

service RoomService {
    @Inject let chat: Chat

    @State var channel: Channel<Command, Event, CommandError>? = nil
    @State var seen: Int = 0

    @Derived let connection: ChannelState = channel?.state ?? .connecting

    func follow(room: Int) async throws RpcOnlyError {
        let opened = try await chat.room(id: room)
        channel = opened
        for await event in opened.messages {
            seen += 1
        }
    }

    func post(text: String) async throws CommandError {
        guard let channel else { throw CommandError.rpcFailure(.offline) }
        try await channel.send(.post(text: text))
    }

    func leave() {
        channel?.close()
    }
}
