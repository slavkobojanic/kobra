// Server-only code builds a channel in a handler; opening one and its client members are the browser's.
import k.rpc.Channel
import k.rpc.RpcOnlyError

enum Event implements Codable {
    case posted(text: String)
}

protocol Chat {
    rpc room(id: Int) throws RpcOnlyError -> Channel<Event, Event, RpcOnlyError>
}

protocol Relay {
    rpc relay(id: Int) throws RpcOnlyError -> Int
}

service RelayApi implements Relay {
    @Inject let chat: Chat

    rpc relay(id: Int) async throws RpcOnlyError -> Int {
        let channel = try await chat.room(id: id)
        try await channel.send(.posted(text: "hi"))
        for await event in channel.messages {
            channel.close()
        }
        return if channel.state == .open { 1 } else { 0 }
    }
}
